//! Dynamic GGUF/GGML discovery and download support for the model picker.
//!
//! The picker never ships a fixed list of local model names. Local entries are
//! discovered by the shell from the DX cache; this module supplies an optional
//! Hugging Face search cache and performs downloads atomically into that same
//! cache. Network failures are soft: the normal model catalog remains usable.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use futures_util::StreamExt;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use crate::slash::command::ArgItem;

const HF_API_URL: &str = "https://huggingface.co/api/models";
const DEFAULT_MAX_DOWNLOAD_BYTES: u64 = 20 * 1024 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct DownloadableModel {
    pub key: String,
    pub display_name: String,
    pub url: String,
}

#[derive(Default)]
struct CatalogState {
    models: HashMap<String, DownloadableModel>,
    in_flight: HashSet<String>,
}

fn state() -> &'static RwLock<CatalogState> {
    static STATE: OnceLock<RwLock<CatalogState>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(CatalogState::default()))
}

/// Start a best-effort search after the user types a meaningful query.
/// Results appear on the next prompt refresh; the current picker remains
/// responsive while Hugging Face is queried in the background.
pub(crate) fn request_search(query: &str) {
    let query = query.trim().to_owned();
    if query.chars().count() < 2 {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    {
        let Ok(mut current) = state().write() else {
            return;
        };
        if !current.in_flight.insert(query.to_ascii_lowercase()) {
            return;
        }
    }
    handle.spawn(async move {
        let result = fetch_search_results(&query).await;
        if let Ok(mut current) = state().write() {
            current.in_flight.remove(&query.to_ascii_lowercase());
            if let Ok(models) = result {
                for model in models {
                    current.models.insert(model.key.clone(), model);
                }
            }
        }
    });
}

/// Return cached remote rows matching the current picker query.
pub(crate) fn suggestions(query: &str) -> Vec<ArgItem> {
    let query = query.trim().to_ascii_lowercase();
    let Ok(current) = state().read() else {
        return Vec::new();
    };
    current
        .models
        .values()
        .filter(|model| {
            query.is_empty()
                || model.key.to_ascii_lowercase().contains(&query)
                || model.display_name.to_ascii_lowercase().contains(&query)
        })
        .map(|model| ArgItem {
            display: format!("{} (Hugging Face)", model.display_name),
            match_text: format!("{} {}", model.display_name, model.key),
            insert_text: model.key.clone(),
            description: "Download GGUF/GGML into the DX model cache".to_string(),
        })
        .collect()
}

pub(crate) fn resolve(key: &str) -> Option<DownloadableModel> {
    state().read().ok()?.models.get(key).cloned()
}

pub(crate) fn cache_dir() -> Result<PathBuf, String> {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .or_else(dirs::home_dir)
        .map(|root| root.join("dx").join("flow").join("models").join("llm"))
        .ok_or_else(|| "could not determine the DX local-data directory".to_string())
}

/// Download one model file to the DX cache and return its final path.
/// Existing files are reused. A `.part` file is removed on failure and the
/// final rename is atomic on the same filesystem.
pub(crate) async fn download(model: &DownloadableModel) -> Result<PathBuf, String> {
    let destination_dir = cache_dir()?;
    tokio::fs::create_dir_all(&destination_dir)
        .await
        .map_err(|error| format!("create model cache: {error}"))?;
    let file_name = safe_file_name(&model.url)?;
    let destination = destination_dir.join(&file_name);
    if tokio::fs::try_exists(&destination)
        .await
        .map_err(|error| format!("check cached model: {error}"))?
    {
        return Ok(destination);
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(60 * 60))
        .user_agent("dx-tui model downloader")
        .build()
        .map_err(|error| format!("build model downloader: {error}"))?;
    let response = client
        .get(&model.url)
        .send()
        .await
        .map_err(|error| format!("download model: {error}"))?
        .error_for_status()
        .map_err(|error| format!("download model: {error}"))?;
    let max_bytes = std::env::var("DX_MAX_MODEL_DOWNLOAD_BYTES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_MAX_DOWNLOAD_BYTES);
    if response
        .content_length()
        .is_some_and(|size| size > max_bytes)
    {
        return Err(format!(
            "model exceeds the {} GiB download limit",
            max_bytes / 1024 / 1024 / 1024
        ));
    }

    let part = destination.with_extension(format!("{}.part", file_name_extension(&file_name)));
    let mut file = tokio::fs::File::create(&part)
        .await
        .map_err(|error| format!("create temporary model file: {error}"))?;
    let mut received = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("read model download: {error}"))?;
        received = received.saturating_add(chunk.len() as u64);
        if received > max_bytes {
            let _ = tokio::fs::remove_file(&part).await;
            return Err("model download exceeded the configured size limit".to_string());
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("write model cache: {error}"))?;
    }
    file.flush()
        .await
        .map_err(|error| format!("flush model cache: {error}"))?;
    drop(file);
    if let Err(error) = tokio::fs::rename(&part, &destination).await {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(format!("finalize model download: {error}"));
    }
    Ok(destination)
}

async fn fetch_search_results(query: &str) -> Result<Vec<DownloadableModel>, String> {
    #[derive(Deserialize)]
    struct Repository {
        id: String,
        #[serde(default)]
        siblings: Vec<Sibling>,
    }
    #[derive(Deserialize)]
    struct Sibling {
        rfilename: String,
    }

    let url = reqwest::Url::parse_with_params(
        HF_API_URL,
        &[("search", query), ("filter", "gguf"), ("limit", "20")],
    )
    .map_err(|error| format!("build model search URL: {error}"))?;
    let response = reqwest::Client::new()
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|error| format!("search GGUF models: {error}"))?
        .error_for_status()
        .map_err(|error| format!("search GGUF models: {error}"))?;
    let repositories = response
        .json::<Vec<Repository>>()
        .await
        .map_err(|error| format!("parse model search: {error}"))?;

    let mut result = Vec::new();
    for repository in repositories {
        let files = repository
            .siblings
            .iter()
            .map(|sibling| sibling.rfilename.clone())
            .collect::<Vec<_>>();
        let Some(file) = choose_model_file(&files) else {
            continue;
        };
        let encoded_repo = repository
            .id
            .split('/')
            .map(|segment| urlencoding::encode(segment).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let encoded_file = file
            .split('/')
            .map(|segment| urlencoding::encode(segment).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let key = format!("hf:{}:{}", repository.id, file);
        result.push(DownloadableModel {
            display_name: format!("{} · {}", repository.id, file),
            url: format!("https://huggingface.co/{encoded_repo}/resolve/main/{encoded_file}"),
            key,
        });
    }
    Ok(result)
}

fn choose_model_file(files: &[String]) -> Option<String> {
    let mut candidates = files
        .iter()
        .filter(|file| {
            let lower = file.to_ascii_lowercase();
            (lower.ends_with(".gguf") || lower.ends_with(".ggml")) && !lower.contains("mmproj")
        })
        .map(Clone::clone)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|file| {
        let upper = file.to_ascii_uppercase();
        (
            !upper.contains("Q4_K_M"),
            !upper.contains("Q4_K_S"),
            !upper.contains("Q5_K_M"),
            file.clone(),
        )
    });
    candidates.into_iter().next()
}

fn safe_file_name(url: &str) -> Result<String, String> {
    let path = reqwest::Url::parse(url)
        .map_err(|error| format!("invalid model URL: {error}"))?
        .path()
        .to_string();
    let name = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".gguf") || lower.ends_with(".ggml")
        })
        .ok_or_else(|| "model URL must end in .gguf or .ggml".to_string())?;
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err("unsafe model filename".to_string());
    }
    Ok(name.to_string())
}

fn file_name_extension(file_name: &str) -> &str {
    Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("model")
}
