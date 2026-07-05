//! 系统级子命令：doctor / info

use clap::Subcommand;

use crate::config::AppConfig;
use crate::error::Result;

/// doctor 子命令枚举。
#[derive(Debug, Subcommand)]
pub enum DoctorCmd {
    /// 完整健康检查
    All,
}

/// info 子命令枚举。
#[derive(Debug, clap::Args)]
pub struct InfoCmd {
    /// 以 JSON 格式输出
    #[arg(long)]
    pub json: bool,
}

/// 运行 doctor。
pub async fn run_doctor(_cmd: DoctorCmd, _config: &AppConfig) -> Result<i32> {
    tracing::info!("running health checks...");

    // 1. 检查 hypervisor 后端
    let hv_status = crate::hypervisor::probe();
    tracing::info!("hypervisor backend: {hv_status}");

    // 2. 检查 Claude Code 可用
    let claude_status = crate::agents::claude::probe();
    tracing::info!("claude code: {claude_status}");

    // 3. 检查 FFmpeg
    let ffmpeg_status = crate::utils::binary::probe("ffmpeg");
    tracing::info!("ffmpeg: {ffmpeg_status}");

    // 4. 检查资源目录
    let resource_status = crate::utils::resource::probe();
    tracing::info!("resources: {resource_status}");

    if hv_status.is_ok() && claude_status.is_ok() {
        tracing::info!("all checks passed");
        Ok(0)
    } else {
        tracing::warn!("some checks failed");
        Ok(1)
    }
}

/// 运行 info。
pub async fn run_info(cmd: InfoCmd, _config: &AppConfig) -> Result<i32> {
    let info = SystemInfo::gather();

    if cmd.json {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        info.print_table();
    }
    Ok(0)
}

/// 系统信息聚合。
#[derive(Debug, serde::Serialize)]
pub struct SystemInfo {
    pub version: String,
    pub platform: String,
    pub hypervisor: String,
    pub cpu_count: usize,
    pub total_memory_mb: u64,
    pub kernel: String,
}

impl SystemInfo {
    pub fn gather() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            hypervisor: crate::hypervisor::backend_name().to_string(),
            cpu_count: num_cpus(),
            total_memory_mb: total_memory_mb(),
            kernel: std::env::consts::FAMILY.to_string(),
        }
    }

    pub fn print_table(&self) {
        println!("version       : {}", self.version);
        println!("platform      : {}", self.platform);
        println!("hypervisor    : {}", self.hypervisor);
        println!("cpu count     : {}", self.cpu_count);
        println!("total memory  : {} MB", self.total_memory_mb);
        println!("kernel family : {}", self.kernel);
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn total_memory_mb() -> u64 {
    // 平台无关的近似实现
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
            for line in s.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    if let Some(kb) = rest.split_whitespace().next() {
                        if let Ok(kb) = kb.parse::<u64>() {
                            return kb / 1024;
                        }
                    }
                }
            }
        }
    }
    0
}