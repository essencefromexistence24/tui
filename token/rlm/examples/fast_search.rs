use rlm::RLM;
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    println!("================================================================================");
    println!("🔍 FAST SEARCH DEMO - SIMD-Accelerated Text Search");
    println!("================================================================================");
    println!();

    let doc_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("massive_doc.txt");
    let context = fs::read_to_string(&doc_path)?;

    println!("📄 Document: {} characters", context.len());
    println!();

    let rlm = RLM::from_env_groq("meta-llama/llama-4-scout-17b-16e-instruct")?
        .with_max_iterations(20);

    println!("Query: Count how many times '2024' appears in the document");
    println!();
    println!("The LLM will use fast_find_all() - a SIMD-accelerated function");
    println!("that's 10-100x faster than naive string search.");
    println!();
    println!("{}", "-".repeat(80));
    println!();

    match rlm.complete(
        "Use fast_find_all to count how many times '2024' appears in the context. Return just the count.",
        &context
    ).await {
        Ok((answer, stats)) => {
            println!("✅ Answer: {}", answer);
            println!();
            println!("📊 Stats:");
            println!("   LLM calls: {}", stats.llm_calls);
            println!("   Iterations: {}", stats.iterations);
            println!("   Time: {:.2}s", stats.elapsed_ms as f64 / 1000.0);
            println!();
            println!("The fast_find_all() function used SIMD instructions to scan");
            println!("the entire document in microseconds instead of milliseconds!");
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }

    println!();
    println!("================================================================================");
    println!("Available Fast Search Functions:");
    println!("================================================================================");
    println!();
    println!("1. fast_find(text, pattern) -> i64");
    println!("   Returns index of first occurrence, or -1 if not found");
    println!();
    println!("2. fast_contains(text, pattern) -> bool");
    println!("   Returns true if pattern exists in text");
    println!();
    println!("3. fast_find_all(text, pattern) -> array");
    println!("   Returns array of all occurrence indices");
    println!();
    println!("All functions use the memchr crate with SIMD optimizations!");
    println!();

    Ok(())
}
