//! 日志初始化。

use std::sync::Once;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

static INIT: Once = Once::new();

/// 初始化日志（全局只执行一次）。
pub fn init() {
    INIT.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));

        let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "text".to_string());

        let registry = tracing_subscriber::registry().with(filter);

        // 注：JSON 格式需要启用 tracing-subscriber 的 "json" feature
        // 暂只支持 text 格式（默认）
        let _ = log_format; // 抑制 unused 警告
        registry.with(fmt::layer().with_target(true)).init();
    });
}

/// 根据 verbose 级别调整日志。
pub fn set_level(verbose: u8) {
    let level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    std::env::set_var("RUST_LOG", level);
}