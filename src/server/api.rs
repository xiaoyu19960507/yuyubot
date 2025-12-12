use rocket::{get, post, serde::json::Json, State};
use ws::WebSocket;
use serde::Serialize;
use rocket::futures::SinkExt;
use crate::logger;
use std::sync::Arc;

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
pub fn logs_stream(ws: WebSocket) -> ws::Channel<'static> {
    ws.channel(move |mut stream| {
        Box::pin(async move {
            let mut rx = logger::subscribe_logs();
            
            while let Ok(log_entry) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&log_entry) {
                    let msg = ws::Message::Text(json);
                    if stream.send(msg).await.is_err() {
                        break;
                    }
                }
            }
            
            Ok(())
        })
    })
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
