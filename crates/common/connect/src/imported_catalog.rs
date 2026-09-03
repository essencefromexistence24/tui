//! Imported source inventories for the Connect surface.
//!
//! These entries are generated from the checked-out Flow-Like and n8n source
//! trees. They are metadata, not an assertion that foreign runtimes can be
//! executed inside the Rust TUI. Each entry keeps an explicit backend status.

use crate::{NodeBackend, NodeDefinition, NodeSource};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

// Static fallback inventories generated from the real n8n + flow-like trees.
// See `static_nodes.rs` for provenance and the regen script.
include!("static_nodes.rs");

pub fn external_catalog() -> Vec<NodeDefinition> {
    // Installed DX builds materialize the real node implementations directly
    // under LOCALAPPDATA/dx/connects. Prefer that inventory over the checked-
    // out/static fallbacks so the TUI and executor see the same nodes users
    // can actually run.
    let local = discover_dx_local_nodes();
    if !local.is_empty() {
        return local;
    }

    let mut nodes = Vec::new();

    // Prefer the checked-out/generated inventories when they are available.
    // The static list remains a deterministic fallback for installed builds.
    let n8n = discover_n8n_nodes();
    if n8n.is_empty() {
        add_static_n8n(&mut nodes);
    } else {
        nodes.extend(n8n);
    }

    let flow_like = discover_flow_like_nodes();
    if flow_like.is_empty() {
        add_static_flow_like(&mut nodes);
    } else {
        nodes.extend(flow_like);
    }

    nodes.push(NodeDefinition {
        id: "flow-like.control.branch".into(),
        display_name: "Branch".into(),
        source: NodeSource::FlowLike,
        backend: NodeBackend::Native,
        description: "Native branch execution".into(),
        inputs: 1,
        outputs: 2,
    });
    nodes
}

/// Small deterministic subset used by interactive surfaces that must not
/// scan the checked-out Flow-Like tree or parse the full n8n inventories.
pub fn external_catalog_limited(limit: usize) -> Vec<NodeDefinition> {
    let local = discover_dx_local_nodes_limited(limit);
    if !local.is_empty() {
        return local;
    }

    let mut nodes = Vec::new();
    add_static_n8n(&mut nodes);
    add_static_flow_like(&mut nodes);
    nodes.push(NodeDefinition {
        id: "flow-like.control.branch".into(),
        display_name: "Branch".into(),
        source: NodeSource::FlowLike,
        backend: NodeBackend::Native,
        description: "Native branch execution".into(),
        inputs: 1,
        outputs: 2,
    });
    nodes.into_iter().take(limit).collect()
}

/// Discover the materialized node folders used by installed DX builds.
///
/// Each node is accepted only when its implementation directory and metadata
/// file are present and the metadata parses as the public node definition. A
/// malformed or incomplete folder is skipped, so one damaged node cannot make
/// the Extensions modal fail to open.
fn discover_dx_local_nodes() -> Vec<NodeDefinition> {
    discover_dx_local_nodes_with_limit(None)
}

/// Bounded local discovery for interactive surfaces. Directory enumeration is
/// cheap, but parsing thousands of `node.json` files is not; sort the folder
/// paths first and parse only the requested page.
fn discover_dx_local_nodes_limited(limit: usize) -> Vec<NodeDefinition> {
    discover_dx_local_nodes_with_limit(Some(limit))
}

fn discover_dx_local_nodes_with_limit(limit: Option<usize>) -> Vec<NodeDefinition> {
    let Some(root) = connects_root() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path.join("implementation").is_dir()
                && path.join("node.json").is_file()
        })
        .collect();
    paths.sort();
    if let Some(limit) = limit {
        paths.truncate(limit);
    }

    let mut nodes = Vec::with_capacity(paths.len());
    for path in paths {
        let Ok(text) = fs::read_to_string(path.join("node.json")) else {
            continue;
        };
        let Ok(node) = serde_json::from_str::<NodeDefinition>(&text) else {
            continue;
        };
        if !node.id.is_empty() && !node.display_name.is_empty() {
            nodes.push(node);
        }
    }
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes
}

fn connects_root() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DX_CONNECTS_ROOT").map(PathBuf::from)
        && path.is_dir()
    {
        return Some(path);
    }
    dx_local_data_dir().map(|path| path.join("dx").join("connects"))
}

fn push_static_n8n(nodes: &mut Vec<NodeDefinition>, package: &str, class_name: &str) {
    let operation = class_name.to_ascii_lowercase();
    let native = package == "n8n-nodes-base"
        && matches!(operation.as_str(), "set" | "if" | "merge" | "noop");
    nodes.push(NodeDefinition {
        id: format!("{package}.{class_name}"),
        display_name: class_name.to_string(),
        source: NodeSource::N8n,
        backend: if native {
            NodeBackend::Native
        } else {
            NodeBackend::N8nAdapter
        },
        description: if native {
            format!("Native execution for {class_name}")
        } else {
            format!("Workflow execution for {class_name}; uses the isolated node runtime")
        },
        inputs: 1,
        outputs: if matches!(operation.as_str(), "if" | "switch") {
            2
        } else {
            1
        },
    });
}

fn add_static_n8n(nodes: &mut Vec<NodeDefinition>) {
    for name in N8N_BASE_NODES {
        push_static_n8n(nodes, "n8n-nodes-base", name);
    }
    for name in N8N_LANGCHAIN_NODES {
        push_static_n8n(nodes, "@n8n/nodes-langchain", name);
    }
}

fn add_static_flow_like(nodes: &mut Vec<NodeDefinition>) {
    for (family, name) in FLOW_LIKE_NODES {
        nodes.push(NodeDefinition {
            id: format!("flow-like.{family}.{}", name.to_ascii_lowercase()),
            display_name: (*name).to_string(),
            source: NodeSource::FlowLike,
            backend: NodeBackend::FlowLikeAdapter,
            description: format!("Workflow {family} node; uses the isolated node runtime"),
            inputs: 1,
            outputs: 1,
        });
    }
}

fn discover_n8n_nodes() -> Vec<NodeDefinition> {
    let Some(root) = runtime_root("DX_N8N_ROOT", "hexxed/n8n") else {
        return Vec::new();
    };
    let packages = [
        (
            "n8n-nodes-base",
            root.join("packages/nodes-base/dist/known/nodes.json"),
        ),
        (
            "@n8n/n8n-nodes-langchain",
            root.join("packages/@n8n/nodes-langchain/dist/known/nodes.json"),
        ),
    ];
    let mut nodes = Vec::new();
    for (package, path) in packages {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(entries) = value.as_object() else {
            continue;
        };
        for (key, metadata) in entries {
            let class_name = metadata
                .get("className")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(key);
            let operation = class_name.to_ascii_lowercase();
            let native = package == "n8n-nodes-base"
                && matches!(operation.as_str(), "set" | "if" | "merge" | "noop");
            nodes.push(NodeDefinition {
                id: format!("{package}.{class_name}"),
                display_name: class_name.to_string(),
                source: NodeSource::N8n,
                backend: if native {
                    NodeBackend::Native
                } else {
                    NodeBackend::N8nAdapter
                },
                description: if native {
                    format!("Native execution for {class_name}")
                } else {
                    format!("Workflow execution for {class_name}; uses the isolated node runtime")
                },
                inputs: 1,
                outputs: if matches!(operation.as_str(), "if" | "switch") {
                    2
                } else {
                    1
                },
            });
        }
    }
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes
}

fn discover_flow_like_nodes() -> Vec<NodeDefinition> {
    let Some(root) = runtime_root("DX_FLOW_LIKE_ROOT", "flow-like") else {
        return Vec::new();
    };
    let catalog_root = root.join("packages/catalog");
    let Ok(families) = fs::read_dir(catalog_root) else {
        return Vec::new();
    };
    let mut nodes = Vec::new();
    let mut seen = HashSet::new();
    for family in families.flatten().filter(|entry| entry.path().is_dir()) {
        let family_name = family.file_name().to_string_lossy().into_owned();
        if matches!(family_name.as_str(), "benches" | "tests" | "src") {
            continue;
        }
        collect_flow_logic(&family.path(), &family_name, &mut nodes, &mut seen);
    }
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    nodes
}

fn collect_flow_logic(
    path: &Path,
    family: &str,
    nodes: &mut Vec<NodeDefinition>,
    seen: &mut HashSet<String>,
) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            collect_flow_logic(&path, family, nodes, seen);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for marker in text.match_indices("impl NodeLogic for ") {
            let start = marker.0 + marker.1.len();
            let name: String = text[start..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            let id = format!("flow-like.{family}.{}", name.to_ascii_lowercase());
            if !seen.insert(id.clone()) {
                continue;
            }
            nodes.push(NodeDefinition {
                id,
                display_name: name.to_string(),
                source: NodeSource::FlowLike,
                backend: NodeBackend::FlowLikeAdapter,
                description: format!("Workflow {family} node; uses the isolated node runtime"),
                inputs: 1,
                outputs: 1,
            });
        }
    }
}

fn runtime_root(env_name: &str, sibling: &str) -> Option<PathBuf> {
    // Installed DX assets live under the platform's local application-data
    // directory. Prefer that copy so the TUI does not depend on the checkout
    // remaining at `G:\Dx` (or an equivalent developer path).
    if let Some(local_data) = dx_local_data_dir() {
        let local_name = match sibling {
            "flow-like" => "core",
            "n8n" => "integrations",
            other => other,
        };
        let local = local_data
            .join("dx")
            .join("connects")
            .join(".runtime")
            .join(local_name);
        if local.is_dir() {
            return Some(local);
        }
    }
    if let Some(path) = std::env::var_os(env_name).map(PathBuf::from) {
        if path.is_dir() {
            return Some(path);
        }
    }
    let cwd = std::env::current_dir().ok()?;
    let candidate = cwd.join("..").join(sibling);
    candidate
        .is_dir()
        .then(|| candidate.canonicalize().unwrap_or(candidate))
}

fn dx_local_data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        Some(PathBuf::from(path))
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
    }
}
