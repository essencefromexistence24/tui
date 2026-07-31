//! DX theme: "T3 Chat" — auto-generated from dx-tui themes.json.
use ratatui::style::{Color, Modifier};
use super::tokyonight::Theme;
const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }
#[allow(dead_code)]
mod palette {
    use super::*;
    pub const BG: Color = rgb(58, 57, 0);
    pub const BG_DARK: Color = rgb(54, 53, 0);
    pub const BG_STORM: Color = rgb(58, 57, 0);
    pub const BG_HIGHLIGHT: Color = rgb(74, 73, 16);
    pub const FG: Color = rgb(212, 208, 0);
    pub const FG_DARK: Color = rgb(170, 166, 0);
    pub const FG_GUTTER: Color = rgb(71, 69, 0);
    pub const COMMENT: Color = rgb(106, 104, 0);
    pub const DARK3: Color = rgb(85, 83, 0);
    pub const DARK5: Color = rgb(127, 125, 0);
    pub const ACCENT: Color = rgb(87, 83, 91);
    pub const ACCENT_DIM: Color = rgb(52, 50, 55);
    pub const GREEN: Color = rgb(116, 133, 72);
    pub const RED: Color = rgb(147, 28, 30);
    pub const RED1: Color = rgb($[Math]::Min(255, 63), $[Math]::Min(255, 52), $[Math]::Min(255, 53));
    pub const YELLOW: Color = rgb(117, 83, 46);
    pub const CYAN: Color = rgb(131, 123, 136);
    pub const BLUE: Color = rgb(44, 42, 121);
    pub const MAGENTA: Color = rgb(127, 42, 131);
    pub const PURPLE: Color = rgb(107, 28, 111);
    pub const ORANGE: Color = rgb(123, 52, 30);
    pub const RED_DARK: Color = rgb(174, 14, 20);
    pub const GREEN_DARK: Color = rgb(6, 181, 6);
}
use palette::*;

impl Theme {
    pub const fn t3_chat() -> Self {
        Self {
            bg_base: BG_STORM,
            bg_light: BG_HIGHLIGHT,
            bg_dark: BG_DARK,
            bg_highlight: BG_HIGHLIGHT,
            bg_hover: rgb(82, 81, 24),
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
            selection_bg: rgb(94, 92, 96),
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
            code_highlight_bg: rgb(90, 89, 32),
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
            paste_bg: rgb(74, 73, 16),
            paste_fg: FG_DARK,
            paste_dim: COMMENT,
            splash_hero_fg: FG_DARK,
            inline_edit_bg: BG_DARK,
        }
    }
}

