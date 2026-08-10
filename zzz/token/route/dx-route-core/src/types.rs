use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionMode {
  Off,
  Lite,
  Caveman,
  Rtk,
  Ultra,
  Aggressive,
  Headroom,
  Stacked,
}

impl CompressionMode {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Off => "off",
      Self::Lite => "lite",
      Self::Caveman => "caveman",
      Self::Rtk => "rtk",
      Self::Ultra => "ultra",
      Self::Aggressive => "aggressive",
      Self::Headroom => "headroom",
      Self::Stacked => "stacked",
    }
  }

  pub fn from_str_name(s: &str) -> Option<Self> {
    match s {
      "off" => Some(Self::Off),
      "lite" => Some(Self::Lite),
      "caveman" => Some(Self::Caveman),
      "rtk" => Some(Self::Rtk),
      "ultra" => Some(Self::Ultra),
      "aggressive" => Some(Self::Aggressive),
      "headroom" => Some(Self::Headroom),
      "stacked" => Some(Self::Stacked),
      _ => None,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStats {
  pub request_id: String,
  pub original_tokens: u32,
  pub compressed_tokens: u32,
  pub savings_pct: f64,
  pub engines: Vec<String>,
  pub duration_ms: f64,
  pub created_at: String,
}

impl CompressionStats {
  pub fn builder() -> CompressionStatsBuilder {
    CompressionStatsBuilder::default()
  }

  pub fn tokens_saved(&self) -> u32 {
    self.original_tokens.saturating_sub(self.compressed_tokens)
  }
}

#[derive(Default)]
pub struct CompressionStatsBuilder {
  request_id: Option<String>,
  original_tokens: u32,
  compressed_tokens: u32,
  savings_pct: f64,
  engines: Vec<String>,
  duration_ms: f64,
}

impl CompressionStatsBuilder {
  pub fn request_id(mut self, id: String) -> Self {
    self.request_id = Some(id);
    self
  }
  pub fn original_tokens(mut self, n: u32) -> Self {
    self.original_tokens = n;
    self
  }
  pub fn compressed_tokens(mut self, n: u32) -> Self {
    self.compressed_tokens = n;
    self
  }
  pub fn savings_pct(mut self, pct: f64) -> Self {
    self.savings_pct = pct;
    self
  }
  pub fn engines(mut self, engines: Vec<String>) -> Self {
    self.engines = engines;
    self
  }
  pub fn duration_ms(mut self, ms: f64) -> Self {
    self.duration_ms = ms;
    self
  }
  pub fn build(self) -> CompressionStats {
    CompressionStats {
      request_id: self.request_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
      original_tokens: self.original_tokens,
      compressed_tokens: self.compressed_tokens,
      savings_pct: self.savings_pct,
      engines: self.engines,
      duration_ms: self.duration_ms,
      created_at: chrono::Utc::now().to_rfc3339(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentRef {
  pub id: String,
  pub hash: String,
  pub original_len: usize,
  pub compressed_len: usize,
  pub engine: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressedBody {
  pub text: String,
  pub stats: CompressionStats,
  pub refs: Vec<ContentRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStep {
  pub engine: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub intensity: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub target_budget: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
  pub enabled: bool,
  pub default_mode: CompressionMode,
  pub auto_trigger_mode: CompressionMode,
  pub auto_trigger_tokens: u32,
  pub combo_overrides: HashMap<String, CompressionMode>,
  pub compression_combos: HashMap<String, CompressionCombo>,
  pub active_combo_id: Option<String>,
  pub preserve_system_prompt: bool,
}

impl Default for Config {
  fn default() -> Self {
    let mut combos = HashMap::new();
    combos.insert(
      "default".to_string(),
      CompressionCombo {
        id: "default".to_string(),
        name: "Default Stack".to_string(),
        description: "RTK + Caveman stacked pipeline".to_string(),
        pipeline: vec![
          EngineStep { engine: "rtk".to_string(), intensity: Some("standard".into()), target_budget: None },
          EngineStep { engine: "caveman".to_string(), intensity: Some("full".into()), target_budget: None },
        ],
        is_default: true,
      },
    );

    Self {
      enabled: true,
      default_mode: CompressionMode::Lite,
      auto_trigger_mode: CompressionMode::Aggressive,
      auto_trigger_tokens: 4096,
      combo_overrides: HashMap::new(),
      compression_combos: combos,
      active_combo_id: Some("default".to_string()),
      preserve_system_prompt: true,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionCombo {
  pub id: String,
  pub name: String,
  pub description: String,
  pub pipeline: Vec<EngineStep>,
  pub is_default: bool,
}

impl CompressionCombo {
  pub fn plan(&self) -> Vec<EngineStep> {
    self.pipeline.clone()
  }
}

#[derive(Debug, Clone)]
pub struct RequestContext {
  pub header_override: Option<CompressionMode>,
  pub combo_id: String,
  pub estimated_tokens: u32,
  pub body: String,
}

#[cfg(test)]
#[allow(missing_docs)]
mod tests {
  use super::*;

  #[test]
  fn compression_mode_roundtrip() {
    for mode in &[
      CompressionMode::Off,
      CompressionMode::Lite,
      CompressionMode::Caveman,
      CompressionMode::Rtk,
      CompressionMode::Ultra,
      CompressionMode::Aggressive,
      CompressionMode::Headroom,
      CompressionMode::Stacked,
    ] {
      assert_eq!(CompressionMode::from_str_name(mode.as_str()), Some(*mode));
    }
  }

  #[test]
  fn stats_builder_defaults() {
    let stats = CompressionStats::builder().build();
    assert!(!stats.request_id.is_empty());
    assert!(!stats.created_at.is_empty());
  }

  #[test]
  fn stats_builder_custom_values() {
    let stats = CompressionStats::builder()
      .original_tokens(1000)
      .compressed_tokens(400)
      .savings_pct(60.0)
      .engines(vec!["lite".into(), "caveman".into()])
      .duration_ms(12.5)
      .build();
    assert_eq!(stats.original_tokens, 1000);
    assert_eq!(stats.compressed_tokens, 400);
    assert_eq!(stats.tokens_saved(), 600);
  }

  #[test]
  fn config_default_has_default_combo() {
    let config = Config::default();
    assert!(config.enabled);
    assert_eq!(config.default_mode, CompressionMode::Lite);
    assert!(config.compression_combos.contains_key("default"));
  }

  #[test]
  fn combo_plan_returns_pipeline() {
    let combo = CompressionCombo {
      id: "test".into(),
      name: "Test".into(),
      description: "".into(),
      pipeline: vec![EngineStep { engine: "ultra".into(), intensity: Some("full".into()), target_budget: None }],
      is_default: true,
    };
    let plan = combo.plan();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].engine, "ultra");
  }
}
