#![allow(missing_docs)]

#![deny(unsafe_code)]
//! dx-route-ultra — token-scoring heuristic compression.

use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum UltraError {
  #[error("regex error: {0}")]
  Regex(#[from] regex_lite::Error),
}

/// Result type for ultra operations.
pub type UltraResult<T> = Result<T, UltraError>;

#[allow(missing_docs)]
#[derive(Debug)]
#[must_use]
pub struct UltraOutput {
  pub text: String,
  pub tier: String,
  pub tokens_removed: usize,
  pub tokens_kept: usize,
  pub original_len: usize,
  pub compressed_len: usize,
}

impl UltraOutput {
  pub fn savings_pct(&self) -> f64 {
    if self.original_len == 0 {
      return 0.0;
    }
    (self.original_len - self.compressed_len) as f64 / self.original_len as f64 * 100.0
  }
}

/// Compress text using token-scoring heuristic (stop words, length, URLs).
/// Supports `standard`, `full`, and `aggressive` intensity levels.
pub fn compress(body: &str, intensity: &str) -> UltraResult<UltraOutput> {
  let original_len = body.len();

  let target_ratio = match intensity {
    "aggressive" => 0.35,
    "full" => 0.50,
    _ => 0.65,
  };

  let tokens: Vec<&str> = body.split_whitespace().collect();
  if tokens.is_empty() {
    return Ok(UltraOutput {
      text: String::new(), tier: "passthrough".into(), tokens_removed: 0,
      tokens_kept: 0, original_len: 0, compressed_len: 0,
    });
  }

  let target_count = (tokens.len() as f64 * target_ratio).max(1.0) as usize;

  if tokens.len() <= target_count {
    return Ok(UltraOutput {
      text: body.to_string(), tier: "passthrough".into(), tokens_removed: 0,
      tokens_kept: tokens.len(), original_len, compressed_len: original_len,
    });
  }

  let scores = score_tokens(&tokens);
  let mut indexed: Vec<(usize, f64)> = scores.iter().copied().enumerate().collect();
  indexed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

  let keep: std::collections::HashSet<usize> =
    indexed.iter().rev().take(target_count).map(|(i, _)| *i).collect();

  let mut result_parts = Vec::with_capacity(target_count);
  let mut removed = 0;
  for (i, token) in tokens.iter().enumerate() {
    if keep.contains(&i) {
      result_parts.push(*token);
    } else {
      removed += 1;
    }
  }

  let text = result_parts.join(" ");
  let compressed_len = text.len();

  tracing::debug!(
    "ultra: {} → {} tokens (ratio={}, tier=heuristic), {} → {} bytes",
    tokens.len(), result_parts.len(), target_ratio, original_len, compressed_len
  );

  Ok(UltraOutput {
    text, tier: "heuristic".into(), tokens_removed: removed,
    tokens_kept: result_parts.len(), original_len, compressed_len,
  })
}

fn is_stop_word(word: &str) -> bool {
  // Pre-sorted list for potential binary search; linear scan is fine for 79 entries
  matches!(
    word,
    "a" | "about" | "all" | "also" | "am" | "an" | "and" | "any" | "are" | "as"
      | "at" | "be" | "been" | "being" | "both" | "but" | "by" | "can" | "could"
      | "did" | "do" | "does" | "each" | "else" | "every" | "few" | "for" | "had"
      | "has" | "have" | "he" | "her" | "here" | "him" | "his" | "how" | "i" | "if"
      | "in" | "is" | "it" | "its" | "just" | "may" | "me" | "might" | "more"
      | "most" | "my" | "no" | "nor" | "not" | "now" | "of" | "off" | "on" | "only"
      | "or" | "other" | "our" | "out" | "over" | "same" | "shall" | "she" | "so"
      | "some" | "such" | "than" | "that" | "the" | "their" | "them" | "then"
      | "there" | "these" | "they" | "this" | "those" | "to" | "too" | "up" | "us"
      | "very" | "was" | "we" | "were" | "what" | "when" | "where" | "which" | "who"
      | "whom" | "why" | "will" | "with" | "would" | "you" | "your"
  )
}

fn score_tokens(tokens: &[&str]) -> Vec<f64> {
  tokens.iter().map(|token| {
    let clean = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/' && c != '-' && c != '_');
    if clean.is_empty() { return 0.0; }
    if is_url_or_path(clean) || clean.bytes().any(|b| b.is_ascii_digit()) { return 1.0; }
    if is_capitalized(clean) { return 0.8; }
    if clean.len() >= 8 { return 0.75; }
    if clean.len() >= 6 { return 0.65; }
    if clean.len() <= 5 && is_stop_word(clean) { return 0.1; }
    // Longer words: check lowercased version
    if is_stop_word(&clean.to_lowercase()) { return 0.1; }
    if clean.len() <= 2 { return 0.2; }
    0.5
  }).collect()
}

fn is_url_or_path(s: &str) -> bool {
  s.starts_with("http://") || s.starts_with("https://") || s.starts_with("ftp://")
    || s.starts_with('/') || s.starts_with("./") || s.starts_with("../")
    || s.ends_with(".com") || s.ends_with(".org")
    || (s.contains('/') && s.contains('.'))
}

fn is_capitalized(s: &str) -> bool {
  s.chars().next().is_some_and(|c| c.is_uppercase())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn retains_errors_and_urls() {
    let input = "the quick brown fox jumped over the lazy dog ERROR: https://example.com/crash failed";
    let r = compress(input, "full").unwrap();
    assert!(r.text.contains("ERROR"), "errors should be kept");
    assert!(r.text.contains("https://example.com/crash"), "URLs should be kept");
  }

  #[test]
  fn removes_stop_words_first() {
    let input = "a the a the and or a the a the but a the";
    let r = compress(input, "full").unwrap();
    assert!(r.text.split_whitespace().count() < input.split_whitespace().count());
  }

  #[test]
  fn short_text_passthrough() {
    let r = compress("short", "aggressive").unwrap();
    assert_eq!(r.tier, "passthrough");
  }

  #[test]
  fn tracks_removed_count() {
    let input = "one two three four five six seven eight nine ten";
    let r = compress(input, "aggressive").unwrap(); // ratio=0.35 → keep ~3
    assert!(r.tokens_removed > 0);
    assert!(r.tokens_kept > 0);
  }

  #[test]
  fn aggressive_more_removal_than_standard() {
    let input = "foo bar baz qux quux corge grault garply waldo fred plugh xyzzy thud".repeat(5);
    let std = compress(&input, "standard").unwrap();
    let agg = compress(&input, "aggressive").unwrap();
    assert!(agg.tokens_removed >= std.tokens_removed);
  }

  #[test]
  fn savings_pct_calculated() {
    let r = compress("hello world foo bar baz", "aggressive").unwrap();
    assert!(r.savings_pct() >= 0.0);
  }

  #[test]
  fn empty_input_ok() {
    let r = compress("", "full").unwrap();
    assert_eq!(r.original_len, 0);
  }
}
