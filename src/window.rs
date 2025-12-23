use rfd::{MessageButtons, MessageDialog, MessageLevel};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use tao::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{Event, WindowEvent},
    event_loop::EventLoopProxy,
    window::{Window, WindowBuilder, WindowId},
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
};
use wry::{NewWindowResponse, WebViewBuilder};

#[cfg(not(debug_assertions))]
use crate::Assets;

#[derive(Clone)]
pub enum UserEvent {
    TitleChanged(WindowId, String),
    TrayIconEvent(TrayIconEvent),
    MenuEvent(MenuEvent),
    NewWindowRequested(String),
    StartDrag(WindowId),
    CloseWindow(WindowId),
    RestartRequested,
}

fn show_error_dialog(title: &str, message: &str) {
    MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(MessageLevel::Error)
        .set_buttons(MessageButtons::Ok)
        .show();
}

fn create_webview(
    window: &Window,
    proxy: EventLoopProxy<UserEvent>,
    url: &str,
    web_context: &mut wry::WebContext,
) -> wry::Result<wry::WebView> {
    let window_id = window.id();
    let proxy_for_title = proxy.clone();
    let proxy_for_new_window = proxy.clone();
    let proxy_for_ipc = proxy.clone();
    let window_id_for_ipc = window_id;

    WebViewBuilder::new_with_web_context(web_context)
        .with_url(url)
        .with_initialization_script(r#"
            window.close = function() {
                window.ipc.postMessage('close_window');
            };
        "#)
        .with_document_title_changed_handler(move |title| {
            let _ = proxy_for_title.send_event(UserEvent::TitleChanged(window_id, title));
        })
        .with_new_window_req_handler(move |request_url, _req| {
            let _ = proxy_for_new_window.send_event(UserEvent::NewWindowRequested(request_url));
            NewWindowResponse::Deny
        })
        .with_ipc_handler(move |request: http::Request<String>| {
            let msg = request.body();
            if msg == "drag_window" {
                let _ = proxy_for_ipc.send_event(UserEvent::StartDrag(window_id_for_ipc));
            } else if msg == "close_window" {
                let _ = proxy_for_ipc.send_event(UserEvent::CloseWindow(window_id_for_ipc));
            }
        })
        .build(window)
}

fn create_window_with_url(
    webviews: &mut HashMap<WindowId, (Window, wry::WebView)>,
    target: &tao::event_loop::EventLoopWindowTarget<UserEvent>,
    proxy: &EventLoopProxy<UserEvent>,
    url: &str,
    web_context: &mut wry::WebContext,
) {
    let version = env!("CARGO_PKG_VERSION");
    let window_res = WindowBuilder::new()
        .with_title(format!("羽羽BOT v{} - 加载中...", version))
        .with_inner_size(LogicalSize::new(1024.0, 768.0))
        .with_window_icon(load_window_icon())
        .with_visible(false)
        .build(target);

    let new_window = match window_res {
        Ok(w) => w,
        Err(e) => {
            let err_msg = format!("创建窗口失败: {}", e);
            log_error!("{}", err_msg);
            show_error_dialog("错误", &err_msg);
            return;
        }
    };

    // 居中 + 随机偏移
    if let Some(monitor) = new_window.current_monitor() {
        let screen_size = monitor.size();
        let window_size = new_window.outer_size();
        let center_x = (screen_size.width as i32 - window_size.width as i32) / 2;
        let center_y = (screen_size.height as i32 - window_size.height as i32) / 2;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as i32;
        let range = 80;
        let offset_x = (nanos % range) - (range / 2);
        let offset_y = ((nanos / 100) % range) - (range / 2);

        new_window.set_outer_position(PhysicalPosition::new(
            center_x + offset_x,
            center_y + offset_y,
        ));
    }

    new_window.set_visible(true);
    new_window.set_focus();
    new_window.set_always_on_top(true);
    new_window.set_always_on_top(false);

    let window_id = new_window.id();
    match create_webview(&new_window, proxy.clone(), url, web_context) {
        Ok(webview) => {
            webviews.insert(window_id, (new_window, webview));
        }
        Err(e) => {
            let err_msg = format!("创建 WebView 失败: {}", e);
            log_error!("{}", err_msg);
            show_error_dialog("错误", &err_msg);
        }
    }
}

fn show_or_create_main_window(
    webviews: &mut HashMap<WindowId, (Window, wry::WebView)>,
    main_window_id: &mut Option<WindowId>,
    target: &tao::event_loop::EventLoopWindowTarget<UserEvent>,
    proxy: &EventLoopProxy<UserEvent>,
    base_url: &str,
    web_context: &mut wry::WebContext,
) {
    if let Some(id) = *main_window_id {
        if let Some((window, _)) = webviews.get(&id) {
            window.set_minimized(false); // 取消最小化
            window.set_visible(true); //设置可见
            window.set_focus(); //设置焦点
            window.set_always_on_top(true); // 开启置顶
            window.set_always_on_top(false); // 关闭置顶

            return;
        } else {
            *main_window_id = None;
        }
    }

    let version = env!("CARGO_PKG_VERSION");
    let window_res = WindowBuilder::new()
        .with_title(format!("羽羽BOT v{} - 加载中...", version))
        .with_inner_size(LogicalSize::new(1024.0, 768.0))
        .with_window_icon(load_window_icon())
        .with_visible(false)
        .build(target);

    let new_window = match window_res {
        Ok(w) => w,
        Err(e) => {
            let err_msg = format!("创建主窗口失败: {}", e);
            log_error!("{}", err_msg);
            show_error_dialog("错误", &err_msg);
            return;
        }
    };

    if let Some(monitor) = new_window.current_monitor() {
        let screen_size = monitor.size();
        let window_size = new_window.outer_size();
        let center_x = (screen_size.width as i32 - window_size.width as i32) / 2;
        let center_y = (screen_size.height as i32 - window_size.height as i32) / 2;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as i32;
        let range = 80;
        let offset_x = (nanos % range) - (range / 2);
        let offset_y = ((nanos / 100) % range) - (range / 2);

        new_window.set_outer_position(PhysicalPosition::new(
            center_x + offset_x,
            center_y + offset_y,
        ));
    }

    new_window.set_visible(true);
    new_window.set_focus();
    new_window.set_always_on_top(true);
    new_window.set_always_on_top(false);

    let window_id = new_window.id();
    match create_webview(&new_window, proxy.clone(), base_url, web_context) {
        Ok(webview) => {
            *main_window_id = Some(window_id);
            webviews.insert(window_id, (new_window, webview));
        }
        Err(e) => {
            let err_msg = format!("创建主 WebView 失败: {}", e);
            log_error!("{}", err_msg);
            show_error_dialog("错误", &err_msg);
        }
    }
}

fn load_icon_data() -> Option<(Vec<u8>, u32, u32)> {
    #[cfg(debug_assertions)]
    {
        use std::path::Path;
        let path = Path::new("res/favicon.ico");
        let image = image::open(path).ok()?.to_rgba8();
        let (width, height) = image.dimensions();
        Some((image.into_raw(), width, height))
    }

    #[cfg(not(debug_assertions))]
    {
        let asset = Assets::get("favicon.ico")?;
        let image = image::load_from_memory(&asset.data).ok()?.to_rgba8();
        let (width, height) = image.dimensions();
        Some((image.into_raw(), width, height))
    }
}

fn load_tray_icon() -> Option<tray_icon::Icon> {
    let (data, width, height) = load_icon_data()?;
    tray_icon::Icon::from_rgba(data, width, height).ok()
}

fn load_window_icon() -> Option<tao::window::Icon> {
    let (data, width, height) = load_icon_data()?;
    tao::window::Icon::from_rgba(data, width, height).ok()
}

use crate::runtime;
use crate::server::ServerState;
use std::sync::Arc;

pub fn run_app(port: u16, server_state: Arc<ServerState>) {
    use tao::event_loop::{ControlFlow, EventLoopBuilder};

    let base_url = format!("http://127.0.0.1:{}", port);
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // 设置全局代理
    {
        let mut p = server_state.main_proxy.proxy.blocking_write();
        *p = Some(proxy.clone());
    }

    let mut webviews = HashMap::new();
    let mut main_window_id: Option<WindowId> = None;
    let mut initial_window_created = false;
    let mut is_restarting = false;

    // 初始化 WebContext
    let data_dir = runtime::get_exe_dir().join("config").join("web_data");
    let _ = std::fs::create_dir_all(&data_dir);
    let mut web_context = wry::WebContext::new(Some(data_dir));

    // 托盘菜单
    let show_item = MenuItem::new("显示窗口", true, None);
    let restart_item = MenuItem::new("重启程序", true, None);
    let quit_item = MenuItem::new("退出程序", true, None);
    let show_item_id = show_item.id().clone();
    let restart_item_id = restart_item.id().clone();
    let quit_item_id = quit_item.id().clone();

    let tray_menu = Menu::new();
    let _ = tray_menu.append_items(&[
        &show_item,
        &PredefinedMenuItem::separator(),
        &restart_item,
        &quit_item,
    ]);

    // 托盘图标
    let _tray_icon = if let Some(icon) = load_tray_icon() {
        let proxy_tray = proxy.clone();
        let proxy_menu = proxy.clone();

        TrayIconEvent::set_event_handler(Some(move |event| {
            let _ = proxy_tray.send_event(UserEvent::TrayIconEvent(event));
        }));
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = proxy_menu.send_event(UserEvent::MenuEvent(event));
        }));

        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("YuyuBot")
            .with_menu(Box::new(tray_menu))
            .build()
            .ok();

        if let Some(t) = &tray {
            t.set_show_menu_on_left_click(false);
        }

        tray
    } else {
        log_warn!("未找到 favicon.ico");
        None
    };

    event_loop.run(move |event, event_loop_window_target, control_flow| {
        *control_flow = ControlFlow::Wait;

        if !initial_window_created {
            show_or_create_main_window(
                &mut webviews,
                &mut main_window_id,
                event_loop_window_target,
                &proxy,
                &base_url,
                &mut web_context,
            );
            initial_window_created = true;
        }

        match event {
            Event::LoopDestroyed => {
                // 事件循环销毁时执行清理，使用全局 Runtime
                log_info!("正在停止所有插件...");
                runtime::block_on(async {
                    server_state
                        .plugin_manager
                        .stop_all_plugins_and_wait(std::time::Duration::from_secs(8))
                        .await;

                    server_state.plugin_manager.cleanup_tmp_apps().await;
                });

                if is_restarting {
                    log_info!("正在重启程序...");
                    let exe_path = runtime::get_exe_dir().join(
                        std::env::current_exe()
                            .ok()
                            .and_then(|p| p.file_name().map(|n| n.to_os_string()))
                            .unwrap_or_else(|| "yuyubot.exe".into()),
                    );

                    use std::os::windows::process::CommandExt;
                    let detached_process = 0x00000008;
                    let _ = std::process::Command::new(&exe_path)
                        .arg("--restarted")
                        .creation_flags(detached_process)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                webviews.remove(&window_id);
                if main_window_id == Some(window_id) {
                    main_window_id = None;
                }
            }
            Event::UserEvent(user_event) => match user_event {
                UserEvent::TitleChanged(window_id, title) => {
                    if let Some((window, _)) = webviews.get(&window_id) {
                        let version = env!("CARGO_PKG_VERSION");
                        window.set_title(&format!("{} v{}", title, version));
                    }
                }
                UserEvent::TrayIconEvent(TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }) => {
                    show_or_create_main_window(
                        &mut webviews,
                        &mut main_window_id,
                        event_loop_window_target,
                        &proxy,
                        &base_url,
                        &mut web_context,
                    );
                }
                UserEvent::MenuEvent(menu_event) => {
                    if menu_event.id == quit_item_id {
                        *control_flow = ControlFlow::Exit;
                    } else if menu_event.id == restart_item_id {
                        is_restarting = true;
                        *control_flow = ControlFlow::Exit;
                    } else if menu_event.id == show_item_id {
                        show_or_create_main_window(
                            &mut webviews,
                            &mut main_window_id,
                            event_loop_window_target,
                            &proxy,
                            &base_url,
                            &mut web_context,
                        );
                    }
                }
                UserEvent::CloseWindow(window_id) => {
                    webviews.remove(&window_id);
                    if main_window_id == Some(window_id) {
                        main_window_id = None;
                    }
                }
                UserEvent::RestartRequested => {
                    is_restarting = true;
                    *control_flow = ControlFlow::Exit;
                }
                UserEvent::NewWindowRequested(url) => {
                    create_window_with_url(
                        &mut webviews,
                        event_loop_window_target,
                        &proxy,
                        &url,
                        &mut web_context,
                    );
                }
                UserEvent::StartDrag(window_id) => {
                    if let Some((window, _)) = webviews.get(&window_id) {
                        let _ = window.drag_window();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    });
}
