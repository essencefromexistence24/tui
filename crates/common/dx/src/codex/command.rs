use codex_app_server_protocol::CommandExecutionApprovalDecision;
use codex_app_server_protocol::FileChangeApprovalDecision;
use codex_app_server_protocol::McpServerElicitationAction;
use codex_app_server_protocol::RequestId as AppServerRequestId;
use codex_protocol::request_permissions::RequestPermissionsResponse;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum CodexCommand {
	ExecApproval {
		id: String,
		decision: CommandExecutionApprovalDecision,
	},
	PatchApproval {
		id: String,
		decision: FileChangeApprovalDecision,
	},
	RequestPermissionsResponse {
		id: String,
		response: RequestPermissionsResponse,
	},
	UserInputAnswer {
		id: String,
		response: codex_app_server_protocol::ToolRequestUserInputResponse,
	},
	ResolveElicitation {
		server_name: String,
		request_id: AppServerRequestId,
		decision: McpServerElicitationAction,
		content: Option<serde_json::Value>,
		meta: Option<serde_json::Value>,
	},
}
