//! Launch videos in the separate native Video Player window.

use crate::app::actions::Action;
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};

pub const SHOWCASE_VIDEO_ARGS: &[(&str, &str, &str)] = &[
    (
        "spiderman",
        "Spiderman Into The SpiderVerse",
        "Play this DX showcase video",
    ),
    ("one-piece", "One Piece", "Play this DX showcase video"),
    (
        "frieren",
        "Frieren Beyond Journey's End",
        "Play this DX showcase video",
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
        "/video <path|showcase|download>"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        true
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("path|showcase|download")
    }

    fn suggest_args(&self, _ctx: &AppCtx, args_query: &str) -> Option<Vec<ArgItem>> {
        let query = args_query.trim().to_ascii_lowercase();
        Some(
            SHOWCASE_VIDEO_ARGS
                .iter()
                .filter(|(arg, title, _description)| {
                    query.is_empty()
                        || arg.contains(&query)
                        || title.to_ascii_lowercase().contains(&query)
                })
                .map(|(arg, title, _description)| ArgItem {
                    // The dropdown renderer places this suffix in a right-
                    // aligned tag column. Selecting the row performs the
                    // download; playback remains available with `/video <name>`
                    // and automatically prefers the downloaded file.
                    display: format!("{title} [Download]"),
                    match_text: format!("download {arg} {title} EssenceFromExistence Showcase"),
                    insert_text: format!("download {arg}"),
                    description: format!(
                        "Download to the OS Downloads folder · {}",
                        crate::video_player::showcase_video_status(arg)
                    ),
                })
                .collect(),
        )
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        if args.trim().is_empty() {
            return CommandResult::Message("Usage: /video <path|showcase|download>".to_string());
        }
        let raw = args.trim();
        let arg = raw.to_ascii_lowercase();
        if let Some(selector) = arg.strip_prefix("download ").map(str::trim)
            && SHOWCASE_VIDEO_ARGS
                .iter()
                .any(|(name, _, _)| *name == selector)
        {
            CommandResult::Action(Action::DownloadVideo {
                selector: selector.to_owned(),
            })
        } else if arg == "showcase" {
            CommandResult::Action(Action::PlayVideo("dx-showcase".into()))
        } else if SHOWCASE_VIDEO_ARGS.iter().any(|(name, _, _)| *name == arg) {
            CommandResult::Action(Action::PlayVideo(format!("dx-showcase:{arg}")))
        } else {
            CommandResult::Action(Action::PlayVideo(raw.to_string()))
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
                CommandResult::Message(message)
                    if message == "Usage: /video <path|showcase|download>"
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
                && trigger.usage == "/video <path|showcase|download>"
        }));
    }

    #[test]
    fn video_argument_suggestions_include_operational_tooltips() {
        let models = crate::acp::model_state::ModelState::default();
        let cwd = std::path::Path::new(".");
        let ctx = crate::slash::command::AppCtx {
            models: &models,
            cwd,
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: false,
            screen_mode: crate::app::ScreenMode::Inline,
            current_title: None,
        };
        let items = VideoCommand
            .suggest_args(&ctx, "")
            .expect("video suggestions");

        assert_eq!(items.len(), 3, "one download row per showcase video");
        assert!(items.iter().all(|item| !item.description.trim().is_empty()));
        assert!(items.iter().all(|item| item.display.ends_with(" [Download]")));
        assert!(items.iter().any(|item| item.insert_text == "download spiderman"));
    }
}
