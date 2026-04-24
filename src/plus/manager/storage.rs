use super::{copy_dir_all, PluginInfo, PluginManager};
use crate::error::AppResult;
use crate::plus::plugin::{Plugin, PluginManifest, PluginStatus};
use crate::runtime;
use std::path::Path;
use tokio::task::spawn_blocking;

impl PluginManager {
    pub async fn load_plugins(&self) -> AppResult<()> {
        let app_dir = self.exe_dir.join("app");

        if tokio::fs::metadata(&app_dir).await.is_err() {
            tokio::fs::create_dir_all(&app_dir).await?;
            return Ok(());
        }

        let mut dir_entries = Vec::new();
        let mut entries = tokio::fs::read_dir(&app_dir).await?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                dir_entries.push(path);
            }
        }

        if dir_entries.is_empty() {
            return Ok(());
        }

        let mut plugins = self.plugins.write().await;

        for path in dir_entries {
            if let Ok(plugin) = self.load_plugin_from_dir(&path).await {
                let id = plugin.id.clone();
                plugins
                    .entry(id)
                    .or_insert_with(|| std::sync::Arc::new(plugin));
            }
        }

        Ok(())
    }

    async fn load_plugin_from_dir(&self, plugin_dir: &Path) -> AppResult<Plugin> {
        let id = plugin_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                crate::error::AppError::Plugin("Invalid plugin directory name".to_string())
            })?
            .to_string();

        let manifest_path = plugin_dir.join("app.json");
        let manifest_content = tokio::fs::read_to_string(&manifest_path).await?;

        let manifest: PluginManifest = serde_json::from_str(&manifest_content)?;

        let tmp_dir = self.exe_dir.join("tmp").join("app").join(&id);

        Ok(Plugin::new(id, manifest, plugin_dir.to_path_buf(), tmp_dir))
    }

    pub(super) async fn copy_plugin_to_tmp(
        &self,
        plugin: &Plugin,
        dest_dir: &Path,
    ) -> Result<(), String> {
        let src_dir = plugin.plugin_dir.clone();
        let dest_dir = dest_dir.to_path_buf();

        spawn_blocking(move || {
            std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

            for entry in std::fs::read_dir(&src_dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                let file_name = entry.file_name();
                let dest = dest_dir.join(&file_name);

                if path.is_file() {
                    std::fs::copy(&path, &dest).map_err(|e| e.to_string())?;
                } else if path.is_dir() {
                    copy_dir_all(&path, &dest).map_err(|e| e.to_string())?;
                }
            }

            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    pub async fn delete_plugin(&self, plugin_id: &str) -> Result<(), String> {
        let mut plugins = self.plugins.write().await;

        if let Some(plugin) = plugins.get(plugin_id) {
            if plugin.get_status().await == PluginStatus::Running {
                return Err("Cannot delete a running plugin. Please stop it first.".to_string());
            }

            let plugin_dir = plugin.plugin_dir.clone();
            if tokio::fs::metadata(&plugin_dir).await.is_ok() {
                tokio::fs::remove_dir_all(&plugin_dir)
                    .await
                    .map_err(|e| format!("Failed to delete plugin directory: {}", e))?;
            }

            plugins.remove(plugin_id);
            drop(plugins);

            self.remove_enabled_plugin(plugin_id).await;

            Ok(())
        } else {
            Err("Plugin not found".to_string())
        }
    }

    pub async fn list_plugins(&self) -> Result<Vec<PluginInfo>, String> {
        let plugins = self.plugins.read().await;
        let mut result = Vec::new();

        for plugin in plugins.values() {
            let status = plugin.get_status().await;
            let enabled = plugin.is_enabled().await;
            let output = plugin.get_output().await;
            let webui_url = plugin.get_webui_url().await;

            result.push(PluginInfo {
                id: plugin.id.clone(),
                name: plugin.manifest.name.clone(),
                description: plugin.manifest.description.clone(),
                version: plugin.manifest.version.clone(),
                author: plugin.manifest.author.clone(),
                status,
                enabled,
                output,
                webui_url,
            });
        }

        Ok(result)
    }

    pub async fn get_plugin_output(&self, plugin_id: &str) -> Result<Vec<String>, String> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_id)
            .ok_or("Plugin not found".to_string())?
            .clone();
        drop(plugins);

        Ok(plugin.get_output().await)
    }

    pub async fn get_plugin_name(&self, plugin_id: &str) -> Option<String> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id).map(|p| p.manifest.name.clone())
    }

    pub async fn clear_plugin_output(&self, plugin_id: &str) -> Result<(), String> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_id)
            .ok_or("Plugin not found".to_string())?
            .clone();
        drop(plugins);

        plugin.clear_output().await;
        Ok(())
    }

    pub async fn get_plugin_id_by_api_token(&self, token: &str) -> Option<String> {
        let plugins = self.plugins.read().await;
        for (id, plugin) in plugins.iter() {
            if plugin.get_api_token().await.as_deref() == Some(token) {
                return Some(id.clone());
            }
        }
        None
    }

    pub async fn set_plugin_webui(&self, plugin_id: &str, webui: String) -> Result<(), String> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_id)
            .ok_or("Plugin not found".to_string())?
            .clone();
        drop(plugins);
        plugin.set_webui(webui).await;
        let status = plugin.get_status().await;
        let enabled = plugin.is_enabled().await;
        let webui_url = plugin.get_webui_url().await;
        let _ = self.status_sender.send(super::PluginStatusEvent {
            plugin_id: plugin_id.to_string(),
            status,
            enabled,
            webui_url,
        });
        Ok(())
    }

    pub async fn open_plugin_dir(&self, plugin_id: &str) -> Result<(), String> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_id)
            .ok_or("Plugin not found".to_string())?
            .clone();
        drop(plugins);

        let path = plugin.plugin_dir.clone();

        runtime::open_in_explorer(&path);
        Ok(())
    }

    pub async fn open_plugin_data_dir(&self, plugin_id: &str) -> Result<(), String> {
        let data_dir = self.exe_dir.join("data").join(plugin_id);
        let _ = tokio::fs::create_dir_all(&data_dir).await;

        runtime::open_in_explorer(&data_dir);
        Ok(())
    }

    pub async fn open_plugins_root(&self) -> Result<(), String> {
        let plugins_root = self.exe_dir.join("app");
        let _ = tokio::fs::create_dir_all(&plugins_root).await;

        runtime::open_in_explorer(&plugins_root);
        Ok(())
    }
}
