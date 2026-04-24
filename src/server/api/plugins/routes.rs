use super::ApiResponse;
use crate::plus::PluginManager;
use rocket::{get, post, serde::json::Json, State};
use std::sync::Arc;

#[get("/plugins/list")]
pub async fn list_plugins(
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<Vec<crate::plus::manager::PluginInfo>>> {
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
