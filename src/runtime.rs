use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// 全局共享的 Tokio Runtime
///
/// 所有需要异步操作的组件都应该使用这个 Runtime，而不是创建自己的。
/// 这样可以确保：
/// 1. Runtime 生命周期与应用程序一致
/// 2. 避免多 Runtime 混用导致的问题
/// 3. 优雅退出时能正确清理所有异步任务
static GLOBAL_RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

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
