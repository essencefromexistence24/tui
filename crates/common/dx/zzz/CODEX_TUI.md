# Codex TUI — Complete Forensic Analysis

> Analysis of the entire `codex-rs` workspace as it relates to TUI vs Backend boundaries.
> Goal: Understand exactly what to extract vs what to inherit by depending on backend crates.

---

## 1. TUI vs Backend: The Big Picture

The `codex-rs/tui` crate (912 files, 205,858 lines) is **just a UI shell** sitting on top of ~45 backend crates. The backend crates contain ALL the AI logic.

```
codex-rs WORKSPACE MAP
========================

BACKEND (what you inherit by depending on codex-app-server + codex-app-server-client):
├── codex-core              (the AI engine — threads, LLM, sessions, tools, MCP, sandbox)
├── codex-app-server        (JSON-RPC server wrapping core)
├── codex-app-server-client (unified client: in-process or remote)
├── codex-login             (OAuth, API keys, token management)
├── codex-config            (multi-layer config, constraints, validation)
├── codex-model-provider    (runtime model provider abstraction + auth)
├── codex-model-provider-info (provider definitions, bundled catalog of 25+ providers)
├── codex-models-manager    (model discovery, caching, metadata)
├── codex-exec-server       (tool execution, sandboxing, filesystem ops)
├── codex-protocol          (shared types: ThreadId, AuthMode, models, permissions)
├── codex-app-server-protocol (JSON-RPC types: ServerNotification, ClientRequest, etc.)
├── codex-rollout           (session persistence via SQLite — StateDbHandle)
├── codex-features          (feature flags)
├── codex-otel              (OpenTelemetry tracing)
├── codex-mcp               (Model Context Protocol integration)
├── codex-core-skills       (skill definitions)
├── codex-core-plugins      (plugin system)
├── codex-file-search       (file search)
├── codex-git-utils         (git operations)
├── codex-shell-command     (shell command parsing)
├── codex-sandboxing        (sandbox abstraction)
├── codex-state             (state management)
├── + 20+ utility crates    (absolute-path, cli, fuzzy-match, home-dir, etc.)
└── (46 total codex-* deps in tui/Cargo.toml)

TUI (what you DROP — replace with dx-tui):
├── bottom_pane/            (input bar, composer, popups — 37 files, 39K lines)
├── chatwidget/             (state machine — 59+23 files, 44K lines)
├── app/                    (event loop, routing — 30+5 files, 21K lines)
└── 70+ other TUI-specific files (keymaps, onboarding, pets, status, etc.)

RENDERING (what you KEEP — extract into codex-tui-render crate):
├── history_cell/           (cell trait + implementations — 15 files, 7K lines)
├── render/                 (layout engine, syntax highlighting — 5 files, 2K lines)
├── streaming/              (streaming state machine — 5 files, 2.6K lines)
├── markdown_render.rs      (pulldown-cmark → ratatui Lines — 1 file, 2.5K lines)
├── diff_render.rs          (diff rendering — 1 file, 2.3K lines)
├── + 17 support files      (style, color, wrapping, hyperlinks, etc. — ~6K lines)
└── TOTAL: ~44 files, ~22K lines
```

---

## 2. Where Features ACTUALLY Live

### AI Login & Auth — **Backend crate: `codex-login`**

| Auth Feature | Where it lives | Do you get it for free? |
|-------------|----------------|------------------------|
| OAuth device code flow (browser) | `codex-login/src/server.rs` | ✅ Yes — call `run_login_server()` |
| OAuth device code flow (CLI) | `codex-login/src/device_code_auth.rs` | ✅ Yes — call `run_device_code_login()` |
| API key auth | `codex-login/src/auth/manager.rs` | ✅ Yes — env var detection + login |
| PAT (Personal Access Token) | `codex-login/src/auth/personal_access_token.rs` | ✅ Yes |
| Agent Identity JWT | `codex-login/src/auth/agent_identity.rs` | ✅ Yes |
| Kimi Code OAuth | `codex-login/src/kimi_code.rs` | ✅ Yes |
| Amazon Bedrock API key | `codex-login/src/auth/bedrock_api_key.rs` | ✅ Yes |
| Token persistence (file/keyring) | `codex-login/src/auth/storage.rs` | ✅ Yes — `AuthManager` handles this |
| Token refresh | `codex-login/src/auth/manager.rs` | ✅ Yes — proactive refresh built-in |
| 401 Unauthorized recovery | `codex-login` exports `UnauthorizedRecovery` | ✅ Yes |
| Auth env var detection | `codex-login/src/auth_env_telemetry.rs` | ✅ Yes |

**What the TUI adds on top (irrelevant to you):**
- Kimi Code OAuth UI (`tui/src/chatwidget/model_popups.rs` — drives the flow with user prompts)
- Login/account status display in the footer
- Onboarding auth screen (`tui/src/onboarding/auth.rs`)

**Bottom line:** Auth is 100% backend. The TUI uses ~5 tiny entry points. If you depend on `codex-app-server`, its `AuthManager` singleton handles everything. You don't need to write any auth code.

---

### Model Providers & Connections — **Backend crates: `codex-model-provider`, `codex-model-provider-info`, `codex-models-manager`**

| Model Feature | Where it lives | Do you get it for free? |
|--------------|----------------|------------------------|
| Provider definitions (OpenAI, Anthropic, DeepSeek, etc.) | `codex-model-provider-info` | ✅ Yes |
| Provider catalog JSON (25+ bundled providers) | `codex-model-provider-info/src/provider_catalog.json` (37K lines) | ✅ Yes |
| Runtime provider abstraction | `codex-model-provider` (`ModelProvider` trait) | ✅ Yes |
| API client construction + auth header resolution | `codex-model-provider/src/auth.rs` | ✅ Yes |
| Model discovery from `/models` endpoint | `codex-models-manager` (`OpenAiModelsManager`) | ✅ Yes |
| Model cache with TTL | `codex-models-manager` | ✅ Yes |
| Model metadata / compatibility enrichment | `codex-models-manager/src/compatibility_enrichment.rs` | ✅ Yes |
| Model list filtering by auth mode / visibility | `codex-models-manager` | ✅ Yes |
| LLM client (Responses API, Chat API, Anthropic) | `codex-core/src/client.rs` | ✅ Yes |
| Session prompt loop | `codex-core/src/codex_thread.rs` | ✅ Yes |
| Tool calling loop | `codex-core` (session/turn processing) | ✅ Yes |

**What the TUI adds on top (irrelevant to you):**
- Model selection popup UI (`chatwidget/model_popups.rs`)
- Model migration prompt (`model_migration.rs`)
- Provider list display with descriptions
- LoadProviderModels event dispatch

**Bottom line:** The entire model connection system is in the backend. You get all providers, all API client construction, model discovery, caching, and LLM inference for free. The TUI only adds the selection UI.

---

### "Free Models" — **Not a feature, here's the truth**

There is **no first-class "free models" concept** in codex-rs. What exists:

1. **ChatGPT Free Plan** — A plan tier (`KnownPlan::Free` from `codex-protocol`). Used for:
   - Targeting announcements to free-tier users
   - Filtering model availability by auth mode

2. **Local/OSS providers** — Providers like Ollama, LM Studio have `requires_openai_auth: false` and no API key requirement. These are effectively "free" because they run locally.

3. **How it works:**
   - Provider definitions in `codex-model-provider-info` have `requires_openai_auth` flag
   - If you set up an Ollama or LM Studio provider in config, it works without any login
   - The TUI shows "No API key required" in provider descriptions for these

4. **Model availability logic** — `ModelsManager::build_available_models()` filters models based on:
   - `uses_codex_backend` (codex-managed vs direct API key)
   - Auth mode (ChatGPT signed-in, API key, etc.)
   - Visibility flags

**Bottom line:** Free/local models work because the provider config says they don't need auth. This is all in the backend crates — you get it automatically.

---

### Other Backend Features (inherited for free)

| Feature | Backend Crate | What you get |
|---------|--------------|-------------|
| Thread/Session Management | `codex-core` | Create, resume, fork, archive threads. Turn submission, interruption, rollback. |
| Context Compaction | `codex-core/src/compact.rs` | Automatic context window management with model fallback |
| MCP Server Integration | `codex-core/src/mcp.rs` | Dynamic tool registration from MCP servers |
| Skills System | `codex-core/src/skills.rs` | Built-in + custom skills, file watching for changes |
| Plugin System | `codex-core/src/plugins/` | App/plugin marketplace, install/uninstall/list |
| Web Search | `codex-core/src/web_search.rs` | Built-in web search tool |
| Safety/Guardian | `codex-core/src/guardian/` | Content safety checks |
| Sandboxed Execution | `codex-sandboxing` | Windows, Linux landlock, macOS sandbox |
| Tool Execution | `codex-exec-server` | Shell commands, file operations, process management |
| Multi-Agent | `codex-core/src/agent/` | Delegation, sub-agents, collaboration |
| Image/Vision | `codex-core/src/image_preparation.rs` | Image processing for vision models |
| Realtime Voice | `codex-core/src/realtime_*.rs` | Voice conversation support |
| Config Loading | `codex-config` | Multi-layer config (defaults + user + project + cloud + CLI) |
| Config Requirements | `codex-config/src/config_requirements.rs` | Security policy constraints |
| State Persistence | `codex-rollout` | SQLite-backed session state, thread store |

### What the TUI Adds (what you DON'T need to reimplement)

| TUI Module | Function | Backend Equivalent? |
|-----------|----------|-------------------|
| `app_server_session.rs` (2,762 lines) | Typed RPC wrappers | ❌ No — you use `AppServerRequestHandle` directly |
| `app/event_dispatch.rs` (2,779 lines) | Event routing | ❌ No — dx-tui has its own event loop |
| `chatwidget.rs` + `chatwidget/` (19,852 lines) | Chat state machine | ❌ No — replace with thin bridge |
| `bottom_pane/` (39,081 lines) | Input bar + popups | ❌ No — dx-tui has its own |
| `keymap.rs` (2,768 lines) | Keybinding config | ❌ No — dx-tui has keybindings |
| `resume_picker.rs` (5,776 lines) | Session list UI | ❌ No — build your own or skip |
| `onboarding/` (2,742 lines) | First-run wizard | ❌ No — dx-tui has its own |
| `pets/` (3,489 lines) | Pet companion | ❌ No — drop |
| `status/` (3,797 lines) | Status indicators | ❌ No — dx-tui has its own |
| `model_popups.rs` (in chatwidget) | Model selection UI | ✅ Yes — you build a thin popup using `ModelListParams` RPC |
| `model_catalog.rs` | Model list display | ✅ Yes — same |

---

## 3. The In-Process Backend Architecture

```
dx-tui binary (single process)
│
├── CodexIntegration (your bridge, ~500-800 lines)
│   ├── Calls codex-app-server-client::AppServerClient
│   │   (either InProcessAppServerClient or RemoteAppServerClient)
│   │
│   └── Processes AppServerEvent stream:
│       ├── ServerNotification → update your local cell state
│       │   ├── ContentDelta → MarkdownStreamCollector → StreamState
│       │   ├── ToolCallStarted → create ExecCell in active cell
│       │   ├── ItemCompleted → consolidate to committed HistoryCell
│       │   └── TurnCompleted → finalize
│       ├── ServerRequest → show approval/confirmation UIs
│       └── Disconnected → reconnect or show error
│
├── codex-tui-render crate (extracted, ~44 files, ~22K lines)
│   └── HistoryCell::display_lines(width) → Vec<Line<'static>>
│
├── codex-app-server-client crate ✅ (backend, inherited)
│   └── AppServerClient, AppServerRequestHandle, AppServerEvent
│
├── codex-app-server crate ✅ (backend, inherited)
│   └── InProcessClientHandle, config manager, all request processors
│
├── codex-core crate ✅ (backend, inherited)
│   └── Full AI engine — threads, LLM client, sessions, MCP, tools
│
├── codex-login crate ✅ (backend, inherited)
│   └── AuthManager, all login flows, token management
│
├── codex-model-provider + info + models-manager ✅ (backend, inherited)
│   └── Model providers, discovery, LLM connection
│
├── codex-exec-server crate ✅ (backend, inherited)
│   └── Tool execution, sandboxing, filesystem
│
├── codex-config crate ✅ (backend, inherited)
│   └── Config loading, constraints, validation
│
└── + 30+ other codex-* crates ✅ (inherited automatically as deps)
```

### Startup sequence in dx-tui (what you'd add to your main.rs/lib.rs)

```
1. Load config (codex-config)
   ├── Defaults → user config.toml → project config → cloud config → CLI overrides
   └── Validate constraints (sandbox mode, network policy, etc.)

2. Init auth (codex-login via codex-app-server)
   ├── AuthManager::shared_from_config()
   ├── Detect env vars (CODEX_API_KEY, CODEX_ACCESS_TOKEN, OPENAI_API_KEY)
   ├── Load persisted auth.json
   └── Proactive token refresh

3. Start exec-server environment (codex-exec-server)
   ├── EnvironmentManager::from_codex_home()
   └── Resolve runtime paths

4. Start app-server (codex-app-server via codex-app-server-client)
   ├── InProcessAppServerClient::start(InProcessClientStartArgs { ... })
   ├── Creates InProcessClientHandle
   └── Returns AppServerClient enum

5. Create AppServerRequestHandle for all RPC calls
   └── thread_start, thread_list, model_list, config_read, etc.

6. Bootstrap session (via RPC)
   ├── Create or resume thread
   └── Subscribe to AppServerEvent stream

7. Enter dx-tui event loop with AppServerEvent multiplexed in

8. On user input:
   └── app_server.request_handle().request_typed(ClientRequest::UserTurn { ... })

9. On AppServerEvent::Notification:
   ├── ContentDelta → feed to MarkdownStreamCollector
   ├── ItemCompleted → consolidate HistoryCell
   └── TurnCompleted → signal done

10. On render frame:
    └── CodexIntegration::render(area, buf) → HistoryCell::display_lines()
```

---

## 4. What You Actually Need to Write (~2,000-3,000 lines total)

### New files in dx-tui

| File | Lines | Purpose |
|------|-------|---------|
| `src/codex_integration/mod.rs` | ~400 | Main bridge: holds `AppServerClient`, `AppServerRequestHandle`, thread ID, transcript cells, active cell |
| `src/codex_integration/handler.rs` | ~300 | `ServerNotification` → state mutations (ContentDelta, ToolCallStarted, ItemCompleted, TurnCompleted) |
| `src/codex_integration/render.rs` | ~200 | Renders committed cells + active cell via `HistoryCell::display_lines()` |
| `src/codex_integration/startup.rs` | ~300 | Config load, auth init, exec-server init, app-server start, session bootstrap |
| `src/codex_integration/rpc.rs` | ~200 | Typed wrappers for `submit_turn`, `interrupt`, `rollback`, `fork`, `list_models` (or use raw `AppServerRequestHandle`) |
| `src/chat_render_mod.rs` (modify) | ~200 | Add codex mode: if `cx.codex_mode`, delegate to `CodexIntegration::render()` |
| `src/input.rs` (modify) | ~50 | On submit: if `codex_mode`, call `codex.submit_turn(text)` |
| `src/lib.rs` (modify) | ~300 | Add startup wiring |

### Extracted crate: `codex-tui-render` (~44 files, ~22K lines)

This is the rendering kernel extracted from `codex-rs/tui/src`. Already detailed in section 6 below.

### Cargo.toml additions

```toml
[dependencies]
# Codex backend (core engine)
codex-app-server = { path = "../codex-rs/app-server" }
codex-app-server-client = { path = "../codex-rs/app-server-client" }
codex-core = { path = "../codex-rs/core" }
codex-login = { path = "../codex-rs/login" }
codex-config = { path = "../codex-rs/config" }
codex-exec-server = { path = "../codex-rs/exec-server" }

# Codex protocol types
codex-app-server-protocol = { path = "../codex-rs/app-server-protocol" }
codex-protocol = { path = "../codex-rs/protocol" }

# Model providers (for model listing/selection UI)
codex-model-provider = { path = "../codex-rs/model-provider" }
codex-model-provider-info = { path = "../codex-rs/model-provider-info" }
codex-models-manager = { path = "../codex-rs/models-manager" }

# Extracted rendering
codex-tui-render = { path = "../codex-tui-render" }
```

---

## 5. What You Inherit AUTOMATICALLY from Backend Crates

### From `codex-app-server-client`

The single most important crate for integration. Provides:

- **`AppServerClient` enum** — `InProcessAppServerClient | RemoteAppServerClient` — switch with one line
- **`AppServerRequestHandle`** — cloneable handle to make RPC calls from anywhere:
  ```rust
  let response = handle.request_typed::<ModelListResponse>(
      ClientRequest::ModelList { ... }
  ).await?;
  ```
- **`AppServerEvent` stream** — `ServerNotification` + `ServerRequest` + `Disconnected` + `Lagged`:
  ```rust
  loop {
      match client.next_event().await {
          AppServerEvent::ServerNotification(notif) => handle_notification(notif),
          AppServerEvent::ServerRequest(req) => show_approval_ui(req),
          AppServerEvent::Disconnected => reconnect(),
          AppServerEvent::Lagged(n) => warn!("skipped {n} events"),
      }
  }
  ```
- **`InProcessAppServerClient::start(args)`** — boots the entire backend in your process:
  ```rust
  let client = InProcessAppServerClient::start(InProcessClientStartArgs {
      config: my_config,
      state: my_state_db,
      environment_manager: my_env,
      capabilities: my_caps,
      ..Default::default()
  }).await?;
  ```
- **Lossless event delivery** — critical events (ContentDelta, ItemCompleted, TurnCompleted) never dropped
- **Graceful shutdown** — drains active turns before exit

### From `codex-app-server` (transitive dependency via client)

- All JSON-RPC request processors:
  - `thread/create`, `thread/read`, `thread/update`, `thread/list`, `thread/archive`, `thread/delete`, `thread/fork`, `thread/rollback`, `thread/compact`
  - `turn/submit`, `turn/interrupt`, `turn/approve`
  - `model/list`, `model/list-presets`
  - `config/read`, `config/write`, `config/batch-write`, `config/requirements`
  - `account/get`, `account/login`, `account/cancelLogin`, `account/logout`, `account/rate-limits`
  - `mcp/server-status`, `mcp/auth-status`
  - `fs/read`, `fs/write`, `fs/mkdir`, `fs/remove`
  - `command/exec`
  - And 30+ more
- `ConfigManager` — multi-layer config loading with validation
- `DynamicTools` — MCP tool registration
- `ModelsRefreshWorker` — periodic model catalog refresh
- Auth management via `AuthManager`

### From `codex-core` (transitive)

- Full LLM client (Responses API, Chat API, Anthropic Messages API)
- Thread session management (state machine, turns, compaction)
- MCP integration (server connections, tool discovery, tool calls)
- Skills engine (building, injecting, loading)
- Plugin system (marketplace, install, uninstall)
- Sandbox enforcement (Windows, Linux landlock, macOS)
- Web search, image processing, safety checks
- Context fragment injection
- Telemetry

### From `codex-login` (transitive)

- `AuthManager` singleton — load, cache, refresh, recover auth
- All login flows (OAuth browser, OAuth device code, API key, PAT, Bedrock, Kimi Code)
- Token persistence (file + OS keyring)
- 401 recovery state machine

### From `codex-config` (transitive)

- Multi-layer config loading (defaults + user + project + cloud + CLI)
- Type-safe TOML parsing for all config fields
- Security policy constraints (network, filesystem, sandbox, residency)
- Config validation with diagnostic error reporting
- MCP server config, hook config, permission profiles
- Thread-level config

---

## 6. What to Extract: `codex-tui-render` Crate

### Manifest

```toml
[package]
name = "codex-tui-render"
version = "0.1.0"
edition = "2024"

[dependencies]
ratatui = { workspace = true }
pulldown-cmark = { workspace = true }
syntect = "5"
two-face = { version = "0.5", default-features = false, features = ["syntect-default-onig"] }
textwrap = { workspace = true }
unicode-width = { workspace = true }
unicode-segmentation = { workspace = true }
diffy = { workspace = true }
codex-app-server-protocol = { path = "../codex-rs/app-server-protocol" }
codex-protocol = { path = "../codex-rs/protocol" }
codex-utils-absolute-path = { path = "../codex-rs/utils/absolute-path" }
codex-utils-string = { path = "../codex-rs/utils/string" }
codex-terminal-detection = { path = "../codex-rs/terminal-detection" }
once_cell = { workspace = true }
```

### File list

All files extracted from `codex-rs/tui/src/` with zero changes to the types within:

| Source file | Lines | Extracted as | Notes |
|-------------|-------|-------------|-------|
| `history_cell/mod.rs` | 332 | `history_cell/mod.rs` | `HistoryCell` trait, `Renderable for Box<dyn HistoryCell>` |
| `history_cell/messages.rs` | 491 | `history_cell/messages.rs` | `AgentMarkdownCell` — main AI response cell |
| `history_cell/exec.rs` | 217 | `history_cell/exec.rs` | `ExecCell` — shell command + output |
| `history_cell/base.rs` | 146 | `history_cell/base.rs` | `PlainHistoryCell` |
| `history_cell/hook_cell.rs` | 971 | `history_cell/hook_cell.rs` | `HookCell` — hook execution |
| `history_cell/mcp.rs` | 622 | `history_cell/mcp.rs` | `McpToolCallCell` — MCP tool calls |
| `history_cell/search.rs` | 133 | `history_cell/search.rs` | `WebSearchCell` |
| `history_cell/plans.rs` | 206 | `history_cell/plans.rs` | `PlanHistoryCell` |
| `history_cell/patches.rs` | 87 | `history_cell/patches.rs` | `PatchCell` |
| `history_cell/approvals.rs` | 341 | `history_cell/approvals.rs` | Approval cells |
| `history_cell/session.rs` | 440 | `history_cell/session.rs` | `SessionHeaderCell` |
| `history_cell/notices.rs` | 217 | `history_cell/notices.rs` | Notice cells |
| `history_cell/separators.rs` | 163 | `history_cell/separators.rs` | Separator cells |
| `history_cell/request_user_input.rs` | 173 | `history_cell/request_user_input.rs` | User input request cells |
| | | | |
| `render/mod.rs` | 50 | `render/mod.rs` | `Insets`, `RectExt` |
| `render/renderable.rs` | 450 | `render/renderable.rs` | `Renderable`, `FlexRenderable`, etc. |
| `render/highlight.rs` | 1,442 | `render/highlight.rs` | syntect syntax highlighting |
| `render/line_utils.rs` | 56 | `render/line_utils.rs` | Line utilities |
| | | | |
| `streaming/mod.rs` | 115 | `streaming/mod.rs` | `StreamState` |
| `streaming/controller.rs` | 1,699 | `streaming/controller.rs` | `StreamController`, `StreamCore` |
| `streaming/chunking.rs` | 404 | `streaming/chunking.rs` | `AdaptiveChunkingPolicy` |
| `streaming/commit_tick.rs` | 196 | `streaming/commit_tick.rs` | Animation frame scheduling |
| `streaming/table_holdback.rs` | 216 | `streaming/table_holdback.rs` | Table detection during streaming |
| | | | |
| `exec_cell/mod.rs` | 10 | `exec_cell/mod.rs` | Re-exports |
| `exec_cell/model.rs` | 157 | `exec_cell/model.rs` | `CommandOutput` |
| `exec_cell/render.rs` | 998 | `exec_cell/render.rs` | `output_lines()` |
| | | | |
| `markdown_render.rs` | 2,554 | `markdown_render.rs` | pulldown-cmark → styled Lines |
| `markdown.rs` | 460 | `markdown.rs` | Higher-level helpers |
| `markdown_stream.rs` | 817 | `markdown_stream.rs` | `MarkdownStreamCollector` |
| `markdown_text_merge.rs` | 43 | `markdown_text_merge.rs` | Text merge utility |
| | | | |
| `terminal_hyperlinks.rs` | 574 | `terminal_hyperlinks.rs` | OSC 8 hyperlinks |
| `wrapping.rs` | 1,481 | `wrapping.rs` | `adaptive_wrap_line`, `word_wrap_line` |
| `text_formatting.rs` | 517 | `text_formatting.rs` | `truncate_text`, `proper_join` |
| `live_wrap.rs` | 263 | `live_wrap.rs` | `RowBuilder` for streaming |
| `style.rs` | 165 | `style.rs` | `app_accent_style`, `user_message_style` |
| `color.rs` | 63 | `color.rs` | `blend()`, `is_light()`, `perceptual_distance()` |
| `terminal_palette.rs` | 579 | `terminal_palette.rs` | Terminal color detection |
| `diff_render.rs` | 2,324 | `diff_render.rs` | Diff rendering |
| `diff_model.rs` | 18 | `diff_model.rs` | `FileChange` struct |
| `shimmer.rs` | 72 | `shimmer.rs` | Shimmer animation |
| `table_detect.rs` | 435 | `table_detect.rs` | Pipe-table detection |
| `line_truncation.rs` | 86 | `line_truncation.rs` | Line truncation |
| `width.rs` | 67 | `width.rs` | `display_width()` |
| `ui_consts.rs` | 11 | `ui_consts.rs` | `LIVE_PREFIX_COLS`, constants |
| `token_usage.rs` | 77 | `token_usage.rs` | `TokenUsage` struct |

Total: **~22,000 lines** across **~44 files**.

---

## 7. Integration Demo: Your Bridge Module

```rust
// src/codex_integration/mod.rs (the bridge)
use codex_app_server_client::AppServerClient;
use codex_app_server_protocol::*;
use codex_tui_render::*;
use std::sync::Arc;

pub struct CodexIntegration {
    client: AppServerClient,
    thread_id: ThreadId,
    transcript: Vec<Arc<dyn HistoryCell>>,
    active_cell: Option<Box<dyn HistoryCell>>,
    stream_state: StreamState,
    stream_controller: StreamController,
    markdown_collector: MarkdownStreamCollector,
}

impl CodexIntegration {
    /// Create from an already-started AppServerClient
    pub fn new(client: AppServerClient) -> Self { /* ... */ }

    /// Handle one server notification — the core state machine
    pub fn handle_notification(&mut self, notif: ServerNotification) {
        match notif {
            ServerNotification::ContentDelta(delta) => {
                self.markdown_collector.push_str(&delta.content_delta);
                self.stream_state.push_lines(
                    self.markdown_collector.drain_ready_lines()
                );
                // StreamController emits into active_cell
                self.stream_controller.tick(&mut self.stream_state);
            }
            ServerNotification::ItemCompleted(item) => {
                // Consolidate active_cell into a committed HistoryCell
                if let Some(cell) = self.active_cell.take() {
                    self.transcript.push(Arc::from(cell));
                }
            }
            ServerNotification::TurnCompleted(_) => {
                // Finalize everything
            }
            ServerNotification::ToolCallStarted(params) => {
                // Create an ExecCell in the active area
                self.active_cell = Some(Box::new(ExecCell::new(params)));
            }
            _ => {}
        }
    }

    /// Render all cells into a ratatui buffer
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut y = area.y;
        for cell in &self.transcript {
            let lines = cell.display_lines(area.width);
            let h = cell.desired_height(area.width);
            let cell_area = Rect::new(area.x, y, area.width, h.min(area.bottom() - y));
            // Render the paragraph
            Paragraph::new(Text::from(lines))
                .wrap(Wrap { trim: false })
                .render(cell_area, buf);
            y += cell_area.height;
        }
        // Also render active_cell if present
    }

    /// Submit user text to the current thread
    pub async fn submit_turn(&self, text: &str) -> Result<(), anyhow::Error> {
        self.client.request_handle()
            .request_typed::<TurnSubmitResponse>(
                ClientRequest::UserTurn {
                    thread_id: self.thread_id.to_string(),
                    content: text.to_string(),
                    ..Default::default()
                }
            ).await?;
        Ok(())
    }
}
```

### Dx-tui integration points

```rust
// In src/state.rs (ChatState)
pub struct ChatState {
    pub codex: Option<CodexIntegration>,
    pub codex_mode: bool,
    // ... all existing fields remain ...
}

// In src/chat_render.rs
impl ChatState {
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        if self.codex_mode && let Some(ref codex) = self.codex {
            // Delegate message area to codex rendering
            codex.render(message_area, buf);
        } else {
            // Existing dx-tui native rendering
            self.render_message_list(message_area, buf);
        }
        // Keep rendering input bar, sidebar, etc. the same
        self.render_input(input_area, buf);
    }
}

// In src/input.rs
impl InputState {
    async fn on_submit(&mut self, text: &str) {
        if let Some(ref codex) = state.codex {
            codex.submit_turn(text).await;
        } else {
            // Existing dx-tui agent loop
        }
    }
}
```

---

## 8. Summary: You Keep ~7%, Inherit ~93%

### What you write (NEW) — ~2,000-3,000 lines

| Component | Lines |
|-----------|-------|
| `CodexIntegration` bridge | ~800 |
| dx-tui integration points (chat_render, input, state) | ~500 |
| Startup wiring (config, auth, exec, app-server) | ~500 |
| Model selection / account UI (popups) | ~500 |
| **Total new code** | **~2,300** |

### What you extract (from codex-rs/tui) — ~22,000 lines

| Component | Lines |
|-----------|-------|
| `codex-tui-render` crate | ~22,000 |

### What you inherit (from codex-rs workspace crates) — ~1,000,000+ lines

| What | Lines (approx) |
|------|---------------|
| AI engine (codex-core) | ~150,000 |
| App server (codex-app-server) | ~80,000 |
| Protocol types (app-server-protocol + protocol) | ~40,000 |
| Config system (codex-config) | ~30,000 |
| Auth system (codex-login) | ~15,000 |
| Model providers (3 crates) | ~20,000 |
| Exec server (codex-exec-server) | ~30,000 |
| 30+ utility crates | ~50,000 |
| **Total inherited** | **~415,000+** |

### What you drop (codex-rs/tui TUI shell) — ~186,000 lines

| Component | Lines |
|-----------|-------|
| `bottom_pane/` (input bar, 37 files) | 39,081 |
| `chatwidget/` (state machine, 59 files) | 19,852 |
| `app/` (event loop, 30 files) | 21,493 |
| `tui/` (terminal init, 7 files) | 1,635 |
| 70+ other TUI feature files | ~104,000 |
| **Total dropped** | **~186,000** |
