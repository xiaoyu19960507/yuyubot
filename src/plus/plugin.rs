use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

/// 插件输出最大行数限制
pub const MAX_OUTPUT_LINES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub entry: String,
    pub description: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginStatus {
    Stopped,
    Running,
    Error,
}

pub struct Plugin {
    /// 插件唯一ID（文件夹名）
    pub id: String,
    pub manifest: PluginManifest,
    pub plugin_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub status: Arc<Mutex<PluginStatus>>,
    pub is_alive: Arc<AtomicBool>,
    pub should_stop: Arc<AtomicBool>,
    pub output: Arc<Mutex<Vec<String>>>,
    pub enabled: Arc<Mutex<bool>>,
}

impl Plugin {
    pub fn new(id: String, manifest: PluginManifest, plugin_dir: PathBuf, tmp_dir: PathBuf) -> Self {
        Self {
            id,
            manifest,
            plugin_dir,
            tmp_dir,
            status: Arc::new(Mutex::new(PluginStatus::Stopped)),
            is_alive: Arc::new(AtomicBool::new(false)),
            should_stop: Arc::new(AtomicBool::new(false)),
            output: Arc::new(Mutex::new(Vec::new())),
            enabled: Arc::new(Mutex::new(false)),
        }
    }

    pub fn should_stop(&self) -> bool {
        self.should_stop.load(Ordering::Relaxed)
    }

    pub fn set_should_stop(&self, stop: bool) {
        self.should_stop.store(stop, Ordering::Relaxed);
    }

    pub async fn get_status(&self) -> PluginStatus {
        *self.status.lock().await
    }

    pub async fn set_status(&self, status: PluginStatus) {
        *self.status.lock().await = status;
    }

    pub fn set_process_alive(&self, alive: bool) {
        self.is_alive.store(alive, Ordering::Relaxed);
    }

    pub async fn is_enabled(&self) -> bool {
        *self.enabled.lock().await
    }

    pub async fn set_enabled(&self, enabled: bool) {
        *self.enabled.lock().await = enabled;
    }

    pub async fn get_output(&self) -> Vec<String> {
        self.output.lock().await.clone()
    }

    pub async fn add_output(&self, line: String) {
        let mut output = self.output.lock().await;
        output.push(line);
        // 限制最大行数
        if output.len() > MAX_OUTPUT_LINES {
            output.remove(0);
        }
    }

    pub async fn clear_output(&self) {
        self.output.lock().await.clear();
    }
}
