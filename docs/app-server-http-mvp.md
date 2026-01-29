# App-Server HTTP Bridge MVP (Design Notes)

## Goal
- Provide an HTTP service that forwards requests to `codex app-server` (JSON-RPC over stdio).
- Support streaming responses (SSE) and multi-turn conversations.
- Avoid client-side context management by using app-server thread persistence.

## Scope (MVP)
- One long-lived `codex app-server` child process.
- Minimal HTTP API:
  - `POST /initialize`
  - `POST /thread/start`
  - `POST /thread/resume`
  - `POST /turn/start` (SSE stream)
- Streaming output via SSE (`text/event-stream`) with JSON payloads.
- Auto-approve command execution and file change requests.

## Non-Goals (MVP)
- Authentication or multi-tenant isolation.
- Multiple concurrent active turns.
- Robust process supervision / restart logic.
- Rich UI-specific item rendering.

## High-Level Architecture
```
Client (HTTP/SSE)
        |
        v
HTTP bridge (Rust, axum)
        |
        | JSON-RPC over stdio (JSONL)
        v
codex app-server (child process)
```

## Flow
1) Start HTTP bridge → spawn `codex app-server` with stdin/stdout pipes.
2) `/initialize`:
   - Send JSON-RPC `initialize` request.
   - Send `initialized` notification.
3) `/thread/start`:
   - Forward `thread/start` request (v2).
   - Return `threadId` and config echo from app-server.
4) `/thread/resume`:
   - Forward `thread/resume` request (v2).
5) `/turn/start` (SSE):
   - Forward `turn/start` request with `UserInput::Text`.
   - Subscribe to app-server notifications.
   - Stream notifications to client until `turn/completed` (matching turn id).

## Streaming Model (SSE)
- Each JSON-RPC notification is serialized and sent as an SSE event.
- Event `data` is a JSON string: `{ "method": "...", "params": ... }`.
- The stream ends when `turn/completed` for the matching turn id arrives.

## Context Management
- App-server persists thread history on disk.
- Client only stores `threadId` and uses `/thread/resume` on reconnect.
- No prompt concatenation or context stitching on the HTTP layer.

## Auto-Approvals (MVP)
- On `item/commandExecution/requestApproval`: respond with `Accept`.
- On `item/fileChange/requestApproval`: respond with `Accept`.

## Data/Concurrency Notes
- Single app-server child process (shared across all HTTP requests).
- MVP assumes at most one active turn at a time.
- HTTP server keeps a broadcast channel for notifications.

## Future Enhancements
- Per-client app-server instances.
- Concurrent turns with per-turn routing and filtering.
- Structured SSE payloads with typed events.
- Auth, rate limits, and process supervision.
