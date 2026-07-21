use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use crate::views::picker::{PickerConfig, PickerOutcome, handle_picker_input};

use super::{ConnectMode, ProviderConnectState};

pub enum ConnectOutcome {
    Close,
    Configure { provider_id: String, api_key: Option<String>, set_default: bool },
    Unchanged,
}

pub fn handle_provider_connect_key(state: &mut ProviderConnectState, key: KeyEvent) -> ConnectOutcome {
    match &state.mode.clone() {
        ConnectMode::Browse => handle_browse_key(state, key),
        ConnectMode::KeyInput { provider_id, input_buffer, set_default, .. } =>
            handle_key_input(state, key, provider_id, input_buffer, *set_default),
    }
}

pub fn handle_provider_connect_mouse(state: &mut ProviderConnectState, mouse: MouseEvent) -> ConnectOutcome {
    if !matches!(state.mode, ConnectMode::Browse) { return ConnectOutcome::Unchanged; }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(ref hits) = state.picker.hit_areas {
                let pos = ratatui::layout::Position::new(mouse.column, mouse.row);
                for (vi, rect) in hits.item_rects.iter().enumerate() {
                    if rect.contains(pos) {
                        if let Some(&ei) = hits.entry_indices.get(vi) {
                            return handle_click(state, ei);
                        }
                    }
                }
            }
            ConnectOutcome::Unchanged
        }
        MouseEventKind::ScrollDown => {
            let (entries, non_sel) = ProviderConnectState::picker_entries(
                &state.free_providers, &state.providers, &state.configured_ids,
            );
            if !entries.is_empty() {
                let mut sel = state.picker.selected;
                let mut next = sel + 1;
                while next < entries.len() && non_sel.get(next).copied().unwrap_or(true) {
                    next += 1;
                }
                if next < entries.len() {
                    state.picker.selected = next;
                }
                drop((entries, non_sel));
            }
            ConnectOutcome::Unchanged
        }
        MouseEventKind::ScrollUp => {
            let (entries, non_sel) = ProviderConnectState::picker_entries(
                &state.free_providers, &state.providers, &state.configured_ids,
            );
            if !entries.is_empty() {
                let mut prev = state.picker.selected.saturating_sub(1);
                while prev > 0 && non_sel.get(prev).copied().unwrap_or(true) {
                    prev = prev.saturating_sub(1);
                }
                if prev < entries.len() && !non_sel.get(prev).copied().unwrap_or(true) {
                    state.picker.selected = prev;
                }
                drop((entries, non_sel));
            }
            ConnectOutcome::Unchanged
        }
        _ => ConnectOutcome::Unchanged,
    }
}

fn handle_click(state: &mut ProviderConnectState, idx: usize) -> ConnectOutcome {
    let (entries, non_sel) = ProviderConnectState::picker_entries(
        &state.free_providers, &state.providers, &state.configured_ids,
    );
    if idx >= entries.len() || non_sel.get(idx).copied().unwrap_or(true) {
        return ConnectOutcome::Unchanged;
    }
    let fc = state.free_providers.len();
    let di = idx.saturating_sub(1);
    let pid = if di < fc {
        state.free_providers.get(di).map(|p| p.id.clone())
    } else {
        state.providers.get(di.saturating_sub(fc)).map(|p| p.id.clone())
    };
    drop((entries, non_sel));
    if let Some(pid) = pid {
        state.mode = ConnectMode::KeyInput { provider_id: pid, input_buffer: String::new(), set_default: true };
        state.error_message = None;
    }
    ConnectOutcome::Unchanged
}

/// Translate a picker selection outcome into opening the KeyInput modal.
fn handle_selection(state: &mut ProviderConnectState, outcome: PickerOutcome) -> ConnectOutcome {
    match outcome {
        PickerOutcome::Selected(idx) | PickerOutcome::Expand(idx) => handle_click(state, idx),
        PickerOutcome::Closed => ConnectOutcome::Close,
        _ => ConnectOutcome::Unchanged,
    }
}

fn make_config<'a>(non_sel: &'a [bool]) -> PickerConfig<'a> {
    PickerConfig {
        title: None, show_search_hint: true, expandable: false, esc_clears_query: true,
        shortcuts: None, pending_hint: None, non_selectable: non_sel, non_selectable_clickable: &[],
        shortcuts_area: None, tabs: None, active_tab: 0,
        filter_label: None, filter_key_hint: None, filter_active: false,
        action_keys: &[], disable_search: false, compact_bottom_bar: false,
        search_only_on_slash: false, vim_normal_first: false,
    }
}

fn handle_browse_key(state: &mut ProviderConnectState, key: KeyEvent) -> ConnectOutcome {
    let outcome = {
        let (entries, non_sel) = ProviderConnectState::picker_entries(
            &state.free_providers, &state.providers, &state.configured_ids,
        );
        let cfg = make_config(&non_sel);
        let ev = ratatui::crossterm::event::Event::Key(key);
        handle_picker_input(&ev, &mut state.picker, entries.len(), &cfg)
    };
    handle_selection(state, outcome)
}

fn handle_key_input(
    state: &mut ProviderConnectState, key: KeyEvent,
    provider_id: &str, input_buffer: &str, set_default: bool,
) -> ConnectOutcome {
    let all: Vec<_> = state.free_providers.iter().chain(state.providers.iter()).collect();
    let free = all.iter().find(|p| p.id == provider_id)
        .is_some_and(|p| p.auth_type == "none" || p.auth_type == "optional" || p.free == "true");

    match key.code {
        KeyCode::Esc => { state.mode = ConnectMode::Browse; state.error_message = None; ConnectOutcome::Unchanged }
        KeyCode::Enter => {
            if free { ConnectOutcome::Configure { provider_id: provider_id.into(), api_key: None, set_default } }
            else if input_buffer.trim().is_empty() {
                state.error_message = Some("API key cannot be empty.".into()); ConnectOutcome::Unchanged
            } else {
                ConnectOutcome::Configure { provider_id: provider_id.into(), api_key: Some(input_buffer.trim().into()), set_default }
            }
        }
        KeyCode::Char(c) => {
            let mut nb = input_buffer.to_string(); nb.push(c);
            state.mode = ConnectMode::KeyInput { provider_id: provider_id.into(), input_buffer: nb, set_default };
            ConnectOutcome::Unchanged
        }
        KeyCode::Backspace => {
            let mut nb = input_buffer.to_string(); nb.pop();
            state.mode = ConnectMode::KeyInput { provider_id: provider_id.into(), input_buffer: nb, set_default };
            ConnectOutcome::Unchanged
        }
        _ => ConnectOutcome::Unchanged,
    }
}
