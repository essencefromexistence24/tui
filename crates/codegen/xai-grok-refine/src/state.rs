//! Continual Harness state: the four editable collections (prompt notes,
//! memory, skills, subagent specs) plus typed CRUD.
//!
//! The base system prompt is immutable. `HarnessState` is the additive layer
//! the model may refine at runtime; every mutation is recorded in the
//! [`crate::log::RefinementLog`] so a bad edit can be rolled back.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// The four harness collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HarnessKind {
    /// Extra standing instructions layered on top of the immutable system prompt.
    Prompt,
    /// Persistent lessons and facts.
    Memory,
    /// Reusable capability descriptions (SKILL.md-style references).
    Skill,
    /// Reusable delegation specs for child agents.
    Subagent,
}

impl fmt::Display for HarnessKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl HarnessKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HarnessKind::Prompt => "prompt",
            HarnessKind::Memory => "memory",
            HarnessKind::Skill => "skill",
            HarnessKind::Subagent => "subagent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "prompt" => Some(HarnessKind::Prompt),
            "memory" => Some(HarnessKind::Memory),
            "skill" => Some(HarnessKind::Skill),
            "subagent" => Some(HarnessKind::Subagent),
            _ => None,
        }
    }
}

/// A single harness entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessEntry {
    pub id: String,
    pub kind: HarnessKind,
    pub title: String,
    pub content: String,
    /// The trajectory fragment that justified this entry (evidence-backed).
    pub evidence: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// The full editable harness layer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HarnessState {
    prompt_notes: BTreeMap<String, HarnessEntry>,
    memories: BTreeMap<String, HarnessEntry>,
    skills: BTreeMap<String, HarnessEntry>,
    subagents: BTreeMap<String, HarnessEntry>,
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl HarnessState {
    pub fn new() -> Self {
        Self::default()
    }

    fn entries_mut(&mut self, kind: HarnessKind) -> &mut BTreeMap<String, HarnessEntry> {
        match kind {
            HarnessKind::Prompt => &mut self.prompt_notes,
            HarnessKind::Memory => &mut self.memories,
            HarnessKind::Skill => &mut self.skills,
            HarnessKind::Subagent => &mut self.subagents,
        }
    }

    fn entries(&self, kind: HarnessKind) -> &BTreeMap<String, HarnessEntry> {
        match kind {
            HarnessKind::Prompt => &self.prompt_notes,
            HarnessKind::Memory => &self.memories,
            HarnessKind::Skill => &self.skills,
            HarnessKind::Subagent => &self.subagents,
        }
    }

    pub fn create(
        &mut self,
        kind: HarnessKind,
        title: impl Into<String>,
        content: impl Into<String>,
        evidence: Option<String>,
    ) -> HarnessEntry {
        let title = title.into();
        let id = slugify(&title, 48);
        let now = now_iso();
        let entry = HarnessEntry {
            id: id.clone(),
            kind,
            title,
            content: content.into(),
            evidence,
            created_at: now.clone(),
            updated_at: now,
        };
        self.entries_mut(kind).insert(id, entry.clone());
        entry
    }

    pub fn update(
        &mut self,
        kind: HarnessKind,
        id: &str,
        content: impl Into<String>,
    ) -> Option<HarnessEntry> {
        let entries = self.entries_mut(kind);
        let entry = entries.get_mut(id)?;
        entry.content = content.into();
        entry.updated_at = now_iso();
        Some(entry.clone())
    }

    pub fn delete(&mut self, kind: HarnessKind, id: &str) -> Option<HarnessEntry> {
        self.entries_mut(kind).remove(id)
    }

    pub fn get(&self, kind: HarnessKind, id: &str) -> Option<&HarnessEntry> {
        self.entries(kind).get(id)
    }

    pub fn list(&self, kind: HarnessKind) -> Vec<HarnessEntry> {
        self.entries(kind).values().cloned().collect()
    }

    pub fn all_entries(&self) -> Vec<HarnessEntry> {
        let mut out = self.list(HarnessKind::Prompt);
        out.extend(self.list(HarnessKind::Memory));
        out.extend(self.list(HarnessKind::Skill));
        out.extend(self.list(HarnessKind::Subagent));
        out
    }

    pub fn len(&self) -> usize {
        self.all_entries().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Compact, bounded prompt dump used to expose the harness to the model.
    pub fn overview(&self, max_chars: usize) -> String {
        let mut out = String::new();
        for kind in [
            HarnessKind::Prompt,
            HarnessKind::Memory,
            HarnessKind::Skill,
            HarnessKind::Subagent,
        ] {
            let entries = self.list(kind);
            if entries.is_empty() {
                continue;
            }
            out.push_str(&format!("# {}\n", kind.as_str()));
            for e in entries {
                out.push_str(&format!("- [{}] {}\n", e.id, e.content));
                if let Some(ev) = &e.evidence {
                    out.push_str(&format!("  evidence: {}\n", truncate(ev, 200)));
                }
            }
        }
        truncate(&out, max_chars)
    }
}

/// Stable slug id, matching `xai_grok_memory` conventions.
pub fn slugify(input: &str, max_len: usize) -> String {
    let mut slug: String = input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else if c.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect();
    if slug.len() > max_len {
        slug.truncate(max_len);
    }
    slug = slug.trim_end_matches(['-', '_']).to_string();
    if slug.is_empty() {
        "entry".to_string()
    } else {
        slug
    }
}

pub fn truncate(input: &str, max_chars: usize) -> String {
    if input.len() <= max_chars {
        input.to_string()
    } else {
        let mut out: String = input.chars().take(max_chars).collect();
        out.push_str("...");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> HarnessState {
        HarnessState::new()
    }

    #[test]
    fn create_read_update_delete_roundtrip() {
        let mut s = state();
        let e = s.create(HarnessKind::Memory, "flaky tests", "retry 3x", None);
        assert!(e.id.contains("flaky-tests"));
        assert_eq!(s.get(HarnessKind::Memory, &e.id).unwrap().content, "retry 3x");

        let u = s.update(HarnessKind::Memory, &e.id, "retry 3x then skip").unwrap();
        assert_eq!(u.content, "retry 3x then skip");
        assert!(u.updated_at >= e.created_at);

        let d = s.delete(HarnessKind::Memory, &e.id).unwrap();
        assert_eq!(d.id, e.id);
        assert!(s.get(HarnessKind::Memory, &e.id).is_none());
    }

    #[test]
    fn kinds_are_isolated() {
        let mut s = state();
        s.create(HarnessKind::Skill, "retry helper", "retry_helper()", None);
        assert!(s.list(HarnessKind::Memory).is_empty());
        assert_eq!(s.list(HarnessKind::Skill).len(), 1);
    }

    #[test]
    fn overview_is_bounded_and_ordered() {
        let mut s = state();
        s.create(HarnessKind::Prompt, "style", "always run cargo fmt", None);
        s.create(HarnessKind::Memory, "lesson", "compile before push", Some("traj-42".to_string()));
        let ov = s.overview(10_000);
        assert!(ov.contains("prompt"));
        assert!(ov.contains("always run cargo fmt"));
        assert!(ov.contains("evidence: traj-42"));
        let small = s.overview(10);
        assert!(small.len() <= 10 + 3);
    }

    #[test]
    fn slugify_stable() {
        assert_eq!(slugify("Retry On Flaky Tests!", 48), "retry-on-flaky-tests");
        assert_eq!(slugify("", 48), "entry");
        assert_eq!(slugify("a".repeat(100).as_str(), 20).len(), 20);
    }

    #[test]
    fn kind_parse_roundtrip() {
        for k in [
            HarnessKind::Prompt,
            HarnessKind::Memory,
            HarnessKind::Skill,
            HarnessKind::Subagent,
        ] {
            assert_eq!(HarnessKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(HarnessKind::parse("nope"), None);
    }

    #[test]
    fn serde_roundtrip() {
        let mut s = state();
        s.create(HarnessKind::Subagent, "reviewer", "review diffs", Some("traj-1".to_string()));
        let json = serde_json::to_string(&s).unwrap();
        let back: HarnessState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
