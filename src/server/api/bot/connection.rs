use super::fetch_bot_login_info_from_config;
use super::sse::handle_bot_sse_stream;
use super::types::{BotConfig, BotStatusResponse, LoginInfo};
use crate::runtime;
use crate::server::api::ApiResponse;
use rocket::{
    get, post,
    response::stream::{Event, EventStream},
    serde::json::Json,
    State,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

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

    if let Some(handle) = bot_state.connection_task.lock().await.take() {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
    }

    update_auto_connect_status(bot_state, false).await;

    {
        let mut config = bot_config_state.write().await;
        config.auto_connect = false;
    }

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

        let connected = bot_state.is_connected.load(std::sync::atomic::Ordering::SeqCst);
        let connecting = bot_state.is_connecting.load(std::sync::atomic::Ordering::SeqCst);
        let status = BotStatusResponse { connected, connecting };
        if let Ok(json) = serde_json::to_string(&status) {
            yield Event::data(json);
        }

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
        bot_state
            .should_connect
            .store(true, std::sync::atomic::Ordering::SeqCst);
        bot_state
            .is_connecting
            .store(true, std::sync::atomic::Ordering::SeqCst);

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

pub(super) async fn update_auto_connect_status(
    bot_state: &Arc<crate::server::BotConnectionState>,
    auto_connect: bool,
) {
    let _guard = bot_state.config_write_lock.lock().await;

    let exe_dir = runtime::get_exe_dir();
    let config_file = exe_dir.join("config").join("config.json");

    if let Ok(content) = tokio::fs::read_to_string(&config_file).await {
        if let Ok(mut config) = serde_json::from_str::<BotConfig>(&content) {
            config.auto_connect = auto_connect;

            if let Ok(json_str) = serde_json::to_string_pretty(&config) {
                let _ = tokio::fs::write(&config_file, json_str).await;
            }
        }
    }
}

pub(super) async fn connect_bot_sse(
    config: BotConfig,
    bot_state: Arc<crate::server::BotConnectionState>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    loop {
        if !bot_state
            .should_connect
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            break;
        }

        let sse_url = config.get_event_url();

        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("Failed to build reqwest client");
        let mut request_builder = client
            .get(&sse_url)
            .header("Accept", "text/event-stream")
            .header("Cache-Control", "no-cache");

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

                            update_auto_connect_status(&bot_state, true).await;

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

                if !bot_state
                    .should_connect
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    bot_state
                        .is_connecting
                        .store(false, std::sync::atomic::Ordering::SeqCst);

                    update_auto_connect_status(&bot_state, false).await;

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

                let status = BotStatusResponse {
                    connected: false,
                    connecting: true,
                };
                let _ = bot_state.status_sender.send(status);

                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(e) => {
                log_error!("连接断开，重连中... (错误: {})", e);
                if !bot_state
                    .should_connect
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    bot_state
                        .is_connecting
                        .store(false, std::sync::atomic::Ordering::SeqCst);

                    update_auto_connect_status(&bot_state, false).await;

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
