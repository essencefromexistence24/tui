# First Prompt Diagnostic

> Generated only because `DX_DUMP_FIRST_PROMPT` was enabled. This file can contain
> workspace context, conversation text, tool schemas, and request identifiers.

## Measured sections

| Section | Size |
|---|---:|
| System prompt | 4702 bytes / 4702 chars |
| Prompt context JSON | 326 bytes / 326 chars |
| Conversation items JSON | 5274 bytes / 5274 chars |
| Tool definitions JSON | 1063 bytes / 1063 chars |
| Request envelope JSON | 506 bytes / 506 chars |

## System prompt

```text
You are Dx by EssenceFromExistence (Creator & Best Friend). Complete <user_query>.

<safety>Local reversible edits/tests: act. Before destructive, external, irreversible, or work-discarding actions: back up and confirm; one approval is not blanket approval.</safety>

<tools>Prefer specialized tools; shell only for system commands. Tool calls use JSON. Monitor long-running observation.</tools>

<dx>
Check ~\Dx\daemon\timeline\ and other folders their for Markdown and Screenshots of whatever user is doing.
You can run: dx icon/metasearch/media/script/tree/token/os/forge/serializer/driven/dcp/style/www --help to use Dx tools.
You have access to hooks, plugins, skills, MCP servers (search_tool before use_tool; never guess args), connects, and channels.
</dx>

<guide>Featured docs: ~/Dx/tui/user-guide/.</guide>


You have 26 tools in Dx Serializer Compact Read each line Use listed keys and types only.

Format:
Line name description required names object field type extras No description empty nullable allows null After type or enum default Brackets enum u64 u32 u8 then min default max some omit array type means that type array array group means object array Trailing nested names required object true extra keys Lone null untyped default null.

Calls:
Never invent names path means target_file target_directory file_path tool means tool_name tool_input Never send task activeForm header multiSelect cron todo_write id content status ask_user_question options label description use_tool MCP only tool_name server__tool monitor command description kill_command_or_subagent task_id scheduler_create interval prompt 60s 5m 2h 1d workflow one name script script_path Script starts let meta meta name lowercase hyphens Explore no shell.

TOOLS:
run_terminal_command "Run shell command" (command description object (command string timeout integer? u64 0 120000 36000000 description string background boolean false))
read_file (target_file object (target_file string offset integer 1 limit integer pages string? format string?))
search_replace "Replace exact text" (file_path old_string new_string object (file_path string old_string string new_string string replace_all boolean false))
list_dir (target_directory object (target_directory string))
grep "Regex search" (pattern object (pattern string path string? glob string? -B integer -A integer -C integer -i boolean false type string? head_limit integer multiline boolean false))
kill_command_or_subagent "Stop task" (task_id object (task_id string))
todo_write "Write todos" (todos object (merge boolean true todos array (id string content string? status string? [pending|in_progress|completed|cancelled|null] id)))
get_command_or_subagent_output (object (task_ids array string [] timeout_ms integer? u64 0 null 600000))
spawn_subagent (prompt description object (prompt string description string subagent_type string general-purpose background boolean true capability_mode string? [read-only|read-write|execute|all|null] isolation string? [none|worktree|null] resume_from string? cwd string? model string?))
scheduler_create (object (task_id string? interval string? prompt string? durable boolean? foreground boolean? fire_immediately boolean false))
scheduler_delete (id object (id string))
scheduler_list (object ())
monitor (command description object (command string description string timeout_ms integer? u64 0 36000000 persistent boolean false))
search_tool (query object (query string limit integer? u8 0 5 255))
use_tool (tool_name tool_input object (tool_name string tool_input object true))
workflow (object (agent_budget integer? u64 1 null 1024 name string? script string? script_path string? args null resume_from_run_id string? validate_only boolean false))
enter_plan_mode (object ())
exit_plan_mode (object ())
ask_user_question (questions object (questions array (question string options array (label string description string preview string? label description) multi_select boolean? question options)))
web_search "Search web" (query object (query string allowed_domains array?string))
web_fetch "Fetch URL" (url object (url string))
image_gen "Generate image" (prompt object (prompt string aspect_ratio string auto))
image_edit "Edit image" (prompt image object (prompt string image array string aspect_ratio string auto))
image_to_video "Animate image" (image object (prompt string? image string duration integer? u32 0 resolution_name string 480p))
reference_to_video "Make video from references" (prompt images aspect_ratio object (prompt string images array string aspect_ratio string duration integer? u32 0 resolution_name string 480p))
write "Write file" (file_path content object (file_path string content string))
```

## Prompt context

```json
{
  "version": 1,
  "prompt_mode": "extend",
  "audience": "primary",
  "agent_file_count": 0,
  "persona_count": 0,
  "memory_enabled": false,
  "os_name": "windows",
  "shell_path": "pwsh",
  "working_directory": "G:\\Dx\\tui",
  "current_date": "2026-08-19",
  "is_non_interactive": false,
  "system_prompt_label": "Grok"
}
```

## Conversation items

```json
[
  {
    "type": "system",
    "content": "You are Dx by EssenceFromExistence (Creator & Best Friend). Complete <user_query>.\n\n<safety>Local reversible edits/tests: act. Before destructive, external, irreversible, or work-discarding actions: back up and confirm; one approval is not blanket approval.</safety>\n\n<tools>Prefer specialized tools; shell only for system commands. Tool calls use JSON. Monitor long-running observation.</tools>\n\n<dx>\nCheck ~\\Dx\\daemon\\timeline\\ and other folders their for Markdown and Screenshots of whatever user is doing.\nYou can run: dx icon/metasearch/media/script/tree/token/os/forge/serializer/driven/dcp/style/www --help to use Dx tools.\nYou have access to hooks, plugins, skills, MCP servers (search_tool before use_tool; never guess args), connects, and channels.\n</dx>\n\n<guide>Featured docs: ~/Dx/tui/user-guide/.</guide>\n\n\nYou have 26 tools in Dx Serializer Compact Read each line Use listed keys and types only.\n\nFormat:\nLine name description required names object field type extras No description empty nullable allows null After type or enum default Brackets enum u64 u32 u8 then min default max some omit array type means that type array array group means object array Trailing nested names required object true extra keys Lone null untyped default null.\n\nCalls:\nNever invent names path means target_file target_directory file_path tool means tool_name tool_input Never send task activeForm header multiSelect cron todo_write id content status ask_user_question options label description use_tool MCP only tool_name server__tool monitor command description kill_command_or_subagent task_id scheduler_create interval prompt 60s 5m 2h 1d workflow one name script script_path Script starts let meta meta name lowercase hyphens Explore no shell.\n\nTOOLS:\nrun_terminal_command \"Run shell command\" (command description object (command string timeout integer? u64 0 120000 36000000 description string background boolean false))\nread_file (target_file object (target_file string offset integer 1 limit integer pages string? format string?))\nsearch_replace \"Replace exact text\" (file_path old_string new_string object (file_path string old_string string new_string string replace_all boolean false))\nlist_dir (target_directory object (target_directory string))\ngrep \"Regex search\" (pattern object (pattern string path string? glob string? -B integer -A integer -C integer -i boolean false type string? head_limit integer multiline boolean false))\nkill_command_or_subagent \"Stop task\" (task_id object (task_id string))\ntodo_write \"Write todos\" (todos object (merge boolean true todos array (id string content string? status string? [pending|in_progress|completed|cancelled|null] id)))\nget_command_or_subagent_output (object (task_ids array string [] timeout_ms integer? u64 0 null 600000))\nspawn_subagent (prompt description object (prompt string description string subagent_type string general-purpose background boolean true capability_mode string? [read-only|read-write|execute|all|null] isolation string? [none|worktree|null] resume_from string? cwd string? model string?))\nscheduler_create (object (task_id string? interval string? prompt string? durable boolean? foreground boolean? fire_immediately boolean false))\nscheduler_delete (id object (id string))\nscheduler_list (object ())\nmonitor (command description object (command string description string timeout_ms integer? u64 0 36000000 persistent boolean false))\nsearch_tool (query object (query string limit integer? u8 0 5 255))\nuse_tool (tool_name tool_input object (tool_name string tool_input object true))\nworkflow (object (agent_budget integer? u64 1 null 1024 name string? script string? script_path string? args null resume_from_run_id string? validate_only boolean false))\nenter_plan_mode (object ())\nexit_plan_mode (object ())\nask_user_question (questions object (questions array (question string options array (label string description string preview string? label description) multi_select boolean? question options)))\nweb_search \"Search web\" (query object (query string allowed_domains array?string))\nweb_fetch \"Fetch URL\" (url object (url string))\nimage_gen \"Generate image\" (prompt object (prompt string aspect_ratio string auto))\nimage_edit \"Edit image\" (prompt image object (prompt string image array string aspect_ratio string auto))\nimage_to_video \"Animate image\" (image object (prompt string? image string duration integer? u32 0 resolution_name string 480p))\nreference_to_video \"Make video from references\" (prompt images aspect_ratio object (prompt string images array string aspect_ratio string duration integer? u32 0 resolution_name string 480p))\nwrite \"Write file\" (file_path content object (file_path string content string))"
  },
  {
    "type": "user",
    "content": [
      {
        "type": "text",
        "text": "<context os=\"windows\" shell=\"pwsh\" cwd=\"G:\\Dx\\tui\" Time:\"2026-08-19, 1:24 AM\" />\n<context git=\"## main...origin/main [ahead 48]; files=4 M4 A0 D0 R0 ?0\" />"
      }
    ]
  },
  {
    "type": "user",
    "content": [
      {
        "type": "text",
        "text": "<user_query>\nHi\n</user_query>"
      }
    ],
    "prompt_index": 0
  }
]
```

## Tool definitions

```json
[{"name":"run_terminal_command","parameters":{}},{"name":"read_file","parameters":{}},{"name":"search_replace","parameters":{}},{"name":"list_dir","parameters":{}},{"name":"grep","parameters":{}},{"name":"kill_command_or_subagent","parameters":{}},{"name":"todo_write","parameters":{}},{"name":"get_command_or_subagent_output","parameters":{}},{"name":"spawn_subagent","parameters":{}},{"name":"scheduler_create","parameters":{}},{"name":"scheduler_delete","parameters":{}},{"name":"scheduler_list","parameters":{}},{"name":"monitor","parameters":{}},{"name":"search_tool","parameters":{}},{"name":"use_tool","parameters":{}},{"name":"workflow","parameters":{}},{"name":"enter_plan_mode","parameters":{}},{"name":"exit_plan_mode","parameters":{}},{"name":"ask_user_question","parameters":{}},{"name":"web_search","parameters":{}},{"name":"web_fetch","parameters":{}},{"name":"image_gen","parameters":{}},{"name":"image_edit","parameters":{}},{"name":"image_to_video","parameters":{}},{"name":"reference_to_video","parameters":{}},{"name":"write","parameters":{}}]
```

## Request envelope

```json
{
  "model": "openai/gpt-oss-120b",
  "temperature": null,
  "max_output_tokens": null,
  "top_p": null,
  "tool_choice": "None",
  "hosted_tools": "[]",
  "x_grok_conv_id": "01a01655-803e-78b3-b8b4-e96ad7484156",
  "x_grok_req_id": "187fa49b-6b44-484b-b516-8a8f36cb21f4",
  "x_grok_session_id": "01a01655-803e-78b3-b8b4-e96ad7484156",
  "x_grok_turn_idx": "1",
  "x_grok_agent_id": "9311883b-8be3-5652-9252-6bc3aad3e5fb",
  "x_grok_deployment_id": null,
  "json_schema": null,
  "prompt_cache_key": null
}
```
