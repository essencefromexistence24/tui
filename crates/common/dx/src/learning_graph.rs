//! Learning graph — builds a knowledge graph from skills + memory entries
//! and renders it as a compact text visualization for the UI sidebar.
//!
//! Inspired by hermes-agent `agent/learning_graph.py`.

#![allow(dead_code)]

use std::collections::HashMap;

use crate::{
	memory_tool::MemoryStore,
	skills::{self, get_all_usage},
};

/// A node in the learning graph.
#[derive(Debug, Clone)]
pub struct GraphNode {
	pub id: String,
	pub kind: NodeKind,
	pub label: String,
	pub description: String,
	pub usage_count: u64,
	pub state: String, // "active", "stale", "archived"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
	Skill,
	Memory,
	UserProfile,
}

impl NodeKind {
	pub fn glyph(&self) -> &'static str {
		match self {
			Self::Skill => "⚡",
			Self::Memory => "📝",
			Self::UserProfile => "👤",
		}
	}

	pub fn label(&self) -> &'static str {
		match self {
			Self::Skill => "Skill",
			Self::Memory => "Memory",
			Self::UserProfile => "User",
		}
	}
}

/// The full learning graph.
#[derive(Debug, Clone)]
pub struct LearningGraph {
	pub nodes: Vec<GraphNode>,
	pub total_nodes: usize,
}

impl LearningGraph {
	/// Build the graph from current skills + memory.
	pub fn build() -> Self {
		let mut nodes = Vec::new();

		// Skills
		let skills = skills::list_skills();
		let usage = get_all_usage();
		for skill in &skills {
			let u = usage.get(&skill.name);
			nodes.push(GraphNode {
				id: format!("skill:{}", skill.name),
				kind: NodeKind::Skill,
				label: skill.name.clone(),
				description: skill.description.clone(),
				usage_count: u.map(|u| u.use_count).unwrap_or(0),
				state: u.map(|u| u.state.clone()).unwrap_or_else(|| "active".into()),
			});
		}

		// Memory entries
		let store = MemoryStore::new();
		let mem = store.list_memory();
		for line in mem.output.lines() {
			let content = line.trim();
			if content.is_empty() || content.starts_with("(no") || content.starts_with('#') {
				continue;
			}
			let clean = content.trim_start_matches(['-', '*', ' ']).trim();
			if clean.is_empty() {
				continue;
			}
			let id = format!("mem:{}", clean.chars().take(40).collect::<String>());
			nodes.push(GraphNode {
				id,
				kind: NodeKind::Memory,
				label: clean.chars().take(60).collect(),
				description: String::new(),
				usage_count: 0,
				state: "active".into(),
			});
			if nodes.len() >= 20 {
				break;
			}
		}

		// User profile entries
		let usr = store.list_user();
		for line in usr.output.lines() {
			let content = line.trim();
			if content.is_empty() || content.starts_with("(no") || content.starts_with('#') {
				continue;
			}
			let clean = content.trim_start_matches(['-', '*', ' ']).trim();
			if clean.is_empty() {
				continue;
			}
			let id = format!("user:{}", clean.chars().take(40).collect::<String>());
			nodes.push(GraphNode {
				id,
				kind: NodeKind::UserProfile,
				label: clean.chars().take(60).collect(),
				description: String::new(),
				usage_count: 0,
				state: "active".into(),
			});
			if nodes.iter().filter(|n| n.kind == NodeKind::UserProfile).count() >= 10 {
				break;
			}
		}

		let total_nodes = nodes.len();
		// Sort: skills first (by usage), then memory, then user
		nodes.sort_by(|a, b| {
			b.usage_count.cmp(&a.usage_count).then_with(|| a.kind.label().cmp(b.kind.label()))
		});

		Self { nodes, total_nodes }
	}

	/// Render as compact text lines for the sidebar.
	pub fn render_lines(&self, max_lines: usize) -> Vec<String> {
		if self.nodes.is_empty() {
			return vec!["—".into()];
		}
		let mut lines = Vec::new();
		let count_by_kind =
			self.nodes.iter().fold(HashMap::new(), |mut acc: HashMap<&str, usize>, n| {
				*acc.entry(n.kind.label()).or_insert(0) += 1;
				acc
			});
		let summary: Vec<String> = count_by_kind.iter().map(|(k, v)| format!("{k}: {v}")).collect();
		lines.push(summary.join(" · "));

		for node in self.nodes.iter().take(max_lines.saturating_sub(1)) {
			let state_glyph = match node.state.as_str() {
				"stale" => "○",
				"archived" => "◌",
				_ => "●",
			};
			let label = node.label.chars().take(40).collect::<String>();
			lines.push(format!(
				"{} {} {} {}",
				node.kind.glyph(),
				state_glyph,
				label,
				if node.usage_count > 0 { format!("· {} used", node.usage_count) } else { String::new() }
			));
		}
		lines
	}
}
