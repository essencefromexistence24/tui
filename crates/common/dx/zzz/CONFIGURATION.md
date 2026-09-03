# Configuration

## Config file

`~/.config/dx/config.toml`

## Environment Variables

| Variable | Description |
|----------|-------------|
| `DX_TUI_CONTINUE_SESSION` | Session ID to resume on startup |
| `DX_TUI_MODEL_PATH` | Path to local GGUF model |
| `DX_AGENT_WORKSPACE` | Agent workspace directory |
| `RUST_LOG` | Log level (debug, info, warn, error) |

## Session Storage

- `~/.config/dx/sessions/` — JSON snapshots + SQLite database with FTS5 search

## Plugin Directory

- `~/.config/dx/plugins/` — Lua plugin scripts
