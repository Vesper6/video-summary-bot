//! VMM 主循环与生命周期管理。

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::Result;
use crate::hypervisor::Hypervisor;
use crate::vmm::{VmConfig, VmState};

/// VMM 实例。
///
/// 每个 Vmm 对应一个 VM。它持有：
/// - [`VmConfig`]（不可变）
/// - [`Hypervisor`] 后端句柄
/// - 当前 [`VmState`]（可变）
pub struct Vmm {
    config: VmConfig,
    hypervisor: Arc<dyn Hypervisor>,
    state: RwLock<VmState>,
}

impl Vmm {
    /// 创建 VMM 实例（不启动 VM）。
    pub fn new(config: VmConfig, hypervisor: Arc<dyn Hypervisor>) -> Self {
        Self {
            config,
            hypervisor,
            state: RwLock::new(VmState::Created),
        }
    }

    /// 获取配置。
    pub fn config(&self) -> &VmConfig {
        &self.config
    }

    /// 获取当前状态。
    pub async fn state(&self) -> VmState {
        *self.state.read().await
    }

    /// 启动 VM。
    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if state.is_running() {
            return Ok(());
        }
        *state = VmState::Starting;
        drop(state);

        tracing::info!(name = %self.config.name, "starting vm");

        // 调用 hypervisor 后端
        self.hypervisor.create_vm(&self.config).await?;
        self.hypervisor.start().await?;

        *self.state.write().await = VmState::Running;
        tracing::info!(name = %self.config.name, "vm started");
        Ok(())
    }

    /// 停止 VM。
    pub async fn stop(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if !state.is_running() {
            return Ok(());
        }
        *state = VmState::Stopping;
        drop(state);

        tracing::info!(name = %self.config.name, "stopping vm");
        self.hypervisor.stop().await?;

        *self.state.write().await = VmState::Stopped;
        Ok(())
    }

    /// 优雅关机（通过 guest agent）。
    pub async fn shutdown(&self) -> Result<()> {
        // TODO: 通过 virtio-serial 通知 guest agent
        self.stop().await
    }

    /// 阻塞运行直到 VM 退出。
    pub async fn run_until_exit(&self) -> Result<i32> {
        if !self.state().await.is_running() {
            self.start().await?;
        }
        self.hypervisor.run().await
    }
}