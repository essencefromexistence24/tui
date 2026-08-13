use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

/// Reserved top-level Connect surface. Provider and messaging-channel
/// configuration have dedicated commands and must not be conflated with it.
pub struct ConnectCommand;

impl SlashCommand for ConnectCommand {
    fn name(&self) -> &str {
        "connect"
    }

    fn description(&self) -> &str {
        "Open the reserved DX Connect surface"
    }

    fn usage(&self) -> &str {
        "/connect"
    }

    fn run(&self, _ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        CommandResult::Message(
            "DX Connect is reserved for future integrations. Use /providers for AI providers or /channels for messaging channels.".into(),
        )
    }
}
