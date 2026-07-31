//! DX theme: "Claude" — auto-generated from dx-tui themes.json.
use ratatui::style::{Color, Modifier};
use super::tokyonight::Theme;
const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }
#[allow(dead_code)]
mod palette {
    use super::*;
    pub const BG: Color = rgb(64, 64, 0);
    pub const BG_DARK: Color = rgb(60, 60, 0);
    pub const BG_STORM: Color = rgb(64, 64, 0);
    pub const BG_HIGHLIGHT: Color = rgb(80, 80, 16);
    pub const FG: Color = rgb(202, 201, 0);
    pub const FG_DARK: Color = rgb(162, 161, 0);
    pub const FG_GUTTER: Color = rgb(67, 67, 0);
    pub const COMMENT: Color = rgb(101, 100, 0);
    pub const DARK3: Color = rgb(81, 80, 0);
    pub const DARK5: Color = rgb(121, 121, 0);
    pub const ACCENT: Color = rgb(51, 50, 49);
    pub const ACCENT_DIM: Color = rgb(31, 30, 29);
    pub const GREEN: Color = rgb(90, 100, 55);
    pub const RED: Color = rgb(111, 17, 16);
    pub const RED1: Color = rgb($[Math]::Min(255, 194), $[Math]::Min(255, 143), $[Math]::Min(255, 140));
    pub const YELLOW: Color = rgb(81, 50, 24);
    pub const CYAN: Color = rgb(89, 90, 74);
    pub const BLUE: Color = rgb(26, 25, 79);
    pub const MAGENTA: Color = rgb(91, 25, 89);
    pub const PURPLE: Color = rgb(71, 17, 69);
    pub const ORANGE: Color = rgb(90, 35, 16);
    pub const RED_DARK: Color = rgb(192, 14, 20);
    pub const GREEN_DARK: Color = rgb(6, 202, 6);
}
use palette::*;

impl Theme {
    pub const fn claude() -> Self {
        Self {
            bg_base: BG_STORM,
            bg_light: BG_HIGHLIGHT,
            bg_dark: BG_DARK,
            bg_highlight: BG_HIGHLIGHT,
            bg_hover: rgb(88, 88, 24),
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
            selection_bg: rgb(76, 75, 74),
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
            code_highlight_bg: rgb(96, 96, 32),
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
            paste_bg: rgb(80, 80, 16),
            paste_fg: FG_DARK,
            paste_dim: COMMENT,
            splash_hero_fg: FG_DARK,
            inline_edit_bg: BG_DARK,
        }
    }
}

