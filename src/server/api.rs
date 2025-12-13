use rocket::{get, post, serde::json::Json, State, response::stream::{EventStream, Event}};
use serde::{Serialize, Deserialize};
use crate::logger;
use crate::plus::PluginManager;
use std::sync::Arc;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub retcode: i32,
    pub data: T,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub logs: Vec<logger::LogEntry>,
}

#[derive(Serialize)]
pub struct SystemInfo {
    pub port: u16,
    pub data_dir: String,
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct UiState {
    #[serde(default)]
    pub last_page: String,
}

#[get("/ui/state")]
pub fn get_ui_state() -> Json<ApiResponse<UiState>> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    let config_file = exe_dir.join("config").join("ui.json");
    
    if let Ok(content) = std::fs::read_to_string(&config_file) {
        if let Ok(state) = serde_json::from_str::<UiState>(&content) {
            return Json(ApiResponse {
                retcode: 0,
                data: state,
            });
        }
    }
    
    Json(ApiResponse {
        retcode: 0,
        data: UiState { last_page: "logs".to_string() },
    })
}

#[post("/ui/state", format = "json", data = "<state>")]
pub fn save_ui_state(state: Json<UiState>) -> Json<ApiResponse<String>> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    let config_dir = exe_dir.join("config");
    let config_file = config_dir.join("ui.json");
    
    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        return Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to create config directory: {}", e),
        });
    }
    
    if let Ok(json_str) = serde_json::to_string_pretty(&state.into_inner()) {
        if let Err(e) = std::fs::write(&config_file, json_str) {
            return Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to write ui config: {}", e),
            });
        }
    }
    
    Json(ApiResponse {
        retcode: 0,
        data: "UI state saved".to_string(),
    })
}

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

#[get("/get_app_nums")]
pub fn get_app_nums() -> Json<ApiResponse<i32>> {
    Json(ApiResponse { retcode: 0, data: 8 })
}

#[get("/logs")]
pub fn get_logs() -> Json<ApiResponse<LogsResponse>> {
    let logs = logger::get_logs();
    Json(ApiResponse {
        retcode: 0,
        data: LogsResponse { logs },
    })
}

#[post("/logs/clear")]
pub fn clear_logs() -> Json<ApiResponse<String>> {
    logger::clear_logs();
    Json(ApiResponse {
        retcode: 0,
        data: "Logs cleared".to_string(),
    })
}

#[get("/logs/stream")]
pub fn logs_stream() -> EventStream![Event + 'static] {
    EventStream! {
        let mut rx = logger::subscribe_logs();
        
        while let Ok(log_entry) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&log_entry) {
                yield Event::data(json);
            }
        }
    }
}

#[get("/system_info")]
pub fn get_system_info(system_info: &State<Arc<SystemInfo>>) -> Json<ApiResponse<SystemInfo>> {
    let info = SystemInfo {
        port: system_info.port,
        data_dir: system_info.data_dir.clone(),
    };
    Json(ApiResponse {
        retcode: 0,
        data: info,
    })
}

#[post("/open_data_dir")]
pub fn open_data_dir(system_info: &State<Arc<SystemInfo>>) -> Json<ApiResponse<String>> {
    let path = &system_info.data_dir;
    
    // 创建目录（如果不存在）
    let _ = std::fs::create_dir_all(path);
    
    // 打开目录
    let _ = std::process::Command::new("explorer")
        .arg(path)
        .spawn();

    Json(ApiResponse {
        retcode: 0,
        data: "Opening directory".to_string(),
    })
}

#[get("/app_info")]
pub fn get_app_info() -> Json<ApiResponse<AppInfo>> {
    Json(ApiResponse {
        retcode: 0,
        data: AppInfo {
            version: env!("APP_VERSION").to_string(),
        },
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

#[get("/bot/get_config")]
pub fn get_bot_config() -> Json<ApiResponse<BotConfig>> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    let config_file = exe_dir.join("config").join("config.json");
    
    match std::fs::read_to_string(&config_file) {
        Ok(content) => {
            // 尝试解析新格式
            if let Ok(config) = serde_json::from_str::<BotConfig>(&content) {
                return Json(ApiResponse {
                    retcode: 0,
                    data: config,
                });
            }
            
            // 尝试解析旧格式并转换
            match serde_json::from_str::<LegacyBotConfig>(&content) {
                Ok(legacy_config) => {
                    let config = if let (Some(host), Some(api_port), Some(event_port)) = 
                        (legacy_config.host, legacy_config.api_port, legacy_config.event_port) {
                        // 新格式
                        BotConfig {
                            host,
                            api_port,
                            event_port,
                            token: legacy_config.token,
                            auto_connect: legacy_config.auto_connect,
                        }
                    } else if let (Some(api), Some(event_sse)) = (legacy_config.api, legacy_config.event_sse) {
                        // 旧格式，尝试解析URL
                        let (host, api_port) = parse_url(&api).unwrap_or(("localhost".to_string(), 3010));
                        let (_, event_port) = parse_url(&event_sse).unwrap_or(("localhost".to_string(), 3011));
                        
                        BotConfig {
                            host,
                            api_port,
                            event_port,
                            token: legacy_config.token,
                            auto_connect: legacy_config.auto_connect,
                        }
                    } else {
                        // 空配置
                        BotConfig {
                            host: "localhost".to_string(),
                            api_port: 3010,
                            event_port: 3011,
                            token: legacy_config.token,
                            auto_connect: false,
                        }
                    };
                    
                    Json(ApiResponse {
                        retcode: 0,
                        data: config,
                    })
                }
                Err(e) => {
                    log_error!("Failed to parse config file: {}", e);
                    Json(ApiResponse {
                        retcode: 1,
                        data: BotConfig {
                            host: "localhost".to_string(),
                            api_port: 3010,
                            event_port: 3011,
                            token: None,
                            auto_connect: false,
                        },
                    })
                }
            }
        }
        Err(_) => {
            // 配置文件不存在，返回默认配置
            Json(ApiResponse {
                retcode: 0,
                data: BotConfig {
                    host: "localhost".to_string(),
                    api_port: 3010,
                    event_port: 3011,
                    token: None,
                    auto_connect: false,
                },
            })
        }
    }
}

fn parse_url(url: &str) -> Option<(String, u16)> {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str()?.to_string();
        let port = parsed.port().unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });
        Some((host, port))
    } else {
        None
    }
}



#[post("/bot/save_config", format = "json", data = "<config>")]
pub fn save_bot_config(config: Json<BotConfig>, _system_info: &State<Arc<SystemInfo>>, bot_state: &State<Arc<crate::server::BotConnectionState>>) -> Json<ApiResponse<String>> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    let config_dir = exe_dir.join("config");
    let config_file = config_dir.join("config.json");
    
    // 创建config目录
    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        log_error!("Failed to create config directory: {}", e);
        return Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to create config directory: {}", e),
        });
    }
    
    // 保存配置到JSON文件，不修改auto_connect状态
    let config_inner = config.into_inner();
    match serde_json::to_string_pretty(&config_inner) {
        Ok(json_str) => {
            match std::fs::write(&config_file, json_str) {
                Ok(_) => {
                    
                    // 尝试连接SSE
                    let token = config_inner.token.clone();
                    let sse_url = config_inner.get_event_url();
                    let bot_state_clone = bot_state.inner().clone();
                    
                    bot_state_clone.should_connect.store(true, std::sync::atomic::Ordering::SeqCst);
                    bot_state_clone.is_connecting.store(true, std::sync::atomic::Ordering::SeqCst);
                    
                    // 发送状态更新
                    let status = BotStatusResponse { 
                        connected: false, 
                        connecting: true 
                    };
                    let _ = bot_state_clone.status_sender.send(status);
                    
                    tokio::spawn(async move {
                        connect_bot_sse(&sse_url, token, bot_state_clone).await;
                    });
                    
                    Json(ApiResponse {
                        retcode: 0,
                        data: "Config saved successfully".to_string(),
                    })
                }
                Err(e) => {
                    log_error!("Failed to write config file: {}", e);
                    Json(ApiResponse {
                        retcode: 1,
                        data: format!("Failed to write config file: {}", e),
                    })
                }
            }
        }
        Err(e) => {
            log_error!("Failed to serialize config: {}", e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to serialize config: {}", e),
            })
        }
    }
}

#[post("/bot/disconnect")]
pub fn disconnect_bot(bot_state: &State<Arc<crate::server::BotConnectionState>>) -> Json<ApiResponse<String>> {
    bot_state.is_connected.store(false, std::sync::atomic::Ordering::SeqCst);
    bot_state.is_connecting.store(false, std::sync::atomic::Ordering::SeqCst);
    bot_state.should_connect.store(false, std::sync::atomic::Ordering::SeqCst);
    
    // 更新配置文件，设置auto_connect为false
    update_auto_connect_status(false);
    
    // 发送状态更新
    let status = BotStatusResponse { 
        connected: false, 
        connecting: false 
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
pub fn get_bot_status(bot_state: &State<Arc<crate::server::BotConnectionState>>) -> Json<ApiResponse<BotStatusResponse>> {
    let connected = bot_state.is_connected.load(std::sync::atomic::Ordering::SeqCst);
    let connecting = bot_state.is_connecting.load(std::sync::atomic::Ordering::SeqCst);
    Json(ApiResponse {
        retcode: 0,
        data: BotStatusResponse { connected, connecting },
    })
}

#[get("/bot/status_stream")]
pub fn bot_status_stream(bot_state: &State<Arc<crate::server::BotConnectionState>>) -> EventStream![Event + 'static] {
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
pub async fn get_login_info(_bot_state: &State<Arc<crate::server::BotConnectionState>>, _body: Json<serde_json::Value>) -> Json<ApiResponse<LoginInfo>> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    let config_file = exe_dir.join("config").join("config.json");
    
    // 读取配置获取bot连接信息
    if let Ok(content) = std::fs::read_to_string(&config_file) {
        if let Ok(config) = serde_json::from_str::<BotConfig>(&content) {
            let api_url = format!("{}/get_login_info", config.get_api_url());
            
            let client = reqwest::Client::new();
            let mut request_builder = client.post(&api_url)
                .header("Content-Type", "application/json");
            
            // 如果有token，添加Authorization header
            if let Some(ref token_str) = config.token {
                request_builder = request_builder.header("Authorization", format!("Bearer {}", token_str));
            }
            
            let body = serde_json::to_string(&serde_json::json!({})).unwrap_or_default();
            match request_builder.body(body).send().await {
                Ok(response) => {
                    if let Ok(text) = response.text().await {
                        // 解析bot返回的响应
                        if let Ok(bot_response) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let (Some(uin), Some(nickname)) = (
                                bot_response.get("data").and_then(|d| d.get("uin")).and_then(|u| u.as_i64()),
                                bot_response.get("data").and_then(|d| d.get("nickname")).and_then(|n| n.as_str())
                            ) {
                                return Json(ApiResponse {
                                    retcode: 0,
                                    data: LoginInfo {
                                        uin,
                                        nickname: nickname.to_string(),
                                    },
                                });
                            }
                        }
                    }
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

pub fn check_and_auto_connect(bot_state: Arc<crate::server::BotConnectionState>) {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    let config_file = exe_dir.join("config").join("config.json");
    
    // 读取配置文件
    if let Ok(content) = std::fs::read_to_string(&config_file) {
        if let Ok(config) = serde_json::from_str::<BotConfig>(&content) {
            if config.auto_connect && !config.host.is_empty() {
                
                // 设置连接状态
                bot_state.should_connect.store(true, std::sync::atomic::Ordering::SeqCst);
                bot_state.is_connecting.store(true, std::sync::atomic::Ordering::SeqCst);
                
                // 发送状态更新
                let status = BotStatusResponse { 
                    connected: false, 
                    connecting: true 
                };
                let _ = bot_state.status_sender.send(status);
                
                // 启动连接
                let token = config.token.clone();
                let sse_url = config.get_event_url();
                let bot_state_clone = bot_state.clone();
                
                tokio::spawn(async move {
                    connect_bot_sse(&sse_url, token, bot_state_clone).await;
                });
            }
        }
    }
}

fn update_auto_connect_status(auto_connect: bool) {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    let config_file = exe_dir.join("config").join("config.json");
    
    // 读取现有配置
    if let Ok(content) = std::fs::read_to_string(&config_file) {
        if let Ok(mut config) = serde_json::from_str::<BotConfig>(&content) {
            config.auto_connect = auto_connect;
            
            // 保存更新后的配置
            if let Ok(json_str) = serde_json::to_string_pretty(&config) {
                let _ = std::fs::write(&config_file, json_str);
            }
        }
    }
}

async fn connect_bot_sse(sse_url: &str, token: Option<String>, bot_state: Arc<crate::server::BotConnectionState>) {
    loop {
        // 检查是否应该继续连接
        if !bot_state.should_connect.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        
        // 构建HTTP客户端
        let client = reqwest::Client::new();
        let mut request_builder = client.get(sse_url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");
        
        // 如果有token，添加Authorization header
        if let Some(ref token_str) = token {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", token_str));
        }
        
        match request_builder.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    log_info!("连接成功");
                    bot_state.is_connected.store(true, std::sync::atomic::Ordering::SeqCst);
                    bot_state.is_connecting.store(false, std::sync::atomic::Ordering::SeqCst);
                    
                    // 连接成功后，设置auto_connect为true
                    update_auto_connect_status(true);
                    
                    // 发送连接成功状态
                    let status = BotStatusResponse { 
                        connected: true, 
                        connecting: false 
                    };
                    let _ = bot_state.status_sender.send(status);
                    
                    // 处理SSE连接
                    let _ = handle_bot_sse_stream(response, bot_state.clone()).await;
                    
                    bot_state.is_connected.store(false, std::sync::atomic::Ordering::SeqCst);
                } else {
                    log_error!("连接断开，重连中... (HTTP {})", response.status());
                }
                
                // 检查是否应该继续重连
                if !bot_state.should_connect.load(std::sync::atomic::Ordering::SeqCst) {
                    bot_state.is_connecting.store(false, std::sync::atomic::Ordering::SeqCst);
                    
                    // 用户主动断开连接，设置auto_connect为false
                    update_auto_connect_status(false);
                    
                    // 发送断开状态
                    let status = BotStatusResponse { 
                        connected: false, 
                        connecting: false 
                    };
                    let _ = bot_state.status_sender.send(status);
                    break;
                }
                
                bot_state.is_connecting.store(true, std::sync::atomic::Ordering::SeqCst);
                
                // 发送重连状态
                let status = BotStatusResponse { 
                    connected: false, 
                    connecting: true 
                };
                let _ = bot_state.status_sender.send(status);
                
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(e) => {
                log_error!("连接断开，重连中... (错误: {})", e);
                // 检查是否应该继续重连
                if !bot_state.should_connect.load(std::sync::atomic::Ordering::SeqCst) {
                    bot_state.is_connecting.store(false, std::sync::atomic::Ordering::SeqCst);
                    
                    // 用户主动断开连接，设置auto_connect为false
                    update_auto_connect_status(false);
                    
                    // 发送断开状态
                    let status = BotStatusResponse { 
                        connected: false, 
                        connecting: false 
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use futures_util::StreamExt;
    
    let mut stream = response.bytes_stream();
    
    loop {
        tokio::select! {
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            // 简单的SSE解析
                            for line in text.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
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

#[get("/plugins/list")]
pub async fn list_plugins(manager: &State<Arc<PluginManager>>) -> Json<ApiResponse<Vec<crate::plus::manager::PluginInfo>>> {
    // 重新加载插件列表
    let _ = manager.load_plugins().await;
    
    match manager.list_plugins().await {
        Ok(plugins) => {
            Json(ApiResponse {
                retcode: 0,
                data: plugins,
            })
        }
        Err(e) => {
            log_error!("Failed to list plugins: {}", e);
            Json(ApiResponse {
                retcode: 1,
                data: Vec::new(),
            })
        }
    }
}

#[post("/plugins/<plugin_id>/start")]
pub async fn start_plugin(plugin_id: String, manager: &State<Arc<PluginManager>>) -> Json<ApiResponse<String>> {
    let name = manager.get_plugin_name(&plugin_id).await.unwrap_or_else(|| plugin_id.clone());
    match manager.start_plugin(&plugin_id).await {
        Ok(_) => {
            log_info!("Plugin {}({}) started", name, plugin_id);
            Json(ApiResponse {
                retcode: 0,
                data: format!("Plugin {}({}) started", name, plugin_id),
            })
        }
        Err(e) => {
            log_error!("Failed to start plugin {}({}): {}", name, plugin_id, e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to start plugin: {}", e),
            })
        }
    }
}

#[post("/plugins/<plugin_id>/stop")]
pub async fn stop_plugin(plugin_id: String, manager: &State<Arc<PluginManager>>) -> Json<ApiResponse<String>> {
    // API 调用被视为用户主动停止
    let name = manager.get_plugin_name(&plugin_id).await.unwrap_or_else(|| plugin_id.clone());
    match manager.stop_plugin(&plugin_id, true).await {
        Ok(_) => {
            log_info!("Plugin {}({}) stopped", name, plugin_id);
            Json(ApiResponse {
                retcode: 0,
                data: format!("Plugin {}({}) stopped", name, plugin_id),
            })
        }
        Err(e) => {
            log_error!("Failed to stop plugin {}({}): {}", name, plugin_id, e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to stop plugin: {}", e),
            })
        }
    }
}

#[get("/plugins/<plugin_id>/output")]
pub async fn get_plugin_output(plugin_id: String, manager: &State<Arc<PluginManager>>) -> Json<ApiResponse<Vec<String>>> {
    match manager.get_plugin_output(&plugin_id).await {
        Ok(output) => {
            Json(ApiResponse {
                retcode: 0,
                data: output,
            })
        }
        Err(e) => {
            log_error!("Failed to get plugin output: {}", e);
            Json(ApiResponse {
                retcode: 1,
                data: Vec::new(),
            })
        }
    }
}

#[post("/plugins/<plugin_id>/output/clear")]
pub async fn clear_plugin_output(plugin_id: String, manager: &State<Arc<PluginManager>>) -> Json<ApiResponse<String>> {
    match manager.clear_plugin_output(&plugin_id).await {
        Ok(_) => {
            Json(ApiResponse {
                retcode: 0,
                data: "Output cleared".to_string(),
            })
        }
        Err(e) => {
            log_error!("Failed to clear plugin output: {}", e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to clear output: {}", e),
            })
        }
    }
}

#[get("/plugins/status_stream")]
pub fn plugins_status_stream(manager: &State<Arc<PluginManager>>) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    EventStream! {
        let mut rx = manager.subscribe_status();
        
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Event::data(json);
                    }
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}

#[get("/plugins/<plugin_id>/output/stream")]
pub fn plugin_output_stream(plugin_id: String, manager: &State<Arc<PluginManager>>) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    let target_plugin = plugin_id.clone();
    EventStream! {
        // 发送现有的输出
        if let Ok(output) = manager.get_plugin_output(&plugin_id).await {
            for line in &output {
                if let Ok(json) = serde_json::to_string(&line) {
                    yield Event::data(json);
                }
            }
        }
        
        // 订阅实时输出
        let mut rx = manager.subscribe_output();
        
        // 持续监听新的输出
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if event.plugin_id == target_plugin {
                        if let Ok(json) = serde_json::to_string(&event.line) {
                            yield Event::data(json);
                        }
                    }
                }
                Err(_) => {
                    // 通道关闭，等待一下再重试
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
}
