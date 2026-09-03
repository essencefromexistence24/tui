# Quick Start

## Prerequisites

- Rust 1.85+ (edition 2024)
- Terminal with Unicode + true color support

## Build & Run

```powershell
# Debug
cargo run -p dx-tui -j12

# Release (optimized, stripped)
cargo run -p dx-tui --release -j12

# With local LLM backend
cargo run -p dx-tui --features llm -j12
```

## Modes

- `dx` — Start new chat session
- `dx continue <id>` — Resume saved session

## Key Bindings

- `Ctrl+B` — Branch picker
- `Ctrl+P` — Command palette
- `Esc` — Close modal / return to chat
- `Tab` — Switch between Chat / FilePicker / Editor modes
