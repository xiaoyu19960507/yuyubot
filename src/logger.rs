use chrono::Local;
use lazy_static::lazy_static;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

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

lazy_static! {
    static ref LOGGER: Arc<Mutex<LoggerState>> = {
        let (tx, _) = broadcast::channel(100);
        Arc::new(Mutex::new(LoggerState {
            logs: VecDeque::with_capacity(1000),
            tx,
        }))
    };
}

pub fn init_logger() {
    // 初始化 tracing 订阅者
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .init();
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
        {
            let msg = format!($($arg)*);
            tracing::info!("{}", msg);
            $crate::logger::log_message("info", "[核心]", msg);
        }
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        {
            let msg = format!($($arg)*);
            tracing::error!("{}", msg);
            $crate::logger::log_message("error", "[核心]", msg);
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        {
            let msg = format!($($arg)*);
            tracing::warn!("{}", msg);
            $crate::logger::log_message("warn", "[核心]", msg);
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        {
            let msg = format!($($arg)*);
            tracing::debug!("{}", msg);
            $crate::logger::log_message("debug", "[核心]", msg);
        }
    };
}
