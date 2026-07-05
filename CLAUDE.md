# CLAUDE.md

> 给 Claude Code / Claude 工具阅读的项目级指引。本文件提供 Claude 在此仓库工作时所需的上下文。

## 🎯 项目目标

**Video Summary Bot** 是一个基于 **Rust + KVM + Claude Code** 的智能视频内容总结工具：
- 解析视频内容，按时间线生成结构化摘要
- 通过 KVM 沙箱抓取评论与弹幕
- 由 Claude Code 驱动智能决策

---

## 🏗️ 技术栈速览

| 类别 | 技术 |
|------|------|
| 语言 | Rust 1.75+ (Edition 2021) |
| 异步运行时 | Tokio |
| HTTP 客户端 | reqwest |
| HTTP 服务 | axum |
| 浏览器自动化 | chromiumoxide |
| 沙箱虚拟化 | KVM + QEMU + libvirt |
| LLM | Claude Code CLI / Anthropic API |
| CLI | clap |
| TUI | ratatui + crossterm |
| 日志 | tracing |
| 配置 | config-rs + dotenvy |

---

## 📂 目录结构（核心模块）

```
src/
├── main.rs              # 程序入口
├── lib.rs               # 库根
├── agents/              # AGENT 调度（基于 Claude Code）
├── workflows/           # 工作流引擎
├── sandbox/             # KVM 沙箱管理
│   ├── kvm.rs           # KVM hypervisor
│   ├── qemu.rs          # QEMU 进程
│   └── libvirt.rs       # libvirt 封装
├── crawler/             # 爬虫（评论 / 弹幕）
├── summarizer/          # 视频内容总结
│   ├── asr.rs           # 语音转文字
│   └── claude.rs        # Claude Code 调用
├── timeline/            # 时间线分段
├── ui/                  # CLI / TUI
├── api/                 # HTTP API 路由
├── config/              # 配置加载
└── utils/               # 工具函数
```

---

## 🚀 常用命令

```bash
# 编译
cargo build
cargo build --release

# 运行测试
cargo test
cargo test --features kvm-libvirt -- --ignored  # 沙箱集成测试

# 代码质量
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings

# 运行程序
./target/release/video-summary-bot --help
./target/release/video-summary-bot summarize --url "..." --level standard
./target/release/video-summary-bot serve --port 8080

# 沙箱管理
./target/release/video-summary-bot sandbox init
./target/release/video-summary-bot sandbox status
```

---

## 🔒 沙箱安全原则

**所有爬虫与浏览器自动化操作必须在 KVM 虚拟机内执行，绝不允许直接访问宿主系统。**

- VM 镜像：`./vm-images/base.qcow2`
- 默认内存：`4096 MB`
- 默认 vCPU：`2`
- 沙箱后端：`kvm-libvirt`（推荐）/ `kvm-direct`
- 资源限制通过 libvirt 配置

涉及沙箱修改时：
1. 默认使用 `libvirt` 而非直接 ioctl
2. 任何 VM 状态变更必须记录日志
3. VM 销毁前确保数据已落盘

---

## 🤖 Claude Code 集成规范

调用 Claude 时：
- 通过 `ANTHROPIC_API_KEY` 鉴权（不要硬编码）
- 默认模型：`claude-sonnet-5`
- 单次请求最大 token：4096（可通过 `.env` 调整）
- 温度：0.3（保持总结稳定性）
- 长任务使用流式响应（避免超时）

调用模式：
1. **规划模式**：让 Claude 输出 JSON 格式的执行计划
2. **执行模式**：Claude 调度各模块 API
3. **反馈模式**：Claude 根据中间结果动态调整

---

## 📝 代码风格

- 使用 `rustfmt` 默认风格
- 公开 API 必须有 `///` 文档注释
- 错误处理：`thiserror`（库）+ `anyhow`（应用）
- 避免 `unwrap()` / `expect()`（测试除外）
- 异步函数返回 `Result<T, Error>`
- 模块通过 `mod.rs` 组织，公开 API 用 `pub use` 重导出

---

## 🧪 测试约定

- 单元测试放在各文件 `#[cfg(test)] mod tests`
- 集成测试放在 `tests/` 目录
- 沙箱相关测试默认 `#[ignore]`，需要真实 KVM 环境
- Mock HTTP 请求使用 `mockito`
- 临时文件用 `tempfile` crate

---

## 📚 关键文档位置

| 主题 | 位置 |
|------|------|
| 项目说明 | `README.md` |
| 贡献指南 | `CONTRIBUTING.md` |
| 许可证 | `LICENSE` |
| 环境变量 | `.env.example` |
| 依赖清单 | `Cargo.toml` |

---

## ⚠️ 给 Claude 的提醒

1. **不要删除或覆盖沙箱 VM 镜像**（`./vm-images/`），除非用户明确要求
2. **修改 `Cargo.toml` 依赖版本前**，先确认兼容性
3. **新增模块** 时，更新 `src/lib.rs` 的 `mod` 声明
4. **环境变量** 默认值与 `.env.example` 保持一致
5. **提交信息** 遵循 Conventional Commits 规范
6. **不要把 `.env`、API key、token 写入代码或提交**

---

## 🛣️ 当前优先级

参考 `README.md` 中的开发路线，当前重点：

1. KVM/libvirt 沙箱基础能力
2. 浏览器自动化与爬虫模块
3. Claude Code SDK 集成
4. 工作流引擎
5. TUI（进度条 + 时间线）