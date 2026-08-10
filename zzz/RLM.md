# RLM — Recursive Language Models, Correctly Mapped

This document clarifies what "RLM" means across the three places it appears in
the DX stack, and where we genuinely stand vs Prime Agent.

> The core confusion: **the name "RLM" is used for two different architectures.**
> `token/rlm` implements one flavor (recursive long-context document
> processing). Prime Agent markets a different flavor (recursive agent
> orchestration). Both are legitimately "Recursive Language Model" ideas, but
> they are not the same program.

---

## 1. `token/rlm` — our Rust implementation (what it actually is)

`token/rlm` is an embeddable **long-context runtime**. It answers questions,
summarizes oversized files, and builds agent-ready context **without pushing the
whole document into one prompt**.

### Files / layout

| Path | Role |
|---|---|
| `src/rlm.rs` | Core loop, builder, typed document/request/response types, recursive chunk reduction |
| `src/repl.rs` | Sandboxed Rhai executor with fast SIMD substring helpers |
| `src/parser.rs` | `FINAL(...)` answer extraction |
| `src/llm.rs` | OpenAI-compatible provider abstraction (Groq or any chat-completions endpoint) |
| `src/lib.rs` | Public API surface |
| `docs/INTEGRATIONS.md` | Host boundaries (Zed / Codex / ZeroClaw / DX) |
| `docs/PRODUCTION_READY.md` | Scope + operator inputs + defaults |
| `RLM_RESULTS.md` / `ROADMAP.md` / `VERIFICATION.txt` | Status and next-phase notes |

### The architecture

```
document ──► chunk_document()        (boundary-aligned chunks, overlap)
        ──► reduce_request_document() (per-chunk LLM call → [chunk n] summaries)
        ──► final complete_request()  (answer from reduced context)
```

- **Model-writes-code loop** (`run_loop`, `rlm.rs:880`): the model is told it
  is a "Recursive Language Model operating through a constrained Rhai REPL".
  It inspects the document using Rhai (`fast_find`, `window`, `head`, `tail`,
  `fast_count`, …), then finishes with `FINAL("answer")`.
- **Recursion is over text**, not agents: chunk → summarize → merge → answer.
- **Sandbox**: Rhai engine limits (`set_max_operations`, `set_max_expr_depths`,
  `set_max_string_size`, `set_max_array_size`) + a 7-function whitelist.
- **Caching**: AST cache + LLM cache + fast/smart model routing (stats in
  `RLMStats`: `cost_savings`, `cache_hit_rate`).
- **Profiles**: `LowMemory` / `Balanced` / `HighThroughput` map to chunking
  configs.
- **Task kinds**: QuestionAnswering, SummarizeDocument, BuildAgentContext,
  ExtractEvidence.
- **Entrypoints**: `complete_document[_recursive]`, `summarize_document[_recursive]`,
  `build_agent_context[_recursive]`, `complete_file`, plus `_auto` variants.

### Scope boundary (stated in its own docs)

> "This crate solves long-context orchestration, not full remote-agent
> execution." — `docs/PRODUCTION_READY.md`

So `token/rlm` is our Rust implementation of **the RLM *scratchpad / context
layer*** — the part where a model writes code and iterates against a large
body of text. It does **not** spawn subagents.

---

## 2. Prime Agent's RLM (TypeScript + Python) — what *they* mean

Prime Agent's RLM is **recursive agent orchestration**:

- The model decomposes a task, **spawns real child agents** (`rlm(...)`), each
  with a scoped context, and aggregates their results.
- A **persistent IPython kernel** is the model's scratchpad: it writes
  arbitrary Python (pandas/numpy/file/network) at runtime, executes it in a
  stateful REPL, inspects results, iterates.
- Continual Harness + `/refine` persist and self-edit supplemental state.
- Implementation: Node (daemon/worker) + ZeroMQ kernel transport + Python.

### The two flavors side by side

| | `token/rlm` (Rust) | Prime Agent RLM (TS + Py) |
|---|---|---|
| Core recursion | Chunk → summarize → merge (**over text**) | Spawn → execute → aggregate (**over agents**) |
| Model-written code | Rhai, sandboxed, 7 search fns | Python, full ecosystem, persistent IPython |
| Subagent fan-out | None | Yes (`rlm(...)`, parallel/background children) |
| Persistent session state | No | Yes (IPython namespace) |
| `/refine` harness | No | Yes |
| Lang/stack | Rust, single process | Node + ZMQ + Python kernel |

**Conclusion: `token/rlm` is a real Rust RLM, but of the *long-context* flavor.**
It is not a clone of Prime Agent's *agent-orchestration* RLM — and it was never
intended to be (its docs say so).

---

## 3. What grok-build already has (the *other* RLM half, already in Rust)

The main codebase already implements **recursive agent orchestration** — the
half `token/rlm` deliberately does not cover:

| Piece | Where | What it does |
|---|---|---|
| Recursive child spawn | `task` tool → `SubagentCoordinator` (`crates\codegen\xai-grok-tools\src\implementations\grok_build\task\coordinator.rs:63`) | Single-writer actor spawning child agents |
| Parallel fan-out | 4 `FuturesUnordered` collections (`coordinator.rs:72-77`) | Concurrent child runs / validations / descriptions / progress |
| Context scoping | `SubagentSpawnContext` (`crates\codegen\xai-grok-shell\src\agent\subagent\mod.rs:102`) | Child inherits lsp, cwd, auth, model, memory config from parent |
| Fork / resume | `InitialContextSource::{New, Forked, Resumed}` (`subagent\mod.rs:51`) | Child from scratch, from parent history, or continuing a completed child |
| Programmatic recursion | `xai-workflow` engine — `agent()`, `parallel()`, `budget()` | Rhai-scriptable recursion with hard budgets (`DEFAULT_AGENT_BUDGET=128`, `MAX_PARALLEL=1024` — `crates\codegen\xai-workflow\src\lib.rs:14`) |
| Result aggregation | `AgentResult {success, output, tokens_used, duration_ms}` | Child results returned to the caller |
| Autonomy + verification | `/goal`, `GoalOrchestration` (`goal_tracker.rs:422`), classifier ack variants | Cross-round autonomy with independent evidence verification |

So in reality:

- **grok-build owns the RLM-agent orchestration half** (subagents, parallel
  fan-out, recursive workflows, budgets) — equivalent to Prime Agent's RLM
  *core*, in native Rust.
- **`token/rlm` owns the RLM-context scratchpad half** (model-writes-code
  against large text, recursive chunk reduction) — the *supplemental* RLM idea,
  in native Rust.

Prime Agent happens to have **both** ideas fused in one product; we have them
**both** in Rust, and the long-context half is now exposed to the agent loop as
the bounded `GrokBuild:rlm` read-only tool. The existing `task`/workflow
surfaces remain the agent-recursion layer.

---

## 4. What is genuinely missing vs Prime Agent

1. **Python scratchpad** — `token/rlm` uses a tiny sandboxed Rhai with search
   helpers. Prime Agent gives the model full Python. We can keep Rhai or add a
   pluggable runtime (Rhai / Lua / RustPython / PyO3) behind a feature flag —
   pure-Rust default, Python optional. (Note: the whole-project xai stack is
   already Rhai-native — see §5 — so Rhai is the zero-mismatch choice.)
2. **Deeper wiring of the two halves** — `GrokBuild:rlm` now runs the
   `token/rlm` loop inside a turn and can return `agent_context` for a later
   child task. It does not yet create child agents directly from inside one RLM
   invocation; composition is still explicit through `task` or `xai-workflow`.
3. **`/refine` harness** — evidence-backed self-editing of supplemental state
   with snapshot rollback (memory today is append-only capture).
4. **Depth cap** — `MAX_SUBAGENT_DEPTH: u32 = 1` (`crates\codegen\xai-grok-tools\src\implementations\grok_build\task\mod.rs:37`) limits deep recursion; true "deep RLM" needs the cap raised and propagated.

---

## 5. Adding RLM to DX — verified integration facts

> "dx-tui" = the whole repo (`G:\Dx\tui`), a single Cargo workspace containing
> `xai-grok-tools`, `xai-grok-shell`, `xai-workflow`, `xai-grok-memory`, plus
> the standalone `token/rlm` crate (currently **not** a workspace member).

### Why it will NOT break

- **The script engine already matches.** `xai-workflow` already depends on
  `rhai` and uses `rhai::Engine` + `rhai::Scope` (`engine.rs:119,171`) — the
  exact same engine `token/rlm`'s REPL uses (`repl.rs:21`). The whole-project
  agent stack is already Rhai-native; adding `token/rlm` introduces **no new
  interpreter** and **no Lua/Rhai conflict**. (Lua only lives in `dx-tui`'s own
  plugin layer, a separate crate.)
- **Dependencies already in the graph.** `token/rlm`'s deps (rhai, reqwest,
  tokio, serde, regex, memchr) are all already used by the workspace crates.
- **No workspace conflict.** `token/rlm` is standalone today (not in the root
  `Cargo.toml` members list); adding it as a member is additive, not a move.

### Why it will produce Prime-Agent-equivalent RLM (if wired correctly)

The orchestration chain is already live and verified in source:

| Chain link | Crate | Evidence |
|---|---|---|
| `xai-grok-shell` links tools + workflow + memory | `xai-grok-shell/Cargo.toml` | `xai-grok-tools`, `xai-workflow`, `xai-grok-memory` path deps |
| Recursive child spawning | `xai-grok-tools` | `task` tool → `SubagentCoordinator`, `spawn_with_foreground_wait` (`backend.rs:272`) |
| Programmatic recursion in Rhai | `xai-workflow` | `agent()`, `parallel()`, `budget()` host fns (`engine.rs:459+`) |
| Long-context scratchpad (model-writes-code) | `token/rlm` | persistent `Scope` across iterations (`rlm.rs:908`) |

The high-impact connections are now in place, but full Prime Agent parity is
still not claimed:
1. ✅ DONE: `token/rlm` → `crates/codegen/xai-rlm` (workspace member) and
   `GrokBuild:rlm` (registry + agent-loop tool). The tool enforces source bounds,
   workspace-local file access, cancellation, timeout, and bounded recursion.
2. ✅ DONE: `/refine` snapshot/diff/rollback layer built as `xai-grok-refine`
   (workspace member, 19/19 unit tests) and wired into the session actor as a
   `/refine` slash command (status / rollback / create / update / delete).

### Does "just add rlm" alone give Prime RLM? No.

`token/rlm` by itself (even added to the workspace) provides only the
*scratchpad/long-context half*. Prime Agent RLM = that half **+ recursive agent
orchestration**. The orchestration half already exists in `xai-grok-tools` +
`xai-workflow` — so the whole-project answer is **yes with wiring**, but the
bare dependency line alone is not enough.

---

## 6. The corrected DX plan

| Item | Status | Action |
|---|---|---|
| Recursive agent orchestration (RLM-core) | ✅ In `xai-grok-tools` + `xai-workflow` | Keep; optionally raise depth cap |
| Long-context scratchpad (RLM-context) | ✅ `crates/codegen/xai-rlm` + `GrokBuild:rlm` | Use bounded tool; direct child spawning inside RLM remains follow-up |
| Rhai engine consistency | ✅ `xai-workflow` already Rhai | No interpreter mismatch to resolve |
| Model-authored code execution | ✅ Bounded Rhai RLM tool | Add a persistent Python-class runtime only if product requirements justify it |
| `/refine` harness with rollback | ✅ `xai-grok-refine` + `/refine` slash command (verified, wired) | Engine host-fn registration + LLM planner (follow-up) |
| Peer agent→agent messaging | ⚠️ Workflow orchestration exists | Add runtime message bus |
| Explicit `/autonomous` surface | ✅ `/goal` covers it | Thin wrapper + quality gates |

We do not need to "port" Prime Agent. The RLM ideas are already split across
our two Rust layers; the work is joining them and filling the three real gaps.
Adding `token/rlm` to the workspace will **not** break the build — the
integration is wiring work, not a rewrite.
