use crate::logger;
use crate::plus::PluginManager;
use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU16;
use std::sync::Arc;

mod bot;
mod plugins;
mod system;

pub use bot::{
    bot_status_stream, check_and_auto_connect, disconnect_bot, get_bot_config, get_bot_status,
    get_login_info, load_bot_config_from_disk, save_bot_config, BotConfig, BotStatusResponse,
};
pub use plugins::{
    clear_plugin_output, export_plugin, get_plugin_output, import_plugin, list_plugins,
    open_plugin_data_dir, open_plugin_dir, plugin_output_stream, plugins_events_stream,
    plugins_status_stream, start_plugin, stop_plugin, uninstall_plugin,
};
pub use system::{
    clear_logs, get_app_info, get_app_nums, get_logs, get_system_info, get_ui_state, logs_stream,
    open_data_dir, open_plugins_dir, restart_program, save_system_config, save_ui_state, set_webui,
};
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

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemConfig {
    #[serde(default)]
    pub auto_start: bool,
}

fn default_ui_last_page() -> String {
    "plugins".to_string()
}

fn default_ui_theme() -> String {
    "light".to_string()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UiState {
    #[serde(default = "default_ui_last_page")]
    pub last_page: String,
    #[serde(default = "default_ui_theme")]
    pub theme: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            last_page: default_ui_last_page(),
            theme: default_ui_theme(),
        }
    }
}

impl UiState {
    fn normalized(mut self) -> Self {
        if self.last_page.trim().is_empty() {
            self.last_page = default_ui_last_page();
        }

        if self.theme != "dark" {
            self.theme = default_ui_theme();
        }

        self
    }
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

#[derive(Serialize)]
pub struct SystemInfoResponse {
    pub port: u16,
    pub data_dir: String,
    #[serde(rename = "plugins_root")]
    pub plugins_root: String,
    pub auto_start: bool,
}
