//! `video_summary_bot` 是 Video Summary Bot 的库根。
//!
//! ## 模块组织
//!
//! - [`cli`] — 命令行接口（基于 clap）
//! - [`config`] — 配置加载
//! - [`vmm`] — VMM 核心（平台无关）
//! - [`hypervisor`] — 跨平台 hypervisor 抽象
//! - [`devices`] — virtio 设备模型
//! - [`image`] — 磁盘镜像（qcow2 / raw）
//! - [`network`] — 网络（NAT / DHCP / LLM 代理）
//! - [`ipc`] — 宿主 ↔ Guest IPC（VirtIO Serial / 命名管道）
//! - [`daemon`] — 守护进程
//! - [`agents`] — AI Agent 集成（Claude Code）
//! - [`crawler`] — 视频数据抓取（在 VM 内运行）
//! - [`summarizer`] — 视频内容总结
//! - [`utils`] — 工具函数

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod agents;
pub mod api;
pub mod cli;
pub mod config;
pub mod crawler;
pub mod daemon;
pub mod devices;
pub mod error;
pub mod gui;
pub mod hypervisor;
pub mod image;
pub mod ipc;
pub mod network;
pub mod summarizer;
pub mod utils;
pub mod vmm;