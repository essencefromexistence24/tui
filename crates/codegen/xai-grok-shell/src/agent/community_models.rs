use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::sampling::ApiBackend;

use super::config::ModelEntryConfig;
use xai_grok_config_types::LazinessDetectorPerModelConfig;

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
            key: "big-pickle",
            name: "Big Pickle",
            model_id: "big-pickle",
            ctx: 200_000,
            title_header: Some("opencode"),
        },
        ModelSpec {
            key: "deepseek-v4-flash-free",
            name: "DeepSeek V4 Flash Free",
            model_id: "deepseek-v4-flash-free",
            ctx: 200_000,
            title_header: None,
        },
        ModelSpec {
            key: "mimo-v2.5-free",
            name: "MiMo V2.5 Free",
            model_id: "mimo-v2.5-free",
            ctx: 131_000,
            title_header: None,
        },
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
        ModelSpec {
            key: "north-mini-code-free",
            name: "North Mini Code Free",
            model_id: "north-mini-code-free",
            ctx: 131_000,
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

    // ChatGPT Sign-in / Codex Responses provider.  The credential is resolved
    // just before each request from ZeroClaw's encrypted OAuth profile; no
    // token is embedded in the catalog or copied into xAI auth state.
    let mut codex_headers = IndexMap::new();
    codex_headers.insert(
        "OpenAI-Beta".to_string(),
        "responses=experimental".to_string(),
    );
    codex_headers.insert("originator".to_string(), "pi".to_string());
    map.insert(
        "gpt-5.6-luna".to_string(),
        ModelEntryConfig {
            id: Some("gpt-5.6-luna".to_string()),
            model: "gpt-5.6-luna".to_string(),
            base_url: crate::auth::codex::CODEX_RESPONSES_URL.to_string(),
            api_base_url: None,
            name: Some("GPT-5.6 Luna · ChatGPT/Codex".to_string()),
            description: Some("OpenAI Codex Responses model via ChatGPT sign-in".to_string()),
            max_completion_tokens: None,
            temperature: None,
            top_p: None,
            api_key: None,
            env_key: None,
            api_backend: ApiBackend::Responses,
            auth_scheme: None,
            reasoning_effort: None,
            supports_reasoning_effort: true,
            reasoning_efforts: vec![],
            extra_headers: codex_headers,
            context_window: NonZeroU64::new(200_000).unwrap(),
            auto_compact_threshold_percent: None,
            system_prompt_label: Some("codex".to_string()),
            use_concise: false,
            agent_type: "codex".to_string(),
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

    map
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
    use super::{builtin_community_models, is_supported_local_model_file, local_model_key};

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
        assert!(is_supported_local_model_file(std::path::Path::new(
            "model.GGUF"
        )));
        assert!(is_supported_local_model_file(std::path::Path::new(
            "model.ggml"
        )));
        assert!(!is_supported_local_model_file(std::path::Path::new(
            "model.bin"
        )));
    }
}
