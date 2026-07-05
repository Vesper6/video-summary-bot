//! VM 生命周期子命令（参考 tenbox vm 命令设计）。

use clap::Subcommand;

use crate::config::AppConfig;
use crate::error::Result;

/// VM 子命令枚举。
#[derive(Debug, Subcommand)]
pub enum VmCmd {
    /// 列出所有 VM
    Ls(LsCmd),

    /// 创建 VM
    Create(CreateCmd),

    /// 编辑 VM 配置
    Edit(EditCmd),

    /// 启动 VM
    Start(NameCmd),

    /// 停止 VM
    Stop(NameCmd),

    /// 重启 VM
    Reboot(NameCmd),

    /// 优雅关机
    Shutdown(NameCmd),

    /// 删除 VM
    Rm(RmCmd),

    /// 连接控制台
    Console(NameCmd),

    /// 查看日志
    Logs(LogsCmd),
}

#[derive(Debug, clap::Args)]
pub struct LsCmd {
    /// 以 JSON 格式输出
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct CreateCmd {
    /// VM 名称
    #[arg(long)]
    pub name: String,

    /// vCPU 数量
    #[arg(long, default_value = "2")]
    pub cpus: u8,

    /// 内存大小（MB）
    #[arg(long, default_value = "2048")]
    pub memory: u32,

    /// 磁盘大小（GB）
    #[arg(long, default_value = "10")]
    pub disk: u32,

    /// Linux 内核路径
    #[arg(long)]
    pub kernel: Option<std::path::PathBuf>,

    /// Initramfs 路径
    #[arg(long)]
    pub initramfs: Option<std::path::PathBuf>,

    /// Rootfs 路径
    #[arg(long)]
    pub rootfs: Option<std::path::PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct EditCmd {
    /// VM 名称
    #[arg(long)]
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct NameCmd {
    /// VM 名称
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct RmCmd {
    /// VM 名称
    pub name: String,

    /// 强制删除（不确认）
    #[arg(long, short)]
    pub force: bool,
}

#[derive(Debug, clap::Args)]
pub struct LogsCmd {
    /// VM 名称
    pub name: String,

    /// 跟踪日志（持续输出）
    #[arg(long, short)]
    pub follow: bool,
}

/// VM 调度入口。
pub async fn run(cmd: VmCmd, _config: &AppConfig) -> Result<i32> {
    match cmd {
        VmCmd::Ls(c) => ls(c).await,
        VmCmd::Create(c) => create(c).await,
        VmCmd::Edit(c) => edit(c).await,
        VmCmd::Start(c) => start(c).await,
        VmCmd::Stop(c) => stop(c).await,
        VmCmd::Reboot(c) => reboot(c).await,
        VmCmd::Shutdown(c) => shutdown(c).await,
        VmCmd::Rm(c) => rm(c).await,
        VmCmd::Console(c) => console(c).await,
        VmCmd::Logs(c) => logs(c).await,
    }
}

async fn ls(_cmd: LsCmd) -> Result<i32> {
    tracing::warn!("vm ls: not yet implemented");
    Ok(0)
}

async fn create(_cmd: CreateCmd) -> Result<i32> {
    tracing::warn!("vm create: not yet implemented");
    Ok(0)
}

async fn edit(_cmd: EditCmd) -> Result<i32> {
    tracing::warn!("vm edit: not yet implemented");
    Ok(0)
}

async fn start(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm start: not yet implemented");
    Ok(0)
}

async fn stop(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm stop: not yet implemented");
    Ok(0)
}

async fn reboot(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm reboot: not yet implemented");
    Ok(0)
}

async fn shutdown(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm shutdown: not yet implemented");
    Ok(0)
}

async fn rm(_cmd: RmCmd) -> Result<i32> {
    tracing::warn!("vm rm: not yet implemented");
    Ok(0)
}

async fn console(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm console: not yet implemented");
    Ok(0)
}

async fn logs(_cmd: LogsCmd) -> Result<i32> {
    tracing::warn!("vm logs: not yet implemented");
    Ok(0)
}