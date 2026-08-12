<tool_schemas format="Dx Serializer Compact" call_arguments="JSON">
Dx Serializer Compact is an ordered, key-deduplicated JSON Schema encoding.
Header `N[name description parameters(required type properties)]` declares the field order and tool count.
Each tool is `name "description" (required_names object (property("description" type extras)...))`.
`[a|b]` is a JSON Schema type union; `[a b ...]` is an enum. A scalar after a type is its default. `object true` means `additionalProperties:true`. `array T` gives the item type; nested parentheses give object items, with trailing names marking required item properties. Numeric extras encode format/range/default as demonstrated by: `uint64 0 120000 36000000` = format/min/default/max; `uint8 0 255 5` = format/min/max/default.
Call tools by their native names with standard JSON object arguments reconstructed from these schemas. Never send compact syntax as tool arguments. Dx validates every JSON call against its canonical schema before execution. Tool results retain their normal JSON/text result contract; this compact format encodes schemas, not calls or results.
Example: `read_file "" (target_file object (...))` means call `read_file` with JSON such as `{"target_file":"G:\\Dx\\tui\\README.md"}`.

26[name description parameters(required type properties)]
run_terminal_command "Shell command; long runs auto-background" (command description object (command("" string) timeout("Max ms; auto-backgrounds" [integer|null] uint64 0 120000 36000000) description("" string) background("" boolean false)))
read_file "" (target_file object (target_file("" string) offset("" integer 1) limit("" integer) pages("PDF pages, max 20" [string|null]) format("" [string|null])))
search_replace "" (file_path old_string new_string object (file_path("" string) old_string("Exact text to replace" string) new_string("Replacement; must differ" string) replace_all("Replace all occurrences" boolean false)))
list_dir "" (target_directory object (target_directory("" string)))
grep "Regex search, respects gitignore" (pattern object (pattern("" string) path("" [string|null]) glob("" [string|null]) -B("" integer) -A("" integer) -C("" integer) -i("" boolean false) type("" [string|null]) head_limit("" integer) multiline("" boolean false)))
kill_command_or_subagent "" (task_id object (task_id("" string)))
todo_write "" (todos object (merge("Merge by id; false replaces" boolean true) todos("" array (id("" string) content("" [string|null]) status("" [string|null] [pending in_progress completed cancelled null]) id))))
get_command_or_subagent_output "Output of background task" ( object (task_ids("Background task ids" array string []) timeout_ms("Max wait ms; 0=non-blocking" [integer|null] uint64 0 null 600000)))
spawn_subagent "" (prompt description object (prompt("" string) description("" string) subagent_type("general-purpose, explore, plan" string general-purpose) background("" boolean true) capability_mode("read-only to all" [string|null] [read-only read-write execute all null] null) isolation("" [string|null] [none worktree null]) resume_from("Continue prior subagent" [string|null]) cwd("" [string|null]) model("" [string|null])))
scheduler_create "Create/update scheduled task" ( object (task_id("" [string|null] null) interval("Min 60s, required" [string|null] null) prompt("" [string|null] null) durable("" [boolean|null] null) foreground("" [boolean|null] null) fire_immediately("" boolean false)))
scheduler_delete "" (id object (id("" string)))
scheduler_list "" ( object ())
monitor "Stream until done" (command description object (command("" string) description("" string) timeout_ms("" [integer|null] uint64 0 36000000) persistent("" boolean false)))
search_tool "Find MCP tools by keyword" (query object (query("Keywords+server" string) limit("" [integer|null] uint8 0 255 5)))
use_tool "Call MCP tool by name" (tool_name tool_input object (tool_name("Qualified server.tool" string) tool_input("JSON per tool schema" object true)))
workflow "Run Rhai workflow" ( object (agent_budget("Cap child calls, default 128" [integer|null] uint64 1 1024 null) name("" [string|null] null) script("Inline Rhai, meta map" [string|null] null) script_path("" [string|null] null) args("" null) resume_from_run_id("" [string|null] null) validate_only("" boolean false)))
enter_plan_mode "" ( object ())
exit_plan_mode "" ( object ())
ask_user_question "" (questions object (questions("" array (question("" string) options("" array (label("" string) description("" string) preview("" [string|null]) label description)) multi_select("Allow multiple answers" [boolean|null] null) question options))))
web_search "" (query object (query("" string) allowed_domains("" [array|null] string)))
web_fetch "" (url object (url("" string)))
image_gen "" (prompt object (prompt("" string) aspect_ratio("auto, 1:1, 16:9, 9:16" string auto)))
image_edit "" (prompt image object (prompt("" string) image("" array string) aspect_ratio("" string auto)))
image_to_video "Animate image to video" (image object (prompt("" [string|null] null) image("" string) duration("6/10s" [integer|null] uint32 0) resolution_name("480p/720p" string 480p)))
reference_to_video "" (prompt images aspect_ratio object (prompt("" string) images("2-7 reference images" array string) aspect_ratio("" string) duration("" [integer|null] uint32 0) resolution_name("" string 480p)))
write "" (file_path content object (file_path("" string) content("" string)))
</tool_schemas>
