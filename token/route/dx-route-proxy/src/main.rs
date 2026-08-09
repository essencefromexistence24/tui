#![deny(unsafe_code)]

use axum::{
  body::Body,
  extract::State,
  http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Method},
  response::{IntoResponse, Json, Response},
  routing::{get, post},
  Router,
};
use clap::Parser;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

#[derive(Parser, Clone)]
#[command(name = "dx-proxy", about = "dx-route HTTP compression proxy", version)]
struct Args {
  #[arg(short, long, default_value = "9812")]
  port: u16,

  #[arg(short, long, default_value = "https://api.openai.com/v1")]
  upstream: String,

  #[arg(short, long, default_value = "~/.dx-route/stats.db")]
  db: String,

  #[arg(short, long)]
  api_key: Option<String>,

  #[arg(long, default_value = "lite")]
  mode: String,

  #[arg(long, default_value = "30000")]
  upstream_timeout_ms: u64,
}

#[derive(Clone)]
struct AppState {
  args: Args,
  client: reqwest::Client,
  store: Option<Arc<dx_route_storage::Store>>,
}

fn build_mode_pipeline(
  body: &str,
  mode: &str,
) -> Result<String, dx_route_core::CoreError> {
  match mode {
    "off" => Ok(body.to_string()),
    "lite" => dx_route_lite::compress(body, "full").map(|r| r.text)
      .map_err(|e| dx_route_core::CoreError::EngineFailed("lite".into(), e.into())),
    "caveman" => dx_route_caveman::compress(body, "full").map(|r| r.text)
      .map_err(|e| dx_route_core::CoreError::EngineFailed("caveman".into(), e.into())),
    "rtk" => dx_route_rtk::compress(body, None).map(|r| r.text)
      .map_err(|e| dx_route_core::CoreError::EngineFailed("rtk".into(), e.into())),
    "ultra" => dx_route_ultra::compress(body, "full").map(|r| r.text)
      .map_err(|e| dx_route_core::CoreError::EngineFailed("ultra".into(), e.into())),
    "aggressive" => dx_route_aggressive::compress(body, "full").map(|r| r.text)
      .map_err(|e| dx_route_core::CoreError::EngineFailed("aggressive".into(), e.into())),
    _ => {
      warn!(mode, "unknown compression mode, using default 'lite'");
      dx_route_lite::compress(body, "full").map(|r| r.text)
        .map_err(|e| dx_route_core::CoreError::EngineFailed("lite".into(), e.into()))
    }
  }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let filter = tracing_subscriber::EnvFilter::builder()
    .with_default_directive(tracing::Level::INFO.into())
    .from_env_lossy();
  tracing_subscriber::fmt().with_env_filter(filter).json().init();

  let args = Args::parse();
  info!(
    port = args.port,
    upstream = %args.upstream,
    mode = %args.mode,
    "proxy starting"
  );

  let client = reqwest::Client::builder()
    .timeout(Duration::from_millis(args.upstream_timeout_ms))
    .user_agent("dx-route/0.1.0")
    .build()?;

  let store = dx_route_storage::Store::open(&shellexpand::tilde(&args.db))
    .map(Arc::new)
    .map_err(|e| warn!("stats db unavailable: {}", e))
    .ok();

  let state = AppState { args: args.clone(), client, store };

  let cors = CorsLayer::new()
    .allow_origin(tower_http::cors::Any)
    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
    .allow_headers(tower_http::cors::Any);

  let app = Router::new()
    .route("/health", get(health))
    .route("/v1/models", get(list_models))
    .route("/models", get(list_models))
    .route("/v1/chat/completions", post(chat_completions))
    .route("/chat/completions", post(chat_completions))
    .layer(TraceLayer::new_for_http())
    .layer(cors)
    .with_state(state);

  let addr = format!("0.0.0.0:{}", args.port);
  let listener = tokio::net::TcpListener::bind(&addr).await?;

  info!("listening on {}", addr);

  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;

  Ok(())
}

async fn shutdown_signal() {
  if let Err(e) = tokio::signal::ctrl_c().await {
    warn!("failed to install ctrl+c handler: {}", e);
  }
  info!("shutting down gracefully");
}

async fn health() -> Json<serde_json::Value> {
  Json(serde_json::json!({ "status": "ok", "service": "dx-route-proxy", "version": "0.1.0" }))
}

async fn list_models() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "object": "list",
    "data": [{
      "id": "dx-route-compressor",
      "object": "model",
      "created": 1710000000,
      "owned_by": "dx-route"
    }]
  }))
}

async fn chat_completions(
  State(state): State<AppState>,
  client_headers: HeaderMap,
  body_bytes: bytes::Bytes,
) -> Response {
  // Parse request to check for streaming
  let is_streaming = serde_json::from_slice::<serde_json::Value>(&body_bytes)
    .ok()
    .and_then(|v| v.get("stream").and_then(|s| s.as_bool()))
    .unwrap_or(false);

  let original_text = match String::from_utf8(body_bytes.to_vec()) {
    Ok(t) => t,
    Err(_) => {
      error!("request body is not valid UTF-8");
      return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
        "error": { "message": "request body must be valid UTF-8", "type": "invalid_request" }
      }))).into_response();
    }
  };
  let original_len = original_text.len();

  let compressed = match build_mode_pipeline(&original_text, &state.args.mode) {
    Ok(c) => c,
    Err(e) => {
      warn!("compression failed: {}, forwarding uncompressed", e);
      original_text.clone()
    }
  };

  let compressed_len = compressed.len();
  let savings = if original_len > 0 {
    (original_len - compressed_len) as f64 / original_len as f64 * 100.0
  } else {
    0.0
  };

  debug!(
    original_len,
    compressed_len,
    savings_pct = format!("{:.1}", savings),
    streaming = is_streaming,
    "request compressed"
  );

  let upstream_url = format!("{}/chat/completions", state.args.upstream.trim_end_matches('/'));

  // Build upstream headers: forward client headers, override auth + content-type
  let mut upstream_headers = HeaderMap::new();
  for (key, value) in client_headers.iter() {
    let key_str = key.as_str().to_lowercase();
    // Skip hop-by-hop headers that should not be forwarded
    if matches!(
      key_str.as_str(),
      "host" | "connection" | "transfer-encoding" | "proxy-connection"
        | "keep-alive" | "upgrade" | "proxy-authorization"
    ) {
      continue;
    }
    upstream_headers.insert(key.clone(), value.clone());
  }
  if let Some(ak) = &state.args.api_key {
    let name = HeaderName::from_static("authorization");
    if let Ok(value) = HeaderValue::from_str(&format!("Bearer {}", ak)) {
      upstream_headers.insert(name, value);
    }
  }
  let name = HeaderName::from_static("content-type");
  upstream_headers.insert(name, HeaderValue::from_static("application/json"));

  let upstream_body = if compressed_len < original_len {
    inject_compression_notice(&compressed)
  } else {
    compressed.clone()
  };

  // Build the upstream request
  let upstream_req = state.client.post(&upstream_url).headers(upstream_headers);

  if is_streaming {
    // Streaming path: pipe upstream SSE stream directly to client
    match upstream_req.body(upstream_body).send().await {
      Ok(up_resp) => {
        let status = up_resp.status();
        let resp_headers: Vec<(HeaderName, HeaderValue)> = up_resp.headers().iter()
          .filter(|(k, _)| k.as_str().to_lowercase() != "transfer-encoding")
          .map(|(k, v)| (k.clone(), v.clone()))
          .collect();

        // Record stats asynchronously
        let store = state.store.clone();
        let orig = original_text.clone();
        let comp = compressed.clone();
        let mode = state.args.mode.clone();
        tokio::spawn(async move {
          record_stats_async(store, &orig, &comp, &mode).await;
        });

        let stream = up_resp.bytes_stream();
        let body = Body::from_stream(stream);
        let mut response = Response::new(body);
        *response.status_mut() = status;
        for (key, value) in &resp_headers {
          if key.as_str().to_lowercase() == "transfer-encoding" {
            continue;
          }
          response.headers_mut().insert(key.clone(), value.clone());
        }
        add_dx_headers(&mut response, &state.args.mode, savings);
        response
      }
      Err(e) => {
        error!(error = %e, upstream = %state.args.upstream, "upstream request failed");
        (StatusCode::BAD_GATEWAY, Json(serde_json::json!({
          "error": { "message": format!("upstream request failed: {}", e), "type": "upstream_error" }
        }))).into_response()
      }
    }
  } else {
    // Non-streaming path: buffer upstream response
    match upstream_req.body(upstream_body).send().await {
      Ok(up_resp) => {
        let status = up_resp.status();
        let resp_headers: Vec<(HeaderName, HeaderValue)> = up_resp.headers().iter()
          .filter(|(k, _)| k.as_str().to_lowercase() != "transfer-encoding")
          .map(|(k, v)| (k.clone(), v.clone()))
          .collect();
        let mut response = match up_resp.bytes().await {
          Ok(body) => {
            let mut resp = Response::new(body.into());
            *resp.status_mut() = status;
            for (key, value) in &resp_headers {
              resp.headers_mut().insert(key.clone(), value.clone());
            }
            resp
          }
          Err(e) => {
            error!(error = %e, "failed to read upstream response body");
            return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({
              "error": { "message": "failed to read upstream response", "type": "proxy_error" }
            }))).into_response();
          }
        };
        add_dx_headers(&mut response, &state.args.mode, savings);

        let store = state.store.clone();
        let orig = original_text.clone();
        let comp = compressed.clone();
        let mode = state.args.mode.clone();
        tokio::spawn(async move {
          record_stats_async(store, &orig, &comp, &mode).await;
        });

        response
      }
      Err(e) => {
        error!(error = %e, upstream = %state.args.upstream, "upstream request failed");
        (StatusCode::BAD_GATEWAY, Json(serde_json::json!({
          "error": { "message": format!("upstream request failed: {}", e), "type": "upstream_error" }
        }))).into_response()
      }
    }
  }
}

fn add_dx_headers(response: &mut Response, mode: &str, savings: f64) {
  if let Ok(v) = HeaderValue::from_str(&format!("{} (saved {:.1}%)", mode, savings)) {
    response
      .headers_mut()
      .insert(HeaderName::from_static("x-dx-route"), v);
  }
  if let Ok(v) = HeaderValue::from_str(&format!("{:.1}", savings)) {
    response
      .headers_mut()
      .insert(HeaderName::from_static("x-dx-route-savings"), v);
  }
}

fn inject_compression_notice(text: &str) -> String {
  if let Ok(mut req) = serde_json::from_str::<serde_json::Value>(text) {
    if let Some(messages) = req.get_mut("messages").and_then(|m| m.as_array_mut()) {
      let notice = "[Note: The following prompt was compressed by dx-route to reduce token usage. Some filler words, articles, and verbose phrasing may have been condensed. The semantic meaning is preserved.]";
      if let Some(first) = messages.first_mut()
        && let Some(content) = first.get_mut("content")
          && let Some(s) = content.as_str()
      {
        *content = serde_json::Value::String(format!("{} {}", notice, s));
      }
    }
    serde_json::to_string(&req).unwrap_or_else(|_| text.to_string())
  } else {
    text.to_string()
  }
}

async fn record_stats_async(
  store: Option<Arc<dx_route_storage::Store>>,
  original: &str,
  compressed: &str,
  mode: &str,
) {
  let store = match store {
    Some(s) => s,
    None => return,
  };
  let original = original.to_string();
  let compressed = compressed.to_string();
  let mode = mode.to_string();

  tokio::task::spawn_blocking(move || {
    let original_tokens = dx_route_core::estimate_tokens(&original).unwrap_or(0) as u32;
    let compressed_tokens = dx_route_core::estimate_tokens(&compressed).unwrap_or(0) as u32;
    if let Err(e) = store.record_stat(&dx_route_storage::models::CompressionStat {
      id: None,
      request_id: uuid::Uuid::new_v4().to_string(),
      original_tokens: original_tokens as i32,
      compressed_tokens: compressed_tokens as i32,
      savings_pct: if original_tokens > 0 {
        (original_tokens - compressed_tokens) as f64 / original_tokens as f64 * 100.0
      } else {
        0.0
      },
      engine: mode.clone(),
      mode,
      duration_ms: 0.0,
      created_at: None,
    }) {
      warn!("failed to record stat: {}", e);
    }
  })
  .await
  .ok();
}
