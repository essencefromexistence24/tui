#![deny(unsafe_code)]
//! dx-route-core — pipeline orchestrator for the dx-route token saver.

pub mod engine;
pub mod error;
pub mod pipeline;
pub mod plan;
pub mod stacked;
pub mod strategy;
pub mod types;

pub use engine::{Engine, EngineOutput, SharedEngine};
pub use error::{CoreError, CoreResult};
pub use pipeline::CompressionPipeline;
pub use plan::CompressionPlan;
pub use stacked::{apply_stacked, estimate_tokens};
pub use strategy::resolve_plan;
pub use types::*;
