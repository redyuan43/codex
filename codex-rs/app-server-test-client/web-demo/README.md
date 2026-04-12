# Web Demo (Jupyter-style)

This demo provides a minimal notebook-like UI with:

- Start/Stop buttons for `http_bridge`
- A simple chat area with multi-turn context
- Local history via `localStorage`

## Run (Recommended: Helper)

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

## Run (Manual http_bridge)

If you prefer to start `http_bridge` yourself, run:

```bash
RUST_LOG=debug cargo run -p codex-app-server-test-client --bin http_bridge -- --listen 127.0.0.1:7000
```

Then open `index.html` in a browser and use a local static server on 7100
to avoid CORS, for example:

```bash
python -m http.server 7100 --directory codex-rs/app-server-test-client/web-demo
```

Open:

```
http://127.0.0.1:7100
```

If you skip the helper, the Start/Stop buttons won't work (they call the helper's
endpoints). The rest of the UI still works as long as `http_bridge` is running
and reachable at `127.0.0.1:7000`.
