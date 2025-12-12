#![windows_subsystem = "windows"]

#[macro_use]
mod logger;
mod server;
mod window;

use rust_embed::Embed;

#[derive(Embed)]
#[folder = "res/"]
pub struct Assets;

fn main() {
    logger::init_logger();
    log_info!("框架启动");
    
    let port = server::start_server();
    window::run_app(port);
}
