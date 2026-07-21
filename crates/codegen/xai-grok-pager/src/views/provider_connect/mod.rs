pub mod input;
pub mod render;

use serde::Deserialize;
use toml_edit::{DocumentMut, Value};
use xai_grok_tools::util::grok_home::grok_home;

use crate::views::picker::{PickerEntry, PickerRow, PickerState};

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

pub const TAB_LABELS: [&str; 7] = ["All", "Free", "Featured", "Gateway", "Enterprise", "China", "Other"];

// ── Categorisation ──

pub fn categorize(provider: &ProviderDef) -> ProviderTab {
    if provider.auth_type == "none" || provider.auth_type == "optional" || provider.free == "true" {
        return ProviderTab::Free;
    }
    match provider.id.as_str() {
        // Featured – major global AI platforms
        "openai" | "anthropic" | "gemini" | "mistral" | "cohere" | "deepseek"
        | "meta-llama" | "xai" | "grok-web" | "perplexity" | "ai21" | "cerebras"
        | "groq" => ProviderTab::Featured,

        // Gateway – multi-model routers / aggregators
        "openrouter" | "deepinfra" | "fireworks" | "hyperbolic" | "aimlapi"
        | "orcarouter" | "tokenrouter" | "agentrouter" | "zenmux" | "zenmux-free"
        | "featherless-ai" | "freeaiapikey" => ProviderTab::Gateway,

        // China / East Asia
        "baidu" | "alibaba" | "tencent" | "moonshot" | "kimi" | "minimax"
        | "stepfun" | "glm" | "baichuan" | "doubao" | "iflytek" | "sparkdesk"
        | "sensenova" | "qianfan" | "volcengine" | "yi" | "zai" | "zai-web"
        | "xiaomi-mimo" | "coze" | "bailian-coding-plan" | "byteplus" | "qiniu"
        | "yuanbao-web" => ProviderTab::China,

        // Enterprise / Cloud
        "vertex" | "databricks" | "snowflake" | "cloudflare-ai" | "nvidia"
        | "ovhcloud" | "scaleway" | "vercel-ai-gateway" | "upstage" | "wandb"
        | "heroku" | "inference-net" | "predibase" => ProviderTab::Enterprise,

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
        if self.name.is_empty() { &self.id } else { &self.name }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStatus {
    NotConfigured,
    Configured,
    Free,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectMode {
    Browse,
    KeyInput { provider_id: String, input_buffer: String, set_default: bool },
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
    pub collapsed_groups: std::collections::HashSet<String>,
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
        // Seed collapsed: collapse everything except the first section with items.
        let mut collapsed = std::collections::HashSet::new();
        for tab in &ProviderTab::ALL[1..] {
            let has_any = api_providers.iter().any(|p| categorize(p) == *tab);
            if *tab != ProviderTab::Free && has_any {
                collapsed.insert(tab.label().to_string());
            }
        }
        Self {
            providers: api_providers,
            free_providers,
            configured_ids: load_configured_providers(),
            picker: PickerState::with_mode(crate::views::picker::PickerMode::Popup(
                crate::views::picker::PopupConfig {
                    width_pct: 0.85, height_pct: 0.7, min_width: 50, min_height: 16,
                },
            )),
            mode: ConnectMode::Browse,
            status_message: None,
            error_message: None,
            window: crate::views::modal_window::ModalWindowState::new(),
            active_tab: ProviderTab::All,
            collapsed_groups: collapsed,
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

    pub fn is_group_expanded(&self, group_key: &str) -> bool {
        let searching = !self.picker.query().is_empty();
        searching || !self.collapsed_groups.contains(group_key)
    }

    /// Build picker entries filtered by the active tab and search query.
    /// Returns (entries, non_selectable_mask, group_keys).
    pub fn picker_entries<'a>(
        free_providers: &'a [ProviderDef],
        providers: &'a [ProviderDef],
        configured_ids: &[String],
        active_tab: ProviderTab,
        query: &str,
        collapsed_groups: &std::collections::HashSet<String>,
    ) -> (Vec<PickerEntry<'a>>, Vec<bool>, Vec<Option<String>>) {
        let mut entries: Vec<PickerEntry<'a>> = Vec::new();
        let mut non_sel: Vec<bool> = Vec::new();
        let mut group_keys: Vec<Option<String>> = Vec::new();
        let empty: &[&str] = &[];
        let searching = !query.is_empty();

        match active_tab {
            ProviderTab::All => {
                // Group providers by category, show collapsible section headers.
                for tab in &ProviderTab::ALL[1..] {
                    let cat_providers: Vec<&ProviderDef> = free_providers.iter()
                        .chain(providers.iter())
                        .filter(|p| categorize(p) == *tab && fuzzy_matches(p.display_name(), query))
                        .collect();
                    if cat_providers.is_empty() && !searching {
                        continue;
                    }
                    if cat_providers.is_empty() {
                        // During search, show a collapsed section only if refiltering would re-add it.
                        continue;
                    }
                    let group_key = tab.label().to_string();
                    let section_collapsed = !searching && collapsed_groups.contains(&group_key);

                    // Section header
                    let label = format!("{} ({})", tab.label(), cat_providers.len());
                    entries.push(PickerEntry::Row(PickerRow {
                        label: &label,
                        right_label: "",
                        selected: false, expanded: false,
                        fields: &[], description_lines: empty, summary_lines: empty,
                        dimmed: false, indent: 0, collapsible: true,
                        badge: "", badge_color: None, underline_last_desc: false,
                    }));
                    non_sel.push(true);
                    group_keys.push(Some(group_key.clone()));

                    if section_collapsed {
                        continue;
                    }

                    for p in &cat_providers {
                        let status = p.status(configured_ids);
                        let (badge, bc) = match &status {
                            ProviderStatus::Free => ("Free", Some(ratatui::style::Color::DarkGray)),
                            ProviderStatus::Configured => ("Configured", Some(ratatui::style::Color::Green)),
                            ProviderStatus::NotConfigured => ("Configure", Some(ratatui::style::Color::Yellow)),
                        };
                        entries.push(PickerEntry::Row(PickerRow {
                            label: p.display_name(),
                            right_label: "",
                            selected: false, expanded: false,
                            fields: &[], description_lines: empty, summary_lines: empty,
                            dimmed: false, indent: 1, collapsible: false,
                            badge, badge_color: bc, underline_last_desc: false,
                        }));
                        non_sel.push(false);
                        group_keys.push(Some(group_key.clone()));
                    }
                }

                if entries.is_empty() {
                    entries.push(PickerEntry::Row(PickerRow {
                        label: "No providers match your search",
                        right_label: "",
                        selected: false, expanded: false,
                        fields: &[], description_lines: empty, summary_lines: empty,
                        dimmed: true, indent: 0, collapsible: false,
                        badge: "", badge_color: None, underline_last_desc: false,
                    }));
                    non_sel.push(true);
                    group_keys.push(None);
                }
            }
            _ => {
                // Single category tab.
                let cat_providers: Vec<&ProviderDef> = free_providers.iter()
                    .chain(providers.iter())
                    .filter(|p| {
                        if active_tab == ProviderTab::Free {
                            categorize(p) == ProviderTab::Free
                        } else {
                            categorize(p) == active_tab
                        }
                    })
                    .collect();

                let mut matched: Vec<&&ProviderDef> = cat_providers.iter()
                    .filter(|p| fuzzy_matches(p.display_name(), query))
                    .collect();
                matched.sort_by(|a, b| a.display_name().cmp(b.display_name()));

                if matched.is_empty() {
                    entries.push(PickerEntry::Row(PickerRow {
                        label: if query.is_empty() {
                            "No providers in this category"
                        } else {
                            "No providers match your search"
                        },
                        right_label: "",
                        selected: false, expanded: false,
                        fields: &[], description_lines: empty, summary_lines: empty,
                        dimmed: true, indent: 0, collapsible: false,
                        badge: "", badge_color: None, underline_last_desc: false,
                    }));
                    non_sel.push(true);
                    group_keys.push(None);
                } else {
                    let header_label = format!("{} ({})", active_tab.label(), matched.len());
                    entries.push(PickerEntry::Row(PickerRow {
                        label: &header_label,
                        right_label: "",
                        selected: false, expanded: false,
                        fields: &[], description_lines: empty, summary_lines: empty,
                        dimmed: false, indent: 0, collapsible: false,
                        badge: "", badge_color: None, underline_last_desc: false,
                    }));
                    non_sel.push(true);
                    group_keys.push(None);

                    for p in matched {
                        let status = p.status(configured_ids);
                        let (badge, bc) = match &status {
                            ProviderStatus::Free => ("Free", Some(ratatui::style::Color::DarkGray)),
                            ProviderStatus::Configured => ("Configured", Some(ratatui::style::Color::Green)),
                            ProviderStatus::NotConfigured => ("Configure", Some(ratatui::style::Color::Yellow)),
                        };
                        entries.push(PickerEntry::Row(PickerRow {
                            label: p.display_name(),
                            right_label: "",
                            selected: false, expanded: false,
                            fields: &[], description_lines: empty, summary_lines: empty,
                            dimmed: false, indent: 1, collapsible: false,
                            badge, badge_color: bc, underline_last_desc: false,
                        }));
                        non_sel.push(false);
                        group_keys.push(None);
                    }
                }
            }
        }

        (entries, non_sel, group_keys)
    }
}

fn load_community_providers() -> Vec<ProviderDef> {
    let json = include_str!("../../../../../../crates/codegen/xai-grok-models/community_providers.json");
    serde_json::from_str(json).unwrap_or_else(|e| {
        tracing::warn!("Failed to parse community_providers.json: {e}"); Vec::new()
    })
}

pub fn load_configured_providers() -> Vec<String> {
    let config_path = grok_home().join("config.toml");
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c, Err(_) => return Vec::new(),
    };
    let doc = match content.parse::<DocumentMut>() {
        Ok(d) => d, Err(_) => return Vec::new(),
    };
    doc.get("model")
        .and_then(|m| m.as_table())
        .map(|t| t.iter().map(|(key, _)| key.to_string()).collect())
        .unwrap_or_default()
}

pub fn save_provider_config(provider_id: &str, api_key: Option<&str>, set_default: bool) -> Result<(), String> {
    let config_path = grok_home().join("config.toml");
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    let mut doc = content.parse::<DocumentMut>().map_err(|e| format!("Failed to parse config: {e}"))?;
    let provider = load_community_providers().into_iter().find(|p| p.id == provider_id);
    let base_url = provider.as_ref().map(|p| p.base_url.as_str()).unwrap_or("https://api.openai.com/v1");
    let api_backend = provider.as_ref().map(|p| p.api_backend.as_str()).unwrap_or("chat_completions");
    let auth_scheme = provider.as_ref().map(|p| p.auth_scheme.as_str()).unwrap_or("bearer");
    let entry = doc.entry("model").or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let model_table = entry.as_table_mut().expect("model entry is always a table");
    let prov_entry = model_table.entry(provider_id).or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let table = prov_entry.as_table_mut().expect("provider entry is always a table");
    table["base_url"] = Value::from(base_url).into();
    table["api_backend"] = Value::from(api_backend).into();
    table["auth_scheme"] = Value::from(auth_scheme).into();
    if let Some(key) = api_key { table["api_key"] = Value::from(key).into(); }
    if set_default {
        let me = doc.entry("models").or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
        me["default"] = Value::from(provider_id).into();
    }
    std::fs::write(&config_path, doc.to_string()).map_err(|e| format!("{e}"))?;
    Ok(())
}
