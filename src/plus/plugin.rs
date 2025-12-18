use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
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

#[derive(Debug, Clone)]
pub struct PluginWebUi {
    pub webui: String,
    pub port: u16,
}

pub struct Plugin {
    /// 插件唯一ID（文件夹名）
    pub id: String,
    pub manifest: PluginManifest,
    pub plugin_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub status: Arc<Mutex<PluginStatus>>,
    pub is_alive: Arc<AtomicBool>,
    pub pid: Arc<AtomicU32>,
    pub run_id: Arc<AtomicU64>,
    pub stop_run_id: Arc<AtomicU64>,
    pub output: Arc<Mutex<Vec<String>>>,
    pub enabled: Arc<Mutex<bool>>,
    pub api_token: Arc<Mutex<Option<String>>>,
    pub webui: Arc<Mutex<Option<PluginWebUi>>>,
}

impl Plugin {
    pub fn new(
        id: String,
        manifest: PluginManifest,
        plugin_dir: PathBuf,
        tmp_dir: PathBuf,
    ) -> Self {
        Self {
            id,
            manifest,
            plugin_dir,
            tmp_dir,
            status: Arc::new(Mutex::new(PluginStatus::Stopped)),
            is_alive: Arc::new(AtomicBool::new(false)),
            pid: Arc::new(AtomicU32::new(0)),
            run_id: Arc::new(AtomicU64::new(0)),
            stop_run_id: Arc::new(AtomicU64::new(0)),
            output: Arc::new(Mutex::new(Vec::new())),
            enabled: Arc::new(Mutex::new(false)),
            api_token: Arc::new(Mutex::new(None)),
            webui: Arc::new(Mutex::new(None)),
        }
    }

    pub fn begin_run(&self) -> u64 {
        let new_run_id = self.run_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.stop_run_id.store(0, Ordering::Relaxed);
        new_run_id
    }

    pub fn current_run_id(&self) -> u64 {
        self.run_id.load(Ordering::Relaxed)
    }

    pub fn is_current_run(&self, run_id: u64) -> bool {
        self.current_run_id() == run_id
    }

    pub fn request_stop_current_run(&self) -> u64 {
        let run_id = self.current_run_id();
        self.stop_run_id.store(run_id, Ordering::Relaxed);
        run_id
    }

    pub fn should_stop_run(&self, run_id: u64) -> bool {
        run_id != 0 && self.stop_run_id.load(Ordering::Relaxed) == run_id
    }

    pub async fn get_status(&self) -> PluginStatus {
        *self.status.lock().await
    }

    pub async fn set_status(&self, status: PluginStatus) {
        *self.status.lock().await = status;
    }

    pub fn set_process_alive(&self, alive: bool) {
        self.is_alive.store(alive, Ordering::Relaxed);
        if !alive {
            self.pid.store(0, Ordering::Relaxed);
        }
    }

    pub fn get_pid(&self) -> u32 {
        self.pid.load(Ordering::Relaxed)
    }

    pub fn is_process_alive(&self) -> bool {
        self.is_alive.load(Ordering::Relaxed)
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

    pub async fn set_api_token(&self, token: Option<String>) {
        *self.api_token.lock().await = token;
    }

    pub async fn get_api_token(&self) -> Option<String> {
        self.api_token.lock().await.clone()
    }

    pub async fn set_webui(&self, webui: String, port: u16) {
        *self.webui.lock().await = Some(PluginWebUi { webui, port });
    }

    pub async fn clear_webui(&self) {
        *self.webui.lock().await = None;
    }

    pub async fn get_webui_url(&self) -> Option<String> {
        let config = self.webui.lock().await.clone()?;
        let mut path = config.webui;
        if path.is_empty() {
            path = "/".to_string();
        }
        if !path.starts_with('/') {
            path = format!("/{}", path);
        }
        Some(format!("http://localhost:{}{}", config.port, path))
    }
}
