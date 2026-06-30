use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

const LOOP_USAGE: &str =
    "Usage: /loop <duration> <prompt>, /loop every <duration> <prompt>, or /loop stop [id]";
static NEXT_LOOP_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVE_LOOPS: LazyLock<Mutex<HashMap<LoopId, ActiveLoop>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LoopId(u64);

impl fmt::Display for LoopId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoopSpec {
    pub(crate) interval: Duration,
    pub(crate) prompt: String,
    pub(crate) repeat: bool,
}

struct ActiveLoop {
    spec: LoopSpec,
    handle: tokio::task::JoinHandle<()>,
}

pub(crate) fn parse_loop_spec(input: &str) -> Result<LoopSpec, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(LOOP_USAGE.to_string());
    }

    let (repeat, rest) = trimmed
        .strip_prefix("every ")
        .map(|rest| (true, rest.trim_start()))
        .unwrap_or((false, trimmed));
    let Some((duration, prompt)) = rest.split_once(char::is_whitespace) else {
        return Err(LOOP_USAGE.to_string());
    };
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Loop prompt must not be empty.".to_string());
    }

    Ok(LoopSpec {
        interval: parse_duration_token(duration)?,
        prompt: prompt.to_string(),
        repeat,
    })
}

pub(crate) fn schedule_loop(spec: LoopSpec, tx: AppEventSender) -> LoopId {
    let id = LoopId(NEXT_LOOP_ID.fetch_add(1, Ordering::Relaxed));
    let scheduled_spec = spec.clone();
    let handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(spec.interval).await;
            tx.send(AppEvent::SubmitUserMessage {
                text: spec.prompt.clone(),
            });
            if !spec.repeat {
                break;
            }
        }
        active_loops().remove(&id);
    });
    active_loops().insert(
        id,
        ActiveLoop {
            spec: scheduled_spec,
            handle,
        },
    );
    id
}

pub(crate) fn stop_all_loops() -> Vec<LoopId> {
    let handles = active_loops()
        .drain()
        .map(|(id, active_loop)| (id, active_loop.handle))
        .collect::<Vec<_>>();
    let mut ids = Vec::with_capacity(handles.len());
    for (id, handle) in handles {
        ids.push(id);
        handle.abort();
    }
    ids
}

pub(crate) fn stop_loop(id: LoopId) -> bool {
    let active_loop = active_loops().remove(&id);
    if let Some(active_loop) = active_loop {
        active_loop.handle.abort();
        true
    } else {
        false
    }
}

pub(crate) fn parse_loop_id(input: &str) -> Result<LoopId, String> {
    input
        .trim()
        .parse::<u64>()
        .map(LoopId)
        .map_err(|_| format!("Invalid /loop id '{input}'."))
}

pub(crate) fn active_loop_descriptions() -> Vec<String> {
    let mut descriptions = active_loops()
        .iter()
        .map(|(id, active_loop)| loop_description(*id, &active_loop.spec))
        .collect::<Vec<_>>();
    descriptions.sort();
    descriptions
}

fn active_loops() -> MutexGuard<'static, HashMap<LoopId, ActiveLoop>> {
    ACTIVE_LOOPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn loop_scheduled_message(id: LoopId, spec: &LoopSpec) -> String {
    let cadence = if spec.repeat { "recurring" } else { "one-shot" };
    let preposition = if spec.repeat { "every" } else { "in" };
    format!(
        "Scheduled {cadence} /loop #{id} {preposition} {}: {}",
        format_duration(spec.interval),
        spec.prompt
    )
}

fn loop_description(id: LoopId, spec: &LoopSpec) -> String {
    let cadence = if spec.repeat { "every" } else { "in" };
    format!(
        "#{id} {cadence} {}: {}",
        format_duration(spec.interval),
        spec.prompt
    )
}

fn parse_duration_token(token: &str) -> Result<Duration, String> {
    let split_at = token
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(token.len());
    let (number, unit) = token.split_at(split_at);
    if number.is_empty() {
        return Err(format!("Invalid /loop duration '{token}'."));
    }
    let value = number
        .parse::<u64>()
        .map_err(|_| format!("Invalid /loop duration '{token}'."))?;
    if value == 0 {
        return Err("/loop duration must be greater than zero.".to_string());
    }
    let seconds = match unit {
        "" | "s" | "sec" | "secs" => value,
        "m" | "min" | "mins" => value.saturating_mul(60),
        "h" | "hr" | "hrs" => value.saturating_mul(60 * 60),
        "d" | "day" | "days" => value.saturating_mul(24 * 60 * 60),
        _ => return Err(format!("Unsupported /loop duration unit '{unit}'.")),
    };
    Ok(Duration::from_secs(seconds))
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds.is_multiple_of(86_400) {
        format!("{}d", seconds / 86_400)
    } else if seconds.is_multiple_of(3_600) {
        format!("{}h", seconds / 3_600)
    } else if seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::time::timeout;

    #[test]
    fn parses_one_shot_loop_spec() {
        assert_eq!(
            parse_loop_spec("10m remind me").unwrap(),
            LoopSpec {
                interval: Duration::from_secs(/*secs*/ 600),
                prompt: "remind me".to_string(),
                repeat: false,
            }
        );
    }

    #[test]
    fn parses_recurring_loop_spec() {
        assert_eq!(
            parse_loop_spec("every 2h check status").unwrap(),
            LoopSpec {
                interval: Duration::from_secs(/*secs*/ 7_200),
                prompt: "check status".to_string(),
                repeat: true,
            }
        );
    }

    #[test]
    fn rejects_empty_prompt() {
        assert!(parse_loop_spec("10m").is_err());
    }

    #[test]
    fn parses_loop_id() {
        assert_eq!(parse_loop_id("42").unwrap(), LoopId(42));
    }

    #[test]
    fn describes_active_loop() {
        assert_eq!(
            loop_description(
                LoopId(7),
                &LoopSpec {
                    interval: Duration::from_secs(5),
                    prompt: "say hello".to_string(),
                    repeat: true,
                },
            ),
            "#7 every 5s: say hello"
        );
    }

    #[tokio::test]
    async fn scheduled_loop_sends_prompt_after_interval() {
        let (tx, mut rx) = unbounded_channel();
        let id = schedule_loop(
            LoopSpec {
                interval: Duration::from_millis(10),
                prompt: "check status".to_string(),
                repeat: false,
            },
            crate::app_event_sender::AppEventSender::new(tx),
        );

        let event = timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("scheduled event should arrive")
            .expect("app event channel should stay open");
        let AppEvent::SubmitUserMessage { text } = event else {
            panic!("expected scheduled user message event, got {event:?}");
        };
        assert_eq!(text, "check status");
        stop_loop(id);
    }
}
