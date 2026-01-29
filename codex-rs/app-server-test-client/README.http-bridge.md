# Codex App-Server HTTP Bridge (MVP)

This is a minimal HTTP wrapper around `codex app-server` that exposes a small
JSON API plus SSE streaming for model output.

## Build & Run

```bash
cd /home/ivan/github/codex/codex-rs
cargo run -p codex-app-server-test-client --bin http_bridge -- --listen 127.0.0.1:7000
```

Optional: pass config overrides to the Codex CLI:

```bash
cargo run -p codex-app-server-test-client --bin http_bridge -- \
  --listen 127.0.0.1:7000 \
  --config 'model=gpt-5.1-codex'
```

## API Summary

- `POST /initialize`
- `POST /thread/start`
- `POST /thread/resume`
- `POST /turn/start` (SSE stream)

## Usage (curl)

Initialize (required once per server start):

```bash
curl -s -X POST http://127.0.0.1:7000/initialize
```

Start a new thread:

```bash
curl -s -X POST http://127.0.0.1:7000/thread/start \
  -H "Content-Type: application/json" \
  -d '{}'
```

Resume an existing thread:

```bash
curl -s -X POST http://127.0.0.1:7000/thread/resume \
  -H "Content-Type: application/json" \
  -d '{"threadId":"thr_xxx"}'
```

Start a turn (SSE stream):

```bash
curl -N -X POST http://127.0.0.1:7000/turn/start \
  -H "Content-Type: application/json" \
  -d '{"threadId":"thr_xxx","message":"你好，帮我总结这个项目"}'
```

The SSE stream emits events where the event name is the JSON-RPC `method`
and the data is the JSON-encoded notification. Example events:

- `turn/started`
- `item/started`
- `item/agentMessage/delta`
- `turn/completed`

## Notes

- Threads are persisted by the app-server; keep the returned `threadId` to
  continue the conversation later via `/thread/resume`.
- The bridge auto-accepts command and file-change approvals.
- This MVP assumes a single active turn at a time.

## Quick Test Script

```bash
#!/usr/bin/env bash
set -euo pipefail

BASE="http://127.0.0.1:7000"

curl -s -X POST "$BASE/initialize" >/dev/null
THREAD=$(curl -s -X POST "$BASE/thread/start" -H "Content-Type: application/json" -d '{}' | jq -r '.thread.id')
echo "threadId=$THREAD"

curl -N -X POST "$BASE/turn/start" \
  -H "Content-Type: application/json" \
  -d "{\"threadId\":\"$THREAD\",\"message\":\"Say hello\"}"
```

This script uses `jq` for parsing.
