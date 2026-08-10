use rlm::{RLMChunkingConfig, RLMDocument, RLMProfile, RLM};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    let document = RLMDocument::from_file(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_document.txt"),
    )?;

    let rlm = RLM::from_env_groq("meta-llama/llama-4-scout-17b-16e-instruct")?
        .with_fast_model("meta-llama/llama-3.3-70b-versatile".to_string())
        .with_profile(RLMProfile::Balanced);

    let recursive = rlm
        .build_agent_context_recursive(
            "Prepare a compact implementation context for an autonomous coding agent.",
            document.clone(),
            RLMChunkingConfig::default(),
        )
        .await?;

    println!("Answer:\n{}\n", recursive.response.answer);
    println!("Reduced: {}", recursive.was_reduced);
    println!("Reduction passes: {}", recursive.reduction_passes.len());
    println!(
        "Reduction ratio: {:.1}%",
        recursive.reduction_ratio(document.content.len())
    );
    println!(
        "Average iteration latency: {:.2} ms",
        recursive.aggregate_stats.average_iteration_latency_ms()
    );

    Ok(())
}
