pub fn try_extract_concatenated_json_objects(arguments: &str) -> Option<Vec<serde_json::Value>> {
    let trimmed = arguments.trim();

    // Quick check: must start with '{'.
    if !trimmed.starts_with('{') {
        return None;
    }

    // If it parses as valid JSON already, no recovery needed.
    if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
        return None;
    }

    // Use serde_json::StreamDeserializer to parse concatenated JSON objects.
    // This handles nested braces correctly (unlike naive string splitting on "}{").
    let stream = serde_json::Deserializer::from_str(trimmed).into_iter::<serde_json::Value>();

    let mut objects = Vec::new();
    for result in stream {
        match result {
            Ok(value) if value.is_object() => objects.push(value),
            _ => break,
        }
    }

    // Need at least 2 objects for this to be concatenated JSON.
    if objects.len() >= 2 {
        Some(objects)
    } else {
        None
    }
}

/// Normalize empty tool call arguments to `"{}"`.
///
/// Zero-arg MCP tools (e.g. `get_me`) sometimes receive `""` from the model
/// instead of `"{}"`, which fails JSON parsing. This normalizes empty/whitespace
/// strings to `"{}"` so downstream parsing succeeds.
pub fn normalize_empty_arguments(arguments: &str) -> &str {
    if arguments.trim().is_empty() {
        "{}"
    } else {
        arguments
    }
}

/// Normalize the small set of argument shapes that model transports commonly
/// serialize incorrectly before they reach the typed tool inputs.
///
/// Tool arguments are supposed to be a JSON object whose values retain their
/// schema types. Some providers/harnesses instead encode an array as a JSON
/// string (`"[ {...} ]"`), or encode a number/boolean as a string. The typed
/// tool structs correctly reject those values, but rejecting them at this
/// boundary makes otherwise valid tool calls fail before dispatch.
///
/// This is deliberately schema-aware rather than recursively parsing every
/// string that happens to contain JSON. A command, file path, prompt, or other
/// free-form string must never be silently changed just because it resembles
/// JSON. The function is idempotent and leaves unknown fields untouched.
pub fn normalize_tool_arguments(
    tool_name: &str,
    mut arguments: serde_json::Value,
) -> serde_json::Value {
    let Some(object) = arguments.as_object_mut() else {
        return arguments;
    };

    // These fields are arrays in the public tool contracts. In particular,
    // nested object arrays (todos/questions) must be decoded before serde
    // tries to deserialize the typed input.
    let array_fields: &[&str] = match tool_name {
        "todo_write" => &["todos"],
        "get_command_or_subagent_output" | "get_task_output" | "wait_tasks" => &["task_ids"],
        "ask_user_question" => &["questions"],
        "image_edit" => &["image"],
        "reference_to_video" => &["images"],
        "web_search" => &["allowed_domains"],
        _ => &[],
    };
    for field in array_fields {
        decode_stringified_array(object, field);
    }

    // Numeric strings are accepted only for fields whose schemas are numeric.
    // Interval remains a duration string ("5m", "2h", ...), so it is not
    // included here.
    let numeric_fields: &[&str] = match tool_name {
        "run_terminal_command" => &["timeout"],
        "read_file" => &["offset", "limit"],
        "grep" => &["-B", "-A", "-C", "head_limit"],
        "get_command_or_subagent_output" | "get_task_output" | "wait_tasks" => &["timeout_ms"],
        "monitor" => &["timeout_ms"],
        "search_tool" => &["limit"],
        "workflow" => &["agent_budget"],
        "image_to_video" | "reference_to_video" => &["duration"],
        _ => &[],
    };
    for field in numeric_fields {
        decode_stringified_number(object, field);
    }

    let boolean_fields: &[&str] = match tool_name {
        "run_terminal_command" => &["background"],
        "search_replace" => &[
            "replace_all",
            "skip_read_before_edit",
            "empty_old_string_does_not_override",
            "unicode_normalized_fallback",
            "include_user_edit_hint",
        ],
        "grep" => &["-i", "multiline"],
        "todo_write" => &["merge"],
        "spawn_subagent" => &["background"],
        "scheduler_create" => &["durable", "foreground", "fire_immediately"],
        "monitor" => &["persistent"],
        "workflow" => &["validate_only"],
        _ => &[],
    };
    for field in boolean_fields {
        decode_stringified_boolean(object, field);
    }

    // `multi_select` is a boolean inside each ask_user_question item, not a
    // top-level argument.
    if tool_name == "ask_user_question"
        && let Some(serde_json::Value::Array(questions)) = object.get_mut("questions")
    {
        for question in questions {
            if let serde_json::Value::Object(question) = question {
                decode_stringified_boolean(question, "multi_select");
            }
        }
    }

    arguments
}

fn decode_stringified_array(object: &mut serde_json::Map<String, serde_json::Value>, field: &str) {
    let Some(serde_json::Value::String(raw)) = object.get(field) else {
        return;
    };
    let Ok(decoded) = serde_json::from_str::<serde_json::Value>(raw) else {
        return;
    };
    if decoded.is_array() {
        object.insert(field.to_owned(), decoded);
    }
}

fn decode_stringified_number(object: &mut serde_json::Map<String, serde_json::Value>, field: &str) {
    let Some(serde_json::Value::String(raw)) = object.get(field) else {
        return;
    };
    let raw = raw.trim();
    let decoded = raw
        .parse::<i64>()
        .map(serde_json::Number::from)
        .ok()
        .or_else(|| raw.parse::<u64>().map(serde_json::Number::from).ok())
        .or_else(|| {
            raw.parse::<f64>()
                .ok()
                .and_then(serde_json::Number::from_f64)
        });
    if let Some(number) = decoded {
        object.insert(field.to_owned(), serde_json::Value::Number(number));
    }
}

fn decode_stringified_boolean(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) {
    let Some(serde_json::Value::String(raw)) = object.get(field) else {
        return;
    };
    let decoded = match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    };
    if let Some(value) = decoded {
        object.insert(field.to_owned(), serde_json::Value::Bool(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_objects() {
        let args = r#"{"target_file": "a.java"}{"target_file": "b.java"}{"target_file": "c.java"}"#;
        let objects = try_extract_concatenated_json_objects(args).unwrap();
        assert_eq!(objects.len(), 3);
        assert_eq!(objects[0]["target_file"], "a.java");
    }

    #[test]
    fn test_no_extract_for_valid_single_object() {
        assert!(
            try_extract_concatenated_json_objects(r#"{"target_file": "src/main.rs"}"#).is_none()
        );
    }

    #[test]
    fn test_no_extract_for_valid_object_with_braces_in_value() {
        assert!(
            try_extract_concatenated_json_objects(r#"{"command": "echo '}{' && ls"}"#).is_none()
        );
    }

    #[test]
    fn test_no_extract_for_array() {
        assert!(
            try_extract_concatenated_json_objects(
                r#"[{"target_file": "a.java"}, {"target_file": "b.java"}]"#
            )
            .is_none()
        );
    }

    #[test]
    fn test_no_extract_for_empty_or_non_json() {
        assert!(try_extract_concatenated_json_objects("").is_none());
        assert!(try_extract_concatenated_json_objects("not json").is_none());
    }

    #[test]
    fn test_extract_with_nested_braces() {
        let args = r#"{"file": "a.rs", "opts": {"line": 1}}{"file": "b.rs", "opts": {"line": 2}}"#;
        let objects = try_extract_concatenated_json_objects(args).unwrap();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0]["opts"]["line"], 1);
    }

    #[test]
    fn test_extract_with_whitespace_between_objects() {
        let objects = try_extract_concatenated_json_objects(r#"{"a": 1} {"b": 2}"#).unwrap();
        assert_eq!(objects.len(), 2);
    }

    #[test]
    fn test_extract_real_world_20_files() {
        let mut args = String::new();
        for i in 0..20 {
            args.push_str(&format!(r#"{{"target_file": "src/File{i}.java"}}"#));
        }
        let objects = try_extract_concatenated_json_objects(&args).unwrap();
        assert_eq!(objects.len(), 20);
    }

    #[test]
    fn test_no_extract_for_truncated_json() {
        assert!(try_extract_concatenated_json_objects(r#"{"a": 1} garbage"#).is_none());
    }

    /// Parse after normalizing — mirrors the production pattern in handle_tool_call.
    fn normalize_and_parse(arguments: &str) -> serde_json::Value {
        let normalized = normalize_empty_arguments(arguments);
        serde_json::from_str(normalized).unwrap_or_else(|_| serde_json::json!({"raw": arguments}))
    }

    #[test]
    fn empty_string_becomes_empty_object() {
        assert_eq!(normalize_and_parse(""), serde_json::json!({}));
    }

    #[test]
    fn whitespace_only_becomes_empty_object() {
        assert_eq!(normalize_and_parse("   "), serde_json::json!({}));
        assert_eq!(normalize_and_parse("\n\t"), serde_json::json!({}));
    }

    #[test]
    fn valid_json_unchanged() {
        assert_eq!(
            normalize_and_parse(r#"{"query": "test"}"#),
            serde_json::json!({"query": "test"})
        );
    }

    #[test]
    fn empty_object_string_unchanged() {
        assert_eq!(normalize_and_parse("{}"), serde_json::json!({}));
    }

    #[test]
    fn invalid_json_falls_back_to_raw() {
        let result = normalize_and_parse("not json");
        assert_eq!(result["raw"], "not json");
    }

    #[test]
    fn complex_args_with_arrays_unchanged() {
        let args = r#"{"pages": [{"title": "Test"}], "limit": 10}"#;
        let result = normalize_and_parse(args);
        assert!(result["pages"].is_array());
        assert_eq!(result["limit"], 10);
    }

    #[test]
    fn normalize_empty_returns_braces() {
        assert_eq!(normalize_empty_arguments(""), "{}");
        assert_eq!(normalize_empty_arguments("   "), "{}");
        assert_eq!(normalize_empty_arguments("\n\t"), "{}");
    }

    #[test]
    fn normalize_non_empty_passthrough() {
        assert_eq!(normalize_empty_arguments(r#"{"a":1}"#), r#"{"a":1}"#);
        assert_eq!(normalize_empty_arguments("not json"), "not json");
    }

    #[test]
    fn normalizes_stringified_array_arguments_without_touching_free_form_text() {
        let normalized = normalize_tool_arguments(
            "todo_write",
            json!({
                "todos": r#"[{"id":"one","content":"x","status":"pending"}]"#,
                "description": "[keep this as text]"
            }),
        );
        assert!(normalized["todos"].is_array());
        assert_eq!(normalized["description"], "[keep this as text]");
    }

    #[test]
    fn normalizes_stringified_nested_question_array_and_boolean() {
        let normalized = normalize_tool_arguments(
            "ask_user_question",
            json!({
                "questions": r#"[{"question":"Pick one","options":[],"multi_select":"false"}]"#
            }),
        );
        assert!(normalized["questions"].is_array());
        assert_eq!(normalized["questions"][0]["multi_select"], false);
    }

    #[test]
    fn normalizes_stringified_numbers_and_booleans_by_schema_field() {
        let normalized = normalize_tool_arguments(
            "monitor",
            json!({"command":"echo ok","description":"test","timeout_ms":"1200","persistent":"true"}),
        );
        assert_eq!(normalized["timeout_ms"], 1200);
        assert_eq!(normalized["persistent"], true);
    }

    #[test]
    fn leaves_duration_intervals_and_unknown_fields_unchanged() {
        let normalized = normalize_tool_arguments(
            "scheduler_create",
            json!({"interval":"60m","prompt":"run","metadata":"{\"keep\":true}"}),
        );
        assert_eq!(normalized["interval"], "60m");
        assert_eq!(normalized["metadata"], "{\"keep\":true}");
    }
}
