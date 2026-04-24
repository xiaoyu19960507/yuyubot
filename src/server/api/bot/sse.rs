use futures_util::StreamExt;
use std::sync::Arc;
use std::time::Duration;

pub(super) async fn handle_bot_sse_stream(
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
                if !bot_state.should_connect.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    Ok(())
}
