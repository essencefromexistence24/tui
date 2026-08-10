#![allow(missing_docs)]

#![deny(unsafe_code)]
//! dx-route-headroom — TOON JSON array compaction.

use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum HeadroomError {
  #[error("regex error: {0}")]
  Regex(#[from] regex_lite::Error),

  #[error("serde json error: {0}")]
  Serde(#[from] serde_json::Error),
}

/// Result type for headroom operations.
pub type HeadroomResult<T> = Result<T, HeadroomError>;

#[allow(missing_docs)]
#[derive(Debug)]
#[must_use]
pub struct HeadroomOutput {
  pub text: String,
  pub arrays_compacted: u32,
  pub format: String,
  pub original_len: usize,
  pub compressed_len: usize,
}

impl HeadroomOutput {
  pub fn savings_pct(&self) -> f64 {
    if self.original_len == 0 {
      return 0.0;
    }
    (self.original_len - self.compressed_len) as f64 / self.original_len as f64 * 100.0
  }
}

/// Compress JSON arrays into compact TOON/tabular format.
pub fn compress(body: &str, _intensity: &str) -> HeadroomResult<HeadroomOutput> {
  {
    let original_len = body.len();
    let mut compacted = 0;
    let mut text = body.to_string();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
      && let Some(compacted_text) = compact_value(&value) {
        text = compacted_text;
        compacted += 1;
      }

    text = compact_inline_arrays(&text, &mut compacted)?;

    let compressed_len = text.len();
    tracing::debug!(
      "headroom: {} arrays compacted, {} → {} bytes",
      compacted, original_len, compressed_len
    );

    Ok(HeadroomOutput {
      text, arrays_compacted: compacted,
      format: if compacted > 0 { "toon".into() } else { "passthrough".into() },
      original_len, compressed_len,
    })
  }
}

fn compact_value(value: &serde_json::Value) -> Option<String> {
  match value {
    serde_json::Value::Array(arr) => {
      if arr.len() < 3 { return None; }
      if all_objects(arr) { compact_object_array(arr) }
      else if all_strings(arr) { compact_string_array(arr) }
      else { None }
    }
    serde_json::Value::Object(map) => {
      let mut modified = serde_json::Map::new();
      let mut changed = false;
      for (k, v) in map {
        if let Some(c) = compact_value(v)
          && let Ok(new_v) = serde_json::from_str(&c) {
            modified.insert(k.clone(), new_v);
            changed = true;
            continue;
          }
        modified.insert(k.clone(), v.clone());
      }
      changed.then(|| serde_json::Value::Object(modified).to_string())
    }
    _ => None,
  }
}

fn all_objects(arr: &[serde_json::Value]) -> bool {
  arr.iter().all(|v| v.is_object())
}

fn all_strings(arr: &[serde_json::Value]) -> bool {
  arr.iter().all(|v| v.is_string())
}

fn compact_object_array(arr: &[serde_json::Value]) -> Option<String> {
  let keys: Vec<String> = arr.iter()
    .filter_map(|v| v.as_object())
    .flat_map(|obj| obj.keys().cloned())
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter().collect();

  if keys.len() > 8 || keys.is_empty() { return None; }

  let mut columns = Vec::new();
  for key in &keys {
    let values: Vec<String> = arr.iter().map(|v| {
      v.as_object().and_then(|obj| obj.get(key))
        .map(|val| match val {
          serde_json::Value::String(s) => s.clone(),
          other => other.to_string(),
        }).unwrap_or_default()
    }).collect();

    let sep = if values.iter().any(|v| v.contains(',')) { "|" } else { "," };
    columns.push(format!("{}: {}", key, values.join(sep)));
  }

  Some(format!("--- omni-tabular ({} rows x {} cols) ---\n{}", arr.len(), keys.len(), columns.join("\n")))
}

fn compact_string_array(arr: &[serde_json::Value]) -> Option<String> {
  let strs: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
  if strs.len() != arr.len() { return None; }
  let avg = strs.iter().map(|s| s.len()).sum::<usize>() / strs.len().max(1);
  if avg < 10 { return None; }
  Some(format!("--- omni-tabular ({} strings) ---\n{}", strs.len(), strs.join("\n")))
}

fn compact_inline_arrays(text: &str, count: &mut u32) -> HeadroomResult<String> {
  let mut result = text.to_string();
  let re = regex_lite::Regex::new(r#"\[(\s*"[^"]*"\s*)(?:,\s*"[^"]*"\s*){4,}\]"#)?;
  result = re.replace_all(&result, |caps: &regex_lite::Captures| {
    *count += 1;
    format!("--- omni-tabular (inline {} items) ---", caps[0].matches(',').count() + 1)
  }).to_string();
  Ok(result)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn compacts_object_array_to_tabular() {
    let input = r#"[
      {"name": "alice", "age": 30},
      {"name": "bob", "age": 25},
      {"name": "carol", "age": 35}
    ]"#;
    let r = compress(input, "full").unwrap();
    assert!(r.text.contains("omni-tabular"), "got: {}", r.text);
    assert!(r.text.contains("alice,bob,carol"));
    assert_eq!(r.arrays_compacted, 1);
  }

  #[test]
  fn small_array_passthrough() {
    let input = r#"[1, 2]"#;
    let r = compress(input, "full").unwrap();
    assert_eq!(r.format, "passthrough");
  }

  #[test]
  fn non_json_passthrough() {
    let input = "just plain text";
    let r = compress(input, "full").unwrap();
    assert_eq!(r.text, input);
  }

  #[test]
  fn tracks_savings() {
    let input = r#"[
      {"a": "long value here", "b": "another long one"},
      {"a": "second entry val", "b": "more data here"},
      {"a": "third one here", "b": "last entry val"}
    ]"#;
    let r = compress(input, "full").unwrap();
    assert_eq!(r.format, "toon");
    assert!(r.savings_pct() > 0.0);
  }

  #[test]
  fn empty_input_ok() {
    let r = compress("", "full").unwrap();
    assert!(r.text.is_empty());
  }
}
