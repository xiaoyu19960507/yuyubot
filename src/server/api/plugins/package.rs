use super::ApiResponse;
use crate::plus::PluginManager;
use rocket::{post, serde::json::Json, State};
use serde::Deserialize;
use std::sync::Arc;

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

    let plugin_dir = match plugin_manager.get_plugin_dir(plugin_id).await {
        Some(path) => path,
        None => {
            return Json(ApiResponse {
                retcode: 1,
                data: "Plugin not found".to_string(),
            })
        }
    };

    let plugin_id_clone = plugin_id.clone();

    let result = tokio::task::spawn_blocking(move || {
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

#[post("/plugins/import")]
pub async fn import_plugin(
    plugin_manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    let plugins_root = plugin_manager.get_plugins_root();

    let result = tokio::task::spawn_blocking(move || {
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

        let file_stem = target_path
            .file_stem()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

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
