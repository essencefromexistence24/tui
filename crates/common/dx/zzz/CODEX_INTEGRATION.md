# Codex CLI Integration

dx-tui can be used alongside Codex CLI as a file browser and chat interface.

## Workflow

1. Use dx-tui's file browser to navigate and select files
2. Switch to chat mode to discuss changes with the AI agent
3. Files are read/written through the embedded editor engine

## Invocation

Codex CLI can launch dx-tui as an external editor:

```bash
codex --editor dx
```

## Configuration

Set `CODEX_EDITOR=dx` in your shell profile to use dx-tui as the default editor for Codex.
