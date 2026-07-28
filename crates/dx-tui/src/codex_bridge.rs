use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use anyhow::Context;
use futures::FutureExt;

use codex_app_server_client::{
	DEFAULT_IN_PROCESS_CHANNEL_CAPACITY, InProcessAppServerClient, InProcessAppServerRequestHandle,
	InProcessClientStartArgs,
	legacy_core::config::{Config, ConfigBuilder},
};
use codex_app_server_protocol::{
	ClientRequest, ConfigWarningNotification, RequestId, ServerNotification, ServerRequest,
	ThreadStartResponse, TurnStartParams, TurnStartResponse, UserInput,
};
use codex_arg0::Arg0DispatchPaths;
use codex_config::{CloudConfigBundleLoader, LoaderOverrides};
use codex_exec_server::EnvironmentManager;
use codex_feedback::CodexFeedback;
use codex_rollout::state_db;

use crate::codex::event_router::{InProcessCodexEvent, handle_app_server_event};
use crate::codex::params::{self as codex_params, ThreadParamsMode};
use crate::codex::pending_requests::PendingAppServerRequests;
use crate::codex::response as codex_response;
use crate::codex::thread_events::ThreadEventStore;
use crate::codex::thread_session_state::ThreadSessionState;

const EVENT_CAPACITY: usize = 1024;

#[derive(Debug)]
pub enum BridgeEvent {
	AgentMessageDelta { delta: String },
	ReasoningTextDelta { delta: String },
	ReasoningSummaryTextDelta { delta: String },
	PlanDelta { delta: String },
	TurnStarted { turn_id: String },
	TurnCompleted { turn_id: String },
	ItemStarted { item: codex_app_server_protocol::ThreadItem },
	ItemCompleted { item: codex_app_server_protocol::ThreadItem },
	CommandExecutionOutputDelta { delta: String },
	FileChangeOutputDelta { delta: String },
	TerminalInteraction { process_id: String, stdin: Option<String> },
	ThreadNameUpdated { name: Option<String> },
	ThreadTokenUsageUpdated { usage: codex_app_server_protocol::ThreadTokenUsage },
	ThreadSettingsUpdated { settings: codex_app_server_protocol::ThreadSettings },
	RequestApproval(ServerRequest),
	HookStarted,
	HookCompleted,
	ServerError { message: String, thread_id: String },
	Warning { message: String },
	Error(String),
	Disconnected { message: String },
	Lagged { skipped: u32 },
	Ignored,
}

#[derive(Debug)]
pub struct ResumeForkResult {
	pub thread_id: String,
	pub(crate) event_store: ThreadEventStore,
}

pub struct CodexBridge {
	client: InProcessAppServerClient,
	handle: InProcessAppServerRequestHandle,
	thread_id: String,
	next_request_id: AtomicI64,
	config: Config,
	thread_params_mode: ThreadParamsMode,
	event_store: ThreadEventStore,
	pending_requests: PendingAppServerRequests,
}

#[allow(dead_code)]
impl CodexBridge {
	pub async fn start() -> anyhow::Result<Self> {
		let config = match ConfigBuilder::default().build().await {
			Ok(config) => config,
			Err(_) => Config::load_default_with_cli_overrides(Vec::new())
				.await
				.expect("default config should load"),
		};

		let state_db =
			state_db::try_init(&config).await.context("Failed to initialize state database")?;

		let environment_manager = Arc::new(EnvironmentManager::default_for_tests());

		let config_warnings: Vec<ConfigWarningNotification> = config
			.startup_warnings
			.iter()
			.map(|w| ConfigWarningNotification {
				summary: w.clone(),
				details: None,
				path: None,
				range: None,
			})
			.collect();

		let client = InProcessAppServerClient::start(InProcessClientStartArgs {
			arg0_paths: Arg0DispatchPaths::default(),
			config: Arc::new(config.clone()),
			cli_overrides: Vec::new(),
			loader_overrides: LoaderOverrides::default(),
			strict_config: false,
			cloud_config_bundle: CloudConfigBundleLoader::default(),
			feedback: CodexFeedback::new(),
			log_db: None,
			state_db: Some(state_db),
			environment_manager,
			config_warnings,
			session_source: serde_json::from_value(serde_json::json!("exec"))
				.unwrap_or_else(|err| panic!("session source should deserialize: {err}")),
			enable_codex_api_key_env: false,
			client_name: "dx-tui".to_string(),
			client_version: "1.0.0".to_string(),
			experimental_api: true,
			mcp_server_openai_form_elicitation: false,
			opt_out_notification_methods: Vec::new(),
			channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
		})
		.await
		.context("Failed to start app server")?;

		let handle = client.request_handle();

		let thread_params_mode = ThreadParamsMode::Embedded;

		let params =
			codex_params::thread_start_params_from_config(&config, thread_params_mode, None, None);

		let response: ThreadStartResponse = handle
			.request_typed(ClientRequest::ThreadStart { request_id: RequestId::Integer(1), params })
			.await
			.context("thread/start failed")?;

		let started =
			codex_response::started_thread_from_start_response(response, &config, thread_params_mode)
				.await
				.map_err(|e| anyhow::anyhow!("failed to map thread start response: {e}"))?;

		let mut event_store = ThreadEventStore::new(EVENT_CAPACITY);
		event_store.set_session(started.session, started.turns);

		Ok(Self {
			client,
			handle,
			thread_id: event_store.session.as_ref().map(|s| s.thread_id.to_string()).unwrap_or_default(),
			next_request_id: AtomicI64::new(2),
			config,
			thread_params_mode,
			event_store,
			pending_requests: PendingAppServerRequests::default(),
		})
	}

	pub fn try_recv_event(&mut self) -> Option<BridgeEvent> {
		let raw_event = match self.client.next_event().now_or_never()? {
			Some(event) => event,
			None => return Some(BridgeEvent::Disconnected { message: "Server shut down".into() }),
		};

		let processed = handle_app_server_event(&mut self.pending_requests, raw_event);

		match processed {
			InProcessCodexEvent::Notification(notification) => {
				self.event_store.push_notification(notification.clone());
				Some(self.map_notification_to_bridge(notification))
			}
			InProcessCodexEvent::Request(request) => {
				self.event_store.push_request(request.clone());
				Some(BridgeEvent::RequestApproval(request))
			}
			InProcessCodexEvent::ServerRequestResolved { .. } => Some(BridgeEvent::Ignored),
			InProcessCodexEvent::AccountRateLimitsUpdated(_) => Some(BridgeEvent::Ignored),
			InProcessCodexEvent::AccountUpdated { .. } => Some(BridgeEvent::Ignored),
			InProcessCodexEvent::AppListUpdated(_) => Some(BridgeEvent::Ignored),
			InProcessCodexEvent::UnsupportedRequest { message, .. } => {
				Some(BridgeEvent::Warning { message })
			}
			InProcessCodexEvent::Lagged { skipped } => Some(BridgeEvent::Lagged { skipped }),
			InProcessCodexEvent::Disconnected { message } => Some(BridgeEvent::Disconnected { message }),
			InProcessCodexEvent::Ignored => None,
		}
	}

	pub fn build_turn_start_params(&self, text: &str) -> TurnStartParams {
		let (sandbox_policy, permissions) = codex_params::turn_permissions_overrides(
			&codex_params::TurnPermissionsOverride::Preserve,
			self.config.cwd.as_path(),
		);
		TurnStartParams {
			thread_id: self.thread_id.clone(),
			input: vec![UserInput::Text { text: text.to_string(), text_elements: Vec::new() }],
			cwd: Some(self.config.cwd.as_path().to_path_buf()),
			runtime_workspace_roots: Some(self.config.workspace_roots.clone()),
			approval_policy: Some(self.config.permissions.approval_policy.value().into()),
			approvals_reviewer: Some(self.config.approvals_reviewer.into()),
			sandbox_policy,
			permissions,
			model: self.config.model.clone(),
			service_tier: codex_params::service_tier_override_from_config(&self.config),
			effort: self.config.model_reasoning_effort.clone(),
			summary: self.config.model_reasoning_summary,
			personality: self.config.personality,
			output_schema: None,
			..TurnStartParams::default()
		}
	}

	pub async fn submit_turn(&self, text: &str) -> anyhow::Result<()> {
		let request_id = RequestId::Integer(self.next_request_id.fetch_add(1, Ordering::Relaxed));
		let params = self.build_turn_start_params(text);
		self
			.handle
			.request_typed::<TurnStartResponse>(ClientRequest::TurnStart { request_id, params })
			.await
			.context("turn/start failed")?;
		Ok(())
	}

	pub async fn turn_interrupt(&self, turn_id: &str) -> anyhow::Result<()> {
		let request_id = RequestId::Integer(self.next_request_id.fetch_add(1, Ordering::Relaxed));
		self
			.handle
			.request_typed::<codex_app_server_protocol::TurnInterruptResponse>(
				ClientRequest::TurnInterrupt {
					request_id,
					params: codex_app_server_protocol::TurnInterruptParams {
						thread_id: self.thread_id.clone(),
						turn_id: turn_id.to_string(),
					},
				},
			)
			.await
			.context("turn/interrupt failed")?;
		Ok(())
	}

	pub async fn startup_interrupt(&self) -> anyhow::Result<()> {
		self.turn_interrupt("").await
	}

	/// Perform the thread/resume RPC using only handle + config (no &mut self needed).
	pub(crate) async fn resume_thread_rpc(
		handle: &InProcessAppServerRequestHandle,
		config: &Config,
		mode: ThreadParamsMode,
		thread_id: &str,
	) -> anyhow::Result<ResumeForkResult> {
		let tid = codex_protocol::ThreadId::from_string(thread_id)
			.map_err(|e| anyhow::anyhow!("invalid thread id: {e}"))?;
		let params = codex_params::thread_resume_params_from_config(
			config.clone(),
			tid,
			mode,
			None,
			codex_params::ResumeModelSettings::OverrideFromCurrentConfig,
		);
		let response: codex_app_server_protocol::ThreadResumeResponse = handle
			.request_typed(ClientRequest::ThreadResume { request_id: RequestId::Integer(1), params })
			.await
			.context("thread/resume failed")?;

		let started = codex_response::started_thread_from_resume_response(response, config, mode)
			.await
			.map_err(|e| anyhow::anyhow!("failed to map resume response: {e}"))?;

		let mut event_store = ThreadEventStore::new(EVENT_CAPACITY);
		event_store.set_session(started.session, started.turns);
		let thread_id =
			event_store.session.as_ref().map(|s| s.thread_id.to_string()).unwrap_or_default();
		Ok(ResumeForkResult { thread_id, event_store })
	}

	/// Perform the thread/fork RPC using only handle + config (no &mut self needed).
	pub(crate) async fn fork_thread_rpc(
		handle: &InProcessAppServerRequestHandle,
		config: &Config,
		mode: ThreadParamsMode,
		thread_id: &str,
	) -> anyhow::Result<ResumeForkResult> {
		let tid = codex_protocol::ThreadId::from_string(thread_id)
			.map_err(|e| anyhow::anyhow!("invalid thread id: {e}"))?;
		let params = codex_params::thread_fork_params_from_config(config.clone(), tid, mode, None);
		let response: codex_app_server_protocol::ThreadForkResponse = handle
			.request_typed(ClientRequest::ThreadFork { request_id: RequestId::Integer(1), params })
			.await
			.context("thread/fork failed")?;

		let started = codex_response::started_thread_from_fork_response(response, config, mode)
			.await
			.map_err(|e| anyhow::anyhow!("failed to map fork response: {e}"))?;

		let mut event_store = ThreadEventStore::new(EVENT_CAPACITY);
		event_store.set_session(started.session, started.turns);
		let thread_id =
			event_store.session.as_ref().map(|s| s.thread_id.to_string()).unwrap_or_default();
		Ok(ResumeForkResult { thread_id, event_store })
	}

	/// Apply the result of a resume/fork RPC to the bridge state.
	pub fn apply_resume_fork(&mut self, result: ResumeForkResult) {
		self.thread_id = result.thread_id;
		self.event_store = result.event_store;
		self.pending_requests.clear();
	}

	/// Resume an existing codex thread by ID (mutates self).
	pub async fn resume_thread(&mut self, thread_id: &str) -> anyhow::Result<()> {
		let result =
			Self::resume_thread_rpc(&self.handle, &self.config, self.thread_params_mode, thread_id)
				.await?;
		self.apply_resume_fork(result);
		Ok(())
	}

	/// Fork an existing codex thread into a new thread (mutates self).
	pub async fn fork_thread(&mut self, thread_id: &str) -> anyhow::Result<()> {
		let result =
			Self::fork_thread_rpc(&self.handle, &self.config, self.thread_params_mode, thread_id).await?;
		self.apply_resume_fork(result);
		Ok(())
	}

	/// Resolve a pending server request (approve/deny).
	pub async fn resolve_server_request(
		&self,
		request_id: codex_app_server_protocol::RequestId,
		result: serde_json::Value,
	) -> std::io::Result<()> {
		self.handle.resolve_server_request(request_id, result).await
	}

	/// Reject a pending server request with an error message.
	pub async fn reject_server_request(
		&self,
		request_id: codex_app_server_protocol::RequestId,
		message: &str,
	) -> std::io::Result<()> {
		self
			.handle
			.reject_server_request(
				request_id,
				codex_app_server_protocol::JSONRPCErrorError {
					code: -32000,
					message: message.to_string(),
					data: None,
				},
			)
			.await
	}

	pub fn request_handle(&self) -> InProcessAppServerRequestHandle {
		self.handle.clone()
	}

	pub fn thread_id(&self) -> &str {
		&self.thread_id
	}

	pub fn config(&self) -> &Config {
		&self.config
	}

	pub(crate) fn thread_params_mode(&self) -> ThreadParamsMode {
		self.thread_params_mode
	}

	pub fn next_request_id(&self) -> i64 {
		self.next_request_id.fetch_add(1, Ordering::Relaxed)
	}

	pub(crate) fn session_state(&self) -> Option<&ThreadSessionState> {
		self.event_store.session.as_ref()
	}

	pub(crate) fn pending_requests(&self) -> &PendingAppServerRequests {
		&self.pending_requests
	}

	pub(crate) fn pending_requests_mut(&mut self) -> &mut PendingAppServerRequests {
		&mut self.pending_requests
	}

	pub(crate) fn event_store(&self) -> &ThreadEventStore {
		&self.event_store
	}

	fn map_notification_to_bridge(&self, notif: ServerNotification) -> BridgeEvent {
		match notif {
			ServerNotification::AgentMessageDelta(d) => BridgeEvent::AgentMessageDelta { delta: d.delta },
			ServerNotification::ReasoningTextDelta(d) => {
				BridgeEvent::ReasoningTextDelta { delta: d.delta }
			}
			ServerNotification::ReasoningSummaryTextDelta(d) => {
				BridgeEvent::ReasoningSummaryTextDelta { delta: d.delta }
			}
			ServerNotification::PlanDelta(d) => BridgeEvent::PlanDelta { delta: d.delta },
			ServerNotification::TurnStarted(t) => BridgeEvent::TurnStarted { turn_id: t.turn.id },
			ServerNotification::TurnCompleted(t) => BridgeEvent::TurnCompleted { turn_id: t.turn.id },
			ServerNotification::ItemStarted(i) => BridgeEvent::ItemStarted { item: i.item },
			ServerNotification::ItemCompleted(i) => BridgeEvent::ItemCompleted { item: i.item },
			ServerNotification::CommandExecutionOutputDelta(d) => {
				BridgeEvent::CommandExecutionOutputDelta { delta: d.delta }
			}
			ServerNotification::FileChangeOutputDelta(d) => {
				BridgeEvent::FileChangeOutputDelta { delta: d.delta }
			}
			ServerNotification::TerminalInteraction(t) => {
				BridgeEvent::TerminalInteraction { process_id: t.process_id, stdin: Some(t.stdin) }
			}
			ServerNotification::ThreadNameUpdated(n) => {
				BridgeEvent::ThreadNameUpdated { name: n.thread_name }
			}
			ServerNotification::ThreadTokenUsageUpdated(u) => {
				BridgeEvent::ThreadTokenUsageUpdated { usage: u.token_usage }
			}
			ServerNotification::ThreadSettingsUpdated(s) => {
				BridgeEvent::ThreadSettingsUpdated { settings: s.thread_settings }
			}
			ServerNotification::HookStarted(_) => BridgeEvent::HookStarted,
			ServerNotification::HookCompleted(_) => BridgeEvent::HookCompleted,
			ServerNotification::Warning(w) => BridgeEvent::Warning { message: w.message },
			ServerNotification::Error(e) => {
				BridgeEvent::ServerError { message: e.error.message, thread_id: e.thread_id }
			}
			_ => BridgeEvent::Ignored,
		}
	}
}
