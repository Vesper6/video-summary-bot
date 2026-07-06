//! 命令行接口模块。
//!
//! 使用 clap 实现类似 tenbox 的 CLI：
//! - `vsb doctor` / `vsb system info`
//! - `vsb vm ls/create/start/stop/...`
//! - `vsb summarize --url ...`
//! - `vsb daemon start/stop/status`
//! - `vsb serve --port ...`
//! - `vsb gui`（桌面应用）

pub mod gui;
pub mod summary;
pub mod system;
pub mod vm;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

use crate::config::AppConfig;
use crate::error::Result;

/// 顶层 CLI 结构。
#[derive(Debug, Parser)]
#[command(
    name = "vsb",
    bin_name = "video-summary-bot",
    version,
    about = "Cross-platform micro VM for AI Agents",
    long_about = "Video Summary Bot - a cross-platform VMM that runs AI agents in hardware-isolated micro VMs."
)]
pub struct Cli {
    /// 全局选项
    #[command(flatten)]
    pub global: GlobalOpts,

    /// 子命令
    #[command(subcommand)]
    pub command: Command,
}

/// 全局选项（所有子命令共有）。
#[derive(Debug, clap::Args)]
pub struct GlobalOpts {
    /// 启用详细日志（-v / -vv / -vvv）
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// 配置文件路径
    #[arg(long, global = true, env = "VSB_CONFIG")]
    pub config: Option<std::path::PathBuf>,

    /// 日志格式（text / json）
    #[arg(long, global = true, env = "LOG_FORMAT", default_value = "text")]
    pub log_format: String,
}

/// 所有子命令枚举。
#[derive(Debug, Subcommand)]
pub enum Command {
    /// 健康检查（hypervisor 可用性、依赖完整性）
    #[command(subcommand)]
    Doctor(system::DoctorCmd),

    /// 系统信息
    Info(system::InfoCmd),

    /// VM 生命周期管理
    #[command(subcommand)]
    Vm(vm::VmCmd),

    /// 总结单个视频
    Summarize(summary::SummarizeCmd),

    /// 仅抓取评论/弹幕
    Crawl(summary::CrawlCmd),

    /// 守护进程管理
    #[command(subcommand)]
    Daemon(crate::daemon::DaemonCmd),

    /// 启动 HTTP API 服务（调试）
    Serve(crate::daemon::ServeCmd),

    /// 启动桌面 GUI（原生窗口应用）
    Gui(gui::GuiCmd),
}

impl Cli {
    /// 从 `std::env::args_os()` 解析。
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

/// 分发执行 CLI 命令。
pub async fn run(cli: Cli, config: &AppConfig) -> Result<i32> {
    // 根据 verbose 设置日志级别
    crate::utils::logging::set_level(cli.global.verbose);

    match cli.command {
        Command::Doctor(cmd) => system::run_doctor(cmd, config).await,
        Command::Info(cmd) => system::run_info(cmd, config).await,
        Command::Vm(cmd) => vm::run(cmd, config).await,
        Command::Summarize(cmd) => summary::run_summarize(cmd, config).await,
        Command::Crawl(cmd) => summary::run_crawl(cmd, config).await,
        Command::Daemon(cmd) => crate::daemon::run(cmd, config).await,
        Command::Serve(cmd) => crate::daemon::run_serve(cmd, config).await,
        Command::Gui(cmd) => gui::run(cmd, config).await,
    }
}

/// 进程退出码包装，便于 main 函数返回。
pub fn exit_code(code: i32) -> ExitCode {
    ExitCode::from(code as u8)
}