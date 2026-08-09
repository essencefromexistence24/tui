//! The "continual harness" doctrine shown to the model, plus prompt builders.

use crate::state::HarnessState;

/// Maximum chars of harness state rendered into the system prompt.
pub const HARNESS_OVERVIEW_MAX_CHARS: usize = 8_000;

/// Builds the continual-harness doctrine paragraph for a system prompt.
///
/// The base system prompt remains immutable; the harness layer described here
/// is purely additive and edited only through the `harness.*` CRUD surface.
pub fn build_doctrine() -> String {
    [
        "Continual harness state is available as `harness` and `harness.overview()`.",
        "CRUD calls are local to this DX session by default:",
        "`harness.create_prompt_note(title, content, evidence?)`,",
        "`harness.create_memory(...)`, `harness.create_skill(...)`, `harness.create_subagent(...)`,",
        "plus `harness.update_(...)`, `harness.delete_(...)`, `harness.list(kind)`, `harness.get(kind, id)`,",
        "and `harness.record_refinement(...)`.",
        "",
        "`refine.run(instructions?)` reviews the recent trajectory and applies the smallest",
        "evidence-backed CRUD edit that improves the harness toward better outcomes.",
        "`refine.rollback(id)` restores the harness snapshot captured before that refinement.",
        "",
        "The base system prompt is immutable. Only the harness layer may be edited.",
    ]
    .join("\n")
}

/// Renders the current harness state for prompt inclusion (bounded).
pub fn build_harness_section(state: &HarnessState) -> String {
    let overview = state.overview(HARNESS_OVERVIEW_MAX_CHARS);
    if overview.is_empty() {
        "# Continual harness\n(empty)\n".to_string()
    } else {
        format!("# Continual harness\n{overview}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HarnessKind;

    #[test]
    fn doctrine_covers_surface() {
        let d = build_doctrine();
        assert!(d.contains("harness.create_memory"));
        assert!(d.contains("refine.rollback"));
        assert!(d.contains("immutable"));
    }

    #[test]
    fn section_renders_empty_state() {
        let s = HarnessState::new();
        assert!(build_harness_section(&s).contains("(empty)"));
    }

    #[test]
    fn section_renders_entries() {
        let mut s = HarnessState::new();
        s.create(HarnessKind::Memory, "lesson", "compile before push", None);
        let out = build_harness_section(&s);
        assert!(out.contains("compile before push"));
    }
}
