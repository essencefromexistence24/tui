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
**both** in Rust, but in two crates that are not yet wired together.

---

## 4. What is genuinely missing vs Prime Agent

1. **Python scratchpad** — `token/rlm` uses a tiny sandboxed Rhai with search
   helpers. Prime Agent gives the model full Python. We can keep Rhai or add a
   pluggable runtime (Rhai / Lua / RustPython / PyO3) behind a feature flag —
   pure-Rust default, Python optional.
2. **Wiring the two halves** — `token/rlm` is not called from the agent loop.
   Integration target: a model-authored-code tool (scratchpad) that runs the
   `token/rlm` loop (or an embedded interpreter) inside a turn.
3. **`/refine` harness** — evidence-backed self-editing of supplemental state
   with snapshot rollback (memory today is append-only capture).
4. **Depth cap** — `MAX_SUBAGENT_DEPTH: u32 = 1` (`crates\codegen\xai-grok-tools\src\implementations\grok_build\task\mod.rs:37`) limits deep recursion; true "deep RLM" needs the cap raised and propagated.

---

## 5. The corrected DX plan

| Item | Status | Action |
|---|---|---|
| Recursive agent orchestration (RLM-core) | ✅ In grok-build | Keep; optionally raise depth cap |
| Long-context scratchpad (RLM-context) | ✅ In `token/rlm` (Rust + Rhai) | Wire into the agent loop as a tool |
| Model-authored code execution | ⚠️ Rhai-only, not exposed | Pluggable interpreter tool (feature-gated) |
| `/refine` harness with rollback | ❌ | Build on existing memory |
| Peer agent→agent messaging | ⚠️ Workflow orchestration exists | Add runtime message bus |
| Explicit `/autonomous` surface | ✅ `/goal` covers it | Thin wrapper + quality gates |

We do not need to "port" Prime Agent. The RLM ideas are already split across
our two Rust layers; the work is joining them and filling the three real gaps.
