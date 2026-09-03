//! `tool_details` — on-demand full JSON schema for one tool.
//!
//! The system prompt carries only a compact one-line-per-tool catalog
//! (names + short arg hints, ~60 tokens for this tool itself). That is
//! intentionally lossy: rare/nested parameter details are omitted so the
//! first request stays small.
//!
//! When the compact line is not enough — the model has failed the same tool
//! call ~3 times — it calls `tool_details` with `{"tool_name": "<name>"}` and
//! receives the *complete* JSON schema (description + every parameter with
//! types, defaults, enums) for exactly that tool, and only that tool.
//!
//! The returned schema is also recorded in the workspace `AGENTS.md` (one
//! `### Tool Schema: <name>` section per tool) so future sessions in the
//! same workspace already know the schema that was hard before, without
//! growing the first prompt. If the model is *still* stuck after reading the
//! full schema, it writes a short corrected usage note under the same
//! heading (`Fix:` line) — that is the "fix data correctly if really hard"
//! path, and it persists for all later sessions.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::modes::AgentMode;
use crate::tools::{ToolCall, ToolResult};

/// Canonical model-facing name of this tool.
pub const TOOL_DETAILS_TOOL_NAME: &str = "tool_details";

/// Heading that groups the auto-recorded sections inside `AGENTS.md`.
pub const AGENTS_SCHEMA_HEADING: &str = "## Tool Schemas (recorded by tool_details)";

/// Heading prefix identifying one recorded tool-schema section.
pub const SECTION_PREFIX: &str = "### Tool Schema: ";

const COMPACT_CATALOG_LINE: &str = "tool_details(tool_name) — full JSON schema for ONE tool; use ONLY after ~3 failed calls of the same tool, then keep the fix in AGENTS.md.";

const MAX_SCHEMA_CHARS: usize = 12_000;

/// OpenAI-compatible `tools` entry for `tool_details`.
pub fn tool_details_schema() -> Value {
	json!({
		"type": "function",
		"function": {
			"name": TOOL_DETAILS_TOOL_NAME,
			"description": "Return the FULL JSON schema (description + every parameter with types, defaults, enums) for exactly one tool. Use ONLY after the same tool call has failed ~3 times — the compact tool list in the system prompt is normally sufficient. Args: tool_name (string). The schema is also saved to the workspace AGENTS.md so future sessions keep the fix.",
			"parameters": {
				"type": "object",
				"properties": {
					"tool_name": { "type": "string", "description": "Tool name, e.g. edit, todowrite, task, shell, server__tool for MCP" }
				},
				"required": ["tool_name"]
			}
		}
	})
}

/// One-line catalog entry appended to the compact tool list in the system prompt.
pub fn compact_catalog_line() -> &'static str {
	COMPACT_CATALOG_LINE
}

/// Execute a `tool_details` call in `cwd`.
pub fn execute_tool_details(call: &ToolCall, cwd: &Path, mode: AgentMode) -> ToolResult {
	let args: Value = serde_json::from_str(&call.arguments).unwrap_or(json!({}));
	let raw = args
		.get("tool_name")
		.or_else(|| args.get("name"))
		.or_else(|| args.get("tool"))
		.and_then(|v| v.as_str())
		.unwrap_or("")
		.trim();
	if raw.is_empty() {
		return ToolResult {
			call_id: call.id.clone(),
			name: TOOL_DETAILS_TOOL_NAME.into(),
			ok: false,
			title: "Tool Details · missing tool_name".into(),
			output: "Missing `tool_name`. Send {\"tool_name\": \"<name>\"} using the exact name from the compact tool list.".into(),
			preview: String::new(),
		};
	}

	let Some((label, schema_json)) = full_schema_for(raw, mode) else {
		let suggestions = close_matches(raw, mode);
		let hint = if suggestions.is_empty() {
			"no similar names found".to_string()
		} else {
			format!("closest known tools: {}", suggestions.join(", "))
		};
		return ToolResult {
			call_id: call.id.clone(),
			name: TOOL_DETAILS_TOOL_NAME.into(),
			ok: false,
			title: format!("Tool Details · unknown tool {raw}"),
			output: format!(
				"Unknown tool `{raw}`. {hint}. Re-check the compact tool list in the system prompt and retry the original call with the exact name."
			),
			preview: raw.to_string(),
		};
	};

	// Record the hard schema into the workspace AGENTS.md so the fix persists.
	// Read-only modes (Ask/Multi/Plan) must not write files: return the schema
	// without recording.
	let record_note = if mode_writes_files(mode) {
		let agents_path = cwd.join("AGENTS.md");
		match record_in_agents_md(&agents_path, &label, &schema_json) {
			Ok(path) => format!("Schema recorded in {} for future sessions.", path.display()),
			Err(e) => format!("AGENTS.md not updated ({e}); schema below is still valid."),
		}
	} else {
		format!(
			"Not recorded ({} mode is read-only). The schema below is still valid for this turn.",
			mode.label()
		)
	};

	let body = truncate(&schema_json, MAX_SCHEMA_CHARS);
	ToolResult {
		call_id: call.id.clone(),
		name: TOOL_DETAILS_TOOL_NAME.into(),
		ok: true,
		title: format!("Tool Details · {label}"),
		output: format!(
			"Full schema for `{label}`:\n{body}\n\n{record_note}\nIf this schema is still unclear, append a short `Fix:` usage note under `### Tool Schema: {label}` in the workspace AGENTS.md so the next session gets it right first try."
		),
		preview: label,
	}
}

fn mode_writes_files(mode: AgentMode) -> bool {
	matches!(
		mode,
		AgentMode::Write
			| AgentMode::Goal
			| AgentMode::Agent
			| AgentMode::Automation
			| AgentMode::Codex
	)
}

/// Full schema for one tool, from the same constructors that build the wire
/// `tools` array — so there is no second hand-maintained list to drift.
fn full_schema_for(tool_name: &str, mode: AgentMode) -> Option<(String, String)> {
	let needle = tool_name.trim().to_ascii_lowercase();
	// Union over modes so the lookup works regardless of the caller's mode;
	// Agent is the superset but Ask/Write add nothing conflicting.
	for m in [AgentMode::Agent, AgentMode::Write, AgentMode::Ask] {
		for schema in crate::tools::openai_tool_schemas(m) {
			let name =
				schema.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
			if name.eq_ignore_ascii_case(&needle) {
				let label = name.to_string();
				let json = serde_json::to_string_pretty(&schema).ok()?;
				return Some((label, json));
			}
		}
	}
	// Task / skill schemas are built on alternate paths; include them too.
	for schema in [
		crate::orchestration::task_tool_schema(),
		crate::skills::skill_manage_schema(),
		crate::memory_tool::memory_tool_schema(),
	] {
		let name =
			schema.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
		if name.eq_ignore_ascii_case(&needle) {
			let label = name.to_string();
			let json = serde_json::to_string_pretty(&schema).ok()?;
			return Some((label, json));
		}
	}
	// Aliases / compat spellings resolve to their canonical tool.
	if let Some(kind) = crate::tools::ToolKind::from_name(&needle) {
		let canonical = kind.name();
		if !canonical.eq_ignore_ascii_case(&needle) {
			return full_schema_for(canonical, mode);
		}
	}
	None
}

/// Best-effort close matches for the unknown-tool error message.
fn close_matches(tool_name: &str, mode: AgentMode) -> Vec<String> {
	let needle = tool_name.trim().to_ascii_lowercase();
	let mut names: Vec<String> = Vec::new();
	for m in [mode, AgentMode::Agent] {
		for t in crate::tools::tools_for_mode(m) {
			let n = t.kind.name().to_string();
			if n != TOOL_DETAILS_TOOL_NAME && !names.contains(&n) {
				names.push(n);
			}
		}
	}
	for extra in ["task", "skill_manage", "memory"] {
		if !names.contains(&extra.to_string()) {
			names.push(extra.to_string());
		}
	}
	let mut matches: Vec<String> =
		names.iter().filter(|n| n.to_ascii_lowercase().contains(&needle)).cloned().collect();
	if matches.is_empty() {
		matches = names;
	}
	matches.sort_unstable();
	matches.truncate(8);
	matches
}

/// Render one `AGENTS.md` section for a tool schema.
pub fn agents_md_section(tool_name: &str, schema_json: &str) -> String {
	format!("{SECTION_PREFIX}{tool_name}\n```json\n{schema_json}\n```\n")
}

/// Insert or replace a tool's section within the full `AGENTS.md` contents.
///
/// Pure string transform so it is unit-testable without a filesystem. When
/// `existing` is `None` a fresh file body is created. User content above the
/// auto-recorded heading is always preserved.
pub fn upsert_agents_md(existing: Option<&str>, tool_name: &str, section: &str) -> String {
	let heading = format!("{SECTION_PREFIX}{tool_name}");
	let Some(existing) = existing else {
		return format!(
			"# AGENTS.md\n\nNotes recorded by the DX agent for this workspace.\n\n\
			 {AGENTS_SCHEMA_HEADING}\n\n{section}"
		);
	};

	let section = if section.ends_with('\n') { section.to_owned() } else { format!("{section}\n") };

	// Replace the existing section for this tool when present.
	if let Some(start) = find_section_start(existing, &heading) {
		let heading_end = start + heading.len();
		let end = existing[heading_end..]
			.find(&format!("\n{SECTION_PREFIX}"))
			.map_or(existing.len(), |rel| heading_end + rel + 1);
		let mut out = String::with_capacity(existing.len() + section.len());
		out.push_str(&existing[..start]);
		out.push_str(&section);
		out.push_str(existing[end..].trim_end_matches('\n'));
		out.push('\n');
		return out;
	}

	// Append under the grouped heading (existing or newly created).
	let trimmed = existing.trim_end_matches('\n');
	if existing.contains(AGENTS_SCHEMA_HEADING) {
		format!("{trimmed}\n\n{section}")
	} else {
		format!("{trimmed}\n\n{AGENTS_SCHEMA_HEADING}\n\n{section}")
	}
}

/// Byte offset of a `### Tool Schema: <tool_name>` heading line, if present.
fn find_section_start(contents: &str, heading: &str) -> Option<usize> {
	let mut search_from = 0usize;
	while let Some(rel) = contents[search_from..].find(SECTION_PREFIX) {
		let line_start = search_from + rel;
		let line_end = contents[line_start..].find('\n').map_or(contents.len(), |n| line_start + n);
		if contents[line_start..line_end].trim_end() == heading {
			return Some(line_start);
		}
		search_from = line_end;
	}
	None
}

/// Read `AGENTS.md` (if present), upsert the section, and write it back.
///
/// Returns the path written on success.
fn record_in_agents_md(path: &Path, tool_name: &str, schema_json: &str) -> Result<PathBuf, String> {
	let existing = std::fs::read_to_string(path).ok();
	let updated =
		upsert_agents_md(existing.as_deref(), tool_name, &agents_md_section(tool_name, schema_json));
	std::fs::write(path, updated).map_err(|e| format!("{}: {e}", path.display()))?;
	Ok(path.to_path_buf())
}

fn truncate(s: &str, cap: usize) -> String {
	let count = s.chars().count();
	if count <= cap {
		return s.to_string();
	}
	let kept: String = s.chars().take(cap).collect();
	format!("{kept}\n…[truncated {} chars]", count - cap)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn schema_is_valid_function_with_tool_name() {
		let v = tool_details_schema();
		assert_eq!(v["function"]["name"], "tool_details");
		assert_eq!(v["function"]["parameters"]["required"], json!(["tool_name"]));
	}

	#[test]
	fn lookup_resolves_canonical_and_alias() {
		let (label, json) = full_schema_for("shell", AgentMode::Agent).expect("shell schema");
		assert_eq!(label, "shell");
		assert!(json.contains("\"command\""));
		// Compat alias from from_name (bash -> shell).
		let (label2, _) = full_schema_for("bash", AgentMode::Agent).expect("alias schema");
		assert_eq!(label2, "shell");
	}

	#[test]
	fn lookup_unknown_returns_none() {
		assert!(full_schema_for("no_such_tool_xyz", AgentMode::Agent).is_none());
	}

	#[test]
	fn execute_missing_tool_name_is_error() {
		let call = ToolCall { id: "c1".into(), name: "tool_details".into(), arguments: "{}".into() };
		let r = execute_tool_details(&call, Path::new("."), AgentMode::Agent);
		assert!(!r.ok);
		assert!(r.output.contains("tool_name"));
	}

	#[test]
	fn execute_unknown_tool_suggests() {
		let call = ToolCall {
			id: "c1".into(),
			name: "tool_details".into(),
			arguments: json!({"tool_name": "shel"}).to_string(),
		};
		let r = execute_tool_details(&call, Path::new("."), AgentMode::Agent);
		assert!(!r.ok);
		assert!(r.output.contains("shell"), "expected suggestion, got: {}", r.output);
	}

	#[test]
	fn execute_records_schema_in_agents_md() {
		let tmp = tempfile::tempdir().unwrap();
		let call = ToolCall {
			id: "c1".into(),
			name: "tool_details".into(),
			arguments: json!({"tool_name": "read"}).to_string(),
		};
		let r = execute_tool_details(&call, tmp.path(), AgentMode::Agent);
		assert!(r.ok, "output: {}", r.output);
		let body = std::fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap();
		assert!(body.contains("### Tool Schema: read"));
		assert!(body.contains(AGENTS_SCHEMA_HEADING));
		// Second call replaces rather than duplicates.
		let r2 = execute_tool_details(&call, tmp.path(), AgentMode::Agent);
		assert!(r2.ok);
		let body2 = std::fs::read_to_string(tmp.path().join("AGENTS.md")).unwrap();
		assert_eq!(body2.matches(SECTION_PREFIX).count(), 1);
	}

	#[test]
	fn execute_read_only_mode_skips_agents_md_write() {
		let tmp = tempfile::tempdir().unwrap();
		let call = ToolCall {
			id: "c1".into(),
			name: "tool_details".into(),
			arguments: json!({"tool_name": "read"}).to_string(),
		};
		let r = execute_tool_details(&call, tmp.path(), AgentMode::Ask);
		assert!(r.ok);
		assert!(!tmp.path().join("AGENTS.md").exists());
		assert!(r.output.contains("read-only"));
	}

	#[test]
	fn fresh_agents_md_contains_heading_and_section() {
		let out = upsert_agents_md(None, "read", &agents_md_section("read", "{}"));
		assert!(out.starts_with("# AGENTS.md"));
		assert!(out.contains(AGENTS_SCHEMA_HEADING));
		assert!(out.contains("### Tool Schema: read"));
		assert!(out.contains("```json\n{}\n```"));
	}

	#[test]
	fn append_preserves_user_notes_above_heading() {
		let existing = "# AGENTS.md\n\nCustom user notes.\n";
		let out = upsert_agents_md(Some(existing), "grep", &agents_md_section("grep", "{}"));
		assert!(out.contains("Custom user notes."));
		let heading_pos = out.find(AGENTS_SCHEMA_HEADING).unwrap();
		let notes_pos = out.find("Custom user notes.").unwrap();
		assert!(notes_pos < heading_pos);
	}

	#[test]
	fn upsert_replaces_only_the_named_tools_section() {
		let first = agents_md_section("read", "{\"old\":true}");
		let second = agents_md_section("grep", "{\"grep\":true}");
		let base = format!("# AGENTS.md\n\n{AGENTS_SCHEMA_HEADING}\n\n{first}{second}");
		let out = upsert_agents_md(Some(&base), "read", &agents_md_section("read", "{\"new\":true}"));
		assert!(out.contains("{\"new\":true}"));
		assert!(!out.contains("{\"old\":true}"));
		assert!(out.contains("{\"grep\":true}"));
		assert_eq!(out.matches(SECTION_PREFIX).count(), 2);
	}

	#[test]
	fn section_prefix_is_not_confused_with_other_tool_names() {
		let first = agents_md_section("read_file", "{\"a\":1}");
		let base = format!("# AGENTS.md\n\n{AGENTS_SCHEMA_HEADING}\n\n{first}");
		let out = upsert_agents_md(Some(&base), "read", &agents_md_section("read", "{\"b\":2}"));
		assert!(out.contains("{\"a\":1}"));
		assert!(out.contains("{\"b\":2}"));
		assert_eq!(out.matches(SECTION_PREFIX).count(), 2);
	}
}
