# Web Demo (Jupyter-style)

This demo provides a minimal notebook-like UI with:

- Start/Stop buttons for `http_bridge`
- A simple chat area with multi-turn context
- Local history via `localStorage`

## Run

From the repository root:

```bash
node codex-rs/app-server-test-client/web-demo/bridge-helper.js
```

Then open:

```
http://127.0.0.1:7100
```

## Notes

- The demo uses `localStorage` to keep a list of thread IDs and messages.
- The helper proxies `/api/*` to `http_bridge` so the browser avoids CORS issues.
- The Start button spawns `http_bridge` via `cargo run`.
