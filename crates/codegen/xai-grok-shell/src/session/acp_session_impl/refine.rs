//! `/refine` slash command execution: the Continual Harness layer.
//!
//! The harness is the additive layer over the immutable base system prompt
//! (prompt notes, memory, skills, subagent specs). Every mutation snapshots
//! the prior `HarnessState` into the refinement log, so `/refine rollback <id>`
//! can restore it.
//!
//! Grammar (deterministic, no LLM round-trip in v1):
//!
//! ```text
//! /refine status
//! /refine rollback <id>
//! /refine create <kind> <title>: <content>
//! /refine update <kind> <id>: <content>
//! /refine delete <kind> <id>
//! ```
//!
//! `kind` is one of `prompt | memory | skill | subagent`.

use super::*;
use xai_grok_refine::state::HarnessKind;

const REFINE_USAGE: &str = "Usage: /refine status\n       /refine rollback <id>\n       /refine \
                            create <kind> <title>: <content>\n       /refine update <kind> \
                            <id>: <content>\n       /refine delete <kind> <id>\nkinds: prompt | \
                            memory | skill | subagent";

fn parse_kind(kind: &str) -> Option<HarnessKind> {
    HarnessKind::parse(kind.to_lowercase().as_str())
}

impl SessionActor {
    /// Apply a single structured harness edit from `/refine <instructions>`.
    pub(super) fn execute_refine_run(&self, instructions: &str) -> String {
        let trimmed = instructions.trim();
        let (verb, rest) = trimmed
            .split_once(char::is_whitespace)
            .unwrap_or((trimmed, ""));
        match verb.to_lowercase().as_str() {
            "create" => self.refine_create(rest.trim()),
            "update" => self.refine_update(rest.trim()),
            "delete" => self.refine_delete(rest.trim()),
            _ => REFINE_USAGE.to_string(),
        }
    }

    fn refine_create(&self, rest: &str) -> String {
        let (kind_word, title_and_content) =
            rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let Some(kind) = parse_kind(kind_word) else {
            return REFINE_USAGE.to_string();
        };
        let Some((title, content)) = title_and_content.split_once(':') else {
            return REFINE_USAGE.to_string();
        };
        let title = title.trim();
        let content = content.trim();
        if title.is_empty() || content.is_empty() {
            return REFINE_USAGE.to_string();
        }
        let mut session = self.refine.lock();
        match session.create(kind, title, content, Some(format!("slash:{rest}")), "slash") {
            Ok(entry) => format!("Created harness {kind} '{}' ({})", entry.title, entry.id),
            Err(err) => format!("Could not create harness {kind}: {err}"),
        }
    }

    fn refine_update(&self, rest: &str) -> String {
        let mut parts = rest.splitn(3, char::is_whitespace);
        let kind_word = parts.next().unwrap_or_default();
        let id = parts.next().unwrap_or_default();
        let content = parts.next().unwrap_or_default().trim();
        let Some(kind) = parse_kind(kind_word) else {
            return REFINE_USAGE.to_string();
        };
        if id.is_empty() || content.is_empty() {
            return REFINE_USAGE.to_string();
        }
        let mut session = self.refine.lock();
        match session.update(kind, id, content, "slash") {
            Ok(Some(entry)) => format!("Updated harness {kind} '{}' ({})", entry.title, entry.id),
            Ok(None) => format!("No harness {kind} entry with id '{id}'"),
            Err(err) => format!("Could not update harness {kind}/{id}: {err}"),
        }
    }

    fn refine_delete(&self, rest: &str) -> String {
        let (kind_word, id) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let Some(kind) = parse_kind(kind_word) else {
            return REFINE_USAGE.to_string();
        };
        let id = id.trim();
        if id.is_empty() {
            return REFINE_USAGE.to_string();
        }
        let mut session = self.refine.lock();
        match session.delete(kind, id, "slash") {
            Ok(Some(entry)) => format!("Deleted harness {kind} '{}' ({})", entry.title, entry.id),
            Ok(None) => format!("No harness {kind} entry with id '{id}'"),
            Err(err) => format!("Could not delete harness {kind}/{id}: {err}"),
        }
    }

    /// Render the harness overview + refinement history.
    pub(super) fn execute_refine_status(&self) -> String {
        let session = self.refine.lock();
        let state = session.state();
        let mut lines = vec![format!("Harness status\n{}", state.overview(2_000))];
        let log = session.log();
        lines.push(format!("\nRefinements recorded: {}", log.len()));
        for result in log.recent(5) {
            lines.push(format!(
                "  {} {} {}/{}",
                result.id,
                result.action.as_str(),
                result.kind,
                result.outcome
            ));
        }
        lines.join("\n")
    }

    /// Restore the harness snapshot recorded by a prior refinement.
    pub(super) fn execute_refine_rollback(&self, id: &str) -> String {
        let id = id.trim();
        if id.is_empty() {
            return "Usage: /refine rollback <id> (see /refine status for recorded ids)"
                .to_string();
        }
        let mut session = self.refine.lock();
        match session.rollback(id) {
            Ok(()) => format!("Rolled harness back to pre-refinement snapshot '{id}'"),
            Err(err) => format!("Could not roll back '{id}': {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_parsing_matches_refine_grammar() {
        assert_eq!(parse_kind("memory"), Some(HarnessKind::Memory));
        assert_eq!(parse_kind("SKILL"), Some(HarnessKind::Skill));
        assert_eq!(parse_kind("prompt"), Some(HarnessKind::Prompt));
        assert_eq!(parse_kind("subagent"), Some(HarnessKind::Subagent));
        assert_eq!(parse_kind("bogus"), None);
        assert_eq!(parse_kind(""), None);
    }
}
