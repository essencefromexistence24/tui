use crate::error::CoreResult;
use crate::plan::CompressionPlan;
use crate::types::{Config, RequestContext};

pub fn select_plan(config: &Config, ctx: &RequestContext) -> CompressionPlan {
  if !config.enabled {
    return CompressionPlan::off();
  }

  if let Some(mode) = &ctx.header_override {
    return CompressionPlan::single(*mode);
  }

  if let Some(mode) = config.combo_overrides.get(&ctx.combo_id) {
    return CompressionPlan::single(*mode);
  }

  if let Some(combo_id) = &config.active_combo_id
    && let Some(combo) = config.compression_combos.get(combo_id) {
      return CompressionPlan::stacked(combo.pipeline.clone());
    }

  if ctx.estimated_tokens >= config.auto_trigger_tokens {
    return CompressionPlan::single(config.auto_trigger_mode);
  }

  CompressionPlan::single(config.default_mode)
}

pub fn resolve_plan(config: &Config, ctx: &RequestContext) -> CoreResult<CompressionPlan> {
  Ok(select_plan(config, ctx))
}

#[cfg(test)]
#[allow(missing_docs)]
mod tests {
  use super::*;
  use crate::types::CompressionMode;

  fn ctx(override_mode: Option<CompressionMode>, combo: &str, tokens: u32) -> RequestContext {
    RequestContext {
      header_override: override_mode,
      combo_id: combo.to_string(),
      estimated_tokens: tokens,
      body: String::new(),
    }
  }

  #[test]
  fn disabled_returns_off() {
    let mut config = Config::default();
    config.enabled = false;
    assert_eq!(select_plan(&config, &ctx(None, "x", 0)).mode, CompressionMode::Off);
  }

  #[test]
  fn header_override_wins() {
    let config = Config::default();
    let plan = select_plan(&config, &ctx(Some(CompressionMode::Ultra), "x", 0));
    assert_eq!(plan.mode, CompressionMode::Ultra);
  }

  #[test]
  fn combo_override_wins_without_header() {
    let mut config = Config::default();
    config.combo_overrides.insert("special".into(), CompressionMode::Rtk);
    let plan = select_plan(&config, &ctx(None, "special", 0));
    assert_eq!(plan.mode, CompressionMode::Rtk);
  }

  #[test]
  fn active_combo_used_when_no_overrides() {
    let mut config = Config::default();
    config.active_combo_id = Some("default".into());
    let plan = select_plan(&config, &ctx(None, "other", 0));
    assert_eq!(plan.mode, CompressionMode::Stacked);
    assert_eq!(plan.pipeline.len(), 2);
  }

  #[test]
  fn auto_triggers_when_over_threshold() {
    let mut config = Config::default();
    config.active_combo_id = None;
    config.auto_trigger_tokens = 1000;
    config.default_mode = CompressionMode::Lite;
    let plan = select_plan(&config, &ctx(None, "x", 5000));
    assert_eq!(plan.mode, config.auto_trigger_mode);
  }

  #[test]
  fn falls_back_to_default_when_no_active_combo() {
    let mut config = Config::default();
    config.active_combo_id = None;
    config.default_mode = CompressionMode::Lite;
    let plan = select_plan(&config, &ctx(None, "nonexistent", 10));
    assert_eq!(plan.mode, config.default_mode);
  }
}
