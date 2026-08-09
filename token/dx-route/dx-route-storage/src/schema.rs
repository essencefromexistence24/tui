#![allow(missing_docs)]

pub const CREATE_TABLES: &[&str] = &[
  r##"CREATE TABLE IF NOT EXISTS compression_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
  )"##,
  r##"CREATE TABLE IF NOT EXISTS compression_combos (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    pipeline TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
  )"##,
  r##"CREATE TABLE IF NOT EXISTS compression_combo_assignments (
    routing_combo_id TEXT PRIMARY KEY,
    compression_combo_id TEXT NOT NULL REFERENCES compression_combos(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
  )"##,
  r##"CREATE TABLE IF NOT EXISTS compression_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    request_id TEXT NOT NULL,
    original_tokens INTEGER NOT NULL DEFAULT 0,
    compressed_tokens INTEGER NOT NULL DEFAULT 0,
    savings_pct REAL NOT NULL DEFAULT 0.0,
    engine TEXT NOT NULL DEFAULT '',
    mode TEXT NOT NULL DEFAULT '',
    duration_ms REAL NOT NULL DEFAULT 0.0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
  )"##,
  r##"CREATE TABLE IF NOT EXISTS ccr_blobs (
    hash TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    ref_count INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
  )"##,
  r##"CREATE INDEX IF NOT EXISTS idx_compression_stats_created
    ON compression_stats(created_at DESC)"##,
  r##"CREATE INDEX IF NOT EXISTS idx_compression_stats_request
    ON compression_stats(request_id)"##,
  r##"CREATE INDEX IF NOT EXISTS idx_ccr_blobs_ref_count
    ON ccr_blobs(ref_count)"##,
];
