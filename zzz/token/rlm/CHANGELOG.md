# Changelog

## 0.1.0

- Refactored `rlm` into a library-first long-context runtime with `RLMDocument`, `RLMRequest`, and `RLMResponse`.
- Added `RLMBuilder`, `RLMProfile`, and OpenAI-compatible provider configuration.
- Replaced the Groq-only client surface with `LLMProviderConfig`.
- Fixed broken source structure in `src/llm.rs` and `src/rlm.rs`.
- Added streaming, cache, and provider configuration cleanup.
- Added recursive chunk reduction with `RLMChunkingConfig`, `RLMChunk`, `RLMReductionPass`, and `RLMRecursiveResponse`.
- Added recursive entrypoints for oversized files and documents.
- Added profile-aware auto-reduction helpers, `.env.example`, and a dedicated `recursive_reduce` example.
- Added dedicated `rlm` CI workflow.
- Validated the crate with `cargo check`, `cargo test`, `cargo check --examples`, and `cargo build`.
- Removed hardcoded credentials from examples, shell scripts, Python demos, and text docs.
- Added a real crate README plus production and integration docs.
