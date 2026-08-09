use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Minimal model-facing metadata used by the router. The canonical tool
/// definition remains outside this crate and is never modified here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCandidate {
    pub name: String,
    pub description: Option<String>,
    pub always_include: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct ToolRoutingConfig {
    pub enabled: bool,
    pub max_tools: usize,
    pub minimum_keyword_length: usize,
    pub minimum_score: usize,
    pub minimum_margin: usize,
    pub mandatory_tool_names: Vec<String>,
    pub fallback_to_all: bool,
}

impl Default for ToolRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_tools: 0,
            minimum_keyword_length: 3,
            minimum_score: 2,
            minimum_margin: 1,
            mandatory_tool_names: Vec::new(),
            fallback_to_all: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolRoute {
    pub selected_indices: Vec<usize>,
    pub routed: bool,
}

/// Selects a deterministic model-facing subset without changing tool IDs or
/// dispatch schemas. A zero `max_tools` means no cap. If no candidate matches,
/// the safe default is to return every candidate.
pub fn route_tools(
    query: &str,
    candidates: &[ToolCandidate],
    config: &ToolRoutingConfig,
) -> ToolRoute {
    let all = || ToolRoute {
        selected_indices: (0..candidates.len()).collect(),
        routed: false,
    };
    if !config.enabled || candidates.is_empty() || query.trim().is_empty() {
        return all();
    }

    let query_keywords = keywords(query, config.minimum_keyword_length);
    if query_keywords.is_empty() {
        return all();
    }

    let mut scored = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let haystack = format!(
                "{} {}",
                candidate.name.to_ascii_lowercase(),
                candidate
                    .description
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            );
            let candidate_words = keywords(&haystack, config.minimum_keyword_length);
            let score = query_keywords
                .iter()
                .filter(|word| candidate_words.contains(*word))
                .count();
            let mandatory = candidate.always_include
                || config
                    .mandatory_tool_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&candidate.name));
            (index, score, mandatory)
        })
        .filter(|(_, score, mandatory)| *score > 0 || *mandatory)
        .collect::<Vec<_>>();

    if scored.is_empty() {
        return if config.fallback_to_all {
            all()
        } else {
            ToolRoute {
                selected_indices: Vec::new(),
                routed: true,
            }
        };
    }

    scored.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    let best_score = scored.first().map_or(0, |entry| entry.1);
    let second_score = scored.get(1).map_or(0, |entry| entry.1);
    if best_score < config.minimum_score
        || best_score.saturating_sub(second_score) < config.minimum_margin
    {
        return all();
    }
    let cap = if config.max_tools == 0 {
        scored.len()
    } else {
        config.max_tools
    };
    let mut selected = scored
        .into_iter()
        .take(cap)
        .map(|(index, _, _)| index)
        .collect::<Vec<_>>();
    selected.sort_unstable();

    for (index, candidate) in candidates.iter().enumerate() {
        let mandatory = candidate.always_include
            || config
                .mandatory_tool_names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&candidate.name));
        if mandatory && !selected.contains(&index) {
            selected.push(index);
        }
    }
    selected.sort_unstable();
    ToolRoute {
        selected_indices: selected,
        routed: true,
    }
}

fn keywords(query: &str, minimum_length: usize) -> HashSet<String> {
    query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|word| word.len() >= minimum_length)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(name: &str, description: &str, always_include: bool) -> ToolCandidate {
        ToolCandidate {
            name: name.into(),
            description: Some(description.into()),
            always_include,
        }
    }

    #[test]
    fn routes_relevant_tools_and_preserves_always_include() {
        let tools = vec![
            candidate("read_file", "Read source files", true),
            candidate("git_status", "Inspect repository status", false),
            candidate("web_search", "Search the web", false),
        ];
        let route = route_tools(
            "check git status",
            &tools,
            &ToolRoutingConfig {
                enabled: true,
                max_tools: 1,
                minimum_margin: 0,
                ..Default::default()
            },
        );
        assert_eq!(route.selected_indices, vec![0, 1]);
        assert!(route.routed);
    }

    #[test]
    fn no_match_falls_back_to_all_by_default() {
        let tools = vec![candidate("read_file", "Read source files", false)];
        let route = route_tools("explain architecture", &tools, &Default::default());
        assert_eq!(route.selected_indices, vec![0]);
        assert!(!route.routed);
    }

    #[test]
    fn ambiguous_or_weak_matches_fall_back_to_all() {
        let tools = vec![
            candidate("read_file", "Read source files", false),
            candidate("edit_file", "Edit source files", false),
        ];
        let route = route_tools("files", &tools, &ToolRoutingConfig::default());
        assert_eq!(route.selected_indices, vec![0, 1]);
        assert!(!route.routed);
    }

    #[test]
    fn mandatory_tools_are_kept_when_routing() {
        let tools = vec![
            candidate("read_file", "Read source files", false),
            candidate("apply_patch", "Apply a patch", false),
            candidate("task", "Run a subagent task", false),
        ];
        let route = route_tools(
            "read source file",
            &tools,
            &ToolRoutingConfig {
                enabled: true,
                max_tools: 1,
                mandatory_tool_names: vec!["apply_patch".into(), "task".into()],
                ..Default::default()
            },
        );
        assert_eq!(route.selected_indices, vec![0, 1, 2]);
    }
}
