---
name: Bug Report
about: 报告一个 Bug
title: '[Bug]: '
labels: bug
assignees: ''
---

## Bug 描述

简要描述出现的 bug。

## 复现步骤

1. 执行命令 `...`
2. 传入参数 `...`
3. 看到错误 `...`

## 预期行为

描述你期望的正确行为。

## 实际行为

描述实际发生了什么。

## 环境信息

```text
- OS: [e.g. Windows 11 / Ubuntu 22.04 / macOS 14]
- Rust version: [e.g. 1.96.1]
- vsb version: [e.g. 0.1.0]
- Hypervisor: [WHVP / HVF / KVM]
- CPU: [e.g. Intel i7-12700]
- 内存: [e.g. 16 GB]
```

## 错误日志

```text
粘贴 `RUST_LOG=debug vsb <command> 2>&1 | tee log.txt` 的完整输出
```

## 截图（如有）

如果是 UI 相关问题，附上截图。

## 附加上下文

其他相关信息（例如：是否是首次运行？其他命令是否正常？）