//! DX right-sidebar renderer, transplanted from
//! `crates/dx-tui/src/chat_render.rs::render_sidebar`.
//!
//! The original accordion, styling, clipping, scrolling, and hit-area
//! behavior is retained. Data is supplied as borrowed Grok view data so this
//! module never owns a second chat or agent state.

use crate::theme::Theme;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};
use unicode_width::UnicodeWidthStr;

pub const SECTION_COUNT: usize = 8;
pub const TASKS_SECTION: usize = 0;
pub const WORKFLOWS_SECTION: usize = 1;
pub const PROMPTS_SECTION: usize = 2;
pub const NOTES_SECTION: usize = 3;
pub const SUBAGENTS_SECTION: usize = 4;
pub const PLUGINS_SECTION: usize = 6;
pub const MCP_SECTION: usize = 7;
pub const SECTION_NAMES: [&str; SECTION_COUNT] = [
    "Tasks",
    "Workflows",
    "Prompts",
    "Notes",
    "Subagents",
    "LSP",
    "Plugins",
    "MCP",
];
pub const MIN_WIDTH: u16 = 100;
pub const PANEL_WIDTH: u16 = 40;

#[derive(Debug, Clone)]
pub struct SidebarSection {
    pub name: &'static str,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SidebarViewModel {
    pub title: String,
    pub session_id: String,
    pub cwd: String,
    pub version: String,
    pub sections: [SidebarSection; SECTION_COUNT],
}

#[derive(Debug, Clone)]
pub struct SidebarUiState {
    pub accordion_open: [bool; SECTION_COUNT],
    pub scroll: u16,
    pub panel_area: Rect,
    pub section_areas: [Rect; SECTION_COUNT],
    pub row_areas: Vec<(usize, usize, Rect)>,
}

impl Default for SidebarUiState {
    fn default() -> Self {
        Self {
            accordion_open: [false, false, false, true, false, false, false, false],
            scroll: 0,
            panel_area: Rect::default(),
            section_areas: [Rect::default(); SECTION_COUNT],
            row_areas: Vec::new(),
        }
    }
}

impl SidebarUiState {
    pub fn toggle_section(&mut self, index: usize) {
        if let Some(open) = self.accordion_open.get_mut(index) {
            *open = !*open;
        }
    }

    pub fn scroll_by(&mut self, delta: i16) {
        self.scroll = if delta.is_negative() {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta as u16)
        };
    }
}

pub fn split(area: Rect, visible: bool) -> (Rect, Option<Rect>) {
    if !visible || area.width < MIN_WIDTH {
        return (area, None);
    }
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(60), Constraint::Length(PANEL_WIDTH)])
        .split(area);
    (chunks[0], Some(chunks[1]))
}

pub fn render(
    state: &mut SidebarUiState,
    model: &SidebarViewModel,
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
) {
    state.panel_area = area;
    state.section_areas = [Rect::default(); SECTION_COUNT];
    state.row_areas.clear();

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].reset();
            buf[(x, y)].set_bg(theme.bg_dark);
        }
    }

    let inner = area;
    let title_rows = wrapped_rows(&model.title, inner.width.saturating_sub(1)).clamp(2, 5);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(title_rows + 1),
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let title_area = Rect {
        x: chunks[1].x.saturating_add(1),
        y: chunks[1].y,
        width: chunks[1].width.saturating_sub(1),
        height: title_rows,
    };
    Paragraph::new(Span::styled(
        model.title.as_str(),
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    ))
    .wrap(ratatui::widgets::Wrap { trim: true })
    .render(title_area, buf);
    Paragraph::new(Span::styled(
        format!("#{}", model.session_id),
        Style::default().fg(theme.gray).add_modifier(Modifier::DIM),
    ))
    .render(
        Rect {
            x: title_area.x,
            y: title_area.bottom(),
            width: title_area.width,
            height: 1,
        },
        buf,
    );

    let sections_area = chunks[3];
    let heights: Vec<u16> = model
        .sections
        .iter()
        .enumerate()
        .map(|(i, section)| {
            if state.accordion_open[i] {
                1 + if i == NOTES_SECTION {
                    section.lines.len().max(8) as u16
                } else {
                    section.lines.len().max(1) as u16
                }
            } else {
                1
            }
        })
        .collect();
    let content_height: u16 = heights.iter().sum();
    state.scroll = state
        .scroll
        .min(content_height.saturating_sub(sections_area.height));

    let mut content_y = 0u16;
    let mut notes_box_area: Option<Rect> = None;
    for (section_index, (section, section_height)) in
        model.sections.iter().zip(heights.iter()).enumerate()
    {
        let open = state.accordion_open[section_index];
        let section_top = content_y;
        content_y = content_y.saturating_add(*section_height);
        if section_index == NOTES_SECTION && open && section_top >= state.scroll {
            let notes_y = sections_area.y + section_top - state.scroll;
            let notes_height = (*section_height).min(sections_area.bottom().saturating_sub(notes_y));
            if notes_height >= 3 {
                // Deferred until after the row loop so its borders paint on
                // top of the following section's header (previously the next
                // header overwrote the notes box's left/bottom border with a
                // different color).
                notes_box_area = Some(Rect {
                    x: sections_area.x + 1,
                    y: notes_y,
                    width: sections_area.width.saturating_sub(3),
                    height: notes_height,
                });
            }
        }
        for row in 0..*section_height {
            let absolute_row = section_top + row;
            if absolute_row < state.scroll {
                continue;
            }
            let screen_row = absolute_row - state.scroll;
            if screen_row >= sections_area.height {
                break;
            }
            let notes_body = section_index == NOTES_SECTION && open;
            let row_area = Rect {
                x: sections_area.x + if notes_body { 2 } else { 1 },
                y: sections_area.y + screen_row,
                width: sections_area
                    .width
                    .saturating_sub(if notes_body { 5 } else { 3 }),
                height: 1,
            };
            if row == 0 {
                if section_index == NOTES_SECTION && open {
                    continue;
                }
                state.section_areas[section_index] = row_area;
                let chevron = if open { "▼" } else { "▶" };
                let count = section
                    .lines
                    .iter()
                    .filter(|line| !line.trim().is_empty() && !line.starts_with("No "))
                    .count();
                let label = if count > 0 {
                    format!("{chevron} {} · {count}", section.name)
                } else {
                    format!("{chevron} {}", section.name)
                };
                Paragraph::new(Span::styled(
                    label,
                    Style::default()
                        .fg(theme.gray_bright)
                        .add_modifier(Modifier::BOLD),
                ))
                .render(row_area, buf);
            } else if open {
                let body_index = (row - 1) as usize;
                let line = section
                    .lines
                    .get(body_index)
                    .map(String::as_str)
                    .unwrap_or("");
                let empty = line.trim().is_empty() || line.starts_with("No ");
                let style = if empty {
                    Style::default()
                        .fg(theme.gray)
                        .add_modifier(Modifier::ITALIC)
                } else if section_index == TASKS_SECTION && line.contains("[done]") {
                    Style::default()
                        .fg(theme.accent_success)
                        .add_modifier(Modifier::DIM)
                } else if (section_index == TASKS_SECTION || section_index == WORKFLOWS_SECTION)
                    && line.contains("[active]")
                {
                    Style::default()
                        .fg(theme.warning)
                        .add_modifier(Modifier::BOLD)
                } else if section_index == TASKS_SECTION && line.contains("[cancelled]") {
                    Style::default()
                        .fg(theme.gray)
                        .add_modifier(Modifier::CROSSED_OUT)
                } else {
                    Style::default().fg(theme.text_primary)
                };
                let display = ellipsize(&format!("  {line}"), row_area.width as usize);
                Paragraph::new(Span::styled(display, style)).render(row_area, buf);
                if !empty {
                    state.row_areas.push((section_index, body_index, row_area));
                }
            }
        }
    }

    // Render the Notes box on top of everything (borders must not be
    // overwritten by the following section's header row).  Clicking the box
    // dispatches EditNote so the user can add or update notes directly — no
    // separate context menu needed.
    if let Some(notes_area) = notes_box_area {
        state.section_areas[NOTES_SECTION] = notes_area;
        Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme.gray_dim))
            .title(Span::styled(
                " Notes ",
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ))
            .render(notes_area, buf);
    }

    if content_height > sections_area.height && sections_area.height > 0 {
        let x = sections_area.right().saturating_sub(1);
        let thumb_height = ((sections_area.height as u32 * sections_area.height as u32)
            / content_height as u32)
            .max(1) as u16;
        let max_thumb_y = sections_area.height.saturating_sub(thumb_height);
        let max_scroll = content_height.saturating_sub(sections_area.height).max(1);
        let thumb_y = (state.scroll as u32 * max_thumb_y as u32 / max_scroll as u32) as u16;
        for y in sections_area.top()..sections_area.bottom() {
            let cell = &mut buf[(x, y)];
            cell.set_char('│');
            cell.set_fg(theme.scrollbar_bg);
        }
        for y in 0..thumb_height {
            let cell = &mut buf[(x, sections_area.y + thumb_y + y)];
            cell.set_char('┃');
            cell.set_fg(theme.scrollbar_fg);
        }
    }

    Paragraph::new(Span::styled(
        format!(
            " {}",
            truncate_start(&model.cwd, chunks[4].width.saturating_sub(1) as usize)
        ),
        Style::default().fg(theme.text_primary),
    ))
    .render(chunks[4], buf);
    Paragraph::new(Span::styled(
        format!(" {}", model.version),
        Style::default().fg(theme.gray),
    ))
    .render(chunks[5], buf);
}

fn wrapped_rows(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }
    (UnicodeWidthStr::width(text) as u16).div_ceil(width).max(1)
}

fn ellipsize(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        if UnicodeWidthStr::width(out.as_str())
            + unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0)
            >= max_width
        {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn truncate_start(text: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    let reversed: String = text.chars().rev().collect();
    let tail: String = ellipsize(&reversed, max_width)
        .trim_end_matches('…')
        .chars()
        .rev()
        .collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_layout_keeps_chat_full_width() {
        let area = Rect::new(0, 0, 80, 24);
        assert_eq!(split(area, true), (area, None));
    }

    #[test]
    fn wide_layout_reserves_dx_sidebar() {
        let (chat, sidebar) = split(Rect::new(0, 0, 120, 30), true);
        assert_eq!(chat.width, 90);
        assert_eq!(sidebar.expect("sidebar").width, PANEL_WIDTH);
    }
}
