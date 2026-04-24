use super::connection::connect_bot_sse;
use super::types::{default_bot_config, parse_url, BotConfig, LegacyBotConfig};
use super::BotStatusResponse;
use crate::runtime;
use crate::server::api::{ApiResponse, SystemInfo};
use rocket::{get, post, serde::json::Json, State};
use std::sync::Arc;
use tokio::sync::RwLock;

pub fn load_bot_config_from_disk(exe_dir: &std::path::Path) -> BotConfig {
    let config_file = exe_dir.join("config").join("config.json");

    let Ok(content) = std::fs::read_to_string(&config_file) else {
        return default_bot_config();
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
            token: legacy_config.token,
            ..default_bot_config()
        };
    }

    default_bot_config()
}

#[get("/bot/get_config")]
pub async fn get_bot_config() -> Json<ApiResponse<BotConfig>> {
    let exe_dir = runtime::get_exe_dir();
    let config_file = exe_dir.join("config").join("config.json");

    let (retcode, config) = match tokio::fs::read_to_string(&config_file).await {
        Ok(content) => {
            if let Ok(config) = serde_json::from_str::<BotConfig>(&content) {
                (0, config)
            } else {
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
                                token: legacy_config.token,
                                ..default_bot_config()
                            }
                        };

                        (0, config)
                    }
                    Err(_) => (1, default_bot_config()),
                }
            }
        }
        Err(_) => (0, default_bot_config()),
    };

    Json(ApiResponse {
        retcode,
        data: config,
    })
}

#[post("/bot/save_config", format = "json", data = "<config>")]
pub async fn save_bot_config(
    config: Json<BotConfig>,
    _system_info: &State<Arc<SystemInfo>>,
    bot_state: &State<Arc<crate::server::BotConnectionState>>,
    bot_config_state: &State<Arc<RwLock<BotConfig>>>,
) -> Json<ApiResponse<String>> {
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

    {
        let _guard = bot_state.config_write_lock.lock().await;

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
