use serde_json::Value;

/// Remove schema metadata that cannot affect validation or tool dispatch.
/// The input is cloned so the canonical dispatch schema is never mutated.
pub fn minify_tool_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(object) => {
            let mut result = serde_json::Map::with_capacity(object.len());
            for (key, value) in object {
                if matches!(key.as_str(), "$schema" | "title" | "examples") {
                    continue;
                }
                result.insert(key.clone(), minify_tool_schema(value));
            }
            Value::Object(result)
        }
        Value::Array(values) => Value::Array(values.iter().map(minify_tool_schema).collect()),
        other => other.clone(),
    }
}

/// Normalize tool output without changing its meaning. Repeated blank lines
/// and immediately repeated identical lines are removed. Optional truncation
/// preserves both ends and emits an explicit marker.
pub fn compress_tool_result(text: &str, max_chars: usize) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut previous_line: Option<&str> = None;
    let mut blank_run = 0usize;
    for line in text.lines() {
        let line = line.trim_end();
        if previous_line == Some(line) {
            continue;
        }
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        if !normalized.is_empty() {
            normalized.push('\n');
        }
        normalized.push_str(line);
        previous_line = Some(line);
    }
    if max_chars == 0 || normalized.chars().count() <= max_chars {
        return normalized;
    }
    let marker = "\n\n[tool output truncated by DX token optimization]\n\n";
    if max_chars <= marker.chars().count() + 2 {
        return normalized.chars().take(max_chars).collect();
    }
    let available = max_chars - marker.chars().count();
    let head = available / 2;
    let tail = available - head;
    let head_text: String = normalized.chars().take(head).collect();
    let tail_text: String = normalized
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{head_text}{marker}{tail_text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_minifier_keeps_validation_fields() {
        let input = serde_json::json!({
            "$schema": "ignored",
            "title": "Input",
            "type": "object",
            "properties": {"path": {"type": "string", "examples": ["a"]}},
            "required": ["path"]
        });
        let output = minify_tool_schema(&input);
        assert!(output.get("$schema").is_none());
        assert!(output.get("title").is_none());
        assert!(output["properties"]["path"].get("examples").is_none());
        assert_eq!(output["required"], serde_json::json!(["path"]));
    }

    #[test]
    fn result_compression_removes_only_repetition_without_limit() {
        assert_eq!(compress_tool_result("a\n\na\n\n\nb", 0), "a\n\na\n\nb");
    }

    #[test]
    fn result_truncation_keeps_head_tail_and_marker() {
        let output = compress_tool_result(&format!("head\n{}\ntail", "0123456789".repeat(20)), 80);
        assert!(output.contains("head"));
        assert!(output.contains("tail"));
        assert!(output.contains("truncated"));
    }
}
