# 让 Codex CLI 使用 LM Studio 的 MiniMax 模型

## Summary
- 这台机器上的 `LM Studio` 本地 API 已经可访问，地址是 `http://localhost:1234/v1`。
- 当前实际暴露出来的 MiniMax 模型 ID 不是 `minimax-m2.7`，而是：
  - `minimax-m2.7@q3_k_s`
  - `minimax-m2.7@q3_k_xl`
- 你现在的 [config.toml](/home/dgx/.codex/config.toml:1) 仍然是 `model = "gpt-5.4"`，所以默认还会走 OpenAI；要切到 LM Studio，需要改默认 `provider` 和 `model`。

## Key Changes
- 推荐先做一次临时验证，不改默认配置：
  - `./target/debug/codex --oss --local-provider lmstudio -m "minimax-m2.7@q3_k_s"`
  - 如果你想用另一个量化版本，就把模型名换成 `minimax-m2.7@q3_k_xl`
- 如果临时验证通过，再改成永久默认：
  - 把 [config.toml](/home/dgx/.codex/config.toml:1) 里的 `model = "gpt-5.4"` 改成目标 LM Studio 模型名
  - 新增 `model_provider = "lmstudio"`
  - 可选新增 `oss_provider = "lmstudio"`，这样以后手动加 `--oss` 时也默认选 LM Studio
- 最小可用配置如下：
```toml
model_provider = "lmstudio"
model = "minimax-m2.7@q3_k_s"
oss_provider = "lmstudio"
```
- 不需要手写 `[model_providers.lmstudio]`，因为 `lmstudio` 是内置 provider。相关入口在 [tui/src/cli.rs](/home/dgx/github/codex/codex-rs/tui/src/cli.rs:57) 和 [model-provider-info/src/lib.rs](/home/dgx/github/codex/codex-rs/model-provider-info/src/lib.rs:311)。
- 如果以后 `LM Studio` 不跑在 `1234` 端口：
  - 优先把 `LM Studio Local Server` 端口改回 `1234`
  - 或在启动 `codex` 前设置 `CODEX_OSS_PORT`
  - 或直接设置 `CODEX_OSS_BASE_URL=http://<host>:<port>/v1`

## Test Plan
- 先确认本地模型列表：
  - `curl http://localhost:1234/v1/models`
- 临时验证 Codex 走本地 MiniMax：
  - `./target/debug/codex --oss --local-provider lmstudio -m "minimax-m2.7@q3_k_s"`
- 如果改了默认配置，直接启动：
  - `./target/debug/codex`
- 进入后检查状态栏或会话元数据，确认显示的是 `lmstudio` + 目标模型，而不是 `openai` + `gpt-5.4`
- 如果启动后报 `/v1/responses` 不支持或模型不支持工具调用，那不是配置名写错，而是当前 `LM Studio` 服务或该模型能力不满足 Codex 期望接口，需要改成兼容模型或单独配自定义 provider。

## Assumptions
- `LM Studio` 的 Local Server 已开启；这台机器当前已经满足这个前提。
- 推荐先用 `minimax-m2.7@q3_k_s` 做首轮验证，稳定后再决定是否切到 `minimax-m2.7@q3_k_xl`。
- 如果你只是偶尔想用本地模型，不要改默认配置，直接用一次性命令更稳妥。
