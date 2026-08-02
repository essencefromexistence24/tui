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

---

## 10. Video playback command (`/video`)

### Implementation status — complete

- [x] `/video <path>` is registered in Grok's live slash-command registry and help/autocomplete metadata.
- [x] Raw quoted arguments are preserved and relative paths resolve from the active session workspace.
- [x] Media paths are canonicalized and checked for existence, file type, and readability.
- [x] `DX_VIDEO_PLAYER`, per-user installation, and debug artifact resolution are implemented in that order.
- [x] The player is launched without a shell, with null stdio, its runtime directory as the working directory, and no console-detach flag that can suppress GUI startup.
- [x] Windows launch success requires a visible player window; early exit or a three-second headless startup returns an error instead of a false `Playing` toast.
- [x] The Windows per-user installer verifies and atomically installs the working player plus its 113-DLL recursive runtime manifest under `%LOCALAPPDATA%\Programs\DX\Video`.
- [x] Unit tests cover registration, case-insensitive dispatch, quoted/relative paths, failures, runtime files, and resolver precedence.
- [x] The installed x64 player was smoke-tested with `G:\Dx\hexxed\terminal\dx-video-player\titanic.mp4`; the resulting visible window was verified by its nonzero `HWND` and `titanic.mp4 - dx` title.

### 10.1 Goal and non-goals

Add a TUI slash command that launches the DX video player for a user-supplied
video path, including videos produced by grok-build:

```text
/video "C:\path\to\video.mp4"
```

The video player opens in its own native window. It must not attempt to embed a
Windows `HWND` into the Ratatui terminal buffer. A terminal TUI is a
character-cell application, and embedding a native window is not portable
across Windows Terminal, ConPTY, SSH, tmux, Linux terminals, or macOS terminals.

Terminal-native frame rendering through Kitty, Sixel, or ANSI escape sequences
is explicitly out of scope for this feature. Those protocols are optional and
terminal-dependent; they are not a replacement for the DX video player's native
window.

### 10.2 Existing integration points

The slash-command and dispatch implementation is split across:

```text
crates\codegen\xai-grok-pager\src\slash\commands\video.rs
crates\codegen\xai-grok-pager\src\slash\commands\mod.rs
crates\codegen\xai-grok-pager\src\app\dispatch\router.rs
crates\codegen\xai-grok-pager\src\video_player.rs
```

The implementation must update all of these in the same change:

1. Register `VideoCommand` in the built-in registry so autocomplete and `/help` show it.
2. Preserve argument text in `execute()` so paths containing spaces can be parsed.
3. Dispatch a typed `Action::PlayVideo` through the application router.
4. Delegate process and path handling to the video-player module instead of
   putting filesystem and process-launching details in the command matcher.

The command performs a bounded startup handshake and returns as soon as the
native window is visible (normally well below one second). It must not take
ownership of the terminal's raw mode, alternate screen, stdin, stdout, or stderr.

### 10.3 Command syntax and path rules

Support these forms:

```text
/video "C:\Users\me\Videos\demo.mp4"
/video C:\Videos\demo.mp4
/video .\output\render.mp4
```

Rules:

- Require exactly one path argument for the first implementation.
- Remove one matching pair of surrounding quotes and preserve spaces inside it.
- Resolve relative paths against the active Grok workspace/current directory.
- Canonicalize the path before launching it.
- Reject missing paths, directories, and unreadable files with a TUI toast.
- Do not invoke `cmd.exe`, PowerShell, a shell string, or shell interpolation.
  Pass the executable and video path as separate `Command` arguments.
- Keep the command case-insensitive and ensure normal non-slash messages are
  unaffected.

Recommended user-facing errors:

```text
Usage: /video <path>
Video file not found: <path>
Video path is a directory: <path>
DX video player is not installed. Run the DX installation/update command.
```

### 10.4 Player installation layout

Windows should use a per-user installation, not `C:\Program Files`, so the
feature works without administrator rights:

```text
%LOCALAPPDATA%\Programs\DX\Video\
    dx-video-player.exe
    runtime-manifest.txt
    avcodec-62.dll
    avformat-62.dll
    avutil-60.dll
    libass-9.dll
    libplacebo-360.dll
    ... all recursively required runtime DLLs
```

`dirs::data_local_dir()` is the preferred Rust API for resolving
`%LOCALAPPDATA%`; do not hardcode a username or assume that the environment is
always installed on `C:`. The final package must include every DLL listed in
the checked-in recursive runtime manifest next to the executable.

Executable resolution order should be:

1. An explicit `DX_VIDEO_PLAYER` environment-variable override, useful for
   development and testing.
2. The installed per-user path above.
3. A development fallback only when running an x64 debug build, using the
   verified working artifact under `G:\Dx\hexxed\terminal\dx-video-player`.

The production player should be copied from a release artifact during the DX
installation/update flow. Do not download or execute an unverified binary
silently from inside `/video`.

For Windows architecture selection, ship the x86_64 player for normal x64
systems and an ARM64 player for native ARM64 systems. The resolver should
select the matching installed artifact and report a clear unsupported-platform
error if no matching player exists.

### 10.5 Process-launch behavior

Implement a `VideoPlayer` helper with responsibilities limited to:

- Resolve the installed executable.
- Validate and canonicalize the media path.
- Spawn the player with one path argument.
- Give the player null standard handles and its own runtime directory as the
  working directory without applying Windows console-detach creation flags.
- Verify that the process creates a visible native window within three seconds.
- Return a structured error for missing executable, missing DLLs, spawn
  failure, early process exit, or headless startup.

The player should open independently while the TUI remains usable. A future
optional `/video --wait <path>` mode may wait for the player, but it should not
be part of the initial command because waiting can interfere with TUI input and
terminal restoration.

### 10.6 Grok-build generated videos

The first version only needs the explicit path form:

```text
/video "<grok-build-output-path>"
```

To make generated videos discoverable later, add a small video-artifact record
to the Grok task/session state containing:

- canonical output path;
- creation time;
- producing task/session identifier;
- optional title and duration;
- existence/readability status.

Then add an optional convenience form such as:

```text
/video last
```

`last` must resolve only through the recorded artifact registry. It must not
scan arbitrary directories or execute a path supplied by model text without
the same validation used by an explicit path.

### 10.7 Testing and acceptance criteria

Unit tests:

- `/video` appears in autocomplete and help output.
- Quoted Windows paths with spaces resolve correctly.
- Relative paths resolve against the active workspace.
- Missing files and directories produce errors without spawning a process.
- Normal chat input containing `/video` in the middle of text remains normal
  chat input.
- Executable resolution honors `DX_VIDEO_PLAYER` before the installed path.

Windows integration/smoke tests:

- Install the player and FFmpeg DLLs under `%LOCALAPPDATA%\Programs\DX\Video`.
- Run `/video "G:\Dx\hexxed\terminal\dx-video-player\titanic.mp4"`.
- Confirm the native player window opens and the TUI remains responsive.
- Confirm paths containing spaces work.
- Confirm a missing player and a missing media file show actionable toasts.
- Confirm the player process does not inherit or corrupt the TUI's raw mode and
  alternate screen.

Cross-terminal acceptance:

- Windows Terminal, ConHost, VS Code terminal, and an SSH/remote terminal may
  differ in their TUI rendering, but all should be able to request the same
  separate native player window when a local GUI session is available.
- A remote/headless session cannot display a local native window; return a
  clear error or document that the player must run on the local machine.

### 10.8 Implementation order

1. Add the player resolver/launcher module and path validation.
2. Add `/video` to the slash-command specification, resolver, handler, help,
   and autocomplete.
3. Add the per-user player package/install/update layout.
4. Add explicit-path support for Grok-generated video outputs.
5. Add unit tests and Windows smoke tests.
6. Add the optional `last` artifact lookup only after explicit-path playback is
   stable.

### 10.9 Definition of done

- `/video "<path>"` launches the installed DX video player in a separate native
  window.
- The TUI remains responsive and its terminal state is preserved.
- The player and all required FFmpeg DLLs are available from the per-user
  `%LOCALAPPDATA%\Programs\DX\Video` installation.
- Quoted paths, relative paths, generated-output paths, and failure cases are
  tested.
- Documentation clearly states that embedded playback inside the terminal is
  not supported and that separate-window playback is intentional.
