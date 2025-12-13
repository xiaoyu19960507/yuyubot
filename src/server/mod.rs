pub mod api;

use rocket::{get, routes, Config};
#[cfg(debug_assertions)]
use rocket::fs::NamedFile;
#[cfg(not(debug_assertions))]
use rocket::http::ContentType;
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::broadcast;
use crate::plus::PluginManager;

#[cfg(not(debug_assertions))]
use crate::Assets;

pub struct BotConnectionState {
    pub is_connected: AtomicBool,
    pub is_connecting: AtomicBool,
    pub should_connect: AtomicBool,
    pub status_sender: broadcast::Sender<api::BotStatusResponse>,
}

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

pub struct ServerState {
    pub plugin_manager: Arc<PluginManager>,
}

pub fn start_server() -> (u16, Arc<ServerState>) {
    let port = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => match listener.local_addr() {
            Ok(addr) => addr.port(),
            Err(_) => {
                log_error!("Failed to get local address");
                return (0, Arc::new(ServerState { plugin_manager: Arc::new(PluginManager::new(std::path::PathBuf::from("."))) }));
            }
        },
        Err(_) => {
            log_error!("Failed to bind to random port");
            return (0, Arc::new(ServerState { plugin_manager: Arc::new(PluginManager::new(std::path::PathBuf::from("."))) }));
        }
    };
    
    // 预先创建插件管理器，以便返回给 main 函数使用
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let plugin_manager = Arc::new(PluginManager::new(exe_dir.clone()));
    
    let server_state = Arc::new(ServerState {
        plugin_manager: plugin_manager.clone(),
    });
    let server_state_clone = server_state.clone();

    thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Runtime::new() {
            rt.block_on(async {
                let address = match "127.0.0.1".parse() {
                    Ok(addr) => addr,
                    Err(_) => return,
                };
                
                let data_dir = exe_dir.join("data").to_string_lossy().to_string();
                
                let system_info = Arc::new(api::SystemInfo {
                    port,
                    data_dir,
                });
                
                let (status_sender, _) = broadcast::channel(100);
                let bot_state = Arc::new(BotConnectionState {
                    is_connected: AtomicBool::new(false),
                    is_connecting: AtomicBool::new(false),
                    should_connect: AtomicBool::new(false),
                    status_sender,
                });
                
                // 加载插件
                if let Err(e) = plugin_manager.load_plugins().await {
                    log_error!("Failed to load plugins: {}", e);
                }
                
                // 自动启动之前启用的插件
                let plugin_manager_for_auto_start = plugin_manager.clone();
                tokio::spawn(async move {
                    // 等待一小段时间确保服务器完全启动
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    let enabled_plugins = plugin_manager_for_auto_start.get_enabled_plugins();
                    for plugin_id in enabled_plugins {
                        let name = plugin_manager_for_auto_start.get_plugin_name(&plugin_id).await.unwrap_or_else(|| plugin_id.clone());
                        log_info!("Auto-starting plugin: {}({})", name, plugin_id);
                        if let Err(e) = plugin_manager_for_auto_start.start_plugin(&plugin_id).await {
                            log_error!("Failed to auto-start plugin {}({}): {}", name, plugin_id, e);
                        }
                    }
                });
                
                // 检查并执行自动连接
                let bot_state_for_auto_connect = bot_state.clone();
                tokio::spawn(async move {
                    // 等待一小段时间确保服务器完全启动
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    api::check_and_auto_connect(bot_state_for_auto_connect);
                });
                
                let config = Config {
                    address,
                    port,
                    ..Config::default()
                };
                let _rocket = rocket::custom(config)
                    .manage(system_info)
                    .manage(bot_state)
                    .manage(plugin_manager)
                    .mount("/", routes![index, assets])
                    .mount("/api", routes![
                        api::get_app_nums, 
                        api::get_logs, 
                        api::clear_logs, 
                        api::logs_stream, 
                        api::get_system_info, 
                        api::open_data_dir, 
                        api::get_app_info, 
                        api::get_bot_config, 
                        api::save_bot_config, 
                        api::disconnect_bot, 
                        api::get_bot_status, 
                        api::bot_status_stream, 
                        api::get_login_info,
                        api::list_plugins,
                        api::start_plugin,
                        api::stop_plugin,
                        api::uninstall_plugin,
                        api::export_plugin,
                        api::import_plugin,
                        api::get_plugin_output,
                        api::clear_plugin_output,
                        api::plugin_output_stream,
                        api::plugins_status_stream,
                        api::get_ui_state,
                        api::save_ui_state
                    ])
                    .launch()
                    .await;
            });
        }
    });

    (port, server_state_clone)
}
