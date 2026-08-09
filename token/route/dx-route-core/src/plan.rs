use crate::types::{CompressionMode, EngineStep};

#[derive(Debug, Clone)]
pub struct CompressionPlan {
  pub mode: CompressionMode,
  pub pipeline: Vec<EngineStep>,
}

impl CompressionPlan {
  pub fn off() -> Self {
    Self { mode: CompressionMode::Off, pipeline: vec![] }
  }

  pub fn single(mode: CompressionMode) -> Self {
    Self {
      mode,
      pipeline: vec![EngineStep {
        engine: mode.as_str().to_string(),
        intensity: Some("full".into()),
        target_budget: None,
      }],
    }
  }

  pub fn stacked(pipeline: Vec<EngineStep>) -> Self {
    Self { mode: CompressionMode::Stacked, pipeline }
  }

  pub fn is_off(&self) -> bool {
    self.mode == CompressionMode::Off || self.pipeline.is_empty()
  }
}

#[cfg(test)]
#[allow(missing_docs)]
mod tests {
  use super::*;

  #[test]
  fn off_plan() {
    let p = CompressionPlan::off();
    assert!(p.is_off());
  }

  #[test]
  fn single_plan() {
    let p = CompressionPlan::single(CompressionMode::Ultra);
    assert_eq!(p.mode, CompressionMode::Ultra);
    assert_eq!(p.pipeline.len(), 1);
    assert_eq!(p.pipeline[0].engine, "ultra");
  }

  #[test]
  fn stacked_plan() {
    let steps = vec![
      EngineStep { engine: "rtk".into(), intensity: Some("standard".into()), target_budget: None },
      EngineStep { engine: "caveman".into(), intensity: Some("full".into()), target_budget: None },
    ];
    let p = CompressionPlan::stacked(steps.clone());
    assert_eq!(p.pipeline.len(), 2);
    assert_eq!(p.pipeline[0].engine, "rtk");
  }
}
