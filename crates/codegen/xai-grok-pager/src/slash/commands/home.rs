//! `/home` -- return to the splash home screen.

use crate::app::actions::Action;
use crate::dx::DxView;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

/// Return to the splash home screen (Animation view).
pub struct HomeCommand;

impl SlashCommand for HomeCommand {
    fn name(&self) -> &str {
        "home"
    }

    fn aliases(&self) -> &[&str] {
        &["welcome"]
    }

    fn description(&self) -> &str {
        "Return to the home screen"
    }

    fn usage(&self) -> &str {
        "/home"
    }

    fn run(&self, _ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        CommandResult::Action(Action::SwitchDxView(DxView::Animation))
    }
}
