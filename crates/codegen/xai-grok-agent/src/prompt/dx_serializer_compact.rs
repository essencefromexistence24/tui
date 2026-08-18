//! Dx Serializer Compact: the single compact tool contract sent to the model.

/// Complete built-in tool catalog and its decoding rule.
pub const TOOL_CATALOG: &str = include_str!("../../templates/dx_serializer_compact_tools.md");
/// Must match the template's header line exactly — the idempotency check
/// depends on it, so the dedup silently breaks when they drift.
const CATALOG_MARKER: &str = "Dx Serializer Compact 26 tools.";

/// Attach the compact tool contract to the canonical system prompt.
///
/// Keeping this in the system item makes the contract survive every provider
/// conversion (Responses, Chat Completions, and Messages). It is deliberately
/// idempotent so rebuilds and model switches cannot duplicate the catalog.
pub fn append_to_system_prompt(prompt: &str) -> String {
    if prompt.contains(CATALOG_MARKER) {
        prompt.to_owned()
    } else {
        format!("{prompt}\n\n{TOOL_CATALOG}")
    }
}

#[cfg(test)]
mod tests {
    use super::{CATALOG_MARKER, TOOL_CATALOG, append_to_system_prompt};

    #[test]
    fn catalog_is_present_and_complete() {
        assert!(TOOL_CATALOG.contains("Dx Serializer Compact"));
        assert!(TOOL_CATALOG.contains("Dx Serializer Compact 26 tools."));
        assert!(TOOL_CATALOG.contains("run_terminal_command"));
        assert!(TOOL_CATALOG.contains("reference_to_video"));
    }

    #[test]
    fn marker_matches_template_header() {
        let header = TOOL_CATALOG.lines().next().expect("catalog header");
        assert_eq!(
            header.trim(),
            CATALOG_MARKER,
            "dedup marker must equal the template's header line or \
             append_to_system_prompt duplicates the catalog"
        );
    }

    #[test]
    fn system_prompt_catalog_injection_is_idempotent() {
        let once = append_to_system_prompt("base");
        let twice = append_to_system_prompt(&once);
        assert_eq!(once, twice);
        assert_eq!(once.matches(CATALOG_MARKER).count(), 1);
    }
}
