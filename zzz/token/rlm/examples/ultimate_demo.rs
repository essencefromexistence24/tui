use rlm::RLM;
use std::fs;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("================================================================================");
    println!("RUST RLM - ULTIMATE DEMONSTRATION");
    println!("================================================================================");
    println!();
    println!("This demo shows how RLM reduces oversized document handling into focused search-and-answer loops.");
    println!();

    dotenvy::dotenv().ok();

    let doc_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("massive_doc.txt");

    println!("Loading document...");
    let context = match fs::read_to_string(&doc_path) {
        Ok(content) => content,
        Err(_) => {
            println!("Could not find massive_doc.txt, generating a synthetic fallback document.");
            println!();

            let mut demo_doc = String::new();
            demo_doc.push_str("# Technology Industry Report 2024\n\n");
            demo_doc.push_str("## AI Market Analysis\n");
            demo_doc.push_str("The global AI market reached $184 billion in 2024, growing at 37.3% annually. ");
            demo_doc.push_str("Major players include OpenAI, Anthropic, Google, and Meta. ");
            demo_doc.push_str("The enterprise AI adoption rate hit 65% in Fortune 500 companies.\n\n");
            demo_doc.push_str("## Space Industry Updates\n");
            demo_doc.push_str("SpaceX completed 96 successful launches in 2024, setting a new record. ");
            demo_doc.push_str("Starship achieved its first orbital flight in March 2024. ");
            demo_doc.push_str("The commercial space market grew to $469 billion.\n\n");
            demo_doc.push_str("## Remote Work Statistics\n");
            demo_doc.push_str("Remote work adoption stabilized at 42% for tech workers in 2024. ");
            demo_doc.push_str("Hybrid models became the norm, with 3 days in office being most common. ");
            demo_doc.push_str("Productivity metrics showed a 12% increase compared to 2023.\n\n");

            for i in 1..20 {
                demo_doc.push_str(&format!("## Additional Section {}\n", i));
                demo_doc.push_str(&format!("This section contains detailed information about topic {}. ", i));
                demo_doc.push_str("It includes market analysis, trends, statistics, and forecasts. ");
                demo_doc.push_str("The data is sourced from industry reports and expert analysis. ");
                demo_doc.push_str("Key metrics show significant growth across all measured parameters.\n\n");
            }

            demo_doc
        }
    };

    let doc_chars = context.len();
    let estimated_tokens = doc_chars / 4;

    println!("Document loaded:");
    println!("  Size: {} characters", doc_chars);
    println!("  Estimated tokens: ~{}", estimated_tokens);
    println!();

    let rlm = RLM::from_env_groq("llama-3.3-70b-versatile")?.with_max_iterations(20);

    let query = "What is the AI market size in 2024? Use fast_find to search for 'AI market'.";
    println!("Query: {}", query);
    println!();

    let start = Instant::now();
    match rlm.complete(query, &context).await {
        Ok((answer, stats)) => {
            let elapsed = start.elapsed();
            println!("Answer: {}", answer);
            println!();
            println!("Performance:");
            println!("  Time: {:.2}s", elapsed.as_secs_f64());
            println!("  LLM calls: {}", stats.llm_calls);
            println!("  Iterations: {}", stats.iterations);
            println!("  Cache hit rate: {:.1}%", stats.cache_hit_rate());
            println!("  Cost savings: {:.1}%", stats.cost_savings());
        }
        Err(error) => {
            println!("Error: {}", error);
        }
    }

    Ok(())
}
