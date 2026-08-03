use std::path::PathBuf;

/// Full-screen DX surfaces hosted by Grok's existing terminal and event loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DxView {
    Chat,
    Editor,
    FileBrowser,
    Diff,
    #[default]
    Animation,
}

/// Presentation-only state for the directly integrated DX components.
///
/// Agent messages, tasks, subagents, tools, models, and persistence remain in
/// Grok's existing state. This type must never grow a second chat/backend state.
pub struct DxUiState {
    pub view: DxView,
    pub minimap_visible: bool,
    pub sidebar_visible: bool,
    pub palette_visible: bool,
    pub selected_sidebar_section: usize,
    pub sidebar: super::sidebar::SidebarUiState,
    pub minimap: super::minimap::MinimapUiState,
    pub diff: super::diff_view::DiffState,
    pub animation: super::animation::AnimationSurface,
    pub cursor_rainbow: super::effects::RainbowEffect,
    pub sound: super::sound::SoundPlayer,
    pub intro_enabled: bool,
    pub outro_enabled: bool,
    pub intro_seen: bool,
    /// When `Some`, the Animation view is showing a timed message intro;
    /// when this instant passes the view snaps back to the Workspace/Chat
    /// screen automatically. `None` means the carousel was opened manually.
    pub intro_deadline: Option<std::time::Instant>,
    pub file_browser: super::file_browser::FileBrowserSurface,
    pub menu: Option<super::menu::Menu>,
    pub editor: super::editor::EditorAdapter,
}

impl Default for DxUiState {
    fn default() -> Self {
        let mut sound = super::sound::SoundPlayer::new();
        sound.play(super::sound::SoundCue::Startup);
        Self {
            // Start in the splash screen; the live chat workspace is the
            // Workspace item in the same carousel.
            view: DxView::Animation,
            minimap_visible: true,
            sidebar_visible: true,
            palette_visible: false,
            selected_sidebar_section: 0,
            sidebar: super::sidebar::SidebarUiState::default(),
            minimap: super::minimap::MinimapUiState::default(),
            diff: super::diff_view::DiffState::empty(),
            animation: super::animation::AnimationSurface::default(),
            cursor_rainbow: super::effects::RainbowEffect::new(),
            sound,
            intro_enabled: true,
            outro_enabled: true,
            intro_seen: false,
            // The Splash carousel is the persistent home screen. A deadline
            // is armed only after a message has actually been submitted.
            intro_deadline: None,
            file_browser: super::file_browser::FileBrowserSurface::default(),
            menu: None,
            editor: super::editor::EditorAdapter::new(),
        }
    }
}

impl DxUiState {
    /// Play the selected intro while a message submitted from the carousel is
    /// being accepted, then reveal Chat after two seconds.
    pub fn begin_message_intro(&mut self) -> bool {
        if self.view != DxView::Animation || self.intro_deadline.is_some() {
            return false;
        }
        self.intro_seen = true;
        self.animation.begin_intro();
        self.intro_deadline = Some(
            std::time::Instant::now() + std::time::Duration::from_secs(2),
        );
        self.sound.stop_animation_loop();
        self.sound
            .start_animation_loop(self.animation.current().sound());
        true
    }

    pub fn close_overlay_or_return_to_chat(&mut self) {
        if self.palette_visible {
            self.palette_visible = false;
        } else {
            self.view = DxView::Chat;
        }
    }
}

/// User intent emitted by DX controls and consumed by Grok's action layer.
///
/// DX components do not perform terminal, filesystem, or agent side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DxAction {
    SwitchView(DxView),
    ToggleMinimap,
    ToggleSidebar,
    TogglePalette,
    OpenFile(PathBuf),
    OpenDiff(PathBuf),
    InsertPrompt(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_surface_is_the_persistent_splash_carousel() {
        let mut state = DxUiState::default();

        assert_eq!(DxView::default(), DxView::Animation);
        assert_eq!(state.view, DxView::Animation);
        assert_eq!(
            state.animation.current(),
            crate::dx::animation::AnimationKind::Splash
        );
        assert!(state.intro_deadline.is_none());

        assert!(state.begin_message_intro());
        assert!(state.intro_deadline.is_some());
        assert!(state.intro_seen);
    }

    #[test]
    fn escape_closes_palette_before_leaving_fullscreen_view() {
        let mut state = DxUiState {
            view: DxView::Editor,
            palette_visible: true,
            ..DxUiState::default()
        };

        state.close_overlay_or_return_to_chat();
        assert!(!state.palette_visible);
        assert_eq!(state.view, DxView::Editor);

        state.close_overlay_or_return_to_chat();
        assert_eq!(state.view, DxView::Chat);
    }
}
