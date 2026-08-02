//! Launch videos in the separate native DX video-player window.

use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

pub struct VideoCommand;

impl SlashCommand for VideoCommand {
    fn name(&self) -> &str {
        "video"
    }

    fn description(&self) -> &str {
        "Play a video in the native DX video player"
    }

    fn usage(&self) -> &str {
        "/video <path>"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        true
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("path to a video file")
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        if args.trim().is_empty() {
            return CommandResult::Message("Usage: /video <path>".to_string());
        }
        CommandResult::Action(Action::PlayVideo(args.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_requires_a_path() {
        let models = crate::acp::model_state::ModelState::default();
        let mut ctx = super::super::tests::make_ctx(&models);
        assert!(matches!(
            VideoCommand.run(&mut ctx, "  "),
            CommandResult::Message(message) if message == "Usage: /video <path>"
        ));
    }

    #[test]
    fn command_preserves_quoted_path_for_dispatch_validation() {
        let models = crate::acp::model_state::ModelState::default();
        let mut ctx = super::super::tests::make_ctx(&models);
        match VideoCommand.run(&mut ctx, r#""C:\Video Files\demo.mp4""#) {
            CommandResult::Action(Action::PlayVideo(path)) => {
                assert_eq!(path, r#""C:\Video Files\demo.mp4""#);
            }
            other => panic!("expected PlayVideo action, got {other:?}"),
        }
    }

    #[test]
    fn video_text_in_the_middle_of_chat_is_not_an_invocation() {
        assert!(crate::slash::parse_invocation("please play /video later").is_none());
    }

    #[test]
    fn video_is_registered_for_help_autocomplete_and_case_insensitive_dispatch() {
        let registry =
            crate::slash::registry::CommandRegistry::new(super::super::builtin_commands());
        assert!(registry.get("video").is_some());
        assert!(registry.get_for_dispatch("VIDEO").is_some());
        assert!(registry.triggers().iter().any(|trigger| {
            trigger.canonical == "video"
                && trigger.display == "/video"
                && trigger.usage == "/video <path>"
        }));
    }
}
