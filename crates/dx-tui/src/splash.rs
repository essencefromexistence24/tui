//! Splash screen rendering with figlet fonts

use super::theme::ChatTheme;
use crate::effects::RainbowEffect;
use figlet_rs::FIGlet;
use once_cell::sync::Lazy;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

/// Max FIGlet rows used for the title so huge fonts never dominate the splash.
const MAX_FIGLET_ROWS: u16 = 12;
/// Leave room for blank line + description under the title.
const SPLASH_TAIL_ROWS: u16 = 3;

pub fn render(
    area: Rect,
    buf: &mut Buffer,
    theme: &ChatTheme,
    font_index: usize,
    rainbow: &RainbowEffect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Fill splash plate with the active theme so mode/theme switches recolor fully.
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buf[(x, y)];
            cell.reset();
            cell.set_bg(theme.bg);
        }
    }

    let all_fonts = get_valid_fonts();
    if all_fonts.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "Dx",
            Style::default().fg(theme.accent),
        )))
        .alignment(ratatui::layout::Alignment::Center)
        .render(area, buf);
        return;
    }
    let current_font = all_fonts[font_index % all_fonts.len()].as_str();

    let max_title_rows = area
        .height
        .saturating_sub(SPLASH_TAIL_ROWS)
        .clamp(1, MAX_FIGLET_ROWS);

    let figlet_lines =
        render_figlet_title(current_font, area.width, max_title_rows, rainbow, theme);

    let mut splash_lines = figlet_lines;
    splash_lines.push(Line::from(""));
    let desc = "Enhanced Development Experience";
    let desc_trimmed: String = desc.chars().take(area.width as usize).collect();
    splash_lines.push(Line::from(Span::styled(
        desc_trimmed,
        Style::default().fg(theme.muted_fg),
    )));
    // Bottom hints removed — the FIGlet logo and description are enough;
    // keyboard shortcuts are discoverable via the command palette / help.

    let content_height = splash_lines.len() as u16;
    let available_height = area.height;
    let vertical_offset = if content_height >= available_height {
        0
    } else {
        (available_height.saturating_sub(content_height)) / 2
    };

    let centered_area = Rect {
        x: area.x,
        y: area.y + vertical_offset,
        width: area.width,
        height: content_height.min(available_height),
    };

    Paragraph::new(splash_lines)
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default())
        .render(centered_area, buf);
}

fn render_figlet_title(
    font_name: &str,
    max_width: u16,
    max_rows: u16,
    rainbow: &RainbowEffect,
    theme: &ChatTheme,
) -> Vec<Line<'static>> {
    let Ok(font_data) = crate::font::read_font(font_name) else {
        return compact_dx_title(rainbow, theme);
    };
    // FIGlet fonts are Latin-1 / ASCII — never fail UTF-8 strictly
    let font_str = String::from_utf8_lossy(&font_data);
    let hardblank = extract_hardblank(&font_str);

    let Ok(font) = FIGlet::from_content(&font_str) else {
        return compact_dx_title(rainbow, theme);
    };

    // Some fonts lack uppercase or certain glyphs — try variants
    let figure = font
        .convert("Dx")
        .or_else(|| font.convert("DX"))
        .or_else(|| font.convert("dx"))
        .or_else(|| font.convert("D"));

    let Some(figure) = figure else {
        return compact_dx_title(rainbow, theme);
    };

    let mut lines: Vec<String> = figure
        .to_string()
        .lines()
        .map(|s| {
            // Hardblank must render as space (otherwise fonts look "broken" with $ / @)
            s.chars()
                .map(|c| if c == hardblank { ' ' } else { c })
                .collect::<String>()
        })
        .collect();

    // Pad all lines to the maximum width so Alignment::Center keeps the ascii art aligned
    let max_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    for line in &mut lines {
        let len = line.chars().count();
        if len < max_len {
            line.push_str(&" ".repeat(max_len - len));
        }
    }

    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }

    // Drop lines that are only spaces
    if lines.iter().all(|l| l.trim().is_empty()) {
        return compact_dx_title(rainbow, theme);
    }

    // Keep vertical center portion so logo stays visible
    if lines.len() as u16 > max_rows {
        let start = (lines.len() - max_rows as usize) / 2;
        let end = start + max_rows as usize;
        if end <= lines.len() {
            lines = lines[start..end].to_vec();
        } else {
            lines.truncate(max_rows as usize);
        }
    }

    let max_w = max_width as usize;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        // Fit width: prefer center crop for wide logos so "DX" stays visible
        let fitted = fit_width(&line, max_w);
        if fitted.chars().all(|c| c == ' ') {
            continue;
        }
        let mut spans = Vec::new();
        for (i, ch) in fitted.chars().enumerate() {
            if ch.is_control() {
                continue;
            }
            let color = if ch == ' ' {
                theme.muted_fg
            } else {
                rainbow.color_at(i)
            };
            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        if !spans.is_empty() {
            out.push(Line::from(spans));
        }
    }

    if out.is_empty() {
        return compact_dx_title(rainbow, theme);
    }
    out
}

/// FIGlet header: `flf2aX` where X is the hardblank character.
fn extract_hardblank(font_str: &str) -> char {
    let first = font_str.lines().next().unwrap_or("");
    // flf2a$ or flf2a@ …
    if let Some(rest) = first.strip_prefix("flf2a") {
        return rest.chars().next().unwrap_or('$');
    }
    if let Some(rest) = first.strip_prefix("flf2A") {
        return rest.chars().next().unwrap_or('$');
    }
    '$'
}

/// Fit a FIGlet line into `max_w` cells, keeping the middle when possible.
fn fit_width(s: &str, max_w: usize) -> String {
    let count = s.chars().count();
    if max_w == 0 {
        return String::new();
    }
    if count <= max_w {
        return s.to_string();
    }
    // Center-crop so the bulk of "DX" stays
    let start = (count - max_w) / 2;
    s.chars().skip(start).take(max_w).collect()
}

fn compact_dx_title(rainbow: &RainbowEffect, theme: &ChatTheme) -> Vec<Line<'static>> {
    // Small built-in block "DX" so we never show a blank splash
    const BLOCK: &[&str] = &[
        "██████╗ ██╗  ██╗",
        "██╔══██╗╚██╗██╔╝",
        "██║  ██║ ╚███╔╝ ",
        "██║  ██║ ██╔██╗ ",
        "██████╔╝██╔╝ ██╗",
        "╚═════╝ ╚═╝  ╚═╝",
    ];
    let mut lines = Vec::new();
    for row in BLOCK {
        let mut spans = Vec::new();
        for (i, ch) in row.chars().enumerate() {
            let color = if ch == ' ' {
                theme.muted_fg
            } else {
                rainbow.color_at(i)
            };
            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// All embedded fonts; preferred readable fonts first in the cycle.
fn get_valid_fonts() -> &'static [String] {
    static FONTS: Lazy<Vec<String>> = Lazy::new(|| {
        let mut names = crate::font::list_font_names();
        let preferred = [
            "Small",
            "Doom",
            "Slant",
            "Block",
            "Banner3",
            "Ogre",
            "Shadow",
            "Rounded",
            "Mini",
            "Short",
            "Chunky",
            "Epic",
            "Colossal",
            "Standard",
            "Big",
            "ANSI Shadow",
            "ansi_shadow",
        ];
        names.sort_by(|a, b| {
            let ai = preferred
                .iter()
                .position(|p| p.eq_ignore_ascii_case(a))
                .unwrap_or(999);
            let bi = preferred
                .iter()
                .position(|p| p.eq_ignore_ascii_case(b))
                .unwrap_or(999);
            ai.cmp(&bi).then_with(|| a.cmp(b))
        });
        // Drop known empty / test fonts that never render
        names.retain(|n| {
            let l = n.to_ascii_lowercase();
            !l.contains("test") && l != "term" && !l.is_empty()
        });
        if names.is_empty() {
            names.push("Small".into());
        }
        names
    });
    FONTS.as_slice()
}

/// Public count for font cycling.
pub fn splash_font_count() -> usize {
    get_valid_fonts().len().max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn splash_logo_uses_animated_rainbow_colors() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        let pager_theme = crate::theme::Theme::tokyonight();
        let theme = ChatTheme::from(&pager_theme);

        render(
            area,
            &mut buffer,
            &theme,
            0,
            &RainbowEffect::new(),
        );

        let logo_colors = buffer
            .content
            .iter()
            .filter(|cell| cell.symbol() != " ")
            .map(|cell| cell.fg)
            .collect::<std::collections::HashSet<_>>();
        assert!(logo_colors.len() > 1);
        assert!(logo_colors.iter().all(|color| matches!(color, Color::Rgb(_, _, _))));
    }
}
