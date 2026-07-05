# 贡献指南

欢迎为 **Video Summary Bot** 贡献代码、文档或提出 Issue！🎉

## 📋 目录

- [行为准则](#-行为准则)
- [提 Issue](#-提-issue)
- [提 PR](#-提-pr)
- [开发环境搭建](#-开发环境搭建)
- [代码规范](#-代码规范)
- [提交规范](#-提交规范)
- [测试要求](#-测试要求)

---

## 🤝 行为准则

- 尊重所有贡献者
- 欢迎不同观点的讨论
- 专注于项目目标，避免无关争论
- 对新手友好，乐于解答问题

---

## 🐛 提 Issue

提交 Issue 前请：

1. **搜索现有 Issue**，避免重复
2. **使用 Issue 模板**（如有）
3. **提供详细信息**：
   - 复现步骤
   - 预期行为 vs 实际行为
   - 环境信息（OS、Rust 版本、KVM 可用性等）
   - 相关日志或截图

Issue 类型：

- `bug` — 程序错误
- `enhancement` — 功能改进
- `documentation` — 文档问题
- `question` — 使用咨询

---

## 🔧 提 PR

### 流程

1. Fork 本仓库
2. 创建特性分支：`git checkout -b feature/your-feature`
3. 编写代码 + 测试
4. 运行本地检查（见下）
5. 提交变更：`git commit -m "feat: add your feature"`
6. 推送到 fork：`git push origin feature/your-feature`
7. 在 GitHub 上创建 Pull Request

### PR 要求

- ✅ 通过所有 CI 检查
- ✅ 包含必要的测试
- ✅ 更新相关文档
- ✅ 一个 PR 聚焦一个改动
- ✅ 与 `main` 分支保持同步

---

## 💻 开发环境搭建

### 系统依赖

```bash
# Ubuntu / Debian
sudo apt update
sudo apt install -y \
    qemu-kvm libvirt-daemon-system libvirt-clients \
    qemu-utils ovmf ffmpeg \
    build-essential pkg-config libssl-dev
```

### Rust 工具链

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 常用组件
rustup component add clippy rustfmt
```

### 项目设置

```bash
git clone https://github.com/Vesper6/video-summary-bot.git
cd video-summary-bot
cp .env.example .env
# 编辑 .env 填入 ANTHROPIC_API_KEY
cargo build
```

---

## 📐 代码规范

### 格式化与 Lint

```bash
# 格式化（提交前必跑）
cargo fmt

# Lint（warning 必须清零）
cargo clippy --all-targets --all-features -- -D warnings

# 编译检查
cargo check --all-targets --all-features
```

### 风格约定

- 使用 `rustfmt` 默认风格
- 公开 API 必须有文档注释（`///`）
- 错误处理优先用 `thiserror`（库）+ `anyhow`（应用）
- 避免 `unwrap()` / `expect()`（测试除外）
- 异步函数命名：动词 + `_async` 或明确语义

### 模块组织

```
src/
├── agents/      # AGENT 模块
├── workflows/   # 工作流
├── sandbox/     # KVM 沙箱
├── crawler/     # 爬虫
├── summarizer/  # 总结
├── timeline/    # 时间线
├── ui/          # CLI / TUI
├── api/         # HTTP API
├── config/      # 配置
└── utils/       # 工具
```

每个模块有 `mod.rs`，公开 API 通过 `pub use` 重导出。

---

## 📝 提交规范

采用 [Conventional Commits](https://www.conventionalcommits.org/) 规范：

```
<type>(<scope>): <subject>

<body>

<footer>
```

### Type

| Type       | 说明                          |
|------------|-------------------------------|
| `feat`     | 新功能                        |
| `fix`      | Bug 修复                      |
| `docs`     | 文档变更                      |
| `style`    | 代码格式（不影响逻辑）        |
| `refactor` | 重构                          |
| `perf`     | 性能优化                      |
| `test`     | 增加/修改测试                 |
| `chore`    | 构建/工具链/CI 变更           |
| `revert`   | 回滚                          |

### 示例

```bash
git commit -m "feat(sandbox): add libvirt VM lifecycle management"
git commit -m "fix(crawler): handle empty comment list gracefully"
git commit -m "docs: update KVM setup instructions in README"
```

---

## ✅ 测试要求

### 必跑项

```bash
# 单元测试
cargo test --lib

# 集成测试
cargo test --test '*'

# 文档测试
cargo test --doc
```

### 测试覆盖

- 新功能必须有单元测试
- Bug 修复必须包含回归测试
- 关键路径必须有集成测试

### 沙箱测试

涉及 KVM 沙箱的测试默认 **跳过**（需真实 KVM 环境）：

```bash
# 启用沙箱集成测试
cargo test --features kvm-libvirt -- --ignored
```

---

## 📮 联系方式

- **Issue**：[GitHub Issues](https://github.com/Vesper6/video-summary-bot/issues)
- **Discussion**：[GitHub Discussions](https://github.com/Vesper6/video-summary-bot/discussions)

感谢你的贡献！🙏