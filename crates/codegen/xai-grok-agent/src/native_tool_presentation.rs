//! Minimal native tool registrations used alongside Dx Serializer Compact.
//!
//! The complete built-in contract is sent once in the first user message as
//! [`crate::prompt::dx_serializer_compact::TOOL_CATALOG`]. Native API tool
//! registrations remain as lightweight name/dispatch handles so providers can
//! emit ordinary function calls. The canonical registry still validates every
//! argument and performs the actual dispatch; MCP definitions remain intact.

use serde_json::json;
use xai_grok_tools::types::definition::ToolDefinition;

/// The built-in tools described by the single Dx Serializer Compact catalog.
const BUILTIN_TOOL_NAMES: &[&str] = &[
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

/// Convert canonical definitions into the minimal native registration.
///
/// Built-in descriptions and parameter schemas are intentionally omitted here
/// because the complete, compact, model-readable schema is already present in
/// the Dx Serializer Compact catalog. Unknown definitions (including MCP
/// tools) are preserved so dynamically registered tools continue to work.
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
    use super::BUILTIN_TOOL_NAMES;

    #[test]
    fn catalog_has_all_26_unique_native_tools() {
        assert_eq!(BUILTIN_TOOL_NAMES.len(), 26);
        let mut names = BUILTIN_TOOL_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BUILTIN_TOOL_NAMES.len());
    }
}
