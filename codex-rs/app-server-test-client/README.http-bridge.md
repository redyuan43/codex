# Codex App-Server HTTP Bridge (MVP)

This is a minimal HTTP wrapper around `codex app-server` that exposes a small
JSON API plus SSE streaming for model output.

## Install

- Rust toolchain (stable) with `cargo`
- `jq` for the shell examples below

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
- `POST /tool/answer`

## Usage (curl)

Initialize (required once per server start):

```bash
curl -s -X POST http://127.0.0.1:7000/initialize
```

Start a new thread (optionally set working directory or model):

```bash
curl -s -X POST http://127.0.0.1:7000/thread/start \
  -H "Content-Type: application/json" \
  -d '{"cwd":"/home/ivan/github/codex","model":"gpt-5.2-codex"}'
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
and the data is the JSON-encoded notification. The first event is a custom
`turn_start` event containing the initial response payload. Example events:

- `turn_start`
- `turn/started`
- `item/started`
- `item/agentMessage/delta`
- `turn/completed`
- `item/tool/requestUserInput`

If you receive `item/tool/requestUserInput`, answer it with:

```bash
curl -s -X POST http://127.0.0.1:7000/tool/answer \
  -H "Content-Type: application/json" \
  -d '{
    "requestId":"<request_id_from_event>",
    "answers":{
      "question-id-1":["your answer"],
      "question-id-2":["choice A","choice B"]
    }
  }'
```

## Notes

- Threads are persisted by the app-server; keep the returned `threadId` to
  continue the conversation later via `/thread/resume`.
- The bridge auto-accepts command and file-change approvals.
- This MVP assumes a single active turn at a time.

## Frontend Usage (Fetch + SSE)

`POST /turn/start` streams SSE. In browsers, use `fetch` + `ReadableStream`
to parse `event:` and `data:` lines (this is a POST, so `EventSource` is not
available).

```js
async function startTurn({ threadId, message }) {
  const res = await fetch("http://127.0.0.1:7000/turn/start", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ threadId, message }),
  });

  if (!res.ok || !res.body) throw new Error("turn/start failed");

  const reader = res.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });

    let idx;
    while ((idx = buffer.indexOf("\n\n")) !== -1) {
      const chunk = buffer.slice(0, idx);
      buffer = buffer.slice(idx + 2);

      const lines = chunk.split("\n");
      const eventLine = lines.find((l) => l.startsWith("event:"));
      const dataLine = lines.find((l) => l.startsWith("data:"));
      if (!eventLine || !dataLine) continue;

      const event = eventLine.slice("event:".length).trim();
      const data = JSON.parse(dataLine.slice("data:".length).trim());

      if (event === "item/agentMessage/delta") {
        const delta = data.params?.delta ?? "";
        // append to UI
      }

      if (event === "item/tool/requestUserInput") {
        // render structured questions from data.params.questions
        // then POST answers to /tool/answer
      }
    }
  }
}
```

Answering tool input:

```js
async function answerToolRequest(requestId, answers) {
  await fetch("http://127.0.0.1:7000/tool/answer", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ requestId, answers }),
  });
}
```

## Web Demo (Jupyter-style)

A minimal UI is included at `codex-rs/app-server-test-client/web-demo`.
It uses a small local helper to start/stop `http_bridge` and proxy `/api/*`
requests to avoid CORS issues.

Run the helper:

```bash
node codex-rs/app-server-test-client/web-demo/bridge-helper.js
```

Then open:

```
http://127.0.0.1:7100
```

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
