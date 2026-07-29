//! DX left minimap transplanted from
//! `crates/dx-tui/src/chat_render.rs::render_left_minimap`.

use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};
use std::time::Instant;

pub const WIDTH: u16 = 4;
pub const MIN_CHAT_WIDTH: u16 = 76;

#[derive(Debug, Clone, Default)]
pub struct MinimapUiState {
    pub scroll: u16,
    pub viewport: u16,
    pub area: Rect,
    pub top_indicator: Rect,
    pub bottom_indicator: Rect,
    pub active_turn: Option<usize>,
    pub hovered_turn: Option<usize>,
    pub hovered_since: Option<Instant>,
}

pub fn render_hover_card(
    state: &MinimapUiState,
    title: &str,
    description: &str,
    screen: Rect,
    buf: &mut Buffer,
    theme: &Theme,
) {
    if state
        .hovered_since
        .is_none_or(|at| at.elapsed().as_millis() < 280)
        || title.trim().is_empty()
    {
        return;
    }
    let width = screen.width.saturating_sub(6).clamp(32, 58);
    let height = 8.min(screen.height);
    if width == 0 || height == 0 {
        return;
    }
    let x = state
        .area
        .right()
        .saturating_add(1)
        .min(screen.right().saturating_sub(width));
    let hover_row = state
        .hovered_turn
        .and_then(|turn| turn.checked_sub(state.scroll as usize))
        .map(|row| state.area.y.saturating_add(row as u16))
        .unwrap_or(state.area.y);
    let y = hover_row
        .saturating_sub(1)
        .min(screen.bottom().saturating_sub(height));
    let card = Rect::new(x, y, width, height);
    Clear.render(card, buf);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.accent_user)
                .add_modifier(Modifier::BOLD),
        )))
        .border_style(Style::default().fg(theme.accent_user))
        .style(Style::default().fg(theme.text_primary).bg(theme.bg_light));
    Paragraph::new(description)
        .block(block)
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(theme.text_primary).bg(theme.bg_light))
        .render(card, buf);
}

pub fn split(area: Rect, visible: bool) -> (Rect, Option<Rect>) {
    if !visible || area.width < MIN_CHAT_WIDTH + WIDTH {
        return (area, None);
    }
    (
        Rect {
            x: area.x + WIDTH,
            y: area.y,
            width: area.width - WIDTH,
            height: area.height,
        },
        Some(Rect {
            x: area.x,
            y: area.y,
            width: WIDTH,
            height: area.height,
        }),
    )
}

pub fn render(
    state: &mut MinimapUiState,
    total_turns: usize,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].set_char(' ');
            buf[(x, y)].set_bg(theme.bg_base);
        }
    }
    state.top_indicator = Rect::default();
    state.bottom_indicator = Rect::default();
    state.area = area;

    if total_turns == 0 || area.height == 0 || area.width == 0 {
        state.viewport = 0;
        state.scroll = 0;
        return;
    }

    let selected = state
        .active_turn
        .unwrap_or_else(|| total_turns.saturating_sub(1))
        .min(total_turns.saturating_sub(1));
    let overflows = total_turns as u16 > area.height && area.height >= 3;
    let content_height = if overflows {
        area.height.saturating_sub(2).max(1)
    } else {
        (total_turns as u16).min(area.height)
    };
    state.viewport = content_height;
    let max_scroll = (total_turns as u16).saturating_sub(content_height);
    state.scroll = state.scroll.min(max_scroll);
    if selected < state.scroll as usize {
        state.scroll = selected as u16;
    } else if selected >= state.scroll as usize + content_height as usize {
        state.scroll = (selected + 1)
            .saturating_sub(content_height as usize)
            .min(max_scroll as usize) as u16;
    }

    let scroll = state.scroll as usize;
    let view_height = content_height as usize;
    let above = scroll;
    let below = total_turns.saturating_sub(scroll + view_height);
    let (content_area, top_indicator, bottom_indicator) = if overflows {
        (
            Rect::new(area.x, area.y + 1, area.width, content_height),
            Some(Rect::new(area.x, area.y, area.width, 1)),
            Some(Rect::new(
                area.x,
                area.y + 1 + content_height,
                area.width,
                1,
            )),
        )
    } else {
        let padding = area.height.saturating_sub(content_height) / 2;
        (
            Rect::new(area.x, area.y + padding, area.width, content_height),
            None,
            None,
        )
    };
    let count = |n: usize| {
        if n > 99 {
            "99+".to_string()
        } else {
            n.to_string()
        }
    };

    if let Some(indicator) = top_indicator {
        state.top_indicator = indicator;
        paint_left(
            indicator,
            &if above > 0 {
                format!("▴{}", count(above))
            } else {
                "▴".to_string()
            },
            Style::default().bg(theme.bg_base).fg(if above > 0 {
                theme.accent_user
            } else {
                theme.gray_dim
            }),
            buf,
        );
    }
    if let Some(indicator) = bottom_indicator {
        state.bottom_indicator = indicator;
        paint_left(
            indicator,
            &if below > 0 {
                format!("▾{}", count(below))
            } else {
                "▾".to_string()
            },
            Style::default().bg(theme.bg_base).fg(if below > 0 {
                theme.accent_user
            } else {
                theme.gray_dim
            }),
            buf,
        );
    }

    for row in 0..view_height {
        let turn = scroll + row;
        if turn >= total_turns {
            break;
        }
        let active = turn == selected;
        let hovered = state.hovered_turn == Some(turn);
        let color = if active {
            theme.accent_user
        } else if hovered {
            theme.text_primary
        } else {
            theme.gray_dim
        };
        let edge_distance = row.min(view_height.saturating_sub(1).saturating_sub(row));
        let symbol = if edge_distance == 0 {
            "━"
        } else if edge_distance == 1 && view_height > 3 {
            "━━"
        } else {
            "━━━"
        };
        let mut style = Style::default().fg(color).bg(theme.bg_base);
        if active {
            style = style.add_modifier(Modifier::BOLD);
        }
        paint_left(
            Rect::new(
                content_area.x,
                content_area.y + row as u16,
                content_area.width,
                1,
            ),
            symbol,
            style,
            buf,
        );
    }
    state.area = content_area;
}

fn paint_left(area: Rect, text: &str, style: Style, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    for (index, ch) in text.chars().enumerate() {
        let x = area.x + index as u16;
        if x >= area.right() {
            break;
        }
        buf[(x, area.y)].set_char(ch).set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimap_reserves_exact_dx_width() {
        let (chat, rail) = split(Rect::new(0, 0, 100, 30), true);
        assert_eq!(chat.x, WIDTH);
        assert_eq!(rail.expect("rail").width, WIDTH);
    }
}
