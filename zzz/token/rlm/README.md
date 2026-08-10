# rlm

`rlm` is the long-context runtime layer for DX. It is designed to answer questions, summarize oversized files, and build agent-ready context without shoving the entire document into a single prompt.

The crate is library-first. It is meant to be embedded into:
- DX desktop and browser-adjacent prep flows
- Zed forks
- Codex forks
- ZeroClaw forks
- other Rust hosts that need recursive long-context handling

## What It Provides

- Recursive document querying through a constrained Rhai REPL
- Fast string search helpers for large text bodies
- Standard and streaming execution modes
- Provider abstraction for Groq or other OpenAI-compatible chat endpoints
- Library-friendly `RLMDocument`, `RLMRequest`, and `RLMResponse` types
- Agent-context preparation and summarization entrypoints
- Cache-aware request routing with optional fast-model selection
- Multi-pass chunk reduction for oversized documents

## Stable Embed Surface

Main exports:
- `RLM`
- `RLMBuilder`
- `RLMChunkingConfig`
- `RLMRecursiveResponse`
- `RLMDocument`
- `RLMRequest`
- `RLMResponse`
- `RLMProfile`
- `RLMRunMode`
- `RLMTaskKind`
- `LLMProviderConfig`

## Quick Start

Set one of these environment variables:
- `RLM_API_KEY`
- `GROQ_API_KEY`

Basic usage:

```rust,no_run
use rlm::{RLM, RLMDocument};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rlm = RLM::from_env_groq("meta-llama/llama-4-scout-17b-16e-instruct")?
        .with_fast_model("meta-llama/llama-3.3-70b-versatile".to_string());

    let document = RLMDocument::from_file("README.md")?;
    let response = rlm
        .complete_document("What are the main integration points?", document)
        .await?;

    println!("{}", response.answer);
    Ok(())
}
```

OpenAI-compatible endpoint:

```rust,no_run
use rlm::{LLMProviderConfig, RLM};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = LLMProviderConfig::openai_compatible(
        std::env::var("RLM_API_KEY")?,
        "http://127.0.0.1:1234/v1/chat/completions",
    )
    .with_provider_label("local-openai-compatible");

    let rlm = RLM::from_provider(provider, "qwen/qwen3-0.6b");
    let (_answer, _stats) = rlm.complete("What does this file contain?", "sample text").await?;
    Ok(())
}
```

## High-Level Operations

- `complete_document(...)`
- `complete_document_recursive(...)`
- `summarize_document(...)`
- `summarize_document_recursive(...)`
- `build_agent_context(...)`
- `build_agent_context_recursive(...)`
- `complete_file(...)`
- `summarize_file(...)`

These are the preferred entrypoints for editor and agent integrations.

Use the recursive variants when the source document is well beyond a normal prompt-sized working set.

```rust,no_run
use rlm::{RLM, RLMChunkingConfig, RLMDocument};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rlm = RLM::from_env_groq("meta-llama/llama-4-scout-17b-16e-instruct")?;
    let document = RLMDocument::from_file("massive_doc.txt")?;
    let recursive = rlm
        .summarize_document_recursive(document, RLMChunkingConfig::default())
        .await?;

    println!("{}", recursive.response.answer);
    println!("Reduction passes: {}", recursive.reduction_passes.len());
    Ok(())
}
```

## Profiles

- `RLMProfile::LowMemory`: better for weak machines and conservative scans
- `RLMProfile::Balanced`: default
- `RLMProfile::HighThroughput`: favors aggressive exploration

## Operator Defaults

- `.env.example` is included in the crate root for local setup.
- `recommended_chunking_config()` maps the current `RLMProfile` to a sane recursive reduction strategy.
- `complete_document_auto(...)`, `summarize_document_auto(...)`, and `build_agent_context_auto(...)` use that profile-aware reduction path automatically.

## Examples

The examples folder contains smoke and demo binaries for:
- benchmarks
- fast search
- caching
- streaming
- multi-model routing
- phase demos

All examples now read credentials from environment variables only and resolve demo files from the local crate root.
The crate also includes `recursive_reduce.rs` as the canonical oversized-document demo.

## Production Notes

- No credentials are embedded in source files.
- The provider layer is not Groq-only anymore; any OpenAI-compatible endpoint can be used.
- The crate is optimized for integration into larger hosts, not for standalone CLI ownership.

See:
- [docs/INTEGRATIONS.md](docs/INTEGRATIONS.md)
- [docs/PRODUCTION_READY.md](docs/PRODUCTION_READY.md)
- [QUICK_START.txt](QUICK_START.txt)
