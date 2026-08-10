#![deny(unsafe_code)]

use clap::{Parser, Subcommand};
use std::io::Read;

#[derive(Parser)]
#[command(name = "dx", about = "dx-route token saver CLI", version, styles = clap::builder::Styles::default())]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output format: text or json
    #[arg(short = 'f', long, default_value = "text", global = true)]
    format: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Full compression pipeline (auto-select strategy)
    Compress,
    /// Lite — whitespace, ANSI, comments
    Lite { intensity: Option<String> },
    /// Caveman — rule-based prose condensation
    Caveman { intensity: Option<String> },
    /// RTK — command-aware output compression
    Rtk { command: Option<String> },
    /// Ultra — token-scoring heuristic
    Ultra { intensity: Option<String> },
    /// Aggressive — 3-stage pipeline
    Aggressive { intensity: Option<String> },
    /// Headroom — JSON array compaction
    Headroom,
    /// Dedup — remove duplicate lines
    Dedup { session: Option<String> },
    /// Stacked — run multiple engines sequentially
    Stacked { steps: Option<String> },
    /// Preview — show compression stats + diff
    Preview { mode: Option<String> },
    /// Stats — query compression statistics
    Stats { db: Option<String> },
}

fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::WARN.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    match cli.command {
        Commands::Compress => output(&run_pipeline(&input)?, &cli.format),
        Commands::Lite { intensity } => {
            let r = dx_route_lite::compress(&input, &intensity.unwrap_or_else(|| "full".into()))?;
            output(&r.text, &cli.format);
        }
        Commands::Caveman { intensity } => {
            let r =
                dx_route_caveman::compress(&input, &intensity.unwrap_or_else(|| "full".into()))?;
            output(&r.text, &cli.format);
        }
        Commands::Rtk { command } => {
            let r = dx_route_rtk::compress(&input, command.as_deref())?;
            output(&r.text, &cli.format);
        }
        Commands::Ultra { intensity } => {
            let r = dx_route_ultra::compress(&input, &intensity.unwrap_or_else(|| "full".into()))?;
            output(&r.text, &cli.format);
        }
        Commands::Aggressive { intensity } => {
            let r =
                dx_route_aggressive::compress(&input, &intensity.unwrap_or_else(|| "full".into()))?;
            output(&r.text, &cli.format);
        }
        Commands::Headroom => {
            let r = dx_route_headroom::compress(&input, "full")?;
            output(&r.text, &cli.format);
        }
        Commands::Dedup { session } => {
            let mut cache = dx_route_dedup::CrossTurnCache::new();
            let r = cache.compress(&session.unwrap_or_else(|| "default".into()), &input);
            output(&r.text, &cli.format);
        }
        Commands::Stacked { steps } => {
            let result = run_stacked(&input, &steps.unwrap_or_else(|| "rtk,caveman,ultra".into()))?;
            output(&result, &cli.format);
        }
        Commands::Preview { mode } => {
            let mode = mode.as_deref().unwrap_or("lite");
            let before = input.len();
            let compressed = run_mode(&input, mode)?;
            let after = compressed.len();
            let savings = if before > 0 {
                (before - after) as f64 / before as f64 * 100.0
            } else {
                0.0
            };
            eprintln!(
                "mode={} original={}B compressed={}B savings={:.1}%",
                mode, before, after, savings
            );
            output(&compressed, &cli.format);
        }
        Commands::Stats { db } => {
            let db_path = db.unwrap_or_else(|| "~/.dx-route/stats.db".into());
            let path = shellexpand::tilde(&db_path);
            match dx_route_storage::Store::open(&path) {
                Ok(store) => {
                    let s = store.get_dashboard_stats()?;
                    println!(
                        "requests={} tokens_saved={} avg_savings={:.1}%",
                        s.total_requests, s.total_tokens_saved, s.avg_savings_pct
                    );
                    for e in &s.engine_breakdown {
                        println!(
                            "  {:20} {} runs, {} tokens saved",
                            e.engine, e.count, e.tokens_saved
                        );
                    }
                }
                Err(e) => eprintln!("error: {}", e),
            }
        }
    }

    Ok(())
}

fn run_pipeline(input: &str) -> Result<String, dx_route_core::CoreError> {
    let mut pipeline = dx_route_core::CompressionPipeline::new(dx_route_core::Config::default());
    pipeline
        .register(LiteEngine)
        .register(CavemanEngine)
        .register(RtkEngine)
        .register(UltraEngine)
        .register(AggressiveEngine)
        .register(HeadroomEngine);

    let ctx = dx_route_core::RequestContext {
        header_override: None,
        combo_id: "default".into(),
        estimated_tokens: dx_route_core::estimate_tokens(input)? as u32,
        body: input.to_string(),
    };
    Ok(pipeline.compress(&ctx)?.text)
}

#[derive(Debug)]
struct LiteEngine;
#[derive(Debug)]
struct CavemanEngine;
#[derive(Debug)]
struct RtkEngine;
#[derive(Debug)]
struct UltraEngine;
#[derive(Debug)]
struct AggressiveEngine;
#[derive(Debug)]
struct HeadroomEngine;

fn wrap<T: std::fmt::Display>(e: T, name: &'static str) -> dx_route_core::CoreError {
    dx_route_core::CoreError::EngineFailed(name.into(), e.to_string().into())
}

impl dx_route_core::Engine for LiteEngine {
    fn name(&self) -> &'static str {
        "lite"
    }
    fn apply(
        &self,
        body: &str,
        intensity: &str,
    ) -> dx_route_core::CoreResult<dx_route_core::EngineOutput> {
        dx_route_lite::compress(body, intensity)
            .map(|r| dx_route_core::EngineOutput::new(r.text))
            .map_err(|e| wrap(e, "lite"))
    }
}
impl dx_route_core::Engine for CavemanEngine {
    fn name(&self) -> &'static str {
        "caveman"
    }
    fn apply(
        &self,
        body: &str,
        intensity: &str,
    ) -> dx_route_core::CoreResult<dx_route_core::EngineOutput> {
        dx_route_caveman::compress(body, intensity)
            .map(|r| dx_route_core::EngineOutput::new(r.text))
            .map_err(|e| wrap(e, "caveman"))
    }
}
impl dx_route_core::Engine for RtkEngine {
    fn name(&self) -> &'static str {
        "rtk"
    }
    fn apply(
        &self,
        body: &str,
        _intensity: &str,
    ) -> dx_route_core::CoreResult<dx_route_core::EngineOutput> {
        dx_route_rtk::compress(body, None)
            .map(|r| dx_route_core::EngineOutput::new(r.text))
            .map_err(|e| wrap(e, "rtk"))
    }
}
impl dx_route_core::Engine for UltraEngine {
    fn name(&self) -> &'static str {
        "ultra"
    }
    fn apply(
        &self,
        body: &str,
        intensity: &str,
    ) -> dx_route_core::CoreResult<dx_route_core::EngineOutput> {
        dx_route_ultra::compress(body, intensity)
            .map(|r| dx_route_core::EngineOutput::new(r.text))
            .map_err(|e| wrap(e, "ultra"))
    }
}
impl dx_route_core::Engine for AggressiveEngine {
    fn name(&self) -> &'static str {
        "aggressive"
    }
    fn apply(
        &self,
        body: &str,
        intensity: &str,
    ) -> dx_route_core::CoreResult<dx_route_core::EngineOutput> {
        dx_route_aggressive::compress(body, intensity)
            .map(|r| dx_route_core::EngineOutput::new(r.text))
            .map_err(|e| wrap(e, "aggressive"))
    }
}
impl dx_route_core::Engine for HeadroomEngine {
    fn name(&self) -> &'static str {
        "headroom"
    }
    fn apply(
        &self,
        body: &str,
        _intensity: &str,
    ) -> dx_route_core::CoreResult<dx_route_core::EngineOutput> {
        dx_route_headroom::compress(body, "full")
            .map(|r| dx_route_core::EngineOutput::new(r.text))
            .map_err(|e| wrap(e, "headroom"))
    }
}

fn run_stacked(input: &str, steps: &str) -> Result<String, dx_route_core::CoreError> {
    let mut current = input.to_string();
    for step in steps.split(',') {
        current = run_mode(&current, step.trim())?;
    }
    Ok(current)
}

fn run_mode(input: &str, mode: &str) -> Result<String, dx_route_core::CoreError> {
    match mode {
        "lite" => dx_route_lite::compress(input, "full")
            .map(|r| r.text)
            .map_err(|e| dx_route_core::CoreError::EngineFailed("lite".into(), e.into())),
        "caveman" => dx_route_caveman::compress(input, "full")
            .map(|r| r.text)
            .map_err(|e| dx_route_core::CoreError::EngineFailed("caveman".into(), e.into())),
        "rtk" => dx_route_rtk::compress(input, None)
            .map(|r| r.text)
            .map_err(|e| dx_route_core::CoreError::EngineFailed("rtk".into(), e.into())),
        "ultra" => dx_route_ultra::compress(input, "full")
            .map(|r| r.text)
            .map_err(|e| dx_route_core::CoreError::EngineFailed("ultra".into(), e.into())),
        "aggressive" => dx_route_aggressive::compress(input, "full")
            .map(|r| r.text)
            .map_err(|e| dx_route_core::CoreError::EngineFailed("aggressive".into(), e.into())),
        "headroom" => dx_route_headroom::compress(input, "full")
            .map(|r| r.text)
            .map_err(|e| dx_route_core::CoreError::EngineFailed("headroom".into(), e.into())),
        other => Err(dx_route_core::CoreError::InvalidMode(other.into())),
    }
}

fn output(text: &str, format: &str) {
    match format {
        "json" => {
            let payload = serde_json::json!({"text": text, "length": text.len()});
            println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        }
        _ => print!("{}", text),
    }
}
