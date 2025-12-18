#![windows_subsystem = "windows"]

#[macro_use]
mod logger;
mod plus;
mod server;
mod window;

use rust_embed::Embed;

#[derive(Embed)]
#[folder = "res/"]
pub struct Assets;

fn main() {
    logger::init_logger();
    log_info!("框架启动");

    let (port, server_state) = server::start_server();

    // 将清理逻辑移入 run_app 中，通过回调或者其他方式触发
    // 或者，由于 run_app 是阻塞的，我们需要确保它在退出时执行清理
    window::run_app(port, server_state);
}
