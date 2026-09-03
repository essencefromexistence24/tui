//! Bridge between LSP operations and the agent tool system.
//!
//! Provides tool definitions, auto-format hooks, and diagnostics reporting
//! that integrate with `tools_for_mode()` and the agent loop.

#![allow(dead_code)]

use std::path::Path;

use serde_json::Value;

use crate::{
	lsp::{
		LspError, fmt_diagnostics, fmt_locations, fmt_symbols, global_registry, language_id_for_path,
		path_to_uri,
	},
	tools::{ToolCall, ToolDef, ToolKind, ToolResult},
};

/// LSP tool kind names — used as canonical identifiers.
pub mod names {
	pub const GO_TO_DEFINITION: &str = "go_to_definition";
	pub const FIND_REFERENCES: &str = "find_references";
	pub const HOVER: &str = "hover";
	pub const DOCUMENT_SYMBOLS: &str = "document_symbols";
	pub const WORKSPACE_SYMBOLS: &str = "workspace_symbols";
	pub const GO_TO_IMPLEMENTATION: &str = "go_to_implementation";
	pub const CALL_HIERARCHY: &str = "call_hierarchy";
	pub const FORMAT_CODE: &str = "format_code";
	pub const GET_DIAGNOSTICS: &str = "get_diagnostics";
	pub const COMPLETE_CODE: &str = "complete_code";
}

/// All LSP tool name constants in one list for iteration.
pub const LSP_TOOL_NAMES: &[&str] = &[
	names::GO_TO_DEFINITION,
	names::FIND_REFERENCES,
	names::HOVER,
	names::DOCUMENT_SYMBOLS,
	names::WORKSPACE_SYMBOLS,
	names::GO_TO_IMPLEMENTATION,
	names::CALL_HIERARCHY,
	names::FORMAT_CODE,
	names::GET_DIAGNOSTICS,
	names::COMPLETE_CODE,
];

/// Check if a tool name is an LSP tool.
pub fn is_lsp_tool(name: &str) -> bool {
	LSP_TOOL_NAMES.contains(&name)
}

/// LSP tool definitions for display / schema generation.
pub fn lsp_tool_defs() -> Vec<ToolDef> {
	vec![
		ToolDef {
			kind: ToolKind::GoToDefinition,
			description: "Navigate to the definition of a symbol. Args: path (string), line (int), character (int).",
		},
		ToolDef {
			kind: ToolKind::FindReferences,
			description: "Find all references to a symbol. Args: path (string), line (int), character (int).",
		},
		ToolDef {
			kind: ToolKind::Hover,
			description: "Get hover/documentation for a symbol. Args: path (string), line (int), character (int).",
		},
		ToolDef {
			kind: ToolKind::DocumentSymbols,
			description: "List all symbols in a document. Args: path (string).",
		},
		ToolDef {
			kind: ToolKind::WorkspaceSymbols,
			description: "Search for symbols across the workspace. Args: query (string).",
		},
		ToolDef {
			kind: ToolKind::GoToImplementation,
			description: "Navigate to the implementation(s) of a symbol. Args: path (string), line (int), character (int).",
		},
		ToolDef {
			kind: ToolKind::CallHierarchy,
			description: "Get call hierarchy for a symbol. Args: path (string), line (int), character (int), direction (string, \"incoming\"|\"outgoing\").",
		},
		ToolDef {
			kind: ToolKind::FormatCode,
			description: "Format a document using the language server. Args: path (string).",
		},
		ToolDef {
			kind: ToolKind::GetDiagnostics,
			description: "Get diagnostics (errors/warnings) for a document. Args: path (string).",
		},
		ToolDef {
			kind: ToolKind::CompleteCode,
			description: "Get code completion suggestions at a position. Args: path (string), line (int), character (int).",
		},
	]
}

/// Execute an LSP tool call. Returns `None` if the name is not an LSP tool.
pub fn execute_lsp(call: &ToolCall, cwd: &Path) -> Option<ToolResult> {
	let kind = ToolKind::from_name(&call.name)?;
	if !is_lsp_tool_schema(kind) {
		return None;
	}

	let args: Value = serde_json::from_str(&call.arguments).unwrap_or_default();

	let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
	if path_str.is_empty() && kind != ToolKind::WorkspaceSymbols {
		return Some(err_result(call, kind, "Missing `path` argument"));
	}

	let full_path = if path_str.is_empty() {
		cwd.to_path_buf()
	} else {
		let p = Path::new(path_str);
		if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) }
	};

	let line = args.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
	let character = args.get("character").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
	let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
	let direction = args.get("direction").and_then(|v| v.as_str()).unwrap_or("incoming").to_string();

	let registry = match global_registry() {
		Some(r) => r,
		None => {
			return Some(err_result(call, kind, "LSP registry not initialized"));
		}
	};

	let rt_handle = match tokio::runtime::Handle::try_current() {
		Ok(h) => h,
		Err(_) => {
			return Some(err_result(
				call,
				kind,
				"No tokio runtime available; LSP requires async runtime",
			));
		}
	};

	let result = tokio::task::block_in_place(move || {
		rt_handle.block_on(async move {
			match kind {
				ToolKind::GoToDefinition => {
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					let locs = client.go_to_definition(&uri, line, character).await?;
					Ok::<_, LspError>(fmt_locations(&locs))
				}
				ToolKind::FindReferences => {
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					let locs = client.find_references(&uri, line, character, true).await?;
					Ok(fmt_locations(&locs))
				}
				ToolKind::Hover => {
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					match client.hover(&uri, line, character).await? {
						Some(h) => Ok(h.contents.to_string()),
						None => Ok("(no hover info)".into()),
					}
				}
				ToolKind::DocumentSymbols => {
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					let syms = client.document_symbols(&uri).await?;
					Ok(fmt_symbols(&syms, 0))
				}
				ToolKind::WorkspaceSymbols => {
					if query.is_empty() {
						return Err(LspError::Other("Missing `query` argument".into()));
					}
					// Try all available languages.
					let mut all_syms = String::new();
					for lang in registry.known_languages() {
						if let Ok(client) = registry.get_client(&lang).await
							&& let Ok(syms) = client.workspace_symbols(&query).await
							&& !syms.is_empty()
						{
							all_syms.push_str(&format!("=== {lang} ===\n"));
							all_syms.push_str(&fmt_symbols(&syms, 0));
							all_syms.push('\n');
						}
					}
					if all_syms.is_empty() { Ok("(no workspace symbols found)".into()) } else { Ok(all_syms) }
				}
				ToolKind::GoToImplementation => {
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					let locs = client.go_to_implementation(&uri, line, character).await?;
					Ok(fmt_locations(&locs))
				}
				ToolKind::CallHierarchy => {
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					let items = client.prepare_call_hierarchy(&uri, line, character).await?;
					if items.is_empty() {
						return Ok("(no call hierarchy)".into());
					}
					let mut out = String::new();
					for item in &items {
						if direction == "incoming" {
							if let Ok(calls) = client.call_hierarchy_incoming(item).await {
								for c in &calls {
									let loc = c
										.from_ranges
										.first()
										.map(|r| format!("{}:{}", r.start.line + 1, r.start.character + 1))
										.unwrap_or_default();
									out.push_str(&format!("  {loc} {}\n", c.from.name));
								}
							}
						} else {
							if let Ok(calls) = client.call_hierarchy_outgoing(item).await {
								for c in &calls {
									let loc = c
										.from_ranges
										.first()
										.map(|r| format!("{}:{}", r.start.line + 1, r.start.character + 1))
										.unwrap_or_default();
									out.push_str(&format!("  {loc} {}\n", c.from.name));
								}
							}
						}
					}
					if out.is_empty() {
						out = "(no call hierarchy)".into();
					}
					Ok(out)
				}
				ToolKind::FormatCode => {
					let content = std::fs::read_to_string(&full_path)
						.map_err(|e| LspError::Other(format!("Cannot read file: {e}")))?;
					let lang = language_id_for_path(&full_path).unwrap_or("plaintext");
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					client.open_document(&uri, &content, lang).await?;
					let edits = client.format_document(&uri).await?;
					if edits.is_empty() {
						return Ok("(no formatting changes)".into());
					}
					// Apply edits in reverse order (end of file first).
					let mut text = content;
					for edit in edits.iter().rev() {
						let start = byte_offset(&text, edit.range.start.line, edit.range.start.character);
						let end = byte_offset(&text, edit.range.end.line, edit.range.end.character);
						if let (Some(s), Some(e)) = (start, end) {
							text.replace_range(s..e, &edit.new_text);
						}
					}
					std::fs::write(&full_path, &text)
						.map_err(|e| LspError::Other(format!("Cannot write formatted file: {e}")))?;
					Ok(format!("Formatted {} ({} edits)", full_path.display(), edits.len()))
				}
				ToolKind::GetDiagnostics => {
					let content = std::fs::read_to_string(&full_path)
						.map_err(|e| LspError::Other(format!("Cannot read file: {e}")))?;
					let lang = language_id_for_path(&full_path).unwrap_or("plaintext");
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					client.open_document(&uri, &content, lang).await?;
					let diags = client.get_diagnostics(&uri).await?;
					Ok(fmt_diagnostics(&diags, &uri))
				}
				ToolKind::CompleteCode => {
					let client = registry.get_client_for_path(&full_path).await?;
					let uri = path_to_uri(&full_path);
					let items = client.complete(&uri, line, character).await?;
					if items.is_empty() {
						return Ok("(no completions)".into());
					}
					let mut out = String::new();
					for item in &items {
						let kind_label = item.kind.map(completion_kind_label).unwrap_or("?");
						let detail = item.detail.as_deref().map(|d| format!(" ({d})")).unwrap_or_default();
						out.push_str(&format!("  [{kind_label}] {}{detail}\n", item.label));
					}
					Ok(out)
				}
				_ => Err(LspError::Other("Unknown LSP tool".into())),
			}
		})
	});

	match result {
		Ok(output) => ToolResult {
			call_id: call.id.clone(),
			name: call.name.clone(),
			ok: true,
			title: format!("{} · {} results", kind.display_title(), output.lines().count()),
			output,
			preview: path_str.into(),
		},
		Err(e) => err_result(call, kind, &e.to_string()),
	}
	.into()
}

fn is_lsp_tool_schema(kind: ToolKind) -> bool {
	matches!(
		kind,
		ToolKind::GoToDefinition
			| ToolKind::FindReferences
			| ToolKind::Hover
			| ToolKind::DocumentSymbols
			| ToolKind::WorkspaceSymbols
			| ToolKind::GoToImplementation
			| ToolKind::CallHierarchy
			| ToolKind::FormatCode
			| ToolKind::GetDiagnostics
			| ToolKind::CompleteCode
	)
}

fn err_result(call: &ToolCall, kind: ToolKind, msg: &str) -> ToolResult {
	ToolResult {
		call_id: call.id.clone(),
		name: kind.name().into(),
		ok: false,
		title: format!("{} · error", kind.display_title()),
		output: msg.to_string(),
		preview: String::new(),
	}
}

// ── Auto-format on write/edit ───────────────────────────────────────────────

/// Auto-format a file after it has been written. Called by the agent loop.
pub fn auto_format_file(path: &Path) -> Option<String> {
	let lang = language_id_for_path(path)?;
	let registry = global_registry()?;

	if !registry.has_server_for(lang) {
		return None;
	}

	let rt_handle = tokio::runtime::Handle::try_current().ok()?;

	tokio::task::block_in_place(move || {
		rt_handle.block_on(async move {
			let content = std::fs::read_to_string(path).ok()?;
			let client = registry.get_client_for_path(path).await.ok()?;
			let uri = path_to_uri(path);
			client.open_document(&uri, &content, lang).await.ok()?;
			let edits = client.format_document(&uri).await.ok()?;
			if edits.is_empty() {
				return Some("(no formatting changes)".to_string());
			}
			let mut text = content;
			for edit in edits.iter().rev() {
				let s = byte_offset(&text, edit.range.start.line, edit.range.start.character)?;
				let e = byte_offset(&text, edit.range.end.line, edit.range.end.character)?;
				text.replace_range(s..e, &edit.new_text);
			}
			std::fs::write(path, &text).ok()?;
			Some(format!("Auto-formatted {} ({} edits)", path.display(), edits.len()))
		})
	})
}

/// Get diagnostics for a file after an edit. Called by the agent loop.
pub fn report_diagnostics(path: &Path) -> Option<String> {
	let lang = language_id_for_path(path)?;
	let registry = global_registry()?;

	if !registry.has_server_for(lang) {
		return None;
	}

	let rt_handle = tokio::runtime::Handle::try_current().ok()?;

	tokio::task::block_in_place(move || {
		rt_handle.block_on(async move {
			let content = std::fs::read_to_string(path).ok()?;
			let client = registry.get_client_for_path(path).await.ok()?;
			let uri = path_to_uri(path);
			client.open_document(&uri, &content, lang).await.ok()?;
			let diags = client.get_diagnostics(&uri).await.ok()?;
			Some(fmt_diagnostics(&diags, &uri))
		})
	})
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn byte_offset(text: &str, line: u32, character: u32) -> Option<usize> {
	let mut current_line = 0u32;
	let mut byte_pos = 0usize;
	let mut char_in_line = 0u32;
	for ch in text.chars() {
		if current_line == line && char_in_line == character {
			return Some(byte_pos);
		}
		if ch == '\n' {
			if current_line == line && character > char_in_line {
				return Some(byte_pos + ch.len_utf8());
			}
			current_line += 1;
			char_in_line = 0;
		} else if current_line == line {
			char_in_line += 1;
		}
		byte_pos += ch.len_utf8();
	}
	if current_line == line && char_in_line >= character {
		return Some(byte_pos);
	}
	if current_line > 0 && current_line - 1 == line {
		return Some(byte_pos);
	}
	None
}

fn completion_kind_label(kind: u32) -> &'static str {
	match kind {
		1 => "Text",
		2 => "Method",
		3 => "Function",
		4 => "Constructor",
		5 => "Field",
		6 => "Variable",
		7 => "Class",
		8 => "Interface",
		9 => "Module",
		10 => "Property",
		11 => "Unit",
		12 => "Value",
		13 => "Enum",
		14 => "Keyword",
		15 => "Snippet",
		16 => "Color",
		17 => "File",
		18 => "Reference",
		19 => "Folder",
		20 => "EnumMember",
		21 => "Constant",
		22 => "Struct",
		23 => "Event",
		24 => "Operator",
		25 => "TypeParameter",
		_ => "Unknown",
	}
}

// ── OpenAI tool schemas for LSP operations ──────────────────────────────────

pub fn lsp_tool_schemas() -> Vec<Value> {
	use serde_json::json;

	let mut schemas = Vec::new();

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::GO_TO_DEFINITION,
			"description": "Navigate to the definition of a symbol.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string", "description": "File path" },
					"line": { "type": "integer", "description": "Line number (0-indexed)" },
					"character": { "type": "integer", "description": "Character offset (0-indexed)" }
				},
				"required": ["path", "line", "character"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::FIND_REFERENCES,
			"description": "Find all references to a symbol.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"line": { "type": "integer" },
					"character": { "type": "integer" }
				},
				"required": ["path", "line", "character"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::HOVER,
			"description": "Get hover information / documentation for a symbol.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"line": { "type": "integer" },
					"character": { "type": "integer" }
				},
				"required": ["path", "line", "character"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::DOCUMENT_SYMBOLS,
			"description": "List all symbols in a document.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string" }
				},
				"required": ["path"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::WORKSPACE_SYMBOLS,
			"description": "Search for symbols across the entire workspace.",
			"parameters": {
				"type": "object",
				"properties": {
					"query": { "type": "string", "description": "Symbol name or partial query" }
				},
				"required": ["query"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::GO_TO_IMPLEMENTATION,
			"description": "Navigate to the implementation(s) of a symbol.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"line": { "type": "integer" },
					"character": { "type": "integer" }
				},
				"required": ["path", "line", "character"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::CALL_HIERARCHY,
			"description": "Get call hierarchy (incoming or outgoing calls) for a symbol.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"line": { "type": "integer" },
					"character": { "type": "integer" },
					"direction": {
						"type": "string",
						"enum": ["incoming", "outgoing"],
						"description": "Direction of call hierarchy (default: incoming)"
					}
				},
				"required": ["path", "line", "character"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::FORMAT_CODE,
			"description": "Format a document using the language server.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string" }
				},
				"required": ["path"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::GET_DIAGNOSTICS,
			"description": "Get diagnostics (errors, warnings, hints) for a document.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string" }
				},
				"required": ["path"]
			}
		}
	}));

	schemas.push(json!({
		"type": "function",
		"function": {
			"name": names::COMPLETE_CODE,
			"description": "Get code completion suggestions at a position.",
			"parameters": {
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"line": { "type": "integer" },
					"character": { "type": "integer" }
				},
				"required": ["path", "line", "character"]
			}
		}
	}));

	schemas
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_is_lsp_tool() {
		assert!(is_lsp_tool(names::GO_TO_DEFINITION));
		assert!(is_lsp_tool(names::HOVER));
		assert!(!is_lsp_tool("shell"));
		assert!(!is_lsp_tool("read"));
	}

	#[test]
	fn test_completion_kind_labels() {
		assert_eq!(completion_kind_label(1), "Text");
		assert_eq!(completion_kind_label(3), "Function");
		assert_eq!(completion_kind_label(99), "Unknown");
	}

	#[test]
	fn test_byte_offset() {
		let text = "hello\nworld\nfoo";
		assert_eq!(byte_offset(text, 0, 0), Some(0));
		assert_eq!(byte_offset(text, 0, 4), Some(4));
		assert_eq!(byte_offset(text, 1, 0), Some(6));
		assert_eq!(byte_offset(text, 1, 3), Some(9));
		assert_eq!(byte_offset(text, 2, 0), Some(12));
		assert_eq!(byte_offset(text, 2, 3), Some(15));
	}

	#[test]
	fn test_byte_offset_beyond_last_line() {
		let text = "hello\n";
		assert_eq!(byte_offset(text, 0, 6), Some(6));
	}

	#[test]
	fn test_byte_offset_empty() {
		assert_eq!(byte_offset("", 0, 0), Some(0));
	}

	#[test]
	fn test_lsp_tool_defs_count() {
		let defs = lsp_tool_defs();
		assert_eq!(defs.len(), 10);
	}

	#[test]
	fn test_lsp_tool_schemas_count() {
		let schemas = lsp_tool_schemas();
		assert_eq!(schemas.len(), 10);
	}
}
