//! 工具函数模块。

pub mod binary;
pub mod logging;
pub mod resource;

/// 项目版本。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 项目名称。
pub const NAME: &str = env!("CARGO_PKG_NAME");