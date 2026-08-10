# REFINE — Continual Harness for DX, in Rust

Prime Agent's `/refine` is ~941 lines of TypeScript (`refinement.ts`) wrapping a
4-collection CRUD store (prompt notes / memory / skills / subagent specs) with
a trajectory-review planner and snapshot rollback. The feature is real; the
code is small. We can ship the same capability in Rust on top of
`xai-grok-memory` — the storage, index, and search layers already exist in this
repo and are more capable than what Prime Agent uses.

Goal: a `/refine` command + a `harness` CRUD surface exposed to the model, with
evidence-backed edits and rollback-by-ID, running on the existing DX agent loop.

---

## 1. What we build (feature parity checklist)

| Prime Agent `/refine` | DX Rust equivalent |
|---|---|
| `HarnessState { prompt, memory, skills, subagents }` (4 collections) | `HarnessState` struct over `xai-grok-memory` storage |
| `rlm.harness.create_/update_/delete_(prompt_note\|memory\|skill\|subagent)` | Rhai `harness` module registered on the existing `xai-workflow` engine |
| `refine.run("...")` — plan (background) → apply (fast) | `refine` tool: `plan_refinement()` (LLM call) + `apply_refinement()` (local write) |
| Trajectory review → smallest CRUD edit | Trajectory pulled from the session/actor state, reviewed by the existing `LLMClient` |
| Snapshot history + rollback by refinement ID | Append-only refinement log + `baseline_state` restore |
| Auto-refine on turn interval / compact | Optional trigger wired to turn-end hook |
| Immutable base system prompt | System prompt stays untouched; harness layer is additive |

---

## 2. Grounding: what `xai-grok-memory` already gives us

Verified in `crates/codegen/xai-grok-memory/src`:

- **`MemoryStorage`** (`storage.rs:27`) — `MemoryScope { Global, Workspace, Session }`,
  `append_to_memory`, `write_long_term`, `read_file`, `list_memory_files`,
  `global_dir`/`workspace_dir`, `gc`, `clear_*`.
  → This is our harness store. Each harness collection = one memory file
  (`harness_prompt.md`, `harness_memory.md`, `harness_skills.md`,
  `harness_subagents.md`) under the workspace scope, or a JSON entry in a
  versioned log for rollback.
- **`MemoryIndex`** (`index.rs:83`) — SQLite + FTS5 (`open_or_create`,
  `reindex_file`, `search_fts`, `get_chunk`, `record_access`) and optional
  SQLite-vec.
  → Retrieval for trajectory evidence lookup.
- **`hybrid_search`** (`search.rs:146`) — semantic + FTS hybrid.
  → Lets the planner pull relevant past lessons before proposing an edit.

No new storage engine needed. We add a thin `refine` layer on top.

---

## 3. Module layout (new crate: `xai-grok-refine`)

```
crates/codegen/xai-grok-refine/
├── Cargo.toml            # deps: xai-grok-memory, rhai, serde, serde_json, tokio, tracing
└── src/
    ├── lib.rs            # pub API + re-exports
    ├── state.rs          # HarnessState, 4 collections, CRUD ops, merge
    ├── log.rs            # append-only refinement log, snapshot, rollback-by-ID
    ├── plan.rs           # plan_refinement(): trajectory → RefinementProposal (LLM call)
    ├── apply.rs          # apply_refinement(): smallest CRUD edit + snapshot write
    ├── rhai.rs           # register harness.* + refine.* on rhai::Engine (xai-workflow style)
    └── prompt.rs         # "you are a continual harness" doctrine text (like prompts/rlm.ts)
```

Estimated size: **~700–1,000 Rust lines** total — same order as Prime Agent's
TS, minus their daemon/RPC/ZMQ plumbing (we already have none of that).

---

## 4. Core types

```rust
// state.rs
#[derive(Serialize, Deserialize, Clone)]
pub struct HarnessState {
    pub prompt_notes:  Vec<HarnessEntry>, // kind = "prompt"
    pub memories:      Vec<HarnessEntry>, // kind = "memory"
    pub skills:        Vec<HarnessEntry>, // kind = "skill"
    pub subagents:     Vec<HarnessEntry>, // kind = "subagent"
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HarnessEntry {
    pub id: String,             // stable, e.g. slugified
    pub kind: String,           // prompt | memory | skill | subagent
    pub title: String,
    pub content: String,
    pub evidence: Option<String>, // the trajectory fragment that justified it
    pub created_at: String,
    pub updated_at: String,
}

// log.rs
pub struct RefinementLog { /* append-only; each entry stores a full state snapshot */ }

pub struct RefinementResult {
    pub id: String,               // rollback target
    pub kind: RefinementKind,
    pub action: RefinementAction, // create | update | delete
    pub trigger: String,          // user | auto_turn | auto_compact | explicit
    pub outcome: String,
    pub baseline_state: HarnessState, // for rollback
}
```

CRUD surface mirrors Prime Agent's names so the RLM prompt stays recognizable:

```rust
pub fn create(&mut self, kind, title, content, evidence) -> HarnessEntry;
pub fn update(&mut self, id, content) -> Option<HarnessEntry>;
pub fn delete(&mut self, id) -> Option<HarnessEntry>;
pub fn list(&self) -> Vec<HarnessEntry>;
pub fn get(&self, id) -> Option<&HarnessEntry>;
pub fn record_refinement(&mut self, result: RefinementResult);
pub fn overview(&self) -> String; // compact prompt dump, bounded
pub fn rollback(&mut self, refinement_id) -> Result<()>;
```

---

## 5. Plan → apply pipeline

1. **Plan** (`plan.rs`, runs in background, does not block the conversation):
   - Collect trajectory window (last N turns / tool calls) from the session.
   - Optional: `hybrid_search` over past harness state for related lessons.
   - One `LLMClient.complete()` call → `RefinementProposal { kind, action, title,
     content, evidence }` — constrained to *one smallest* edit.
2. **Apply** (`apply.rs`, fast, at next turn boundary):
   - Snapshot current `HarnessState` → append to `RefinementLog` (this is the
     rollback point).
   - Apply the single CRUD edit.
   - Write state to disk (`xai-grok-memory` storage).
   - Rebuild the additive harness prompt fragment for the next turn.
3. **Rollback**: `/refine rollback <id>` restores `baseline_state` from the log.

Base system prompt is never rewritten — the harness layer is purely additive,
exactly like Prime Agent's guardrail.

---

## 6. Wiring into DX — DONE (v1)

- **Slash command**: `/refine status | rollback <id> | create <kind> <title>: <content>
  | update <kind> <id>: <content> | delete <kind> <id>` — `BuiltinGate::AlwaysOn`,
  dispatched in `xai-grok-shell/src/session/acp_session_impl/slash_exec.rs`,
  executed by `acp_session_impl/refine.rs`. `RefineSession` is owned per-session
  (`SessionActor.refine`), loaded/persisted under the session dir
  (`harness_state.json` + `refinement_log.json`).
- **Model-facing tool**: register `harness.*` and `refine.*` on the
  `xai-workflow` Rhai engine (`engine.rs:459` already registers host fns this
  way) — follow-up, not yet wired. The registration API already exists
  (`xai_grok_refine::rhai::register_refine_fns`).
- **Auto-trigger** (optional, not yet wired): turn-end hook runs a planner
  when a repeated failure signature is seen (same hook family as `/goal`'s
  classifier-ack path in `goal_tracker.rs`).

## 7. Honest scope note

- Prime Agent's `/refine` is ~941 lines because it is only a small part of a
  big TS app. We are adding a comparable module to an already-larger Rust
  agent stack that already owns orchestration (`xai-grok-tools` +
  `xai-workflow`), memory (`xai-grok-memory`), and autonomy (`/goal`). The
  feature is the same; the surrounding harness is not — ours is native.
- Storage already exists. Retrieval already exists. Engine registration already
  exists. The genuinely new code is the state/log/plan/apply module (~700–1k
  lines) and the doctrine prompt.

---

## 8. Ship order — v1 SHIPPED

1. ✅ `state.rs` + `log.rs` (CRUD + snapshot/rollback) — pure Rust, unit-tested.
2. ✅ `prompt.rs` + `rhai.rs` (doctrine + `harness.*` surface).
3. ⏳ `plan.rs` + `apply.rs` (LLM planner + single-edit apply) — v1 applies
   deterministic structured edits from the slash command instead; the LLM
   planner is the next step.
4. ✅ `/refine` slash command + per-session harness state.
5. ✅ Tests: CRUD round-trip, rollback restores baseline, slug stability,
   persistence across sessions (19 unit tests + doc test, in-crate; shell-side
   grammar test in `acp_session_impl/refine.rs`).

Status: `xai-grok-refine` is a workspace member, `cargo check`/`clippy` clean,
wired into the session actor. Next: engine host-fn registration, LLM planner,
auto-trigger, long-horizon evals.
