use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn progress_placeholder_shows_anchor_and_recent_progress_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.show_welcome_banner = false;
    chat.set_session_context_anchor(Some("修复输入框提示".to_string()));
    chat.set_recent_progress_summary(Some("Applied code changes".to_string()));

    let height = chat.desired_height(/*width*/ 80);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height))
        .expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw chat widget");
    assert_chatwidget_snapshot!(
        "progress_placeholder_shows_anchor_and_recent_progress",
        normalized_backend_snapshot(terminal.backend())
    );
}

#[tokio::test]
async fn short_chinese_task_message_seeds_anchor() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.maybe_seed_session_context_anchor("修复提示");

    assert_eq!(chat.session_context_anchor.as_deref(), Some("修复提示"));
}

#[tokio::test]
async fn short_operational_follow_up_does_not_override_anchor() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.maybe_seed_session_context_anchor("修复输入框提示");
    chat.maybe_seed_session_context_anchor("继续");

    assert_eq!(
        chat.session_context_anchor.as_deref(),
        Some("修复输入框提示")
    );
}

#[tokio::test]
async fn successful_exec_updates_recent_progress_summary() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_exec_end_now(ExecCommandEndEvent {
        call_id: "exec-1".to_string(),
        process_id: None,
        turn_id: "turn-1".to_string(),
        command: vec!["rg".to_string(), "placeholder".to_string()],
        cwd: PathBuf::from("/tmp"),
        parsed_cmd: vec![ParsedCommand::Search {
            cmd: "rg placeholder".to_string(),
            query: Some("placeholder".to_string()),
            path: None,
        }],
        source: ExecCommandSource::Agent,
        interaction_input: None,
        stdout: String::new(),
        stderr: String::new(),
        aggregated_output: String::new(),
        exit_code: 0,
        duration: Duration::from_millis(10),
        formatted_output: String::new(),
        status: CoreExecCommandStatus::Completed,
    });

    assert_eq!(
        chat.recent_progress_summary.as_deref(),
        Some("Searched for `placeholder`")
    );
}

#[tokio::test]
async fn recent_progress_survives_turn_complete_after_status_resets() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_session_context_anchor(Some("修复输入框提示".to_string()));
    chat.set_recent_progress_summary(Some("Applied code changes".to_string()));

    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            started_at: None,
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "**Tracing render path**".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
            completed_at: None,
            duration_ms: Some(Duration::from_millis(10).as_millis() as i64),
        }),
    });

    assert_eq!(chat.live_stage_summary, None);
    assert_eq!(
        chat.recent_progress_summary.as_deref(),
        Some("Applied code changes")
    );
}
