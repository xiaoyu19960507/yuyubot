use super::{PluginConfig, PluginManager};

impl PluginManager {
    pub(super) fn get_config_path(&self) -> std::path::PathBuf {
        self.exe_dir.join("config").join("plugins.json")
    }

    pub(super) async fn load_config(&self) -> PluginConfig {
        let config_path = self.get_config_path();
        if let Ok(content) = tokio::fs::read_to_string(&config_path).await {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            PluginConfig::default()
        }
    }

    pub(super) async fn save_config(&self, config: &PluginConfig) {
        let _guard = self.config_lock.lock().await;

        let config_path = self.get_config_path();
        let content = match serde_json::to_string_pretty(config) {
            Ok(c) => c,
            Err(_) => return,
        };

        if let Some(parent) = config_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&config_path, content).await;
    }

    pub async fn get_enabled_plugins(&self) -> Vec<String> {
        let config = self.load_config().await;
        if config.enabled_plugins.is_empty() {
            return Vec::new();
        }

        let plugins = self.plugins.read().await;
        let mut loaded_enabled_plugins = Vec::new();
        let mut new_config_enabled_plugins = Vec::new();
        let mut config_changed = false;

        let plugins_root = self.get_plugins_root();
        for plugin_id in config.enabled_plugins {
            if plugins.contains_key(&plugin_id) {
                loaded_enabled_plugins.push(plugin_id.clone());
                new_config_enabled_plugins.push(plugin_id);
                continue;
            }

            if plugins_root.join(&plugin_id).is_dir() {
                new_config_enabled_plugins.push(plugin_id);
            } else {
                config_changed = true;
            }
        }

        drop(plugins);

        if config_changed {
            self.save_config(&PluginConfig {
                enabled_plugins: new_config_enabled_plugins,
            })
            .await;
        }

        loaded_enabled_plugins
    }

    pub async fn purge_enabled_plugin_if_absent(&self, plugin_id: &str) -> bool {
        if self.get_plugins_root().join(plugin_id).is_dir() {
            return false;
        }
        self.remove_enabled_plugin(plugin_id).await;
        true
    }

    pub(super) async fn add_enabled_plugin(&self, name: &str) {
        let mut config = self.load_config().await;
        if !config.enabled_plugins.contains(&name.to_string()) {
            config.enabled_plugins.push(name.to_string());
            self.save_config(&config).await;
        }
    }

    pub(super) async fn remove_enabled_plugin(&self, name: &str) {
        let mut config = self.load_config().await;
        config.enabled_plugins.retain(|n| n != name);
        self.save_config(&config).await;
    }
}
