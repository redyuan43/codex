---
name: codex-safe-sandbox-no-approval
description: 把 Codex 全局默认权限切到“相对安全的 sandbox + 不审批”组合：`sandbox_mode = "workspace-write"` 与 `approval_policy = "never"`，并保留 `~/.codex/config.toml` 里的其他设置。适用于用户希望取消默认 `danger-full-access`，但又不想每次再手动批准命令时使用。
---

# Codex Safe Sandbox No Approval

当用户想把 Codex 的默认权限恢复到：

- `sandbox_mode = "workspace-write"`
- `approval_policy = "never"`

并且希望保留 `~/.codex/config.toml` 里的其他配置项时，使用这个 skill。

## 标准工作流

1. 先查看当前状态：

```bash
bash scripts/manage_codex_safe_no_approval.sh status
```

2. 再写入安全 sandbox + 不审批模式：

```bash
bash scripts/manage_codex_safe_no_approval.sh apply
```

3. 如果要操作其他配置文件路径，可以显式传入：

```bash
bash scripts/manage_codex_safe_no_approval.sh apply /path/to/config.toml
```

## 行为说明

- 默认目标文件是 `~/.codex/config.toml`
- 写入前会自动生成时间戳备份
- 只更新顶层 `approval_policy` 与 `sandbox_mode`
- 不覆盖用户已有的模型、插件、项目 trust 和其他配置

## 目标写入值

```toml
approval_policy = "never"
sandbox_mode = "workspace-write"
```

## 文件

- `scripts/manage_codex_safe_no_approval.sh`
