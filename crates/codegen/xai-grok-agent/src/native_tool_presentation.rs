//! Compact native tool registrations used alongside Dx Serializer Compact.
//!
//! The provider request must still receive the real parameter contract. A
//! prompt-side compact catalog is useful documentation, but `{}` parameters
//! are not a tool schema: providers cannot validate calls or reliably emit
//! arguments from them. This module projects the checked-in 26-entry native
//! catalog onto the enabled canonical definitions and leaves MCP definitions
//! untouched.

use serde_json::json;
use xai_grok_tools::types::definition::ToolDefinition;

/// Built-in tools described by the Dx Serializer Compact catalog.
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

/// Controls how built-in tools are presented to providers during the test
/// period. MCP definitions are never reduced by this switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeToolSchemaMode {
    /// Advertise only the callable names while the compact contract is off.
    NamesOnly,
    /// Advertise the complete native parameter schemas.
    Full,
}

/// Current experiment mode. Set to `Full` when the provider-schema work is
/// ready to be re-enabled.
pub const NATIVE_TOOL_SCHEMA_MODE: NativeToolSchemaMode = NativeToolSchemaMode::NamesOnly;

/// Convert enabled canonical definitions into the compact native registrations.
///
/// Only the schema payload is replaced. The enabled-tool list, plan-mode
/// filtering, tool aliases, and MCP definitions still come from the canonical
/// runtime registry. This prevents advertising a catalog entry that cannot be
/// dispatched while also preventing the old `{}`-schema regression.
pub fn native_registration_definitions(definitions: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    match NATIVE_TOOL_SCHEMA_MODE {
        NativeToolSchemaMode::NamesOnly => names_only_definitions(definitions),
        NativeToolSchemaMode::Full => {
            // Sumon Sir will work on it later.
            names_only_definitions(definitions)
        }
    }
}

fn names_only_definitions(definitions: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    definitions
        .into_iter()
        .map(|mut definition| {
            if BUILTIN_TOOL_NAMES.contains(&definition.function.name.as_str()) {
                definition.function.description = None;
                definition.function.parameters = json!({});
            }
            definition
        })
        .collect()
}

/// Backwards-compatible name for callers that used the old projection API.
/// It follows the active presentation mode.
pub fn compact_native_definitions(definitions: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    native_registration_definitions(definitions)
}

// Sumon Sir will work on it later.
// The previous full-schema implementation is intentionally retained here as
// the switch's future `Full` branch, but is disabled for this measurement:
//
// fn catalog_by_name() -> std::collections::HashMap<String, serde_json::Value> {
//     let entries: Vec<serde_json::Value> = serde_json::from_str(
//         &crate::prompt::native_tool_catalog::native_tool_definitions_json(),
//     ).expect("Dx native tool catalog must be valid JSON");
//     entries.into_iter().filter_map(|mut entry| {
//         let object = entry.as_object_mut()?;
//         let name = object.get("name")?.as_str()?.to_owned();
//         let mut parameters = object.remove("parameters")?;
//         complete_parameters_schema(&name, &mut parameters);
//         Some((name, parameters))
//     }).collect()
// }
//
// fn complete_parameters_schema(tool_name: &str, parameters: &mut serde_json::Value) {
//     let Some(object) = parameters.as_object_mut() else {
//         *parameters = json!({"type": "object", "properties": {}, "required": []});
//         return;
//     };
//     object.entry("type").or_insert_with(|| json!("object"));
//     object.entry("properties").or_insert_with(|| json!({}));
//     let required = object.get("required").and_then(serde_json::Value::as_array)
//         .cloned().unwrap_or_default();
//     let properties = object.get_mut("properties").and_then(serde_json::Value::as_object_mut)
//         .expect("parameters.properties is an object");
//     for name in required.iter().filter_map(serde_json::Value::as_str) {
//         properties.entry(name.to_owned()).or_insert_with(|| json!({"type": "string"}));
//     }
//     if matches!(tool_name, "run_terminal_command" | "monitor") {
//         properties.entry("description").or_insert_with(|| json!({"type": "string"}));
//     }
// }

#[cfg(test)]
mod tests {
    use super::{BUILTIN_TOOL_NAMES, native_registration_definitions};
    use serde_json::json;
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
    fn names_only_mode_keeps_callable_names_without_schema_payload() {
        let definitions = vec![ToolDefinition::function(
            "run_terminal_command",
            Some("run a command"),
            serde_json::json!({
                "type": "object",
                "properties": {"command": {"type": "string"}},
                "required": ["command"]
            }),
        )];

        let output = native_registration_definitions(definitions);
        assert_eq!(output[0].function.parameters, json!({}));
        assert_eq!(output[0].function.description, None);
    }

    #[test]
    fn mcp_definitions_are_not_replaced() {
        let schema = json!({"type": "object", "properties": {"x": {"type": "string"}}});
        let definitions = vec![ToolDefinition::function(
            "tasks__list",
            Some("MCP list"),
            schema.clone(),
        )];
        let output = native_registration_definitions(definitions);
        assert_eq!(output[0].function.parameters, schema);
        assert_eq!(output[0].function.description.as_deref(), Some("MCP list"));
    }
}
