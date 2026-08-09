//! Safe, transport-agnostic token optimization primitives.
//!
//! This crate owns policy, budgets, accounting, and safety contracts. The
//! agent, shell, and tools crates provide adapters for concrete DX payloads.

mod budget;
mod config;
mod metrics;
mod policy;
mod routing;
mod transforms;

pub use budget::{BudgetDecision, BudgetInput, BudgetLimits, evaluate_budget};
pub use config::{OptimizationConfig, OptimizationMode};
pub use metrics::{OptimizationMetrics, OptimizationSample, OptimizationStage};
pub use policy::{OptimizationPolicy, OptimizationRequestKind};
pub use routing::{ToolCandidate, ToolRoute, ToolRoutingConfig, route_tools};
pub use transforms::{compress_tool_result, minify_tool_schema};
