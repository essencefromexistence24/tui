#![allow(missing_docs)]
#![deny(unsafe_code)]
//! dx-route-lite — whitespace, ANSI, comments, and JSON null stripping.

use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum LiteError {
    #[error("regex error: {0}")]
    Regex(#[from] regex_lite::Error),
}

/// Result type for lite compression operations.
pub type LiteResult<T> = Result<T, LiteError>;

#[allow(missing_docs)]
#[derive(Debug)]
#[must_use]
pub struct LiteOutput {
    pub text: String,
    pub techniques: Vec<String>,
    pub original_len: usize,
    pub compressed_len: usize,
}

impl LiteOutput {
    pub fn savings_pct(&self) -> f64 {
        if self.original_len == 0 {
            return 0.0;
        }
        (self.original_len - self.compressed_len) as f64 / self.original_len as f64 * 100.0
    }
}

/// Compress text using lite techniques (whitespace, ANSI, comments, JSON nulls).
pub fn compress(body: &str, intensity: &str) -> LiteResult<LiteOutput> {
    {
        let original_len = body.len();
        let mut text = body.to_string();
        let mut techniques = Vec::new();

        text = collapse_whitespace(&text);
        techniques.push("collapse_whitespace");

        text = strip_ansi(&text);
        techniques.push("strip_ansi");

        if intensity == "full" || intensity == "aggressive" {
            text = remove_comments(&text);
            techniques.push("remove_comments");

            text = strip_json_nulls(&text);
            techniques.push("strip_json_nulls");
        }

        let compressed_len = text.len();
        tracing::debug!(
            "lite: {} techniques applied at intensity={}, {} → {} bytes",
            techniques.len(),
            intensity,
            original_len,
            compressed_len
        );

        Ok(LiteOutput {
            text,
            techniques: techniques.into_iter().map(String::from).collect(),
            original_len,
            compressed_len,
        })
    }
}

fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut newline_count = 0;

    for ch in text.chars() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count <= 2 {
                result.push(ch);
            }
        } else {
            newline_count = 0;
            result.push(ch);
        }
    }

    result
}

fn strip_ansi(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_escape = false;

    for ch in text.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch == 'm' || ch == '~' || ch == '@' || ch == '`' || ch.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else {
            result.push(ch);
        }
    }

    result
}

fn remove_comments(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_line_comment = false;

    for ch in text.chars() {
        if in_line_comment && ch == '\n' {
            in_line_comment = false;
            result.push(ch);
        } else if !in_line_comment && ch == '#' {
            in_line_comment = true;
        } else if !in_line_comment {
            result.push(ch);
        }
    }

    result
}

fn regex_replace(text: &str, pattern: &str, replacement: &str) -> String {
    regex_lite::Regex::new(pattern)
        .map(|re| re.replace_all(text, replacement).to_string())
        .unwrap_or_else(|_| text.to_string())
}

fn strip_json_nulls(text: &str) -> String {
    let result = regex_replace(text, r#""[^"]*"\s*:\s*null\s*,?\s*"#, "");
    let result = regex_replace(&result, r",?\s*null\s*,?", ",");
    let result = regex_replace(&result, r",\s*\}", "}");
    let result = regex_replace(&result, r",\s*\]", "]");
    let result = regex_replace(&result, r"\{\s*,", "{");
    regex_replace(&result, r"\[\s*,", "[")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapse_repeated_newlines() {
        assert_eq!(collapse_whitespace("a\n\n\n\n\nb"), "a\n\nb");
    }

    #[test]
    fn ansi_sequence_removed() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
    }

    #[test]
    fn shell_comment_removed() {
        assert_eq!(remove_comments("keep\n# drop\nstay"), "keep\n\nstay");
    }

    #[test]
    fn json_null_stripped() {
        let input = r#"{"a": null, "b": "keep"}"#;
        let result = strip_json_nulls(input);
        assert!(
            !result.contains("null"),
            "null should be stripped, got: {}",
            result
        );
        assert!(
            result.contains("keep"),
            "keep should remain, got: {}",
            result
        );
    }

    #[test]
    fn full_compress_tracks_savings() {
        let input = "hello\n\n\n  \x1b[31mworld\x1b[0m  ";
        let result = compress(input, "aggressive").unwrap();
        assert!(result.compressed_len < result.original_len);
        assert!(result.savings_pct() > 0.0);
        assert!(!result.text.contains('\x1b'));
    }

    #[test]
    fn intensity_standard_skips_json() {
        let input = r#"{"x": null, "y": "z"}"#;
        let result = compress(input, "standard").unwrap();
        assert!(
            result.text.contains("null"),
            "standard should not strip nulls"
        );
    }

    #[test]
    fn empty_input() {
        let result = compress("", "full").unwrap();
        assert!(result.text.is_empty());
        assert_eq!(result.original_len, 0);
    }
}
