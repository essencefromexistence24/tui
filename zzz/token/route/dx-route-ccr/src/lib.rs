#![allow(missing_docs)]

#![deny(unsafe_code)]
//! dx-route-ccr — reversible compression with content-addressed blob retrieval.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum CcrError {
  #[error("regex error: {0}")]
  Regex(#[from] regex_lite::Error),

  #[error("blob not found: {0}")]
  BlobNotFound(String),
}

/// Result type for CCR operations.
pub type CcrResult<T> = Result<T, CcrError>;

#[allow(missing_docs)]
#[derive(Debug)]
#[must_use]
pub struct CcrOutput {
  pub text: String,
  pub refs_inserted: u32,
  pub original_len: usize,
  pub compressed_len: usize,
}

impl CcrOutput {
  pub fn savings_pct(&self) -> f64 {
    if self.original_len == 0 { return 0.0; }
    (self.original_len - self.compressed_len) as f64 / self.original_len as f64 * 100.0
  }
}

#[derive(Debug, Default)]
pub struct BlobStore {
  blobs: HashMap<String, String>,
}

impl BlobStore {
  pub fn new() -> Self {
    Self { blobs: HashMap::new() }
  }

  pub fn store(&mut self, content: &str) -> String {
    let hash = hash_content(content);
    self.blobs.entry(hash.clone()).or_insert_with(|| content.to_string());
    hash
  }

  pub fn retrieve(&self, hash: &str) -> Option<&str> {
    self.blobs.get(hash).map(|s| s.as_str())
  }

  pub fn len(&self) -> usize { self.blobs.len() }
  pub fn is_empty(&self) -> bool { self.blobs.is_empty() }
}

pub fn compress(text: &str, store: &mut BlobStore) -> CcrOutput {
  let original_len = text.len();
  let mut result = text.to_string();
  let mut refs_inserted = 0u32;

  result = replace_long_blocks(&result, store, &mut refs_inserted);
  result = replace_repeated_blocks(&result, store, &mut refs_inserted);

  let text = result;
  CcrOutput {
    compressed_len: text.len(),
    text, refs_inserted, original_len,
  }
}

pub fn expand(text: &str, store: &BlobStore) -> String {
  let mut result = text.to_string();
  let re = regex_lite::Regex::new(r"§ref:([a-f0-9]{64})§").expect("static regex is valid");
  loop {
    let expanded = re.replace_all(&result, |caps: &regex_lite::Captures| {
      store.retrieve(&caps[1]).map(|s| s.to_string()).unwrap_or(caps[0].to_string())
    }).to_string();
    if expanded == result { break; }
    result = expanded;
  }
  result
}

fn hash_content(content: &str) -> String {
  hex::encode(Sha256::digest(content.as_bytes()))
}

fn replace_long_blocks(text: &str, store: &mut BlobStore, count: &mut u32) -> String {
  let mut result = text.to_string();
  let re = regex_lite::Regex::new(r"(?s)```[\s\S]{500,}?```").expect("static regex is valid");
  result = re.replace_all(&result, |caps: &regex_lite::Captures| {
    *count += 1;
    format!("§ref:{}§", store.store(&caps[0]))
  }).to_string();
  result
}

fn replace_repeated_blocks(text: &str, store: &mut BlobStore, count: &mut u32) -> String {
  let lines: Vec<&str> = text.lines().collect();
  let mut freq: HashMap<&str, usize> = HashMap::new();
  for line in &lines {
    let trimmed = line.trim();
    if trimmed.len() > 80 {
      *freq.entry(trimmed).or_insert(0) += 1;
    }
  }

  let mut result = text.to_string();
  for (line, ct) in &freq {
    if *ct > 3 {
      let hash = store.store(line);
      result = result.replace(line, &format!("§ref:{}§", hash));
      *count += 1;
    }
  }
  result
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn roundtrip_preserves_content() {
    let input = "before\n```\n".to_string() + &"x".repeat(1000) + "\n```\nafter";
    let mut store = BlobStore::new();
    let compressed = compress(&input, &mut store);
    assert_eq!(compressed.refs_inserted, 1);
    assert!(compressed.text.contains("§ref:"));

    let expanded = expand(&compressed.text, &store);
    assert_eq!(expanded, input);
  }

  #[test]
  fn small_blocks_passthrough() {
    let mut store = BlobStore::new();
    let r = compress("```small```", &mut store);
    assert_eq!(r.refs_inserted, 0);
  }

  #[test]
  fn repeated_lines_deduplicated() {
    let long_line = "this line is well over eighty characters long, so it should definitely be deduplicated by the ccr engine!";
    assert!(long_line.len() > 80, "test line must be > 80 chars, got {}", long_line.len());
    let input = std::iter::repeat(long_line).take(5).collect::<Vec<_>>().join("\n");
    let mut store = BlobStore::new();
    let r = compress(&input, &mut store);
    assert!(r.refs_inserted > 0, "expected refs, got {} refs", r.refs_inserted);
    assert!(r.text.contains("§ref:"));
  }

  #[test]
  fn unknown_hash_left_as_ref() {
    let store = BlobStore::new();
    let hash = "a".repeat(64);
    let input = format!("hello §ref:{}§ world", hash);
    let result = expand(&input, &store);
    assert_eq!(result, input);
  }

  #[test]
  fn tracks_savings() {
    let mut store = BlobStore::new();
    let input = "a\n```\n".to_string() + &"y".repeat(600) + "\n```\nb\n```\n" + &"z".repeat(700) + "\n```";
    let r = compress(&input, &mut store);
    assert!(r.savings_pct() >= 0.0);
  }

  #[test]
  fn empty_input_ok() {
    let mut store = BlobStore::new();
    let r = compress("", &mut store);
    assert!(r.text.is_empty());
  }
}
