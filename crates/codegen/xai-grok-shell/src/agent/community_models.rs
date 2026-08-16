use std::collections::HashSet;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::sampling::ApiBackend;

use super::config::{EnvKeys, ModelEntryConfig};
use xai_grok_config_types::LazinessDetectorPerModelConfig;

const MODELS_DEV_CACHE_FILE: &str = "models-dev.json";
const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

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

#[derive(Debug, Deserialize)]
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
struct OpenRouterModelsResponse {
    #[serde(default)]
    data: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
}

/// Live OpenRouter model IDs for this process. `models.dev` is intentionally
/// cached for hours, while OpenRouter can remove free endpoints sooner. A
/// successful live response therefore becomes the authority for OpenRouter;
/// a failed response leaves the catalog usable offline instead of hiding every
/// configured model.
static OPENROUTER_LIVE_MODELS: OnceLock<Option<HashSet<String>>> = OnceLock::new();

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

fn live_openrouter_models() -> Option<&'static HashSet<String>> {
    OPENROUTER_LIVE_MODELS
        .get_or_init(|| {
            let key = ["OPENROUTER_API_KEY", "OPENROUTER_KEY"]
                .into_iter()
                .find_map(|name| {
                    std::env::var(name)
                        .ok()
                        .map(|value| value.trim().to_owned())
                        .filter(|value| !value.is_empty())
                })?;
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .user_agent("dx-tui/openrouter-models")
                .build()
                .ok()?;
            let response = client
                .get(OPENROUTER_MODELS_URL)
                .bearer_auth(key)
                .send()
                .ok()?;
            if !response.status().is_success() {
                tracing::debug!(status = %response.status(), "OpenRouter live model catalog unavailable");
                return None;
            }
            let payload = response.json::<OpenRouterModelsResponse>().ok()?;
            let ids = payload
                .data
                .into_iter()
                .map(|model| model.id)
                .filter(|id| !id.trim().is_empty())
                .collect::<HashSet<_>>();
            (!ids.is_empty()).then_some(ids)
        })
        .as_ref()
}

fn include_openrouter_model(model_id: &str, live_ids: Option<&HashSet<String>>) -> bool {
    live_ids.is_none_or(|ids| ids.contains(model_id))
}

/// Load API-key providers from DX's cached models.dev catalog. OpenRouter
/// entries are additionally checked once against its live `/models` endpoint
/// so removed endpoints do not remain selectable. If that check is unavailable,
/// cached entries remain visible for offline use. A provider is included only
/// when one of its declared environment variables contains a non-empty value.
pub(crate) fn configured_catalog_model_entries() -> IndexMap<String, ModelEntryConfig> {
    let Some(cache) = load_models_dev_cache() else {
        return IndexMap::new();
    };
    configured_catalog_model_entries_from(cache, |name| {
        std::env::var(name)
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
    })
}

fn configured_catalog_model_entries_from(
    cache: ModelsDevCache,
    is_env_configured: impl Fn(&str) -> bool,
) -> IndexMap<String, ModelEntryConfig> {
    let mut entries = IndexMap::new();
    for provider in cache.providers {
        let env_names = provider_env_names(&provider);
        let Some(env_key) = env_names
            .iter()
            .find(|name| is_env_configured(name))
            .map(|_| EnvKeys::new(env_names.clone()))
        else {
            continue;
        };
        let Some(base_url) = compatible_provider_endpoint(&provider).map(str::to_owned) else {
            continue;
        };
        let live_ids = (provider.id == "openrouter")
            .then(live_openrouter_models)
            .flatten();
        let provider_name = provider_display_name(&provider.id, provider.name.as_deref());
        for model in provider.models {
            let model_id = model.id.trim();
            if model_id.is_empty() {
                continue;
            }
            if provider.id == "openrouter" && !include_openrouter_model(model_id, live_ids) {
                tracing::debug!(
                    model = model_id,
                    "skipping OpenRouter model absent from live catalog"
                );
                continue;
            }
            let key = format!("{}/{}", provider.id, model_id);
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
                    description: Some(format!("{provider_name} API-key model")),
                    max_completion_tokens: None,
                    temperature: None,
                    top_p: None,
                    api_key: None,
                    env_key: Some(env_key.clone()),
                    api_backend: ApiBackend::ChatCompletions,
                    auth_scheme: None,
                    reasoning_effort: None,
                    supports_reasoning_effort: model.reasoning,
                    reasoning_efforts: Vec::new(),
                    extra_headers: IndexMap::new(),
                    context_window: NonZeroU64::new(context_window)
                        .expect("context window was clamped above zero"),
                    auto_compact_threshold_percent: None,
                    system_prompt_label: None,
                    use_concise: false,
                    agent_type: "grok-build".to_owned(),
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

fn compatible_provider_endpoint(provider: &ModelsDevProvider) -> Option<&str> {
    if let Some((_, endpoint)) = OPENAI_COMPATIBLE_ENDPOINTS
        .iter()
        .find(|(id, _)| *id == provider.id)
    {
        return Some(*endpoint);
    }
    provider
        .api
        .as_deref()
        .filter(|api| api.starts_with("https://") && api.contains("/v1"))
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
        is_supported_local_model_file, local_model_key,
    };

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
        let models =
            configured_catalog_model_entries_from(cache, |name| name == "OPENROUTER_API_KEY");
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
        let models = configured_catalog_model_entries_from(cache, |_| false);
        assert!(models.is_empty());
    }
}
