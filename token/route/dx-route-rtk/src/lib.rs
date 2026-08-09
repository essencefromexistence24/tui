#![allow(missing_docs)]

#![deny(unsafe_code)]
//! dx-route-rtk — command-aware tool output compression.

use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum RtkError {
  #[error("regex error: {0}")]
  Regex(#[from] regex_lite::Error),
}

/// Result type for RTK operations.
pub type RtkResult<T> = Result<T, RtkError>;

#[allow(missing_docs)]
#[derive(Debug)]
#[must_use]
pub struct RtkOutput {
  pub text: String,
  pub command_type: String,
  pub filters_applied: Vec<String>,
  pub original_len: usize,
  pub compressed_len: usize,
}

impl RtkOutput {
  pub fn savings_pct(&self) -> f64 {
    if self.original_len == 0 {
      return 0.0;
    }
    (self.original_len - self.compressed_len) as f64 / self.original_len as f64 * 100.0
  }
}

/// Compress command output using command-aware RTK filters.
/// Detects command type from the `command` hint and applies matching filters.
pub fn compress(body: &str, command: Option<&str>) -> RtkResult<RtkOutput> {
  {
    let original_len = body.len();
    let command_type = detect_command(command);
    let filters = load_filters(&command_type);

    let mut text = body.to_string();
    let mut applied = Vec::new();

    for filter in &filters {
      let before = text.len();
      text = apply_filter(&text, filter)?;
      if text.len() != before {
        applied.push(filter.name.clone());
      }
    }

    text = deduplicate_lines(&text);
    applied.push("deduplicate".into());
    text = smart_truncate(&text, &command_type);

    let compressed_len = text.len();
    tracing::debug!(
      "rtk: cmd={}, {} filters, {} → {} bytes",
      command_type, applied.len(), original_len, compressed_len
    );

    Ok(RtkOutput { text, command_type, filters_applied: applied, original_len, compressed_len })
  }
}

fn detect_command(command: Option<&str>) -> String {
  command.map(|cmd| cmd.to_lowercase()).map(|lower| {
    if lower.contains("git diff") || lower.contains("git log") || lower.contains("git show") { "git-diff" }
    else if lower.contains("git status") { "git-status" }
    else if lower.contains("cargo test") || lower.contains("cargo nextest") { "test-cargo" }
    else if lower.contains("npm test") || lower.contains("jest") { "test-jest" }
    else if lower.contains("pytest") || lower.contains("python -m pytest") { "test-pytest" }
    else if lower.contains("go test") { "test-go" }
    else if lower.contains("docker build") || lower.contains("docker compose") { "docker-build" }
    else if lower.contains("kubectl") { "kubectl" }
    else if lower.contains("terraform") || lower.contains("tofu") { "terraform-plan" }
    else if lower.contains("npm install") || lower.contains("npm ci") || lower.contains("pnpm install") { "npm-install" }
    else if lower.contains("npm run build") || lower.contains("tsc") { "build-typescript" }
    else if lower.contains("eslint") { "build-eslint" }
    else if lower.contains("cargo build") || lower.contains("cargo check") { "build-cargo" }
    else if lower.contains("curl") || lower.contains("wget") { "curl" }
    else if lower.contains("gh ") { "gh" }
    else if lower.contains("aws ") { "aws" }
    else if lower.contains("gcloud") { "gcloud" }
    else if lower.contains("ssh") { "ssh" }
    else if lower.contains("pip install") || lower.contains("poetry") { "pip" }
    else if lower.contains("make ") { "make" }
    else if lower.contains("systemctl") { "systemctl-status" }
    else if lower.contains("grep ") || lower.contains("rg ") { "shell-grep" }
    else if lower.contains("ls ") || lower.contains("ll ") { "shell-ls" }
    else if lower.contains("ps ") || lower.contains("top ") { "ps" }
    else if lower.contains("df ") || lower.contains("du ") { "df" }
    else if lower.contains("rsync") { "rsync" }
    else { "generic-output" }
  }.to_string()).unwrap_or_else(|| "generic-output".to_string())
}

struct Filter {
  name: String,
  drop_re: Vec<regex_lite::Regex>,
  preserve_re: Vec<regex_lite::Regex>,
  collapse_empty: bool,
}

fn compile_patterns(patterns: &[&'static str]) -> Vec<regex_lite::Regex> {
  patterns.iter().filter_map(|p| regex_lite::Regex::new(p).ok()).collect()
}

fn load_filters(command_type: &str) -> Vec<Filter> {
  match command_type {
    "git-diff" => vec![Filter {
      name: "git-diff".into(),
      drop_re: compile_patterns(&[r"^index [0-9a-f]+\.\.[0-9a-f]+.*$", r"^--- a/", r"^\+\+\+ b/"]),
      preserve_re: compile_patterns(&[r"^diff --git ", r"^@@ ", r"^[+-](?![+-]{2})"]),
      collapse_empty: true,
    }],
    "npm-install" => vec![Filter {
      name: "npm-install".into(),
      drop_re: compile_patterns(&[r"^\s+node_modules/", r"^\s+\[[=# ]+\]", r"^added \d+ package", r"^\d+ packages? are? looking"]),
      preserve_re: compile_patterns(&[r"^ERR!", r"^npm ERR!", r"error", r"failed"]),
      collapse_empty: true,
    }],
    "test-cargo" | "test-jest" | "test-pytest" | "test-go" => vec![Filter {
      name: "test-output".into(),
      drop_re: compile_patterns(&[r"^  (?:✓|✗|·|pass|FAIL)\s", r"^test .* \.\.\. ok$", r"^running \d+ test", r"^test result: ok"]),
      preserve_re: compile_patterns(&[r"^test .* FAILED", r"panicked", r"thread '", r"stack backtrace", r"assertion failed"]),
      collapse_empty: true,
    }],
    "build-typescript" => vec![Filter {
      name: "tsc-output".into(),
      drop_re: compile_patterns(&[r"^\s+at ", r"^\[\d+:\d+:\d+\]"]),
      preserve_re: compile_patterns(&[r"^src/.*\.ts\(\d+,\d+\)", r"error TS", r"warning TS", r"Found \d+ error"]),
      collapse_empty: true,
    }],
    "kubectl" => vec![Filter {
      name: "kubectl-output".into(),
      drop_re: compile_patterns(&[r"^NAME\s+READY\s+STATUS", r"^default\s+", r"^\s+\d+/\d+\s+Running"]),
      preserve_re: compile_patterns(&[r"Error", r"CrashLoop", r"ImagePull", r"Pending", r"Failed"]),
      collapse_empty: true,
    }],
    "docker-build" => vec![Filter {
      name: "docker-build".into(),
      drop_re: compile_patterns(&[r"^Step \d+/\d+ :", r"^ ---> [0-9a-f]+", r"^ ---> Using cache", r"^Successfully (built|tagged)"]),
      preserve_re: compile_patterns(&[r"^ERROR", r"failed", r"error during"]),
      collapse_empty: true,
    }],
    _ => vec![Filter {
      name: "generic-output".into(),
      drop_re: compile_patterns(&[r"^\s+at ", r"^\[\d+:\d+:\d+\]", r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}", r"^\[info\]", r"^\[debug\]"]),
      preserve_re: compile_patterns(&[r"error", r"Error", r"ERROR", r"failed", r"panic", r"Panic", r"warning", r"Warning"]),
      collapse_empty: true,
    }],
  }
}

fn apply_filter(text: &str, filter: &Filter) -> RtkResult<String> {
  let mut result = Vec::new();

  for line in text.lines() {
    let trimmed = line.trim();

    if trimmed.is_empty() && filter.collapse_empty
      && result.last().map(|s: &&str| !s.is_empty()).unwrap_or(true) {
        continue;
      }

    let is_preserved = filter.preserve_re.iter().any(|re| re.is_match(line));

    if is_preserved {
      result.push(line);
      continue;
    }

    let is_dropped = filter.drop_re.iter().any(|re| re.is_match(line));

    if !is_dropped {
      result.push(line);
    }
  }

  Ok(result.join("\n"))
}

fn deduplicate_lines(text: &str) -> String {
  let mut result = Vec::new();
  let mut seen = std::collections::HashSet::new();

  for line in text.lines() {
    let trimmed = line.trim().to_string();
    if trimmed.is_empty() || seen.insert(trimmed) {
      result.push(line);
    }
  }

  result.join("\n")
}

fn smart_truncate(text: &str, command_type: &str) -> String {
  let lines: Vec<&str> = text.lines().collect();
  let max = match command_type {
    "test-jest" | "test-pytest" | "test-cargo" | "test-go" => 80,
    "npm-install" | "pip" => 60,
    "build-typescript" | "build-eslint" => 100,
    "docker-build" | "docker-logs" => 120,
    _ => 150,
  };

  if lines.len() <= max {
    return text.to_string();
  }

  let head = max / 2;
  let tail = max / 2;
  let mut result: Vec<&str> = lines.iter().take(head).copied().collect();
  result.push("--- [truncated --dx-route] ---");
  result.extend(lines.iter().skip(lines.len().saturating_sub(tail)).copied());

  let error_keywords = &["error", "Error", "ERROR", "failed", "panic", "Panic", "segmentation fault"];
  let mut seen: std::collections::HashSet<&str> = result.iter().copied().collect();

  for line in &lines {
    if error_keywords.iter().any(|e| line.contains(e)) && seen.insert(line) {
      result.push(line);
    }
  }

  result.join("\n")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn detects_command() {
    assert_eq!(detect_command(Some("git diff")), "git-diff");
    assert_eq!(detect_command(Some("cargo test")), "test-cargo");
    assert_eq!(detect_command(Some("docker build -t foo .")), "docker-build");
    assert_eq!(detect_command(Some("kubectl get pods")), "kubectl");
    assert_eq!(detect_command(None), "generic-output");
  }

  #[test]
  fn git_diff_removes_index() {
    let input = "diff --git a/a.rs b/a.rs\nindex abc..def 100644\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-foo\n+bar";
    let r = compress(input, Some("git diff")).unwrap();
    assert!(!r.text.contains("index abc"));
    assert!(r.text.contains("@@"));
    assert!(r.text.contains("+bar"));
  }

  #[test]
  fn dedup_removes_duplicates() {
    assert_eq!(deduplicate_lines("a\nb\na\nc"), "a\nb\nc");
  }

  #[test]
  fn truncation_respects_budget() {
    let input = (0..300).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n");
    let result = smart_truncate(&input, "generic-output");
    let count = result.lines().count();
    assert!(count < 200, "should be truncated, got {} lines", count);
    assert!(result.contains("--dx-route]"));
  }

  #[test]
  fn error_lines_preserved_in_truncation() {
    let mut lines: Vec<String> = (0..200).map(|i| format!("normal {}", i)).collect();
    lines.push("ERROR: critical failure".into());
    let input = lines.join("\n");
    let result = smart_truncate(&input, "generic-output");
    assert!(result.contains("ERROR"));
  }

  #[test]
  fn tracks_savings() {
    let input = format!("header\n{}\nfooter", "data\n".repeat(50));
    let r = compress(&input, Some("generic")).unwrap();
    assert!(r.savings_pct() >= 0.0);
  }
}
