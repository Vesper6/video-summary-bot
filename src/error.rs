//! 全局错误类型定义。

use thiserror::Error;

/// 项目统一错误类型。
#[derive(Debug, Error)]
pub enum Error {
    /// 配置错误
    #[error("config error: {0}")]
    Config(String),

    /// CLI 解析错误
    #[error("cli error: {0}")]
    Cli(String),

    /// Hypervisor / VMM 错误
    #[error("vmm error: {0}")]
    Vmm(String),

    /// 设备错误
    #[error("device error: {0}")]
    Device(String),

    /// 镜像错误
    #[error("image error: {0}")]
    Image(String),

    /// 网络错误
    #[error("network error: {0}")]
    Network(String),

    /// Agent 错误
    #[error("agent error: {0}")]
    Agent(String),

    /// 抓取错误
    #[error("crawler error: {0}")]
    Crawler(String),

    /// 总结错误
    #[error("summarizer error: {0}")]
    Summarizer(String),

    /// I/O 错误
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// 序列化错误
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// 其他错误
    #[error("{0}")]
    Other(String),
}

/// 项目 Result 类型别名。
pub type Result<T> = std::result::Result<T, Error>;