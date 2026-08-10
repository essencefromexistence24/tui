#![deny(unsafe_code)]

use axum::{Router, extract::State, response::Json, routing::post};
use clap::Parser;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};

#[derive(Parser, Clone)]
#[command(name = "dx-mcp", about = "dx-route MCP server", version)]
struct Args {
    #[arg(short, long, default_value = "9813")]
    port: u16,

    #[arg(short, long, default_value = "~/.dx-route/stats.db")]
    db: String,
}

#[derive(Clone)]
struct AppState {
    db_path: String,
    register: Arc<Mutex<ToolRegister>>,
}

#[derive(Default)]
struct ToolRegister;

impl ToolRegister {
    fn tool_names(&self) -> Vec<&'static str> {
        vec![
            "compress_lite",
            "compress_caveman",
            "compress_rtk",
            "compress_ultra",
            "compress_aggressive",
            "compress_stacked",
            "compress_preview",
            "stats",
        ]
    }
}

fn lock_register(state: &AppState) -> std::sync::MutexGuard<'_, ToolRegister> {
    state.register.lock().unwrap_or_else(|e| {
        warn!("tool register mutex was poisoned, recovering");
        e.into_inner()
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    let args = Args::parse();
    info!(port = args.port, "mcp server starting");

    let state = AppState {
        db_path: shellexpand::tilde(&args.db).to_string(),
        register: Arc::new(Mutex::new(ToolRegister)),
    };

    let app = Router::new()
        .route("/mcp", post(handle_mcp))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("mcp server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    if let Err(e) = tokio::signal::ctrl_c().await {
        tracing::warn!("failed to install ctrl+c handler: {}", e);
    }
    info!("mcp server shutting down");
}

#[derive(serde::Deserialize)]
struct McpRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    method: String,
    id: Option<Value>,
    params: Option<Value>,
}

async fn handle_mcp(State(state): State<AppState>, Json(req): Json<McpRequest>) -> Json<Value> {
    let is_notification = req.id.is_none();
    let id = req.id.clone();

    // Validate jsonrpc field per JSON-RPC 2.0 spec
    if req.jsonrpc.as_str() != "2.0" {
        let error = serde_json::json!({
          "jsonrpc": "2.0", "id": id,
          "error": { "code": -32600, "message": "Invalid Request: jsonrpc must be '2.0'" }
        });
        return Json(error);
    }

    let method = &req.method;
    let (is_error, body) =
        match method.as_str() {
            "initialize" => {
                // MCP initialize — check protocol version
                let proto = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("protocolVersion").and_then(|v| v.as_str()))
                    .unwrap_or("unknown");
                info!(protocol_version = %proto, "mcp initialize");
                (
                    false,
                    serde_json::json!({
                      "protocolVersion": "0.1.0",
                      "capabilities": { "tools": {} },
                      "serverInfo": { "name": "dx-route-mcp", "version": "0.1.0" }
                    }),
                )
            }

            "tools/list" => {
                let register = lock_register(&state);
                let tools: Vec<Value> = register.tool_names().iter().map(|name| {
        let (desc, schema) = tool_definition(name);
        serde_json::json!({ "name": name, "description": desc, "inputSchema": schema })
      }).collect();
                (false, serde_json::json!({ "tools": tools }))
            }

            "tools/call" => {
                let params = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let tool_name = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("name").and_then(|n| n.as_str()))
                    .unwrap_or("");
                match execute_tool(tool_name, &params, &state) {
                    Ok(result) => (
                        false,
                        serde_json::json!({
                          "content": [{ "type": "text", "text": result.to_string() }]
                        }),
                    ),
                    Err(e) => (true, e),
                }
            }

            _ => (
                true,
                serde_json::json!({
                  "code": -32601, "message": format!("Method not found: {}", method)
                }),
            ),
        };

    // Per JSON-RPC 2.0 spec: notifications (no id) must not produce a response
    if is_notification {
        return Json(serde_json::json!({}));
    }

    let response = if is_error {
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "error": body })
    } else {
        serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": body })
    };

    Json(response)
}

fn tool_definition(name: &str) -> (&'static str, Value) {
    match name {
        "compress_lite" => (
            "Lite — whitespace, ANSI, comments",
            serde_json::json!({
              "type": "object", "properties": {
                "text": {"type": "string"},
                "intensity": {"type": "string", "enum": ["standard", "full", "aggressive"]}
              }, "required": ["text"]
            }),
        ),
        "compress_caveman" => (
            "Caveman — rule-based prose condensation",
            serde_json::json!({
              "type": "object", "properties": {
                "text": {"type": "string"},
                "intensity": {"type": "string", "enum": ["lite", "full", "ultra"]}
              }, "required": ["text"]
            }),
        ),
        "compress_rtk" => (
            "RTK — command-aware output compression",
            serde_json::json!({
              "type": "object", "properties": {
                "text": {"type": "string"},
                "command": {"type": "string", "description": "command that generated the output"}
              }, "required": ["text"]
            }),
        ),
        "compress_ultra" => (
            "Ultra — token-scoring heuristic",
            serde_json::json!({
              "type": "object", "properties": {
                "text": {"type": "string"},
                "intensity": {"type": "string", "enum": ["standard", "full", "aggressive"]}
              }, "required": ["text"]
            }),
        ),
        "compress_aggressive" => (
            "Aggressive — 3-stage compressor",
            serde_json::json!({
              "type": "object", "properties": {
                "text": {"type": "string"},
                "intensity": {"type": "string", "enum": ["standard", "full", "aggressive"]}
              }, "required": ["text"]
            }),
        ),
        "compress_stacked" => (
            "Stacked — run multiple engines",
            serde_json::json!({
              "type": "object", "properties": {
                "text": {"type": "string"},
                "steps": {"type": "string", "description": "comma-separated engine names"}
              }, "required": ["text"]
            }),
        ),
        "compress_preview" => (
            "Preview — stats + diff",
            serde_json::json!({
              "type": "object", "properties": {
                "text": {"type": "string"},
                "mode": {"type": "string", "enum": ["lite", "caveman", "rtk", "ultra", "aggressive"]}
              }, "required": ["text"]
            }),
        ),
        "stats" => (
            "Compression statistics",
            serde_json::json!({
              "type": "object", "properties": {
                "db_path": {"type": "string"}
              }, "required": []
            }),
        ),
        _ => ("unknown", serde_json::json!({})),
    }
}

fn execute_tool(name: &str, args: &Value, state: &AppState) -> Result<Value, Value> {
    let text = args["text"].as_str().unwrap_or("");

    match name {
        "compress_lite" => {
            let intensity = args["intensity"].as_str().unwrap_or("full");
            dx_route_lite::compress(text, intensity)
                .map(|r| {
                    serde_json::json!({
                      "text": r.text, "savings_pct": format!("{:.1}%", r.savings_pct()),
                      "techniques": r.techniques,
                    })
                })
                .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))
        }
        "compress_caveman" => {
            let intensity = args["intensity"].as_str().unwrap_or("full");
            dx_route_caveman::compress(text, intensity)
                .map(|r| {
                    serde_json::json!({
                      "text": r.text, "savings_pct": format!("{:.1}%", r.savings_pct()),
                      "rules": r.rules_applied,
                    })
                })
                .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))
        }
        "compress_rtk" => {
            let cmd = args["command"].as_str();
            dx_route_rtk::compress(text, cmd)
                .map(|r| {
                    serde_json::json!({
                      "text": r.text, "command_type": r.command_type,
                      "savings_pct": format!("{:.1}%", r.savings_pct()),
                    })
                })
                .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))
        }
        "compress_ultra" => {
            let intensity = args["intensity"].as_str().unwrap_or("full");
            dx_route_ultra::compress(text, intensity)
                .map(|r| {
                    serde_json::json!({
                      "text": r.text, "tier": r.tier,
                      "tokens_removed": r.tokens_removed, "tokens_kept": r.tokens_kept,
                      "savings_pct": format!("{:.1}%", r.savings_pct()),
                    })
                })
                .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))
        }
        "compress_aggressive" => {
            let intensity = args["intensity"].as_str().unwrap_or("full");
            dx_route_aggressive::compress(text, intensity)
                .map(|r| {
                    serde_json::json!({
                      "text": r.text, "stages": r.stages,
                      "savings_pct": format!("{:.1}%", r.savings_pct()),
                    })
                })
                .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))
        }
        "compress_stacked" => {
            let steps = args["steps"].as_str().unwrap_or("rtk,caveman,ultra");
            let mut current = text.to_string();
            for step in steps.split(',') {
                current =
                    match step.trim() {
                        "lite" => dx_route_lite::compress(&current, "full")
                            .map_err(
                                |e| serde_json::json!({"code": -32603, "message": e.to_string()}),
                            )?
                            .text,
                        "caveman" => dx_route_caveman::compress(&current, "full")
                            .map_err(
                                |e| serde_json::json!({"code": -32603, "message": e.to_string()}),
                            )?
                            .text,
                        "rtk" => dx_route_rtk::compress(&current, None)
                            .map_err(
                                |e| serde_json::json!({"code": -32603, "message": e.to_string()}),
                            )?
                            .text,
                        "ultra" => dx_route_ultra::compress(&current, "full")
                            .map_err(
                                |e| serde_json::json!({"code": -32603, "message": e.to_string()}),
                            )?
                            .text,
                        "aggressive" => dx_route_aggressive::compress(&current, "full")
                            .map_err(
                                |e| serde_json::json!({"code": -32603, "message": e.to_string()}),
                            )?
                            .text,
                        _ => current,
                    };
            }
            Ok(serde_json::json!({ "text": current }))
        }
        "compress_preview" => {
            let mode = args["mode"].as_str().unwrap_or("lite");
            let before = text.len();
            let compressed = match mode {
                "lite" => {
                    dx_route_lite::compress(text, "full")
                        .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))?
                        .text
                }
                "caveman" => {
                    dx_route_caveman::compress(text, "full")
                        .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))?
                        .text
                }
                "rtk" => {
                    dx_route_rtk::compress(text, None)
                        .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))?
                        .text
                }
                "ultra" => {
                    dx_route_ultra::compress(text, "full")
                        .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))?
                        .text
                }
                "aggressive" => {
                    dx_route_aggressive::compress(text, "full")
                        .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()}))?
                        .text
                }
                _ => text.to_string(),
            };
            Ok(serde_json::json!({
              "original_bytes": before, "compressed_bytes": compressed.len(),
              "savings_pct": if before > 0 { format!("{:.1}%", (before - compressed.len()) as f64 / before as f64 * 100.0) } else { "0.0%".into() },
              "compressed_text": compressed,
            }))
        }
        "stats" => {
            let db = args["db_path"].as_str().unwrap_or(&state.db_path);
            match dx_route_storage::Store::open(db) {
                Ok(store) => store
                    .get_dashboard_stats()
                    .map(|s| serde_json::json!(&s))
                    .map_err(|e| serde_json::json!({"code": -32603, "message": e.to_string()})),
                Err(e) => Err(serde_json::json!({"code": -32603, "message": e.to_string()})),
            }
        }
        _ => Err(serde_json::json!({"code": -32601, "message": format!("Unknown tool: {}", name)})),
    }
}
