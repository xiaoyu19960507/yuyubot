use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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
    pub url: String,
}

pub struct PluginState {
    pub status: PluginStatus,
    pub is_alive: bool,
    pub pid: u32,
    pub output: Vec<String>,
    pub enabled: bool,
    pub api_token: Option<String>,
    pub webui: Option<PluginWebUi>,
}

pub struct Plugin {
    /// 插件唯一ID（文件夹名）
    pub id: String,
    pub manifest: PluginManifest,
    pub plugin_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub run_id: AtomicU64,
    pub stop_run_id: AtomicU64,
    pub state: Mutex<PluginState>,
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
            run_id: AtomicU64::new(0),
            stop_run_id: AtomicU64::new(0),
            state: Mutex::new(PluginState {
                status: PluginStatus::Stopped,
                is_alive: false,
                pid: 0,
                output: Vec::new(),
                enabled: false,
                api_token: None,
                webui: None,
            }),
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
        self.state.lock().await.status
    }

    pub async fn set_status(&self, status: PluginStatus) {
        self.state.lock().await.status = status;
    }

    pub async fn set_process_alive(&self, alive: bool) {
        let mut state = self.state.lock().await;
        state.is_alive = alive;
        if !alive {
            state.pid = 0;
        }
    }

    pub async fn get_pid(&self) -> u32 {
        self.state.lock().await.pid
    }

    pub async fn is_process_alive(&self) -> bool {
        self.state.lock().await.is_alive
    }

    pub async fn is_enabled(&self) -> bool {
        self.state.lock().await.enabled
    }

    pub async fn set_enabled(&self, enabled: bool) {
        self.state.lock().await.enabled = enabled;
    }

    pub async fn get_output(&self) -> Vec<String> {
        self.state.lock().await.output.clone()
    }

    pub async fn add_output(&self, line: String) {
        let mut state = self.state.lock().await;
        state.output.push(line);
        // 限制最大行数
        if state.output.len() > MAX_OUTPUT_LINES {
            state.output.remove(0);
        }
    }

    pub async fn clear_output(&self) {
        self.state.lock().await.output.clear();
    }

    pub async fn set_api_token(&self, token: Option<String>) {
        self.state.lock().await.api_token = token;
    }

    pub async fn get_api_token(&self) -> Option<String> {
        self.state.lock().await.api_token.clone()
    }

    pub async fn set_webui(&self, url: String) {
        self.state.lock().await.webui = Some(PluginWebUi { url });
    }

    pub async fn clear_webui(&self) {
        self.state.lock().await.webui = None;
    }

    pub async fn get_webui_url(&self) -> Option<String> {
        let state = self.state.lock().await;
        let webui = state.webui.as_ref()?;
        Some(webui.url.clone())
    }
}
