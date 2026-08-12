//! Token-bounded native tool definitions for the sampling API.
//!
//! The model receives one native tool list. Compact signatures live in each
//! tool's description; calls still use ordinary JSON, while the registry's
//! canonical schemas remain authoritative for validation and dispatch.

use serde_json::json;
use xai_grok_tools::types::definition::ToolDefinition;

/// Native tools and their compact argument signatures.
///
/// Legend (carried once by the first definition): `!` required, `?` nullable,
/// `s/i/b/o/a` string/integer/boolean/object/array, `=` default, `|` enum.
const SIGNATURES: &[(&str, &str)] = &[
    (
        "run_terminal_command",
        "J=JSON; !req ?null s/i/b/o/a types =default |enum; command!s,description!s,timeout?i=120000,backgroundb=false",
    ),
    (
        "read_file",
        "target_file!s,offseti=1,limiti,pages?s,format?s",
    ),
    (
        "search_replace",
        "file_path!s,old_string!s,new_string!s,replace_allb=false",
    ),
    ("list_dir", "target_directory!s"),
    (
        "grep",
        "pattern!s,path?s,glob?s,-B:i,-A:i,-C:i,-i:b=false,type?s,head_limit:i,multiline:b=false",
    ),
    ("kill_command_or_subagent", "task_id!s"),
    (
        "todo_write",
        "todos!a<{id!s,content?s,status?s=pending|in_progress|completed|cancelled|null}>,mergeb=true",
    ),
    (
        "get_command_or_subagent_output",
        "task_ids:a<s>=[],timeout_ms?i",
    ),
    (
        "spawn_subagent",
        "prompt!s,description!s,subagent_types=general-purpose,backgroundb=true,capability_mode?s,isolation?s,resume_from?s,cwd?s,model?s",
    ),
    (
        "scheduler_create",
        "task_id?s,interval?s,prompt?s,durable?b,foreground?b,fire_immediatelyb=false",
    ),
    ("scheduler_delete", "id!s"),
    ("scheduler_list", "{}"),
    (
        "monitor",
        "command!s,description!s,timeout_ms?i,persistentb=false",
    ),
    ("search_tool", "query!s,limit?i=5"),
    ("use_tool", "tool_name!s,tool_input!o"),
    (
        "workflow",
        "agent_budget?i,name?s,script?s,script_path?s,args:null,resume_from_run_id?s,validate_onlyb=false",
    ),
    ("enter_plan_mode", "{}"),
    ("exit_plan_mode", "{}"),
    (
        "ask_user_question",
        "questions!a<{question!s,options!a<{label!s,description!s,preview?s}>,multi_select?b}>",
    ),
    ("web_search", "query!s,allowed_domains?a<s>"),
    ("web_fetch", "url!s"),
    ("image_gen", "prompt!s,aspect_ratios=auto"),
    ("image_edit", "prompt!s,image!a<s>,aspect_ratios=auto"),
    (
        "image_to_video",
        "image!s,prompt?s,duration?i,resolution_names=480p",
    ),
    (
        "reference_to_video",
        "prompt!s,images!a<s>,aspect_ratio!s,duration?i,resolution_names=480p",
    ),
    ("write", "file_path!s,content!s"),
];

/// Convert canonical definitions into the single compact native presentation.
pub fn compact_native_definitions(mut definitions: Vec<ToolDefinition>) -> Vec<ToolDefinition> {
    definitions.retain_mut(|definition| {
        let Some((_, signature)) = SIGNATURES
            .iter()
            .find(|(name, _)| *name == definition.function.name)
        else {
            // Preserve dynamically registered/MCP definitions. They are not
            // part of the fixed built-in budget and still need native API
            // registration when a session exposes them.
            return true;
        };
        definition.function.description = Some((*signature).to_owned());
        // The provider requires an explicit object type. The typed registry
        // validates the actual JSON arguments against the canonical schema.
        definition.function.parameters = json!({"type": "object"});
        true
    });
    definitions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_all_26_unique_native_tools() {
        assert_eq!(SIGNATURES.len(), 26);
        let names = SIGNATURES
            .iter()
            .map(|(name, _)| *name)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), SIGNATURES.len());
    }
}
