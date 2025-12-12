pub mod api;

use rocket::{get, routes, Config};
#[cfg(debug_assertions)]
use rocket::fs::NamedFile;
#[cfg(not(debug_assertions))]
use rocket::http::ContentType;
use std::net::TcpListener;
use std::path::Path;
use std::thread;

#[cfg(not(debug_assertions))]
use crate::Assets;

#[cfg(debug_assertions)]
#[get("/")]
async fn index() -> Option<NamedFile> {
    NamedFile::open("res/index.html").await.ok()
}

#[cfg(not(debug_assertions))]
#[get("/")]
fn index() -> Option<(ContentType, Vec<u8>)> {
    serve_embedded_asset("index.html")
}

#[cfg(debug_assertions)]
#[get("/<path..>")]
async fn assets(path: std::path::PathBuf) -> Option<NamedFile> {
    let file_path = Path::new("res").join(&path);
    NamedFile::open(file_path).await.ok()
}

#[cfg(not(debug_assertions))]
#[get("/<path..>")]
fn assets(path: std::path::PathBuf) -> Option<(ContentType, Vec<u8>)> {
    serve_embedded_asset(path.to_str()?)
}

#[cfg(not(debug_assertions))]
fn serve_embedded_asset(path: &str) -> Option<(ContentType, Vec<u8>)> {
    let content_type = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| ContentType::from_extension(ext))
        .unwrap_or(ContentType::Binary);

    let asset = Assets::get(path)?;
    Some((content_type, asset.data.to_vec()))
}

pub fn start_server() -> u16 {
    let port = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(_) => {
                log_error!("Failed to get local address");
                return 0;
            }
        },
        Err(_) => {
            log_error!("Failed to bind to random port");
            return 0;
        }
    };

    thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                let address = match "127.0.0.1".parse() {
                    Ok(addr) => addr,
                    Err(_) => return,
                };
                
                // 获取可执行文件所在目录
                let exe_dir = std::env::current_exe()
                    .ok()
                    .and_then(|path| path.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                let data_dir = exe_dir.join("data").to_string_lossy().to_string();
                
                let system_info = std::sync::Arc::new(api::SystemInfo {
                    port,
                    data_dir,
                });
                
                let config = Config {
                    address,
                    port,
                    ..Config::default()
                };
                let _rocket = rocket::custom(config)
                    .manage(system_info)
                    .mount("/", routes![index, assets])
                    .mount("/api", routes![api::get_app_nums, api::get_logs, api::clear_logs, api::logs_stream, api::get_system_info, api::open_data_dir, api::get_app_info])
                    .launch()
                    .await;
            });
        }
    });

    port
}
