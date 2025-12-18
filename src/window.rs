use std::collections::HashMap;
use std::path::Path;
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
}

fn create_webview(
    window: &Window,
    proxy: EventLoopProxy<UserEvent>,
    url: &str,
) -> wry::Result<wry::WebView> {
    let window_id = window.id();
    let proxy_for_title = proxy.clone();

    WebViewBuilder::new()
        .with_url(url)
        .with_document_title_changed_handler(move |title| {
            let _ = proxy_for_title.send_event(UserEvent::TitleChanged(window_id, title));
        })
        .with_new_window_req_handler(move |request_url, _req| {
            let _ = proxy.send_event(UserEvent::NewWindowRequested(request_url));
            NewWindowResponse::Deny
        })
        .build(window)
}

fn create_window_with_url(
    webviews: &mut HashMap<WindowId, (Window, wry::WebView)>,
    target: &tao::event_loop::EventLoopWindowTarget<UserEvent>,
    proxy: &EventLoopProxy<UserEvent>,
    url: &str,
) {
    let new_window = WindowBuilder::new()
        .with_title("加载中...")
        .with_inner_size(LogicalSize::new(1024.0, 768.0))
        .with_window_icon(load_window_icon())
        .with_visible(false)
        .build(target)
        .unwrap();

    // 居中 + 随机偏移
    if let Some(monitor) = new_window.current_monitor() {
        let screen_size = monitor.size();
        let window_size = new_window.outer_size();
        let center_x = (screen_size.width as i32 - window_size.width as i32) / 2;
        let center_y = (screen_size.height as i32 - window_size.height as i32) / 2;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
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
    match create_webview(&new_window, proxy.clone(), url) {
        Ok(webview) => {
            webviews.insert(window_id, (new_window, webview));
        }
        Err(e) => log_error!("创建窗口失败: {}", e),
    }
}

fn show_or_create_main_window(
    webviews: &mut HashMap<WindowId, (Window, wry::WebView)>,
    main_window_id: &mut Option<WindowId>,
    target: &tao::event_loop::EventLoopWindowTarget<UserEvent>,
    proxy: &EventLoopProxy<UserEvent>,
    base_url: &str,
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

    let new_window = WindowBuilder::new()
        .with_title("加载中...")
        .with_inner_size(LogicalSize::new(1024.0, 768.0))
        .with_window_icon(load_window_icon())
        .with_visible(false)
        .build(target)
        .unwrap();

    if let Some(monitor) = new_window.current_monitor() {
        let screen_size = monitor.size();
        let window_size = new_window.outer_size();
        let center_x = (screen_size.width as i32 - window_size.width as i32) / 2;
        let center_y = (screen_size.height as i32 - window_size.height as i32) / 2;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
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
    match create_webview(&new_window, proxy.clone(), base_url) {
        Ok(webview) => {
            *main_window_id = Some(window_id);
            webviews.insert(window_id, (new_window, webview));
        }
        Err(e) => log_error!("创建窗口失败: {}", e),
    }
}

fn load_icon_data() -> Option<(Vec<u8>, u32, u32)> {
    #[cfg(debug_assertions)]
    {
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

use crate::server::ServerState;
use std::sync::Arc;

pub fn run_app(port: u16, server_state: Arc<ServerState>) {
    use tao::event_loop::{ControlFlow, EventLoopBuilder};

    let base_url = format!("http://127.0.0.1:{}", port);
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let mut webviews = HashMap::new();
    let mut main_window_id: Option<WindowId> = None;
    let mut initial_window_created = false;

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

        #[cfg(target_os = "windows")]
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
            );
            initial_window_created = true;
        }

        match event {
            Event::LoopDestroyed => {
                // 事件循环销毁时执行清理
                log_info!("正在停止所有插件...");
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    server_state
                        .plugin_manager
                        .stop_all_plugins_and_wait(std::time::Duration::from_secs(8))
                        .await;
                });
                server_state.plugin_manager.cleanup_tmp_apps();
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
                        window.set_title(&title);
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
                    );
                }
                UserEvent::MenuEvent(menu_event) => {
                    if menu_event.id == quit_item_id {
                        *control_flow = ControlFlow::Exit;
                    } else if menu_event.id == restart_item_id {
                        // 重启前先停止所有插件
                        let rt = tokio::runtime::Runtime::new().unwrap();
                        rt.block_on(async {
                            server_state.plugin_manager.stop_all_plugins().await;
                        });

                        if let Ok(exe_path) = std::env::current_exe() {
                            let _ = std::process::Command::new(exe_path).spawn();
                        }
                        *control_flow = ControlFlow::Exit;
                    } else if menu_event.id == show_item_id {
                        show_or_create_main_window(
                            &mut webviews,
                            &mut main_window_id,
                            event_loop_window_target,
                            &proxy,
                            &base_url,
                        );
                    }
                }
                UserEvent::NewWindowRequested(url) => {
                    create_window_with_url(&mut webviews, event_loop_window_target, &proxy, &url);
                }
                _ => {}
            },
            _ => {}
        }
    });
}
