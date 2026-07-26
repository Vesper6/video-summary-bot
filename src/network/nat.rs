//! NAT 代理（占位）。
//!
//! 真实实现可用 lwIP 或 smoltcp 处理 TCP/UDP/ICMP 转发。

use std::net::Ipv4Addr;

/// NAT 代理。
pub struct NatProxy {
    /// 外部网卡名（None 表示自动选择）
    pub external_interface: Option<String>,
}

impl NatProxy {
    /// 创建 NAT 代理。
    pub fn new() -> Self {
        Self {
            external_interface: None,
        }
    }

    /// 转换地址。
    pub fn translate(&self, src: Ipv4Addr) -> Ipv4Addr {
        // 简化：直接返回
        src
    }
}

impl Default for NatProxy {
    fn default() -> Self {
        Self::new()
    }
}
