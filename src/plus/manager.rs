use super::plugin::{Plugin, PluginManifest, PluginStatus};
use expectrl::{process::Healthcheck, Session};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::thread;
use tokio::sync::{broadcast, RwLock};
use crate::server::api::BotConfig;

#[derive(Serialize, Deserialize, Default)]
pub struct PluginConfig {
    pub enabled_plugins: Vec<String>,
}

pub struct PluginManager {
    plugins: Arc<RwLock<HashMap<String, Arc<Plugin>>>>,
    exe_dir: PathBuf,
    output_sender: broadcast::Sender<PluginOutputEvent>,
    status_sender: broadcast::Sender<PluginStatusEvent>,
}

#[derive(Clone, Debug)]
pub struct PluginOutputEvent {
    pub plugin_id: String,
    pub line: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PluginStatusEvent {
    pub plugin_id: String,
    pub status: PluginStatus,
    pub enabled: bool,
}

impl PluginManager {
    pub fn new(exe_dir: PathBuf) -> Self {
        let (output_sender, _) = broadcast::channel(1000);
        let (status_sender, _) = broadcast::channel(100);
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            exe_dir,
            output_sender,
            status_sender,
        }
    }

    pub fn subscribe_output(&self) -> broadcast::Receiver<PluginOutputEvent> {
        self.output_sender.subscribe()
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

    fn get_config_path(&self) -> PathBuf {
        self.exe_dir.join("config").join("plugins.json")
    }

    fn load_config(&self) -> PluginConfig {
        let config_path = self.get_config_path();
        if let Ok(content) = std::fs::read_to_string(&config_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            PluginConfig::default()
        }
    }

    fn save_config(&self, config: &PluginConfig) {
        let config_path = self.get_config_path();
        if let Some(parent) = config_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(content) = serde_json::to_string_pretty(config) {
            let _ = std::fs::write(&config_path, content);
        }
    }

    pub fn get_enabled_plugins(&self) -> Vec<String> {
        self.load_config().enabled_plugins
    }

    fn add_enabled_plugin(&self, name: &str) {
        let mut config = self.load_config();
        if !config.enabled_plugins.contains(&name.to_string()) {
            config.enabled_plugins.push(name.to_string());
            self.save_config(&config);
        }
    }

    fn remove_enabled_plugin(&self, name: &str) {
        let mut config = self.load_config();
        config.enabled_plugins.retain(|n| n != name);
        self.save_config(&config);
    }

    pub async fn load_plugins(&self) -> Result<(), String> {
        let app_dir = self.exe_dir.join("app");

        if !app_dir.exists() {
            std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
            return Ok(());
        }

        let mut plugins = self.plugins.write().await;

        for entry in std::fs::read_dir(&app_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            if path.is_dir() {
                if let Ok(plugin) = self.load_plugin_from_dir(&path).await {
                    let id = plugin.id.clone();
                    // 只添加新插件，不覆盖已存在的（保留运行状态）
                    if !plugins.contains_key(&id) {
                        plugins.insert(id, Arc::new(plugin));
                    }
                }
            }
        }

        Ok(())
    }

    async fn load_plugin_from_dir(&self, plugin_dir: &Path) -> Result<Plugin, String> {
        // 使用文件夹名作为插件唯一ID
        let id = plugin_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid plugin directory name")?
            .to_string();

        let manifest_path = plugin_dir.join("app.json");
        let manifest_content =
            std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
        let manifest: PluginManifest =
            serde_json::from_str(&manifest_content).map_err(|e| e.to_string())?;

        let tmp_dir = self.exe_dir.join("tmp").join("app").join(&id);

        Ok(Plugin::new(id, manifest, plugin_dir.to_path_buf(), tmp_dir))
    }

    pub async fn start_plugin(&self, plugin_id: &str) -> Result<(), String> {
        let plugins = self.plugins.read().await;
        let plugin = plugins
            .get(plugin_id)
            .ok_or("Plugin not found".to_string())?
            .clone();
        drop(plugins);

        // 复制插件到tmp目录
        self.copy_plugin_to_tmp(&plugin).await?;

        // 解析 entry 命令行
        let entry_parts: Vec<&str> = plugin.manifest.entry.split_whitespace().collect();
        if entry_parts.is_empty() {
            return Err("Entry cannot be empty".to_string());
        }

        let program = entry_parts[0];
        let args: Vec<String> = entry_parts[1..].iter().map(|s| s.to_string()).collect();

        // 检查程序是否存在于插件目录
        let local_program_path = plugin.tmp_dir.join(program);
        let program_path = if local_program_path.exists() {
             local_program_path
        } else {
             // 如果本地不存在，假设是系统命令
             PathBuf::from(program)
        };

        // 读取 Bot 配置
        let config_path = self.exe_dir.join("config").join("config.json");
        let bot_config = if let Ok(content) = std::fs::read_to_string(&config_path) {
            serde_json::from_str::<BotConfig>(&content).unwrap_or_else(|_| {
                // 如果解析失败，使用默认配置
                BotConfig {
                    host: "localhost".to_string(),
                    api_port: 3010,
                    event_port: 3011,
                    token: None,
                    auto_connect: false,
                }
            })
        } else {
            // 配置文件不存在，使用默认配置
            BotConfig {
                host: "localhost".to_string(),
                api_port: 3010,
                event_port: 3011,
                token: None,
                auto_connect: false,
            }
        };

        // 准备数据目录
        let data_dir = self.exe_dir.join("data").join(plugin_id);
        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
        }
        let data_dir_str = data_dir.to_string_lossy().to_string();

        // 重置停止标志
        plugin.set_should_stop(false);
        
        // 设置插件状态为运行中
        plugin.set_status(PluginStatus::Running).await;
        plugin.set_enabled(true).await;
        plugin.set_process_alive(true);

        // 保存启用状态到配置
        self.add_enabled_plugin(plugin_id);

        // 在启动线程之前获取 runtime handle 和 senders
        let rt_handle = tokio::runtime::Handle::current();
        let output_sender = self.output_sender.clone();
        let status_sender = self.status_sender.clone();

        // 使用 expectrl 启动进程
        let plugin_clone = plugin.clone();
        let program_path_clone = program_path.clone();
        let args_clone = args.clone();
        let tmp_dir_clone = plugin.tmp_dir.clone();
        let plugin_id_clone = plugin_id.to_string();

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
            cmd.current_dir(&tmp_dir_clone);

            // 传递当前进程的所有环境变量
            for (key, value) in std::env::vars() {
                cmd.env(key, value);
            }

            // 设置环境变量
            cmd.env("YUYU_HOST", &bot_config.host);
            cmd.env("YUYU_API_PORT", &bot_config.api_port.to_string());
            cmd.env("YUYU_EVENT_PORT", &bot_config.event_port.to_string());
            if let Some(token) = &bot_config.token {
                cmd.env("YUYU_TOKEN", token);
            }
            cmd.env("YUYU_DATA_DIR", &data_dir_str);


            // 使用 expectrl 启动会话
            match Session::spawn(cmd) {
                Ok(mut session) => {
                    // 添加启动成功消息
                    {
                        let msg = format!("[系统] 插件已启动: {}", display_cmd);
                        let plugin_inner = plugin_clone.clone();
                        let sender = output_sender.clone();
                        let id = plugin_id_clone.clone();
                        rt_handle.block_on(async {
                            plugin_inner.add_output(msg.clone()).await;
                        });
                        let _ = sender.send(PluginOutputEvent { plugin_id: id, line: msg });
                    }

                    let mut buf = [0u8; 4096];
                    
                    // 设置读取超时，这样可以定期检查进程状态
                    session.set_expect_timeout(Some(std::time::Duration::from_millis(500)));

                    loop {
                        // 检查是否应该停止
                        if plugin_clone.should_stop() {
                            // 尝试杀死进程，特别是对于 windows_subsystem = "windows" 的插件
                            #[cfg(windows)]
                            {
                                // expectrl 的 WinProcess 可能没有 kill 方法，通过 PID 强制终止
                                let pid = session.get_process().pid();
                                // 使用 creation_flags(0x08000000) (CREATE_NO_WINDOW) 隐藏窗口
                                use std::os::windows::process::CommandExt;
                                let _ = std::process::Command::new("taskkill")
                                    .args(&["/PID", &pid.to_string(), "/F"])
                                    .creation_flags(0x08000000)
                                    .output();
                            }
                            break;
                        }
                        
                        // 检查进程是否存活
                        if !session.is_alive().unwrap_or(false) {
                            plugin_clone.set_process_alive(false);
                            // 进程已死，尝试读取剩余数据后退出
                            loop {
                                match session.try_read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                                        process_output(&rt_handle, &plugin_clone, &output_sender, &plugin_id_clone, &text);
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
                                plugin_clone.set_process_alive(false);
                                break;
                            }
                            Ok(n) => {
                                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                                process_output(&rt_handle, &plugin_clone, &output_sender, &plugin_id_clone, &text);
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                // 超时，继续循环检查进程状态
                                continue;
                            }
                            Err(e) => {
                                // 检查是否是因为停止导致的错误
                                if plugin_clone.should_stop() {
                                    break;
                                }
                                // 读取错误，可能是进程被杀死
                                if !session.is_alive().unwrap_or(false) {
                                    plugin_clone.set_process_alive(false);
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
                                let _ = sender.send(PluginOutputEvent { plugin_id: id, line: err_msg });
                                plugin_clone.set_process_alive(false);
                                break;
                            }
                        }
                    }

                    // 进程结束
                    plugin_clone.set_process_alive(false);

                    // 进程结束后清理 tmp 目录
                    if plugin_clone.tmp_dir.exists() {
                        // 等待一小段时间确保进程完全释放文件锁
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        let _ = std::fs::remove_dir_all(&plugin_clone.tmp_dir);
                    }

                    {
                        let plugin_inner = plugin_clone.clone();
                        let was_stopped = plugin_clone.should_stop();
                        let (msg, new_enabled) = if was_stopped {
                            // 用户主动停止，设置 enabled = false
                            ("[系统] 插件已被用户停止".to_string(), false)
                        } else {
                            // 意外退出，保持 enabled = true（下次启动时会自动重启）
                            ("[系统] 插件进程已退出".to_string(), true)
                        };
                        let sender = output_sender.clone();
                        let id = plugin_id_clone.clone();
                        rt_handle.block_on(async {
                            plugin_inner.add_output(msg.clone()).await;
                            plugin_inner.set_status(PluginStatus::Stopped).await;
                            if !new_enabled {
                                plugin_inner.set_enabled(false).await;
                            }
                        });
                        let _ = sender.send(PluginOutputEvent { plugin_id: id.clone(), line: msg });
                        // 发送状态变化事件
                        let _ = status_sender.send(PluginStatusEvent {
                            plugin_id: id,
                            status: PluginStatus::Stopped,
                            enabled: new_enabled,
                        });
                    }
                }
                Err(e) => {
                    // 启动失败，保持 enabled = true（用户可能想重试）
                    plugin_clone.set_process_alive(false);

                    // 清理 tmp 目录
                    if plugin_clone.tmp_dir.exists() {
                        let _ = std::fs::remove_dir_all(&plugin_clone.tmp_dir);
                    }

                    let err_msg = format!("[错误] 启动插件失败: {}", e);
                    let plugin_inner = plugin_clone.clone();
                    let sender = output_sender.clone();
                    let id = plugin_id_clone.clone();
                    rt_handle.block_on(async {
                        plugin_inner.add_output(err_msg.clone()).await;
                        plugin_inner.set_status(PluginStatus::Error).await;
                        // 不修改 enabled 状态
                    });
                    let _ = sender.send(PluginOutputEvent { plugin_id: id.clone(), line: err_msg });
                    // 发送状态变化事件
                    let _ = status_sender.send(PluginStatusEvent {
                        plugin_id: id,
                        status: PluginStatus::Error,
                        enabled: true,
                    });
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

        // 设置停止标志，让读取线程退出
        plugin.set_should_stop(true);

        // 只有用户主动停止时才设置为禁用
        if is_user_action {
            plugin.set_enabled(false).await;
            // 从配置中移除启用状态
            self.remove_enabled_plugin(plugin_id);
        }

        Ok(())
    }

    async fn copy_plugin_to_tmp(&self, plugin: &Plugin) -> Result<(), String> {
        // 创建tmp目录
        std::fs::create_dir_all(&plugin.tmp_dir).map_err(|e| e.to_string())?;

        // 复制所有文件
        for entry in std::fs::read_dir(&plugin.plugin_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let file_name = entry.file_name();
            let dest = plugin.tmp_dir.join(&file_name);

            if path.is_file() {
                std::fs::copy(&path, &dest).map_err(|e| e.to_string())?;
            } else if path.is_dir() {
                copy_dir_all(&path, &dest).map_err(|e| e.to_string())?;
            }
        }

        Ok(())
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
            if plugin.plugin_dir.exists() {
                std::fs::remove_dir_all(&plugin.plugin_dir).map_err(|e| format!("Failed to delete plugin directory: {}", e))?;
            }
            
            // 从内存中移除
            plugins.remove(plugin_id);
            
            // 确保从配置中移除
            self.remove_enabled_plugin(plugin_id);
            
            Ok(())
        } else {
            Err("Plugin not found".to_string())
        }
    }

    pub async fn stop_all_plugins(&self) {
        let plugins = self.plugins.read().await;
        let plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        drop(plugins);

        for id in plugin_ids {
            // 系统退出时的停止，不算作用户主动停止
            let _ = self.stop_plugin(&id, false).await;
        }
    }

    pub async fn list_plugins(&self) -> Result<Vec<PluginInfo>, String> {
        let plugins = self.plugins.read().await;
        let mut result = Vec::new();

        for plugin in plugins.values() {
            let status = plugin.get_status().await;
            let enabled = plugin.is_enabled().await;
            let output = plugin.get_output().await;

            result.push(PluginInfo {
                id: plugin.id.clone(),
                name: plugin.manifest.name.clone(),
                description: plugin.manifest.description.clone(),
                version: plugin.manifest.version.clone(),
                author: plugin.manifest.author.clone(),
                status,
                enabled,
                output,
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
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(&dst)?;
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
