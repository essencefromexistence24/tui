//! Profile policy helpers (system text lives in `dx_system`).

use crate::modes::AgentMode;

/// Approval / sandbox policy labels per profile.
pub fn profile_policy(mode: AgentMode) -> (&'static str, &'static str) {
	match mode {
		AgentMode::Ask => ("read-only", "strict"),
		AgentMode::Write => ("on-request", "workspace-write"),
		AgentMode::Plan => ("read-only", "strict"),
		AgentMode::Goal => ("on-failure", "workspace-write"),
		AgentMode::Agent => ("on-request", "workspace-write"),
		AgentMode::Multi => ("read-only", "strict"),
		AgentMode::Automation => ("auto-approve", "workspace-write"),
		AgentMode::Codex => ("managed-by-codex", "managed-by-codex"),
	}
}

/// Short profile blurb (UI / status). Full text is in `dx_system::profile_layer`.
#[allow(dead_code)]
pub fn profile_label_line(mode: AgentMode) -> String {
	let (a, s) = profile_policy(mode);
	format!("{} · approval:{a} · sandbox:{s}", mode.label())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn policy_ask_readonly() {
		let (a, s) = profile_policy(AgentMode::Ask);
		assert_eq!(a, "read-only");
		assert_eq!(s, "strict");
	}
}
