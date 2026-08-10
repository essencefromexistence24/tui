# Tool Schemas — Token Size Report

Measured with `dx token count --json` against the real serialized tool-definition
payload of the `grok-build-plan` agent, dumped from a live `AgentBuilder` build.

## Summary

| Metric | Value |
|---|---|
| Number of tools | 19 |
| Serialized payload | 28,127 bytes (pretty JSON, Chat Completions shape) |
| Tokens (o200k-base) | 6,197 |
| Tokens (cl100k-base) | 6,191 |
| Tokens (p50k-base) | 7,257 |
| Tokens (r50k-base) | 12,563 |
| Tokens per byte (o200k) | ~0.22 |

The tool schemas are the dominant fixed-cost component of the first-turn prompt
payload. They are generated at runtime via `schemars` from the Rust tool structs —
there are **no static schema JSON files** in the repo.

## How it was measured

```powershell
# 1. Dump the real tool definitions from a grok-build-plan agent build:
#    crates/codegen/xai-grok-agent/tests/dump_tools.rs
#    writes serde_json::to_vec_pretty(tool_definitions_builtins_only()) to
#    G:\Temp\UserTemp\opencode\grok_build_plan_tools.json

# 2. Count the whole payload:
dx token count grok_build_plan_tools.json --json

# 3. Count each tool individually:
dx token count "per_tool\*.json" --json
```

`dx token count` tokenizers available: `cl100k-base`, `p50k-base`, `r50k-base`,
`o200k-base`, `character`, `word`, `heuristic`.

## Per-tool breakdown (sorted by o200k-base tokens)

| # | Tool | Bytes | cl100k | o200k | p50k | r50k |
|---|------|------:|-------:|------:|-----:|-----:|
| 1 | `grep` | 2,761 | 651 | 651 | 755 | 1,173 |
| 2 | `scheduler_create` | 2,742 | 619 | 620 | 715 | 1,099 |
| 3 | `run_terminal_command` | 2,622 | 582 | 581 | 639 | 855 |
| 4 | `read_file` | 2,176 | 516 | 516 | 579 | 821 |
| 5 | `ask_user_question` | 2,613 | 470 | 469 | 558 | 1,392 |
| 6 | `monitor` | 1,828 | 423 | 425 | 492 | 708 |
| 7 | `todo_write` | 1,982 | 389 | 390 | 476 | 1,012 |
| 8 | `get_command_or_subagent_output` | 1,655 | 385 | 388 | 445 | 623 |
| 9 | `search_replace` | 1,470 | 340 | 341 | 407 | 577 |
| 10 | `update_goal` | 1,442 | 312 | 311 | 363 | 583 |
| 11 | `use_tool` | 1,027 | 248 | 248 | 301 | 409 |
| 12 | `search_tool` | 999 | 232 | 233 | 277 | 431 |
| 13 | `list_dir` | 898 | 201 | 201 | 241 | 313 |
| 14 | `write` | 734 | 173 | 173 | 211 | 311 |
| 15 | `kill_command_or_subagent` | 677 | 166 | 166 | 205 | 271 |
| 16 | `scheduler_delete` | 565 | 138 | 138 | 173 | 239 |
| 17 | `enter_plan_mode` | 494 | 117 | 117 | 141 | 165 |
| 18 | `exit_plan_mode` | 401 | 101 | 101 | 126 | 150 |
| 19 | `scheduler_list` | 355 | 88 | 88 | 112 | 136 |
| | **Sum of individual files** | **28,127** | **6,151** | **6,157** | **7,206** | **12,528** |

> Note: summing the per-tool files (6,157 o200k) is slightly lower than the whole
> payload (6,197 o200k) because the whole payload wraps all tools in a JSON array
> (`[`, `]`, inter-object commas). The whole-payload number matches what is
> actually serialized to the model.

## Top-5 heaviest tools (o200k)

1. `grep` — 651 tokens (2,761 B)
2. `scheduler_create` — 620 tokens (2,742 B)
3. `run_terminal_command` — 581 tokens (2,622 B)
4. `read_file` — 516 tokens (2,176 B)
5. `ask_user_question` — 469 tokens (2,613 B)

## How this relates to the 12,804-token first prompt

Measured first-turn payload components (o200k-base):

| Component | Tokens |
|---|---|
| Tool schemas (19 tools, whole payload) | 6,197 |
| `prompt.md` (base system template) | 982 |
| `AGENTS.md` (workspace) | 437 |
| `subagent_prompt.md` | 1,048 |
| `apply_patch_prompt.md` | 4,498 |
| `<user_info>` prefix + user query | ~500 |

The log that reported `input_tokens=12804` (2026-07-27, model
`minicpm5-1b-tooluse` via llama-server) shipped **26 tools** and was tokenized by
MiniCPM5's tokenizer, which counts JSON more densely than cl100k/o200k — that
explains the gap between the ~6,200 counted here and the 12,804 seen in the log.

Commit `dc885b4` ("fix: make local model responses reliable", 2026-07-31) clears
these tool definitions (`defs.clear()` in `prepare_tool_definitions_inner`,
`sampler_turn.rs`) when the backend is a local model, eliminating this ~6,200-token
overhead for local llama-server sessions. Hosted models still receive all schemas
on every first turn.
