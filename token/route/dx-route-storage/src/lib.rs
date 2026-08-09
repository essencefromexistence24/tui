#![allow(missing_docs)]

#![deny(unsafe_code)]
//! dx-route-storage — SQLite persistence for settings, combos, stats, and blobs.

pub mod models;
pub mod schema;

use models::*;
use rusqlite::{params, Connection};
use std::sync::Mutex;
use tracing::{info, warn};

fn lock_conn(lock: &Mutex<Connection>) -> std::sync::MutexGuard<'_, Connection> {
  lock.lock().unwrap_or_else(|e| {
    warn!("database mutex was poisoned, recovering connection");
    e.into_inner()
  })
}

/// SQLite-backed storage for dx-route compression data.
#[derive(Debug)]
pub struct Store {
  conn: Mutex<Connection>,
}

impl Store {
  pub fn open(path: &str) -> anyhow::Result<Self> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL")?;
    conn.execute_batch("PRAGMA busy_timeout=5000")?;

    for stmt in schema::CREATE_TABLES {
      conn.execute_batch(stmt)?;
    }

    info!("Storage opened at {}", path);
    Ok(Self { conn: Mutex::new(conn) })
  }

  pub fn open_in_memory() -> anyhow::Result<Self> {
    let conn = Connection::open_in_memory()?;
    for stmt in schema::CREATE_TABLES {
      conn.execute_batch(stmt)?;
    }
    Ok(Self { conn: Mutex::new(conn) })
  }

  pub fn get_setting(&self, key: &str) -> anyhow::Result<Option<String>> {
    let conn = lock_conn(&self.conn);
    let mut stmt = conn.prepare("SELECT value FROM compression_settings WHERE key = ?1")?;
    let result = stmt.query_row(params![key], |row| row.get::<_, String>(0)).ok();
    Ok(result)
  }

  pub fn set_setting(&self, key: &str, value: &str) -> anyhow::Result<()> {
    let conn = lock_conn(&self.conn);
    conn.execute(
      "INSERT INTO compression_settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
       ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = datetime('now')",
      params![key, value],
    )?;
    Ok(())
  }

  pub fn list_combos(&self) -> anyhow::Result<Vec<CompressionCombo>> {
    let conn = lock_conn(&self.conn);
    let mut stmt = conn.prepare(
      "SELECT id, name, description, pipeline, is_default, created_at, updated_at
       FROM compression_combos ORDER BY name",
    )?;

    let combos = stmt.query_map([], |row| {
      Ok(CompressionCombo {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        pipeline: row.get(3)?,
        is_default: row.get::<_, i32>(4)? != 0,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
      })
    })?
    .filter_map(|r| r.ok())
    .collect();

    Ok(combos)
  }

  pub fn upsert_combo(&self, combo: &CompressionCombo) -> anyhow::Result<()> {
    let conn = lock_conn(&self.conn);
    conn.execute(
      "INSERT INTO compression_combos (id, name, description, pipeline, is_default, updated_at)
       VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
       ON CONFLICT(id) DO UPDATE SET
         name = ?2, description = ?3, pipeline = ?4, is_default = ?5,
         updated_at = datetime('now')",
      params![combo.id, combo.name, combo.description, combo.pipeline, combo.is_default as i32],
    )?;
    Ok(())
  }

  pub fn delete_combo(&self, id: &str) -> anyhow::Result<()> {
    let conn = lock_conn(&self.conn);
    conn.execute_batch("BEGIN TRANSACTION")?;
    conn.execute("DELETE FROM compression_combos WHERE id = ?1", params![id])?;
    conn.execute(
      "DELETE FROM compression_combo_assignments WHERE compression_combo_id = ?1",
      params![id],
    )?;
    conn.execute_batch("COMMIT")?;
    Ok(())
  }

  pub fn assign_combo(&self, routing_combo_id: &str, compression_combo_id: &str) -> anyhow::Result<()> {
    let conn = lock_conn(&self.conn);
    conn.execute(
      "INSERT INTO compression_combo_assignments (routing_combo_id, compression_combo_id, created_at)
       VALUES (?1, ?2, datetime('now'))
       ON CONFLICT(routing_combo_id) DO UPDATE SET compression_combo_id = ?2",
      params![routing_combo_id, compression_combo_id],
    )?;
    Ok(())
  }

  pub fn get_assignment(&self, routing_combo_id: &str) -> anyhow::Result<Option<String>> {
    let conn = lock_conn(&self.conn);
    let mut stmt = conn.prepare(
      "SELECT compression_combo_id FROM compression_combo_assignments WHERE routing_combo_id = ?1",
    )?;
    let result = stmt.query_row(params![routing_combo_id], |row| row.get::<_, String>(0)).ok();
    Ok(result)
  }

  pub fn record_stat(&self, stat: &CompressionStat) -> anyhow::Result<()> {
    let conn = lock_conn(&self.conn);
    conn.execute(
      "INSERT INTO compression_stats (request_id, original_tokens, compressed_tokens, savings_pct, engine, mode, duration_ms)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
      params![
        stat.request_id,
        stat.original_tokens,
        stat.compressed_tokens,
        stat.savings_pct,
        stat.engine,
        stat.mode,
        stat.duration_ms,
      ],
    )?;
    Ok(())
  }

  pub fn get_dashboard_stats(&self) -> anyhow::Result<DashboardStats> {
    let conn = lock_conn(&self.conn);

    let total_requests: i64 = conn.query_row(
      "SELECT COUNT(*) FROM compression_stats",
      [],
      |row| row.get(0),
    )?;

    let total_tokens_saved: i64 = conn.query_row(
      "SELECT COALESCE(SUM(original_tokens - compressed_tokens), 0) FROM compression_stats",
      [],
      |row| row.get(0),
    )?;

    let avg_savings_pct: f64 = conn.query_row(
      "SELECT COALESCE(AVG(savings_pct), 0.0) FROM compression_stats",
      [],
      |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
      "SELECT engine, COUNT(*) as count, COALESCE(SUM(original_tokens - compressed_tokens), 0) as saved
       FROM compression_stats GROUP BY engine ORDER BY saved DESC",
    )?;

    let engine_breakdown: Vec<EngineBreakdown> = stmt
      .query_map([], |row| {
        Ok(EngineBreakdown {
          engine: row.get(0)?,
          count: row.get(1)?,
          tokens_saved: row.get(2)?,
        })
      })?
      .filter_map(|r| r.ok())
      .collect();

    Ok(DashboardStats { total_requests, total_tokens_saved, avg_savings_pct, engine_breakdown })
  }

  pub fn store_blob(&self, hash: &str, content: &str) -> anyhow::Result<()> {
    let conn = lock_conn(&self.conn);
    conn.execute(
      "INSERT INTO ccr_blobs (hash, content, size, ref_count) VALUES (?1, ?2, ?3, 1)
       ON CONFLICT(hash) DO UPDATE SET ref_count = ref_count + 1",
      params![hash, content, content.len() as i32],
    )?;
    Ok(())
  }

  pub fn get_blob(&self, hash: &str) -> anyhow::Result<Option<String>> {
    let conn = lock_conn(&self.conn);
    let mut stmt = conn.prepare("SELECT content FROM ccr_blobs WHERE hash = ?1")?;
    let result = stmt.query_row(params![hash], |row| row.get::<_, String>(0)).ok();
    Ok(result)
  }

  #[cfg(test)]
  pub fn clear_all(&self) -> anyhow::Result<()> {
    let conn = lock_conn(&self.conn);
    conn.execute_batch("DELETE FROM compression_stats; DELETE FROM compression_combo_assignments; DELETE FROM compression_combos; DELETE FROM compression_settings; DELETE FROM ccr_blobs")?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_tables() {
    let store = Store::open_in_memory().unwrap();
    let result = store.get_setting("test");
    assert!(result.is_ok());
  }

  #[test]
  fn set_and_get_setting() {
    let store = Store::open_in_memory().unwrap();
    store.set_setting("mode", "aggressive").unwrap();
    assert_eq!(store.get_setting("mode").unwrap(), Some("aggressive".to_string()));
  }

  #[test]
  fn combo_crud() {
    let store = Store::open_in_memory().unwrap();
    let combo = CompressionCombo {
      id: "test".into(),
      name: "Test Combo".into(),
      description: "A test".into(),
      pipeline: r#"[{"engine":"rtk","intensity":"standard"}]"#.into(),
      is_default: true,
      created_at: None,
      updated_at: None,
    };
    store.upsert_combo(&combo).unwrap();
    let combos = store.list_combos().unwrap();
    assert_eq!(combos.len(), 1);
    assert_eq!(combos[0].name, "Test Combo");
  }

  #[test]
  fn record_and_query_stats() {
    let store = Store::open_in_memory().unwrap();
    let stat = CompressionStat {
      id: None,
      request_id: "req-1".into(),
      original_tokens: 1000,
      compressed_tokens: 500,
      savings_pct: 50.0,
      engine: "lite".into(),
      mode: "lite".into(),
      duration_ms: 5.0,
      created_at: None,
    };
    store.record_stat(&stat).unwrap();
    let dashboard = store.get_dashboard_stats().unwrap();
    assert_eq!(dashboard.total_requests, 1);
    assert_eq!(dashboard.total_tokens_saved, 500);
  }

  #[test]
  fn blob_store_and_retrieve() {
    let store = Store::open_in_memory().unwrap();
    store.store_blob("abc123", "large content here").unwrap();
    let result = store.get_blob("abc123").unwrap();
    assert_eq!(result, Some("large content here".to_string()));
  }
}
