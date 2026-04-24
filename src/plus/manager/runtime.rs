use super::{
    generate_plugin_api_token, generate_tmp_run_suffix, process_output, try_send_ctrl_c,
    wait_tcp_ready, PluginManager, PluginOutputEvent, PluginStatusEvent,
};
use crate::plus::plugin::PluginStatus;
use crate::runtime;
use expectrl::{process::Healthcheck, Session};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::Ordering;
use std::thread;

impl PluginManager {
    pub async fn start_plugin(&self, plugin_id: &str) -> Result<(), String> {
        self.wait_for_port().await;
        self.wait_for_milky().await;

        let (plugin, run_id) = {
            let plugins = self.plugins.read().await;
            let plugin = plugins
                .get(plugin_id)
                .ok_or("Plugin not found".to_string())?
                .clone();
            let run_id = plugin.begin_run();
            (plugin, run_id)
        };
        let run_tmp_dir =
            plugin
                .tmp_dir
                .join(format!("run-{}-{}", run_id, generate_tmp_run_suffix()));

        self.copy_plugin_to_tmp(&plugin, &run_tmp_dir).await?;

        let entry_parts: Vec<&str> = plugin.manifest.entry.split_whitespace().collect();
        if entry_parts.is_empty() {
            return Err("Entry cannot be empty".to_string());
        }

        let program = entry_parts[0];
        let args: Vec<String> = entry_parts[1..].iter().map(|s| s.to_string()).collect();

        let local_program_path = run_tmp_dir.join(program);
        let program_path = if local_program_path.exists() {
            local_program_path
        } else {
            PathBuf::from(program)
        };

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

        plugin.set_status(PluginStatus::Running).await;
        plugin.set_enabled(true).await;
        plugin.set_process_alive(true).await;

        self.add_enabled_plugin(plugin_id).await;

        let rt_handle = runtime::get_handle();
        let output_sender = self.output_sender.clone();
        let status_sender = self.status_sender.clone();

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

        let display_cmd = if args.is_empty() {
            program_path.to_string_lossy().to_string()
        } else {
            format!("{} {}", program_path.to_string_lossy(), args.join(" "))
        };

        thread::spawn(move || {
            let mut cmd = Command::new(&program_path_clone);
            cmd.args(&args_clone);
            cmd.current_dir(&run_tmp_dir_clone);

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

            match Session::spawn(cmd) {
                Ok(mut session) => {
                    if plugin_clone.is_current_run(run_id) {
                        let msg = format!("[系统] 插件已启动: {}", display_cmd);
                        let plugin_inner = plugin_clone.clone();
                        let sender = output_sender.clone();
                        let id = plugin_id_clone.clone();
                        let msg_for_async = msg.clone();
                        let _handle = rt_handle.spawn(async move {
                            plugin_inner.add_output(msg_for_async).await;
                        });
                        let _ = sender.send(PluginOutputEvent {
                            plugin_id: id,
                            line: msg,
                        });
                    }

                    let mut buf = [0u8; 4096];
                    session.set_expect_timeout(Some(std::time::Duration::from_millis(500)));

                    loop {
                        if plugin_clone.should_stop_run(run_id) {
                            let pid = session.get_process().pid();
                            let sent = try_send_ctrl_c(pid);
                            if sent {
                                let deadline =
                                    std::time::Instant::now() + std::time::Duration::from_secs(3);
                                while session.is_alive().unwrap_or(false)
                                    && std::time::Instant::now() < deadline
                                {
                                    match session.try_read(&mut buf) {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            let bytes = &buf[..n];
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
                                    std::thread::sleep(std::time::Duration::from_millis(50));
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
                        }

                        if !session.is_alive().unwrap_or(false) {
                            if plugin_clone.is_current_run(run_id) {
                                rt_handle.block_on(plugin_clone.set_process_alive(false));
                            }
                            loop {
                                match session.try_read(&mut buf) {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let bytes = &buf[..n];
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

                        match session.try_read(&mut buf) {
                            Ok(0) => {
                                if plugin_clone.is_current_run(run_id) {
                                    rt_handle.block_on(plugin_clone.set_process_alive(false));
                                }
                                break;
                            }
                            Ok(n) => {
                                let bytes = &buf[..n];
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
                                std::thread::sleep(std::time::Duration::from_millis(30));
                                continue;
                            }
                            Err(e) => {
                                if plugin_clone.should_stop_run(run_id) {
                                    break;
                                }
                                if !session.is_alive().unwrap_or(false) {
                                    if plugin_clone.is_current_run(run_id) {
                                        rt_handle.block_on(plugin_clone.set_process_alive(false));
                                    }
                                    break;
                                }
                                let err_msg = format!("[错误] 读取输出失败: {}", e);
                                let plugin_inner = plugin_clone.clone();
                                let sender = output_sender.clone();
                                let id = plugin_id_clone.clone();
                                let err_msg_for_async = err_msg.clone();
                                let _handle = rt_handle.spawn(async move {
                                    plugin_inner.add_output(err_msg_for_async).await;
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

                    if run_tmp_dir.exists() {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        let _ = std::fs::remove_dir_all(&run_tmp_dir);
                    }

                    if plugin_clone.is_current_run(run_id) {
                        rt_handle.block_on(plugin_clone.set_process_alive(false));
                    }

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
                        let msg_for_async = msg.clone();
                        let _handle = rt_handle.spawn(async move {
                            plugin_inner.add_output(msg_for_async).await;
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
                Err(e) => {
                    if run_tmp_dir.exists() {
                        let _ = std::fs::remove_dir_all(&run_tmp_dir);
                    }

                    if plugin_clone.is_current_run(run_id) {
                        rt_handle.block_on(plugin_clone.set_process_alive(false));
                    }

                    let err_msg = format!("[错误] 启动插件失败: {}", e);
                    let plugin_inner = plugin_clone.clone();
                    let sender = output_sender.clone();
                    let id = plugin_id_clone.clone();
                    if plugin_clone.is_current_run(run_id) {
                        let err_msg_for_async = err_msg.clone();
                        let _handle = rt_handle.spawn(async move {
                            plugin_inner.add_output(err_msg_for_async).await;
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

        if is_user_action && plugin.is_current_run(stop_run_id) {
            plugin.set_enabled(false).await;
            plugin.set_api_token(None).await;
            plugin.clear_webui().await;
            self.remove_enabled_plugin(plugin_id).await;
        }

        Ok(())
    }
}
