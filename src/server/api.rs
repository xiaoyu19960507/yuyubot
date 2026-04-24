use crate::logger;
use crate::plus::PluginManager;
use crate::runtime;
use crate::server::MainProxy;
use crate::window::UserEvent;
use rocket::{
    get,
    http::Status,
    post,
    request::{FromRequest, Outcome},
    response::stream::{Event, EventStream},
    serde::json::Json,
    Request, State,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

mod bot;
mod plugins;

pub use bot::{
    bot_status_stream, check_and_auto_connect, disconnect_bot, get_bot_config, get_bot_status,
    get_login_info, load_bot_config_from_disk, save_bot_config, BotConfig, BotStatusResponse,
};
pub use plugins::{
    clear_plugin_output, export_plugin, get_plugin_output, import_plugin, list_plugins,
    open_plugin_data_dir, open_plugin_dir, plugin_output_stream, plugins_events_stream,
    plugins_status_stream, start_plugin, stop_plugin, uninstall_plugin,
};
#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub retcode: i32,
    pub data: T,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub logs: Vec<logger::LogEntry>,
}

#[derive(Serialize)]
pub struct SystemInfo {
    pub port: AtomicU16,
    pub data_dir: String,
    pub plugins_root: String,
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
}

fn default_ui_last_page() -> String {
    "plugins".to_string()
}

fn default_ui_theme() -> String {
    "light".to_string()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UiState {
    #[serde(default = "default_ui_last_page")]
    pub last_page: String,
    #[serde(default = "default_ui_theme")]
    pub theme: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            last_page: default_ui_last_page(),
            theme: default_ui_theme(),
        }
    }
}

impl UiState {
    fn normalized(mut self) -> Self {
        if self.last_page.trim().is_empty() {
            self.last_page = default_ui_last_page();
        }

        if self.theme != "dark" {
            self.theme = default_ui_theme();
        }

        self
    }
}

pub struct PluginCaller {
    pub plugin_id: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PluginCaller {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let auth = match req.headers().get_one("Authorization") {
            Some(v) => v,
            None => return Outcome::Error((Status::Unauthorized, ())),
        };
        let token = auth.strip_prefix("Bearer ").unwrap_or(auth).trim();
        if token.is_empty() {
            return Outcome::Error((Status::Unauthorized, ()));
        }

        let manager = match req.rocket().state::<Arc<PluginManager>>() {
            Some(m) => m,
            None => return Outcome::Error((Status::InternalServerError, ())),
        };

        match manager.get_plugin_id_by_api_token(token).await {
            Some(plugin_id) => Outcome::Success(PluginCaller { plugin_id }),
            None => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

#[derive(Deserialize)]
pub struct SetWebuiRequest {
    pub webui: String,
}

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
    let info = SystemInfoResponse {
        port: system_info.port.load(Ordering::SeqCst),
        data_dir: system_info.data_dir.clone(),
        plugins_root: system_info.plugins_root.clone(),
    };
    Json(ApiResponse {
        retcode: 0,
        data: info,
    })
}

#[derive(Serialize)]
pub struct SystemInfoResponse {
    pub port: u16,
    pub data_dir: String,
    #[serde(rename = "plugins_root")]
    pub plugins_root: String,
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
