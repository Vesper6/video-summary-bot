# Video Summary Bot

> 🎬 视频总结工具 — 用 yt-dlp 下载字幕 + Claude AI 分析，自动生成结构化视频总结，支持视觉分析

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue.svg)]()
[![CI](https://github.com/Vesper6/video-summary-bot/actions/workflows/ci.yml/badge.svg)](https://github.com/Vesper6/video-summary-bot/actions/workflows/ci.yml)
[![LLM](https://img.shields.io/badge/LLM-Claude%20Sonnet-cc785c.svg)]()
[![Vision](https://img.shields.io/badge/vision-multimodal-9b59b6.svg)]()

**Video Summary Bot**（`vsb`）是一个跨平台命令行工具（含可选桌面 GUI），通过 **yt-dlp 下载字幕** + **Claude AI 分析**，自动对 B 站、YouTube 等平台的视频生成结构化总结报告。

除了字幕总结，它还支持 **视觉分析**：下载视频、按时间间隔抽帧，让 Claude 结合画面与字幕做多模态总结。

底层基于 Rust + Tokio 构建，跨平台运行于 Windows / macOS / Linux。

---

## 📑 目录

- [核心功能](#-核心功能)
- [快速开始](#-快速开始)
- [使用方式](#-使用方式)
- [视觉分析](#-视觉分析)
- [B 站 / YouTube cookies 配置](#-b-站--youtube-cookies-配置)
- [健康检查](#-健康检查)
- [技术栈](#-技术栈)
- [项目结构](#-项目结构)
- [配置说明](#-配置说明)
- [开发路线](#-开发路线)
- [贡献指南](#-贡献指南)
- [许可证](#-许可证)

---

## ✅ 核心功能

| 功能 | 状态 | 说明 |
|------|------|------|
| `vsb summarize --url` | ✅ 可用 | yt-dlp 获取字幕 + Claude 分析 |
| `vsb summarize --text` | ✅ 可用 | 直接传入文字稿，Claude 总结 |
| `vsb summarize --text-file` | ✅ 可用 | 读取 .vtt / .srt / .txt 字幕文件 |
| `vsb summarize --vision` | ✅ 可用 | 视觉分析：抽帧 + Claude 多模态结合画面+字幕总结 |
| `vsb doctor all` | ✅ 可用 | 健康检查（必选/可选依赖） |
| `vsb info` | ✅ 可用 | 系统信息（平台/工具可用性） |
| `vsb serve --port 8080` | ✅ 可用 | HTTP API + Web GUI |
| `vsb gui` | ✅ 可用 | 桌面窗口应用（tao + wry） |

**总结深度**（`--level`）：`light`（简要，3-5 要点）/ `standard`（标准，默认）/ `full`（深度分析）。

---

## 🚀 快速开始

### 1. 环境要求

| 依赖 | 是否必须 | 说明 |
|------|---------|------|
| Rust 1.75+ | ✅ 必须 | `rustup` 安装 |
| yt-dlp | ✅ 必须* | `pip install yt-dlp`（*仅 `--text` / `--text-file` 时可省略） |
| Claude API Token | ✅ 必须 | `ANTHROPIC_API_KEY` 或 `ANTHROPIC_AUTH_TOKEN` |
| FFmpeg | 视觉分析必须 | `--vision` 抽帧需要（普通字幕总结不需要） |
| cookies.txt | 可选 | 访问 B 站 / YouTube 需登录的内容 |

### 2. 构建

```bash
git clone https://github.com/Vesper6/video-summary-bot.git
cd video-summary-bot

# 默认构建（CLI）
cargo build --release

# 启用 HTTP API + 桌面 GUI
cargo build --release --features "api gui"

# 可选：加入 PATH
cp target/release/video-summary-bot ~/.local/bin/vsb   # Linux/macOS
# 或手动把 target/release/ 加入 PATH（Windows）
```

### 3. 配置 Claude API

```bash
cp .env.example .env
# 编辑 .env，填写：
ANTHROPIC_API_KEY=sk-ant-xxxxx
# 或（兼容 CC Switch 代理）
ANTHROPIC_AUTH_TOKEN=sk-ant-xxxxx

# 使用代理 API（CC Switch）时可选配置：
ANTHROPIC_BASE_URL=https://api.your-proxy.com
ANTHROPIC_MODEL=claude-sonnet-4-6
```

> CC Switch 用户通常无需额外配置，环境变量已自动注入。

### 4. 验证环境

```bash
vsb doctor all
```

---

## 📖 使用方式

Video Summary Bot 支持三种输入模式，外加可选的视觉分析。

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

# 指定输出语言
vsb summarize --url "..." --language zh-CN

# 保存为 Markdown 文件
vsb summarize --url "..." --output ./output/

# 跳过字幕下载，直接让 Claude 根据其知识分析
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

### 模式三：手动下载字幕后分析

```bash
# 先用 yt-dlp 下载字幕
yt-dlp --write-auto-sub --sub-lang zh-Hans --skip-download \
       --cookies cookies.txt \
       "https://www.bilibili.com/video/BV1xxx"

# 然后分析
vsb summarize --text-file "视频标题.zh-Hans.vtt" --level standard
```

---

## 👁️ 视觉分析

普通总结只看字幕。**视觉分析**（`--vision`）会下载视频、抽取关键帧，让 Claude
**结合画面 + 字幕**做多模态总结 —— 适合演示、教程、无字幕或字幕信息量不足的视频。

```bash
# 开启视觉分析（默认每 30 秒抽 1 帧）
vsb summarize --url "https://www.youtube.com/watch?v=xxxxx" --vision

# 自定义抽帧间隔（秒）
vsb summarize --url "..." --vision --vision-interval 15

# 结合总结深度与输出
vsb summarize --url "..." --vision --level full --output ./output/
```

**工作原理与限制：**

- 需要 **yt-dlp**（下载视频）+ **FFmpeg**（抽帧），两者都必须安装。
- 默认每 **30 秒抽 1 帧**，帧数有**硬上限 40 帧**；超长视频会自动拉大间隔，避免一次塞入过多图片导致 token 爆炸。
- 视频下载限制到 **≤480p**，节省流量与 token。
- 帧以 base64 image block 通过 **Anthropic Messages API** 发送给 Claude（Sonnet / Opus 均为视觉模型）。
- 视觉分析需要下载整段视频，因此比只下字幕**慢很多**，所以它是**可选功能**，只有加 `--vision` 才启用。

---

## 🍪 B 站 / YouTube cookies 配置

B 站和 YouTube 的部分内容需要登录才能获取完整字幕/视频。yt-dlp 的字幕语言列表已包含
B 站 AI 生成字幕的语言代码 **`ai-zh`**（以及 `zh-Hans` / `zh-Hant` / `zh-CN` / `zh` / `zh-TW` / `en`）。

导出 cookies：

1. 安装浏览器插件 **[Get cookies.txt LOCALLY](https://chrome.google.com/webstore/detail/get-cookiestxt-locally/cclelndahbckbenkjhflpdbgdldlbecc)**（Chrome / Edge）
2. 登录 bilibili.com（或 youtube.com）
3. 点击插件图标 → **Export**，保存为 `cookies.txt`
4. 将文件放到项目根目录（自动检测），或用 `--cookies` 参数指定路径

```bash
# 项目根目录放 cookies.txt（自动检测）
vsb summarize --url "https://www.bilibili.com/video/BV1xxx"

# 或指定路径
vsb summarize --url "..." --cookies /path/to/cookies.txt
```

---

## 🩺 健康检查

```bash
vsb doctor all   # 完整健康检查（必选/可选依赖分级）
vsb info         # 系统信息（平台 / CPU / 工具可用性）
```

`vsb doctor all` 会检查：

- **必选**：Claude API、yt-dlp
- **可选**：FFmpeg（视觉分析需要）、cookies.txt（登录内容需要）

---

## 🛠️ 技术栈

| 类别 | 技术 |
|------|------|
| 语言 | Rust 1.75+（Edition 2021） |
| 异步运行时 | Tokio（rt-multi-thread + process） |
| CLI 框架 | clap 4 |
| HTTP 客户端 | reqwest（rustls-tls），调用 Anthropic Messages API |
| HTTP 服务 | axum（`api` feature） |
| 桌面 GUI | tao + wry（`gui` feature） |
| LLM | Anthropic Messages API（含多模态 image block，兼容 CC Switch） |
| 字幕下载 | yt-dlp（subprocess） |
| 视频抽帧 | FFmpeg（subprocess） |
| 序列化 | serde + serde_json |
| 日志 | tracing + tracing-subscriber |
| 临时文件 | tempfile |

### Cargo Features

| Feature | 说明 |
|---------|------|
| `default` | 空（仅 CLI） |
| `api` | 启用 `vsb serve` 的 HTTP API + Web GUI |
| `gui` | 启用 `vsb gui` 桌面窗口（tao + wry） |

---

## 📂 项目结构

```
video-summary-bot/
├── src/
│   ├── main.rs              # 程序入口
│   ├── lib.rs               # 库根
│   ├── error.rs             # 统一错误类型
│   ├── agents/
│   │   └── claude.rs        # Anthropic Messages API 客户端（含多模态 send_with_images）
│   ├── crawler/
│   │   ├── subtitle.rs      # yt-dlp 字幕下载 + 视频下载（VTT/SRT 解析）
│   │   └── vision.rs        # FFmpeg 抽帧 + base64 编码
│   ├── cli/                 # summarize / doctor / info / serve / gui
│   │   ├── summary.rs       # summarize 命令
│   │   ├── system.rs        # doctor / info
│   │   └── gui.rs           # gui 命令入口
│   ├── api/mod.rs           # HTTP API（/summarize /doctor /system/info）+ Web GUI 静态服务
│   ├── daemon/mod.rs        # serve 命令
│   ├── gui/                 # 桌面窗口（tao + wry）
│   ├── config/             # 配置加载
│   └── utils/              # 工具函数
├── .github/
│   └── workflows/          # CI（3 平台 lint/build/test）+ release
├── Cargo.toml
├── .env.example
├── CLAUDE.md               # Claude Code 项目级指引
├── COLLAB.md               # 协作记录
└── CONTRIBUTING.md
```

---

## ⚙️ 配置说明

复制 `.env.example` 并按需修改：

```bash
cp .env.example .env
```

| 变量 | 说明 | 默认值 |
|------|------|--------|
| `ANTHROPIC_API_KEY` | 标准 Anthropic API Key | — |
| `ANTHROPIC_AUTH_TOKEN` | Claude API Token（CC Switch 用这个） | — |
| `ANTHROPIC_BASE_URL` | API 代理地址 | `https://api.anthropic.com` |
| `ANTHROPIC_MODEL` | 使用的模型 | `claude-sonnet-4-6` |
| `LOG_FORMAT` | 日志格式（`text` / `json`） | `text` |
| `RUST_LOG` | 日志级别（`info` / `debug` / `warn`） | `warn` |

> `ANTHROPIC_API_KEY` 与 `ANTHROPIC_AUTH_TOKEN` 二选一即可。

---

## 🛣️ 开发路线

### 已完成
- [x] 项目基础（Cargo.toml、CI/CD、Issue 模板、Dependabot）
- [x] Anthropic Messages API 客户端（兼容 CC Switch 代理）
- [x] yt-dlp 字幕下载器（VTT/SRT 解析，含 B 站 `ai-zh` AI 字幕）
- [x] `summarize` 命令（3 种输入模式：URL / 文字稿 / 文件）
- [x] 视觉分析（`--vision`）：yt-dlp 下视频 + FFmpeg 抽帧 + Claude 多模态总结
- [x] `doctor` 健康检查（必选/可选分级）+ `info` 系统信息
- [x] HTTP API（`vsb serve`，axum）+ Web GUI
- [x] 桌面窗口应用（`vsb gui`，tao + wry）
- [x] GitHub Actions CI（3 平台 lint/build/test）+ Release 工作流

### 进行中 / 近期
- [ ] B 站完整字幕获取端到端验证（cookies）
- [ ] 视觉分析抽帧策略优化（关键帧检测）
- [ ] `summarize` 输出 HTML 格式

### 长期
- [ ] ASR 转写（无字幕视频，Whisper 集成）
- [ ] 时间线分段结构化输出
- [ ] 批量/队列总结

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
