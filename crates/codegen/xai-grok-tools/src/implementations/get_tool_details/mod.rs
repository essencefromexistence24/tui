//! `get_tool_details` — full-schema lookup for a tool the model failed to call.
//!
//! The first prompt carries only the Dx Serializer Compact tool contract
//! (~1-2k tokens). That is intentionally lossy: rare/nested parameter details
//! are omitted. When the model fails to call a tool 3 times with the compact
//! contract, it can call `get_tool_details` to receive the *complete* JSON
//! schema (name, description, every parameter with types, defaults, enums)
//! for exactly the tool it got wrong — and only that tool.
//!
//! The returned schema is also recorded in the workspace `AGENTS.md`
//! (one `### Tool Schema: <name>` section per requested tool) so future
//! sessions in the same workspace already know the schema that was wrong
//! before, without growing the first prompt.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::types::output::ToolOutput;
use crate::types::tool::{ToolKind, ToolNamespace};

/// Canonical model-facing name of this tool.
pub const GET_TOOL_DETAILS_TOOL_NAME: &str = "get_tool_details";

/// Heading that groups the auto-recorded sections inside `AGENTS.md`.
pub const AGENTS_SCHEMA_HEADING: &str = "## Tool Schemas (recorded by get_tool_details)";

/// Heading prefix identifying one recorded tool-schema section.
pub const SECTION_PREFIX: &str = "### Tool Schema: ";

/// Input for the `get_tool_details` tool.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetToolDetailsInput {
    /// Name of the tool whose full schema you need (e.g. `todo_write`,
    /// `GrokBuild:read_file`, or an MCP tool like `linear__save_issue`).
    pub tool_name: String,
}

/// Meta tool returning the full schema of one tool after repeated call failures.
///
/// Schemas come from the runtime registry snapshot (`BuiltinToolSchemas`,
/// built at `finalize()` from the canonical definitions) so there is no
/// second hand-maintained schema list to drift. MCP integration tools are
/// resolved through the same `ToolIndex` that backs `search_tool`.
#[derive(Debug, Default)]
pub struct GetToolDetailsTool;

impl crate::types::tool_metadata::ToolMetadata for GetToolDetailsTool {
    fn kind(&self) -> ToolKind {
        ToolKind::GetToolDetails
    }

    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::GrokBuild
    }

    fn description_template(&self) -> &str {
        "Return the FULL JSON schema (description + every parameter with types, \
         defaults, and enums) for exactly one tool.\n\nUse this ONLY after the \
         same tool call has failed 3 times — the Dx Serializer Compact catalog \
         in the system prompt is normally sufficient. Send only `tool_name`. \
         On success the full schema is also recorded under the tool's own \
         heading in the workspace AGENTS.md so future sessions keep it."
    }
}

impl xai_tool_runtime::Tool for GetToolDetailsTool {
    type Args = GetToolDetailsInput;
    type Output = ToolOutput;

    fn id(&self) -> xai_tool_protocol::ToolId {
        xai_tool_protocol::ToolId::new(GET_TOOL_DETAILS_TOOL_NAME).expect("valid tool id")
    }

    fn description(
        &self,
        _ctx: &::xai_tool_runtime::ListToolsContext,
    ) -> xai_tool_types::ToolDescription {
        xai_tool_types::ToolDescription::new(
            GET_TOOL_DETAILS_TOOL_NAME,
            crate::types::tool_metadata::ToolMetadata::sanitized_description_template(self),
        )
    }

    fn capabilities(&self) -> xai_tool_protocol::ToolCapabilities {
        xai_tool_protocol::ToolCapabilities {
            // Writes only the local AGENTS.md index — not read-only.
            is_read_only: false,
            tool_scope: Some(xai_tool_protocol::ToolScope::Write),
            ..Default::default()
        }
    }

    async fn run(
        &self,
        ctx: xai_tool_runtime::ToolCallContext,
        input: GetToolDetailsInput,
    ) -> Result<ToolOutput, xai_tool_runtime::ToolError> {
        let tool_name = input.tool_name.trim().to_owned();
        if tool_name.is_empty() {
            return Err(xai_tool_runtime::ToolError::invalid_arguments(
                "tool_name must not be empty",
            ));
        }

        let resources = crate::types::tool_metadata::shared_resources(&ctx)?;

        // Resolve the schema. The guard is dropped before any further
        // `resources.lock()` (resolve_cwd re-locks).
        let found = {
            let guard = resources.lock().await;
            lookup_builtin(&guard, &tool_name).or_else(|| lookup_mcp(&guard, &tool_name))
        };
        let (tool_label, schema_json) = match found {
            Some(found) => found,
            None => {
                let suggestions = {
                    let guard = resources.lock().await;
                    close_matches(&guard, &tool_name)
                };
                return Err(xai_tool_runtime::ToolError::invalid_arguments(format!(
                    "Unknown tool '{tool_name}'. Closest known tools: {}. \
                     Re-check the Dx Serializer Compact catalog in the system \
                     prompt and retry the original call with the exact name.",
                    suggestions.join(", ")
                )));
            }
        };

        // Record the schema the model got wrong into the workspace AGENTS.md.
        let cwd = crate::types::tool_metadata::resolve_cwd(&ctx, &resources).await?;
        let agents_path = cwd.join("AGENTS.md");
        let record_note = match record_in_agents_md(&agents_path, &tool_label, &schema_json) {
            Ok(path) => format!("Schema recorded in {} for future sessions.", path.display()),
            Err(e) => format!("AGENTS.md not updated ({e}); schema below is still valid."),
        };

        tracing::info!(tool_name = %tool_label, "get_tool_details.resolved");

        Ok(ToolOutput::Text(format!(
            "Full schema for `{tool_label}`:\n{schema_json}\n\n{record_note}"
        ).into()))
    }
}

/// Look up a built-in tool's full definition by client name or qualified id.
///
/// Accepts `read_file`, `GrokBuild:read_file`, and (best-effort) any
/// `<namespace>:<name>` spelling via a suffix scan.
fn lookup_builtin(
    guard: &tokio::sync::MutexGuard<'_, crate::types::resources::Resources>,
    tool_name: &str,
) -> Option<(String, String)> {
    let schemas = guard.get::<crate::types::resources::BuiltinToolSchemas>()?;
    let definition = schemas.get(tool_name)?;
    let json = serde_json::to_string_pretty(definition).ok()?;
    Some((definition.function.name.clone(), json))
}

/// Look up an MCP integration tool's schema through the `ToolIndex`.
fn lookup_mcp(
    guard: &tokio::sync::MutexGuard<'_, crate::types::resources::Resources>,
    tool_name: &str,
) -> Option<(String, String)> {
    let index = guard.get::<crate::types::tool_index::ToolIndex>()?.0.clone();
    let snapshot = index.search_snapshot(tool_name, 5);
    let exact = snapshot
        .results
        .iter()
        .find(|r| r.tool_name == tool_name)?;
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "type": "function",
        "function": {
            "name": exact.tool_name,
            "description": exact.description,
            "parameters": exact.input_schema,
        },
    }))
    .ok()?;
    Some((exact.tool_name.clone(), json))
}

/// Best-effort close matches for the unknown-tool error message.
fn close_matches(
    guard: &tokio::sync::MutexGuard<'_, crate::types::resources::Resources>,
    tool_name: &str,
) -> Vec<String> {
    let needle = tool_name.to_ascii_lowercase();
    let mut names: Vec<String> = guard
        .get::<crate::types::resources::BuiltinToolSchemas>()
        .map(|schemas| {
            schemas
                .client_names()
                .filter(|n| *n != GET_TOOL_DETAILS_TOOL_NAME)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if let Some(index) = guard.get::<crate::types::tool_index::ToolIndex>() {
        let snapshot = index.0.search_snapshot(tool_name, 5);
        for r in &snapshot.results {
            if !names.contains(&r.tool_name) {
                names.push(r.tool_name.clone());
            }
        }
    }
    // Prefer substring matches, then cap the list to keep the error short.
    let mut matches: Vec<String> = names
        .iter()
        .filter(|n| n.to_ascii_lowercase().contains(&needle))
        .cloned()
        .collect();
    if matches.is_empty() {
        matches = names;
    }
    matches.truncate(8);
    matches.sort_unstable();
    matches
}

/// Render one `AGENTS.md` section for a tool schema.
pub fn agents_md_section(tool_name: &str, schema_json: &str) -> String {
    format!("{SECTION_PREFIX}{tool_name}\n```json\n{schema_json}\n```\n")
}

/// Insert or replace a tool's section within the full `AGENTS.md` contents.
///
/// Pure string transform so it is unit-testable without a filesystem. When
/// `existing` is `None` a fresh file body is created.
pub fn upsert_agents_md(existing: Option<&str>, tool_name: &str, section: &str) -> String {
    let heading = format!("{SECTION_PREFIX}{tool_name}");
    let Some(existing) = existing else {
        // Fresh file: intro + grouped heading + the section.
        return format!(
            "# AGENTS.md\n\nNotes recorded by the Dx agent for this workspace.\n\n\
             {AGENTS_SCHEMA_HEADING}\n\n{section}"
        );
    };

    let section = if section.ends_with('\n') {
        section.to_owned()
    } else {
        format!("{section}\n")
    };

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
        let line_end = contents[line_start..]
            .find('\n')
            .map_or(contents.len(), |n| line_start + n);
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
    let updated = upsert_agents_md(
        existing.as_deref(),
        tool_name,
        &agents_md_section(tool_name, schema_json),
    );
    std::fs::write(path, updated).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::definition::ToolDefinition;

    fn sample_def_json() -> String {
        let def = ToolDefinition::function(
            "todo_write",
            Some("Write todos"),
            serde_json::json!({
                "type": "object",
                "properties": {"merge": {"type": "boolean", "default": true}},
                "required": ["todos"]
            }),
        );
        serde_json::to_string_pretty(&def).unwrap()
    }

    #[test]
    fn fresh_agents_md_contains_heading_and_section() {
        let out = upsert_agents_md(None, "todo_write", &agents_md_section("todo_write", "{}"));
        assert!(out.starts_with("# AGENTS.md"));
        assert!(out.contains(AGENTS_SCHEMA_HEADING));
        assert!(out.contains("### Tool Schema: todo_write"));
        assert!(out.contains("```json\n{}\n```"));
    }

    #[test]
    fn append_adds_section_under_existing_file() {
        let existing = "# AGENTS.md\n\nCustom user notes.\n";
        let out = upsert_agents_md(
            Some(existing),
            "grep",
            &agents_md_section("grep", "{\"type\":\"object\"}"),
        );
        assert!(out.contains("Custom user notes."));
        assert!(out.contains(AGENTS_SCHEMA_HEADING));
        assert!(out.ends_with("```\n"));
        assert!(out.contains("### Tool Schema: grep"));
        // The user's own content stays before the auto-recorded heading.
        let heading_pos = out.find(AGENTS_SCHEMA_HEADING).unwrap();
        let notes_pos = out.find("Custom user notes.").unwrap();
        assert!(notes_pos < heading_pos);
    }

    #[test]
    fn upsert_replaces_only_the_named_tools_section() {
        let first = agents_md_section("todo_write", "{\"old\":true}");
        let second = agents_md_section("grep", "{\"grep\":true}");
        let base = format!("# AGENTS.md\n\n{AGENTS_SCHEMA_HEADING}\n\n{first}{second}");
        let out = upsert_agents_md(
            Some(&base),
            "todo_write",
            &agents_md_section("todo_write", "{\"new\":true}"),
        );
        assert!(out.contains("{\"new\":true}"));
        assert!(!out.contains("{\"old\":true}"));
        assert!(out.contains("{\"grep\":true}"));
        assert_eq!(out.matches(SECTION_PREFIX).count(), 2);
    }

    #[test]
    fn section_prefix_is_not_confused_with_other_tool_names() {
        // `read_file` must not match `### Tool Schema: read_file_concise`.
        let first = agents_md_section("read_file_concise", "{\"a\":1}");
        let base = format!("# AGENTS.md\n\n{AGENTS_SCHEMA_HEADING}\n\n{first}");
        let out = upsert_agents_md(
            Some(&base),
            "read_file",
            &agents_md_section("read_file", "{\"b\":2}"),
        );
        assert!(out.contains("{\"a\":1}"));
        assert!(out.contains("{\"b\":2}"));
        assert_eq!(out.matches(SECTION_PREFIX).count(), 2);
    }

    #[test]
    fn find_section_start_matches_full_heading_only() {
        let base = format!(
            "{AGENTS_SCHEMA_HEADING}\n\n{}\n",
            agents_md_section("todo_write", "{}")
        );
        assert_eq!(
            find_section_start(&base, &format!("{SECTION_PREFIX}todo_write")),
            Some(AGENTS_SCHEMA_HEADING.len() + 2)
        );
        assert_eq!(
            find_section_start(&base, &format!("{SECTION_PREFIX}todo_writex")),
            None
        );
    }

    #[test]
    fn record_writes_and_updates_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("AGENTS.md");
        let json = sample_def_json();
        record_in_agents_md(&path, "todo_write", &json).unwrap();
        let first = std::fs::read_to_string(&path).unwrap();
        assert!(first.contains("### Tool Schema: todo_write"));
        // Second call replaces rather than duplicates.
        record_in_agents_md(&path, "todo_write", &json).unwrap();
        let second = std::fs::read_to_string(&path).unwrap();
        assert_eq!(second.matches(SECTION_PREFIX).count(), 1);
    }
}
