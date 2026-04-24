use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Clone)]
pub struct BotStatusResponse {
    pub connected: bool,
    pub connecting: bool,
}

#[derive(Deserialize)]
pub(super) struct LegacyBotConfig {
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

impl BotConfig {
    pub fn get_api_url(&self) -> String {
        format!("http://{}:{}/api", self.host, self.api_port)
    }

    pub fn get_event_url(&self) -> String {
        format!("http://{}:{}/event", self.host, self.event_port)
    }
}

pub(super) fn default_bot_config() -> BotConfig {
    BotConfig {
        host: "localhost".to_string(),
        api_port: 3010,
        event_port: 3011,
        token: None,
        auto_connect: false,
    }
}

pub(super) fn parse_url(url: &str) -> Option<(String, u16)> {
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
