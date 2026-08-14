use crate::slash::command::{ArgItem, CommandExecCtx, CommandResult, SlashCommand};

/// DX Connect node surface. Provider and messaging-channel configuration have
/// dedicated plural commands and must not be conflated with `/connects`.
pub struct ConnectsCommand;

impl SlashCommand for ConnectsCommand {
    fn name(&self) -> &str {
        "connects"
    }

    fn description(&self) -> &str {
        "Browse and run DX Connect nodes"
    }

    fn usage(&self) -> &str {
        "/connects [nodes|run <node> <json>]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn suggest_args(
        &self,
        _ctx: &crate::slash::command::AppCtx,
        args_query: &str,
    ) -> Option<Vec<ArgItem>> {
        let query = args_query.trim().to_ascii_lowercase();
        let mut items = vec![
            ArgItem {
                display: "nodes".into(),
                match_text: "nodes".into(),
                insert_text: "nodes".into(),
                description: "List native, Flow-Like, and n8n Connect nodes".into(),
            },
            ArgItem {
                display: "run".into(),
                match_text: "run".into(),
                insert_text: "run ".into(),
                description: "Run a native or configured isolated adapter node".into(),
            },
        ];
        if !query.is_empty() {
            items.retain(|item| item.match_text.contains(&query));
        }
        Some(items)
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let args = args.trim();
        if args.is_empty() || args == "nodes" {
            let mut message = String::from("DX Connect nodes:\n");
            for node in dx_connect::catalog() {
                message.push_str(&format!(
                    "- {} [{} / {:?}] — {}\n",
                    node.id,
                    match node.source {
                        dx_connect::NodeSource::DxNative => "dx",
                        dx_connect::NodeSource::FlowLike => "flow-like",
                        dx_connect::NodeSource::N8n => "n8n",
                    },
                    node.backend,
                    node.description
                ));
            }
            message.push_str(
                "\nNative nodes run in-process. Flow-Like and n8n nodes use their configured isolated JSONL adapters.",
            );
            return CommandResult::Message(message);
        }

        let Some(rest) = args.strip_prefix("run ") else {
            return CommandResult::Error(
                "Usage: /connects nodes | /connects run <node-id> <json-context>".into(),
            );
        };
        let Some((node_id, context_json)) = rest.split_once(' ') else {
            return CommandResult::Error("Usage: /connects run <node-id> <json-context>".into());
        };
        let context: dx_connect::NodeContext = match serde_json::from_str(context_json) {
            Ok(context) => context,
            Err(error) => return CommandResult::Error(format!("Invalid Connect JSON: {error}")),
        };
        match dx_connect::execute(node_id, context) {
            Ok(outputs) => CommandResult::Message(
                serde_json::to_string_pretty(&outputs).unwrap_or_else(|_| "[]".into()),
            ),
            Err(error) => CommandResult::Error(error.to_string()),
        }
    }
}
