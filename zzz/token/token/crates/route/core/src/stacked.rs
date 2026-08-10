use crate::engine::Engine;
use crate::error::{CoreError, CoreResult};
use crate::types::{CompressedBody, CompressionStats, ContentRef, EngineStep};
use std::sync::Arc;
use tracing::{debug, instrument};

pub fn estimate_tokens(text: &str) -> CoreResult<usize> {
    match tiktoken_rs::cl100k_base() {
        Ok(bpe) => Ok(bpe.encode_with_special_tokens(text).len()),
        Err(_e) => {
            // CJK-friendly fallback: count whitespace-separated words + chars/3 for CJK
            let words = text.split_whitespace().count().max(1);
            let cjk_chars = text
                .chars()
                .filter(|&c| {
                    c as u32 >= 0x4E00 && c as u32 <= 0x9FFF
                        || c as u32 >= 0x3040 && c as u32 <= 0x30FF
                        || c as u32 >= 0xAC00 && c as u32 <= 0xD7AF
                })
                .count();
            let fallback = words + cjk_chars + text.len() / 4;
            debug!(
                "tiktoken unavailable, using fallback estimate: {}",
                fallback
            );
            Ok(fallback)
        }
    }
}

pub fn enforce_budget(text: &str, max_tokens: u32) -> CoreResult<String> {
    let bpe = tiktoken_rs::cl100k_base().map_err(|e| CoreError::Tokenizer(e.to_string()))?;
    let tokens = bpe.encode_with_special_tokens(text);

    if tokens.len() <= max_tokens as usize {
        return Ok(text.to_string());
    }

    let truncated: Vec<u32> = tokens.into_iter().take(max_tokens as usize).collect();
    bpe.decode(truncated)
        .map_err(|e| CoreError::Tokenizer(e.to_string()))
}

#[instrument(skip_all, fields(steps = %plan.len()))]
pub fn apply_stacked(
    body: &str,
    plan: &[EngineStep],
    engines: &[Arc<dyn Engine>],
) -> CoreResult<CompressedBody> {
    let start = std::time::Instant::now();
    let mut current = body.to_string();
    let mut all_engines: Vec<String> = Vec::with_capacity(plan.len());
    let mut all_refs: Vec<ContentRef> = Vec::new();

    for step in plan {
        let engine = engines
            .iter()
            .find(|e| e.name() == step.engine)
            .ok_or_else(|| CoreError::EngineNotFound(step.engine.clone()))?;

        let intensity = step.intensity.as_deref().unwrap_or("full");

        let result = engine
            .apply(&current, intensity)
            .map_err(|e| CoreError::EngineFailed(step.engine.clone(), e.into()))?;

        all_engines.push(engine.name().to_string());
        all_refs.extend(result.refs);
        current = result.text;

        if let Some(budget) = step.target_budget {
            let tokens = estimate_tokens(&current)?;
            if tokens > budget as usize {
                current = enforce_budget(&current, budget)?;
                debug!(
                    budget,
                    "hard budget enforced after engine '{}'", step.engine
                );
            }
        }
    }

    let original_tokens = estimate_tokens(body)? as u32;
    let compressed_tokens = estimate_tokens(&current)? as u32;
    let duration = start.elapsed().as_secs_f64() * 1000.0;

    let savings_pct = if original_tokens > 0 {
        (original_tokens.saturating_sub(compressed_tokens)) as f64 / original_tokens as f64 * 100.0
    } else {
        0.0
    };

    let stats = CompressionStats::builder()
        .original_tokens(original_tokens)
        .compressed_tokens(compressed_tokens)
        .savings_pct(savings_pct)
        .engines(all_engines)
        .duration_ms(duration)
        .build();

    Ok(CompressedBody {
        text: current,
        stats,
        refs: all_refs,
    })
}

pub async fn apply_stacked_async(
    body: &str,
    plan: &[EngineStep],
    engines: &[Arc<dyn Engine>],
) -> CoreResult<CompressedBody> {
    let start = std::time::Instant::now();
    let mut current = body.to_string();
    let mut all_refs: Vec<ContentRef> = Vec::new();
    let mut engine_names: Vec<String> = Vec::new();

    for step in plan {
        let engine = engines
            .iter()
            .find(|e| e.name() == step.engine)
            .ok_or_else(|| CoreError::EngineNotFound(step.engine.clone()))?;

        let intensity = step.intensity.as_deref().unwrap_or("full");
        let result = engine
            .apply_async(&current, intensity)
            .await
            .map_err(|e| CoreError::EngineFailed(step.engine.clone(), e.into()))?;

        engine_names.push(engine.name().to_string());
        all_refs.extend(result.refs);
        current = result.text;
    }

    let original_tokens = estimate_tokens(body)? as u32;
    let compressed_tokens = estimate_tokens(&current)? as u32;
    let duration = start.elapsed().as_secs_f64() * 1000.0;

    let savings_pct = if original_tokens > 0 {
        (original_tokens.saturating_sub(compressed_tokens)) as f64 / original_tokens as f64 * 100.0
    } else {
        0.0
    };

    let stats = CompressionStats::builder()
        .original_tokens(original_tokens)
        .compressed_tokens(compressed_tokens)
        .savings_pct(savings_pct)
        .engines(engine_names)
        .duration_ms(duration)
        .build();

    Ok(CompressedBody {
        text: current,
        stats,
        refs: all_refs,
    })
}

#[cfg(test)]
#[allow(missing_docs)]
mod tests {
    use super::*;
    use crate::engine::EngineOutput;

    #[derive(Debug)]
    struct FakeEngine(&'static str);

    impl Engine for FakeEngine {
        fn name(&self) -> &'static str {
            self.0
        }
        fn apply(&self, body: &str, _intensity: &str) -> CoreResult<EngineOutput> {
            Ok(EngineOutput::new(format!("<{}>{}", self.0, body)))
        }
    }

    #[test]
    fn estimate_tokens_short() {
        let n = estimate_tokens("hello world").unwrap();
        assert!(n > 0, "should produce non-zero token count");
    }

    #[test]
    fn estimate_tokens_long() {
        let text = "token ".repeat(500);
        let n = estimate_tokens(&text).unwrap();
        assert!(n > 200, "long text should produce many tokens");
    }

    #[test]
    fn enforce_budget_noop_when_under() {
        let result = enforce_budget("short", 10_000).unwrap();
        assert_eq!(result, "short");
    }

    #[test]
    fn apply_stacked_empty_plan() {
        let result = apply_stacked("hello", &[], &[]).unwrap();
        assert_eq!(result.text, "hello");
    }

    #[test]
    fn apply_stacked_single_engine() {
        let engines: Vec<Arc<dyn Engine>> = vec![Arc::new(FakeEngine("mock"))];
        let plan = vec![EngineStep {
            engine: "mock".into(),
            intensity: Some("full".into()),
            target_budget: None,
        }];
        let result = apply_stacked("test", &plan, &engines).unwrap();
        assert_eq!(result.text, "<mock>test");
    }

    #[test]
    fn apply_stacked_two_engines() {
        let engines: Vec<Arc<dyn Engine>> =
            vec![Arc::new(FakeEngine("a")), Arc::new(FakeEngine("b"))];
        let plan = vec![
            EngineStep {
                engine: "a".into(),
                intensity: Some("full".into()),
                target_budget: None,
            },
            EngineStep {
                engine: "b".into(),
                intensity: Some("full".into()),
                target_budget: None,
            },
        ];
        let result = apply_stacked("x", &plan, &engines).unwrap();
        assert_eq!(result.text, "<b><a>x");
    }

    #[test]
    fn engine_not_found_error() {
        let engines: Vec<Arc<dyn Engine>> = vec![];
        let plan = vec![EngineStep {
            engine: "nope".into(),
            intensity: None,
            target_budget: None,
        }];
        let err = apply_stacked("x", &plan, &engines).unwrap_err();
        assert!(err.to_string().contains("nope"));
    }

    #[tokio::test]
    async fn apply_stacked_async_works() {
        let engines: Vec<Arc<dyn Engine>> = vec![Arc::new(FakeEngine("async"))];
        let plan = vec![EngineStep {
            engine: "async".into(),
            intensity: Some("full".into()),
            target_budget: None,
        }];
        let result = apply_stacked_async("test", &plan, &engines).await.unwrap();
        assert_eq!(result.text, "<async>test");
    }
}
