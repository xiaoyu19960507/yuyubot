#![windows_subsystem = "windows"]

#[macro_use]
mod logger;
mod error;
mod plus;
mod runtime;
mod server;
mod window;

use rust_embed::Embed;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Threading::{CreateMutexW, CreateEventW, SetEvent};

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

struct SingleInstanceGuard {
    mutex_handle: usize,
    activate_event_handle: usize,
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        if self.activate_event_handle != 0 {
            unsafe { CloseHandle(self.activate_event_handle as HANDLE) };
        }
        if self.mutex_handle != 0 {
            unsafe { CloseHandle(self.mutex_handle as HANDLE) };
        }
    }
}

fn fnv1a_hash(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in input.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn acquire_single_instance_or_exit() -> Result<SingleInstanceGuard, String> {
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("获取当前可执行文件路径失败: {}", e))?;
    let exe_path_norm = exe_path
        .to_string_lossy()
        .to_lowercase();
    let hash = fnv1a_hash(&exe_path_norm);

    let mutex_name = format!("Global\\yuyubot_single_instance_mutex_{:016x}", hash);
    let event_name = format!("Global\\yuyubot_single_instance_activate_{:016x}", hash);

    let mutex_name_wide: Vec<u16> = OsStr::new(&mutex_name)
        .encode_wide()
        .chain(Some(0))
        .collect();

    let event_name_wide: Vec<u16> = OsStr::new(&event_name)
        .encode_wide()
        .chain(Some(0))
        .collect();

    let mutex_handle = unsafe { CreateMutexW(std::ptr::null_mut(), 0, mutex_name_wide.as_ptr()) };
    if mutex_handle.is_null() {
        let err_code = unsafe { GetLastError() };
        return Err(format!("CreateMutexW 失败: {}", err_code));
    }

    let event_handle = unsafe { CreateEventW(std::ptr::null_mut(), 0, 0, event_name_wide.as_ptr()) };
    if event_handle.is_null() {
        unsafe { CloseHandle(mutex_handle) };
        let err_code = unsafe { GetLastError() };
        return Err(format!("CreateEventW 失败: {}", err_code));
    }

    let duplicated = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if duplicated {
        // 触发激活事件，让主进程窗口聚焦
        unsafe {
            SetEvent(event_handle);
            CloseHandle(event_handle);
            CloseHandle(mutex_handle);
        }
        return Err("已存在正在运行的程序实例".into());
    }

    Ok(SingleInstanceGuard {
        mutex_handle: mutex_handle as usize,
        activate_event_handle: event_handle as usize,
    })
}

fn main() {
    logger::init_logger();

    let single_instance_guard = match acquire_single_instance_or_exit() {
        Ok(g) => g,
        Err(err_msg) => {
            log_warn!("{}", err_msg);
            // 已有实例正在运行；直接退出即可，已有实例会收到激活事件并聚焦窗口。
            return;
        }
    };

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
    let mutex_handle = single_instance_guard.mutex_handle;
    let activate_event_handle = single_instance_guard.activate_event_handle;
    // 放弃 guard 的析构，将句柄交给 run_app 自己管理，以便在重启前主动释放
    std::mem::forget(single_instance_guard);

    window::run_app(port, server_state, activate_event_handle, mutex_handle);
}
