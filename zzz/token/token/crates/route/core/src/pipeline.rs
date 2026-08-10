use crate::engine::{Engine, SharedEngine};
use crate::error::CoreResult;
use crate::plan::CompressionPlan;
use crate::stacked::{apply_stacked, apply_stacked_async};
use crate::strategy::resolve_plan;
use crate::types::{CompressedBody, CompressionStats, Config, RequestContext};
use std::sync::Arc;
use tracing::instrument;

#[derive(Debug)]
pub struct CompressionPipeline {
    pub config: Config,
    pub engines: Vec<Arc<dyn Engine>>,
}

impl CompressionPipeline {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            engines: Vec::new(),
        }
    }

    pub fn register(&mut self, engine: impl Engine + 'static) -> &mut Self {
        self.engines.push(Arc::new(engine));
        self
    }

    pub fn register_shared(&mut self, engine: SharedEngine) -> &mut Self {
        self.engines.push(engine);
        self
    }

    pub fn engine_names(&self) -> Vec<&str> {
        self.engines.iter().map(|e| e.name()).collect()
    }

    #[instrument(skip(self, ctx), fields(combo = %ctx.combo_id, tokens = ctx.estimated_tokens))]
    pub fn compress(&self, ctx: &RequestContext) -> CoreResult<CompressedBody> {
        let plan = resolve_plan(&self.config, ctx)?;
        self.compress_with_plan(&ctx.body, &plan)
    }

    #[instrument(skip(self))]
    pub fn compress_with_plan(
        &self,
        body: &str,
        plan: &CompressionPlan,
    ) -> CoreResult<CompressedBody> {
        if plan.is_off() || body.is_empty() {
            return Ok(CompressedBody {
                text: body.to_string(),
                stats: CompressionStats::builder().build(),
                refs: vec![],
            });
        }

        apply_stacked(body, &plan.pipeline, &self.engines)
    }

    #[instrument(skip(self, ctx))]
    pub async fn compress_async(&self, ctx: &RequestContext) -> CoreResult<CompressedBody> {
        let plan = resolve_plan(&self.config, ctx)?;
        self.compress_with_plan_async(&ctx.body, &plan).await
    }

    pub async fn compress_with_plan_async(
        &self,
        body: &str,
        plan: &CompressionPlan,
    ) -> CoreResult<CompressedBody> {
        if plan.is_off() || body.is_empty() {
            return Ok(CompressedBody {
                text: body.to_string(),
                stats: CompressionStats::builder().build(),
                refs: vec![],
            });
        }

        apply_stacked_async(body, &plan.pipeline, &self.engines).await
    }
}

#[cfg(test)]
#[allow(missing_docs)]
mod tests {
    use super::*;
    use crate::engine::EngineOutput;
    use crate::types::CompressionMode;

    #[derive(Debug)]
    struct Passthrough;

    impl Engine for Passthrough {
        fn name(&self) -> &'static str {
            "passthrough"
        }
        fn apply(&self, body: &str, _intensity: &str) -> CoreResult<EngineOutput> {
            Ok(EngineOutput {
                text: body.to_string(),
                refs: vec![],
                tokens_saved: None,
            })
        }
    }

    #[test]
    fn passthrough_when_disabled() {
        let config = Config {
            enabled: false,
            ..Default::default()
        };
        let pipeline = CompressionPipeline::new(config);
        let ctx = RequestContext {
            header_override: None,
            combo_id: "x".into(),
            estimated_tokens: 100,
            body: "hello".into(),
        };
        let result = pipeline.compress(&ctx).unwrap();
        assert_eq!(result.text, "hello");
    }

    #[test]
    fn register_engine_appears_in_names() {
        let mut pipeline = CompressionPipeline::new(Config::default());
        pipeline.register(Passthrough);
        assert_eq!(pipeline.engine_names(), vec!["passthrough"]);
    }

    #[test]
    fn compress_applies_plan() {
        let mut pipeline = CompressionPipeline::new(Config::default());
        pipeline.register(Passthrough);
        let plan = CompressionPlan::off();
        let result = pipeline.compress_with_plan("data", &plan).unwrap();
        assert_eq!(result.text, "data");
    }

    #[tokio::test]
    async fn compress_async_produces_stats() {
        #[derive(Debug)]
        struct LiteStub;
        impl Engine for LiteStub {
            fn name(&self) -> &'static str {
                "lite"
            }
            fn apply(&self, body: &str, _i: &str) -> CoreResult<EngineOutput> {
                Ok(EngineOutput::new(body.to_string()))
            }
        }
        let config = Config {
            active_combo_id: None,
            ..Default::default()
        };
        let mut pipeline = CompressionPipeline::new(config);
        pipeline.register(LiteStub);
        let ctx = RequestContext {
            header_override: Some(CompressionMode::Lite),
            combo_id: "x".into(),
            estimated_tokens: 10,
            body: "test data".into(),
        };
        let result = pipeline.compress_async(&ctx).await.unwrap();
        assert!(result.stats.original_tokens > 0);
    }
}
