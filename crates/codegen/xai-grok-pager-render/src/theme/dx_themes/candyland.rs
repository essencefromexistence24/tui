//! DX theme: "Candyland" — auto-generated from dx-tui themes.json.
use ratatui::style::{Color, Modifier};
use super::tokyonight::Theme;
const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }
#[allow(dead_code)]
mod palette {
    use super::*;
    pub const BG: Color = rgb(54, 55, 0);
    pub const BG_DARK: Color = rgb(50, 51, 0);
    pub const BG_STORM: Color = rgb(54, 55, 0);
    pub const BG_HIGHLIGHT: Color = rgb(70, 71, 16);
    pub const FG: Color = rgb(232, 232, 0);
    pub const FG_DARK: Color = rgb(186, 186, 0);
    pub const FG_GUTTER: Color = rgb(77, 77, 0);
    pub const COMMENT: Color = rgb(116, 116, 0);
    pub const DARK3: Color = rgb(93, 93, 0);
    pub const DARK5: Color = rgb(139, 139, 0);
    pub const ACCENT: Color = rgb(184, 205, 212);
    pub const ACCENT_DIM: Color = rgb(110, 123, 127);
    pub const GREEN: Color = rgb(214, 255, 132);
    pub const RED: Color = rgb(244, 68, 71);
    pub const RED1: Color = rgb($[Math]::Min(255, 194), $[Math]::Min(255, 143), $[Math]::Min(255, 140));
    pub const YELLOW: Color = rgb(214, 205, 106);
    pub const CYAN: Color = rgb(252, 245, 255);
    pub const BLUE: Color = rgb(92, 102, 242);
    pub const MAGENTA: Color = rgb(224, 102, 252);
    pub const PURPLE: Color = rgb(204, 68, 232);
    pub const ORANGE: Color = rgb(245, 112, 71);
    pub const RED_DARK: Color = rgb(162, 14, 20);
    pub const GREEN_DARK: Color = rgb(6, 175, 6);
}
use palette::*;

impl Theme {
    pub const fn candyland() -> Self {
        Self {
            bg_base: BG_STORM,
            bg_light: BG_HIGHLIGHT,
            bg_dark: BG_DARK,
            bg_highlight: BG_HIGHLIGHT,
            bg_hover: rgb(78, 79, 24),
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
            selection_bg: rgb(142, 152, 156),
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
            code_highlight_bg: rgb(86, 87, 32),
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
            paste_bg: rgb(70, 71, 16),
            paste_fg: FG_DARK,
            paste_dim: COMMENT,
            splash_hero_fg: FG_DARK,
            inline_edit_bg: BG_DARK,
        }
    }
}

