//! Bridge between the Lua plugin system and the built-in ToolKind/ToolDef system.

#![allow(dead_code)]
//!
//! Plugin-declared tools are surfaced as `ToolDef` entries and executed through
//! the `PluginRegistry`.  The permission system is consulted for tools that
//! declare `permission_required = true` in their manifest.toml.

use std::sync::Arc;

use serde_json::Value;

use crate::{
	plugin_system::{PluginRegistry, PluginToolDef, global_registry, try_global_registry},
	tools::{ToolCall, ToolResult},
};

// ── ToolDef conversion ──────────────────────────────────────────────────

/// Convert a (plugin_name, PluginToolDef) pair into a ToolDef-compatible
/// description string for the agent's tool listing.
pub fn plugin_tool_description(plugin_name: &str, def: &PluginToolDef) -> String {
	format!(
		"[plugin:{plugin_name}] {} — Args: {}",
		def.description,
		serde_json::to_string(&def.parameters).unwrap_or_else(|_| "{}".into())
	)
}

/// Build OpenAI-compatible tool schema for a plugin tool.
pub fn plugin_tool_schema(plugin_name: &str, def: &PluginToolDef) -> Value {
	serde_json::json!({
		"type": "function",
		"function": {
			"name": def.name,
			"description": format!("[{}] {}", plugin_name, def.description),
			"parameters": if def.parameters.is_object() && !def.parameters.as_object().unwrap().is_empty() {
				def.parameters.clone()
			} else {
				serde_json::json!({
					"type": "object",
					"properties": {
						"input": { "type": "string", "description": "Input for the plugin tool" }
					},
					"required": ["input"]
				})
			}
		}
	})
}

/// List all currently registered plugin tools as (plugin_name, ToolDef-like) pairs.
pub fn list_plugin_tools() -> Vec<(String, Arc<PluginToolDef>)> {
	global_registry().plugin_tool_defs()
}

/// Check whether a tool name is a plugin tool.
pub fn is_plugin_tool(name: &str) -> bool {
	match try_global_registry() {
		Some(reg) => reg.is_plugin_tool(name),
		None => false,
	}
}

/// Check whether a plugin tool requires user permission.
pub fn plugin_tool_needs_permission(tool_name: &str) -> bool {
	for (_, def) in list_plugin_tools() {
		if def.name == tool_name {
			return def.permission_required;
		}
	}
	false
}

// ── Execution ───────────────────────────────────────────────────────────

/// Execute a plugin tool call, returning a `ToolResult`.
///
/// This function handles both the successful and error cases, converting the
/// plugin's JSON response into the standard `ToolResult` shape.
pub fn execute_plugin_tool(call: &ToolCall) -> ToolResult {
	let args: Value = serde_json::from_str(&call.arguments).unwrap_or_default();

	match global_registry().execute_tool(&call.name, &args) {
		Ok(result) => {
			let output = serde_json::to_string_pretty(&result)
				.unwrap_or_else(|_| "plugin returned non-serializable value".into());
			ToolResult {
				call_id: call.id.clone(),
				name: call.name.clone(),
				ok: true,
				title: format!("Plugin · {}", call.name),
				output,
				preview: call.name.clone(),
			}
		}
		Err(e) => ToolResult {
			call_id: call.id.clone(),
			name: call.name.clone(),
			ok: false,
			title: format!("Plugin Error · {}", call.name),
			output: format!("Plugin tool '{}' failed: {e}", call.name),
			preview: call.name.clone(),
		},
	}
}

/// Check whether a tool call is a plugin tool and if so execute it, returning
/// `Some(ToolResult)`.  Returns `None` if the tool is not a plugin tool.
pub fn try_execute_plugin(call: &ToolCall) -> Option<ToolResult> {
	if !is_plugin_tool(&call.name) {
		return None;
	}
	Some(execute_plugin_tool(call))
}

/// Collect all plugin openai tool schemas for the tool list.
pub fn plugin_openai_schemas(registry: &PluginRegistry) -> Vec<Value> {
	registry.plugin_tool_defs().into_iter().map(|(pn, def)| plugin_tool_schema(&pn, &def)).collect()
}

/// Merge plugin tool schemas into a base set of built-in schemas.
pub fn merge_tool_schemas(builtin: Vec<Value>) -> Vec<Value> {
	let mut all = builtin;
	all.extend(plugin_openai_schemas(global_registry()));
	all
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_is_plugin_tool_empty() {
		assert!(!is_plugin_tool("nonexistent"));
	}

	#[test]
	fn test_plugin_tool_schema_shape() {
		let def = PluginToolDef {
			name: "test_tool".into(),
			description: "A test".into(),
			parameters: serde_json::json!({
				"type": "object",
				"properties": {
					"msg": { "type": "string" }
				},
				"required": ["msg"]
			}),
			permission_required: false,
		};
		let schema = plugin_tool_schema("test_plugin", &def);
		assert_eq!(schema["function"]["name"], "test_tool");
		assert!(schema["function"]["description"].as_str().unwrap().contains("test_plugin"));
	}

	#[test]
	fn test_try_execute_plugin_none() {
		let call = ToolCall { id: "1".into(), name: "shell".into(), arguments: "{}".into() };
		assert!(try_execute_plugin(&call).is_none());
	}
}
