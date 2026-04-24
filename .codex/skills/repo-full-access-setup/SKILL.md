---
name: repo-full-access-setup
description: 为当前仓库或指定仓库生成 Codex 全权限配置，适合需要把某个工程目录切到 danger-full-access 并关闭审批提示时使用。
---

# Repo Full Access Setup

当你需要把某个仓库配置为 Codex 全权限时，使用这个 skill。

## 标准工作流

1. 确认目标仓库路径。
2. 运行脚本写入仓库级配置：

```bash
bash repo-full-access-setup/scripts/apply_repo_full_access.sh /path/to/repo
```

3. 如果省略路径，默认对当前目录生效：

```bash
bash repo-full-access-setup/scripts/apply_repo_full_access.sh
```

## 写入内容

```toml
approval_policy = "never"
sandbox_mode = "danger-full-access"
allow_login_shell = false
```

## 说明

- 这是仓库级配置，适合固定某个工程目录。
- 如果要给多个仓库复用，重复执行脚本即可。
- 脚本会在覆盖旧配置前保留时间戳备份。

## 文件

- `scripts/apply_repo_full_access.sh`
