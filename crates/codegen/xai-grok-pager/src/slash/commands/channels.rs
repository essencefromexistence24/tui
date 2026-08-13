use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

/// ZeroClaw messaging-channel configuration and lifecycle surface.
pub struct ChannelsCommand;

impl SlashCommand for ChannelsCommand {
    fn name(&self) -> &str {
        "channels"
    }

    fn description(&self) -> &str {
        "Configure and manage messaging channels"
    }

    fn usage(&self) -> &str {
        "/channels"
    }

    fn run(&self, _ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        CommandResult::Action(Action::OpenExtensionsModal {
            tab: crate::views::extensions_modal::ExtensionsTab::Connect,
            trigger: xai_grok_telemetry::events::ExtensionsModalTrigger::SlashCommand,
        })
    }
}
