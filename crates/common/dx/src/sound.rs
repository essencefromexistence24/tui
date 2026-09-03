use std::{
    env,
    io::Cursor,
    sync::{
        OnceLock,
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
    time::{Duration, Instant},
};

use rodio::Source;
use tracing::debug;

const DEFAULT_SOUND_VOLUME: f32 = 0.03;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundCue {
    Startup,
    Navigate,
    Toggle,
    Confirm,
    Submit,
    Exit,
    MenuOpen,
    MenuClose,
    TextInput,
    /// Chat-input delete / backspace (also used as the soft TUI startup cue).
    TextDelete,
    SpecialKey,
    Animation(AnimationSound),
}

impl SoundCue {
    fn configured_sounds(self) -> &'static [SoundAsset] {
        match self {
            // Soft delete-key cue at startup (not panel-open / keyboard fanfare).
            Self::Startup => &[SoundAsset::DxPanelClose],
            Self::Navigate => &[SoundAsset::DxHoverSoft],
            Self::Toggle => &[SoundAsset::DxMenuSnap],
            Self::Confirm => &[SoundAsset::DxActionConfirm],
            Self::Submit | Self::SpecialKey => &[SoundAsset::SpecialKeyTrigger],
            Self::Exit => &[SoundAsset::TrainWhistle, SoundAsset::TrainRunning],
            Self::MenuOpen => &[SoundAsset::DxPanelOpen, SoundAsset::DxMenuSnap],
            Self::MenuClose => &[SoundAsset::DxPanelClose],
            Self::TextInput => &[SoundAsset::DxTypingKey],
            Self::TextDelete => &[SoundAsset::DxPanelClose],
            Self::Animation(animation) => animation.configured_sounds(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationSound {
    Splash,
    Workspace,
    Train,
    Matrix,
    Confetti,
    GameOfLife,
    Starfield,
    Rain,
    NyanCat,
    DvdLogo,
    Fire,
    Plasma,
    Waves,
    Fireworks,
    FileBrowser,
}

impl AnimationSound {
    fn configured_sounds(self) -> &'static [SoundAsset] {
        match self {
            // Splash uses the matrix ambience (not birds).
            Self::Splash => &[SoundAsset::Matrix],
            Self::Train => &[SoundAsset::TrainRunning],
            Self::Matrix => &[SoundAsset::Matrix],
            Self::Confetti => &[SoundAsset::Confetti],
            Self::GameOfLife => &[SoundAsset::GameOfLife],
            Self::Starfield => &[SoundAsset::Space],
            Self::Rain => &[SoundAsset::Rain],
            Self::NyanCat => &[SoundAsset::NeonCat],
            Self::DvdLogo => &[SoundAsset::Jump],
            Self::Fire => &[SoundAsset::Fire],
            Self::Plasma => &[SoundAsset::Plasma],
            Self::Waves => &[SoundAsset::Wave],
            Self::Fireworks => &[SoundAsset::Fireworks],
            // File browser: silent (no eagle).
            Self::FileBrowser => &[],
            // Workspace is a live chat surface rather than an animation.
            Self::Workspace => &[],
        }
    }

    fn loop_asset(self) -> Option<SoundAsset> {
        self.configured_sounds().first().copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SoundAsset {
    #[allow(dead_code)]
    Birds,
    Confetti,
    DxActionConfirm,
    DxHoverSoft,
    DxMenuSnap,
    DxPanelClose,
    DxPanelOpen,
    DxTypingKey,
    #[allow(dead_code)]
    Eagle,
    Fire,
    Fireworks,
    GameOfLife,
    Jump,
    Matrix,
    NeonCat,
    Plasma,
    Rain,
    #[cfg(test)]
    Soil,
    Space,
    SpecialKeyTrigger,
    TrainRunning,
    TrainWhistle,
    Wave,
}

impl SoundAsset {
    fn file_name(self) -> &'static str {
        match self {
            Self::Birds => "birds.mp3",
            Self::Confetti => "confetti.mp3",
            Self::DxActionConfirm => "dx_action_confirm.wav",
            Self::DxHoverSoft => "dx_hover_soft.wav",
            Self::DxMenuSnap => "dx_menu_snap.wav",
            Self::DxPanelClose => "dx_panel_close.wav",
            Self::DxPanelOpen => "dx_panel_open.wav",
            Self::DxTypingKey => "dx_typing_key.wav",
            Self::Eagle => "eagle.mp3",
            Self::Fire => "fire.mp3",
            Self::Fireworks => "fireworks.mp3",
            Self::GameOfLife => "game-of-life.mp3",
            Self::Jump => "jump.mp3",
            Self::Matrix => "matrix.mp3",
            Self::NeonCat => "neon-cat.mp3",
            Self::Plasma => "plasma.mp3",
            Self::Rain => "rain.mp3",
            #[cfg(test)]
            Self::Soil => "soil.mp3",
            Self::Space => "space.mp3",
            Self::SpecialKeyTrigger => "special-key-trigger.mp3",
            Self::TrainRunning => "train-running.mp3",
            Self::TrainWhistle => "train-whistle.mp3",
            Self::Wave => "wave.mp3",
        }
    }

    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Birds => include_bytes!("../assets/birds.mp3"),
            Self::Confetti => include_bytes!("../assets/confetti.mp3"),
            Self::DxActionConfirm => include_bytes!("../assets/dx_action_confirm.wav"),
            Self::DxHoverSoft => include_bytes!("../assets/dx_hover_soft.wav"),
            Self::DxMenuSnap => include_bytes!("../assets/dx_menu_snap.wav"),
            Self::DxPanelClose => include_bytes!("../assets/dx_panel_close.wav"),
            Self::DxPanelOpen => include_bytes!("../assets/dx_panel_open.wav"),
            Self::DxTypingKey => include_bytes!("../assets/dx_typing_key.wav"),
            Self::Eagle => include_bytes!("../assets/eagle.mp3"),
            Self::Fire => include_bytes!("../assets/fire.mp3"),
            Self::Fireworks => include_bytes!("../assets/fireworks.mp3"),
            Self::GameOfLife => include_bytes!("../assets/game-of-life.mp3"),
            Self::Jump => include_bytes!("../assets/jump.mp3"),
            Self::Matrix => include_bytes!("../assets/matrix.mp3"),
            Self::NeonCat => include_bytes!("../assets/neon-cat.mp3"),
            Self::Plasma => include_bytes!("../assets/plasma.mp3"),
            Self::Rain => include_bytes!("../assets/rain.mp3"),
            #[cfg(test)]
            Self::Soil => include_bytes!("../assets/soil.mp3"),
            Self::Space => include_bytes!("../assets/space.mp3"),
            Self::SpecialKeyTrigger => include_bytes!("../assets/special-key-trigger.mp3"),
            Self::TrainRunning => include_bytes!("../assets/train-running.mp3"),
            Self::TrainWhistle => include_bytes!("../assets/train-whistle.mp3"),
            Self::Wave => include_bytes!("../assets/wave.mp3"),
        }
    }

    fn one_shot_start_trim(self) -> Duration {
        match self {
            Self::DxActionConfirm => Duration::from_millis(10),
            Self::DxHoverSoft => Duration::from_millis(8),
            Self::DxMenuSnap => Duration::from_millis(12),
            Self::SpecialKeyTrigger => Duration::from_millis(250),
            Self::TrainWhistle => Duration::from_millis(365),
            _ => Duration::ZERO,
        }
    }

    fn loop_start_trim(self) -> Duration {
        match self {
            Self::TrainRunning => Duration::from_millis(25),
            _ => Duration::ZERO,
        }
    }
}

#[cfg(test)]
const ALL_SOUND_ASSETS: &[SoundAsset] = &[
    SoundAsset::Birds,
    SoundAsset::Confetti,
    SoundAsset::DxActionConfirm,
    SoundAsset::DxHoverSoft,
    SoundAsset::DxMenuSnap,
    SoundAsset::DxPanelClose,
    SoundAsset::DxPanelOpen,
    SoundAsset::DxTypingKey,
    SoundAsset::Eagle,
    SoundAsset::Fire,
    SoundAsset::Fireworks,
    SoundAsset::GameOfLife,
    SoundAsset::Jump,
    SoundAsset::Matrix,
    SoundAsset::NeonCat,
    SoundAsset::Plasma,
    SoundAsset::Rain,
    SoundAsset::Soil,
    SoundAsset::Space,
    SoundAsset::SpecialKeyTrigger,
    SoundAsset::TrainRunning,
    SoundAsset::TrainWhistle,
    SoundAsset::Wave,
];

type SoundSequence = &'static [SoundAsset];

#[derive(Debug, Clone, Copy)]
enum SoundCommand {
    PlayOnce(SoundSequence),
    StartLoop(SoundAsset),
    StopLoop,
}

static SOUND_WORKER: OnceLock<SyncSender<SoundCommand>> = OnceLock::new();

#[derive(Debug)]
pub struct SoundPlayer {
    enabled: bool,
    last_cue: Option<(SoundCue, Instant)>,
    last_any_play: Option<Instant>,
    looping_animation: Option<AnimationSound>,
}

impl SoundPlayer {
    pub fn new() -> Self {
        Self {
            enabled: sound_enabled_by_default(),
            last_cue: None,
            last_any_play: None,
            looping_animation: None,
        }
    }

    pub fn play(&mut self, cue: SoundCue) {
        if !self.enabled || self.is_throttled(cue) {
            return;
        }

        let sequence = cue.configured_sounds();
        if sequence.is_empty() {
            return;
        }

        let now = Instant::now();
        self.last_cue = Some((cue, now));
        self.last_any_play = Some(now);
        queue_sound(SoundCommand::PlayOnce(sequence));
    }

    pub fn start_animation_loop(&mut self, animation: AnimationSound) {
        if self.looping_animation == Some(animation) {
            return;
        }

        // Silent animations (e.g. FileBrowser): stop any prior loop and stay quiet.
        let Some(asset) = animation.loop_asset() else {
            self.stop_animation_loop();
            self.looping_animation = Some(animation);
            return;
        };

        self.looping_animation = Some(animation);
        if self.enabled {
            queue_sound(SoundCommand::StartLoop(asset));
        }
    }

    pub fn stop_animation_loop(&mut self) {
        if self.looping_animation.take().is_none() {
            return;
        }

        if self.enabled {
            queue_sound(SoundCommand::StopLoop);
        }
    }

    pub fn current_animation_loop(&self) -> Option<AnimationSound> {
        self.looping_animation
    }

    fn is_throttled(&self, cue: SoundCue) -> bool {
        // Global minimum gap: at least 80ms between ANY two sounds
        if let Some(last) = self.last_any_play
            && last.elapsed() < Duration::from_millis(80)
        {
            return true;
        }

        // Per-cue throttle: same cue must wait 100ms
        let Some((last_cue, last_time)) = self.last_cue else {
            return false;
        };

        last_cue == cue && last_time.elapsed() < Duration::from_millis(100)
    }
}

impl SoundPlayer {
    pub fn global() -> &'static std::sync::Mutex<Self> {
        static INSTANCE: std::sync::OnceLock<std::sync::Mutex<SoundPlayer>> =
            std::sync::OnceLock::new();
        INSTANCE.get_or_init(|| std::sync::Mutex::new(SoundPlayer::new()))
    }
}

impl Default for SoundPlayer {
    fn default() -> Self {
        Self::new()
    }
}

fn sound_enabled_by_default() -> bool {
    !cfg!(test)
        && !env::var("DX_TUI_SOUND")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "0" | "false" | "off" | "no"
                )
            })
            .unwrap_or(false)
}

fn queue_sound(command: SoundCommand) {
    let sender = SOUND_WORKER.get_or_init(|| {
        let (sender, receiver) = sync_channel::<SoundCommand>(64);
        std::thread::Builder::new()
            .name("dx-tui-sound".to_string())
            .spawn(move || {
                let mut backend = AudioBackend::new();
                while let Ok(command) = receiver.recv() {
                    if backend.is_none() {
                        backend = AudioBackend::new();
                    }
                    if let Some(backend) = backend.as_mut() {
                        backend.handle(command);
                    }
                }
            })
            .ok();
        sender
    });

    match sender.try_send(command) {
        Ok(()) | Err(TrySendError::Full(_)) => {}
        Err(TrySendError::Disconnected(_)) => {}
    }
}

struct AudioBackend {
    device: rodio::MixerDeviceSink,
    loop_player: Option<rodio::Player>,
}

impl AudioBackend {
    fn new() -> Option<Self> {
        let mut device = rodio::DeviceSinkBuilder::open_default_sink()
            .map_err(|err| {
                debug!("DX TUI audio backend is unavailable: {err}");
            })
            .ok()?;
        device.log_on_drop(false);
        Some(Self {
            device,
            loop_player: None,
        })
    }

    fn handle(&mut self, command: SoundCommand) {
        match command {
            SoundCommand::PlayOnce(sequence) => self.play_sequence(sequence),
            SoundCommand::StartLoop(asset) => self.start_loop(asset),
            SoundCommand::StopLoop => self.stop_loop(),
        }
    }

    fn play_sequence(&self, sequence: SoundSequence) {
        let player = rodio::Player::connect_new(self.device.mixer());
        player.set_volume(DEFAULT_SOUND_VOLUME);

        for asset in sequence {
            match rodio::Decoder::try_from(Cursor::new(asset.bytes())) {
                Ok(source) => {
                    let trim = asset.one_shot_start_trim();
                    if trim == Duration::ZERO {
                        player.append(source);
                    } else {
                        player.append(source.skip_duration(trim));
                    }
                }
                Err(error) => {
                    debug!(
                        "failed to decode bundled DX TUI sound {}: {error}",
                        asset.file_name()
                    )
                }
            }
        }

        player.detach();
    }

    fn start_loop(&mut self, asset: SoundAsset) {
        self.stop_loop();

        let player = rodio::Player::connect_new(self.device.mixer());
        player.set_volume(DEFAULT_SOUND_VOLUME);
        match rodio::Decoder::try_from(Cursor::new(asset.bytes())) {
            Ok(source) => {
                let trim = asset.loop_start_trim();
                if trim == Duration::ZERO {
                    player.append(source.repeat_infinite());
                } else {
                    player.append(source.skip_duration(trim).repeat_infinite());
                }
                self.loop_player = Some(player);
            }
            Err(error) => {
                debug!(
                    "failed to decode bundled DX TUI ambience {}: {error}",
                    asset.file_name()
                );
            }
        }
    }

    fn stop_loop(&mut self) {
        self.loop_player = None;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        ALL_SOUND_ASSETS, AnimationSound, DEFAULT_SOUND_VOLUME, SoundAsset, SoundCue, SoundPlayer,
    };

    #[test]
    fn player_is_silent_in_tests() {
        let player = SoundPlayer::new();

        assert!(!player.enabled);
    }

    #[test]
    fn default_sound_volume_is_three_percent() {
        assert!((DEFAULT_SOUND_VOLUME - 0.03).abs() < f32::EPSILON);
    }

    #[test]
    fn one_shot_assets_trim_detected_leading_silence() {
        let trims = [
            (SoundAsset::DxActionConfirm, 10),
            (SoundAsset::DxHoverSoft, 8),
            (SoundAsset::DxMenuSnap, 12),
            (SoundAsset::SpecialKeyTrigger, 250),
            (SoundAsset::TrainWhistle, 365),
        ];

        for (asset, millis) in trims {
            assert_eq!(asset.one_shot_start_trim(), Duration::from_millis(millis));
        }

        assert_eq!(SoundAsset::Matrix.one_shot_start_trim(), Duration::ZERO);
    }

    #[test]
    fn train_ambience_trims_detected_leading_silence() {
        assert_eq!(
            SoundAsset::TrainRunning.loop_start_trim(),
            Duration::from_millis(25)
        );
        assert_eq!(SoundAsset::Matrix.loop_start_trim(), Duration::ZERO);
    }

    #[test]
    fn bundled_audio_manifest_contains_dx_tui_assets() {
        let asset_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        let names = ALL_SOUND_ASSETS
            .iter()
            .map(|asset| asset.file_name())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            [
                "birds.mp3",
                "confetti.mp3",
                "dx_action_confirm.wav",
                "dx_hover_soft.wav",
                "dx_menu_snap.wav",
                "dx_panel_close.wav",
                "dx_panel_open.wav",
                "dx_typing_key.wav",
                "eagle.mp3",
                "fire.mp3",
                "fireworks.mp3",
                "game-of-life.mp3",
                "jump.mp3",
                "matrix.mp3",
                "neon-cat.mp3",
                "plasma.mp3",
                "rain.mp3",
                "soil.mp3",
                "space.mp3",
                "special-key-trigger.mp3",
                "train-running.mp3",
                "train-whistle.mp3",
                "wave.mp3",
            ]
        );

        for asset in ALL_SOUND_ASSETS {
            assert!(
                asset_dir.join(asset.file_name()).is_file(),
                "missing bundled audio asset: {}",
                asset.file_name()
            );
        }
    }

    #[test]
    fn bundled_audio_assets_are_embedded() {
        for asset in ALL_SOUND_ASSETS {
            assert!(
                !asset.bytes().is_empty(),
                "empty bundled audio asset: {}",
                asset.file_name()
            );
        }
    }

    #[test]
    fn exit_cue_uses_train_whistle_then_running_audio() {
        let names = SoundCue::Exit
            .configured_sounds()
            .iter()
            .map(|sound| sound.file_name())
            .collect::<Vec<_>>();

        assert_eq!(names, ["train-whistle.mp3", "train-running.mp3"]);
    }

    #[test]
    fn interaction_cues_use_dx_code_audio() {
        let cue_names: &[(SoundCue, &[&str])] = &[
            (SoundCue::Startup, &["dx_panel_close.wav"]),
            (SoundCue::TextDelete, &["dx_panel_close.wav"]),
            (SoundCue::Navigate, &["dx_hover_soft.wav"]),
            (SoundCue::Toggle, &["dx_menu_snap.wav"]),
            (SoundCue::Confirm, &["dx_action_confirm.wav"]),
            (SoundCue::Submit, &["special-key-trigger.mp3"]),
            (
                SoundCue::MenuOpen,
                &["dx_panel_open.wav", "dx_menu_snap.wav"],
            ),
            (SoundCue::MenuClose, &["dx_panel_close.wav"]),
            (SoundCue::TextInput, &["dx_typing_key.wav"]),
            (SoundCue::SpecialKey, &["special-key-trigger.mp3"]),
        ];

        for (cue, expected) in cue_names.iter().copied() {
            let names = cue
                .configured_sounds()
                .iter()
                .map(|sound| sound.file_name())
                .collect::<Vec<_>>();
            assert_eq!(names, expected, "wrong sound mapping for {cue:?}");
        }
    }

    #[test]
    fn animation_cues_use_original_dx_audio() {
        let animation_names: &[(AnimationSound, &[&str])] = &[
            (AnimationSound::Splash, &["matrix.mp3"]),
            (AnimationSound::Train, &["train-running.mp3"]),
            (AnimationSound::Matrix, &["matrix.mp3"]),
            (AnimationSound::Confetti, &["confetti.mp3"]),
            (AnimationSound::GameOfLife, &["game-of-life.mp3"]),
            (AnimationSound::Starfield, &["space.mp3"]),
            (AnimationSound::Rain, &["rain.mp3"]),
            (AnimationSound::NyanCat, &["neon-cat.mp3"]),
            (AnimationSound::DvdLogo, &["jump.mp3"]),
            (AnimationSound::Fire, &["fire.mp3"]),
            (AnimationSound::Plasma, &["plasma.mp3"]),
            (AnimationSound::Waves, &["wave.mp3"]),
            (AnimationSound::Fireworks, &["fireworks.mp3"]),
            (AnimationSound::FileBrowser, &[]),
        ];

        for (animation, expected) in animation_names.iter().copied() {
            let names = SoundCue::Animation(animation)
                .configured_sounds()
                .iter()
                .map(|sound| sound.file_name())
                .collect::<Vec<_>>();
            assert_eq!(names, expected, "wrong sound mapping for {animation:?}");
        }
    }

    #[test]
    fn all_cues_map_to_bundled_audio_assets() {
        for cue in [
            SoundCue::Startup,
            SoundCue::TextDelete,
            SoundCue::Navigate,
            SoundCue::Toggle,
            SoundCue::Confirm,
            SoundCue::Submit,
            SoundCue::Exit,
            SoundCue::MenuOpen,
            SoundCue::MenuClose,
            SoundCue::TextInput,
            SoundCue::SpecialKey,
            SoundCue::Animation(AnimationSound::Matrix),
        ] {
            for sound in cue.configured_sounds() {
                assert!(sound.file_name().ends_with(".mp3") || sound.file_name().ends_with(".wav"));
                assert!(!sound.bytes().is_empty());
            }
        }
    }

    #[test]
    fn animation_loop_tracks_current_ambience_until_stopped() {
        let mut player = SoundPlayer::new();

        player.start_animation_loop(AnimationSound::Matrix);
        assert_eq!(player.looping_animation, Some(AnimationSound::Matrix));

        player.start_animation_loop(AnimationSound::Rain);
        assert_eq!(player.looping_animation, Some(AnimationSound::Rain));

        player.stop_animation_loop();
        assert_eq!(player.looping_animation, None);
    }
}
