//! `video_summary_bot` 是 Video Summary Bot 的库根。
//!
//! 一个 yt-dlp + Claude 视频总结工具：用 yt-dlp 下载字幕/元数据，
//! 交给 Claude AI 生成结构化的视频总结报告。
//!
//! ## 模块组织
//!
//! - [`cli`] — 命令行接口（基于 clap）
//! - [`config`] — 配置加载
//! - [`daemon`] — HTTP API 服务
//! - [`api`] — HTTP API + Web GUI
//! - [`agents`] — AI Agent 集成（Claude Code）
//! - [`crawler`] — 视频数据抓取（yt-dlp 字幕）
//! - [`gui`] — 桌面 GUI
//! - [`utils`] — 工具函数

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod agents;
pub mod api;
pub mod cli;
pub mod config;
pub mod crawler;
pub mod daemon;
pub mod error;
pub mod gui;
pub mod utils;
