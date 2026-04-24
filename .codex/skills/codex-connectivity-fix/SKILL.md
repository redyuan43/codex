---
name: codex-connectivity-fix
description: 修复 Linux 机器上 Codex CLI “命令找不到 / 启动即崩 / 能登录但连不上 OpenAI / SSH 上去不能用” 这类连通性问题。适用于 `codex` 只装在 `~/.nvm/.../bin`、系统默认 Node 太老、`api.openai.com` 直连 DNS 被污染、以及本地 `xray/v2ray/clash` 代理已运行但 Codex 没继承代理环境的场景。
---

# Codex Connectivity Fix

当用户说“SSH 上去以后 `codex` 不能用”“`codex` 启动报 `Unexpected reserved word`”“OpenAI 连不上”“这台机器只能走本地代理”时，使用这个 skill。

这个 skill 解决四类高频问题：

- `codex` 已安装，但当前 shell 的 `PATH` 没带上 `~/.nvm/.../bin`
- 系统默认 `node` 太老，导致新版 Codex 启动即崩
- `api.openai.com` 直连不通，或 DNS 被污染解析到可疑 IP
- 本地 `xray/v2ray/clash` 已启动，但 Codex 没继承 `ALL_PROXY` / `HTTP(S)_PROXY`

## 标准工作流

1. 先诊断：

```bash
bash codex-connectivity-fix/scripts/fix_codex_connectivity.sh diagnose
```

重点看：

- `codex` 是否在当前 `PATH`
- 当前 shell 的 `node` 版本，以及 `~/.nvm` 下是否有更高版本 Node
- 是否能用正确的 Node 显式启动 Codex
- `~/.codex/auth.json` 是否存在
- `api.openai.com` 的 DNS 解析结果是否可疑
- 直连 `curl` 是否失败，而走本地代理是否成功

2. 若确认是“Codex 没继承正确 Node / 代理环境”，安装用户级包装器：

```bash
bash codex-connectivity-fix/scripts/fix_codex_connectivity.sh apply-wrapper
```

它会：

- 选择 `~/.nvm/versions/node/*` 下最新且可用的 Node
- 找到该 Node 对应的 `@openai/codex/bin/codex.js`
- 写入 `~/.local/bin/codex`
- 在检测到本地代理监听时，自动导出代理变量再启动 Codex

默认代理探测顺序：

- `127.0.0.1:10808` -> `socks5://127.0.0.1:10808`
- `127.0.0.1:7891` -> `socks5://127.0.0.1:7891`
- `127.0.0.1:7890` -> `http://127.0.0.1:7890`
- `127.0.0.1:20171` -> `http://127.0.0.1:20171`

3. 若用户已经明确要修复，可直接一键执行：

```bash
bash codex-connectivity-fix/scripts/fix_codex_connectivity.sh all
```

4. 如果本机代理端口不是默认值，显式传入：

```bash
bash codex-connectivity-fix/scripts/fix_codex_connectivity.sh apply-wrapper --proxy-url socks5://127.0.0.1:10809
```

## 经验规则

- `codex: command not found` 不等于没装；先查 `~/.nvm/versions/node/*/bin/codex`
- `Unexpected reserved word`、`node:path` 等错误，优先怀疑“新版 Codex + 老 Node”
- 若 `curl https://api.openai.com/...` 超时，但 `curl --socks5-hostname 127.0.0.1:10808 ...` 成功，说明核心问题是“代理没被 Codex 继承”
- 若 `api.openai.com` 解析到明显异常 IP，先按 DNS 污染处理，不要先怀疑账号
- 优先使用用户级 `~/.local/bin/codex` 包装器，不改系统级 `/usr/bin`，这样最稳、最容易回滚

## 验证

包装器写完后，验证：

```bash
bash -lc 'command -v codex; codex --version'
```

再做最小联网烟测：

```bash
bash -lc 'codex exec --skip-git-repo-check "Reply with exactly OK"'
```

若能返回 `OK`，说明：

- 命令解析正常
- Node 版本正常
- 认证文件可用
- OpenAI 连通性已恢复

## 安全规则

- 默认只改用户目录下的 `~/.local/bin/codex`，不改系统包、不重装 Node
- 不要默认修改系统 DNS；这属于更高风险网络变更，应在用户明确同意后再做
- 不要默认覆盖用户显式设置的 `ALL_PROXY` / `HTTP_PROXY` / `HTTPS_PROXY`
