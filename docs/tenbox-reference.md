# tenbox 架构参考

> 本文档整理自 [78/tenbox](https://github.com/78/tenbox) 项目，作为 video-summary-bot 架构设计的参考。

## 📌 tenbox 是什么

**TenBox** = "Tiny, Easy, Native Agent VM"

- **定位**：让 AI Agent 在个人电脑上安全运行（每个 Agent 在隔离 VM 中）
- **跨平台**：Windows / macOS / Linux
- **核心**：跨平台 VMM（Virtual Machine Monitor）
- **语言**：C++（共享运行时）
- **仓库**：https://github.com/78/tenbox
- **官网**：https://tenbox.ai
- **License**：GPL v3
- **Star**：249（截至 2026-07）

---

## 🏗️ tenbox 顶层架构

```
┌──────────────────────────────────────────────────────┐
│                       Host OS                        │
│                                                      │
│   ┌──────────────────────────────────────────────┐   │
│   │            tenbox (VMM 核心)                │   │
│   │                                              │   │
│   │  ┌────────────┐  ┌────────────┐  ┌────────┐  │   │
│   │  │ WHVP       │  │ HVF        │  │ KVM    │  │   │
│   │  │ (Windows)  │  │ (macOS)    │  │ (Linux)│  │   │
│   │  └────────────┘  └────────────┘  └────────┘  │   │
│   │                                              │   │
│   │  ┌──────────────────────────────────────┐    │   │
│   │  │      共享 C++ Runtime                │    │   │
│   │  │      • virtio device models          │    │   │
│   │  │      • qcow2 / raw disk              │    │   │
│   │  │      • SPICE vdagent                 │    │   │
│   │  │      • lwIP NAT + DHCP               │    │   │
│   │  │      • libdatachannel (WebRTC)       │    │   │
│   │  └──────────────────────────────────────┘    │   │
│   └──────────────────────────────────────────────┘   │
│                                                      │
│   ┌────────────┐  ┌─────────────┐  ┌──────────────┐  │
│   │ Win32      │  │ SwiftUI     │  │ tenboxd      │  │
│   │ GUI Mgr    │  │ AppKit Mgr  │  │ (systemd)    │  │
│   │ (Windows)  │  │ (macOS)     │  │ (Linux)      │  │
│   └────────────┘  └─────────────┘  └──────────────┘  │
└──────────────────────────────────────────────────────┘
```

---

## 🧩 tenbox 模块结构（src/）

| 目录 | 职责 |
|------|------|
| `core/` | VMM 核心（vCPU 调度、客户机内存、设备路由） |
| `runtime/` | 跨平台 C++ 运行时（virtio、qcow2、网络） |
| `platform/` | 各平台 hypervisor 实现 |
| `daemon/` | Linux 守护进程（systemd 集成、CLI） |
| `cli/` | `tenbox` 命令行 |
| `client/` | 客户端库 |
| `manager/` | 通用 GUI 管理器逻辑 |
| `manager-macos/` | macOS SwiftUI 管理器 |
| `ipc/` | 进程间通信（Unix socket / Named Pipe） |
| `common/` | 公共工具 |

---

## 🌐 跨平台 Hypervisor 后端

| 平台 | Hypervisor | 关键 API |
|------|-----------|---------|
| **Windows** | **WHVP**（Windows Hypervisor Platform） | `WHvCreatePartition` / `WHvRunPartition` |
| **macOS (Apple Silicon + Intel)** | **Hypervisor Framework** | `hv_vm_create` / `hv_vcpu_run` |
| **Linux** | **KVM** | `/dev/kvm` ioctls |

tenbox 的设计哲学：**每个平台用原生最优的 hypervisor**，而不是抽象统一。

---

## 🖥️ Linux 守护进程 tenboxd

**特点**：
- 由 systemd 管理（`tenboxd.service`）
- 通过 `/run/tenbox/tenbox.sock` 提供本地 RPC
- 通过 `tenbox` Unix group 控制访问权限
- 支持 cloud pairing（8 位配对码 → https://my.tenbox.ai/pair）
- 内嵌浏览器远程桌面（libdatachannel + FFmpeg H.264 + Opus）

**我们的映射**：`daemon/` 模块 + `cli/vm.rs` 子命令

---

## 📺 virtio 设备模型

tenbox 实现了完整的 virtio 设备集：

| 设备 | 说明 | 用途 |
|------|------|------|
| `virtio-blk` | 块设备 | 客户机磁盘 |
| `virtio-net` | 网络 | NAT 网络 |
| `virtio-gpu` | 图形 | SPICE 显示 |
| `virtio-input` | 输入 | 键鼠 |
| `virtio-snd` | 音频 | WASAPI / CoreAudio 输出 |
| `virtio-fs` | 文件系统 | 共享目录 |
| `serial` | 串口 | 控制台 |

**总线**：virtio MMIO（更适合 micro VM）

**我们的映射**：`src/devices/` 下的各 `virtio_*.rs`

---

## 💾 磁盘格式

- **qcow2**：支持 zlib / zstd 压缩，支持 copy-on-write
- **raw**：原始格式
- **用途**：客户机 rootfs、内核、initramfs 存放

**我们的映射**：`src/image/qcow2.rs` + `src/image/raw.rs`

---

## 🌐 网络

tenbox 内置：

| 组件 | 说明 |
|------|------|
| **NAT 代理** | 内置 lwIP TCP/UDP NAT |
| **DHCP 服务器** | 自动分配客户机 IP |
| **ICMP 中继** | ping 透传 |
| **端口转发** | host-forward / guest-forward |

**我们的映射**：`src/network/` 模块

---

## 🔊 音频

| 平台 | 后端 |
|------|------|
| Windows | WASAPI |
| macOS | CoreAudio |
| Linux | PulseAudio / ALSA |

**我们的映射**：`src/devices/virtio_snd.rs` + 平台适配

---

## 🖼️ 远程桌面（Linux 专属）

tenbox 内置浏览器远程桌面：

```
Customer Browser ──┐
                   │
  WebRTC + SPICE   ▼
              ┌──────────────┐
              │   tenboxd    │
              │  • libdatachannel │
              │  • FFmpeg H.264   │
              │  • Opus 音频      │
              └──────────────┘
                   │
              ┌────▼────┐
              │  Guest  │
              │  VM     │
              └─────────┘
```

- 双 DataChannel：`input-fast` / `control`
- 双向剪贴板同步

**我们的映射**：暂缓，先实现基础 console

---

## 🔌 CLI 设计参考

tenbox 的 CLI 命令集非常清晰：

```bash
tenbox doctor              # 健康检查
tenbox system info         # 系统信息
tenbox vm ls               # 列出 VM
tenbox vm create --name X  # 创建
tenbox vm edit --name X    # 编辑
tenbox vm start X          # 启动
tenbox vm stop X           # 停止
tenbox vm reboot X         # 重启
tenbox vm shutdown X       # 优雅关机
tenbox vm rm X             # 删除
tenbox vm console X        # 控制台
tenbox vm logs X           # 日志
```

**我们的 CLI**：在 `src/cli/vm.rs` 中实现相同语义。

---

## 🤖 LLM Proxy

tenbox 内置 **OpenAI 兼容的 HTTP 代理**：

- 拦截 guest VM 内应用的 LLM 请求
- 映射到配置的上游 provider
- 在 `tenboxd`（Linux）和 GUI 管理器（Windows/macOS）中实现

**对我们的意义**：Claude Code 在 VM 内运行时，自动通过这个代理与外部通信。

---

## 🔐 客户机镜像

- 基于 **Alpine Linux**（轻量，约 5 MB）
- 启动后自动运行 `claude-code` agent
- 通过 virtiofs 与宿主共享项目目录
- 串口输出日志，可通过 `tenbox vm console` 查看

**我们的客户机**：Alpine + Claude Code + Chromium + FFmpeg

---

## 📊 tenbox vs video-summary-bot 对比

| 维度 | tenbox | video-summary-bot |
|------|--------|-------------------|
| **定位** | 通用 Agent VM | 视频总结 VM |
| **语言** | C++ | Rust |
| **平台** | Windows/macOS/Linux | Windows/macOS/Linux |
| **hypervisor** | WHVP/HVF/KVM | WHVP/HVF/KVM |
| **客户机** | Alpine Linux | Alpine + Claude Code |
| **GUI** | 有（Win32/SwiftUI） | 暂只 CLI/TUI |
| **CLI** | `tenbox` | `video-summary-bot` |
| **守护进程** | tenboxd（systemd） | daemon（计划中） |
| **远程桌面** | WebRTC + SPICE | 暂缓 |
| **LLM Proxy** | OpenAI 兼容 | Claude 集成 |
| **License** | GPL v3 | MIT |

---

## 🎯 我们从 tenbox 学到的

1. **跨平台 VMM 不是抽象统一，而是各平台用最优后端**
2. **virtio MMIO 总线**适合 micro VM（比 PCI 简单）
3. **Alpine Linux** 是 micro VM 的理想客户机
4. **CLI 命令语义**（vm ls/create/start/...）是行业标准
5. **守护进程 + Unix socket** 提供干净的 IPC
6. **配置持久化**（vm.json）让 VM 可管理
7. **NAT + DHCP** 集成在 VMM 中，不依赖外部网络栈

---

## 📖 推荐阅读

- tenbox README：https://github.com/78/tenbox/blob/main/README.md
- tenbox `CLAUDE.md` / `AGENTS.md`：架构意图说明（值得一读）
- tenbox `PLAN.md`：技术方案（24 KB，很详细）
- tenbox `docs/build.md`：构建说明

---

## 🔄 持续同步

本项目会持续参考 tenbox 上游更新。建议：

```bash
# 添加 tenbox 为参考远程
git remote add tenbox https://github.com/78/tenbox.git
git fetch tenbox

# 定期查看更新
git log --oneline tenbox/main | head -20
```