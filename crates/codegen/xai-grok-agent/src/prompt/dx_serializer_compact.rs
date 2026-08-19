//! Dx Serializer Compact: the single compact tool contract sent to the model.

/// Complete built-in tool catalog and its decoding rule.
pub const TOOL_CATALOG: &str = include_str!("../../templates/dx_serializer_compact_tools.md");

/// First line of [`TOOL_CATALOG`]. Used for presence + dedup so a drifted
/// hardcoded marker cannot silently append a second copy.
fn catalog_marker() -> &'static str {
    TOOL_CATALOG
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or("You have 26 tools in Dx Serializer Compact Read each line Use listed keys and types only.")
}

/// Attach the compact tool contract to the canonical system prompt.
///
/// Keeping this in the system item makes the contract survive every provider
/// conversion (Responses, Chat Completions, and Messages). It is deliberately
/// idempotent so rebuilds and model switches cannot duplicate the catalog.
/// Already-duplicated prompts (from the old period-suffixed marker miss) are
/// collapsed back to a single catalog.
pub fn append_to_system_prompt(prompt: &str) -> String {
    if prompt.contains(catalog_marker()) {
        dedup_catalog(prompt)
    } else {
        format!("{prompt}\n\n{TOOL_CATALOG}")
    }
}

/// Keep the first catalog block; drop any later copies of the same header.
fn dedup_catalog(prompt: &str) -> String {
    let marker = catalog_marker();
    let Some(first) = prompt.find(marker) else {
        return prompt.to_owned();
    };
    let after_first = first + marker.len();
    let Some(rel) = prompt[after_first..].find(marker) else {
        return prompt.to_owned();
    };
    let second = after_first + rel;
    prompt[..second].trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::{TOOL_CATALOG, append_to_system_prompt, catalog_marker};

    #[test]
    fn catalog_is_present_and_complete() {
        assert!(TOOL_CATALOG.contains("Dx Serializer Compact"));
        assert!(TOOL_CATALOG.contains(catalog_marker()));
        assert!(TOOL_CATALOG.contains("run_terminal_command"));
        assert!(TOOL_CATALOG.contains("reference_to_video"));
    }

    #[test]
    fn marker_is_template_header_not_a_hardcoded_period() {
        let header = TOOL_CATALOG.lines().next().expect("catalog header");
        assert_eq!(header.trim(), catalog_marker());
        assert!(
            !catalog_marker().ends_with('.'),
            "header is '26 tools Line …', not '26 tools.' — a trailing \
             period made contains() miss and duplicated the catalog"
        );
        assert!(
            catalog_marker().starts_with("Dx Serializer Compact 26 tools"),
            "unexpected catalog header: {}",
            catalog_marker()
        );
    }

    #[test]
    fn system_prompt_catalog_injection_is_idempotent() {
        let once = append_to_system_prompt("base");
        let twice = append_to_system_prompt(&once);
        assert_eq!(once, twice);
        assert_eq!(once.matches(catalog_marker()).count(), 1);
    }

    #[test]
    fn heals_catalog_already_appended_twice() {
        let once = append_to_system_prompt("base");
        let doubled = format!("{once}\n\n{TOOL_CATALOG}");
        assert_eq!(doubled.matches(catalog_marker()).count(), 2);
        let healed = append_to_system_prompt(&doubled);
        assert_eq!(healed, once);
        assert_eq!(healed.matches(catalog_marker()).count(), 1);
    }
}
