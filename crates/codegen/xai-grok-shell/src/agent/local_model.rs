//! Managed local model server — auto-starts and manages `llama-server`
//! as a child process when a local model is selected.
//!
//! The user never sees or touches `llama-server` directly. The app:
//! 1. Detects a local model by `base_url` containing `localhost`
//! 2. Finds a free port
//! 3. Spawns `llama-server` as a managed subprocess
//! 4. Polls `/health` until the model is loaded
//! 5. Kills the server on shutdown via `global_process_scope()`

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Mutex;
use xai_grok_tools::util::{ProcessGroup, detach_command, global_process_scope};

/// Global singleton handle for the managed local server.
fn global_handle() -> &'static LocalServerHandle {
    static HANDLE: OnceLock<LocalServerHandle> = OnceLock::new();
    HANDLE.get_or_init(|| LocalServerHandle::new())
}

/// Configuration for a managed local inference server.
#[derive(Clone, Debug)]
pub(crate) struct LocalServerConfig {
    /// Path to the llama-server binary.
    pub server_path: PathBuf,
    /// Path to the GGUF model file.
    pub model_path: PathBuf,
    /// Context size in tokens.
    pub context_size: u64,
    /// Number of CPU threads for inference.
    pub threads: u16,
    /// Port from the model's configured base URL.
    pub port: u16,
}

/// Handle to a managed local server process.
/// Cloning shares the same underlying state — only one server runs at a time.
#[derive(Clone)]
pub(crate) struct LocalServerHandle {
    state: Arc<Mutex<ServerState>>,
}

struct ServerState {
    port: u16,
    started: bool,
    // ProcessScope stores only a Weak reference. Retain the strong owner for
    // the whole managed-server lifetime or ProcessGroup::drop kills the child
    // immediately after startup.
    process_group: Option<Arc<ProcessGroup>>,
}

impl LocalServerHandle {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ServerState {
                port: 0,
                started: false,
                process_group: None,
            })),
        }
    }

    /// Ensure the managed server is running. Returns the port it's listening on.
    /// If already running, health-checks and returns immediately.
    pub async fn ensure_running(&self, config: &LocalServerConfig) -> Result<u16, String> {
        let mut state = self.state.lock().await;

        if state.started {
            if check_health(state.port).await {
                return Ok(state.port);
            }
            // Server died — will restart below
            state.started = false;
            state.process_group = None;
        }

        // Sampling continues to use the configured URL, so starting on some
        // other free port would create a healthy but unreachable server.
        let port = config.port;
        // A server may already have been started manually or by another app
        // instance. Adopt it instead of treating its listening port as an
        // error. This is also the normal path after restarting the TUI.
        if check_health(port).await {
            state.port = port;
            state.started = true;
            tracing::info!(port, "using existing llama-server");
            return Ok(port);
        }
        if port_in_use(port).await {
            // llama-server opens its socket while the model may still be
            // loading. Wait for health instead of failing or racing the first
            // completion request against initialization.
            let deadline = tokio::time::Instant::now() + Duration::from_secs(65);
            while tokio::time::Instant::now() < deadline {
                if check_health(port).await {
                    state.port = port;
                    state.started = true;
                    tracing::info!(port, "existing llama-server became ready");
                    return Ok(port);
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            return Err(format!(
                "process on port {port} did not become a healthy llama-server within 65 seconds"
            ));
        }
        let model_path_str = config.model_path.to_string_lossy().to_string();
        let server_path_str = config.server_path.to_string_lossy().to_string();

        tracing::info!(
            port = port,
            model = %model_path_str,
            "starting managed llama-server"
        );

        let mut cmd = Command::new(&server_path_str);
        cmd.args([
            "-m",
            &model_path_str,
            "-c",
            &config.context_size.to_string(),
            "-t",
            &config.threads.to_string(),
            "--port",
            &port.to_string(),
            "--load-mode",
            "mlock",
            // This model can spend its entire completion in reasoning_content,
            // which the normal TUI hides. Disable thinking at the server so
            // every successful completion contains a visible answer.
            "--reasoning",
            "off",
            "--repeat-penalty",
            "1.1",
            "--repeat-last-n",
            "128",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

        detach_command(&mut cmd);

        let child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn llama-server: {e}"))?;

        // Register with global process scope for cleanup on exit
        if let Ok(mut group) = ProcessGroup::new() {
            group.attach(&child).ok();
            let arc = Arc::new(group);
            global_process_scope().register(&arc);
            state.process_group = Some(arc);
        }

        // Poll health endpoint up to 60s
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        let mut last_err = String::new();

        while tokio::time::Instant::now() < deadline {
            if check_health(port).await {
                state.port = port;
                state.started = true;
                tracing::info!(port = port, "llama-server ready");
                return Ok(port);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        let err = format!("llama-server failed to start within 60s: {last_err}");
        tracing::error!("{err}");
        Err(err)
    }
}

/// Quick health check against the llama-server `/health` endpoint.
async fn check_health(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok();
    match client {
        Some(c) => match c.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        },
        None => false,
    }
}

/// Ensure the local server is running for the given model. Call this
/// before creating a `SamplingClient` for a local model.
/// Returns the port the server is listening on, or `None` if the model
/// is not a local model or the server can't be started.
pub(crate) async fn ensure_local_server(model_key: &str, base_url: &str) -> Option<u16> {
    let config = local_server_config_for_model(model_key, base_url)?;
    let handle = global_handle();
    match handle.ensure_running(&config).await {
        Ok(port) => Some(port),
        Err(e) => {
            tracing::error!("failed to start local server: {e}");
            None
        }
    }
}

async fn port_in_use(port: u16) -> bool {
    use tokio::net::TcpStream;
    TcpStream::connect(("127.0.0.1", port)).await.is_ok()
}

/// Resolve the llama-server binary path.
pub(crate) fn resolve_llama_server_path() -> Option<PathBuf> {
    // Prefer the complete setup distribution. On Windows the executable
    // requires sibling runtime DLLs.
    let setup = std::env::temp_dir().join("llama").join("llama-server.exe");
    if setup.exists() {
        return Some(setup);
    }
    // Check PATH by iterating PATH dirs
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("llama-server.exe");
            if candidate.exists() {
                return Some(candidate);
            }
            let candidate_no_ext = dir.join("llama-server");
            if candidate_no_ext.exists() {
                return Some(candidate_no_ext);
            }
        }
    }
    let bundled = PathBuf::from("bin/llama-server.exe");
    if bundled.exists() {
        return Some(bundled);
    }
    None
}

/// Build config for a local model if the base_url is localhost.
pub(crate) fn local_server_config_for_model(
    model_key: &str,
    base_url: &str,
) -> Option<LocalServerConfig> {
    if !base_url.contains("localhost") && !base_url.contains("127.0.0.1") {
        return None;
    }

    let server_path = resolve_llama_server_path()?;
    let port = url::Url::parse(base_url)
        .ok()
        .and_then(|url| url.port_or_known_default())
        .unwrap_or(8080);

    // Resolve model path: check ~/.dx/flow/models/llm/ by convention
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    let model_dir = PathBuf::from(format!("{home}\\.dx\\flow\\models\\llm"));

    // Try to find a matching GGUF file in the model directory
    let model_file = find_model_file(&model_dir, model_key)?;

    Some(LocalServerConfig {
        server_path,
        model_path: model_file,
        context_size: if model_key.contains("qwen2.5") {
            // The compact request profile only needs a modest KV cache. A
            // 32K allocation makes CPU-only startup look hung on many PCs.
            8_192
        } else {
            131_072
        },
        threads: 6,
        port,
    })
}

fn find_model_file(dir: &PathBuf, key: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().map(|e| e == "gguf").unwrap_or(false) {
            let name = normalized_model_tokens(&path.file_stem()?.to_string_lossy());
            let wanted = normalized_model_tokens(key);
            if wanted.iter().all(|token| name.contains(token)) {
                return Some(path);
            }
        }
    }
    None
}

fn normalized_model_tokens(value: &str) -> Vec<String> {
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        // "local" describes the catalog slot, not the downloaded GGUF name.
        .filter(|part| part != "local")
        .collect()
}
