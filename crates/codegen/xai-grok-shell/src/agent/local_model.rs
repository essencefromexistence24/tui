//! Managed local model server — auto-starts and manages `llama-server`
//! as a child process when a local model is selected.
//!
//! The user never sees or touches `llama-server` directly. The app:
//! 1. Detects a local model by its loopback `base_url`
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
    HANDLE.get_or_init(LocalServerHandle::new)
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

        let log_path = std::env::temp_dir().join(format!("grok-llama-server-{port}.log"));
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&log_path)
            .map_err(|e| {
                format!(
                    "failed to create llama-server log {}: {e}",
                    log_path.display()
                )
            })?;
        let stderr_log = log_file
            .try_clone()
            .map_err(|e| format!("failed to clone llama-server log handle: {e}"))?;

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
            "mmap",
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
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(stderr_log));

        detach_command(&mut cmd);

        // Spawn and enroll with the global process scope so the server is
        // reaped on shutdown even if the session actor is no longer running.
        let (mut child, process_group) = global_process_scope()
            .spawn(cmd)
            .map_err(|e| format!("failed to spawn llama-server: {e}"))?;
        state.process_group = Some(process_group);

        // Loading can take longer on CPU-only Windows hosts, especially while
        // antivirus scans the GGUF mapping. Fail immediately if the process
        // exits, otherwise allow a bounded two-minute readiness window.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);

        while tokio::time::Instant::now() < deadline {
            if check_health(port).await {
                state.port = port;
                state.started = true;
                tracing::info!(port = port, "llama-server ready");
                return Ok(port);
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|e| format!("failed to inspect llama-server process: {e}"))?
            {
                state.process_group = None;
                return Err(format!(
                    "llama-server exited with {status}: {}",
                    read_log_tail(&log_path)
                ));
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        let _ = child.kill().await;
        state.process_group = None;
        let err = format!(
            "llama-server failed to start within 120s: {}",
            read_log_tail(&log_path)
        );
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
/// Returns the port the server is listening on, or an actionable error when the
/// model is not local or its managed server cannot be started.
pub(crate) async fn ensure_local_server(model_key: &str, base_url: &str) -> Result<u16, String> {
    let config = local_server_config_for_model(model_key, base_url).ok_or_else(|| {
        format!(
            "no local GGUF matching '{model_key}' or usable llama-server was found; \
             set GROK_LOCAL_MODELS_DIR or place the model under a DX flow models directory"
        )
    })?;
    let handle = global_handle();
    handle.ensure_running(&config).await
}

/// Whether an inference endpoint is hosted on this machine.
///
/// Parse the URL instead of using substring matching so a remote hostname such
/// as `localhost.example.com` can never be mistaken for a trusted local server.
pub(crate) fn is_local_base_url(base_url: &str) -> bool {
    let Ok(url) = url::Url::parse(base_url) else {
        return false;
    };
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
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
    if !is_local_base_url(base_url) {
        return None;
    }

    let server_path = resolve_llama_server_path()?;
    let port = url::Url::parse(base_url)
        .ok()
        .and_then(|url| url.port_or_known_default())
        .unwrap_or(8080);

    let model_file = candidate_model_dirs()
        .into_iter()
        .find_map(|dir| find_model_file(&dir, model_key))?;

    Some(LocalServerConfig {
        server_path,
        model_path: model_file,
        // A 131K KV cache is unnecessary for the compact local profile and
        // can prevent a 1B model from starting on ordinary Windows machines.
        // 32K comfortably fits agent turns (a request can exceed 8K) while
        // keeping the KV cache bounded. Keep in sync with the catalog's
        // context_window for the local models so the client compacts before
        // the server rejects the request.
        context_size: 32_768,
        threads: 6,
        port,
    })
}

fn candidate_model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for name in ["GROK_LOCAL_MODELS_DIR", "DX_FLOW_MODELS_DIR"] {
        if let Some(path) = std::env::var_os(name) {
            dirs.push(PathBuf::from(path));
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let root = PathBuf::from(home).join(".dx").join("flow").join("models");
        dirs.push(root.join("llm"));
        dirs.push(root);
    }
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(4) {
            let root = ancestor.join("flow").join("models");
            dirs.push(root.join("llm"));
            dirs.push(root);
        }
    }
    dirs.dedup();
    dirs
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

fn read_log_tail(path: &PathBuf) -> String {
    const MAX_BYTES: usize = 8 * 1024;
    let Ok(bytes) = std::fs::read(path) else {
        return format!("see {}", path.display());
    };
    let start = bytes.len().saturating_sub(MAX_BYTES);
    String::from_utf8_lossy(&bytes[start..]).trim().to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_lookup_matches_catalog_slug_to_gguf_name() {
        let dir = tempfile::tempdir().expect("temp model dir");
        let expected = dir
            .path()
            .join("MiniCPM5-1B-Agentic-Tooluse-Nemotron-DPO.Q4_K_M.gguf");
        std::fs::File::create(&expected).expect("create fake GGUF");

        assert_eq!(
            find_model_file(&dir.path().to_path_buf(), "minicpm5-1b-tooluse"),
            Some(expected)
        );
    }

    #[test]
    fn local_catalog_suffix_is_not_required_in_filename() {
        assert_eq!(
            normalized_model_tokens("qwen2.5-coder-1.5b-local"),
            vec!["qwen2", "5", "coder", "1", "5b"]
        );
    }

    #[test]
    fn local_endpoint_detection_requires_an_actual_loopback_host() {
        assert!(is_local_base_url("http://localhost:8080/v1"));
        assert!(is_local_base_url("http://127.0.0.1:8080/v1"));
        assert!(is_local_base_url("http://[::1]:8080/v1"));
        assert!(!is_local_base_url("https://localhost.example.com/v1"));
        assert!(!is_local_base_url("https://example.com/localhost"));
    }
}
