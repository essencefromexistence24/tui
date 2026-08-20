//! DX native video-player discovery and launch support.
//!
//! Playback intentionally happens in a separate native window. This module
//! never touches the pager terminal and never invokes a shell.

use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;

const PLAYER_ENV: &str = "DX_VIDEO_PLAYER";
const RUNTIME_MANIFEST: &str = "runtime-manifest.txt";
const SHOWCASE_PLAYLIST: &str = "dx-showcase.m3u8";
const SHOWCASE_CACHE_RESERVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const SHOWCASE_CACHE_MIN_BYTES: u64 = 512 * 1024 * 1024;
const SHOWCASE_CACHE_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const SHOWCASE_DOWNLOAD_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShowcaseVideo {
    pub selector: &'static str,
    pub title: &'static str,
    pub url: &'static str,
    pub filename: &'static str,
}

const SHOWCASE_VIDEOS: &[ShowcaseVideo] = &[
    ShowcaseVideo {
        selector: "spiderman",
        title: "Spiderman Into The SpiderVerse",
        url: "https://files.catbox.moe/wfyf2z.mp4",
        filename: "spiderman.mp4",
    },
    ShowcaseVideo {
        selector: "one-piece",
        title: "One Piece",
        url: "https://files.catbox.moe/ff8oz1.mp4",
        filename: "one-piece.mp4",
    },
    ShowcaseVideo {
        selector: "frieren",
        title: "Frieren Beyond Journey's End",
        url: "https://files.catbox.moe/6rtwwl.mp4",
        filename: "frieren.mp4",
    },
];

#[cfg(windows)]
const WINDOWS_RUNTIME_FILES: &[&str] = &[
    "avcodec-62.dll",
    "avdevice-62.dll",
    "avfilter-11.dll",
    "avformat-62.dll",
    "avutil-60.dll",
    "libass-9.dll",
    "libplacebo-360.dll",
    "lua51.dll",
    "swresample-6.dll",
    "swscale-9.dll",
    "vulkan-1.dll",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerSource {
    Environment,
    Installed,
    Development,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedPlayer {
    executable: PathBuf,
    source: PlayerSource,
}

#[derive(Debug)]
pub enum VideoPlayerError {
    Usage,
    UnsupportedPlatform(String),
    PlayerNotInstalled(PathBuf),
    PlayerNotFound(PathBuf),
    MissingRuntimeFiles {
        directory: PathBuf,
        files: Vec<String>,
    },
    RuntimeManifestUnreadable {
        path: PathBuf,
        error: std::io::Error,
    },
    NoGraphicalSession(String),
    MediaNotFound(PathBuf),
    MediaIsDirectory(PathBuf),
    MediaUnreadable {
        path: PathBuf,
        error: std::io::Error,
    },
    Spawn {
        executable: PathBuf,
        error: std::io::Error,
    },
    PlayerExited {
        executable: PathBuf,
        status: std::process::ExitStatus,
    },
    WindowDidNotOpen(PathBuf),
    PlaylistWrite {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl fmt::Display for VideoPlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => write!(f, "Usage: /video <path|showcase>"),
            Self::UnsupportedPlatform(platform) => {
                write!(f, "Video Player is not available for {platform}")
            }
            Self::PlayerNotInstalled(path) => write!(
                f,
                "Video Player is not installed at {}. Run the video-player installation/update command.",
                path.display()
            ),
            Self::PlayerNotFound(path) => {
                write!(
                    f,
                    "DX_VIDEO_PLAYER does not point to a file: {}",
                    path.display()
                )
            }
            Self::MissingRuntimeFiles { directory, files } => write!(
                f,
                "Video Player installation is incomplete in {}. Missing: {}",
                directory.display(),
                files.join(", ")
            ),
            Self::RuntimeManifestUnreadable { path, error } => write!(
                f,
                "Video Player runtime manifest is unreadable: {} ({error})",
                path.display()
            ),
            Self::NoGraphicalSession(platform) => write!(
                f,
                "Video Player cannot open a native window: no graphical session is available on {platform}"
            ),
            Self::MediaNotFound(path) => write!(f, "Video file not found: {}", path.display()),
            Self::MediaIsDirectory(path) => {
                write!(f, "Video path is a directory: {}", path.display())
            }
            Self::MediaUnreadable { path, error } => {
                write!(
                    f,
                    "Video file is not readable: {} ({error})",
                    path.display()
                )
            }
            Self::Spawn { executable, error } => write!(
                f,
                "Failed to launch Video Player {}: {error}",
                executable.display()
            ),
            Self::PlayerExited { executable, status } => write!(
                f,
                "Video Player exited before opening a window: {} ({status})",
                executable.display()
            ),
            Self::WindowDidNotOpen(executable) => write!(
                f,
                "Video Player started but did not open a window: {}. Reinstall the complete player runtime.",
                executable.display()
            ),
            Self::PlaylistWrite { path, error } => {
                write!(
                    f,
                    "Failed to write video playlist {}: {error}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for VideoPlayerError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoLaunch {
    pub media: PathBuf,
    pub executable: PathBuf,
}

/// Launch one local media path or online URL in the detached native player.
///
/// The native player must outlive the TUI (detached on unix, fire-and-forget
/// on Windows), so it is intentionally NOT enrolled in the TUI process scope.
#[allow(clippy::disallowed_methods)]
pub fn launch(raw_path: &str, workspace: &Path) -> Result<VideoLaunch, VideoPlayerError> {
    validate_graphical_session()?;
    if raw_path.trim().eq_ignore_ascii_case("dx-showcase") {
        return launch_showcase();
    }
    if let Some(selector) = raw_path
        .trim()
        .strip_prefix("dx-showcase:")
        .filter(|selector| !selector.trim().is_empty())
    {
        return launch_showcase_video(selector.trim());
    }
    let media = resolve_media_source(raw_path, workspace)?;
    let player = resolve_player()?;

    let mut command = Command::new(&player.executable);
    command
        .arg(&media)
        .current_dir(player.executable.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // Keep native-player/yt-dlp diagnostics visible. Online services can
        // reject a request after the player starts, and swallowing stderr made
        // that look like a no-op to the user.
        .stderr(Stdio::inherit());
    #[cfg(not(windows))]
    xai_grok_tools::util::detach_std_command(&mut command);
    let mut child = command.spawn().map_err(|error| VideoPlayerError::Spawn {
        executable: player.executable.clone(),
        error,
    })?;
    verify_player_started(&mut child, &player.executable)?;

    Ok(VideoLaunch {
        media,
        executable: player.executable,
    })
}

/// Native player outlives the TUI; see [`launch`].
#[allow(clippy::disallowed_methods)]
fn launch_showcase() -> Result<VideoLaunch, VideoPlayerError> {
    validate_graphical_session()?;
    let directory = showcase_assets_dir().map_err(|error| VideoPlayerError::PlaylistWrite {
        path: PathBuf::from("Downloads/Dx/Showcase Videos"),
        error: std::io::Error::other(error),
    })?;
    std::fs::create_dir_all(&directory).map_err(|error| VideoPlayerError::PlaylistWrite {
        path: directory.clone(),
        error,
    })?;
    let cache_directory = directory.join("cache");
    std::fs::create_dir_all(&cache_directory).map_err(|error| VideoPlayerError::PlaylistWrite {
        path: cache_directory.clone(),
        error,
    })?;

    let mut playlist_entries = Vec::with_capacity(SHOWCASE_VIDEOS.len());
    for video in SHOWCASE_VIDEOS {
        let cached = directory.join(video.filename);
        let source = if cached.is_file() {
            cached.to_string_lossy().into_owned()
        } else {
            video.url.to_owned()
        };
        playlist_entries.push((video.title, source));
    }
    let playlist = directory.join(SHOWCASE_PLAYLIST);
    let mut contents = String::from("#EXTM3U\n");
    for (title, source) in &playlist_entries {
        contents.push_str(&format!("#EXTINF:-1,{title} (Showcase)\n{source}\n"));
    }
    std::fs::write(&playlist, contents).map_err(|error| VideoPlayerError::PlaylistWrite {
        path: playlist.clone(),
        error,
    })?;
    let player = resolve_player()?;
    let cache_max_bytes = showcase_cache_max_bytes();
    let mut command = Command::new(&player.executable);
    command
        // dx-video-player follows mpv's `--option=value` form for playlist
        // options. Passing the path as a separate argv item makes the player
        // treat it as the positional media argument and exit before opening.
        .arg(format!("--playlist={}", playlist.display()))
        // Open the native window before Catbox responds. Without this, the
        // launch verification can mistake slow network startup for a failed
        // player and terminate the process before the first frame arrives.
        .arg("--force-window=immediate")
        .arg("--loop-playlist=inf")
        .arg("--cache=yes")
        .arg("--cache-on-disk=yes")
        .arg(format!("--demuxer-cache-dir={}", cache_directory.display()))
        .arg("--cache-pause=yes")
        .arg("--cache-pause-initial=yes")
        .arg("--cache-pause-wait=3")
        .arg("--demuxer-cache-unlink-files=whendone")
        .arg("--cache-secs=60")
        .arg("--demuxer-readahead-secs=60")
        .arg(format!(
            "--demuxer-max-bytes={}MiB",
            cache_max_bytes / (1024 * 1024)
        ))
        .arg("--stream-buffer-size=32MiB")
        .current_dir(player.executable.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(not(windows))]
    xai_grok_tools::util::detach_std_command(&mut command);
    let mut child = command.spawn().map_err(|error| VideoPlayerError::Spawn {
        executable: player.executable.clone(),
        error,
    })?;
    verify_player_started(&mut child, &player.executable)?;
    Ok(VideoLaunch {
        media: playlist,
        executable: player.executable,
    })
}

/// Native player outlives the TUI; see [`launch`].
#[allow(clippy::disallowed_methods)]
fn launch_showcase_video(selector: &str) -> Result<VideoLaunch, VideoPlayerError> {
    let video = showcase_video(selector).ok_or(VideoPlayerError::Usage)?;
    let assets = showcase_assets_dir().map_err(|error| VideoPlayerError::PlaylistWrite {
        path: PathBuf::from("dx/assets/video"),
        error: std::io::Error::other(error),
    })?;
    std::fs::create_dir_all(&assets).map_err(|error| VideoPlayerError::PlaylistWrite {
        path: assets.clone(),
        error,
    })?;
    let cached = assets.join(video.filename);
    let media = if cached.is_file() {
        cached
    } else {
        PathBuf::from(video.url)
    };
    let cache_directory = assets.join("cache");
    std::fs::create_dir_all(&cache_directory).map_err(|error| VideoPlayerError::PlaylistWrite {
        path: cache_directory.clone(),
        error,
    })?;
    let player = resolve_player()?;
    let cache_max_bytes = showcase_cache_max_bytes();
    let mut command = Command::new(&player.executable);
    command
        .arg("--force-window=immediate")
        .arg("--cache=yes")
        .arg("--cache-on-disk=yes")
        .arg(format!("--demuxer-cache-dir={}", cache_directory.display()))
        .arg("--cache-pause=yes")
        .arg("--cache-pause-initial=yes")
        .arg("--cache-pause-wait=3")
        .arg("--demuxer-cache-unlink-files=whendone")
        .arg("--cache-secs=60")
        .arg("--demuxer-readahead-secs=60")
        .arg(format!(
            "--demuxer-max-bytes={}MiB",
            cache_max_bytes / (1024 * 1024)
        ))
        .arg("--stream-buffer-size=32MiB")
        .arg(&media)
        .current_dir(player.executable.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    #[cfg(not(windows))]
    xai_grok_tools::util::detach_std_command(&mut command);
    let mut child = command.spawn().map_err(|error| VideoPlayerError::Spawn {
        executable: player.executable.clone(),
        error,
    })?;
    verify_player_started(&mut child, &player.executable)?;
    Ok(VideoLaunch {
        media,
        executable: player.executable,
    })
}

pub(crate) fn showcase_video(selector: &str) -> Option<ShowcaseVideo> {
    SHOWCASE_VIDEOS
        .iter()
        .copied()
        .find(|video| video.selector.eq_ignore_ascii_case(selector.trim()))
}

pub(crate) fn showcase_assets_dir() -> Result<PathBuf, String> {
    dirs::download_dir()
        .map(|base| base.join("Dx").join("Showcase Videos"))
        .ok_or_else(|| "the OS Downloads directory is unavailable".to_owned())
}

/// Download a built-in showcase video into the user's OS Downloads directory.
///
/// The final file is never exposed until the complete response has been
/// flushed and atomically renamed from its `.part` sibling. This prevents an
/// interrupted download from being mistaken for a playable local asset.
pub(crate) async fn download_showcase_video(
    selector: &str,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, String> {
    let video = showcase_video(selector).ok_or_else(|| "unknown DX showcase video".to_owned())?;
    let directory = showcase_assets_dir()?;
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| format!("create video cache {}: {error}", directory.display()))?;
    let final_path = directory.join(video.filename);
    if let Ok(metadata) = tokio::fs::metadata(&final_path).await
        && metadata.is_file()
        && metadata.len() > 0
    {
        progress(metadata.len(), Some(metadata.len()));
        return Ok(final_path);
    }

    let partial_path = final_path.with_extension("mp4.part");
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|error| format!("create video download client: {error}"))?;
    let response = client
        .get(video.url)
        .send()
        .await
        .map_err(|error| format!("request {}: {error}", video.title))?
        .error_for_status()
        .map_err(|error| format!("download {}: {error}", video.title))?;
    let total = response.content_length();
    if total.is_some_and(|size| size > SHOWCASE_DOWNLOAD_MAX_BYTES) {
        return Err(format!("{} exceeds the 4 GiB download limit", video.title));
    }
    let mut file = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|error| format!("create partial video cache: {error}"))?;
    let mut downloaded = 0u64;
    progress(0, total);
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("read {}: {error}", video.title))?
    {
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > SHOWCASE_DOWNLOAD_MAX_BYTES {
            drop(file);
            let _ = tokio::fs::remove_file(&partial_path).await;
            return Err(format!("{} exceeds the 4 GiB download limit", video.title));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("write video cache: {error}"))?;
        progress(downloaded, total);
    }
    file.flush()
        .await
        .map_err(|error| format!("flush video cache: {error}"))?;
    file.sync_all()
        .await
        .map_err(|error| format!("sync video cache: {error}"))?;
    drop(file);
    if let Err(error) = tokio::fs::rename(&partial_path, &final_path).await {
        if tokio::fs::metadata(&final_path)
            .await
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        {
            let _ = tokio::fs::remove_file(&partial_path).await;
        } else {
            return Err(format!("finalize video cache: {error}"));
        }
    }
    progress(downloaded, total.or(Some(downloaded)));
    Ok(final_path)
}

fn showcase_cache_max_bytes() -> u64 {
    let fallback = SHOWCASE_CACHE_MIN_BYTES;
    let Some(free_bytes) = free_space_bytes(system_volume_path()) else {
        return fallback;
    };
    free_bytes
        .saturating_sub(SHOWCASE_CACHE_RESERVE_BYTES)
        .clamp(SHOWCASE_CACHE_MIN_BYTES, SHOWCASE_CACHE_MAX_BYTES)
}

#[cfg(windows)]
fn system_volume_path() -> &'static Path {
    Path::new(r"C:\")
}

#[cfg(not(windows))]
fn system_volume_path() -> &'static Path {
    Path::new("/")
}

#[cfg(windows)]
fn free_space_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut available = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    let result =
        unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) };
    (result != 0).then_some(available)
}

#[cfg(unix)]
fn free_space_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;

    let bytes = path.as_os_str().as_bytes();
    let mut c_path = bytes.to_vec();
    c_path.push(0);
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(c_path.as_ptr().cast(), stats.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    let stats = unsafe { stats.assume_init() };
    Some(u64::from(stats.f_bavail).saturating_mul(u64::from(stats.f_frsize)))
}

#[cfg(not(any(windows, unix)))]
fn free_space_bytes(_path: &Path) -> Option<u64> {
    None
}

#[cfg(windows)]
fn verify_player_started(
    child: &mut std::process::Child,
    executable: &Path,
) -> Result<(), VideoPlayerError> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().map_err(|error| VideoPlayerError::Spawn {
            executable: executable.to_path_buf(),
            error,
        })? {
            return Err(VideoPlayerError::PlayerExited {
                executable: executable.to_path_buf(),
                status,
            });
        }
        if process_has_visible_window(child.id()) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err(VideoPlayerError::WindowDidNotOpen(executable.to_path_buf()));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(not(windows))]
fn verify_player_started(
    child: &mut std::process::Child,
    executable: &Path,
) -> Result<(), VideoPlayerError> {
    thread::sleep(Duration::from_millis(500));
    if let Some(status) = child.try_wait().map_err(|error| VideoPlayerError::Spawn {
        executable: executable.to_path_buf(),
        error,
    })? {
        return Err(VideoPlayerError::PlayerExited {
            executable: executable.to_path_buf(),
            status,
        });
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_graphical_session() -> Result<(), VideoPlayerError> {
    let has_display = ["WAYLAND_DISPLAY", "DISPLAY"]
        .into_iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()));
    if has_display {
        Ok(())
    } else {
        Err(VideoPlayerError::NoGraphicalSession("Linux".to_owned()))
    }
}

#[cfg(not(target_os = "linux"))]
fn validate_graphical_session() -> Result<(), VideoPlayerError> {
    Ok(())
}

#[cfg(windows)]
fn process_has_visible_window(process_id: u32) -> bool {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct Search {
        process_id: u32,
        found: bool,
    }

    unsafe extern "system" fn visit(window: HWND, state: LPARAM) -> BOOL {
        let search = unsafe { &mut *(state as *mut Search) };
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(window, &mut owner) };
        if owner == search.process_id && unsafe { IsWindowVisible(window) } != 0 {
            search.found = true;
            return 0;
        }
        1
    }

    let mut search = Search {
        process_id,
        found: false,
    };
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    search.found
}

fn resolve_media_source(raw_path: &str, workspace: &Path) -> Result<PathBuf, VideoPlayerError> {
    let path_text = strip_matching_quotes(raw_path)?;
    let supplied = local_path_from_user_text(path_text);
    let candidate = if supplied.is_absolute() {
        supplied
    } else {
        workspace.join(supplied)
    };

    if !candidate.exists() {
        return Err(VideoPlayerError::MediaNotFound(candidate));
    }
    if candidate.is_dir() {
        return Err(VideoPlayerError::MediaIsDirectory(candidate));
    }
    let canonical =
        dunce::canonicalize(&candidate).map_err(|error| VideoPlayerError::MediaUnreadable {
            path: candidate.clone(),
            error,
        })?;
    File::open(&canonical).map_err(|error| VideoPlayerError::MediaUnreadable {
        path: canonical.clone(),
        error,
    })?;
    Ok(canonical)
}

fn local_path_from_user_text(path_text: &str) -> PathBuf {
    #[cfg(windows)]
    if path_text.len() >= 2
        && path_text.as_bytes()[1] == b':'
        && !path_text
            .as_bytes()
            .get(2)
            .is_some_and(|byte| *byte == b'\\' || *byte == b'/')
    {
        return PathBuf::from(format!(
            "{}\\{}",
            &path_text[..2],
            path_text[2..].trim_start_matches(['\\', '/'])
        ));
    }
    PathBuf::from(path_text)
}

fn strip_matching_quotes(raw: &str) -> Result<&str, VideoPlayerError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(VideoPlayerError::Usage);
    }

    let first = trimmed.as_bytes()[0];
    let quoted = first == b'\'' || first == b'"';
    let last_matches = trimmed.as_bytes().last().copied() == Some(first);
    if quoted {
        if trimmed.len() < 2 || !last_matches {
            return Err(VideoPlayerError::Usage);
        }
        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.is_empty() {
            return Err(VideoPlayerError::Usage);
        }
        return Ok(inner);
    }
    if matches!(trimmed.as_bytes().last(), Some(b'\'' | b'"')) {
        return Err(VideoPlayerError::Usage);
    }
    Ok(trimmed)
}

fn resolve_player() -> Result<ResolvedPlayer, VideoPlayerError> {
    let explicit = std::env::var_os(PLAYER_ENV).filter(|value| !value.is_empty());
    let installed = installed_executable()?;
    let development = development_executable();
    resolve_player_from(
        explicit.as_deref().map(Path::new),
        &installed,
        development.as_deref(),
    )
}

fn resolve_player_from(
    explicit: Option<&Path>,
    installed: &Path,
    development: Option<&Path>,
) -> Result<ResolvedPlayer, VideoPlayerError> {
    if let Some(path) = explicit {
        if !path.is_file() {
            return Err(VideoPlayerError::PlayerNotFound(path.to_path_buf()));
        }
        return Ok(ResolvedPlayer {
            executable: path.to_path_buf(),
            source: PlayerSource::Environment,
        });
    }

    if installed.is_file() {
        validate_installed_runtime(installed)?;
        return Ok(ResolvedPlayer {
            executable: installed.to_path_buf(),
            source: PlayerSource::Installed,
        });
    }

    if let Some(path) = development.filter(|path| path.is_file()) {
        return Ok(ResolvedPlayer {
            executable: path.to_path_buf(),
            source: PlayerSource::Development,
        });
    }

    Err(VideoPlayerError::PlayerNotInstalled(
        installed.to_path_buf(),
    ))
}

fn installed_executable() -> Result<PathBuf, VideoPlayerError> {
    if !matches!(std::env::consts::OS, "windows" | "macos" | "linux") {
        return Err(VideoPlayerError::UnsupportedPlatform(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )));
    }
    if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
        return Err(VideoPlayerError::UnsupportedPlatform(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )));
    }
    let base = dirs::data_local_dir().ok_or_else(|| {
        VideoPlayerError::UnsupportedPlatform(format!(
            "{}-{} (no per-user data directory)",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;
    Ok(installed_directory(&base, std::env::consts::OS)
        .join(player_filename_for(std::env::consts::OS)))
}

fn installed_directory(base: &Path, os: &str) -> PathBuf {
    match os {
        "windows" => base.join("Programs").join("DX").join("Video"),
        "macos" => base.join("DX").join("Video"),
        "linux" => base.join("dx").join("video"),
        _ => base.join("dx").join("video"),
    }
}

fn player_filename_for(os: &str) -> &'static str {
    if os == "windows" {
        "dx-video-player.exe"
    } else {
        "dx-video-player"
    }
}

fn development_executable() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    #[cfg(windows)]
    {
        if std::env::consts::ARCH != "x86_64" {
            return None;
        }
        Some(PathBuf::from(
            r"G:\Dx\hexxed\terminal\dx-video-player\dx-video-player.exe",
        ))
    }
    #[cfg(not(windows))]
    None
}

fn validate_installed_runtime(executable: &Path) -> Result<(), VideoPlayerError> {
    let directory = executable.parent().unwrap_or_else(|| Path::new("."));
    let manifest = directory.join(RUNTIME_MANIFEST);
    let contents = std::fs::read_to_string(&manifest).map_err(|error| {
        VideoPlayerError::RuntimeManifestUnreadable {
            path: manifest.clone(),
            error,
        }
    })?;
    let mut required: Vec<String> = contents
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    #[cfg(windows)]
    required.extend(
        WINDOWS_RUNTIME_FILES
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>(),
    );
    required.sort();
    required.dedup();
    let missing: Vec<String> = required
        .into_iter()
        .filter(|name| !directory.join(name).is_file())
        .collect();
    if !missing.is_empty() {
        return Err(VideoPlayerError::MissingRuntimeFiles {
            directory: directory.to_path_buf(),
            files: missing,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_one_matching_quote_pair_and_preserves_spaces() {
        assert_eq!(
            strip_matching_quotes(r#""C:\Video Files\demo.mp4""#).unwrap(),
            r"C:\Video Files\demo.mp4"
        );
        assert_eq!(
            strip_matching_quotes(r"C:\Video Files\demo.mp4").unwrap(),
            r"C:\Video Files\demo.mp4"
        );
        assert!(matches!(
            strip_matching_quotes("\"broken"),
            Err(VideoPlayerError::Usage)
        ));
    }

    #[test]
    fn relative_media_resolves_against_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let media = temp.path().join("rendered clip.mp4");
        std::fs::write(&media, b"test").unwrap();
        let resolved = resolve_media_source("rendered clip.mp4", temp.path()).unwrap();
        assert_eq!(resolved, dunce::canonicalize(media).unwrap());
    }

    #[test]
    fn missing_media_and_directories_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        assert!(matches!(
            resolve_media_source("missing.mp4", temp.path()),
            Err(VideoPlayerError::MediaNotFound(_))
        ));
        assert!(matches!(
            resolve_media_source(".", temp.path()),
            Err(VideoPlayerError::MediaIsDirectory(_))
        ));
    }

    #[test]
    fn explicit_player_override_wins_over_installed_player() {
        let temp = tempfile::tempdir().unwrap();
        let explicit = temp.path().join("explicit-player");
        let installed = temp.path().join("installed-player");
        std::fs::write(&explicit, b"explicit").unwrap();
        std::fs::write(&installed, b"installed").unwrap();

        let resolved = resolve_player_from(Some(&explicit), &installed, None).unwrap();
        assert_eq!(resolved.executable, explicit);
        assert_eq!(resolved.source, PlayerSource::Environment);
    }

    #[test]
    fn installed_layout_uses_each_operating_system_convention() {
        let base = Path::new("base");
        assert_eq!(
            installed_directory(base, "windows"),
            base.join("Programs").join("DX").join("Video")
        );
        assert_eq!(
            installed_directory(base, "macos"),
            base.join("DX").join("Video")
        );
        assert_eq!(
            installed_directory(base, "linux"),
            base.join("dx").join("video")
        );
        assert_eq!(player_filename_for("windows"), "dx-video-player.exe");
        assert_eq!(player_filename_for("macos"), "dx-video-player");
        assert_eq!(player_filename_for("linux"), "dx-video-player");
    }

    #[cfg(windows)]
    #[test]
    fn incomplete_installed_player_reports_missing_runtime_files() {
        let temp = tempfile::tempdir().unwrap();
        let installed = temp.path().join("dx-video-player.exe");
        std::fs::write(&installed, b"player").unwrap();

        let error = resolve_player_from(None, &installed, None).unwrap_err();
        assert!(matches!(
            error,
            VideoPlayerError::RuntimeManifestUnreadable { .. }
        ));
    }
}
