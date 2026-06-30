use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn progress_placeholder_shows_recent_progress_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.show_welcome_banner = false;
    chat.set_recent_progress_summary(Some("Applied code changes".to_string()));

    let height = chat.desired_height(/*width*/ 80);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height))
        .expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw chat widget");
    assert_chatwidget_snapshot!(
        "progress_placeholder_shows_recent_progress",
        normalized_backend_snapshot(terminal.backend())
    );
}

#[tokio::test]
async fn hook_summary_placeholder_uses_cleaned_summary_input_after_turn_complete() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let mut run = hook_run(
        "stop-hook-1",
        AppServerHookEventName::Stop,
        AppServerHookRunStatus::Completed,
        "stop hook",
        Vec::new(),
    );
    run.summary_input = Some(
        "Implemented the local summary path.\n```text\nsource: generated/noise.rs\n```".to_string(),
    );

    handle_hook_completed(&mut chat, run);
    handle_turn_completed(&mut chat, "turn-1", Some(10));

    assert_eq!(
        chat.hook_placeholder_summary.as_deref(),
        Some("摘要：Implemented the local summary path.")
    );
}

#[tokio::test]
async fn successful_exec_updates_recent_progress_summary() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let cwd = chat.config.cwd.clone();

    handle_exec_end(
        &mut chat,
        AppServerThreadItem::CommandExecution {
            id: "exec-1".to_string(),
            command: "rg placeholder".to_string(),
            cwd: cwd.into(),
            process_id: None,
            source: ExecCommandSource::Agent,
            status: AppServerCommandExecutionStatus::Completed,
            command_actions: vec![AppServerCommandAction::Search {
                command: "rg placeholder".to_string(),
                query: Some("placeholder".to_string()),
                path: None,
            }],
            aggregated_output: None,
            exit_code: Some(0),
            duration_ms: Some(10),
        },
    );

    assert_eq!(
        chat.recent_progress_summary.as_deref(),
        Some("Searched for `placeholder`")
    );
}

#[tokio::test]
async fn failed_exec_keeps_recent_progress_summary() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_recent_progress_summary(Some("Applied code changes".to_string()));
    let cwd = chat.config.cwd.clone();

    handle_exec_end(
        &mut chat,
        AppServerThreadItem::CommandExecution {
            id: "exec-1".to_string(),
            command: "rg placeholder".to_string(),
            cwd: cwd.into(),
            process_id: None,
            source: ExecCommandSource::Agent,
            status: AppServerCommandExecutionStatus::Failed,
            command_actions: vec![AppServerCommandAction::Search {
                command: "rg placeholder".to_string(),
                query: Some("placeholder".to_string()),
                path: None,
            }],
            aggregated_output: None,
            exit_code: Some(1),
            duration_ms: Some(10),
        },
    );

    assert_eq!(
        chat.recent_progress_summary.as_deref(),
        Some("Applied code changes")
    );
}

#[tokio::test]
async fn status_header_updates_live_stage_summary() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.set_status_header("Tracing render path".to_string());

    assert_eq!(
        chat.live_stage_summary.as_deref(),
        Some("Tracing render path")
    );
}

#[tokio::test]
async fn turn_start_updates_live_stage_and_keeps_recent_progress() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_recent_progress_summary(Some("Applied code changes".to_string()));
    chat.set_status_header("Tracing render path".to_string());

    handle_turn_started(&mut chat, "turn-1");

    assert_eq!(chat.live_stage_summary, None);
    assert_eq!(
        chat.recent_progress_summary.as_deref(),
        Some("Applied code changes")
    );
}

#[tokio::test]
async fn successful_patch_updates_recent_progress_summary() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_file_change_completed_now(AppServerThreadItem::FileChange {
        id: "patch-1".to_string(),
        changes: Vec::new(),
        status: AppServerPatchApplyStatus::Completed,
    });

    assert_eq!(
        chat.recent_progress_summary.as_deref(),
        Some("Applied code changes")
    );
}

#[tokio::test]
async fn recent_progress_survives_turn_complete_after_status_resets() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_recent_progress_summary(Some("Applied code changes".to_string()));
    chat.set_status_header("Tracing render path".to_string());

    handle_turn_started(&mut chat, "turn-1");
    chat.set_status_header("Tracing render path".to_string());
    handle_turn_completed(&mut chat, "turn-1", Some(10));

    assert_eq!(chat.live_stage_summary, None);
    assert_eq!(
        chat.recent_progress_summary.as_deref(),
        Some("Applied code changes")
    );
}
