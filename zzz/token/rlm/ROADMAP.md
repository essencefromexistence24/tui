# RLM Roadmap

## Current State

Completed in source:
- Library-first crate surface
- Typed document and task API
- OpenAI-compatible provider config
- Streaming and standard execution modes
- Search-oriented Rhai runtime helpers
- Example and doc cleanup

## Next Phase

- Real recursive chunk decomposition for documents larger than a single working pass
- Structured output helpers on top of `FINAL(...)`
- Stronger evidence extraction from executed search snippets
- CI and compile-validated coverage
- Host-specific adapters if DX wants a thinner embedding layer
