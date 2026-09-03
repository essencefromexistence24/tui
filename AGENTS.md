# AGENTS.md — DX TUI workspace

## Tool schemas: compact by default, full on demand

The system prompt carries a **compact tool list** (one line per tool: name + arg
hints) plus the full JSON schemas in the `tools` array. It does NOT paste every
tool's full documentation into the prompt.

- If a compact tool line is hard to understand, call **`tool_details`** with
  `{"tool_name": "<name>"}` (aliases: `get_tool_details`, `tool_schema`).
  It returns the FULL JSON schema (description + every parameter with types,
  defaults, enums) for exactly that one tool.
- Rule of thumb: use `tool_details` only after ~3 failed calls of the same tool.
  The compact list is normally sufficient.
- `tool_details` schemas come from the same constructors as the wire `tools`
  array (`openai_tool_schemas`), so they cannot drift.

## If really hard: record the fix here

`tool_details` auto-records each fetched schema in the workspace `AGENTS.md`
under `## Tool Schemas (recorded by tool_details)` as `### Tool Schema: <name>`.
If the full schema was still unclear, append a short usage note under the same
heading:

```md
### Tool Schema: edit
`Fix:` pass `path`, `old_string`, `new_string`; set `replace_all: true` when the
match appears more than once.
```json
{ ...full schema... }
```
```

Rules for the recorded section:

- One section per tool; re-fetching a tool REPLACES its section (never duplicate).
- User content above the auto-recorded heading is preserved — never rewrite it.
- In read-only modes (Ask / Plan) `tool_details` returns the schema WITHOUT
  writing `AGENTS.md`.
- Future sessions in the same workspace read this file first, so a recorded fix
  pays off forever without growing the first prompt.

## Token budget: why the first turn was ~2k and how it stays ~1.5k now

Measured first-turn budget (chars/4 estimate):

| Layer | Before | Now |
|---|---|---|
| Base prompt + compact tool list | ~240 | ~270 (added 2-line `tool_details` hint) |
| Profile policy + reasoning guide | ~110 (forced `<think>`) | ~110, no forced thinking |
| First-turn `TITLE` block | ~250 in / ~40 out (14–28 words, 3 sidebar lines) | ~90 in / ~15 out (6–10 words; parser floor is 6 words / 36 chars) |
| Empty `<skills_index>` placeholder (Agent/Goal) | ~30 | 0 (omitted when no skills) |
| `tool_details` schema (one small tool) | — | ~60 |
| Other JSON (history, wire envelope) | <500 | <500 |
| **Model-visible reasoning (`<think>`)** | **~300–500 billed output tokens** | **~0 for routine work** |

### The reasoning overhead, explained

`profile_layer()` used to instruct the model to "show your step-by-step analysis
inside `<think>` tags" on every mode, every turn — while the Zen wire path
(`zen.rs`) never even sends the UI's `reasoning_effort` value to the provider,
so the model could not know the effort was Medium and many models treated the
instruction as "always think out loud". Visible `<think>` tokens are billed as
output tokens AND retained in context (re-billed as input on later turns).

That is the ~500-token gap: system (~1000) + JSON (<500) = <1.5k expected, but
forced thinking + long TITLE pushed every first turn to ~2k — paid by us on
every session.

Pi, OpenCode and other harnesses do not inject visible-think instructions and do
not force title generation; they rely on hidden provider-side reasoning (or
none) and pay input + answer only.

Fixes applied here (support for `<think>` is kept — the stream renderer parses
it and markdown recovery strips it — it is just no longer forced):

- Reasoning guide says: keep reasoning internal, no `<think>` for routine work,
  1–3 sentence plan for multi-step changes, never put tool commands in thinking.
- `TITLE` is a 6–10 word chat name, not a 90–180 char sidebar paragraph.
- Empty skills index is omitted instead of sending a placeholder.
- If per-model control is ever needed, send `reasoning_effort` on the wire
  (currently UI-only for the Zen path) rather than prompting for visible CoT.
