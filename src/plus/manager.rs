use crate::error::AppResult;
use crate::plus::plugin::{Plugin, PluginManifest, PluginStatus};
use crate::runtime;
use expectrl::{process::Healthcheck, Session};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::thread;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Notify, RwLock};
use tokio::task::spawn_blocking;

#[derive(Serialize, Deserialize, Default)]
pub struct PluginConfig {
    pub enabled_plugins: Vec<String>,
}

pub struct PluginManager {
    plugins: Arc<RwLock<HashMap<String, Arc<Plugin>>>>,
    exe_dir: PathBuf,
    server_port: AtomicU16,
    milky_proxy_host: String,
    milky_proxy_api_port: AtomicU16,
    milky_proxy_event_port: AtomicU16,
    output_sender: broadcast::Sender<PluginOutputEvent>,
    status_sender: broadcast::Sender<PluginStatusEvent>,
    port_ready: Notify,
    milky_ready: Notify,
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
        // 等待大部分时间用于优雅退出
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

    fn get_config_path(&self) -> PathBuf {
        self.exe_dir.join("config").join("plugins.json")
    }

    async fn load_config(&self) -> PluginConfig {
        let config_path = self.get_config_path();
        if let Ok(content) = tokio::fs::read_to_string(&config_path).await {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            PluginConfig::default()
        }
    }

    async fn save_config(&self, config: &PluginConfig) {
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

    async fn add_enabled_plugin(&self, name: &str) {
        let mut config = self.load_config().await;
        if !config.enabled_plugins.contains(&name.to_string()) {
            config.enabled_plugins.push(name.to_string());
            self.save_config(&config).await;
        }
    }

    async fn remove_enabled_plugin(&self, name: &str) {
        let mut config = self.load_config().await;
        config.enabled_plugins.retain(|n| n != name);
        self.save_config(&config).await;
    }

    pub async fn load_plugins(&self) -> AppResult<()> {
        let app_dir = self.exe_dir.join("app");

        // 检查并创建目录
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
                // 只添加新插件，不覆盖已存在的（保留运行状态）
                plugins.entry(id).or_insert_with(|| Arc::new(plugin));
            }
        }

        Ok(())
    }

    async fn load_plugin_from_dir(&self, plugin_dir: &Path) -> AppResult<Plugin> {
        // 使用文件夹名作为插件唯一ID
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

    pub async fn start_plugin(&self, plugin_id: &str) -> Result<(), String> {
        // 等待基础服务端口就绪
        self.wait_for_port().await;
        self.wait_for_milky().await;

        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_id)
            .ok_or("Plugin not found".to_string())?
            .clone();
        drop(plugins);

        let run_id = plugin.begin_run();
        let run_tmp_dir =
            plugin
                .tmp_dir
                .join(format!("run-{}-{}", run_id, generate_tmp_run_suffix()));

        // 复制插件到tmp目录
        self.copy_plugin_to_tmp(&plugin, &run_tmp_dir).await?;

        // 解析 entry 命令行
        let entry_parts: Vec<&str> = plugin.manifest.entry.split_whitespace().collect();
        if entry_parts.is_empty() {
            return Err("Entry cannot be empty".to_string());
        }

        let program = entry_parts[0];
        let args: Vec<String> = entry_parts[1..].iter().map(|s| s.to_string()).collect();

        // 检查程序是否存在于插件目录
        let local_program_path = run_tmp_dir.join(program);
        let program_path = if local_program_path.exists() {
            local_program_path
        } else {
            // 如果本地不存在，假设是系统命令
            PathBuf::from(program)
        };

        // 准备数据目录
        let data_dir = self.exe_dir.join("data").join(plugin_id);
        if tokio::fs::metadata(&data_dir).await.is_err() {
            tokio::fs::create_dir_all(&data_dir)
                .await
                .map_err(|e| format!("Failed to create data dir: {}", e))?;
        }
        let data_dir_str = data_dir.to_string_lossy().to_string();

        plugin.clear_webui().await;

        let plugin_api_token = generate_plugin_api_token();
        plugin.set_api_token(Some(plugin_api_token.clone())).await;

        // 设置插件状态为运行中
        plugin.set_status(PluginStatus::Running).await;
        plugin.set_enabled(true).await;
        plugin.set_process_alive(true).await;

        // 保存启用状态到配置
        self.add_enabled_plugin(plugin_id).await;

        // 使用全局 Runtime 的 handle，确保生命周期一致
        let rt_handle = runtime::get_handle();
        let output_sender = self.output_sender.clone();
        let status_sender = self.status_sender.clone();

        // 使用 expectrl 启动进程
        let plugin_clone = plugin.clone();
        let program_path_clone = program_path.clone();
        let args_clone = args.clone();
        let run_tmp_dir_clone = run_tmp_dir.clone();
        let plugin_id_clone = plugin_id.to_string();
        let server_port = self.server_port.load(Ordering::SeqCst);
        let plugin_api_token_for_env = plugin_api_token.clone();
        let milky_proxy_host = self.milky_proxy_host.clone();
        let milky_proxy_api_port = self.milky_proxy_api_port.load(Ordering::SeqCst);
        let milky_proxy_event_port = self.milky_proxy_event_port.load(Ordering::SeqCst);

        if milky_proxy_api_port == 0 || milky_proxy_event_port == 0 {
            return Err("Milky proxy not available".to_string());
        }

        if !wait_tcp_ready(
            &milky_proxy_host,
            milky_proxy_api_port,
            std::time::Duration::from_secs(2),
        )
        .await
            || !wait_tcp_ready(
                &milky_proxy_host,
                milky_proxy_event_port,
                std::time::Duration::from_secs(2),
            )
            .await
        {
            return Err("Milky proxy not ready".to_string());
        }

        // 用于显示的命令字符串
        let display_cmd = if args.is_empty() {
            program_path.to_string_lossy().to_string()
        } else {
            format!("{} {}", program_path.to_string_lossy(), args.join(" "))
        };

        thread::spawn(move || {
            // 创建命令
            let mut cmd = Command::new(&program_path_clone);
            cmd.args(&args_clone);
            cmd.current_dir(&run_tmp_dir_clone);

            // 传递当前进程的所有环境变量
            for (key, value) in std::env::vars() {
                cmd.env(key, value);
            }

            cmd.env("MILKY_HOST", &milky_proxy_host);
            cmd.env("MILKY_API_PORT", milky_proxy_api_port.to_string());
            cmd.env("MILKY_EVENT_PORT", milky_proxy_event_port.to_string());
            cmd.env("MILKY_TOKEN", &plugin_api_token_for_env);
            cmd.env("YUYU_DATA_DIR", &data_dir_str);
            cmd.env("YUYU_HOST", "localhost");
            cmd.env("YUYU_PORT", server_port.to_string());
            cmd.env("YUYU_TOKEN", &plugin_api_token_for_env);

            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x00000200);

            // 使用 expectrl 启动会话
            match Session::spawn(cmd) {
                Ok(mut session) => {
                    if plugin_clone.is_current_run(run_id) {
                        let msg = format!("[系统] 插件已启动: {}", display_cmd);
                        let plugin_inner = plugin_clone.clone();
                        let sender = output_sender.clone();
                        let id = plugin_id_clone.clone();
                        rt_handle.block_on(async {
                            plugin_inner.add_output(msg.clone()).await;
                        });
                        let _ = sender.send(PluginOutputEvent {
                            plugin_id: id,
                            line: msg,
                        });
                    }

                    let mut buf = [0u8; 4096];

                    // 设置读取超时，这样可以定期检查进程状态
                    session.set_expect_timeout(Some(std::time::Duration::from_millis(500)));

                    loop {
                        // 检查是否应该停止
                        if plugin_clone.should_stop_run(run_id) {
                            let pid = session.get_process().pid();
                            let sent = try_send_ctrl_c(pid);
                            if sent {
                                let deadline = std::time::Instant::now()
                                    + std::time::Duration::from_secs(5);
                                while session.is_alive().unwrap_or(false)
                                    && std::time::Instant::now() < deadline
                                {
                                    match session.try_read(&mut buf) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            let bytes = &buf[..n];
                                            // 去除 ANSI 转义序列
                                            let stripped = strip_ansi_escapes::strip(bytes);
                                            let text =
                                                String::from_utf8_lossy(&stripped).to_string();
                                            process_output(
                                                &rt_handle,
                                                &plugin_clone,
                                                &output_sender,
                                                &plugin_id_clone,
                                                &text,
                                            );
                                        }
                                        Err(ref e)
                                            if e.kind() == std::io::ErrorKind::WouldBlock => {}
                                        Err(_) => {}
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                }
                            }

                            if session.is_alive().unwrap_or(false) {
                                let pid_str = pid.to_string();
                                use std::os::windows::process::CommandExt;
                                let _ = std::process::Command::new("taskkill")
                                    .args(["/PID", pid_str.as_str(), "/F", "/T"])
                                    .creation_flags(0x08000000)
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .output();
                            }
                            break;
                        }

                        // 检查进程是否存活
                        if !session.is_alive().unwrap_or(false) {
                            if plugin_clone.is_current_run(run_id) {
                                rt_handle.block_on(plugin_clone.set_process_alive(false));
                            }
                            // 进程已死，尝试读取剩余数据后退出
                            loop {
                                match session.try_read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let bytes = &buf[..n];
                                        // 去除 ANSI 转义序列
                                        let stripped = strip_ansi_escapes::strip(bytes);
                                        let text = String::from_utf8_lossy(&stripped).to_string();
                                        process_output(
                                            &rt_handle,
                                            &plugin_clone,
                                            &output_sender,
                                            &plugin_id_clone,
                                            &text,
                                        );
                                    }
                                    Err(_) => break,
                                }
                            }
                            break;
                        }

                        // 读取输出（带超时）
                        match session.try_read(&mut buf) {
                            Ok(0) => {
                                // EOF
                                if plugin_clone.is_current_run(run_id) {
                                    rt_handle.block_on(plugin_clone.set_process_alive(false));
                                }
                                break;
                            }
                            Ok(n) => {
                                let bytes = &buf[..n];
                                // 去除 ANSI 转义序列
                                let stripped = strip_ansi_escapes::strip(bytes);
                                let text = String::from_utf8_lossy(&stripped).to_string();
                                process_output(
                                    &rt_handle,
                                    &plugin_clone,
                                    &output_sender,
                                    &plugin_id_clone,
                                    &text,
                                );
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                // 超时，继续循环检查进程状态
                                continue;
                            }
                            Err(e) => {
                                // 检查是否是因为停止导致的错误
                                if plugin_clone.should_stop_run(run_id) {
                                    break;
                                }
                                // 读取错误，可能是进程被杀死
                                if !session.is_alive().unwrap_or(false) {
                                    if plugin_clone.is_current_run(run_id) {
                                        rt_handle.block_on(plugin_clone.set_process_alive(false));
                                    }
                                    break;
                                }
                                // 其他读取错误
                                let err_msg = format!("[错误] 读取输出失败: {}", e);
                                let plugin_inner = plugin_clone.clone();
                                let sender = output_sender.clone();
                                let id = plugin_id_clone.clone();
                                rt_handle.block_on(async {
                                    plugin_inner.add_output(err_msg.clone()).await;
                                });
                                let _ = sender.send(PluginOutputEvent {
                                    plugin_id: id,
                                    line: err_msg,
                                });
                                if plugin_clone.is_current_run(run_id) {
                                    rt_handle.block_on(plugin_clone.set_process_alive(false));
                                }
                                break;
                            }
                        }
                    }

                    // 进程结束后清理 tmp 目录
                    if run_tmp_dir.exists() {
                        // 等待一小段时间确保进程完全释放文件锁
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        let _ = std::fs::remove_dir_all(&run_tmp_dir);
                    }

                    // 只有在清理工作完成后，才将进程标记为死亡
                    // 这样可以避免 stop_all_plugins_and_wait 提前返回，导致 cleanup_tmp_apps 发生竞争
                    if plugin_clone.is_current_run(run_id) {
                        rt_handle.block_on(plugin_clone.set_process_alive(false));
                    }

                    {
                        if plugin_clone.is_current_run(run_id) {
                            let plugin_inner = plugin_clone.clone();
                            let was_stopped = plugin_clone.should_stop_run(run_id);
                            let (msg, new_enabled) = if was_stopped {
                                ("[系统] 插件已被用户停止".to_string(), false)
                            } else {
                                ("[系统] 插件进程已退出".to_string(), true)
                            };
                            let sender = output_sender.clone();
                            let id = plugin_id_clone.clone();
                            rt_handle.block_on(async {
                                plugin_inner.add_output(msg.clone()).await;
                                plugin_inner.set_status(PluginStatus::Stopped).await;
                                plugin_inner.set_api_token(None).await;
                                plugin_inner.clear_webui().await;
                                if !new_enabled {
                                    plugin_inner.set_enabled(false).await;
                                }
                            });
                            let _ = sender.send(PluginOutputEvent {
                                plugin_id: id.clone(),
                                line: msg,
                            });
                            let _ = status_sender.send(PluginStatusEvent {
                                plugin_id: id,
                                status: PluginStatus::Stopped,
                                enabled: new_enabled,
                                webui_url: None,
                            });
                        }
                    }
                }
                Err(e) => {
                    // 清理 tmp 目录
                    if run_tmp_dir.exists() {
                        let _ = std::fs::remove_dir_all(&run_tmp_dir);
                    }

                    // 启动失败，保持 enabled = true（用户可能想重试）
                    // 同样要等到清理完成后才标记
                    if plugin_clone.is_current_run(run_id) {
                        rt_handle.block_on(plugin_clone.set_process_alive(false));
                    }

                    let err_msg = format!("[错误] 启动插件失败: {}", e);
                    let plugin_inner = plugin_clone.clone();
                    let sender = output_sender.clone();
                    let id = plugin_id_clone.clone();
                    if plugin_clone.is_current_run(run_id) {
                        rt_handle.block_on(async {
                            plugin_inner.add_output(err_msg.clone()).await;
                            plugin_inner.set_status(PluginStatus::Error).await;
                            plugin_inner.set_api_token(None).await;
                            plugin_inner.clear_webui().await;
                        });
                        let _ = sender.send(PluginOutputEvent {
                            plugin_id: id.clone(),
                            line: err_msg,
                        });
                        let _ = status_sender.send(PluginStatusEvent {
                            plugin_id: id,
                            status: PluginStatus::Error,
                            enabled: true,
                            webui_url: None,
                        });
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop_plugin(&self, plugin_id: &str, is_user_action: bool) -> Result<(), String> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_id)
            .ok_or("Plugin not found".to_string())?
            .clone();
        drop(plugins);

        let stop_run_id = plugin.request_stop_current_run();

        // 只有用户主动停止时才设置为禁用
        if is_user_action && plugin.is_current_run(stop_run_id) {
            plugin.set_enabled(false).await;
            plugin.set_api_token(None).await;
            plugin.clear_webui().await;
            self.remove_enabled_plugin(plugin_id).await;
        }

        Ok(())
    }

    async fn copy_plugin_to_tmp(&self, plugin: &Plugin, dest_dir: &Path) -> Result<(), String> {
        let src_dir = plugin.plugin_dir.clone();
        let dest_dir = dest_dir.to_path_buf();

        spawn_blocking(move || {
            // 创建tmp目录
            std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

            // 复制所有文件
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

        // 检查插件是否存在
        if let Some(plugin) = plugins.get(plugin_id) {
            // 检查插件是否正在运行
            if plugin.get_status().await == PluginStatus::Running {
                return Err("Cannot delete a running plugin. Please stop it first.".to_string());
            }

            // 删除插件目录（保留数据目录）
            let plugin_dir = plugin.plugin_dir.clone();
            if tokio::fs::metadata(&plugin_dir).await.is_ok() {
                tokio::fs::remove_dir_all(&plugin_dir)
                    .await
                    .map_err(|e| format!("Failed to delete plugin directory: {}", e))?;
            }

            // 从内存中移除
            plugins.remove(plugin_id);
            drop(plugins);

            // 确保从配置中移除
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

    pub async fn set_plugin_webui(
        &self,
        plugin_id: &str,
        webui: String,
    ) -> Result<(), String> {
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
        let _ = self.status_sender.send(PluginStatusEvent {
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

fn process_output(
    rt: &tokio::runtime::Handle,
    plugin: &Arc<Plugin>,
    sender: &broadcast::Sender<PluginOutputEvent>,
    plugin_id: &str,
    text: &str,
) {
    // 按行分割输出
    for line in text.lines() {
        if !line.is_empty() {
            let line_clone = line.to_string();
            let plugin_clone = plugin.clone();
            rt.block_on(async {
                plugin_clone.add_output(line_clone.clone()).await;
            });
            let _ = sender.send(PluginOutputEvent {
                plugin_id: plugin_id.to_string(),
                line: line_clone,
            });
        }
    }
}

#[derive(serde::Serialize)]
pub struct PluginInfo {
    /// 插件唯一ID（文件夹名）
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
        AttachConsole, FreeConsole, GenerateConsoleCtrlEvent, SetConsoleCtrlHandler,
        SetStdHandle, CTRL_C_EVENT, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
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

        // 重新将标准句柄设为 NULL，恢复 GUI 状态
        // 避免因 FreeConsole 导致的句柄无效问题
        SetStdHandle(STD_INPUT_HANDLE, std::ptr::null_mut());
        SetStdHandle(STD_OUTPUT_HANDLE, std::ptr::null_mut());
        SetStdHandle(STD_ERROR_HANDLE, std::ptr::null_mut());

        ok
    }
}
