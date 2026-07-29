//! Slash commands for the directly embedded DX surfaces.

use crate::app::actions::Action;
use crate::dx::DxView;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

pub struct DxViewCommand {
    name: &'static str,
    description: &'static str,
    view: DxView,
}

impl DxViewCommand {
    pub const fn editor() -> Self {
        Self {
            name: "editor",
            description: "Open the embedded DX code editor",
            view: DxView::Editor,
        }
    }

    pub const fn browser() -> Self {
        Self {
            name: "browser",
            description: "Open the embedded DX file browser",
            view: DxView::FileBrowser,
        }
    }

    pub const fn diff() -> Self {
        Self {
            name: "diff",
            description: "Open the embedded DX diff viewer",
            view: DxView::Diff,
        }
    }

    pub const fn animation() -> Self {
        Self {
            name: "anim",
            description: "Open the embedded DX train animation",
            view: DxView::Animation,
        }
    }
}

impl SlashCommand for DxViewCommand {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn aliases(&self) -> &[&str] {
        match self.view {
            DxView::Editor => &["code"],
            DxView::FileBrowser => &["files"],
            _ => &[],
        }
    }

    fn usage(&self) -> &str {
        match self.view {
            DxView::Editor => "/editor",
            DxView::FileBrowser => "/browser",
            DxView::Diff => "/diff",
            DxView::Animation => "/anim",
            DxView::Chat => unreachable!("not exposed by DxViewCommand"),
        }
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn run(&self, _ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        CommandResult::Action(Action::SwitchDxView(self.view))
    }
}
