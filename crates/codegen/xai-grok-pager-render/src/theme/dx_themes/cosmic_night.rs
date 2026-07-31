//! DX theme: "Cosmic Night" — auto-generated from dx-tui themes.json.
use ratatui::style::{Color, Modifier};
use super::tokyonight::Theme;
const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }
#[allow(dead_code)]
mod palette {
    use super::*;
    pub const BG: Color = rgb(41, 42, 0);
    pub const BG_DARK: Color = rgb(37, 38, 0);
    pub const BG_STORM: Color = rgb(41, 42, 0);
    pub const BG_HIGHLIGHT: Color = rgb(57, 58, 16);
    pub const FG: Color = rgb(231, 232, 0);
    pub const FG_DARK: Color = rgb(185, 186, 0);
    pub const FG_GUTTER: Color = rgb(77, 77, 0);
    pub const COMMENT: Color = rgb(116, 116, 0);
    pub const DARK3: Color = rgb(92, 93, 0);
    pub const DARK5: Color = rgb(139, 139, 0);
    pub const ACCENT: Color = rgb(76, 80, 92);
    pub const ACCENT_DIM: Color = rgb(46, 48, 55);
    pub const GREEN: Color = rgb(114, 130, 70);
    pub const RED: Color = rgb(136, 27, 31);
    pub const RED1: Color = rgb($[Math]::Min(255, 208), $[Math]::Min(255, 155), $[Math]::Min(255, 159));
    pub const YELLOW: Color = rgb(106, 80, 46);
    pub const CYAN: Color = rgb(132, 120, 138);
    pub const BLUE: Color = rgb(38, 40, 122);
    pub const MAGENTA: Color = rgb(116, 40, 132);
    pub const PURPLE: Color = rgb(96, 27, 112);
    pub const ORANGE: Color = rgb(120, 50, 31);
    pub const RED_DARK: Color = rgb(123, 14, 20);
    pub const GREEN_DARK: Color = rgb(6, 136, 6);
}
use palette::*;

impl Theme {
    pub const fn cosmic_night() -> Self {
        Self {
            bg_base: BG_STORM,
            bg_light: BG_HIGHLIGHT,
            bg_dark: BG_DARK,
            bg_highlight: BG_HIGHLIGHT,
            bg_hover: rgb(65, 66, 24),
            bg_terminal: BG,
            accent_user: ACCENT,
            accent_assistant: MAGENTA,
            accent_thinking: MAGENTA,
            accent_tool: DARK5,
            accent_system: BLUE,
            accent_running: GREEN,
            accent_plan: ORANGE,
            accent_success: GREEN,
            accent_error: RED,
            accent_composer: PURPLE,
            accent_chat: CYAN,
            text_primary: FG,
            text_secondary: FG_DARK,
            gray_dim: FG_GUTTER,
            gray_bright: DARK5,
            gray: COMMENT,
            gray_accent: DARK3,
            diff_insert_fg: GREEN,
            diff_insert_bg: GREEN_DARK,
            diff_delete_fg: RED1,
            diff_delete_bg: RED_DARK,
            warning: YELLOW,
            error: RED,
            selection_bg: rgb(88, 90, 96),
            selection_fg: FG,
            cursor_bg: ACCENT,
            cursor_fg: BG_STORM,
            link_fg: CYAN,
            link_hover_fg: ACCENT,
            prompt_border: COMMENT,
            prompt_border_active: DARK5,
            prompt_prefix_focused: ACCENT,
            prompt_prefix_unfocused: FG_GUTTER,
            prompt_text: FG,
            prompt_text_dim: FG_DARK,
            prompt_bg: BG_STORM,
            prompt_accent: ACCENT,
            prompt_placeholder: FG_GUTTER,
            prompt_chip_bg: BG_HIGHLIGHT,
            prompt_chip_fg: FG_DARK,
            prompt_chip_border: COMMENT,
            scrollbar_bg: FG_GUTTER,
            scrollbar_fg: COMMENT,
            sash_border: COMMENT,
            sash_label: FG_GUTTER,
            button_bg: BG_HIGHLIGHT,
            button_fg: FG,
            button_hover_bg: ACCENT_DIM,
            button_hover_fg: FG,
            button_active_bg: ACCENT,
            button_active_fg: BG_STORM,
            input_bg: BG_DARK,
            input_fg: FG,
            toast_fg: FG_DARK,
            code_bg: BG_DARK,
            code_fg: FG,
            code_border: COMMENT,
            code_highlight_bg: rgb(73, 74, 32),
            code_highlight_fg: ACCENT,
            markdown_header_fg: ACCENT,
            markdown_link_fg: CYAN,
            markdown_bold_fg: FG,
            markdown_code_bg: BG_DARK,
            markdown_code_fg: FG_DARK,
            markdown_blockquote_border: ACCENT_DIM,
            markdown_table_header_bg: BG_HIGHLIGHT,
            bg_visual: BG_DARK,
            bg_del: RED_DARK,
            bg_ins: GREEN_DARK,
            modal_overlay: rgb(0, 0, 0),
            modal_window_bg: BG_STORM,
            modal_window_fg: FG,
            modal_window_border: COMMENT,
            modal_input_bg: BG_DARK,
            paste_bg: rgb(57, 58, 16),
            paste_fg: FG_DARK,
            paste_dim: COMMENT,
            splash_hero_fg: FG_DARK,
            inline_edit_bg: BG_DARK,
        }
    }
}

