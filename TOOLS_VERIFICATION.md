# Dx Tool Verification Report

**Generated:** 2026-08-18
**Agent:** Dx (EssenceFromExistence)
**Method:** Every tool below was *actually invoked* during this session, not just declared.
**Repo:** `G:\Dx\tui` (Windows / pwsh)

---

## Summary verdict

| Category | Tools available | Working | Partially blocked |
|---|---|---|---|
| Local file / system | 6 | 6 | 0 |
| Planning | 2 | 2 | 0 |
| Subagents | 4 | 4 (callable) | 1 caveat* |
| Tasks / Scheduler | 4 | 4 | 0 |
| Tool discovery | 3 | 3 (callable) | 1 caveat** |
| Web | 2 | 2 | 0 |
| Media (gen/edit) | 4 | 2 | 2 (ZDR policy) |
| User interaction | 1 | 1 | 0 |
| **Total** | **26** | **24** | **2 hard-blocked, 2 caveats** |

\* `spawn_subagent` works, but the `explore` subagent type has **no `run_terminal_command`** in its toolset (only read/file/web/image tools).
\** `workflow` is callable (lookup is wired) but no valid workflow name was discovered to fully execute; `use_tool` is callable but is a router for **MCP integration tools only** (native tools must be called directly).

---

## Detailed table

### Local file / system tools
| Tool | Status | Required params (as learned) | Notes |
|---|---|---|---|
| `list_dir` | ✅ WORKS | `target_directory` | (not `path`) |
| `run_terminal_command` | ✅ WORKS | `command`, `description` | pwsh; ran `echo` + `git rev-parse` |
| `read_file` | ✅ WORKS | `target_file` | (not `path`) |
| `write` | ✅ WORKS | `file_path`, `content` | created temp file, later removed |
| `search_replace` | ✅ WORKS | `file_path`, `old_string`, `new_string` | (not `path`) |
| `grep` | ✅ WORKS | `pattern`, `path` | returned 50 matches across repo |

### Planning tools
| Tool | Status | Required params | Notes |
|---|---|---|---|
| `enter_plan_mode` | ✅ WORKS | — | writes plan to `sessions/<id>/plan.md`, switches to read-only mode |
| `exit_plan_mode` | ✅ WORKS | — | presents plan, requires approval |

### Subagent tools
| Tool | Status | Required params | Notes |
|---|---|---|---|
| `spawn_subagent` | ✅ WORKS | `prompt`, `subagent_type`, `description` | runs in background; returns `subagent_id` |
| `get_command_or_subagent_output` | ✅ WORKS | `task_ids[]`, `timeout_ms` | retrieved subagent result |
| `kill_command_or_subagent` | ✅ WORKS | `task_id` | returned `already_exited` for completed task |
| `monitor` | ✅ WORKS | `command`, `description` | started background monitor, notifies on events |

> Caveat: spawned `explore` subagent reported its toolset = `read_file, list_dir, grep, web_search, web_fetch, image_gen, image_edit, image_to_video, reference_to_video, write, enter_plan_mode, exit_plan_mode, ask_user_question`. It has **no `run_terminal_command`**, so shell commands inside subagents fail.

### Task / scheduler tools
| Tool | Status | Required params | Notes |
|---|---|---|---|
| `todo_write` | ✅ WORKS | `todos[]` (each: `id`, `status`, `activeForm`, `task`) | (not `task`/`status` flat) |
| `scheduler_create` | ✅ WORKS | `schedule`, `prompt`, `name`, `interval` | e.g. `interval:"2h"` → "every 2 hours" |
| `scheduler_list` | ✅ WORKS | — | returned "No scheduled tasks." |
| `scheduler_delete` | ✅ WORKS | `id` | cancelled test task |

### Tool-discovery tools
| Tool | Status | Required params | Notes |
|---|---|---|---|
| `search_tool` | ✅ WORKS | `query` | returns registry status (`total_hidden_tools: 10`); did NOT surface built-ins by keyword |
| `use_tool` | ✅ WORKS | `tool_name`, `tool_input` | MCP-integration router; errors if you pass a native tool (`web_search`) |
| `workflow` | ✅ CALLABLE | `name` | "unknown workflow: test" → lookup wired, but no valid name discovered to fully run |

### Web tools
| Tool | Status | Required params | Notes |
|---|---|---|---|
| `web_search` | ✅ WORKS | `query` | returned current results (Rust 1.97.1) |
| `web_fetch` | ✅ WORKS | `url` | fetched example.com |

### Media tools
| Tool | Status | Required params | Notes |
|---|---|---|---|
| `image_gen` | ✅ WORKS | `prompt`, `size` | saved `sessions/<id>/images/1.jpg` |
| `image_edit` | ✅ WORKS | `image` (**[array]**), `prompt` | saved `2.jpg` |
| `image_to_video` | ⛔ BLOCKED | `image`, `prompt` (+ `output.upload_url`) | **Zero-Data-Retention env requires `output.upload_url`** (HTTP 400) |
| `reference_to_video` | ⛔ BLOCKED | `images` (**[array, ≥2]**), `prompt`, `aspect_ratio` (+ `output.upload_url`) | same ZDR `upload_url` error; also requires ≥2 images |

### User-interaction tool
| Tool | Status | Required params | Notes |
|---|---|---|---|
| `ask_user_question` | ✅ WORKS | `questions[]` (each: `question`, `header`, `multiSelect`, `options[]`) | (not `question`/`options` flat) |

---

## Environment notes

- **`dx` CLI is reachable and works** via `run_terminal_command` (e.g. `dx --help`). Subcommands include: `new, init, dev, build, run, js, py, graph, icon, add, check, style, forge, deploy, serializer, www, native, media, flow, driven, dcp`. Many map to the `dx <subcommand> --help` tools mentioned in the system prompt.
- **Video generation (`image_to_video`, `reference_to_video`) is hard-blocked in this Zero-Data-Retention session** because an `output.upload_url` must be supplied for the generated video. The tools themselves are correctly wired (they validate inputs and call the backend).
- **Subagents cannot run shell commands** with the `explore` type (no `run_terminal_command` in their toolset). File/web/image/read tools do work inside subagents.

---

## Test artifacts created during verification
- `sessions/G%3A%5CDx%5Ctui/<uuid>/plan.md` — plan-mode test file
- `sessions/G%3A%5CDx%5Ctui/<uuid>/images/1.jpg` — generated test image
- `sessions/G%3A%5CDx%5Ctui/<uuid>/images/2.jpg` — edited test image
- A background `monitor` task is still registered (on an already-completed subagent; harmless, auto-expires).
- A temporary `.dx_tool_test_tmp.txt` was created and **deleted** during the `write`/`search_replace` test.
