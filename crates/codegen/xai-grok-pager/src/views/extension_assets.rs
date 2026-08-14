//! Optional visual assets used by the Extensions modal.
//!
//! Logo art is deliberately loaded only when a row is expanded. The Connects
//! catalog can contain thousands of entries, so loading every text asset while
//! building the picker would make opening the modal needlessly expensive.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::channel_connect::ChannelEntry;

/// Read the generated ASCII art for a connect node, if the installed asset
/// bundle contains it.
pub fn connect_ascii(node: &dx_connect::NodeDefinition) -> Option<String> {
    let mut stems = Vec::new();
    if let Some(last) = node.id.rsplit('.').next() {
        push_stem_variants(&mut stems, last);
    }
    push_stem_variants(&mut stems, &node.display_name);
    read_asset("connects", &stems)
}

/// Read the generated ASCII art for a messaging channel, if available.
pub fn channel_ascii(channel: &ChannelEntry) -> Option<String> {
    let mut stems = Vec::new();
    push_stem_variants(&mut stems, channel.name);
    push_stem_variants(&mut stems, channel.kind);
    read_asset("channels", &stems)
}

fn push_stem_variants(stems: &mut Vec<String>, raw: &str) {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return;
    }
    push_unique(stems, normalized.replace('_', "-"));
    push_unique(
        stems,
        normalized
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-"),
    );
    push_unique(
        stems,
        normalized
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect(),
    );
}

fn push_unique(stems: &mut Vec<String>, stem: String) {
    if !stem.is_empty() && !stems.iter().any(|existing| existing == &stem) {
        stems.push(stem);
    }
}

fn read_asset(category: &str, stems: &[String]) -> Option<String> {
    let cache = asset_cache();
    for root in asset_roots() {
        for stem in stems {
            let path = root
                .join("ascii")
                .join(category)
                .join(format!("{stem}.txt"));
            let text = {
                let mut entries = cache.lock().expect("extension asset cache poisoned");
                if let Some(cached) = entries.get(&path) {
                    cached.clone()
                } else {
                    let loaded = read_text_asset(&path);
                    entries.insert(path.clone(), loaded.clone());
                    loaded
                }
            };
            if let Some(text) = text {
                return Some(text);
            }
        }
    }
    None
}

fn asset_cache() -> &'static Mutex<HashMap<PathBuf, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn read_text_asset(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim_end_matches(['\r', '\n']);
    (!text.trim().is_empty()).then(|| text.to_owned())
}

fn asset_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("DX_LOGOS_ROOT").map(PathBuf::from) {
        roots.push(root);
    }

    // Development checkout: CARGO_MANIFEST_DIR is the pager crate.
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../logos"));

    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.push(parent.join("logos"));
    }

    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        roots.push(local_app_data.join("dx").join("logos"));
        roots.push(local_app_data.join("Dx").join("tui").join("logos"));
    }
    if let Some(grok_home) = std::env::var_os("GROK_HOME").map(PathBuf::from) {
        roots.push(grok_home.join("logos"));
    }

    roots
}

#[cfg(test)]
mod tests {
    use super::push_stem_variants;

    #[test]
    fn node_stems_cover_catalog_and_logo_naming() {
        let mut stems = Vec::new();
        push_stem_variants(&mut stems, "ActionNetwork");
        assert!(stems.contains(&"actionnetwork".to_string()));

        let mut stems = Vec::new();
        push_stem_variants(&mut stems, "NextCloud Talk");
        assert!(stems.contains(&"nextcloud-talk".to_string()));
    }
}
