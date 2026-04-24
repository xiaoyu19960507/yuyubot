use super::{
    ApiResponse, AppInfo, LogsResponse, PluginCaller, SetWebuiRequest, SystemConfig, SystemInfo,
    SystemInfoResponse, UiState,
};
use crate::logger;
use crate::plus::PluginManager;
use crate::runtime;
use crate::server::MainProxy;
use crate::window::UserEvent;
use rocket::{
    get, post,
    response::stream::{Event, EventStream},
    serde::json::Json,
    State,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[post("/set_webui", format = "json", data = "<req_body>")]
pub async fn set_webui(
    caller: PluginCaller,
    req_body: Json<SetWebuiRequest>,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager
        .set_plugin_webui(&caller.plugin_id, req_body.webui.clone())
        .await
    {
        Ok(_) => Json(ApiResponse {
            retcode: 0,
            data: "ok".to_string(),
        }),
        Err(e) => Json(ApiResponse {
            retcode: -1,
            data: e.to_string(),
        }),
    }
}

fn system_config_path(exe_dir: &Path) -> PathBuf {
    exe_dir.join("config").join("system.json")
}

fn load_system_config_from_disk(exe_dir: &Path) -> SystemConfig {
    let config_file = system_config_path(exe_dir);

    let Ok(content) = std::fs::read_to_string(&config_file) else {
        return SystemConfig::default();
    };

    serde_json::from_str::<SystemConfig>(&content).unwrap_or_default()
}

fn resolve_system_config(exe_dir: &Path) -> SystemConfig {
    let fallback = load_system_config_from_disk(exe_dir);

    match runtime::is_auto_start_enabled() {
        Ok(auto_start) => SystemConfig { auto_start },
        Err(err) => {
            log_warn!("Failed to read auto-start state from registry: {}", err);
            fallback
        }
    }
}

#[get("/ui/state")]
pub async fn get_ui_state() -> Json<ApiResponse<UiState>> {
    let exe_dir = runtime::get_exe_dir();
    let config_file = exe_dir.join("config").join("ui.json");

    let state = if let Ok(content) = tokio::fs::read_to_string(&config_file).await {
        serde_json::from_str::<UiState>(&content)
            .unwrap_or_default()
            .normalized()
    } else {
        UiState::default()
    };

    Json(ApiResponse {
        retcode: 0,
        data: state,
    })
}

#[post("/ui/state", format = "json", data = "<state>")]
pub async fn save_ui_state(state: Json<UiState>) -> Json<ApiResponse<String>> {
    let state_inner = state.into_inner().normalized();
    let exe_dir = runtime::get_exe_dir();

    let config_dir = exe_dir.join("config");
    let config_file = config_dir.join("ui.json");

    if let Err(e) = tokio::fs::create_dir_all(&config_dir).await {
        return Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to create config directory: {}", e),
        });
    }

    match serde_json::to_string_pretty(&state_inner) {
        Ok(json_str) => {
            if let Err(e) = tokio::fs::write(&config_file, json_str).await {
                return Json(ApiResponse {
                    retcode: 1,
                    data: format!("Failed to write ui config: {}", e),
                });
            }
            Json(ApiResponse {
                retcode: 0,
                data: "UI state saved".to_string(),
            })
        }
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to serialize ui state: {}", e),
        }),
    }
}

#[get("/get_app_nums")]
pub fn get_app_nums() -> Json<ApiResponse<i32>> {
    Json(ApiResponse {
        retcode: 0,
        data: 9,
    })
}

#[get("/logs")]
pub fn get_logs() -> Json<ApiResponse<LogsResponse>> {
    let logs = logger::get_logs();
    Json(ApiResponse {
        retcode: 0,
        data: LogsResponse { logs },
    })
}

#[post("/logs/clear")]
pub fn clear_logs() -> Json<ApiResponse<String>> {
    logger::clear_logs();
    Json(ApiResponse {
        retcode: 0,
        data: "Logs cleared".to_string(),
    })
}

#[get("/logs/stream")]
pub fn logs_stream() -> EventStream![Event + 'static] {
    EventStream! {
        let mut rx = logger::subscribe_logs();

        while let Ok(log_entry) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&log_entry) {
                yield Event::data(json);
            }
        }
    }
}

#[get("/system_info")]
pub fn get_system_info(
    system_info: &State<Arc<SystemInfo>>,
) -> Json<ApiResponse<SystemInfoResponse>> {
    let exe_dir = runtime::get_exe_dir();
    let system_config = resolve_system_config(&exe_dir);
    let info = SystemInfoResponse {
        port: system_info.port.load(std::sync::atomic::Ordering::SeqCst),
        data_dir: system_info.data_dir.clone(),
        plugins_root: system_info.plugins_root.clone(),
        auto_start: system_config.auto_start,
    };
    Json(ApiResponse {
        retcode: 0,
        data: info,
    })
}

#[post("/system/save_config", format = "json", data = "<config>")]
pub async fn save_system_config(config: Json<SystemConfig>) -> Json<ApiResponse<String>> {
    let config_inner = config.into_inner();

    if let Err(err) = runtime::set_auto_start_enabled(config_inner.auto_start) {
        log_error!("Failed to update auto-start setting: {}", err);
        return Json(ApiResponse {
            retcode: 1,
            data: err,
        });
    }

    let exe_dir = runtime::get_exe_dir();
    let config_file = system_config_path(&exe_dir);

    if let Some(config_dir) = config_file.parent() {
        if let Err(err) = tokio::fs::create_dir_all(config_dir).await {
            log_error!("Failed to create system config directory: {}", err);
            return Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to create config directory: {}", err),
            });
        }
    }

    let json = match serde_json::to_string_pretty(&config_inner) {
        Ok(value) => value,
        Err(err) => {
            log_error!("Failed to serialize system config: {}", err);
            return Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to serialize system config: {}", err),
            });
        }
    };

    if let Err(err) = tokio::fs::write(&config_file, json).await {
        log_error!("Failed to write system config: {}", err);
        return Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to write system config: {}", err),
        });
    }

    Json(ApiResponse {
        retcode: 0,
        data: "System config saved".to_string(),
    })
}

#[post("/open_data_dir")]
pub async fn open_data_dir(system_info: &State<Arc<SystemInfo>>) -> Json<ApiResponse<String>> {
    let path = system_info.data_dir.clone();

    let _ = tokio::fs::create_dir_all(&path).await;
    runtime::open_in_explorer(&path);

    Json(ApiResponse {
        retcode: 0,
        data: "Opening directory".to_string(),
    })
}

#[post("/open_plugins_dir")]
pub async fn open_plugins_dir(manager: &State<Arc<PluginManager>>) -> Json<ApiResponse<String>> {
    match manager.open_plugins_root().await {
        Ok(_) => Json(ApiResponse {
            retcode: 0,
            data: "Opening directory".to_string(),
        }),
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: e,
        }),
    }
}

#[post("/restart_program")]
pub async fn restart_program(main_proxy: &State<Arc<MainProxy>>) -> Json<ApiResponse<String>> {
    let proxy_lock = main_proxy.proxy.read().await;
    if let Some(proxy) = &*proxy_lock {
        let _ = proxy.send_event(UserEvent::RestartRequested);
    }

    Json(ApiResponse {
        retcode: 0,
        data: "Restarting...".to_string(),
    })
}

#[get("/app_info")]
pub fn get_app_info() -> Json<ApiResponse<AppInfo>> {
    Json(ApiResponse {
        retcode: 0,
        data: AppInfo {
            version: env!("APP_VERSION").to_string(),
        },
    })
}
