pub mod api;
pub mod milky_proxy;
pub mod permissions;

use crate::plus::PluginManager;
use crate::runtime;
use crate::window::UserEvent;
use rocket::fairing::AdHoc;
#[cfg(debug_assertions)]
use rocket::fs::NamedFile;
#[cfg(not(debug_assertions))]
use rocket::http::ContentType;
use rocket::{get, routes, Config};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tao::event_loop::EventLoopProxy;
use tokio::sync::broadcast;
use tokio::sync::RwLock;

#[cfg(not(debug_assertions))]
use crate::Assets;

pub struct MainProxy {
    pub proxy: RwLock<Option<EventLoopProxy<UserEvent>>>,
}

pub struct BotConnectionState {
    pub is_connected: AtomicBool,
    pub is_connecting: AtomicBool,
    pub should_connect: AtomicBool,
    pub status_sender: broadcast::Sender<api::BotStatusResponse>,
    pub connection_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub cancel_sender: tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    /// 配置文件写入锁，防止并发写入导致配置损坏
    pub config_write_lock: tokio::sync::Mutex<()>,
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
    pub main_proxy: Arc<MainProxy>,
}

pub fn start_server_safe() -> Result<(u16, Arc<ServerState>), String> {
    let (tx, rx) = std::sync::mpsc::channel();

    // 预先创建插件管理器，以便返回给 main 函数使用
    let exe_dir = runtime::get_exe_dir();

    // 使用全局 Runtime spawn 服务器任务，而不是创建新的 Runtime
    runtime::spawn(async move {
        // 使用 0 让系统分配随机端口
        let main_port = 0;
        let mut milky_api_port = 0;
        let mut milky_event_port = 0;

        let get_random_port = || -> u16 {
            TcpListener::bind("127.0.0.1:0")
                .ok()
                .and_then(|l| l.local_addr().ok())
                .map(|a| a.port())
                .unwrap_or(0)
        };

        loop {
            let plugin_manager = Arc::new(PluginManager::new(
                exe_dir.clone(),
                main_port, // 这里传入 0，会在 on_liftoff 中更新
                "127.0.0.1".to_string(),
                milky_api_port,
                milky_event_port,
            ));

            let main_proxy = Arc::new(MainProxy {
                proxy: RwLock::new(None),
            });

            let server_state = Arc::new(ServerState {
                plugin_manager: plugin_manager.clone(),
                main_proxy: main_proxy.clone(),
            });

            let address = match "127.0.0.1".parse() {
                Ok(addr) => addr,
                Err(_) => return,
            };

            let data_dir = exe_dir.join("data").to_string_lossy().to_string();
            let plugins_root = exe_dir.join("app").to_string_lossy().to_string();

            let system_info = Arc::new(api::SystemInfo {
                port: std::sync::atomic::AtomicU16::new(main_port),
                data_dir,
                plugins_root,
            });

            let (status_sender, _) = broadcast::channel(100);
            let bot_state = Arc::new(BotConnectionState {
                is_connected: AtomicBool::new(false),
                is_connecting: AtomicBool::new(false),
                should_connect: AtomicBool::new(false),
                status_sender,
                connection_task: tokio::sync::Mutex::new(None),
                cancel_sender: tokio::sync::Mutex::new(None),
                config_write_lock: tokio::sync::Mutex::new(()),
            });

            let bot_config_state = Arc::new(RwLock::new(api::load_bot_config_from_disk(&exe_dir)));
            let permission_config_state = Arc::new(RwLock::new(
                permissions::load_permission_config_from_disk(&exe_dir),
            ));

            let config = Config {
                address,
                port: main_port,
                log_level: rocket::config::LogLevel::Off,
                ..Config::default()
            };

            let tx_clone = tx.clone();
            let plugin_manager_clone = plugin_manager.clone();
            let server_state_clone = server_state.clone();
            let system_info_clone = system_info.clone();

            let main_rocket = rocket::custom(config)
                .manage(system_info.clone())
                .manage(bot_state.clone())
                .manage(bot_config_state.clone())
                .manage(permission_config_state.clone())
                .manage(plugin_manager.clone())
                .manage(main_proxy.clone())
                .mount("/", routes![index, assets, api::set_webui])
                .mount(
                    "/api",
                    routes![
                        api::get_app_nums,
                        api::get_logs,
                        api::clear_logs,
                        api::logs_stream,
                        api::get_system_info,
                        api::open_data_dir,
                        api::open_plugins_dir,
                        api::restart_program,
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
                        api::open_plugin_dir,
                        api::open_plugin_data_dir,
                        api::plugin_output_stream,
                        api::plugins_status_stream,
                        api::plugins_events_stream,
                        api::get_ui_state,
                        api::save_ui_state,
                        permissions::get_permission_config,
                        permissions::get_permission_group_options,
                        permissions::save_permission_config
                    ],
                )
                .attach(AdHoc::on_liftoff("Get Port", move |rocket| {
                    Box::pin(async move {
                        let port = rocket.config().port;
                        log_info!("🚀 WebUI 实际上监听的端口是: {}", port);
                        plugin_manager_clone.set_server_port(port);
                        system_info_clone
                            .port
                            .store(port, std::sync::atomic::Ordering::SeqCst);
                        let _ = tx_clone.send((port, server_state_clone));
                    })
                }));

            // Ignite Main Server
            let main_ignited = match main_rocket.ignite().await {
                Ok(rocket) => rocket,
                Err(e) => {
                    log_warn!("Main server ignite failed on port {}: {:?}", main_port, e);
                    // 端口为 0 时不太可能失败，除非系统资源耗尽
                    // 只有当 milky 端口冲突导致 continue 时，main_port 才会是 0 以外的值（如果被修改过）
                    // 但这里我们始终用 0，所以不需要 retry logic for main port
                    // 为了保持代码结构，我们等待一下再重试
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
            };

            // Ignite Milky Proxy Servers
            let _handles = match milky_proxy::spawn_milky_proxy_servers(
                milky_api_port,
                milky_event_port,
                bot_config_state.clone(),
                permission_config_state.clone(),
                plugin_manager.clone(),
                bot_state.clone(),
            )
            .await
            {
                Ok(h) => h,
                Err(e) => {
                    log_warn!("Milky proxy ignite failed: {:?}", e);
                    milky_api_port = get_random_port();
                    milky_event_port = get_random_port();
                    continue;
                }
            };

            // 加载插件
            if let Err(e) = plugin_manager.load_plugins().await {
                log_error!("Failed to load plugins: {}", e);
            }

            // 自动启动之前启用的插件
            let plugin_manager_for_auto_start = plugin_manager.clone();
            tokio::spawn(async move {
                // 等待服务器端口就绪
                plugin_manager_for_auto_start.wait_for_port().await;

                let enabled_plugins = plugin_manager_for_auto_start.get_enabled_plugins().await;
                for plugin_id in enabled_plugins {
                    let name = plugin_manager_for_auto_start
                        .get_plugin_name(&plugin_id)
                        .await
                        .unwrap_or_else(|| plugin_id.clone());
                    log_info!("Auto-starting plugin: {}({})", name, plugin_id);
                    if let Err(e) = plugin_manager_for_auto_start.start_plugin(&plugin_id).await {
                        if e == "Plugin not found"
                            && plugin_manager_for_auto_start
                                .purge_enabled_plugin_if_absent(&plugin_id)
                                .await
                        {
                            continue;
                        }
                        log_error!("Failed to auto-start plugin {}({}): {}", name, plugin_id, e);
                    }
                }
            });

            // 检查并执行自动连接
            let bot_state_for_auto_connect = bot_state.clone();
            let plugin_manager_for_auto_connect = plugin_manager.clone();
            tokio::spawn(async move {
                // 等待服务器端口就绪
                plugin_manager_for_auto_connect.wait_for_port().await;
                api::check_and_auto_connect(bot_state_for_auto_connect).await;
            });

            // Launch Main Server
            tokio::spawn(main_ignited.launch());

            // Notify main thread - moved to on_liftoff
            // let _ = tx.send((main_port, server_state));
            break;
        }
    });

    rx.recv().map_err(|e| format!("接收服务器端口失败: {}", e))
}
