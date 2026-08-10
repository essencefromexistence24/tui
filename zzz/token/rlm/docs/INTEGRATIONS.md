# Integrations

`rlm` is intended to sit in the prompt-preparation and long-context analysis layer for DX and its forks.

## Zed Fork

Use `RLMDocument::from_file(...)` on active files, project notes, or concatenated buffers, then:
- `complete_document(...)` for question answering
- `summarize_document(...)` for context panels
- `build_agent_context(...)` for agent-side task prep
- switch to the `*_recursive(...)` variants when the merged context is too large for a single efficient pass

## Codex Fork

Use `build_agent_context(...)` to reduce oversized repo context before sending work to a coding agent. Feed the output into the agent panel, background task runner, or review pipeline.
For very large repos or bundled prompt context, prefer `build_agent_context_recursive(...)`.

## ZeroClaw Fork

Use `build_agent_context(...)` and `complete_document(...)` for daemon tasks, channel requests, and operator-side context reduction. Pair it with `dx-serializer` when you need compact prompt or tool payload reuse.

## DX Main Runtime

Preferred flow:
1. Select a source document or merged context bundle.
2. Route through `rlm` when the input is too large or too noisy for direct prompting.
3. Pass the reduced answer or agent context into the selected local or remote model runtime.

## Provider Strategy

- Use `RLM::from_env_groq(...)` for Groq-backed deployments.
- Use `RLM::from_provider(...)` with `LLMProviderConfig::openai_compatible(...)` for local gateways, LM Studio-style endpoints, or any compatible server.

## Recommended Host Boundary

Keep `rlm` behind a thin application adapter. The host should own:
- auth and secret loading
- telemetry
- retries/backoff policies
- persistent caching if needed

The crate should own:
- prompt construction
- search-oriented REPL execution
- recursive long-context loop behavior
