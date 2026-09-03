use std::{
	path::PathBuf,
	time::{Duration, Instant},
};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};

use crate::{
	components::{Message, MessageRole},
	modes::{AgentMode, RuntimeMode},
	slash_commands::StoredSession,
};

/// Obfuscate bytes using XOR with a derived key.
/// This is NOT cryptography — it prevents casual reading of session data.
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

const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

fn hex_encode(data: &[u8]) -> String {
	let mut s = String::with_capacity(data.len() * 2);
	for &b in data {
		s.push(HEX_CHARS[(b >> 4) as usize] as char);
		s.push(HEX_CHARS[(b & 0x0f) as usize] as char);
	}
	s
}

fn hex_decode(s: &str) -> Vec<u8> {
	let bytes = s.as_bytes();
	let mut out = Vec::with_capacity(bytes.len() / 2);
	let mut i = 0;
	while i + 1 < bytes.len() {
		let hi = (bytes[i] as char).to_digit(16).unwrap_or(0) as u8;
		let lo = (bytes[i + 1] as char).to_digit(16).unwrap_or(0) as u8;
		out.push((hi << 4) | lo);
		i += 2;
	}
	out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
	pub id: String,
	pub name: String,
	pub model: String,
	pub model_display: String,
	pub provider: String,
	pub agent_mode: String,
	pub runtime_mode: String,
	pub created_at: String,
	pub updated_at: String,
	pub shared: bool,
	pub share_url: Option<String>,
	pub project_dir: String,
	pub tags: Vec<String>,
	pub is_archived: bool,
	pub message_count: usize,
	pub total_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkRecord {
	pub child_id: String,
	pub fork_from_message: usize,
	pub created_at: String,
}

fn meta_from_row(row: &rusqlite::Row) -> rusqlite::Result<SessionMeta> {
	let tags_str: String = row.get("tags")?;
	let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
	Ok(SessionMeta {
		id: row.get("id")?,
		name: row.get("name")?,
		model: row.get("model")?,
		model_display: row.get("model_display")?,
		provider: row.get("provider")?,
		agent_mode: row.get("agent_mode")?,
		runtime_mode: row.get("runtime_mode")?,
		created_at: row.get("created_at")?,
		updated_at: row.get("updated_at")?,
		shared: row.get::<_, i32>("shared")? != 0,
		share_url: row.get("share_url")?,
		project_dir: row.get("project_dir")?,
		tags,
		is_archived: row.get::<_, i32>("is_archived")? != 0,
		message_count: row.get::<_, i32>("message_count")? as usize,
		total_tokens: row.get::<_, i32>("total_tokens")? as usize,
	})
}

pub struct SessionDatabase {
	/// SQLite connection wrapped in a mutex for interior mutability.
	/// Note: Connection is not Clone, so SessionDatabase is not Clone either.
	conn: Connection,
	base_dir: PathBuf,
	archive_dir: PathBuf,
	dirty: bool,
	last_persist: Instant,
	#[allow(dead_code)]
	persist_interval: Duration,
}

impl SessionDatabase {
	pub fn new() -> Self {
		let base_dir =
			dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("dx").join("sessions");
		Self::with_dir(base_dir)
	}

	pub fn with_dir(base_dir: PathBuf) -> Self {
		let archive_dir = base_dir.join("archive");
		std::fs::create_dir_all(&base_dir).ok();
		std::fs::create_dir_all(&archive_dir).ok();

		let db_path = base_dir.join("dx-tui.db");
		let conn = Connection::open_with_flags(
			&db_path,
			OpenFlags::SQLITE_OPEN_CREATE
				| OpenFlags::SQLITE_OPEN_READ_WRITE
				| OpenFlags::SQLITE_OPEN_FULL_MUTEX,
		)
		.unwrap_or_else(|e| panic!("Failed to open SQLite database at {}: {e}", db_path.display()));

		conn
			.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")
			.ok();

		let sql = "
            CREATE TABLE IF NOT EXISTS sessions (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL DEFAULT '',
                model       TEXT NOT NULL DEFAULT '',
                model_display TEXT NOT NULL DEFAULT '',
                provider    TEXT NOT NULL DEFAULT '',
                agent_mode  TEXT NOT NULL DEFAULT 'ask',
                runtime_mode TEXT NOT NULL DEFAULT 'remote',
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                shared      INTEGER NOT NULL DEFAULT 0,
                share_url   TEXT,
                project_dir TEXT NOT NULL DEFAULT '.',
                is_archived INTEGER NOT NULL DEFAULT 0,
                tags        TEXT NOT NULL DEFAULT '[]',
                fork_parent TEXT,
                message_count INTEGER NOT NULL DEFAULT 0,
                total_tokens  INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS messages (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                seq         INTEGER NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                timestamp   TEXT NOT NULL,
                token_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                content, role,
                content=messages,
                content_rowid=id,
                tokenize='porter unicode61'
            );
            CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
                INSERT INTO messages_fts(rowid, content, role) VALUES (new.id, new.content, new.role);
            END;
            CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
                INSERT INTO messages_fts(messages_fts, rowid, content, role) VALUES ('delete', old.id, old.content, old.role);
            END;
            CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
                INSERT INTO messages_fts(messages_fts, rowid, content, role) VALUES ('delete', old.id, old.content, old.role);
                INSERT INTO messages_fts(rowid, content, role) VALUES (new.id, new.content, new.role);
            END;
            CREATE TABLE IF NOT EXISTS session_events (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                seq         INTEGER NOT NULL,
                timestamp   TEXT NOT NULL,
                event_type  TEXT NOT NULL,
                payload     TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE IF NOT EXISTS fork_records (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                child_id    TEXT NOT NULL,
                fork_from_message INTEGER NOT NULL,
                created_at  TEXT NOT NULL
            );
        ";
		conn.execute_batch(sql).unwrap_or_else(|e| panic!("Failed to create SQLite schema: {e}"));

		Self {
			conn,
			base_dir,
			archive_dir,
			dirty: false,
			last_persist: Instant::now(),
			persist_interval: Duration::from_secs(30),
		}
	}

	pub fn base_dir(&self) -> &PathBuf {
		&self.base_dir
	}
	pub fn archive_dir(&self) -> &PathBuf {
		&self.archive_dir
	}

	fn ensure_dir(&self) -> Result<()> {
		std::fs::create_dir_all(&self.base_dir)
			.with_context(|| format!("create {}", self.base_dir.display()))?;
		std::fs::create_dir_all(&self.archive_dir)
			.with_context(|| format!("create {}", self.archive_dir.display()))?;
		Ok(())
	}

	pub fn save_session(&mut self, session: &StoredSession, messages: &[Message]) -> Result<()> {
		self.ensure_dir()?;
		let total_tokens: usize = messages.iter().map(|m| m.token_count).sum();
		let tags_json = "[]";

		let tx = self.conn.transaction().context("begin transaction")?;

		tx.execute(
            "INSERT OR REPLACE INTO sessions (id, name, model, model_display, provider, agent_mode, runtime_mode, created_at, updated_at, shared, share_url, project_dir, is_archived, tags, message_count, total_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                session.id,
                session.name,
                session.model,
                session.model_display,
                session.provider,
                session.agent_mode.label(),
                session.runtime_mode.label(),
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                session.shared as i32,
                session.share_url,
                session.project_dir,
                false,
                tags_json,
                messages.len() as i32,
                total_tokens as i32,
            ],
        )?;

		tx.execute("DELETE FROM messages WHERE session_id = ?1", params![session.id])?;
		for (seq, msg) in messages.iter().enumerate() {
			let encrypted = obfuscate(msg.content.as_bytes());
			let encoded = hex_encode(&encrypted);
			tx.execute(
                "INSERT INTO messages (session_id, seq, role, content, timestamp, token_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session.id,
                    seq as i32,
                    match msg.role { MessageRole::User => "user", MessageRole::Assistant => "assistant" },
                    encoded,
                    msg.timestamp.to_rfc3339(),
                    msg.token_count as i32,
                ],
            )?;
		}

		append_event_tx(
			&tx,
			session,
			"snapshot",
			&serde_json::json!({
					"message_count": messages.len(),
					"name": &session.name,
			}),
		)?;

		tx.commit().context("commit transaction")?;

		self.dirty = false;
		self.last_persist = Instant::now();
		Ok(())
	}

	pub fn load_session(&self, id: &str) -> Result<(StoredSession, Vec<Message>)> {
		let meta = self
			.conn
			.query_row("SELECT * FROM sessions WHERE id = ?1", params![id], meta_from_row)
			.with_context(|| format!("session not found: {id}"))?;

		let mut stmt = self.conn.prepare(
            "SELECT seq, role, content, timestamp, token_count FROM messages WHERE session_id = ?1 ORDER BY seq",
        )?;
		let msg_iter = stmt.query_map(params![id], |row| {
			let role_str: String = row.get(1)?;
			let content_encoded: String = row.get(2)?;
			let content_bytes = hex_decode(&content_encoded);
			let content_decrypted = obfuscate(&content_bytes);
			let content = String::from_utf8(content_decrypted).unwrap_or_default();
			Ok({
				let mut m = Message {
					id: Message::new_id(),
					parent_id: None,
					branch_id: "main".into(),
					hidden: false,
					role: if role_str == "user" { MessageRole::User } else { MessageRole::Assistant },
					content,
					parts: Vec::new(),
					timestamp: parse_dt(&row.get::<_, String>(3)?),
					token_count: row.get::<_, i32>(4)? as usize,
					thinking_expanded: false,
					commands_expanded: false,
					subagents_expanded: false,
					command_expand: std::collections::HashMap::new(),
					subagent_expand: std::collections::HashMap::new(),
					thinking_expand: std::collections::HashMap::new(),
					footer_profile: None,
					footer_model: None,
					footer_duration: None,
					thinking_duration: None,
					footer_reasoning: None,
					tokens_in: None,
					tokens_out: None,
					tool_count: 0,
					interrupted: false,
				};
				m.sync_parts_from_content();
				m
			})
		})?;

		let mut messages = Vec::new();
		for msg in msg_iter {
			messages.push(msg?);
		}

		let stored = StoredSession {
			id: meta.id.clone(),
			name: meta.name,
			messages: messages.clone(),
			model: meta.model,
			model_display: meta.model_display,
			provider: meta.provider,
			agent_mode: agent_mode_from(&meta.agent_mode),
			runtime_mode: runtime_mode_from(&meta.runtime_mode),
			created_at: parse_dt(&meta.created_at),
			updated_at: parse_dt(&meta.updated_at),
			shared: meta.shared,
			share_url: meta.share_url,
			project_dir: meta.project_dir,
		};

		Ok((stored, messages))
	}

	pub fn find_by_prefix(&self, prefix: &str) -> Vec<SessionMeta> {
		if prefix.is_empty() {
			return self.list_sessions();
		}
		let needle = format!("%{}%", prefix.trim().to_lowercase());
		let Ok(mut stmt) = self
            .conn
            .prepare(
                "SELECT * FROM sessions WHERE LOWER(id) LIKE ?1 OR LOWER(name) LIKE ?1 ORDER BY updated_at DESC LIMIT 200",
            ) else {
            return Vec::new();
        };
		let Ok(rows) = stmt.query_map(params![needle], meta_from_row) else {
			return Vec::new();
		};
		rows.filter_map(|r| r.ok()).collect()
	}

	pub fn list_sessions(&self) -> Vec<SessionMeta> {
		let Ok(mut stmt) =
			self.conn.prepare("SELECT * FROM sessions ORDER BY updated_at DESC LIMIT 200")
		else {
			return Vec::new();
		};
		let Ok(rows) = stmt.query_map([], meta_from_row) else {
			return Vec::new();
		};
		rows.filter_map(|r| r.ok()).collect()
	}

	pub fn delete_session(&mut self, id: &str) -> Result<()> {
		self.conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
		self.conn.execute("DELETE FROM messages WHERE session_id = ?1", params![id])?;
		self.conn.execute("DELETE FROM session_events WHERE session_id = ?1", params![id])?;
		self.conn.execute("DELETE FROM fork_records WHERE session_id = ?1", params![id])?;
		Ok(())
	}

	pub fn archive_session(&mut self, id: &str) -> Result<()> {
		let rows = self.conn.execute(
			"UPDATE sessions SET is_archived = 1, updated_at = ?1 WHERE id = ?2",
			params![chrono::Local::now().to_rfc3339(), id],
		)?;
		if rows == 0 {
			anyhow::bail!("session not found: {id}");
		}
		Ok(())
	}

	pub fn unarchive_session(&mut self, id: &str) -> Result<()> {
		let rows = self.conn.execute(
			"UPDATE sessions SET is_archived = 0, updated_at = ?1 WHERE id = ?2",
			params![chrono::Local::now().to_rfc3339(), id],
		)?;
		if rows == 0 {
			anyhow::bail!("archived session not found: {id}");
		}
		Ok(())
	}

	pub fn fork_session(
		&mut self,
		source_id: &str,
		from_message: usize,
		new_id: &str,
		new_name: &str,
	) -> Result<(StoredSession, Vec<Message>)> {
		let (mut stored, mut messages) = self.load_session(source_id)?;
		if from_message < messages.len() {
			messages.truncate(from_message + 1);
		}
		stored.id = new_id.to_string();
		stored.name = new_name.to_string();
		stored.created_at = chrono::Local::now();
		stored.updated_at = chrono::Local::now();

		let now = chrono::Local::now().to_rfc3339();
		self.conn.execute(
            "INSERT INTO fork_records (session_id, child_id, fork_from_message, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![source_id, new_id, from_message as i32, now],
        )?;

		self.save_session(&stored, &messages)?;
		Ok((stored, messages))
	}

	pub fn export_session(
		&self,
		id: &str,
		include_thinking: bool,
		include_tools: bool,
	) -> Result<String> {
		let (stored, messages) = self.load_session(id)?;
		let mut out = String::new();
		out.push_str(&format!("# {}\n\n", stored.name));
		for msg in &messages {
			let role = match msg.role {
				MessageRole::User => "User",
				MessageRole::Assistant => "Assistant",
			};
			let mut content = msg.content.clone();
			if !include_thinking {
				content = strip_thinking_blocks(&content);
			}
			if !include_tools {
				content = strip_tool_blocks(&content);
			}
			out.push_str(&format!("**{}:**\n{}\n\n", role, content));
		}
		Ok(out)
	}

	pub fn compact_events(&mut self, id: &str) -> Result<()> {
		self.conn.execute("DELETE FROM session_events WHERE session_id = ?1", params![id])?;
		Ok(())
	}

	pub fn set_tags(&mut self, id: &str, tags: Vec<String>) -> Result<()> {
		let tags_json = serde_json::to_string(&tags)?;
		let rows =
			self.conn.execute("UPDATE sessions SET tags = ?1 WHERE id = ?2", params![tags_json, id])?;
		if rows == 0 {
			anyhow::bail!("session not found: {id}");
		}
		Ok(())
	}

	pub fn mark_dirty(&mut self) {
		self.dirty = true;
	}

	pub fn flush(&mut self) -> bool {
		self.dirty = false;
		self.last_persist = Instant::now();
		true
	}

	pub fn migrate_legacy(&mut self) -> Result<usize> {
		let legacy_dir = self.base_dir.clone();
		if !legacy_dir.is_dir() {
			return Ok(0);
		}
		let mut count = 0;
		if let Ok(rd) = std::fs::read_dir(&legacy_dir) {
			for entry in rd.flatten() {
				let path = entry.path();
				if path.extension().and_then(|e| e.to_str()) != Some("json") {
					continue;
				}
				let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
				if fname == "sessions.idx" || fname == "index.json" || fname == "dx-tui.db" {
					continue;
				}
				let Ok(text) = std::fs::read_to_string(&path) else {
					continue;
				};
				let snapshot: Result<SessionSnapshot, _> = serde_json::from_str(&text);
				if let Ok(snapshot) = snapshot {
					if self
						.conn
						.query_row(
							"SELECT 1 FROM sessions WHERE id = ?1",
							params![snapshot.meta.id],
							|_| Ok(()),
						)
						.is_err()
					{
						let stored = snapshot_to_stored(&snapshot);
						let messages: Vec<Message> =
							snapshot.messages.into_iter().map(message_from_disk).collect();
						if self.save_session(&stored, &messages).is_ok() {
							count += 1;
						}
					}
					let _ = std::fs::remove_file(&path);
				}
			}
		}
		Ok(count)
	}

	#[allow(dead_code)]
	fn append_event(
		&self,
		session: &StoredSession,
		event_type: &str,
		payload: &serde_json::Value,
	) -> Result<()> {
		let seq = self
			.conn
			.query_row(
				"SELECT COALESCE(MAX(seq), 0) + 1 FROM session_events WHERE session_id = ?1",
				params![session.id],
				|row| row.get::<_, i32>(0),
			)
			.unwrap_or(0);
		self.conn.execute(
            "INSERT INTO session_events (session_id, seq, timestamp, event_type, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session.id,
                seq,
                chrono::Local::now().to_rfc3339(),
                event_type,
                serde_json::to_string(payload)?,
            ],
        )?;
		Ok(())
	}

	pub fn search_fts(&self, query: &str, limit: usize) -> Vec<(String, f64)> {
		if query.trim().is_empty() {
			return Vec::new();
		}
		let Ok(mut stmt) = self.conn.prepare(
			"SELECT sessions.id, rank FROM messages_fts
             JOIN sessions ON sessions.id = messages.session_id
             WHERE messages_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
		) else {
			tracing::warn!("FTS5 query preparation failed, falling back to LIKE search");
			return self.fts_fallback(query, limit);
		};
		let Ok(rows) = stmt.query_map(params![query, limit as i32], |row| {
			let id: String = row.get(0)?;
			let rank: f64 = row.get(1)?;
			Ok((id, rank))
		}) else {
			return Vec::new();
		};
		rows.filter_map(|r| r.ok()).collect()
	}

	fn fts_fallback(&self, query: &str, limit: usize) -> Vec<(String, f64)> {
		let needle = format!("%{}%", query.replace('\'', "''"));
		let Ok(mut stmt) = self.conn.prepare(
			"SELECT DISTINCT s.id, CAST(COUNT(m.id) AS REAL) as rank
             FROM sessions s
             JOIN messages m ON m.session_id = s.id
             WHERE m.content LIKE ?1
             GROUP BY s.id
             ORDER BY rank DESC
             LIMIT ?2",
		) else {
			return Vec::new();
		};
		let Ok(rows) = stmt.query_map(params![needle, limit as i32], |row| {
			let id: String = row.get(0)?;
			let rank: f64 = row.get(1)?;
			Ok((id, rank))
		}) else {
			return Vec::new();
		};
		rows.filter_map(|r| r.ok()).collect()
	}
}

impl Default for SessionDatabase {
	fn default() -> Self {
		Self::new()
	}
}

// Free function to append an event within a transaction (not a method on SessionDatabase to avoid borrow conflicts)
fn append_event_tx(
	tx: &rusqlite::Transaction,
	session: &StoredSession,
	event_type: &str,
	payload: &serde_json::Value,
) -> Result<()> {
	let seq = tx
		.query_row(
			"SELECT COALESCE(MAX(seq), 0) + 1 FROM session_events WHERE session_id = ?1",
			params![session.id],
			|row| row.get::<_, i32>(0),
		)
		.unwrap_or(0);
	tx.execute(
        "INSERT INTO session_events (session_id, seq, timestamp, event_type, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            session.id,
            seq,
            chrono::Local::now().to_rfc3339(),
            event_type,
            serde_json::to_string(payload)?,
        ],
    )?;
	Ok(())
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

#[allow(dead_code)]
pub fn short_session_id(id: &str) -> String {
	id.split('-').next().unwrap_or(id).chars().take(8).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionSnapshot {
	meta: SessionMeta,
	messages: Vec<DiskMessage>,
	fork_parent: Option<String>,
	fork_records: Vec<ForkRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskMessage {
	role: String,
	content: String,
	timestamp: String,
	token_count: usize,
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
		thinking_expanded: false,
		commands_expanded: false,
		subagents_expanded: false,
		command_expand: std::collections::HashMap::new(),
		subagent_expand: std::collections::HashMap::new(),
		thinking_expand: std::collections::HashMap::new(),
		footer_profile: None,
		footer_model: None,
		footer_duration: None,
		footer_reasoning: None,
		thinking_duration: None,
		tokens_in: None,
		tokens_out: None,
		tool_count: 0,
		interrupted: false,
	};
	msg.sync_parts_from_content();
	msg
}

fn snapshot_to_stored(snapshot: &SessionSnapshot) -> StoredSession {
	StoredSession {
		id: snapshot.meta.id.clone(),
		name: snapshot.meta.name.clone(),
		messages: snapshot.messages.iter().map(|d| message_from_disk(d.clone())).collect(),
		model: snapshot.meta.model.clone(),
		model_display: snapshot.meta.model_display.clone(),
		provider: snapshot.meta.provider.clone(),
		agent_mode: agent_mode_from(&snapshot.meta.agent_mode),
		runtime_mode: runtime_mode_from(&snapshot.meta.runtime_mode),
		created_at: parse_dt(&snapshot.meta.created_at),
		updated_at: parse_dt(&snapshot.meta.updated_at),
		shared: snapshot.meta.shared,
		share_url: snapshot.meta.share_url.clone(),
		project_dir: snapshot.meta.project_dir.clone(),
	}
}

fn strip_thinking_blocks(content: &str) -> String {
	let mut result = String::new();
	let mut in_think = false;
	for line in content.lines() {
		if line.trim() == "<think>" {
			in_think = true;
			continue;
		}
		if line.trim() == "</think>" {
			in_think = false;
			continue;
		}
		if !in_think {
			result.push_str(line);
			result.push('\n');
		}
	}
	result.trim().to_string()
}

fn strip_tool_blocks(content: &str) -> String {
	let mut result = String::new();
	let mut in_tool = false;
	for line in content.lines() {
		if line.trim().starts_with("```") && line.contains("command") {
			in_tool = true;
			continue;
		}
		if in_tool && line.trim() == "```" {
			in_tool = false;
			continue;
		}
		if !in_tool {
			result.push_str(line);
			result.push('\n');
		}
	}
	result.trim().to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn test_session(id: &str) -> (StoredSession, Vec<Message>) {
		let s = StoredSession {
			id: id.to_string(),
			name: format!("Session {}", &id[..4]),
			messages: vec![Message::user("hello".into())],
			model: "test-model".into(),
			model_display: "Test".into(),
			provider: "test".into(),
			agent_mode: AgentMode::Ask,
			runtime_mode: RuntimeMode::Remote,
			created_at: chrono::Local::now(),
			updated_at: chrono::Local::now(),
			shared: false,
			share_url: None,
			project_dir: ".".into(),
		};
		let msgs = vec![Message::user("hello".into()), Message::assistant("hi".into())];
		(s, msgs)
	}

	#[test]
	fn test_save_and_load() {
		let dir = tempfile::tempdir().unwrap();
		let mut db = SessionDatabase::with_dir(dir.path().to_path_buf());
		let (session, messages) = test_session("test-1234");
		db.save_session(&session, &messages).unwrap();
		let (loaded, msgs) = db.load_session("test-1234").unwrap();
		assert_eq!(loaded.id, "test-1234");
		assert_eq!(msgs.len(), 2);
	}

	#[test]
	fn test_list_sessions() {
		let dir = tempfile::tempdir().unwrap();
		let mut db = SessionDatabase::with_dir(dir.path().to_path_buf());
		let (s1, m1) = test_session("abc-1111");
		let (s2, m2) = test_session("abc-2222");
		db.save_session(&s1, &m1).unwrap();
		db.save_session(&s2, &m2).unwrap();
		let list = db.list_sessions();
		assert_eq!(list.len(), 2);
	}

	#[test]
	fn test_find_by_prefix() {
		let dir = tempfile::tempdir().unwrap();
		let mut db = SessionDatabase::with_dir(dir.path().to_path_buf());
		let (s1, m1) = test_session("abcdef12-3456");
		let (s2, m2) = test_session("12345678-90ab");
		db.save_session(&s1, &m1).unwrap();
		db.save_session(&s2, &m2).unwrap();
		let results = db.find_by_prefix("abcd");
		assert_eq!(results.len(), 1);
		assert_eq!(results[0].id, "abcdef12-3456");
	}

	#[test]
	fn test_delete_session() {
		let dir = tempfile::tempdir().unwrap();
		let mut db = SessionDatabase::with_dir(dir.path().to_path_buf());
		let (s, m) = test_session("del-session");
		db.save_session(&s, &m).unwrap();
		db.delete_session("del-session").unwrap();
		assert!(db.list_sessions().is_empty());
		assert!(db.load_session("del-session").is_err());
	}

	#[test]
	fn test_archive_unarchive() {
		let dir = tempfile::tempdir().unwrap();
		let mut db = SessionDatabase::with_dir(dir.path().to_path_buf());
		let (s, m) = test_session("archive-test");
		db.save_session(&s, &m).unwrap();
		db.archive_session("archive-test").unwrap();
		let meta = db.find_by_prefix("archive-test");
		assert!(meta.first().map(|m| m.is_archived).unwrap_or(false));
		db.unarchive_session("archive-test").unwrap();
		let meta = db.find_by_prefix("archive-test");
		assert!(!meta.first().map(|m| m.is_archived).unwrap_or(true));
	}

	#[test]
	fn test_set_tags() {
		let dir = tempfile::tempdir().unwrap();
		let mut db = SessionDatabase::with_dir(dir.path().to_path_buf());
		let (s, m) = test_session("tag-test");
		db.save_session(&s, &m).unwrap();
		db.set_tags("tag-test", vec!["important".into(), "work".into()]).unwrap();
		let (_, _) = db.load_session("tag-test").unwrap();
	}

	#[test]
	fn test_short_session_id() {
		assert_eq!(short_session_id("abcdef12-3456-7890"), "abcdef12");
		assert_eq!(short_session_id("short"), "short");
	}

	#[test]
	fn test_migrate_legacy_empty() {
		let dir = tempfile::tempdir().unwrap();
		let mut db = SessionDatabase::with_dir(dir.path().to_path_buf());
		let count = db.migrate_legacy().unwrap();
		assert_eq!(count, 0);
	}

	#[test]
	fn test_fts_search() {
		let dir = tempfile::tempdir().unwrap();
		let mut db = SessionDatabase::with_dir(dir.path().to_path_buf());
		let (s1, m1) = test_session("sess-1");
		db.save_session(&s1, &m1).unwrap();

		// Content is encrypted at rest — FTS searches encrypted text,
		// so plaintext queries won't match.
		let results = db.search_fts("hello", 10);
		assert!(results.is_empty());
	}
}
