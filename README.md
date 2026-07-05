# Video Summary Bot

> 🎬 视频总结机器人 — 基于 Rust + KVM + Claude Code 的智能视频内容总结工具

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Linux-lightgrey.svg)]()
[![Sandbox](https://img.shields.io/badge/sandbox-KVM%2FQEMU-blueviolet.svg)]()
[![LLM](https://img.shields.io/badge/LLM-Claude%20Code-cc785c.svg)]()
[![Status](https://img.shields.io/badge/status-WIP-yellow.svg)]()

Video Summary Bot 是一个基于 **AGENT + 工作流 + KVM 沙箱** 的视频内容智能总结工具。它通过 **Claude Code** 驱动智能决策，自动解析视频内容，按时间线划分视频片段，生成结构化摘要，并支持抓取评论区与弹幕数据，帮助用户快速了解视频核心内容。

---

## 📑 目录

- [核心功能](#-核心功能)
- [技术栈](#-技术栈)
- [项目架构](#-项目架构)
- [快速开始](#-快速开始)
- [使用方式](#-使用方式)
- [配置说明](#-配置说明)
- [KVM 沙箱](#-kvm-沙箱)
- [Claude Code 集成](#-claude-code-集成)
- [开发路线](#-开发路线)
- [贡献指南](#-贡献指南)
- [许可证](#-许可证)

---

## ✨ 核心功能

### 🎥 视频解析与总结
- **智能内容总结**：由 Claude Code 驱动的语义级视频内容总结
- **关键信息提取**：按时间线呈现视频中的重要信息
- **进度条定位**：配备交互式进度条，可精准跳转到任意时间点
- **分段讲解**：自动划分视频端（片段），分章节进行讲解

### 🕷️ 数据抓取（在 KVM 虚拟机内执行）
- **评论区抓取**：抓取视频评论区的评论内容
- **弹幕抓取**：抓取视频中的实时弹幕
- **多档位支持**：
  | 档位 | 评论数量 | 弹幕数量 |
  |------|---------|---------|
  | 轻量 (light)  | 100     | 100     |
  | 标准 (standard) | 1,000   | 1,000   |
  | 全量 (full)   | 全部    | 全部    |
- **可配置量级**：按需选择爬取规模，平衡速度与覆盖度

### 🤖 智能化能力
- **AGENT 智能调度**：内置 AGENT 协调多步骤任务
- **工作流编排**：基于 Tokio 异步运行时的工作流引擎
- **真实机器模拟**：通过 KVM 启动完整 Linux 虚拟机，模拟真实用户操作
- **硬件级隔离**：基于 KVM 硬件辅助虚拟化，与宿主完全隔离

---

## 🛠️ 技术栈

| 类别 | 技术 |
|------|------|
| **语言** | Rust 1.75+ (Edition 2021) |
| **异步运行时** | Tokio |
| **HTTP 客户端** | reqwest |
| **浏览器自动化** | headless_chrome / chromiumoxide |
| **LLM 集成** | Claude Code CLI / Anthropic SDK |
| **沙箱虚拟化** | KVM + QEMU |
| **VM 管控** | libvirt-rs |
| **配置管理** | config-rs + dotenvy |
| **日志** | tracing + tracing-subscriber |
| **序列化** | serde + serde_json / serde_yaml |
| **CLI 框架** | clap |
| **TUI** | ratatui + crossterm |

---

## 🏗️ 项目架构

```
Video-Summary-Bot/
├── 📂 src/
│   ├── 📄 main.rs              # 程序入口（CLI / API 启动）
│   ├── 📄 lib.rs               # 库根
│   ├── 📂 agents/              # AGENT 模块（基于 Claude Code）
│   │   ├── mod.rs
│   │   ├── planner.rs          # 任务规划
│   │   └── executor.rs         # 任务执行
│   ├── 📂 workflows/           # 工作流引擎
│   │   ├── mod.rs
│   │   ├── pipeline.rs         # 流水线编排
│   │   └── steps/              # 各步骤实现
│   ├── 📂 sandbox/             # KVM 沙箱管理
│   │   ├── mod.rs
│   │   ├── kvm.rs              # KVM hypervisor 接口
│   │   ├── qemu.rs             # QEMU 进程管理
│   │   └── libvirt.rs          # libvirt 封装
│   ├── 📂 crawler/             # 爬虫模块
│   │   ├── mod.rs
│   │   ├── comments.rs         # 评论抓取
│   │   └── danmaku.rs          # 弹幕抓取
│   ├── 📂 summarizer/          # 视频内容总结
│   │   ├── mod.rs
│   │   ├── asr.rs              # 语音转文字
│   │   └── claude.rs           # Claude Code 调用
│   ├── 📂 timeline/            # 时间线分段
│   │   ├── mod.rs
│   │   └── segment.rs          # 章节划分
│   ├── 📂 ui/                  # 用户界面
│   │   ├── mod.rs
│   │   ├── cli.rs              # CLI 模式
│   │   └── tui.rs              # TUI 模式（进度条等）
│   ├── 📂 api/                 # HTTP API 服务
│   │   ├── mod.rs
│   │   └── routes.rs
│   ├── 📂 config/              # 配置加载
│   │   └── mod.rs
│   └── 📂 utils/               # 工具函数
│       └── mod.rs
├── 📂 tests/                   # 集成测试
│   ├── integration_test.rs
│   └── sandbox_test.rs
├── 📂 vm-images/               # KVM 使用的磁盘镜像（gitignore）
├── 📂 docs/                    # 补充文档
├── 📄 Cargo.toml               # Rust 项目清单
├── 📄 Cargo.lock
├── 📄 .env.example             # 环境变量示例
├── 📄 README.md
├── 📄 LICENSE
└── 📄 CONTRIBUTING.md
```

---

## 🚀 快速开始

### 环境要求

| 组件 | 要求 |
|------|------|
| **操作系统** | Linux（Ubuntu 22.04+ / Debian 12+ / Arch） |
| **Rust** | 1.75 或更高版本 |
| **KVM** | 内核启用 KVM（`/dev/kvm` 可访问） |
| **QEMU** | 7.0+ |
| **libvirt** | 8.0+（可选，用于高级 VM 管理） |
| **Claude Code** | 已安装并配置（[安装指南](https://claude.com/claude-code)） |
| **FFmpeg** | 视频处理 |

### 检查 KVM 是否可用

```bash
# 检查 KVM 是否在内核中启用
ls -la /dev/kvm

# 检查 CPU 虚拟化支持
egrep -c '(vmx|svm)' /proc/cpuinfo
# 输出应大于 0
```

### 安装步骤

```bash
# 1. 克隆仓库
git clone https://github.com/Vesper6/video-summary-bot.git
cd video-summary-bot

# 2. 安装系统依赖（Ubuntu/Debian）
sudo apt update
sudo apt install -y qemu-kvm libvirt-daemon-system libvirt-clients \
                    qemu-utils ovmf ffmpeg

# 3. 安装 Rust（如未安装）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 4. 安装 Claude Code
npm install -g @anthropic-ai/claude-code
# 或参考官方安装方式

# 5. 配置环境变量
cp .env.example .env
# 编辑 .env，填入 ANTHROPIC_API_KEY 等

# 6. 编译项目（release 模式）
cargo build --release

# 7. 启动 KVM 沙箱（首次需要拉取/构建 VM 镜像）
./target/release/video-summary-bot sandbox init

# 8. 运行程序
./target/release/video-summary-bot summarize --url "https://example.com/video/123"
```

---

## 📖 使用方式

### CLI 模式

```bash
# 查看帮助
./target/release/video-summary-bot --help

# 总结单个视频
./target/release/video-summary-bot summarize \
    --url "https://example.com/video/123" \
    --level standard

# 指定抓取档位（light / standard / full）
./target/release/video-summary-bot summarize \
    --url "..." \
    --level full

# 仅抓取评论
./target/release/video-summary-bot crawl \
    --url "..." \
    --type comments \
    --level heavy

# 仅抓取弹幕
./target/release/video-summary-bot crawl \
    --url "..." \
    --type danmaku \
    --level full

# 启动 TUI 模式（带进度条）
./target/release/video-summary-bot tui --url "..."

# 沙箱管理
./target/release/video-summary-bot sandbox init     # 初始化 VM 镜像
./target/release/video-summary-bot sandbox status   # 查看沙箱状态
./target/release/video-summary-bot sandbox destroy  # 销毁沙箱
```

### API 模式

```bash
# 启动 HTTP API 服务
./target/release/video-summary-bot serve --port 8080

# 调用总结接口
curl -X POST http://localhost:8080/api/summarize \
    -H "Content-Type: application/json" \
    -d '{
        "url": "https://example.com/video/123",
        "level": "standard",
        "language": "zh-CN"
    }'

# 调用抓取接口
curl -X POST http://localhost:8080/api/crawl \
    -H "Content-Type: application/json" \
    -d '{
        "url": "...",
        "type": "comments",
        "level": "standard"
    }'
```

> 📌 **注**：CLI 与 API 的具体参数待补充。

---

## ⚙️ 配置说明

主要通过 `.env` 文件与 `config/` 目录进行配置：

| 配置项 | 说明 | 默认值 |
|--------|------|--------|
| `ANTHROPIC_API_KEY` | Claude API 密钥 | 必填 |
| `CLAUDE_CODE_PATH` | Claude Code 可执行路径 | `/usr/local/bin/claude` |
| `LLM_MODEL` | 使用的 Claude 模型 | `claude-sonnet-5` |
| `SANDBOX_BACKEND` | 沙箱后端（kvm） | `kvm` |
| `VM_IMAGE_PATH` | KVM 磁盘镜像路径 | `./vm-images/base.qcow2` |
| `VM_MEMORY_MB` | VM 内存大小 | `4096` |
| `VM_VCPUS` | VM vCPU 数量 | `2` |
| `CRAWL_LEVEL` | 默认抓取档位 | `standard` |
| `TIMELINE_PRECISION` | 时间线分段精度（秒） | `30` |
| `SUMMARY_LANGUAGE` | 总结输出语言 | `zh-CN` |
| `RUST_LOG` | 日志级别 | `info` |

详细配置请参见 `.env.example`。

---

## 🔒 KVM 沙箱

为确保安全性与真实性，所有爬虫与浏览器自动化操作均在 **KVM 虚拟机** 中执行：

- ✅ **硬件级隔离**：基于 KVM 硬件辅助虚拟化，与宿主系统完全隔离
- ✅ **完整 OS 环境**：运行完整 Linux 操作系统（Ubuntu / Debian / Alpine）
- ✅ **可重置**：每次任务可使用快照恢复，秒级重建
- ✅ **可观测**：支持日志、截图、录屏、网络抓包
- ✅ **资源可控**：可限制 vCPU / 内存 / 磁盘 / 网络带宽

### 沙箱拓扑

```
┌──────────────────────────────────────────┐
│            Host (Linux + KVM)            │
│  ┌────────────────────────────────────┐  │
│  │   Video Summary Bot (Rust)        │  │
│  │   ┌────────────────────────────┐   │  │
│  │   │  Claude Code Agent         │   │  │
│  │   │  Workflow Engine           │   │  │
│  │   └────────────┬───────────────┘   │  │
│  │                │ virtio-serial     │  │
│  │                ▼                   │  │
│  │   ┌────────────────────────────┐   │  │
│  │   │  KVM Guest VM (Linux)      │   │  │
│  │   │  ┌──────────────────────┐  │   │  │
│  │   │  │  Chromium Headless   │  │   │  │
│  │   │  │  Crawler (comments / │  │   │  │
│  │   │  │  danmaku)            │  │   │  │
│  │   │  └──────────────────────┘  │   │  │
│  │   └────────────────────────────┘   │  │
│  └────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

### VM 镜像管理

```bash
# 初始化基础 VM 镜像（首次需要）
./target/release/video-summary-bot sandbox init

# 自定义镜像构建（可选）
./target/release/video-summary-bot sandbox build --base ubuntu-22.04
```

---

## 🤖 Claude Code 集成

本项目以 **Claude Code** 作为 AGENT 决策核心：

- 通过 `claude` CLI / Anthropic SDK 调度模型
- Claude Code 负责：
  - 视频内容语义分析
  - 时间线分段决策
  - 评论/弹幕价值筛选
  - 结构化摘要生成
  - 异常处理与重试策略

### 调用示例（伪代码）

```rust
// src/agents/planner.rs
use claude_code_sdk::{Claude, Message};

pub async fn plan_summary(video_url: &str) -> Result<Plan, Error> {
    let claude = Claude::new(env::var("ANTHROPIC_API_KEY")?);
    let response = claude
        .message(Message::user(format!(
            "分析视频 {} 的内容，规划总结步骤", video_url
        )))
        .model(env::var("LLM_MODEL").unwrap_or_else(|_| "claude-sonnet-5".into()))
        .send()
        .await?;
    Ok(plan_from_response(response))
}
```

---

## 🛣️ 开发路线

- [x] 项目立项与文档初始化
- [ ] Rust 项目骨架与依赖选型
- [ ] KVM/QEMU 沙箱基础能力
- [ ] libvirt 集成与 VM 生命周期管理
- [ ] 基础爬虫模块（评论 + 弹幕）
- [ ] 视频 ASR 转写（FFmpeg + Whisper）
- [ ] Claude Code SDK 集成
- [ ] AGENT 调度框架
- [ ] 工作流引擎（Tokio pipeline）
- [ ] 时间线分段与 TUI 进度条
- [ ] 多档位抓取策略
- [ ] HTTP API 服务（axum）
- [ ] 性能优化与稳定性测试
- [ ] 打包发布（cargo-dist）

---

## 🤝 贡献指南

欢迎贡献代码、提 Issue 或完善文档！

1. Fork 本仓库
2. 创建特性分支：`git checkout -b feature/your-feature`
3. 提交变更：`git commit -m "feat: add your feature"`
4. 推送分支：`git push origin feature/your-feature`
5. 提交 Pull Request

提交前请确保：

```bash
# 格式化代码
cargo fmt

# Lint 检查
cargo clippy --all-targets -- -D warnings

# 运行所有测试
cargo test --all

# 构建 release
cargo build --release
```

---

## 📄 许可证

本项目基于 [MIT License](LICENSE) 开源。

---

## 📮 联系方式

- **项目维护者**：[Your Name](mailto:your.email@example.com)
- **问题反馈**：[GitHub Issues](https://github.com/Vesper6/video-summary-bot/issues)
- **讨论区**：[GitHub Discussions](https://github.com/Vesper6/video-summary-bot/discussions)

---

<div align="center">

**⚡ 让视频总结变得简单而智能 ⚡**

</div>