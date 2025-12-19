#![windows_subsystem = "windows"]

#[macro_use]
mod logger;
mod error;
mod plus;
mod runtime;
mod server;
mod window;

use rust_embed::Embed;

#[derive(Embed)]
#[folder = "res/"]
pub struct Assets;

mod windows_console {
    use windows_sys::Win32::System::Console::{
        SetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    pub fn ensure_hidden_console() {
        unsafe {
            // 还原标准句柄为 NULL (0)
            // 模拟 GUI 程序首次启动时的“无句柄”状态
            // 这样 expectrl 会认为没有控制台，从而自动创建 ConPTY 环境
            SetStdHandle(STD_INPUT_HANDLE, std::ptr::null_mut());
            SetStdHandle(STD_OUTPUT_HANDLE, std::ptr::null_mut());
            SetStdHandle(STD_ERROR_HANDLE, std::ptr::null_mut());
        }
    }
}

fn main() {
    logger::init_logger();

    // 如果是重启启动的，重新分配一个隐藏的控制台，以支持 expectrl/pty
    if std::env::args().any(|arg| arg == "--restarted") {
        windows_console::ensure_hidden_console();
        log_info!("检测到重启启动，已重建控制台环境");
    }

    log_info!("框架启动");

    // 初始化全局 Runtime（必须在所有异步操作之前）
    let _runtime = runtime::init_runtime();

    let (port, server_state) = match server::start_server_safe() {
        Ok(res) => res,
        Err(e) => {
            let err_msg = format!("服务器启动失败: {}", e);
            log_error!("{}", err_msg);
            rfd::MessageDialog::new()
                .set_title("错误")
                .set_description(&err_msg)
                .set_level(rfd::MessageLevel::Error)
                .show();
            return;
        }
    };

    // 将清理逻辑移入 run_app 中，通过回调或者其他方式触发
    // 或者，由于 run_app 是阻塞的，我们需要确保它在退出时执行清理
    window::run_app(port, server_state);
}
