//! # Recursive Language Models (RLM)
//!
//! `rlm` is an embeddable long-context runtime for Rust hosts that need to
//! search, summarize, or reduce oversized text before handing work to a model.
//!
//! ## Features
//!
//! - Typed document, request, and response surfaces
//! - OpenAI-compatible provider configuration
//! - Search-oriented Rhai execution with fast substring helpers
//! - Standard and streaming execution modes
//! - Optional fast-model routing for cheaper exploration passes
//!
//! ## Quick Start
//!
//! ```no_run
//! use rlm::{RLM, RLMDocument};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let rlm = RLM::from_env_groq("meta-llama/llama-4-scout-17b-16e-instruct")?
//!         .with_fast_model("meta-llama/llama-3.3-70b-versatile".to_string())
//!         .with_max_iterations(24);
//!
//!     let document = RLMDocument::from_text(
//!         "demo",
//!         "Your large document here..."
//!     );
//!     let response = rlm
//!         .complete_document("What is this about?", document)
//!         .await?;
//!     
//!     println!("Answer: {}", response.answer);
//!     
//!     Ok(())
//! }
//! ```

pub mod rlm;
pub mod llm;
pub mod repl;
pub mod parser;
pub mod error;

pub use llm::{LLMAuthScheme, LLMProviderConfig, Message};
pub use rlm::{
    RLMBuilder, RLMChunk, RLMChunkingConfig, RLMDocument, RLMProfile, RLMRecursiveResponse,
    RLMReductionPass, RLMRequest, RLMResponse, RLMRunMode, RLMStats, RLMTaskKind, RLM,
};
pub use error::{RLMError, Result};
