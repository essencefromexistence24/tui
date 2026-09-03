# DX TUI

DX TUI is a Ratatui-based terminal UI for the DX CLI ecosystem. It combines an embedded file
browser, an AI chat interface, animation system, and multi-provider LLM orchestration in one
terminal experience.

## Features

- **AI Chat Interface** — Full-featured chat with streaming responses, thinking accordion,
  tool call rendering, and Markdown display
- **File Browser** — Embedded async file browser with multi-tab support, image preview (Kitty,
  iTerm2, Sixel), and VFS layer
- **Multi-Provider LLM** — Zen API, OpenCode-compatible providers, Ollama/local models via
  `dx-flow`, and omniroute for provider fallback
- **Animation System** — Matrix rain, train, confetti, Nyan cat, Game of Life, starfield,
  rain, fire, plasma, and more, with intro/outro transitions
- **Permission Hub** — Confirm-before-execute for tool calls (shell, write, MCP, LSP)
- **Goal Mode** — Autonomous multi-step agent loop with iteration budgets and time limits
- **Plugin System** — Lua-based plugins with tool definitions
- **MCP & LSP** — Model Context Protocol tools and Language Server Protocol integration
- **Session Persistence** — Dual JSON + SQLite persistence with FTS5 full-text search
- **Session Continuation** — `/continue <id>` or `dx continue <id>` to resume sessions
- **Channels** — Share transcripts to external channels (Slack, Discord, etc.)
- **Sound** — Audio cues for events (submit, error, train whistle, etc.)
- **Voice** — Voice input support (whisper/local STT)

## Prerequisites

- Rust 1.85+ (edition 2024)
- A GPU-less build takes ~2 minutes; release build with LTO takes 10-20 min

## Quick Start

```powershell
# Basic launch
cargo run -p dx-tui -j12

# With local LLM backend (requires CMake + GGUF model)
cargo run -p dx-tui --features llm -j12

# With a specific model path
$env:DX_TUI_MODEL_PATH = "F:\models\qwen-0.5b.gguf"
cargo run -p dx-tui --features llm -j12
```

See [QUICKSTART.md](QUICKSTART.md) for detailed setup instructions.

## Configuration

- `~/.config/dx/config.toml` — Main configuration file
- `~/.config/dx/sessions/` — Session storage (JSON + SQLite)
- `~/.config/dx/plugins/` — Lua plugin directory
- Environment variables: `DX_TUI_CONTINUE_SESSION`, `DX_TUI_MODEL_PATH`, `DX_AGENT_WORKSPACE`

See [CONFIGURATION.md](CONFIGURATION.md) for all available options.

## Commands

| Command | Description |
|---------|-------------|
| `dx` | Start new session |
| `dx continue <id>` | Resume a saved session |
| `/new` | New session |
| `/sessions` | List and switch sessions |
| `/rename <name>` | Rename current session |
| `/export` | Export session as Markdown |
| `/model <name>` | Switch LLM model |
| `/mode <mode>` | Set agent mode (ask/write/plan/agent/goal) |
| `/skills` | List and manage skills |
| `/channels` | Configure sharing channels |

## Verification

Always use **`-j12`** for all cargo commands on this project:

```powershell
cargo fmt --check
cargo check -p dx-tui -j12
cargo clippy -p dx-tui -j12 -- -D warnings
cargo test -p dx-tui -j12
cargo build -p dx-tui --release -j12
```

## Project Structure

```
src/
├── main.rs              # Entry point
├── lib.rs               # Crate root, CLI arg parsing, global allocator
├── state.rs             # ChatState — the main application state (153 fields)
├── dispatcher.rs        # Event dispatch (keyboard, mouse, events)
├── chat_render.rs       # Main chat rendering (2,219 lines)
├── input.rs             # Multi-line input with suggestions (1,449 lines)
├── slash_commands.rs    # /command system (1,924 lines)
├── agent_loop.rs        # Multi-step LLM tool loop
├── agent_backend.rs     # Zen/OpenCode API agent backend
├── flow_backend.rs      # Local dx-flow backend
├── session_db.rs        # SQLite persistence layer
├── session_store.rs     # JSON persistence layer
├── animations.rs        # 15+ animation renderers
├── plugin_system.rs     # Lua plugin system
├── mcp.rs               # Model Context Protocol client
├── lsp.rs               # Language Server Protocol client
├── omniroute.rs         # Multi-provider routing
├── permission_hub.rs    # Tool execution permission system
├── skills.rs            # Skill management + curator
├── components.rs        # Message components, Markdown renderer
├── theme.rs             # Color theme system
└── file_browser/        # 26 crate workspace — embedded async file browser
```

## Architecture Notes

- **Async runtime**: Tokio (full features) — all I/O is async
- **UI framework**: Ratatui 0.30 with tachyonfx 0.25 for animations
- **Session persistence**: Dual-path — JSON (`session_store.rs`) for snapshots, SQLite
  (`session_db.rs`) with FTS5 for full-text search
- **Message flow**: User input → `state.rs::add_user_message()` → `tokio::spawn(agent)`
  → streaming response → `drain_agent_response_chunks()` → `on_assistant_turn_finished()`
  → persistence
- **File browser**: Embedded async file-browser engine with VFS, plugins, and multi-format
  previews — see `src/file_browser/` for the full workspace

## Documentation

- [QUICKSTART.md](QUICKSTART.md) — Local build and launch commands
- [CODEX_INTEGRATION.md](CODEX_INTEGRATION.md) — Codex CLI + DX TUI workflow
- [CONFIGURATION.md](CONFIGURATION.md) — All configuration options
- [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) — Source layout
- [CONTRIBUTING.md](CONTRIBUTING.md) — Contribution and verification expectations
- [ARCHITECTURE.md](ARCHITECTURE.md) — Module boundary documentation
- [PROVIDERS.md](PROVIDERS.md) — LLM provider setup
- [DX.md](DX.md) — DX-specific development notes

## Bug Reports & Feature Requests

Report issues at: https://github.com/anomalyco/dx-tui/issues

Before filing a bug, run with `RUST_LOG=debug` and include the output.

## License

MIT. See [LICENSE](LICENSE) for details.
