//! Lightweight HTTP + WebSocket API server for dx-tui.
//!
//! Exposes RESTful endpoints and a real-time WebSocket for external clients.
//! Uses only `tokio` (no axum/warp) to keep dependencies minimal.
//! SSE (Server-Sent Events) streams chat responses.

#![allow(dead_code)]

use std::{
	collections::HashMap,
	net::SocketAddr,
	sync::{
		Arc,
		atomic::{AtomicU64, Ordering},
	},
	time::Instant,
};

use anyhow::{Context as _, Result};
use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::{
	io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
	net::tcp::{OwnedReadHalf, OwnedWriteHalf},
	net::{TcpListener, TcpStream},
	sync::watch,
};
use tracing::{debug, info, warn};

// ── Config ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ApiConfig {
	pub host: String,
	pub port: u16,
	pub auth_token: Option<String>,
}

impl Default for ApiConfig {
	fn default() -> Self {
		Self { host: "127.0.0.1".into(), port: 10245, auth_token: None }
	}
}

// ── Telemetry ───────────────────────────────────────────────────────────

pub struct Telemetry {
	pub total_requests: AtomicU64,
	pub total_errors: AtomicU64,
	pub active_connections: AtomicU64,
	pub start_time: Instant,
	pub route_counts: Arc<parking_lot::Mutex<HashMap<String, u64>>>,
}

impl Telemetry {
	pub fn new() -> Self {
		Self {
			total_requests: AtomicU64::new(0),
			total_errors: AtomicU64::new(0),
			active_connections: AtomicU64::new(0),
			start_time: Instant::now(),
			route_counts: Arc::new(parking_lot::Mutex::new(HashMap::new())),
		}
	}

	fn record_request(&self, route: &str) {
		self.total_requests.fetch_add(1, Ordering::Relaxed);
		let mut counts = self.route_counts.lock();
		*counts.entry(route.to_string()).or_insert(0) += 1;
	}

	fn record_error(&self) {
		self.total_errors.fetch_add(1, Ordering::Relaxed);
	}

	pub fn snapshot(&self) -> TelemetrySnapshot {
		let counts = self.route_counts.lock().clone();
		TelemetrySnapshot {
			uptime_seconds: self.start_time.elapsed().as_secs(),
			total_requests: self.total_requests.load(Ordering::Relaxed),
			total_errors: self.total_errors.load(Ordering::Relaxed),
			active_connections: self.active_connections.load(Ordering::Relaxed),
			route_counts: counts,
		}
	}
}

#[derive(Debug, Clone, Serialize)]
pub struct TelemetrySnapshot {
	pub uptime_seconds: u64,
	pub total_requests: u64,
	pub total_errors: u64,
	pub active_connections: u64,
	pub route_counts: HashMap<String, u64>,
}

// ── SSE Event ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum SseEvent {
	Data(String),
	Error(String),
	Done,
}

// ── Handler Trait ───────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait ApiHandler: Send + Sync {
	/// POST /api/chat — stream response as SSE events.
	async fn handle_chat(&self, body: Value, events: tokio::sync::mpsc::Sender<SseEvent>);

	/// GET /api/sessions
	async fn list_sessions(&self) -> Result<Value>;

	/// GET /api/sessions/:id
	async fn get_session(&self, id: &str) -> Result<Option<Value>>;

	/// DELETE /api/sessions/:id
	async fn delete_session(&self, id: &str) -> Result<bool>;

	/// GET /api/providers
	async fn list_providers(&self) -> Result<Value>;

	/// POST /api/providers/connect
	async fn connect_provider(&self, body: Value) -> Result<Value>;

	/// GET /api/tools
	async fn list_tools(&self) -> Result<Value>;

	/// POST /api/tools/execute
	async fn execute_tool(&self, body: Value) -> Result<Value>;

	/// GET /api/status
	async fn status(&self) -> Result<Value>;

	/// GET /api/models
	async fn list_models(&self) -> Result<Value>;

	/// WebSocket message received.
	async fn on_ws_message(&self, msg: Value) -> Result<Option<Value>>;
}

// ── API Server ──────────────────────────────────────────────────────────

pub struct ApiServer {
	config: ApiConfig,
	handler: Arc<dyn ApiHandler>,
	telemetry: Arc<Telemetry>,
	shutdown_tx: Option<watch::Sender<bool>>,
}

impl ApiServer {
	pub fn new(config: ApiConfig, handler: Arc<dyn ApiHandler>) -> Self {
		Self { config, handler, telemetry: Arc::new(Telemetry::new()), shutdown_tx: None }
	}

	/// Start the server in a background task.
	pub async fn start(&mut self) -> Result<tokio::task::JoinHandle<()>> {
		let addr: SocketAddr = format!("{}:{}", self.config.host, self.config.port)
			.parse()
			.context("invalid API server address")?;

		let listener = TcpListener::bind(addr).await?;
		info!("API server listening on http://{addr}");

		let (shutdown_tx, shutdown_rx) = watch::channel(false);
		self.shutdown_tx = Some(shutdown_tx);

		let config = self.config.clone();
		let handler = self.handler.clone();
		let telemetry = self.telemetry.clone();

		let handle = tokio::spawn(async move {
			run_server(listener, config, handler, telemetry, shutdown_rx).await;
		});

		Ok(handle)
	}

	/// Request graceful shutdown.
	pub fn stop(&self) {
		if let Some(tx) = &self.shutdown_tx {
			let _ = tx.send(true);
		}
	}

	pub fn telemetry(&self) -> &Telemetry {
		&self.telemetry
	}

	pub fn config(&self) -> &ApiConfig {
		&self.config
	}
}

// ── Server Loop ─────────────────────────────────────────────────────────

async fn run_server(
	listener: TcpListener,
	config: ApiConfig,
	handler: Arc<dyn ApiHandler>,
	telemetry: Arc<Telemetry>,
	mut shutdown_rx: watch::Receiver<bool>,
) {
	loop {
		tokio::select! {
			_ = shutdown_rx.changed() => {
				if *shutdown_rx.borrow() {
					info!("API server shutting down");
					break;
				}
			}
			result = listener.accept() => {
				match result {
					Ok((stream, addr)) => {
						telemetry.active_connections.fetch_add(1, Ordering::Relaxed);
						let handler = handler.clone();
						let config = config.clone();
						let telemetry = telemetry.clone();
						tokio::spawn(async move {
							handle_connection(stream, addr, handler, config, telemetry).await;
						});
					}
					Err(e) => {
						warn!("API accept error: {e}");
					}
				}
			}
		}
	}
}

// ── Connection Handler ──────────────────────────────────────────────────

async fn handle_connection(
	stream: TcpStream,
	addr: SocketAddr,
	handler: Arc<dyn ApiHandler>,
	config: ApiConfig,
	telemetry: Arc<Telemetry>,
) {
	debug!("API connection from {addr}");

	let (reader, mut writer) = stream.into_split();
	let mut buf_reader = BufReader::new(reader);
	let mut request_line = String::new();

	if buf_reader.read_line(&mut request_line).await.unwrap_or(0) == 0 {
		telemetry.active_connections.fetch_sub(1, Ordering::Relaxed);
		return;
	}

	let request_line = request_line.trim().to_string();
	let parts: Vec<&str> = request_line.split_whitespace().collect();
	if parts.len() < 2 {
		let _ = send_error(&mut writer, 400, "Bad Request").await;
		telemetry.active_connections.fetch_sub(1, Ordering::Relaxed);
		return;
	}

	let method = parts[0].to_uppercase();
	let path = parts[1].to_string();

	// Read headers
	let mut headers: HashMap<String, String> = HashMap::new();
	let mut content_length: usize = 0;
	loop {
		let mut line = String::new();
		if buf_reader.read_line(&mut line).await.unwrap_or(0) == 0 {
			break;
		}
		let line = line.trim().to_string();
		if line.is_empty() {
			break;
		}
		if let Some((key, value)) = line.split_once(':') {
			let k = key.trim().to_lowercase();
			let v = value.trim().to_string();
			if k == "content-length" {
				content_length = v.parse().unwrap_or(0);
			}
			headers.insert(k, v);
		}
	}

	// Authentication check
	if let Some(ref token) = config.auth_token {
		let auth_header = headers.get("authorization").cloned().unwrap_or_default();
		let expected = format!("Bearer {token}");
		if auth_header != expected {
			let _ = send_error(&mut writer, 401, "Unauthorized").await;
			telemetry.active_connections.fetch_sub(1, Ordering::Relaxed);
			return;
		}
	}

	// Read body if present
	let body = if content_length > 0 && (method == "POST" || method == "PUT" || method == "PATCH") {
		let mut body_bytes = vec![0u8; content_length];
		if buf_reader.read_exact(&mut body_bytes).await.is_ok() {
			String::from_utf8_lossy(&body_bytes).to_string()
		} else {
			String::new()
		}
	} else {
		String::new()
	};

	// Route
	match (method.as_str(), path.as_str()) {
		("GET", "/api/status") => {
			telemetry.record_request("status");
			route_status(&mut writer, &telemetry).await;
		}
		("GET", "/api/models") => {
			telemetry.record_request("models");
			route_json(&mut writer, handler.list_models().await).await;
		}
		("GET", "/api/providers") => {
			telemetry.record_request("providers");
			route_json(&mut writer, handler.list_providers().await).await;
		}
		("POST", "/api/providers/connect") => {
			telemetry.record_request("providers_connect");
			let body_val: Value = serde_json::from_str(&body).unwrap_or_default();
			route_json(&mut writer, handler.connect_provider(body_val).await).await;
		}
		("GET", "/api/sessions") => {
			telemetry.record_request("sessions");
			route_json(&mut writer, handler.list_sessions().await).await;
		}
		("GET", path) if path.starts_with("/api/sessions/") && !path.contains("/delete") => {
			telemetry.record_request("session_get");
			let id = path.trim_start_matches("/api/sessions/");
			let result = match handler.get_session(id).await {
				Ok(Some(session)) => Ok(session),
				Ok(None) => Ok(json!({"error": "session not found"})),
				Err(e) => Err(e),
			};
			route_json(&mut writer, result).await;
		}
		("DELETE", path) if path.starts_with("/api/sessions/") => {
			telemetry.record_request("session_delete");
			let id = path.trim_start_matches("/api/sessions/");
			let result = handler.delete_session(id).await.map(|deleted| json!({"deleted": deleted}));
			route_json(&mut writer, result).await;
		}
		("GET", "/api/tools") => {
			telemetry.record_request("tools");
			route_json(&mut writer, handler.list_tools().await).await;
		}
		("POST", "/api/tools/execute") => {
			telemetry.record_request("tools_execute");
			let body_val: Value = serde_json::from_str(&body).unwrap_or_default();
			route_json(&mut writer, handler.execute_tool(body_val).await).await;
		}
		("POST", "/api/chat") => {
			telemetry.record_request("chat");
			let body_val: Value = serde_json::from_str(&body).unwrap_or_default();
			route_chat_sse(&mut writer, &handler, body_val, &telemetry).await;
		}
		("GET", "/ws") | ("GET", "/api/ws") => {
			telemetry.record_request("ws");
			route_websocket(&mut writer, &handler, &headers, &mut buf_reader).await;
		}
		("OPTIONS", _) => {
			let _ = send_cors(&mut writer).await;
		}
		_ => {
			telemetry.record_error();
			let _ = send_error(&mut writer, 404, "Not Found").await;
		}
	}

	telemetry.active_connections.fetch_sub(1, Ordering::Relaxed);
}

// ── Route Implementations ───────────────────────────────────────────────

async fn route_status(writer: &mut OwnedWriteHalf, telemetry: &Telemetry) {
	let snap = telemetry.snapshot();
	let body = serde_json::to_string(&snap).unwrap_or_else(|_| "{}".into());
	send_json(writer, 200, &body).await;
}

async fn route_json(writer: &mut OwnedWriteHalf, result: Result<Value>) {
	match result {
		Ok(val) => {
			let body = serde_json::to_string(&val).unwrap_or_else(|_| "null".into());
			send_json(writer, 200, &body).await;
		}
		Err(e) => {
			let body = serde_json::to_string(&serde_json::json!({
				"error": e.to_string()
			}))
			.unwrap_or_else(|_| r#"{"error":"internal"}"#.into());
			send_json(writer, 500, &body).await;
		}
	}
}

async fn route_chat_sse(
	writer: &mut OwnedWriteHalf,
	handler: &Arc<dyn ApiHandler>,
	body: Value,
	telemetry: &Telemetry,
) {
	// Send SSE headers
	let headers = "\
HTTP/1.1 200 OK\r\n\
Content-Type: text/event-stream\r\n\
Cache-Control: no-cache\r\n\
Connection: keep-alive\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
\r\n";

	if writer.write_all(headers.as_bytes()).await.is_err() {
		telemetry.record_error();
		return;
	}
	if writer.flush().await.is_err() {
		telemetry.record_error();
		return;
	}

	let (tx, mut rx) = tokio::sync::mpsc::channel::<SseEvent>(64);
	handler.handle_chat(body, tx).await;

	while let Some(event) = rx.recv().await {
		match event {
			SseEvent::Data(data) => {
				let frame = format!("data: {data}\n\n");
				if writer.write_all(frame.as_bytes()).await.is_err() {
					break;
				}
				if writer.flush().await.is_err() {
					break;
				}
			}
			SseEvent::Error(err) => {
				let frame = format!("event: error\ndata: {err}\n\n");
				let _ = writer.write_all(frame.as_bytes()).await;
				let _ = writer.flush().await;
				break;
			}
			SseEvent::Done => {
				let _ = writer.write_all(b"data: [DONE]\n\n").await;
				let _ = writer.flush().await;
				break;
			}
		}
	}
}

// ── WebSocket ───────────────────────────────────────────────────────────

async fn route_websocket(
	writer: &mut OwnedWriteHalf,
	handler: &Arc<dyn ApiHandler>,
	headers: &HashMap<String, String>,
	reader: &mut BufReader<OwnedReadHalf>,
) {
	// Check WebSocket key
	let ws_key = match headers.get("sec-websocket-key") {
		Some(k) => k.clone(),
		None => {
			let _ = send_error(writer, 400, "Missing Sec-WebSocket-Key").await;
			return;
		}
	};

	let accept = websocket_accept_key(&ws_key);
	let response = format!(
		"HTTP/1.1 101 Switching Protocols\r\n\
		 Upgrade: websocket\r\n\
		 Connection: Upgrade\r\n\
		 Sec-WebSocket-Accept: {accept}\r\n\
		 Access-Control-Allow-Origin: *\r\n\
		 \r\n"
	);

	if writer.write_all(response.as_bytes()).await.is_err() {
		return;
	}
	if writer.flush().await.is_err() {
		return;
	}

	// WebSocket frame loop
	while let Ok(frame) = read_ws_frame(reader).await {
		match frame.opcode {
			0x8 => break, // Close
			0x9 => {
				// Ping → Pong
				let _ = write_ws_frame(writer, 0xA, &frame.payload).await;
			}
			0x1 | 0x2 => {
				// Text or Binary
				let text = String::from_utf8_lossy(&frame.payload).to_string();
				let msg: Value = serde_json::from_str(&text).unwrap_or_default();
				match handler.on_ws_message(msg).await {
					Ok(Some(reply)) => {
						let reply_text = serde_json::to_string(&reply).unwrap_or_default();
						let _ = write_ws_frame(writer, 0x1, reply_text.as_bytes()).await;
					}
					Ok(None) => {}
					Err(e) => {
						let err = serde_json::json!({
							"type": "error",
							"message": e.to_string()
						});
						let err_text = serde_json::to_string(&err).unwrap_or_default();
						let _ = write_ws_frame(writer, 0x1, err_text.as_bytes()).await;
					}
				}
			}
			_ => {}
		}
	}
}

// ── WebSocket Frame I/O ─────────────────────────────────────────────────

struct WsFrame {
	opcode: u8,
	payload: Vec<u8>,
}

async fn read_ws_frame(reader: &mut BufReader<OwnedReadHalf>) -> Result<WsFrame> {
	let mut header = [0u8; 2];
	reader.read_exact(&mut header).await?;

	let opcode = header[0] & 0x0F;
	let masked = (header[1] & 0x80) != 0;
	let mut len = (header[1] & 0x7F) as u64;

	if len == 126 {
		let mut ext = [0u8; 2];
		reader.read_exact(&mut ext).await?;
		len = u16::from_be_bytes(ext) as u64;
	} else if len == 127 {
		let mut ext = [0u8; 8];
		reader.read_exact(&mut ext).await?;
		len = u64::from_be_bytes(ext);
	}

	let mask_key = if masked {
		let mut key = [0u8; 4];
		reader.read_exact(&mut key).await?;
		Some(key)
	} else {
		None
	};

	let mut payload = vec![0u8; len as usize];
	if len > 0 {
		reader.read_exact(&mut payload).await?;
	}

	if let Some(key) = mask_key {
		for (i, byte) in payload.iter_mut().enumerate() {
			*byte ^= key[i % 4];
		}
	}

	Ok(WsFrame { opcode, payload })
}

async fn write_ws_frame(writer: &mut OwnedWriteHalf, opcode: u8, payload: &[u8]) -> Result<()> {
	let mut header = Vec::with_capacity(10);
	header.push(0x80 | opcode); // FIN + opcode

	let len = payload.len();
	if len < 126 {
		header.push(len as u8);
	} else if len <= 0xFFFF {
		header.push(126);
		header.extend_from_slice(&(len as u16).to_be_bytes());
	} else {
		header.push(127);
		header.extend_from_slice(&(len as u64).to_be_bytes());
	}

	writer.write_all(&header).await?;
	if !payload.is_empty() {
		writer.write_all(payload).await?;
	}
	writer.flush().await?;
	Ok(())
}

// ── WebSocket Accept Key ────────────────────────────────────────────────

fn websocket_accept_key(key: &str) -> String {
	const MAGIC: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
	let mut input = key.as_bytes().to_vec();
	input.extend_from_slice(MAGIC);
	let hash = sha1(&input);
	base64::engine::general_purpose::STANDARD.encode(hash)
}

// Minimal SHA-1 implementation (for WebSocket handshake only).
fn sha1(data: &[u8]) -> [u8; 20] {
	let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

	let mut padded = data.to_vec();
	let bit_len = (data.len() as u64) * 8;
	padded.push(0x80);
	while (padded.len() * 8) % 512 != 448 {
		padded.push(0);
	}
	padded.extend_from_slice(&bit_len.to_be_bytes());

	for chunk in padded.chunks(64) {
		let mut w = [0u32; 80];
		for (i, word) in chunk.chunks(4).enumerate() {
			w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
		}
		for i in 16..80 {
			w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
		}

		let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);

		for (i, &wi) in w.iter().enumerate().take(80) {
			let (f, k) = match i {
				0..=19 => ((b & c) | (!b & d), 0x5A827999),
				20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
				40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
				_ => (b ^ c ^ d, 0xCA62C1D6),
			};
			let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(wi);
			e = d;
			d = c;
			c = b.rotate_left(30);
			b = a;
			a = temp;
		}

		h[0] = h[0].wrapping_add(a);
		h[1] = h[1].wrapping_add(b);
		h[2] = h[2].wrapping_add(c);
		h[3] = h[3].wrapping_add(d);
		h[4] = h[4].wrapping_add(e);
	}

	let mut result = [0u8; 20];
	for (i, val) in h.iter().enumerate() {
		result[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
	}
	result
}

// ── HTTP Response Helpers ───────────────────────────────────────────────

async fn send_json(writer: &mut OwnedWriteHalf, status: u16, body: &str) {
	let reason = if status == 200 {
		"OK"
	} else if status == 400 {
		"Bad Request"
	} else if status == 401 {
		"Unauthorized"
	} else if status == 404 {
		"Not Found"
	} else {
		"Error"
	};
	let resp = format!(
		"HTTP/1.1 {status} {reason}\r\n\
		 Content-Type: application/json\r\n\
		 Content-Length: {}\r\n\
		 Access-Control-Allow-Origin: *\r\n\
		 Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
		 \r\n\
		 {body}",
		body.len()
	);
	let _ = writer.write_all(resp.as_bytes()).await;
	let _ = writer.flush().await;
}

async fn send_error(writer: &mut OwnedWriteHalf, status: u16, message: &str) {
	let body = serde_json::json!({ "error": message }).to_string();
	send_json(writer, status, &body).await;
}

async fn send_cors(writer: &mut OwnedWriteHalf) {
	let resp = "\
HTTP/1.1 204 No Content\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\
Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
Access-Control-Max-Age: 86400\r\n\
\r\n";
	let _ = writer.write_all(resp.as_bytes()).await;
	let _ = writer.flush().await;
}
