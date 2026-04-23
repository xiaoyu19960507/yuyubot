use crate::runtime;
use crate::server::api::{ApiResponse, BotConfig};
use rocket::{get, post, serde::json::Json, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    #[default]
    Blacklist,
    Whitelist,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PermissionConfig {
    #[serde(default)]
    pub mode: PermissionMode,
    #[serde(default)]
    pub blacklist_groups: Vec<u64>,
    #[serde(default)]
    pub whitelist_groups: Vec<u64>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PermissionGroupOption {
    pub group_id: u64,
    pub group_name: String,
    pub member_count: u64,
    pub max_member_count: u64,
}

#[derive(Debug, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PermissionGroupOptionsData {
    pub groups: Vec<PermissionGroupOption>,
    pub connected: bool,
    pub message: String,
}

impl PermissionConfig {
    pub fn normalized(mut self) -> Self {
        normalize_group_ids(&mut self.blacklist_groups);
        normalize_group_ids(&mut self.whitelist_groups);
        self
    }
}

#[derive(Clone, Copy)]
enum ProtectedGroupApiKind {
    GroupId,
    GroupScenePeerId,
}

fn normalize_group_ids(groups: &mut Vec<u64>) {
    groups.sort_unstable();
    groups.dedup();
}

fn permission_config_path(exe_dir: &Path) -> std::path::PathBuf {
    exe_dir.join("config").join("permissions.json")
}

fn protected_group_api_kind(api: &str) -> Option<ProtectedGroupApiKind> {
    match api {
        "accept_group_invitation"
        | "accept_group_request"
        | "create_group_folder"
        | "delete_group_announcement"
        | "delete_group_file"
        | "delete_group_folder"
        | "kick_group_member"
        | "move_group_file"
        | "quit_group"
        | "recall_group_message"
        | "reject_group_invitation"
        | "reject_group_request"
        | "rename_group_file"
        | "rename_group_folder"
        | "send_group_announcement"
        | "send_group_message"
        | "send_group_message_reaction"
        | "send_group_nudge"
        | "set_group_avatar"
        | "set_group_essence_message"
        | "set_group_member_admin"
        | "set_group_member_card"
        | "set_group_member_mute"
        | "set_group_member_special_title"
        | "set_group_name"
        | "set_group_whole_mute"
        | "upload_group_file" => Some(ProtectedGroupApiKind::GroupId),
        "mark_message_as_read" | "set_peer_pin" => Some(ProtectedGroupApiKind::GroupScenePeerId),
        _ => None,
    }
}

pub fn load_permission_config_from_disk(exe_dir: &Path) -> PermissionConfig {
    let config_file = permission_config_path(exe_dir);

    let Ok(content) = std::fs::read_to_string(&config_file) else {
        return PermissionConfig::default();
    };

    serde_json::from_str::<PermissionConfig>(&content)
        .unwrap_or_default()
        .normalized()
}

pub fn extract_u64_field(value: &Value, field: &str) -> Option<u64> {
    match value.get(field) {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text)) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
}

pub fn extract_target_group_id_from_api(api: &str, body: &[u8]) -> Result<Option<u64>, String> {
    let Some(kind) = protected_group_api_kind(api) else {
        return Ok(None);
    };

    let payload: Value = serde_json::from_slice(body)
        .map_err(|_| format!("Protected API '{}' requires a valid JSON body", api))?;

    match kind {
        ProtectedGroupApiKind::GroupId => extract_u64_field(&payload, "group_id")
            .map(Some)
            .ok_or_else(|| format!("Protected API '{}' is missing a valid group_id", api)),
        ProtectedGroupApiKind::GroupScenePeerId => {
            let scene = payload
                .get("message_scene")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("Protected API '{}' is missing a valid message_scene", api)
                })?;

            if scene != "group" {
                return Ok(None);
            }

            extract_u64_field(&payload, "peer_id")
                .map(Some)
                .ok_or_else(|| format!("Protected API '{}' is missing a valid peer_id", api))
        }
    }
}

pub fn extract_target_group_id_from_event(data: &str) -> Option<u64> {
    let payload: Value = serde_json::from_str(data).ok()?;
    let event_data = payload.get("data")?;

    if let Some(group_id) = extract_u64_field(event_data, "group_id") {
        return Some(group_id);
    }

    let scene = event_data.get("message_scene").and_then(Value::as_str)?;
    if scene != "group" {
        return None;
    }

    extract_u64_field(event_data, "peer_id")
}

pub fn is_group_allowed(config: &PermissionConfig, group_id: u64) -> bool {
    match config.mode {
        PermissionMode::Blacklist => !config.blacklist_groups.contains(&group_id),
        PermissionMode::Whitelist => config.whitelist_groups.contains(&group_id),
    }
}

pub fn permission_mode_name(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Blacklist => "blacklist",
        PermissionMode::Whitelist => "whitelist",
    }
}

async fn fetch_group_options_from_config(
    config: &BotConfig,
) -> Result<Vec<PermissionGroupOption>, String> {
    let api_url = format!("{}/get_group_list", config.get_api_url());

    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to build reqwest client: {}", e))?;

    let mut request_builder = client
        .post(&api_url)
        .header("Content-Type", "application/json");

    if let Some(token_str) = config.token.as_deref() {
        request_builder =
            request_builder.header("Authorization", format!("Bearer {}", token_str));
    }

    let response = request_builder
        .body("{}".to_string())
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read API response: {}", e))?;

    if !status.is_success() {
        return Err(format!("API returned HTTP {}", status));
    }

    let payload: Value = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to parse API response: {}", e))?;

    if let Some(retcode) = payload.get("retcode").and_then(Value::as_i64) {
        if retcode != 0 {
            let message = payload
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| payload.get("data").and_then(Value::as_str))
                .unwrap_or("Milky API returned an error");
            return Err(message.to_string());
        }
    }

    let groups = payload
        .get("data")
        .and_then(|data| data.get("groups"))
        .and_then(Value::as_array)
        .ok_or_else(|| "API response missing data.groups".to_string())?;

    let mut result = Vec::with_capacity(groups.len());

    for group in groups {
        let Some(group_id) = extract_u64_field(group, "group_id") else {
            continue;
        };

        let group_name = group
            .get("group_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();

        let member_count = extract_u64_field(group, "member_count").unwrap_or(0);
        let max_member_count = extract_u64_field(group, "max_member_count").unwrap_or(0);

        result.push(PermissionGroupOption {
            group_id,
            group_name: if group_name.is_empty() {
                format!("群 {}", group_id)
            } else {
                group_name
            },
            member_count,
            max_member_count,
        });
    }

    result.sort_by(|left, right| left.group_name.cmp(&right.group_name));
    Ok(result)
}

#[get("/permissions/get_config")]
pub async fn get_permission_config(
    permission_config_state: &State<Arc<RwLock<PermissionConfig>>>,
) -> Json<ApiResponse<PermissionConfig>> {
    Json(ApiResponse {
        retcode: 0,
        data: permission_config_state.read().await.clone(),
    })
}

#[get("/permissions/group_options")]
pub async fn get_permission_group_options(
    bot_state: &State<Arc<crate::server::BotConnectionState>>,
    bot_config_state: &State<Arc<RwLock<BotConfig>>>,
) -> Json<ApiResponse<PermissionGroupOptionsData>> {
    if !bot_state.is_connected.load(Ordering::SeqCst) {
        return Json(ApiResponse {
            retcode: 0,
            data: PermissionGroupOptionsData {
                groups: Vec::new(),
                connected: false,
                message: "Bot not connected".to_string(),
            },
        });
    }

    let config = bot_config_state.read().await.clone();
    match fetch_group_options_from_config(&config).await {
        Ok(groups) => Json(ApiResponse {
            retcode: 0,
            data: PermissionGroupOptionsData {
                groups,
                connected: true,
                message: String::new(),
            },
        }),
        Err(message) => Json(ApiResponse {
            retcode: 1,
            data: PermissionGroupOptionsData {
                groups: Vec::new(),
                connected: true,
                message,
            },
        }),
    }
}

#[post("/permissions/save_config", format = "json", data = "<config>")]
pub async fn save_permission_config(
    config: Json<PermissionConfig>,
    permission_config_state: &State<Arc<RwLock<PermissionConfig>>>,
) -> Json<ApiResponse<String>> {
    let config_inner = config.into_inner().normalized();
    let json_str = match serde_json::to_string_pretty(&config_inner) {
        Ok(value) => value,
        Err(e) => {
            log_error!("Failed to serialize permission config: {}", e);
            return Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to serialize permission config: {}", e),
            });
        }
    };

    let exe_dir = runtime::get_exe_dir();
    let config_file = permission_config_path(&exe_dir);

    if let Some(config_dir) = config_file.parent() {
        if let Err(e) = tokio::fs::create_dir_all(config_dir).await {
            log_error!("Failed to create config directory: {}", e);
            return Json(ApiResponse {
                retcode: 1,
                data: format!("Failed to create config directory: {}", e),
            });
        }
    }

    if let Err(e) = tokio::fs::write(&config_file, json_str).await {
        log_error!("Failed to write permission config: {}", e);
        return Json(ApiResponse {
            retcode: 1,
            data: format!("Failed to write permission config: {}", e),
        });
    }

    *permission_config_state.write().await = config_inner;

    Json(ApiResponse {
        retcode: 0,
        data: "Permission config saved".to_string(),
    })
}
