# Merge Plan: grok-build as Base + Port dx-tui Features

> **Strategy**: Keep grok-build as the base binary (`xai-grok-pager`), agent backend, and core architecture. Port dx-tui's TUI features (message rendering, file browser, code editor) into grok-build's existing view system as new screens. The "message screen" becomes the default view, replacing grok-build's current scrollback+prompt layout.

---

## 1. Project Comparison

| Dimension | dx-tui | grok-build |
|-----------|--------|------------|
| Binary | `dx` | `xai-grok-pager` (shipped as `grok`) |
| Workspace | 32 members (26 `fb-*` + 6 `dx-editor`) | ~85 crates (`crates/codegen/` + `crates/common/`) |
| ratatui | **0.30** | **0.29** |
| crossterm | **0.29** | **0.28** |
| tokio | 1.42 | 1.x |
| Rust edition | 2024 | 2021 |
| TUI strengths | File browser (26 crates, 47K), editor (280K lines), animations (tachyonfx), msg_ui with PTY, Lua scripting | Scrollback/pager, inline viewports, theme system, Action/Effect pattern |
| Agent backend | codex-rs (in-process, migrating from native) | Grok's own agent (leader/stdio/headless, ACP, MCP) |
| Key deps | tachyonfx, mlua, termimad, rodio, cpal, syntect | protobuf, OTel, jemalloc, sentry |
| License | MIT | Apache 2.0 |

---

## 2. Architecture: grok-build's Existing View System

grok-build's TUI is organized with an **Action/Effect pattern**:

```
main.rs → app::run()
  └── init_terminal() → PagerTerminal (xai_ratatui_inline::Terminal)
  └── event_loop::run()
        ├── AppView (root component)
        │     ├── handle_input()  → Action → Effect
        │     └── draw()          → render state to Buffer
        │           ├── views/welcome/          ← startup screen
        │           ├── views/agent.rs          ← main session screen (scrollback+prompt)
        │           ├── views/status_bar.rs     ← footer
        │           ├── views/shortcuts_bar.rs  ← keyboard hints
        │           ├── views/overlay_list.rs   ← modals/overlays
        │           └── ... (53 view modules)
        └── views/               ← screen widgets
```

**The merge adds 3 new view modules** — message screen, file browser, editor — alongside the existing 53 views. The message screen becomes the **default** screen mode for agent sessions.

---

## 3. Target Architecture

```
grok-build workspace (xai-grok-pager-bin)
  │
  ├── EXISTING: backend crates (keep as-is)
  │   ├── xai-grok-shell           ← agent, session, config, auth
  │   ├── xai-grok-tools           ← tool execution
  │   ├── xai-grok-mcp             ← Model Context Protocol
  │   ├── xai-grok-workspace       ← workspace server
  │   ├── xai-grok-auth            ← credential management
  │   ├── xai-grok-http            ← HTTP client
  │   ├── xai-grok-config          ← config loading
  │   ├── xai-grok-update          ← auto-update
  │   ├── xai-crash-handler        ← crash diagnostics
  │   ├── xai-grok-telemetry       ← OTel/Sentry
  │   ├── xai-grok-pager-render    ← presentation primitives (extend)
  │   ├── xai-ratatui-inline       ← terminal backend (upgrade to 0.30)
  │   ├── xai-ratatui-textarea     ← text input widget (upgrade)
  │   └── xai-grok-markdown        ← markdown rendering
  │
  ├── EXISTING: TUI crates (extend)
  │   ├── xai-grok-pager           ← main TUI library
  │   │     ├── app/               ← AppView, AgentView, dispatch, effects
  │   │     └── views/             ← 53 screen widgets
  │   │           ├── agent.rs     ← EXISTING: scrollback+prompt (keep as alt)
  │   │           ├── message_screen/  ← NEW: ported msg_ui rendering
  │   │           ├── file_browser/    ← NEW: ported fb-* crates
  │   │           └── editor/          ← NEW: ported dx-editor
  │   └── xai-grok-pager-minimal   ← inline mode (keep)
  │
  └── NEW: ported from dx-tui
      ├── msg_ui/                  ← message rendering core (~5K lines)
      ├── animations.rs            ← decorative effects (~3K lines)
      ├── fb-* (26 crates)         ← file browser engine (~47K lines)
      └── dx-editor (6 crates)     ← code editor engine (~280K lines)
```

---

## 4. Screen Mode Concept

The merge extends grok-build's existing `ScreenMode` enum or adds a new view selection mechanism:

```rust
// In grok-build's app_view.rs or equivalent
pub enum AgentViewMode {
    /// Current scrollback+prompt layout (default before merge)
    Scrollback,
    /// New rich-message screen using dx-tui's msg_ui rendering (default after merge)
    MessageScreen,
    /// Full-screen file browser
    FileBrowser,
    /// Full-screen code editor
    Editor,
}
```

The user switches between modes with keybindings or slash-commands. The **MessageScreen** is the default when starting an agent session.

---

## 5. Dependency Reconciliation

### 5.1 Version targets

| Dependency | grok-build (current) | dx-tui (current) | Target | Effort |
|-----------|---------------------|-------------------|--------|--------|
| ratatui | 0.29 | 0.30 | **0.30** | High |
| crossterm | 0.28 | 0.29 | **0.29** | Medium |
| tokio | 1.x | 1.42 | **1.42+** | Low |
| Rust edition | 2021 | 2024 | **2024** | Medium |
| syntect | 5.3 | 5.2 | **5.3** | Low |
| reqwest | 0.12 | 0.12 | **0.12** | None |
| serde | 1 | 1 | **1** | None |
| clap | 4 | 4.5 | **4.5+** | Low |
| pulldown-cmark | 0.13 | 0.13.4 | **0.13.4** | None |

### 5.2 ratatui 0.29 → 0.30 audit (Phase 0 — gating item)

| Crate | Risk | Impact |
|-------|------|--------|
| `xai-ratatui-inline` | **Critical** | Custom fork of `ratatui::Terminal`. 0.30 split ratatui into `ratatui-core`, `ratatui-crossterm`, `ratatui-widgets`. The `Terminal` API changed: `draw()` uses `Frame`, `insert_before()` API may differ, `Viewport`/`TerminalOptions` may have changed. |
| `xai-ratatui-textarea` | Medium | Widget-level code. Uses `Buffer`, `Widget` trait — stable across 0.30. |
| `xai-grok-pager-render` | Medium | Uses `Buffer`, `Cell`, `Style`. Some minor `Into<Color>` changes. |
| `xai-grok-pager` | Medium | Uses `Layout`, `Constraint`, widgets. 0.30 added `Layout::horizontal()`/`vertical()` but old API still works. |
| `xai-grok-pager-minimal` | Medium | Depends on `xai-ratatui-inline` for inline viewport. |

**ratatui 0.30 breaking changes that affect grok-build:**
- `ratatui` split into sub-crates; update `Cargo.toml` paths
- `Terminal::draw()` now takes `FnOnce(&mut Frame)` — closure signature change
- `Frame::size()` → `Frame::area()`
- `Buffer::filled()` → `Buffer::filled_with()`
- `Style::fg(Color)` — some `Into<Color>` blanket impls removed
- `Line::styled(text, style)` → `Line::from(Span::styled(text, style))`
- `symbols::scrollbar` → moved to `ratatui-widgets` feature
- `Paragraph::new(text).wrap(Wrap { trim: true })` → API unchanged

### 5.3 Workspace merge strategy

**Do NOT physically merge source trees.** Instead:

1. Add dx-tui's crates as a **second workspace root** or **path dependencies** in grok-build's workspace `Cargo.toml`
2. Each ported dx-tui crate gets its own directory under grok-build's `crates/` tree
3. Cargo resolver 2 handles dependency deduplication
4. The main binary stays `xai-grok-pager-bin` — it gains new optional features

```toml
# grok-build/Cargo.toml additions
[dependencies]
# Ported from dx-tui
dx-editor = { path = "crates/dx-editor", optional = true }
fb-core = { path = "crates/fb-core", optional = true }
msg-ui = { path = "crates/msg-ui", optional = true }

[features]
default = ["message-screen"]
message-screen = ["dep:msg-ui", "dep:dx-animations"]
file-browser = ["dep:fb-core", "dep:fb-widgets", "dep:fb-fs"]
editor = ["dep:dx-editor"]
```

---

## 6. Merge Phases

### Phase 0 — ratatui 0.30 Upgrade (gating item)

- [ ] Audit `xai-ratatui-inline/src/lib.rs` — identify every `ratatui` API call
- [ ] Try compiling `xai-ratatui-inline` with ratatui 0.30 deps
- [ ] Fix breaking changes (Terminal::draw, Frame, etc.)
- [ ] Compile `xai-grok-pager-render` with 0.30
- [ ] Compile `xai-grok-pager` with 0.30
- [ ] Compile entire grok-build workspace with ratatui 0.30 + crossterm 0.29
- [ ] Run existing tests to verify no regressions

**Fallback if Phase 0 fails**: Keep grok-build on ratatui 0.29. Ported dx-tui features that need 0.30 (tachyonfx animations, msg_ui) run in a separate embedded process via the leader protocol. The message screen becomes a thin client that connects to the local leader.

### Phase 1 — Port `msg_ui/` Rendering into `xai-grok-pager`

The message screen replaces the scrollback+prompt view in `views/agent.rs`. Port dx-tui's `msg_ui/` modules as grok-build views.

#### 1.1 Create `views/message_screen/` module

```
xai-grok-pager/src/views/message_screen/
├── mod.rs           ← module declarations + MessageScreenView struct
├── parse.rs         ← StreamPart, PlanStep parsing (port from dx-tui msg_ui/parse.rs)
├── render.rs        ← MessageScreenView::draw() — renders message cells to Buffer
├── live.rs          ← Live streaming append/push for tool bodies, PTY (port from msg_ui/live.rs)
├── diff_review.rs   ← Diff accept/reject UI (port from msg_ui/diff_review.rs)
├── branch_ui.rs     ← Branch picker UI (port from msg_ui/branch_ui.rs)
├── pty_host.rs      ← PTY host for interactive terminal sessions (port from msg_ui/pty_host.rs)
├── vt_grid.rs       ← VT cell grid rendering (port from msg_ui/vt_grid.rs)
├── ansi.rs          ← ANSI escape handling (port from msg_ui/ansi.rs)
├── copy.rs          ← Clean copy extraction (port from msg_ui/copy.rs)
└── animations.rs    ← Loading spinners, transitions (port from dx-tui animations.rs)
```

**Porting principle**: Take dx-tui's `msg_ui/` files (which are pure rendering with no dx-tui-specific state dependencies) and adapt them to work with grok-build's event stream. The `MessageScreenView` struct replaces dx-tui's `ChatState` for message rendering.

#### 1.2 Integrate with grok-build's ACP event stream

In grok-build's `AgentView`, agent events arrive via ACP (Agent Client Protocol). The message screen subscribes to the same event stream but renders differently.

```rust
// MessageScreenView receives ACP events and renders cells
impl MessageScreenView {
    pub fn handle_acp_event(&mut self, event: &AcpEvent) {
        match event {
            AcpEvent::ContentDelta { text } => {
                self.stream_collector.push_str(text);
                self.drain_ready_lines();
            }
            AcpEvent::ToolCallStarted { tool, args } => {
                self.active_tool = Some(ToolCell::new(tool, args));
            }
            AcpEvent::ToolCallFinished { tool, output } => {
                if let Some(cell) = self.active_tool.take() {
                    cell.set_output(output);
                    self.message_cells.push(Arc::new(cell));
                }
            }
            AcpEvent::TurnCompleted { .. } => {
                self.finalize_turn();
            }
            AcpEvent::RequestApproval { request } => {
                self.pending_approvals.push(request);
            }
            _ => {}
        }
    }

    pub fn draw(&self, area: Rect, buf: &mut Buffer) {
        // Render message cells using dx-tui's cell rendering logic
        // Render prompt bar at bottom
        // Render approval overlays if pending
    }
}
```

#### 1.3 Connect to `AppView`

`AppView` gains a new variant or field:

```rust
pub struct AppView {
    // ... existing fields for scrollback, prompts, etc.

    // NEW: message screen view
    pub message_screen: Option<MessageScreenView>,

    // Which view mode is active
    pub agent_view_mode: AgentViewMode,
}
```

In `AppView::draw()`:

```rust
fn draw(&mut self, area: Rect, buf: &mut Buffer) {
    match self.agent_view_mode {
        AgentViewMode::Scrollback => {
            // Existing scrollback+prompt rendering
            self.draw_scrollback_view(area, buf);
        }
        AgentViewMode::MessageScreen => {
            // New message screen rendering
            self.message_screen.draw(area, buf);
        }
        AgentViewMode::FileBrowser => {
            // File browser view
            self.file_browser.draw(area, buf);
        }
        AgentViewMode::Editor => {
            // Code editor view
            self.editor.draw(area, buf);
        }
    }
    // Shared overlay layer (modals, notifications, etc.)
    self.draw_overlays(area, buf);
}
```

#### 1.4 Message screen as default

In startup:

```rust
// In app::run() or session startup
agent_view_mode = AgentViewMode::MessageScreen;  // default for new sessions
```

User can switch with:
- `/screen scrollback` → back to traditional layout
- `/screen message` → message screen (default)
- `/screen browser` → file browser
- `/screen editor` → code editor

---

### Phase 2 — Port File Browser (26 fb-* crates)

dx-tui's file browser is a **26-crate sub-workspace** (~47K lines) that's nearly independent of dx-tui's TUI. It depends on:
- ratatui (for rendering)
- crossterm (for terminal interaction)
- tokio, serde, etc.
- Lua scripting (mlua) for plugins
- image libraries (image, resvg) for previews

#### 2.1 Add fb-* crates to grok-build workspace

```
grok-build/crates/fb/
├── fb-actor/
├── fb-adapter/
├── fb-binding/
├── fb-boot/
├── fb-build/
├── fb-cli/
├── fb-codegen/
├── fb-config/
├── fb-core/
├── fb-dds/
├── fb-emulator/
├── fb-ffi/
├── fb-fs/
├── fb-macro/
├── fb-packing/
├── fb-parser/
├── fb-plugin/
├── fb-proxy/
├── fb-scheduler/
├── fb-sftp/          ← keep commented out (447 transitive deps)
├── fb-shared/
├── fb-shim/
├── fb-term/
├── fb-tty/
├── fb-vfs/
├── fb-watcher/
└── fb-widgets/
```

#### 2.2 Adapt file browser to grok-build's TUI

The file browser in dx-tui is a full-screen application. In grok-build, it becomes a **view/widget** that renders within the TUI layout.

Key integration points:

| File browser concept | grok-build integration |
|---------------------|----------------------|
| `fb-widgets` rendering | Registered as a view in `views/file_browser/` |
| Lua plugin system | Uses `mlua` — grok-build has no Lua dep currently. Either add `mlua` or gate plugins behind a feature |
| Image preview | Uses image libraries (kitty, sixel, iTerm2) — grok-build already has image support in `xai-grok-pager` via `crate::terminal::image` |
| Key bindings | Map fb keybindings to grok-build's action system or use the fb's own handler |
| VFS/Lua bindings | grok-build's workspace VFS can be bridged to fb's VFS layer |

#### 2.3 File browser as a screen

```
AppView::draw()
  ├── AgentViewMode::FileBrowser → fb_widgets render in fullscreen area
  │     ├── Multi-tab file navigation
  │     ├── File preview pane
  │     └── Status bar (reuse grok's status_bar.rs)
```

---

### Phase 3 — Port Code Editor (6 dx-editor crates)

dx-editor is a **full terminal code editor** (~280K lines src + ~215K tests) with:
- Syntax highlighting (tree-sitter + syntect)
- LSP integration
- Multi-cursor editing
- File tree
- Git integration

#### 3.1 Add dx-editor crates to grok-build workspace

```
grok-build/crates/editor/
├── dx-editor-core/
├── dx-editor/
├── dx-editor-languages/
├── dx-editor-parser-js/
├── dx-editor-plugin-runtime/
├── dx-editor-plugin-api-macros/
└── dx-editor-winterm/
```

#### 3.2 Editor rendering via CaptureBackend

dx-editor uses `CaptureBackend` to render into a ratatui buffer. This pattern works directly in grok-build's TUI — the CaptureBackend writes rendered cells into an area of the TUI buffer.

```rust
// In views/editor/mod.rs
pub struct EditorView {
    editor: dx_editor::Editor,
    capture: dx_editor::winterm::CaptureBackend,
}

impl EditorView {
    pub fn draw(&mut self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        self.capture.resize(area.width as usize, area.height as usize);
        self.editor.render(&mut self.capture);
        // Copy captured cells to buf offset by area position
        for (y, row) in self.capture.rows().enumerate() {
            for (x, cell) in row.cells().enumerate() {
                if let Some(buf_cell) = buf.get_mut(area.x + x as u16, area.y + y as u16) {
                    *buf_cell = cell.clone();
                }
            }
        }
    }
}
```

#### 3.3 Two-runtime pattern

dx-editor has its own tokio runtime (for LSP, file watching, plugins). grok-build's TUI runs on another tokio runtime. This matches the existing pattern documented in `DX.md`:

```rust
// In EditorView::new()
let editor_runtime = tokio::runtime::Runtime::new()?;
let editor = dx_editor::Editor::new_with_runtime(editor_runtime);

// When calling editor methods from the TUI runtime:
tokio::task::block_in_place(|| {
    editor_runtime.enter();
    editor.do_something();
});
```

---

### Phase 4 — Port Animations and Tachyonfx

dx-tui has a rich animation system using `tachyonfx` 0.25 (~3K lines in `animations.rs`).

#### 4.1 Add tachyonfx dependency

```toml
# grok-build/Cargo.toml
tachyonfx = "0.25"
```

This is only possible after the ratatui 0.30 upgrade (Phase 0), since tachyonfx 0.25 depends on ratatui 0.30.

#### 4.2 Port animation modules

| dx-tui animation | Target | Notes |
|------------------|--------|-------|
| Loading spinners | `views/message_screen/animations.rs` | Used during agent streaming |
| Confetti | `views/message_screen/` | Used on turn completion (opt-in) |
| Matrix rain | Optional decoration | Easter egg |
| Starfield, Fire, Plasma | Optional decoration | Screen savers |

#### 4.3 Animation integration

```rust
// In MessageScreenView, during agent response streaming:
fn draw_streaming_indicator(&self, area: Rect, buf: &mut Buffer) {
    let elapsed = self.stream_start.elapsed();
    let spinner = SPRITE_FRAMES[elapsed.as_millis() as usize % SPRITE_FRAMES.len()];
    // Render spinner character at cursor position
}
```

---

### Phase 5 — View Switching & Integration

#### 5.1 View selection

Add a `Views` panel or quick-switch mechanism:

| Keybinding | Action |
|-----------|--------|
| `Ctrl+1` | Message screen (default) |
| `Ctrl+2` | Scrollback view (traditional) |
| `Ctrl+3` | File browser |
| `Ctrl+4` | Code editor |

Or via slash-commands:
```
/view message     → message screen
/view scrollback  → traditional scrollback
/view browser     → file browser
/view editor      → code editor
```

#### 5.2 Shared components

All four views share grok-build's existing infrastructure:

| Component | Source | Used by |
|-----------|--------|---------|
| Status bar | `views/status_bar.rs` | All views |
| Shortcuts bar | `views/shortcuts_bar.rs` | All views |
| Modal system | `views/modal_window.rs` | All views |
| Theme system | `theme/` | All views |
| Notification system | `notifications/` | All views |
| Input/prompt | `input/` + `views/prompt_widget/` | Message screen + scrollback |
| Overlay system | `views/overlay.rs` | All views |

#### 5.3 View persistence

When switching views, the state of each view persists:

```rust
pub struct AppView {
    // Existing state
    pub scrollback_view: ScrollbackState,
    pub agent_view: AgentView,

    // New state
    pub message_screen: MessageScreenView,
    pub file_browser: FileBrowserView,
    pub editor: EditorView,

    // Active selection
    pub active_view: AgentViewMode,
}
```

---

## 6. Files to Port from dx-tui

### 6.1 Message screen core (~5,000 LOC from dx-tui `msg_ui/`)

| dx-tui source | grok-build target | Lines | Adaptations needed |
|---------------|-------------------|:-----:|--------------------|
| `src/msg_ui/parse.rs` | `views/message_screen/parse.rs` | 600 | Replace dx-tui types with grok types |
| `src/msg_ui/render.rs` | `views/message_screen/render.rs` | 800 | Replace `ChatState` refs with `MessageScreenView` |
| `src/msg_ui/live.rs` | `views/message_screen/live.rs` | 500 | Adapt to ACP event stream |
| `src/msg_ui/branch_ui.rs` | `views/message_screen/branch_ui.rs` | 300 | Minimal changes |
| `src/msg_ui/pty_host.rs` | `views/message_screen/pty_host.rs` | 400 | Uses `portable-pty` — already in grok-build's deps |
| `src/msg_ui/vt_grid.rs` | `views/message_screen/vt_grid.rs` | 300 | Pure rendering, minimal changes |
| `src/msg_ui/diff_review.rs` | `views/message_screen/diff_review.rs` | 500 | Adapt to grok's diff format |
| `src/msg_ui/ansi.rs` | `views/message_screen/ansi.rs` | 200 | Already uses `ansi-to-tui` |
| `src/msg_ui/copy.rs` | `views/message_screen/copy.rs` | 100 | Clipboard integration |
| `src/animations.rs` | `views/message_screen/animations.rs` | 1,000 | Port animation constants + tachyonfx effects |

### 6.2 File browser (~47,000 LOC from dx-tui `fb-*` crates)

| dx-tui crate | grok-build target | Lines | Dependencies added |
|--------------|-------------------|:-----:|-------------------|
| `fb-actor` | `crates/fb/fb-actor` | 500 | tokio |
| `fb-adapter` | `crates/fb/fb-adapter` | 1,000 | image, ratatui |
| `fb-binding` | `crates/fb/fb-binding` | 500 | mlua |
| `fb-boot` | `crates/fb/fb-boot` | 300 | tokio |
| `fb-build` | `crates/fb/fb-build` | 200 | build-time only |
| `fb-cli` | `crates/fb/fb-cli` | 100 | clap |
| `fb-codegen` | `crates/fb/fb-codegen` | 200 | proc-macro2 |
| `fb-config` | `crates/fb/fb-config` | 500 | serde |
| `fb-core` | `crates/fb/fb-core` | 8,000 | tokio, futures |
| `fb-dds` | `crates/fb/fb-dds` | 500 | tokio |
| `fb-emulator` | `crates/fb/fb-emulator` | 500 | term |
| `fb-ffi` | `crates/fb/fb-ffi` | 200 | libc |
| `fb-fs` | `crates/fb/fb-fs` | 4,000 | tokio, notify |
| `fb-macro` | `crates/fb/fb-macro` | 100 | proc-macro |
| `fb-packing` | `crates/fb/fb-packing` | 200 | serde |
| `fb-parser` | `crates/fb/fb-parser` | 500 | nom |
| `fb-plugin` | `crates/fb/fb-plugin` | 2,000 | mlua |
| `fb-proxy` | `crates/fb/fb-proxy` | 300 | tokio |
| `fb-scheduler` | `crates/fb/fb-scheduler` | 1,000 | tokio |
| `fb-shared` | `crates/fb/fb-shared` | 3,000 | serde, parking_lot |
| `fb-shim` | `crates/fb/fb-shim` | 100 | — |
| `fb-term` | `crates/fb/fb-term` | 500 | crossterm |
| `fb-tty` | `crates/fb/fb-tty` | 300 | nix, windows-sys |
| `fb-vfs` | `crates/fb/fb-vfs` | 3,000 | tokio, async-fs |
| `fb-watcher` | `crates/fb/fb-watcher` | 500 | notify |
| `fb-widgets` | `crates/fb/fb-widgets` | 15,000 | ratatui, tachyonfx |

### 6.3 Code editor (~280,000 LOC from dx-tui `dx-editor`)

| dx-tui crate | grok-build target | Lines | Notes |
|--------------|-------------------|:-----:|-------|
| `dx-editor-core` | `crates/editor/core` | 40,000 | Core editor types, buffer management |
| `dx-editor` | `crates/editor/editor` | 120,000 | Main editor crate |
| `dx-editor-languages` | `crates/editor/languages` | 50,000 | Tree-sitter grammars, LSP clients |
| `dx-editor-parser-js` | `crates/editor/parser-js` | 20,000 | JavaScript parser (optional) |
| `dx-editor-plugin-runtime` | `crates/editor/plugin-runtime` | 30,000 | Plugin system |
| `dx-editor-plugin-api-macros` | `crates/editor/plugin-api-macros` | 5,000 | proc macros |
| `dx-editor-winterm` | `crates/editor/winterm` | 15,000 | Winterm terminal backend |

---

## 7. What to Leave Behind (Not Ported)

These dx-tui features are **not ported** because grok-build already has equivalents:

| dx-tui feature | grok-build equivalent | Reason |
|----------------|----------------------|--------|
| `codex_bridge.rs` | ACP connection via `xai-acp-lib` | grok-build already talks ACP natively |
| `codex-rs` backend crates | `xai-grok-shell` agent | Different agent implementations |
| `zen.rs`, `agent_backend.rs` | `xai_grok_shell::agent` | Native grok agent is the target |
| `provider_registry.rs` | `xai-grok-config` + models | grok has its model system |
| `session_db.rs` | `xai_grok_shell::session` | grok's session persistence |
| `permission_hub.rs` | grok's permission system | Already in shell/workspace |
| `tui/event_stream.rs` | grok's `input/` + event loop | grok has mature event handling |
| `tui/frame_rate_limiter.rs` | grok's render timing | Already in pager-render |
| `keymap_setup/` | grok's `actions/` + keybindings | Different action system |
| `onboarding/` | grok's welcome view | Uses `views/welcome/` |

---

## 8. Feature Gating

```toml
# xai-grok-pager/Cargo.toml
[features]
default = ["message-screen"]

# View modes
message-screen = ["dep:msg-ui", "dep:dx-animations", "dep:tachyonfx"]
file-browser = ["dep:fb-core", "dep:fb-widgets", "dep:fb-fs", "dep:mlua"]
editor = ["dep:dx-editor", "dep:dx-editor-winterm"]
scrollback = []  # always available

# Optional enhancements
editor-languages = ["editor", "dep:dx-editor-languages"]
editor-plugins = ["editor", "dep:dx-editor-plugin-runtime"]
file-browser-images = ["file-browser", "dep:image", "dep:resvg"]
file-browser-sftp = ["file-browser", "dep:russh"]  # optional SSH/SFTP
```

---

## 9. Key Risks & Mitigations

| Risk | Severity | Impact | Mitigation |
|------|----------|--------|------------|
| **ratatui 0.29→0.30 in `xai-ratatui-inline`** | **Critical** | Blocks all other work — the core terminal backend | Audit first. If 0.30 break, fallback to leader-mode IPC |
| **File browser + editor complexity** | High | ~327K LOC to port and maintain | Feature-gate each; port incrementally. Message screen first, file browser second, editor last |
| **Two tokio runtimes** (editor + TUI) | Medium | Complex blocking patterns needed | Use `block_in_place` + `runtime.enter()` pattern from dx-tui |
| **mlua dependency** | Medium | Adds Lua VM to grok-build | Gate behind `file-browser` feature; keep optional |
| **Build time increase** | Medium | ~120+ workspace crates | Use `--features` to compile only needed crates |
| **Windows compatibility** | Medium | File browser + editor need Windows testing | grok-build already supports Windows (with protoc workaround) |
| **License** | Low | MIT + Apache 2.0 | Compatible |

---

## 10. Implementation Order

### Step 1 (Phase 0): ratatui 0.30 upgrade
- [ ] Audit `xai-ratatui-inline` API surface
- [ ] Bump all grok-build TUI crates to ratatui 0.30 + crossterm 0.29
- [ ] Fix compilation errors
- [ ] Verify existing tests pass
- **Decision gate**: If this fails, switch to leader-mode fallback

### Step 2: Message screen (the default)
- [ ] Create `views/message_screen/` with 10 files
- [ ] Port `msg_ui/parse.rs` (StreamPart, PlanStep types)
- [ ] Port `msg_ui/render.rs` (MessageScreenView draw logic)
- [ ] Port `msg_ui/live.rs` (streaming append)
- [ ] Port `msg_ui/diff_review.rs` (diff accept/reject)
- [ ] Port `msg_ui/pty_host.rs` (interactive PTY sessions)
- [ ] Wire into `AppView` as `AgentViewMode::MessageScreen`
- [ ] Set as default view mode
- **Deliverable**: grok session renders via message screen instead of scrollback

### Step 3: File browser
- [ ] Copy 26 `fb-*` crates into `crates/fb/`
- [ ] Create `views/file_browser/` wrapper
- [ ] Register as `AgentViewMode::FileBrowser`
- [ ] Wire keybindings
- **Deliverable**: `/view browser` opens file browser

### Step 4: Code editor
- [ ] Copy 7 `dx-editor` crates into `crates/editor/`
- [ ] Create `views/editor/` with CaptureBackend adapter
- [ ] Register as `AgentViewMode::Editor`
- [ ] Wire two-runtime pattern
- **Deliverable**: `/view editor` opens code editor

### Step 5: Animations & polish
- [ ] Port `animations.rs` (spinners, effects)
- [ ] Add tachyonfx effects to message screen transitions
- [ ] Theme integration
- **Deliverable**: Smooth animations during message streaming

---

## 11. Estimated Timeline

| Step | What | Lines | Est. time |
|:----:|------|:-----:|:---------:|
| 0 | ratatui 0.30 audit + upgrade | patch | **4-8 hr** |
| 1 | Message screen (msg_ui port) | ~5,000 | **16-24 hr** |
| 2 | File browser (fb-* copy + adapt) | ~47,000 | **16-24 hr** |
| 3 | Code editor (dx-editor copy + adapt) | ~280,000 | **24-40 hr** |
| 4 | Animations + polish | ~3,000 | **8-12 hr** |
| **Testing** | Build, compile, smoke test | — | **16-24 hr** |
| **Total** | | **~335,000** | **~84-132 hr** |

---

## 12. Getting Started

1. **Phase 0 first**: `cd crates/codegen/xai-ratatui-inline && cargo check` — then bump to ratatui 0.30 and see what breaks
2. **If Phase 0 passes**: Start Step 1 — port `msg_ui/parse.rs` as the first file; it has zero grok-build dependencies and establishes the types
3. **Parallel work possible**: Steps 2 (file browser) and 3 (editor) can proceed in parallel once the crate structure is set up
4. **Testing**: Each step should compile and pass existing tests before moving to the next

The message screen is the **core deliverable**. File browser and editor are valuable but independent additions that can be deferred.
