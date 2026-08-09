use rlm::RLM;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Rust RLM - Simple Test");
    println!();

    dotenvy::dotenv().ok();

    let context = r#"
# Tech Report 2024

## AI Market
The global AI market reached $184 billion in 2024, growing at 37.3% annually.
Major players include OpenAI, Anthropic, Google, and Meta.

## Space Industry
SpaceX completed 96 successful launches in 2024.
Starship achieved its first orbital flight in March 2024.
"#;

    println!("Context: {} characters", context.len());
    println!();

    let rlm = RLM::from_env_groq("llama-3.3-70b-versatile")?.with_max_iterations(10);

    println!("Query: What is the AI market size in 2024?");
    println!();

    match rlm
        .complete(
            "What is the AI market size in 2024? Use fast_find to search for 'AI market'.",
            context,
        )
        .await
    {
        Ok((answer, stats)) => {
            println!("Answer: {}", answer);
            println!();
            println!("Stats:");
            println!("  LLM calls: {}", stats.llm_calls);
            println!("  Iterations: {}", stats.iterations);
            println!("  Cache hit rate: {:.1}%", stats.cache_hit_rate());
        }
        Err(error) => {
            println!("Error: {}", error);
        }
    }

    Ok(())
}
