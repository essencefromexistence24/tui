use std::collections::HashMap;
use std::collections::VecDeque;

use super::command::CodexCommand;
use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::CommandExecutionRequestApprovalResponse;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::FileChangeRequestApprovalResponse;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::McpServerElicitationRequestResponse;
use codex_app_server_protocol::PermissionsRequestApprovalResponse;
use codex_app_server_protocol::RequestId as AppServerRequestId;
use codex_app_server_protocol::ServerRequest;
use codex_protocol::request_permissions::RequestPermissionProfile as CoreRequestPermissionProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppServerRequestResolution {
	pub(crate) request_id: AppServerRequestId,
	pub(crate) result: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnsupportedAppServerRequest {
	pub(crate) request_id: AppServerRequestId,
	pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolvedAppServerRequest {
	ExecApproval { id: String },
	FileChangeApproval { id: String },
	PermissionsApproval { id: String },
	UserInput { call_id: String },
	McpElicitation { server_name: String, request_id: AppServerRequestId },
}

#[derive(Debug, Default)]
pub(crate) struct PendingAppServerRequests {
	exec_approvals: HashMap<String, AppServerRequestId>,
	file_change_approvals: HashMap<String, AppServerRequestId>,
	permissions_approvals: HashMap<String, AppServerRequestId>,
	user_inputs: HashMap<String, VecDeque<PendingUserInputRequest>>,
	mcp_requests: HashMap<McpRequestKey, AppServerRequestId>,
}

#[allow(dead_code)]
impl PendingAppServerRequests {
	pub(crate) fn clear(&mut self) {
		self.exec_approvals.clear();
		self.file_change_approvals.clear();
		self.permissions_approvals.clear();
		self.user_inputs.clear();
		self.mcp_requests.clear();
	}

	pub(crate) fn note_server_request(
		&mut self,
		request: &ServerRequest,
	) -> Option<UnsupportedAppServerRequest> {
		match request {
			ServerRequest::CommandExecutionRequestApproval { request_id, params } => {
				let approval_id = params.approval_id.clone().unwrap_or_else(|| params.item_id.clone());
				self.exec_approvals.insert(approval_id, request_id.clone());
				None
			}
			ServerRequest::FileChangeRequestApproval { request_id, params } => {
				self.file_change_approvals.insert(params.item_id.clone(), request_id.clone());
				None
			}
			ServerRequest::PermissionsRequestApproval { request_id, params } => {
				if let Err(err) = CoreRequestPermissionProfile::try_from(params.permissions.clone()) {
					return Some(UnsupportedAppServerRequest {
						request_id: request_id.clone(),
						message: format!("failed to localize requested filesystem paths: {err}"),
					});
				}
				self.permissions_approvals.insert(params.item_id.clone(), request_id.clone());
				None
			}
			ServerRequest::ToolRequestUserInput { request_id, params } => {
				self.user_inputs.entry(params.turn_id.clone()).or_default().push_back(
					PendingUserInputRequest {
						item_id: params.item_id.clone(),
						request_id: request_id.clone(),
					},
				);
				None
			}
			ServerRequest::McpServerElicitationRequest { request_id, params } => {
				self.mcp_requests.insert(
					McpRequestKey { server_name: params.server_name.clone(), request_id: request_id.clone() },
					request_id.clone(),
				);
				None
			}
			ServerRequest::DynamicToolCall { request_id, .. } => Some(UnsupportedAppServerRequest {
				request_id: request_id.clone(),
				message: "Dynamic tool calls are not available in TUI yet.".to_string(),
			}),
			ServerRequest::ChatgptAuthTokensRefresh { .. } => None,
			ServerRequest::AttestationGenerate { request_id, .. } => Some(UnsupportedAppServerRequest {
				request_id: request_id.clone(),
				message: "Attestation generation is not available in TUI.".to_string(),
			}),
			ServerRequest::CurrentTimeRead { request_id, .. } => Some(UnsupportedAppServerRequest {
				request_id: request_id.clone(),
				message: "External current time is not available in TUI.".to_string(),
			}),
			ServerRequest::ApplyPatchApproval { request_id, .. } => Some(UnsupportedAppServerRequest {
				request_id: request_id.clone(),
				message: "Legacy patch approval requests are not available in TUI yet.".to_string(),
			}),
			ServerRequest::ExecCommandApproval { request_id, .. } => Some(UnsupportedAppServerRequest {
				request_id: request_id.clone(),
				message: "Legacy command approval requests are not available in TUI yet.".to_string(),
			}),
		}
	}

	/// Resolve a pending approval with a simple approve/deny decision.
	/// Returns the resolution ready to be sent back to the server.
	pub(crate) fn resolve_approval(
		&mut self,
		request: &ServerRequest,
		approved: bool,
	) -> Option<AppServerRequestResolution> {
		match request {
			ServerRequest::CommandExecutionRequestApproval { request_id: _, params } => {
				let id = params.approval_id.clone().unwrap_or_else(|| params.item_id.clone());
				self.exec_approvals.remove(&id).map(|rid| {
					let decision = if approved {
						CommandExecutionApprovalDecision::Accept
					} else {
						CommandExecutionApprovalDecision::Decline
					};
					AppServerRequestResolution {
						request_id: rid,
						result: serde_json::to_value(CommandExecutionRequestApprovalResponse { decision })
							.unwrap_or_default(),
					}
				})
			}
			ServerRequest::FileChangeRequestApproval { request_id: _, params } => {
				self.file_change_approvals.remove(&params.item_id).map(|rid| {
					let decision = if approved {
						FileChangeApprovalDecision::Accept
					} else {
						FileChangeApprovalDecision::Decline
					};
					AppServerRequestResolution {
						request_id: rid,
						result: serde_json::to_value(FileChangeRequestApprovalResponse { decision })
							.unwrap_or_default(),
					}
				})
			}
			ServerRequest::PermissionsRequestApproval { request_id: _, params } => {
				self.permissions_approvals.remove(&params.item_id).map(|rid| {
					let permissions = if approved {
						if let Ok(core_perms) =
							CoreRequestPermissionProfile::try_from(params.permissions.clone())
						{
							granted_permission_profile_from_request(core_perms)
						} else {
							codex_app_server_protocol::GrantedPermissionProfile {
								network: None,
								file_system: None,
							}
						}
					} else {
						codex_app_server_protocol::GrantedPermissionProfile { network: None, file_system: None }
					};
					AppServerRequestResolution {
						request_id: rid,
						result: serde_json::to_value(PermissionsRequestApprovalResponse {
							permissions,
							scope: codex_app_server_protocol::PermissionGrantScope::Turn,
							strict_auto_review: None,
						})
						.unwrap_or_default(),
					}
				})
			}
			ServerRequest::McpServerElicitationRequest { request_id, params } => {
				let key =
					McpRequestKey { server_name: params.server_name.clone(), request_id: request_id.clone() };
				self.mcp_requests.remove(&key).map(|rid| {
					let action = if approved {
						McpServerElicitationAction::Accept
					} else {
						McpServerElicitationAction::Decline
					};
					AppServerRequestResolution {
						request_id: rid,
						result: serde_json::to_value(McpServerElicitationRequestResponse {
							action,
							content: None,
							meta: None,
						})
						.unwrap_or_default(),
					}
				})
			}
			_ => None,
		}
	}

	pub(crate) fn take_resolution(
		&mut self,
		op: &CodexCommand,
	) -> Option<AppServerRequestResolution> {
		let resolution = match op {
			CodexCommand::ExecApproval { id, decision } => {
				self.exec_approvals.remove(id).and_then(|request_id| {
					serde_json::to_value(CommandExecutionRequestApprovalResponse {
						decision: decision.clone(),
					})
					.ok()
					.map(|result| AppServerRequestResolution { request_id, result })
				})?
			}
			CodexCommand::PatchApproval { id, decision } => {
				self.file_change_approvals.remove(id).and_then(|request_id| {
					serde_json::to_value(FileChangeRequestApprovalResponse { decision: decision.clone() })
						.ok()
						.map(|result| AppServerRequestResolution { request_id, result })
				})?
			}
			CodexCommand::RequestPermissionsResponse { id, response } => {
				self.permissions_approvals.remove(id).and_then(|request_id| {
					let permissions = granted_permission_profile_from_request(response.permissions.clone());
					serde_json::to_value(PermissionsRequestApprovalResponse {
						permissions,
						scope: response.scope.into(),
						strict_auto_review: response.strict_auto_review.then_some(true),
					})
					.ok()
					.map(|result| AppServerRequestResolution { request_id, result })
				})?
			}
			CodexCommand::UserInputAnswer { id, response } => {
				self.pop_user_input_request_for_turn(id).and_then(|pending| {
					serde_json::to_value(response)
						.ok()
						.map(|result| AppServerRequestResolution { request_id: pending.request_id, result })
				})?
			}
			CodexCommand::ResolveElicitation { server_name, request_id, decision, content, meta } => self
				.mcp_requests
				.remove(&McpRequestKey {
					server_name: server_name.to_string(),
					request_id: request_id.clone(),
				})
				.and_then(|request_id| {
					serde_json::to_value(McpServerElicitationRequestResponse {
						action: *decision,
						content: content.clone(),
						meta: meta.clone(),
					})
					.ok()
					.map(|result| AppServerRequestResolution { request_id, result })
				})?,
		};
		Some(resolution)
	}

	pub(crate) fn resolve_notification(
		&mut self,
		request_id: &AppServerRequestId,
	) -> Option<ResolvedAppServerRequest> {
		if let Some(id) =
			self.exec_approvals.iter().find_map(|(id, value)| (value == request_id).then(|| id.clone()))
		{
			self.exec_approvals.remove(&id);
			return Some(ResolvedAppServerRequest::ExecApproval { id });
		}

		if let Some(id) = self
			.file_change_approvals
			.iter()
			.find_map(|(id, value)| (value == request_id).then(|| id.clone()))
		{
			self.file_change_approvals.remove(&id);
			return Some(ResolvedAppServerRequest::FileChangeApproval { id });
		}

		if let Some(id) = self
			.permissions_approvals
			.iter()
			.find_map(|(id, value)| (value == request_id).then(|| id.clone()))
		{
			self.permissions_approvals.remove(&id);
			return Some(ResolvedAppServerRequest::PermissionsApproval { id });
		}

		if let Some(pending) = self.remove_user_input_request(request_id) {
			return Some(ResolvedAppServerRequest::UserInput { call_id: pending.item_id });
		}

		if let Some(key) =
			self.mcp_requests.iter().find_map(|(key, value)| (value == request_id).then(|| key.clone()))
		{
			self.mcp_requests.remove(&key);
			return Some(ResolvedAppServerRequest::McpElicitation {
				server_name: key.server_name,
				request_id: key.request_id,
			});
		}

		None
	}

	pub(crate) fn contains_server_request(&self, request: &ServerRequest) -> bool {
		match request {
			ServerRequest::CommandExecutionRequestApproval { request_id, .. } => {
				self.exec_approvals.values().any(|pending_request_id| pending_request_id == request_id)
			}
			ServerRequest::FileChangeRequestApproval { request_id, .. } => self
				.file_change_approvals
				.values()
				.any(|pending_request_id| pending_request_id == request_id),
			ServerRequest::PermissionsRequestApproval { request_id, .. } => self
				.permissions_approvals
				.values()
				.any(|pending_request_id| pending_request_id == request_id),
			ServerRequest::ToolRequestUserInput { request_id, .. } => self
				.user_inputs
				.values()
				.any(|queue| queue.iter().any(|pending| &pending.request_id == request_id)),
			ServerRequest::McpServerElicitationRequest { request_id, .. } => {
				self.mcp_requests.values().any(|pending_request_id| pending_request_id == request_id)
			}
			ServerRequest::DynamicToolCall { .. }
			| ServerRequest::ChatgptAuthTokensRefresh { .. }
			| ServerRequest::AttestationGenerate { .. }
			| ServerRequest::CurrentTimeRead { .. }
			| ServerRequest::ApplyPatchApproval { .. }
			| ServerRequest::ExecCommandApproval { .. } => true,
		}
	}

	fn pop_user_input_request_for_turn(&mut self, turn_id: &str) -> Option<PendingUserInputRequest> {
		let pending = self.user_inputs.get_mut(turn_id).and_then(VecDeque::pop_front);
		if self.user_inputs.get(turn_id).is_some_and(VecDeque::is_empty) {
			self.user_inputs.remove(turn_id);
		}
		pending
	}

	fn remove_user_input_request(
		&mut self,
		request_id: &AppServerRequestId,
	) -> Option<PendingUserInputRequest> {
		let (turn_id, index) = self.user_inputs.iter().find_map(|(turn_id, queue)| {
			queue
				.iter()
				.position(|pending| &pending.request_id == request_id)
				.map(|index| (turn_id.clone(), index))
		})?;
		let queue = self.user_inputs.get_mut(&turn_id)?;
		let removed = queue.remove(index);
		if queue.is_empty() {
			self.user_inputs.remove(&turn_id);
		}
		removed
	}
}

#[derive(Debug)]
struct PendingUserInputRequest {
	item_id: String,
	request_id: AppServerRequestId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct McpRequestKey {
	server_name: String,
	request_id: AppServerRequestId,
}

pub(crate) fn extract_approval_summary(request: &ServerRequest) -> String {
	match request {
		ServerRequest::CommandExecutionRequestApproval { params, .. } => {
			if let Some(ref cmd) = params.command {
				format!("run `{cmd}`")
			} else {
				"execute command".to_string()
			}
		}
		ServerRequest::FileChangeRequestApproval { .. } => "apply file change".to_string(),
		ServerRequest::PermissionsRequestApproval { params, .. } => {
			format!("request permissions (item: {})", params.item_id)
		}
		ServerRequest::ToolRequestUserInput { params, .. } => {
			format!("request user input (turn: {})", params.turn_id)
		}
		ServerRequest::McpServerElicitationRequest { params, .. } => {
			format!("MCP elicitation request (server: {})", params.server_name)
		}
		_ => "unknown request".to_string(),
	}
}

#[allow(dead_code)]
pub(crate) fn request_id_from_server_request(
	request: &ServerRequest,
) -> Option<codex_app_server_protocol::RequestId> {
	match request {
		ServerRequest::CommandExecutionRequestApproval { request_id, .. } => Some(request_id.clone()),
		ServerRequest::FileChangeRequestApproval { request_id, .. } => Some(request_id.clone()),
		ServerRequest::PermissionsRequestApproval { request_id, .. } => Some(request_id.clone()),
		ServerRequest::ToolRequestUserInput { request_id, .. } => Some(request_id.clone()),
		ServerRequest::McpServerElicitationRequest { request_id, .. } => Some(request_id.clone()),
		ServerRequest::DynamicToolCall { request_id, .. } => Some(request_id.clone()),
		ServerRequest::CurrentTimeRead { request_id, .. } => Some(request_id.clone()),
		ServerRequest::ChatgptAuthTokensRefresh { request_id, .. } => Some(request_id.clone()),
		ServerRequest::AttestationGenerate { request_id, .. } => Some(request_id.clone()),
		ServerRequest::ApplyPatchApproval { request_id, .. } => Some(request_id.clone()),
		ServerRequest::ExecCommandApproval { request_id, .. } => Some(request_id.clone()),
	}
}

fn granted_permission_profile_from_request(
	permissions: codex_protocol::request_permissions::RequestPermissionProfile,
) -> codex_app_server_protocol::GrantedPermissionProfile {
	codex_app_server_protocol::GrantedPermissionProfile {
		network: permissions
			.network
			.map(|n| codex_app_server_protocol::AdditionalNetworkPermissions { enabled: n.enabled }),
		file_system: permissions.file_system.map(|fs| {
			let (read, write) = fs.legacy_read_write_roots().unwrap_or_default();
			codex_app_server_protocol::AdditionalFileSystemPermissions {
				read: read.map(|paths| paths.into_iter().map(|p| p.into()).collect()),
				write: write.map(|paths| paths.into_iter().map(|p| p.into()).collect()),
				glob_scan_max_depth: None,
				entries: None,
			}
		}),
	}
}
