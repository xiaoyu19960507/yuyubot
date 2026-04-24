mod config;
mod connection;
mod sse;
mod types;

pub use config::{get_bot_config, load_bot_config_from_disk, save_bot_config};
pub use connection::{
    bot_status_stream, check_and_auto_connect, disconnect_bot, get_bot_status, get_login_info,
};
pub use types::{BotConfig, BotStatusResponse, LoginInfo};

use std::time::Duration;

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
