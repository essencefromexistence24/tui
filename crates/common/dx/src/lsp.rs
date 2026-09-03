//! LSP (Language Server Protocol) client for dx-tui.

#![allow(dead_code)]
//!
//! Provides JSON-RPC 2.0 transport over stdio for LSP servers,
//! with auto-discovery, lifecycle management, and thread-safe access.

use std::{
	collections::{HashMap, HashSet},
	path::{Path, PathBuf},
	sync::{
		Arc, OnceLock,
		atomic::{AtomicBool, AtomicU64, Ordering},
	},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::{
	io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
	process::{Child, ChildStdin, ChildStdout, Command},
	sync::{Mutex, broadcast, oneshot},
	task::JoinHandle,
};

// ── Error ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum LspError {
	#[error("JSON-RPC error ({code}): {message}")]
	JsonRpc { code: i64, message: String, data: Option<Value> },
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
	#[error("JSON error: {0}")]
	Json(#[from] serde_json::Error),
	#[error("Server not started for language `{0}`")]
	NotStarted(String),
	#[error("Server already started for `{0}`")]
	AlreadyStarted(String),
	#[error("No known LSP server for language `{0}`")]
	ServerNotFound(String),
	#[error("Timed out waiting for server response")]
	Timeout,
	#[error("Server exited unexpectedly: {0}")]
	ServerExited(String),
	#[error("Channel closed before response received")]
	ChannelClosed,
	#[error("{0}")]
	Other(String),
}

pub type LspResult<T> = Result<T, LspError>;

// ── LSP data types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspPosition {
	pub line: u32,
	pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspRange {
	pub start: LspPosition,
	pub end: LspPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspLocation {
	pub uri: String,
	pub range: LspRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspHover {
	#[serde(default)]
	pub contents: LspMarkupContent,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub range: Option<LspRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspMarkupContent {
	Plain(String),
	Markup { kind: String, value: String },
	Rich(Vec<LspMarkedString>),
}

impl Default for LspMarkupContent {
	fn default() -> Self {
		Self::Plain(String::new())
	}
}

impl std::fmt::Display for LspMarkupContent {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Plain(s) => f.write_str(s),
			Self::Markup { value, .. } => f.write_str(value),
			Self::Rich(items) => {
				for item in items {
					write!(f, "{item}")?;
				}
				Ok(())
			}
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LspMarkedString {
	Plain(String),
	Language { language: String, value: String },
}

impl std::fmt::Display for LspMarkedString {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Plain(s) => f.write_str(s),
			Self::Language { value, .. } => f.write_str(value),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspSymbol {
	pub name: String,
	pub kind: u32,
	#[serde(default)]
	pub detail: Option<String>,
	pub range: LspRange,
	pub selection_range: LspRange,
	#[serde(default)]
	pub children: Vec<LspSymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDiagnostic {
	pub range: LspRange,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub severity: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub code: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub source: Option<String>,
	pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspCompletionItem {
	pub label: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub kind: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub detail: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub documentation: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub insert_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspCallHierarchyItem {
	pub name: String,
	pub kind: u32,
	#[serde(default)]
	pub detail: Option<String>,
	pub uri: String,
	pub range: LspRange,
	pub selection_range: LspRange,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspCallHierarchyCall {
	pub from: LspCallHierarchyItem,
	pub from_ranges: Vec<LspRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspTextEdit {
	pub range: LspRange,
	pub new_text: String,
}

#[derive(Debug, Clone)]
pub struct LspDiagnosticNotification {
	pub uri: String,
	pub diagnostics: Vec<LspDiagnostic>,
}

// ── LspServer config ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LspServer {
	pub id: String,
	pub language_id: String,
	pub command: String,
	pub args: Vec<String>,
	pub root_uri: String,
}

impl LspServer {
	pub fn new(
		id: impl Into<String>,
		language_id: impl Into<String>,
		command: impl Into<String>,
	) -> Self {
		Self {
			id: id.into(),
			language_id: language_id.into(),
			command: command.into(),
			args: Vec::new(),
			root_uri: String::new(),
		}
	}

	pub fn with_args(mut self, args: Vec<String>) -> Self {
		self.args = args;
		self
	}

	pub fn with_root_uri(mut self, root_uri: impl Into<String>) -> Self {
		self.root_uri = root_uri.into();
		self
	}
}

// ── LspClient ───────────────────────────────────────────────────────────────

pub struct LspClient {
	server: LspServer,
	process: Option<Child>,
	writer: Arc<Mutex<BufWriter<ChildStdin>>>,
	pending: Arc<Mutex<HashMap<u64, oneshot::Sender<LspResult<Value>>>>>,
	diagnostics_tx: broadcast::Sender<LspDiagnosticNotification>,
	capabilities: Arc<Mutex<Value>>,
	open_docs: Arc<Mutex<HashSet<String>>>,
	next_id: AtomicU64,
	initialized: AtomicBool,
	_reader_handle: JoinHandle<()>,
}

impl Drop for LspClient {
	fn drop(&mut self) {
		let id = self.next_id.fetch_add(1, Ordering::SeqCst);
		let server = self.server.clone();
		let writer = self.writer.clone();
		let pending = self.pending.clone();
		tokio::spawn(async move {
			shutdown_inner(id, &server, &writer, &pending).await;
		});
	}
}

async fn shutdown_inner(
	id: u64,
	_server: &LspServer,
	writer: &Mutex<BufWriter<ChildStdin>>,
	pending: &Mutex<HashMap<u64, oneshot::Sender<LspResult<Value>>>>,
) {
	let shutdown = json!({
		"jsonrpc": "2.0",
		"id": id,
		"method": "shutdown",
	});
	let mut w = writer.lock().await;
	let _ = write_message(&mut w, &shutdown).await;
	let _ = w.flush().await;

	let exit = json!({
		"jsonrpc": "2.0",
		"method": "exit",
	});
	let _ = write_message(&mut w, &exit).await;
	let _ = w.flush().await;

	pending.lock().await.clear();
}

impl LspClient {
	pub async fn connect(server: LspServer) -> LspResult<Self> {
		let mut cmd = Command::new(&server.command);
		cmd
			.args(&server.args)
			.stdin(std::process::Stdio::piped())
			.stdout(std::process::Stdio::piped())
			.stderr(std::process::Stdio::inherit());

		if !server.root_uri.is_empty() {
			cmd.env("LSP_ROOT_URI", &server.root_uri);
		}

		let mut child = cmd
			.spawn()
			.map_err(|e| LspError::Other(format!("Failed to spawn {}: {e}", server.command)))?;

		let stdin =
			child.stdin.take().ok_or_else(|| LspError::Other("Failed to capture stdin".into()))?;
		let stdout =
			child.stdout.take().ok_or_else(|| LspError::Other("Failed to capture stdout".into()))?;

		let writer = Arc::new(Mutex::new(BufWriter::new(stdin)));
		let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<LspResult<Value>>>>> =
			Arc::new(Mutex::new(HashMap::new()));
		let diagnostics_tx: broadcast::Sender<LspDiagnosticNotification> = broadcast::channel(256).0;
		let capabilities = Arc::new(Mutex::new(Value::Null));
		let open_docs = Arc::new(Mutex::new(HashSet::new()));

		let p_pending = Arc::clone(&pending);
		let p_diagnostics_tx = diagnostics_tx.clone();
		let p_capabilities = Arc::clone(&capabilities);
		let reader_handle =
			tokio::spawn(reader_task(stdout, p_pending, p_diagnostics_tx, p_capabilities));

		let mut client = Self {
			server: server.clone(),
			process: Some(child),
			writer,
			pending,
			diagnostics_tx,
			capabilities,
			open_docs,
			next_id: AtomicU64::new(1),
			initialized: AtomicBool::new(false),
			_reader_handle: reader_handle,
		};

		client.initialize().await?;

		Ok(client)
	}

	async fn initialize(&mut self) -> LspResult<()> {
		let mut params = json!({
			"processId": std::process::id(),
			"clientInfo": {
				"name": "dx-tui",
				"version": env!("CARGO_PKG_VERSION")
			},
			"capabilities": {
				"textDocument": {
					"hover": { "dynamicRegistration": true },
					"definition": { "dynamicRegistration": true },
					"references": { "dynamicRegistration": true },
					"documentSymbol": { "dynamicRegistration": true },
					"formatting": { "dynamicRegistration": true },
					"completion": {
						"dynamicRegistration": true,
						"completionItem": {
							"documentationFormat": ["plaintext", "markdown"]
						}
					},
					"implementation": { "dynamicRegistration": true },
					"callHierarchy": { "dynamicRegistration": true },
					"diagnostic": { "dynamicRegistration": true }
				},
				"workspace": {
					"symbol": { "dynamicRegistration": true }
				}
			}
		});

		if !self.server.root_uri.is_empty()
			&& let Some(params) = params.as_object_mut()
		{
			// params is &mut Map, insert rootUri
			params.insert("rootUri".into(), Value::String(self.server.root_uri.clone()));
		}

		let resp = self.send_request("initialize", params).await?;

		if let Some(caps) = resp.get("capabilities") {
			*self.capabilities.lock().await = caps.clone();
		}

		let _ = self.send_notification("initialized", json!({})).await;

		self.initialized.store(true, Ordering::SeqCst);

		Ok(())
	}

	pub async fn send_request(&self, method: &str, params: Value) -> LspResult<Value> {
		let id = self.next_id.fetch_add(1, Ordering::SeqCst);
		let msg = json!({
			"jsonrpc": "2.0",
			"id": id,
			"method": method,
			"params": params,
		});

		let (tx, rx) = oneshot::channel();
		{
			self.pending.lock().await.insert(id, tx);
		}

		{
			let mut writer = self.writer.lock().await;
			write_message(&mut writer, &msg).await?;
			writer.flush().await?;
		}

		tokio::time::timeout(std::time::Duration::from_secs(30), rx)
			.await
			.map_err(|_| LspError::Timeout)?
			.map_err(|_| LspError::ChannelClosed)?
	}

	pub async fn send_notification(&self, method: &str, params: Value) -> LspResult<()> {
		let msg = json!({
			"jsonrpc": "2.0",
			"method": method,
			"params": params,
		});
		let mut writer = self.writer.lock().await;
		write_message(&mut writer, &msg).await?;
		writer.flush().await?;
		Ok(())
	}

	pub async fn open_document(&self, uri: &str, text: &str, language_id: &str) -> LspResult<()> {
		let mut open = self.open_docs.lock().await;
		if open.contains(uri) {
			return Ok(());
		}
		open.insert(uri.to_string());
		drop(open);

		let params = json!({
			"textDocument": {
				"uri": uri,
				"languageId": language_id,
				"version": 1,
				"text": text,
			}
		});
		self.send_notification("textDocument/didOpen", params).await
	}

	pub async fn change_document(&self, uri: &str, text: &str, version: i32) -> LspResult<()> {
		let params = json!({
			"textDocument": {
				"uri": uri,
				"version": version,
			},
			"contentChanges": [{
				"text": text,
			}]
		});
		self.send_notification("textDocument/didChange", params).await
	}

	pub async fn close_document(&self, uri: &str) -> LspResult<()> {
		let mut open = self.open_docs.lock().await;
		open.remove(uri);
		drop(open);

		let params = json!({
			"textDocument": { "uri": uri }
		});
		self.send_notification("textDocument/didClose", params).await
	}

	pub async fn shutdown(&self) -> LspResult<()> {
		let id = self.next_id.fetch_add(1, Ordering::SeqCst);
		let params = json!({});
		let msg = json!({
			"jsonrpc": "2.0",
			"id": id,
			"method": "shutdown",
			"params": params,
		});
		let (tx, rx) = oneshot::channel();
		{
			self.pending.lock().await.insert(id, tx);
		}
		{
			let mut writer = self.writer.lock().await;
			write_message(&mut writer, &msg).await?;
			writer.flush().await?;
		}
		let _ = tokio::time::timeout(std::time::Duration::from_secs(10), rx).await;

		let exit = json!({
			"jsonrpc": "2.0",
			"method": "exit",
		});
		{
			let mut writer = self.writer.lock().await;
			write_message(&mut writer, &exit).await.ok();
			writer.flush().await.ok();
		}

		self.initialized.store(false, Ordering::SeqCst);
		Ok(())
	}

	pub fn subscribe_diagnostics(&self) -> broadcast::Receiver<LspDiagnosticNotification> {
		self.diagnostics_tx.subscribe()
	}

	pub fn server_info(&self) -> &LspServer {
		&self.server
	}

	pub fn is_initialized(&self) -> bool {
		self.initialized.load(Ordering::SeqCst)
	}

	// ── LSP Operations ──

	pub async fn go_to_definition(
		&self,
		uri: &str,
		line: u32,
		character: u32,
	) -> LspResult<Vec<LspLocation>> {
		let params = json!({
			"textDocument": { "uri": uri },
			"position": { "line": line, "character": character },
		});
		let resp = self.send_request("textDocument/definition", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn find_references(
		&self,
		uri: &str,
		line: u32,
		character: u32,
		include_declaration: bool,
	) -> LspResult<Vec<LspLocation>> {
		let params = json!({
			"textDocument": { "uri": uri },
			"position": { "line": line, "character": character },
			"context": { "includeDeclaration": include_declaration },
		});
		let resp = self.send_request("textDocument/references", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn hover(&self, uri: &str, line: u32, character: u32) -> LspResult<Option<LspHover>> {
		let params = json!({
			"textDocument": { "uri": uri },
			"position": { "line": line, "character": character },
		});
		let resp = self.send_request("textDocument/hover", params).await?;
		if resp.is_null() {
			return Ok(None);
		}
		serde_json::from_value(resp).map(Some).map_err(LspError::Json)
	}

	pub async fn document_symbols(&self, uri: &str) -> LspResult<Vec<LspSymbol>> {
		let params = json!({
			"textDocument": { "uri": uri },
		});
		let resp = self.send_request("textDocument/documentSymbol", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn workspace_symbols(&self, query: &str) -> LspResult<Vec<LspSymbol>> {
		let params = json!({
			"query": query,
		});
		let resp = self.send_request("workspace/symbol", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn go_to_implementation(
		&self,
		uri: &str,
		line: u32,
		character: u32,
	) -> LspResult<Vec<LspLocation>> {
		let params = json!({
			"textDocument": { "uri": uri },
			"position": { "line": line, "character": character },
		});
		let resp = self.send_request("textDocument/implementation", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn prepare_call_hierarchy(
		&self,
		uri: &str,
		line: u32,
		character: u32,
	) -> LspResult<Vec<LspCallHierarchyItem>> {
		let params = json!({
			"textDocument": { "uri": uri },
			"position": { "line": line, "character": character },
		});
		let resp = self.send_request("textDocument/prepareCallHierarchy", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn call_hierarchy_incoming(
		&self,
		item: &LspCallHierarchyItem,
	) -> LspResult<Vec<LspCallHierarchyCall>> {
		let params = json!(item);
		let resp = self.send_request("textDocument/callHierarchy/incomingCalls", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn call_hierarchy_outgoing(
		&self,
		item: &LspCallHierarchyItem,
	) -> LspResult<Vec<LspCallHierarchyCall>> {
		let params = json!(item);
		let resp = self.send_request("textDocument/callHierarchy/outgoingCalls", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn format_document(&self, uri: &str) -> LspResult<Vec<LspTextEdit>> {
		let params = json!({
			"textDocument": { "uri": uri },
			"options": {
				"tabSize": 4,
				"insertSpaces": true,
			},
		});
		let resp = self.send_request("textDocument/formatting", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn get_diagnostics(&self, uri: &str) -> LspResult<Vec<LspDiagnostic>> {
		let params = json!({
			"textDocument": { "uri": uri },
		});
		let resp = self.send_request("textDocument/diagnostic", params).await?;
		serde_json::from_value(resp).map_err(LspError::Json)
	}

	pub async fn complete(
		&self,
		uri: &str,
		line: u32,
		character: u32,
	) -> LspResult<Vec<LspCompletionItem>> {
		let params = json!({
			"textDocument": { "uri": uri },
			"position": { "line": line, "character": character },
			"context": {
				"triggerKind": 1,
			},
		});
		let resp = self.send_request("textDocument/completion", params).await?;

		// Completion can return a list or an object with an `items` array.
		if resp.as_array().is_some() {
			serde_json::from_value(resp).map_err(LspError::Json)
		} else if resp.get("items").and_then(|v| v.as_array()).is_some() {
			serde_json::from_value(resp["items"].clone()).map_err(LspError::Json)
		} else {
			Ok(Vec::new())
		}
	}
}

// ── Reader task ─────────────────────────────────────────────────────────────

async fn reader_task(
	stdout: ChildStdout,
	pending: Arc<Mutex<HashMap<u64, oneshot::Sender<LspResult<Value>>>>>,
	diagnostics_tx: broadcast::Sender<LspDiagnosticNotification>,
	capabilities: Arc<Mutex<Value>>,
) {
	let mut reader = BufReader::new(stdout);
	let mut buf = Vec::new();
	let mut content_length: Option<usize> = None;

	loop {
		buf.clear();
		let mut line_buf = Vec::new();

		// Read header lines until empty line.
		loop {
			line_buf.clear();
			match reader.read_until(b'\n', &mut line_buf).await {
				Ok(0) => return, // EOF
				Ok(_) => {}
				Err(_) => return,
			}

			let line_str = String::from_utf8_lossy(&line_buf).trim().to_string();

			if line_str.is_empty() {
				break; // end of headers
			}

			if let Some(len) =
				line_str.strip_prefix("Content-Length:").and_then(|s| s.trim().parse::<usize>().ok())
			{
				content_length = Some(len);
			}
		}

		let Some(len) = content_length.take() else {
			continue;
		};

		// Read exactly `len` bytes.
		buf.resize(len, 0);
		if read_exact(&mut reader, &mut buf).await.is_err() {
			return;
		}

		let body: Value = match serde_json::from_slice(&buf) {
			Ok(v) => v,
			Err(e) => {
				tracing::warn!("lsp: failed to parse JSON-RPC body: {e}");
				continue;
			}
		};

		handle_message(body, &pending, &diagnostics_tx, &capabilities).await;
	}
}

async fn read_exact<R: tokio::io::AsyncRead + Unpin>(
	reader: &mut BufReader<R>,
	buf: &mut [u8],
) -> std::io::Result<()> {
	let mut offset = 0;
	while offset < buf.len() {
		let n = tokio::io::AsyncReadExt::read(&mut *reader, &mut buf[offset..]).await?;
		if n == 0 {
			return Err(std::io::Error::new(
				std::io::ErrorKind::UnexpectedEof,
				"unexpected EOF reading LSP response body",
			));
		}
		offset += n;
	}
	Ok(())
}

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<LspResult<Value>>>>>;

async fn handle_message(
	body: Value,
	pending: &PendingMap,
	diagnostics_tx: &broadcast::Sender<LspDiagnosticNotification>,
	_capabilities: &Arc<Mutex<Value>>,
) {
	// Response with id
	if let Some(id_val) = body.get("id") {
		let id = id_val.as_u64().unwrap_or(0);
		let mut map = pending.lock().await;
		if let Some(tx) = map.remove(&id) {
			if let Some(err) = body.get("error") {
				let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
				let msg =
					err.get("message").and_then(|v| v.as_str()).unwrap_or("unknown error").to_string();
				let data = err.get("data").cloned();
				let _ = tx.send(Err(LspError::JsonRpc { code, message: msg, data }));
			} else {
				let result = body.get("result").cloned().unwrap_or(Value::Null);
				let _ = tx.send(Ok(result));
			}
		}
		return;
	}

	// Notification - no id
	if let Some(method) = body.get("method").and_then(|v| v.as_str()) {
		match method {
			"textDocument/publishDiagnostics" => {
				if let Some(uri) = body.pointer("/params/uri").and_then(|v| v.as_str()) {
					let diags: Vec<LspDiagnostic> = body
						.pointer("/params/diagnostics")
						.and_then(|v| serde_json::from_value(v.clone()).ok())
						.unwrap_or_default();
					let note = LspDiagnosticNotification { uri: uri.to_string(), diagnostics: diags };
					let _ = diagnostics_tx.send(note);
				}
			}
			"window/showMessage" => {
				if let Some(msg) = body.pointer("/params/message").and_then(|v| v.as_str()) {
					tracing::info!("lsp server message: {msg}");
				}
			}
			"window/logMessage" => {
				if let Some(msg) = body.pointer("/params/message").and_then(|v| v.as_str()) {
					tracing::debug!("lsp server log: {msg}");
				}
			}
			_ => {
				tracing::trace!("lsp unhandled notification: {method}");
			}
		}
	}
}

// ── Message framing helpers ─────────────────────────────────────────────────

async fn write_message(writer: &mut BufWriter<ChildStdin>, msg: &Value) -> std::io::Result<()> {
	let body = serde_json::to_string(msg)?;
	let header = format!("Content-Length: {}\r\n\r\n", body.len());
	writer.write_all(header.as_bytes()).await?;
	writer.write_all(body.as_bytes()).await?;
	Ok(())
}

pub fn path_to_uri(path: &Path) -> String {
	let abs = if path.is_absolute() {
		path.to_path_buf()
	} else {
		let Ok(cwd) = std::env::current_dir() else {
			return format!("file:///{}", path.display());
		};
		cwd.join(path)
	};
	let path_str = abs.to_string_lossy().replace('\\', "/");
	if cfg!(windows) {
		format!("file:///{}", path_str.trim_start_matches('/'))
	} else {
		format!("file://{}", path_str)
	}
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
	let rest = uri.strip_prefix("file://")?;
	// Handle Windows paths: file:///C:/...
	let path_str = if cfg!(windows) { rest.trim_start_matches('/') } else { rest };
	Some(PathBuf::from(path_str))
}

// ── Language detection ──────────────────────────────────────────────────────

fn language_id_from_path(path: &Path) -> Option<&'static str> {
	let ext = path.extension()?.to_str()?.to_ascii_lowercase();
	match ext.as_str() {
		"rs" => Some("rust"),
		"ts" | "tsx" => Some("typescript"),
		"js" | "jsx" | "mjs" => Some("javascript"),
		"py" => Some("python"),
		"go" => Some("go"),
		"c" | "h" => Some("c"),
		"cpp" | "hpp" | "cc" | "cxx" => Some("cpp"),
		"cs" => Some("csharp"),
		"java" => Some("java"),
		"rb" => Some("ruby"),
		"php" => Some("php"),
		"swift" => Some("swift"),
		"kt" | "kts" => Some("kotlin"),
		"scala" => Some("scala"),
		"vue" => Some("vue"),
		"json" => Some("json"),
		"yaml" | "yml" => Some("yaml"),
		"toml" => Some("toml"),
		"md" | "markdown" => Some("markdown"),
		"html" => Some("html"),
		"css" | "scss" | "less" => Some("css"),
		"sql" => Some("sql"),
		"sh" | "bash" | "zsh" => Some("shellscript"),
		"lua" => Some("lua"),
		"dart" => Some("dart"),
		_ => None,
	}
}

// ── LspRegistry ─────────────────────────────────────────────────────────────

pub struct LspRegistry {
	clients: Mutex<HashMap<String, Arc<LspClient>>>,
	known_servers: Vec<LspServer>,
	root_uri: String,
}

impl LspRegistry {
	pub fn new(root_uri: impl Into<String>) -> Self {
		let root_uri = root_uri.into();
		Self {
			clients: Mutex::new(HashMap::new()),
			known_servers: discover_known_servers(&root_uri),
			root_uri,
		}
	}

	pub fn with_servers(mut self, extra: Vec<LspServer>) -> Self {
		self.known_servers.extend(extra);
		self
	}

	pub async fn get_client(&self, language_id: &str) -> LspResult<Arc<LspClient>> {
		let clients = self.clients.lock().await;
		if let Some(client) = clients.get(language_id) {
			return Ok(Arc::clone(client));
		}
		drop(clients);

		self.start_server(language_id).await
	}

	pub async fn get_client_for_path(&self, path: &Path) -> LspResult<Arc<LspClient>> {
		let lang = language_id_from_path(path).ok_or_else(|| {
			LspError::ServerNotFound(format!("No language mapping for `{}`", path.display()))
		})?;
		self.get_client(lang).await
	}

	pub async fn start_server(&self, language_id: &str) -> LspResult<Arc<LspClient>> {
		let server = self
			.known_servers
			.iter()
			.find(|s| s.language_id == language_id)
			.cloned()
			.ok_or_else(|| LspError::ServerNotFound(language_id.to_string()))?;

		let client = Arc::new(LspClient::connect(server).await?);

		let mut clients = self.clients.lock().await;
		// If another task beat us, use theirs.
		if let Some(existing) = clients.get(language_id) {
			return Ok(Arc::clone(existing));
		}
		clients.insert(language_id.to_string(), Arc::clone(&client));
		Ok(client)
	}

	pub async fn shutdown_all(&self) {
		let clients = self.clients.lock().await;
		for (_lang, client) in clients.iter() {
			client.shutdown().await.ok();
		}
	}

	pub fn known_languages(&self) -> Vec<String> {
		let mut langs: Vec<String> = self.known_servers.iter().map(|s| s.language_id.clone()).collect();
		langs.sort();
		langs.dedup();
		langs
	}

	pub fn has_server_for(&self, language_id: &str) -> bool {
		self.known_servers.iter().any(|s| s.language_id == language_id)
	}
}

// ── Auto-discovery ──────────────────────────────────────────────────────────

const KNOWN_SERVERS: &[(&str, &str, &[&str], &[&str])] = &[
	("rust", "rust-analyzer", &[], &["rust"]),
	("typescript", "typescript-language-server", &["--stdio"], &["ts", "tsx"]),
	("javascript", "typescript-language-server", &["--stdio"], &["js", "jsx"]),
	("python", "pyright-langserver", &["--stdio"], &["py"]),
	("python", "basedpyright", &["--stdio"], &["py"]),
	("go", "gopls", &[], &["go"]),
	("c", "clangd", &[], &["c", "h"]),
	("cpp", "clangd", &[], &["cpp", "hpp", "cc"]),
	("csharp", "omnisharp", &[], &["cs"]),
	("java", "eclipse-jdtls", &[], &["java"]),
	("ruby", "solargraph", &[], &["rb"]),
	("php", "intelephense", &[], &["php"]),
	("swift", "sourcekit-lsp", &[], &["swift"]),
	("kotlin", "kotlin-language-server", &[], &["kt"]),
	("lua", "lua-language-server", &[], &["lua"]),
	("dart", "dart", &["language-server", "--protocol=lsp"], &["dart"]),
	("json", "vscode-json-languageserver", &["--stdio"], &["json"]),
	("yaml", "yaml-language-server", &["--stdio"], &["yaml", "yml"]),
	("toml", "taplo", &["lsp", "--stdio"], &["toml"]),
	("css", "vscode-css-languageserver", &["--stdio"], &["css", "scss", "less"]),
	("html", "vscode-html-languageserver", &["--stdio"], &["html"]),
	("markdown", "marksman", &[], &["md"]),
	("sql", "sql-language-server", &[], &["sql"]),
	("shellscript", "bash-language-server", &["start"], &["sh", "bash"]),
];

fn discover_known_servers(root_uri: &str) -> Vec<LspServer> {
	let mut servers: HashMap<&str, LspServer> = HashMap::new();

	for &(lang, cmd, args, _exts) in KNOWN_SERVERS {
		// Skip if we already have a server for this language.
		if servers.contains_key(lang) {
			continue;
		}

		let found = which::which(cmd).ok().map(|p| p.to_string_lossy().to_string());
		if let Some(full_cmd) = found {
			let server = LspServer::new(format!("{lang}-lsp"), lang, full_cmd)
				.with_args(args.iter().map(|s| s.to_string()).collect())
				.with_root_uri(root_uri);
			servers.insert(lang, server);
		}
	}

	servers.into_values().collect()
}

pub fn language_id_for_path(path: &Path) -> Option<&'static str> {
	language_id_from_path(path)
}

pub fn available_language_servers() -> Vec<(&'static str, &'static str)> {
	let mut result = Vec::new();
	let mut seen = std::collections::HashSet::new();
	for &(lang, cmd, _, _) in KNOWN_SERVERS {
		if seen.contains(&(lang, cmd)) {
			continue;
		}
		seen.insert((lang, cmd));
		if which::which(cmd).is_ok() {
			result.push((lang, cmd));
		}
	}
	result
}

// ── Global LSP registry ────────────────────────────────────────────────────

static GLOBAL_REGISTRY: OnceLock<Arc<LspRegistry>> = OnceLock::new();

pub fn init_global_registry(root_uri: &str) -> &'static Arc<LspRegistry> {
	GLOBAL_REGISTRY.get_or_init(|| Arc::new(LspRegistry::new(root_uri)))
}

pub fn global_registry() -> Option<&'static Arc<LspRegistry>> {
	GLOBAL_REGISTRY.get()
}

// ── Helpers for CLI output ──────────────────────────────────────────────────

pub fn fmt_diagnostics(diags: &[LspDiagnostic], uri: &str) -> String {
	if diags.is_empty() {
		return format!("✓ No diagnostics for {uri}");
	}

	let path =
		uri_to_path(uri).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|| uri.to_string());

	let mut out = format!("Diagnostics for {path}:\n");
	for d in diags {
		let sev = match d.severity {
			Some(1) => "ERROR",
			Some(2) => "WARN",
			Some(3) => "INFO",
			Some(4) => "HINT",
			_ => "NOTE",
		};
		let loc = format!("{}:{}", d.range.start.line + 1, d.range.start.character + 1);
		out.push_str(&format!("  {sev} {loc} {}\n", d.message));
	}
	out
}

pub fn fmt_locations(locs: &[LspLocation]) -> String {
	if locs.is_empty() {
		return "(no results)".to_string();
	}
	locs
		.iter()
		.map(|loc| {
			let path = uri_to_path(&loc.uri)
				.map(|p| p.to_string_lossy().to_string())
				.unwrap_or_else(|| loc.uri.clone());
			format!(
				"{}:{}:{}-{}:{}",
				path,
				loc.range.start.line + 1,
				loc.range.start.character + 1,
				loc.range.end.line + 1,
				loc.range.end.character + 1,
			)
		})
		.collect::<Vec<_>>()
		.join("\n")
}

pub fn fmt_symbols(symbols: &[LspSymbol], indent: usize) -> String {
	let mut out = String::new();
	for sym in symbols {
		let prefix = " ".repeat(indent);
		let detail = sym.detail.as_deref().map(|d| format!(" ({d})")).unwrap_or_default();
		out.push_str(&format!(
			"{prefix}{} [{:?}]{} — {}:{}\n",
			sym.name,
			sym.kind,
			detail,
			sym.range.start.line + 1,
			sym.range.start.character + 1,
		));
		if !sym.children.is_empty() {
			out.push_str(&fmt_symbols(&sym.children, indent + 2));
		}
	}
	out
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_path_to_uri() {
		let p = Path::new("/home/user/project/main.rs");
		let uri = path_to_uri(p);
		assert!(uri.starts_with("file://"));
		assert!(uri.contains("main.rs"));
	}

	#[test]
	fn test_uri_to_path() {
		let uri = "file:///home/user/project/main.rs";
		let p = uri_to_path(uri);
		assert!(p.is_some());
		assert!(p.unwrap().to_string_lossy().contains("main.rs"));
	}

	#[test]
	fn test_language_id_from_path() {
		assert_eq!(language_id_from_path(Path::new("main.rs")), Some("rust"));
		assert_eq!(language_id_from_path(Path::new("app.ts")), Some("typescript"));
		assert_eq!(language_id_from_path(Path::new("test.py")), Some("python"));
		assert_eq!(language_id_from_path(Path::new("main.go")), Some("go"));
		assert_eq!(language_id_from_path(Path::new("unknown.xyz")), None);
	}

	#[test]
	fn test_fmt_diagnostics_empty() {
		let result = fmt_diagnostics(&[], "file:///test.rs");
		assert!(result.contains("No diagnostics"));
	}

	#[test]
	fn test_fmt_diagnostics_with_items() {
		let diags = vec![LspDiagnostic {
			range: LspRange {
				start: LspPosition { line: 0, character: 0 },
				end: LspPosition { line: 0, character: 5 },
			},
			severity: Some(1),
			code: None,
			source: Some("rustc".into()),
			message: "unused variable".into(),
		}];
		let result = fmt_diagnostics(&diags, "file:///test.rs");
		assert!(result.contains("ERROR"));
		assert!(result.contains("unused variable"));
	}

	#[test]
	fn test_fmt_locations() {
		let locs = vec![LspLocation {
			uri: "file:///test.rs".into(),
			range: LspRange {
				start: LspPosition { line: 10, character: 0 },
				end: LspPosition { line: 10, character: 5 },
			},
		}];
		let result = fmt_locations(&locs);
		assert!(result.contains("test.rs"));
		assert!(result.contains("11:1"));
	}

	#[test]
	fn test_fmt_symbols() {
		let syms = vec![LspSymbol {
			name: "main".into(),
			kind: 12,
			detail: Some("fn".into()),
			range: LspRange {
				start: LspPosition { line: 0, character: 0 },
				end: LspPosition { line: 0, character: 4 },
			},
			selection_range: LspRange {
				start: LspPosition { line: 0, character: 0 },
				end: LspPosition { line: 0, character: 4 },
			},
			children: vec![],
		}];
		let result = fmt_symbols(&syms, 0);
		assert!(result.contains("main"));
	}

	#[test]
	fn test_available_servers_is_vec() {
		let servers = available_language_servers();
		assert!(servers.iter().all(|(lang, cmd)| { !lang.is_empty() && !cmd.is_empty() }));
	}

	#[test]
	fn test_lsp_error_display() {
		let err = LspError::JsonRpc { code: -32601, message: "Method not found".into(), data: None };
		let msg = err.to_string();
		assert!(msg.contains("-32601"));
		assert!(msg.contains("Method not found"));
	}

	#[test]
	fn test_lsp_markup_content_plain() {
		let content = LspMarkupContent::Plain("hello".into());
		assert_eq!(content.to_string(), "hello");
	}

	#[test]
	fn test_lsp_markup_content_markup() {
		let content = LspMarkupContent::Markup { kind: "markdown".into(), value: "# title".into() };
		assert_eq!(content.to_string(), "# title");
	}
}
