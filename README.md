# Video Summary Bot

> 🎬 视频总结机器人 — 用 yt-dlp + Claude AI 自动分析视频内容，基于 Rust 构建

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue.svg)]()
[![CI](https://github.com/Vesper6/video-summary-bot/actions/workflows/ci.yml/badge.svg)](https://github.com/Vesper6/video-summary-bot/actions/workflows/ci.yml)
[![LLM](https://img.shields.io/badge/LLM-Claude%20Sonnet-cc785c.svg)]()
[![Status](https://img.shields.io/badge/status-demo%20ready-green.svg)]()

Video Summary Bot（`vsb`）是一个命令行工具，通过 **yt-dlp 下载字幕** + **Claude AI 分析**，自动对 B 站、YouTube 等平台的视频生成结构化总结报告。

底层基于 Rust 构建，集成了一个跨平台轻量级 VMM（参考 [TenBox](https://github.com/78/tenbox) 架构），未来支持在硬件隔离的 micro VM 中运行 AI Agent。

---

## 📑 目录

- [当前功能](#-当前功能)
- [快速开始](#-快速开始)
- [使用方式](#-使用方式)
- [健康检查](#-健康检查)
- [架构概览](#-架构概览)
- [技术栈](#-技术栈)
- [项目结构](#-项目结构)
- [配置说明](#-配置说明)
- [开发路线](#-开发路线)
- [贡献指南](#-贡献指南)

---

## ✅ 当前功能

| 功能 | 状态 | 说明 |
|------|------|------|
| `vsb summarize --url` | ✅ 可用 | yt-dlp 获取字幕 + Claude 分析 |
| `vsb summarize --text` | ✅ 可用 | 直接传入文字稿，Claude 总结 |
| `vsb summarize --text-file` | ✅ 可用 | 读取 .vtt/.srt/.txt 字幕文件 |
| `vsb doctor all` | ✅ 可用 | 健康检查（必选/可选依赖） |
| `vsb info` | ✅ 可用 | 系统信息（平台/hypervisor/工具） |
| `vsb vm *` | 🚧 骨架 | VM 生命周期（未接通） |
| `vsb crawl` | 🚧 骨架 | 评论/弹幕抓取（需 micro VM） |
| WHVP 后端 | 🔧 实现 | 核心 API 已实现，待端到端测试 |
| KVM 后端 | 🔧 实现 | 核心 API 已实现，待 Linux 测试 |

---

## 🚀 快速开始

### 1. 环境要求

| 依赖 | 是否必须 | 说明 |
|------|---------|------|
| Rust 1.75+ | ✅ 必须 | `rustup` 安装 |
| yt-dlp | ✅ 必须 | `pip install yt-dlp` |
| Claude API Token | ✅ 必须 | CC Switch 或 `ANTHROPIC_AUTH_TOKEN` 环境变量 |
| FFmpeg | 可选 | 提升字幕转换质量 |
| cookies.txt | 可选 | 访问 B 站 / YouTube 需要登录的内容 |

### 2. 构建

```bash
git clone https://github.com/Vesper6/video-summary-bot.git
cd video-summary-bot

cargo build --release

# 可选：加入 PATH
cp target/release/video-summary-bot ~/.local/bin/vsb   # Linux/macOS
# 或手动添加 target/release/ 到 PATH（Windows）
```

### 3. 配置 Claude API

CC Switch 用户无需额外配置，环境变量已自动注入。

手动配置：

```bash
# .env 文件（复制 .env.example）
cp .env.example .env
# 编辑 .env，填写：
ANTHROPIC_AUTH_TOKEN=sk-ant-xxxxx
# 或
ANTHROPIC_API_KEY=sk-ant-xxxxx

# 如果使用代理 API（CC Switch）
ANTHROPIC_BASE_URL=https://api.your-proxy.com
ANTHROPIC_MODEL=claude-sonnet-4-6
```

### 4. 验证环境

```bash
vsb doctor all
```

输出示例（就绪状态）：

```
🩺 Video Summary Bot - 健康检查
────────────────────────────────────────

【必选】
  ✅ Claude API                     Claude API ready (model: claude-sonnet-4-6)
  ✅ yt-dlp                         yt-dlp

【可选】
  ⬜ Hypervisor (VM)                feature 'whvp' not enabled
  ✅ FFmpeg (ASR)                   ffmpeg
  ⬜ cookies.txt (B站/YouTube)       未找到 - 用浏览器插件导出（见下方）
  ⬜ assets/ (VM kernels)           assets/ does not exist

────────────────────────────────────────
✅ 必选依赖全部就绪（2/2）
   运行 `vsb summarize --url <URL>` 开始总结视频
```

---

## 📖 使用方式

### 模式一：URL 自动获取字幕（推荐）

```bash
# B 站视频（需要 cookies.txt，见下方）
vsb summarize --url "https://www.bilibili.com/video/BV1xxx" --cookies cookies.txt

# YouTube 视频（部分公开视频无需 cookies）
vsb summarize --url "https://www.youtube.com/watch?v=xxxxx"

# 控制总结深度
vsb summarize --url "..." --level light     # 简要（3-5 要点）
vsb summarize --url "..." --level standard  # 标准（默认）
vsb summarize --url "..." --level full      # 深度分析

# 保存为 Markdown 文件
vsb summarize --url "..." --output ./output/

# 跳过 yt-dlp，直接让 Claude 根据知识分析
vsb summarize --url "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --no-subtitle
```

### 模式二：直接传入文字稿

```bash
# 内联文字
vsb summarize --text "本视频讲解了 Rust 异步编程原理..."

# 从字幕文件读取（.vtt / .srt / .txt）
vsb summarize --text-file subtitle.vtt
vsb summarize --text-file transcript.txt --level full --output ./output/
```

### 模式三：从 yt-dlp 手动下载字幕后分析

```bash
# 先用 yt-dlp 下载字幕
yt-dlp --write-auto-sub --sub-lang zh-Hans --skip-download \
       --cookies cookies.txt \
       "https://www.bilibili.com/video/BV1xxx"

# 然后分析
vsb summarize --text-file "视频标题.zh-Hans.vtt" --level standard
```

---

## 🍪 B 站 / YouTube cookies 配置

B 站和 YouTube 需要登录才能获取完整字幕。导出 cookies：

1. 安装浏览器插件 **[Get cookies.txt LOCALLY](https://chrome.google.com/webstore/detail/get-cookiestxt-locally/cclelndahbckbenkjhflpdbgdldlbecc)**（Chrome/Edge）
2. 登录 bilibili.com（或 youtube.com）
3. 点击插件图标 → **Export**，保存为 `cookies.txt`
4. 将文件放到项目根目录，或通过 `--cookies` 参数指定路径

```bash
# 项目根目录放 cookies.txt（自动检测）
vsb summarize --url "https://www.bilibili.com/video/BV1xxx"

# 或指定路径
vsb summarize --url "..." --cookies /path/to/cookies.txt
```

---

## 🩺 健康检查

```bash
vsb doctor all   # 完整健康检查
vsb info         # 系统信息
```

`vsb info` 输出：

```
version       : 0.1.0
platform      : windows
hypervisor    : WHVP
cpu count     : 32
total memory  : 0 MB
kernel family : windows
yt-dlp        : ✓ yt-dlp (可用)
claude        : ✓ Claude API ready (model: claude-sonnet-4-6) (可用)
```

---

## 🏗️ 架构概览

```
┌─────────────────────────────────────────────────────────┐
│                     Host OS                              │
│                                                          │
│   vsb (Rust CLI)                                         │
│   ┌──────────┐   ┌────────────┐   ┌──────────────────┐  │
│   │   CLI    │   │  Claude    │   │  SubtitleFetcher │  │
│   │  (clap)  │──▶│  Client   │   │  (yt-dlp wrapper)│  │
│   └──────────┘   └─────┬──────┘   └────────┬─────────┘  │
│                        │                   │             │
│                  Anthropic API         yt-dlp process    │
│                  (CC Switch)           (字幕下载)         │
│                                                          │
│   ┌──────────────────────────────────────────────────┐   │
│   │  VMM Core（骨架，待接通）                         │   │
│   │  WHVP(Windows) / KVM(Linux) / HVF(macOS)        │   │
│   └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

**当前实现的数据流：**

```
视频 URL
  │
  ├── yt-dlp → 字幕(.vtt) + 元数据(标题/时长/简介)
  │                    │
  │              parse_vtt() 清理时间戳
  │                    │
  └──────────────▶ Claude API
                       │
                  结构化总结报告
                       │
                  stdout + 可选 .md 文件
```

---

## 🛠️ 技术栈

| 类别 | 技术 |
|------|------|
| 语言 | Rust 1.75+（Edition 2021） |
| 异步运行时 | Tokio（rt-multi-thread + process） |
| CLI 框架 | clap 4 |
| HTTP 客户端 | reqwest（rustls-tls） |
| LLM | Anthropic Messages API（兼容 CC Switch） |
| 字幕下载 | yt-dlp（subprocess） |
| 序列化 | serde + serde_json |
| 日志 | tracing + tracing-subscriber |
| 临时文件 | tempfile |

### 平台 Hypervisor 后端

| 平台 | 后端 | 状态 |
|------|------|------|
| Windows 10/11 | WHVP（Windows Hypervisor Platform） | 🔧 核心实现 |
| macOS 11+ | Hypervisor Framework（HVF） | 🚧 骨架 |
| Linux 5.10+ | KVM（via kvm-ioctls） | 🔧 核心实现 |

---

## 📂 项目结构

```
video-summary-bot/
├── src/
│   ├── main.rs              # 程序入口
│   ├── lib.rs               # 库根（13 个模块）
│   ├── error.rs             # 统一错误类型
│   ├── cli/                 # CLI 命令
│   │   ├── mod.rs           # 顶层路由
│   │   ├── summary.rs       # summarize / crawl ✅
│   │   ├── vm.rs            # vm ls/create/start/... 🚧
│   │   └── system.rs        # doctor / info ✅
│   ├── agents/
│   │   ├── claude.rs        # Claude API 客户端 ✅
│   │   └── planner.rs       # 任务规划 🚧
│   ├── crawler/
│   │   ├── subtitle.rs      # yt-dlp 字幕下载 ✅
│   │   ├── comments.rs      # 评论抓取 🚧
│   │   └── danmaku.rs       # 弹幕抓取 🚧
│   ├── hypervisor/
│   │   ├── mod.rs           # Hypervisor trait
│   │   ├── windows.rs       # WHVP 实现 🔧
│   │   ├── linux.rs         # KVM 实现 🔧
│   │   └── macos.rs         # HVF 骨架 🚧
│   ├── vmm/                 # VMM 核心 🚧
│   ├── devices/             # virtio 设备 🚧
│   ├── image/               # 磁盘镜像 🚧
│   ├── network/             # NAT/DHCP 🚧
│   ├── summarizer/          # ASR + 时间线 🚧
│   ├── config/              # 配置加载
│   └── utils/               # 工具函数
├── .github/
│   ├── workflows/
│   │   ├── ci.yml           # 3 平台 CI（lint/build/test）
│   │   └── release.yml      # 跨平台 release 构建
│   ├── dependabot.yml
│   └── ISSUE_TEMPLATE/      # Bug / Feature / Question 模板
├── docs/
│   └── tenbox-reference.md
├── Cargo.toml
├── .env.example
└── CONTRIBUTING.md
```

图例：✅ 已实现可用 | 🔧 核心已实现待测试 | 🚧 骨架/未实现

---

## ⚙️ 配置说明

复制 `.env.example` 并按需修改：

```bash
cp .env.example .env
```

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `ANTHROPIC_AUTH_TOKEN` | Claude API Token（CC Switch 用这个） | — |
| `ANTHROPIC_API_KEY` | 标准 Anthropic API Key | — |
| `ANTHROPIC_BASE_URL` | API 代理地址 | `https://api.anthropic.com` |
| `ANTHROPIC_MODEL` | 使用的模型 | `claude-sonnet-4-6` |
| `LOG_FORMAT` | 日志格式（`text` / `json`） | `text` |
| `RUST_LOG` | 日志级别（`info` / `debug` / `warn`） | `warn` |
| `VM_DEFAULT_CPUS` | VM 默认 vCPU 数（未来使用） | `2` |
| `VM_DEFAULT_MEMORY_MB` | VM 默认内存（未来使用） | `2048` |

---

## 🛣️ 开发路线

### 已完成
- [x] 项目基础（Cargo.toml、CI/CD、Issue 模板、Dependabot）
- [x] 全模块骨架（45 个 .rs 文件）
- [x] Claude API 客户端（兼容 CC Switch 代理）
- [x] yt-dlp 字幕下载器（VTT/SRT 解析）
- [x] `summarize` 命令（3 种输入模式：URL/文字稿/文件）
- [x] `doctor` 健康检查（必选/可选分级）
- [x] Windows WHVP 后端骨架（partition 创建、内存映射、vCPU run loop）
- [x] Linux KVM 后端骨架（mmap、KVM_CREATE_VM、vCPU 寄存器、run loop）
- [x] GitHub Actions CI（3 平台 lint/build/test）
- [x] Release 工作流（5 架构跨平台构建）

### 进行中 / 近期
- [ ] B 站完整字幕获取（需 cookies，端到端验证）
- [ ] `summarize` 输出 HTML 格式
- [ ] HTTP API（`vsb serve`，axum 路由）

### 中期
- [ ] Windows WHVP 端到端 VM 启动验证
- [ ] Linux KVM 端到端 VM 启动验证
- [ ] Alpine Linux rootfs + 内核准备脚本
- [ ] virtio-net / virtio-block 设备实现
- [ ] VM 生命周期 CLI 接通（`vsb vm start/stop`）

### 长期
- [ ] macOS HVF 后端实现
- [ ] virtio-fs 共享目录
- [ ] ASR 转写（Whisper 集成）
- [ ] 评论/弹幕爬虫（在 micro VM 内运行）
- [ ] SPICE 远程桌面（可选）
- [ ] 守护进程（systemd / Windows Service）

---

## 🤝 贡献指南

详见 [CONTRIBUTING.md](CONTRIBUTING.md)。提交前必跑：

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build
```

---

## 📄 许可证

本项目基于 [MIT License](LICENSE) 开源。

---

## 📮 联系方式

- **问题反馈**：[GitHub Issues](https://github.com/Vesper6/video-summary-bot/issues)
- **讨论区**：[GitHub Discussions](https://github.com/Vesper6/video-summary-bot/discussions)

---

<div align="center">

**⚡ yt-dlp + Claude = 视频秒懂 ⚡**

</div>
