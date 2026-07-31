//! DX theme: "Ocean Breeze" — auto-generated from dx-tui themes.json.
use ratatui::style::{Color, Modifier};
use super::tokyonight::Theme;
const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }
#[allow(dead_code)]
mod palette {
    use super::*;
    pub const BG: Color = rgb(47, 51, 0);
    pub const BG_DARK: Color = rgb(43, 47, 0);
    pub const BG_STORM: Color = rgb(47, 51, 0);
    pub const BG_HIGHLIGHT: Color = rgb(63, 67, 16);
    pub const FG: Color = rgb(217, 218, 0);
    pub const FG_DARK: Color = rgb(174, 174, 0);
    pub const FG_GUTTER: Color = rgb(72, 73, 0);
    pub const COMMENT: Color = rgb(108, 109, 0);
    pub const DARK3: Color = rgb(87, 87, 0);
    pub const DARK5: Color = rgb(130, 131, 0);
    pub const ACCENT: Color = rgb(84, 87, 92);
    pub const ACCENT_DIM: Color = rgb(50, 52, 55);
    pub const GREEN: Color = rgb(120, 137, 74);
    pub const RED: Color = rgb(144, 29, 31);
    pub const RED1: Color = rgb($[Math]::Min(255, 194), $[Math]::Min(255, 143), $[Math]::Min(255, 140));
    pub const YELLOW: Color = rgb(114, 87, 46);
    pub const CYAN: Color = rgb(132, 127, 138);
    pub const BLUE: Color = rgb(42, 44, 122);
    pub const MAGENTA: Color = rgb(124, 44, 132);
    pub const PURPLE: Color = rgb(104, 29, 112);
    pub const ORANGE: Color = rgb(127, 54, 31);
    pub const RED_DARK: Color = rgb(141, 14, 20);
    pub const GREEN_DARK: Color = rgb(6, 163, 6);
}
use palette::*;

impl Theme {
    pub const fn ocean_breeze() -> Self {
        Self {
            bg_base: BG_STORM,
            bg_light: BG_HIGHLIGHT,
            bg_dark: BG_DARK,
            bg_highlight: BG_HIGHLIGHT,
            bg_hover: rgb(71, 75, 24),
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
            selection_bg: rgb(92, 94, 96),
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
            code_highlight_bg: rgb(79, 83, 32),
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
            paste_bg: rgb(63, 67, 16),
            paste_fg: FG_DARK,
            paste_dim: COMMENT,
            splash_hero_fg: FG_DARK,
            inline_edit_bg: BG_DARK,
        }
    }
}

