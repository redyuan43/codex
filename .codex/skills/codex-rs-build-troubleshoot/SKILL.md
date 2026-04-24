---
name: codex-rs-build-troubleshoot
description: 同步 `codex-rs` 最新代码并编译 `codex-cli`，同时判断 `core`、`tui`、`app-server`、`mcp` 等核心模块是否随依赖链成功编译，并排查“源码已改但运行行为没变”、feature gate、配置文件、`/loop`/alarm 等构建与运行错位问题。适用于需要拉取云端最新代码、开始编译、确认核心模块编译状态或定位 `/loop` 功能异常时使用。
---

# codex-rs Build Troubleshoot

当用户需要在 `codex-rs` 仓库里同步最新代码、编译 `codex-cli`、确认核心模块是否一起通过编译，或排查 `/loop`、feature gate、二进制路径和配置错位问题时，使用这个 skill。

## 标准工作流

1. 查看当前仓库与二进制状态：

```bash
bash scripts/build_codex_rs.sh status
```

2. 先同步最新代码，再开始编译：

```bash
bash scripts/build_codex_rs.sh sync
bash scripts/build_codex_rs.sh build
```

3. 如果要确认当前真正运行的是哪个 `codex`，以及本地 debug 二进制是否最新：

```bash
bash scripts/build_codex_rs.sh check-binary
```

4. 如果问题与 `/loop` 或 alarm 功能有关，跑专项诊断：

```bash
bash scripts/build_codex_rs.sh doctor-loop
```

5. 如果需要补一轮关键测试验证：

```bash
bash scripts/build_codex_rs.sh tests
```

## 行为规则

- 默认目标仓库是 `~/github/codex/codex-rs`
- `sync` 使用 `git pull --ff-only`
- `build` 使用 `cargo build -p codex-cli`
- `tests` 只跑关键验证：
  - `cargo test -p codex-tui`
  - `cargo test -p codex-app-server-protocol`
- `check-binary` 只做只读核对，不修改配置
- `doctor-loop` 只输出诊断信息，不自动修复 feature 或配置

## 核心判断口径

- `cargo build -p codex-cli` 成功，通常意味着 `codex-cli` 依赖链上的 `codex-core`、`codex-tui`、`codex-app-server`、`codex-app-server-protocol`、`codex-mcp`、`codex-mcp-server` 等核心 crate 已成功编译
- 这不等于整个 workspace 的所有 crate、所有测试、所有 feature 组合都验证通过
- 如果“源码已经改了，但运行行为没变化”，优先检查：
  - 实际运行的 `codex` 二进制路径
  - `target/debug/codex` 是否是最新构建
  - `~/.codex/config.toml` 是否启用了相关 feature
  - 是否仍在使用旧 thread 或旧进程

## /loop 专项

`/loop` 相关问题优先做这四步：

1. `bash scripts/build_codex_rs.sh doctor-loop`
2. 确认 `~/.codex/config.toml` 里有：

```toml
[features]
alarm_tool = true
```

3. 重新编译并使用最新的 `./target/debug/codex`
4. 如果是 `resume` 旧 thread，重启进程后重新验证，必要时新开 thread 对比

更完整的案例和排障解释见 `references/troubleshooting.md`。

## 文件

- `scripts/build_codex_rs.sh`
- `references/troubleshooting.md`
