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
    println!("🩺 Video Summary Bot - 健康检查");
    println!("────────────────────────────────────────");

    let mut required_ok = 0usize;
    let mut required_total = 0usize;

    // ── 必选依赖 ────────────────────────────────
    println!("\n【必选】");

    // 1. Claude API
    required_total += 1;
    let claude = crate::agents::claude::probe();
    print_check("Claude API", &claude, false);
    if claude.is_ok() { required_ok += 1; }

    // 2. yt-dlp（字幕下载）
    required_total += 1;
    let ytdlp = probe_ytdlp();
    print_check("yt-dlp", &ytdlp, false);
    if ytdlp.is_ok() { required_ok += 1; }

    // ── 可选依赖 ────────────────────────────────
    println!("\n【可选】");

    // 3. Hypervisor（运行 micro VM 才需要）
    let hv = crate::hypervisor::probe();
    print_check("Hypervisor (VM)", &hv, true);

    // 4. FFmpeg（音频转写才需要）
    let ffmpeg = crate::utils::binary::probe_named("ffmpeg");
    print_check("FFmpeg (ASR)", &ffmpeg, true);

    // 5. cookies.txt（B站/YouTube 登录）
    let cookies = probe_cookies();
    print_check("cookies.txt (B站/YouTube)", &cookies, true);

    // 6. assets/ 目录（micro VM 内核）
    let assets = crate::utils::resource::probe();
    print_check("assets/ (VM kernels)", &assets, true);

    // ── 汇总 ───────────────────────────────────
    println!("\n────────────────────────────────────────");
    if required_ok == required_total {
        println!("✅ 必选依赖全部就绪（{}/{}）", required_ok, required_total);
        println!("   运行 `vsb summarize --url <URL>` 开始总结视频");
        Ok(0)
    } else {
        println!(
            "❌ 必选依赖未就绪（{}/{}），请按上方提示修复",
            required_ok, required_total
        );
        if !claude.is_ok() {
            println!("\n修复 Claude API：");
            println!("  设置环境变量 ANTHROPIC_AUTH_TOKEN=<your-token>");
            println!("  或安装 Claude Code CLI: npm i -g @anthropic-ai/claude-code");
        }
        if !ytdlp.is_ok() {
            println!("\n修复 yt-dlp：");
            println!("  pip install yt-dlp");
            println!("  或从 https://github.com/yt-dlp/yt-dlp/releases 下载");
        }
        Ok(1)
    }
}

fn print_check(
    name: &str,
    result: &crate::hypervisor::ProbeResult,
    optional: bool,
) {
    let prefix = if result.is_ok() {
        "  ✅"
    } else if optional {
        "  ⬜"
    } else {
        "  ❌"
    };

    if result.is_ok() {
        println!("{} {:<30} {}", prefix, name, result.backend);
    } else {
        let reason = result.error.unwrap_or("不可用");
        println!("{} {:<30} {}", prefix, name, reason);
    }
}

fn probe_ytdlp() -> crate::hypervisor::ProbeResult {
    use crate::hypervisor::ProbeResult;

    // 从 PATH 查找
    let names = if cfg!(windows) {
        vec!["yt-dlp.exe", "yt-dlp"]
    } else {
        vec!["yt-dlp"]
    };

    for name in &names {
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                if dir.join(name).exists() {
                    return ProbeResult::ok("yt-dlp");
                }
            }
        }
    }

    // 常见安装路径
    let candidates = [
        "D:/software/python/3115/Scripts/yt-dlp.exe",
        "/usr/local/bin/yt-dlp",
        "/usr/bin/yt-dlp",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            return ProbeResult::ok("yt-dlp");
        }
    }

    ProbeResult::err("yt-dlp", "未安装 - 运行: pip install yt-dlp")
}

fn probe_cookies() -> crate::hypervisor::ProbeResult {
    use crate::hypervisor::ProbeResult;

    let candidates = [
        "./cookies.txt",
        "./bilibili_cookies.txt",
        "./youtube_cookies.txt",
    ];
    for c in candidates {
        if std::path::Path::new(c).exists() {
            let msg: &'static str = Box::leak(format!("找到 {c}").into_boxed_str());
            return ProbeResult::ok(msg);
        }
    }
    ProbeResult::err("cookies.txt", "未找到 - 用浏览器插件导出（见 README）")
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
    pub ytdlp: String,
    pub claude: String,
}

impl SystemInfo {
    pub fn gather() -> Self {
        let ytdlp = probe_ytdlp();
        let claude = crate::agents::claude::probe();
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            platform: std::env::consts::OS.to_string(),
            hypervisor: crate::hypervisor::backend_name().to_string(),
            cpu_count: num_cpus(),
            total_memory_mb: total_memory_mb(),
            kernel: std::env::consts::FAMILY.to_string(),
            ytdlp: ytdlp.to_string(),
            claude: claude.to_string(),
        }
    }

    pub fn print_table(&self) {
        println!("version       : {}", self.version);
        println!("platform      : {}", self.platform);
        println!("hypervisor    : {}", self.hypervisor);
        println!("cpu count     : {}", self.cpu_count);
        println!("total memory  : {} MB", self.total_memory_mb);
        println!("kernel family : {}", self.kernel);
        println!("yt-dlp        : {}", self.ytdlp);
        println!("claude        : {}", self.claude);
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn total_memory_mb() -> u64 {
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
