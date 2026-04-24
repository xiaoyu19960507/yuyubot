use once_cell::sync::OnceCell;
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Runtime;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

/// 全局共享的 Tokio Runtime
///
/// 所有需要异步操作的组件都应该使用这个 Runtime，而不是创建自己的。
/// 这样可以确保：
/// 1. Runtime 生命周期与应用程序一致
/// 2. 避免多 Runtime 混用导致的问题
/// 3. 优雅退出时能正确清理所有异步任务
static GLOBAL_RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();
const AUTO_START_REG_PATH: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

/// 初始化全局 Runtime
///
/// 应该在程序启动时调用一次。如果已经初始化过，会返回现有的 Runtime。
pub fn init_runtime() -> Arc<Runtime> {
    GLOBAL_RUNTIME
        .get_or_init(|| {
            Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create global Tokio runtime"),
            )
        })
        .clone()
}

/// 获取全局 Runtime 的引用
///
/// 如果 Runtime 尚未初始化，会自动初始化。
pub fn get_runtime() -> Arc<Runtime> {
    init_runtime()
}

/// 获取全局 Runtime 的 Handle
///
/// 用于在非异步上下文中执行异步操作。
pub fn get_handle() -> tokio::runtime::Handle {
    get_runtime().handle().clone()
}

/// 在全局 Runtime 上执行异步任务并阻塞等待结果
///
/// 注意：不要在已经处于异步上下文中时调用此函数，否则会 panic。
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    get_runtime().block_on(future)
}

/// 在全局 Runtime 上 spawn 一个异步任务
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    get_runtime().spawn(future)
}

/// 获取可执行文件所在的目录
pub fn get_exe_dir() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

fn auto_start_command() -> Result<String, String> {
    let exe_path =
        std::env::current_exe().map_err(|e| format!("Failed to resolve executable path: {}", e))?;
    Ok(format!("\"{}\" --autostart", exe_path.display()))
}

fn normalize_path_for_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

fn fnv1a_hash(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn auto_start_value_name() -> Result<String, String> {
    let exe_path =
        std::env::current_exe().map_err(|e| format!("Failed to resolve executable path: {}", e))?;
    let normalized = normalize_path_for_key(&exe_path);
    Ok(format!("YuyuBot_{:016x}", fnv1a_hash(&normalized)))
}

fn registry_value_matches_command(
    run_key: &RegKey,
    value_name: &str,
    command: &str,
) -> Result<bool, String> {
    match run_key.get_value::<String, _>(value_name) {
        Ok(value) => Ok(value.trim().eq_ignore_ascii_case(command)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(format!(
            "Failed to read auto-start registry value '{}': {}",
            value_name, err
        )),
    }
}

pub fn is_auto_start_enabled() -> Result<bool, String> {
    let command = auto_start_command()?;
    let value_name = auto_start_value_name()?;
    let run_key = match RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(AUTO_START_REG_PATH, KEY_READ)
    {
        Ok(key) => key,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(format!("Failed to open auto-start registry key: {}", err));
        }
    };

    registry_value_matches_command(&run_key, &value_name, &command)
}

pub fn set_auto_start_enabled(enabled: bool) -> Result<(), String> {
    let command = auto_start_command()?;
    let value_name = auto_start_value_name()?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run_key = hkcu
        .open_subkey_with_flags(AUTO_START_REG_PATH, KEY_READ | KEY_SET_VALUE)
        .or_else(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                hkcu.create_subkey(AUTO_START_REG_PATH).map(|(key, _)| key)
            } else {
                Err(err)
            }
        })
        .map_err(|err| format!("Failed to open auto-start registry key: {}", err))?;

    if enabled {
        run_key
            .set_value(&value_name, &command)
            .map_err(|err| format!("Failed to enable auto-start: {}", err))
    } else {
        match run_key.delete_value(&value_name) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("Failed to disable auto-start: {}", err)),
        }

        Ok(())
    }
}

pub fn is_auto_start_launch() -> bool {
    std::env::args().any(|arg| arg == "--autostart")
}

/// 在文件资源管理器中打开指定路径
pub fn open_in_explorer<P: AsRef<std::path::Path>>(path: P) {
    let path = path.as_ref();
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(path);

    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let rsp = cmd.spawn();
    if let Err(e) = rsp {
        log_error!("Failed to open explorer: {},{}", path.display(), e);
    }
}
