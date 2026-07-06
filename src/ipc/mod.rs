//! IPC 模块：宿主进程间通信 + Guest Agent 协议。
//!
//! 参考 tenbox src/ipc/：
//! - Windows：命名管道（\\.\pipe\vsb-<vm-name>）
//! - Linux/macOS：Unix Domain Socket（/tmp/vsb-<vm-name>.sock）

pub mod guest_agent;

use crate::error::{Error, Result};

/// IPC 端点路径（平台自适应）。
pub fn ipc_path(vm_name: &str) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::path::PathBuf::from(format!(r"\\.\pipe\vsb-{vm_name}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::path::PathBuf::from(format!("/tmp/vsb-{vm_name}.sock"))
    }
}

/// VirtIO Serial 宿主侧路径（VMM 创建，Guest Agent 连接用）。
pub fn serial_path(vm_name: &str) -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::path::PathBuf::from(format!(r"\\.\pipe\vsb-serial-{vm_name}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::path::PathBuf::from(format!("/tmp/vsb-serial-{vm_name}.sock"))
    }
}
