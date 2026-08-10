//! Parser utilities for extracting `FINAL(...)` answers from LLM responses.
//!
//! The RLM loop expects the model to return a terminating `FINAL(...)` call.
//! This module keeps the parsing rules narrow and deterministic so downstream
//! integrations can rely on a stable answer boundary.

use regex::Regex;

/// Checks if a response contains a `FINAL(` statement.
pub fn is_final(response: &str) -> bool {
    response.contains("FINAL(")
}

/// Extracts the answer payload from a `FINAL(...)` statement.
///
/// Supported forms:
/// - `FINAL("""multi-line""")`
/// - `FINAL('''multi-line''')`
/// - `FINAL("single line")`
/// - `FINAL('single line')`
pub fn extract_final(response: &str) -> Option<String> {
    let patterns = [
        r#"(?s)FINAL\s*\(\s*"""(.*?)"""\s*\)"#,
        r#"(?s)FINAL\s*\(\s*'''(.*?)'''\s*\)"#,
        r#"FINAL\s*\(\s*"([^"]*)"\s*\)"#,
        r#"FINAL\s*\(\s*'([^']*)'\s*\)"#,
    ];

    patterns
        .iter()
        .filter_map(|pattern| Regex::new(pattern).ok())
        .find_map(|regex| {
            regex
                .captures(response)
                .and_then(|captures| captures.get(1))
                .map(|value| value.as_str().trim().to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_single_line_final() {
        assert_eq!(
            extract_final(r#"FINAL("test answer")"#),
            Some("test answer".to_string())
        );
        assert_eq!(
            extract_final(r#"FINAL('test answer')"#),
            Some("test answer".to_string())
        );
    }

    #[test]
    fn detects_multiline_final() {
        let response = "Thoughts...\nFINAL(\"\"\"Line 1\nLine 2\"\"\")";
        assert_eq!(
            extract_final(response),
            Some("Line 1\nLine 2".to_string())
        );
    }

    #[test]
    fn returns_none_when_not_present() {
        assert_eq!(extract_final("No final here"), None);
    }
}
