use crate::error::CoreResult;
use crate::types::ContentRef;
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Output from a single engine execution.
#[derive(Debug)]
#[must_use]
pub struct EngineOutput {
    /// Compressed or transformed text.
    pub text: String,
    /// Content references (CCR refs or dedup markers).
    pub refs: Vec<ContentRef>,
    /// Tokens saved by this engine, if known.
    pub tokens_saved: Option<u32>,
}

impl EngineOutput {
    /// Create output with compressed text and no refs.
    pub fn new(text: String) -> Self {
        Self {
            text,
            refs: vec![],
            tokens_saved: None,
        }
    }

    /// Create output with compressed text and content refs.
    pub fn with_refs(text: String, refs: Vec<ContentRef>) -> Self {
        Self {
            text,
            refs,
            tokens_saved: None,
        }
    }
}

/// A compression engine that can transform request bodies.
pub trait Engine: Debug + Send + Sync {
    /// Stable name for this engine (used in plan resolution and routing).
    fn name(&self) -> &'static str;

    /// Apply compression synchronously.
    fn apply(&self, body: &str, intensity: &str) -> CoreResult<EngineOutput>;

    /// Apply compression asynchronously (default delegates to `apply`).
    fn apply_async<'a>(
        &'a self,
        body: &'a str,
        intensity: &'a str,
    ) -> Pin<Box<dyn Future<Output = CoreResult<EngineOutput>> + Send + 'a>>
    where
        Self: Sync,
    {
        Box::pin(async move { self.apply(body, intensity) })
    }
}

/// Thread-safe shared reference to an engine.
pub type SharedEngine = Arc<dyn Engine>;
