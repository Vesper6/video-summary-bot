//! 图形界面模块（桌面原生窗口，参考 TenBox Manager）。

#[cfg(feature = "gui")]
mod desktop;

use crate::error::{Error, Result};

/// 桌面 GUI 选项。
#[derive(Debug, Clone)]
pub struct DesktopOpts {
    pub width: u32,
    pub height: u32,
}

impl Default for DesktopOpts {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 840,
        }
    }
}

/// 启动图形界面（需 `--features gui` 编译）。
pub fn run(opts: DesktopOpts) -> Result<i32> {
    #[cfg(feature = "gui")]
    {
        return desktop::run(opts);
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = opts;
        Err(Error::Cli(
            "桌面 GUI 未编译进此构建。请运行: cargo build --features gui".into(),
        ))
    }
}