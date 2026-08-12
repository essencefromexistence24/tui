//! Model-facing `Dx Serializer Compact` tool-schema transport.

use serde_json::json;
use xai_grok_tools::types::definition::ToolDefinition;

/// First-turn instructions and compact schemas for Dx's built-in tools.
pub const CATALOG: &str = include_str!("../../templates/dx_serializer_compact_tools.md");

/// Tools whose complete schemas are represented in [`CATALOG`].
pub const TOOL_NAMES: &[&str] = &[
    "run_terminal_command",
    "read_file",
    "search_replace",
    "list_dir",
    "grep",
    "kill_command_or_subagent",
    "todo_write",
    "get_command_or_subagent_output",
    "spawn_subagent",
    "scheduler_create",
    "scheduler_delete",
    "scheduler_list",
    "monitor",
    "search_tool",
    "use_tool",
    "workflow",
    "enter_plan_mode",
    "exit_plan_mode",
    "ask_user_question",
    "web_search",
    "web_fetch",
    "image_gen",
    "image_edit",
    "image_to_video",
    "reference_to_video",
    "write",
];

/// Whether a canonical tool definition is fully described by [`CATALOG`].
pub fn contains_tool(name: &str) -> bool {
    TOOL_NAMES.contains(&name)
}

/// Replace only the model-facing schema with a minimal JSON-call envelope.
///
/// The registry retains its canonical definition and performs typed argument
/// deserialization, validation, authorization, hooks, and dispatch as usual.
pub fn use_compact_transport(definition: &mut ToolDefinition) {
    if !contains_tool(&definition.function.name) {
        return;
    }
    definition.function.description = None;
    definition.function.parameters = json!({
        "type": "object",
        "additionalProperties": true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_the_exact_brand_and_declared_tool_count() {
        assert!(CATALOG.contains("format=\"Dx Serializer Compact\""));
        assert!(CATALOG.contains("26[name description parameters(required type properties)]"));
        assert_eq!(TOOL_NAMES.len(), 26);
        for name in TOOL_NAMES {
            assert!(
                CATALOG.lines().any(|line| line.starts_with(name)),
                "missing compact schema for {name}"
            );
        }
    }

    #[test]
    fn transport_keeps_name_and_uses_json_object_envelope() {
        let mut definition = ToolDefinition::function(
            "read_file",
            Some("Read a file"),
            json!({"type": "object", "properties": {"target_file": {"type": "string"}}}),
        );
        use_compact_transport(&mut definition);
        assert_eq!(definition.function.name, "read_file");
        assert_eq!(definition.function.description, None);
        assert_eq!(
            definition.function.parameters,
            json!({"type": "object", "additionalProperties": true})
        );
    }
}
