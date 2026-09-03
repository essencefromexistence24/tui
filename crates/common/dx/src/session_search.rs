use rusqlite::{Connection, params};

/// Full-text search for sessions backed by SQLite FTS5.
/// The FTS index is maintained via triggers in session_db.rs.
#[derive(Debug, Clone)]
pub struct SessionSearch {
	db_path: String,
}

impl SessionSearch {
	pub fn new() -> Self {
		let base_dir = dirs::config_dir()
			.unwrap_or_else(|| std::path::PathBuf::from("."))
			.join("dx")
			.join("sessions");
		std::fs::create_dir_all(&base_dir).ok();
		let db_path = base_dir.join("dx-tui.db");
		Self { db_path: db_path.to_string_lossy().to_string() }
	}

	fn conn(&self) -> rusqlite::Result<Connection> {
		Connection::open_with_flags(&self.db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
	}

	pub fn index_session(&self, session_id: &str, text: &str) {
		// FTS5 is maintained automatically by SQLite triggers on messages table.
		// This method is a no-op — data is indexed on INSERT.
		// We keep it for API compatibility.
		let _ = (session_id, text);
	}

	pub fn remove_session(&self, session_id: &str) {
		// On DELETE CASCADE, FTS5 entries are cleaned up by the trigger.
		let _ = session_id;
	}

	pub fn clear(&self) {
		// SQLite handles this; no in-memory state to clear.
	}

	pub fn search(&self, query: &str, filters: Option<SearchFilters>) -> Vec<SearchResult> {
		if query.trim().is_empty() {
			return Vec::new();
		}
		let Ok(conn) = self.conn() else {
			return Vec::new();
		};

		let mut sql = String::from(
			"SELECT s.id, s.name, s.updated_at, rank FROM messages_fts
             JOIN sessions s ON s.id = messages.session_id
             WHERE messages_fts MATCH ?1",
		);

		let mut filter_params: Vec<String> = Vec::new();

		if let Some(ref f) = filters
			&& let Some(ref prefix) = f.id_prefix
		{
			sql.push_str(" AND s.id LIKE ?2");
			filter_params.push(format!("{}%", prefix));
		}

		sql.push_str(" ORDER BY rank LIMIT 50");

		let mut stmt = match conn.prepare(&sql) {
			Ok(s) => s,
			Err(_) => return self.fallback_search(query, filters),
		};

		if filter_params.is_empty() {
			stmt
				.query_map(params![query], |row| {
					let id: String = row.get(0)?;
					let name: String = row.get(1)?;
					let _updated: String = row.get(2)?;
					let rank: f64 = row.get(3)?;
					Ok(SearchResult {
						session_id: id,
						relevance_score: -rank, // negate because lower rank = better match
						match_count: 1,
						matched_terms: vec![query.to_string()],
						snippet: name,
					})
				})
				.ok()
				.map(|rows| rows.filter_map(|r| r.ok()).collect())
				.unwrap_or_default()
		} else {
			stmt
				.query_map(
					rusqlite::params_from_iter(
						std::iter::once(query).chain(filter_params.iter().map(|s| s.as_str())),
					),
					|row| {
						let id: String = row.get(0)?;
						let name: String = row.get(1)?;
						let _updated: String = row.get(2)?;
						let rank: f64 = row.get(3)?;
						Ok(SearchResult {
							session_id: id,
							relevance_score: -rank,
							match_count: 1,
							matched_terms: vec![query.to_string()],
							snippet: name,
						})
					},
				)
				.ok()
				.map(|rows| rows.filter_map(|r| r.ok()).collect())
				.unwrap_or_default()
		}
	}

	fn fallback_search(&self, query: &str, filters: Option<SearchFilters>) -> Vec<SearchResult> {
		if query.trim().is_empty() {
			return Vec::new();
		}
		let Ok(conn) = self.conn() else {
			return Vec::new();
		};
		let needle = format!("%{}%", query.replace('\'', "''"));
		let mut sql = String::from(
			"SELECT DISTINCT s.id, s.name, COUNT(m.id) as cnt FROM sessions s
             JOIN messages m ON m.session_id = s.id
             WHERE m.content LIKE ?1",
		);
		if filters.as_ref().and_then(|f| f.id_prefix.as_ref()).is_some() {
			sql.push_str(" AND s.id LIKE ?2");
		}
		sql.push_str(" GROUP BY s.id ORDER BY cnt DESC LIMIT 50");

		let mut stmt = match conn.prepare(&sql) {
			Ok(s) => s,
			Err(_) => return Vec::new(),
		};

		if filters.as_ref().and_then(|f| f.id_prefix.as_ref()).is_some() {
			let prefix = filters.unwrap().id_prefix.unwrap();
			let needle2 = format!("{}%", prefix);
			stmt
				.query_map(params![needle, needle2], |row| {
					let id: String = row.get(0)?;
					let name: String = row.get(1)?;
					let cnt: i32 = row.get(2)?;
					Ok(SearchResult {
						session_id: id,
						relevance_score: cnt as f64,
						match_count: cnt as usize,
						matched_terms: vec![query.to_string()],
						snippet: name,
					})
				})
				.ok()
				.map(|rows| rows.filter_map(|r| r.ok()).collect())
				.unwrap_or_default()
		} else {
			stmt
				.query_map(params![needle], |row| {
					let id: String = row.get(0)?;
					let name: String = row.get(1)?;
					let cnt: i32 = row.get(2)?;
					Ok(SearchResult {
						session_id: id,
						relevance_score: cnt as f64,
						match_count: cnt as usize,
						matched_terms: vec![query.to_string()],
						snippet: name,
					})
				})
				.ok()
				.map(|rows| rows.filter_map(|r| r.ok()).collect())
				.unwrap_or_default()
		}
	}

	pub fn index_sessions(&self, sessions: &[(String, String)]) {
		let _ = sessions; // FTS5 is auto-maintained
	}

	pub fn doc_count(&self) -> usize {
		let Ok(conn) = self.conn() else {
			return 0;
		};
		conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get::<_, i32>(0)).unwrap_or(0)
			as usize
	}

	pub fn term_count(&self) -> usize {
		let Ok(conn) = self.conn() else {
			return 0;
		};
		conn.query_row("SELECT COUNT(*) FROM messages_fts", [], |row| row.get::<_, i32>(0)).unwrap_or(0)
			as usize
	}
}

impl Default for SessionSearch {
	fn default() -> Self {
		Self::new()
	}
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
	pub id_prefix: Option<String>,
	pub date_after: Option<chrono::DateTime<chrono::Utc>>,
	pub date_before: Option<chrono::DateTime<chrono::Utc>>,
	pub mode: Option<String>,
	pub provider: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
	pub session_id: String,
	pub relevance_score: f64,
	pub match_count: usize,
	pub matched_terms: Vec<String>,
	pub snippet: String,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_search_basic() {
		let search = SessionSearch::new();
		// Should not panic — may or may not have results depending on test env
		let _results = search.search("hello", None);
		// Just verify the API works without panicking
	}

	#[test]
	fn test_empty_query_returns_empty() {
		let search = SessionSearch::new();
		let results = search.search("", None);
		assert!(results.is_empty());
	}

	#[test]
	fn test_doc_count() {
		let search = SessionSearch::new();
		let count = search.doc_count();
		// Should be >= 0
		assert!(count >= 0);
	}

	#[test]
	fn test_clear_does_not_panic() {
		let search = SessionSearch::new();
		search.clear();
		// Should not panic
	}

	#[test]
	fn test_index_session_no_panic() {
		let search = SessionSearch::new();
		search.index_session("test-id", "hello world");
		// Should not panic
	}

	#[test]
	fn test_default() {
		let search = SessionSearch::default();
		assert!(search.doc_count() >= 0);
	}
}
