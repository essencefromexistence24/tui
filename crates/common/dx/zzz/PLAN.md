# Integration Plan: dx-tui → codex-rs Backend

Replace dx-tui's local agent loop with codex-rs's **in-process app-server** backend.
dx-tui keeps: all UI (file browser, editor, animations, themes, input bar, diff view, etc.)
dx-tui gains: codex-rs's AI engine, thread management, 25+ model providers, sandbox, permissions, approvals

## Strategy

Copy framework-agnostic logic from `codex-rs/tui/` (param builders, state machines, event routing) directly into `src/codex/`. The copied files are pure protocol/state logic with no ratatui dependencies. For UI features (approvals, auth, MCP forms), write inline/text-based versions using dx-tui's existing rendering style instead of copying codex-rs-tui's ratatui popup widgets.

---

## Step 1: Add Cargo Dependencies

Add 3 crates that are already in the transitive dep graph:

```toml
codex-protocol = { path = "../protocol" }
codex-utils-absolute-path = { path = "../utils/absolute-path" }
codex-features = { path = "../features" }
```

---

## Step 2: Create `src/codex/` Module Directory

```
src/codex/
  mod.rs                   # module declarations + re-exports
  event_targets.rs         # copied from app/app_server_event_targets.rs
  permission_compat.rs     # copied from permission_compat.rs
  auto_review_denials.rs   # copied from auto_review_denials.rs
  thread_session_state.rs  # copied from session_state.rs
  service_tier.rs          # adapted from service_tier_resolution.rs
  params.rs                # extracted param builders from app_server_session.rs
  response.rs              # extracted response mappers from app_server_session.rs
  pending_requests.rs      # adapted from app/app_server_requests.rs
  thread_events.rs         # adapted from app/thread_events.rs
  event_router.rs          # adapted from app/app_server_events.rs
```

---

## Step 3: Copy 5 Verbatim Files

These have **zero** dependencies on codex-rs-tui internal types. Copy logic, drop tests.

| Source | Target | LOC | Changes |
|--------|--------|:---:|---------|
| `app/app_server_event_targets.rs` | `codex/event_targets.rs` | 325 | `pub(super)` → `pub(crate)`, drop tests |
| `permission_compat.rs` | `codex/permission_compat.rs` | 93 | drop tests |
| `auto_review_denials.rs` | `codex/auto_review_denials.rs` | 133 | drop tests |
| `session_state.rs` | `codex/thread_session_state.rs` | 78 | drop tests |
| `service_tier_resolution.rs` | `codex/service_tier.rs` | 64 | `crate::legacy_core::config::Config` → `codex_config::Config`, drop tests |

---

## Step 4: Extract Param Builders from `app_server_session.rs`

Copy config→protocol conversion functions (~270 LOC) into `codex/params.rs`:

- `config_request_overrides_from_config()` — build HashMap for thread RPC
- `service_tier_override_from_config()` — extract optional service tier
- `sandbox_mode_from_permission_profile()` — PermissionProfile → SandboxMode
- `permissions_selection_from_config()` — extract active profile ID
- `turn_permissions_overrides()` — TurnPermissionsOverride → (sandbox, permissions)
- `thread_start_params_from_config()` — build full ThreadStartParams (13 fields)
- `thread_resume_params_from_config()` — build full ThreadResumeParams
- `thread_fork_params_from_config()` — build full ThreadForkParams
- `thread_cwd_from_config()` — resolve CWD for embedded/remote
- `approvals_reviewer_override_from_config()` — convert ApprovalsReviewer
- `permission_profile_id_from_active_profile()` — extract profile ID string
- Supporting types: `ThreadParamsMode`, `TurnPermissionsOverride`, `ResumeModelSettings`, `AppServerStartedThread`

Import changes: `crate::legacy_core::config::Config` → `codex_config::Config`, internal refs to our copied modules.

---

## Step 5: Extract Response Mappers from `app_server_session.rs`

Copy response→session mapping functions (~200 LOC) into `codex/response.rs`:

- `started_thread_from_start_response()` / `resume` / `fork`
- `thread_session_state_from_thread_*_response()`
- `display_permission_profile_from_thread_response()`
- `thread_session_state_from_thread_response()` — core mapping

Simplifications for MVP: skip `instruction_source_paths`, `message_history`, `network_proxy`.

---

## Step 6: Copy + Adapt `PendingAppServerRequests`

Copy from `app/app_server_requests.rs` (~320 LOC logic + ~500 LOC tests to drop) into `codex/pending_requests.rs`.

- Remove `impl App` block (uses `AppServerSession` — we use dx-tui's handle directly)
- Replace `crate::app_command::AppCommand` with dx-tui's own `CodexCommand` enum
- Replace `granted_permission_profile_from_request` with a simple 20-LOC equivalent
- Create `codex/command.rs` with just the needed variants:
  - `ExecApproval { id, decision }`
  - `PatchApproval { id, decision }`
  - `RequestPermissionsResponse { id, response }`
  - `UserInputAnswer { id, response }`
  - `ResolveElicitation { server_name, request_id, decision, content, meta }`

---

## Step 7: Copy + Adapt `ThreadEventStore`

Copy from `app/thread_events.rs` (~275 LOC logic + ~360 LOC tests to drop) into `codex/thread_events.rs`.

- Replace `use super::*` with explicit imports from `codex_app_server_protocol`, etc.
- Replace `AppCommand` references with `CodexCommand`
- Simplify `ThreadBufferedEvent` — remove `HistoryEntryResponse` and `FeedbackSubmission`
- Drop `PendingInteractiveReplayState` dependency for MVP

---

## Step 8: Copy + Adapt Event Router

Copy dispatch pattern from `app/app_server_events.rs` (~130 LOC logic) into `codex/event_router.rs`.

- Replace `ChatWidget` handler calls with dx-tui's `ChatState` methods
- Stub out MCP startup tracking for now
- Simplify thread routing (dx-tui only has one primary thread)
- Map `Disconnected` event to dx-tui's disconnection handler

---

## Step 9: Integrate into `codex_bridge.rs`

Enhance the existing bridge with copied modules:

- Use `params.rs` builders for full `thread/start` (adds sandbox, permissions, runtime_workspace_roots, approvals_reviewer)
- Use `params.rs` builders for full `turn/start` (adds effort, summary, personality, sandbox_policy, permissions, output_schema)
- Wire `PendingAppServerRequests` to handle incoming `ServerRequest` events
- Wire `ThreadEventStore` for event buffering
- Expand `BridgeEvent` enum with more variants (ItemStarted, ItemCompleted, TurnStarted, RequestApproval, etc.)
- Route incoming events through event router

---

## Step 10: Wire into `ChatState`

- Add `PendingAppServerRequests` field to track in-flight approvals
- Add approval/input request handling in the event loop
- Show inline text prompts for approvals
- Wire interrupt command (Ctrl+C during codex turn)
- Wire /resume and /fork slash commands

---

## Summary: Estimated LOC

| Step | What | LOC | Effort |
|:----:|------|:---:|:------:|
| 1 | Add 3 Cargo deps | 3 | 2 min |
| 2 | Create module structure | 1 file | 5 min |
| 3 | Copy 5 verbatim files | ~693 | 30 min |
| 4 | Extract param builders | ~300 | 1 hr |
| 5 | Extract response mappers | ~200 | 1 hr |
| 6 | Copy+adapt PendingAppServerRequests | ~370 | 1 hr |
| 7 | Copy+adapt ThreadEventStore | ~250 | 1 hr |
| 8 | Copy+adapt event router | ~150 | 1 hr |
| 9 | Integrate into codex_bridge.rs | ~150 added | 1 hr |
| 10 | Wire into state.rs | ~200 added | 1 hr |
| **Total** | | **~2,316** | **~7.5 hr** |

---

## Feature Parity After Steps 1-10

| Feature | Status |
|---------|--------|
| In-process app-server lifecycle | ✅ |
| `thread/start` with full params (13 fields) | ✅ |
| `turn/start` with full params (14+ fields) | ✅ |
| Sandbox/permissions handling | ✅ |
| Config overrides (reasoning_summary, personality, web_search, verbosity, etc.) | ✅ |
| Approval request tracking (exec, file, permissions, user_input) | ✅ |
| Event buffering/replay | ✅ |
| Thread resume/fork param builders | ✅ |
| Session state tracking | ✅ |
| Turn interrupt | ✅ |
| `is_loading` bug fix | ✅ |
| Event routing (notification/request dispatch) | ✅ |

**After steps 1-10: ~70% feature parity with codex-rs-tui**

---

## Remaining Gaps (within scope of steps 1-10)

These are features that the PLAN lists as DONE but still need wiring:

None. All features listed in steps 1-10 are now fully wired.

---

## Future Tasks

These require separate UI work and are NOT included in steps 1-10:

- **Auth flows** — login screens, API key entry, device code flow, ChatGPT auth, account status display, logout. Depends on `AccountUpdated`, `AccountRateLimitsUpdated` notifications and `account/read`, `account/logout` RPCs.
- **MCP elicitation UI** — form-based elicitation requests from MCP servers. Schema parsing (500 LOC reusable) + inline form rendering.
- **Plugin/skills catalog UI** — browse, install, and manage plugins and skills.
- **Session resume picker UI** — browse past threads, search/filter, preview transcript, select session to resume.
- **Rate limit display widgets** — show rolling rate limit snapshots, reset credits, plan type.
- **Telemetry** — OpenTelemetry provider, SQLite telemetry recorder, session metrics, log database layer.
