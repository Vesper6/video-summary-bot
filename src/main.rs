//! video-summary-bot: 跨平台 VMM，为 AI Agent 提供 micro VM 沙箱
//!
//! 入口文件：解析命令行参数并分发到对应的子命令处理器。

use video_summary_bot::cli::Cli;
use video_summary_bot::config::AppConfig;

#[tokio::main]
async fn main() {
    // 加载 .env 文件（如果存在）
    let _ = dotenvy::dotenv();

    // 初始化日志
    video_summary_bot::utils::logging::init();

    // 加载配置
    let config = match AppConfig::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("failed to load config: {e}");
            std::process::exit(1);
        }
    };

    // 解析 CLI
    let cli = Cli::parse_args();

    // 分发执行
    let exit_code = match video_summary_bot::cli::run(cli, &config).await {
        Ok(code) => code,
        Err(e) => {
            tracing::error!("command failed: {e:#}");
            1
        }
    };

    std::process::exit(exit_code);
}