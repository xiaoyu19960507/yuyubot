use super::{ApiResponse, SystemInfo};
use crate::runtime;
use futures_util::StreamExt;
use rocket::{
    get, post,
    response::stream::{Event, EventStream},
    serde::json::Json,
    State,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
#[derive(Serialize, Deserialize, Clone)]
pub struct BotConfig {
    pub host: String,
    #[serde(rename = "apiPort")]
    pub api_port: u16,
    #[serde(rename = "eventPort")]
    pub event_port: u16,
    pub token: Option<String>,
    #[serde(default)]
    pub auto_connect: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LoginInfo {
    pub uin: i64,
    pub nickname: String,
}

impl BotConfig {
    pub fn get_api_url(&self) -> String {
        format!("http://{}:{}/api", self.host, self.api_port)
    }

    pub fn get_event_url(&self) -> String {
        format!("http://{}:{}/event", self.host, self.event_port)
    }
}

async fn fetch_bot_login_info_from_config(config: &BotConfig) -> Result<LoginInfo, String> {
    let api_url = format!("{}/get_login_info", config.get_api_url());

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build reqwest client: {}", e))?;

    let mut request_builder = client
        .post(&api_url)
        .header("Content-Type", "application/json");

    if let Some(token_str) = config.token.as_deref() {
        request_builder = request_builder.header("Authorization", format!("Bearer {}", token_str));
    }

    let response = request_builder
        .body("{}".to_string())
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read API response: {}", e))?;

    if !status.is_success() {
        return Err(format!("API returned HTTP {}", status));
    }

    let bot_response = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    let uin = bot_response
        .get("data")
        .and_then(|d| d.get("uin"))
        .and_then(|u| u.as_i64())
        .ok_or_else(|| "API response missing data.uin".to_string())?;

    let nickname = bot_response
        .get("data")
        .and_then(|d| d.get("nickname"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| "API response missing data.nickname".to_string())?;

    Ok(LoginInfo {
        uin,
        nickname: nickname.to_string(),
    })
}

#[derive(Deserialize)]
struct LegacyBotConfig {
    pub api: Option<String>,
    #[serde(rename = "eventSse")]
    pub event_sse: Option<String>,
    pub host: Option<String>,
    #[serde(rename = "apiPort")]
    pub api_port: Option<u16>,
    #[serde(rename = "eventPort")]
    pub event_port: Option<u16>,
    pub token: Option<String>,
    #[serde(default)]
    pub auto_connect: bool,
}

pub fn load_bot_config_from_disk(exe_dir: &std::path::Path) -> BotConfig {
    let config_file = exe_dir.join("config").join("config.json");

    let Ok(content) = std::fs::read_to_string(&config_file) else {
        return BotConfig {
            host: "localhost".to_string(),
            api_port: 3010,
            event_port: 3011,
            token: None,
            auto_connect: false,
        };
    };

    if let Ok(config) = serde_json::from_str::<BotConfig>(&content) {
        return config;
    }

    if let Ok(legacy_config) = serde_json::from_str::<LegacyBotConfig>(&content) {
        if let (Some(host), Some(api_port), Some(event_port)) = (
            legacy_config.host,
            legacy_config.api_port,
            legacy_config.event_port,
        ) {
            return BotConfig {
                host,
                api_port,
                event_port,
                token: legacy_config.token,
                auto_connect: legacy_config.auto_connect,
            };
        }

        if let (Some(api), Some(event_sse)) = (legacy_config.api, legacy_config.event_sse) {
            let (host, api_port) = parse_url(&api).unwrap_or(("localhost".to_string(), 3010));
            let (_, event_port) = parse_url(&event_sse).unwrap_or(("localhost".to_string(), 3011));

            return BotConfig {
                host,
                api_port,
                event_port,
                token: legacy_config.token,
                auto_connect: legacy_config.auto_connect,
            };
        }

        return BotConfig {
            host: "localhost".to_string(),
            api_port: 3010,
            event_port: 3011,
            token: legacy_config.token,
            auto_connect: false,
        };
    }

    BotConfig {
        host: "localhost".to_string(),
        api_port: 3010,
        event_port: 3011,
        token: None,
        auto_connect: false,
    }
}

#[get("/bot/get_config")]
pub async fn get_bot_config() -> Json<ApiResponse<BotConfig>> {
    let exe_dir = runtime::get_exe_dir();

    let config_file = exe_dir.join("config").join("config.json");

    let (retcode, config) = match tokio::fs::read_to_string(&config_file).await {
        Ok(content) => {
            // 尝试解析新格式
            if let Ok(config) = serde_json::from_str::<BotConfig>(&content) {
                (0, config)
            } else {
                // 尝试解析旧格式并转换
                match serde_json::from_str::<LegacyBotConfig>(&content) {
                    Ok(legacy_config) => {
                        let config = if let (Some(host), Some(api_port), Some(event_port)) = (
                            legacy_config.host,
                            legacy_config.api_port,
                            legacy_config.event_port,
                        ) {
                            BotConfig {
                                host,
                                api_port,
                                event_port,
                                token: legacy_config.token,
                                auto_connect: legacy_config.auto_connect,
                            }
                        } else if let (Some(api), Some(event_sse)) =
                            (legacy_config.api, legacy_config.event_sse)
                        {
                            let (host, api_port) =
                                parse_url(&api).unwrap_or(("localhost".to_string(), 3010));
                            let (_, event_port) =
                                parse_url(&event_sse).unwrap_or(("localhost".to_string(), 3011));

                            BotConfig {
                                host,
                                api_port,
                                event_port,
                                token: legacy_config.token,
                                auto_connect: legacy_config.auto_connect,
                            }
                        } else {
                            BotConfig {
                                host: "localhost".to_string(),
                                api_port: 3010,
                                event_port: 3011,
                                token: legacy_config.token,
                                auto_connect: false,
                            }
                        };

                        (0, config)
                    }
                    Err(_) => (
                        1,
                        BotConfig {
                            host: "localhost".to_string(),
                            api_port: 3010,
                            event_port: 3011,
                            token: None,
                            auto_connect: false,
                        },
                    ),
                }
            }
        }
        Err(_) => (
            0,
            BotConfig {
                host: "localhost".to_string(),
                api_port: 3010,
                event_port: 3011,
                token: None,
                auto_connect: false,
            },
        ),
    };

    Json(ApiResponse {
        retcode,
        data: config,
    })
}

fn parse_url(url: &str) -> Option<(String, u16)> {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str()?.to_string();
        let port = parsed
            .port()
            .unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
        Some((host, port))
    } else {
        None
    }
}

#[post("/bot/save_config", format = "json", data = "<config>")]
pub async fn save_bot_config(
    config: Json<BotConfig>,
    _system_info: &State<Arc<SystemInfo>>,
    bot_state: &State<Arc<crate::server::BotConnectionState>>,
    bot_config_state: &State<Arc<RwLock<BotConfig>>>,
) -> Json<ApiResponse<String>> {
    // 保存配置到JSON文件，不修改auto_connect状态
    let config_inner = config.into_inner();
    let json_str = match serde_json::to_string_pretty(&config_inner) {
        Ok(s) => s,
        Err(e) => {
            log_error!("Failed to serialize config: {}", e);
            return Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to serialize config: {}", e),
            });
        }
    };

    let exe_dir = runtime::get_exe_dir();

    let config_dir = exe_dir.join("config");
    let config_file = config_dir.join("config.json");

    // 获取配置写入锁，防止并发写入
    {
        let _guard = bot_state.config_write_lock.lock().await;

        // 创建config目录
        if let Err(e) = tokio::fs::create_dir_all(&config_dir).await {
            log_error!("Failed to create config directory: {}", e);
            return Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to create config directory: {}", e),
            });
        }

        if let Err(e) = tokio::fs::write(&config_file, &json_str).await {
            log_error!("Failed to write config file: {}", e);
            return Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to write config file: {}", e),
            });
        }
    }

    *bot_config_state.write().await = config_inner.clone();

    // 尝试连接SSE
    let bot_state_clone = bot_state.inner().clone();

    if let Some(cancel) = bot_state_clone.cancel_sender.lock().await.take() {
        let _ = cancel.send(());
    }
    if let Some(handle) = bot_state_clone.connection_task.lock().await.take() {
        handle.abort();
    }

    bot_state_clone
        .should_connect
        .store(true, std::sync::atomic::Ordering::SeqCst);
    bot_state_clone
        .is_connecting
        .store(true, std::sync::atomic::Ordering::SeqCst);

    // 发送状态更新
    let status = BotStatusResponse {
        connected: false,
        connecting: true,
    };
    let _ = bot_state_clone.status_sender.send(status);

    let bot_state_for_task = bot_state_clone.clone();
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    *bot_state_clone.cancel_sender.lock().await = Some(cancel_tx);

    let config_for_task = config_inner.clone();
    let handle = tokio::spawn(async move {
        connect_bot_sse(config_for_task, bot_state_for_task, cancel_rx).await;
    });
    *bot_state_clone.connection_task.lock().await = Some(handle);

    Json(ApiResponse {
        retcode: 0,
        data: "Config saved successfully".to_string(),
    })
}

#[post("/bot/disconnect")]
pub async fn disconnect_bot(
    bot_state: &State<Arc<crate::server::BotConnectionState>>,
    bot_config_state: &State<Arc<RwLock<BotConfig>>>,
) -> Json<ApiResponse<String>> {
    bot_state
        .is_connected
        .store(false, std::sync::atomic::Ordering::SeqCst);
    bot_state
        .is_connecting
        .store(false, std::sync::atomic::Ordering::SeqCst);
    bot_state
        .should_connect
        .store(false, std::sync::atomic::Ordering::SeqCst);

    if let Some(cancel) = bot_state.cancel_sender.lock().await.take() {
        let _ = cancel.send(());
    }

    // 等待任务自然结束，不使用 abort
    if let Some(handle) = bot_state.connection_task.lock().await.take() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
    }

    // 更新配置文件，设置auto_connect为false
    update_auto_connect_status(bot_state, false).await;

    {
        let mut config = bot_config_state.write().await;
        config.auto_connect = false;
    }

    // 发送状态更新
    let status = BotStatusResponse {
        connected: false,
        connecting: false,
    };
    let _ = bot_state.status_sender.send(status);

    log_info!("连接断开");
    Json(ApiResponse {
        retcode: 0,
        data: "Disconnected".to_string(),
    })
}

#[derive(Serialize, Clone)]
pub struct BotStatusResponse {
    pub connected: bool,
    pub connecting: bool,
}

#[get("/bot/get_status")]
pub fn get_bot_status(
    bot_state: &State<Arc<crate::server::BotConnectionState>>,
) -> Json<ApiResponse<BotStatusResponse>> {
    let connected = bot_state
        .is_connected
        .load(std::sync::atomic::Ordering::SeqCst);
    let connecting = bot_state
        .is_connecting
        .load(std::sync::atomic::Ordering::SeqCst);
    Json(ApiResponse {
        retcode: 0,
        data: BotStatusResponse {
            connected,
            connecting,
        },
    })
}

#[get("/bot/status_stream")]
pub fn bot_status_stream(
    bot_state: &State<Arc<crate::server::BotConnectionState>>,
) -> EventStream![Event + 'static] {
    let bot_state = bot_state.inner().clone();
    EventStream! {
        let mut rx = bot_state.status_sender.subscribe();

        // 发送当前状态
        let connected = bot_state.is_connected.load(std::sync::atomic::Ordering::SeqCst);
        let connecting = bot_state.is_connecting.load(std::sync::atomic::Ordering::SeqCst);
        let status = BotStatusResponse { connected, connecting };
        if let Ok(json) = serde_json::to_string(&status) {
            yield Event::data(json);
        }

        // 监听状态变化
        while let Ok(status) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&status) {
                yield Event::data(json);
            }
        }
    }
}

#[post("/get_login_info", format = "json", data = "<_body>")]
pub async fn get_login_info(
    _bot_state: &State<Arc<crate::server::BotConnectionState>>,
    _body: Json<serde_json::Value>,
) -> Json<ApiResponse<LoginInfo>> {
    let exe_dir = runtime::get_exe_dir();

    let config_file = exe_dir.join("config").join("config.json");

    // 读取配置获取bot连接信息
    if let Ok(content) = std::fs::read_to_string(&config_file) {
        if let Ok(config) = serde_json::from_str::<BotConfig>(&content) {
            match fetch_bot_login_info_from_config(&config).await {
                Ok(login_info) => {
                    return Json(ApiResponse {
                        retcode: 0,
                        data: login_info,
                    });
                }
                Err(e) => {
                    log_error!("Failed to get login info: {}", e);
                }
            }
        }
    }

    Json(ApiResponse {
        retcode: 1,
        data: LoginInfo {
            uin: 0,
            nickname: "未连接".to_string(),
        },
    })
}

pub async fn check_and_auto_connect(bot_state: Arc<crate::server::BotConnectionState>) {
    let config_result = tokio::task::spawn_blocking(|| {
        let exe_dir = runtime::get_exe_dir();

        let config_file = exe_dir.join("config").join("config.json");

        // 读取配置文件
        if let Ok(content) = std::fs::read_to_string(&config_file) {
            if let Ok(config) = serde_json::from_str::<BotConfig>(&content) {
                if config.auto_connect && !config.host.is_empty() {
                    return Some(config);
                }
            }
        }
        None
    })
    .await;

    if let Ok(Some(config)) = config_result {
        // 设置连接状态
        bot_state
            .should_connect
            .store(true, std::sync::atomic::Ordering::SeqCst);
        bot_state
            .is_connecting
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // 发送状态更新
        let status = BotStatusResponse {
            connected: false,
            connecting: true,
        };
        let _ = bot_state.status_sender.send(status);

        if let Some(cancel) = bot_state.cancel_sender.lock().await.take() {
            let _ = cancel.send(());
        }
        if let Some(handle) = bot_state.connection_task.lock().await.take() {
            handle.abort();
        }

        let bot_state_for_task = bot_state.clone();
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        *bot_state.cancel_sender.lock().await = Some(cancel_tx);

        let config_for_task = config.clone();
        let handle = tokio::spawn(async move {
            connect_bot_sse(config_for_task, bot_state_for_task, cancel_rx).await;
        });
        *bot_state.connection_task.lock().await = Some(handle);
    }
}

async fn update_auto_connect_status(
    bot_state: &Arc<crate::server::BotConnectionState>,
    auto_connect: bool,
) {
    // 获取配置写入锁，防止并发写入
    let _guard = bot_state.config_write_lock.lock().await;

    let exe_dir = runtime::get_exe_dir();

    let config_file = exe_dir.join("config").join("config.json");

    // 读取现有配置
    if let Ok(content) = tokio::fs::read_to_string(&config_file).await {
        if let Ok(mut config) = serde_json::from_str::<BotConfig>(&content) {
            config.auto_connect = auto_connect;

            // 保存更新后的配置
            if let Ok(json_str) = serde_json::to_string_pretty(&config) {
                let _ = tokio::fs::write(&config_file, json_str).await;
            }
        }
    }
}

async fn connect_bot_sse(
    config: BotConfig,
    bot_state: Arc<crate::server::BotConnectionState>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    loop {
        // 检查是否应该继续连接
        if !bot_state
            .should_connect
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            break;
        }

        let sse_url = config.get_event_url();

        // 构建HTTP客户端
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("Failed to build reqwest client");
        let mut request_builder = client
            .get(&sse_url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");

        // 如果有token，添加Authorization header
        if let Some(token_str) = config.token.as_deref() {
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", token_str));
        }

        match request_builder.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match fetch_bot_login_info_from_config(&config).await {
                        Ok(_) => {
                            log_info!("连接成功");
                            bot_state
                                .is_connected
                                .store(true, std::sync::atomic::Ordering::SeqCst);
                            bot_state
                                .is_connecting
                                .store(false, std::sync::atomic::Ordering::SeqCst);

                            // 连接成功后，设置auto_connect为true
                            update_auto_connect_status(&bot_state, true).await;

                            // 发送连接成功状态
                            let status = BotStatusResponse {
                                connected: true,
                                connecting: false,
                            };
                            let _ = bot_state.status_sender.send(status);

                            let _ =
                                handle_bot_sse_stream(response, bot_state.clone(), &mut cancel_rx)
                                    .await;

                            bot_state
                                .is_connected
                                .store(false, std::sync::atomic::Ordering::SeqCst);
                        }
                        Err(e) => {
                            bot_state
                                .is_connected
                                .store(false, std::sync::atomic::Ordering::SeqCst);
                            log_error!(
                                "Event SSE connected but API validation failed, reconnecting... ({})",
                                e
                            );
                        }
                    }
                } else {
                    log_error!("连接断开，重连中... (HTTP {})", response.status());
                }

                // 检查是否应该继续重连
                if !bot_state
                    .should_connect
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    bot_state
                        .is_connecting
                        .store(false, std::sync::atomic::Ordering::SeqCst);

                    // 用户主动断开连接，设置auto_connect为false
                    update_auto_connect_status(&bot_state, false).await;

                    // 发送断开状态
                    let status = BotStatusResponse {
                        connected: false,
                        connecting: false,
                    };
                    let _ = bot_state.status_sender.send(status);
                    break;
                }

                bot_state
                    .is_connecting
                    .store(true, std::sync::atomic::Ordering::SeqCst);

                // 发送重连状态
                let status = BotStatusResponse {
                    connected: false,
                    connecting: true,
                };
                let _ = bot_state.status_sender.send(status);

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(e) => {
                log_error!("连接断开，重连中... (错误: {})", e);
                // 检查是否应该继续重连
                if !bot_state
                    .should_connect
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    bot_state
                        .is_connecting
                        .store(false, std::sync::atomic::Ordering::SeqCst);

                    // 用户主动断开连接，设置auto_connect为false
                    update_auto_connect_status(&bot_state, false).await;

                    // 发送断开状态
                    let status = BotStatusResponse {
                        connected: false,
                        connecting: false,
                    };
                    let _ = bot_state.status_sender.send(status);
                    break;
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn handle_bot_sse_stream(
    response: reqwest::Response,
    bot_state: Arc<crate::server::BotConnectionState>,
    cancel_rx: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = response.bytes_stream();

    loop {
        tokio::select! {
            _ = &mut *cancel_rx => {
                break;
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            // 简单的SSE解析
                            for line in text.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                        if let Some(event_type) = json.get("event_type").and_then(|v| v.as_str()) {
                                            if event_type == "message_receive" {
                                                if let Some(msg_data) = json.get("data") {
                                                    let scene = msg_data.get("message_scene").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                    let peer_id = msg_data.get("peer_id").and_then(|v| v.as_i64()).unwrap_or(0);
                                                    let sender_id = msg_data.get("sender_id").and_then(|v| v.as_i64()).unwrap_or(0);
                                                    let nickname = msg_data.get("group_member")
                                                        .and_then(|m| m.get("nickname"))
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("未知");
                                                    let group_name = msg_data.get("group")
                                                        .and_then(|g| g.get("group_name"))
                                                        .and_then(|v| v.as_str());

                                                    let mut content = String::new();
                                                    if let Some(segments) = msg_data.get("segments").and_then(|v| v.as_array()) {
                                                        for seg in segments {
                                                            if let Some(seg_type) = seg.get("type").and_then(|v| v.as_str()) {
                                                                match seg_type {
                                                                    "text" => {
                                                                        if let Some(text) = seg.get("data").and_then(|d| d.get("text")).and_then(|v| v.as_str()) {
                                                                            content.push_str(text);
                                                                        }
                                                                    }
                                                                    "face" => content.push_str("[表情]"),
                                                                    "image" => content.push_str("[图片]"),
                                                                    "at" => content.push_str("[at]"),
                                                                    _ => content.push_str(&format!("[{}]", seg_type)),
                                                                }
                                                            }
                                                        }
                                                    }

                                                    if let Some(name) = group_name {
                                                        log_info!("[{}:{}] {}({}): {}", name, peer_id, nickname, sender_id, content);
                                                    } else {
                                                        log_info!("[{}:{}] {}({}): {}", scene, peer_id, nickname, sender_id, content);
                                                    }
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                    log_info!("收到消息: {}", data);
                                }
                            }
                        }
                    }
                    Some(Err(_)) | None => {
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // 定期检查是否应该断开
                if !bot_state.should_connect.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    Ok(())
}

// 插件管理API
