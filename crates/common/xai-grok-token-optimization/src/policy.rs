use crate::{BudgetDecision, BudgetInput, BudgetLimits, OptimizationConfig, OptimizationMode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptimizationRequestKind {
    FirstTurn,
    ToolHeavyTurn,
    LongContextTurn,
    SubagentTurn,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OptimizationPolicy {
    pub route_tools: bool,
    pub minify_schemas: bool,
    pub compress_results: bool,
    pub compact_history: bool,
    pub use_prefix_cache: bool,
    pub use_response_cache: bool,
    pub use_rlm: bool,
}

impl OptimizationPolicy {
    pub fn for_request(
        config: &OptimizationConfig,
        request_kind: OptimizationRequestKind,
        budget: BudgetInput,
    ) -> (Self, BudgetDecision) {
        let decision = if config.effective_mode() == OptimizationMode::Off {
            BudgetDecision::Proceed
        } else {
            crate::evaluate_budget(
                budget,
                BudgetLimits {
                    warning_tokens: config.warning_threshold_tokens,
                    hard_limit_tokens: config.hard_limit_tokens,
                },
            )
        };
        let enabled = config.effective_mode() != OptimizationMode::Off;
        let compact_history = enabled
            && config.compact_history
            && matches!(
                request_kind,
                OptimizationRequestKind::LongContextTurn | OptimizationRequestKind::SubagentTurn
            );
        let use_rlm = enabled
            && config.enable_rlm
            && matches!(
                request_kind,
                OptimizationRequestKind::LongContextTurn | OptimizationRequestKind::ToolHeavyTurn
            );
        (
            Self {
                route_tools: enabled && config.route_tools,
                minify_schemas: enabled && config.optimize_tool_schemas,
                compress_results: enabled && config.compress_tool_results,
                compact_history,
                use_prefix_cache: enabled && config.enable_prefix_cache,
                use_response_cache: enabled && config.enable_response_cache,
                use_rlm,
            },
            decision,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_first_turn_routes_and_minifies_without_rlm() {
        let (policy, decision) = OptimizationPolicy::for_request(
            &OptimizationConfig::default(),
            OptimizationRequestKind::FirstTurn,
            BudgetInput::default(),
        );
        assert_eq!(decision, BudgetDecision::Proceed);
        assert!(policy.route_tools && policy.minify_schemas);
        assert!(!policy.use_rlm && !policy.compact_history);
    }
}
