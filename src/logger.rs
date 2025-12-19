use chrono::Local;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Clone, Debug, Serialize)]
pub struct LogEntry {
    pub time: String,
    pub level: String,
    pub source: String,
    pub message: String,
}

pub struct LoggerState {
    logs: VecDeque<LogEntry>,
    tx: broadcast::Sender<LogEntry>,
}

static LOGGER: Lazy<Arc<Mutex<LoggerState>>> = Lazy::new(|| {
    let (tx, _) = broadcast::channel(100);
    Arc::new(Mutex::new(LoggerState {
        logs: VecDeque::with_capacity(1000),
        tx,
    }))
});

use once_cell::sync::OnceCell;

static LOGGER_INIT: OnceCell<()> = OnceCell::new();

struct AppLogLayer;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for AppLogLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();
        let level = metadata.level().to_string().to_lowercase();
        let target = metadata.target();

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        if !visitor.message.is_empty() {
            log_message(&level, target, visitor.message);
        }
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
            // 去除包裹在消息外的引号（如果是 Debug 格式化出来的）
            if self.message.starts_with('"') && self.message.ends_with('"') {
                self.message = self.message[1..self.message.len() - 1].to_string();
            }
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

pub fn init_logger() {
    LOGGER_INIT.get_or_init(|| {
        let filter = EnvFilter::from_default_env()
            .add_directive(tracing::Level::INFO.into())
            .add_directive("rocket=off".parse().unwrap()) // 完全屏蔽 Rocket 的所有日志
            .add_directive("hyper=off".parse().unwrap()); // 屏蔽底层 hyper 库的日志

        let registry = tracing_subscriber::registry()
            .with(AppLogLayer)
            .with(filter);

        // 仅在环境变量 YUYU_LOG_STDERR 设置时才输出到 stderr
        // 避免在 Windows GUI 模式下 hijack 插件控制台输出
        if std::env::var("YUYU_LOG_STDERR").is_ok() {
            registry
                .with(fmt::layer().with_writer(std::io::stderr).with_ansi(true))
                .init();
        } else {
            registry.init();
        }
    });
}

pub fn log_message(level: &str, source: &str, message: String) {
    let entry = LogEntry {
        time: Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
        level: level.to_string(),
        source: source.to_string(),
        message,
    };

    if let Ok(mut state) = LOGGER.lock() {
        if state.logs.len() >= 1000 {
            state.logs.pop_front();
        }
        state.logs.push_back(entry.clone());
        let _ = state.tx.send(entry);
    }
}

pub fn get_logs() -> Vec<LogEntry> {
    LOGGER
        .lock()
        .map(|state| state.logs.iter().cloned().collect())
        .unwrap_or_default()
}

pub fn clear_logs() {
    if let Ok(mut state) = LOGGER.lock() {
        state.logs.clear();
    }
}

pub fn subscribe_logs() -> broadcast::Receiver<LogEntry> {
    LOGGER
        .lock()
        .map(|state| state.tx.subscribe())
        .unwrap_or_else(|_| {
            let (tx, _) = broadcast::channel(100);
            tx.subscribe()
        })
}

// 便捷宏
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        tracing::info!(target: "[核心]", $($arg)*);
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        tracing::error!(target: "[核心]", $($arg)*);
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        tracing::warn!(target: "[核心]", $($arg)*);
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        tracing::debug!(target: "[核心]", $($arg)*);
    };
}
