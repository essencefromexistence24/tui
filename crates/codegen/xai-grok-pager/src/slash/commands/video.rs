//! Launch videos in the separate native Video Player window.

use crate::app::actions::Action;
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};

pub const SHOWCASE_VIDEO_ARGS: &[(&str, &str, &str)] = &[
    (
        "spiderman",
        "Spiderman Into The SpiderVerse",
        "Play the DX showcase playlist",
    ),
    ("one-piece", "One Piece", "Play the DX showcase playlist"),
    (
        "frieren",
        "Frieren Beyond Journey's End",
        "Play the DX showcase playlist",
    ),
];

pub struct VideoCommand;

impl SlashCommand for VideoCommand {
    fn name(&self) -> &str {
        "video"
    }

    fn description(&self) -> &str {
        "Play a video in the native Video Player"
    }

    fn usage(&self) -> &str {
        "/video <path|showcase>"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        true
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("path|showcase")
    }

    fn suggest_args(&self, _ctx: &AppCtx, args_query: &str) -> Option<Vec<ArgItem>> {
        let query = args_query.trim().to_ascii_lowercase();
        let mut items = vec![ArgItem {
            display: "Dx Showcase Playlist — made by EssenceFromExistence".into(),
            match_text: "showcase playlist videos".into(),
            insert_text: "showcase".into(),
            description: crate::video_player::showcase_playlist_status(),
        }];
        items.extend(
            SHOWCASE_VIDEO_ARGS
                .iter()
                .filter(|(arg, title, _description)| {
                    query.is_empty()
                        || arg.contains(&query)
                        || title.to_ascii_lowercase().contains(&query)
                })
                .map(|(arg, title, _description)| ArgItem {
                    display: format!("{title} (Showcase)"),
                    match_text: format!("{arg} {title} EssenceFromExistence Showcase"),
                    insert_text: (*arg).into(),
                    description: crate::video_player::showcase_video_status(arg),
                }),
        );
        Some(items)
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        if args.trim().is_empty() {
            return CommandResult::Message("Usage: /video <path|showcase>".to_string());
        }
        let arg = args.trim().to_ascii_lowercase();
        if arg == "showcase" || SHOWCASE_VIDEO_ARGS.iter().any(|(name, _, _)| *name == arg) {
            CommandResult::Action(Action::PlayVideo("dx-showcase".into()))
        } else {
            CommandResult::Action(Action::PlayVideo(args.to_string()))
        }
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
            CommandResult::Message(message) if message == "Usage: /video <path|showcase>"
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
                && trigger.usage == "/video <path|showcase>"
        }));
    }

    #[test]
    fn video_argument_suggestions_include_operational_tooltips() {
        let models = crate::acp::model_state::ModelState::default();
        let ctx = super::super::tests::make_ctx(&models);
        let items = VideoCommand
            .suggest_args(&ctx, "")
            .expect("video suggestions");

        assert_eq!(items.len(), 4, "playlist plus three showcase videos");
        assert!(items.iter().all(|item| !item.description.trim().is_empty()));
        assert!(items.iter().any(|item| item.insert_text == "showcase"));
    }
}
