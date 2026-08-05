use std::path::Path;
use std::path::PathBuf;

use codex_app_server_client::legacy_core::config::Config;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ThreadForkResponse;
use codex_app_server_protocol::ThreadResumeResponse;
use codex_app_server_protocol::ThreadStartResponse;
use codex_protocol::ThreadId;
use codex_protocol::models::ActivePermissionProfile;
use codex_protocol::models::PermissionProfile;
use codex_utils_absolute_path::AbsolutePathBuf;

use super::params::AppServerStartedThread;
use super::params::ThreadParamsMode;
use super::thread_session_state::ThreadSessionState;

#[allow(clippy::too_many_arguments)]
async fn thread_session_state_from_thread_response(
	thread_id: &str,
	forked_from_id: Option<String>,
	thread_name: Option<String>,
	rollout_path: Option<PathBuf>,
	model: String,
	model_provider_id: String,
	service_tier: Option<String>,
	approval_policy: AskForApproval,
	approvals_reviewer: codex_protocol::config_types::ApprovalsReviewer,
	permission_profile: PermissionProfile,
	active_permission_profile: Option<ActivePermissionProfile>,
	cwd: AbsolutePathBuf,
	runtime_workspace_roots: Vec<AbsolutePathBuf>,
	reasoning_effort: Option<codex_protocol::openai_models::ReasoningEffort>,
	config: &Config,
) -> Result<ThreadSessionState, String> {
	let thread_id = ThreadId::from_string(thread_id)
		.map_err(|err| format!("thread id `{thread_id}` is invalid: {err}"))?;
	let forked_from_id = forked_from_id
		.as_deref()
		.map(ThreadId::from_string)
		.transpose()
		.map_err(|err| format!("forked_from_id is invalid: {err}"))?;
	Ok(ThreadSessionState {
		thread_id,
		forked_from_id,
		fork_parent_title: None,
		thread_name,
		model,
		model_provider_id,
		service_tier,
		approval_policy,
		approvals_reviewer,
		permission_profile,
		active_permission_profile,
		cwd,
		runtime_workspace_roots,
		reasoning_effort,
		collaboration_mode: None,
		personality: config.personality,
		message_history: None,
		rollout_path,
	})
}

fn display_permission_profile_from_thread_response(
	sandbox: &codex_app_server_protocol::SandboxPolicy,
	cwd: &Path,
	config: &Config,
	thread_params_mode: ThreadParamsMode,
) -> PermissionProfile {
	match thread_params_mode {
		ThreadParamsMode::Embedded => config.permissions.effective_permission_profile(),
		ThreadParamsMode::Remote => {
			PermissionProfile::from_legacy_sandbox_policy_for_cwd(&sandbox.to_core(), cwd)
		}
	}
}

pub(crate) async fn started_thread_from_start_response(
	response: ThreadStartResponse,
	config: &Config,
	thread_params_mode: ThreadParamsMode,
) -> Result<AppServerStartedThread, String> {
	let permission_profile = display_permission_profile_from_thread_response(
		&response.sandbox,
		response.cwd.as_path(),
		config,
		thread_params_mode,
	);
	let session = thread_session_state_from_thread_response(
		&response.thread.id,
		response.thread.forked_from_id.clone(),
		response.thread.name.clone(),
		response.thread.path.clone(),
		response.model.clone(),
		response.model_provider.clone(),
		response.service_tier.clone(),
		response.approval_policy,
		response.approvals_reviewer.to_core(),
		permission_profile,
		response.active_permission_profile.clone().map(Into::into),
		response.cwd.clone(),
		response.runtime_workspace_roots.clone(),
		response.reasoning_effort.clone(),
		config,
	)
	.await?;
	Ok(AppServerStartedThread { session, turns: response.thread.turns })
}

pub(crate) async fn started_thread_from_resume_response(
	response: ThreadResumeResponse,
	config: &Config,
	thread_params_mode: ThreadParamsMode,
) -> Result<AppServerStartedThread, String> {
	let permission_profile = if matches!(thread_params_mode, ThreadParamsMode::Embedded)
		&& response.active_permission_profile.is_none()
	{
		PermissionProfile::from_legacy_sandbox_policy_for_cwd(
			&response.sandbox.to_core(),
			response.cwd.as_path(),
		)
	} else {
		display_permission_profile_from_thread_response(
			&response.sandbox,
			response.cwd.as_path(),
			config,
			thread_params_mode,
		)
	};
	let session = thread_session_state_from_thread_response(
		&response.thread.id,
		response.thread.forked_from_id.clone(),
		response.thread.name.clone(),
		response.thread.path.clone(),
		response.model.clone(),
		response.model_provider.clone(),
		response.service_tier.clone(),
		response.approval_policy,
		response.approvals_reviewer.to_core(),
		permission_profile,
		response.active_permission_profile.clone().map(Into::into),
		response.cwd.clone(),
		response.runtime_workspace_roots.clone(),
		response.reasoning_effort.clone(),
		config,
	)
	.await?;
	Ok(AppServerStartedThread { session, turns: response.thread.turns })
}

pub(crate) async fn started_thread_from_fork_response(
	response: ThreadForkResponse,
	config: &Config,
	thread_params_mode: ThreadParamsMode,
) -> Result<AppServerStartedThread, String> {
	let permission_profile = display_permission_profile_from_thread_response(
		&response.sandbox,
		response.cwd.as_path(),
		config,
		thread_params_mode,
	);
	let session = thread_session_state_from_thread_response(
		&response.thread.id,
		response.thread.forked_from_id.clone(),
		response.thread.name.clone(),
		response.thread.path.clone(),
		response.model.clone(),
		response.model_provider.clone(),
		response.service_tier.clone(),
		response.approval_policy,
		response.approvals_reviewer.to_core(),
		permission_profile,
		response.active_permission_profile.clone().map(Into::into),
		response.cwd.clone(),
		response.runtime_workspace_roots.clone(),
		response.reasoning_effort.clone(),
		config,
	)
	.await?;
	Ok(AppServerStartedThread { session, turns: response.thread.turns })
}
