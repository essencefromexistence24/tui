//! Bridge between MCP tools and the dx-tui tool system.
//!
//! Adapts `McpTool` definitions into `ToolDef` entries and routes
//! execution through the `McpRegistry`. Integrates with the
//! permission system for user approval before running MCP tool calls.

#![allow(dead_code)]

use std::sync::Arc;

use serde_json::{Value, json};

use crate::{
	mcp::{McpRegistry, McpTool},
	tools::{ToolCall, ToolDef, ToolKind, ToolResult},
};

/// Prefix used to namespace MCP tool names per server.
/// Format: `{server_id}__{tool_name}` to avoid name collisions.
const MCP_TOOL_PREFIX_SEPARATOR: &str = "__";

/// Maximum MCP tool output length (chars) for ToolResult.
const MCP_OUT_CAP: usize = 12_000;

// ── Tool name helpers ────────────────────────────────────────────────────

/// Build a unique tool name: `{server_id}__{tool_name}`.
pub fn mcp_tool_qualified_name(server_id: &str, tool_name: &str) -> String {
	format!("{server_id}{MCP_TOOL_PREFIX_SEPARATOR}{tool_name}")
}

/// Parse a qualified tool name into `(server_id, tool_name)`.
pub fn parse_mcp_tool_name(qualified: &str) -> Option<(String, String)> {
	let sep = MCP_TOOL_PREFIX_SEPARATOR;
	let idx = qualified.find(sep)?;
	if idx == 0 {
		return None;
	}
	Some((qualified[..idx].to_string(), qualified[idx + sep.len()..].to_string()))
}

/// Check if a tool name is an MCP-qualified tool name.
pub fn is_mcp_tool_name(name: &str) -> bool {
	name.contains(MCP_TOOL_PREFIX_SEPARATOR)
}

// ── Tool definition generation ───────────────────────────────────────────

/// Convert an `McpTool` into a `ToolDef` for the dx-tui tool system.
/// Note: all MCP tools share `ToolKind::McpTool`; they are distinguished
/// at call time by their qualified name (`server_id__tool_name`).
pub fn mcp_tool_to_tool_def(server_id: &str, tool: &McpTool) -> ToolDef {
	let desc = if tool.description.is_empty() {
		format!("MCP tool via server `{server_id}`: {}", tool.name)
	} else {
		format!("{} (MCP via {server_id})", tool.description)
	};

	ToolDef { kind: ToolKind::McpTool, description: Box::leak(desc.into_boxed_str()) }
}

/// Collect all `ToolDef` entries from all connected MCP servers.
pub async fn mcp_tool_defs(registry: &McpRegistry) -> Vec<ToolDef> {
	let mut defs = Vec::new();
	for client in registry.all_clients().await {
		let server_id = client.id().to_string();
		for tool in client.tools().await {
			defs.push(mcp_tool_to_tool_def(&server_id, &tool));
		}
	}
	defs
}

/// Get the JSON schema for a specific MCP tool.
/// Returns None if the tool is not found.
pub async fn mcp_tool_schema(registry: &McpRegistry, qualified_name: &str) -> Option<Value> {
	let (server_id, tool_name) = parse_mcp_tool_name(qualified_name)?;
	let client = registry.get_client(&server_id).await?;
	let tools = client.tools().await;
	tools.iter().find(|t| t.name == tool_name).map(|t| t.input_schema.clone())
}

/// Generate an OpenAI-compatible tool schema for an MCP tool definition.
pub fn mcp_tool_openai_schema(server_id: &str, tool: &McpTool) -> Value {
	let qualified_name = mcp_tool_qualified_name(server_id, &tool.name);
	json!({
			"type": "function",
			"function": {
					"name": qualified_name,
					"description": tool.description,
					"parameters": tool.input_schema
			}
	})
}

/// Collect all OpenAI-compatible schemas from all connected servers.
pub async fn mcp_tool_openai_schemas(registry: &McpRegistry) -> Vec<Value> {
	let mut schemas = Vec::new();
	for client in registry.all_clients().await {
		let server_id = client.id().to_string();
		for tool in client.tools().await {
			schemas.push(mcp_tool_openai_schema(&server_id, &tool));
		}
	}
	schemas
}

// ── Tool execution ───────────────────────────────────────────────────────

/// Execute an MCP tool call and produce a `ToolResult`.
pub async fn execute_mcp_tool(call: &ToolCall, registry: &McpRegistry) -> ToolResult {
	let qualified_name = &call.name;

	let Some((server_id, tool_name)) = parse_mcp_tool_name(qualified_name) else {
		return ToolResult {
			call_id: call.id.clone(),
			name: qualified_name.clone(),
			ok: false,
			title: "MCP · invalid tool name".into(),
			output: format!("Invalid MCP tool name format: {qualified_name}"),
			preview: String::new(),
		};
	};

	let args: Value = serde_json::from_str(&call.arguments).unwrap_or(json!({}));

	// Look up the server
	let client = match registry.get_client(&server_id).await {
		Some(c) => c,
		None => {
			return ToolResult {
				call_id: call.id.clone(),
				name: qualified_name.clone(),
				ok: false,
				title: format!("MCP · server offline · {server_id}"),
				output: format!(
					"MCP server `{server_id}` is not connected. \
                     Use `/mcps` to check server status."
				),
				preview: server_id,
			};
		}
	};

	// Verify the tool exists
	let tools = client.tools().await;
	let _tool = match tools.iter().find(|t| t.name == tool_name) {
		Some(t) => t.clone(),
		None => {
			return ToolResult {
				call_id: call.id.clone(),
				name: qualified_name.clone(),
				ok: false,
				title: format!("MCP · unknown tool · {tool_name}"),
				output: format!(
					"Tool `{tool_name}` not found on server `{server_id}`. \
                     Available: {}",
					tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
				),
				preview: tool_name,
			};
		}
	};

	let preview = tool_name.chars().take(72).collect::<String>();

	// Execute the tool
	match client.call_tool(&tool_name, args).await {
		Ok(result) => {
			let output = serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into());
			let is_error = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);

			let truncated = truncate(&output, MCP_OUT_CAP);
			let content = result
				.get("content")
				.and_then(|c| c.as_array())
				.map(|arr| {
					arr
						.iter()
						.filter_map(|item| item.get("text").and_then(|t| t.as_str()))
						.collect::<Vec<_>>()
						.join("\n")
				})
				.unwrap_or_default();

			ToolResult {
				call_id: call.id.clone(),
				name: qualified_name.clone(),
				ok: !is_error,
				title: format!("MCP · {tool_name} · {}", if is_error { "error" } else { "ok" }),
				output: if content.is_empty() { truncated } else { truncate(&content, MCP_OUT_CAP) },
				preview,
			}
		}
		Err(e) => ToolResult {
			call_id: call.id.clone(),
			name: qualified_name.clone(),
			ok: false,
			title: format!("MCP · {tool_name} · failed"),
			output: format!("MCP tool call failed: {e}"),
			preview,
		},
	}
}

/// Check if a tool name corresponds to an MCP tool in any registered server.
pub async fn is_known_mcp_tool(registry: &McpRegistry, name: &str) -> bool {
	if !is_mcp_tool_name(name) {
		return false;
	}
	let Some((server_id, tool_name)) = parse_mcp_tool_name(name) else {
		return false;
	};
	let Some(client) = registry.get_client(&server_id).await else {
		return false;
	};
	client.tools().await.iter().any(|t| t.name == tool_name)
}

/// Summary of all connected MCP servers and their tools for display.
pub async fn mcp_tool_summary(registry: &McpRegistry) -> String {
	let clients = registry.all_clients().await;
	if clients.is_empty() {
		return "No MCP servers connected.".into();
	}

	let mut lines = Vec::new();
	for client in &clients {
		let status = client.status().await;
		let tools = status.tools;
		let tool_count = tools.len();
		let status_icon = if status.connected { "●" } else { "○" };
		lines.push(format!("  {} {} ({} tools)", status_icon, status.config.name, tool_count));
		for tool in tools.iter().take(5) {
			lines.push(format!("    - {}", tool.name));
		}
		if tool_count > 5 {
			lines.push(format!("    ... and {} more", tool_count - 5));
		}
	}
	lines.join("\n")
}

// ── Permission integration ───────────────────────────────────────────────

/// Determine if an MCP tool call needs user permission.
pub fn mcp_tool_needs_permission(tool_name: &str) -> bool {
	// MCP tools always require permission since they run external commands
	// on the user's machine. This can be refined per-server in the future.
	is_mcp_tool_name(tool_name)
}

/// Generate a human-readable preview for MCP tool permission prompts.
pub fn mcp_tool_permission_preview(qualified_name: &str, args: &Value) -> String {
	let (server_id, tool_name) = parse_mcp_tool_name(qualified_name).unwrap_or_default();
	let arg_summary: Vec<String> = args
		.as_object()
		.map(|obj| {
			obj
				.iter()
				.map(|(k, v)| {
					let val = v.as_str().map(|s| s.chars().take(40).collect::<String>());
					format!("{}: {}", k, val.unwrap_or_else(|| v.to_string()))
				})
				.collect()
		})
		.unwrap_or_default();
	if arg_summary.is_empty() {
		format!("MCP · {server_id} · {tool_name}")
	} else {
		format!("MCP · {server_id} · {tool_name} ({})", arg_summary.join(", "))
	}
}

// ── Agent loop integration ───────────────────────────────────────────────

/// Inject MCP tool definitions into a mode's tool list.
/// Call from `tools_for_mode()` when MCP tools are available.
pub fn inject_mcp_into_tools(mut tools: Vec<ToolDef>, mcp_defs: Vec<ToolDef>) -> Vec<ToolDef> {
	// Only inject if there are MCP tools to offer
	if mcp_defs.is_empty() {
		return tools;
	}
	tools.extend(mcp_defs);
	tools
}

/// Execute an MCP tool within the blocking `tools::execute()` call.
/// Since MCP tools are async, we use `tokio::runtime::Handle` to block on them.
pub fn execute_mcp_tool_blocking(
	call: &ToolCall,
	registry: Option<&Arc<McpRegistry>>,
) -> ToolResult {
	let Some(registry) = registry.cloned() else {
		return ToolResult {
			call_id: call.id.clone(),
			name: call.name.clone(),
			ok: false,
			title: "MCP · no registry".into(),
			output: "MCP registry is not available.".into(),
			preview: String::new(),
		};
	};

	let call_c =
		ToolCall { id: call.id.clone(), name: call.name.clone(), arguments: call.arguments.clone() };

	tokio::task::block_in_place(|| {
		let handle = tokio::runtime::Handle::current();
		handle.block_on(async { execute_mcp_tool(&call_c, &registry).await })
	})
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn truncate(s: &str, cap: usize) -> String {
	let count = s.chars().count();
	if count <= cap {
		return s.to_string();
	}
	let kept: String = s.chars().take(cap).collect();
	format!("{kept}\n…[truncated {} chars]", count - cap)
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_qualified_name() {
		let (sid, tn) = parse_mcp_tool_name("filesystem__read_file").unwrap();
		assert_eq!(sid, "filesystem");
		assert_eq!(tn, "read_file");
	}

	#[test]
	fn test_parse_qualified_name_no_separator() {
		assert!(parse_mcp_tool_name("read_file").is_none());
	}

	#[test]
	fn test_parse_qualified_name_leading_separator() {
		assert!(parse_mcp_tool_name("__tool").is_none());
	}

	#[test]
	fn test_is_mcp_tool_name_positive() {
		assert!(is_mcp_tool_name("fs__read"));
	}

	#[test]
	fn test_is_mcp_tool_name_negative() {
		assert!(!is_mcp_tool_name("read"));
	}

	#[test]
	fn test_mcp_tool_qualified_name() {
		let qn = mcp_tool_qualified_name("fs", "read_file");
		assert_eq!(qn, "fs__read_file");
	}

	#[test]
	fn test_mcp_tool_to_tool_def() {
		let tool = McpTool {
			name: "read_file".into(),
			description: "Read a file".into(),
			input_schema: json!({"type": "object"}),
		};
		let def = mcp_tool_to_tool_def("filesystem", &tool);
		assert_eq!(def.kind, ToolKind::McpTool);
		assert!(def.description.contains("Read a file"));
		assert!(def.description.contains("filesystem"));
	}

	#[test]
	fn test_truncate_under_limit() {
		assert_eq!(truncate("hello", 100), "hello");
	}

	#[test]
	fn test_truncate_over_limit() {
		let s = truncate("hello world", 5);
		assert!(s.starts_with("hello"));
		assert!(s.contains("truncated"));
	}

	#[test]
	fn test_mcp_tool_needs_permission() {
		assert!(mcp_tool_needs_permission("fs__read_file"));
		assert!(!mcp_tool_needs_permission("shell"));
	}
}
