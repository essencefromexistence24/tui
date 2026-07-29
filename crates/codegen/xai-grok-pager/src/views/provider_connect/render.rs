use ratatui::layout::Rect;
use ratatui::prelude::Buffer;
use ratatui::style::Style;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};

use crate::theme::Theme;
use crate::views::modal_window::{
    self, ModalContentArea, ModalSizing, ModalWindowConfig, Shortcut,
};
use crate::views::picker::{self, PickerEntry, PickerRow};

use super::{ConnectMode, ProviderConnectState, TAB_LABELS};

pub fn render_provider_connect(buf: &mut Buffer, area: Rect, state: &mut ProviderConnectState) {
    let theme = Theme::current();
    let active_tab = state.active_tab.index();
    let cfg = ModalWindowConfig {
        title: "AI Provider Connect",
        tabs: Some(&TAB_LABELS),
        shortcuts: &[
            Shortcut {
                label: "Tab tabs",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "j/k nav",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "/ search",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "Enter configure",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "Esc close",
                clickable: false,
                id: 0,
            },
            Shortcut {
                label: "Space toggle accordion",
                clickable: false,
                id: 0,
            },
        ],
        sizing: ModalSizing {
            width_pct: 0.85,
            max_width: 120,
            min_width: 50,
            v_margin: 4,
            h_pad: 2,
            v_pad: 1,
            footer_lines: 2,
        },
        fold_info: None,
    };

    let Some(ModalContentArea {
        content: content_area,
        inner_x,
        inner_width,
        ..
    }) = modal_window::render_modal_window(buf, area, &mut state.window, &cfg, &theme)
    else {
        return;
    };

    state.window.active_tab = active_tab;

    match &state.mode {
        ConnectMode::Browse => {
            render_browse_list(buf, content_area, inner_x, inner_width, state, &theme)
        }
        ConnectMode::KeyInput {
            provider_id,
            input_buffer,
            set_default,
            ..
        } => render_key_input(
            buf,
            content_area,
            provider_id,
            input_buffer,
            *set_default,
            state,
            &theme,
        ),
    }
}

fn render_browse_list(
    buf: &mut Buffer,
    area: Rect,
    inner_x: u16,
    inner_width: u16,
    state: &mut ProviderConnectState,
    theme: &Theme,
) {
    let searching = state.picker.search_active;
    let query = state.picker.query().to_string();
    let data = ProviderConnectState::picker_entry_data(
        &state.free_providers,
        &state.providers,
        &state.configured_ids,
        state.active_tab,
        &query,
    );

    picker::render_picker_search_bar(
        buf,
        area.x,
        area.y,
        area.width,
        theme,
        &state.picker,
        searching,
        !searching,
        Some(theme.bg_base),
    );

    let sep_y = area.y + 1;
    if sep_y < area.y + area.height {
        picker::render_divider(buf, inner_x, sep_y, inner_width, theme, Some(theme.bg_base));
    }

    let esy = sep_y + 1;
    let ea = Rect {
        x: area.x,
        y: esy,
        width: area.width,
        height: area.height.saturating_sub(esy.saturating_sub(area.y)),
    };

    let empty: &[&str] = &[];
    let entries: Vec<PickerEntry<'_>> = data
        .labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            PickerEntry::Row(PickerRow {
                label: label.as_str(),
                right_label: "",
                selected: false,
                expanded: false,
                fields: &[],
                description_lines: empty,
                summary_lines: empty,
                dimmed: data.dimmed[i],
                indent: data.indents[i],
                collapsible: data.collapsible[i],
                badge: data.badges[i],
                badge_color: data.badge_colors[i],
                underline_last_desc: false,
            })
        })
        .collect();

    let content_hit = picker::render_picker_content_with_scrollbar_x(
        buf,
        ea,
        theme,
        &mut state.picker,
        &entries,
        &data.non_sel,
        &[],
        Some(theme.bg_base),
        false,
        0,
        inner_x + inner_width - 1,
    );
    state.picker.hit_areas = Some(picker::PickerHitAreas {
        close_button: Rect::default(),
        search_bar: Rect::new(area.x, area.y, area.width, 1),
        item_rects: content_hit.item_rects,
        entry_indices: content_hit.entry_indices,
        tab_rects: vec![],
        filter_rect: None,
    });
}

fn render_key_input(
    buf: &mut Buffer,
    area: Rect,
    provider_id: &str,
    input_buffer: &str,
    _set_default: bool,
    state: &ProviderConnectState,
    theme: &Theme,
) {
    let bg_style = Style::default().bg(theme.bg_base);
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_style(bg_style);
                cell.set_symbol(" ");
            }
        }
    }

    let ch = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Length(3),
            ratatui::layout::Constraint::Length(3),
            ratatui::layout::Constraint::Min(0),
        ])
        .margin(1)
        .split(area);

    let all: Vec<_> = state
        .free_providers
        .iter()
        .chain(state.providers.iter())
        .collect();
    let pvd = all.iter().find(|p| p.id == provider_id).copied();
    let pn = pvd.map(|p| p.display_name()).unwrap_or(provider_id);
    let free =
        pvd.is_some_and(|p| p.auth_type == "none" || p.auth_type == "optional" || p.free == "true");

    let pt = if free {
        format!("{pn} requires no API key. Press Enter to enable.")
    } else {
        let hint = pvd.map(|p| p.env_key_hint.as_str()).unwrap_or("API_KEY");
        format!("Paste your {hint} for {pn}:")
    };
    Paragraph::new(pt)
        .style(Style::default().fg(theme.gray))
        .render(ch[0], buf);

    let disp = if input_buffer.is_empty() && free {
        "(Press Enter to enable)".into()
    } else {
        input_buffer.to_string()
    };
    let bc = if state.error_message.is_some() {
        theme.accent_error
    } else {
        theme.gray
    };
    Paragraph::new(disp)
        .style(Style::default().fg(theme.text_primary))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" API Key ")
                .border_style(Style::default().fg(bc)),
        )
        .render(ch[1], buf);

    if let Some(ref e) = state.error_message {
        Paragraph::new(e.as_str())
            .style(Style::default().fg(theme.accent_error))
            .render(ch[2], buf);
    }
    if let Some(ref m) = state.status_message {
        Paragraph::new(m.as_str())
            .style(Style::default().fg(theme.accent_success))
            .render(ch[3], buf);
    }
}

/// Render the provider credential form inside another modal's content area.
///
/// The extensions modal uses this to keep provider setup in the unified
/// management surface without duplicating credential-field behavior.
pub(crate) fn render_embedded_key_input(
    buf: &mut Buffer,
    area: Rect,
    state: &ProviderConnectState,
) {
    if let ConnectMode::KeyInput {
        provider_id,
        input_buffer,
        set_default,
    } = &state.mode
    {
        render_key_input(
            buf,
            area,
            provider_id,
            input_buffer,
            *set_default,
            state,
            &Theme::current(),
        );
    }
}
