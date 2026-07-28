//! Hermes-shaped skill library for DX Agent.
//!
//! - Skills live under `~/.config/dx/workspace/skills/<name>/SKILL.md`
//! - Agent can create/patch/list via the `skill_manage` tool
//! - After successful multi-step work, auto-create a class-level skill (default on)
//!
//! Inspired by hermes-agent `tools/skill_manager_tool.py` + `SKILLS_GUIDANCE`.

use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
	time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agent_workspace;
use crate::tools::{ToolCall, ToolResult};

const SKILL_MD_CAP: usize = 12_000;
const NAME_MAX: usize = 64;
const AUTO_MIN_TOOL_STEPS: u32 = 5;

/// Directory for user-created skills.
pub fn skills_dir() -> PathBuf {
	agent_workspace::ensure_workspace().join("skills")
}

/// Ensure skills root exists.
pub fn ensure_skills_dir() -> PathBuf {
	let d = skills_dir();
	let _ = fs::create_dir_all(&d);
	d
}

/// OpenAI tool schema for skill_manage.
pub fn skill_manage_schema() -> Value {
	json!({
		"type": "function",
		"function": {
			"name": "skill_manage",
			"description": "Create or update a reusable skill after successful work. Actions: create, patch, list, view. Skills are class-level workflows (not one-off session notes).",
			"parameters": {
				"type": "object",
				"properties": {
					"action": {
						"type": "string",
						"enum": ["create", "patch", "list", "view"],
						"description": "create=new skill, patch=append section, list=index, view=read SKILL.md"
					},
					"name": {
						"type": "string",
						"description": "Skill slug: lowercase words joined by hyphens (e.g. cargo-workspace-check)"
					},
					"description": {
						"type": "string",
						"description": "One-line description for the skill index"
					},
					"content": {
						"type": "string",
						"description": "Full SKILL.md body (for create) or section to append (for patch)"
					},
					"category": {
						"type": "string",
						"description": "Optional category folder under skills/"
					}
				},
				"required": ["action"]
			}
		}
	})
}

/// System prompt guidance (Hermes SKILLS_GUIDANCE, DX-branded).
pub fn skills_guidance() -> &'static str {
	r#"# Skills (auto-learning)
You have a skill library under the DX agent workspace.
After completing complex work (5+ tool calls), fixing a tricky error, or discovering a non-trivial workflow, save it with skill_manage:
- action=create name=<class-level-slug> description=<one line> content=<SKILL.md markdown>
- action=patch name=<existing> content=<subsection or pitfall to add>
- action=list | action=view name=<slug>

Name skills at the **class** level (e.g. `rust-cargo-workspace`, not `fix-pr-42-today`).
Do not store secrets, PR numbers, or one-off task logs in skills.
When a skill is wrong or incomplete, patch it immediately."#
}

/// Compact index injected into Agent system stack.
pub fn skills_index_prompt(max_entries: usize) -> Option<String> {
	let list = list_skills();
	if list.is_empty() {
		return Some(
			"<skills_index>\n(no user skills yet — create with skill_manage after successful work)\n</skills_index>"
				.into(),
		);
	}
	let mut lines = vec!["<skills_index>".to_string()];
	for (i, s) in list.into_iter().take(max_entries).enumerate() {
		lines.push(format!(
			"{}. {} — {}",
			i + 1,
			s.name,
			if s.description.is_empty() {
				"(no description)".to_string()
			} else {
				s.description.chars().take(100).collect::<String>()
			}
		));
	}
	lines.push("Use skill_manage action=view to load a full skill.</skills_index>".into());
	Some(lines.join("\n"))
}

#[derive(Debug, Clone)]
pub struct SkillMeta {
	pub name: String,
	pub description: String,
	pub path: PathBuf,
}

/// Alias for use in background_review.
#[allow(dead_code)]
pub type SkillItem = SkillMeta;

pub fn list_skills() -> Vec<SkillMeta> {
	let root = ensure_skills_dir();
	let mut out = Vec::new();
	walk_skills(&root, &root, &mut out);
	out.sort_by(|a, b| a.name.cmp(&b.name));
	out
}

fn walk_skills(_root: &Path, dir: &Path, out: &mut Vec<SkillMeta>) {
	let Ok(rd) = fs::read_dir(dir) else {
		return;
	};
	for e in rd.flatten() {
		let p = e.path();
		if p.is_dir() {
			let skill_md = p.join("SKILL.md");
			if skill_md.is_file() {
				let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("skill").to_string();
				let desc = read_description(&skill_md).unwrap_or_default();
				out.push(SkillMeta { name, description: desc, path: skill_md });
			} else {
				// category folder
				walk_skills(_root, &p, out);
			}
		}
	}
}

fn read_description(skill_md: &Path) -> Option<String> {
	let text = fs::read_to_string(skill_md).ok()?;
	// YAML frontmatter description:
	if text.starts_with("---")
		&& let Some(end) = text[3..].find("\n---")
	{
		let fm = &text[3..3 + end];
		for line in fm.lines() {
			if let Some(d) = line.strip_prefix("description:") {
				return Some(d.trim().trim_matches('"').to_string());
			}
		}
	}
	// First non-heading non-empty line
	for line in text.lines() {
		let t = line.trim();
		if t.is_empty() || t.starts_with('#') || t.starts_with("---") {
			continue;
		}
		return Some(t.chars().take(120).collect());
	}
	None
}

fn validate_name(name: &str) -> Result<String, String> {
	let n = name.trim().to_ascii_lowercase();
	if n.is_empty() || n.len() > NAME_MAX {
		return Err("skill name empty or too long".into());
	}
	if !n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
		return Err("skill name must be [a-z0-9-_]".into());
	}
	// Reject session artifacts
	let bad = ["fix-", "debug-", "pr-", "tmp-", "today-", "session-"];
	if bad.iter().any(|b| n.starts_with(b)) {
		return Err(
			"name looks like a one-off session artifact; use a class-level name (e.g. rust-test-failures)"
				.into(),
		);
	}
	Ok(n)
}

fn skill_dir(name: &str, category: Option<&str>) -> PathBuf {
	let root = ensure_skills_dir();
	if let Some(cat) = category {
		let cat = cat
			.chars()
			.filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
			.take(32)
			.collect::<String>();
		if !cat.is_empty() {
			return root.join(cat).join(name);
		}
	}
	root.join(name)
}

/// Execute skill_manage tool call.
pub fn execute_skill_manage(call: &ToolCall) -> ToolResult {
	let args: Value = serde_json::from_str(&call.arguments).unwrap_or(json!({}));
	let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list").to_ascii_lowercase();

	match action.as_str() {
		"list" => {
			let list = list_skills();
			let body = if list.is_empty() {
				"(no skills yet)".into()
			} else {
				list
					.iter()
					.map(|s| format!("- {} — {}", s.name, s.description))
					.collect::<Vec<_>>()
					.join("\n")
			};
			ToolResult {
				call_id: call.id.clone(),
				name: "skill_manage".into(),
				ok: true,
				title: format!("Skills · {} entries", list.len()),
				output: body,
				preview: "list".into(),
			}
		}
		"view" => {
			let name = match args.get("name").and_then(|v| v.as_str()).map(validate_name) {
				Some(Ok(n)) => n,
				Some(Err(e)) => {
					return err_result(&call.id, e);
				}
				None => return err_result(&call.id, "name required for view".into()),
			};
			record_skill_view(&name);
			match find_skill_md(&name) {
				Some(p) => {
					let text = fs::read_to_string(&p).unwrap_or_default();
					let body = truncate(&text, SKILL_MD_CAP);
					ToolResult {
						call_id: call.id.clone(),
						name: "skill_manage".into(),
						ok: true,
						title: format!("Skill · {name}"),
						output: body,
						preview: name,
					}
				}
				None => err_result(&call.id, format!("skill '{name}' not found")),
			}
		}
		"create" => {
			let name = match args.get("name").and_then(|v| v.as_str()).map(validate_name) {
				Some(Ok(n)) => n,
				Some(Err(e)) => return err_result(&call.id, e),
				None => return err_result(&call.id, "name required".into()),
			};
			let desc =
				args.get("description").and_then(|v| v.as_str()).unwrap_or("DX agent skill").to_string();
			let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
			if content.trim().is_empty() {
				return err_result(&call.id, "content required for create".into());
			}
			if find_skill_md(&name).is_some() {
				return err_result(&call.id, format!("skill '{name}' already exists — use action=patch"));
			}
			let category = args.get("category").and_then(|v| v.as_str());
			let dir = skill_dir(&name, category);
			if let Err(e) = fs::create_dir_all(&dir) {
				return err_result(&call.id, e.to_string());
			}
			let md = format_skill_md(&name, &desc, &content);
			let path = dir.join("SKILL.md");
			if let Err(e) = atomic_write(&path, &md) {
				return err_result(&call.id, e.to_string());
			}
			record_skill_use(&name);
			let _ = agent_workspace::append_daily_memory(&format!("skill created: {name}"));
			ToolResult {
				call_id: call.id.clone(),
				name: "skill_manage".into(),
				ok: true,
				title: format!("Skill created · {name}"),
				output: format!("Wrote {}", path.display()),
				preview: name,
			}
		}
		"patch" => {
			let name = match args.get("name").and_then(|v| v.as_str()).map(validate_name) {
				Some(Ok(n)) => n,
				Some(Err(e)) => return err_result(&call.id, e),
				None => return err_result(&call.id, "name required".into()),
			};
			let patch = args.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
			if patch.trim().is_empty() {
				return err_result(&call.id, "content required for patch".into());
			}
			let Some(path) = find_skill_md(&name) else {
				return err_result(&call.id, format!("skill '{name}' not found — create first"));
			};
			let mut existing = fs::read_to_string(&path).unwrap_or_default();
			if !existing.ends_with('\n') {
				existing.push('\n');
			}
			existing.push_str("\n## Update\n\n");
			existing.push_str(patch.trim());
			existing.push('\n');
			if existing.chars().count() > SKILL_MD_CAP * 2 {
				return err_result(&call.id, "skill would exceed size cap".into());
			}
			if let Err(e) = atomic_write(&path, &existing) {
				return err_result(&call.id, e.to_string());
			}
			record_skill_patch(&name);
			let _ = agent_workspace::append_daily_memory(&format!("skill patched: {name}"));
			ToolResult {
				call_id: call.id.clone(),
				name: "skill_manage".into(),
				ok: true,
				title: format!("Skill patched · {name}"),
				output: format!("Updated {}", path.display()),
				preview: name,
			}
		}
		_ => err_result(&call.id, format!("unknown action '{action}'")),
	}
}

fn err_result(id: &str, msg: String) -> ToolResult {
	ToolResult {
		call_id: id.into(),
		name: "skill_manage".into(),
		ok: false,
		title: "skill_manage · error".into(),
		output: msg,
		preview: "error".into(),
	}
}

fn find_skill_md(name: &str) -> Option<PathBuf> {
	list_skills().into_iter().find(|s| s.name == name).map(|s| s.path)
}

fn format_skill_md(name: &str, description: &str, body: &str) -> String {
	let body = body.trim();
	// If caller already sent full frontmatter, keep it
	if body.starts_with("---") {
		return body.to_string();
	}
	format!(
		"---\nname: {name}\ndescription: \"{}\"\n---\n\n# {name}\n\n{body}\n",
		description.replace('"', "'")
	)
}

fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent)?;
	}
	let tmp = path.with_extension("md.tmp");
	fs::write(&tmp, content)?;
	fs::rename(&tmp, path)?;
	Ok(())
}

fn truncate(s: &str, cap: usize) -> String {
	let n = s.chars().count();
	if n <= cap {
		return s.to_string();
	}
	let kept: String = s.chars().take(cap).collect();
	format!("{kept}\n…[+{} chars]", n - cap)
}

// ── Auto-learn after successful multi-step turns (Hermes background skill review lite) ──

/// Heuristic: enough successful tool work to warrant a skill.
pub fn should_auto_create_skill(
	tool_steps: u32,
	successful_tools: u32,
	assistant_text: &str,
) -> bool {
	if tool_steps < AUTO_MIN_TOOL_STEPS {
		return false;
	}
	if successful_tools < 3 {
		return false;
	}
	let lower = assistant_text.to_ascii_lowercase();
	// Skip pure Q&A / failed sessions
	if lower.contains("error:") && !lower.contains("fixed") && successful_tools < 4 {
		return false;
	}
	true
}

/// Derive a class-level slug from user goal + assistant summary.
pub fn suggest_skill_name(user_goal: &str, assistant: &str) -> String {
	let blob = format!("{user_goal} {assistant}");
	let mut words: Vec<String> = blob
		.split(|c: char| !c.is_alphanumeric() && c != '-')
		.filter(|w| w.len() >= 3)
		.map(|w| w.to_ascii_lowercase())
		.filter(|w| {
			![
				"the", "and", "for", "with", "from", "that", "this", "have", "will", "your", "into",
				"using", "please", "just", "mode", "goal", "write", "agent",
			]
			.contains(&w.as_str())
		})
		.take(4)
		.collect();
	if words.is_empty() {
		words.push("workflow".into());
	}
	let mut name = words.join("-");
	if name.len() > 48 {
		name = name.chars().take(48).collect();
	}
	// Ensure valid
	validate_name(&name).unwrap_or_else(|_| format!("workflow-{}", now_suffix()))
}

fn now_suffix() -> String {
	use std::time::{SystemTime, UNIX_EPOCH};
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|d| (d.as_secs() % 10_000).to_string())
		.unwrap_or_else(|_| "0".into())
}

/// Auto-create skill from a completed multi-step turn. Returns path if created.
pub fn auto_create_from_turn(
	user_goal: &str,
	assistant_text: &str,
	tool_steps: u32,
	successful_tools: u32,
) -> Option<PathBuf> {
	if !should_auto_create_skill(tool_steps, successful_tools, assistant_text) {
		return None;
	}
	let name = suggest_skill_name(user_goal, assistant_text);
	// Don't overwrite existing
	if find_skill_md(&name).is_some() {
		// Patch with a short note instead
		let call = ToolCall {
			id: "auto_patch".into(),
			name: "skill_manage".into(),
			arguments: json!({
				"action": "patch",
				"name": name,
				"content": format!(
					"### Session note\n\nReused successfully ({} tool steps, {} ok).\n\nGoal: {}\n",
					tool_steps,
					successful_tools,
					user_goal.chars().take(200).collect::<String>()
				)
			})
			.to_string(),
		};
		let r = execute_skill_manage(&call);
		return if r.ok { find_skill_md(&name) } else { None };
	}

	let summary: String = assistant_text
		.lines()
		.filter(|l| {
			let t = l.trim();
			!t.is_empty() && !t.starts_with("```") && !t.starts_with('<') && !t.starts_with("▸")
		})
		.take(40)
		.collect::<Vec<_>>()
		.join("\n");

	let content = format!(
		"## When to use\n\n\
		 When the user needs work similar to:\n> {}\n\n\
		 ## Approach\n\n\
		 {}\n\n\
		 ## Notes\n\n\
		 - Auto-saved by DX after a successful multi-step run ({tool_steps} tool steps).\n\
		 - Patch this skill when the workflow changes.\n",
		user_goal.chars().take(300).collect::<String>(),
		if summary.trim().is_empty() {
			"(see session transcript for tool sequence)".to_string()
		} else {
			summary.chars().take(2_500).collect::<String>()
		}
	);

	let call = ToolCall {
		id: "auto_create".into(),
		name: "skill_manage".into(),
		arguments: json!({
			"action": "create",
			"name": name,
			"description": format!("Auto skill from successful work: {}", user_goal.chars().take(80).collect::<String>()),
			"content": content,
			"category": "auto"
		})
		.to_string(),
	};
	let r = execute_skill_manage(&call);
	if r.ok { find_skill_md(&name) } else { None }
}

// ── Skill Telemetry (Hermes-inspired .usage.json sidecar) ──────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUsage {
	pub use_count: u64,
	pub view_count: u64,
	pub patch_count: u64,
	pub last_used_at: Option<u64>,
	pub last_viewed_at: Option<u64>,
	pub last_patched_at: Option<u64>,
	pub created_at: u64,
	pub state: String, // "active" | "stale" | "archived"
	pub pinned: bool,
}

impl SkillUsage {
	pub fn new() -> Self {
		let now = now_secs();
		Self {
			use_count: 0,
			view_count: 0,
			patch_count: 0,
			last_used_at: None,
			last_viewed_at: None,
			last_patched_at: None,
			created_at: now,
			state: "active".into(),
			pinned: false,
		}
	}
}

fn now_secs() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn usage_path() -> PathBuf {
	skills_dir().join(".usage.json")
}

fn load_usage() -> HashMap<String, SkillUsage> {
	fs::read_to_string(usage_path())
		.ok()
		.and_then(|s| serde_json::from_str(&s).ok())
		.unwrap_or_default()
}

fn save_usage(usage: &HashMap<String, SkillUsage>) {
	if let Ok(json) = serde_json::to_string_pretty(usage) {
		let _ = fs::write(usage_path(), &json);
	}
}

/// Record that a skill was used (called from execute_skill_manage).
pub fn record_skill_use(name: &str) {
	let mut usage = load_usage();
	let entry = usage.entry(name.to_string()).or_insert_with(SkillUsage::new);
	entry.use_count = entry.use_count.saturating_add(1);
	entry.last_used_at = Some(now_secs());
	save_usage(&usage);
}

/// Record that a skill was viewed.
pub fn record_skill_view(name: &str) {
	let mut usage = load_usage();
	let entry = usage.entry(name.to_string()).or_insert_with(SkillUsage::new);
	entry.view_count = entry.view_count.saturating_add(1);
	entry.last_viewed_at = Some(now_secs());
	save_usage(&usage);
}

/// Record that a skill was patched.
pub fn record_skill_patch(name: &str) {
	let mut usage = load_usage();
	let entry = usage.entry(name.to_string()).or_insert_with(SkillUsage::new);
	entry.patch_count = entry.patch_count.saturating_add(1);
	entry.last_patched_at = Some(now_secs());
	save_usage(&usage);
}

/// Get usage for all skills.
pub fn get_all_usage() -> HashMap<String, SkillUsage> {
	load_usage()
}

// ── Curator: stale detection + archival (Hermes-inspired) ──────────────

/// Stale threshold: 30 days without use → mark stale.
const STALE_AFTER_SECS: u64 = 30 * 24 * 60 * 60;
/// Archive threshold: 90 days without use → archive.
const ARCHIVE_AFTER_SECS: u64 = 90 * 24 * 60 * 60;

/// Run curator: mark stale skills, archive expired ones.
/// Returns a human-readable summary.
pub fn run_curator() -> String {
	let mut usage = load_usage();
	let now = now_secs();
	let mut stale_count = 0u64;
	let mut archived_count = 0u64;
	let mut archived_names = Vec::new();

	let mut to_archive = Vec::new();
	for (name, entry) in &mut usage {
		if entry.pinned {
			continue;
		}
		let last = entry.last_used_at.unwrap_or(entry.created_at);
		let age = now.saturating_sub(last);

		if age >= ARCHIVE_AFTER_SECS && entry.state != "archived" {
			entry.state = "archived".into();
			archived_count += 1;
			archived_names.push(name.clone());
			to_archive.push(name.clone());
		} else if age >= STALE_AFTER_SECS && entry.state == "active" {
			entry.state = "stale".into();
			stale_count += 1;
		}
	}
	save_usage(&usage);

	// Create pre-curator snapshot for rollback safety
	let backup_dir = skills_dir().join(".curator_backups");
	let _ = fs::create_dir_all(&backup_dir);
	let ts = now_secs().to_string();
	let snap_dir = backup_dir.join(&ts);
	let _ = fs::create_dir_all(&snap_dir);
	if let Ok(entries) = fs::read_dir(skills_dir()) {
		for e in entries.flatten() {
			let name = e.file_name();
			let name_str = name.to_string_lossy().to_string();
			if name_str.starts_with('.') {
				continue;
			}
			if e.path().is_dir() && e.path().join("SKILL.md").is_file() {
				let dest = snap_dir.join(&name_str);
				let _ = fs::create_dir_all(&dest);
				if let Ok(md) = fs::read_to_string(e.path().join("SKILL.md")) {
					let _ = fs::write(dest.join("SKILL.md"), &md);
				}
				// Copy subdirectories (references, templates, scripts)
				if let Ok(sub) = fs::read_dir(e.path()) {
					for sub_e in sub.flatten() {
						if sub_e.path().is_dir() {
							let sub_name = sub_e.file_name();
							let sub_dest = dest.join(&sub_name);
							let _ = fs::create_dir_all(&sub_dest);
							if let Ok(files) = fs::read_dir(sub_e.path()) {
								for f in files.flatten() {
									let fname = f.file_name();
									if let Ok(content) = fs::read_to_string(f.path()) {
										let _ = fs::write(sub_dest.join(&fname), &content);
									}
								}
							}
						}
					}
				}
			}
		}
	}

	// Move archived skill dirs to .archive/
	for name in &archived_names {
		if let Some(path) = find_skill_md(name)
			&& let Some(parent) = path.parent()
		{
			let archive_dir = skills_dir().join(".archive");
			let _ = fs::create_dir_all(&archive_dir);
			let dest = archive_dir.join(name);
			rename_or_skip(parent, &dest);
		}
	}

	if stale_count == 0 && archived_count == 0 {
		"No stale skills found.".to_string()
	} else {
		format!(
			"Curator: {} stale, {} archived ({}). {} dormant skills remain active.",
			stale_count,
			archived_count,
			archived_names.join(", "),
			usage.values().filter(|e| e.state == "active").count(),
		)
	}
}

fn rename_or_skip(src: &Path, dst: &Path) {
	if dst.exists() {
		let _ = fs::remove_dir_all(dst);
	}
	let _ = fs::rename(src, dst);
}

/// Rollback skills to the latest curator backup.
#[allow(dead_code)]
pub fn curator_rollback() -> String {
	let backup_dir = skills_dir().join(".curator_backups");
	let Ok(entries) = fs::read_dir(&backup_dir) else {
		return "No curator backups found.".to_string();
	};
	let mut snaps: Vec<_> = entries
		.filter_map(|e| e.ok())
		.map(|e| e.file_name().to_string_lossy().to_string())
		.filter(|n| n.parse::<u64>().is_ok())
		.collect();
	snaps.sort();
	if let Some(latest) = snaps.last() {
		let snap_path = backup_dir.join(latest);
		if let Ok(snap_entries) = fs::read_dir(&snap_path) {
			let mut restored = 0u32;
			for e in snap_entries.flatten() {
				let name = e.file_name();
				let skill_dir = skills_dir().join(&name);
				let _ = fs::create_dir_all(&skill_dir);
				let src_md = e.path().join("SKILL.md");
				if src_md.is_file()
					&& let Ok(content) = fs::read_to_string(&src_md)
				{
					let _ = fs::write(skill_dir.join("SKILL.md"), &content);
					restored += 1;
				}
			}
			return format!("Rolled back {} skills from backup {latest}.", restored);
		}
	}
	"No curator backup to restore.".to_string()
}

/// Check if curator should run (once per session start).
pub fn should_run_curator() -> bool {
	let state_path = skills_dir().join(".curator_state");
	if let Ok(text) = fs::read_to_string(&state_path)
		&& let Ok(last_run) = text.trim().parse::<u64>()
	{
		let elapsed = now_secs().saturating_sub(last_run);
		return elapsed >= STALE_AFTER_SECS;
	}
	true
}

/// Mark curator as run.
pub fn mark_curator_run() {
	let _ = fs::write(skills_dir().join(".curator_state"), now_secs().to_string());
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Mutex;

	static ENV_LOCK: Mutex<()> = Mutex::new(());

	#[test]
	fn validates_names() {
		assert!(validate_name("rust-cargo-check").is_ok());
		assert!(validate_name("fix-pr-42").is_err());
		assert!(validate_name("Bad Name").is_err());
	}

	#[test]
	fn auto_threshold() {
		assert!(!should_auto_create_skill(2, 2, "done"));
		assert!(should_auto_create_skill(6, 4, "Implemented and verified."));
	}

	#[test]
	fn create_and_list() {
		let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		let tmp = tempfile::tempdir().unwrap();
		let old = std::env::var("DX_AGENT_WORKSPACE").ok();
		// SAFETY: serialised by ENV_LOCK; test runs single-threaded before any concurrent env access
		unsafe {
			std::env::set_var("DX_AGENT_WORKSPACE", tmp.path());
		}
		// ensure_workspace is called inside execute_skill_manage
		let call = ToolCall {
			id: "1".into(),
			name: "skill_manage".into(),
			arguments: json!({
				"action": "create",
				"name": "demo-workflow",
				"description": "Demo",
				"content": "Do the thing carefully."
			})
			.to_string(),
		};
		let r = execute_skill_manage(&call);
		assert!(r.ok, "{}", r.output);
		let list = list_skills();
		assert!(list.iter().any(|s| s.name == "demo-workflow"));
		if let Some(val) = old {
			// SAFETY: serialised by ENV_LOCK; test runs single-threaded
			unsafe {
				std::env::set_var("DX_AGENT_WORKSPACE", val);
			}
		} else {
			// SAFETY: serialised by ENV_LOCK; test runs single-threaded
			unsafe {
				std::env::remove_var("DX_AGENT_WORKSPACE");
			}
		}
	}

	#[test]
	fn skill_telemetry_records_usage() {
		let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		let tmp = tempfile::Builder::new().prefix("dx-tui-test-telemetry-").tempdir().unwrap();
		let old = std::env::var("DX_AGENT_WORKSPACE").ok();
		// SAFETY: serialised by ENV_LOCK; test runs single-threaded before any concurrent env access
		unsafe {
			std::env::set_var("DX_AGENT_WORKSPACE", tmp.path());
		}
		record_skill_use("test-skill");
		record_skill_view("test-skill");
		record_skill_patch("test-skill");
		let usage = get_all_usage();
		let entry = usage.get("test-skill").expect("telemetry entry");
		assert_eq!(entry.use_count, 1);
		assert_eq!(entry.view_count, 1);
		assert_eq!(entry.patch_count, 1);
		assert_eq!(entry.state, "active");
		if let Some(val) = old {
			// SAFETY: serialised by ENV_LOCK; test runs single-threaded
			unsafe {
				std::env::set_var("DX_AGENT_WORKSPACE", val);
			}
		} else {
			// SAFETY: serialised by ENV_LOCK; test runs single-threaded
			unsafe {
				std::env::remove_var("DX_AGENT_WORKSPACE");
			}
		}
	}

	#[test]
	fn curator_no_stale_skills() {
		let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		let tmp = tempfile::tempdir().unwrap();
		let old = std::env::var("DX_AGENT_WORKSPACE").ok();
		// SAFETY: serialised by ENV_LOCK; test runs single-threaded before any concurrent env access
		unsafe {
			std::env::set_var("DX_AGENT_WORKSPACE", tmp.path());
		}
		let report = run_curator();
		assert!(report.contains("No stale"));
		if let Some(val) = old {
			// SAFETY: serialised by ENV_LOCK; test runs single-threaded
			unsafe {
				std::env::set_var("DX_AGENT_WORKSPACE", val);
			}
		} else {
			// SAFETY: serialised by ENV_LOCK; test runs single-threaded
			unsafe {
				std::env::remove_var("DX_AGENT_WORKSPACE");
			}
		}
	}

	#[test]
	fn curator_new_session_runs_once() {
		let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
		let tmp = tempfile::tempdir().unwrap();
		let old = std::env::var("DX_AGENT_WORKSPACE").ok();
		// SAFETY: serialised by ENV_LOCK; test runs single-threaded before any concurrent env access
		unsafe {
			std::env::set_var("DX_AGENT_WORKSPACE", tmp.path());
		}
		assert!(should_run_curator());
		mark_curator_run();
		assert!(!should_run_curator());
		if let Some(val) = old {
			// SAFETY: serialised by ENV_LOCK; test runs single-threaded
			unsafe {
				std::env::set_var("DX_AGENT_WORKSPACE", val);
			}
		} else {
			// SAFETY: serialised by ENV_LOCK; test runs single-threaded
			unsafe {
				std::env::remove_var("DX_AGENT_WORKSPACE");
			}
		}
	}
}
