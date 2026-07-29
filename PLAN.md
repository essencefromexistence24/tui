# Plan: Merge dx-tui into grok-build

> **Goal**: Make grok-build the new dx-tui. A single unified TUI that combines grok-build's professional chat with all of dx-tui's components — sidebar, minimap, animations, code editor, file browser, diff viewer, command palette, and everything else.

## Implementation Status — Complete

- [x] Ratatui 0.30 and Crossterm 0.29 unified across the host and DX crates.
- [x] Grok remains the sole terminal, raw-mode, alternate-screen, and input-stream owner.
- [x] DX sidebar and minimap render from Grok's live session state.
- [x] DX diff viewer is directly embedded with keyboard and mouse navigation.
- [x] DX command palette is directly embedded as an overlay.
- [x] DX editor is directly embedded and receives Grok's events.
- [x] DX file-browser Core, Lua layout, widgets, keymap, actors, and event dispatcher run in-process.
- [x] The file-browser engine is shared safely across Grok session views.
- [x] DX train animation is directly embedded as presentation-only state.
- [x] Ctrl+1–Ctrl+5, Esc, `/editor`, `/browser`, `/diff`, and `/anim` are wired.
- [x] Windows production build and executable smoke check pass.
- [x] Focused animation and multi-session browser ownership tests pass.

## 0. Non-Negotiable Integration Invariants

The finished application is one native Rust TUI, not two applications joined by
a process or synchronization bridge:

1. `xai-grok-pager` is the only owner of the Ratatui terminal.
2. `xai-grok-pager` is the only owner of Crossterm raw mode, alternate screen,
   terminal restoration, and the input event stream.
3. Grok's existing application/agent state is the sole source of truth for
   conversations, tasks, tools, subagents, models, permissions, and persistence.
4. Ported DX code may own presentation state only: selection, scrolling,
   expanded sections, active panels, editor/file-browser controllers, and
   animation clocks.
5. Ported DX controls emit typed user intent into Grok's Actions/Effects path;
   they do not perform agent or terminal side effects themselves.
6. Every component renders into the `Rect` and `Buffer` supplied by Grok.
7. The DX `ChatState`, dispatcher, terminal loop, backends, session database,
   provider registry, and Codex bridge are migration sources, not runtime
   dependencies of the merged application.

---

## 1. Repository Layout

```
grok-build/                          ← workspace root
├── crates/
│   ├── codegen/                     ← existing grok crates (63 crates)
│   │   ├── xai-grok-pager/          ← main TUI (host)
│   │   ├── xai-grok-pager-render/   ← presentation primitives
│   │   ├── xai-ratatui-inline/      ← terminal fork (needs 0.30 bump)
│   │   └── ...
│   │
│   └── dx-tui/                      ← ported from G:\Dx\tui\codex-rs\dx-tui
│       ├── src/                     ← main dx-tui library
│       │   ├── state.rs             ← ChatState (4,908 lines)
│       │   ├── dispatcher.rs        ← event routing (3,455 lines)
│       │   ├── components.rs        ← message blocks & rendering (3,106 lines)
│       │   ├── chat_render.rs       ← minimap + chat cards (2,864 lines)
│       │   ├── sidebar_data.rs      ← right sidebar (498 lines)
│       │   ├── diff_view.rs         ← diff viewer (956 lines)
│       │   ├── animations.rs        ← animation renderers (1,220 lines)
│       │   ├── effects.rs           ← shimmer/rainbow/typing (182 lines)
│       │   ├── modes.rs             ← AgentMode, RuntimeMode (299 lines)
│       │   ├── theme.rs             ← ChatTheme (192 lines)
│       │   ├── input.rs             ← multi-line input (1,449 lines)
│       │   ├── slash_commands.rs    ← slash command system (1,924 lines)
│       │   ├── menu/                ← command palette + 27 submenus (8 files)
│       │   ├── msg_ui/              ← professional message cells (10 files)
│       │   ├── editor/              ← dx-editor adapter + 7 crates
│       │   ├── file_browser/        ← 26 fb-* crates + Lua
│       │   ├── codex/               ← codex bridge (10 files — NOT ported)
│       │   └── ... (52+ modules)
│       └── Cargo.toml               ← own workspace (30+ member crates)
│
├── Cargo.toml                       ← add dx-tui members here
└── PLAN.md                          ← this file
```

---

## 2. Architecture: Merged TUI

```
xai-grok-pager (grok's main TUI, host of everything)
  └── AppView (root component, 11,689 lines)
        │
        ├── DxUiState (NEW — owns presentation state only)
        │     ├── minimap: MinimapUiState
        │     ├── sidebar: SidebarState (right panel)
        │     ├── editor: EditorAdapter (code editor)
        │     ├── diff: DiffState (diff viewer)
        │     ├── menu: Menu (command palette)
        │     └── fb_core: Option<Core> (file browser engine)
        │
        ├── draw() ─────────────────────────────────────────────┐
        │     │                                                  │
        │     ├── Match active_view:                             │
        │     │     ├── Scrollback (default)                     │
        │     │     │     ├── LEFT: Minimap panel (dx-tui)       │
        │     │     │     ├── CENTER: Scrollback + Prompt (grok) │
        │     │     │     └── RIGHT: Sidebar panel (dx-tui)      │
        │     │     │                                             │
        │     │     ├── DxAnimation                              │
        │     │     │     └── Train / Matrix / Confetti / etc.   │
        │     │     │                                             │
        │     │     ├── DxEditor                                 │
        │     │     │     └── EditorAdapter::render(area, buf)   │
        │     │     │                                             │
        │     │     ├── DxFileBrowser                            │
        │     │     │     └── TerminalRoot::render(area, buf)    │
        │     │     │                                             │
        │     │     └── DxDiff                                   │
        │     │           └── DiffState render(area, buf)        │
        │     │                                                  │
        │     └── draw_overlays() (always rendered)              │
        │           ├── Command palette (dx-tui menu/)           │
        │           ├── Modals (shared)                          │
        │           └── Status bar (shared)                      │
        │                                                        │
        └── handle_input() ─────────────────────────────────────┐
              │                                                  │
              └── Route to active dx-tui component:             │
                    ├── Editor → EditorAdapter::handle_event()   │
                    ├── FileBrowser → fb Router                  │
                    ├── Diff → DiffState::navigate()             │
                    └── Chat → existing grok dispatch            │
```

### Rendering Contract

Every dx-tui component renders into `(Rect, &mut Buffer)` — the same buffer grok owns. No dx-tui component creates its own `ratatui::Terminal` or touches crossterm directly.

| Component | Rendering method | Buffer source |
|-----------|-----------------|---------------|
| Editor | `EditorAdapter::render(area, buf)` via `CaptureBackend` | Grok's frame buffer |
| File browser | `TerminalRoot::render_file_browser(area, buf)` via Lua | Grok's frame buffer |
| Diff viewer | `DiffState` renders with ratatui widgets into buf | Grok's frame buffer |
| Sidebar | `SidebarState::render(area, buf)` | Grok's frame buffer |
| Minimap | `ChatState::render_minimap(area, buf)` | Grok's frame buffer |
| Animations | `AnimationType::render(area, buf)` | Grok's frame buffer |
| Command palette | `Menu::render(area, buf)` | Grok's frame buffer |

---

## 3. What Stays / What Comes

### Stays from grok-build (as-is)
| Component | Why |
|-----------|-----|
| Chat scrollback + prompt (`views/agent.rs`) | More professional, keep as default |
| Event loop (`event_loop.rs`) | Host loop, owns terminal |
| Actions/Effects pattern | Mature dispatch system |
| `PagerTerminal` (`xai-ratatui-inline`) | Custom terminal fork — upgrade to 0.30 |
| Theme system (`xai-grok-pager-render/theme/`) | 5 built-in themes, quantization |
| Terminal detection (`terminal/`) | 23+ terminal brands |
| ACP agent communication | Mature agent protocol |
| All 58 view modules | Full feature set |

### Comes from dx-tui (as-is, integrated)
| Component | Lines | How it integrates |
|-----------|:-----:|-------------------|
| `sidebar_data.rs` — Right panel | 498 | New sidebar overlay in scrollback view |
| `chat_render.rs` — Minimap | 2,864 | New left panel in scrollback view |
| `components.rs` — Message rendering | 3,106 | Used by minimap + msg_ui |
| `msg_ui/` — Rich message cells | ~3,800 | Alternate message view mode |
| `diff_view.rs` — Diff viewer | 956 | New full-screen view mode |
| `editor/adapter.rs` — Code editor | 236 | New full-screen view mode |
| `file_browser/` — 26 fb-* crates | ~47K | New full-screen view mode |
| `animations.rs` — Animations | 1,220 | Decorative screens / splash |
| `effects.rs` — Shimmer/Rainbow/Typing | 182 | Visual polish |
| `menu/` — Command palette | ~2,500 | Overlay system |
| `modes.rs` — Agent modes | 299 | Shared enum types |
| `input.rs` — Text input | 1,449 | Optional replacement for grok's prompt |
| `slash_commands.rs` — Slash commands | 1,924 | Shared command system |
| `state.rs` — ChatState | 4,908 | Owned by DxSession, provides data to panels |

### NOT ported from dx-tui
| Component | Reason |
|-----------|--------|
| `codex/` bridge (10 files) | Grok has ACP instead |
| `dispatcher.rs` (3,455 lines) | Grok has Actions/Effects pattern |
| `zen.rs`, `agent_backend.rs`, `flow_backend.rs` | Grok has its own agent system |
| `session_db.rs`, `session_store.rs` | Grok has its own persistence |
| `permission_hub.rs` | Grok has its own permission system |
| `provider_registry.rs` | Grok has its own model system |

---

## 4. Work Phases

### Phase 0 — Foundation

| Step | Detail | Est. time |
|------|--------|:---------:|
| 0.1 | Bump ratatui 0.29→0.30 across all grok TUI crates | **4-8 hr** |
| 0.2 | Bump crossterm 0.28→0.29 | **1-2 hr** |
| 0.3 | Add dx-tui's 30+ member crates as paths in grok's workspace `Cargo.toml` | **2-4 hr** |
| 0.4 | Reconcile dependency versions between grok and dx-tui | **2-4 hr** |
| 0.5 | Create `DxSession` struct in xai-grok-pager/src/dx_bridge/mod.rs | **2 hr** |
| 0.6 | `cargo check` passes for entire workspace | **varies** |

**Dependency reconciliation table:**

| Dep | grok | dx-tui | Target |
|-----|:----:|:------:|:------:|
| ratatui | 0.29 | 0.30 | **0.30** |
| crossterm | 0.28 | 0.29 | **0.29** |
| tokio | 1.x | 1.42 | **1.42+** |
| Rust edition | 2021 | 2024 | **2024** |
| syntect | 5.3 | 5.2 | **5.3** |
| clap | 4 | 4.5 | **4.5+** |
| pulldown-cmark | 0.13 | 0.13.4 | **0.13.4** |
| mlua | — | 0.11.6 | **0.11.6** (new, gate behind `file-browser` feature) |
| tachyonfx | — | 0.25 | **0.25** (new, gate behind `animations` feature) |

### Phase 1 — Chat Screen Upgrade (3-panel layout)

| Step | Detail | Files |
|------|--------|-------|
| 1.1 | Add left minimap panel to `views/agent.rs` layout | `chat_render.rs` minimap section |
| 1.2 | Add right sidebar panel to `views/agent.rs` layout | `sidebar_data.rs` |
| 1.3 | Wire sidebar state refresh (tasks, LSP, MCP, plugins, subagents, prompts, notes) | — |
| 1.4 | Verify existing scrollback + prompt still works correctly | — |

**Layout:**
```
┌──────┬────────────────────────┬──────────────┐
│ Mini │    Scrollback          │    Sidebar    │
│ map  │    (existing grok)     │  Tasks (☐◐☑) │
│      │                        │  Prompts      │
│      │    ─────────────       │  Notes        │
│      │    Prompt input        │  Subagents    │
│      │    (existing grok)     │  LSP status   │
│      │                        │  Plugins      │
├──────┴────────────────────────┴──────────────┤
│              Status bar (shared)              │
└──────────────────────────────────────────────┘
```

### Phase 2 — Diff Viewer

| Step | Detail | Est. time |
|------|--------|:---------:|
| 2.1 | Add `AgentViewMode::DxDiff` variant | **1 hr** |
| 2.2 | Import `DiffState` from dx-tui's `diff_view.rs` | **2 hr** |
| 2.3 | Wire `/diff` slash command and keybinding | **1 hr** |
| 2.4 | Diff renders in fullscreen, returns to chat on Esc | **1 hr** |

### Phase 3 — Command Palette

| Step | Detail | Est. time |
|------|--------|:---------:|
| 3.1 | Import `menu/` module (8 files) | **2 hr** |
| 3.2 | Wire keybinding (e.g., `Ctrl+P` or `Cmd+P`) | **1 hr** |
| 3.3 | Render as overlay above scrollback | **2 hr** |
| 3.4 | Wire 27 submenus to grok's action system | **4 hr** |

### Phase 4 — Editor

| Step | Detail | Est. time |
|------|--------|:---------:|
| 4.1 | Add 7 dx-editor crates to workspace | **2 hr** |
| 4.2 | Import `EditorAdapter` from `editor/adapter.rs` | **1 hr** |
| 4.3 | Add `AgentViewMode::DxEditor` variant | **1 hr** |
| 4.4 | Wire `/editor` and `Ctrl+E` keybinding | **1 hr** |
| 4.5 | Handle two-runtime pattern (editor's tokio runtime vs grok's) | **2 hr** |
| 4.6 | Wire event forwarding: key events → `EditorAdapter::handle_event()` | **2 hr** |

### Phase 5 — File Browser

| Step | Detail | Est. time |
|------|--------|:---------:|
| 5.1 | Add 26 fb-* crates to workspace | **2 hr** |
| 5.2 | Initialize fb subsystems (`Core`, `Term`, config) in `DxSession` | **4 hr** |
| 5.3 | Import `TerminalRoot` rendering into grok's buffer | **2 hr** |
| 5.4 | Add `AgentViewMode::DxFileBrowser` variant | **1 hr** |
| 5.5 | Wire `/browser` and `Ctrl+B` keybinding | **1 hr** |
| 5.6 | Wire event forwarding to file browser's Router | **2 hr** |
| 5.7 | Gate behind `file-browser` feature (adds mlua, image deps) | **1 hr** |

### Phase 6 — Animations & Visual Polish

| Step | Detail | Est. time |
|------|--------|:---------:|
| 6.1 | Import `animations.rs` | **2 hr** |
| 6.2 | Wire train animation on session startup | **1 hr** |
| 6.3 | Wire confetti on turn completion | **1 hr** |
| 6.4 | Import `effects.rs` shimmer/rainbow/typing | **2 hr** |
| 6.5 | Gate behind `animations` feature (adds tachyonfx) | **1 hr** |

---

## 5. Key Code: The Bridge (DxSession)

The bridge is ~3 new files in `xai-grok-pager/src/dx_bridge/`:

### `src/dx/state.rs`
```rust
pub struct DxUiState {
    pub sidebar: SidebarUiState,       // selection/accordion state only
    pub editor: EditorAdapter,         // code editor
    pub diff: DiffState,               // diff viewer
    pub menu: Menu,                    // command palette
    pub active_animation: Option<AnimationType>, // animation screen
    pub file_browser: Option<FileBrowserState>,
    pub mode: DxViewMode,              // which dx view is active
}

pub enum DxViewMode {
    None,          // grok's native scrollback
    Editor,
    FileBrowser,
    Diff,
    Animation,
}
```

### `src/dx/render.rs`
Routes rendering to the active dx-tui component:

```rust
impl DxUiState {
    pub fn render(&mut self, area: Rect, buf: &mut Buffer, grok: &GrokViewModel) {
        match self.mode {
            DxViewMode::None => {} // handled by grok's own rendering
            DxViewMode::Editor => self.editor.render(area, buf),
            DxViewMode::FileBrowser => {
                if let Some(ref mut core) = self.fb_core {
                    let root = TerminalRoot::new(core);
                    root.render_file_browser(area, buf);
                }
            }
            DxViewMode::Diff => self.diff.render(area, buf),
            DxViewMode::Animation => {
                if let Some(anim) = self.active_animation {
                    anim.render(area, buf);
                }
            }
        }
    }
}
```

### `src/dx_bridge/event.rs`
Routes events to the active dx-tui component.

---

## 6. View Switching

| Input | Action |
|-------|--------|
| `Ctrl+1` | Chat (grok's scrollback + prompt + minimap + sidebar) |
| `Ctrl+2` | Code editor |
| `Ctrl+3` | File browser |
| `Ctrl+4` | Diff viewer |
| `Ctrl+P` | Command palette (overlay) |
| `/editor` | Switch to editor |
| `/browser` | Switch to file browser |
| `/diff` | Switch to diff viewer |
| `/anim` | Switch to animation screen |
| `Esc` | Return to chat from any fullscreen mode |

---

## 7. Feature Gating

Add to `xai-grok-pager/Cargo.toml`:

```toml
[features]
default = ["dx-chat"]

# Panel features
dx-chat = ["dep:dx-tui"]                    # minimap + sidebar
dx-diff = ["dx-chat"]                        # diff viewer
dx-palette = ["dx-chat"]                     # command palette
dx-editor = ["dx-chat", "dep:dx-editor"]     # code editor
dx-file-browser = ["dx-chat", "dep:fb-core", "dep:mlua"]  # file browser
dx-animations = ["dx-chat", "dep:tachyonfx"] # animations
```

---

## 8. Timeline

| Phase | What | Lines | Est. time |
|:-----:|------|:-----:|:---------:|
| 0 | Foundation (workspace + deps + bump) | config | **8-16 hr** |
| 1 | Chat upgrade: minimap + sidebar | ~3,362 | **8-16 hr** |
| 2 | Diff viewer | 956 | **4-6 hr** |
| 3 | Command palette | ~2,500 | **8-12 hr** |
| 4 | Code editor | ~280K | **16-24 hr** |
| 5 | File browser | ~47K | **24-40 hr** |
| 6 | Animations + polish | ~1,400 | **8-12 hr** |
| **Total** | | **~335K** | **~76-126 hr** |

---

## 9. Key Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| ratatui 0.29→0.30 breaks `xai-ratatui-inline` | Blocks everything | Audit custom Terminal fork first; upstream ratatui changelog |
| dx-editor adds 280K LOC and 7 crates | Build time, complexity | Gate behind feature flag; incremental compilation |
| mlua adds Lua VM | Security surface | Gate behind `file-browser` feature; keep optional |
| Two tokio runtimes (grok + editor) | Complex blocking patterns | Already handled in `EditorAdapter` via `block_in_place` |
| Windows compatibility | Testing burden | Grok already supports Windows (protoc workaround documented in AGENTS.md) |
