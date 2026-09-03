use std::collections::HashMap;
use std::path::Path;

use codex_app_server_client::legacy_core::config::Config;
use codex_app_server_protocol::ApprovalsReviewer;
use codex_app_server_protocol::SandboxMode;
use codex_app_server_protocol::ThreadForkParams;
use codex_app_server_protocol::ThreadResumeParams;
use codex_app_server_protocol::ThreadSource;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartSource;
use codex_protocol::config_types::SERVICE_TIER_DEFAULT_REQUEST_VALUE;
use codex_protocol::models::ActivePermissionProfile;
use codex_protocol::models::PermissionProfile;
use serde_json::Value as JsonValue;

use super::permission_compat::legacy_compatible_permission_profile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum ThreadParamsMode {
	Embedded,
	Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResumeModelSettings {
	OverrideFromCurrentConfig,
	RestoreFromThread,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum TurnPermissionsOverride {
	Preserve,
	ActiveProfile(ActivePermissionProfile),
	LegacySandbox(PermissionProfile),
}

#[derive(Debug)]
pub(crate) struct AppServerStartedThread {
	pub(crate) session: super::thread_session_state::ThreadSessionState,
	pub(crate) turns: Vec<codex_app_server_protocol::Turn>,
}

impl ThreadParamsMode {
	pub(crate) fn model_provider_from_config(self, config: &Config) -> Option<String> {
		match self {
			Self::Embedded => Some(config.model_provider_id.clone()),
			Self::Remote => None,
		}
	}
}

pub(crate) fn approvals_reviewer_override_from_config(
	config: &Config,
) -> Option<ApprovalsReviewer> {
	Some(config.approvals_reviewer.into())
}

pub(crate) fn config_request_overrides_from_config(
	config: &Config,
) -> Option<HashMap<String, JsonValue>> {
	let mut overrides = HashMap::new();
	let mut insert = |key: &str, value: Option<String>| {
		if let Some(value) = value {
			overrides.insert(key.to_string(), JsonValue::String(value));
		}
	};
	insert(
		"model_reasoning_effort",
		config.model_reasoning_effort.as_ref().map(std::string::ToString::to_string),
	);
	insert(
		"model_reasoning_summary",
		config.model_reasoning_summary.map(|summary| summary.to_string()),
	);
	insert("model_verbosity", config.model_verbosity.map(|verbosity| verbosity.to_string()));
	insert("personality", config.personality.map(|personality| personality.to_string()));
	insert("web_search", Some(config.web_search_mode.value().to_string()));
	if config.bypass_hook_trust {
		overrides.insert("bypass_hook_trust".to_string(), true.into());
	}
	if overrides.is_empty() { None } else { Some(overrides) }
}

pub(crate) fn service_tier_override_from_config(config: &Config) -> Option<Option<String>> {
	config.service_tier.clone().map(Some).or_else(|| {
		(config.notices.fast_default_opt_out == Some(true))
			.then(|| Some(SERVICE_TIER_DEFAULT_REQUEST_VALUE.to_string()))
	})
}

pub(crate) fn sandbox_mode_from_permission_profile(
	permission_profile: &PermissionProfile,
	cwd: &Path,
) -> Option<SandboxMode> {
	match permission_profile {
		PermissionProfile::Disabled => Some(SandboxMode::DangerFullAccess),
		PermissionProfile::External { .. } => None,
		PermissionProfile::Managed { .. } => {
			let file_system_policy = permission_profile.file_system_sandbox_policy();
			if file_system_policy.has_full_disk_write_access() {
				permission_profile
					.network_sandbox_policy()
					.is_enabled()
					.then_some(SandboxMode::DangerFullAccess)
			} else if file_system_policy.can_write_path_with_cwd(cwd, cwd) {
				Some(SandboxMode::WorkspaceWrite)
			} else {
				Some(SandboxMode::ReadOnly)
			}
		}
	}
}

fn permission_profile_id_from_active_profile(active: ActivePermissionProfile) -> String {
	active.id
}

pub(crate) fn permissions_selection_from_config(
	config: &Config,
	thread_params_mode: ThreadParamsMode,
) -> Option<String> {
	if matches!(thread_params_mode, ThreadParamsMode::Remote) {
		return None;
	}

	config.permissions.active_permission_profile().map(permission_profile_id_from_active_profile)
}

pub(crate) fn turn_permissions_overrides(
	permissions_override: &TurnPermissionsOverride,
	cwd: &Path,
) -> (Option<codex_app_server_protocol::SandboxPolicy>, Option<String>) {
	match permissions_override {
		TurnPermissionsOverride::Preserve => (None, None),
		TurnPermissionsOverride::ActiveProfile(active_permission_profile) => {
			(None, Some(permission_profile_id_from_active_profile(active_permission_profile.clone())))
		}
		TurnPermissionsOverride::LegacySandbox(permission_profile) => {
			let legacy_profile = legacy_compatible_permission_profile(permission_profile, cwd);
			let policy = legacy_profile.to_legacy_sandbox_policy(cwd).unwrap_or_else(|err| {
				unreachable!("legacy-compatible permissions must project to legacy policy: {err}")
			});
			(Some(policy.into()), None)
		}
	}
}

fn thread_cwd_from_config(
	config: &Config,
	thread_params_mode: ThreadParamsMode,
	remote_cwd_override: Option<&Path>,
) -> Option<String> {
	match thread_params_mode {
		ThreadParamsMode::Embedded => Some(config.cwd.to_string_lossy().to_string()),
		ThreadParamsMode::Remote => remote_cwd_override.map(|cwd| cwd.to_string_lossy().to_string()),
	}
}

pub(crate) fn thread_start_params_from_config(
	config: &Config,
	thread_params_mode: ThreadParamsMode,
	remote_cwd_override: Option<&Path>,
	session_start_source: Option<ThreadStartSource>,
) -> ThreadStartParams {
	let permissions = permissions_selection_from_config(config, thread_params_mode);
	let sandbox = permissions
		.is_none()
		.then(|| {
			sandbox_mode_from_permission_profile(
				&config.permissions.effective_permission_profile(),
				config.cwd.as_path(),
			)
		})
		.flatten();
	ThreadStartParams {
		model: config.model.clone(),
		model_provider: thread_params_mode.model_provider_from_config(config),
		service_tier: service_tier_override_from_config(config),
		cwd: thread_cwd_from_config(config, thread_params_mode, remote_cwd_override),
		runtime_workspace_roots: Some(config.workspace_roots.clone()),
		approval_policy: Some(config.permissions.approval_policy.value().into()),
		approvals_reviewer: approvals_reviewer_override_from_config(config),
		sandbox,
		permissions,
		config: config_request_overrides_from_config(config),
		ephemeral: Some(config.ephemeral),
		session_start_source,
		thread_source: Some(ThreadSource::User),
		developer_instructions: None,
		..ThreadStartParams::default()
	}
}

pub(crate) fn thread_resume_params_from_config(
	config: Config,
	thread_id: codex_protocol::ThreadId,
	thread_params_mode: ThreadParamsMode,
	remote_cwd_override: Option<&Path>,
	model_settings: ResumeModelSettings,
) -> ThreadResumeParams {
	let permissions = permissions_selection_from_config(&config, thread_params_mode);
	let sandbox = permissions
		.is_none()
		.then(|| {
			sandbox_mode_from_permission_profile(
				&config.permissions.effective_permission_profile(),
				config.cwd.as_path(),
			)
		})
		.flatten();
	let mut config_overrides = config_request_overrides_from_config(&config);
	if model_settings == ResumeModelSettings::RestoreFromThread
		&& let Some(overrides) = config_overrides.as_mut()
	{
		overrides.remove("model_reasoning_effort");
		if overrides.is_empty() {
			config_overrides = None;
		}
	}
	let (model, model_provider) = match model_settings {
		ResumeModelSettings::OverrideFromCurrentConfig => {
			(config.model.clone(), thread_params_mode.model_provider_from_config(&config))
		}
		ResumeModelSettings::RestoreFromThread => (None, None),
	};
	ThreadResumeParams {
		thread_id: thread_id.to_string(),
		model,
		model_provider,
		service_tier: service_tier_override_from_config(&config),
		cwd: thread_cwd_from_config(&config, thread_params_mode, remote_cwd_override),
		runtime_workspace_roots: Some(config.workspace_roots.clone()),
		approval_policy: Some(config.permissions.approval_policy.value().into()),
		approvals_reviewer: approvals_reviewer_override_from_config(&config),
		sandbox,
		permissions,
		config: config_overrides,
		developer_instructions: None,
		..ThreadResumeParams::default()
	}
}

pub(crate) fn thread_fork_params_from_config(
	config: Config,
	thread_id: codex_protocol::ThreadId,
	thread_params_mode: ThreadParamsMode,
	remote_cwd_override: Option<&Path>,
) -> ThreadForkParams {
	let permissions = permissions_selection_from_config(&config, thread_params_mode);
	let sandbox = permissions
		.is_none()
		.then(|| {
			sandbox_mode_from_permission_profile(
				&config.permissions.effective_permission_profile(),
				config.cwd.as_path(),
			)
		})
		.flatten();
	ThreadForkParams {
		thread_id: thread_id.to_string(),
		model: config.model.clone(),
		model_provider: thread_params_mode.model_provider_from_config(&config),
		service_tier: service_tier_override_from_config(&config),
		cwd: thread_cwd_from_config(&config, thread_params_mode, remote_cwd_override),
		runtime_workspace_roots: Some(config.workspace_roots.clone()),
		approval_policy: Some(config.permissions.approval_policy.value().into()),
		approvals_reviewer: approvals_reviewer_override_from_config(&config),
		sandbox,
		permissions,
		config: config_request_overrides_from_config(&config),
		base_instructions: config.base_instructions.clone(),
		developer_instructions: None,
		ephemeral: config.ephemeral,
		thread_source: Some(ThreadSource::User),
		..ThreadForkParams::default()
	}
}
