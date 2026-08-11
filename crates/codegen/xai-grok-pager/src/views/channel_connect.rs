//! Real ZeroClaw channel inventory used by the Extensions → Connect tab.
//!
//! This module deliberately delegates channel identity and compiled support to
//! ZeroClaw's `listing` registry. It does not maintain a second hand-written
//! list of fake integrations.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorPhase {
    Stopped,
    Starting,
    Running,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorStatus {
    pub phase: SupervisorPhase,
    pub message: Option<String>,
    pub qr_channel: Option<String>,
    pub qr_payload: Option<String>,
}

struct SupervisorHandle {
    cancel: tokio_util::sync::CancellationToken,
    thread: std::thread::JoinHandle<()>,
}

static SUPERVISOR: OnceLock<Mutex<Option<SupervisorHandle>>> = OnceLock::new();
static SUPERVISOR_STATUS: OnceLock<Arc<Mutex<SupervisorStatus>>> = OnceLock::new();
static OUTBOUND_STATUS: OnceLock<Arc<Mutex<Option<String>>>> = OnceLock::new();
static INBOUND: OnceLock<tokio::sync::broadcast::Sender<zeroclaw_api::channel::ChannelMessage>> =
    OnceLock::new();
static SESSION_BINDINGS: OnceLock<
    Mutex<std::collections::HashMap<String, crate::app::agent::AgentId>>,
> = OnceLock::new();
static SESSION_ROUTES: OnceLock<
    Mutex<std::collections::HashMap<crate::app::agent::AgentId, (String, String)>>,
> = OnceLock::new();

fn supervisor_slot() -> &'static Mutex<Option<SupervisorHandle>> {
    SUPERVISOR.get_or_init(|| Mutex::new(None))
}

fn supervisor_status_slot() -> &'static Arc<Mutex<SupervisorStatus>> {
    SUPERVISOR_STATUS.get_or_init(|| {
        Arc::new(Mutex::new(SupervisorStatus {
            phase: SupervisorPhase::Stopped,
            message: None,
            qr_channel: None,
            qr_payload: None,
        }))
    })
}

fn outbound_status_slot() -> &'static Arc<Mutex<Option<String>>> {
    OUTBOUND_STATUS.get_or_init(|| Arc::new(Mutex::new(None)))
}

fn inbound_sender() -> &'static tokio::sync::broadcast::Sender<zeroclaw_api::channel::ChannelMessage>
{
    INBOUND.get_or_init(|| tokio::sync::broadcast::channel(256).0)
}

fn session_bindings()
-> &'static Mutex<std::collections::HashMap<String, crate::app::agent::AgentId>> {
    SESSION_BINDINGS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn session_routes()
-> &'static Mutex<std::collections::HashMap<crate::app::agent::AgentId, (String, String)>> {
    SESSION_ROUTES.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Bind one configured ZeroClaw channel to one TUI session. The binding is
/// keyed by the canonical `<type>.<alias>` identity used by ZeroClaw.
pub fn bind_session(channel_id: String, agent_id: crate::app::agent::AgentId) {
    session_bindings()
        .lock()
        .expect("channel session binding mutex poisoned")
        .insert(channel_id, agent_id);
}

pub fn bind_session_route(
    channel_id: String,
    recipient: String,
    agent_id: crate::app::agent::AgentId,
) {
    bind_session(channel_id.clone(), agent_id);
    session_routes()
        .lock()
        .expect("channel session route mutex poisoned")
        .insert(agent_id, (channel_id, recipient));
}

pub fn send_latest_response_async(
    app: &crate::app::app_view::AppView,
    agent_id: crate::app::agent::AgentId,
) {
    let Some((channel_id, recipient)) = session_routes()
        .lock()
        .expect("channel session route mutex poisoned")
        .get(&agent_id)
        .cloned()
    else { return };
    let Some(agent) = app.agents.get(&agent_id) else { return };
    let entries: Vec<_> = agent.scrollback.iter_entries().collect();
    let Some(message) = entries.into_iter().rev().find_map(|(_, entry)| {
        matches!(&entry.block, crate::scrollback::block::RenderBlock::AgentMessage(_))
            .then(|| entry.block.copy_text(false))
            .flatten()
    }) else { return };
    if !message.trim().is_empty() {
        send_message_async(channel_id, recipient, message);
    }
}

pub fn session_binding(channel_id: &str) -> Option<crate::app::agent::AgentId> {
    session_bindings()
        .lock()
        .expect("channel session binding mutex poisoned")
        .get(channel_id)
        .copied()
}

pub fn subscribe_inbound() -> tokio::sync::broadcast::Receiver<zeroclaw_api::channel::ChannelMessage>
{
    inbound_sender().subscribe()
}

#[derive(Debug, Clone)]
pub struct ChannelSetupField {
    pub path: String,
    pub label: String,
    pub description: String,
    pub secret: bool,
    pub initial_value: String,
}

#[derive(Debug, Clone)]
pub struct ChannelEntry {
    pub kind: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub configured: bool,
}

#[derive(Debug, Clone)]
pub struct ChannelConnectState {
    pub entries: Vec<ChannelEntry>,
    pub selected: usize,
    pub error: Option<String>,
    pub supervisor: SupervisorStatus,
    pub outbound_status: Option<String>,
}

impl Default for ChannelConnectState {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelConnectState {
    pub fn new() -> Self {
        let mut state = Self {
            entries: Vec::new(),
            selected: 0,
            error: None,
            supervisor: supervisor_status(),
            outbound_status: outbound_status(),
        };
        state.refresh();
        state
    }

    /// Reload readiness from ZeroClaw's canonical config loader.
    pub fn refresh(&mut self) {
        match load_agent_config() {
            Ok(config) => {
                self.entries = zeroclaw_channels::listing::compiled_channels(&config.channels)
                    .into_iter()
                    .map(|channel| ChannelEntry {
                        kind: channel.kind,
                        name: channel.name,
                        description: channel.desc,
                        configured: channel.configured,
                    })
                    .collect();
                self.error = None;
            }
            Err(error) => {
                self.entries.clear();
                self.error = Some(error.to_string());
            }
        }
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        self.supervisor = supervisor_status();
        self.outbound_status = outbound_status();
    }

    pub fn selected_entry(&self) -> Option<&ChannelEntry> {
        self.entries.get(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() - 1;
        self.selected = if delta.is_negative() {
            self.selected.saturating_sub(delta.unsigned_abs())
        } else {
            self.selected.saturating_add(delta as usize).min(last)
        };
    }

    /// The shared ZeroClaw config path. The Connect tab opens this path through
    /// the TUI's existing external-editor action for schema-specific setup.
    pub fn config_path() -> PathBuf {
        load_agent_config()
            .map(|config| config.config_path)
            .unwrap_or_else(|_| {
                std::env::var_os("ZEROCLAW_CONFIG")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        dirs::home_dir()
                            .unwrap_or_else(|| PathBuf::from("."))
                            .join(".zeroclaw")
                            .join("config.toml")
                    })
            })
    }

    pub fn setup_fields(kind: &str, alias: &str) -> Result<Vec<ChannelSetupField>> {
        let mut config = load_agent_config()?;
        let section = config_section(kind);
        let prefix = format!("channels.{section}.{alias}");
        config.ensure_map_key_for_path(&format!("{prefix}.enabled"));
        let fields = config
            .prop_fields()
            .into_iter()
            .filter(|field| field.name.starts_with(&format!("{prefix}.")))
            .map(|field| ChannelSetupField {
                path: field.name.clone(),
                label: field
                    .name
                    .rsplit('.')
                    .next()
                    .unwrap_or("value")
                    .replace('_', "-"),
                description: field.description.to_string(),
                secret: field.is_secret,
                initial_value: if field.is_secret {
                    String::new()
                } else {
                    field.display_value
                },
            })
            .collect::<Vec<_>>();
        if fields.is_empty() {
            return Err(anyhow!("No configurable fields found for channel `{kind}`"));
        }
        Ok(fields)
    }

    pub fn save_setup(kind: &str, alias: &str, fields: &[(String, String)]) -> Result<()> {
        let kind = kind.to_owned();
        let alias = alias.to_owned();
        let fields = fields.to_owned();
        std::thread::Builder::new()
            .name("dx-channel-config-write".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("failed to initialize ZeroClaw config runtime")?;
                runtime.block_on(async move {
                    let mut config = zeroclaw_config::schema::Config::load_or_init().await?;
                    let section = config_section(&kind);
                    config.ensure_map_key_for_path(&format!("channels.{section}.{alias}.enabled"));
                    let known = config
                        .prop_fields()
                        .into_iter()
                        .map(|field| (field.name, field.is_secret))
                        .collect::<std::collections::HashMap<_, _>>();
                    for (path, value) in &fields {
                        let value = value.trim();
                        if value.is_empty() {
                            continue;
                        }
                        match known.get(path).copied() {
                            Some(true) => config.set_secret_persistent(path, value.to_string())?,
                            Some(false) => config.set_prop_persistent(path, value)?,
                            None => return Err(anyhow!("unknown channel config field `{path}`")),
                        }
                    }
                    config.set_prop_persistent(
                        &format!("channels.{section}.{alias}.enabled"),
                        "true",
                    )?;
                    config.save_dirty().await
                })
            })
            .context("failed to spawn channel config writer")?
            .join()
            .map_err(|_| anyhow!("channel config writer thread panicked"))?
    }
}

fn config_section(kind: &str) -> &str {
    match kind {
        "whatsapp-web" => "whatsapp",
        "nextcloud-talk" => "nextcloud_talk",
        "voice-call" => "voice_call",
        "voice-wake" => "voice_wake",
        _ => kind,
    }
}

pub fn supervisor_status() -> SupervisorStatus {
    supervisor_status_slot()
        .lock()
        .expect("channel supervisor status mutex poisoned")
        .clone()
}

pub fn outbound_status() -> Option<String> {
    outbound_status_slot()
        .lock()
        .expect("channel outbound status mutex poisoned")
        .clone()
}

/// Send one message through the real ZeroClaw channel implementation without
/// blocking the TUI render/input thread. `channel_id` is the canonical
/// `<type>.<alias>` key (or a bare singleton channel name).
pub fn send_message_async(channel_id: String, recipient: String, message: String) {
    *outbound_status_slot()
        .lock()
        .expect("channel outbound status mutex poisoned") =
        Some(format!("Sending via {channel_id}…"));
    std::thread::Builder::new()
        .name("dx-channel-send".into())
        .spawn(move || {
            let result = (|| -> Result<()> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("failed to initialize channel send runtime")?;
                let config = load_agent_config()?;
                runtime.block_on(zeroclaw_channels::orchestrator::send_channel_message(
                    &config,
                    &channel_id,
                    &recipient,
                    &message,
                ))
            })();
            *outbound_status_slot()
                .lock()
                .expect("channel outbound status mutex poisoned") = Some(match result {
                Ok(()) => format!("Message sent via {channel_id}"),
                Err(error) => format!("Channel send failed: {error}"),
            });
        })
        .map_err(|error| {
            *outbound_status_slot()
                .lock()
                .expect("channel outbound status mutex poisoned") =
                Some(format!("Failed to start channel send: {error}"));
        })
        .ok();
}

pub fn start_supervisor() -> Result<()> {
    let config = load_agent_config()?;
    let mut slot = supervisor_slot()
        .lock()
        .expect("channel supervisor mutex poisoned");
    if let Some(existing) = slot.as_ref()
        && !existing.thread.is_finished()
    {
        return Ok(());
    }
    slot.take();
    let cancel = tokio_util::sync::CancellationToken::new();
    let child_cancel = cancel.clone();
    let status = Arc::clone(supervisor_status_slot());
    *status
        .lock()
        .expect("channel supervisor status mutex poisoned") = SupervisorStatus {
        phase: SupervisorPhase::Starting,
        message: None,
        qr_channel: None,
        qr_payload: None,
    };
    let thread = std::thread::Builder::new()
        .name("dx-zeroclaw-channels".into())
        .spawn(move || {
            let thread_status = Arc::clone(&status);
            let runtime_status = Arc::clone(&thread_status);
            let result = (|| -> Result<()> {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .context("failed to initialize channel supervisor runtime")?;
                runtime.block_on(async move {
                    *runtime_status
                        .lock()
                        .expect("channel supervisor status mutex poisoned") = SupervisorStatus {
                        phase: SupervisorPhase::Running,
                        message: None,
                        qr_channel: None,
                        qr_payload: None,
                    };
                    let mut events = zeroclaw_log::subscribe_or_install();
                    let event_status = Arc::clone(&runtime_status);
                    let event_task = tokio::spawn(async move {
                        while let Ok(event) = events.recv().await {
                            let Some(login) =
                                event.get("attributes").and_then(|attrs| attrs.get("login"))
                            else {
                                continue;
                            };
                            let Some(payload) = login.get("qr_payload").and_then(|v| v.as_str())
                            else {
                                continue;
                            };
                            let channel = login
                                .get("channel")
                                .or_else(|| event.get("channel"))
                                .and_then(|v| v.as_str())
                                .map(str::to_owned);
                            let mut status = event_status
                                .lock()
                                .expect("channel supervisor status mutex poisoned");
                            status.qr_channel = channel;
                            status.qr_payload = Some(payload.to_owned());
                        }
                    });
                    let inbound = inbound_sender().clone();
                    let result = zeroclaw_channels::orchestrator::start_channels_with_ingress(
                        config,
                        None,
                        child_cancel,
                        None,
                        None,
                        Some(Arc::new(move |message| {
                            let _ = inbound.send(message);
                        })),
                    )
                    .await;
                    event_task.abort();
                    result
                })
            })();
            let mut state = thread_status
                .lock()
                .expect("channel supervisor status mutex poisoned");
            *state = match result {
                Ok(()) => SupervisorStatus {
                    phase: SupervisorPhase::Stopped,
                    message: None,
                    qr_channel: None,
                    qr_payload: None,
                },
                Err(error) => SupervisorStatus {
                    phase: SupervisorPhase::Failed,
                    message: Some(error.to_string()),
                    qr_channel: None,
                    qr_payload: None,
                },
            };
        })
        .context("failed to spawn channel supervisor")?;
    *slot = Some(SupervisorHandle { cancel, thread });
    Ok(())
}

pub fn stop_supervisor() {
    if let Some(handle) = supervisor_slot()
        .lock()
        .expect("channel supervisor mutex poisoned")
        .take()
    {
        handle.cancel.cancel();
    }
    *supervisor_status_slot()
        .lock()
        .expect("channel supervisor status mutex poisoned") = SupervisorStatus {
        phase: SupervisorPhase::Stopped,
        message: None,
        qr_channel: None,
        qr_payload: None,
    };
}

fn load_agent_config() -> anyhow::Result<zeroclaw_config::schema::Config> {
    // Connect-tab rendering and form actions can run on the TUI's Tokio
    // runtime. Config::load_or_init is async, so never call block_on directly
    // here; a nested runtime panics when Extensions is opened.
    std::thread::Builder::new()
        .name("dx-channel-config-read".into())
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(zeroclaw_config::schema::Config::load_or_init())
        })
        .context("failed to spawn channel config reader")?
        .join()
        .map_err(|_| anyhow::anyhow!("channel config reader thread panicked"))?
}
