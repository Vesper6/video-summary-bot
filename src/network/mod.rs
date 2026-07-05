//! 客户机网络栈。
//!
//! 提供：
//! - [`nat`] — NAT 代理（TCP / UDP）
//! - [`dhcp`] — DHCP 服务器（自动分配客户机 IP）
//! - 端口转发（host-forward / guest-forward）

pub mod dhcp;
pub mod nat;

/// 网络模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    /// NAT（默认）
    Nat,
    /// 仅主机模式（无外网）
    HostOnly,
    /// 桥接模式（接入宿主网络）
    Bridged,
}

impl Default for NetworkMode {
    fn default() -> Self {
        Self::Nat
    }
}

/// 网络配置。
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    /// 客户机默认网关（NAT 模式下是 VMM 的虚拟网关）
    pub guest_ip: std::net::Ipv4Addr,
    pub gateway_ip: std::net::Ipv4Addr,
    pub netmask: std::net::Ipv4Addr,
    /// 端口转发规则
    pub port_forwards: Vec<crate::vmm::PortForward>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::Nat,
            guest_ip: "10.0.0.2".parse().unwrap(),
            gateway_ip: "10.0.0.1".parse().unwrap(),
            netmask: "255.255.255.0".parse().unwrap(),
            port_forwards: Vec::new(),
        }
    }
}