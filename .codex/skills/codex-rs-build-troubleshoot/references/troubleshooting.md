# codex-rs Build Troubleshooting

## 1. `cargo build -p codex-cli` 成功，能说明什么

如果 `cargo build -p codex-cli` 成功，通常说明这些核心 crate 已经沿依赖链一起编译通过：

- `codex-core`
- `codex-tui`
- `codex-app-server`
- `codex-app-server-client`
- `codex-app-server-protocol`
- `codex-mcp`
- `codex-mcp-server`
- `codex-tools`
- `codex-rollout`

但它不自动等于：

- 整个 workspace 所有 crate 都已构建
- 所有测试通过
- 所有 feature 组合都没问题

如果用户追问“核心模块是不是都编译过了”，可以明确说：

- `codex-cli` 依赖链上的核心模块已编译通过
- 更强验证需要补 `cargo test -p codex-tui`、`cargo test -p codex-app-server-protocol`，或做 workspace 级别测试

## 2. 源码改了，但运行行为没变

优先检查：

1. 当前实际运行的是哪个 `codex`
2. `./target/debug/codex` 是否为最新构建
3. 用户是否仍在运行旧进程
4. 用户是否在运行全局 npm 安装版 `codex`，而不是本地 Rust 构建

最小核对命令：

```bash
which codex
readlink -f "$(which codex)"
codex --version
./target/debug/codex --version
stat -c '%y %n' ./target/debug/codex
```

## 3. `/loop` 仍然报 `Unrecognized command`

优先排查顺序：

1. `~/.codex/config.toml` 是否已启用：

```toml
[features]
alarm_tool = true
```

2. 当前运行的是不是最新编出来的 `./target/debug/codex`
3. 用户是否仍在旧进程里测试
4. 是否仍在旧 `resume` thread 里测试，导致误以为新代码未生效

典型现象：

- 配置已开，但运行的是旧二进制
- 新二进制已编好，但仍在旧进程里
- 改的是 Rust 源码，运行的却是全局 npm `codex`

## 4. `resume` 线程和新线程的区别

如果用户用的是：

```bash
./target/debug/codex resume <thread-id>
```

先提醒：

- 需要先确认已经完全退出旧进程
- 建议在同一份最新构建上重新启动后再测试
- 如果行为仍异常，可以新开 thread 做对照

如果新 thread 正常、旧 thread 异常，说明问题更可能在恢复状态、旧线程上下文或用户观察路径，而不是编译失败。

## 5. 两个常见但不阻塞 build 的 warning

如果看到类似：

- `completed_session_loop_termination` 未使用
- `mcp_startup_cancellation_token` 未使用

这类 warning 不会阻塞 `cargo build -p codex-cli` 成功，不应误判成构建失败。
