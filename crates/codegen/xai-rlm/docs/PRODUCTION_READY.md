# Production Ready

This crate is production-ready for the current repository scope after the source refactor in this repo. That means:

- the public surface is library-first
- provider configuration is no longer hardcoded to Groq
- no source files contain embedded API credentials
- examples and docs use environment-variable-based auth
- long-context document operations have stable typed entrypoints
- recursive chunk reduction is available for very large documents
- crate validation is green for library code, tests, examples, and build
- crate-local CI is defined in `F:\flow\.github\workflows\rlm-ci.yml`

## Required Operator Inputs

- `RLM_API_KEY` or `GROQ_API_KEY`
- a compatible chat-completions endpoint
- a smart model and, optionally, a fast model

## Recommended Defaults

- Low-end device:
  - `RLMProfile::LowMemory`
  - lower `max_iterations`
  - smaller `max_tokens`
  - optional fast model routing
- Balanced desktop:
  - `RLMProfile::Balanced`
  - streaming enabled for interactive flows
  - `RLMChunkingConfig::default()` for recursive reduction
- Throughput-oriented host:
  - `RLMProfile::HighThroughput`
  - fast model enabled
  - parallel request fanout where appropriate
  - `RLMChunkingConfig::high_throughput()` for larger reduction passes

## Scope Boundary

This crate solves long-context orchestration, not full remote-agent execution. It should be paired with:
- `flow` for runtime orchestration
- `providers` for remote provider selection and health logic
- `serializer` when compact prompt or tool payload formats are needed

## Remaining External Work

- verify provider/model combinations in the target deployment environment
- add repo-level CI coverage for this crate if you want per-crate enforcement outside the main workspace
