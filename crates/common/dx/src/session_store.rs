//! Persist chat sessions to disk so `/sessions` survives restarts.
//!
//! Layout: `~/.config/dx/sessions/<id>.json` (+ `index.json` for fast listing).

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
	components::{Message, MessageRole},
	modes::{AgentMode, RuntimeMode},
	slash_commands::StoredSession,
};

/// Obfuscate bytes using XOR with a derived key.
/// This is NOT cryptography — it prevents casual reading of session files.
/// For real encryption, SQLCipher would be needed.
fn obfuscate(data: &[u8]) -> Vec<u8> {
	let key = obfuscation_key();
	data.iter().enumerate().map(|(i, b)| b ^ key[i % key.len()]).collect()
}

fn obfuscation_key() -> Vec<u8> {
	let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "dx-tui-default".into());
	let pepper = "dx-tui-session-v1";
	let combined = format!("{hostname}:{pepper}");
	combined.bytes().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskMessage {
	role: String,
	content: String,
	timestamp: String,
	#[serde(default)]
	token_count: usize,
	#[serde(default)]
	thinking_expanded: bool,
	#[serde(default)]
	commands_expanded: bool,
	#[serde(default)]
	subagents_expanded: bool,
	#[serde(default)]
	footer_profile: Option<String>,
	#[serde(default)]
	footer_model: Option<String>,
	#[serde(default)]
	footer_duration_ms: Option<u64>,
	#[serde(default)]
	footer_reasoning: Option<String>,
	#[serde(default)]
	thinking_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskSession {
	id: String,
	name: String,
	messages: Vec<DiskMessage>,
	model: String,
	model_display: String,
	provider: String,
	agent_mode: String,
	runtime_mode: String,
	created_at: String,
	updated_at: String,
	#[serde(default)]
	shared: bool,
	share_url: Option<String>,
	#[serde(default)]
	project_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionIndex {
	/// Newest-first session ids.
	ids: Vec<String>,
}

fn sessions_dir() -> PathBuf {
	dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("dx").join("sessions")
}

fn index_path() -> PathBuf {
	sessions_dir().join("index.json")
}

fn session_path(id: &str) -> PathBuf {
	// Sanitize id for filesystem
	let safe: String = id
		.chars()
		.map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
		.collect();
	sessions_dir().join(format!("{safe}.json"))
}

fn ensure_dir() -> Result<PathBuf> {
	let dir = sessions_dir();
	fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
	Ok(dir)
}

fn agent_mode_from(s: &str) -> AgentMode {
	match s.to_ascii_lowercase().as_str() {
		"write" => AgentMode::Write,
		"plan" => AgentMode::Plan,
		"goal" => AgentMode::Goal,
		"agent" => AgentMode::Agent,
		_ => AgentMode::Ask,
	}
}

fn runtime_mode_from(s: &str) -> RuntimeMode {
	if s.eq_ignore_ascii_case("local") { RuntimeMode::Local } else { RuntimeMode::Remote }
}

fn parse_dt(s: &str) -> chrono::DateTime<chrono::Local> {
	chrono::DateTime::parse_from_rfc3339(s)
		.map(|dt| dt.with_timezone(&chrono::Local))
		.unwrap_or_else(|_| chrono::Local::now())
}

fn message_to_disk(m: &Message) -> DiskMessage {
	DiskMessage {
		role: match m.role {
			MessageRole::User => "user".into(),
			MessageRole::Assistant => "assistant".into(),
		},
		content: m.content.clone(),
		timestamp: m.timestamp.to_rfc3339(),
		token_count: m.token_count,
		thinking_expanded: m.thinking_expanded,
		commands_expanded: m.commands_expanded,
		subagents_expanded: m.subagents_expanded,
		footer_profile: m.footer_profile.clone(),
		footer_model: m.footer_model.clone(),
		footer_duration_ms: m.footer_duration.map(|d| d.as_millis() as u64),
		footer_reasoning: m.footer_reasoning.clone(),
		thinking_duration_ms: m.thinking_duration.map(|d| d.as_millis() as u64),
	}
}

fn message_from_disk(m: DiskMessage) -> Message {
	let role =
		if m.role.eq_ignore_ascii_case("user") { MessageRole::User } else { MessageRole::Assistant };
	let mut msg = Message {
		id: Message::new_id(),
		parent_id: None,
		branch_id: "main".into(),
		hidden: false,
		role,
		content: m.content,
		parts: Vec::new(),
		timestamp: parse_dt(&m.timestamp),
		token_count: m.token_count,
		thinking_expanded: m.thinking_expanded,
		commands_expanded: m.commands_expanded,
		subagents_expanded: m.subagents_expanded,
		command_expand: std::collections::HashMap::new(),
		subagent_expand: std::collections::HashMap::new(),
		thinking_expand: std::collections::HashMap::new(),
		footer_profile: m.footer_profile,
		footer_model: m.footer_model,
		footer_duration: m.footer_duration_ms.map(std::time::Duration::from_millis),
		footer_reasoning: m.footer_reasoning,
		thinking_duration: m.thinking_duration_ms.map(std::time::Duration::from_millis),
		tokens_in: None,
		tokens_out: None,
		tool_count: 0,
		interrupted: false,
	};
	msg.sync_parts_from_content();
	msg
}

fn to_disk(s: &StoredSession) -> DiskSession {
	DiskSession {
		id: s.id.clone(),
		name: s.name.clone(),
		messages: s.messages.iter().map(message_to_disk).collect(),
		model: s.model.clone(),
		model_display: s.model_display.clone(),
		provider: s.provider.clone(),
		agent_mode: s.agent_mode.label().to_string(),
		runtime_mode: s.runtime_mode.label().to_string(),
		created_at: s.created_at.to_rfc3339(),
		updated_at: s.updated_at.to_rfc3339(),
		shared: s.shared,
		share_url: s.share_url.clone(),
		project_dir: s.project_dir.clone(),
	}
}

fn from_disk(s: DiskSession) -> StoredSession {
	StoredSession {
		id: s.id,
		name: s.name,
		messages: s.messages.into_iter().map(message_from_disk).collect(),
		model: s.model,
		model_display: s.model_display,
		provider: s.provider,
		agent_mode: agent_mode_from(&s.agent_mode),
		runtime_mode: runtime_mode_from(&s.runtime_mode),
		created_at: parse_dt(&s.created_at),
		updated_at: parse_dt(&s.updated_at),
		shared: s.shared,
		share_url: s.share_url,
		project_dir: s.project_dir,
	}
}

/// Save one session snapshot to disk and update the index.
pub fn save_session(session: &StoredSession) -> Result<()> {
	ensure_dir()?;
	let path = session_path(&session.id);
	let disk = to_disk(session);
	let json = serde_json::to_string_pretty(&disk).context("serialize session")?;
	let encrypted = obfuscate(json.as_bytes());
	// Atomic write: write to temp file, then rename
	let tmp_path = path.with_extension("tmp");
	fs::write(&tmp_path, &encrypted).with_context(|| format!("write {}", tmp_path.display()))?;
	fs::rename(&tmp_path, &path)
		.with_context(|| format!("rename {} -> {}", tmp_path.display(), path.display()))?;

	let mut index = load_index().unwrap_or_default();
	index.ids.retain(|id| id != &session.id);
	index.ids.insert(0, session.id.clone());
	// Cap index
	if index.ids.len() > 200 {
		index.ids.truncate(200);
	}
	save_index(&index)?;
	Ok(())
}

/// Load all known sessions (newest first). Missing files are skipped.
pub fn load_all_sessions() -> Vec<StoredSession> {
	let index = match load_index() {
		Ok(i) if !i.ids.is_empty() => i,
		_ => return scan_sessions_dir(),
	};
	let mut out = Vec::new();
	for id in index.ids {
		if let Ok(s) = load_session_by_id(&id) {
			out.push(s);
		}
	}
	if out.is_empty() {
		return scan_sessions_dir();
	}
	out
}

pub fn load_session_by_id(id: &str) -> Result<StoredSession> {
	let path = session_path(id);
	if path.is_file() {
		let encrypted = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
		let decrypted = obfuscate(&encrypted);
		let text = String::from_utf8(decrypted).context("decode session")?;
		let disk: DiskSession = serde_json::from_str(&text).context("parse session json")?;
		return Ok(from_disk(disk));
	}
	// Prefix match (short ids from `dx continue abcdef12`)
	let needle = id.trim();
	if needle.len() >= 4 {
		for s in load_all_sessions() {
			if s.id.starts_with(needle) || short_session_id(&s.id) == needle {
				return Ok(s);
			}
		}
	}
	anyhow::bail!("session not found: {id}")
}

/// Short id for CLI resume: first 8 chars of UUID (before first `-` or first 8).
pub fn short_session_id(id: &str) -> String {
	id.split('-').next().unwrap_or(id).chars().take(8).collect()
}

/// Shell command to resume this session.
pub fn continue_command(id: &str) -> String {
	format!("dx continue {}", short_session_id(id))
}

fn scan_sessions_dir() -> Vec<StoredSession> {
	let dir = sessions_dir();
	let Ok(rd) = fs::read_dir(&dir) else {
		return Vec::new();
	};
	let mut out = Vec::new();
	for e in rd.flatten() {
		let path = e.path();
		if path.extension().and_then(|x| x.to_str()) != Some("json") {
			continue;
		}
		if path.file_name().and_then(|n| n.to_str()) == Some("index.json") {
			continue;
		}
		if let Ok(encrypted) = fs::read(&path) {
			let decrypted = obfuscate(&encrypted);
			if let Ok(text) = String::from_utf8(decrypted)
				&& let Ok(disk) = serde_json::from_str::<DiskSession>(&text)
			{
				out.push(from_disk(disk));
			}
		}
	}
	out.sort_unstable_by_key(|b| std::cmp::Reverse(b.updated_at));
	out
}

fn load_index() -> Result<SessionIndex> {
	let path = index_path();
	if !path.is_file() {
		return Ok(SessionIndex::default());
	}
	let encrypted = fs::read(&path)?;
	let decrypted = obfuscate(&encrypted);
	let text = String::from_utf8(decrypted).unwrap_or_default();
	Ok(serde_json::from_str(&text).unwrap_or_default())
}

fn save_index(index: &SessionIndex) -> Result<()> {
	ensure_dir()?;
	let path = index_path();
	let json = serde_json::to_string_pretty(index)?;
	let encrypted = obfuscate(json.as_bytes());
	let tmp_path = path.with_extension("tmp");
	fs::write(&tmp_path, &encrypted).with_context(|| format!("write {}", tmp_path.display()))?;
	fs::rename(&tmp_path, &path)
		.with_context(|| format!("rename {} -> {}", tmp_path.display(), path.display()))?;
	Ok(())
}

/// Directory used for sessions (for status/debug).
pub fn sessions_root() -> PathBuf {
	sessions_dir()
}

/// Best-effort auto-save current session.
pub fn autosave(session: &StoredSession) {
	if let Err(e) = save_session(session) {
		tracing::warn!("session autosave failed: {e}");
	}
}

/// Delete a session file from disk (used by session cleanup / `/new` GC).
pub fn delete_session(id: &str) -> Result<()> {
	let path = session_path(id);
	if path.is_file() {
		fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
	}
	if let Ok(mut index) = load_index() {
		index.ids.retain(|x| x != id);
		save_index(&index)?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::modes::AgentMode;

	#[test]
	fn roundtrip_message_role() {
		let m = Message::user("hello".into());
		let d = message_to_disk(&m);
		let back = message_from_disk(d);
		assert_eq!(back.content, "hello");
		assert_eq!(back.role, MessageRole::User);
	}

	#[test]
	fn agent_mode_parse() {
		assert_eq!(agent_mode_from("Agent"), AgentMode::Agent);
		assert_eq!(agent_mode_from("goal"), AgentMode::Goal);
		assert_eq!(agent_mode_from("???"), AgentMode::Ask);
	}

	#[test]
	fn session_path_sanitizes() {
		let p = session_path("abc/../def");
		assert!(p.file_name().unwrap().to_string_lossy().contains("abc"));
	}

	#[test]
	fn continue_command_uses_short_id() {
		let cmd = continue_command("abcdef12-3456-7890-abcd-ef1234567890");
		assert_eq!(cmd, "dx continue abcdef12");
	}
}
