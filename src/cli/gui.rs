//! 桌面 GUI 子命令。

use clap::Args;

use crate::config::AppConfig;
use crate::error::Result;
use crate::gui::{self, DesktopOpts};

/// 启动桌面 GUI（原生窗口，非浏览器）。
#[derive(Debug, Args)]
pub struct GuiCmd {
    /// 窗口宽度（像素）
    #[arg(long, default_value = "1280")]
    pub width: u32,

    /// 窗口高度（像素）
    #[arg(long, default_value = "840")]
    pub height: u32,
}

pub async fn run(cmd: GuiCmd, _config: &AppConfig) -> Result<i32> {
    gui::run(DesktopOpts {
        width: cmd.width,
        height: cmd.height,
    })
}