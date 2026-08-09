#![allow(missing_docs)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionCombo {
  pub id: String,
  pub name: String,
  pub description: String,
  pub pipeline: String,
  pub is_default: bool,
  pub created_at: Option<String>,
  pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionAssignment {
  pub routing_combo_id: String,
  pub compression_combo_id: String,
  pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStat {
  pub id: Option<i64>,
  pub request_id: String,
  pub original_tokens: i32,
  pub compressed_tokens: i32,
  pub savings_pct: f64,
  pub engine: String,
  pub mode: String,
  pub duration_ms: f64,
  pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CcrBlob {
  pub hash: String,
  pub content: String,
  pub size: i32,
  pub ref_count: i32,
  pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
  pub total_requests: i64,
  pub total_tokens_saved: i64,
  pub avg_savings_pct: f64,
  pub engine_breakdown: Vec<EngineBreakdown>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineBreakdown {
  pub engine: String,
  pub count: i64,
  pub tokens_saved: i64,
}
