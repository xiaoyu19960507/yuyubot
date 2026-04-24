mod config;
mod runtime;
mod storage;

use crate::plus::plugin::{Plugin, PluginStatus};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex, Notify, RwLock};

#[derive(Serialize, Deserialize, Default)]
pub struct PluginConfig {
    pub enabled_plugins: Vec<String>,
}

pub struct PluginManager {
    pub(super) plugins: Arc<RwLock<HashMap<String, Arc<Plugin>>>>,
    pub(super) exe_dir: PathBuf,
    pub(super) server_port: AtomicU16,
    pub(super) milky_proxy_host: String,
    pub(super) milky_proxy_api_port: AtomicU16,
    pub(super) milky_proxy_event_port: AtomicU16,
    pub(super) output_sender: broadcast::Sender<PluginOutputEvent>,
    pub(super) status_sender: broadcast::Sender<PluginStatusEvent>,
    pub(super) port_ready: Notify,
    pub(super) milky_ready: Notify,
    pub(super) config_lock: Mutex<()>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginOutputEvent {
    pub plugin_id: String,
    pub line: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginStatusEvent {
    pub plugin_id: String,
    pub status: PluginStatus,
    pub enabled: bool,
    pub webui_url: Option<String>,
}

#[derive(serde::Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Option<String>,
    pub status: PluginStatus,
    pub enabled: bool,
    pub output: Vec<String>,
    pub webui_url: Option<String>,
}

impl PluginManager {
    pub fn new(
        exe_dir: PathBuf,
        server_port: u16,
        milky_proxy_host: String,
        milky_proxy_api_port: u16,
        milky_proxy_event_port: u16,
    ) -> Self {
        let (output_sender, _) = broadcast::channel(1000);
        let (status_sender, _) = broadcast::channel(100);
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            exe_dir,
            server_port: AtomicU16::new(server_port),
            milky_proxy_host,
            milky_proxy_api_port: AtomicU16::new(milky_proxy_api_port),
            milky_proxy_event_port: AtomicU16::new(milky_proxy_event_port),
            output_sender,
            status_sender,
            port_ready: Notify::new(),
            milky_ready: Notify::new(),
            config_lock: Mutex::new(()),
        }
    }

    pub async fn wait_for_port(&self) {
        if self.server_port.load(Ordering::SeqCst) != 0 {
            return;
        }
        self.port_ready.notified().await;
    }

    pub async fn wait_for_milky(&self) {
        if self.milky_proxy_api_port.load(Ordering::SeqCst) != 0
            && self.milky_proxy_event_port.load(Ordering::SeqCst) != 0
        {
            return;
        }
        self.milky_ready.notified().await;
    }

    pub fn subscribe_output(&self) -> broadcast::Receiver<PluginOutputEvent> {
        self.output_sender.subscribe()
    }

    pub fn set_server_port(&self, port: u16) {
        self.server_port.store(port, Ordering::SeqCst);
        self.port_ready.notify_waiters();
    }

    pub fn set_milky_proxy_api_port(&self, port: u16) {
        self.milky_proxy_api_port.store(port, Ordering::SeqCst);
        if self.milky_proxy_event_port.load(Ordering::SeqCst) != 0 {
            self.milky_ready.notify_waiters();
        }
    }

    pub fn set_milky_proxy_event_port(&self, port: u16) {
        self.milky_proxy_event_port.store(port, Ordering::SeqCst);
        if self.milky_proxy_api_port.load(Ordering::SeqCst) != 0 {
            self.milky_ready.notify_waiters();
        }
    }

    pub fn subscribe_status(&self) -> broadcast::Receiver<PluginStatusEvent> {
        self.status_sender.subscribe()
    }

    pub async fn get_plugin_dir(&self, plugin_id: &str) -> Option<PathBuf> {
        let plugins = self.plugins.read().await;
        plugins.get(plugin_id).map(|p| p.plugin_dir.clone())
    }

    pub fn get_plugins_root(&self) -> PathBuf {
        self.exe_dir.join("app")
    }

    pub async fn cleanup_tmp_apps(&self) {
        let tmp_apps_dir = self.exe_dir.join("tmp").join("app");

        if tokio::fs::metadata(&tmp_apps_dir).await.is_err() {
            return;
        }

        let mut last_err = None;
        for _ in 0..40 {
            if let Ok(mut entries) = tokio::fs::read_dir(&tmp_apps_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    let _ = tokio::fs::remove_dir_all(&path).await;
                }
            }

            match tokio::fs::remove_dir_all(&tmp_apps_dir).await {
                Ok(_) => return,
                Err(e) => last_err = Some(e),
            }

            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }

        if tokio::fs::metadata(&tmp_apps_dir).await.is_ok() {
            if let Some(e) = last_err {
                log_warn!("Failed to cleanup tmp/app: {}", e);
            } else {
                log_warn!("Failed to cleanup tmp/app");
            }
        }
    }

    pub async fn stop_all_plugins_and_wait(&self, max_wait: std::time::Duration) {
        let plugins = self.plugins.read().await;
        let plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        let plugin_refs: Vec<Arc<Plugin>> = plugins.values().cloned().collect();
        drop(plugins);

        for id in plugin_ids {
            let _ = self.stop_plugin(&id, false).await;
        }

        let deadline = std::time::Instant::now() + max_wait;
        let force_kill_threshold = deadline
            .checked_sub(std::time::Duration::from_secs(1))
            .unwrap_or(std::time::Instant::now());

        loop {
            let mut any_alive = false;
            let now = std::time::Instant::now();
            let should_force_kill = now >= force_kill_threshold;

            for plugin in &plugin_refs {
                if plugin.is_process_alive().await {
                    any_alive = true;

                    if should_force_kill {
                        let pid = plugin.get_pid().await;
                        if pid > 0 {
                            log_warn!("Force killing plugin process: {}", pid);
                            use std::os::windows::process::CommandExt;
                            let _ = std::process::Command::new("taskkill")
                                .args(["/PID", &pid.to_string(), "/F", "/T"])
                                .creation_flags(0x08000000)
                                .stdin(std::process::Stdio::null())
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .output();
                        }
                    }
                }
            }

            if !any_alive {
                break;
            }

            if now >= deadline {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

fn process_output(
    rt: &tokio::runtime::Handle,
    plugin: &Arc<Plugin>,
    sender: &broadcast::Sender<PluginOutputEvent>,
    plugin_id: &str,
    text: &str,
) {
    for line in text.lines() {
        if !line.is_empty() {
            let line_clone = line.to_string();
            let plugin_clone = plugin.clone();
            let line_for_async = line_clone.clone();
            let _handle = rt.spawn(async move {
                plugin_clone.add_output(line_for_async).await;
            });
            let _ = sender.send(PluginOutputEvent {
                plugin_id: plugin_id.to_string(),
                line: line_clone,
            });
        }
    }
}

fn generate_plugin_api_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    let mut token = String::with_capacity(64);
    for b in bytes {
        let _ = write!(&mut token, "{:02x}", b);
    }
    token
}

fn generate_tmp_run_suffix() -> String {
    let mut bytes = [0u8; 8];
    rand::rng().fill(&mut bytes);
    let mut out = String::with_capacity(16);
    for b in bytes {
        let _ = write!(&mut out, "{:02x}", b);
    }
    out
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest = dst.join(&file_name);

        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

async fn wait_tcp_ready(host: &str, port: u16, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if TcpStream::connect((host, port)).await.is_ok() {
            return true;
        }

        if std::time::Instant::now() >= deadline {
            return false;
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

fn try_send_ctrl_c(pid: u32) -> bool {
    use windows_sys::Win32::System::Console::{
        AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler, SetStdHandle,
        CTRL_C_EVENT, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    unsafe {
        let _ = FreeConsole();
        if AttachConsole(pid) == 0 {
            return false;
        }

        if SetConsoleCtrlHandler(None, 1) == 0 {
            let _ = FreeConsole();
            return false;
        }

        let ok = GenerateConsoleCtrlEvent(CTRL_C_EVENT, pid) != 0;

        std::thread::sleep(std::time::Duration::from_millis(50));
        let _ = FreeConsole();
        let _ = SetConsoleCtrlHandler(None, 0);

        SetStdHandle(STD_INPUT_HANDLE, std::ptr::null_mut());
        SetStdHandle(STD_OUTPUT_HANDLE, std::ptr::null_mut());
        SetStdHandle(STD_ERROR_HANDLE, std::ptr::null_mut());

        ok
    }
}
