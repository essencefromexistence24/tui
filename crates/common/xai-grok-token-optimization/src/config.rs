use crate::ToolRoutingConfig;
use serde::{Deserialize, Serialize};

/// Controls how optimization is applied to a model-facing request.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptimizationMode {
    /// Preserve the current payload. Useful for diagnostics and A/B tests.
    Off,
    /// Apply only transformations with an explicit safety contract.
    #[default]
    Safe,
    /// Permit bounded summaries and truncation when the request is over budget.
    Balanced,
    /// Permit all configured transformations. This remains explicit and never
    /// changes canonical tool dispatch data.
    Aggressive,
}

/// Runtime configuration shared by the agent, shell, and tools adapters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct OptimizationConfig {
    pub mode: OptimizationMode,
    pub enabled: bool,
    pub optimize_tool_schemas: bool,
    /// Advertise built-in tool schemas through the first-turn
    /// `Dx Serializer Compact` catalog. Native tool calls still carry JSON,
    /// and canonical schemas remain unchanged for runtime validation.
    pub dx_serializer_compact_tools: bool,
    pub route_tools: bool,
    pub compress_tool_results: bool,
    pub compact_history: bool,
    pub enable_prefix_cache: bool,
    pub enable_response_cache: bool,
    pub enable_rlm: bool,
    pub preserve_canonical_tool_data: bool,
    /// Lossless-first result normalization is enabled by default. A non-zero
    /// value additionally permits bounded preview truncation.
    pub max_tool_result_chars: usize,
    pub warning_threshold_tokens: u64,
    pub hard_limit_tokens: u64,
    pub tool_routing: ToolRoutingConfig,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            mode: OptimizationMode::Safe,
            enabled: true,
            optimize_tool_schemas: true,
            dx_serializer_compact_tools: true,
            route_tools: true,
            compress_tool_results: true,
            compact_history: true,
            enable_prefix_cache: true,
            enable_response_cache: false,
            enable_rlm: true,
            preserve_canonical_tool_data: true,
            max_tool_result_chars: 0,
            warning_threshold_tokens: 8_192,
            hard_limit_tokens: 0,
            tool_routing: ToolRoutingConfig::default(),
        }
    }
}

impl OptimizationConfig {
    pub fn effective_mode(&self) -> OptimizationMode {
        if self.enabled {
            self.mode
        } else {
            OptimizationMode::Off
        }
    }

    pub fn validates(&self) -> bool {
        self.preserve_canonical_tool_data
            && self.warning_threshold_tokens > 0
            && (self.hard_limit_tokens == 0
                || self.hard_limit_tokens >= self.warning_threshold_tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let config = OptimizationConfig::default();
        assert_eq!(config.effective_mode(), OptimizationMode::Safe);
        assert!(config.validates());
        assert!(config.preserve_canonical_tool_data);
        assert!(config.dx_serializer_compact_tools);
    }

    #[test]
    fn disabled_config_is_off() {
        let config = OptimizationConfig {
            enabled: false,
            ..Default::default()
        };
        assert_eq!(config.effective_mode(), OptimizationMode::Off);
    }
}
