//! Model Context Protocol (MCP) client implementation.

#![allow(dead_code)]
//!
//! Supports stdio-based MCP servers with JSON-RPC 2.0 messaging,
//! tool/resource/prompt discovery, and hot-reload via config file watching.

use std::{
	collections::HashMap,
	path::PathBuf,
	sync::{Arc, OnceLock, atomic::AtomicU64},
};

use anyhow::{Context, Result};
use notify::Watcher as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
	io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
	process::{Child, ChildStdin, ChildStdout, Command},
	sync::{Mutex, RwLock},
};

// ── Error types ──────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum McpError {
	#[error("JSON-RPC error {code}: {message}")]
	JsonRpc { code: i32, message: String, data: Option<Value> },
	#[error("Server not connected: {0}")]
	NotConnected(String),
	#[error("Transport error: {0}")]
	Transport(String),
	#[error("Request timed out: {0}")]
	Timeout(String),
	#[error("Server error: {0}")]
	Server(String),
	#[error("Serialization error: {0}")]
	Serialization(#[from] serde_json::Error),
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
}

// ── JSON-RPC 2.0 wire types ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcRequest {
	jsonrpc: String,
	id: Value,
	method: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcResponse {
	jsonrpc: String,
	id: Value,
	#[serde(skip_serializing_if = "Option::is_none")]
	result: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	error: Option<JsonRpcErrorBody>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcErrorBody {
	code: i32,
	message: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcNotification {
	jsonrpc: String,
	method: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	params: Option<Value>,
}

// ── MCP protocol types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpCapabilities {
	#[serde(default)]
	pub tools: bool,
	#[serde(default)]
	pub resources: bool,
	#[serde(default)]
	pub prompts: bool,
	#[serde(default)]
	pub logging: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
	pub name: String,
	#[serde(default)]
	pub description: String,
	#[serde(default)]
	pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
	pub uri: String,
	pub name: String,
	#[serde(default)]
	pub description: String,
	#[serde(default)]
	pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceContent {
	pub uri: String,
	#[serde(default)]
	pub mime_type: String,
	#[serde(default)]
	pub text: String,
	#[serde(default)]
	pub blob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptArgument {
	pub name: String,
	#[serde(default)]
	pub description: String,
	#[serde(default)]
	pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
	pub name: String,
	#[serde(default)]
	pub description: String,
	#[serde(default)]
	pub arguments: Vec<McpPromptArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptMessage {
	pub role: String,
	pub content: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum McpTransportType {
	#[default]
	Stdio,
	Sse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
	#[serde(default)]
	pub id: String,
	pub name: String,
	#[serde(default)]
	pub transport: McpTransportType,
	pub command: String,
	#[serde(default)]
	pub args: Vec<String>,
	#[serde(default)]
	pub env: HashMap<String, String>,
	#[serde(default = "default_enabled")]
	pub enabled: bool,
}

fn default_enabled() -> bool {
	true
}

#[derive(Debug, Clone)]
pub struct McpServerStatus {
	pub config: McpServerConfig,
	pub connected: bool,
	pub capabilities: McpCapabilities,
	pub tools: Vec<McpTool>,
	pub resources: Vec<McpResource>,
	pub prompts: Vec<McpPrompt>,
	pub error: Option<String>,
}

// ── Config file format ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct McpConfigFile {
	#[serde(default)]
	mcp_servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpServerEntry {
	#[serde(default)]
	transport: McpTransportType,
	command: String,
	#[serde(default)]
	args: Vec<String>,
	#[serde(default)]
	env: HashMap<String, String>,
	#[serde(default = "default_enabled")]
	enabled: bool,
}

// ── Stdio connection ─────────────────────────────────────────────────────

/// Owns the child process and its stdio handles for a single MCP server.
struct StdioConnection {
	stdin: ChildStdin,
	reader: BufReader<ChildStdout>,
	child: Child,
}

impl StdioConnection {
	async fn spawn(config: &McpServerConfig) -> Result<Self> {
		let mut cmd = Command::new(&config.command);
		cmd
			.args(&config.args)
			.stdin(std::process::Stdio::piped())
			.stdout(std::process::Stdio::piped())
			.stderr(std::process::Stdio::piped())
			.kill_on_drop(true);

		for (k, v) in &config.env {
			cmd.env(k, v);
		}

		let mut child =
			cmd.spawn().with_context(|| format!("Failed to spawn MCP server: {}", config.command))?;

		let stdin = child.stdin.take().context("Failed to capture MCP server stdin")?;
		let stdout = child.stdout.take().context("Failed to capture MCP server stdout")?;

		Ok(Self { stdin, reader: BufReader::new(stdout), child })
	}

	async fn send_request(&mut self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
		let json = serde_json::to_string(request)?;
		self.stdin.write_all(json.as_bytes()).await.context("Failed to write to MCP server stdin")?;
		self.stdin.write_all(b"\n").await.context("Failed to write newline to MCP server stdin")?;
		self.stdin.flush().await.context("Failed to flush MCP server stdin")?;

		let mut line = String::new();
		self.reader.read_line(&mut line).await.context("Failed to read from MCP server stdout")?;

		if line.is_empty() {
			anyhow::bail!("MCP server closed stdout unexpectedly");
		}

		let response: JsonRpcResponse =
			serde_json::from_str(line.trim()).context("Failed to parse MCP server response")?;
		Ok(response)
	}

	async fn send_notification(&mut self, notification: &JsonRpcNotification) -> Result<()> {
		let json = serde_json::to_string(notification)?;
		self
			.stdin
			.write_all(json.as_bytes())
			.await
			.context("Failed to write notification to MCP server stdin")?;
		self.stdin.write_all(b"\n").await.context("Failed to write newline for notification")?;
		self.stdin.flush().await.context("Failed to flush notification")?;
		Ok(())
	}

	async fn try_read_stderr(&mut self) -> String {
		if let Some(ref mut stderr) = self.child.stderr {
			let mut buf = String::new();
			let _ = stderr.read_to_string(&mut buf).await;
			buf
		} else {
			String::new()
		}
	}
}

// ── MCP Client ───────────────────────────────────────────────────────────

/// A connected MCP client for a single server.
pub struct McpClient {
	config: McpServerConfig,
	conn: Arc<Mutex<Option<StdioConnection>>>,
	capabilities: Arc<RwLock<McpCapabilities>>,
	tools: Arc<RwLock<Vec<McpTool>>>,
	resources: Arc<RwLock<Vec<McpResource>>>,
	prompts: Arc<RwLock<Vec<McpPrompt>>>,
	next_id: AtomicU64,
}

impl McpClient {
	/// Connect to an MCP server via stdio transport and perform initialization handshake.
	pub async fn connect(config: McpServerConfig) -> Result<Arc<Self>> {
		let config_name = config.name.clone();
		let conn = StdioConnection::spawn(&config).await?;

		let client = Arc::new(Self {
			config,
			conn: Arc::new(Mutex::new(Some(conn))),
			capabilities: Arc::new(RwLock::new(McpCapabilities::default())),
			tools: Arc::new(RwLock::new(Vec::new())),
			resources: Arc::new(RwLock::new(Vec::new())),
			prompts: Arc::new(RwLock::new(Vec::new())),
			next_id: AtomicU64::new(1),
		});

		// Initialize handshake
		let result = client
			.send_request(
				"initialize",
				Some(json!({
						"protocolVersion": "2024-11-05",
						"capabilities": {},
						"clientInfo": {
								"name": "dx-tui",
								"version": env!("CARGO_PKG_VERSION")
						}
				})),
			)
			.await?;

		// Parse server capabilities from initialize response
		if let Some(server_caps) = result.get("capabilities").and_then(|c| c.as_object()) {
			let mut caps = client.capabilities.write().await;
			caps.tools = server_caps.contains_key("tools");
			caps.resources = server_caps.contains_key("resources");
			caps.prompts = server_caps.contains_key("prompts");
			caps.logging = server_caps.contains_key("logging");
		}

		// Send initialized notification
		client.send_notification("notifications/initialized", None).await?;

		tracing::info!(name = %config_name, "MCP server connected");

		Ok(client)
	}

	/// The server id from config.
	pub fn id(&self) -> &str {
		&self.config.id
	}

	/// The server display name.
	pub fn name(&self) -> &str {
		&self.config.name
	}

	/// The server config.
	pub fn config(&self) -> &McpServerConfig {
		&self.config
	}

	/// Discover available tools from this server.
	pub async fn discover_tools(&self) -> Result<()> {
		let result = self.send_request("tools/list", None).await?;
		let tools: Vec<McpTool> =
			result.get("tools").and_then(|t| serde_json::from_value(t.clone()).ok()).unwrap_or_default();

		*self.tools.write().await = tools;
		Ok(())
	}

	/// Execute a tool on this server.
	pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
		self
			.send_request(
				"tools/call",
				Some(json!({
						"name": name,
						"arguments": arguments
				})),
			)
			.await
	}

	/// Cached tools from last discovery.
	pub async fn tools(&self) -> Vec<McpTool> {
		self.tools.read().await.clone()
	}

	/// Discover available resources.
	pub async fn discover_resources(&self) -> Result<()> {
		let result = self.send_request("resources/list", None).await?;
		let resources: Vec<McpResource> = result
			.get("resources")
			.and_then(|r| serde_json::from_value(r.clone()).ok())
			.unwrap_or_default();

		*self.resources.write().await = resources;
		Ok(())
	}

	/// Read a specific resource by URI.
	pub async fn read_resource(&self, uri: &str) -> Result<Vec<McpResourceContent>> {
		let result = self.send_request("resources/read", Some(json!({ "uri": uri }))).await?;
		let contents: Vec<McpResourceContent> = result
			.get("contents")
			.and_then(|c| serde_json::from_value(c.clone()).ok())
			.unwrap_or_default();
		Ok(contents)
	}

	/// Cached resources.
	pub async fn resources(&self) -> Vec<McpResource> {
		self.resources.read().await.clone()
	}

	/// Discover available prompts.
	pub async fn discover_prompts(&self) -> Result<()> {
		let result = self.send_request("prompts/list", None).await?;
		let prompts: Vec<McpPrompt> = result
			.get("prompts")
			.and_then(|p| serde_json::from_value(p.clone()).ok())
			.unwrap_or_default();

		*self.prompts.write().await = prompts;
		Ok(())
	}

	/// Get a specific prompt with arguments.
	pub async fn get_prompt(
		&self,
		name: &str,
		args: HashMap<String, String>,
	) -> Result<Vec<McpPromptMessage>> {
		let result = self
			.send_request(
				"prompts/get",
				Some(json!({
						"name": name,
						"arguments": args
				})),
			)
			.await?;
		let messages: Vec<McpPromptMessage> = result
			.get("messages")
			.and_then(|m| serde_json::from_value(m.clone()).ok())
			.unwrap_or_default();
		Ok(messages)
	}

	/// Cached prompts.
	pub async fn prompts(&self) -> Vec<McpPrompt> {
		self.prompts.read().await.clone()
	}

	/// Current server capabilities.
	pub async fn capabilities(&self) -> McpCapabilities {
		self.capabilities.read().await.clone()
	}

	/// Full status snapshot.
	pub async fn status(&self) -> McpServerStatus {
		let mut conn = self.conn.lock().await;
		let connected = conn.is_some();
		let stderr = if let Some(c) = conn.as_mut() {
			let s = c.try_read_stderr().await;
			if s.is_empty() { None } else { Some(s) }
		} else {
			None
		};
		drop(conn);

		// Batch-read all cached data under a single read lock
		let (tools, resources, prompts, capabilities) = {
			let t = self.tools.read().await;
			let r = self.resources.read().await;
			let p = self.prompts.read().await;
			let c = self.capabilities.read().await;
			(t.clone(), r.clone(), p.clone(), c.clone())
		};

		McpServerStatus {
			config: self.config.clone(),
			connected,
			capabilities,
			tools,
			resources,
			prompts,
			error: stderr,
		}
	}

	/// Disconnect and kill the server process.
	pub async fn disconnect(&self) {
		let mut conn = self.conn.lock().await;
		if let Some(mut c) = conn.take() {
			let _ = c.child.start_kill();
			tracing::info!(name = %self.config.name, "MCP server disconnected");
		}
	}

	/// Check if the server is still connected (process alive).
	pub async fn is_connected(&self) -> bool {
		let mut conn = self.conn.lock().await;
		conn.as_mut().is_some_and(|c| c.child.try_wait().ok().map(|s| s.is_none()).unwrap_or(false))
	}

	// ── Internal JSON-RPC helpers ────────────────────────────────────

	async fn send_request(&self, method: &str, params: Option<Value>) -> Result<Value> {
		let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

		let request = JsonRpcRequest {
			jsonrpc: "2.0".into(),
			id: Value::Number(serde_json::Number::from(id)),
			method: method.into(),
			params,
		};

		let response = {
			let mut conn_lock = self.conn.lock().await;
			let conn = conn_lock
				.as_mut()
				.ok_or_else(|| anyhow::anyhow!("MCP server {} is not connected", self.config.name))?;
			conn.send_request(&request).await?
		};

		if let Some(err) = response.error {
			anyhow::bail!(McpError::JsonRpc { code: err.code, message: err.message, data: err.data });
		}

		Ok(response.result.unwrap_or(json!(null)))
	}

	async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()> {
		let notification = JsonRpcNotification { jsonrpc: "2.0".into(), method: method.into(), params };

		let mut conn_lock = self.conn.lock().await;
		let conn = conn_lock
			.as_mut()
			.ok_or_else(|| anyhow::anyhow!("MCP server {} is not connected", self.config.name))?;
		conn.send_notification(&notification).await?;
		Ok(())
	}
}

// ── MCP Registry ─────────────────────────────────────────────────────────

type ServerMap = HashMap<String, Arc<McpClient>>;

/// Manages a collection of connected MCP servers with config persistence.
pub struct McpRegistry {
	servers: Arc<RwLock<ServerMap>>,
	config_path: PathBuf,
	config_dir: PathBuf,
}

impl McpRegistry {
	/// Create a new registry from the default config path `~/.config/dx/mcp.toml`.
	pub async fn new() -> Result<Arc<Self>> {
		let home = dirs::home_dir().context("Cannot determine home directory")?;
		let config_dir = home.join(".config/dx");
		let config_path = config_dir.join("mcp.toml");
		Ok(Arc::new(Self::new_with_path(config_path, config_dir).await?))
	}

	/// Create a registry with a specific config path.
	pub async fn new_with_path(config_path: PathBuf, config_dir: PathBuf) -> Result<Self> {
		let registry =
			Self { servers: Arc::new(RwLock::new(ServerMap::new())), config_path, config_dir };

		// Ensure config directory exists
		if let Err(e) = tokio::fs::create_dir_all(&registry.config_dir).await {
			tracing::warn!("Failed to create MCP config directory: {e}");
		}

		Ok(registry)
	}

	/// Load config and connect all enabled servers.
	pub async fn load_and_connect(&self) -> Result<()> {
		let configs = self.load_config().await.unwrap_or_default();
		for cfg in configs {
			if !cfg.enabled {
				continue;
			}
			if let Err(e) = self.connect_server(cfg).await {
				tracing::warn!("Failed to connect MCP server: {e}");
			}
		}
		Ok(())
	}

	/// Load server configurations from the TOML file.
	pub async fn load_config(&self) -> Result<Vec<McpServerConfig>> {
		let text = match tokio::fs::read_to_string(&self.config_path).await {
			Ok(t) => t,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
				return Ok(Vec::new());
			}
			Err(e) => anyhow::bail!("Failed to read MCP config: {e}"),
		};

		let parsed: McpConfigFile = toml::from_str(&text).context("Failed to parse MCP config file")?;

		Ok(
			parsed
				.mcp_servers
				.into_iter()
				.map(|(name, entry)| {
					let id = name.clone();
					McpServerConfig {
						id,
						name,
						transport: entry.transport,
						command: entry.command,
						args: entry.args,
						env: entry.env,
						enabled: entry.enabled,
					}
				})
				.collect(),
		)
	}

	/// Save current server configurations to the TOML file.
	pub async fn save_config(&self, configs: &[McpServerConfig]) -> Result<()> {
		let mut map = HashMap::new();
		for cfg in configs {
			map.insert(
				cfg.name.clone(),
				McpServerEntry {
					transport: cfg.transport.clone(),
					command: cfg.command.clone(),
					args: cfg.args.clone(),
					env: cfg.env.clone(),
					enabled: cfg.enabled,
				},
			);
		}
		let file = McpConfigFile { mcp_servers: map };

		let toml_str = toml::to_string_pretty(&file)?;
		tokio::fs::write(&self.config_path, toml_str.as_bytes()).await?;
		Ok(())
	}

	/// Connect to a server and add it to the registry.
	pub async fn connect_server(&self, config: McpServerConfig) -> Result<Arc<McpClient>> {
		let server_id = config.id.clone();
		let server_name = config.name.clone();
		let client = McpClient::connect(config).await?;

		// Discover tools/resources/prompts
		if client.capabilities().await.tools
			&& let Err(e) = client.discover_tools().await
		{
			tracing::warn!("Failed to discover tools for {}: {e}", client.name());
		}
		if client.capabilities().await.resources
			&& let Err(e) = client.discover_resources().await
		{
			tracing::warn!("Failed to discover resources for {}: {e}", client.name());
		}
		if client.capabilities().await.prompts
			&& let Err(e) = client.discover_prompts().await
		{
			tracing::warn!("Failed to discover prompts for {}: {e}", client.name());
		}

		self.servers.write().await.insert(server_id, client.clone());
		tracing::info!(name = %server_name, "MCP server registered");
		Ok(client)
	}

	/// Remove and disconnect a server by id.
	pub async fn remove_server(&self, id: &str) {
		if let Some(client) = self.servers.write().await.remove(id) {
			client.disconnect().await;
			tracing::info!(name = %id, "MCP server removed");
		}
	}

	/// Get a connected client by server id.
	pub async fn get_client(&self, id: &str) -> Option<Arc<McpClient>> {
		self.servers.read().await.get(id).cloned()
	}

	/// Get all connected clients.
	pub async fn all_clients(&self) -> Vec<Arc<McpClient>> {
		self.servers.read().await.values().cloned().collect()
	}

	/// Get status of all registered servers.
	pub async fn all_status(&self) -> Vec<McpServerStatus> {
		let mut statuses = Vec::new();
		for client in self.all_clients().await {
			statuses.push(client.status().await);
		}
		statuses
	}

	/// Find which server provides a given tool. Returns (server_id, client).
	pub async fn find_tool_provider(&self, tool_name: &str) -> Option<(String, Arc<McpClient>)> {
		for client in self.all_clients().await {
			let tools = client.tools().await;
			if tools.iter().any(|t| t.name == tool_name) {
				return Some((client.id().to_string(), client));
			}
		}
		None
	}

	/// Check if a tool name is provided by any MCP server.
	pub async fn is_mcp_tool(&self, tool_name: &str) -> bool {
		self.find_tool_provider(tool_name).await.is_some()
	}

	/// Execute an MCP tool by name across all servers.
	pub async fn execute_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
		let (_server_id, client) = self
			.find_tool_provider(tool_name)
			.await
			.ok_or_else(|| anyhow::anyhow!("No MCP server provides tool `{tool_name}`"))?;
		client.call_tool(tool_name, arguments).await
	}

	/// Collect all tool definitions from all connected servers.
	pub async fn all_tool_defs(&self) -> Vec<McpTool> {
		let mut all = Vec::new();
		for client in self.all_clients().await {
			all.extend(client.tools().await);
		}
		all
	}

	/// Disconnect all servers.
	pub async fn disconnect_all(&self) {
		let mut servers = self.servers.write().await;
		for (_id, client) in servers.drain() {
			client.disconnect().await;
		}
	}

	/// The config file path.
	pub fn config_path(&self) -> &PathBuf {
		&self.config_path
	}

	/// The config directory.
	pub fn config_dir(&self) -> &PathBuf {
		&self.config_dir
	}
}

// ── Global MCP Registry ─────────────────────────────────────────────────

static GLOBAL_MCP_REGISTRY: OnceLock<Arc<McpRegistry>> = OnceLock::new();

/// Initialize the global MCP registry. Should be called once at startup from an async context.
pub async fn init_global_registry() -> Result<&'static Arc<McpRegistry>> {
	let reg = McpRegistry::new().await?;
	reg.load_and_connect().await?;
	Ok(GLOBAL_MCP_REGISTRY.get_or_init(move || reg))
}

/// Get the global MCP registry if initialized.
pub fn global_registry() -> Option<&'static Arc<McpRegistry>> {
	GLOBAL_MCP_REGISTRY.get()
}

// ── Hot-reload watcher ───────────────────────────────────────────────────

/// Watches the MCP config file for changes and triggers a reload callback.
pub struct McpConfigWatcher {
	_guard: notify::RecommendedWatcher,
}

impl McpConfigWatcher {
	/// Start watching the config file. Calls `on_change` when the file changes.
	pub fn spawn(
		registry: Arc<McpRegistry>,
		mut on_change: impl FnMut() + Send + 'static,
	) -> Result<Self> {
		let watch_path = registry.config_path().clone();
		let watch_dir = registry.config_dir().clone();

		let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
			if let Ok(event) = event
				&& event.paths.iter().any(|p| p.ends_with(&watch_path) || p.ends_with(&watch_dir))
			{
				on_change();
			}
		})?;

		// Watch both the config file and the directory (for creation/deletion)
		if registry.config_path().exists() {
			watcher.watch(registry.config_path(), notify::RecursiveMode::NonRecursive)?;
		}
		if registry.config_dir().exists() {
			watcher.watch(registry.config_dir(), notify::RecursiveMode::NonRecursive)?;
		}

		Ok(Self { _guard: watcher })
	}
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_config_roundtrip() {
		let configs = vec![
			McpServerConfig {
				id: "fs".into(),
				name: "filesystem".into(),
				transport: McpTransportType::Stdio,
				command: "npx".into(),
				args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into(), "/tmp".into()],
				env: HashMap::new(),
				enabled: true,
			},
			McpServerConfig {
				id: "gh".into(),
				name: "github".into(),
				transport: McpTransportType::Stdio,
				command: "npx".into(),
				args: vec!["-y".into(), "@modelcontextprotocol/server-github".into()],
				env: [("GITHUB_TOKEN".into(), "test-token".into())].into_iter().collect(),
				enabled: false,
			},
		];

		let mut map = HashMap::new();
		for cfg in &configs {
			map.insert(
				cfg.name.clone(),
				McpServerEntry {
					transport: cfg.transport.clone(),
					command: cfg.command.clone(),
					args: cfg.args.clone(),
					env: cfg.env.clone(),
					enabled: cfg.enabled,
				},
			);
		}
		let file = McpConfigFile { mcp_servers: map };

		let toml_str = toml::to_string_pretty(&file).expect("serialize");
		let parsed: McpConfigFile = toml::from_str(&toml_str).expect("deserialize");

		assert_eq!(parsed.mcp_servers.len(), 2);
		let fs_entry = parsed.mcp_servers.get("filesystem").expect("filesystem entry");
		assert_eq!(fs_entry.command, "npx");
		assert!(fs_entry.enabled);

		let gh_entry = parsed.mcp_servers.get("github").expect("github entry");
		assert!(!gh_entry.enabled);
		assert_eq!(gh_entry.env.get("GITHUB_TOKEN").unwrap(), "test-token");
	}

	#[test]
	fn test_json_rpc_roundtrip() {
		let req = JsonRpcRequest {
			jsonrpc: "2.0".into(),
			id: json!(1),
			method: "tools/list".into(),
			params: None,
		};
		let json = serde_json::to_string(&req).expect("serialize");
		assert!(json.contains("\"jsonrpc\":\"2.0\""));
		assert!(json.contains("\"method\":\"tools/list\""));
	}

	#[test]
	fn test_mcp_tool_serde() {
		let tool = McpTool {
			name: "read_file".into(),
			description: "Read a file from the filesystem".into(),
			input_schema: json!({
					"type": "object",
					"properties": {
							"path": { "type": "string" }
					},
					"required": ["path"]
			}),
		};
		let json = serde_json::to_string(&tool).expect("serialize");
		let parsed: McpTool = serde_json::from_str(&json).expect("deserialize");
		assert_eq!(parsed.name, "read_file");
		assert!(parsed.input_schema.get("required").is_some());
	}

	#[test]
	fn test_mcp_error_display() {
		let err = McpError::JsonRpc { code: -32601, message: "Method not found".into(), data: None };
		let msg = err.to_string();
		assert!(msg.contains("-32601"));
		assert!(msg.contains("Method not found"));
	}

	#[test]
	fn test_transport_type_default() {
		assert_eq!(McpTransportType::default(), McpTransportType::Stdio);
	}

	#[test]
	fn test_transport_type_serde() {
		let json = serde_json::to_string(&McpTransportType::Stdio).unwrap();
		assert_eq!(json, "\"stdio\"");
		let json = serde_json::to_string(&McpTransportType::Sse).unwrap();
		assert_eq!(json, "\"sse\"");
	}

	#[test]
	fn test_mcp_capabilities_default() {
		let caps = McpCapabilities::default();
		assert!(!caps.tools);
		assert!(!caps.resources);
		assert!(!caps.prompts);
		assert!(!caps.logging);
	}

	#[test]
	fn test_resource_content_serde() {
		let content = McpResourceContent {
			uri: "file:///test.txt".into(),
			mime_type: "text/plain".into(),
			text: "hello".into(),
			blob: None,
		};
		let json = serde_json::to_string(&content).unwrap();
		let parsed: McpResourceContent = serde_json::from_str(&json).unwrap();
		assert_eq!(parsed.uri, "file:///test.txt");
		assert_eq!(parsed.text, "hello");
	}

	#[test]
	fn test_prompt_argument_serde() {
		let arg = McpPromptArgument {
			name: "topic".into(),
			description: "The topic to discuss".into(),
			required: true,
		};
		let json = serde_json::to_string(&arg).unwrap();
		let parsed: McpPromptArgument = serde_json::from_str(&json).unwrap();
		assert!(parsed.required);
		assert_eq!(parsed.name, "topic");
	}
}
