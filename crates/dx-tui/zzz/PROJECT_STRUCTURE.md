# Project Structure

```
dx-tui/
├── src/
│   ├── main.rs              # Entry point
│   ├── lib.rs               # Crate root, CLI arg parsing, global allocator
│   ├── state.rs             # ChatState — main application state
│   ├── dispatcher.rs        # Event dispatch (keyboard, mouse, events)
│   ├── chat_render.rs       # Main chat rendering
│   ├── agent_loop.rs        # Multi-step LLM tool loop
│   ├── components.rs        # Message components, Markdown renderer
│   ├── token_save.rs        # Token compression and estimation
│   ├── bridge.rs            # Mode bridge (Chat / FilePicker / Editor)
│   ├── editor/              # Editor adapter + dx-editor crate
│   │   └── adapter.rs       # Ratatui CaptureBackend for editor rendering
│   ├── file_browser/        # 26 crate workspace — async file browser
│   └── ...                  # ~100 additional modules
├── figlet/                  # FIGlet fonts (114 .dx files)
└── assets/                  # Logo and desktop files
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for module boundary documentation.
