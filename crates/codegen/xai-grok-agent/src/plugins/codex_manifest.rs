//! Compatibility loader for the OpenAI Agent Plugin manifest format.
//!
//! This is intentionally a small adapter at DX's existing plugin boundary.
//! The Codex runtime's process/MCP implementation is not copied here; DX's
//! trusted plugin registry and `xai-grok-mcp` runtime remain authoritative.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::manifest::{Author, PathOrInline, PathOrPaths, PluginManifest};

const MANIFEST_RELATIVE_PATH: &str = ".codex-plugin/plugin.json";
const CODEX_EXTENSION_NAMESPACE: &str = "com.openai";

/// Load a Codex Agent Plugin manifest when one exists at `plugin_root`.
///
/// Codex manifests use `skills/` and `mcp.json` by convention. DX also reads
/// the optional `extensions.com.openai` object so MCP/hooks declared there are
/// available to the existing trusted runtime.
pub(super) fn load(plugin_root: &Path) -> Result<Option<PluginManifest>, String> {
    let path = plugin_root.join(MANIFEST_RELATIVE_PATH);
    if !path.is_file() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&contents)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    let object = value
        .as_object()
        .ok_or_else(|| "Agent Plugin manifest must be a JSON object".to_string())?;

    let name = object
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Agent Plugin manifest requires a string `name`".to_string())?
        .trim();
    if name.is_empty() {
        return Err("Agent Plugin manifest `name` must not be empty".to_string());
    }

    let extension = object
        .get("extensions")
        .and_then(Value::as_object)
        .and_then(|extensions| extensions.get(CODEX_EXTENSION_NAMESPACE))
        .filter(|value| value.is_object());

    let extension = extension.and_then(Value::as_object);
    let mcp_value = extension.and_then(|value| value.get("mcpServers"));
    let hooks_value = extension.and_then(|value| value.get("hooks"));

    let mcp_servers = match mcp_value {
        Some(Value::String(relative)) => Some(PathOrInline::Path(relative.clone())),
        Some(value) if value.is_object() => Some(PathOrInline::Inline(value.clone())),
        Some(_) => return Err("Agent Plugin `mcpServers` must be a path or object".to_string()),
        None => {
            let conventional = plugin_root.join("mcp.json");
            conventional.is_file().then_some(PathOrInline::Path(
                conventional
                    .strip_prefix(plugin_root)
                    .unwrap_or(Path::new("mcp.json"))
                    .to_string_lossy()
                    .into_owned(),
            ))
        }
    };

    let hooks = match hooks_value {
        Some(Value::String(relative)) => Some(PathOrInline::Path(relative.clone())),
        Some(value) if value.is_object() || value.is_array() => {
            Some(PathOrInline::Inline(value.clone()))
        }
        Some(_) => return Err("Agent Plugin `hooks` must be a path or JSON value".to_string()),
        None => None,
    };

    let author = object.get("author").and_then(|value| {
        value.as_object().map(|author| Author {
            name: author
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            email: author
                .get("email")
                .and_then(Value::as_str)
                .map(str::to_owned),
            url: author.get("url").and_then(Value::as_str).map(str::to_owned),
        })
    });

    let skills = plugin_root
        .join("skills")
        .is_dir()
        .then_some(PathOrPaths::Single("skills".to_string()));

    let manifest = PluginManifest {
        name: name.to_string(),
        version: object
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned),
        description: object
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
        author,
        homepage: object
            .get("homepage")
            .and_then(Value::as_str)
            .map(str::to_owned),
        repository: object
            .get("repository")
            .and_then(Value::as_str)
            .map(str::to_owned),
        license: object
            .get("license")
            .and_then(Value::as_str)
            .map(str::to_owned),
        keywords: object
            .get("keywords")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        skills,
        commands: None,
        agents: None,
        hooks,
        mcp_servers,
        lsp_servers: None,
    };

    manifest
        .validate()
        .map_err(|error| format!("invalid Agent Plugin manifest: {error}"))?;
    Ok(Some(manifest))
}

pub(super) fn manifest_path(plugin_root: &Path) -> PathBuf {
    plugin_root.join(MANIFEST_RELATIVE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_codex_manifest_and_com_openai_mcp_servers() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".codex-plugin")).unwrap();
        std::fs::create_dir_all(root.path().join("skills/example")).unwrap();
        std::fs::write(
            manifest_path(root.path()),
            r#"{
                "$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
                "name":"example.tools",
                "version":"1.0.0",
                "extensions":{"com.openai":{"mcpServers":{"demo":{"type":"stdio","command":"node","args":["server.js"]}}}}
            }"#,
        )
        .unwrap();

        let manifest = load(root.path()).unwrap().unwrap();
        assert_eq!(manifest.name, "example.tools");
        assert!(
            manifest
                .skill_dirs(root.path())
                .iter()
                .any(|path| path.ends_with("skills"))
        );
        assert!(manifest.inline_mcp_servers().is_some());
    }
}
