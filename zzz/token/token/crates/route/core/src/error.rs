use thiserror::Error;

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("engine '{0}' not found in registry")]
    EngineNotFound(String),

    #[error("engine '{0}' failed: {1}")]
    EngineFailed(String, #[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("stacked pipeline failed at engine '{0}': {1}")]
    StackedPipelineError(String, String),

    #[error("token budget {budget} exceeded by {actual} tokens")]
    BudgetExceeded { budget: u32, actual: usize },

    #[error("unknown compression mode: {0}")]
    InvalidMode(String),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("tiktoken error: {0}")]
    Tokenizer(String),

    #[error("invalid engine step: {0}")]
    InvalidEngineStep(String),
}

pub type CoreResult<T> = Result<T, CoreError>;
