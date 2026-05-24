#[cfg(not(test))]
use codex_app_server_protocol::HookEventName;
#[cfg(test)]
use codex_app_server_protocol::HookOutputEntry;
use codex_app_server_protocol::HookOutputEntryKind;
use codex_app_server_protocol::HookRunStatus;
use codex_app_server_protocol::HookRunSummary;
use serde::Deserialize;
#[cfg(not(test))]
use serde_json::json;
#[cfg(not(test))]
use std::env;
#[cfg(not(test))]
use std::sync::mpsc;
#[cfg(not(test))]
use std::time::Duration;
use tungstenite::Message;

#[cfg(not(test))]
const DEFAULT_LLM_URL: &str = "ws://agx.taild500c8.ts.net:18011/api/llm/chat";
#[cfg(not(test))]
const DEFAULT_LLM_MODEL: &str = "caps-voice-edit-qwen3-4b";
#[cfg(not(test))]
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    text: Option<String>,
    data: Option<ChatCompletionData>,
    success: Option<bool>,
    #[serde(default)]
    r#type: String,
    error: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionData {
    choices: Option<Vec<ChatCompletionChoice>>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: Option<ChatCompletionMessage>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionMessage {
    content: Option<String>,
}

pub(super) fn summarize_hook_run(run: &HookRunSummary) -> Option<String> {
    summarize_hook_run_with_remote(run, remote_hook_summary)
}

fn summarize_hook_run_with_remote(
    run: &HookRunSummary,
    remote_summary: impl FnOnce(&HookRunSummary) -> Option<String>,
) -> Option<String> {
    let remote = remote_summary(run);
    if remote.is_some() {
        return remote;
    }

    fallback_hook_summary(run)
}

#[cfg(test)]
fn remote_hook_summary(_run: &HookRunSummary) -> Option<String> {
    None
}

#[cfg(not(test))]
fn remote_hook_summary(run: &HookRunSummary) -> Option<String> {
    let body = hook_body(run);
    if body.trim().is_empty() {
        return None;
    }
    let url = env::var("CHECK_BOARDS_LLM_URL").unwrap_or_else(|_| DEFAULT_LLM_URL.to_string());
    let model =
        env::var("CHECK_BOARDS_LLM_MODEL").unwrap_or_else(|_| DEFAULT_LLM_MODEL.to_string());
    let timeout_ms = env::var("CHECK_BOARDS_LLM_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TIMEOUT_MS);
    let source_path = run.source_path.to_string_lossy().to_string();
    let status = format!("{:?}", run.status).to_lowercase();
    let payload = json!({
        "type": "chat",
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "你是 check_boards 的本地播报摘要器。只输出简体中文纯文本，不要 Markdown，不要前缀。用 1 到 2 句、80 个中文字以内，总结诊断结论、风险和下一步。不要复述设备名、文件夹名、目录路径或来源信息。"
            },
            {
                "role": "user",
                "content": format!(
                    "标题：{} hook\n设备：local\n目录：{}\n状态：{}\n内容：\n{}",
                    hook_event_name(run.event_name),
                    source_path,
                    status,
                    body
                )
            }
        ],
        "temperature": 0.1,
        "max_tokens": 256,
        "timeout_ms": timeout_ms,
        "stream": false
    });

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let result = chat_completion(&url, &payload.to_string());
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(Ok(summary)) => {
            let cleaned = clean_remote_summary(&summary);
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned)
            }
        }
        Ok(Err(_)) | Err(_) => None,
    }
}

fn chat_completion(url: &str, payload: &str) -> Result<String, String> {
    let (mut socket, _) = tungstenite::connect(url).map_err(|err| err.to_string())?;
    socket
        .send(Message::Text(payload.to_string().into()))
        .map_err(|err| err.to_string())?;
    loop {
        let message = socket.read().map_err(|err| err.to_string())?;
        let Message::Text(text) = message else {
            continue;
        };
        let response: ChatCompletionResponse =
            serde_json::from_str(&text).map_err(|err| err.to_string())?;
        if response.success == Some(false) || response.r#type == "error" {
            return Err(response
                .error
                .or(response.message)
                .unwrap_or_else(|| "小模型请求失败".to_string()));
        }
        if let Some(text) = response.text {
            return Ok(text);
        }
        if let Some(content) = response
            .data
            .and_then(|data| data.choices)
            .and_then(|choices| choices.into_iter().next())
            .and_then(|choice| choice.message)
            .and_then(|message| message.content)
        {
            return Ok(content);
        }
        return Err("小模型响应为空".to_string());
    }
}

fn fallback_hook_summary(run: &HookRunSummary) -> Option<String> {
    let selected = run
        .entries
        .iter()
        .find(|entry| entry.kind == HookOutputEntryKind::Stop)
        .or_else(|| {
            run.entries
                .iter()
                .find(|entry| entry.kind == HookOutputEntryKind::Error)
        })
        .or_else(|| {
            run.entries
                .iter()
                .find(|entry| entry.kind == HookOutputEntryKind::Feedback)
        })
        .or_else(|| {
            run.entries
                .iter()
                .find(|entry| entry.kind == HookOutputEntryKind::Warning)
        })
        .or_else(|| {
            run.entries
                .iter()
                .find(|entry| entry.kind == HookOutputEntryKind::Context)
        });

    let (kind, text) = match selected {
        Some(entry) => (Some(entry.kind), clean_hook_summary_text(&entry.text)),
        None => (
            None,
            run.status_message
                .as_deref()
                .map(clean_hook_summary_text)
                .unwrap_or_default(),
        ),
    };
    if text.is_empty() {
        return None;
    }

    let summary = match kind {
        Some(HookOutputEntryKind::Stop | HookOutputEntryKind::Error) => {
            format!("Hook 已拦截，需要处理：{text}")
        }
        Some(HookOutputEntryKind::Feedback) => {
            format!("Hook 给出下一步建议：{text}")
        }
        Some(HookOutputEntryKind::Warning) => {
            format!("Hook 提醒注意风险：{text}")
        }
        Some(HookOutputEntryKind::Context) => {
            if run.status == HookRunStatus::Completed {
                format!("Hook 补充上下文：{text}")
            } else {
                format!("Hook 状态需要关注：{text}")
            }
        }
        None => match run.status {
            HookRunStatus::Completed => format!("Hook 状态：{text}"),
            HookRunStatus::Failed | HookRunStatus::Blocked | HookRunStatus::Stopped => {
                format!("Hook 状态需要关注：{text}")
            }
            HookRunStatus::Running => format!("Hook 正在处理：{text}"),
        },
    };
    Some(limit_summary_chars(&summary, 90))
}

fn hook_body(run: &HookRunSummary) -> String {
    let mut lines = Vec::new();
    if let Some(status_message) = run.status_message.as_deref()
        && !status_message.trim().is_empty()
    {
        lines.push(format!("status：{status_message}"));
    }
    lines.extend(
        run.entries
            .iter()
            .map(|entry| format!("{}：{}", hook_output_kind_label(entry.kind), entry.text)),
    );
    lines.into_iter().collect::<Vec<_>>().join("\n")
}

fn hook_output_kind_label(kind: HookOutputEntryKind) -> &'static str {
    match kind {
        HookOutputEntryKind::Warning => "warning",
        HookOutputEntryKind::Stop => "stop",
        HookOutputEntryKind::Feedback => "feedback",
        HookOutputEntryKind::Context => "context",
        HookOutputEntryKind::Error => "error",
    }
}

#[cfg(not(test))]
fn hook_event_name(event_name: HookEventName) -> &'static str {
    match event_name {
        HookEventName::PreToolUse => "PreToolUse",
        HookEventName::PermissionRequest => "PermissionRequest",
        HookEventName::PostToolUse => "PostToolUse",
        HookEventName::PreCompact => "PreCompact",
        HookEventName::PostCompact => "PostCompact",
        HookEventName::SessionStart => "SessionStart",
        HookEventName::UserPromptSubmit => "UserPromptSubmit",
        HookEventName::SubagentStart => "SubagentStart",
        HookEventName::SubagentStop => "SubagentStop",
        HookEventName::Stop => "Stop",
    }
}

fn clean_remote_summary(value: &str) -> String {
    let without_thinking = remove_tagged_block(value, "think");
    let without_fences = remove_code_fences(&without_thinking);
    limit_summary_chars(
        &without_fences
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        180,
    )
}

fn clean_hook_summary_text(value: &str) -> String {
    let without_fences = remove_code_fences(value);
    let mut output = String::new();
    for line in without_fences.replace('\r', "").lines() {
        let trimmed = line.trim();
        let cleaned = trimmed
            .trim_start_matches(['#', '>', '-', '*', '•', ' '])
            .replace(['#', '*', '_', '`', '>'], "");
        if cleaned.is_empty() || looks_like_path_or_source(&cleaned) {
            continue;
        }
        if !output.is_empty() {
            output.push(' ');
        }
        output.push_str(&cleaned);
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn remove_tagged_block(value: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut remaining = value;
    let mut output = String::new();
    while let Some(start) = remaining.to_lowercase().find(&open) {
        output.push_str(&remaining[..start]);
        let after_open = &remaining[start + open.len()..];
        let Some(end) = after_open.to_lowercase().find(&close) else {
            remaining = "";
            break;
        };
        remaining = &after_open[end + close.len()..];
    }
    output.push_str(remaining);
    output
}

fn remove_code_fences(value: &str) -> String {
    let mut output = String::new();
    let mut in_code = false;
    for line in value.replace('\r', "").lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn looks_like_path_or_source(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("device:")
        || lower.starts_with("cwd:")
        || lower.starts_with("source:")
        || lower.starts_with("path:")
        || value.starts_with('/')
        || value.starts_with("~/")
}

fn limit_summary_chars(value: &str, max_chars: usize) -> String {
    let mut summary = String::new();
    for ch in value.chars().take(max_chars) {
        summary.push(ch);
    }
    if value.chars().count() > max_chars {
        summary.push('…');
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::PathBufExt;
    use crate::test_support::test_path_buf;
    use pretty_assertions::assert_eq;
    use std::net::TcpListener;
    use std::thread;
    use tungstenite::accept;

    #[test]
    fn uses_remote_summary_when_available() {
        let run = hook_run_summary(vec![HookOutputEntry {
            kind: HookOutputEntryKind::Warning,
            text: "fallback text that should not appear".to_string(),
        }]);

        assert_eq!(
            summarize_hook_run_with_remote(&run, |_| Some("远端模型语义摘要".to_string())),
            Some("远端模型语义摘要".to_string())
        );
    }

    #[test]
    fn chat_completion_reads_websocket_text_response() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind websocket listener");
        let url = format!(
            "ws://{}",
            listener.local_addr().expect("websocket listener address")
        );
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept websocket client");
            let mut socket = accept(stream).expect("accept websocket handshake");
            let message = socket.read().expect("read websocket payload");
            assert_eq!(
                message.to_text().expect("text payload"),
                r#"{"type":"chat"}"#
            );
            socket
                .send(Message::Text(
                    r#"{"success":true,"text":"远端模型语义摘要"}"#.to_string().into(),
                ))
                .expect("send websocket response");
        });

        assert_eq!(
            chat_completion(&url, r#"{"type":"chat"}"#),
            Ok("远端模型语义摘要".to_string())
        );
        server.join().expect("websocket server thread");
    }

    #[test]
    fn prioritizes_stop_over_warning() {
        let run = hook_run_summary(vec![
            HookOutputEntry {
                kind: HookOutputEntryKind::Warning,
                text: "Use Plan Mode first".to_string(),
            },
            HookOutputEntry {
                kind: HookOutputEntryKind::Stop,
                text: "prompt blocked".to_string(),
            },
        ]);

        assert_eq!(
            fallback_hook_summary(&run),
            Some("Hook 已拦截，需要处理：prompt blocked".to_string())
        );
    }

    #[test]
    fn cleans_markdown_code_and_source_noise() {
        let run = hook_run_summary(vec![HookOutputEntry {
            kind: HookOutputEntryKind::Warning,
            text: "device: mi\ncwd: /home/ivan/github/check_boards\n```log\nnoisy\n```\n- **先跑测试** 再继续"
                .to_string(),
        }]);

        assert_eq!(
            fallback_hook_summary(&run),
            Some("Hook 提醒注意风险：先跑测试 再继续".to_string())
        );
    }

    #[test]
    fn summarizes_status_message_when_entries_are_empty() {
        let mut run = hook_run_summary(Vec::new());
        run.status = HookRunStatus::Stopped;
        run.status_message = Some("Applying OMX prompt routing".to_string());

        assert_eq!(
            summarize_hook_run_with_remote(&run, |_| None),
            Some("Hook 状态需要关注：Applying OMX prompt routing".to_string())
        );
    }

    #[test]
    fn hook_body_includes_status_and_entries_for_remote_model() {
        let run = hook_run_summary(vec![HookOutputEntry {
            kind: HookOutputEntryKind::Warning,
            text: "go-workflow must start from PlanMode".to_string(),
        }]);

        assert_eq!(
            hook_body(&run),
            "status：checking input policy\nwarning：go-workflow must start from PlanMode"
        );
    }

    #[test]
    fn cleans_remote_summary_like_check_boards() {
        assert_eq!(
            clean_remote_summary(
                "<think>hidden</think>\n```md\nnoise\n```\n**需要先修复审批失败，再重试。**"
            ),
            "**需要先修复审批失败，再重试。**"
        );
    }

    fn hook_run_summary(entries: Vec<HookOutputEntry>) -> HookRunSummary {
        HookRunSummary {
            id: "user-prompt-submit:0:/tmp/hooks.json".to_string(),
            event_name: codex_app_server_protocol::HookEventName::UserPromptSubmit,
            handler_type: codex_app_server_protocol::HookHandlerType::Command,
            execution_mode: codex_app_server_protocol::HookExecutionMode::Sync,
            scope: codex_app_server_protocol::HookScope::Turn,
            source_path: test_path_buf("/tmp/hooks.json").abs(),
            source: codex_app_server_protocol::HookSource::User,
            display_order: 0,
            status: HookRunStatus::Stopped,
            status_message: Some("checking input policy".to_string()),
            started_at: 1,
            completed_at: Some(11),
            duration_ms: Some(10),
            entries,
        }
    }
}
