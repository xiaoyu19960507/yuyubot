use crate::logger;
use crate::plus::PluginManager;
use crate::runtime;
use crate::server::MainProxy;
use crate::window::UserEvent;
use rfd;
use rocket::{
    get,
    http::Status,
    post,
    request::{FromRequest, Outcome},
    response::stream::{Event, EventStream},
    serde::json::Json,
    Request, State,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

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
    pub port: AtomicU16,
    pub data_dir: String,
    pub plugins_root: String,
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

pub struct PluginCaller {
    pub plugin_id: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PluginCaller {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let auth = match req.headers().get_one("Authorization") {
            Some(v) => v,
            None => return Outcome::Error((Status::Unauthorized, ())),
        };
        let token = auth.strip_prefix("Bearer ").unwrap_or(auth).trim();
        if token.is_empty() {
            return Outcome::Error((Status::Unauthorized, ()));
        }

        let manager = match req.rocket().state::<Arc<PluginManager>>() {
            Some(m) => m,
            None => return Outcome::Error((Status::InternalServerError, ())),
        };

        match manager.get_plugin_id_by_api_token(token).await {
            Some(plugin_id) => Outcome::Success(PluginCaller { plugin_id }),
            None => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

#[derive(Deserialize)]
pub struct SetWebuiRequest {
    pub webui: String,
}

#[post("/set_webui", format = "json", data = "<req_body>")]
pub async fn set_webui(
    caller: PluginCaller,
    req_body: Json<SetWebuiRequest>,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager
        .set_plugin_webui(&caller.plugin_id, req_body.webui.clone())
        .await
    {
        Ok(_) => Json(ApiResponse {
            retcode: 0,
            data: "ok".to_string(),
        }),
        Err(e) => Json(ApiResponse {
            retcode: -1,
            data: e.to_string(),
        }),
    }
}

#[get("/ui/state")]
pub async fn get_ui_state() -> Json<ApiResponse<UiState>> {
    let exe_dir = runtime::get_exe_dir();

    let config_file = exe_dir.join("config").join("ui.json");

    let state = if let Ok(content) = tokio::fs::read_to_string(&config_file).await {
        serde_json::from_str::<UiState>(&content).unwrap_or(UiState {
            last_page: "logs".to_string(),
        })
    } else {
        UiState {
            last_page: "logs".to_string(),
        }
    };

    Json(ApiResponse {
        retcode: 0,
        data: state,
    })
}

#[post("/ui/state", format = "json", data = "<state>")]
pub async fn save_ui_state(state: Json<UiState>) -> Json<ApiResponse<String>> {
    let state_inner = state.into_inner();
    let exe_dir = runtime::get_exe_dir();

    let config_dir = exe_dir.join("config");
    let config_file = config_dir.join("ui.json");

    if let Err(e) = tokio::fs::create_dir_all(&config_dir).await {
        return Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to create config directory: {}", e),
        });
    }

    match serde_json::to_string_pretty(&state_inner) {
        Ok(json_str) => {
            if let Err(e) = tokio::fs::write(&config_file, json_str).await {
                return Json(ApiResponse {
                    retcode: 1,
                    data: format!("Failed to write ui config: {}", e),
                });
            }
            Json(ApiResponse {
                retcode: 0,
                data: "UI state saved".to_string(),
            })
        }
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to serialize ui state: {}", e),
        }),
    }
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
    Json(ApiResponse {
        retcode: 0,
        data: 8,
    })
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
pub fn get_system_info(
    system_info: &State<Arc<SystemInfo>>,
) -> Json<ApiResponse<SystemInfoResponse>> {
    let info = SystemInfoResponse {
        port: system_info.port.load(Ordering::SeqCst),
        data_dir: system_info.data_dir.clone(),
        plugins_root: system_info.plugins_root.clone(),
    };
    Json(ApiResponse {
        retcode: 0,
        data: info,
    })
}

#[derive(Serialize)]
pub struct SystemInfoResponse {
    pub port: u16,
    pub data_dir: String,
    #[serde(rename = "plugins_root")]
    pub plugins_root: String,
}

#[post("/open_data_dir")]
pub async fn open_data_dir(system_info: &State<Arc<SystemInfo>>) -> Json<ApiResponse<String>> {
    let path = system_info.data_dir.clone();

    
    let _ = tokio::fs::create_dir_all(&path).await;

    runtime::open_in_explorer(&path);

    Json(ApiResponse {
        retcode: 0,
        data: "Opening directory".to_string(),
    })
}

#[post("/open_plugins_dir")]
pub async fn open_plugins_dir(
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.open_plugins_root().await {
        Ok(_) => Json(ApiResponse {
            retcode: 0,
            data: "Opening directory".to_string(),
        }),
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: e,
        }),
    }
}

#[post("/restart_program")]
pub async fn restart_program(main_proxy: &State<Arc<MainProxy>>) -> Json<ApiResponse<String>> {
    let proxy_lock = main_proxy.proxy.read().await;
    if let Some(proxy) = &*proxy_lock {
        let _ = proxy.send_event(UserEvent::RestartRequested);
    }

    Json(ApiResponse {
        retcode: 0,
        data: "Restarting...".to_string(),
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

    // 创建config目录
    if let Err(e) = tokio::fs::create_dir_all(&config_dir).await {
        log_error!("Failed to create config directory: {}", e);
        return Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to create config directory: {}", e),
        });
    }

    if let Err(e) = tokio::fs::write(&config_file, json_str).await {
        log_error!("Failed to write config file: {}", e);
        return Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to write config file: {}", e),
        });
    }

    *bot_config_state.write().await = config_inner.clone();

    // 尝试连接SSE
    let token = config_inner.token.clone();
    let sse_url = config_inner.get_event_url();
    let bot_state_clone = bot_state.inner().clone();

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

    tokio::spawn(async move {
        connect_bot_sse(&sse_url, token, bot_state_clone).await;
    });

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

    // 更新配置文件，设置auto_connect为false
    update_auto_connect_status(false).await;

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
            let api_url = format!("{}/get_login_info", config.get_api_url());

            let client = reqwest::Client::new();
            let mut request_builder = client
                .post(&api_url)
                .header("Content-Type", "application/json");

            // 如果有token，添加Authorization header
            if let Some(ref token_str) = config.token {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", token_str));
            }

            let body = serde_json::to_string(&serde_json::json!({})).unwrap_or_default();
            match request_builder.body(body).send().await {
                Ok(response) => {
                    if let Ok(text) = response.text().await {
                        // 解析bot返回的响应
                        if let Ok(bot_response) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let (Some(uin), Some(nickname)) = (
                                bot_response
                                    .get("data")
                                    .and_then(|d| d.get("uin"))
                                    .and_then(|u| u.as_i64()),
                                bot_response
                                    .get("data")
                                    .and_then(|d| d.get("nickname"))
                                    .and_then(|n| n.as_str()),
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

        // 启动连接
        let token = config.token.clone();
        let sse_url = config.get_event_url();
        let bot_state_clone = bot_state.clone();

        tokio::spawn(async move {
            connect_bot_sse(&sse_url, token, bot_state_clone).await;
        });
    }
}

async fn update_auto_connect_status(auto_connect: bool) {
    let _ = tokio::task::spawn_blocking(move || {
        let exe_dir = runtime::get_exe_dir();

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
    })
    .await;
}

async fn connect_bot_sse(
    sse_url: &str,
    token: Option<String>,
    bot_state: Arc<crate::server::BotConnectionState>,
) {
    loop {
        // 检查是否应该继续连接
        if !bot_state
            .should_connect
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            break;
        }

        // 构建HTTP客户端
        let client = reqwest::Client::new();
        let mut request_builder = client
            .get(sse_url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");

        // 如果有token，添加Authorization header
        if let Some(ref token_str) = token {
            request_builder =
                request_builder.header("Authorization", format!("Bearer {}", token_str));
        }

        match request_builder.send().await {
            Ok(response) => {
                if response.status().is_success() {
                    log_info!("连接成功");
                    bot_state
                        .is_connected
                        .store(true, std::sync::atomic::Ordering::SeqCst);
                    bot_state
                        .is_connecting
                        .store(false, std::sync::atomic::Ordering::SeqCst);

                    // 连接成功后，设置auto_connect为true
                    update_auto_connect_status(true).await;

                    // 发送连接成功状态
                    let status = BotStatusResponse {
                        connected: true,
                        connecting: false,
                    };
                    let _ = bot_state.status_sender.send(status);

                    let _ = handle_bot_sse_stream(response, bot_state.clone()).await;

                    bot_state
                        .is_connected
                        .store(false, std::sync::atomic::Ordering::SeqCst);
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
                    update_auto_connect_status(false).await;

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
                    update_auto_connect_status(false).await;

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
pub async fn list_plugins(
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<Vec<crate::plus::manager::PluginInfo>>> {
    // 重新加载插件列表
    let _ = manager.load_plugins().await;

    match manager.list_plugins().await {
        Ok(plugins) => Json(ApiResponse {
            retcode: 0,
            data: plugins,
        }),
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
pub async fn start_plugin(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    let name = manager
        .get_plugin_name(&plugin_id)
        .await
        .unwrap_or_else(|| plugin_id.clone());
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
pub async fn stop_plugin(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    // API 调用被视为用户主动停止
    let name = manager
        .get_plugin_name(&plugin_id)
        .await
        .unwrap_or_else(|| plugin_id.clone());
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
pub async fn get_plugin_output(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<Vec<String>>> {
    match manager.get_plugin_output(&plugin_id).await {
        Ok(output) => Json(ApiResponse {
            retcode: 0,
            data: output,
        }),
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
pub async fn clear_plugin_output(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.clear_plugin_output(&plugin_id).await {
        Ok(_) => Json(ApiResponse {
            retcode: 0,
            data: "Output cleared".to_string(),
        }),
        Err(e) => {
            log_error!("Failed to clear plugin output: {}", e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to clear output: {}", e),
            })
        }
    }
}

#[post("/plugins/<plugin_id>/open_dir")]
pub async fn open_plugin_dir(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.open_plugin_dir(&plugin_id).await {
        Ok(_) => Json(ApiResponse {
            retcode: 0,
            data: "Opening directory".to_string(),
        }),
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: e,
        }),
    }
}

#[post("/plugins/<plugin_id>/open_data_dir")]
pub async fn open_plugin_data_dir(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.open_plugin_data_dir(&plugin_id).await {
        Ok(_) => Json(ApiResponse {
            retcode: 0,
            data: "Opening directory".to_string(),
        }),
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: e,
        }),
    }
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum PluginUnifiedEvent {
    Output(crate::plus::manager::PluginOutputEvent),
    Status(crate::plus::manager::PluginStatusEvent),
}

#[get("/plugins/events_stream")]
pub fn plugins_events_stream(manager: &State<Arc<PluginManager>>) -> EventStream![Event + 'static] {
    let manager = manager.inner().clone();
    EventStream! {
        let mut rx_output = manager.subscribe_output();
        let mut rx_status = manager.subscribe_status();

        loop {
            let event = tokio::select! {
                res = rx_output.recv() => match res {
                    Ok(e) => Some(PluginUnifiedEvent::Output(e)),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(_) => None,
                },
                res = rx_status.recv() => match res {
                    Ok(e) => Some(PluginUnifiedEvent::Status(e)),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    Err(_) => None,
                },
            };

            if let Some(event) = event {
                if let Ok(json) = serde_json::to_string(&event) {
                    yield Event::data(json);
                }
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
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
pub fn plugin_output_stream(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> EventStream![Event + 'static] {
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

#[derive(Deserialize)]
pub struct ExportPluginRequest {
    pub plugin_id: String,
}

#[post("/plugins/export", format = "json", data = "<req>")]
pub async fn export_plugin(
    req: Json<ExportPluginRequest>,
    plugin_manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    let plugin_id = &req.plugin_id;

    // Get plugin directory
    let plugin_dir = match plugin_manager.get_plugin_dir(plugin_id).await {
        Some(path) => path,
        None => {
            return Json(ApiResponse {
                retcode: 1,
                data: "Plugin not found".to_string(),
            })
        }
    };

    // Run file dialog and compression in a blocking task
    // clone plugin_id for the closure
    let plugin_id_clone = plugin_id.clone();

    let result = tokio::task::spawn_blocking(move || {
        // Open file dialog
        // Default name: <plugin_id>.yuyu.7z
        let target_path = rfd::FileDialog::new()
            .set_file_name(format!("{}.yuyu.7z", plugin_id_clone))
            .add_filter("Yuyu Plugin", &["yuyu.7z"])
            .add_filter("7z Archive", &["7z"])
            .save_file();

        let target_path = match target_path {
            Some(p) => p,
            None => return Ok("Export cancelled".to_string()),
        };

        sevenz_rust2::compress_to_path(&plugin_dir, &target_path)
            .map_err(|e| format!("Failed to create 7z archive: {}", e))?;

        Ok::<String, String>("Export successful".to_string())
    })
    .await;

    match result {
        Ok(Ok(msg)) => Json(ApiResponse {
            retcode: 0,
            data: msg,
        }),
        Ok(Err(e)) => Json(ApiResponse {
            retcode: 1,
            data: e,
        }),
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: format!("Task failed: {}", e),
        }),
    }
}

#[post("/plugins/<plugin_id>/uninstall")]
pub async fn uninstall_plugin(
    plugin_id: String,
    manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    match manager.delete_plugin(&plugin_id).await {
        Ok(_) => {
            log_info!("Plugin {} uninstalled", plugin_id);
            Json(ApiResponse {
                retcode: 0,
                data: format!("Plugin {} uninstalled successfully", plugin_id),
            })
        }
        Err(e) => {
            log_error!("Failed to uninstall plugin {}: {}", plugin_id, e);
            Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to uninstall plugin: {}", e),
            })
        }
    }
}

#[post("/plugins/import")]
pub async fn import_plugin(
    plugin_manager: &State<Arc<PluginManager>>,
) -> Json<ApiResponse<String>> {
    let plugins_root = plugin_manager.get_plugins_root();

    let result = tokio::task::spawn_blocking(move || {
        // Open file dialog
        let target_path = rfd::FileDialog::new()
            .add_filter("Yuyu Plugin", &["yuyu.7z"])
            .pick_file();

        let target_path = match target_path {
            Some(p) => p,
            None => return Ok("Import cancelled".to_string()),
        };

        let filename = target_path
            .file_name()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

        if !filename.ends_with(".yuyu.7z") {
            return Err("Invalid plugin file. Must end with .yuyu.7z".to_string());
        }

        // Determine plugin ID from filename
        // e.g. "myplugin.yuyu.7z" -> "myplugin"
        let file_stem = target_path
            .file_stem()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

        // If it ends with .yuyu, remove it
        let plugin_id = if file_stem.ends_with(".yuyu") {
            file_stem.trim_end_matches(".yuyu").to_string()
        } else {
            file_stem
        };

        if plugin_id.is_empty() {
            return Err("Could not determine plugin ID from filename".to_string());
        }

        let target_dir = plugins_root.join(&plugin_id);

        if target_dir.exists() {
            return Err(format!("Plugin directory already exists: {}", plugin_id));
        }

        // Create target directory
        std::fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        let detect_format_and_extract = || -> Result<(), String> {
            use std::io::Read;

            let mut header = [0u8; 6];
            let mut f = std::fs::File::open(&target_path)
                .map_err(|e| format!("Failed to open file: {}", e))?;
            let n = f
                .read(&mut header)
                .map_err(|e| format!("Failed to read file header: {}", e))?;

            if n < 6 || header != [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
                return Err("Invalid plugin archive. Must be a valid 7z file".to_string());
            }

            sevenz_rust2::decompress_file(&target_path, &target_dir)
                .map_err(|e| format!("Failed to extract 7z archive: {}", e))?;

            Ok(())
        };

        if let Err(e) = detect_format_and_extract() {
            let _ = std::fs::remove_dir_all(&target_dir);
            return Err(e);
        }

        Ok::<String, String>(format!("Plugin {} imported successfully", plugin_id))
    })
    .await;

    match result {
        Ok(Ok(msg)) => Json(ApiResponse {
            retcode: 0,
            data: msg,
        }),
        Ok(Err(e)) => Json(ApiResponse {
            retcode: 1,
            data: e,
        }),
        Err(e) => Json(ApiResponse {
            retcode: 1,
            data: format!("Task failed: {}", e),
        }),
    }
}
