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
            let mut new_config = self.load_config().await;
            new_config.enabled_plugins = new_config_enabled_plugins;
            self.save_config(&new_config).await;
        }

        loaded_enabled_plugins
    }

    pub async fn purge_enabled_plugin_if_absent(&self, plugin_id: &str) -> bool {
        if self.get_plugins_root().join(plugin_id).is_dir() {
            return false;
        }
        self.remove_enabled_plugin(plugin_id).await;
        self.remove_from_plugin_order(plugin_id).await;
        true
    }

    pub(super) async fn add_enabled_plugin(&self, name: &str) {
        let mut config = self.load_config().await;
        if !config.enabled_plugins.contains(&name.to_string()) {
            config.enabled_plugins.push(name.to_string());
        }
        if !config.plugin_order.contains(&name.to_string()) {
            config.plugin_order.push(name.to_string());
        }
        self.save_config(&config).await;
    }

    pub(super) async fn remove_enabled_plugin(&self, name: &str) {
        let mut config = self.load_config().await;
        config.enabled_plugins.retain(|n| n != name);
        self.save_config(&config).await;
    }

    pub async fn get_plugin_order(&self) -> Vec<String> {
        let mut config = self.load_config().await;
        let plugins = self.plugins.read().await;
        let mut plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        plugin_ids.sort();

        for id in &plugin_ids {
            if !config.plugin_order.contains(id) {
                config.plugin_order.push(id.clone());
            }
        }
        config.plugin_order.retain(|id| plugins.contains_key(id));
        drop(plugins);
        self.save_config(&config).await;
        config.plugin_order
    }

    pub async fn save_plugin_order(&self, order: Vec<String>) {
        let mut config = self.load_config().await;
        config.plugin_order = order;
        self.save_config(&config).await;
    }

    pub(super) async fn remove_from_plugin_order(&self, plugin_id: &str) {
        let mut config = self.load_config().await;
        config.plugin_order.retain(|id| id != plugin_id);
        self.save_config(&config).await;
    }
}
