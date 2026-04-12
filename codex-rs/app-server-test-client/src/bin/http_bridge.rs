use std::collections::HashMap;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::net::SocketAddr;
use std::process::Child;
use std::process::ChildStdin;
use std::process::ChildStdout;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::Event;
use axum::response::sse::Sse;
use axum::routing::post;
use clap::ArgAction;
use clap::Parser;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientNotification;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalParams;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalParams;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::InitializeResponse;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

#[derive(Parser)]
#[command(author = "Codex", version, about = "HTTP bridge for codex app-server")]
struct Cli {
    /// Path to the `codex` CLI binary.
    #[arg(long, env = "CODEX_BIN", default_value = "codex")]
    codex_bin: String,

    /// Forwarded to the `codex` CLI as `--config key=value`. Repeatable.
    #[arg(
        short = 'c',
        long = "config",
        value_name = "key=value",
        action = ArgAction::Append
    )]
    config_overrides: Vec<String>,

    /// HTTP listen address, e.g. 127.0.0.1:7000
    #[arg(long, default_value = "127.0.0.1:7000")]
    listen: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let server = CodexAppServer::spawn(&cli.codex_bin, &cli.config_overrides)?;
    let state = AppState { server };

    let app = Router::new()
        .route("/initialize", post(initialize))
        .route("/thread/start", post(thread_start))
        .route("/thread/resume", post(thread_resume))
        .route("/turn/start", post(turn_start))
        .route("/tool/answer", post(tool_answer))
        .with_state(state);

    let addr: SocketAddr = cli.listen.parse().context("invalid listen address")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("failed to bind listener")?;
    axum::serve(listener, app)
        .await
        .context("http server exited")?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    server: Arc<CodexAppServer>,
}

struct CodexAppServer {
    stdin: Mutex<ChildStdin>,
    pending: Mutex<HashMap<RequestId, oneshot::Sender<Result<Value, String>>>>,
    events_tx: broadcast::Sender<BridgeEvent>,
    _child: Mutex<Child>,
}

#[derive(Clone, Debug)]
enum BridgeEvent {
    Notification(JSONRPCNotification),
    ToolRequest {
        request_id: RequestId,
        params: codex_app_server_protocol::ToolRequestUserInputParams,
    },
}

impl CodexAppServer {
    fn spawn(codex_bin: &str, config_overrides: &[String]) -> Result<Arc<Self>> {
        let mut cmd = Command::new(codex_bin);
        for override_kv in config_overrides {
            cmd.arg("--config").arg(override_kv);
        }

        let mut child = cmd
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to start `{codex_bin}` app-server"))?;

        let stdin = child
            .stdin
            .take()
            .context("codex app-server stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("codex app-server stdout unavailable")?;

        let (events_tx, _events_rx) = broadcast::channel(256);

        let server = Arc::new(Self {
            stdin: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            events_tx,
            _child: Mutex::new(child),
        });

        Self::spawn_reader_thread(Arc::clone(&server), stdout);
        Ok(server)
    }

    fn spawn_reader_thread(server: Arc<Self>, stdout: ChildStdout) {
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                let bytes = match reader.read_line(&mut line) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        eprintln!("app-server stdout read failed: {err}");
                        break;
                    }
                };

                if bytes == 0 {
                    eprintln!("app-server stdout closed");
                    break;
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let parsed: Value = match serde_json::from_str(trimmed) {
                    Ok(value) => value,
                    Err(err) => {
                        eprintln!("invalid JSON-RPC from app-server: {err}");
                        continue;
                    }
                };

                let message: JSONRPCMessage = match serde_json::from_value(parsed) {
                    Ok(message) => message,
                    Err(err) => {
                        eprintln!("invalid JSON-RPC shape: {err}");
                        continue;
                    }
                };

                match message {
                    JSONRPCMessage::Response(JSONRPCResponse { id, result }) => {
                        if let Some(tx) = server
                            .pending
                            .lock()
                            .ok()
                            .and_then(|mut pending| pending.remove(&id))
                        {
                            let _ = tx.send(Ok(result));
                        }
                    }
                    JSONRPCMessage::Error(JSONRPCError { id, error }) => {
                        if let Some(tx) = server
                            .pending
                            .lock()
                            .ok()
                            .and_then(|mut pending| pending.remove(&id))
                        {
                            let _ = tx.send(Err(error.message));
                        }
                    }
                    JSONRPCMessage::Notification(notification) => {
                        let _ = server
                            .events_tx
                            .send(BridgeEvent::Notification(notification));
                    }
                    JSONRPCMessage::Request(request) => {
                        if let Err(err) = server.handle_server_request(request) {
                            eprintln!("failed to handle server request: {err}");
                        }
                    }
                }
            }
        });
    }

    async fn initialize(&self) -> Result<InitializeResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::Initialize {
            request_id: request_id.clone(),
            params: InitializeParams {
                client_info: ClientInfo {
                    name: "codex_http_bridge".to_string(),
                    title: Some("Codex HTTP Bridge".to_string()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
            },
        };

        let response = self.send_request(request, request_id, "initialize").await?;
        self.send_notification(ClientNotification::Initialized)?;
        Ok(response)
    }

    async fn thread_start(&self, params: ThreadStartParams) -> Result<ThreadStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadStart {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/start").await
    }

    async fn thread_resume(&self, params: ThreadResumeParams) -> Result<ThreadResumeResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadResume {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "thread/resume")
            .await
    }

    async fn turn_start(&self, params: TurnStartParams) -> Result<TurnStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::TurnStart {
            request_id: request_id.clone(),
            params,
        };

        self.send_request(request, request_id, "turn/start").await
    }

    fn subscribe(&self) -> broadcast::Receiver<BridgeEvent> {
        self.events_tx.subscribe()
    }

    async fn send_request<T>(
        &self,
        request: ClientRequest,
        request_id: RequestId,
        method: &str,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| anyhow!("pending lock poisoned"))?
            .insert(request_id.clone(), tx);
        self.write_request(&request)?;
        let response = rx.await.context("app-server response channel closed")?;
        let response = response.map_err(|err| anyhow!(err))?;
        serde_json::from_value(response)
            .with_context(|| format!("{method} response missing payload"))
    }

    fn send_notification(&self, notification: ClientNotification) -> Result<()> {
        let message = serde_json::to_value(notification)?;
        let message: JSONRPCMessage = serde_json::from_value(message)?;
        self.write_jsonrpc_message(message)
    }

    fn handle_server_request(&self, request: JSONRPCRequest) -> Result<()> {
        let server_request = ServerRequest::try_from(request)
            .context("failed to deserialize ServerRequest from JSONRPCRequest")?;

        match server_request {
            ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
                self.handle_command_execution_request_approval(request_id, params)?;
            }
            ServerRequest::FileChangeRequestApproval { request_id, params } => {
                self.handle_file_change_request_approval(request_id, params)?;
            }
            ServerRequest::ToolRequestUserInput { request_id, params } => {
                let _ = self
                    .events_tx
                    .send(BridgeEvent::ToolRequest { request_id, params });
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_command_execution_request_approval(
        &self,
        request_id: RequestId,
        _params: CommandExecutionRequestApprovalParams,
    ) -> Result<()> {
        let response = CommandExecutionRequestApprovalResponse {
            decision: CommandExecutionApprovalDecision::Accept,
        };
        self.send_server_request_response(request_id, &response)
    }

    fn handle_file_change_request_approval(
        &self,
        request_id: RequestId,
        _params: FileChangeRequestApprovalParams,
    ) -> Result<()> {
        let response = FileChangeRequestApprovalResponse {
            decision: FileChangeApprovalDecision::Accept,
        };
        self.send_server_request_response(request_id, &response)
    }

    fn send_server_request_response<T>(&self, request_id: RequestId, response: &T) -> Result<()>
    where
        T: Serialize,
    {
        let message = JSONRPCMessage::Response(JSONRPCResponse {
            id: request_id,
            result: serde_json::to_value(response)?,
        });
        self.write_jsonrpc_message(message)
    }

    fn write_jsonrpc_message(&self, message: JSONRPCMessage) -> Result<()> {
        let payload = serde_json::to_string(&message)?;
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| anyhow!("stdin lock poisoned"))?;
        writeln!(stdin, "{payload}")?;
        stdin.flush().context("failed to flush request")?;
        Ok(())
    }

    fn write_request(&self, request: &ClientRequest) -> Result<()> {
        let payload = serde_json::to_string(request)?;
        let mut stdin = self
            .stdin
            .lock()
            .map_err(|_| anyhow!("stdin lock poisoned"))?;
        writeln!(stdin, "{payload}")?;
        stdin.flush().context("failed to flush request")?;
        Ok(())
    }

    fn request_id(&self) -> RequestId {
        RequestId::String(Uuid::new_v4().to_string())
    }
}

#[derive(Debug)]
struct AppError(anyhow::Error);

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0.to_string()).into_response()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadStartRequest {
    model: Option<String>,
    cwd: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThreadResumeRequest {
    thread_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnStartRequest {
    thread_id: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolAnswerRequest {
    request_id: Value,
    answers: HashMap<String, Vec<String>>,
}

async fn initialize(State(state): State<AppState>) -> Result<Json<InitializeResponse>, AppError> {
    let response = state.server.initialize().await?;
    Ok(Json(response))
}

async fn thread_start(
    State(state): State<AppState>,
    Json(request): Json<ThreadStartRequest>,
) -> Result<Json<ThreadStartResponse>, AppError> {
    let params = ThreadStartParams {
        model: request.model,
        model_provider: None,
        cwd: request.cwd,
        approval_policy: None,
        sandbox: None,
        config: None,
        base_instructions: None,
        developer_instructions: None,
        personality: None,
        ephemeral: None,
        dynamic_tools: None,
        experimental_raw_events: false,
    };

    let response = state.server.thread_start(params).await?;
    Ok(Json(response))
}

async fn thread_resume(
    State(state): State<AppState>,
    Json(request): Json<ThreadResumeRequest>,
) -> Result<Json<ThreadResumeResponse>, AppError> {
    let params = ThreadResumeParams {
        thread_id: request.thread_id,
        history: None,
        path: None,
        model: None,
        model_provider: None,
        cwd: None,
        approval_policy: None,
        sandbox: None,
        config: None,
        base_instructions: None,
        developer_instructions: None,
        personality: None,
    };

    let response = state.server.thread_resume(params).await?;
    Ok(Json(response))
}

async fn turn_start(
    State(state): State<AppState>,
    Json(request): Json<TurnStartRequest>,
) -> Result<Sse<ReceiverStream<Result<Event, std::convert::Infallible>>>, AppError> {
    let params = TurnStartParams {
        thread_id: request.thread_id.clone(),
        input: vec![UserInput::Text {
            text: request.message,
            text_elements: Vec::new(),
        }],
        ..Default::default()
    };

    let response = state.server.turn_start(params).await?;
    let turn_id = response.turn.id.clone();
    let thread_id = request.thread_id;

    let (tx, rx) = mpsc::channel(64);
    let initial = serde_json::to_string(&response)?;
    tx.send(Ok(Event::default().event("turn_start").data(initial)))
        .await
        .ok();

    let mut events = BroadcastStream::new(state.server.subscribe());
    tokio::spawn(async move {
        while let Some(item) = events.next().await {
            let Ok(notification) = item else {
                break;
            };

            let (method, payload, done) = match notification {
                BridgeEvent::Notification(notification) => {
                    let method = notification.method.clone();
                    let payload = serde_json::to_string(&notification).unwrap_or_else(|_| {
                        "{\"error\":\"failed to serialize notification\"}".to_string()
                    });
                    let done = is_turn_completed(&notification, &thread_id, &turn_id);
                    (method, payload, done)
                }
                BridgeEvent::ToolRequest { request_id, params } => {
                    let payload = serde_json::json!({
                        "method": "item/tool/requestUserInput",
                        "id": request_id,
                        "params": params,
                    });
                    (
                        "item/tool/requestUserInput".to_string(),
                        serde_json::to_string(&payload).unwrap_or_else(|_| {
                            "{\"error\":\"failed to serialize tool request\"}".to_string()
                        }),
                        false,
                    )
                }
            };

            let event = Event::default().event(method).data(payload);
            if tx.send(Ok(event)).await.is_err() {
                break;
            }

            if done {
                break;
            }
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)))
}

fn is_turn_completed(notification: &JSONRPCNotification, thread_id: &str, turn_id: &str) -> bool {
    let Ok(server_notification) = ServerNotification::try_from(notification.clone()) else {
        return false;
    };

    match server_notification {
        ServerNotification::TurnCompleted(payload) => {
            payload.thread_id == thread_id && payload.turn.id == turn_id
        }
        _ => false,
    }
}

async fn tool_answer(
    State(state): State<AppState>,
    Json(request): Json<ToolAnswerRequest>,
) -> Result<StatusCode, AppError> {
    let request_id: RequestId =
        serde_json::from_value(request.request_id).context("invalid requestId")?;
    let answers = request
        .answers
        .into_iter()
        .map(|(id, answers)| {
            (
                id,
                codex_app_server_protocol::ToolRequestUserInputAnswer { answers },
            )
        })
        .collect();
    let response = codex_app_server_protocol::ToolRequestUserInputResponse { answers };
    state
        .server
        .send_server_request_response(request_id, &response)?;
    Ok(StatusCode::NO_CONTENT)
}
