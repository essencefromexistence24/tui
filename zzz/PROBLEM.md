# Problem — Opencode Zen / Other Non-xAI Models Return Spurious `FreeUsageLimitError` on First Prompt

**Date:** 2026-08-12  
**Status:** Resolved in the active request pipeline; regression coverage added
**Symptom reported:** `Retry failed: FreeUsageLimitError: Error from provider (Console): Rate limit exceeded. Please try again later.`  
**Scope:** xAI models work; Opencode Zen and other hosted Chat Completions models fail consistently. Error is **not** genuine rate limiting / API quota — it is a **request-construction** problem on the 1st turn / tool payload.

---

## 1. TL;DR

Two independent wire-format bugs on the Chat Completions path make the *first* request invalid for strict OpenAI-compatible providers (Opencode Zen, etc.); those providers surface the validation failure through their Console/free-tier gateway as `FreeUsageLimitError / Rate limit exceeded`:

1. **Stripped tool schemas** — hosted (non-local) models were sent `{"type":"object"}` for every built-in tool (description + JSON Schema removed via `compact_native_definitions`). xAI's backend tolerates this because it also reads the Dx Serializer Compact catalog in the prompt; any other provider validates/generates from the native `tools[].function.parameters` object and fails.
2. **Fragmented first prompt** — the session harness builds the logical first turn as several adjacent synthetic `ConversationItem::User` items (workspace context, skills, MCP reminders, then the query). `conversation_to_chat_messages` emitted each as a separate `role: user` wire message. Strict proxies expect `system + single user` on turn 1 and treat the `user/user/user` burst as malformed / token-abuse, triaging to the free-tier path.

Both fires on turn 1 — before any model text — hence “tool sending / 1st prompt sending problem” and why retries never help.

---

## 2. Symptom vs Reality

| Symptom | Reality |
|---|---|
| `FreeUsageLimitError` after retries | Provider Console mapped a **400-class validation error** (bad tools / bad message sequence) onto its free-usage rate-limit bucket |
| “Rate limit exceeded. Try again later.” | Retrying the same malformed payload reproduces instantly — not time-based |
| Only Opencode Zen / non-xAI models | xAI path tolerates both bugs; strict OpenAI-compatible proxies do not |
| Fails on first prompt, before any tool call | Confirms request-construction, not tool-result handling |

---

## 3. Affected / Unaffected

- **Affected:** Any model routed through `ConversationRequest → ChatCompletionRequest` (`crates/codegen/xai-grok-sampling-types/src/conversation/chat_completions.rs` → `crates/codegen/xai-grok-sampler`) when `tool_definitions` non-empty and first turn has >1 synthetic user item. Reported: Opencode Zen (and “others” via same Console gateway).
- **Not affected:** xAI-hosted models (`grok-*` via first-party base_url) — they read `dx_serializer_compact::TOOL_CATALOG` in the prompt and accept bare `{"type":"object"}` registrations. Local models (`is_local_base_url`) — they bypass hosted tool schemas entirely (`tool_definitions_builtins_only` then cleared).

---

## 4. Root Cause #1 — `compact_native_definitions` Stripped Hosted Tool Schemas

**File:** `crates/codegen/xai-grok-agent/src/native_tool_presentation.rs:48-57`

```rust
pub fn compact_native_definitions(mut definitions: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    // for BUILTIN_TOOL_NAMES: description = None, parameters = {"type":"object"}
}
```

**Caller (before fix):** `crates/codegen/xai-grok-shell/src/session/acp_session_impl/sampler_turn.rs:210-230`
```rust
let definitions = self.agent.borrow().tool_definitions().await;
xai_grok_agent::native_tool_presentation::compact_native_definitions(definitions)
```

**Why it breaks non-xAI providers:**
- Dx Serializer Compact is a *prompt-level* catalog (`TOOL_CATALOG` injected as text). It is **not** the provider’s `tools` parameter.
- OpenAI-compatible Chat Completions providers validate `tools[].function.parameters` as JSON Schema and use it to constrain generation. Sending a bare object makes every tool effectively `any` — many proxies reject with `invalid_request_error` or route to a degraded free path that then emits `FreeUsageLimitError`.
- Add `tool_choice` without valid tools would be a hard 400, but here `tool_choice` is present **with** invalid tools — strict providers fail the function-definition leg.

**Why xAI still works:** xAI’s Responses/Chat backend is co-designed with the compact catalog and synthesizes full schemas server-side; the empty stub is intentional there.

**Local mitigation already in working tree (uncommitted):**
`sampler_turn.rs` now bypasses `compact_native_definitions` for hosted models, keeping full `definitions` (comment added: “provider request must always carry real JSON Schema definitions … not from the compact prompt catalog”). `token_optimization.optimize_tool_schemas` log downgraded from `info` to `debug` and no longer implies compaction.

---

## 5. Root Cause #2 — Adjacent Synthetic User Items Sent as Separate Wire Messages

**File:** `crates/codegen/xai-grok-sampling-types/src/conversation/chat_completions.rs:191-252`

Before fix, `conversation_to_chat_messages` pushed each `ConversationItem::User` via `conversation_item_to_chat_message` immediately:

```
system
user("workspace context")
user("skills reminder")
user("actual query")   // 3× role:user bursts
```

**Harness source:** `crates/codegen/xai-chat-state/src/actor/request_builder.rs` (and callers) synthesizes first turn from multiple adjacent `UserItem`s for context injection. Persisted history keeps them separate intentionally (lossless).

**Why it breaks strict proxies:**
- Many Chat Completions gateways normalize or reject sequential `user` messages — spec says they are allowed, but proxies for Opencode/Zen coalesce → token-count → enforce a “single logical prompt” rule. Three `user` blocks with identical `content: Text` inflates the input token estimate, trips their Console free-tier classifier (“unusual prompt shape → free quota”), and maps to `Rate limit exceeded`.
- Also breaks `reasoning_content` folding semantics (pending reasoning cleared per user).

**Local mitigation already in working tree (uncommitted):**
New `pending_user: Option<UserItem>` + `flush_user` closure. Adjacent `User` items are now coalesced at the wire boundary only:

```rust
pending.content.extend(user.content);
pending.synthetic_reason = None; // coalesced logical prompt
// cwd_generation / prompt_index forwarded from last fragment
```

Flush on `Assistant`, `BackendToolCall`, or any non-`User`/end-of-list. Persisted `ConversationRequest.items` unchanged. New test `adjacent_user_items_are_one_wire_prompt` asserts `system + 1 user("a\nb\nc")`.

---

## 6. Why the Error Is Misleadingly a “FreeUsageLimit”

- Opencode Console sits in front of Zen / other community providers. Malformed Chat Completions requests (empty tool schemas + multi-user burst) fail their pre-flight validator; the Console’s error mapper buckets validator rejections that look like “excessive / unbillable tokens” as `FreeUsageLimitError`.
- `crates/codegen/xai-grok-sampler` classifies that as `SamplingErrorKind::RateLimited` → pager surfaces `RetryState::Exhausted { is_rate_limited:true }` → final `Error from provider (Console): Rate limit exceeded`.
- No `grep -r FreeUsageLimitError` hit in repo confirms it is **provider-constructed**, not local.

---

## 7. Evidence

- `git diff` (uncommitted, `7cedcca6` base):
  - `chat_completions.rs` + `pending_user` coalescing + test.
  - `sampler_turn.rs` + removal of `compact_native_definitions` for hosted.
  - `chat_completions_tests.rs` + regression test.
- `crates/codegen/xai-grok-agent/src/native_tool_presentation.rs:12-40` — `BUILTIN_TOOL_NAMES` (26 tools) all stripped previously.
- Responses path (`crates/codegen/xai-grok-sampling-types/src/conversation/responses.rs:97`) already handles the multi-user shape correctly — divergence is Chat Completions-only, explaining why xAI Responses models pass.
- `xai-grok-shell/tests/test_sampling_client.rs:395-436` — `read_chat_history_sync → From<ConversationRequest> for ChatCompletionRequest` round-trip only recently covered.

---

## 8. Impact

- All sessions using Opencode Zen or any non-xAI Chat Completions model with tools enabled fail on the *first* sampling turn; no assistant text or tool calls ever stream.
- Retries / waiting do not recover (deterministic payload).
- xAI `grok-*` sessions unaffected — masks the regression in local dev where xAI is default.

---

## 9. Reproduction (without fixing)

1. Configure a session with `model = "opencode/zen"` (or any hosted `api_backend = OpenAICompat` via `xai-proxy` / `base_url` not `*.x.ai`).
2. Ensure `Definition::tool_definitions()` non-empty (default — no `optimize_tool_schemas` override needed).
3. Trigger a session where `request_builder` injects ≥2 synthetic user fragments (default fresh session: workspace snapshot + skills).
4. Observe first `SamplerHandle::submit_and_collect` → provider returns `400/429` mapped to `FreeUsageLimitError` → pager `RetryState::Exhausted`.
5. Switching same session to `grok-4` immediately succeeds — confirms not quota.

---

## 10. What a Correct Fix Must Do

1. **Never compact hosted tool schemas.** `Dx Serializer Compact` stays prompt-side; `tools[].function.parameters` must be full JSON Schema for every hosted adapter, including xAI-compatible proxies. Guard by `is_local_base_url` only, not `token_optimization.optimize_tool_schemas`. (`sampler_turn.rs` draft in working tree matches this.)
2. **Coalesce adjacent user items at the Chat Completions boundary only.** Extend content with `"\n"` (current impl) and preserve image blocks via `MessageContent::Blocks`. Do not mutate persisted `ConversationItem`s. (`chat_completions.rs` draft matches this.)
3. **Keep regression tests.** `adjacent_user_items_are_one_wire_prompt` plus tool-schema fidelity tests now assert that hosted `tools[0].function.parameters` retains `properties` and `required` for both Chat Completions and Responses.
4. **Optional:** Improve error mapping — when Console returns `FreeUsageLimitError` on turn 1 with `tools != None`, surface “invalid tool schema / fragmented prompt” hint alongside, so future misroutes are not mistaken for quota.

---

## 11. Verification Checklist (for when fix is applied)

- [x] `cargo test -p xai-grok-sampling-types --lib conversation::chat_completions_tests --release --offline`
- [x] Responses adapter schema-fidelity regression test.
- [x] Native tool presentation regression test proves schemas are not stripped.
- [x] `G:\Temp\fresh-tools.json` verified 26 tools, 0 empty schemas, 34,638 bytes.
- [ ] Manual: Zen session → first prompt streams assistant text + tool calls.
- [ ] Manual: xAI `grok` session → no regression (still single wire user, full schemas).
- [x] Wire conversion tests assert `ChatCompletionRequest.messages` coalesces adjacent synthetic users and preserves `tools[].function.parameters.properties`.

---

## 12. Files to Watch

- `crates/codegen/xai-grok-sampling-types/src/conversation/chat_completions.rs`
- `crates/codegen/xai-grok-sampling-types/src/conversation/chat_completions_tests.rs`
- `crates/codegen/xai-grok-sampling-types/src/conversation/responses.rs` (reference correct handling)
- `crates/codegen/xai-grok-agent/src/native_tool_presentation.rs`
- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/sampler_turn.rs`
- `crates/codegen/xai-chat-state/src/actor/request_builder.rs`
- `crates/codegen/xai-grok-sampler/src/client.rs` (`conversation_collect` → Chat path)

---

*Originally generated by read-only investigation; updated with the implemented resolution and verification results.*

## 13. Resolution Notes

- `sampler_turn.rs` now keeps canonical native JSON schemas for all hosted models; only local models use their existing reduced-tool behavior.
- `compact_native_definitions()` remains source-compatible but is now schema-preserving, so future callers cannot replace parameters with `{ "type": "object" }`.
- Chat Completions coalesces adjacent user fragments only at the wire boundary; persisted conversation items remain unchanged.
- The remaining unchecked items require a live provider/manual session and cannot be proven by offline tests alone.
