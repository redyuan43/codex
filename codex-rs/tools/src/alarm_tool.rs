//! Responses API tool specs for thread-local persistent alarm management.
//!
//! These specs expose the `AlarmCreate`, `AlarmDelete`, and `AlarmList`
//! built-in tools so models can create, inspect, and delete alarms on the
//! current thread.

use crate::AdditionalProperties;
use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

pub fn create_alarm_create_tool() -> ToolSpec {
    let trigger_properties = BTreeMap::from([
        (
            "kind".to_string(),
            JsonSchema::string(Some(
                "Trigger kind. Use `delay` or `schedule`.".to_string(),
            )),
        ),
        (
            "seconds".to_string(),
            JsonSchema::number(Some(
                "Delay trigger seconds from creation time.".to_string(),
            )),
        ),
        (
            "repeat".to_string(),
            JsonSchema::boolean(Some(
                "Delay trigger recurrence flag. With seconds 0, repeat means run whenever the thread is idle."
                    .to_string(),
            )),
        ),
        (
            "dtstart".to_string(),
            JsonSchema::string(Some(
                "Schedule trigger floating local datetime in YYYY-MM-DDTHH:MM:SS format."
                    .to_string(),
            )),
        ),
        (
            "rrule".to_string(),
            JsonSchema::string(Some("Schedule trigger RRULE string.".to_string())),
        ),
    ]);
    let properties = BTreeMap::from([
        (
            "trigger".to_string(),
            JsonSchema::object(
                trigger_properties,
                Some(vec!["kind".to_string()]),
                Some(AdditionalProperties::Boolean(false)),
            ),
        ),
        (
            "prompt".to_string(),
            JsonSchema::string(Some(
                "Prompt to execute when the alarm fires.".to_string(),
            )),
        ),
        (
            "delivery".to_string(),
            JsonSchema::string(Some(
                "Delivery mode for the alarm. Use `after-turn` or `steer-current-turn`."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "AlarmCreate".to_string(),
        description: "Create a thread alarm using a structured trigger, prompt, and delivery mode."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec![
                "trigger".to_string(),
                "prompt".to_string(),
                "delivery".to_string(),
            ]),
            Some(AdditionalProperties::Boolean(false)),
        ),
        output_schema: None,
    })
}

pub fn create_alarm_delete_tool() -> ToolSpec {
    let properties = BTreeMap::from([(
        "id".to_string(),
        JsonSchema::string(Some("Identifier of the alarm to delete.".to_string())),
    )]);

    ToolSpec::Function(ResponsesApiTool {
        name: "AlarmDelete".to_string(),
        description: "Delete a thread alarm by id.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["id".to_string()]),
            Some(AdditionalProperties::Boolean(false)),
        ),
        output_schema: None,
    })
}

pub fn create_alarm_list_tool() -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: "AlarmList".to_string(),
        description: "List thread alarms for the current thread.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            BTreeMap::new(),
            None,
            Some(AdditionalProperties::Boolean(false)),
        ),
        output_schema: None,
    })
}

#[cfg(test)]
#[path = "alarm_tool_tests.rs"]
mod tests;
