use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptimizationStage {
    ToolSchema,
    PromptAssembly,
    PreCall,
    ToolResult,
    InterTurn,
    Cache,
    Rlm,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OptimizationSample {
    pub stage: OptimizationStage,
    pub before_tokens: u64,
    pub after_tokens: u64,
    pub transformed: bool,
    pub information_loss_possible: bool,
}

impl OptimizationSample {
    pub fn saved_tokens(&self) -> u64 {
        self.before_tokens.saturating_sub(self.after_tokens)
    }

    pub fn savings_percent(&self) -> f64 {
        if self.before_tokens == 0 {
            0.0
        } else {
            self.saved_tokens() as f64 / self.before_tokens as f64 * 100.0
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OptimizationMetrics {
    pub samples: Vec<OptimizationSample>,
    /// Final request-boundary measurement. Stage samples intentionally do not
    /// populate this because stages can overlap and must not be double-counted.
    pub request_before_tokens: Option<u64>,
    pub request_after_tokens: Option<u64>,
}

impl OptimizationMetrics {
    pub fn record(&mut self, sample: OptimizationSample) {
        self.samples.push(sample);
    }

    /// Record the only values suitable for the headline end-to-end savings
    /// number. Stage measurements remain useful for attribution.
    pub fn record_request(&mut self, before_tokens: u64, after_tokens: u64) {
        self.request_before_tokens = Some(before_tokens);
        self.request_after_tokens = Some(after_tokens.min(before_tokens));
    }

    pub fn stage_before_tokens(&self) -> u64 {
        self.samples.iter().map(|sample| sample.before_tokens).sum()
    }

    pub fn stage_after_tokens(&self) -> u64 {
        self.samples.iter().map(|sample| sample.after_tokens).sum()
    }

    /// End-to-end savings at the final model boundary. Returns zero until a
    /// request-boundary sample has been recorded.
    pub fn savings_percent(&self) -> f64 {
        let (Some(before), Some(after)) = (self.request_before_tokens, self.request_after_tokens)
        else {
            return 0.0;
        };
        if before == 0 {
            0.0
        } else {
            before.saturating_sub(after) as f64 / before as f64 * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headline_savings_requires_request_boundary_measurement() {
        let mut metrics = OptimizationMetrics::default();
        metrics.record(OptimizationSample {
            stage: OptimizationStage::ToolSchema,
            before_tokens: 100,
            after_tokens: 50,
            transformed: true,
            information_loss_possible: false,
        });
        metrics.record(OptimizationSample {
            stage: OptimizationStage::ToolResult,
            before_tokens: 100,
            after_tokens: 50,
            transformed: true,
            information_loss_possible: false,
        });
        assert_eq!(metrics.savings_percent(), 0.0);
        assert_eq!(metrics.stage_before_tokens(), 200);
        metrics.record_request(100, 50);
        assert_eq!(metrics.savings_percent(), 50.0);
    }
}
