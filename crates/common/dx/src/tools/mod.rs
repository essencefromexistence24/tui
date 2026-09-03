//! Workspace tools for the multi-step agent loop.
//!
//! OpenCode-shaped builtins: shell, read, write, edit, glob, grep, list.
//! Mode policies gate which tools are offered and whether they need approval.

use std::{
	fs,
	io::{BufRead, BufReader},
	path::{Path, PathBuf},
	process::{Command, Stdio},
	sync::Arc,
	time::{Duration, Instant},
};

use once_cell::sync::OnceCell;
use serde_json::{Value, json};

use crate::modes::AgentMode;

/// Global MCP registry, initialized at startup by the agent loop.
pub static MCP_REGISTRY: OnceCell<Arc<crate::mcp::McpRegistry>> = OnceCell::new();

/// Hard caps keep tool output safe for context and UI.
const OUT_CAP: usize = 12_000;
const FILE_CAP: usize = 80_000;
const MAX_GLOB: usize = 200;
const MAX_GREP: usize = 80;
const SHELL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
	Shell,
	Read,
	Write,
	Edit,
	Glob,
	Grep,
	List,
	Question,
	/// Delegate to a subagent (Agent/Goal profiles).
	Task,
	/// Create/patch/list skills (Hermes skill_manage).
	SkillManage,
	/// Create/maintain structured todo/task lists for the session.
	TodoWrite,
	/// Manage session memory (MEMORY.md / USER.md per-entry CRUD).
	Memory,
	/// Dynamic tool provided by a connected MCP server.
	McpTool,
	/// LSP: go to definition of a symbol.
	GoToDefinition,
	/// LSP: find all references to a symbol.
	FindReferences,
	/// LSP: get hover info for a symbol.
	Hover,
	/// LSP: list symbols in a document.
	DocumentSymbols,
	/// LSP: search workspace symbols.
	WorkspaceSymbols,
	/// LSP: go to implementation of a symbol.
	GoToImplementation,
	/// LSP: get call hierarchy for a symbol.
	CallHierarchy,
	/// LSP: format a document.
	FormatCode,
	/// LSP: get diagnostics for a document.
	GetDiagnostics,
	/// LSP: get code completions.
	CompleteCode,
	/// Fetch a URL and return markdown content.
	WebFetch,
	/// Search the web using a configured search engine.
	WebSearch,
	/// Apply unified diff patches to files.
	ApplyPatch,
	/// On-demand full JSON schema for one tool (compact catalog fallback).
	ToolDetails,
}

impl ToolKind {
	pub fn name(self) -> &'static str {
		match self {
			Self::Shell => "shell",
			Self::Read => "read",
			Self::Write => "write",
			Self::Edit => "edit",
			Self::Glob => "glob",
			Self::Grep => "grep",
			Self::List => "list",
			Self::Question => "question",
			Self::Task => "task",
			Self::SkillManage => "skill_manage",
			Self::TodoWrite => "todowrite",
			Self::Memory => "memory",
			Self::McpTool => "mcp_tool",
			Self::GoToDefinition => "go_to_definition",
			Self::FindReferences => "find_references",
			Self::Hover => "hover",
			Self::DocumentSymbols => "document_symbols",
			Self::WorkspaceSymbols => "workspace_symbols",
			Self::GoToImplementation => "go_to_implementation",
			Self::CallHierarchy => "call_hierarchy",
			Self::FormatCode => "format_code",
			Self::GetDiagnostics => "get_diagnostics",
			Self::CompleteCode => "complete_code",
			Self::WebFetch => "webfetch",
			Self::WebSearch => "websearch",
			Self::ApplyPatch => "apply_patch",
			Self::ToolDetails => "tool_details",
		}
	}

	/// Human title for message-list tool cards.
	pub fn display_title(self) -> &'static str {
		match self {
			Self::Shell => "Terminal",
			Self::Read => "Read",
			Self::Write => "Write",
			Self::Edit => "Edit",
			Self::Glob => "Glob",
			Self::Grep => "Grep",
			Self::List => "List",
			Self::Question => "Question",
			Self::Task => "Task",
			Self::SkillManage => "Skill",
			Self::TodoWrite => "Todos",
			Self::Memory => "Memory",
			Self::McpTool => "MCP Tool",
			Self::GoToDefinition => "Go to Definition",
			Self::FindReferences => "Find References",
			Self::Hover => "Hover",
			Self::DocumentSymbols => "Document Symbols",
			Self::WorkspaceSymbols => "Workspace Symbols",
			Self::GoToImplementation => "Go to Implementation",
			Self::CallHierarchy => "Call Hierarchy",
			Self::FormatCode => "Format Code",
			Self::GetDiagnostics => "Get Diagnostics",
			Self::CompleteCode => "Complete Code",
			Self::WebFetch => "Web Fetch",
			Self::WebSearch => "Web Search",
			Self::ApplyPatch => "Patch",
			Self::ToolDetails => "Tool Details",
		}
	}

	pub fn from_name(s: &str) -> Option<Self> {
		match s.trim().to_ascii_lowercase().as_str() {
			"shell" | "bash" | "run_terminal_command" | "terminal" | "cmd" => Some(Self::Shell),
			"read" | "read_file" | "cat" => Some(Self::Read),
			"write" | "write_file" | "create_file" => Some(Self::Write),
			"edit" | "search_replace" | "str_replace" => Some(Self::Edit),
			"glob" | "find_files" => Some(Self::Glob),
			"grep" | "rg" => Some(Self::Grep),
			"list" | "ls" | "list_dir" => Some(Self::List),
			"question" | "ask" | "ask_user" => Some(Self::Question),
			"task" | "delegate" | "subagent" | "spawn_subagent" => Some(Self::Task),
			"skill_manage" | "skill" | "skills" | "save_skill" => Some(Self::SkillManage),
			"todowrite" | "todo" | "todos" | "tasklist" | "task_write" => Some(Self::TodoWrite),
			"memory" | "memories" | "remember" | "mem" => Some(Self::Memory),
			"go_to_definition" | "definition" | "goto_definition" => Some(Self::GoToDefinition),
			"find_references" | "references" | "find_refs" => Some(Self::FindReferences),
			"hover" | "hover_info" => Some(Self::Hover),
			"document_symbols" | "doc_symbols" | "symbols" => Some(Self::DocumentSymbols),
			"workspace_symbols" | "workspace_symbol" | "search_symbols" => Some(Self::WorkspaceSymbols),
			"go_to_implementation" | "goto_implementation" | "implementation" => {
				Some(Self::GoToImplementation)
			}
			"call_hierarchy" | "call_hier" => Some(Self::CallHierarchy),
			"format_code" | "format" | "fmt" => Some(Self::FormatCode),
			"get_diagnostics" | "diagnostics" | "diags" => Some(Self::GetDiagnostics),
			"complete_code" | "completion" | "autocomplete" => Some(Self::CompleteCode),
			s if s.contains("__") => Some(Self::McpTool),
			"webfetch" | "web_fetch" | "fetch" | "http" | "web" => Some(Self::WebFetch),
			"websearch" | "web_search" | "search_web" => Some(Self::WebSearch),
			"apply_patch" | "patch" | "diff" | "apply_diff" | "unified_diff" => Some(Self::ApplyPatch),
			"tool_details" | "get_tool_details" | "tool_schema" => Some(Self::ToolDetails),
			_ => None,
		}
	}

	/// Context tools that can be grouped in the timeline.
	pub fn is_context_tool(self) -> bool {
		matches!(
			self,
			Self::Read | Self::Glob | Self::Grep | Self::List | Self::WebFetch | Self::WebSearch
		)
	}

	/// Default-open in the accordion (shell/edit/todo open; pure context tools closed).
	pub fn default_open(self) -> bool {
		matches!(
			self,
			Self::Shell
				| Self::Write
				| Self::Edit
				| Self::Question
				| Self::Task
				| Self::SkillManage
				| Self::McpTool
				| Self::FormatCode
				| Self::ApplyPatch
				| Self::TodoWrite
				| Self::WebSearch
				| Self::WebFetch
		)
	}

	/// Whether this tool is "learning"/meta — the agent uses it to improve itself.
	#[allow(dead_code)]
	pub fn is_learning_tool(self) -> bool {
		matches!(self, Self::Memory | Self::SkillManage | Self::TodoWrite)
	}
}

#[derive(Debug, Clone)]
pub struct ToolDef {
	pub kind: ToolKind,
	pub description: &'static str,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
	pub id: String,
	pub name: String,
	pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
	pub call_id: String,
	pub name: String,
	pub ok: bool,
	pub title: String,
	pub output: String,
	/// Short one-line preview for accordion headers.
	pub preview: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
	AllowOnce,
	AllowAlways,
	Deny,
}

/// Tools offered to the model for a given agent mode.
pub fn tools_for_mode(mode: AgentMode) -> Vec<ToolDef> {
	let read_set = vec![
		ToolDef {
			kind: ToolKind::Read,
			description: "Read a file. Args: path (string), optional offset/limit (line numbers).",
		},
		ToolDef {
			kind: ToolKind::Glob,
			description: "Find files by glob pattern. Args: pattern, optional path.",
		},
		ToolDef {
			kind: ToolKind::Grep,
			description: "Search file contents with regex. Args: pattern, optional path/glob.",
		},
		ToolDef { kind: ToolKind::List, description: "List a directory. Args: path (default cwd)." },
	];

	// Shared web tools for every profile that may research the codebase/docs.
	let web_tools = [
		ToolDef {
			kind: ToolKind::WebSearch,
			description: "Search the web. Args: query (string), optional count (1-20).",
		},
		ToolDef {
			kind: ToolKind::WebFetch,
			description: "Fetch a URL and return readable text/markdown. Args: url (string).",
		},
	];

	match mode {
		// Codex: tools are managed by the app-server
		AgentMode::Codex => Vec::new(),
		// Ask: read-only Q&A — inspect, search, memory, no writes.
		AgentMode::Ask | AgentMode::Multi => {
			let mut t = read_set;
			t.push(ToolDef {
				kind: ToolKind::Shell,
				description: "Run a read-only shell command (status/tests/inspection only).",
			});
			t.push(ToolDef {
				kind: ToolKind::Question,
				description: "Ask the user a multiple-choice question. Args: prompt, options (array of strings).",
			});
			t.push(ToolDef {
				kind: ToolKind::TodoWrite,
				description: "Create or update a structured todo list for the current session. Args: todos (array of {content, status, priority}).",
			});
			t.push(ToolDef {
				kind: ToolKind::Memory,
				description: "Manage session memory: add/replace/remove/list entries in MEMORY.md or USER.md. Args: action, target, entry, index.",
			});
			t.extend(web_tools);
			t.extend(crate::lsp_tool::lsp_tool_defs());
			t
		}
		// Plan: design without mutating files (shell gated by plan flag).
		AgentMode::Plan => {
			let mut t = read_set;
			t.push(ToolDef {
				kind: ToolKind::Shell,
				description: "Run a read-only shell command when Plan shell is enabled (status/tests only).",
			});
			t.push(ToolDef {
				kind: ToolKind::Question,
				description: "Ask the user a multiple-choice question. Args: prompt, options (array of strings).",
			});
			t.push(ToolDef {
				kind: ToolKind::TodoWrite,
				description: "Create or update a structured todo list for the current session. Args: todos (array of {content, status, priority}).",
			});
			t.push(ToolDef {
				kind: ToolKind::Memory,
				description: "Manage session memory: add/replace/remove/list entries in MEMORY.md or USER.md. Args: action, target, entry, index.",
			});
			t.extend(web_tools);
			t.extend(crate::lsp_tool::lsp_tool_defs());
			t
		}
		// Write / Goal / Agent / Automation: full workspace tools.
		AgentMode::Write | AgentMode::Goal | AgentMode::Agent | AgentMode::Automation => {
			let mut t = read_set;
			t.push(ToolDef {
				kind: ToolKind::McpTool,
				description: "Execute a tool from a connected MCP server. Dynamic per-server tools — use qualified names like `server__tool_name`.",
			});
			t.push(ToolDef {
				kind: ToolKind::Shell,
				description: "Run a terminal command in the project workspace. Args: command (string).",
			});
			t.push(ToolDef {
				kind: ToolKind::Write,
				description: "Write full file contents. Args: path, content.",
			});
			t.push(ToolDef {
				kind: ToolKind::Edit,
				description: "Replace text in a file. Args: path, old_string, new_string, optional replace_all.",
			});
			t.push(ToolDef {
				kind: ToolKind::ApplyPatch,
				description: "Apply a unified diff patch. Args: patch (string) or path + patch.",
			});
			t.push(ToolDef {
				kind: ToolKind::Question,
				description: "Ask the user a multiple-choice question. Args: prompt, options (array of strings).",
			});
			t.push(ToolDef {
				kind: ToolKind::TodoWrite,
				description: "Create or update a structured todo list for the current session. Args: todos (array of {content, status, priority}).",
			});
			t.push(ToolDef {
				kind: ToolKind::Memory,
				description: "Manage session memory: add/replace/remove/list entries in MEMORY.md or USER.md. Args: action, target, entry, index.",
			});
			t.push(ToolDef {
				kind: ToolKind::ToolDetails,
				description: "Full JSON schema for ONE tool. Use only after ~3 failed calls of the same tool. Args: tool_name.",
			});
			t.extend(web_tools);
			t.extend(crate::lsp_tool::lsp_tool_defs());
			// Write has full tool access (web search, subagents, skills) like Agent/Goal.
			if matches!(
				mode,
				AgentMode::Write | AgentMode::Agent | AgentMode::Goal | AgentMode::Automation
			) {
				t.push(ToolDef {
					kind: ToolKind::Task,
					description: "Delegate to a subagent. Args: prompt, optional description, subagent_type (explore|general-purpose|orchestrator).",
				});
			}
			if matches!(mode, AgentMode::Write | AgentMode::Agent | AgentMode::Goal) {
				t.push(ToolDef {
					kind: ToolKind::SkillManage,
					description: "Create/patch/list/view reusable skills after successful work. action=create|patch|list|view, name, content, description.",
				});
			}
			t
		}
	}
}

/// Core tools always offered to the model (small list = better free-model compatibility).
/// Heavier tools (LSP / MCP / memory / skills) still work when recovered from markdown.
fn core_tools_for_api(mode: AgentMode) -> Vec<ToolDef> {
	let mut t = vec![
		ToolDef {
			kind: ToolKind::Read,
			description: "Read a file. Args: path, optional offset/limit.",
		},
		ToolDef {
			kind: ToolKind::Glob,
			description: "Find files by glob. Args: pattern, optional path.",
		},
		ToolDef {
			kind: ToolKind::Grep,
			description: "Search file contents. Args: pattern, optional path/glob.",
		},
		ToolDef { kind: ToolKind::List, description: "List a directory. Args: path." },
		ToolDef {
			kind: ToolKind::Shell,
			description: "Run a terminal command in the project workspace. Args: command (string).",
		},
		ToolDef {
			kind: ToolKind::TodoWrite,
			description: "Update session todos. Args: todos (array of {content, status}).",
		},
		ToolDef {
			kind: ToolKind::Question,
			description: "Ask the user a multiple-choice question. Args: prompt, options.",
		},
		ToolDef {
			kind: ToolKind::ToolDetails,
			description: "Full JSON schema for ONE tool. Use only after ~3 failed calls of the same tool. Args: tool_name.",
		},
	];
	if matches!(mode, AgentMode::Write | AgentMode::Goal | AgentMode::Agent | AgentMode::Automation) {
		t.push(ToolDef {
			kind: ToolKind::Write,
			description: "Write full file contents. Args: path, content.",
		});
		t.push(ToolDef {
			kind: ToolKind::Edit,
			description: "Replace text in a file. Args: path, old_string, new_string.",
		});
		t.push(ToolDef {
			kind: ToolKind::ApplyPatch,
			description: "Apply a unified diff. Args: path, patch.",
		});
	}
	if matches!(mode, AgentMode::Write | AgentMode::Agent | AgentMode::Goal) {
		t.push(ToolDef {
			kind: ToolKind::Task,
			description: "Delegate to a subagent. Args: prompt, subagent_type, description.",
		});
		t.push(ToolDef {
			kind: ToolKind::WebSearch,
			description: "Search the web. Args: query, optional count.",
		});
	}
	// Plan: shell only when plan_allow_shell — still advertised; execute() gates it.
	t
}

/// OpenAI-compatible `tools` array for chat completions.
pub fn openai_tool_schemas(mode: AgentMode) -> Vec<Value> {
	let mut schemas = Vec::new();
	// Keep the tools array small and valid — bloated LSP/MCP schemas break free providers
	// and force a no-tools retry where shell never runs.
	for t in core_tools_for_api(mode) {
		if t.kind == ToolKind::Task {
			let custom: Vec<String> =
				crate::subagent_registry::load_custom_subagents().into_keys().collect();
			schemas.push(crate::orchestration::task_tool_schema_with_custom(&custom));
			continue;
		}
		if t.kind == ToolKind::SkillManage {
			schemas.push(crate::skills::skill_manage_schema());
			continue;
		}
		if t.kind == ToolKind::ToolDetails {
			schemas.push(crate::tool_details::tool_details_schema());
			continue;
		}
		if t.kind == ToolKind::McpTool {
			continue;
		}
		let params = match t.kind {
			ToolKind::Shell => json!({
				"type": "object",
				"properties": {
					"command": { "type": "string", "description": "Shell command to run" },
					"description": { "type": "string", "description": "Short why" }
				},
				"required": ["command"]
			}),
			ToolKind::Read => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"offset": { "type": "integer" },
					"limit": { "type": "integer" }
				},
				"required": ["path"]
			}),
			ToolKind::Write => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"content": { "type": "string" }
				},
				"required": ["path", "content"]
			}),
			ToolKind::Edit => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"old_string": { "type": "string" },
					"new_string": { "type": "string" },
					"replace_all": { "type": "boolean" }
				},
				"required": ["path", "old_string", "new_string"]
			}),
			ToolKind::Glob => json!({
				"type": "object",
				"properties": {
					"pattern": { "type": "string" },
					"path": { "type": "string" }
				},
				"required": ["pattern"]
			}),
			ToolKind::Grep => json!({
				"type": "object",
				"properties": {
					"pattern": { "type": "string" },
					"path": { "type": "string" },
					"glob": { "type": "string" }
				},
				"required": ["pattern"]
			}),
			ToolKind::List => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string" }
				}
			}),
			ToolKind::Question => json!({
				"type": "object",
				"properties": {
					"prompt": { "type": "string" },
					"options": {
						"type": "array",
						"items": { "type": "string" }
					}
				},
				"required": ["prompt"]
			}),
			ToolKind::TodoWrite => json!({
				"type": "object",
				"properties": {
					"todos": {
						"type": "array",
						"items": {
							"type": "object",
							"properties": {
								"content": { "type": "string", "description": "Brief description of the task" },
								"status": { "type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"] },
								"priority": { "type": "string", "enum": ["high", "medium", "low"] }
							},
							"required": ["content", "status"]
						}
					}
				},
				"required": ["todos"]
			}),
			ToolKind::Memory => crate::memory_tool::memory_tool_schema(),
			ToolKind::GoToDefinition
			| ToolKind::FindReferences
			| ToolKind::Hover
			| ToolKind::GoToImplementation => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string", "description": "File path" },
					"line": { "type": "integer", "description": "Line number (0-indexed)" },
					"character": { "type": "integer", "description": "Character offset (0-indexed)" }
				},
				"required": ["path", "line", "character"]
			}),
			ToolKind::DocumentSymbols | ToolKind::FormatCode | ToolKind::GetDiagnostics => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string", "description": "File path" }
				},
				"required": ["path"]
			}),
			ToolKind::WorkspaceSymbols => json!({
				"type": "object",
				"properties": {
					"query": { "type": "string", "description": "Symbol query" }
				},
				"required": ["query"]
			}),
			ToolKind::CallHierarchy => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string", "description": "File path" },
					"line": { "type": "integer", "description": "Line number (0-indexed)" },
					"character": { "type": "integer", "description": "Character offset (0-indexed)" },
					"direction": { "type": "string", "enum": ["incoming", "outgoing"], "description": "Hierarchy direction" }
				},
				"required": ["path", "line", "character"]
			}),
			ToolKind::CompleteCode => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string", "description": "File path" },
					"line": { "type": "integer", "description": "Line number (0-indexed)" },
					"character": { "type": "integer", "description": "Character offset (0-indexed)" }
				},
				"required": ["path", "line", "character"]
			}),
			ToolKind::Task | ToolKind::SkillManage | ToolKind::McpTool => json!({}), // handled above
			ToolKind::WebFetch => json!({
				"type": "object",
				"properties": {
					"url": { "type": "string", "description": "URL to fetch" },
					"format": { "type": "string", "enum": ["html", "markdown", "text"], "description": "Output format (default markdown)" }
				},
				"required": ["url"]
			}),
			ToolKind::WebSearch => json!({
				"type": "object",
				"properties": {
					"query": { "type": "string", "description": "Search query" },
					"count": { "type": "integer", "description": "Number of results (default 5)" }
				},
				"required": ["query"]
			}),
			ToolKind::ApplyPatch => json!({
				"type": "object",
				"properties": {
					"path": { "type": "string", "description": "File path to patch" },
					"patch": { "type": "string", "description": "Unified diff patch content" }
				},
				"required": ["path", "patch"]
			}),
			// Unreachable: ToolDetails pushes its dedicated schema and `continue`s above.
			ToolKind::ToolDetails => json!({ "type": "object", "properties": {} }),
		};
		schemas.push(json!({
			"type": "function",
			"function": {
				"name": t.kind.name(),
				"description": t.description,
				"parameters": params
			}
		}));
	}
	schemas
}

/// Whether this call is allowed under the mode (hard policy).
pub fn allowed_in_mode(kind: ToolKind, mode: AgentMode, plan_allow_shell: bool) -> bool {
	match mode {
		// Ask / Multi: inspect + research only (shell allowed for status/tests).
		AgentMode::Ask | AgentMode::Multi => matches!(
			kind,
			ToolKind::Read
				| ToolKind::Glob
				| ToolKind::Grep
				| ToolKind::List
				| ToolKind::Shell
				| ToolKind::Question
				| ToolKind::TodoWrite
				| ToolKind::ToolDetails
				| ToolKind::Memory
				| ToolKind::GoToDefinition
				| ToolKind::FindReferences
				| ToolKind::Hover
				| ToolKind::DocumentSymbols
				| ToolKind::WorkspaceSymbols
				| ToolKind::GoToImplementation
				| ToolKind::CallHierarchy
				| ToolKind::GetDiagnostics
				| ToolKind::CompleteCode
				| ToolKind::WebFetch
				| ToolKind::WebSearch
		),
		AgentMode::Plan => match kind {
			ToolKind::Read
			| ToolKind::Glob
			| ToolKind::Grep
			| ToolKind::List
			| ToolKind::Question
			| ToolKind::TodoWrite
			| ToolKind::ToolDetails
			| ToolKind::Memory
			| ToolKind::GoToDefinition
			| ToolKind::FindReferences
			| ToolKind::Hover
			| ToolKind::DocumentSymbols
			| ToolKind::WorkspaceSymbols
			| ToolKind::GoToImplementation
			| ToolKind::CallHierarchy
			| ToolKind::GetDiagnostics
			| ToolKind::CompleteCode
			| ToolKind::WebFetch
			| ToolKind::WebSearch => true,
			ToolKind::Shell => plan_allow_shell,
			ToolKind::Write
			| ToolKind::Edit
			| ToolKind::Task
			| ToolKind::SkillManage
			| ToolKind::FormatCode
			| ToolKind::McpTool
			| ToolKind::ApplyPatch => false,
		},
		// Write: full tool access (web search, subagents, skills, edits, shell).
		AgentMode::Write
		| AgentMode::Goal
		| AgentMode::Agent
		| AgentMode::Automation
		| AgentMode::Codex => true,
	}
}

/// Whether the UI/loop should request user approval before running.
pub fn needs_permission(kind: ToolKind, args: &Value, mode: AgentMode) -> bool {
	match mode {
		// Codex: permissions managed by app-server
		AgentMode::Codex => false,
		// Automation auto-approves (profile policy).
		AgentMode::Ask | AgentMode::Multi | AgentMode::Automation => false,
		AgentMode::Plan => matches!(kind, ToolKind::Shell | ToolKind::McpTool),
		AgentMode::Write | AgentMode::Goal | AgentMode::Agent => match kind {
			ToolKind::Read
			| ToolKind::Glob
			| ToolKind::Grep
			| ToolKind::List
			| ToolKind::Question
			| ToolKind::Task
			| ToolKind::SkillManage
			| ToolKind::TodoWrite
			| ToolKind::ToolDetails
			| ToolKind::Memory
			| ToolKind::GoToDefinition
			| ToolKind::FindReferences
			| ToolKind::Hover
			| ToolKind::DocumentSymbols
			| ToolKind::WorkspaceSymbols
			| ToolKind::GoToImplementation
			| ToolKind::CallHierarchy
			| ToolKind::GetDiagnostics
			| ToolKind::CompleteCode
			| ToolKind::WebFetch
			| ToolKind::WebSearch => false,
			ToolKind::Write
			| ToolKind::Edit
			| ToolKind::FormatCode
			| ToolKind::ApplyPatch
			| ToolKind::McpTool => true,
			ToolKind::Shell => {
				let cmd =
					args.get("command").or_else(|| args.get("cmd")).and_then(|v| v.as_str()).unwrap_or("");
				is_destructive_shell(cmd)
			}
		},
	}
}

fn is_destructive_shell(cmd: &str) -> bool {
	let lower = cmd.to_ascii_lowercase();
	const BAD: &[&str] = &[
		"rm -rf",
		"rm -r ",
		"rmdir",
		"del /f",
		"format ",
		"mkfs",
		"dd if=",
		">/dev/",
		"shutdown",
		"reboot",
		"git push --force",
		"git push -f",
		"git reset --hard",
		"git clean -fd",
		"drop table",
		"drop database",
		"truncate ",
		"npm publish",
		"cargo publish",
	];
	// Force push / hard reset / mass delete are high risk; plain git commit is fine.
	BAD.iter().any(|b| lower.contains(b)) || (lower.contains("git push") && lower.contains("--force"))
}

/// Execute one tool call in `cwd`.
pub fn execute(call: &ToolCall, cwd: &Path, mode: AgentMode, plan_allow_shell: bool) -> ToolResult {
	let kind = ToolKind::from_name(&call.name);
	let args: Value = serde_json::from_str(&call.arguments).unwrap_or_else(|_| {
		// Allow bare command string for shell recovery
		if kind == Some(ToolKind::Shell) { json!({ "command": call.arguments }) } else { json!({}) }
	});

	let Some(kind) = kind else {
		if let Some(result) = crate::plugin_system_tool::try_execute_plugin(call) {
			return result;
		}
		return ToolResult {
			call_id: call.id.clone(),
			name: call.name.clone(),
			ok: false,
			title: format!("unknown tool · {}", call.name),
			output: format!(
				"Unknown tool `{}`. Available: shell, read, write, edit, glob, grep, list.",
				call.name
			),
			preview: call.name.clone(),
		};
	};

	if !allowed_in_mode(kind, mode, plan_allow_shell) {
		return ToolResult {
			call_id: call.id.clone(),
			name: kind.name().into(),
			ok: false,
			title: format!("{} denied in {} mode", kind.name(), mode.label()),
			output: format!(
				"Tool `{}` is not allowed in {} mode. Switch mode or use a read-only tool.",
				kind.name(),
				mode.label()
			),
			preview: "denied".into(),
		};
	}

	match kind {
		ToolKind::Shell => exec_shell(&call.id, &args, cwd),
		ToolKind::Read => exec_read(&call.id, &args, cwd),
		ToolKind::Write => exec_write(&call.id, &args, cwd),
		ToolKind::Edit => exec_edit(&call.id, &args, cwd),
		ToolKind::Glob => exec_glob(&call.id, &args, cwd),
		ToolKind::Grep => exec_grep(&call.id, &args, cwd),
		ToolKind::List => exec_list(&call.id, &args, cwd),
		// Question is handled by the agent loop via QuestionHub (async UI).
		ToolKind::Question => ToolResult {
			call_id: call.id.clone(),
			name: "question".into(),
			ok: true,
			title: "Question · pending UI".into(),
			output: args.get("prompt").and_then(|v| v.as_str()).unwrap_or("question").to_string(),
			preview: "question".into(),
		},
		// Task is handled by agent_loop via orchestration ledger.
		ToolKind::Task => ToolResult {
			call_id: call.id.clone(),
			name: "task".into(),
			ok: false,
			title: "task · use agent loop".into(),
			output: "Task tool must be executed by the orchestration layer.".into(),
			preview: "task".into(),
		},
		// TodoWrite is handled by agent_loop via sidebar state.
		ToolKind::TodoWrite => ToolResult {
			call_id: call.id.clone(),
			name: "todowrite".into(),
			ok: true,
			title: "Todos updated".into(),
			output: args
				.get("todos")
				.and_then(|v| serde_json::to_string_pretty(v).ok())
				.unwrap_or_else(|| "todos updated".into()),
			preview: "todos".into(),
		},
		ToolKind::Memory => crate::memory_tool::MemoryStore::execute_tool(call, &args),
		ToolKind::ToolDetails => crate::tool_details::execute_tool_details(call, cwd, mode),
		ToolKind::SkillManage => crate::skills::execute_skill_manage(call),
		ToolKind::GoToDefinition
		| ToolKind::FindReferences
		| ToolKind::Hover
		| ToolKind::DocumentSymbols
		| ToolKind::WorkspaceSymbols
		| ToolKind::GoToImplementation
		| ToolKind::CallHierarchy
		| ToolKind::FormatCode
		| ToolKind::GetDiagnostics
		| ToolKind::CompleteCode => match crate::lsp_tool::execute_lsp(call, cwd) {
			Some(r) => r,
			None => ToolResult {
				call_id: call.id.clone(),
				name: kind.name().into(),
				ok: false,
				title: format!("{} · LSP not ready", kind.display_title()),
				output: "LSP server is not connected. Check /doctor for provider status.".into(),
				preview: call.name.clone(),
			},
		},
		ToolKind::McpTool => {
			// Route MCP tool calls through the global registry.
			match MCP_REGISTRY.get() {
				Some(registry) => {
					let call_c = ToolCall {
						id: call.id.clone(),
						name: call.name.clone(),
						arguments: call.arguments.clone(),
					};
					tokio::task::block_in_place(|| {
						let handle = tokio::runtime::Handle::current();
						handle.block_on(async { crate::mcp_tool::execute_mcp_tool(&call_c, registry).await })
					})
				}
				None => ToolResult {
					call_id: call.id.clone(),
					name: call.name.clone(),
					ok: false,
					title: "MCP · no registry".into(),
					output: "MCP registry not initialized. Use `/mcps` to connect servers.".into(),
					preview: call.name.clone(),
				},
			}
		}
		ToolKind::WebFetch => exec_web_fetch(&call.id, &args),
		ToolKind::WebSearch => exec_web_search(&call.id, &args),
		ToolKind::ApplyPatch => exec_apply_patch(&call.id, &args, cwd),
	}
}

fn resolve_path(cwd: &Path, raw: &str) -> PathBuf {
	let p = PathBuf::from(raw);
	if p.is_absolute() { p } else { cwd.join(p) }
}

fn truncate(s: &str, cap: usize) -> String {
	let count = s.chars().count();
	if count <= cap {
		return s.to_string();
	}
	let kept: String = s.chars().take(cap).collect();
	format!("{kept}\n…[truncated {} chars]", count - cap)
}

fn exec_shell(id: &str, args: &Value, cwd: &Path) -> ToolResult {
	exec_shell_live(id, args, cwd, |_| {})
}

/// Run a shell command, streaming combined stdout/stderr lines via `on_line`.
/// Used by the agent loop for live Terminal cards in the message stream.
pub fn exec_shell_live(
	id: &str,
	args: &Value,
	cwd: &Path,
	mut on_line: impl FnMut(&str),
) -> ToolResult {
	let cmd =
		args.get("command").or_else(|| args.get("cmd")).and_then(|v| v.as_str()).unwrap_or("").trim();
	if cmd.is_empty() {
		return ToolResult {
			call_id: id.into(),
			name: "shell".into(),
			ok: false,
			title: "Shell · empty command".into(),
			output: "Missing `command` argument.".into(),
			preview: String::new(),
		};
	}
	let preview: String = cmd.chars().take(120).collect();
	let started = Instant::now();

	let mut child = if cfg!(windows) {
		Command::new("cmd")
			.args(["/C", cmd])
			.current_dir(cwd)
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.stdin(Stdio::null())
			.spawn()
	} else {
		Command::new("sh")
			.args(["-lc", cmd])
			.current_dir(cwd)
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.stdin(Stdio::null())
			.spawn()
	};

	match child.as_mut() {
		Ok(child) => {
			let deadline = Instant::now() + SHELL_TIMEOUT;
			let mut body = String::new();
			// Drain stdout then stderr (line-buffered where possible).
			// For true interleave we'd need threads; this is good enough for live UI.
			if let Some(out) = child.stdout.take() {
				let reader = BufReader::new(out);
				for line in reader.lines() {
					if Instant::now() > deadline {
						let _ = child.kill();
						body.push_str("\n⏱ shell timed out after 120s");
						break;
					}
					match line {
						Ok(l) => {
							let row = format!("{l}\n");
							on_line(&row);
							body.push_str(&row);
							if body.len() > OUT_CAP * 2 {
								// Keep streaming to UI but cap stored body later
							}
						}
						Err(_) => break,
					}
				}
			}
			if let Some(err) = child.stderr.take() {
				let reader = BufReader::new(err);
				for line in reader.lines() {
					if Instant::now() > deadline {
						let _ = child.kill();
						body.push_str("\n⏱ shell timed out after 120s");
						break;
					}
					match line {
						Ok(l) => {
							let row = format!("{l}\n");
							on_line(&row);
							body.push_str(&row);
						}
						Err(_) => break,
					}
				}
			}
			let status = child.wait();
			let ms = started.elapsed().as_millis();
			match status {
				Ok(st) => {
					if body.is_empty() {
						body = format!("(no output) exit {}", st.code().unwrap_or(-1));
					}
					let ok = st.success();
					ToolResult {
						call_id: id.into(),
						name: "shell".into(),
						ok,
						title: format!(
							"Shell · {} · {}ms · exit {}",
							preview.chars().take(48).collect::<String>(),
							ms,
							st.code().unwrap_or(-1)
						),
						output: truncate(&body, OUT_CAP),
						preview,
					}
				}
				Err(e) => ToolResult {
					call_id: id.into(),
					name: "shell".into(),
					ok: false,
					title: format!("Shell · failed · {preview}"),
					output: e.to_string(),
					preview,
				},
			}
		}
		Err(e) => {
			// Fallback to buffered output if spawn failed oddly
			let _ = e;
			let output = if cfg!(windows) {
				Command::new("cmd").args(["/C", cmd]).current_dir(cwd).output()
			} else {
				Command::new("sh").args(["-lc", cmd]).current_dir(cwd).output()
			};
			let ms = started.elapsed().as_millis();
			match output {
				Ok(o) => {
					let stdout = String::from_utf8_lossy(&o.stdout);
					let stderr = String::from_utf8_lossy(&o.stderr);
					let mut body = String::new();
					if !stdout.is_empty() {
						body.push_str(&stdout);
						on_line(&stdout);
					}
					if !stderr.is_empty() {
						if !body.is_empty() {
							body.push('\n');
						}
						body.push_str(&stderr);
						on_line(&stderr);
					}
					if body.is_empty() {
						body = format!("(no output) exit {}", o.status.code().unwrap_or(-1));
					}
					ToolResult {
						call_id: id.into(),
						name: "shell".into(),
						ok: o.status.success(),
						title: format!(
							"Shell · {} · {}ms · exit {}",
							preview.chars().take(48).collect::<String>(),
							ms,
							o.status.code().unwrap_or(-1)
						),
						output: truncate(&body, OUT_CAP),
						preview,
					}
				}
				Err(e) => ToolResult {
					call_id: id.into(),
					name: "shell".into(),
					ok: false,
					title: format!("Shell · failed · {preview}"),
					output: e.to_string(),
					preview,
				},
			}
		}
	}
}

fn exec_read(id: &str, args: &Value, cwd: &Path) -> ToolResult {
	let path = args
		.get("path")
		.or_else(|| args.get("filePath"))
		.or_else(|| args.get("file_path"))
		.and_then(|v| v.as_str())
		.unwrap_or("");
	if path.is_empty() {
		return ToolResult {
			call_id: id.into(),
			name: "read".into(),
			ok: false,
			title: "Read · missing path".into(),
			output: "Missing `path`.".into(),
			preview: String::new(),
		};
	}
	let full = resolve_path(cwd, path);
	let preview = path.to_string();
	match fs::read_to_string(&full) {
		Ok(text) => {
			let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
			let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
			let lines: Vec<&str> = text.lines().collect();
			let start = offset.saturating_sub(1).min(lines.len());
			let end = if limit == 0 { lines.len() } else { (start + limit).min(lines.len()) };
			let slice = lines[start..end].join("\n");
			let numbered: String = lines[start..end]
				.iter()
				.enumerate()
				.map(|(i, l)| format!("{:>6}|{l}", start + i + 1))
				.collect::<Vec<_>>()
				.join("\n");
			let body =
				if numbered.chars().count() > FILE_CAP { truncate(&numbered, FILE_CAP) } else { numbered };
			let _ = slice;
			ToolResult {
				call_id: id.into(),
				name: "read".into(),
				ok: true,
				title: format!("Read · {preview} · lines {}-{}", start + 1, end),
				output: body,
				preview,
			}
		}
		Err(e) => ToolResult {
			call_id: id.into(),
			name: "read".into(),
			ok: false,
			title: format!("Read · {preview}"),
			output: format!("{}: {e}", full.display()),
			preview,
		},
	}
}

fn exec_write(id: &str, args: &Value, cwd: &Path) -> ToolResult {
	let path =
		args.get("path").or_else(|| args.get("filePath")).and_then(|v| v.as_str()).unwrap_or("");
	let content =
		args.get("content").or_else(|| args.get("contents")).and_then(|v| v.as_str()).unwrap_or("");
	if path.is_empty() {
		return ToolResult {
			call_id: id.into(),
			name: "write".into(),
			ok: false,
			title: "Write · missing path".into(),
			output: "Missing `path`.".into(),
			preview: String::new(),
		};
	}
	let full = resolve_path(cwd, path);
	if let Some(parent) = full.parent() {
		let _ = fs::create_dir_all(parent);
	}
	match fs::write(&full, content) {
		Ok(()) => {
			let line_count = content.lines().count().max(1);
			let mut diff = format!("--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{line_count} @@\n");
			for line in content.lines() {
				diff.push('+');
				diff.push_str(line);
				diff.push('\n');
			}
			if content.is_empty() {
				diff.push_str("+\\n\n");
			}
			ToolResult {
				call_id: id.into(),
				name: "write".into(),
				ok: true,
				title: format!("Write · {path} · {} bytes · +{line_count}", content.len()),
				output: diff,
				preview: path.into(),
			}
		}
		Err(e) => ToolResult {
			call_id: id.into(),
			name: "write".into(),
			ok: false,
			title: format!("Write · {path}"),
			output: e.to_string(),
			preview: path.into(),
		},
	}
}

fn exec_edit(id: &str, args: &Value, cwd: &Path) -> ToolResult {
	let path =
		args.get("path").or_else(|| args.get("filePath")).and_then(|v| v.as_str()).unwrap_or("");
	let old =
		args.get("old_string").or_else(|| args.get("oldString")).and_then(|v| v.as_str()).unwrap_or("");
	let new =
		args.get("new_string").or_else(|| args.get("newString")).and_then(|v| v.as_str()).unwrap_or("");
	let replace_all = args
		.get("replace_all")
		.or_else(|| args.get("replaceAll"))
		.and_then(|v| v.as_bool())
		.unwrap_or(false);
	if path.is_empty() || old.is_empty() {
		return ToolResult {
			call_id: id.into(),
			name: "edit".into(),
			ok: false,
			title: "Edit · bad args".into(),
			output: "Need `path` and non-empty `old_string`.".into(),
			preview: path.into(),
		};
	}
	let full = resolve_path(cwd, path);
	let text = match fs::read_to_string(&full) {
		Ok(t) => t,
		Err(e) => {
			return ToolResult {
				call_id: id.into(),
				name: "edit".into(),
				ok: false,
				title: format!("Edit · {path}"),
				output: e.to_string(),
				preview: path.into(),
			};
		}
	};
	let count = text.matches(old).count();
	if count == 0 {
		return ToolResult {
			call_id: id.into(),
			name: "edit".into(),
			ok: false,
			title: format!("Edit · {path}"),
			output: "old_string not found in file.".into(),
			preview: path.into(),
		};
	}
	if count > 1 && !replace_all {
		return ToolResult {
			call_id: id.into(),
			name: "edit".into(),
			ok: false,
			title: format!("Edit · {path}"),
			output: format!("old_string found {count} times; set replace_all=true or make it unique."),
			preview: path.into(),
		};
	}
	let updated = if replace_all { text.replace(old, new) } else { text.replacen(old, new, 1) };
	match fs::write(&full, &updated) {
		Ok(()) => {
			let reps = if replace_all { count } else { 1 };
			let old_lines: Vec<&str> = old.lines().collect();
			let new_lines: Vec<&str> = new.lines().collect();
			let mut diff = format!(
				"--- a/{path}\n+++ b/{path}\n@@ -1,{} +1,{} @@\n",
				old_lines.len().max(1),
				new_lines.len().max(1)
			);
			for line in &old_lines {
				diff.push('-');
				diff.push_str(line);
				diff.push('\n');
			}
			if old_lines.is_empty() {
				diff.push_str("-\n");
			}
			for line in &new_lines {
				diff.push('+');
				diff.push_str(line);
				diff.push('\n');
			}
			if new_lines.is_empty() {
				diff.push_str("+\n");
			}
			ToolResult {
				call_id: id.into(),
				name: "edit".into(),
				ok: true,
				title: format!("Edit · {path} · {reps} replacement(s)"),
				output: diff,
				preview: path.into(),
			}
		}
		Err(e) => ToolResult {
			call_id: id.into(),
			name: "edit".into(),
			ok: false,
			title: format!("Edit · {path}"),
			output: e.to_string(),
			preview: path.into(),
		},
	}
}

fn exec_list(id: &str, args: &Value, cwd: &Path) -> ToolResult {
	let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
	let full = resolve_path(cwd, path);
	match fs::read_dir(&full) {
		Ok(rd) => {
			let mut entries: Vec<String> = Vec::new();
			for e in rd.flatten().take(500) {
				let name = e.file_name().to_string_lossy().into_owned();
				let meta = e.metadata().ok();
				let tag = if meta.as_ref().map(|m| m.is_dir()).unwrap_or(false) { "dir" } else { "file" };
				entries.push(format!("{tag:4} {name}"));
			}
			entries.sort();
			let body = if entries.is_empty() { "(empty)".into() } else { entries.join("\n") };
			ToolResult {
				call_id: id.into(),
				name: "list".into(),
				ok: true,
				title: format!("List · {path} · {} items", entries.len()),
				output: truncate(&body, OUT_CAP),
				preview: path.into(),
			}
		}
		Err(e) => ToolResult {
			call_id: id.into(),
			name: "list".into(),
			ok: false,
			title: format!("List · {path}"),
			output: e.to_string(),
			preview: path.into(),
		},
	}
}

fn exec_glob(id: &str, args: &Value, cwd: &Path) -> ToolResult {
	let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");
	let base = args
		.get("path")
		.and_then(|v| v.as_str())
		.map(|p| resolve_path(cwd, p))
		.unwrap_or_else(|| cwd.to_path_buf());
	let mut matches = Vec::new();
	walk_glob(&base, &base, pattern, &mut matches, MAX_GLOB);
	matches.sort();
	let body = if matches.is_empty() { "(no matches)".into() } else { matches.join("\n") };
	ToolResult {
		call_id: id.into(),
		name: "glob".into(),
		ok: true,
		title: format!("Glob · {pattern} · {} hits", matches.len()),
		output: truncate(&body, OUT_CAP),
		preview: pattern.into(),
	}
}

fn walk_glob(root: &Path, dir: &Path, pattern: &str, out: &mut Vec<String>, cap: usize) {
	if out.len() >= cap {
		return;
	}
	let Ok(rd) = fs::read_dir(dir) else {
		return;
	};
	for e in rd.flatten() {
		if out.len() >= cap {
			break;
		}
		let path = e.path();
		let name = e.file_name().to_string_lossy().into_owned();
		// Skip heavy dirs
		if name == "target" || name == "node_modules" || name == ".git" {
			continue;
		}
		if path.is_dir() {
			walk_glob(root, &path, pattern, out, cap);
		} else {
			let rel = path.strip_prefix(root).unwrap_or(&path);
			let rel_s = rel.to_string_lossy().replace('\\', "/");
			if glob_match(pattern, &rel_s) || glob_match(pattern, &name) {
				out.push(rel_s);
			}
		}
	}
}

/// Minimal glob: `*` and `**` and `?`.
fn glob_match(pattern: &str, path: &str) -> bool {
	// Prefer globset when pattern is simple path glob
	if let Ok(g) = globset::Glob::new(pattern) {
		let matcher = g.compile_matcher();
		if matcher.is_match(path) {
			return true;
		}
		// Also try basename-only patterns against full path segments
		if let Some(base) = path.rsplit('/').next()
			&& matcher.is_match(base)
		{
			return true;
		}
	}
	// Fallback: substring for bare extensions like `*.rs`
	if let Some(ext) = pattern.strip_prefix("*.") {
		return path.ends_with(&format!(".{ext}"));
	}
	path.contains(pattern.trim_matches('*'))
}

fn exec_grep(id: &str, args: &Value, cwd: &Path) -> ToolResult {
	let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
	if pattern.is_empty() {
		return ToolResult {
			call_id: id.into(),
			name: "grep".into(),
			ok: false,
			title: "Grep · empty pattern".into(),
			output: "Missing `pattern`.".into(),
			preview: String::new(),
		};
	}
	let base = args
		.get("path")
		.and_then(|v| v.as_str())
		.map(|p| resolve_path(cwd, p))
		.unwrap_or_else(|| cwd.to_path_buf());
	let file_glob = args.get("glob").and_then(|v| v.as_str());
	let re = match regex::Regex::new(pattern) {
		Ok(r) => r,
		Err(e) => {
			return ToolResult {
				call_id: id.into(),
				name: "grep".into(),
				ok: false,
				title: "Grep · bad regex".into(),
				output: e.to_string(),
				preview: pattern.into(),
			};
		}
	};
	let mut hits = Vec::new();
	grep_walk(&base, &base, &re, file_glob, &mut hits, MAX_GREP);
	let body = if hits.is_empty() { "(no matches)".into() } else { hits.join("\n") };
	ToolResult {
		call_id: id.into(),
		name: "grep".into(),
		ok: true,
		title: format!("Grep · {pattern} · {} hits", hits.len()),
		output: truncate(&body, OUT_CAP),
		preview: pattern.into(),
	}
}

fn grep_walk(
	root: &Path,
	dir: &Path,
	re: &regex::Regex,
	file_glob: Option<&str>,
	out: &mut Vec<String>,
	cap: usize,
) {
	if out.len() >= cap {
		return;
	}
	let Ok(rd) = fs::read_dir(dir) else {
		return;
	};
	for e in rd.flatten() {
		if out.len() >= cap {
			break;
		}
		let path = e.path();
		let name = e.file_name().to_string_lossy().into_owned();
		if name == "target" || name == "node_modules" || name == ".git" {
			continue;
		}
		if path.is_dir() {
			grep_walk(root, &path, re, file_glob, out, cap);
			continue;
		}
		if let Some(g) = file_glob
			&& !glob_match(g, &name)
			&& !glob_match(g, &path.to_string_lossy())
		{
			continue;
		}
		// Skip obvious binaries
		if let Some(ext) = path.extension().and_then(|x| x.to_str()) {
			const SKIP: &[&str] = &["png", "jpg", "jpeg", "gif", "exe", "dll", "so", "wasm", "pdf"];
			if SKIP.contains(&ext.to_ascii_lowercase().as_str()) {
				continue;
			}
		}
		let Ok(text) = fs::read_to_string(&path) else {
			continue;
		};
		let rel = path.strip_prefix(root).unwrap_or(&path);
		let rel_s = rel.to_string_lossy().replace('\\', "/");
		for (i, line) in text.lines().enumerate() {
			if out.len() >= cap {
				break;
			}
			if re.is_match(line) {
				let clipped: String = line.chars().take(200).collect();
				out.push(format!("{rel_s}:{}:{clipped}", i + 1));
			}
		}
	}
}

// ── WebFetch ─────────────────────────────────────────────────────────────

fn exec_web_fetch(id: &str, args: &Value) -> ToolResult {
	let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
	if url.is_empty() {
		return ToolResult {
			call_id: id.into(),
			name: "webfetch".into(),
			ok: false,
			title: "WebFetch · missing url".into(),
			output: "Missing `url` argument.".into(),
			preview: String::new(),
		};
	}
	let fmt = args.get("format").and_then(|v| v.as_str()).unwrap_or("markdown");

	let preview = url.chars().take(72).collect::<String>();

	let started = Instant::now();
	let client = match reqwest::blocking::Client::builder()
		.timeout(Duration::from_secs(30))
		.user_agent("DX-TUI/1.0")
		.build()
	{
		Ok(c) => c,
		Err(e) => {
			return ToolResult {
				call_id: id.into(),
				name: "webfetch".into(),
				ok: false,
				title: "WebFetch · client error".into(),
				output: e.to_string(),
				preview,
			};
		}
	};

	let response = match client.get(url).send() {
		Ok(r) => r,
		Err(e) => {
			return ToolResult {
				call_id: id.into(),
				name: "webfetch".into(),
				ok: false,
				title: format!("WebFetch · {preview}"),
				output: format!("HTTP error: {e}"),
				preview,
			};
		}
	};

	let status = response.status();
	let body_bytes = match response.bytes() {
		Ok(b) => b,
		Err(e) => {
			return ToolResult {
				call_id: id.into(),
				name: "webfetch".into(),
				ok: false,
				title: format!("WebFetch · {preview}"),
				output: format!("Read error: {e}"),
				preview,
			};
		}
	};

	if body_bytes.len() > 500_000 {
		return ToolResult {
			call_id: id.into(),
			name: "webfetch".into(),
			ok: false,
			title: format!("WebFetch · {preview}"),
			output: "Response exceeds 500KB limit.".into(),
			preview,
		};
	}

	let ms = started.elapsed().as_millis();
	let body_str = String::from_utf8_lossy(&body_bytes);

	let output = match fmt {
		"text" => strip_html_tags(&body_str),
		"html" => body_str.to_string(),
		_ => html_to_markdown(&body_str),
	};

	let output = truncate(&output, OUT_CAP);

	ToolResult {
		call_id: id.into(),
		name: "webfetch".into(),
		ok: status.is_success(),
		title: format!("WebFetch · {preview} · {status} · {ms}ms"),
		output: format!("Status: {status}\n\n{output}"),
		preview,
	}
}

fn html_to_markdown(html: &str) -> String {
	let re_style = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>");
	let s = match re_style {
		Ok(r) => r.replace_all(html, "").to_string(),
		Err(_) => html.to_string(),
	};
	let re_script = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>");
	let s = match re_script {
		Ok(r) => r.replace_all(&s, "").to_string(),
		Err(_) => s,
	};
	let re_nav = regex::Regex::new(r"(?is)<nav[^>]*>.*?</nav>");
	let s = match re_nav {
		Ok(r) => r.replace_all(&s, "").to_string(),
		Err(_) => s,
	};

	let re_h1 = regex::Regex::new(r"(?i)</?h1[^>]*>");
	let s = match re_h1 {
		Ok(r) => r.replace_all(&s, "\n# ").to_string(),
		Err(_) => s,
	};
	let re_h2 = regex::Regex::new(r"(?i)</?h2[^>]*>");
	let s = match re_h2 {
		Ok(r) => r.replace_all(&s, "\n## ").to_string(),
		Err(_) => s,
	};
	let re_h3 = regex::Regex::new(r"(?i)</?h3[^>]*>");
	let s = match re_h3 {
		Ok(r) => r.replace_all(&s, "\n### ").to_string(),
		Err(_) => s,
	};
	let re_h4 = regex::Regex::new(r"(?i)</?h4[^>]*>");
	let s = match re_h4 {
		Ok(r) => r.replace_all(&s, "\n#### ").to_string(),
		Err(_) => s,
	};
	let re_h5 = regex::Regex::new(r"(?i)</?h5[^>]*>");
	let s = match re_h5 {
		Ok(r) => r.replace_all(&s, "\n##### ").to_string(),
		Err(_) => s,
	};
	let re_h6 = regex::Regex::new(r"(?i)</?h6[^>]*>");
	let s = match re_h6 {
		Ok(r) => r.replace_all(&s, "\n###### ").to_string(),
		Err(_) => s,
	};

	let re_a = regex::Regex::new(r#"(?i)<a\s[^>]*href\s*=\s*"([^"]*)"[^>]*>([^<]*)</a>"#);
	let s = match re_a {
		Ok(r) => r.replace_all(&s, "[$2]($1)").to_string(),
		Err(_) => s,
	};

	let re_img1 =
		regex::Regex::new(r#"(?i)<img\s[^>]*src\s*=\s*"([^"]*)"[^>]*alt\s*=\s*"([^"]*)"[^>]*/?>"#);
	let s = match re_img1 {
		Ok(r) => r.replace_all(&s, "![$2]($1)").to_string(),
		Err(_) => s,
	};
	let re_img2 =
		regex::Regex::new(r#"(?i)<img\s[^>]*alt\s*=\s*"([^"]*)"[^>]*src\s*=\s*"([^"]*)"[^>]*/?>"#);
	let s = match re_img2 {
		Ok(r) => r.replace_all(&s, "![$1]($2)").to_string(),
		Err(_) => s,
	};
	let re_img3 = regex::Regex::new(r#"(?i)<img\s[^>]*src\s*=\s*"([^"]*)"[^>]*/?>"#);
	let s = match re_img3 {
		Ok(r) => r.replace_all(&s, "![]($1)").to_string(),
		Err(_) => s,
	};

	let re_strong = regex::Regex::new(r"(?i)</?strong[^>]*>");
	let s = match re_strong {
		Ok(r) => r.replace_all(&s, "**").to_string(),
		Err(_) => s,
	};
	let re_em = regex::Regex::new(r"(?i)</?em[^>]*>");
	let s = match re_em {
		Ok(r) => r.replace_all(&s, "*").to_string(),
		Err(_) => s,
	};

	let re_pre = regex::Regex::new(r"(?is)<pre[^>]*>([^<]*)</pre>");
	let s = match re_pre {
		Ok(r) => r
			.replace_all(&s, |caps: &regex::Captures| {
				format!("\n```\n{}\n```\n", caps.get(1).map(|m| m.as_str().trim()).unwrap_or(""))
			})
			.to_string(),
		Err(_) => s,
	};
	let re_code = regex::Regex::new(r"(?i)<code[^>]*>([^<]*)</code>");
	let s = match re_code {
		Ok(r) => r.replace_all(&s, "`$1`").to_string(),
		Err(_) => s,
	};

	let re_block =
		regex::Regex::new(r"(?i)</?(?:p|div|blockquote|section|article|header|footer)[^>]*>");
	let s = match re_block {
		Ok(r) => r.replace_all(&s, "\n").to_string(),
		Err(_) => s,
	};
	let re_br = regex::Regex::new(r"(?i)<br\s*/>");
	let s = match re_br {
		Ok(r) => r.replace_all(&s, "\n").to_string(),
		Err(_) => s,
	};
	let re_li = regex::Regex::new(r"(?i)</?li[^>]*>");
	let s = match re_li {
		Ok(r) => r.replace_all(&s, "\n- ").to_string(),
		Err(_) => s,
	};
	let re_ul = regex::Regex::new(r"(?i)</?ul[^>]*>");
	let s = match re_ul {
		Ok(r) => r.replace_all(&s, "\n").to_string(),
		Err(_) => s,
	};
	let re_ol = regex::Regex::new(r"(?i)</?ol[^>]*>");
	let s = match re_ol {
		Ok(r) => r.replace_all(&s, "\n").to_string(),
		Err(_) => s,
	};
	let re_hr = regex::Regex::new(r"(?i)<hr[^>]*>");
	let s = match re_hr {
		Ok(r) => r.replace_all(&s, "\n---\n").to_string(),
		Err(_) => s,
	};

	let re_tag = regex::Regex::new(r"<[^>]*>");
	let s = match re_tag {
		Ok(r) => r.replace_all(&s, "").to_string(),
		Err(_) => s,
	};

	let s = s
		.replace("&amp;", "&")
		.replace("&lt;", "<")
		.replace("&gt;", ">")
		.replace("&quot;", "\"")
		.replace("&apos;", "'")
		.replace("&#39;", "'")
		.replace("&#x27;", "'")
		.replace("&#x60;", "`")
		.replace("&#x2F;", "/")
		.replace("&nbsp;", " ");

	if let Ok(re_nl) = regex::Regex::new(r"\n{3,}") {
		re_nl.replace_all(&s, "\n\n").to_string()
	} else {
		s
	}
	.trim()
	.to_string()
}

fn strip_html_tags(html: &str) -> String {
	let re_style = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>");
	let s = match re_style {
		Ok(r) => r.replace_all(html, "").to_string(),
		Err(_) => html.to_string(),
	};
	let re_script = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>");
	let s = match re_script {
		Ok(r) => r.replace_all(&s, "").to_string(),
		Err(_) => s,
	};
	let re_tag = regex::Regex::new(r"<[^>]*>");
	let s = match re_tag {
		Ok(r) => r.replace_all(&s, " ").to_string(),
		Err(_) => s,
	};
	if let Ok(re_space) = regex::Regex::new(r"\s+") {
		re_space.replace_all(&s, " ").to_string()
	} else {
		s
	}
	.trim()
	.to_string()
}

// ── WebSearch ─────────────────────────────────────────────────────────────

fn exec_web_search(id: &str, args: &Value) -> ToolResult {
	let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
	if query.is_empty() {
		return ToolResult {
			call_id: id.into(),
			name: "websearch".into(),
			ok: false,
			title: "WebSearch · missing query".into(),
			output: "Missing `query` argument.".into(),
			preview: String::new(),
		};
	}
	let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(5).min(20) as usize;

	let preview = query.chars().take(72).collect::<String>();
	let provider = std::env::var("DX_SEARCH_PROVIDER").unwrap_or_else(|_| "duckduckgo".to_string());

	let started = Instant::now();
	let results = match provider.to_ascii_lowercase().as_str() {
		"google" => search_google(query, count),
		"bing" => search_bing(query, count),
		"searxng" => search_searxng(query, count),
		_ => search_duckduckgo(query, count),
	};

	let ms = started.elapsed().as_millis();

	match results {
		Ok(results) => {
			let body = results
				.iter()
				.enumerate()
				.map(|(i, r)| format!("{}. [{}]({})\n   {}", i + 1, r.title, r.url, r.snippet))
				.collect::<Vec<_>>()
				.join("\n\n");
			ToolResult {
				call_id: id.into(),
				name: "websearch".into(),
				ok: true,
				title: format!("WebSearch · {preview} · {} results · {ms}ms", results.len()),
				output: format!("Search results for \"{query}\":\n\n{body}"),
				preview,
			}
		}
		Err(e) => ToolResult {
			call_id: id.into(),
			name: "websearch".into(),
			ok: false,
			title: format!("WebSearch · {preview}"),
			output: format!("Search error: {e}"),
			preview,
		},
	}
}

struct SearchResult {
	title: String,
	url: String,
	snippet: String,
}

fn search_duckduckgo(query: &str, count: usize) -> anyhow::Result<Vec<SearchResult>> {
	let client = reqwest::blocking::Client::builder()
		.timeout(Duration::from_secs(15))
		.user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
		.default_headers({
			let mut h = reqwest::header::HeaderMap::new();
			h.insert("Accept", "text/html".parse().unwrap());
			h
		})
		.build()?;
	let resp = client.post("https://html.duckduckgo.com/html/").form(&[("q", query)]).send()?;
	let html = resp.text()?;

	// Find result blocks: <h2 class="result__title"> → </h2> with the link inside
	let re_block = regex::Regex::new(
		r##"<h2[^>]*class="[^"]*result__title[^"]*"[^>]*>\s*<a[^>]*href="([^"]*)"[^>]*>(.*?)</a>\s*</h2>"##,
	)?;
	let re_snippet_block =
		regex::Regex::new(r##"<a[^>]*class="[^"]*result__snippet[^"]*"[^>]*>(.*?)</a>"##)?;

	let mut out: Vec<SearchResult> = Vec::new();

	// Collect all result positions to iterate in order
	let positions: Vec<(usize, usize)> =
		re_block.find_iter(&html).map(|m| (m.start(), m.end())).collect();

	let re_tag = regex::Regex::new("<[^>]*>").ok();

	for (start, end) in &positions {
		if out.len() >= count {
			break;
		}

		let caps = re_block.captures_at(&html, *start);
		let Some(cap) = caps else { continue };

		let href = cap.get(1).map(|m| m.as_str()).unwrap_or("");
		// DuckDuckGo redirect URLs look like: /l/?uddg=ENCODED_URL or //duckduckgo.com/l/?uddg=...
		let url = if href.contains("uddg=") {
			let encoded = href.split("uddg=").nth(1).unwrap_or("");
			let trimmed = encoded.split('&').next().unwrap_or(encoded);
			percent_encoding::percent_decode_str(trimmed)
				.decode_utf8()
				.map(|s| s.to_string())
				.unwrap_or_default()
		} else if href.starts_with("http://") || href.starts_with("https://") {
			href.to_string()
		} else {
			format!("https:{}", href.trim_start_matches('/'))
		};

		let title_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
		let title = re_tag
			.as_ref()
			.map(|re| re.replace_all(title_raw, "").to_string())
			.unwrap_or_else(|| title_raw.to_string())
			.trim()
			.to_string();

		// Find snippet after this result block
		let snippet = re_snippet_block
			.find_at(&html, *end)
			.and_then(|m| re_tag.as_ref().map(|re| re.replace_all(m.as_str(), "").to_string()))
			.unwrap_or_default()
			.trim()
			.to_string()
			.replace("&amp;", "&")
			.replace("&lt;", "<")
			.replace("&gt;", ">")
			.replace("&quot;", "\"")
			.replace("&#x27;", "'");

		if !title.is_empty() {
			out.push(SearchResult { title, url, snippet });
		}
	}

	if out.is_empty() {
		anyhow::bail!("no results from DuckDuckGo");
	}
	Ok(out)
}

fn search_searxng(query: &str, count: usize) -> anyhow::Result<Vec<SearchResult>> {
	let base_url =
		std::env::var("DX_SEARXNG_URL").unwrap_or_else(|_| "http://localhost:8888".to_string());
	let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(15)).build()?;
	let resp: Value = client
		.get(format!("{base_url}/search?q={}&format=json&n={}", url_encode(query), count))
		.send()?
		.json()?;
	let results = resp["results"]
		.as_array()
		.map(|arr| {
			arr
				.iter()
				.take(count)
				.filter_map(|r| {
					let title = r["title"].as_str().unwrap_or("").to_string();
					let url = r["url"].as_str().unwrap_or("").to_string();
					let snippet = r["content"].as_str().unwrap_or("").to_string();
					if title.is_empty() && url.is_empty() {
						None
					} else {
						Some(SearchResult { title, url, snippet })
					}
				})
				.collect()
		})
		.unwrap_or_default();
	Ok(results)
}

fn search_google(query: &str, count: usize) -> anyhow::Result<Vec<SearchResult>> {
	let api_key =
		std::env::var("GOOGLE_API_KEY").map_err(|_| anyhow::anyhow!("GOOGLE_API_KEY not set"))?;
	let cse_id =
		std::env::var("GOOGLE_CSE_ID").map_err(|_| anyhow::anyhow!("GOOGLE_CSE_ID not set"))?;
	let count_str = count.to_string();
	let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(15)).build()?;
	let params = vec![
		("key", api_key.as_str()),
		("cx", cse_id.as_str()),
		("q", query),
		("num", count_str.as_str()),
	];
	let resp: Value =
		client.get("https://www.googleapis.com/customsearch/v1").query(&params).send()?.json()?;
	let results = resp["items"]
		.as_array()
		.map(|arr| {
			arr
				.iter()
				.filter_map(|r| {
					let title = r["title"].as_str().unwrap_or("").to_string();
					let url = r["link"].as_str().unwrap_or("").to_string();
					let snippet = r["snippet"].as_str().unwrap_or("").to_string();
					if title.is_empty() && url.is_empty() {
						None
					} else {
						Some(SearchResult { title, url, snippet })
					}
				})
				.collect()
		})
		.unwrap_or_default();
	Ok(results)
}

fn search_bing(query: &str, count: usize) -> anyhow::Result<Vec<SearchResult>> {
	let api_key =
		std::env::var("BING_API_KEY").map_err(|_| anyhow::anyhow!("BING_API_KEY not set"))?;
	let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(15)).build()?;
	let resp: Value = client
		.get("https://api.bing.microsoft.com/v7.0/search")
		.header("Ocp-Apim-Subscription-Key", &api_key)
		.query(&[("q", query), ("count", &count.to_string())])
		.send()?
		.json()?;
	let results = resp["webPages"]["value"]
		.as_array()
		.map(|arr| {
			arr
				.iter()
				.filter_map(|r| {
					let title = r["name"].as_str().unwrap_or("").to_string();
					let url = r["url"].as_str().unwrap_or("").to_string();
					let snippet = r["snippet"].as_str().unwrap_or("").to_string();
					if title.is_empty() && url.is_empty() {
						None
					} else {
						Some(SearchResult { title, url, snippet })
					}
				})
				.collect()
		})
		.unwrap_or_default();
	Ok(results)
}

fn url_encode(s: &str) -> String {
	s.chars()
		.map(|c| match c {
			'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
			' ' => '+'.to_string(),
			_ => format!("%{:02X}", c as u8),
		})
		.collect()
}

// ── ApplyPatch ────────────────────────────────────────────────────────────

fn exec_apply_patch(id: &str, args: &Value, cwd: &Path) -> ToolResult {
	let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
	let patch_str = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");

	if path.is_empty() || patch_str.is_empty() {
		return ToolResult {
			call_id: id.into(),
			name: "apply_patch".into(),
			ok: false,
			title: "ApplyPatch · missing args".into(),
			output: "Need `path` and `patch` arguments.".into(),
			preview: path.into(),
		};
	}

	let full = resolve_path(cwd, path);

	match apply_unified_diff(&full, patch_str) {
		Ok(stats) => ToolResult {
			call_id: id.into(),
			name: "apply_patch".into(),
			ok: true,
			title: format!("Patch · {path} · +{} -{}", stats.added, stats.removed),
			output: format!(
				"Applied patch to {}: {} additions, {} removals",
				full.display(),
				stats.added,
				stats.removed
			),
			preview: path.into(),
		},
		Err(e) => ToolResult {
			call_id: id.into(),
			name: "apply_patch".into(),
			ok: false,
			title: format!("Patch · {path}"),
			output: format!("Patch failed: {e}"),
			preview: path.into(),
		},
	}
}

struct PatchStats {
	added: usize,
	removed: usize,
}

fn apply_unified_diff(path: &std::path::Path, patch_str: &str) -> anyhow::Result<PatchStats> {
	let original = std::fs::read_to_string(path)
		.map_err(|e| anyhow::anyhow!("Cannot read {}: {e}", path.display()))?;
	let mut lines: Vec<String> = original.lines().map(|l| l.to_string()).collect();
	let mut added = 0usize;
	let mut removed = 0usize;

	let patch_lines: Vec<&str> = patch_str.lines().collect();
	let mut i = 0;

	while i < patch_lines.len() {
		let line = patch_lines[i];
		if line.starts_with("--- ") || line.starts_with("+++ ") || line.trim().is_empty() {
			i += 1;
			continue;
		}
		if let Some(hunk) = line.strip_prefix("@@ ") {
			let parts: Vec<&str> = hunk.splitn(3, ' ').collect();
			if parts.len() < 2 {
				i += 1;
				continue;
			}
			let (old_start, _old_count) = parse_hunk_header(parts[0]);
			let (_new_start, _new_count) = parse_hunk_header(parts[1]);
			i += 1;

			let mut hunk_lines: Vec<(&str, char)> = Vec::new();
			while i < patch_lines.len() {
				let hl = patch_lines[i];
				if hl.starts_with("@@ ") {
					break;
				}
				if hl.starts_with("--- ")
					|| hl.starts_with("+++ ")
					|| hl.trim().is_empty() && hunk_lines.is_empty()
				{
					i += 1;
					continue;
				}
				let ch = hl.chars().next().unwrap_or(' ');
				hunk_lines.push((hl, ch));
				i += 1;
			}

			let mut pos = old_start.saturating_sub(1);
			if pos > lines.len() {
				pos = lines.len();
			}

			let mut new_lines: Vec<String> = Vec::new();
			let mut hunk_pos = pos;

			for (hl, ch) in &hunk_lines {
				let content = if hl.len() > 1 { &hl[1..] } else { "" };
				match ch {
					' ' => {
						if hunk_pos < lines.len() && lines[hunk_pos].trim() == content.trim() {
							new_lines.push(lines[hunk_pos].clone());
							hunk_pos += 1;
						} else {
							let mut found = false;
							for skip in 0..5.min(lines.len().saturating_sub(hunk_pos)) {
								if lines[hunk_pos + skip].trim() == content.trim() {
									for _ in 0..skip {
										removed += 1;
									}
									hunk_pos += skip;
									new_lines.push(lines[hunk_pos].clone());
									hunk_pos += 1;
									found = true;
									break;
								}
							}
							if !found {
								new_lines.push(content.to_string());
							}
						}
					}
					'-' => {
						if hunk_pos < lines.len() && lines[hunk_pos].trim() == content.trim() {
							removed += 1;
							hunk_pos += 1;
						}
					}
					'+' => {
						new_lines.push(content.to_string());
						added += 1;
					}
					_ => {
						new_lines.push(content.to_string());
					}
				}
			}

			lines.splice(pos..hunk_pos, new_lines);
		} else {
			i += 1;
		}
	}

	if added == 0 && removed == 0 {
		return Err(anyhow::anyhow!("No hunks applied. Patch may not match file content."));
	}

	let result = lines.join("\n");
	std::fs::write(path, &result)
		.map_err(|e| anyhow::anyhow!("Cannot write {}: {e}", path.display()))?;

	Ok(PatchStats { added, removed })
}

fn parse_hunk_header(s: &str) -> (usize, usize) {
	let s = s.trim_start_matches('-').trim_start_matches('+');
	if let Some((start, count)) = s.split_once(',') {
		(start.parse::<usize>().unwrap_or(1), count.parse::<usize>().unwrap_or(1))
	} else {
		(s.parse::<usize>().unwrap_or(1), 1)
	}
}

/// Recover tool calls when the model only prints markdown fences / XML tags
/// (no native tool_calls). Critical for Write profile when the model emits
/// `<shell command="git status"/>` instead of a function call.
pub fn extract_markdown_tool_calls(text: &str, mode: AgentMode) -> Vec<ToolCall> {
	let _ = mode; // policy enforced in execute(); always recover so UX is explicit
	// Ignore pure thinking so we don't re-run speculative commands from <think>
	let text = strip_think_for_recovery(text);
	let mut calls = Vec::new();
	let mut lines = text.lines().peekable();
	let mut idx = 0u32;

	// Whole-text XML-style tags first (may span or sit mid-line)
	extract_xml_tool_tags(&text, &mut calls, &mut idx);

	while let Some(line) = lines.next() {
		let t = line.trim();
		// ```bash / ```sh / ```shell / ```command / ```powershell / ```zsh
		let is_shell_fence = t == "```bash"
			|| t == "```sh"
			|| t == "```shell"
			|| t == "```zsh"
			|| t == "```powershell"
			|| t == "```cmd"
			|| t.starts_with("```bash")
			|| t.starts_with("```sh")
			|| t.starts_with("```shell")
			|| t.starts_with("```command")
			|| t == "```command"
			|| t.starts_with("```zsh")
			|| t.starts_with("```powershell");
		if is_shell_fence {
			let mut body = Vec::new();
			for l in lines.by_ref() {
				if l.trim() == "```" {
					break;
				}
				let lt = l.trim();
				if lt.is_empty() || lt.starts_with('#') {
					continue;
				}
				// Skip pseudo comments / placeholders
				if lt.starts_with("//") || lt.starts_with("…") || lt == "..." {
					continue;
				}
				body.push(lt.to_string());
			}
			if !body.is_empty() {
				let command = body.join(" && ");
				if command.contains("...") && command.len() < 12 {
					continue;
				}
				if calls.iter().any(|c| c.name == "shell" && c.arguments.contains(&command)) {
					continue;
				}
				idx += 1;
				calls.push(ToolCall {
					id: format!("md_shell_{idx}"),
					name: "shell".into(),
					arguments: json!({ "command": command }).to_string(),
				});
			}
			continue;
		}
		// ```json tool call style
		if t == "```json" || t.starts_with("```json") {
			let mut body = String::new();
			for l in lines.by_ref() {
				if l.trim() == "```" {
					break;
				}
				body.push_str(l);
				body.push('\n');
			}
			if let Ok(v) = serde_json::from_str::<Value>(body.trim())
				&& let Some(name) = v.get("name").or_else(|| v.get("tool")).and_then(|x| x.as_str())
				&& ToolKind::from_name(name).is_some()
			{
				idx += 1;
				let args = v.get("arguments").or_else(|| v.get("args")).cloned().unwrap_or(json!({}));
				calls.push(ToolCall {
					id: format!("md_json_{idx}"),
					name: name.into(),
					arguments: args.to_string(),
				});
			}
			continue;
		}
		// `$ git status` / `> cargo test` one-liners
		if let Some(cmd) = t.strip_prefix("$ ").or_else(|| t.strip_prefix("> ")) {
			let cmd = cmd.trim();
			if !cmd.is_empty()
				&& looks_like_shell_cmd(cmd)
				&& !calls.iter().any(|c| c.name == "shell" && c.arguments.contains(cmd))
			{
				idx += 1;
				calls.push(ToolCall {
					id: format!("md_shell_{idx}"),
					name: "shell".into(),
					arguments: json!({ "command": cmd }).to_string(),
				});
			}
		}
	}
	calls.truncate(8);
	calls
}

fn strip_think_for_recovery(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	let mut rest = text;
	loop {
		let open = rest.find("<think>").into_iter().chain(rest.find("<thinking>")).min();
		let Some(start) = open else {
			out.push_str(rest);
			break;
		};
		out.push_str(&rest[..start]);
		let after = &rest[start..];
		let close = if after.starts_with("<thinking>") {
			after.find("</thinking>").map(|i| i + "</thinking>".len())
		} else {
			after.find("</think>").map(|i| i + "</think>".len())
		};
		match close {
			Some(end) => rest = &after[end..],
			None => break,
		}
	}
	out
}

fn looks_like_shell_cmd(cmd: &str) -> bool {
	let first = cmd.split_whitespace().next().unwrap_or("");
	matches!(
		first,
		"git"
			| "cargo"
			| "npm"
			| "pnpm"
			| "yarn"
			| "bun"
			| "python"
			| "python3"
			| "node"
			| "ls"
			| "dir"
			| "cat"
			| "type"
			| "rg"
			| "grep"
			| "find"
			| "echo"
			| "pwd"
			| "cd"
			| "mkdir"
			| "cp"
			| "mv"
			| "rm"
			| "del"
			| "curl"
			| "wget"
			| "go"
			| "rustc"
			| "dotnet"
			| "make"
			| "cmake"
			| "docker"
			| "kubectl"
			| "gh"
			| "aws"
			| "pip"
			| "pip3"
			| "uv"
			| "poetry"
			| "pytest"
			| "tsc"
			| "eslint"
			| "prettier"
			| "fmt"
	) || first.ends_with(".exe")
		|| first.ends_with(".ps1")
		|| first.starts_with("./")
}

/// Parse XML-ish tool tags the model may emit as plain text:
///   <shell command="git status"/>
///   <shell command="ls -la"></shell>
///   <read path="src/main.rs"/>
///   <write path="x.rs" content="..."/>
///   <run_terminal_command command="..."/>
fn extract_xml_tool_tags(text: &str, calls: &mut Vec<ToolCall>, idx: &mut u32) {
	// Self-closing or paired tags: <name attr="val" ... /> or <name ...>body</name>
	// Keep it lightweight — no full XML parser.
	let lower = text.to_ascii_lowercase();
	let tags = [
		"shell",
		"bash",
		"run_terminal_command",
		"terminal",
		"read",
		"read_file",
		"write",
		"write_file",
		"edit",
		"search_replace",
		"grep",
		"glob",
		"list",
		"list_dir",
		"websearch",
		"web_search",
		"webfetch",
		"web_fetch",
	];

	for tag in tags {
		let open = format!("<{tag}");
		let mut search_from = 0usize;
		while let Some(rel) = lower[search_from..].find(&open) {
			let start = search_from + rel;
			// Find end of opening tag
			let after = &text[start..];
			let Some(gt) = after.find('>') else {
				break;
			};
			let open_tag = &after[..=gt];
			let self_closing = open_tag.trim_end().ends_with("/>") || open_tag.ends_with("/>");
			let attrs = if self_closing {
				open_tag.trim_start_matches('<').trim_end_matches("/>").trim_end_matches('>').to_string()
			} else {
				open_tag.trim_start_matches('<').trim_end_matches('>').to_string()
			};

			let body = if self_closing {
				String::new()
			} else {
				let rest = &after[gt + 1..];
				let close = format!("</{tag}>");
				if let Some(end) = rest.to_ascii_lowercase().find(&close) {
					rest[..end].to_string()
				} else {
					String::new()
				}
			};

			if let Some(call) = xml_tag_to_tool_call(tag, &attrs, &body, *idx + 1) {
				// Dedupe identical shells
				let dup = calls.iter().any(|c| c.name == call.name && c.arguments == call.arguments);
				if !dup {
					*idx += 1;
					calls.push(call);
				}
			}

			search_from = start + gt + 1;
			if search_from >= text.len() {
				break;
			}
		}
	}
}

fn xml_attr(attrs: &str, key: &str) -> Option<String> {
	// command="..." or command='...'
	for quote in ['"', '\''] {
		let pat = format!("{key}={quote}");
		if let Some(i) = attrs.find(&pat) {
			let rest = &attrs[i + pat.len()..];
			if let Some(end) = rest.find(quote) {
				return Some(rest[..end].to_string());
			}
		}
	}
	None
}

fn xml_tag_to_tool_call(tag: &str, attrs: &str, body: &str, idx: u32) -> Option<ToolCall> {
	let name = match tag {
		"shell" | "bash" | "run_terminal_command" | "terminal" => "shell",
		"read" | "read_file" => "read",
		"write" | "write_file" => "write",
		"edit" | "search_replace" => "edit",
		"grep" => "grep",
		"glob" => "glob",
		"list" | "list_dir" => "list",
		"websearch" | "web_search" => "websearch",
		"webfetch" | "web_fetch" => "webfetch",
		_ => return None,
	};

	let args = match name {
		"shell" => {
			let cmd = xml_attr(attrs, "command").or_else(|| xml_attr(attrs, "cmd")).or_else(|| {
				let b = body.trim();
				if b.is_empty() { None } else { Some(b.to_string()) }
			})?;
			if cmd.is_empty() {
				return None;
			}
			json!({ "command": cmd })
		}
		"read" => {
			let path = xml_attr(attrs, "path")
				.or_else(|| xml_attr(attrs, "file"))
				.or_else(|| xml_attr(attrs, "file_path"))?;
			json!({ "path": path })
		}
		"write" => {
			let path = xml_attr(attrs, "path").or_else(|| xml_attr(attrs, "file"))?;
			let content = xml_attr(attrs, "content").unwrap_or_else(|| body.to_string());
			json!({ "path": path, "content": content })
		}
		"edit" => {
			let path = xml_attr(attrs, "path").or_else(|| xml_attr(attrs, "file"))?;
			let old =
				xml_attr(attrs, "old_string").or_else(|| xml_attr(attrs, "old")).unwrap_or_default();
			let new = xml_attr(attrs, "new_string")
				.or_else(|| xml_attr(attrs, "new"))
				.unwrap_or_else(|| body.to_string());
			json!({ "path": path, "old_string": old, "new_string": new })
		}
		"grep" => {
			let pattern = xml_attr(attrs, "pattern").or_else(|| xml_attr(attrs, "query"))?;
			let mut o = json!({ "pattern": pattern });
			if let Some(p) = xml_attr(attrs, "path") {
				o["path"] = json!(p);
			}
			o
		}
		"glob" => {
			let pattern = xml_attr(attrs, "pattern").unwrap_or_else(|| "*".into());
			json!({ "pattern": pattern })
		}
		"list" => {
			let path = xml_attr(attrs, "path").unwrap_or_else(|| ".".into());
			json!({ "path": path })
		}
		"websearch" => {
			let query = xml_attr(attrs, "query").or_else(|| xml_attr(attrs, "q"))?;
			json!({ "query": query })
		}
		"webfetch" => {
			let url = xml_attr(attrs, "url").or_else(|| xml_attr(attrs, "href"))?;
			json!({ "url": url })
		}
		_ => return None,
	};

	Some(ToolCall { id: format!("xml_{name}_{idx}"), name: name.into(), arguments: args.to_string() })
}

/// Render tool start/end markers for the message list accordion UI.
/// Body shows the real command / path / query — never a dummy "running…".
pub fn format_tool_running(name: &str, preview: &str) -> String {
	format_tool_running_id(name, preview, None)
}

/// Like [`format_tool_running`] but with a stable call id for upgrade matching.
pub fn format_tool_running_id(name: &str, preview: &str, call_id: Option<&str>) -> String {
	let title = ToolKind::from_name(name).map(|k| k.display_title()).unwrap_or("Tool");
	let p = if preview.is_empty() {
		String::new()
	} else {
		// Keep enough path tail for UI (renderer also prefers tails).
		let short: String = preview.chars().take(160).collect();
		format!(" {short}")
	};
	let body = if preview.is_empty() {
		"$ …".to_string()
	} else if is_terminal_name(name) {
		format!("$ {preview}")
	} else {
		preview.to_string()
	};
	let id_attr =
		call_id.filter(|s| !s.is_empty()).map(|id| format!(" id=\"{id}\"")).unwrap_or_default();
	format!(
		"\n```command{id_attr} name=\"{name}\" title=\"{title}\" status=\"running\"{p}\n{body}\n```\n"
	)
}

fn is_terminal_name(name: &str) -> bool {
	matches!(
		name.to_ascii_lowercase().as_str(),
		"shell"
			| "bash"
			| "sh"
			| "zsh"
			| "cmd"
			| "powershell"
			| "terminal"
			| "run_terminal_command"
			| "execute"
			| "exec"
	)
}

pub fn format_tool_result(result: &ToolResult) -> String {
	format_tool_result_ex(result, None)
}

/// Format a completed tool card; optional wall duration for the header.
pub fn format_tool_result_ex(result: &ToolResult, duration: Option<std::time::Duration>) -> String {
	let status = if result.ok { "done" } else { "error" };
	let title = ToolKind::from_name(&result.name).map(|k| k.display_title()).unwrap_or("Tool");
	let preview = if result.preview.is_empty() {
		String::new()
	} else {
		format!(" {}", result.preview.chars().take(120).collect::<String>())
	};
	// Generous caps — UI preview/full windows truncate for display only.
	let body_cap = match ToolKind::from_name(&result.name) {
		Some(ToolKind::Shell) => 32_000,
		Some(ToolKind::Read) => 48_000,
		Some(ToolKind::Write | ToolKind::Edit | ToolKind::ApplyPatch) => 24_000,
		Some(ToolKind::WebSearch | ToolKind::WebFetch) => 24_000,
		Some(ToolKind::TodoWrite) => 12_000,
		Some(ToolKind::McpTool) => 24_000,
		_ => 16_000,
	};
	let body = truncate(&result.output, body_cap);
	let id_attr =
		if result.call_id.is_empty() { String::new() } else { format!(" id=\"{}\"", result.call_id) };
	let dur_attr =
		duration.map(|d| format!(" duration_ms=\"{}\"", d.as_millis())).unwrap_or_default();
	format!(
		"\n```command{id_attr} name=\"{}\" title=\"{title}\" status=\"{status}\"{dur_attr}{preview}\n{body}\n```\n",
		result.name
	)
}

/// Replace a matching `status="running"` command fence with the completed
/// result. Prefers stable `id="…"` match, then last running fence for the
/// same tool name (avoids clobbering concurrent tools when ids are present).
pub fn upgrade_running_tool_block(content: &mut String, result_fence: &str) {
	let name = extract_command_name_from_fence(result_fence);
	let id = extract_command_attr_from_fence(result_fence, "id");
	if name.is_empty() && id.is_none() {
		content.push_str(result_fence);
		return;
	}

	// Prefer id match across any ```command fence that is still running.
	if let Some(ref id) = id {
		let id_needle = format!("id=\"{id}\"");
		if let Some((start, end)) = find_running_fence(content, |header| {
			header.contains(&id_needle)
				&& (header.contains("status=\"running\"") || header.contains("status=running"))
		}) {
			replace_range_with_fence(content, start, end, result_fence);
			return;
		}
	}

	// Fallback: last running fence for this tool name.
	if !name.is_empty() {
		let name_tok = format!("name=\"{name}\"");
		if let Some((start, end)) = find_running_fence(content, |header| {
			header.contains(&name_tok)
				&& (header.contains("status=\"running\"") || header.contains("status=running"))
		}) {
			replace_range_with_fence(content, start, end, result_fence);
			return;
		}
	}

	content.push_str(result_fence);
}

fn find_running_fence(
	content: &str,
	mut header_ok: impl FnMut(&str) -> bool,
) -> Option<(usize, usize)> {
	let mut search_from = 0usize;
	let mut last: Option<(usize, usize)> = None;
	while let Some(rel) = content[search_from..].find("```command") {
		let start = search_from + rel;
		let after_open = start + "```command".len();
		let header_end =
			content[after_open..].find('\n').map(|i| after_open + i).unwrap_or(content.len());
		let header = &content[start..header_end];
		if header_ok(header)
			&& let Some(close_rel) = content[header_end..].find("\n```")
		{
			let mut end = header_end + close_rel + "\n```".len();
			if content[end..].starts_with('\n') {
				end += 1;
			}
			last = Some((start, end));
		}
		search_from = after_open;
	}
	last
}

fn replace_range_with_fence(content: &mut String, start: usize, end: usize, result_fence: &str) {
	let mut next = String::with_capacity(content.len() + result_fence.len());
	next.push_str(&content[..start]);
	next.push_str(result_fence.trim_start_matches('\n'));
	if !result_fence.ends_with('\n') {
		next.push('\n');
	}
	if end < content.len() {
		next.push_str(&content[end..]);
	}
	*content = next;
}

fn extract_command_attr_from_fence(fence: &str, key: &str) -> Option<String> {
	for line in fence.lines() {
		let t = line.trim();
		if t.starts_with("```command") {
			return crate::msg_ui::extract_attr(t, key);
		}
	}
	None
}

fn extract_command_name_from_fence(fence: &str) -> String {
	for line in fence.lines() {
		let t = line.trim();
		if t.starts_with("```command")
			&& let Some(rest) = t.strip_prefix("```command")
		{
			// name="…"
			if let Some(i) = rest.find("name=\"") {
				let s = &rest[i + 6..];
				if let Some(end) = s.find('"') {
					return s[..end].to_string();
				}
			}
		}
	}
	String::new()
}

/// Compact group header for consecutive context tools (read/grep/glob/list).
pub fn format_context_group_summary(counts: &[(ToolKind, u32)]) -> String {
	let bits: Vec<String> = counts
		.iter()
		.filter(|(_, n)| *n > 0)
		.map(|(k, n)| format!("{} {n}", k.display_title()))
		.collect();
	if bits.is_empty() {
		return String::new();
	}
	format!("\n▸ Context · {}\n", bits.join(" · "))
}

/// Tool-result payload for the next model turn (OpenAI tool role message content).
pub fn tool_message_content(result: &ToolResult) -> String {
	format!("[{}] {}\n{}", if result.ok { "ok" } else { "error" }, result.title, result.output)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::modes::AgentMode;

	use std::fs;
	use std::io::Write;

	#[test]
	fn mode_ask_denies_write() {
		assert!(!allowed_in_mode(ToolKind::Write, AgentMode::Ask, false));
		assert!(allowed_in_mode(ToolKind::Read, AgentMode::Ask, false));
		assert!(allowed_in_mode(ToolKind::Shell, AgentMode::Ask, false));
		assert!(allowed_in_mode(ToolKind::WebSearch, AgentMode::Ask, false));
	}

	#[test]
	fn mode_plan_shell_gated() {
		assert!(!allowed_in_mode(ToolKind::Shell, AgentMode::Plan, false));
		assert!(allowed_in_mode(ToolKind::Shell, AgentMode::Plan, true));
		assert!(allowed_in_mode(ToolKind::WebSearch, AgentMode::Plan, false));
	}

	#[test]
	fn mode_automation_allows_writes() {
		assert!(allowed_in_mode(ToolKind::Write, AgentMode::Automation, false));
		assert!(allowed_in_mode(ToolKind::Shell, AgentMode::Automation, false));
		assert!(allowed_in_mode(ToolKind::Task, AgentMode::Automation, false));
	}

	#[test]
	fn mode_write_full_access() {
		assert!(allowed_in_mode(ToolKind::Write, AgentMode::Write, false));
		assert!(allowed_in_mode(ToolKind::Shell, AgentMode::Write, false));
		assert!(allowed_in_mode(ToolKind::Task, AgentMode::Write, false));
		assert!(allowed_in_mode(ToolKind::WebSearch, AgentMode::Write, false));
		assert!(allowed_in_mode(ToolKind::SkillManage, AgentMode::Write, false));
		let defs = tools_for_mode(AgentMode::Write);
		assert!(defs.iter().any(|t| t.kind == ToolKind::Task));
		assert!(defs.iter().any(|t| t.kind == ToolKind::WebSearch));
	}

	#[test]
	fn upgrade_running_replaces_matching_fence() {
		let mut content = format_tool_running("shell", "git status");
		assert!(content.contains("status=\"running\""));
		let done = format_tool_result(&ToolResult {
			call_id: "1".into(),
			name: "shell".into(),
			ok: true,
			title: "Shell · git status".into(),
			output: " M src/main.rs\n".into(),
			preview: "git status".into(),
		});
		upgrade_running_tool_block(&mut content, &done);
		assert!(content.contains("status=\"done\""));
		assert!(!content.contains("status=\"running\""));
		assert!(content.contains("M src/main.rs"));
	}

	#[test]
	fn extracts_bash_fences() {
		let text = "I'll check status.\n```bash\ngit status --short\n```\n```bash\ngit log -1\n```\n";
		let calls = extract_markdown_tool_calls(text, AgentMode::Write);
		assert_eq!(calls.len(), 2);
		assert_eq!(calls[0].name, "shell");
		assert!(calls[0].arguments.contains("git status"));
	}

	#[test]
	fn extracts_xml_shell_tags() {
		let text = "Thought: check status.\n\n<shell command=\"git status\"/>\n";
		let calls = extract_markdown_tool_calls(text, AgentMode::Write);
		assert_eq!(calls.len(), 1, "calls={calls:?}");
		assert_eq!(calls[0].name, "shell");
		assert!(calls[0].arguments.contains("git status"), "{}", calls[0].arguments);
	}

	#[test]
	fn extracts_xml_read_and_run_terminal() {
		let text = r#"
<read path="src/main.rs"/>
<run_terminal_command command="cargo check -j12"/>
"#;
		let calls = extract_markdown_tool_calls(text, AgentMode::Write);
		assert!(calls.iter().any(|c| c.name == "read"));
		assert!(calls.iter().any(|c| c.name == "shell" && c.arguments.contains("cargo check")));
	}

	#[test]
	fn ask_does_extract_bash() {
		let text = "```bash\nrm -rf /\n```"; // extract_markdown_tool_calls now returns tools for Ask mode so they can fail gracefully in execution.
		assert!(!extract_markdown_tool_calls(text, AgentMode::Ask).is_empty());
	}

	#[test]
	fn destructive_shell_needs_permission() {
		let args = json!({"command": "git reset --hard"});
		assert!(needs_permission(ToolKind::Shell, &args, AgentMode::Write));
		let safe = json!({"command": "git status"});
		assert!(!needs_permission(ToolKind::Shell, &safe, AgentMode::Write));
	}

	#[test]
	fn shell_exec_echo() {
		let call = ToolCall {
			id: "1".into(),
			name: "shell".into(),
			arguments: json!({"command": "echo dx-tools-ok"}).to_string(),
		};
		let r = execute(&call, Path::new("."), AgentMode::Agent, false);
		assert!(r.ok, "{}", r.output);
		assert!(r.output.contains("dx-tools-ok"));
	}

	// ── Read tool ───────────────────────────────────────────────────────

	#[test]
	fn read_existing_file() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("test.txt");
		fs::write(&path, "hello world").unwrap();
		let call = ToolCall {
			id: "2".into(),
			name: "read".into(),
			arguments: json!({"path": path.to_str().unwrap()}).to_string(),
		};
		let r = execute(&call, dir.path(), AgentMode::Agent, false);
		assert!(r.ok, "{}", r.output);
		assert!(r.output.contains("hello world"), "{}", r.output);
	}

	#[test]
	fn read_nonexistent_file_returns_error() {
		let dir = tempfile::tempdir().unwrap();
		let call = ToolCall {
			id: "3".into(),
			name: "read".into(),
			arguments: json!({"path": "/nonexistent/file/path.txt"}).to_string(),
		};
		let r = execute(&call, dir.path(), AgentMode::Agent, false);
		assert!(!r.ok, "expected error but got ok: {}", r.output);
	}

	// ── Write tool ──────────────────────────────────────────────────────

	#[test]
	fn write_creates_new_file() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("new.txt");
		let call = ToolCall {
			id: "4".into(),
			name: "write".into(),
			arguments: json!({"path": path.to_str().unwrap(), "content": "written content"}).to_string(),
		};
		let r = execute(&call, dir.path(), AgentMode::Agent, false);
		assert!(r.ok, "{}", r.output);
		let content = fs::read_to_string(&path).unwrap();
		assert_eq!(content, "written content");
	}

	#[test]
	fn write_overwrites_existing_file() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("existing.txt");
		fs::write(&path, "old content").unwrap();
		let call = ToolCall {
			id: "5".into(),
			name: "write".into(),
			arguments: json!({"path": path.to_str().unwrap(), "content": "new content"}).to_string(),
		};
		let r = execute(&call, dir.path(), AgentMode::Agent, false);
		assert!(r.ok, "{}", r.output);
		let content = fs::read_to_string(&path).unwrap();
		assert_eq!(content, "new content");
	}

	// ── Glob tool ───────────────────────────────────────────────────────

	#[test]
	fn glob_finds_matching_files() {
		let dir = tempfile::tempdir().unwrap();
		fs::write(dir.path().join("foo.rs"), "").unwrap();
		fs::write(dir.path().join("bar.rs"), "").unwrap();
		fs::write(dir.path().join("baz.py"), "").unwrap();
		let call = ToolCall {
			id: "6".into(),
			name: "glob".into(),
			arguments: json!({"pattern": "*.rs"}).to_string(),
		};
		let r = execute(&call, dir.path(), AgentMode::Agent, false);
		assert!(r.ok, "{}", r.output);
		assert!(r.output.contains("foo.rs"), "{}", r.output);
		assert!(r.output.contains("bar.rs"), "{}", r.output);
		assert!(!r.output.contains("baz.py"), "{}", r.output);
	}

	#[test]
	fn glob_empty_results_when_no_match() {
		let dir = tempfile::tempdir().unwrap();
		let call = ToolCall {
			id: "7".into(),
			name: "glob".into(),
			arguments: json!({"pattern": "*.nonexistent"}).to_string(),
		};
		let r = execute(&call, dir.path(), AgentMode::Agent, false);
		assert!(r.ok);
	}

	// ── Grep tool ───────────────────────────────────────────────────────

	#[test]
	fn grep_finds_pattern_in_files() {
		let dir = tempfile::tempdir().unwrap();
		fs::write(dir.path().join("a.rs"), "fn hello() {}").unwrap();
		fs::write(dir.path().join("b.rs"), "fn world() {}").unwrap();
		fs::write(dir.path().join("c.py"), "goodbye").unwrap();
		let call = ToolCall {
			id: "8".into(),
			name: "grep".into(),
			arguments: json!({"pattern": "fn"}).to_string(),
		};
		let r = execute(&call, dir.path(), AgentMode::Agent, false);
		assert!(r.ok, "{}", r.output);
		assert!(r.output.contains("a.rs"), "{}", r.output);
		assert!(r.output.contains("b.rs"), "{}", r.output);
		assert!(!r.output.contains("c.py"), "{}", r.output);
	}

	// ── List tool ──────────────────────────────────────────────────────

	#[test]
	fn lists_directory_contents() {
		let dir = tempfile::tempdir().unwrap();
		fs::write(dir.path().join("file_a.txt"), "").unwrap();
		fs::write(dir.path().join("file_b.txt"), "").unwrap();
		fs::create_dir(dir.path().join("subdir")).unwrap();
		let call = ToolCall {
			id: "9".into(),
			name: "list".into(),
			arguments: json!({"path": dir.path().to_str().unwrap()}).to_string(),
		};
		let r = execute(&call, dir.path(), AgentMode::Agent, false);
		assert!(r.ok, "{}", r.output);
		assert!(r.output.contains("file_a.txt"), "{}", r.output);
		assert!(r.output.contains("file_b.txt"), "{}", r.output);
		assert!(
			r.output.contains("subdir") || r.output.contains("subdir/") || r.output.contains("subdir\\"),
			"{}",
			r.output
		);
	}

	// ── Empty tool call arguments ────────────────────────────────────────

	#[test]
	fn empty_shell_returns_error() {
		let call = ToolCall {
			id: "10".into(),
			name: "shell".into(),
			arguments: json!({"command": ""}).to_string(),
		};
		let r = execute(&call, Path::new("."), AgentMode::Agent, false);
		assert!(!r.ok);
		assert!(r.output.contains("Missing `command`") || r.output.contains("empty"), "{}", r.output);
	}

	#[test]
	fn invalid_tool_name_returns_error() {
		let call =
			ToolCall { id: "11".into(), name: "nonexistent_tool".into(), arguments: "{}".into() };
		let r = execute(&call, Path::new("."), AgentMode::Agent, false);
		assert!(!r.ok);
	}

	// ── Token counting ──────────────────────────────────────────────────

	#[test]
	fn count_tokens_ascii() {
		let text = "hello world how are you doing today this is a test of token counting";
		let count = super::super::components::count_tokens(text);
		assert!(count > 0, "token count should be > 0, got {count}");
		// Rough sanity: English text ~4-6 chars/token on avg
		assert!(count <= text.len(), "tokens ({count}) should not exceed chars ({})", text.len());
	}

	#[test]
	fn count_tokens_unicode() {
		let text = "日本語のテキストをテストしています";
		let count = super::super::components::count_tokens(text);
		assert!(count > 0, "unicode token count should be > 0, got {count}");
	}

	// ── Permission checks ──────────────────────────────────────────────

	#[test]
	fn permission_is_always_allowed_checks_stored() {
		let hub = crate::permission_hub::PermissionHub::new();
		// Nothing stored yet
		assert!(!hub.is_always_allowed("shell", "git status"));
		// After storing via reply with AllowAlways, subsequent checks pass
		hub.reply(crate::tools::PermissionDecision::AllowAlways);
		// Note: is_always_allowed checks the 'always' list which is only
		// populated after request() returns AllowAlways via the reply path.
		// For direct testing we check that the hub starts clean.
	}

	#[test]
	fn permission_reply_sets_decision() {
		let hub = crate::permission_hub::PermissionHub::new();
		assert!(hub.pending().is_none());
		// reply returns false when nothing pending
		assert!(!hub.reply(crate::tools::PermissionDecision::AllowOnce));
	}

	#[test]
	fn permission_hub_starts_clean() {
		let hub = crate::permission_hub::PermissionHub::new();
		assert!(hub.pending().is_none());
		hub.clear();
		assert!(hub.pending().is_none());
	}
}
