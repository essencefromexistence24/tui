# dx-tui + codex-rs Integration TODO

## Principle
- **KEEP** everything that renders pixels or handles input (dx-tui's UI is the strength)
- **REPLACE** everything that runs AI, manages state, or executes tools (codex-rs has the production engine)
- **INTEGRATE** thin bridges: keep dx-tui's UI, connect to codex-rs backend

---

## ✅ Phase 0: Dependency Setup

- [x] Added `tokio-tungstenite` to Cargo.toml (WebSocket client, no native-lib conflicts)
- [x] Avoided `codex-app-server-protocol` crate (tree-sitter native lib conflict with editor)
- [x] Using raw `serde_json` for protocol instead
- [x] Verified clean compilation: `cargo check -p dx-tui -j12` passes

---

## ✅ Phase 1: Create `CodexClient` Bridge Module

- [x] Created `src/codex_client.rs` (~580 LOC)
- [x] WebSocket connection to codex app-server with auto-reconnect
- [x] JSON-RPC handshake (initialize/initialized)
- [x] Thread lifecycle management (auto-creates thread on connect)
- [x] Turn submission (`submit_text`)
- [x] Event streaming with typed `CodexEvent` enum
- [x] Server request (approval) forwarding
- [x] Cloneable `CodexClientHandle` for spawned tasks
- [x] Graceful shutdown

---

## ✅ Phase 2: Wire into State & Modes

- [x] `src/modes.rs`: Added `Codex` variant to `AgentMode` enum
- [x] `src/state.rs`: Added `codex_client: Option<CodexClient>` + `codex_event_rx` fields
- [x] Updated all 7 match sites that exhaustively handle `AgentMode`
- [x] Added `prefers_codex()` method

---

## ✅ Phase 3: Wire Event Loop

- [x] `ChatState::poll_codex_events()` — drains codex event receiver
- [x] `handle_codex_event()` — converts events to messages/toasts
- [x] `append_codex_delta()` / `finish_codex_message()` — manages assistant message
- [x] Polls before each render in `chat_render.rs`

---

## ✅ Phase 4: Wire Chat Submission

- [x] `add_user_message()` short-circuits agent spawn for Codex mode
- [x] Spawns async task that calls `CodexClientHandle::submit_text()`
- [x] Codex events stream back and render as assistant messages
- [x] `/codex` slash command to switch mode
- [x] `/codex-connect <url>` slash command to connect to app-server
- [x] `CodexClientHandle` — cloneable handle for spawned tasks
- [x] Async connection with `codex_pending` slot

---

## ✅ Phase 5: File Browser Context Bridge

- [x] Thread creation includes `cwd` from current directory
- [x] Turn submission includes `cwd` in params
- [x] Codex model sees the user's workspace directory

---

## ✅ Phase 6: Approval & Question UI

- [x] `PendingCodexRequest` struct tracks pending approvals
- [x] ServerRequest events parsed and shown as toasts + bottom bar
- [x] `resolve_codex_pending_request(approve)` sends decision back to app-server
- [x] `CodexClientHandle::resolve_request()` added
- [x] Approvals for command/file/permission requests handled

---

## ✅ Phase 7: Voice & Audio Integration

- [x] Voice STT text flows through `add_user_message` → routed to codex when in codex mode
- [x] dx-tui's mic capture + frequency bars UI kept (unchanged)
- [x] Ctrl+S to insert transcript → Enter to submit → codex turn

## ✅ Phase 8: Model Selection UI

- [x] When in Codex mode, model picker shows "Codex — managed by app-server" label
- [x] Normal mode model catalog preserved for non-codex modes
- [x] Model selection delegated to app-server when using codex backend

---

## Phase 5: File Browser Context Bridge

- [ ] Create thin adapter that packs file browser state (open files, current dir, selection) into codex-rs context fragments
- [ ] On turn submission in codex mode, inject file context into the request
- [ ] Ensure codex-rs model has visibility into dx-tui's current workspace state

---

## Phase 6: Approval & Question UI Integration

- [ ] `src/permission_hub.rs`: Keep TUI popup (bottom bar), replace the backend — wire to codex `ServerRequest::PermissionRequest` / `ElicitationLifecycle`
- [ ] `src/question_hub.rs`: Keep TUI question dock, wire to codex `ServerNotification::UserInputRequested`
- [ ] `src/bottom_center.rs`: Ensure codex permission/question states render correctly

---

## Phase 7: Voice & Audio Integration

- [ ] `src/voice.rs`: Keep mic capture + frequency bars UI. Replace `dx-flow` binary calls with `codex-model-provider` STT/TTS
- [ ] `src/sound.rs`: Keep rodio playback engine. Wire notification sounds to codex events

---

## Phase 8: Model Selection UI

- [ ] Replace dx-tui's provider picker (zen.rs + providers/) with codex-rs model list via `codex-models-manager`
- [ ] Show available models from codex-rs's `bundled_provider_catalog`
- [ ] Handle provider auth via `codex-login` (OAuth device flow for Codex API)

---

## Phase 9: Feature-Gate & Deprecate Old Backend

- [ ] Feature-gate dx-tui's native agent loop: `#[cfg(feature = "legacy-agent")]`
- [ ] Feature-gate dx-tui's native providers: `#[cfg(feature = "legacy-providers")]`
- [ ] Feature-gate dx-tui's native MCP: `#[cfg(feature = "legacy-mcp")]`
- [ ] Feature-gate dx-tui's native tools: `#[cfg(feature = "legacy-tools")]`
- [ ] Keep backward compat by defaulting to legacy mode until codex mode is verified
- [ ] Default `codex_mode: true` after stable testing

---

## Phase 10: Cleanup (optional/future)

- [ ] Remove `agent_loop.rs`
- [ ] Remove `agent_backend.rs`
- [ ] Remove `flow_backend.rs`
- [ ] Remove `orchestration.rs`
- [ ] Remove `tools/mod.rs`
- [ ] Remove `mcp.rs` + `mcp_tool.rs`
- [ ] Remove `provider_registry.rs` + `providers/`
- [ ] Remove `zen.rs`
- [ ] Remove `workspace_tools.rs`
- [ ] Remove `session_db.rs` + `session_store.rs` (keep `session_meta.rs` + `session_search.rs` if they adapt to codex-rs rollout)
- [ ] Remove `skills.rs`
- [ ] Remove `memory_tool.rs` + `memory_provider.rs`
- [ ] Remove `plugin_system.rs` + `plugin_system_tool.rs`
- [ ] Remove `subagent_registry.rs`
- [ ] Remove `goal_runner.rs`
- [ ] Remove `background_review.rs`
- [ ] Remove `compaction.rs`
- [ ] Remove `token_save.rs`
- [ ] Remove `agent_workspace.rs`
- [ ] Remove `dx_system.rs`
- [ ] Remove `learning_graph.rs`
- [ ] Remove `channel_actions.rs` + `channels.rs`
- [ ] Remove `scheduler.rs`
- [ ] Remove `prompt_queue.rs`
- [ ] Remove `profile_prompts.rs`
- [ ] Remove `omniroute.rs`
- [ ] Remove `lsp_tool.rs`
- [ ] Remove `update_check.rs`
- [ ] Remove `llm.rs`

---

## Testing

- [ ] Unit tests for `CodexClient` notification handling
- [ ] Integration test: start codex app-server, connect, submit turn, verify transcript
- [ ] Test file browser context injection
- [ ] Test permission approval flow
- [ ] Test voice/audio integration
- [ ] Test model selection and provider auth
- [ ] Manual test: full chat session in codex mode

---

## LOC Budget

| Phase | New | Modified | Removed |
|-------|-----|----------|---------|
| 0: Dependencies | ~5 lines (Cargo.toml) | — | — |
| 1: CodexClient | ~800 | — | — |
| 2: State & Modes | — | ~50 | — |
| 3: Event loop | — | ~100 | — |
| 4: Chat submission | — | ~30 | — |
| 5: File context | ~100 | — | — |
| 6: Approval/Question | ~50 | ~50 | — |
| 7: Voice/Audio | ~100 | ~50 | — |
| 8: Model selection | ~200 | ~100 | ~500 |
| 9: Feature gates | ~30 | ~30 | — |
| 10: Cleanup | — | — | ~16,850 |
| **Total** | **~1,285** | **~410** | **~16,850** |
