//! Real STT / TTS for dx-tui, backed by dx-flow binaries + local models.
//!
//! - **Ctrl+S**: capture microphone → Parakeet/Moonshine STT → insert transcript
//! - **Ctrl+T**: Kokoro TTS speak selection / last assistant reply
//! - Live frequency bars driven by real mic energy (not fake sine waves)

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Number of frequency bands shown in the input-box wave.
pub const WAVE_BANDS: usize = 28;

/// Voice I/O state for the TUI panel + inline listen mode.
#[derive(Debug)]
pub struct VoicePanel {
	pub open: bool,
	pub mode: VoiceMode,
	pub last_transcript: String,
	pub last_tts_path: Option<PathBuf>,
	pub status: String,
	pub cursor: usize,
	pub stt_ready: bool,
	pub tts_ready: bool,
	pub input_path: String,
	pub tts_text: String,
	/// Ctrl+S: microphone open.
	pub listening: bool,
	/// Transcribing after stop.
	pub processing: bool,
	pub listen_started: Option<Instant>,
	/// Live band levels 0.0..1.0 (from mic when listening).
	pub wave_levels: [f32; WAVE_BANDS],
	/// Peak hold for professional meter look.
	pub wave_peaks: [f32; WAVE_BANDS],
	pub speaking: bool,
	/// Active mic capture session (stream lives on a background thread).
	mic: Option<MicSession>,
}

impl Default for VoicePanel {
	fn default() -> Self {
		Self {
			open: false,
			mode: VoiceMode::default(),
			last_transcript: String::new(),
			last_tts_path: None,
			status: String::new(),
			cursor: 0,
			stt_ready: false,
			tts_ready: false,
			input_path: String::new(),
			tts_text: String::new(),
			listening: false,
			processing: false,
			listen_started: None,
			wave_levels: [0.08; WAVE_BANDS],
			wave_peaks: [0.08; WAVE_BANDS],
			speaking: false,
			mic: None,
		}
	}
}

impl Clone for VoicePanel {
	fn clone(&self) -> Self {
		// Mic session is not cloned — only UI state.
		Self {
			open: self.open,
			mode: self.mode,
			last_transcript: self.last_transcript.clone(),
			last_tts_path: self.last_tts_path.clone(),
			status: self.status.clone(),
			cursor: self.cursor,
			stt_ready: self.stt_ready,
			tts_ready: self.tts_ready,
			input_path: self.input_path.clone(),
			tts_text: self.tts_text.clone(),
			listening: self.listening,
			processing: self.processing,
			listen_started: self.listen_started,
			wave_levels: self.wave_levels,
			wave_peaks: self.wave_peaks,
			speaking: self.speaking,
			mic: None,
		}
	}
}

impl VoicePanel {
	/// Start mic listen. Returns Err if device unavailable.
	pub fn start_listening(&mut self) -> Result<()> {
		if self.listening {
			return Ok(());
		}
		let session = MicSession::start().context("Failed to open microphone")?;
		self.mic = Some(session);
		self.listening = true;
		self.processing = false;
		self.listen_started = Some(Instant::now());
		self.status = "● REC · speak · Ctrl+S stop → STT".into();
		Ok(())
	}

	/// Stop mic and return recorded mono f32 samples + sample rate.
	pub fn stop_listening(&mut self) -> Result<Option<(Vec<f32>, u32)>> {
		self.listening = false;
		self.listen_started = None;
		let Some(session) = self.mic.take() else {
			self.wave_levels = [0.06; WAVE_BANDS];
			self.wave_peaks = [0.06; WAVE_BANDS];
			return Ok(None);
		};
		let captured = session.stop()?;
		self.wave_levels = [0.06; WAVE_BANDS];
		self.wave_peaks = [0.06; WAVE_BANDS];
		if captured.0.is_empty() || captured.0.len() < (captured.1 as usize / 4) {
			// < 250ms of audio
			self.status = "No speech captured".into();
			return Ok(None);
		}
		self.processing = true;
		self.status = "Transcribing…".into();
		Ok(Some(captured))
	}

	pub fn toggle_listening(&mut self) -> Result<ListenToggle> {
		if self.listening {
			let audio = self.stop_listening()?;
			Ok(ListenToggle::Stopped { audio })
		} else {
			self.start_listening()?;
			Ok(ListenToggle::Started)
		}
	}

	/// Pull live levels from the mic session (call on timer).
	pub fn tick_waves(&mut self) {
		if let Some(mic) = &self.mic {
			let (levels, peaks) = mic.poll_levels();
			self.wave_levels = levels;
			self.wave_peaks = peaks;
			return;
		}
		if self.processing {
			// Soft pulse while STT runs
			let t = Instant::now().elapsed().as_secs_f32();
			for (i, level) in self.wave_levels.iter_mut().enumerate() {
				*level = 0.12 + 0.08 * ((t * 6.0 + i as f32 * 0.4).sin().abs());
				self.wave_peaks[i] = (*level).max(self.wave_peaks[i] * 0.92);
			}
			return;
		}
		// Idle decay
		for i in 0..WAVE_BANDS {
			self.wave_levels[i] *= 0.88;
			if self.wave_levels[i] < 0.05 {
				self.wave_levels[i] = 0.05;
			}
			self.wave_peaks[i] = (self.wave_peaks[i] * 0.94).max(self.wave_levels[i]);
		}
	}

	pub fn open_panel(&mut self) {
		self.open = true;
		self.status = "Voice · Tab STT/TTS · Enter run · Esc close".into();
	}

	pub fn close(&mut self) {
		self.open = false;
		if self.listening {
			let _ = self.stop_listening();
		}
	}

	pub fn menu_rows(&self) -> Vec<(String, String)> {
		match self.mode {
			VoiceMode::Stt => vec![
				(
					format!("Mode: {}", self.mode.label()),
					if self.stt_ready { "ready" } else { "probe flow" }.into(),
				),
				(
					format!(
						"Audio file: {}",
						if self.input_path.is_empty() { "(or Ctrl+S mic)" } else { &self.input_path }
					),
					"Enter to transcribe".into(),
				),
				(
					"Insert transcript into input".into(),
					if self.last_transcript.is_empty() { "empty" } else { "ready" }.into(),
				),
				("Status".into(), self.status.chars().take(48).collect()),
			],
			VoiceMode::Tts => vec![
				(
					format!("Mode: {}", self.mode.label()),
					if self.tts_ready { "ready" } else { "probe flow" }.into(),
				),
				(
					format!(
						"Text: {}",
						if self.tts_text.is_empty() { "(last assistant / type)" } else { &self.tts_text }
					),
					"Enter / Ctrl+T speak".into(),
				),
				(
					format!(
						"Output: {}",
						self
							.last_tts_path
							.as_ref()
							.map(|p| p.display().to_string())
							.unwrap_or_else(|| "—".into())
					),
					"wav path".into(),
				),
				("Status".into(), self.status.chars().take(48).collect()),
			],
		}
	}
}

pub enum ListenToggle {
	Started,
	Stopped { audio: Option<(Vec<f32>, u32)> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceMode {
	#[default]
	Stt,
	Tts,
}

impl VoiceMode {
	pub fn label(self) -> &'static str {
		match self {
			Self::Stt => "STT",
			Self::Tts => "TTS",
		}
	}

	pub fn toggle(self) -> Self {
		match self {
			Self::Stt => Self::Tts,
			Self::Tts => Self::Stt,
		}
	}
}

// ── Microphone capture ───────────────────────────────────────────────────────

struct MicSession {
	stop: Arc<AtomicBool>,
	samples: Arc<Mutex<Vec<f32>>>,
	levels: Arc<Mutex<[f32; WAVE_BANDS]>>,
	peaks: Arc<Mutex<[f32; WAVE_BANDS]>>,
	sample_rate: Arc<AtomicU32>,
	_join: JoinHandle<()>,
}

impl std::fmt::Debug for MicSession {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("MicSession")
			.field("sample_rate", &self.sample_rate.load(Ordering::Relaxed))
			.finish_non_exhaustive()
	}
}

impl MicSession {
	fn start() -> Result<Self> {
		let stop = Arc::new(AtomicBool::new(false));
		let samples = Arc::new(Mutex::new(Vec::with_capacity(16000 * 30)));
		let levels = Arc::new(Mutex::new([0.1f32; WAVE_BANDS]));
		let peaks = Arc::new(Mutex::new([0.1f32; WAVE_BANDS]));
		let sample_rate = Arc::new(AtomicU32::new(16_000));

		let stop_t = stop.clone();
		let samples_t = samples.clone();
		let levels_t = levels.clone();
		let peaks_t = peaks.clone();
		let rate_t = sample_rate.clone();

		let join = thread::Builder::new()
			.name("dx-mic".into())
			.spawn(move || {
				if let Err(e) = run_mic_loop(stop_t, samples_t, levels_t, peaks_t, rate_t) {
					tracing::warn!("mic capture ended: {e:#}");
				}
			})
			.context("spawn mic thread")?;

		// Give stream a moment to fail fast
		thread::sleep(Duration::from_millis(80));
		if join.is_finished() {
			// Thread exited early — try join for error context
			let _ = join.join();
			bail!("Microphone stream failed to start (check default input device)");
		}

		Ok(Self { stop, samples, levels, peaks, sample_rate, _join: join })
	}

	fn stop(self) -> Result<(Vec<f32>, u32)> {
		self.stop.store(true, Ordering::SeqCst);
		// Wait for thread (with timeout via busy join)
		let rate = self.sample_rate.load(Ordering::SeqCst).max(8_000);
		// Join may block briefly until stream drops
		let _ = self._join.join();
		let samples = self.samples.lock().map(|g| g.clone()).unwrap_or_default();
		Ok((samples, rate))
	}

	fn poll_levels(&self) -> ([f32; WAVE_BANDS], [f32; WAVE_BANDS]) {
		let levels = self.levels.lock().map(|g| *g).unwrap_or([0.1; WAVE_BANDS]);
		let peaks = self.peaks.lock().map(|g| *g).unwrap_or([0.1; WAVE_BANDS]);
		(levels, peaks)
	}
}

fn run_mic_loop(
	stop: Arc<AtomicBool>,
	samples: Arc<Mutex<Vec<f32>>>,
	levels: Arc<Mutex<[f32; WAVE_BANDS]>>,
	peaks: Arc<Mutex<[f32; WAVE_BANDS]>>,
	sample_rate_out: Arc<AtomicU32>,
) -> Result<()> {
	let host = cpal::default_host();
	let device = host.default_input_device().context("No default microphone")?;
	let config = device.default_input_config().context("Failed to query mic config")?;
	// cpal 0.17: SampleRate is a type alias for u32
	let sample_rate: u32 = config.sample_rate();
	let channels = config.channels() as usize;
	sample_rate_out.store(sample_rate, Ordering::SeqCst);

	let err_fn = |e| tracing::warn!("mic stream error: {e}");

	let stream = match config.sample_format() {
		cpal::SampleFormat::F32 => {
			let conf: cpal::StreamConfig = config.clone().into();
			let stop_c = stop.clone();
			let samples_c = samples.clone();
			let levels_c = levels.clone();
			let peaks_c = peaks.clone();
			device.build_input_stream(
				&conf,
				move |data: &[f32], _| {
					if stop_c.load(Ordering::Relaxed) {
						return;
					}
					let mono = to_mono(data, channels);
					push_audio(&mono, &samples_c, &levels_c, &peaks_c, sample_rate);
				},
				err_fn,
				None,
			)?
		}
		cpal::SampleFormat::I16 => {
			let conf: cpal::StreamConfig = config.clone().into();
			let stop_c = stop.clone();
			let samples_c = samples.clone();
			let levels_c = levels.clone();
			let peaks_c = peaks.clone();
			device.build_input_stream(
				&conf,
				move |data: &[i16], _| {
					if stop_c.load(Ordering::Relaxed) {
						return;
					}
					let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
					let mono = to_mono(&f, channels);
					push_audio(&mono, &samples_c, &levels_c, &peaks_c, sample_rate);
				},
				err_fn,
				None,
			)?
		}
		cpal::SampleFormat::U16 => {
			let conf: cpal::StreamConfig = config.clone().into();
			let stop_c = stop.clone();
			let samples_c = samples.clone();
			let levels_c = levels.clone();
			let peaks_c = peaks.clone();
			device.build_input_stream(
				&conf,
				move |data: &[u16], _| {
					if stop_c.load(Ordering::Relaxed) {
						return;
					}
					let f: Vec<f32> = data.iter().map(|&s| (s as f32 / 32768.0) - 1.0).collect();
					let mono = to_mono(&f, channels);
					push_audio(&mono, &samples_c, &levels_c, &peaks_c, sample_rate);
				},
				err_fn,
				None,
			)?
		}
		other => bail!("Unsupported mic sample format: {other:?}"),
	};

	stream.play().context("mic stream play")?;
	while !stop.load(Ordering::Relaxed) {
		thread::sleep(Duration::from_millis(20));
	}
	drop(stream);
	Ok(())
}

fn to_mono(data: &[f32], channels: usize) -> Vec<f32> {
	if channels <= 1 {
		return data.to_vec();
	}
	data.chunks(channels).map(|c| c.iter().sum::<f32>() / channels as f32).collect()
}

fn push_audio(
	mono: &[f32],
	samples: &Arc<Mutex<Vec<f32>>>,
	levels: &Arc<Mutex<[f32; WAVE_BANDS]>>,
	peaks: &Arc<Mutex<[f32; WAVE_BANDS]>>,
	_sample_rate: u32,
) {
	if mono.is_empty() {
		return;
	}
	if let Ok(mut buf) = samples.lock() {
		// Cap ~90s to avoid unbounded growth
		if buf.len() < 16000 * 90 {
			buf.extend_from_slice(mono);
		}
	}
	// Multi-band energy: split chunk into WAVE_BANDS windows
	let mut new_levels = [0.0f32; WAVE_BANDS];
	let n = mono.len().max(1);
	for (i, band) in new_levels.iter_mut().enumerate() {
		let start = i * n / WAVE_BANDS;
		let end = ((i + 1) * n / WAVE_BANDS).max(start + 1).min(n);
		let slice = &mono[start..end];
		let rms = (slice.iter().map(|s| s * s).sum::<f32>() / slice.len() as f32).sqrt();
		// Soft-knee gain so quiet speech still moves bars
		*band = (rms * 14.0).clamp(0.0, 1.0).powf(0.65);
	}
	// Light temporal smoothing toward previous
	if let (Ok(mut lv), Ok(mut pk)) = (levels.lock(), peaks.lock()) {
		for i in 0..WAVE_BANDS {
			// Attack fast, release slower
			let prev = lv[i];
			let next = new_levels[i];
			lv[i] = if next > prev { prev * 0.25 + next * 0.75 } else { prev * 0.72 + next * 0.28 };
			pk[i] = if lv[i] > pk[i] { lv[i] } else { pk[i] * 0.96 };
		}
	}
}

// ── WAV write ────────────────────────────────────────────────────────────────

pub fn write_wav_mono(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
	let spec = hound::WavSpec {
		channels: 1,
		sample_rate,
		bits_per_sample: 16,
		sample_format: hound::SampleFormat::Int,
	};
	let mut writer = hound::WavWriter::create(path, spec)
		.with_context(|| format!("create wav {}", path.display()))?;
	for &s in samples {
		let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
		writer.write_sample(v)?;
	}
	writer.finalize()?;
	Ok(())
}

// ── Flow binary discovery ────────────────────────────────────────────────────

fn flow_root() -> PathBuf {
	for p in [
		std::env::var_os("DX_FLOW_ROOT").map(PathBuf::from),
		std::env::var_os("FLOW_ROOT").map(PathBuf::from),
		Some(PathBuf::from(r"G:\Dx\flow")),
		Some(PathBuf::from("G:/Dx/flow")),
		Some(PathBuf::from("../flow")),
	]
	.into_iter()
	.flatten()
	{
		if p.join("models").is_dir() {
			return p;
		}
	}
	PathBuf::from(r"G:\Dx\flow")
}

fn flow_binaries() -> Vec<PathBuf> {
	let mut out = Vec::new();
	for name in ["dx-flow", "flow", "dx-flow.exe", "flow.exe"] {
		if let Ok(p) = which::which(name) {
			out.push(p);
		}
	}
	for p in [
		PathBuf::from(r"G:\Dx\bin\dx-flow.exe"),
		PathBuf::from(r"G:\Dx\bin\dx-flow"),
		flow_root().join("target/release/flow.exe"),
		flow_root().join("target/release/dx-flow.exe"),
		flow_root().join("target/debug/flow.exe"),
	] {
		if p.is_file() {
			out.push(p);
		}
	}
	// de-dupe
	out.sort();
	out.dedup();
	out
}

// ── STT ──────────────────────────────────────────────────────────────────────

/// Transcribe an audio file via dx-flow (models resolved under flow root).
pub async fn transcribe_file(path: &str) -> Result<String> {
	if path.trim().is_empty() {
		bail!("audio path required");
	}
	if !Path::new(path).is_file() {
		bail!("file not found: {path}");
	}

	#[cfg(feature = "dx-flow")]
	if let Ok(runtime) = dx_flow::runtime::FlowLocalRuntime::detect()
		&& let Ok(text) = runtime.transcribe_file(path).await
	{
		let t = text.trim().to_string();
		if !t.is_empty() {
			return Ok(t);
		}
	}

	let path_owned = path.to_string();
	let root = flow_root();
	tokio::task::spawn_blocking(move || transcribe_via_cli(&path_owned, &root))
		.await
		.context("STT task join")?
}

fn transcribe_via_cli(path: &str, root: &Path) -> Result<String> {
	let bins = flow_binaries();
	if bins.is_empty() {
		bail!(
			"No dx-flow binary found. Expected G:\\Dx\\bin\\dx-flow.exe or `flow` on PATH.\n\
			 Place models under {}\\models",
			root.display()
		);
	}

	// Raw STT only (--transcribe). Never --wispr: LLM cleanup echoes
	// "No planning. No explanation..." instruction text as fake transcripts.
	let mut last_err = String::new();
	for bin in &bins {
		let args = ["--transcribe", path];
		let output = Command::new(bin).args(args).current_dir(root).output();
		match output {
			Ok(o) if o.status.success() => {
				let stdout = String::from_utf8_lossy(&o.stdout);
				let stderr = String::from_utf8_lossy(&o.stderr);
				// STT logs sometimes go to stderr; combine for parse.
				let combined = format!("{stdout}\n{stderr}");
				if let Some(text) = parse_stt_stdout(&combined) {
					let text = sanitize_stt_text(&text);
					if is_instruction_leak(&text) {
						last_err =
							format!("{}: rejected instruction leak (not real STT): {text}", bin.display());
						continue;
					}
					if text.is_empty() {
						last_err = format!("{}: empty transcript", bin.display());
						continue;
					}
					return Ok(text);
				}
				last_err = format!(
					"{} --transcribe: empty STT parse from: {}",
					bin.display(),
					combined.chars().take(240).collect::<String>()
				);
			}
			Ok(o) => {
				last_err = format!(
					"{} --transcribe: {}\n{}",
					bin.display(),
					o.status,
					String::from_utf8_lossy(&o.stderr).chars().take(300).collect::<String>()
				);
			}
			Err(e) => {
				last_err = format!("{}: {e}", bin.display());
			}
		}
	}
	bail!("STT failed. Last error: {last_err}")
}

fn parse_stt_stdout(stdout: &str) -> Option<String> {
	// Formats from dx-flow --transcribe:
	// [stt] hello world
	// Prefer raw [stt] only — never LLM [cleaned] (echoes system prompts).
	let mut stt: Option<String> = None;
	for line in stdout.lines() {
		let t = line.trim();
		if let Some(rest) = t.strip_prefix("[stt]") {
			let rest = rest.trim();
			if !rest.is_empty() {
				stt = Some(rest.to_string());
			}
		} else if let Some(rest) = t.strip_prefix("[stt/raw]") {
			let rest = rest.trim();
			if !rest.is_empty() {
				stt = Some(rest.to_string());
			}
		}
	}
	stt.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Strip model-instruction garbage that sometimes leaks into transcripts.
fn sanitize_stt_text(text: &str) -> String {
	let mut t = text.trim().to_string();
	// Drop common leaked instruction phrases
	for bad in [
		"/no_think",
		"No planning.",
		"No explanation.",
		"Do not output <think> tags.",
		"Do not output think tags.",
		"Answer directly.",
		"CRITICAL: Output ONLY the cleaned text",
	] {
		t = t.replace(bad, "");
	}
	t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_instruction_leak(text: &str) -> bool {
	let lower = text.to_ascii_lowercase();
	if lower.is_empty() {
		return true;
	}
	// Exact / near-exact echoes of Flow Qwen no_think suffix
	let leaks = [
		"no planning",
		"no explanation",
		"answer directly",
		"do not output",
		"<think>",
		"/no_think",
		"cleaned text",
		"you are a speech-to-text",
		"you are flow",
	];
	let hit = leaks.iter().filter(|p| lower.contains(*p)).count();
	// If most of the text is instruction boilerplate, reject
	hit >= 2
		|| leaks
			.iter()
			.any(|p| lower == **p || lower.trim_matches(|c: char| !c.is_alphanumeric()) == *p)
}

/// Recorded samples → temp wav → STT model (no LLM rewrite).
pub async fn transcribe_samples(samples: Vec<f32>, sample_rate: u32) -> Result<String> {
	if samples.is_empty() {
		bail!("empty recording");
	}
	// Reject near-silence so we never "hallucinate" from empty audio
	let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
	let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
	if peak < 0.008 || rms < 0.0015 {
		bail!(
			"Recording too quiet (peak={peak:.4} rms={rms:.4}). Speak closer to the mic and try again."
		);
	}

	let path = std::env::temp_dir().join(format!("dx-stt-{}.wav", uuid::Uuid::new_v4()));
	// Resample to 16k for STT engines
	let (samples, rate) = if sample_rate != 16_000 {
		(resample_linear(&samples, sample_rate, 16_000), 16_000)
	} else {
		(samples, sample_rate)
	};
	write_wav_mono(&path, &samples, rate)?;
	let text = transcribe_file(path.to_str().unwrap_or("")).await;
	// Keep wav on failure for debug; delete on success
	match &text {
		Ok(_) => {
			let _ = std::fs::remove_file(&path);
		}
		Err(_) => {
			tracing::warn!("STT failed; kept debug wav at {}", path.display());
		}
	}
	text
}

fn resample_linear(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
	if from == 0 || to == 0 || samples.is_empty() {
		return samples.to_vec();
	}
	let ratio = to as f64 / from as f64;
	let new_len = ((samples.len() as f64) * ratio) as usize;
	let mut out = Vec::with_capacity(new_len);
	for i in 0..new_len {
		let pos = i as f64 / ratio;
		let idx = pos as usize;
		let frac = (pos - idx as f64) as f32;
		if idx + 1 < samples.len() {
			out.push(samples[idx] * (1.0 - frac) + samples[idx + 1] * frac);
		} else if idx < samples.len() {
			out.push(samples[idx]);
		}
	}
	out
}

// ── TTS ──────────────────────────────────────────────────────────────────────

/// Result of synthesis: path to wav + whether the CLI already played it.
struct SynthResult {
	path: PathBuf,
	/// True if the CLI already emitted audio (skip TUI play to avoid double).
	already_played: bool,
}

/// Synthesize speech to a temp wav via dx-flow Kokoro (no guarantee of play).
pub async fn synthesize_to_file(text: &str) -> Result<PathBuf> {
	Ok(synthesize_to_file_ex(text).await?.path)
}

async fn synthesize_to_file_ex(text: &str) -> Result<SynthResult> {
	if text.trim().is_empty() {
		bail!("empty TTS text");
	}

	#[cfg(feature = "dx-flow")]
	if let Ok(runtime) = dx_flow::runtime::FlowLocalRuntime::detect() {
		let out = std::env::temp_dir().join(format!("dx-tts-{}.wav", uuid::Uuid::new_v4()));
		if runtime.synthesize_text_to_file(text, out.to_str().unwrap_or("dx-tts.wav")).await.is_ok()
			&& out.is_file()
			&& out.metadata().map(|m| m.len() > 44).unwrap_or(false)
		{
			return Ok(SynthResult { path: out, already_played: false });
		}
	}

	let text = text.to_string();
	let root = flow_root();
	tokio::task::spawn_blocking(move || synthesize_via_cli(&text, &root))
		.await
		.context("TTS task join")?
}

/// TTS playback volume (5%).
pub const TTS_PLAYBACK_VOLUME: f32 = 0.05;

fn synthesize_via_cli(text: &str, root: &Path) -> Result<SynthResult> {
	let bins = flow_binaries();
	if bins.is_empty() {
		bail!("No dx-flow binary for TTS (expected G:\\Dx\\bin\\dx-flow.exe)");
	}

	// Unique output inside flow root so relative model paths resolve
	let out_name = format!("tmp-tts-{}.wav", uuid::Uuid::new_v4());
	let out_path = root.join(&out_name);
	// Speak writes output.wav in cwd when not using flow-tts.
	let default_out = root.join("output.wav");
	let _ = std::fs::remove_file(&default_out);

	let mut last_err = String::new();
	for bin in &bins {
		// flow-tts: write-only (no auto-play)
		if bin.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains("tts")) {
			let o = Command::new(bin)
				.args(["--text", text, "--output", out_path.to_str().unwrap_or("out.wav")])
				.current_dir(root)
				.output();
			if let Ok(out) = o
				&& out.status.success()
				&& out_path.is_file()
			{
				return Ok(SynthResult { path: out_path, already_played: false });
			}
		}

		// dx-flow --speak writes output.wav.
		// DX_TTS_NO_PLAY (new flow builds) skips CLI auto-play so TUI can play at 5%.
		// Old binaries still auto-play — detect via stdout and skip TUI play.
		let output = Command::new(bin)
			.args(["--speak", text])
			.current_dir(root)
			.env("DX_TTS_NO_PLAY", "1")
			.env("FLOW_TTS_NO_PLAY", "1")
			.output();
		match output {
			Ok(o) if o.status.success() => {
				if default_out.is_file() && default_out.metadata().map(|m| m.len() > 44).unwrap_or(false) {
					let _ = std::fs::copy(&default_out, &out_path);
					let path = if out_path.is_file() { out_path } else { default_out };
					let combined = format!(
						"{}\n{}",
						String::from_utf8_lossy(&o.stdout),
						String::from_utf8_lossy(&o.stderr)
					);
					// CLI played if it logged audio play and did NOT skip.
					let already_played =
						combined.contains("[AUDIO] Playing") && !combined.contains("play skipped");
					return Ok(SynthResult { path, already_played });
				}
				last_err = format!(
					"{} --speak: success but no wav ({})",
					bin.display(),
					String::from_utf8_lossy(&o.stdout).chars().take(120).collect::<String>()
				);
			}
			Ok(o) => {
				last_err = format!(
					"{} --speak failed: {}\n{}",
					bin.display(),
					o.status,
					String::from_utf8_lossy(&o.stderr).chars().take(300).collect::<String>()
				);
			}
			Err(e) => last_err = format!("{}: {e}", bin.display()),
		}
	}
	bail!("TTS failed. Last error: {last_err}")
}

/// Play a WAV once on the default output device at 5% volume (non-blocking).
pub fn play_audio_file(path: &Path) -> Result<()> {
	if !path.is_file() {
		bail!("audio file missing: {}", path.display());
	}
	if path.metadata().map(|m| m.len() < 64).unwrap_or(true) {
		bail!("audio file too small: {}", path.display());
	}

	let path = path.to_path_buf();
	thread::Builder::new()
		.name("dx-tts-play".into())
		.spawn(move || {
			if let Err(e) = play_audio_blocking(&path, TTS_PLAYBACK_VOLUME) {
				tracing::warn!("TTS play error: {e:#}");
			}
		})
		.context("spawn play thread")?;
	Ok(())
}

fn play_audio_blocking(path: &Path, volume: f32) -> Result<()> {
	use std::fs::File;
	let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
	let mut device = rodio::DeviceSinkBuilder::open_default_sink()
		.map_err(|e| anyhow::anyhow!("audio device unavailable: {e}"))?;
	device.log_on_drop(false);
	let player = rodio::Player::connect_new(device.mixer());
	player.set_volume(volume.clamp(0.0, 1.0));
	let source = rodio::Decoder::try_from(file)
		.map_err(|e| anyhow::anyhow!("decode {}: {e}", path.display()))?;
	// Approximate duration from file size for sleep hold
	let bytes = path.metadata().map(|m| m.len()).unwrap_or(48_000);
	// 16-bit mono 24kHz ≈ 48000 bytes/s; Kokoro is 24k
	let secs = ((bytes as f64) / 48_000.0).clamp(0.5, 120.0) + 0.4;
	player.append(source);
	// Keep sink alive for duration of playback
	thread::sleep(Duration::from_secs_f64(secs));
	drop(player);
	drop(device);
	Ok(())
}

/// Synthesize then play **once** at 5% volume.
/// Skips TUI play if the CLI already played (avoids double playback).
pub async fn speak_text(text: &str) -> Result<PathBuf> {
	let synth = synthesize_to_file_ex(text).await?;
	if !synth.already_played {
		play_audio_file(&synth.path)?;
	}
	Ok(synth.path)
}

pub fn probe_voice_ready() -> (bool, bool) {
	let flow_ok = !flow_binaries().is_empty();
	let models = flow_root().join("models");
	let stt = models.join("stt").join("encoder_model.onnx").is_file()
		|| models.join("stt").join("parakeet-tdt-0.6b-v3-int8").join("encoder.int8.onnx").is_file();
	let tts = models.join("tts").join("kokoro-v1.0.int8.onnx").is_file();
	(flow_ok && stt, flow_ok && tts)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_stt_prefers_raw_not_cleaned() {
		// LLM [cleaned] can echo "No planning..." — ignore it.
		let out = "\
[stt] actual spoken words
[cleaned]
No planning. No explanation. Answer directly.
";
		assert_eq!(parse_stt_stdout(out).as_deref(), Some("actual spoken words"));
	}

	#[test]
	fn parse_stt_simple() {
		assert_eq!(parse_stt_stdout("[stt] hi world\n").as_deref(), Some("hi world"));
	}

	#[test]
	fn rejects_instruction_leak() {
		assert!(is_instruction_leak(
			"No planning. No explanation. Do not output <think> tags. Answer directly."
		));
		assert!(!is_instruction_leak("open the file browser please"));
	}
}
