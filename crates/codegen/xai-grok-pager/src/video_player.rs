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

const PLAYER_ENV: &str = "DX_VIDEO_PLAYER";
const RUNTIME_MANIFEST: &str = "runtime-manifest.txt";

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
}

impl fmt::Display for VideoPlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => write!(f, "Usage: /video <path>"),
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
        }
    }
}

impl std::error::Error for VideoPlayerError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoLaunch {
    pub media: PathBuf,
    pub executable: PathBuf,
}

/// Launch one media path in the detached native DX player.
pub fn launch(raw_path: &str, workspace: &Path) -> Result<VideoLaunch, VideoPlayerError> {
    validate_graphical_session()?;
    let media = resolve_media_path(raw_path, workspace)?;
    let player = resolve_player()?;

    let mut command = Command::new(&player.executable);
    command
        .arg(&media)
        .current_dir(player.executable.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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

fn resolve_media_path(raw_path: &str, workspace: &Path) -> Result<PathBuf, VideoPlayerError> {
    let path_text = strip_matching_quotes(raw_path)?;
    let supplied = PathBuf::from(path_text);
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
        return Some(PathBuf::from(
            r"G:\Dx\hexxed\terminal\dx-video-player\dx-video-player.exe",
        ));
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
        let resolved = resolve_media_path("rendered clip.mp4", temp.path()).unwrap();
        assert_eq!(resolved, dunce::canonicalize(media).unwrap());
    }

    #[test]
    fn missing_media_and_directories_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        assert!(matches!(
            resolve_media_path("missing.mp4", temp.path()),
            Err(VideoPlayerError::MediaNotFound(_))
        ));
        assert!(matches!(
            resolve_media_path(".", temp.path()),
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
