use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, MouseEvent};

use crate::views::picker::{PickerConfig, PickerOutcome, handle_picker_input};

use super::{
    ConnectMode, ProviderConnectState, ProviderDef, ProviderTab, TAB_LABELS, categorize,
    fuzzy_matches,
};

pub enum ConnectOutcome {
    Close,
    Configure {
        provider_id: String,
        api_key: Option<String>,
        set_default: bool,
    },
    Unchanged,
}

pub fn handle_provider_connect_key(
    state: &mut ProviderConnectState,
    key: KeyEvent,
) -> ConnectOutcome {
    match &state.mode.clone() {
        ConnectMode::Browse => handle_browse_key(state, key),
        ConnectMode::KeyInput {
            provider_id,
            input_buffer,
            set_default,
            ..
        } => handle_key_input(state, key, provider_id, input_buffer, *set_default),
    }
}

pub fn handle_provider_connect_mouse(
    state: &mut ProviderConnectState,
    mouse: MouseEvent,
) -> ConnectOutcome {
    if !matches!(state.mode, ConnectMode::Browse) {
        return ConnectOutcome::Unchanged;
    }
    let query = state.picker.query().to_string();
    let data = ProviderConnectState::picker_entry_data(
        &state.free_providers,
        &state.providers,
        &state.configured_ids,
        state.active_tab,
        &query,
    );

    let cfg = make_config(&data.non_sel, state.active_tab.index());
    let ev = Event::Mouse(mouse);
    let entry_count = data.len();
    let outcome = handle_picker_input(&ev, &mut state.picker, entry_count, &cfg);

    handle_picker_outcome(state, outcome, &data)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_config<'a>(non_sel: &'a [bool], active_tab: usize) -> PickerConfig<'a> {
    PickerConfig {
        title: None,
        show_search_hint: true,
        expandable: false,
        esc_clears_query: true,
        shortcuts: None,
        pending_hint: None,
        non_selectable: non_sel,
        non_selectable_clickable: &[],
        shortcuts_area: None,
        tabs: Some(&TAB_LABELS),
        active_tab,
        filter_label: None,
        filter_key_hint: None,
        filter_active: false,
        header_note: None,
        action_keys: &[],
        disable_search: false,
        compact_bottom_bar: false,
        search_only_on_slash: false,
        vim_normal_first: false,
    }
}

fn handle_picker_outcome(
    state: &mut ProviderConnectState,
    outcome: PickerOutcome,
    data: &super::PickerEntryData,
) -> ConnectOutcome {
    match outcome {
        PickerOutcome::Closed => ConnectOutcome::Close,
        PickerOutcome::TabChanged(idx) => {
            if let Some(tab) = ProviderTab::from_index(idx) {
                state.switch_tab(tab);
            }
            ConnectOutcome::Unchanged
        }
        PickerOutcome::Selected(idx) => handle_selection(state, idx, data),
        _ => ConnectOutcome::Unchanged,
    }
}

/// Count how many selectable provider entries appear before `idx`.
fn provider_count_before(idx: usize, data: &super::PickerEntryData) -> usize {
    data.non_sel.iter().take(idx).filter(|&&ns| !ns).count()
}

/// Map a picker entry index to a provider and open KeyInput mode.
fn handle_selection(
    state: &mut ProviderConnectState,
    idx: usize,
    data: &super::PickerEntryData,
) -> ConnectOutcome {
    // Skip non-selectable entries (category headers).
    if data.non_sel.get(idx).copied().unwrap_or(true) {
        return ConnectOutcome::Unchanged;
    }

    let prov_n = provider_count_before(idx, data);

    // Rebuild the provider list in the same order as picker_entry_data.
    let all: Vec<_> = state
        .free_providers
        .iter()
        .chain(state.providers.iter())
        .collect();
    let mut cat: Vec<&ProviderDef> = all
        .into_iter()
        .filter(|p| match state.active_tab {
            ProviderTab::All => true,
            ProviderTab::Free => categorize(p) == ProviderTab::Free,
            tab => categorize(p) == tab,
        })
        .collect();
    let query = state.picker.query().to_string();
    cat.retain(|p| fuzzy_matches(p.display_name(), &query));
    cat.sort_by(|a, b| a.display_name().cmp(b.display_name()));

    if let Some(p) = cat.get(prov_n) {
        state.mode = ConnectMode::KeyInput {
            provider_id: p.id.clone(),
            input_buffer: String::new(),
            set_default: true,
        };
        state.error_message = None;
    }
    ConnectOutcome::Unchanged
}

// ---------------------------------------------------------------------------
// Keyboard handling (Browse mode)
// ---------------------------------------------------------------------------

fn handle_browse_key(state: &mut ProviderConnectState, key: KeyEvent) -> ConnectOutcome {
    let query = state.picker.query().to_string();
    let data = ProviderConnectState::picker_entry_data(
        &state.free_providers,
        &state.providers,
        &state.configured_ids,
        state.active_tab,
        &query,
    );

    let cfg = make_config(&data.non_sel, state.active_tab.index());
    let ev = Event::Key(key);
    let entry_count = data.len();
    let outcome = handle_picker_input(&ev, &mut state.picker, entry_count, &cfg);

    handle_picker_outcome(state, outcome, &data)
}

// ---------------------------------------------------------------------------
// Keyboard handling (KeyInput mode)
// ---------------------------------------------------------------------------

fn handle_key_input(
    state: &mut ProviderConnectState,
    key: KeyEvent,
    provider_id: &str,
    input_buffer: &str,
    set_default: bool,
) -> ConnectOutcome {
    let all: Vec<_> = state
        .free_providers
        .iter()
        .chain(state.providers.iter())
        .collect();
    let free = all
        .iter()
        .find(|p| p.id == provider_id)
        .is_some_and(|p| p.auth_type == "none" || p.auth_type == "optional" || p.free == "true");

    match key.code {
        KeyCode::Esc => {
            state.mode = ConnectMode::Browse;
            state.error_message = None;
            ConnectOutcome::Unchanged
        }
        KeyCode::Enter => {
            if free {
                ConnectOutcome::Configure {
                    provider_id: provider_id.into(),
                    api_key: None,
                    set_default,
                }
            } else if input_buffer.trim().is_empty() {
                state.error_message = Some("API key cannot be empty.".into());
                ConnectOutcome::Unchanged
            } else {
                ConnectOutcome::Configure {
                    provider_id: provider_id.into(),
                    api_key: Some(input_buffer.trim().into()),
                    set_default,
                }
            }
        }
        KeyCode::Char(c) => {
            let mut nb = input_buffer.to_string();
            nb.push(c);
            state.mode = ConnectMode::KeyInput {
                provider_id: provider_id.into(),
                input_buffer: nb,
                set_default,
            };
            ConnectOutcome::Unchanged
        }
        KeyCode::Backspace => {
            let mut nb = input_buffer.to_string();
            nb.pop();
            state.mode = ConnectMode::KeyInput {
                provider_id: provider_id.into(),
                input_buffer: nb,
                set_default,
            };
            ConnectOutcome::Unchanged
        }
        _ => ConnectOutcome::Unchanged,
    }
}
