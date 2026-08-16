use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use indexmap::IndexMap;
use serde::Deserialize;
use xai_grok_sampler::AuthScheme;

use crate::auth::codex::CODEX_RESPONSES_URL;
use crate::sampling::ApiBackend;

use super::config::{EnvKeys, ModelEntryConfig};
use xai_grok_config_types::LazinessDetectorPerModelConfig;

const MODELS_DEV_CACHE_FILE: &str = "models-dev.json";
const COMMUNITY_PROVIDERS_JSON: &str =
    include_str!("../../../xai-grok-models/community_providers.json");
const AUTH_PROFILES_FILENAME: &str = "auth-profiles.json";

/// Provider APIs that expose an OpenAI-compatible chat-completions surface.
/// The models.dev catalog contains many native APIs; only these compatible
/// endpoints are injected into the ACP model map until a native adapter exists.
const OPENAI_COMPATIBLE_ENDPOINTS: &[(&str, &str)] = &[
    ("cerebras", "https://api.cerebras.ai/v1"),
    ("cohere", "https://api.cohere.com/compatibility/v1"),
    ("deepseek", "https://api.deepseek.com"),
    ("github-models", "https://models.github.ai/inference"),
    (
        "google",
        "https://generativelanguage.googleapis.com/v1beta/openai",
    ),
    ("groq", "https://api.groq.com/openai/v1"),
    ("huggingface", "https://router.huggingface.co/v1"),
    ("mistral", "https://api.mistral.ai/v1"),
    ("openai", "https://api.openai.com/v1"),
    ("openrouter", "https://openrouter.ai/api/v1"),
    ("sambanova", "https://api.sambanova.ai/v1"),
    ("togetherai", "https://api.together.xyz/v1"),
];

#[derive(Debug, Default, Deserialize)]
struct ModelsDevCache {
    #[serde(default)]
    providers: Vec<ModelsDevProvider>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevProvider {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    api: Option<String>,
    #[serde(default)]
    models: Vec<ModelsDevModel>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevModel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    reasoning: bool,
    #[serde(default)]
    context: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct CommunityProviderDef {
    id: String,
    #[serde(default, rename = "baseUrl")]
    base_url: String,
    #[serde(default, rename = "apiBackend")]
    api_backend: String,
    #[serde(default, rename = "authScheme")]
    auth_scheme: String,
    #[serde(default, rename = "envKeyHint")]
    #[allow(dead_code)]
    env_key_hint: String,
}

#[derive(Debug, Deserialize)]
struct AuthProfilesIndex {
    #[serde(default)]
    profiles: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    active_profiles: serde_json::Map<String, serde_json::Value>,
}

const CODEX_OAUTH_MODELS: &[(&str, &str, u64)] = &[
    ("gpt-5.3-codex", "GPT-5.3 Codex", 256_000),
    ("gpt-5.2-codex", "GPT-5.2 Codex", 256_000),
    ("gpt-5.1-codex", "GPT-5.1 Codex", 256_000),
    ("gpt-5.1-codex-max", "GPT-5.1 Codex Max", 256_000),
    ("gpt-5.1-codex-mini", "GPT-5.1 Codex Mini", 256_000),
    ("gpt-5-codex", "GPT-5 Codex", 256_000),
    ("gpt-5.4", "GPT-5.4", 256_000),
    ("gpt-5.1", "GPT-5.1", 256_000),
    ("o3", "o3", 200_000),
    ("o4-mini", "o4-mini", 200_000),
    ("gpt-4.1", "GPT-4.1", 1_047_576),
];

pub(crate) fn builtin_community_models() -> IndexMap<String, ModelEntryConfig> {
    let mut map = IndexMap::new();

    struct ModelSpec {
        key: &'static str,
        name: &'static str,
        model_id: &'static str,
        ctx: u64,
        title_header: Option<&'static str>,
    }

    let models = [
        ModelSpec {
            key: "hy3-free",
            name: "HY3 Free",
            model_id: "hy3-free",
            ctx: 131_000,
            title_header: None,
        },
        ModelSpec {
            key: "nemotron-3-ultra-free",
            name: "Nemotron 3 Ultra Free",
            model_id: "nemotron-3-ultra-free",
            ctx: 1_000_000,
            title_header: None,
        },
    ];

    for m in &models {
        let mut extra_headers = IndexMap::new();
        extra_headers.insert(
            "HTTP-Referer".to_string(),
            "https://opencode.ai/".to_string(),
        );
        if let Some(title) = m.title_header {
            extra_headers.insert("X-Title".to_string(), title.to_string());
        }

        map.insert(
            m.key.to_string(),
            ModelEntryConfig {
                id: Some(m.key.to_string()),
                model: m.model_id.to_string(),
                base_url: "https://opencode.ai/zen/v1".to_string(),
                api_base_url: None,
                name: Some(m.name.to_string()),
                description: None,
                max_completion_tokens: None,
                temperature: None,
                top_p: None,
                // OpenCode Zen's free catalog is intentionally public.
                api_key: Some("public".to_string()),
                env_key: None,
                api_backend: ApiBackend::ChatCompletions,
                auth_scheme: None,
                reasoning_effort: None,
                supports_reasoning_effort: false,
                reasoning_efforts: vec![],
                extra_headers,
                context_window: NonZeroU64::new(m.ctx).unwrap(),
                auto_compact_threshold_percent: None,
                system_prompt_label: None,
                use_concise: false,
                agent_type: "grok-build".to_string(),
                inference_idle_timeout_secs: None,
                max_retries: None,
                hidden: false,
                supported_in_api: true,
                supports_backend_search: false,
                compactions_remaining: None,
                compaction_at_tokens: None,
                show_model_fingerprint: false,
                stream_tool_calls: None,
                local_model_path: None,
                laziness_detector: LazinessDetectorPerModelConfig::default(),
            },
        );
    }

    map
}

/// Load models for providers that this user actually connected: a dedicated
/// environment key, an OAuth/token profile under `~/.grok/agent`, or a
/// `[model.<provider>]` / `[model.<provider>/…]` table in Grok config.
///
/// Providers without a known request adapter are skipped even if some other
/// tool on the machine exported a similarly named environment variable.
pub(crate) fn configured_catalog_model_entries() -> IndexMap<String, ModelEntryConfig> {
    let cache = load_models_dev_cache().unwrap_or_default();
    let connected = connected_provider_ids(&cache, |name| env_is_set(name));
    let api_keys = grok_config_provider_api_keys();
    configured_catalog_model_entries_from(cache, &connected, &api_keys, |name| env_is_set(name))
}

fn env_is_set(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

fn configured_catalog_model_entries_from(
    cache: ModelsDevCache,
    connected_ids: &HashSet<String>,
    api_keys: &HashMap<String, String>,
    is_env_configured: impl Fn(&str) -> bool,
) -> IndexMap<String, ModelEntryConfig> {
    let mut entries = IndexMap::new();
    if connected_ids.contains("openai-codex") {
        insert_codex_oauth_models(&mut entries);
    }
    for provider in cache.providers {
        if !provider_is_connected(&provider, connected_ids, &is_env_configured) {
            continue;
        }
        let Some((base_url, api_backend, auth_scheme)) = resolve_provider_route(&provider) else {
            continue;
        };
        let env_names = provider_env_names(&provider);
        let env_key = env_names
            .iter()
            .any(|name| is_env_configured(name))
            .then(|| EnvKeys::new(env_names));
        let provider_name = provider_display_name(&provider.id, provider.name.as_deref());
        for model in provider.models {
            let model_id = model.id.trim();
            if model_id.is_empty() {
                continue;
            }
            let key = format!("{}/{}", provider.id, model_id);
            if entries.contains_key(&key) {
                continue;
            }
            let display_name = model
                .name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| model_id.to_owned());
            let context_window = model.context.unwrap_or(200_000).max(1);
            entries.insert(
                key.clone(),
                ModelEntryConfig {
                    id: Some(key),
                    model: model_id.to_owned(),
                    base_url: base_url.clone(),
                    api_base_url: None,
                    name: Some(format!("{display_name} ({provider_name})")),
                    description: Some(format!("{provider_name} connected model")),
                    max_completion_tokens: None,
                    temperature: None,
                    top_p: None,
                    api_key: api_keys.get(&provider.id).cloned(),
                    env_key: env_key.clone(),
                    api_backend: api_backend.clone(),
                    auth_scheme,
                    reasoning_effort: None,
                    supports_reasoning_effort: model.reasoning,
                    reasoning_efforts: Vec::new(),
                    extra_headers: IndexMap::new(),
                    context_window: NonZeroU64::new(context_window)
                        .expect("context window was clamped above zero"),
                    auto_compact_threshold_percent: None,
                    system_prompt_label: None,
                    use_concise: false,
                    agent_type: if provider.id == "openai-codex" {
                        "codex".to_owned()
                    } else {
                        "grok-build".to_owned()
                    },
                    inference_idle_timeout_secs: Some(300),
                    max_retries: None,
                    hidden: false,
                    supported_in_api: true,
                    supports_backend_search: false,
                    compactions_remaining: None,
                    compaction_at_tokens: None,
                    show_model_fingerprint: false,
                    stream_tool_calls: None,
                    local_model_path: None,
                    laziness_detector: LazinessDetectorPerModelConfig::default(),
                },
            );
        }
    }
    entries
}

fn insert_codex_oauth_models(entries: &mut IndexMap<String, ModelEntryConfig>) {
    for (id, name, ctx) in CODEX_OAUTH_MODELS {
        let key = format!("openai-codex/{id}");
        entries.insert(
            key.clone(),
            ModelEntryConfig {
                id: Some(key),
                model: (*id).to_owned(),
                base_url: CODEX_RESPONSES_URL.to_owned(),
                api_base_url: None,
                name: Some(format!("{name} (OpenAI Codex)")),
                description: Some("ChatGPT / Codex OAuth model".to_owned()),
                max_completion_tokens: None,
                temperature: None,
                top_p: None,
                api_key: None,
                env_key: None,
                api_backend: ApiBackend::Responses,
                auth_scheme: Some(AuthScheme::Bearer),
                reasoning_effort: None,
                supports_reasoning_effort: true,
                reasoning_efforts: Vec::new(),
                extra_headers: IndexMap::new(),
                context_window: NonZeroU64::new(*ctx).expect("codex context is non-zero"),
                auto_compact_threshold_percent: None,
                system_prompt_label: None,
                use_concise: false,
                agent_type: "codex".to_owned(),
                inference_idle_timeout_secs: Some(300),
                max_retries: None,
                hidden: false,
                supported_in_api: true,
                supports_backend_search: false,
                compactions_remaining: None,
                compaction_at_tokens: None,
                show_model_fingerprint: false,
                stream_tool_calls: None,
                local_model_path: None,
                laziness_detector: LazinessDetectorPerModelConfig::default(),
            },
        );
    }
}

fn provider_is_connected(
    provider: &ModelsDevProvider,
    connected_ids: &HashSet<String>,
    is_env_configured: &impl Fn(&str) -> bool,
) -> bool {
    if connected_ids.contains(&provider.id) {
        return true;
    }
    provider_env_names(provider)
        .iter()
        .any(|name| is_env_configured(name))
}

fn connected_provider_ids(
    cache: &ModelsDevCache,
    is_env_configured: impl Fn(&str) -> bool,
) -> HashSet<String> {
    let mut ids = HashSet::new();
    for provider in oauth_connected_provider_ids() {
        ids.insert(provider);
    }
    for id in grok_config_connected_provider_ids() {
        ids.insert(id);
    }
    for provider in &cache.providers {
        if provider_env_names(provider)
            .iter()
            .any(|name| is_env_configured(name))
        {
            ids.insert(provider.id.clone());
        }
    }
    ids
}

fn grok_config_connected_provider_ids() -> HashSet<String> {
    grok_config_model_table()
        .map(|models| {
            models
                .keys()
                .map(|key| key.split('/').next().unwrap_or(key).to_string())
                .filter(|id| !id.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// API keys saved under `[model.<provider>]` / `[model.<provider>/…]`.
/// Applied to every expanded catalog model for that provider so a Connect
/// flow that writes the key into config.toml actually authenticates.
fn grok_config_provider_api_keys() -> HashMap<String, String> {
    let Some(models) = grok_config_model_table() else {
        return HashMap::new();
    };
    let mut keys = HashMap::new();
    for (key, value) in models {
        let Some(table) = value.as_table() else {
            continue;
        };
        let Some(api_key) = table
            .get("api_key")
            .and_then(|item| item.as_str())
            .map(str::trim)
            .filter(|item| !item.is_empty())
        else {
            continue;
        };
        let provider = key.split('/').next().unwrap_or(key.as_str());
        keys.entry(provider.to_string()).or_insert_with(|| api_key.to_string());
    }
    keys
}

fn grok_config_model_table() -> Option<toml::Table> {
    let path = crate::util::grok_home::grok_home().join("config.toml");
    let text = std::fs::read_to_string(path).ok()?;
    let doc = text.parse::<toml::Table>().ok()?;
    doc.get("model")?.as_table().cloned()
}

fn oauth_connected_provider_ids() -> HashSet<String> {
    let path = crate::util::grok_home::grok_home()
        .join("agent")
        .join(AUTH_PROFILES_FILENAME);
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashSet::new();
    };
    let Ok(index) = serde_json::from_str::<AuthProfilesIndex>(&text) else {
        return HashSet::new();
    };
    let mut ids = HashSet::new();
    for key in index.profiles.keys().chain(index.active_profiles.keys()) {
        if let Some(provider) = key.split(':').next()
            && !provider.is_empty()
        {
            ids.insert(provider.to_string());
        }
    }
    ids
}

fn community_provider_defs() -> &'static [CommunityProviderDef] {
    static DEFS: OnceLock<Vec<CommunityProviderDef>> = OnceLock::new();
    DEFS.get_or_init(|| serde_json::from_str(COMMUNITY_PROVIDERS_JSON).unwrap_or_default())
}

fn community_provider(id: &str) -> Option<&'static CommunityProviderDef> {
    community_provider_defs()
        .iter()
        .find(|provider| provider.id == id)
}

fn parse_api_backend(value: &str) -> Option<ApiBackend> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "chat_completions" | "openai" | "openai-compatible" => {
            Some(ApiBackend::ChatCompletions)
        }
        "responses" => Some(ApiBackend::Responses),
        "messages" | "anthropic" => Some(ApiBackend::Messages),
        _ => None,
    }
}

fn parse_auth_scheme(value: &str) -> Option<AuthScheme> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "" | "bearer" | "oauth" => Some(AuthScheme::Bearer),
        "x_api_key" | "xapikey" => Some(AuthScheme::XApiKey),
        _ => None,
    }
}

fn resolve_provider_route(
    provider: &ModelsDevProvider,
) -> Option<(String, ApiBackend, Option<AuthScheme>)> {
    if provider.id == "openai-codex" {
        return Some((
            CODEX_RESPONSES_URL.to_owned(),
            ApiBackend::Responses,
            Some(AuthScheme::Bearer),
        ));
    }
    if let Some(community) = community_provider(&provider.id) {
        let backend = parse_api_backend(&community.api_backend)?;
        let base = if community.base_url.trim().is_empty() {
            compatible_provider_endpoint(provider)?.to_owned()
        } else {
            community.base_url.clone()
        };
        return Some((base, backend, parse_auth_scheme(&community.auth_scheme)));
    }
    let endpoint = compatible_provider_endpoint(provider)?;
    Some((
        endpoint.to_owned(),
        ApiBackend::ChatCompletions,
        Some(AuthScheme::Bearer),
    ))
}

fn compatible_provider_endpoint(provider: &ModelsDevProvider) -> Option<&str> {
    if let Some((_, endpoint)) = OPENAI_COMPATIBLE_ENDPOINTS
        .iter()
        .find(|(id, _)| *id == provider.id)
    {
        return Some(*endpoint);
    }
    if let Some(community) = community_provider(&provider.id)
        && community.base_url.starts_with("https://")
    {
        return Some(community.base_url.as_str());
    }
    // Only accept models.dev `api` when it is an HTTPS OpenAI-compatible
    // `/v1` surface. Native-only APIs (Bedrock, Vertex, many China
    // portals) are listed in the catalog but have no adapter here.
    provider.api.as_deref().filter(|api| {
        api.starts_with("https://") && (api.contains("/v1") || api.ends_with("/v1"))
    })
}

fn provider_display_name(id: &str, catalog_name: Option<&str>) -> String {
    if let Some(name) = catalog_name.filter(|name| !name.trim().is_empty()) {
        return name.to_owned();
    }
    match id {
        "github-models" => "GitHub Models".to_owned(),
        "openrouter" => "OpenRouter".to_owned(),
        "togetherai" => "Together AI".to_owned(),
        other => other
            .split(['-', '_'])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn provider_env_names(provider: &ModelsDevProvider) -> Vec<String> {
    let fallback = match provider.id.as_str() {
        "github-models" => Some("GITHUB_MODELS_API_KEY"),
        "huggingface" => Some("HUGGINGFACE_API_KEY"),
        "replicate" => Some("REPLICATE_API_TOKEN"),
        "sambanova" => Some("SAMBANOVA_API_KEY"),
        _ => None,
    };
    let mut names = provider.env.clone();
    if let Some(name) = fallback
        && !names.iter().any(|existing| existing == name)
    {
        names.push(name.to_owned());
    }
    names
}

fn load_models_dev_cache() -> Option<ModelsDevCache> {
    for path in models_dev_cache_paths() {
        let Ok(contents) = std::fs::read_to_string(path) else {
            continue;
        };
        match serde_json::from_str::<ModelsDevCache>(&contents) {
            Ok(cache) => return Some(cache),
            Err(error) => tracing::debug!(%error, "ignored invalid models.dev cache"),
        }
    }
    None
}

fn models_dev_cache_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config) = dirs::config_dir() {
        paths.push(config.join("dx").join("cache").join(MODELS_DEV_CACHE_FILE));
    }
    if let Some(data_local) = dirs::data_local_dir() {
        paths.push(
            data_local
                .join("dx")
                .join("cache")
                .join(MODELS_DEV_CACHE_FILE),
        );
    }
    if let Some(data) = dirs::data_dir() {
        paths.push(data.join("dx").join("cache").join(MODELS_DEV_CACHE_FILE));
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".dx").join("cache").join(MODELS_DEV_CACHE_FILE));
        paths.push(
            home.join(".config")
                .join("dx")
                .join("cache")
                .join(MODELS_DEV_CACHE_FILE),
        );
    }
    paths.sort();
    paths.dedup();
    paths
}

pub(crate) fn is_stale_bundled_local_model(key: &str, model_id: &str) -> bool {
    matches!(key, "minicpm5-1b-tooluse" | "qwen2.5-coder-1.5b-local")
        || matches!(model_id, "minicpm5-1b-tooluse" | "qwen2.5-coder-1.5b-local")
}

/// Build model entries from GGUF/GGML files already present in DX's local
/// model cache. The catalog deliberately has no bundled local model names:
/// a model appears only after the user places/downloads the file.
pub(crate) fn cached_local_model_entries() -> IndexMap<String, ModelEntryConfig> {
    let mut entries = IndexMap::new();
    for directory in local_model_directories() {
        let Ok(read_dir) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if !is_supported_local_model_file(&path) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|v| v.to_str()).map(str::to_owned) else {
                continue;
            };
            let key = local_model_key(&stem);
            entries
                .entry(key.clone())
                .or_insert_with(|| local_model_config(key, &stem, path));
        }
    }
    entries
}

fn local_model_config(key: String, stem: &str, path: PathBuf) -> ModelEntryConfig {
    ModelEntryConfig {
        id: Some(key.clone()),
        model: key,
        base_url: "http://127.0.0.1:8080/v1".to_string(),
        api_base_url: None,
        name: Some(stem.replace(['_', '-'], " ")),
        description: Some("Local GGUF/GGML model · DX cache".to_string()),
        max_completion_tokens: None,
        temperature: None,
        top_p: None,
        api_key: Some("local".to_string()),
        env_key: None,
        api_backend: ApiBackend::ChatCompletions,
        auth_scheme: None,
        reasoning_effort: None,
        supports_reasoning_effort: false,
        reasoning_efforts: Vec::new(),
        extra_headers: IndexMap::new(),
        context_window: NonZeroU64::new(32_768).expect("non-zero context window"),
        auto_compact_threshold_percent: None,
        system_prompt_label: None,
        use_concise: false,
        agent_type: "grok-build".to_string(),
        inference_idle_timeout_secs: Some(600),
        max_retries: None,
        hidden: false,
        supported_in_api: true,
        supports_backend_search: false,
        compactions_remaining: None,
        compaction_at_tokens: None,
        show_model_fingerprint: false,
        stream_tool_calls: None,
        local_model_path: Some(path),
        laziness_detector: LazinessDetectorPerModelConfig::default(),
    }
}

fn is_supported_local_model_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("gguf") || extension.eq_ignore_ascii_case("ggml")
            })
}

fn local_model_key(stem: &str) -> String {
    let slug: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("local-{}", slug.trim_matches('-'))
}

fn local_model_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(path) = std::env::var_os("DX_FLOW_MODELS_DIR") {
        directories.push(PathBuf::from(path));
    }
    if let Some(data_local) = dirs::data_local_dir() {
        directories.push(
            data_local
                .join("dx")
                .join("flow")
                .join("models")
                .join("llm"),
        );
    }
    if let Some(data) = dirs::data_dir() {
        directories.push(data.join("dx").join("flow").join("models").join("llm"));
    }
    if let Some(home) = dirs::home_dir() {
        directories.push(home.join(".dx").join("flow").join("models").join("llm"));
    }
    directories.sort();
    directories.dedup();
    directories
}

#[cfg(test)]
mod tests {
    use super::{
        ModelsDevCache, builtin_community_models, configured_catalog_model_entries_from,
        insert_codex_oauth_models, is_supported_local_model_file, local_model_key,
    };
    use crate::sampling::ApiBackend;
    use indexmap::IndexMap;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn bundled_community_catalog_has_no_local_model_names() {
        let models = builtin_community_models();
        assert!(!models.keys().any(|key| key.starts_with("local-")));
        assert!(!models.contains_key("minicpm5-1b-tooluse"));
        assert!(!models.contains_key("qwen2.5-coder-1.5b-local"));
    }

    #[test]
    fn local_model_keys_are_stable_and_files_are_extension_filtered() {
        assert_eq!(
            local_model_key("Qwen2.5-Coder_Q4"),
            "local-qwen2-5-coder-q4"
        );
        let directory = tempfile::tempdir().expect("temporary model directory");
        let uppercase = directory.path().join("model.GGUF");
        let lowercase = directory.path().join("model.ggml");
        let unsupported = directory.path().join("model.bin");
        std::fs::write(&uppercase, b"gguf").expect("uppercase model fixture");
        std::fs::write(&lowercase, b"ggml").expect("lowercase model fixture");
        std::fs::write(&unsupported, b"binary").expect("unsupported model fixture");
        assert!(is_supported_local_model_file(&uppercase));
        assert!(is_supported_local_model_file(&lowercase));
        assert!(!is_supported_local_model_file(&unsupported));
    }

    #[test]
    fn configured_catalog_models_use_the_declared_env_key_and_provider_endpoint() {
        let cache: ModelsDevCache = serde_json::from_str(
            r#"{
                "providers": [{
                    "id": "openrouter",
                    "name": "OpenRouter",
                    "env": ["OPENROUTER_API_KEY"],
                    "api": "https://openrouter.ai/api/v1",
                    "models": [{
                        "id": "qwen/qwen3-coder",
                        "name": "Qwen 3 Coder",
                        "reasoning": true,
                        "tool_call": true,
                        "context": 131072
                    }]
                }]
            }"#,
        )
        .expect("test models.dev cache");
        let connected = HashSet::from(["openrouter".to_string()]);
        let models = configured_catalog_model_entries_from(
            cache,
            &connected,
            &HashMap::new(),
            |name| name == "OPENROUTER_API_KEY",
        );
        let entry = models
            .get("openrouter/qwen/qwen3-coder")
            .expect("configured provider model");
        assert_eq!(entry.model, "qwen/qwen3-coder");
        assert_eq!(entry.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(
            entry.env_key.as_ref().and_then(|key| key.primary()),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(entry.context_window.get(), 131072);
    }

    #[test]
    fn unconfigured_catalog_providers_are_not_injected() {
        let cache: ModelsDevCache = serde_json::from_str(
            r#"{
                "providers": [{
                    "id": "openrouter",
                    "env": ["OPENROUTER_API_KEY"],
                    "api": "https://openrouter.ai/api/v1",
                    "models": [{ "id": "model", "name": "Model" }]
                }]
            }"#,
        )
        .expect("test models.dev cache");
        let models =
            configured_catalog_model_entries_from(cache, &HashSet::new(), &HashMap::new(), |_| false);
        assert!(models.is_empty());
    }

    #[test]
    fn config_api_key_is_copied_onto_expanded_models() {
        let cache: ModelsDevCache = serde_json::from_str(
            r#"{
                "providers": [{
                    "id": "groq",
                    "name": "Groq",
                    "env": ["GROQ_API_KEY"],
                    "api": "https://api.groq.com/openai/v1",
                    "models": [{ "id": "llama-3.3-70b", "name": "Llama 3.3 70B" }]
                }]
            }"#,
        )
        .expect("test models.dev cache");
        let connected = HashSet::from(["groq".to_string()]);
        let api_keys = HashMap::from([("groq".to_string(), "gsk-test".to_string())]);
        let models =
            configured_catalog_model_entries_from(cache, &connected, &api_keys, |_| false);
        let entry = models
            .get("groq/llama-3.3-70b")
            .expect("expanded groq model");
        assert_eq!(entry.api_key.as_deref(), Some("gsk-test"));
        assert_eq!(entry.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(entry.model, "llama-3.3-70b");
    }

    #[test]
    fn oauth_codex_injects_responses_models() {
        let mut entries = IndexMap::new();
        insert_codex_oauth_models(&mut entries);
        let entry = entries
            .get("openai-codex/gpt-5.3-codex")
            .expect("codex oauth model");
        assert_eq!(entry.model, "gpt-5.3-codex");
        assert_eq!(entry.base_url, crate::auth::codex::CODEX_RESPONSES_URL);
        assert_eq!(entry.api_backend, ApiBackend::Responses);
        assert_eq!(entry.agent_type, "codex");
    }
}
