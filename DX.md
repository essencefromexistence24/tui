# DX — Why We Win, and the Plan to Prove It

## The reality check

Prime Agent is a fork of the `pi` coding agent, re-branded and shipped with a
single novel idea — the RLM (Recursive Language Model) pattern built around a
persistent IPython REPL. It went viral on the strength of that one idea and the
Prime Intellect brand. It is not a superior agent. It is not a superior TUI. It
is a TypeScript codebase layered on a terminal UI inherited from `pi`, wrapped
in daemon processes and a viral blog post.

DX is a professional Rust codebase. Grok Build's agent runtime is a mature,
multi-crate architecture (64+ codegen crates) with a full-screen Rust TUI built
on Ratatui — and on top of that we merged the entire dx-tui component suite:
minimap, sidebar, embedded code editor, file browser, diff viewer, command
palette, and animations, all in one native process.

### Head-to-head — why we are not close in their favor

| Dimension | Prime Agent | DX (Grok Build + dx-tui) |
|---|---|---|
| Language | TypeScript/Node | Rust (native, no VM, no JIT) |
| TUI | Inherited `pi` TUI, character-cell terminal UI | Best-in-class Rust TUI: minimap, sidebar, embedded editor, file browser, diff viewer, command palette, animations |
| Runtime model | Daemon + worker + kernel + ZeroMQ processes | Single native process, in-process leader/session architecture |
| Startup / latency | Node startup + daemon handshake + kernel spawn | Native Rust startup, TTFT dominated only by the model |
| Performance | Interpreted, allocation-heavy | Zero-cost abstractions, no runtime |
| Headless / scripting | JSONL / RPC modes bolted on | Full `-p` headless with json / streaming-json / streaming-messages-json |
| Interop | ACP client | ACP client **and** JSON-RPC leader relay, SDK MCP servers |
| Platform | macOS / Linux | macOS / Linux / Windows |
| Video | — | Native `/video` playback in a separate window |
| Memory | Refine-the-harness | Cross-session memory with `/dream` consolidation, staleness, file watcher |
| Workflows | — | Rhai-based workflows with pause/resume, agent budgets, `/deep-research` with adversarial claim verification |

The one place Prime Agent leads is a **single product idea**: a persistent Python
REPL as the model's tool, with recursive subagents and a self-improving harness.
That is a feature gap, not an architecture gap. Feature gaps we close in Rust.

---

## The honest audit — what we already have vs what we truly lack

Before building anything, we audited their headline features against our
codebase. Result: their "unique" features are mostly **already implemented here
under different names**. Only three are real gaps.

| # | Prime Agent feature | Our equivalent | Verdict |
|---|--------------------|----------------|---------|
| 5 | `/autonomous` + quality gates | **`/goal`** — cross-round autonomy, token budget (`--budget`), independent classifier/evidence verification (`UpdateGoalAck::Classifier*`), adversarial verification via workflows, pause/resume/clear, stall/block detection | ✅ Already have |
| 3 | `/refine` continual harness | **Memory** (`/flush`, `/dream`, `/memory`, auto-save, staleness, file watcher) | ⚠️ Partial — capture yes, self-editing state with rollback no |
| 4 | Peer agent→agent messaging | **Workflow orchestration** (`agent()`, `parallel()`, `budget()`), subagents, dashboard dispatch | ⚠️ Partial — orchestration yes, live peer message bus no |
| 2 | Python skills as importable packages | **Skills** — markdown/prompt packages, `/create-skill`, plugins | ❌ Missing — no Python entrypoints |
| 1 | RLM / persistent IPython REPL | **None** — `python3` only ever spawned as a subprocess; no embedded interpreter | ❌ Missing |

So the mission is **three** real features, plus one enhancement to goal mode
that makes our autonomy story explicit.

---

## The mission

We will implement the three real gaps natively in Rust, faster, safer, and
better integrated — then launch DX publicly and let the benchmark compare the
engineering.

Three features to build, plus one explicit-autonomy enhancement:

---

### 1. RLM / persistent Python REPL — programmatic context as variables

**What they have:** a persistent IPython kernel where the model's working
context lives in live Python variables; tools like file ops and shell run
through code; recursive subagents are function calls that return values.

**What we build (in Rust):**
- Embed a persistent Python interpreter as a first-class tool, backed by
  PyO3 (or a dedicated kernel subprocess with our own protocol) so we do not
  inherit their ZeroMQ/IPython transport baggage.
- Live namespace shared across turns: variables, dataframes, function
  definitions persist in session state, snapshot-able and restorable.
- A `repl` tool: `repl.eval(code) -> (result, stdout, state_delta)`, with a
  guardrail layer (timeouts, memory caps, kill-on-hang) that Rust enforces
  natively — not a forked sandbox script.
- Result objects flow back as first-class tool results that the model can
  reference by handle, not re-printed text.

**Why we win:** their REPL is a subprocess protocol (`rlm-runtime.md`: ZeroMQ
kernel transport). Ours is embedded and memory-safe; state snapshots and
rollback are ours natively.

---

### 2. Executable Python skills — skills as importable packages

**What they have:** skills are importable Python packages; the built-in skill
creator turns recurring workflows into project or personal skills.

**What we lack:** our skills are markdown/prompt packages. There is no Python
runtime, no package entrypoint, no dependency handling.

**What we build (in Rust):**
- Skills become packages with a manifest (`skill.toml`) and an optional Python
  entrypoint. The Rust skill runner executes the package with its declared
  dependencies in a scoped environment.
- The built-in skill creator runs as a Rust-driven flow: capture the workflow,
  generate the package scaffold, wire it into the slash-command menu and the
  `use_tool` dispatch — same UX, native implementation.
- Keep existing markdown skills; add the `python` backend as a parallel skill
  type. No regression for current users.

**Why we win:** their skills ride on the IPython kernel; ours will work with or
without a REPL session, typed and schema-checked by Rust at load time.

---

### 3. `/refine` — continual harness, evidence-backed self-improvement with rollback

**What they have:** `/refine` reviews the trajectory and applies small,
evidence-backed updates to supplemental prompt/memory/skill/subagent state,
with recorded refinement history and snapshot rollback; the base system prompt
is immutable.

**What we lack:** we capture memory, but memory is append-only capture. We do
not have a self-editing layer: evidence-backed updates, refinement history, or
snapshot rollback over the immutable base prompt.

**What we build (in Rust):**
- A harness layer over our existing memory system: durable, reviewable
  "lessons" as supplemental prompt blocks, memories, skill descriptions, and
  subagent specs.
- Evidence-backed writes: each refinement records the supporting turn/source,
  an LLM-generated summary, and a confidence signal. The base prompt stays
  immutable — exactly their rule.
- Snapshot + rollback: every refinement writes a session snapshot; `/refine
  rollback <n>` restores it atomically. Refinement history is a first-class
  entity in session state.
- Reuse our memory index (`index.sqlite`) and staleness/file-watcher machinery
  instead of building a parallel store.

**Why we win:** they built a harness store from scratch; we already have
cross-session memory, dream consolidation, and staleness detection. We attach
the refine loop to an existing, tested persistence layer.

---

### 4. Peer agent-to-agent direct messaging

**What they have:** running agents and retained subagents discover each other,
exchange messages, and steer active work without routing through the user.

**What we lack:** we orchestrate (workflows, subagents, dashboard dispatch) but
have no runtime message bus — no live agent-to-agent channel outside a model's
prompt loop.

**What we build (in Rust):**
- A session-level message bus keyed by agent/session ID. Any running subagent
  or top-level session can `send(agent_id, message)` and `receive(timeout)`.
- Discovery: an `agents()`/roster endpoint returning live agent IDs, roles, and
  capabilities (we already maintain roster state for the dashboard).
- Steering: a host-side relay that delivers messages without the model being in
  the loop — this is a runtime service, not a prompt trick.
- Integrate with existing subagent + workflow infrastructure: workflows may
  already orchestrate; peer messaging adds the direct channel.

**Why we win:** their messaging rides on daemon processes; ours is an in-process
bus with zero serialization overhead, and it composes with the dashboard and
workflow roster we already ship.

---

### 5. Explicit autonomous mode (`/autonomous`)

**What they have:** `/autonomous` continues within configured turn, token, and
time budgets and runs user-defined quality gates; a passed gate checks only
what that gate verifies.

**What we already have:** `/goal` covers the core — cross-round autonomy, token
budget, classifier + adversarial verification, pause/resume/clear, stall/block
detection.

**What we build (in Rust):**
- An explicit `/autonomous` surface that wraps `/goal` semantics with
  user-defined quality gates as executable scripts (or harness checks) in
  config; a gate runs after a completion candidate and only a verified pass
  marks the objective done.
- Rust-enforced hard turn/token/time caps on top of the existing token budget,
  not prompt admonition.
- Compose with `/goal` (already evidence-verified) and `/workflows`
  (already bounded by `agent_budget`).

**Why we win:** we already ship the verification engine — classifier
(`UpdateGoalAck`), adversarial workflows, `agent_budget`. We are adding the
explicit surface and executable gates, not rebuilding the machinery.

---

## Execution order

1. **Peer messaging (4)** — smallest surface, highest foundation value; enables
   the other agent-centric features and reuses the roster.
2. **`/autonomous` surface (5)** — thin wrapper over the goal engine we already
   ship; fast to build, strong marketing story.
3. **Persistent Python REPL (1)** — the flagship; needs the PyO3/embedding
   spike and the guardrail layer first.
4. **Python skills (2)** — depends on the REPL runtime; reuses existing skill
   loader/dispatch.
5. **`/refine` harness (3)** — builds on existing memory; last because it needs
   the most design care around immutable-base-prompt semantics and rollback.

Each feature lands behind a feature flag, ships with tests, and ships in Rust.

---

## What launching DX looks like

- Every feature here is native Rust, single-process, with the professional TUI
  we already have. No fork of `pi`, no Node runtime, no daemon-process tax.
- We close the three real feature gaps (REPL, Python skills, refine) and make
  our existing autonomy explicit — then publish the comparison openly. Much of
  what they market as novel (autonomous goals, verification) is already ours
  under different names.
- The TUI alone is the differentiator; closing their unique feature gaps
  removes every reason to choose Prime Agent.

## The one rule

Everything we build is native Rust, in our codebase, under our architecture.
We do not port their code. We implement their ideas — then ship ours.
