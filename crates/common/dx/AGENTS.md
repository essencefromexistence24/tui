# dx-tui: Terminal UI for Codex CLI

## Project Overview

DX TUI is the Ratatui shell for the DX CLI. It combines the embedded DX file browser with an integrated chat interface for AI-assisted coding workflows.

## Stack

- **Language**: Rust (Edition 2024, requires 1.85+)
- **UI Framework**: Ratatui 0.30.0 with tachyonfx 0.25.0 for animations
- **Async Runtime**: Tokio 1.42 (full features)
- **Scripting**: Lua 5.4 via mlua 0.11.6 (vendored, with async support)
- **Terminal**: Crossterm 0.29.0 with event-stream support
- **File Browser**: Embedded async file-browser engine
- **LLM Integration** (optional): llama-cpp-2, tiktoken-rs for local model support
- **Build System**: Cargo with workspace structure (26 member crates)

## Architecture

### Workspace Structure

```
dx-tui/
├── src/                          # Main TUI application
│   ├── main.rs                   # Entry point
│   ├── chat.rs                   # Chat interface
│   ├── chat_components.rs        # Chat UI components
│   ├── chat_input.rs             # Chat input handling
│   ├── animations.rs             # Animation system
│   ├── dispatcher.rs             # Event dispatcher
│   └── file_browser/             # File browser modules (26 crates)
│       ├── actor/                # Async task actors
│       ├── adapter/              # Image/preview adapters
│       ├── boot/                 # Bootstrap logic
│       ├── config/               # Configuration management
│       ├── core/                 # Core data structures
│       ├── widgets/              # UI widgets
│       └── ...                   # Other specialized modules
├── figlet/                       # FIGlet fonts (114 .dx files)
├── assets/                       # Logo and desktop files
└── scripts/                      # Build and validation scripts
```

### Key Features

1. **Async File Browser**: Full async I/O with non-blocking operations
2. **Animation System**: Matrix rain, train animations, FIGlet rendering
3. **Chat Interface**: AI-powered coding assistant with context awareness
4. **Image Preview**: Built-in support for Kitty, iTerm2, Sixel, and more
5. **Plugin System**: Lua-based plugins for extensibility
6. **Multi-Tab Support**: Cross-directory selection and navigation

## Conventions

### Code Style

- **Use Rust 2024 idioms**: Leverage latest edition features
- **Async-first**: All I/O operations must be async (tokio runtime)
- **Error handling**: Use `anyhow::Result` for application errors, `thiserror` for library errors
- **Naming**:
  - Modules: `snake_case` (e.g., `chat_components.rs`)
  - Types: `PascalCase` (e.g., `ChatState`, `AnimationType`)
  - Functions: `snake_case` (e.g., `render_chat`, `handle_input`)
  - Constants: `SCREAMING_SNAKE_CASE` (e.g., `MAX_HISTORY`)

### File Organization

- **Main application code**: `src/*.rs`
- **File browser modules**: `src/file_browser/{module}/`
- **Each file browser module**: Has its own `Cargo.toml` and is a workspace member
- **Shared types**: Use workspace dependencies defined in root `Cargo.toml`

### Dependencies

- **Always use workspace dependencies**: Reference via `workspace = true` in member crates
- **Feature flags**: 
  - `default = ["vendored-lua"]`
  - `vendored-lua`: Bundles Lua interpreter
  - `llm`: Enables opt-in local LLM support
- **Add dependencies via CLI**: `cargo add <crate>` (updates workspace dependencies)

### Animation System

- **AnimationType enum**: Defines available animations (Matrix, Train, FIGlet variants)
- **Intro/Outro animations**: Configurable per-session in `ChatState`
- **Toast notifications**: Use for user feedback on animation selection
- **Spinner**: Block spinner on right side of chat input (Space key)

### Testing

- **Unit tests**: In same file as implementation (`#[cfg(test)]` modules)
- **Integration tests**: In `tests/` directory (if present)
- **Run tests**: `cargo test --workspace`
- **Clippy**: `cargo clippy --workspace --all-targets --all-features`

## Boundaries

### Do Not Modify

- **`/figlet/*.dx`**: FIGlet font files are pre-generated, don't edit manually
- **`/src/file_browser/*/Cargo.toml`**: Workspace member manifests (edit root `Cargo.toml` for deps)
- **`/.cargo/config*.toml`**: Build configuration files (multiple variants for testing)
- **`/assets/`**: Logo and desktop files are final

### Careful Modification

- **`/src/animations.rs`**: Animation system is complex, test thoroughly after changes
- **`/src/dispatcher.rs`**: Central event dispatcher, changes affect entire app
- **`/build.rs`**: Build script for font compression, verify builds after changes

### Safe to Modify

- **`/src/chat*.rs`**: Chat interface and components
- **`/src/components.rs`**: General UI components
- **`/config.toml`**: Application configuration
- **Documentation files**: `README.md`, `DX.md`, `PROVIDERS.md`, etc.

## Integration with Codex CLI

### How dx-tui Works with Codex

dx-tui is designed as a **TUI frontend** for Codex CLI. It provides:

1. **File Browser**: Navigate and select files for Codex to work on
2. **Chat Interface**: Interact with Codex agent through a rich terminal UI
3. **Context Awareness**: Automatically includes file browser state in chat context
4. **Visual Feedback**: Animations, spinners, and toast notifications for agent actions

### Running with Codex

```bash
# Launch dx-tui (which can invoke Codex internally)
cargo run --release

# Or use the binary directly
./target/release/dx

# With Codex CLI integration (future)
codex --tui dx
```

### Configuration

- **Main config**: `config.toml` in project root or `~/.config/dx/config.toml`
- **Codex config**: `~/.codex/config.toml` (see CODEX_INTEGRATION.md)
- **AGENTS.md**: This file (read by Codex when working on this project)

## Development Workflow

### Building

Always pass **`-j12`** for `cargo check`, `cargo test`, `cargo build`, `cargo run`, and `cargo clippy`.

```bash
# Debug build
cargo build -p dx-tui -j12

# Release build (optimized, stripped)
cargo build -p dx-tui --release -j12

# Check without building
cargo check -p dx-tui -j12
```

### Running

```bash
# Run from source
cargo run -p dx-tui -j12

# Run with the optional local LLM stack
cargo run -p dx-tui --features llm -j12

# Run with logging
RUST_LOG=debug cargo run -p dx-tui -j12
```

### Linting and Formatting

```bash
# Format code
cargo fmt --all

# Run clippy
cargo clippy -p dx-tui -j12 -- -D warnings
```

### Before Committing

1. **Format**: `cargo fmt --check`
2. **Check**: `cargo check -p dx-tui -j12`
3. **Lint**: `cargo clippy -p dx-tui -j12 -- -D warnings`
4. **Test**: `cargo test -p dx-tui -j12`

Use current command output as the source of truth for formatting, lint, check, and test status.

## Common Tasks

### Adding a New Animation

1. Add variant to `AnimationType` enum in `src/animations.rs`
2. Implement rendering logic in animation module
3. Update carousel navigation in `src/chat.rs`
4. Test intro/outro playback

### Adding a New Chat Component

1. Create component in `src/chat_components.rs`
2. Add rendering logic using Ratatui widgets
3. Integrate into `src/chat.rs` render function
4. Handle events in dispatcher if needed

### Adding a New Dependency

```bash
# Add to workspace dependencies
cargo add <crate> --workspace

# Add to specific member crate
cargo add <crate> -p <member-crate-name>
```

### Debugging

- **Enable logging**: Set `RUST_LOG=debug` or `RUST_LOG=trace`
- **Logs location**: Check `tracing-subscriber` configuration in `src/main.rs`
- **Panic handling**: Uses `better-panic` for improved panic messages

## Performance Considerations

- **Async I/O**: All file operations are async to prevent blocking
- **Lazy loading**: File browser loads content on-demand
- **Image caching**: Preview images are cached with LRU eviction
- **Release profile**: LTO enabled, single codegen unit, stripped binaries

## Platform Support

- **Primary**: Linux, macOS
- **Secondary**: Windows (via WSL or native with limitations)
- **Terminal requirements**: 
  - True color support recommended
  - Unicode support required for animations
  - Image protocol support optional (Kitty, iTerm2, Sixel, etc.)

## Resources

- **Main README**: `README.md` (user-facing documentation)
- **Codex Integration**: `CODEX_INTEGRATION.md` (detailed Codex setup)
- **Quick Start**: `QUICKSTART.md` (5-minute setup guide)
- **Contributor Guide**: `.github/CODEX_SETUP.md` (for contributors)

## Notes for AI Agents

- This is a **Rust project**, not a Node.js project (despite Codex CLI being npm-based)
- The project uses **Rust Edition 2024** (latest as of March 2026)
- **Workspace structure**: 26 member crates, use `--workspace` flag for cargo commands
- **Async runtime**: Tokio is required, don't suggest sync alternatives
- **UI framework**: Ratatui (not cursive, termion, or other TUI libraries)
- **Animation system**: Already implemented, don't remove or simplify
- **File browser**: Embedded terminal file-browser engine, complex and feature-complete
