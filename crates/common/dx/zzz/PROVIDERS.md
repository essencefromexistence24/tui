# LLM Providers

## Default: Zen API

dx-tui uses the Zen API by default for LLM access. Configured via `~/.config/dx/config.toml`.

## Local Models

Use the `llm` feature with a GGUF model:

```powershell
cargo run -p dx-tui --features llm -j12
$env:DX_TUI_MODEL_PATH = "F:\models\qwen-0.5b.gguf"
```

## dx-flow

If the `dx-stack` feature is enabled, dx-tui can use `dx-flow` for local model orchestration with provider fallback ("omniroute").

## OpenCode Providers

dx-tui is compatible with OpenCode's provider ecosystem. See OpenCode docs for setting up provider configurations.
