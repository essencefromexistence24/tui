#![allow(missing_docs)]

#![deny(unsafe_code)]
//! dx-route-aggressive — 3-stage: tool result compression, aging, summarizer.

use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum AggressiveError {
  #[error("regex error: {0}")]
  Regex(#[from] regex_lite::Error),
}

/// Result type for aggressive operations.
pub type AggressiveResult<T> = Result<T, AggressiveError>;

#[allow(missing_docs)]
#[derive(Debug)]
#[must_use]
pub struct AggressiveOutput {
  pub text: String,
  pub stages: Vec<String>,
  pub original_len: usize,
  pub compressed_len: usize,
}

impl AggressiveOutput {
  pub fn savings_pct(&self) -> f64 {
    if self.original_len == 0 {
      return 0.0;
    }
    (self.original_len - self.compressed_len) as f64 / self.original_len as f64 * 100.0
  }
}

/// Compress text using the 3-stage aggressive pipeline.
/// Stages: tool result truncation → age-based degradation → summarizer fallback.
pub fn compress(body: &str, intensity: &str) -> AggressiveResult<AggressiveOutput> {
  let original_len = body.len();
  let mut text = body.to_string();
  let mut stages = Vec::new();

  text = compress_tool_results(&text)?;
  stages.push("tool_result_compress".into());

  text = age_based_degradation(&text, intensity);
  stages.push("age_degradation".into());

    if estimate_savings(body, &text) < 0.05 {
      let (summarized, did_run) = fallback_summarize(&text);
      text = summarized;
      if did_run {
        stages.push("fallback_summarizer".into());
      }
    }

  let compressed_len = text.len();
  tracing::debug!(
    "aggressive: {} stages, {} → {} bytes",
    stages.len(), original_len, compressed_len
  );

  Ok(AggressiveOutput { text, stages, original_len, compressed_len })
}

fn compress_tool_results(text: &str) -> AggressiveResult<String> {
  let mut result = text.to_string();
  let re = regex_lite::Regex::new(r"(?s)(```[\s\S]*?```)")?;

  result = re.replace_all(&result, |caps: &regex_lite::Captures| {
    let block = &caps[1];
    let lines: Vec<&str> = block.lines().collect();
    if lines.len() <= 50 {
      return block.to_string();
    }
    let head: Vec<&str> = lines.iter().take(20).copied().collect();
    let tail: Vec<&str> = lines.iter().skip(lines.len().saturating_sub(20)).copied().collect();
    let mut out = head.join("\n");
    out.push_str(&format!("\n--- [truncated {} lines]\n", lines.len() - 40));
    out.push_str(&tail.join("\n"));
    out
  }).to_string();

  let re_json = regex_lite::Regex::new(r"(?s)\{[^}]{2000,}\}")?;
  result = re_json.replace_all(&result, |caps: &regex_lite::Captures| {
    format!("{{/* [truncated {} byte JSON] */}}", caps[0].len())
  }).to_string();

  Ok(result)
}

fn age_based_degradation(text: &str, intensity: &str) -> String {
  let sections: Vec<&str> = text.split("\n\n").collect();
  if sections.len() <= 3 {
    return text.to_string();
  }

  let threshold = match intensity {
    "aggressive" => 0.3,
    "full" => 0.5,
    _ => 0.7,
  };

  let mid = (sections.len() as f64 * threshold) as usize;
  let mut result: Vec<String> = Vec::new();

  for (i, section) in sections.iter().enumerate() {
    if i < mid {
      result.push(section.to_string());
    } else if i < sections.len() - 2 {
      let words: Vec<&str> = section.split_whitespace().collect();
      if words.len() > 20 {
        let summary: Vec<&str> = words.iter().take(10).copied().collect();
        result.push(format!("{}... [summarized]", summary.join(" ")));
      } else {
        result.push(section.to_string());
      }
    } else {
      result.push(section.to_string());
    }
  }

  result.join("\n\n")
}

fn fallback_summarize(text: &str) -> (String, bool) {
  if text.split_whitespace().count() < 50 {
    return (text.to_string(), false);
  }

  let lines: Vec<&str> = text.lines().collect();
  let mut summary = String::new();

  if let Some(first) = lines.first() {
    summary.push_str(first);
    summary.push('\n');
  }

  let errors: Vec<&&str> = lines.iter()
    .filter(|l| l.to_lowercase().contains("error") || l.to_lowercase().contains("fail")
      || l.contains("panic") || l.contains("exception"))
    .collect();

  if !errors.is_empty() {
    summary.push_str("\nErrors:\n");
    for line in errors.iter().take(10) {
      summary.push_str(line);
      summary.push('\n');
    }
  }

  if let Some(last) = lines.last()
    && !last.trim().is_empty() {
      summary.push_str("\nLast line:\n");
      summary.push_str(last);
      summary.push('\n');
    }

  (summary, true)
}

fn estimate_savings(original: &str, compressed: &str) -> f64 {
  if original.is_empty() { return 0.0; }
  (original.len() - compressed.len()) as f64 / original.len() as f64
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn long_code_block_truncated() {
    let input = "```\n".to_string() + &"line\n".repeat(100) + "```";
    let r = compress_tool_results(&input).unwrap();
    assert!(r.lines().count() < 60, "got {} lines", r.lines().count());
    assert!(r.contains("[truncated"));
  }

  #[test]
  fn short_block_preserved() {
    let input = "```\nfn main() {}\n```";
    assert_eq!(compress_tool_results(input).unwrap(), input);
  }

  #[test]
  fn age_keeps_recent_sections() {
    let input = (0..20).map(|i| format!("section {}", i)).collect::<Vec<_>>().join("\n\n");
    let r = age_based_degradation(&input, "full");
    assert!(r.contains("section 19"), "last section should be preserved");
  }

  #[test]
  fn summarizer_runs_for_long_text() {
    let input = "word ".repeat(100) + "ERROR: critical failure";
    let (result, ran) = fallback_summarize(&input);
    assert!(ran);
    assert!(result.contains("ERROR"));
  }

  #[test]
  fn full_pipeline_produces_savings() {
    let input = "intro\n\n```\n".to_string() + &"data\n".repeat(80) + "```\n\nmiddle\n\nERROR: crash";
    let r = compress(&input, "full").unwrap();
    assert!(r.savings_pct() > 0.0);
    assert!(r.text.contains("ERROR"));
  }

  #[test]
  fn short_text_does_not_summarize() {
    let input = "short text";
    let r = compress(input, "full").unwrap();
    assert!(!r.stages.contains(&"fallback_summarizer".to_string()));
  }

  #[test]
  fn empty_input_ok() {
    let r = compress("", "full").unwrap();
    assert!(r.text.is_empty());
  }
}
