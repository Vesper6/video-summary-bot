//! VMM 核心模块（平台无关）。
//!
//! ## 模块组成
//!
//! - [`core`] — VMM 主循环与生命周期
//! - [`vcpu`] — vCPU 抽象
//! - [`memory`] — 客户机物理内存管理
//! - [`loader`] — 内核 / Initramfs 加载器（占位）

pub mod core;
pub mod loader;
pub mod memory;
pub mod vcpu;

use serde::{Deserialize, Serialize};

/// VM 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmState {
    /// 已创建但未启动
    Created,
    /// 正在启动
    Starting,
    /// 正在运行
    Running,
    /// 正在停止
    Stopping,
    /// 已停止
    Stopped,
    /// 崩溃
    Crashed,
}

impl VmState {
    pub fn is_running(&self) -> bool {
        matches!(self, VmState::Running)
    }
}

/// VM 配置（持久化为 `vm.json`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmConfig {
    /// VM 唯一名称
    pub name: String,

    /// vCPU 数量
    pub cpus: u8,

    /// 内存大小（MB）
    pub memory_mb: u32,

    /// 磁盘大小（GB）
    pub disk_gb: u32,

    /// Linux 内核路径
    pub kernel: Option<std::path::PathBuf>,

    /// Initramfs 路径
    pub initramfs: Option<std::path::PathBuf>,

    /// Rootfs 路径
    pub rootfs: Option<std::path::PathBuf>,

    /// 共享目录（virtiofs）
    #[serde(default)]
    pub shared_dirs: Vec<SharedDir>,

    /// 端口转发规则
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,

    /// 是否启用音频
    #[serde(default)]
    pub audio: bool,
}

impl VmConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            cpus: 2,
            memory_mb: 2048,
            disk_gb: 10,
            kernel: None,
            initramfs: None,
            rootfs: None,
            shared_dirs: Vec::new(),
            port_forwards: Vec::new(),
            audio: true,
        }
    }
}

/// 共享目录（virtiofs）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedDir {
    /// 标签（在 VM 内可见为挂载点）
    pub tag: String,

    /// 宿主机路径
    pub host_path: std::path::PathBuf,

    /// 是否只读
    #[serde(default)]
    pub read_only: bool,
}

/// 端口转发规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForward {
    /// 协议（"tcp" / "udp"）
    pub protocol: String,

    /// 宿主机端口
    pub host_port: u16,

    /// 客户机端口
    pub guest_port: u16,
}