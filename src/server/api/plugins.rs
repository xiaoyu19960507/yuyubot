use super::ApiResponse;
use crate::plus::PluginManager;
use rocket::{
    get, post,
    response::stream::{Event, EventStream},
    serde::json::Json,
    State,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
#[get("/plugins/list")]
pub async fn list_plugins(
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<Vec<crate::plus::manager::PluginInfo>>> {
    // 重新加载插件列表
    let _ = manager.load_plugins().await;

    match manager.list_plugins().await {
        Ok(plugins) => Json(ApiResponse {
            retcode: 0,
            data: plugins,
        }),
        Err(e) => {
            log_error!("Failed to list plugins: {}", e);
            Json(ApiResponse {
                retcode: 1,
                data: Vec::new(),
            })
        }
    }
}

#[post("/plugins/<plugin_id>/start")]
pub async fn start_plugin(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    let name = manager
        .get_plugin_name(&plugin_id)
        .await
        .unwrap_or_else(|| plugin_id.clone());
    match manager.start_plugin(&plugin_id).await {
        Ok(_) => {
            log_info!("Plugin {}({}) started", name, plugin_id);
            Json(ApiResponse {
                retcode: 0,
                data: format!("Plugin {}({}) started", name, plugin_id),
            })
        }
        Err(e) => {
            log_error!("Failed to start plugin {}({}): {}", name, plugin_id, e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to start plugin: {}", e),
            })
        }
    }
}

#[post("/plugins/<plugin_id>/stop")]
pub async fn stop_plugin(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    // API 调用被视为用户主动停止
    let name = manager
        .get_plugin_name(&plugin_id)
        .await
        .unwrap_or_else(|| plugin_id.clone());
    match manager.stop_plugin(&plugin_id, true).await {
        Ok(_) => {
            log_info!("Plugin {}({}) stopped", name, plugin_id);
            Json(ApiResponse {
                retcode: 0,
                data: format!("Plugin {}({}) stopped", name, plugin_id),
            })
        }
        Err(e) => {
            log_error!("Failed to stop plugin {}({}): {}", name, plugin_id, e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to stop plugin: {}", e),
            })
        }
    }
}

#[get("/plugins/<plugin_id>/output")]
pub async fn get_plugin_output(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<Vec<String>>> {
    match manager.get_plugin_output(&plugin_id).await {
        Ok(output) => Json(ApiResponse {
            retcode: 0,
            data: output,
        }),
        Err(e) => {
            log_error!("Failed to get plugin output: {}", e);
            Json(ApiResponse {
                retcode: 1,
                data: Vec::new(),
            })
        }
    }
}

#[post("/plugins/<plugin_id>/output/clear")]
pub async fn clear_plugin_output(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.clear_plugin_output(&plugin_id).await {
        Ok(_) => Json(ApiResponse {
            retcode: 0,
            data: "Output cleared".to_string(),
        }),
        Err(e) => {
            log_error!("Failed to clear plugin output: {}", e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to clear output: {}", e),
            })
        }
    }
}

#[post("/plugins/<plugin_id>/open_dir")]
pub async fn open_plugin_dir(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.open_plugin_dir(&plugin_id).await {
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

#[post("/plugins/<plugin_id>/open_data_dir")]
pub async fn open_plugin_data_dir(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.open_plugin_data_dir(&plugin_id).await {
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

#[derive(Serialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum PluginUnifiedEvent {
    Output(crate::plus::manager::PluginOutputEvent),
    Status(crate::plus::manager::PluginStatusEvent),
}

#[get("/plugins/events_stream")]
pub fn plugins_events_stream(manager: &State<Arc<PluginManager>>) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    EventStream! {
        let mut rx_output = manager.subscribe_output();
        let mut rx_status = manager.subscribe_status();

        loop {
            let event = tokio::select! {
                res = rx_output.recv() => match res {
                    Ok(e) => Some(PluginUnifiedEvent::Output(e)),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(_) => None,
                },
                res = rx_status.recv() => match res {
                    Ok(e) => Some(PluginUnifiedEvent::Status(e)),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(_) => None,
                },
            };

            if let Some(event) = event {
                if let Ok(json) = serde_json::to_string(&event) {
                    yield Event::data(json);
                }
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}

#[get("/plugins/status_stream")]
pub fn plugins_status_stream(manager: &State<Arc<PluginManager>>) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    EventStream! {
        let mut rx = manager.subscribe_status();

        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Event::data(json);
                    }
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

#[get("/plugins/<plugin_id>/output/stream")]
pub fn plugin_output_stream(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    let target_plugin = plugin_id.clone();
    EventStream! {
        // 发送现有的输出
        if let Ok(output) = manager.get_plugin_output(&plugin_id).await {
            for line in &output {
                if let Ok(json) = serde_json::to_string(&line) {
                    yield Event::data(json);
                }
            }
        }

        // 订阅实时输出
        let mut rx = manager.subscribe_output();

        // 持续监听新的输出
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if event.plugin_id == target_plugin {
                        if let Ok(json) = serde_json::to_string(&event.line) {
                            yield Event::data(json);
                        }
                    }
                }
                Err(_) => {
                    // 通道关闭，等待一下再重试
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

#[derive(Deserialize)]
pub struct ExportPluginRequest {
    pub plugin_id: String,
}

#[post("/plugins/export", format = "json", data = "<req>")]
pub async fn export_plugin(
    req: Json<ExportPluginRequest>,
    plugin_manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    let plugin_id = &req.plugin_id;

    // Get plugin directory
    let plugin_dir = match plugin_manager.get_plugin_dir(plugin_id).await {
        Some(path) => path,
        None => {
            return Json(ApiResponse {
                retcode: 1,
                data: "Plugin not found".to_string(),
            })
        }
    };

    // Run file dialog and compression in a blocking task
    // clone plugin_id for the closure
    let plugin_id_clone = plugin_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        // Open file dialog
        // Default name: <plugin_id>.yuyu.7z
        let target_path = rfd::FileDialog::new()
            .set_file_name(format!("{}.yuyu.7z", plugin_id_clone))
            .add_filter("Yuyu Plugin", &["yuyu.7z"])
            .add_filter("7z Archive", &["7z"])
            .save_file();

        let target_path = match target_path {
            Some(p) => p,
            None => return Ok("Export cancelled".to_string()),
        };

        sevenz_rust2::compress_to_path(&plugin_dir, &target_path)
            .map_err(|e| format!("Failed to create 7z archive: {}", e))?;

        Ok::<String, String>("Export successful".to_string())
    })
    .await;

    match result {
        Ok(Ok(msg)) => Json(ApiResponse {
            retcode: 0,
            data: msg,
        }),
        Ok(Err(e)) => Json(ApiResponse {
            retcode: 1,
            data: e,
        }),
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: format!("Task failed: {}", e),
        }),
    }
}

#[post("/plugins/<plugin_id>/uninstall")]
pub async fn uninstall_plugin(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.delete_plugin(&plugin_id).await {
        Ok(_) => {
            log_info!("Plugin {} uninstalled", plugin_id);
            Json(ApiResponse {
                retcode: 0,
                data: format!("Plugin {} uninstalled successfully", plugin_id),
            })
        }
        Err(e) => {
            log_error!("Failed to uninstall plugin {}: {}", plugin_id, e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to uninstall plugin: {}", e),
            })
        }
    }
}

#[post("/plugins/import")]
pub async fn import_plugin(
    plugin_manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    let plugins_root = plugin_manager.get_plugins_root();

    let result = tokio::task::spawn_blocking(move || {
        // Open file dialog
        let target_path = rfd::FileDialog::new()
            .add_filter("Yuyu Plugin", &["yuyu.7z"])
            .pick_file();

        let target_path = match target_path {
            Some(p) => p,
            None => return Ok("Import cancelled".to_string()),
        };

        let filename = target_path
            .file_name()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

        if !filename.ends_with(".yuyu.7z") {
            return Err("Invalid plugin file. Must end with .yuyu.7z".to_string());
        }

        // Determine plugin ID from filename
        // e.g. "myplugin.yuyu.7z" -> "myplugin"
        let file_stem = target_path
            .file_stem()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

        // If it ends with .yuyu, remove it
        let plugin_id = if file_stem.ends_with(".yuyu") {
            file_stem.trim_end_matches(".yuyu").to_string()
        } else {
            file_stem
        };

        if plugin_id.is_empty() {
            return Err("Could not determine plugin ID from filename".to_string());
        }

        let target_dir = plugins_root.join(&plugin_id);

        if target_dir.exists() {
            return Err(format!("Plugin directory already exists: {}", plugin_id));
        }

        // Create target directory
        std::fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        let detect_format_and_extract = || -> Result<(), String> {
            use std::io::Read;

            let mut header = [0u8; 6];
            let mut f = std::fs::File::open(&target_path)
                .map_err(|e| format!("Failed to open file: {}", e))?;
            let n = f
                .read(&mut header)
                .map_err(|e| format!("Failed to read file header: {}", e))?;

            if n < 6 || header != [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
                return Err("Invalid plugin archive. Must be a valid 7z file".to_string());
            }

            sevenz_rust2::decompress_file(&target_path, &target_dir)
                .map_err(|e| format!("Failed to extract 7z archive: {}", e))?;

            Ok(())
        };

        if let Err(e) = detect_format_and_extract() {
            let _ = std::fs::remove_dir_all(&target_dir);
            return Err(e);
        }

        Ok::<String, String>(format!("Plugin {} imported successfully", plugin_id))
    })
    .await;

    match result {
        Ok(Ok(msg)) => Json(ApiResponse {
            retcode: 0,
            data: msg,
        }),
        Ok(Err(e)) => Json(ApiResponse {
            retcode: 1,
            data: e,
        }),
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: format!("Task failed: {}", e),
        }),
    }
}
