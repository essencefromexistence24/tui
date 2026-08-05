use std::path::PathBuf;

use codex_app_server_protocol::AskForApproval;
use codex_protocol::ThreadId;
use codex_protocol::config_types::CollaborationMode;
use codex_protocol::config_types::Personality;
use codex_protocol::models::ActivePermissionProfile;
use codex_protocol::models::PermissionProfile;
use codex_protocol::openai_models::ReasoningEffort;
use codex_utils_absolute_path::AbsolutePathBuf;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MessageHistoryMetadata {
	pub(crate) log_id: u64,
	pub(crate) entry_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ThreadSessionState {
	pub(crate) thread_id: ThreadId,
	pub(crate) forked_from_id: Option<ThreadId>,
	pub(crate) fork_parent_title: Option<String>,
	pub(crate) thread_name: Option<String>,
	pub(crate) model: String,
	pub(crate) model_provider_id: String,
	pub(crate) service_tier: Option<String>,
	pub(crate) approval_policy: AskForApproval,
	pub(crate) approvals_reviewer: codex_protocol::config_types::ApprovalsReviewer,
	pub(crate) permission_profile: PermissionProfile,
	pub(crate) active_permission_profile: Option<ActivePermissionProfile>,
	pub(crate) cwd: AbsolutePathBuf,
	pub(crate) runtime_workspace_roots: Vec<AbsolutePathBuf>,
	pub(crate) reasoning_effort: Option<ReasoningEffort>,
	pub(crate) collaboration_mode: Option<Box<CollaborationMode>>,
	pub(crate) personality: Option<Personality>,
	pub(crate) message_history: Option<MessageHistoryMetadata>,
	pub(crate) rollout_path: Option<PathBuf>,
}
