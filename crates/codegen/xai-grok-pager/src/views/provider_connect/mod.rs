pub mod input;
pub mod render;
mod router_api_key_providers;

use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::{future::Future, thread};
use toml_edit::{DocumentMut, Value};
use xai_grok_tools::util::grok_home::grok_home;

use crate::views::picker::PickerState;

// ---------------------------------------------------------------------------
// Tab enumeration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderTab {
    All,
    Free,
    Featured,
    Gateway,
    Enterprise,
    China,
    Other,
}

impl ProviderTab {
    pub const ALL: [ProviderTab; 7] = [
        ProviderTab::All,
        ProviderTab::Free,
        ProviderTab::Featured,
        ProviderTab::Gateway,
        ProviderTab::Enterprise,
        ProviderTab::China,
        ProviderTab::Other,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ProviderTab::All => "All",
            ProviderTab::Free => "Free",
            ProviderTab::Featured => "Featured",
            ProviderTab::Gateway => "Gateway",
            ProviderTab::Enterprise => "Enterprise",
            ProviderTab::China => "China",
            ProviderTab::Other => "Other",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            ProviderTab::All => 0,
            ProviderTab::Free => 1,
            ProviderTab::Featured => 2,
            ProviderTab::Gateway => 3,
            ProviderTab::Enterprise => 4,
            ProviderTab::China => 5,
            ProviderTab::Other => 6,
        }
    }

    pub fn from_index(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }
}

pub const TAB_LABELS: [&str; 7] = [
    "All",
    "Free",
    "Featured",
    "Gateway",
    "Enterprise",
    "China",
    "Other",
];

// ── Categorisation ──

pub fn categorize(provider: &ProviderDef) -> ProviderTab {
    if provider.auth_type == "none" || provider.auth_type == "optional" || provider.free == "true" {
        return ProviderTab::Free;
    }
    match provider.id.as_str() {
        // Featured – major global AI platforms
        "openai" | "anthropic" | "gemini" | "mistral" | "cohere" | "deepseek" | "meta-llama"
        | "xai" | "grok-web" | "perplexity" | "ai21" | "cerebras" | "groq" => ProviderTab::Featured,

        // Gateway – multi-model routers / aggregators
        "openrouter" | "deepinfra" | "fireworks" | "hyperbolic" | "aimlapi" | "orcarouter"
        | "tokenrouter" | "agentrouter" | "zenmux" | "zenmux-free" | "featherless-ai"
        | "freeaiapikey" => ProviderTab::Gateway,

        // China / East Asia
        "baidu"
        | "alibaba"
        | "tencent"
        | "moonshot"
        | "kimi"
        | "minimax"
        | "stepfun"
        | "glm"
        | "baichuan"
        | "doubao"
        | "iflytek"
        | "sparkdesk"
        | "sensenova"
        | "qianfan"
        | "volcengine"
        | "yi"
        | "zai"
        | "zai-web"
        | "xiaomi-mimo"
        | "coze"
        | "bailian-coding-plan"
        | "byteplus"
        | "qiniu"
        | "yuanbao-web" => ProviderTab::China,

        // Enterprise / Cloud
        "vertex" | "databricks" | "snowflake" | "cloudflare-ai" | "nvidia" | "ovhcloud"
        | "scaleway" | "vercel-ai-gateway" | "upstage" | "wandb" | "heroku" | "inference-net"
        | "predibase" => ProviderTab::Enterprise,

        _ => ProviderTab::Other,
    }
}

// ── Search ──

pub fn fuzzy_matches(text: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    let mut qi = q.chars().peekable();
    for c in lower.chars() {
        if qi.peek() == Some(&c) {
            qi.next();
            if qi.peek().is_none() {
                return true;
            }
        }
    }
    false
}

// ── Provider data types ──

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDef {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_backend: String,
    #[serde(default)]
    pub auth_scheme: String,
    #[serde(default)]
    pub env_key_hint: String,
    #[serde(default)]
    pub auth_type: String,
    #[serde(default)]
    pub model_count: u32,
    #[serde(default)]
    pub free: String,
}

impl ProviderDef {
    pub fn status(&self, configured: &[String]) -> ProviderStatus {
        if self.auth_type == "none" || self.auth_type == "optional" || self.free == "true" {
            ProviderStatus::Free
        } else if configured.contains(&self.id) {
            ProviderStatus::Configured
        } else {
            ProviderStatus::NotConfigured
        }
    }

    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.id
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStatus {
    NotConfigured,
    Configured,
    Free,
}

#[derive(Debug, Clone)]
pub enum ConnectMode {
    Browse,
    KeyInput {
        provider_id: String,
        input_buffer: String,
        set_default: bool,
    },
    OAuth {
        provider_id: String,
        job: Arc<Mutex<Option<Result<(), String>>>>,
    },
}

#[derive(Debug, Clone)]
pub struct ProviderConnectState {
    pub providers: Vec<ProviderDef>,
    pub free_providers: Vec<ProviderDef>,
    pub configured_ids: Vec<String>,
    pub picker: PickerState,
    pub mode: ConnectMode,
    pub status_message: Option<String>,
    pub error_message: Option<String>,
    pub window: crate::views::modal_window::ModalWindowState,
    pub active_tab: ProviderTab,
}

fn oauth_provider(id: &str) -> Option<&'static str> {
    match id {
        "openai-codex" => Some("openai"),
        "gemini-oauth" => Some("gemini"),
        "copilot" => Some("copilot"),
        _ => None,
    }
}

fn oauth_refresh_provider(id: &str) -> Option<&'static str> {
    match id {
        "qwen-oauth" => Some("qwen"),
        "minimax-oauth" => Some("minimax"),
        _ => None,
    }
}

fn user_home() -> std::path::PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| grok_home())
}

fn external_oauth_cache_exists(provider_id: &str) -> bool {
    match provider_id {
        "qwen-oauth" => user_home().join(".qwen").join("oauth_creds.json").is_file(),
        "gemini_cli" => user_home()
            .join(".gemini")
            .join("oauth_creds.json")
            .is_file(),
        _ => false,
    }
}

/// Run a ZeroClaw async operation without creating or blocking a Tokio runtime
/// on the pager's event-loop thread. Provider configuration is invoked from
/// synchronous picker handlers, while the pager itself may already be inside
/// Tokio; spawning a dedicated short-lived worker avoids nested-runtime
/// panics and keeps the UI runtime ownership in one place.
fn run_agent_operation<T, F, Fut>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, String>> + Send + 'static,
{
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to initialize Agent auth runtime: {e}"))?;
        runtime.block_on(operation())
    })
    .join()
    .map_err(|_| "Agent auth worker panicked while saving credentials".to_string())?
}

fn save_oauth_refresh_token(provider_id: &str, token: &str) -> Result<(), String> {
    let provider = oauth_refresh_provider(provider_id)
        .ok_or_else(|| "Unsupported OAuth refresh-token provider".to_string())?;
    let token = token.trim().to_string();
    run_agent_operation(move || async move {
        let mut config = zeroclaw_config::schema::Config::load_or_init()
            .await
            .map_err(|e| format!("Failed to load Agent config: {e}"))?;
        let base = format!("providers.models.{provider}.default");
        config
            .set_prop_persistent(&format!("{base}.auth_mode"), "oauth")
            .map_err(|e| format!("Failed to configure OAuth mode: {e}"))?;
        config
            .set_secret_persistent(&format!("{base}.oauth_refresh_token"), token)
            .map_err(|e| format!("Failed to protect OAuth refresh token: {e}"))?;
        config
            .save_dirty()
            .await
            .map_err(|e| format!("Failed to save Agent OAuth configuration: {e}"))
    })
}

pub(crate) fn start_oauth_job(provider_id: &str) -> Option<Arc<Mutex<Option<Result<(), String>>>>> {
    let provider = oauth_provider(provider_id)?.to_string();
    let job = Arc::new(Mutex::new(None));
    let result = Arc::clone(&job);
    std::thread::spawn(move || {
        let outcome = (|| -> Result<(), String> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("OAuth runtime initialization failed: {e}"))?;
            runtime.block_on(async move {
                let client = reqwest::Client::new();
                let service = zeroclaw_auth_service();
                if provider == "copilot" {
                    let token =
                        zeroclaw_providers::copilot::CopilotModelProvider::builder("default")
                            .build()
                            .device_code_login_for_frontend()
                            .await
                            .map_err(|e| format!("GitHub Copilot OAuth failed: {e}"))?;
                    service
                        .store_model_provider_token(
                            "copilot",
                            "default",
                            &token,
                            std::collections::HashMap::new(),
                            true,
                        )
                        .await
                        .map_err(|e| format!("Saving Copilot credentials failed: {e}"))?;
                    return Ok(());
                }
                let (token_set, account_id) = if provider == "openai" {
                    let tokens = match
                        zeroclaw_providers::auth::openai_oauth::start_device_code_flow(&client)
                            .await
                    {
                        Ok(device) => {
                            open_verification_page(
                                device
                                    .verification_uri_complete
                                    .as_deref()
                                    .unwrap_or(&device.verification_uri),
                            );
                            zeroclaw_providers::auth::openai_oauth::poll_device_code_tokens(
                                &client, &device,
                            )
                            .await
                            .map_err(|e| format!("OpenAI OAuth failed: {e}"))?
                        }
                        Err(device_error) => {
                            // OpenAI may reject the device-code endpoint for
                            // this client with 403. Fall back to the browser
                            // authorization-code flow, which uses PKCE and
                            // the localhost callback already supported by
                            // ZeroClaw's OpenAI auth implementation.
                            let pkce =
                                zeroclaw_providers::auth::openai_oauth::generate_pkce_state();
                            let authorize_url =
                                zeroclaw_providers::auth::openai_oauth::build_authorize_url(&pkce);
                            // Bind before launching the browser. A logged-in
                            // browser can follow the callback immediately;
                            // binding afterward loses that callback and makes
                            // a successful login appear to have failed.
                            let listener =
                                zeroclaw_providers::auth::openai_oauth::bind_loopback_listener()
                                    .await
                                    .map_err(|e| format!("OpenAI OAuth callback setup failed: {e}"))?;
                            open_verification_page(&authorize_url);
                            let code = zeroclaw_providers::auth::openai_oauth::receive_loopback_code_on_listener(
                                    listener,
                                    &pkce.state,
                                    std::time::Duration::from_secs(300),
                                )
                                .await
                                .map_err(|e| {
                                    format!(
                                        "OpenAI browser OAuth failed after device-code fallback ({device_error}): {e}"
                                    )
                                })?;
                            zeroclaw_providers::auth::openai_oauth::exchange_code_for_tokens(
                                &client, &code, &pkce,
                            )
                            .await
                            .map_err(|e| format!("OpenAI OAuth token exchange failed: {e}"))?
                        }
                    };
                    let account =
                        zeroclaw_providers::auth::openai_oauth::extract_account_id_from_jwt(
                            &tokens.access_token,
                        );
                    (tokens, account)
                } else {
                    let client_id = std::env::var("GEMINI_OAUTH_CLIENT_ID")
                        .map_err(|_| "Gemini OAuth requires GEMINI_OAUTH_CLIENT_ID".to_string())?;
                    let client_secret =
                        std::env::var("GEMINI_OAUTH_CLIENT_SECRET").map_err(|_| {
                            "Gemini OAuth requires GEMINI_OAUTH_CLIENT_SECRET".to_string()
                        })?;
                    let device = zeroclaw_providers::auth::gemini_oauth::start_device_code_flow(
                        &client, &client_id,
                    )
                    .await
                    .map_err(|e| format!("Gemini OAuth start failed: {e}"))?;
                    open_verification_page(
                        device
                            .verification_uri_complete
                            .as_deref()
                            .unwrap_or(&device.verification_uri),
                    );
                    let tokens = zeroclaw_providers::auth::gemini_oauth::poll_device_code_tokens(
                        &client,
                        &client_id,
                        &client_secret,
                        &device,
                    )
                    .await
                    .map_err(|e| format!("Gemini OAuth failed: {e}"))?;
                    let account = tokens.id_token.as_deref().and_then(
                        zeroclaw_providers::auth::gemini_oauth::extract_account_email_from_id_token,
                    );
                    (tokens, account)
                };
                if provider == "openai" {
                    service
                        .store_openai_tokens("default", token_set, account_id, true)
                        .await
                        .map_err(|e| format!("Saving OpenAI credentials failed: {e}"))?;
                } else {
                    service
                        .store_gemini_tokens("default", token_set, account_id, true)
                        .await
                        .map_err(|e| format!("Saving Gemini credentials failed: {e}"))?;
                }
                Ok(())
            })
        })();
        if let Ok(mut slot) = result.lock() {
            *slot = Some(outcome);
        }
    });
    Some(job)
}

pub(crate) fn poll_oauth_job(state: &mut ProviderConnectState) {
    let ConnectMode::OAuth { provider_id, job } = &state.mode else {
        return;
    };
    let result = job.lock().ok().and_then(|mut slot| slot.take());
    let Some(result) = result else { return };
    let name = provider_id.clone();
    match result {
        Ok(()) => {
            state.mode = ConnectMode::Browse;
            state.configured_ids = load_configured_providers();
            state.status_message = Some(format!("{name} OAuth connected."));
            state.error_message = None;
        }
        Err(error) => {
            // Keep the OAuth screen open on failure. The previous behavior
            // returned to Browse immediately, making a failed device-code
            // start or token poll look like a cancelled flow and hiding the
            // actionable provider error behind the preview list.
            state.error_message = Some(error);
            state.status_message = None;
        }
    }
}

fn open_verification_page(url: &str) {
    // OAuth authorize URLs contain query separators (`&`). Validate the
    // destination before handing it to the platform launcher; on Windows we
    // must not route it through `cmd /C start`, because cmd.exe interprets
    // every unquoted `&name=value` as another command.
    let Ok(parsed) = url::Url::parse(url) else {
        tracing::warn!("Refusing to open invalid OAuth verification URL");
        return;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        tracing::warn!(scheme = parsed.scheme(), "Refusing non-web OAuth URL");
        return;
    }
    #[cfg(target_os = "windows")]
    {
        // Use Windows' URL protocol handler directly. `explorer.exe` can
        // treat a URL as a shell/file target on some configurations and open
        // the file picker instead of the default browser. Passing the URL as
        // one argument also preserves PKCE query parameters containing `&`.
        if let Err(error) = std::process::Command::new("rundll32.exe")
            .args(["url.dll,FileProtocolHandler", parsed.as_str()])
            .spawn()
        {
            tracing::warn!(%error, "failed to open OAuth URL with Windows URL handler");
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

/// Owned entry data returned by [`ProviderConnectState::picker_entry_data`].
/// Callers convert to `Vec<PickerEntry<'_>>` by calling [`PickerEntryData::into_entries`].
pub struct PickerEntryData {
    pub labels: Vec<String>,
    pub badges: Vec<&'static str>,
    pub badge_colors: Vec<Option<ratatui::style::Color>>,
    pub non_sel: Vec<bool>,
    pub group_keys: Vec<Option<String>>,
    pub collapsible: Vec<bool>,
    pub indents: Vec<u8>,
    pub dimmed: Vec<bool>,
}

impl PickerEntryData {
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

impl Default for ProviderConnectState {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderConnectState {
    pub fn new() -> Self {
        let all_providers = load_community_providers();
        let mut free_providers = Vec::new();
        let mut api_providers = Vec::new();
        for p in all_providers {
            if p.auth_type == "none" || p.auth_type == "optional" || p.free == "true" {
                free_providers.push(p);
            } else {
                api_providers.push(p);
            }
        }
        free_providers.sort_by(|a, b| a.display_name().cmp(b.display_name()));
        api_providers.sort_by(|a, b| a.display_name().cmp(b.display_name()));
        Self {
            providers: api_providers,
            free_providers,
            configured_ids: load_configured_providers(),
            picker: PickerState::with_mode(crate::views::picker::PickerMode::Popup(
                crate::views::picker::PopupConfig {
                    width_pct: 0.85,
                    height_pct: 0.7,
                    min_width: 50,
                    min_height: 16,
                },
            )),
            mode: ConnectMode::Browse,
            status_message: None,
            error_message: None,
            window: crate::views::modal_window::ModalWindowState::new(),
            active_tab: ProviderTab::All,
        }
    }

    pub fn switch_tab(&mut self, tab: ProviderTab) {
        self.active_tab = tab;
        self.picker.selected = 0;
        self.picker.scroll_offset = None;
        self.picker.search_active = false;
        self.picker.tabs_focused = false;
        self.picker.selection_hidden = false;
        self.picker.expanded.clear();
        self.picker.hovered = None;
        self.error_message = None;
        self.status_message = None;
    }

    /// Build picker entry data (owned label strings + metadata).
    /// Callers use these to build `PickerEntry` rows in their own scope.
    pub fn picker_entry_data(
        free_providers: &[ProviderDef],
        providers: &[ProviderDef],
        configured_ids: &[String],
        active_tab: ProviderTab,
        query: &str,
    ) -> PickerEntryData {
        let mut labels: Vec<String> = Vec::new();
        let mut badges: Vec<&'static str> = Vec::new();
        let mut badge_colors: Vec<Option<ratatui::style::Color>> = Vec::new();
        let mut non_sel: Vec<bool> = Vec::new();
        let mut group_keys: Vec<Option<String>> = Vec::new();
        let mut collapsible: Vec<bool> = Vec::new();
        let mut indents: Vec<u8> = Vec::new();
        let mut dimmed: Vec<bool> = Vec::new();
        let dim = ratatui::style::Color::DarkGray;
        let grn = ratatui::style::Color::Green;
        let ylw = ratatui::style::Color::Yellow;

        let cat: Vec<&ProviderDef> = match active_tab {
            ProviderTab::All => free_providers.iter().chain(providers.iter()).collect(),
            ProviderTab::Free => free_providers
                .iter()
                .chain(providers.iter())
                .filter(|p| categorize(p) == ProviderTab::Free)
                .collect(),
            tab => free_providers
                .iter()
                .chain(providers.iter())
                .filter(|p| categorize(p) == tab)
                .collect(),
        };

        let mut matched: Vec<&ProviderDef> = cat
            .into_iter()
            .filter(|p| fuzzy_matches(p.display_name(), query))
            .collect();
        matched.sort_by(|a, b| a.display_name().cmp(b.display_name()));

        if matched.is_empty() {
            let msg = if query.is_empty() {
                if active_tab == ProviderTab::All {
                    "No providers available"
                } else {
                    "No providers in this category"
                }
            } else {
                "No providers match your search"
            };
            labels.push(msg.to_string());
            badges.push("");
            badge_colors.push(None);
            non_sel.push(true);
            group_keys.push(None);
            collapsible.push(false);
            indents.push(0);
            dimmed.push(true);
        } else if active_tab != ProviderTab::All {
            // Single-category tab: show a header row then the providers.
            labels.push(active_tab.label().to_string());
            badges.push("");
            badge_colors.push(None);
            non_sel.push(true);
            group_keys.push(None);
            collapsible.push(false);
            indents.push(0);
            dimmed.push(false);

            for p in &matched {
                let st = p.status(configured_ids);
                let (b, bc) = match &st {
                    ProviderStatus::Free => ("Free", Some(dim)),
                    ProviderStatus::Configured => ("Configured", Some(grn)),
                    ProviderStatus::NotConfigured => ("Configure", Some(ylw)),
                };
                labels.push(p.display_name().to_string());
                badges.push(b);
                badge_colors.push(bc);
                non_sel.push(false);
                group_keys.push(None);
                collapsible.push(false);
                indents.push(1);
                dimmed.push(false);
            }
        } else {
            // "All" tab: flat list of every provider, no category headers.
            for p in &matched {
                let st = p.status(configured_ids);
                let (b, bc) = match &st {
                    ProviderStatus::Free => ("Free", Some(dim)),
                    ProviderStatus::Configured => ("Configured", Some(grn)),
                    ProviderStatus::NotConfigured => ("Configure", Some(ylw)),
                };
                labels.push(p.display_name().to_string());
                badges.push(b);
                badge_colors.push(bc);
                non_sel.push(false);
                group_keys.push(None);
                collapsible.push(false);
                indents.push(0);
                dimmed.push(false);
            }
        }

        PickerEntryData {
            labels,
            badges,
            badge_colors,
            non_sel,
            group_keys,
            collapsible,
            indents,
            dimmed,
        }
    }
}

fn load_community_providers() -> Vec<ProviderDef> {
    let json =
        include_str!("../../../../../../crates/codegen/xai-grok-models/community_providers.json");
    let mut providers: Vec<ProviderDef> = serde_json::from_str(json).unwrap_or_else(|e| {
        tracing::warn!("Failed to parse community_providers.json: {e}");
        Vec::new()
    });

    // ZeroClaw is the Agent backend's canonical provider registry. Merge its
    // entries into the same menu, but retain DX's richer endpoint metadata for
    // providers that already exist in the community catalog.
    let known: std::collections::HashSet<String> = providers
        .iter()
        .map(|p| normalize_provider_id(&p.id))
        .collect();
    for provider in zeroclaw_providers::list_model_providers() {
        let id = provider.name.to_string();
        if known.contains(&normalize_provider_id(&id)) {
            continue;
        }
        providers.push(ProviderDef {
            id,
            name: provider.display_name.to_string(),
            base_url: String::new(),
            api_backend: "chat_completions".to_string(),
            auth_scheme: "bearer".to_string(),
            env_key_hint: format!("{} API key", provider.display_name),
            auth_type: if provider.local {
                "none"
            } else if provider.name == "copilot" {
                "oauth"
            } else {
                "api_key"
            }
            .to_string(),
            model_count: 0,
            free: if provider.local { "true" } else { "false" }.to_string(),
        });
    }
    for (id, name, auth_type, auth_scheme, env_key_hint) in [
        (
            "openai-codex",
            "OpenAI Codex OAuth",
            "oauth",
            "oauth",
            "OAuth device/browser login",
        ),
        (
            "gemini-oauth",
            "Google Gemini OAuth",
            "oauth",
            "oauth",
            "OAuth device login",
        ),
        (
            "qwen-oauth",
            "Qwen OAuth Refresh Token",
            "oauth_refresh",
            "oauth",
            "OAuth refresh token",
        ),
        (
            "minimax-oauth",
            "MiniMax OAuth Refresh Token",
            "oauth_refresh",
            "oauth",
            "OAuth refresh token",
        ),
        (
            "anthropic-setup-token",
            "Anthropic Claude Code Setup Token",
            "setup_token",
            "bearer",
            "Claude Code setup token",
        ),
        (
            "gemini_cli",
            "Gemini CLI Existing OAuth",
            "external_oauth",
            "oauth",
            "Existing ~/.gemini OAuth cache",
        ),
    ] {
        // ZeroClaw may already have contributed `gemini_cli` to the catalog.
        // Update that existing row instead of creating a duplicate with the
        // misleading API-key auth classification.
        if let Some(provider) = providers.iter_mut().find(|provider| provider.id == id) {
            provider.name = name.to_string();
            provider.auth_type = auth_type.to_string();
            provider.auth_scheme = auth_scheme.to_string();
            provider.env_key_hint = env_key_hint.to_string();
            continue;
        }
        if !providers.iter().any(|provider| provider.id == id) {
            providers.push(ProviderDef {
                id: id.to_string(),
                name: name.to_string(),
                base_url: String::new(),
                api_backend: String::new(),
                auth_scheme: auth_scheme.to_string(),
                env_key_hint: env_key_hint.to_string(),
                auth_type: auth_type.to_string(),
                model_count: 0,
                free: "false".to_string(),
            });
        }
    }

    // Import Router's API-key-only identities after the DX and Agent catalogs.
    // Keep the merge normalized so aliases such as `cloudflare-ai` and
    // `cloudflare-ai-gateway` do not create duplicate rows.
    let known: std::collections::HashSet<String> = providers
        .iter()
        .map(|provider| normalize_provider_id(&provider.id))
        .collect();
    for id in router_api_key_providers::PROVIDERS {
        if known.contains(&normalize_provider_id(id)) {
            continue;
        }
        providers.push(ProviderDef {
            id: (*id).to_string(),
            name: router_api_key_providers::display_name(id),
            base_url: String::new(),
            api_backend: "chat_completions".to_string(),
            auth_scheme: "bearer".to_string(),
            env_key_hint: "Provider API key".to_string(),
            auth_type: "api_key".to_string(),
            model_count: 0,
            free: "false".to_string(),
        });
    }
    providers
}

fn normalize_provider_id(id: &str) -> String {
    id.to_ascii_lowercase()
        .replace("-ai", "")
        .replace("_", "")
        .replace("-", "")
}

fn zeroclaw_provider_ids() -> Vec<String> {
    zeroclaw_providers::list_model_providers()
        .into_iter()
        .map(|p| p.name.to_string())
        .collect()
}

fn zeroclaw_auth_service() -> zeroclaw_providers::auth::AuthService {
    let state_dir = grok_home().join("agent");
    zeroclaw_providers::auth::AuthService::new(&state_dir, true)
}

fn load_zeroclaw_configured_providers() -> Vec<String> {
    let service = zeroclaw_auth_service();
    // This function is called by synchronous TUI rendering code, which may
    // itself run on Tokio's runtime. Always use a dedicated thread so this
    // inspection never attempts to nest `block_on` on the UI runtime.
    std::thread::Builder::new()
        .name("dx-provider-profile-read".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            Some(
                runtime
                    .block_on(service.list_profile_ids())
                    .unwrap_or_default(),
            )
        })
        .ok()
        .and_then(|join| join.join().ok())
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|profile| profile.split(':').next().map(str::to_string))
        .collect()
}

pub fn load_configured_providers() -> Vec<String> {
    let config_path = grok_home().join("config.toml");
    let mut configured: Vec<String> = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|content| content.parse::<DocumentMut>().ok())
        .and_then(|doc| {
            doc.get("model")
                .and_then(|m| m.as_table())
                .map(|t| t.iter().map(|(key, _)| key.to_string()).collect())
        })
        .unwrap_or_default();
    configured.extend(load_zeroclaw_configured_providers());
    for provider in ["qwen-oauth", "gemini_cli"] {
        if external_oauth_cache_exists(provider) && !configured.iter().any(|id| id == provider) {
            configured.push(provider.to_string());
        }
    }
    // The setup-token entry is a dedicated UI auth mode backed by ZeroClaw's
    // canonical Anthropic profile. Keep its status synchronized with that
    // profile without creating a second credential store.
    if configured.iter().any(|id| id == "anthropic")
        && !configured.iter().any(|id| id == "anthropic-setup-token")
    {
        configured.push("anthropic-setup-token".to_string());
    }
    configured
}

pub fn save_provider_config(
    provider_id: &str,
    api_key: Option<&str>,
    set_default: bool,
) -> Result<(), String> {
    if oauth_refresh_provider(provider_id).is_some() {
        let token = api_key
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| "OAuth refresh token cannot be empty.".to_string())?;
        save_oauth_refresh_token(provider_id, token)?;
        return Ok(());
    }
    if provider_id == "anthropic-setup-token" {
        let token = api_key
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| "Anthropic setup token cannot be empty.".to_string())?;
        return save_provider_config("anthropic", Some(token), set_default);
    }
    if provider_id == "gemini_cli" {
        return if external_oauth_cache_exists(provider_id) {
            Ok(())
        } else {
            Err("Gemini CLI OAuth cache was not found at ~/.gemini/oauth_creds.json.".to_string())
        };
    }
    if zeroclaw_provider_ids().iter().any(|id| id == provider_id) {
        let is_local = zeroclaw_providers::list_model_providers()
            .into_iter()
            .any(|provider| provider.name == provider_id && provider.local);
        if is_local {
            return Ok(());
        }
        let token = api_key
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| "This Agent provider requires a credential.".to_string())?;
        let provider_id = provider_id.to_string();
        let token = token.to_string();
        let agent_provider_id = provider_id.clone();
        let agent_token = token.clone();
        run_agent_operation(move || async move {
            let service = zeroclaw_auth_service();
            service
                .store_model_provider_token(
                    &agent_provider_id,
                    "default",
                    &agent_token,
                    std::collections::HashMap::new(),
                    true,
                )
                .await
                .map_err(|e| format!("Failed to save Agent credential: {e}"))
        })?;
        // Keep Grok ACP as the session/tool host, but mirror providers into a
        // Grok-native protocol configuration. Native protocols are selected
        // explicitly; they are never mislabeled as OpenAI-compatible.
        if let Some((base_url, api_backend, auth_scheme)) = grok_protocol_for_provider(&provider_id)
        {
            save_grok_provider(
                &provider_id,
                base_url,
                api_backend,
                auth_scheme,
                &token,
                set_default,
            )?;
        }
        return Ok(());
    }
    let config_path = grok_home().join("config.toml");
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    let mut doc = content
        .parse::<DocumentMut>()
        .map_err(|e| format!("Failed to parse config: {e}"))?;
    let provider = load_community_providers()
        .into_iter()
        .find(|p| p.id == provider_id);
    let base_url = provider
        .as_ref()
        .map(|p| p.base_url.as_str())
        .unwrap_or("https://api.openai.com/v1");
    if base_url.is_empty() {
        return Err(
            "This provider requires its own API endpoint; configure a base URL before saving credentials."
                .to_string(),
        );
    }
    let api_backend = provider
        .as_ref()
        .map(|p| p.api_backend.as_str())
        .unwrap_or("chat_completions");
    let auth_scheme = provider
        .as_ref()
        .map(|p| p.auth_scheme.as_str())
        .unwrap_or("bearer");
    let entry = doc
        .entry("model")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let model_table = entry.as_table_mut().expect("model entry is always a table");
    let prov_entry = model_table
        .entry(provider_id)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let table = prov_entry
        .as_table_mut()
        .expect("provider entry is always a table");
    table["base_url"] = Value::from(base_url).into();
    table["api_backend"] = Value::from(api_backend).into();
    table["auth_scheme"] = Value::from(auth_scheme).into();
    if let Some(key) = api_key {
        table["api_key"] = Value::from(key).into();
    }
    if set_default {
        let me = doc
            .entry("models")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        me["default"] = Value::from(provider_id).into();
    }
    std::fs::write(&config_path, doc.to_string()).map_err(|e| format!("{e}"))?;
    Ok(())
}

fn grok_protocol_for_provider(
    provider_id: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    // ZeroClaw's Gemini implementation uses Google's generateContent/SSE
    // protocol, which is not interchangeable with Grok's chat-completions
    // client. Leave it for the dedicated Gemini adapter.
    if provider_id == "gemini" {
        return None;
    }
    let base_url = zeroclaw_providers::default_model_provider_url(provider_id)?;
    if provider_id == "anthropic" {
        Some((base_url, "messages", "x-api-key"))
    } else {
        Some((base_url, "chat_completions", "bearer"))
    }
}

fn save_grok_provider(
    provider_id: &str,
    base_url: &str,
    api_backend: &str,
    auth_scheme: &str,
    api_key: &str,
    set_default: bool,
) -> Result<(), String> {
    let config_path = grok_home().join("config.toml");
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    let mut doc = content
        .parse::<DocumentMut>()
        .map_err(|e| format!("Failed to parse Grok config: {e}"))?;
    let model = doc
        .entry("model")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let model_table = model
        .as_table_mut()
        .ok_or_else(|| "Grok config `model` must be a table".to_string())?;
    let provider = model_table
        .entry(provider_id)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let table = provider
        .as_table_mut()
        .ok_or_else(|| format!("Grok model provider `{provider_id}` must be a table"))?;
    table["base_url"] = Value::from(base_url).into();
    table["api_backend"] = Value::from(api_backend).into();
    table["auth_scheme"] = Value::from(auth_scheme).into();
    table["api_key"] = Value::from(api_key).into();
    if set_default {
        let models = doc
            .entry("models")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        models["default"] = Value::from(provider_id).into();
    }
    std::fs::write(&config_path, doc.to_string())
        .map_err(|e| format!("Failed to update Grok provider config: {e}"))
}
