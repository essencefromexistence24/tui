use serde::{Deserialize, Serialize};

/// Inputs used by the pre-call budget gate.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BudgetInput {
    pub prompt_tokens: u64,
    pub tool_schema_tokens: u64,
    pub tool_result_tokens: u64,
    pub reserved_output_tokens: u64,
    pub context_window_tokens: u64,
}

impl BudgetInput {
    pub fn total_input_tokens(self) -> u64 {
        self.prompt_tokens
            .saturating_add(self.tool_schema_tokens)
            .saturating_add(self.tool_result_tokens)
    }

    pub fn total_reserved_tokens(self) -> u64 {
        self.total_input_tokens()
            .saturating_add(self.reserved_output_tokens)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BudgetLimits {
    pub warning_tokens: u64,
    pub hard_limit_tokens: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BudgetDecision {
    Proceed,
    Optimize,
    Compact,
    Reject,
}

impl BudgetDecision {
    pub fn requires_action(self) -> bool {
        !matches!(self, Self::Proceed)
    }
}

pub fn evaluate_budget(input: BudgetInput, limits: BudgetLimits) -> BudgetDecision {
    let total = input.total_reserved_tokens();
    if limits.hard_limit_tokens > 0 && total > limits.hard_limit_tokens {
        return BudgetDecision::Reject;
    }
    if input.context_window_tokens > 0 && total >= input.context_window_tokens {
        return BudgetDecision::Compact;
    }
    if limits.warning_tokens > 0 && total >= limits.warning_tokens {
        return BudgetDecision::Optimize;
    }
    BudgetDecision::Proceed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturating_totals_prevent_overflow() {
        let input = BudgetInput {
            prompt_tokens: u64::MAX,
            tool_schema_tokens: 1,
            ..Default::default()
        };
        assert_eq!(input.total_input_tokens(), u64::MAX);
    }

    #[test]
    fn hard_limit_takes_precedence() {
        let input = BudgetInput {
            prompt_tokens: 901,
            ..Default::default()
        };
        assert_eq!(
            evaluate_budget(
                input,
                BudgetLimits {
                    warning_tokens: 500,
                    hard_limit_tokens: 900
                }
            ),
            BudgetDecision::Reject
        );
    }

    #[test]
    fn context_overflow_requires_compaction_before_normal_optimization() {
        let input = BudgetInput {
            prompt_tokens: 1_001,
            context_window_tokens: 1_000,
            ..Default::default()
        };
        assert_eq!(
            evaluate_budget(
                input,
                BudgetLimits {
                    warning_tokens: 500,
                    hard_limit_tokens: 0,
                }
            ),
            BudgetDecision::Compact
        );
    }
}
