use std::num::NonZeroU64;

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
        ModelSpec { key: "big-pickle", name: "Big Pickle", model_id: "big-pickle", ctx: 200_000, title_header: Some("opencode") },
        ModelSpec { key: "deepseek-v4-flash-free", name: "DeepSeek V4 Flash Free", model_id: "deepseek-v4-flash-free", ctx: 200_000, title_header: None },
        ModelSpec { key: "mimo-v2.5-free", name: "MiMo V2.5 Free", model_id: "mimo-v2.5-free", ctx: 131_000, title_header: None },
        ModelSpec { key: "hy3-free", name: "HY3 Free", model_id: "hy3-free", ctx: 131_000, title_header: None },
        ModelSpec { key: "nemotron-3-ultra-free", name: "Nemotron 3 Ultra Free", model_id: "nemotron-3-ultra-free", ctx: 1_000_000, title_header: None },
        ModelSpec { key: "north-mini-code-free", name: "North Mini Code Free", model_id: "north-mini-code-free", ctx: 131_000, title_header: None },
    ];

    for m in &models {
        let mut extra_headers = IndexMap::new();
        extra_headers.insert("HTTP-Referer".to_string(), "https://opencode.ai/".to_string());
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
                laziness_detector: LazinessDetectorPerModelConfig::default(),
            },
        );
    }

    map
}
