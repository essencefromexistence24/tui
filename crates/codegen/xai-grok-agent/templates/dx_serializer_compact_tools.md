You have 27 toolsead each line - Use listed keys and types only.

Calls:
Never invent names path means target_file target_directory file_path tool means tool_name tool_input Never send task activeForm header multiSelect cron todo_write id content status ask_user_question options label description use_tool MCP only tool_name server__tool monitor command description kill_command_or_subagent task_id scheduler_create interval prompt 60s 5m 2h 1d workflow one name script script_path Script starts let meta meta name lowercase hyphens Explore no shell get_tool_details only after 3 failed attempts on the same tool it returns that one tool full schema and records it in workspace AGENTS.md.

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
get_tool_details "Full schema of one tool - use ONLY after 3 failed calls of the same tool, then record to AGENTS.md" (tool_name object (tool_name string))
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