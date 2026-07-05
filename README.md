# Video Summary Bot

> 🎬 视频总结机器人 — 基于 Rust 的跨平台 VMM（Virtual Machine Monitor），为 AI Agent 提供 micro VM 沙箱

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.96+-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue.svg)]()
[![Hypervisor](https://img.shields.io/badge/hypervisor-WHVP%20%7C%20HVF%20%7C%20KVM-purple.svg)]()
[![LLM](https://img.shields.io/badge/LLM-Claude%20Code-cc785c.svg)]()
[![Status](https://img.shields.io/badge/status-WIP-yellow.svg)]()

Video Summary Bot 是一个**跨平台轻量级 VMM**，参考 [TenBox](https://github.com/78/tenbox) 架构。它通过硬件辅助虚拟化（Windows WHVP / macOS Hypervisor Framework / Linux KVM）创建**微型虚拟机**，在隔离环境中运行 AI Agent 完成视频解析、内容总结、评论弹幕抓取等任务。

> 💡 设计灵感来自 [tenbox.ai](https://tenbox.ai/) 的"tiny, easy, native agent VM"理念。

---

## 📑 目录

- [核心特性](#-核心特性)
- [架构概览](#-架构概览)
- [技术栈](#-技术栈)
- [项目结构](#-项目结构)
- [快速开始](#-快速开始)
- [使用方式](#-使用方式)
- [跨平台 Hypervisor](#-跨平台-hypervisor)
- [VM 生命周期](#-vm-生命周期)
- [配置说明](#-配置说明)
- [参考 tenbox](#-参考-tenbox)
- [开发路线](#-开发路线)
- [贡献指南](#-贡献指南)
- [许可证](#-许可证)

---

## ✨ 核心特性

### 🎥 视频解析与总结（在 VM 内执行）
- **智能内容总结**：由 Claude Code 驱动的语义级视频内容总结
- **关键信息提取**：按时间线呈现视频中的重要信息
- **进度条定位**：配备交互式进度条，可精准跳转到任意时间点
- **分段讲解**：自动划分视频端（片段），分章节进行讲解

### 🕷️ 数据抓取（在 micro VM 内执行）
- **评论区抓取**：抓取视频评论区的评论内容
- **弹幕抓取**：抓取视频中的实时弹幕
- **多档位支持**：
  | 档位 | 评论数量 | 弹幕数量 |
  |------|---------|---------|
  | 轻量 (light)  | 100     | 100     |
  | 标准 (standard) | 1,000   | 1,000   |
  | 全量 (full)   | 全部    | 全部    |

### 🖥️ 跨平台 Micro VM
- **硬件虚拟化**：原生使用平台最优的 hypervisor
- **轻量级**：每个 VM 都是精简的 Linux 客户机，启动快、占用小
- **硬件隔离**：每个 Agent 在独立 VM 中运行
- **可观测**：支持日志、串口、virtio-snd 音频、SPICE 显示

---

## 🏗️ 架构概览

```
┌────────────────────────────────────────────────────────┐
│                       Host OS                          │
│   ┌────────────────────────────────────────────────┐   │
│   │          video-summary-bot (Rust)              │   │
│   │                                                │   │
│   │   ┌──────────────┐   ┌─────────────────────┐   │   │
│   │   │  CLI (clap)  │   │   LLM Proxy         │   │   │
│   │   └──────┬───────┘   └─────────┬───────────┘   │   │
│   │          │                     │               │   │
│   │   ┌──────▼─────────────────────▼────────────┐  │   │
│   │   │      VMM Core (平台无关)                │  │   │
│   │   │      • VM Lifecycle                     │  │   │
│   │   │      • Device Model                     │  │   │
│   │   │      • Boot Loader                      │  │   │
│   │   └──────┬──────────────────────────────────┘  │   │
│   │          │                                     │   │
│   │   ┌──────▼─────────┬─────────────┬──────────┐  │   │
│   │   │ WHVP Backend   │ HVF Backend │ KVM      │  │   │
│   │   │ (Windows)      │ (macOS)     │ Backend  │  │   │
│   │   └────────────────┴─────────────┴──────────┘  │   │
│   └────────────────────────────────────────────────┘   │
│                                                        │
│   ┌────────────────────────────────────────────────┐   │
│   │  Micro VM (Linux guest, 精简 Alpine/Ubuntu)    │   │
│   │  ┌──────────────────────────────────────────┐  │   │
│   │  │  Claude Code Agent                       │  │   │
│   │  │  + Chromium (headless)                   │  │   │
│   │  │  + FFmpeg                                │  │   │
│   │  └──────────────────────────────────────────┘  │   │
│   │  virtio: net | block | fs | gpu | snd | input │   │
│   └────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────┘
```

---

## 🛠️ 技术栈

| 类别 | 技术 |
|------|------|
| **语言** | Rust 1.96+ (Edition 2021) |
| **异步运行时** | Tokio |
| **CLI 框架** | clap |
| **HTTP 服务** | axum |
| **HTTP 客户端** | reqwest |
| **序列化** | serde + serde_json + serde_yaml + toml |
| **日志** | tracing + tracing-subscriber |
| **配置** | config-rs + dotenvy |
| **LLM** | Claude Code CLI / Anthropic API |
| **跨平台 Hypervisor** | WHVP / Hypervisor Framework / KVM |
| **链接器** | rust-lld（Rust 自带） |

### 平台后端

| 平台 | Hypervisor | Rust crate |
|------|-----------|------------|
| **Windows** | WHVP (Windows Hypervisor Platform) | `winhv` / `winhvplatform` |
| **macOS (Apple Silicon + Intel)** | Hypervisor Framework | `hypervisor-rs` |
| **Linux (x86_64 / arm64)** | KVM | `kvm-ioctls` / `kvm-bindings` |

---

## 📂 项目结构

```
Video-Summary-Bot/
├── src/
│   ├── main.rs              # 程序入口（CLI / daemon）
│   ├── lib.rs               # 库根
│   ├── cli/                 # CLI 命令（vm ls/create/start/...）
│   │   ├── mod.rs
│   │   ├── vm.rs
│   │   ├── summary.rs
│   │   └── system.rs
│   ├── vmm/                 # VMM 核心（平台无关）
│   │   ├── mod.rs
│   │   ├── core.rs          # VMM 主循环
│   │   ├── vcpu.rs          # vCPU 抽象
│   │   ├── memory.rs        # 客户机内存管理
│   │   └── loader.rs        # 内核/Initramfs 加载
│   ├── devices/             # virtio 设备模型
│   │   ├── mod.rs
│   │   ├── virtio_block.rs
│   │   ├── virtio_net.rs
│   │   ├── virtio_fs.rs     # virtiofs 共享目录
│   │   ├── virtio_gpu.rs    # virtio-gpu + SPICE
│   │   ├── virtio_snd.rs    # virtio-snd 音频
│   │   ├── virtio_input.rs
│   │   └── serial.rs        # 串口
│   ├── hypervisor/          # 平台 Hypervisor 抽象
│   │   ├── mod.rs           # trait Hypervisor
│   │   ├── windows.rs       # WHVP 实现（Windows）
│   │   ├── macos.rs         # HVF 实现（macOS）
│   │   └── linux.rs         # KVM 实现（Linux）
│   ├── image/               # 磁盘镜像
│   │   ├── mod.rs
│   │   ├── qcow2.rs         # qcow2 格式读写
│   │   └── raw.rs
│   ├── network/             # 网络（NAT、端口转发）
│   │   ├── mod.rs
│   │   ├── dhcp.rs
│   │   └── nat.rs
│   ├── daemon/              # 系统守护进程（参考 tenboxd）
│   │   ├── mod.rs
│   │   └── rpc.rs           # 本地 RPC over Unix socket / Named Pipe
│   ├── agents/              # 视频总结 AGENT
│   │   ├── mod.rs
│   │   ├── planner.rs
│   │   └── claude.rs        # Claude Code 集成
│   ├── crawler/             # 爬虫模块（在 VM 内运行）
│   │   ├── mod.rs
│   │   ├── comments.rs
│   │   └── danmaku.rs
│   ├── summarizer/          # 内容总结
│   │   ├── mod.rs
│   │   ├── asr.rs           # 语音转文字（Whisper）
│   │   └── timeline.rs      # 时间线分段
│   ├── config/              # 配置加载
│   │   └── mod.rs
│   └── utils/
│       └── mod.rs
├── assets/                  # 内置资源
│   ├── kernels/             # Linux 内核镜像
│   ├── initramfs/           # 初始 ramdisk
│   └── rootfs/              # 客户机 rootfs
├── docs/                    # 文档
│   ├── architecture.md      # 架构详解
│   ├── hypervisor.md        # Hypervisor 后端说明
│   └── tenbox-reference.md  # tenbox 架构参考
├── tests/                   # 测试
├── Cargo.toml
├── Cargo.lock
├── .env.example
├── LICENSE
├── README.md
├── CLAUDE.md
└── CONTRIBUTING.md
```

---

## 🚀 快速开始

### 环境要求

| 平台 | 要求 |
|------|------|
| **Windows** | Windows 10/11（已启用 Hyper-V / WHP）；Rust 1.96+；rust-lld（已自带） |
| **macOS** | macOS 11+（Apple Silicon 或 Intel）；Rust 1.96+ |
| **Linux** | 内核 ≥ 5.10；`/dev/kvm` 可用；Rust 1.96+ |

通用前置：
- FFmpeg（视频处理）
- Claude Code CLI（[安装](https://claude.com/claude-code)）
- 可选：`qemu-img`（用于生成客户机磁盘镜像）

### 构建

```bash
git clone https://github.com/Vesper6/video-summary-bot.git
cd video-summary-bot
cargo build --release
```

### 验证环境

```bash
./target/release/video-summary-bot doctor
# 检查 hypervisor 是否可用、依赖是否完整
```

### 首次运行

```bash
# 初始化（下载 Linux 内核、initramfs、基础 rootfs）
./target/release/video-summary-bot init

# 创建一个 VM 配置
./target/release/video-summary-bot vm create --name summary-vm --cpus 2 --memory 2G

# 启动 VM
./target/release/video-summary-bot vm start summary-vm

# 列出 VM
./target/release/video-summary-bot vm ls
```

---

## 📖 使用方式

### CLI 命令

```bash
# 系统诊断
video-summary-bot doctor
video-summary-bot system info

# VM 生命周期
video-summary-bot vm ls                    # 列出所有 VM
video-summary-bot vm create --name X       # 创建 VM
video-summary-bot vm edit --name X         # 编辑 VM 配置
video-summary-bot vm start X               # 启动
video-summary-bot vm stop X                # 停止
video-summary-bot vm reboot X              # 重启
video-summary-bot vm shutdown X            # 优雅关机
video-summary-bot vm rm X                  # 删除
video-summary-bot vm console X             # 连接串口/控制台
video-summary-bot vm logs X                # 查看日志

# 视频总结
video-summary-bot summarize --url "..." --level standard

# 抓取评论/弹幕
video-summary-bot crawl --url "..." --type comments --level heavy

# 守护进程（Linux）
video-summary-bot daemon start
video-summary-bot daemon stop
video-summary-bot daemon status
```

### HTTP API

```bash
# 启动 API 服务
video-summary-bot serve --port 8080

# 调用
curl -X POST http://localhost:8080/api/summarize \
  -H "Content-Type: application/json" \
  -d '{"url": "...", "level": "standard"}'
```

---

## 🖥️ 跨平台 Hypervisor

VMM 的核心是平台抽象层 `Hypervisor` trait：

```rust
// src/hypervisor/mod.rs
pub trait Hypervisor: Send + Sync {
    fn create_vm(&self, config: &VmConfig) -> Result<VmHandle>;
    fn start_vcpu(&self, vm: VmHandle, vcpu_id: u32) -> Result<()>;
    fn handle_exit(&self, vm: VmHandle, exit: VcpuExit) -> Result<VcpuAction>;
    fn map_memory(&self, vm: VmHandle, gpa: u64, hva: u64, size: usize) -> Result<()>;
    // ... 更多接口
}
```

各平台实现：

| 文件 | 后端 | 平台 |
|------|------|------|
| `hypervisor/windows.rs` | WHVP via `WHvCreatePartition` | Windows 10/11 |
| `hypervisor/macos.rs` | Hypervisor Framework via `hv_vm_create` | macOS 11+ |
| `hypervisor/linux.rs` | KVM via `/dev/kvm` ioctls | Linux 5.10+ |

启用对应平台特性（`Cargo.toml` 中的 `[target.'cfg(...)']`）。

---

## 🔄 VM 生命周期

参考 tenboxd 的实现，提供完整 VM 管理：

```
┌────────┐    ┌────────┐    ┌──────────┐    ┌─────────┐
│ Stopped│───▶│ Created│───▶│ Starting │───▶│ Running │
└────────┘    └────────┘    └──────────┘    └────┬────┘
     ▲                                           │
     │           ┌──────────┐                    │
     └───────────│ Stopping │◀───────────────────┘
                 └──────────┘
```

每个状态都有对应的事件回调与持久化机制（`vm.json` 配置文件）。

---

## ⚙️ 配置说明

参见 [.env.example](.env.example)。关键变量：

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Claude API 密钥 |
| `LLM_MODEL` | Claude 模型（默认 `claude-sonnet-5`） |
| `VM_DEFAULT_CPUS` | VM 默认 vCPU 数 |
| `VM_DEFAULT_MEMORY_MB` | VM 默认内存 |
| `VM_ROOTFS_PATH` | 默认 rootfs 路径 |
| `KERNEL_PATH` | Linux 内核路径 |
| `INITRAMFS_PATH` | initramfs 路径 |

---

## 📚 参考 tenbox

本项目架构深受 [TenBox](https://github.com/78/tenbox) 启发，关键设计借鉴：

| TenBox 特性 | 本项目借鉴 |
|-------------|-----------|
| 跨平台 VMM（WHVP/HVF/KVM） | ✅ 同 |
| 共享 C++ 运行时 | ✅ 改为共享 Rust 运行时 |
| `tenboxd` 守护进程 | ✅ `daemon` 模块 |
| virtio 全套设备 | ✅ `devices/` 模块 |
| qcow2 镜像 | ✅ `image/qcow2.rs` |
| SPICE 远程桌面 | 🚧 计划中 |
| LLM proxy | ✅ 内置 |
| `tenbox` CLI | ✅ `cli/` 模块 |

详细对比见 [docs/tenbox-reference.md](docs/tenbox-reference.md)。

---

## 🛣️ 开发路线

- [x] 项目立项 + README + Cargo.toml + 配套文件
- [x] Rust 工具链验证（rust-lld 自带，无需 MSVC Build Tools）
- [ ] Hypervisor trait 设计
- [ ] Windows WHVP 后端
- [ ] Linux KVM 后端
- [ ] macOS HVF 后端
- [ ] vCPU 与客户机内存抽象
- [ ] virtio 设备模型（net/block/fs/gpu/snd/input）
- [ ] qcow2 镜像读写
- [ ] Linux 客户机 rootfs（Alpine 基础）
- [ ] 内核与 initramfs 加载
- [ ] VM 生命周期管理（CLI + daemon）
- [ ] Claude Code 集成
- [ ] 视频 ASR 转写
- [ ] 评论/弹幕爬虫（在 VM 内）
- [ ] 时间线分段与总结
- [ ] HTTP API（axum）
- [ ] SPICE 远程桌面（可选）

---

## 🤝 贡献指南

详见 [CONTRIBUTING.md](CONTRIBUTING.md)。提交前必跑：

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
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

**⚡ 让视频总结变得简单而智能 ⚡**

</div>