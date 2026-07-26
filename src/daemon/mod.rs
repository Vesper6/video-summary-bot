//! HTTP API 服务模块。
//!
//! 提供 `serve` 命令，启动 HTTP API + Web GUI。

use crate::config::AppConfig;
use crate::error::Result;

/// HTTP API 服务子命令。
#[derive(Debug, clap::Args)]
pub struct ServeCmd {
    /// 监听端口
    #[arg(long, short, default_value = "8080")]
    pub port: u16,

    /// 监听地址
    #[arg(long, default_value = "0.0.0.0")]
    pub bind: String,
}

/// 运行 serve 命令（HTTP API + Web GUI）。
pub async fn run_serve(cmd: ServeCmd, _config: &AppConfig) -> Result<i32> {
    tracing::info!("starting HTTP API + GUI on {}:{}", cmd.bind, cmd.port);
    crate::api::serve(&cmd.bind, cmd.port).await?;
    Ok(0)
}
