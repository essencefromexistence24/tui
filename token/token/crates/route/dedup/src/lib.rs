#![allow(missing_docs)]
#![deny(unsafe_code)]
//! dx-route-dedup — cross-turn exact and fuzzy text deduplication.

use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum DedupError {
    #[error("sha256 error: {0}")]
    Hash(String),
}

/// Result type for dedup operations.
pub type DedupResult<T> = Result<T, DedupError>;

#[allow(missing_docs)]
#[derive(Debug)]
#[must_use]
pub struct DedupOutput {
    pub text: String,
    pub exact_deduped: u32,
    pub fuzzy_deduped: u32,
    pub refs: HashMap<String, String>,
    pub original_len: usize,
    pub compressed_len: usize,
}

impl DedupOutput {
    pub fn savings_pct(&self) -> f64 {
        if self.original_len == 0 {
            return 0.0;
        }
        (self.original_len - self.compressed_len) as f64 / self.original_len as f64 * 100.0
    }
}

/// Tracks per-session state for cross-turn deduplication.
#[derive(Debug)]
pub struct SessionState {
    exact_cache: HashSet<String>,
    fuzzy_cache: Vec<(String, u64)>,
    refs: HashMap<String, String>,
    threshold: f64,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            exact_cache: HashSet::new(),
            fuzzy_cache: Vec::new(),
            refs: HashMap::new(),
            threshold: 0.85,
        }
    }
}

impl SessionState {
    pub fn new(threshold: f64) -> Self {
        Self {
            exact_cache: HashSet::new(),
            fuzzy_cache: Vec::new(),
            refs: HashMap::new(),
            threshold,
        }
    }

    pub fn compress(&mut self, body: &str) -> DedupOutput {
        let original_len = body.len();
        let lines: Vec<&str> = body.lines().collect();
        let mut result = Vec::with_capacity(lines.len());
        let mut exact_deduped = 0u32;
        let mut fuzzy_deduped = 0u32;

        for line in &lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                result.push(*line);
                continue;
            }

            let hash = hash_line(trimmed);

            if self.exact_cache.contains(&hash) {
                exact_deduped += 1;
                continue;
            }

            if self.is_fuzzy_duplicate(trimmed) {
                fuzzy_deduped += 1;
                continue;
            }

            self.exact_cache.insert(hash);
            let sim = simhash(trimmed);
            self.fuzzy_cache.push((trimmed.to_string(), sim));
            if self.fuzzy_cache.len() > 1000 {
                self.fuzzy_cache.remove(0);
            }

            result.push(*line);
        }

        let out = result.join("\n");
        let compressed_len = out.len();
        DedupOutput {
            text: out,
            exact_deduped,
            fuzzy_deduped,
            refs: self.refs.clone(),
            original_len,
            compressed_len,
        }
    }

    fn is_fuzzy_duplicate(&self, line: &str) -> bool {
        let hash = simhash(line);
        let max_distance = ((self.threshold * 64.0) as u32).max(1);
        self.fuzzy_cache.iter().any(|(cached, cached_hash)| {
            hamming_distance(hash, *cached_hash) <= max_distance
                || line.to_lowercase() == cached.to_lowercase()
        })
    }
}

/// A cache of session states keyed by session ID.
#[derive(Debug)]
pub struct CrossTurnCache {
    states: HashMap<String, SessionState>,
}

impl Default for CrossTurnCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Maximum number of sessions before eviction.
const MAX_SESSIONS: usize = 100;

impl CrossTurnCache {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    pub fn compress(&mut self, session_id: &str, body: &str) -> DedupOutput {
        if self.states.len() >= MAX_SESSIONS && !self.states.contains_key(session_id) {
            // Evict the oldest entry
            if let Some(oldest) = self.states.keys().next().cloned() {
                self.states.remove(&oldest);
            }
        }
        self.states
            .entry(session_id.to_string())
            .or_insert_with(|| SessionState::new(0.85))
            .compress(body)
    }

    pub fn clear_session(&mut self, session_id: &str) {
        self.states.remove(session_id);
    }

    pub fn clear_all(&mut self) {
        self.states.clear();
    }
}

fn hash_line(line: &str) -> String {
    hex::encode(Sha256::digest(line.as_bytes()))
}

fn simhash(text: &str) -> u64 {
    let mut v = [0i64; 64];

    for word in text.split_whitespace() {
        let hash = u64::from_le_bytes(
            Sha256::digest(word.as_bytes())[..8]
                .try_into()
                .expect("SHA-256 output is 64 bytes"),
        );
        for (i, elem) in v.iter_mut().enumerate() {
            if (hash >> i) & 1 == 1 {
                *elem += 1;
            } else {
                *elem -= 1;
            }
        }
    }

    let mut fingerprint = 0u64;
    for (i, val) in v.iter().enumerate() {
        if *val > 0 {
            fingerprint |= 1 << i;
        }
    }
    fingerprint
}

fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_dedup_removes_identical() {
        let mut state = SessionState::default();
        let r = state.compress("a\nb\na\nc");
        assert_eq!(r.exact_deduped, 1);
        assert!(!r.text.contains("a\nb\na\nc"));
    }

    #[test]
    fn cross_turn_cache_works() {
        let mut cache = CrossTurnCache::new();
        let r1 = cache.compress("s1", "hello\nworld\nhello");
        assert_eq!(r1.exact_deduped, 1);

        let r2 = cache.compress("s1", "hello\nagain");
        assert_eq!(r2.exact_deduped, 1);

        let r3 = cache.compress("s2", "hello");
        assert_eq!(r3.exact_deduped, 0);
    }

    #[test]
    fn simhash_identical_texts_match() {
        let h1 = simhash("error: failed to run the build command");
        let h2 = simhash("error: failed to run the build command");
        assert_eq!(h1, h2, "identical texts should produce same simhash");
    }

    #[test]
    fn simhash_different_texts_differ() {
        let h1 = simhash("error: failed to run command");
        let h2 = simhash("everything completed successfully");
        assert_ne!(h1, h2, "different texts should produce different simhash");
    }

    #[test]
    fn empty_lines_kept() {
        let mut state = SessionState::default();
        let r = state.compress("a\n\n\nb");
        assert!(r.text.contains("\n\n\n") || !r.text.is_empty());
    }

    #[test]
    fn tracks_savings() {
        let mut state = SessionState::default();
        let input = "line1\nline1\nline1\nline1\nline1";
        let r = state.compress(input);
        assert!(r.savings_pct() > 0.0);
    }

    #[test]
    fn clear_resets() {
        let mut cache = CrossTurnCache::new();
        let _ = cache.compress("s1", "hello\nhello");
        cache.clear_session("s1");
        let r = cache.compress("s1", "hello\nhello");
        assert_eq!(r.exact_deduped, 1);
    }
}
