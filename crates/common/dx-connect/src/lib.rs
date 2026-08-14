//! DX Connect: a source-aware catalog and deterministic node executor.
//!
//! Flow-Like and n8n execute behind a versioned JSONL process boundary. The
//! TUI owns discovery and routing; the source runtimes own their execution
//! contexts, credentials, expression engines, and capability sandboxes.

#![forbid(unsafe_code)]

mod imported_catalog;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const ADAPTER_PROTOCOL: &str = "dx-connect/1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeSource {
    DxNative,
    FlowLike,
    N8n,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeBackend {
    Native,
    FlowLikeAdapter,
    N8nAdapter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeDefinition {
    pub id: String,
    pub display_name: String,
    pub source: NodeSource,
    pub backend: NodeBackend,
    pub description: String,
    pub inputs: u8,
    pub outputs: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeItem {
    pub json: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeContext {
    pub items: Vec<NodeItem>,
    pub parameters: Map<String, Value>,
    /// Credential material is passed only over the adapter's stdin. It is
    /// never placed in process arguments, logs, or catalog metadata.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub credentials: Map<String, Value>,
    /// Runtime-specific context (for example a Flow-Like execution request).
    /// Adapters must validate this data before using it.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRequest {
    pub protocol: String,
    pub request_id: String,
    pub node_id: String,
    pub context: NodeContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterResponse {
    pub protocol: String,
    pub request_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<Vec<NodeItem>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AdapterConfig {
    program: PathBuf,
    args: Vec<OsString>,
    working_dir: Option<PathBuf>,
    timeout: Duration,
    max_output_bytes: usize,
}

#[derive(Debug, Error, PartialEq)]
pub enum ConnectError {
    #[error("unknown connect node: {0}")]
    UnknownNode(String),
    #[error("node `{node}` cannot run: {runtime} adapter is not configured ({hint})")]
    AdapterUnavailable {
        node: String,
        runtime: &'static str,
        hint: String,
    },
    #[error("{runtime} adapter configuration is invalid: {message}")]
    AdapterConfiguration {
        runtime: &'static str,
        message: String,
    },
    #[error("{runtime} adapter failed for `{node}`: {message}")]
    AdapterExecution {
        runtime: &'static str,
        node: String,
        message: String,
    },
    #[error("{runtime} adapter returned an invalid response for `{node}`: {message}")]
    AdapterProtocol {
        runtime: &'static str,
        node: String,
        message: String,
    },
    #[error("node `{0}` parameter `{1}` must be {2}")]
    InvalidParameter(String, String, &'static str),
}

/// Return the built-in catalog. The catalog intentionally contains source
/// provenance so the TUI can show whether a node came from Flow-Like or n8n.
pub fn catalog() -> Vec<NodeDefinition> {
    let mut nodes = Vec::new();
    for (id, display_name, description) in [
        ("set", "Set", "Set or merge JSON fields"),
        ("if", "If", "Route items by a boolean condition"),
        ("merge", "Merge", "Combine item streams"),
        ("noop", "No Op", "Pass items through unchanged"),
    ] {
        nodes.push(NodeDefinition {
            id: format!("dx.{id}"),
            display_name: display_name.into(),
            source: NodeSource::DxNative,
            backend: NodeBackend::Native,
            description: description.into(),
            inputs: 1,
            outputs: 1,
        });
    }
    nodes.extend(imported_catalog::external_catalog());
    nodes
}

/// Return a bounded catalog for interactive surfaces.
///
/// The complete catalog remains available to execution and machine-facing
/// discovery, but rendering thousands of rows in a terminal modal makes the
/// UI unresponsive. The Connects tab uses this bounded view until a dedicated
/// paged search endpoint is available.
pub fn catalog_limited(limit: usize) -> Vec<NodeDefinition> {
    let mut nodes = Vec::with_capacity(limit);
    for (id, display_name, description) in [
        ("set", "Set", "Set or merge JSON fields"),
        ("if", "If", "Route items by a boolean condition"),
        ("merge", "Merge", "Combine item streams"),
        ("noop", "No Op", "Pass items through unchanged"),
    ] {
        nodes.push(NodeDefinition {
            id: format!("dx.{id}"),
            display_name: display_name.into(),
            source: NodeSource::DxNative,
            backend: NodeBackend::Native,
            description: description.into(),
            inputs: 1,
            outputs: 1,
        });
    }
    nodes.extend(imported_catalog::external_catalog_limited(
        limit.saturating_sub(nodes.len()),
    ));
    nodes
}

pub fn find_node(id: &str) -> Option<NodeDefinition> {
    catalog()
        .into_iter()
        .find(|node| node.id == id || node.id.eq_ignore_ascii_case(id))
}

/// Execute the guaranteed DX-native subset.
pub fn execute(node_id: &str, context: NodeContext) -> Result<Vec<Vec<NodeItem>>, ConnectError> {
    let node = find_node(node_id).ok_or_else(|| ConnectError::UnknownNode(node_id.into()))?;
    match node.backend {
        NodeBackend::Native => execute_native(node_id, context),
        NodeBackend::FlowLikeAdapter => execute_adapter("Flow-Like", node_id, context),
        NodeBackend::N8nAdapter => execute_adapter("n8n", node_id, context),
    }
}

fn execute_adapter(
    runtime: &'static str,
    node_id: &str,
    context: NodeContext,
) -> Result<Vec<Vec<NodeItem>>, ConnectError> {
    let config = adapter_config(runtime, node_id)?;
    let request = AdapterRequest {
        protocol: ADAPTER_PROTOCOL.into(),
        request_id: request_id(),
        node_id: node_id.into(),
        context,
    };
    let request_id = request.request_id.clone();
    let payload = serde_json::to_vec(&request).map_err(|error| ConnectError::AdapterProtocol {
        runtime,
        node: node_id.into(),
        message: format!("could not encode request: {error}"),
    })?;
    let response = run_adapter(runtime, node_id, &config, &payload)?;

    if response.protocol != ADAPTER_PROTOCOL {
        return Err(ConnectError::AdapterProtocol {
            runtime,
            node: node_id.into(),
            message: format!("protocol `{}` is not supported", response.protocol),
        });
    }
    if response.request_id != request_id {
        return Err(ConnectError::AdapterProtocol {
            runtime,
            node: node_id.into(),
            message: "request id mismatch".into(),
        });
    }
    if !response.ok {
        return Err(ConnectError::AdapterExecution {
            runtime,
            node: node_id.into(),
            message: response
                .error
                .unwrap_or_else(|| "adapter returned an unsuccessful response".into()),
        });
    }
    response
        .outputs
        .ok_or_else(|| ConnectError::AdapterProtocol {
            runtime,
            node: node_id.into(),
            message: "successful response did not contain outputs".into(),
        })
}

fn adapter_config(runtime: &'static str, node_id: &str) -> Result<AdapterConfig, ConnectError> {
    let (program_var, args_var, cwd_var, runtime_root_var) = match runtime {
        "Flow-Like" => (
            "DX_FLOW_LIKE_ADAPTER",
            "DX_FLOW_LIKE_ADAPTER_ARGS",
            "DX_FLOW_LIKE_ADAPTER_CWD",
            "DX_FLOW_LIKE_ROOT",
        ),
        "n8n" => (
            "DX_N8N_ADAPTER",
            "DX_N8N_ADAPTER_ARGS",
            "DX_N8N_ADAPTER_CWD",
            "DX_N8N_ROOT",
        ),
        _ => unreachable!("only known adapter runtimes reach this function"),
    };

    let (program, mut args, working_dir) = if let Some(program) = std::env::var_os(program_var) {
        let args = adapter_args(runtime, args_var)?;
        let cwd = std::env::var_os(cwd_var).map(PathBuf::from);
        (PathBuf::from(program), args, cwd)
    } else if runtime == "n8n" {
        let Some(root) = std::env::var_os(runtime_root_var).map(PathBuf::from) else {
            return Err(ConnectError::AdapterUnavailable {
                node: node_id.into(),
                runtime,
                hint: "set DX_N8N_ROOT or DX_N8N_ADAPTER".into(),
            });
        };
        let worker = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("adapters")
            .join("n8n")
            .join("worker.cjs");
        if !worker.is_file() {
            return Err(ConnectError::AdapterConfiguration {
                runtime,
                message: format!("bundled worker is missing: {}", worker.display()),
            });
        }
        let program = std::env::var_os("DX_NODE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("node"));
        let loader = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("adapters")
            .join("n8n")
            .join("loader.mjs");
        (
            program,
            vec![
                "--experimental-loader".into(),
                file_url(loader),
                worker.into_os_string(),
            ],
            Some(root),
        )
    } else {
        return Err(ConnectError::AdapterUnavailable {
            node: node_id.into(),
            runtime,
            hint: "set DX_FLOW_LIKE_ADAPTER to the isolated JSONL adapter executable".into(),
        });
    };

    if let Some(root) = std::env::var_os(runtime_root_var).map(PathBuf::from)
        && working_dir.is_none()
    {
        args.shrink_to_fit();
        return Ok(AdapterConfig {
            program,
            args,
            working_dir: Some(root),
            timeout: adapter_timeout(runtime)?,
            max_output_bytes: adapter_output_limit(runtime)?,
        });
    }

    Ok(AdapterConfig {
        program,
        args,
        working_dir,
        timeout: adapter_timeout(runtime)?,
        max_output_bytes: adapter_output_limit(runtime)?,
    })
}

fn file_url(path: PathBuf) -> OsString {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        OsString::from(format!("file:///{path}"))
    } else {
        OsString::from(format!("file://{path}"))
    }
}

fn adapter_args(runtime: &'static str, variable: &str) -> Result<Vec<OsString>, ConnectError> {
    let Some(raw) = std::env::var_os(variable) else {
        return Ok(Vec::new());
    };
    let raw = raw.to_string_lossy();
    let values: Vec<String> =
        serde_json::from_str(&raw).map_err(|error| ConnectError::AdapterConfiguration {
            runtime,
            message: format!("{variable} must be a JSON string array: {error}"),
        })?;
    Ok(values.into_iter().map(OsString::from).collect())
}

fn adapter_timeout(runtime: &'static str) -> Result<Duration, ConnectError> {
    let variable = if runtime == "n8n" {
        "DX_N8N_ADAPTER_TIMEOUT_MS"
    } else {
        "DX_FLOW_LIKE_ADAPTER_TIMEOUT_MS"
    };
    let value = std::env::var(variable)
        .ok()
        .map(|raw| raw.parse::<u64>())
        .transpose()
        .map_err(|error| ConnectError::AdapterConfiguration {
            runtime,
            message: format!("{variable} must be an integer: {error}"),
        })?
        .unwrap_or(30_000)
        .clamp(1, 600_000);
    Ok(Duration::from_millis(value))
}

fn adapter_output_limit(runtime: &'static str) -> Result<usize, ConnectError> {
    let variable = if runtime == "n8n" {
        "DX_N8N_ADAPTER_MAX_OUTPUT_BYTES"
    } else {
        "DX_FLOW_LIKE_ADAPTER_MAX_OUTPUT_BYTES"
    };
    let value = std::env::var(variable)
        .ok()
        .map(|raw| raw.parse::<usize>())
        .transpose()
        .map_err(|error| ConnectError::AdapterConfiguration {
            runtime,
            message: format!("{variable} must be an integer: {error}"),
        })?
        .unwrap_or(16 * 1024 * 1024)
        .clamp(1024, 64 * 1024 * 1024);
    Ok(value)
}

struct AdapterClient {
    config: AdapterConfig,
    child: Child,
    stdin: ChildStdin,
    responses: mpsc::Receiver<Result<Vec<u8>, String>>,
}

impl AdapterClient {
    fn spawn(runtime: &'static str, config: &AdapterConfig) -> Result<Self, ConnectError> {
        let mut command = Command::new(&config.program);
        command
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(working_dir) = &config.working_dir {
            command.current_dir(working_dir);
        }
        let mut child = command
            .spawn()
            .map_err(|error| ConnectError::AdapterExecution {
                runtime,
                node: "<adapter startup>".into(),
                message: format!("could not start `{}`: {error}", config.program.display()),
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ConnectError::AdapterExecution {
                runtime,
                node: "<adapter startup>".into(),
                message: "adapter stdin was not available".into(),
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ConnectError::AdapterExecution {
                runtime,
                node: "<adapter startup>".into(),
                message: "adapter stdout was not available".into(),
            })?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ConnectError::AdapterExecution {
                runtime,
                node: "<adapter startup>".into(),
                message: "adapter stderr was not available".into(),
            })?;

        let (response_tx, response_rx) = mpsc::channel();
        let output_limit = config.max_output_bytes;
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        if line.len() > output_limit {
                            let _ = response_tx.send(Err(format!(
                                "adapter response exceeded {} bytes",
                                output_limit
                            )));
                            break;
                        }
                        if line.iter().all(u8::is_ascii_whitespace) {
                            continue;
                        }
                        if response_tx.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = response_tx
                            .send(Err(format!("could not read adapter stdout: {error}")));
                        break;
                    }
                }
            }
        });
        // Drain stderr continuously so a noisy runtime cannot block on a full
        // pipe. Secrets are not copied into TUI error text.
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        Ok(Self {
            config: config.clone(),
            child,
            stdin,
            responses: response_rx,
        })
    }

    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn send(&mut self, payload: &[u8], node_id: &str) -> Result<Vec<u8>, String> {
        if !self.is_alive() {
            return Err("adapter process is not running".into());
        }
        self.stdin
            .write_all(payload)
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("could not send request for `{node_id}`: {error}"))?;
        match self.responses.recv_timeout(self.config.timeout) {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => {
                self.stop();
                Err(error)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.stop();
                Err(format!(
                    "timed out after {} ms",
                    self.config.timeout.as_millis()
                ))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.stop();
                Err("adapter stdout closed before a response".into())
            }
        }
    }
}

impl Drop for AdapterClient {
    fn drop(&mut self) {
        self.stop();
    }
}

static ADAPTERS: OnceLock<Mutex<HashMap<&'static str, AdapterClient>>> = OnceLock::new();

fn adapter_clients() -> &'static Mutex<HashMap<&'static str, AdapterClient>> {
    ADAPTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn run_adapter(
    runtime: &'static str,
    node_id: &str,
    config: &AdapterConfig,
    payload: &[u8],
) -> Result<AdapterResponse, ConnectError> {
    let mut clients = adapter_clients()
        .lock()
        .map_err(|_| ConnectError::AdapterExecution {
            runtime,
            node: node_id.into(),
            message: "adapter registry lock was poisoned".into(),
        })?;
    let needs_restart = clients
        .get_mut(&runtime)
        .map(|client| client.config != *config || !client.is_alive())
        .unwrap_or(true);
    if needs_restart {
        if let Some(mut old) = clients.remove(&runtime) {
            old.stop();
        }
        clients.insert(runtime, AdapterClient::spawn(runtime, config)?);
    }

    let response = clients
        .get_mut(&runtime)
        .expect("adapter is inserted above")
        .send(payload, node_id)
        .map_err(|message| ConnectError::AdapterExecution {
            runtime,
            node: node_id.into(),
            message,
        });
    if response.is_err() {
        clients.remove(&runtime);
    }
    let response = response?;
    serde_json::from_slice(&response).map_err(|error| ConnectError::AdapterProtocol {
        runtime,
        node: node_id.into(),
        message: format!("stdout was not one JSON response: {error}"),
    })
}

fn request_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("dx-{}-{}", std::process::id(), nanos)
}

fn execute_native(
    node_id: &str,
    mut context: NodeContext,
) -> Result<Vec<Vec<NodeItem>>, ConnectError> {
    let operation = node_id
        .rsplit('.')
        .next()
        .unwrap_or(node_id)
        .to_ascii_lowercase();
    match operation.as_str() {
        "noop" => Ok(vec![context.items]),
        "merge" => Ok(vec![context.items]),
        "set" => {
            let values = context.parameters.remove("values").ok_or_else(|| {
                ConnectError::InvalidParameter(node_id.into(), "values".into(), "an object")
            })?;
            let values = values.as_object().ok_or_else(|| {
                ConnectError::InvalidParameter(node_id.into(), "values".into(), "an object")
            })?;
            for item in &mut context.items {
                item.json.extend(values.clone());
            }
            Ok(vec![context.items])
        }
        "if" | "branch" => {
            let field = context
                .parameters
                .remove("field")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(|| {
                    ConnectError::InvalidParameter(node_id.into(), "field".into(), "a string")
                })?;
            let expected = context
                .parameters
                .remove("equals")
                .unwrap_or(Value::Bool(true));
            let mut yes = Vec::new();
            let mut no = Vec::new();
            for item in context.items {
                if item.json.get(&field) == Some(&expected) {
                    yes.push(item);
                } else {
                    no.push(item);
                }
            }
            Ok(vec![yes, no])
        }
        _ => Err(ConnectError::UnknownNode(node_id.into())),
    }
}

/// Small JSON description suitable for the Connect tab and AI tool discovery.
pub fn catalog_json() -> Value {
    json!({
        "schema_version": 1,
        "sources": {
            "flow_like": { "adapter": "flow-like-wasm", "configured": adapter_is_configured("Flow-Like") },
            "n8n": { "adapter": "n8n-node-runtime", "configured": adapter_is_configured("n8n") }
        },
        "nodes": catalog()
    })
}

fn adapter_is_configured(runtime: &'static str) -> bool {
    match runtime {
        "Flow-Like" => std::env::var_os("DX_FLOW_LIKE_ADAPTER").is_some(),
        "n8n" => {
            std::env::var_os("DX_N8N_ADAPTER").is_some()
                || std::env::var_os("DX_N8N_ROOT").is_some()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_merges_values_without_losing_existing_fields() {
        let mut json = Map::new();
        json.insert("id".into(), 1.into());
        let mut parameters = Map::new();
        parameters.insert("values".into(), json!({"status": "ready"}));
        let output = execute(
            "dx.set",
            NodeContext {
                items: vec![NodeItem { json }],
                parameters,
                ..NodeContext::default()
            },
        )
        .unwrap();
        assert_eq!(output[0][0].json["id"], 1);
        assert_eq!(output[0][0].json["status"], "ready");
    }

    #[test]
    fn if_routes_to_two_outputs() {
        let mut yes = Map::new();
        yes.insert("ok".into(), true.into());
        let mut no = Map::new();
        no.insert("ok".into(), false.into());
        let mut parameters = Map::new();
        parameters.insert("field".into(), "ok".into());
        let output = execute(
            "dx.if",
            NodeContext {
                items: vec![NodeItem { json: yes }, NodeItem { json: no }],
                parameters,
                ..NodeContext::default()
            },
        )
        .unwrap();
        assert_eq!(output[0].len(), 1);
        assert_eq!(output[1].len(), 1);
    }

    #[test]
    fn unsupported_source_is_explicit() {
        assert!(matches!(
            execute("n8n-nodes-base.httpRequest", NodeContext::default()).unwrap_err(),
            ConnectError::AdapterUnavailable { runtime: "n8n", .. }
        ));
    }

    #[test]
    fn imported_catalog_contains_all_checked_out_n8n_directories() {
        let n8n_count = catalog()
            .iter()
            .filter(|node| node.source == NodeSource::N8n)
            .count();
        // Installed DX inventories may contain more than the checked-out
        // fallback list; the minimum protects the imported baseline without
        // rejecting a real LOCALAPPDATA/dx/connects inventory.
        assert!(
            n8n_count >= 308,
            "expected at least the imported n8n baseline"
        );
    }

    #[test]
    fn imported_core_nodes_run_through_dx_native_semantics() {
        let mut json = Map::new();
        json.insert("ok".into(), true.into());
        let mut parameters = Map::new();
        parameters.insert("field".into(), "ok".into());
        let output = execute(
            "n8n-nodes-base.If",
            NodeContext {
                items: vec![NodeItem { json }],
                parameters,
                ..NodeContext::default()
            },
        )
        .unwrap();
        assert_eq!(output[0].len(), 1);
        assert!(output[1].is_empty());
    }
}
