# CLAUDE.md

> 给 Claude Code / Claude 工具阅读的项目级指引。本文件提供 Claude 在此仓库工作时所需的上下文。

## 🎯 项目目标

**Video Summary Bot**（`vsb`）是一个跨平台命令行工具（含可选桌面 GUI），通过
**yt-dlp 下载视频字幕** + **Claude AI 分析**，自动生成结构化视频总结：

- 从 URL 自动获取字幕，或直接分析传入的文字稿/字幕文件（.vtt/.srt/.txt）
- 可选**视觉分析**（`--vision`）：下载视频、FFmpeg 抽帧，让 Claude 结合画面与字幕做多模态总结
- 支持 B 站（含 `ai-zh` AI 字幕）、YouTube 等平台，登录内容通过 `cookies.txt`

跨平台运行于 Windows / macOS / Linux，无任何平台专属后端。

---

## 🏗️ 技术栈速览

| 类别 | 技术 |
|------|------|
| 语言 | Rust 1.75+（Edition 2021） |
| 异步运行时 | Tokio（rt-multi-thread + process） |
| HTTP 客户端 | reqwest（rustls-tls），调用 Anthropic Messages API |
| HTTP 服务 | axum（`api` feature） |
| 桌面 GUI | tao + wry（`gui` feature） |
| LLM | Anthropic Messages API（含多模态 image block） |
| 字幕下载 | yt-dlp（subprocess） |
| 视频抽帧 | FFmpeg（subprocess） |
| CLI | clap 4 |
| 序列化 | serde + serde_json |
| 日志 | tracing |
| 配置 | config-rs + dotenvy |

**Cargo features**：`default`（空，仅 CLI）/ `api`（HTTP API + Web GUI）/ `gui`（桌面窗口）。

---

## 📂 目录结构（核心模块）

```
src/
├── main.rs              # 程序入口
├── lib.rs               # 库根
├── error.rs             # 统一错误类型
├── agents/
│   └── claude.rs        # Anthropic Messages API 客户端（含多模态 send_with_images）
├── crawler/
│   ├── subtitle.rs      # yt-dlp 字幕下载 + 视频下载（VTT/SRT 解析）
│   └── vision.rs        # FFmpeg 抽帧 + base64 编码
├── cli/                 # summarize / doctor / info / serve / gui
│   ├── summary.rs       # summarize 命令
│   ├── system.rs        # doctor / info
│   └── gui.rs           # gui 命令入口
├── api/mod.rs           # HTTP API（/summarize /doctor /system/info）+ Web GUI 静态服务
├── daemon/mod.rs        # serve 命令
├── gui/                 # 桌面窗口（tao + wry）
├── config/             # 配置加载
└── utils/              # 工具函数
```

---

## 🚀 常用命令

```bash
# 编译
cargo build
cargo build --release
cargo build --release --features "api gui"   # 启用 HTTP API + 桌面 GUI

# 运行测试
cargo test

# 代码质量
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings

# 运行程序
./target/release/video-summary-bot --help

# 字幕总结（三种输入模式）
vsb summarize --url "https://www.bilibili.com/video/BV1xxx" --cookies cookies.txt
vsb summarize --text "..." --level full
vsb summarize --text-file subtitle.vtt --output ./output/

# 视觉分析（需要 yt-dlp + FFmpeg）
vsb summarize --url "..." --vision
vsb summarize --url "..." --vision --vision-interval 15

# 健康检查 / 系统信息
vsb doctor all
vsb info

# HTTP API + Web GUI / 桌面窗口
vsb serve --port 8080
vsb gui
```

---

## 🎬 字幕与视觉分析要点

- **字幕语言**：yt-dlp 的 `--sub-lang` 包含 `ai-zh`（B 站 AI 字幕）+ `zh-Hans/zh-Hant/zh-CN/zh/zh-TW/en`。
- **cookies**：B 站 / YouTube 登录内容需 `cookies.txt`（浏览器插件导出），放项目根目录或 `--cookies` 指定。
- **视觉分析**（`--vision`）：
  1. 需要 yt-dlp（下视频）+ FFmpeg（抽帧）
  2. 默认每 30 秒抽 1 帧，**硬上限 40 帧**（`vision::MAX_FRAMES`），超长视频自动拉大间隔，避免 token 爆炸
  3. 视频下载限制到 **≤480p**，省流量/token
  4. 帧以 base64 image block 走 Anthropic Messages API 发给 Claude（Sonnet/Opus 均为视觉模型）
  5. 因为要下载整段视频，比只下字幕慢很多，所以是**可选功能**

---

## 🤖 Claude 集成规范

调用 Claude 时：
- 通过 `ANTHROPIC_API_KEY` 或 `ANTHROPIC_AUTH_TOKEN` 鉴权（**不要硬编码**）
- 默认模型：`claude-sonnet-4-6`，可用 `ANTHROPIC_MODEL` 覆盖
- 代理地址可用 `ANTHROPIC_BASE_URL` 配置（兼容 CC Switch）
- 客户端在 `src/agents/claude.rs`：`send`（纯文本）与 `send_with_images`（多模态）
- 长任务注意超时与 token 上限（超长视频靠帧数上限控制）

---

## 📝 代码风格

- 使用 `rustfmt` 默认风格
- 公开 API 必须有 `///` 文档注释（clippy `missing_docs` 已开启）
- 错误处理：`thiserror`（库）+ `anyhow`（应用）
- 避免 `unwrap()` / `expect()`（测试除外）
- 异步函数返回 `Result<T, Error>`
- 模块通过 `mod.rs` 组织，公开 API 用 `pub use` 重导出
- 提交前必跑 `cargo fmt` + `cargo clippy --all-targets --all-features -- -D warnings`

---

## 🧪 测试约定

- 单元测试放在各文件 `#[cfg(test)] mod tests`
- 集成测试放在 `tests/` 目录
- Mock HTTP 请求使用 `mockito`
- 临时文件用 `tempfile` crate
- 涉及 yt-dlp / FFmpeg 的外部子进程测试尽量 mock 或标 `#[ignore]`

---

## 📚 关键文档位置

| 主题 | 位置 |
|------|------|
| 项目说明 | `README.md` |
| 协作记录 | `COLLAB.md` |
| 贡献指南 | `CONTRIBUTING.md` |
| 许可证 | `LICENSE` |
| 环境变量 | `.env.example` |
| 依赖清单 | `Cargo.toml` |

---

## ⚠️ 给 Claude 的提醒

1. **修改 `Cargo.toml` 依赖版本前**，先确认兼容性
2. **新增模块** 时，更新 `src/lib.rs` 的 `mod` 声明
3. **环境变量** 默认值与 `.env.example` 保持一致
4. **不要把 `.env`、API key、token 写入代码或提交**
5. **提交信息** 遵循 Conventional Commits 规范

---

## 🛣️ 当前优先级

1. 字幕总结（URL / 文字稿 / 字幕文件三种输入模式）
2. 视觉分析（`--vision`：FFmpeg 抽帧 + Claude 多模态）
3. B 站 / YouTube cookies 与 `ai-zh` 字幕端到端验证
4. HTTP API + Web GUI / 桌面 GUI 打磨
5. 输出格式扩展（HTML）、ASR 转写（无字幕视频）
