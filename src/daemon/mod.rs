//! 守护进程模块（参考 tenboxd）。
//!
//! Linux 下由 systemd 管理，Windows/macOS 下以后台服务形式运行。
//!
//! 通过 Unix socket（Linux）/ Named Pipe（Windows）提供本地 RPC。

use clap::Subcommand;

use crate::config::AppConfig;
use crate::error::Result;

/// 守护进程子命令。
#[derive(Debug, Subcommand)]
pub enum DaemonCmd {
    /// 启动守护进程
    Start,

    /// 停止守护进程
    Stop,

    /// 查看状态
    Status,
}

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

/// 运行 daemon 命令。
pub async fn run(cmd: DaemonCmd, _config: &AppConfig) -> Result<i32> {
    match cmd {
        DaemonCmd::Start => {
            tracing::warn!("daemon start: not yet implemented");
            Ok(0)
        }
        DaemonCmd::Stop => {
            tracing::warn!("daemon stop: not yet implemented");
            Ok(0)
        }
        DaemonCmd::Status => {
            tracing::warn!("daemon status: not yet implemented");
            Ok(0)
        }
    }
}

/// 运行 serve 命令。
pub async fn run_serve(cmd: ServeCmd, _config: &AppConfig) -> Result<i32> {
    tracing::info!("starting HTTP API server on {}:{}", cmd.bind, cmd.port);
    tracing::warn!("serve: not yet implemented");
    Ok(0)
}