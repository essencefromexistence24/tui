//! Minimal native tool registrations used alongside Dx Serializer Compact.
//!
//! The complete built-in contract is sent in the first prompt by
//! [`crate::prompt::dx_serializer_compact::TOOL_CATALOG`].

use serde_json::json;
use xai_grok_tools::types::definition::ToolDefinition;

/// Built-in tools described by the Dx Serializer Compact catalog.
const BUILTIN_TOOL_NAMES: &[&str] = &[
    "run_terminal_command", "read_file", "search_replace", "list_dir", "grep",
    "kill_command_or_subagent", "todo_write", "get_command_or_subagent_output",
    "spawn_subagent", "scheduler_create", "scheduler_delete", "scheduler_list", "monitor",
    "search_tool", "use_tool", "workflow", "enter_plan_mode", "exit_plan_mode",
    "ask_user_question", "web_search", "web_fetch", "image_gen", "image_edit", "image_to_video",
    "reference_to_video", "write",
];

/// Convert canonical definitions into compact native registrations.
///
/// Built-in schemas are represented by the prompt-side Dx Serializer Compact
/// catalog to avoid sending the full native JSON payload. Unknown definitions,
/// including MCP tools, remain intact.
pub fn compact_native_definitions(mut definitions: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    definitions.retain_mut(|definition| {
        if BUILTIN_TOOL_NAMES.contains(&definition.function.name.as_str()) {
            definition.function.description = None;
            definition.function.parameters = json!({"type": "object"});
        }
        true
    });
    definitions
}

#[cfg(test)]
mod tests {
    use super::{compact_native_definitions, BUILTIN_TOOL_NAMES};
    use xai_grok_tools::types::definition::ToolDefinition;

    #[test]
    fn catalog_has_all_26_unique_native_tools() {
        assert_eq!(BUILTIN_TOOL_NAMES.len(), 26);
        let mut names = BUILTIN_TOOL_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BUILTIN_TOOL_NAMES.len());
    }

    #[test]
    fn compact_native_definitions_uses_prompt_catalog_handles() {
        let definitions = vec![ToolDefinition::function(
            "run_terminal_command",
            Some("run a command"),
            serde_json::json!({
                "type": "object",
                "properties": {"command": {"type": "string"}},
                "required": ["command"]
            }),
        )];

        let output = compact_native_definitions(definitions);
        assert_eq!(output[0].function.parameters, json!({"type": "object"}));
        assert_eq!(output[0].function.description, None);
    }
}
