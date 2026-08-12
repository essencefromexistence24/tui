//! Native tool registrations used alongside Dx Serializer Compact.
//!
//! The complete built-in contract is also described in the first prompt by
//! [`crate::prompt::dx_serializer_compact::TOOL_CATALOG`]. That compact catalog
//! is supplementary: provider-facing native registrations must retain their
//! complete JSON Schema so strict OpenAI-compatible adapters can validate and
//! generate arguments correctly.

use xai_grok_tools::types::definition::ToolDefinition;

/// Return provider-facing definitions without stripping their JSON Schemas.
///
/// The function name is retained for source compatibility with older callers,
/// but native schemas must never be replaced by `{ "type": "object" }`.
/// Dx Serializer Compact is prompt-side metadata, not a substitute for the
/// provider's `tools[].function.parameters` contract.
pub fn compact_native_definitions(definitions: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    definitions
}

#[cfg(test)]
mod tests {
    use super::compact_native_definitions;
    use xai_grok_tools::types::definition::ToolDefinition;

    #[test]
    fn compact_native_definitions_preserves_builtin_json_schemas() {
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
        assert_eq!(
            output[0].function.parameters["properties"]["command"]["type"],
            "string"
        );
        assert_eq!(
            output[0].function.description.as_deref(),
            Some("run a command")
        );
    }
}
