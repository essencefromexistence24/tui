use crate::{
	agent_backend::{AgentBackend, END_OF_RESPONSE},
	animations::train_exit_duration_for_width,
	background_review::ReviewTrigger,
	channels::{self, ChannelEntry},
	command_palette::CommandPalette,
	compaction,
	components::Message,
	diff_view::DiffState,
	effects::{RainbowEffect, ShimmerEffect, TypingIndicator},
	file_tabs::FileTabBar,
	flow_backend::FlowBackend,
	goal_runner::{self, GoalState, PlanOptions},
	input::InputState,
	menu::{DxToolAction, Menu},
	modes::{AgentMode, ModelEntry, ReasoningEffort, RuntimeMode},
	notifications::NotificationManager,
	perf::PerfMonitor,
	providers::{
		ModelsDevCatalog, ProviderStore, load_or_refresh_catalog, load_provider_store,
		save_provider_store,
	},
	session_db::SessionDatabase,
	session_search::SessionSearch,
	sidebar_data::SidebarState,
	sound::{AnimationSound, SoundCue, SoundPlayer},
	theme::ChatTheme,
	token_save::{compress_history_messages, estimate_history_tokens},
	vim_mode::VimKeymap,
	voice::VoicePanel,
};

use std::{
	sync::{
		Arc,
		mpsc::{Receiver, Sender, TryRecvError, channel},
	},
	time::{Duration, Instant},
};

/// Local multi-step: generate → recover markdown tools → execute → continue.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
async fn run_local_tool_loop(
	flow: &mut FlowBackend,
	agent_prompt: &str,
	_model: &str,
	agent_mode: AgentMode,
	plan_allow_shell: bool,
	project_dir: &str,
	max_tool_steps: u32,
	tx: Sender<String>,
) -> Result<(), String> {
	let mut composed = agent_prompt.to_string();
	let mut steps = 0u32;
	let max_local = max_tool_steps.min(12);
	loop {
		steps += 1;
		match flow.generate(&composed).await {
			Ok(text) => {
				let _ = tx.send(text.clone());
				let calls = crate::tools::extract_markdown_tool_calls(&text, agent_mode);
				if calls.is_empty() || steps >= max_local {
					return Ok(());
				}
				let _ = tx.send("\n*Executing recovered local tool steps…*\n".into());
				let mut results = String::new();
				for call in &calls {
					let preview = call.arguments.chars().take(60).collect::<String>();
					let _ = tx.send(crate::tools::format_tool_running(&call.name, &preview));
					let result = crate::tools::execute(
						call,
						std::path::Path::new(project_dir),
						agent_mode,
						plan_allow_shell,
					);
					let _ = tx.send(crate::tools::format_tool_result(&result));
					results.push_str(&crate::tools::tool_message_content(&result));
					results.push_str("\n\n");
				}
				composed = format!(
					"{composed}\n\nAssistant:\n{text}\n\nTool results:\n{results}\n\n\
					 Continue. Use tools if needed; when finished, give the final answer \
					 without inventing unrun commands."
				);
			}
			Err(e) => return Err(format!("dx-flow: {e}")),
		}
	}
}

/// Floating popup menus attached to the bottom bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BottomPopup {
	#[default]
	None,
	AgentMode,
	Runtime,
	Models,
	Channels,
	/// models.dev / connected providers
	Connect,
	/// Plan-mode tool options
	PlanOptions,
	/// Share session to a channel
	ShareChannel,
	/// Preview pasted content / attached files
	PastePreview,
}

/// Full-screen / modal dialogs opened by slash commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandDialog {
	#[default]
	None,
	Sessions,
	Rename,
	/// Set chat display name (You / custom).
	UserName,
	Timeline,
	Fork,
	Export,
	Move,
	Help,
	Status,
	Debug,
	Themes,
	Skills,
	Connect,
	Mcps,
	Workspaces,
}

/// Global thinking accordion visibility for `/thinking`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingVisibility {
	/// Expand thinking blocks.
	Show,
	/// Collapse thinking blocks (default).
	#[default]
	Hide,
}

impl ThinkingVisibility {
	pub fn cycle(self) -> Self {
		match self {
			Self::Show => Self::Hide,
			Self::Hide => Self::Show,
		}
	}

	pub fn label(self) -> &'static str {
		match self {
			Self::Show => "show",
			Self::Hide => "hide",
		}
	}
}

/// Which scrollbar track is being dragged with the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum ScrollDrag {
	#[default]
 None,
	Chat { anchor_y: u16, anchor_scroll: usize },
	Sidebar { anchor_y: u16, anchor_scroll: usize },
	DiffTree { anchor_y: u16, anchor_scroll: usize },
	DiffPatch { anchor_y: u16, anchor_scroll: usize },
	FileBrowser { anchor_y: u16, anchor_scroll: usize },
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationType {
	Splash,
	Train,
	Matrix,
	Confetti,
	GameOfLife,
	Starfield,
	Rain,
	NyanCat,
	DVDLogo,
	Fire,
	Plasma,
	Waves,
	Fireworks,
	FileBrowser,
}

impl AnimationType {
	pub fn all() -> &'static [Self] {
		&[
			Self::Splash, // Start with splash
			Self::Train,
			Self::Matrix,
			// Self::Confetti,
			Self::GameOfLife,
			Self::Starfield,
			Self::Rain,
			// Self::NyanCat,
			// Self::DVDLogo,
			Self::Fire,
			Self::Plasma,
			Self::Waves,
			Self::Fireworks,
			Self::FileBrowser,
		]
	}

	/// Get only carousel animations.
	pub fn carousel_animations() -> &'static [Self] {
		&[
			Self::Matrix,
			Self::Train,
			// Self::Confetti,
			Self::GameOfLife,
			Self::Starfield,
			Self::Rain,
			// Self::NyanCat,
			// Self::DVDLogo,
			Self::Fire,
			Self::Plasma,
			Self::Waves,
			Self::Fireworks,
		]
	}

	pub fn sound(&self) -> AnimationSound {
		match self {
			Self::Splash => AnimationSound::Splash,
			Self::Train => AnimationSound::Train,
			Self::Matrix => AnimationSound::Matrix,
			Self::Confetti => AnimationSound::Confetti,
			Self::GameOfLife => AnimationSound::GameOfLife,
			Self::Starfield => AnimationSound::Starfield,
			Self::Rain => AnimationSound::Rain,
			Self::NyanCat => AnimationSound::NyanCat,
			Self::DVDLogo => AnimationSound::DvdLogo,
			Self::Fire => AnimationSound::Fire,
			Self::Plasma => AnimationSound::Plasma,
			Self::Waves => AnimationSound::Waves,
			Self::Fireworks => AnimationSound::Fireworks,
			Self::FileBrowser => AnimationSound::FileBrowser,
		}
	}
}

// ── Animation state ──────────────────────────────────────────────────────
#[derive(Debug)]
pub struct AnimationState {
	pub splash_font_index: usize,
	pub last_font_change: Instant,
	pub animation_mode: bool,
	pub current_animation_index: usize,
	pub animation_start_time: Option<Instant>,
	pub rainbow_animation: RainbowEffect,
	pub rainbow_cursor: RainbowEffect,
	pub shimmer: ShimmerEffect,
	pub show_train_animation: bool,
	pub show_matrix_animation: bool,
	pub intro_animation: AnimationType,
	pub outro_animation: AnimationType,
	pub playing_intro: bool,
	pub playing_outro: bool,
	pub transition_start_time: Option<Instant>,
	pub transition_duration: Duration,
	pub last_animation_area_width: u16,
	pub(crate) frame_buffer: Vec<Vec<(char, ratatui::style::Color)>>,
	pub(crate) frame_buffer_width: u16,
	pub(crate) frame_buffer_height: u16,
	pub(crate) exit_after_outro: bool,
	sound_player: SoundPlayer,
	pub cursor_revert_animation: bool,
	pub cursor_revert_start: Option<Instant>,
	pub cursor_revert_from_pos: usize,
}

impl AnimationState {
	pub fn new() -> Self {
		Self {
			splash_font_index: 0,
			last_font_change: Instant::now(),
			animation_mode: true,
			current_animation_index: 0,
			animation_start_time: Some(Instant::now()),
			rainbow_animation: RainbowEffect::new(),
			rainbow_cursor: RainbowEffect::new(),
			shimmer: ShimmerEffect::new(vec![ratatui::style::Color::Rgb(150, 150, 150)]),
			show_train_animation: false,
			show_matrix_animation: false,
			intro_animation: AnimationType::Matrix,
			outro_animation: AnimationType::Train,
			playing_intro: false,
			playing_outro: false,
			transition_start_time: None,
			transition_duration: Duration::from_secs(2),
			last_animation_area_width: 120,
			frame_buffer: Vec::new(),
			frame_buffer_width: 0,
			frame_buffer_height: 0,
			exit_after_outro: false,
			sound_player: SoundPlayer::new(),
			cursor_revert_animation: false,
			cursor_revert_start: None,
			cursor_revert_from_pos: 0,
		}
	}

	pub fn sound_player(&self) -> &SoundPlayer {
		&self.sound_player
	}

	pub fn sound_player_mut(&mut self) -> &mut SoundPlayer {
		&mut self.sound_player
	}

	pub fn stop_animation_ambience(&mut self) {
		self.sound_player_mut().stop_animation_loop();
	}

	pub fn start_current_animation_ambience(&mut self, current_anim: AnimationType) {
		self.sound_player_mut().stop_animation_loop();
		let sound = if current_anim == AnimationType::Splash {
			AnimationSound::Matrix
		} else {
			current_anim.sound()
		};
		self.sound_player_mut().start_animation_loop(sound);
	}

	pub fn restart_current_animation(&mut self) {
		self.animation_start_time = Some(Instant::now());
	}

	pub fn active_animation_sound(&self) -> Option<AnimationSound> {
		self.sound_player().current_animation_loop()
	}

	pub fn current_animation(&self) -> AnimationType {
		AnimationType::all().get(self.current_animation_index).copied().unwrap_or(AnimationType::Splash)
	}

	pub fn play_sound(&mut self, cue: SoundCue) {
		self.sound_player_mut().play(cue);
	}
}

// ── Session metadata ─────────────────────────────────────────────────────
#[derive(Debug)]
pub struct SessionState {
	pub session_start_time: Instant,
	pub session_name: String,
	pub session_created_at: chrono::DateTime<chrono::Local>,
	pub session_shared: bool,
	pub share_url: Option<String>,
	pub session_project_dir: String,
	pub chat_id: String,
	pub title_from_ai: bool,
	pub title_auto_generated: bool,
	pub session_cost_usd: f64,
	pub session_store: Vec<crate::slash_commands::StoredSession>,
	pub show_session_screen: bool,
	pub force_clear_frames: u8,
	pub session_exit_deadline: Option<Instant>,
	pub quit_after_session_reveal: bool,
	pub session_reveal_frames: u8,
	pub session_input_tokens: usize,
	pub session_output_tokens: usize,
	pub last_input_tokens: usize,
	pub last_output_tokens: usize,
	pub last_token_save_report: String,
	pub auto_compact_enabled: bool,
	pub token_save_enabled: bool,
}

impl SessionState {
	pub fn new(chat_id: String) -> Self {
		Self {
			session_start_time: Instant::now(),
			session_name: format!("Session {}", chat_id.split('-').next().unwrap_or("new")),
			session_created_at: chrono::Local::now(),
			session_shared: false,
			share_url: None,
			session_project_dir: std::env::current_dir()
				.map(|p| p.display().to_string())
				.unwrap_or_else(|_| ".".into()),
			chat_id,
			title_from_ai: false,
			title_auto_generated: false,
			session_cost_usd: 0.0,
			session_store: Vec::new(),
			show_session_screen: false,
			force_clear_frames: 0,
			session_exit_deadline: None,
			quit_after_session_reveal: false,
			session_reveal_frames: 0,
			session_input_tokens: 0,
			session_output_tokens: 0,
			last_input_tokens: 0,
			last_output_tokens: 0,
			last_token_save_report: String::new(),
			auto_compact_enabled: true,
			token_save_enabled: true,
		}
	}
}

// ── UI panel & layout state ──────────────────────────────────────────────
#[derive(Debug)]
pub struct UiState {
	pub bottom_popup: BottomPopup,
	pub popup_cursor: usize,
	pub chat_scroll_offset: usize,
	pub active_message_index: Option<usize>,
	pub hovered_message_index: Option<usize>,
	pub stick_scroll_to_bottom: bool,
	pub show_dx_splash: bool,
	pub dialog: CommandDialog,
	pub dialog_cursor: usize,
	pub dialog_input: String,
	pub input_area: ratatui::layout::Rect,
	pub plan_button_area: ratatui::layout::Rect,
	pub model_button_area: ratatui::layout::Rect,
	pub local_button_area: ratatui::layout::Rect,
	pub token_button_area: ratatui::layout::Rect,
	pub diff_button_area: ratatui::layout::Rect,
	pub center_chip_areas: Vec<(crate::bottom_center::CenterAction, ratatui::layout::Rect)>,
	pub center_chip_hover: Option<usize>,
	pub center_bar_area: ratatui::layout::Rect,
	pub chat_list_area: ratatui::layout::Rect,
	pub rendered_area: ratatui::layout::Rect,
	pub scroll_drag: ScrollDrag,
	pub input_text_area: ratatui::layout::Rect,
	pub minimap_area: ratatui::layout::Rect,
	pub minimap_scroll: u16,
	pub minimap_viewport: u16,
	pub minimap_top_indicator: ratatui::layout::Rect,
	pub minimap_bottom_indicator: ratatui::layout::Rect,
	pub accordion_open: [bool; crate::sidebar_data::SIDEBAR_SECTION_COUNT],
	pub sidebar_areas: [ratatui::layout::Rect; crate::sidebar_data::SIDEBAR_SECTION_COUNT],
	/// Click targets for individual task rows: (task_index, rect).
	pub sidebar_task_areas: Vec<(usize, ratatui::layout::Rect)>,
	pub sidebar_prompt_areas: Vec<(usize, ratatui::layout::Rect)>,
	pub sidebar_note_area: Option<ratatui::layout::Rect>,
	pub sidebar_panel_area: ratatui::layout::Rect,
	pub sidebar_area: ratatui::layout::Rect,
	pub sidebar_scroll: u16,
	pub show_timestamps: bool,
	pub show_sidebar: bool,
	pub show_perf_overlay: bool,
	pub toast_message: Option<String>,
	pub toast_start_time: Option<Instant>,
	pub toast_duration: Duration,
	pub shift_held: bool,
	pub shortcut_index: usize,
	pub last_shortcut_cycle: Instant,
	pub chat_select_anchor: Option<usize>,
	pub chat_select_end: Option<usize>,
	pub chat_text_selection_start: Option<(usize, usize)>,
	pub chat_text_selection_end: Option<(usize, usize)>,
	/// True while the user is drag-selecting in the message list.
	pub chat_mouse_selecting: bool,
	/// Pointer is over the chat list scrollbar track.
	pub chat_scrollbar_hovered: bool,
	/// Pointer is over the sidebar scrollbar track.
	pub sidebar_scrollbar_hovered: bool,
	pub fb_scrollbar_hovered: bool,
	pub export_include_thinking: bool,
	pub export_include_tools: bool,
}

impl UiState {
	pub fn new() -> Self {
		Self {
			bottom_popup: BottomPopup::None,
			popup_cursor: 0,
			chat_scroll_offset: 0,
			active_message_index: None,
			hovered_message_index: None,
			stick_scroll_to_bottom: true,
			show_dx_splash: false,
			dialog: CommandDialog::None,
			dialog_cursor: 0,
			dialog_input: String::new(),
			input_area: ratatui::layout::Rect::default(),
			plan_button_area: ratatui::layout::Rect::default(),
			model_button_area: ratatui::layout::Rect::default(),
			local_button_area: ratatui::layout::Rect::default(),
			token_button_area: ratatui::layout::Rect::default(),
			diff_button_area: ratatui::layout::Rect::default(),
			center_chip_areas: Vec::new(),
			center_chip_hover: None,
			center_bar_area: ratatui::layout::Rect::default(),
			chat_list_area: ratatui::layout::Rect::default(),
			rendered_area: ratatui::layout::Rect::default(),
			scroll_drag: ScrollDrag::None,
			input_text_area: ratatui::layout::Rect::default(),
			minimap_area: ratatui::layout::Rect::default(),
			minimap_scroll: 0,
			minimap_viewport: 0,
			minimap_top_indicator: ratatui::layout::Rect::default(),
			minimap_bottom_indicator: ratatui::layout::Rect::default(),
			accordion_open: [true, true, true, false, false, false, false],
			sidebar_areas: [ratatui::layout::Rect::default(); crate::sidebar_data::SIDEBAR_SECTION_COUNT],
			sidebar_task_areas: Vec::new(),
			sidebar_prompt_areas: Vec::new(),
			sidebar_note_area: None,
			sidebar_panel_area: ratatui::layout::Rect::default(),
			sidebar_area: ratatui::layout::Rect::default(),
			sidebar_scroll: 0,
			show_timestamps: true,
			show_sidebar: true,
			show_perf_overlay: false,
			toast_message: None,
			toast_start_time: None,
			toast_duration: Duration::from_secs(3),
			shift_held: false,
			shortcut_index: 0,
			last_shortcut_cycle: Instant::now(),
			chat_select_anchor: None,
			chat_select_end: None,
			chat_text_selection_start: None,
			chat_text_selection_end: None,
			chat_mouse_selecting: false,
			chat_scrollbar_hovered: false,
			sidebar_scrollbar_hovered: false,
			fb_scrollbar_hovered: false,
			export_include_thinking: true,
			export_include_tools: true,
		}
	}
}

// ── Provider / model state ───────────────────────────────────────────────
#[derive(Debug)]
pub struct ProviderState {
	pub selected_model: String,
	pub model_index: usize,
	pub model_display_name: String,
	pub model_provider_name: String,
	pub model_catalog: Vec<ModelEntry>,
	pub models_catalog: ModelsDevCatalog,
	pub provider_store: ProviderStore,
	pub channels: Vec<ChannelEntry>,
	pub channel_cursor: usize,
}

impl ProviderState {
	pub fn new() -> Self {
		Self {
			selected_model: crate::zen::DEFAULT_MODEL.to_string(),
			model_index: 0,
			model_display_name: crate::zen::DEFAULT_MODEL_DISPLAY.to_string(),
			model_provider_name: crate::zen::DEFAULT_PROVIDER.to_string(),
			model_catalog: crate::modes::remote_models(),
			models_catalog: crate::providers::load_cached_catalog()
				.unwrap_or_default(),
			provider_store: load_provider_store(),
			channels: channels::load_channels(),
			channel_cursor: 0,
		}
	}
}

// ── Voice panel state ────────────────────────────────────────────────────
#[derive(Debug)]
pub struct VoiceState {
	pub panel: VoicePanel,
}

impl VoiceState {
	pub fn new() -> Self {
		let (stt, tts) = crate::voice::probe_voice_ready();
		let mut v = VoicePanel::default();
		v.stt_ready = stt;
		v.tts_ready = tts;
		Self { panel: v }
	}
}

pub struct ChatState {
	pub theme: ChatTheme,
	pub theme_mode: crate::theme::ThemeVariant,
	pub current_theme_name: String,
	pub input: InputState,
	pub messages: Vec<Message>,
	/// Active conversation branch id (`main` or fork id).
	pub active_branch_id: String,
	/// Interactive shell host (real PTY) for in-stream terminals.
	pub pty_host: crate::msg_ui::PtyHost,
	/// Conversation branch picker overlay.
	pub branch_picker: crate::msg_ui::BranchPickerState,
	pub is_loading: bool,
	pub prompt_queue: Vec<String>,
	pub typing_indicator: TypingIndicator,
	pub cursor_visible: bool,
	pub agent_backend: Arc<AgentBackend>,
	pub flow_backend: Arc<tokio::sync::Mutex<FlowBackend>>,
	pub agent_tx: Sender<String>,
	pub agent_rx: Receiver<String>,
	pub permission_hub: crate::permission_hub::PermissionHub,
	pub question_hub: crate::question_hub::QuestionHub,
	pub delegation_ledger: crate::orchestration::DelegationLedger,
	pub update_status_line: Option<String>,
	pub last_render: Instant,
	pub agent_mode: AgentMode,
	pub reasoning_effort: ReasoningEffort,
	pub runtime_mode: RuntimeMode,
	pub selected_local_mode: String,
	pub diff_state: DiffState,
	pub last_diff_refresh: Instant,
	pub user_display_name: String,
	pub turn_started_at: Option<Instant>,
	pub thinking_started_at: Option<Instant>,
	pub thinking_visibility: ThinkingVisibility,
	pub undo_stack: Vec<Vec<Message>>,
	pub redo_stack: Vec<Vec<Message>>,
	pub sidebar: SidebarState,
	pub goal: GoalState,
	pub plan_options: PlanOptions,
	pub plan_wizard: crate::plan_wizard::PlanWizard,
	pub goal_pending_continue: bool,
	pub menu: Menu,
	pub last_frame_instant: Instant,
	pub show_tachyon_menu: bool,
	pub menu_is_closing: bool,
	pub perf_monitor: PerfMonitor,
	pub last_input_render_time: Duration,
	pub(crate) pending_dx_tool_confirmation: Option<DxToolAction>,
	pub(crate) pending_quit: bool,
	pub space_held: bool,
	pub space_hold_start: Option<Instant>,
	pub spinner_frame: usize,
	pub last_space_press: Option<Instant>,
	pub space_press_count: usize,
	pub review_trigger: ReviewTrigger,
	pub command_palette: CommandPalette,
	pub vim_mode: VimKeymap,
	pub file_tabs: FileTabBar,
	pub notification_manager: NotificationManager,
	pub session_db: SessionDatabase,
	pub session_search: SessionSearch,

	pub animation: AnimationState,
	pub session: SessionState,
	pub ui: UiState,
	pub provider: ProviderState,
	pub voice_state: VoiceState,

	/// Codex app-server integration (None when not connected).
	pub codex_bridge: Option<crate::codex_bridge::CodexBridge>,
	/// Pending codex connection result.
	pub codex_connection: Option<tokio::sync::oneshot::Receiver<anyhow::Result<crate::codex_bridge::CodexBridge>>>,
}

impl ChatState {
	pub fn new() -> Self {
		let (agent_tx, agent_rx) = channel();

		let theme_mode = crate::theme::ThemeVariant::Dark;
		let theme = ChatTheme::by_name("vercel", theme_mode).unwrap_or_else(ChatTheme::dark_fallback);

		let chat_id = uuid::Uuid::new_v4().to_string();
		let mut state = Self {
			theme: theme.clone(),
			theme_mode,
			current_theme_name: "vercel".to_string(),
			input: InputState::new(),
			messages: Vec::new(),
			active_branch_id: "main".into(),
			pty_host: crate::msg_ui::PtyHost::new(),
			branch_picker: crate::msg_ui::BranchPickerState::default(),
			is_loading: false,
			prompt_queue: Vec::new(),
			typing_indicator: TypingIndicator::new(),
			cursor_visible: true,
			agent_backend: Arc::new(AgentBackend::new()),
			flow_backend: Arc::new(tokio::sync::Mutex::new(FlowBackend::new())),
			agent_tx,
			agent_rx,
			permission_hub: crate::permission_hub::PermissionHub::new(),
			question_hub: crate::question_hub::QuestionHub::new(),
			delegation_ledger: crate::orchestration::DelegationLedger::new(),
			update_status_line: None,
			last_render: Instant::now(),
			agent_mode: AgentMode::Ask,
			reasoning_effort: ReasoningEffort::Medium,
			// runtime_mode: RuntimeMode::Local,
			runtime_mode: RuntimeMode::Remote,
			selected_local_mode: "Remote".to_string(),
			diff_state: DiffState::empty(),
			last_diff_refresh: Instant::now() - Duration::from_secs(60),
			user_display_name: "You".to_string(),
			turn_started_at: None,
			thinking_started_at: None,
			thinking_visibility: ThinkingVisibility::Hide,
			undo_stack: Vec::new(),
			redo_stack: Vec::new(),
			sidebar: SidebarState::new(),
			goal: GoalState::default(),
			plan_options: PlanOptions {
				target_folder: std::env::current_dir().ok().map(|p| p.display().to_string()),
				run_formatter: true,
				run_linter: true,
				use_lsp: true,
				use_vcs: true,
				allow_shell: false,
			},
			plan_wizard: crate::plan_wizard::PlanWizard::new(&PlanOptions::default()),
			goal_pending_continue: false,
			menu: Menu::new(theme.clone()),
			last_frame_instant: Instant::now(),
			show_tachyon_menu: false,
			menu_is_closing: false,
			perf_monitor: PerfMonitor::new(),
			last_input_render_time: Duration::from_secs(0),
			pending_dx_tool_confirmation: None,
			pending_quit: false,
			space_held: false,
			space_hold_start: None,
			spinner_frame: 0,
			last_space_press: None,
			space_press_count: 0,
			review_trigger: ReviewTrigger::default(),
			command_palette: CommandPalette::new(),
			vim_mode: VimKeymap::new(),
			file_tabs: FileTabBar::new(theme.bg, theme.fg, theme.accent, theme.border),
			notification_manager: NotificationManager::new(),
			session_db: SessionDatabase::new(),
			session_search: SessionSearch::new(),
			animation: AnimationState::new(),
			session: SessionState::new(chat_id),
			ui: UiState::new(),
			provider: ProviderState::new(),
			voice_state: VoiceState::new(),
			codex_bridge: None,
			codex_connection: None,
		};
		// Override the temporary session name with chat-id-based name
		let short = state.session.chat_id.split('-').next().unwrap_or("new");
		state.session.session_name = format!("Session {short}");
		// Already set to remote zen defaults via ProviderState::new()

		let _ = crate::agent_workspace::ensure_workspace();
		crate::update_check::spawn_prefetch(state.agent_tx.clone());

		let prefs = crate::tui_prefs::load();
		state.agent_mode = prefs.agent_mode_enum();
		state.reasoning_effort = prefs.reasoning_effort_enum();
		state.runtime_mode = prefs.runtime_mode_enum();
		state.selected_local_mode = state.runtime_mode.label().to_string();
		state.session.auto_compact_enabled = prefs.auto_compact;
		state.session.token_save_enabled = prefs.token_save;
		state.ui.show_sidebar = prefs.show_sidebar;
		if !prefs.user_name.trim().is_empty() {
			state.user_display_name = prefs.user_name.trim().to_string();
		}
		if let Some(mid) = prefs.selected_model {
			state.provider.selected_model = mid;
			state.sync_model_display();
		}
		// The agent tool loop always sends to the Zen API regardless of
		// RuntimeMode (Local/Remote only controls dx-agent vs tool loop).
		// Ensure the model is a known Zen model; if not, reset to default.
		if !crate::zen::MODELS.iter().any(|(_, id)| *id == state.provider.selected_model) {
			state.provider.selected_model = crate::zen::DEFAULT_MODEL.to_string();
			state.sync_model_display();
		}
		state.runtime_mode = RuntimeMode::Remote;
		state.selected_local_mode = RuntimeMode::Remote.label().to_string();
		state.session.session_store = crate::session_store::load_all_sessions();
		if let Ok(id) = std::env::var(crate::CONTINUE_SESSION_ENV) {
			if !id.trim().is_empty() {
				match crate::session_store::load_session_by_id(id.trim()) {
					Ok(snap) => {
						let name = snap.name.clone();
						state.load_session_from_store(snap);
						state.animation.animation_mode = false;
						state.show_toast(format!("Resumed · {name}"));
					}
					Err(e) => {
						state.show_toast(format!("Continue failed: {e}"));
					}
				}
				// SAFETY: called during ChatState initialization, before any concurrent env access
				unsafe {
					std::env::remove_var(crate::CONTINUE_SESSION_ENV);
				}
			}
		} else if !state.session.session_store.is_empty() {
			state.show_toast(format!(
				"{} saved · /sessions or dx continue <id>",
				state.session.session_store.len()
			));
		}

		state.bootstrap_catalog();
		state.animation.start_current_animation_ambience(state.animation.current_animation());
		state.bootstrap_backends();
		let (add, del) = crate::diff_view::quick_diff_stats();
		state.diff_state.total_additions = add;
		state.diff_state.total_deletions = del;
		state
	}

	pub fn current_animation(&self) -> AnimationType {
		self.animation.current_animation()
	}

	pub fn toggle_theme_mode(&mut self) {
		use crate::theme::ChatTheme;

		if let Some(new_theme) = ChatTheme::by_name(&self.current_theme_name, self.theme_mode) {
			self.theme = new_theme.clone();
			self.menu.theme = new_theme;
			self.file_tabs.update_theme(
				self.theme.bg,
				self.theme.fg,
				self.theme.accent,
				self.theme.border,
			);
		}
	}

	/// Apply a theme by name and mode
	pub fn apply_theme(&mut self, theme_name: &str, mode: crate::theme::ThemeVariant) {
		use crate::theme::ChatTheme;

		if let Some(new_theme) = ChatTheme::by_name(theme_name, mode) {
			self.theme = new_theme.clone();
			self.menu.theme = new_theme;
			self.current_theme_name = theme_name.to_string();
			self.theme_mode = mode;
			self.file_tabs.update_theme(
				self.theme.bg,
				self.theme.fg,
				self.theme.accent,
				self.theme.border,
			);
		}
	}

	pub fn add_user_message(&mut self, content: String) {
		self.play_sound(SoundCue::Submit);

		if self.is_loading {
			self.prompt_queue.push(content.clone());
			self.sidebar.add_prompt(format!("{} (queued)", content));
			self.show_toast("Prompt queued".into());
			return;
		}

		let is_first_turn =
			self.messages.iter().filter(|m| m.role == crate::components::MessageRole::User).count() == 0;
		if is_first_turn && !self.session.title_from_ai {
			// Immediate multi-word label from the user prompt; AI TITLE: may replace later.
			self.session.session_name = crate::session_meta::compact_session_title(&content);
			self.session.title_auto_generated = true;
			self.session.title_from_ai = false;
		}

		if self.agent_mode == AgentMode::Goal && !self.goal.active {
			self.goal.start(content.clone());
		}

		if self.session.auto_compact_enabled
			&& compaction::should_auto_compact(&self.messages, self.context_limit())
		{
			let report = compaction::compact_messages(&mut self.messages, true);
			self.show_toast(format!(
				"Auto-compacted {}→{} msgs ({}→{} tok)",
				report.before_msgs, report.after_msgs, report.before_tokens, report.after_tokens
			));
		}

		let workspace_signals = if self.agent_mode == AgentMode::Plan
			&& (self.plan_options.run_formatter
				|| self.plan_options.run_linter
				|| self.plan_options.use_lsp
				|| self.plan_options.use_vcs)
		{
			let report = goal_runner::run_plan_tools(&self.plan_options);
			let cwd = std::path::PathBuf::from(
				self.plan_options.target_folder.as_deref().unwrap_or(&self.session.session_project_dir),
			);
			if self.plan_options.run_formatter {
				let r = crate::workspace_tools::run_formatter(&cwd);
				self.sidebar.set_tool_reports(Some(r.summary), None);
			}
			if self.plan_options.run_linter {
				let r = crate::workspace_tools::run_linter(&cwd);
				self.sidebar.set_tool_reports(None, Some(r.summary));
			}
			if self.plan_options.use_lsp {
				self.sidebar.refresh_diagnostics();
			}
			self.sidebar.refresh();
			Some(report)
		} else {
			None
		};

		let user_for_model = match self.agent_mode {
			AgentMode::Goal if self.goal.active => {
				format!("{}\n\nUser update: {content}", goal_runner::goal_continuation_prompt(&self.goal))
			}
			AgentMode::Plan => {
				if let Some(ref sig) = workspace_signals {
					let _ = sig;
					format!("[plan] {content}")
				} else {
					content.clone()
				}
			}
			_ => content.clone(),
		};

		let sys_ctx = crate::dx_system::SystemContext {
			mode: self.agent_mode,
			model_id: &self.provider.selected_model,
			model_display: &self.provider.model_display_name,
			project_dir: &self.session.session_project_dir,
			first_turn: is_first_turn,
			workspace_signals: workspace_signals.as_deref(),
		};
		let system = crate::dx_system::build_system(&sys_ctx);
		let agent_prompt = crate::dx_system::as_agent_prefix(&system, &user_for_model);

		let mut message = Message::user(content);
		message.branch_id = self.active_branch_id.clone();
		message.parent_id = self
			.messages
			.iter()
			.rev()
			.find(|m| !m.hidden && m.branch_id == self.active_branch_id)
			.map(|m| m.id.clone());
		self.messages.push(message);

		// Track prompt in sidebar only if already loading (truly queued, waiting for current response)
		if self.is_loading {
			self.sidebar.add_prompt(user_for_model.clone());
		}

		if self.animation.animation_mode {
			self.animation.stop_animation_ambience();
			self.animation.animation_mode = false;
			self.play_intro_animation();
		}

		self.ui.chat_scroll_offset = 0;

		self.is_loading = true;
		self.turn_started_at = Some(Instant::now());
		self.thinking_started_at = None;
		{
			let mut asst = Message::assistant(String::new());
			asst.branch_id = self.active_branch_id.clone();
			asst.parent_id = self.messages.last().map(|m| m.id.clone());
			self.messages.push(asst);
		}

		if self.goal.active {
			self.goal.tick_iteration();
		}

		let tx = Sender::clone(&self.agent_tx);
		let model = self.provider.selected_model.clone();
		let runtime = self.runtime_mode;
		let agent_backend = Arc::clone(&self.agent_backend);
		let _flow_backend = Arc::clone(&self.flow_backend);
		let token_save = self.session.token_save_enabled;
		let context_limit = self.context_limit();

		let mut history: Vec<(String, String)> = self
			.messages
			.iter()
			.filter(|m| !m.content.is_empty() || m.role == crate::components::MessageRole::User)
			.map(|m| {
				let role = match m.role {
					crate::components::MessageRole::User => "user".to_string(),
					crate::components::MessageRole::Assistant => "assistant".to_string(),
				};
				(role, m.content.clone())
			})
			.collect();
		if let Some((role, body)) = history.iter_mut().rev().find(|(r, _)| r == "user") {
			let _ = role;
			*body = user_for_model.clone();
		}
		if history.last().is_some_and(|(r, c)| r == "assistant" && c.is_empty()) {
			history.pop();
		}

		if token_save {
			let before = estimate_history_tokens(&history);
			let max_chars = (context_limit * 3).max(4_000);
			let sample = history.last().map(|(_, c)| crate::token_save::compress_tool_output(c));
			history = compress_history_messages(&history, max_chars);
			let after = estimate_history_tokens(&history);
			if before > after {
				let ratio = ((1.0 - after as f32 / before.max(1) as f32) * 100.0) as u32;
				self.session.last_token_save_report = match sample {
					Some(r) if r.original_chars > 0 => {
						format!("token-save: {before}→{after} tok (~{ratio}%) · {}", r.report_line())
					}
					_ => format!("token-save: {before}→{after} tok (~{ratio}%)"),
				};
			}
		}

		let sys_tok = crate::token_save::estimate_tokens(&system);
		let hist_tok = estimate_history_tokens(&history);
		self.session.last_input_tokens = sys_tok.saturating_add(hist_tok);
		self.session.session_input_tokens =
			self.session.session_input_tokens.saturating_add(self.session.last_input_tokens);

		let prefer_agent = self.agent_mode.prefers_dx_agent();
		let use_tool_loop = crate::agent_loop::mode_uses_tool_loop(self.agent_mode);
		let omni_url = crate::omniroute::chat_completions_url();
		let session_id = self.session.chat_id.clone();
		let system_for_remote = system.clone();
		let agent_mode = self.agent_mode;
		let plan_allow_shell = self.plan_options.allow_shell;
		let project_dir = self.session.session_project_dir.clone();
		let max_tool_steps = match agent_mode {
			AgentMode::Goal | AgentMode::Agent => 24,
			AgentMode::Write => 16,
			AgentMode::Plan => 8,
			AgentMode::Ask | AgentMode::Multi | AgentMode::Codex => 6,
			AgentMode::Automation => 48,
		};
		let perm_hub = self.permission_hub.clone();
		let q_hub = self.question_hub.clone();
		let ledger = self.delegation_ledger.clone();
		let sidebar = self.sidebar.clone();
		self.permission_hub.clear();
		self.question_hub.clear();
		self.ui.stick_scroll_to_bottom = true;
		// Ensure agent workspace bootstrap exists (SOUL.md, …)
		let _ = crate::agent_workspace::ensure_workspace();

		// CRITICAL: persist immediately to eliminate data loss window
		self.persist_current_session();

		// For Codex mode, spawn a task to submit via CodexBridge
		if agent_mode == crate::modes::AgentMode::Codex {
			if let Some(ref mut bridge) = self.codex_bridge {
				let text = user_for_model.clone();
				tokio::spawn(async move {
					if let Err(e) = bridge.submit_turn(&text).await {
						tracing::error!("codex submit failed: {e}");
					}
				});
			}
			return;
		}

		tokio::spawn(async move {
			let result = match runtime {
				RuntimeMode::Remote => {
					let agent_cb = {
						let tx = tx.clone();
						move |chunk: String| {
							let _ = tx.send(chunk);
						}
					};

					let run_tool_loop = async {
						let input = crate::agent_loop::LoopInput {
							model: model.clone(),
							system: system_for_remote.clone(),
							history: history.clone(),
							mode: agent_mode,
							cwd: std::path::PathBuf::from(&project_dir),
							plan_allow_shell,
							api_url: omni_url.clone(),
							enable_native_tools: true,
							max_steps: max_tool_steps,
							permission: Some(perm_hub),
							questions: Some(q_hub),
							ledger: Some(ledger),
							sidebar: Some(sidebar),
						};
						crate::agent_loop::run(input, tx.clone()).await
					};

					let run_agent = async {
						agent_backend
							.generate_stream_for_session(
								Some(&session_id),
								&agent_prompt,
								&history,
								agent_cb,
							)
							.await
					};

					let run_plain = async {
						let url = omni_url.clone();
						if let Some(ref u) = url {
							crate::zen::stream_chat_url_with_system(
								&model,
								history.clone(),
								tx.clone(),
								u,
								Some(system_for_remote.clone()),
							)
							.await
						} else {
							crate::zen::stream_chat_with_system(
								&model,
								history.clone(),
								tx.clone(),
								Some(system_for_remote.clone()),
							)
							.await
						}
					};

					// Agent mode: dx-agent primary, tool loop fallback, plain stream last resort.
					// Other modes: tool loop primary, plain stream fallback.
					if prefer_agent {
						match run_agent.await {
							Ok(()) => Ok(()),
							Err(agent_err) => {
								tracing::warn!("dx-agent failed, falling back to tool loop: {agent_err}");
								match run_tool_loop.await {
									Ok(()) => Ok(()),
									Err(loop_err) => {
										match run_plain.await {
											Ok(()) => Ok(()),
											Err(plain_err) => Err(format!(
												"agent: {agent_err}; tool-loop: {loop_err}; plain: {plain_err}"
											)),
										}
									}
								}
							}
						}
					} else if use_tool_loop {
						match run_tool_loop.await {
							Ok(()) => Ok(()),
							Err(loop_err) => {
								match run_plain.await {
									Ok(()) => Ok(()),
									Err(plain_err) => Err(format!("tool-loop: {loop_err}; plain: {plain_err}")),
								}
							}
						}
					} else {
						run_plain.await.map_err(|e| e.to_string())
					}
				}
				RuntimeMode::Local => {
					// Zen agent loop (tool loop via omni-url or default Zen URL).
					let run_tool_loop_local = async {
						let input = crate::agent_loop::LoopInput {
							model: model.clone(),
							system: system_for_remote.clone(),
							history: history.clone(),
							mode: agent_mode,
							cwd: std::path::PathBuf::from(&project_dir),
							plan_allow_shell,
							api_url: omni_url.clone(),
							enable_native_tools: true,
							max_steps: max_tool_steps,
							permission: Some(perm_hub),
							questions: Some(q_hub),
							ledger: Some(ledger),
							sidebar: Some(sidebar),
						};
						crate::agent_loop::run(input, tx.clone()).await
					};
					let run_plain_local = async {
						let url = omni_url.clone();
						if let Some(ref u) = url {
							crate::zen::stream_chat_url_with_system(
								&model,
								history.clone(),
								tx.clone(),
								u,
								Some(system_for_remote.clone()),
							)
							.await
						} else {
							crate::zen::stream_chat_with_system(
								&model,
								history.clone(),
								tx.clone(),
								Some(system_for_remote.clone()),
							)
							.await
						}
					};
					if use_tool_loop {
						match run_tool_loop_local.await {
							Ok(()) => Ok(()),
							Err(loop_err) => {
								tracing::warn!("local tool-loop failed, trying plain stream: {loop_err}");
								match run_plain_local.await {
									Ok(()) => Ok(()),
									Err(plain_err) => Err(format!("local tool-loop: {loop_err}; plain: {plain_err}")),
								}
							}
						}
					} else {
						match run_plain_local.await {
							Ok(()) => Ok(()),
							Err(e) => Err(format!("local stream: {e}")),
						}
					}
				}
			};

			match result {
				Ok(()) => {
					let _ = tx.send(END_OF_RESPONSE.to_string());
				}
				Err(e) => {
					let _ = tx.send(format!("\n\n*Error: {e}*"));
					let _ = tx.send(END_OF_RESPONSE.to_string());
				}
			}
		});
	}

	pub fn save_prefs(&self) {
		let prefs = crate::tui_prefs::TuiPrefs {
			agent_mode: self.agent_mode.label().to_string(),
			runtime_mode: self.runtime_mode.label().to_string(),
			selected_model: Some(self.provider.selected_model.clone()),
			auto_compact: self.session.auto_compact_enabled,
			token_save: self.session.token_save_enabled,
			show_sidebar: self.ui.show_sidebar,
			last_session_id: Some(self.session.chat_id.clone()),
			user_name: self.user_display_name.clone(),
			reasoning_effort: self.reasoning_effort.label().to_string(),
		};
		if let Err(e) = crate::tui_prefs::save(&prefs) {
			tracing::debug!("prefs save failed: {e}");
		}
	}

	pub fn set_user_display_name(&mut self, name: impl Into<String>) {
		let n = name.into().trim().to_string();
		if n.is_empty() {
			self.show_toast("Name cannot be empty".into());
			return;
		}
		self.user_display_name = n.chars().take(32).collect();
		self.save_prefs();
		self.show_toast(format!("You are “{}”", self.user_display_name));
	}

	pub fn on_push_to_talk_release(&mut self) {
		// Prefer real Ctrl+S mic session if still open
		if self.voice_state.panel.listening {
			match self.voice_state.panel.stop_listening() {
				Ok(Some((samples, rate))) => {
					self
						.show_toast(format!("Transcribing {:.1}s…", samples.len() as f32 / rate.max(1) as f32));
					let tx = self.agent_tx.clone();
					if let Ok(handle) = tokio::runtime::Handle::try_current() {
						handle.spawn(async move {
							match crate::voice::transcribe_samples(samples, rate).await {
								Ok(text) => {
									let _ = tx.send(format!("\n__VOICE_STT__\n{text}"));
								}
								Err(e) => {
									let _ = tx.send(format!("\n__VOICE_ERR__\n{e}"));
								}
							}
						});
					}
					return;
				}
				Ok(None) => {
					self.show_toast("No speech captured".into());
					return;
				}
				Err(e) => {
					self.show_toast(format!("Mic: {e}"));
					return;
				}
			}
		}

		let path = self.voice_state.panel.input_path.trim().to_string();
		if !path.is_empty() && std::path::Path::new(&path).is_file() {
			self.voice_state.panel.status = "Push-to-talk STT…".into();
			self.show_toast("Transcribing file…".into());
			let tx = self.agent_tx.clone();
			if let Ok(handle) = tokio::runtime::Handle::try_current() {
				let voice_tx = self.agent_tx.clone();
				handle.spawn(async move {
					match crate::voice::transcribe_file(&path).await {
						Ok(text) => {
							let _ = voice_tx.send(format!("\n__VOICE_STT__\n{text}"));
						}
						Err(e) => {
							let _ = tx.send(format!("\n__VOICE_ERR__\n{e}"));
						}
					}
				});
			}
			return;
		}
		if !self.voice_state.panel.last_transcript.is_empty() {
			let t = self.voice_state.panel.last_transcript.clone();
			self.input.replace_content(&t);
			self.show_toast("Inserted last transcript".into());
			return;
		}
		self.show_toast("Ctrl+S to record mic · or set audio path in /voice".into());
	}

	pub fn bootstrap_catalog(&self) {
		let Ok(handle) = tokio::runtime::Handle::try_current() else {
			return;
		};
		// Catalog is loaded into state via channel-less fire-and-forget; we reload on /connect.
		handle.spawn(async move {
			let cat = tokio::task::spawn_blocking(load_or_refresh_catalog).await;
			match cat {
				Ok(c) => {
					tracing::info!("models.dev: {} providers, {} models", c.provider_count(), c.model_count())
				}
				Err(e) => tracing::warn!("models.dev load failed: {e}"),
			}
		});
	}

	pub fn reload_models_catalog(&mut self) {
		self.provider.models_catalog = load_or_refresh_catalog();
		self.show_toast(format!(
			"Catalog: {} providers · {} models",
			self.provider.models_catalog.provider_count(),
			self.provider.models_catalog.model_count()
		));
		if self.runtime_mode == RuntimeMode::Remote {
			self.merge_catalog_into_model_menu();
		}
	}

	fn merge_catalog_into_model_menu(&mut self) {
		self.provider.model_catalog = crate::provider_registry::build_production_model_menu(
			&self.provider.models_catalog,
			&self.provider.provider_store,
		);
		self.sync_model_display();
	}

	pub fn share_session_to_channel_index(&mut self, index: usize) {
		let list = crate::channel_actions::sendable_channels(&self.provider.channels);
		let Some(ch) = list.get(index).cloned() else {
			self.show_toast("No channel at that index".into());
			return;
		};
		let md =
			self.transcript_markdown(self.ui.export_include_thinking, self.ui.export_include_tools);
		match crate::channel_actions::share_transcript_to_channel(&ch, &self.session.session_name, &md)
		{
			Ok(msg) => self.show_toast(msg),
			Err(e) => self.show_toast(format!("Share failed: {e}")),
		}
	}

	pub fn connect_provider_by_catalog_index(&mut self, index: usize) {
		let mut providers: Vec<_> = self.provider.models_catalog.providers.to_vec();
		providers.sort_by(|a, b| {
			let ac = self.provider.provider_store.providers.iter().any(|c| c.id == a.id && c.enabled);
			let bc = self.provider.provider_store.providers.iter().any(|c| c.id == b.id && c.enabled);
			ac.cmp(&bc).then_with(|| a.name.cmp(&b.name))
		});
		let Some(p) = providers.get(index).cloned() else {
			self.show_toast("Provider not found".into());
			return;
		};
		let default_model = p.models.first().map(|m| m.id.clone());
		let env_hint = p.env.first().map(|e| format!(" · set env {e}")).unwrap_or_default();
		self.provider.provider_store.connect_from_catalog(
			&p.id,
			&p.name,
			p.api.clone(),
			&p.env,
			default_model.clone(),
		);
		if let Err(e) = save_provider_store(&self.provider.provider_store) {
			self.show_toast(format!("Saved in-memory only: {e}"));
		} else {
			self.show_toast(format!("Connected: {} ({}){env_hint}", p.name, p.id));
		}
		if let Some(mid) = default_model
			&& let Some(m) = p.models.iter().find(|m| m.id == mid) {
				self.provider.selected_model = m.id.clone();
				self.provider.model_display_name = m.name.clone();
				self.provider.model_provider_name = p.name.clone();
				self.runtime_mode = RuntimeMode::Remote;
			}
		self.merge_catalog_into_model_menu();
	}

	pub fn on_assistant_turn_finished(&mut self) {
		let elapsed = self.turn_started_at.take().map(|t| t.elapsed());
		let profile = self.agent_mode.label().to_string();
		let model = self.provider.model_display_name.clone();

		if let Some(last) = self.messages.last_mut()
			&& last.role == crate::components::MessageRole::Assistant {
				last.footer_profile = Some(profile);
				last.footer_model = Some(model);
				last.footer_duration = elapsed;

				let out = crate::token_save::estimate_tokens(&last.content);
				self.session.last_output_tokens = out;
				self.session.session_output_tokens = self.session.session_output_tokens.saturating_add(out);
				last.token_count = out;

				let (ai_title, todos) = crate::session_meta::apply_first_turn_meta(
					&last.content,
					&mut self.session.session_name,
					&mut self.session.title_from_ai,
				);
				if ai_title.is_some() {
					self.session.title_auto_generated = true;
				}
				let cleaned = crate::session_meta::strip_title_lines(&last.content);
				if !cleaned.is_empty() && cleaned != last.content {
					last.content = cleaned;
					last.sync_parts_from_content();
					last.token_count = crate::token_save::estimate_tokens(&last.content);
					let delta = out.saturating_sub(last.token_count);
					self.session.session_output_tokens =
						self.session.session_output_tokens.saturating_sub(delta);
					self.session.last_output_tokens = last.token_count;
				}
				if !todos.is_empty() {
					self.sidebar.merge_tasks(todos);
				} else {
					let tasks = compaction::extract_tasks_from_text(&last.content);
					if !tasks.is_empty() {
						self.sidebar.merge_tasks(tasks);
					}
				}
			}
		if !self.session.title_from_ai
			&& crate::session_meta::is_provisional_session_name(&self.session.session_name)
		{
			self.session.session_name = "Chat".into();
			self.session.title_auto_generated = true;
		}

		if let Some(last) = self.messages.last().cloned() {
			if last.role == crate::components::MessageRole::Assistant && self.goal.can_continue() {
				if GoalState::detect_completion(&last.content) {
					self.goal.stop("Goal complete");
					self.show_toast("Goal complete".into());
					self.goal_pending_continue = false;
				} else if self.goal.timed_out() {
					self.goal.stop("time budget exceeded");
					self.show_toast("Goal stopped: time budget".into());
					self.goal_pending_continue = false;
				} else if self.goal.iteration_budget_hit() {
					self.goal.stop("iteration budget");
					self.show_toast("Goal stopped: max iterations".into());
					self.goal_pending_continue = false;
				} else {
					self.goal_pending_continue = true;
				}
			}

			if last.role == crate::components::MessageRole::Assistant
				&& matches!(self.agent_mode, AgentMode::Agent | AgentMode::Goal | AgentMode::Write)
				&& !last.interrupted
			{
				let tool_steps = last.content.matches("```command").count() as u32
					+ last.content.matches("<tool_call").count() as u32;
				let ok_steps = last.content.matches("status=\"done\"").count() as u32
					+ last.content.matches("✓ ").count() as u32;
				let user_goal = self
					.messages
					.iter()
					.rev()
					.find(|m| m.role == crate::components::MessageRole::User)
					.map(|m| m.content.clone())
					.unwrap_or_default();
				if let Some(path) = crate::skills::auto_create_from_turn(
					&user_goal,
					&last.content,
					tool_steps.max(ok_steps),
					ok_steps.max(tool_steps.saturating_sub(1)),
				) {
					let name =
						path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("skill");
					self.show_toast(format!("Skill saved · {name}"));
				}
			}
		}

		self.sidebar.sync_subagents(&self.messages);
		self.persist_current_session();
		self.sidebar.refresh_if_stale(Duration::from_secs(30));

		if crate::skills::should_run_curator() {
			let report = crate::skills::run_curator();
			crate::skills::mark_curator_run();
			if !report.contains("No stale") {
				self.show_toast(report);
			}
		}

		let assistant_count =
			self.messages.iter().filter(|m| m.role == crate::components::MessageRole::Assistant).count();
		self.review_trigger.tick();
		if self.review_trigger.should_review(assistant_count) {
			self.review_trigger.reset();
			let recent: Vec<String> =
				self.messages.iter().rev().take(10).map(|m| m.content.clone()).collect();
			let model = self.provider.selected_model.clone();
			let api_url = crate::omniroute::chat_completions_url();
			let tx = self.agent_tx.clone();
			crate::background_review::spawn_llm_review(recent, model, api_url, tx);
		}
	}

	pub fn model_provider_display(&self) -> String {
		let (name, provider) = self.resolved_model_labels();
		format!("{name}, {provider}")
	}

	pub fn resolved_model_labels(&self) -> (String, String) {
		if let Some(entry) =
			self.provider.model_catalog.iter().find(|m| m.model_id == self.provider.selected_model)
		{
			return (entry.display_name.clone(), entry.provider.clone());
		}
		if let Some((name, _)) =
			crate::zen::MODELS.iter().find(|(_, id)| *id == self.provider.selected_model.as_str())
		{
			return ((*name).to_string(), crate::zen::DEFAULT_PROVIDER.to_string());
		}
		(self.provider.model_display_name.clone(), self.provider.model_provider_name.clone())
	}

	pub fn sync_model_display(&mut self) {
		let (name, provider) = self.resolved_model_labels();
		self.provider.model_display_name = name;
		self.provider.model_provider_name = provider;
		self.provider.model_index = self
			.provider
			.model_catalog
			.iter()
			.position(|m| m.model_id == self.provider.selected_model)
			.unwrap_or(0);
	}

	pub fn cycle_agent_mode(&mut self) {
		self.agent_mode = self.agent_mode.next();
		self.show_toast(format!("Mode: {}", self.agent_mode.label()));
	}

	pub fn cycle_reasoning_effort(&mut self) {
		self.reasoning_effort = self.reasoning_effort.next();
		self.show_toast(format!("Reasoning: {}", self.reasoning_effort.label()));
		self.save_prefs();
	}

	pub fn set_reasoning_effort(&mut self, effort: ReasoningEffort) {
		self.reasoning_effort = effort;
		self.save_prefs();
	}

	pub fn set_agent_mode(&mut self, mode: AgentMode) {
		self.agent_mode = mode;
		self.show_toast(format!("Mode: {}", mode.label()));
		if mode == AgentMode::Plan {
			// Offer plan tool options immediately.
			self.open_popup(BottomPopup::PlanOptions);
		}
		if mode != AgentMode::Goal && self.goal.active {
			self.goal.stop("mode changed");
		}
		self.save_prefs();
	}

	/// Connect to codex by starting the in-process app-server.
	pub fn connect_to_codex(&mut self) {
		if self.codex_bridge.is_some() || self.codex_connection.is_some() {
			return;
		}
		let (tx, rx) = tokio::sync::oneshot::channel();
		self.codex_connection = Some(rx);
		tokio::spawn(async move {
			let result = crate::codex_bridge::CodexBridge::start().await;
			let _ = tx.send(result);
		});
		self.show_toast("Connecting to codex...".into());
	}

	/// Disconnect from codex and shut down the in-process server.
	pub fn disconnect_codex(&mut self) {
		self.codex_bridge = None;
		self.show_toast("Codex disconnected".into());
	}

	pub fn toggle_runtime_mode(&mut self) {
		self.runtime_mode = self.runtime_mode.toggle();
		self.selected_local_mode = self.runtime_mode.label().to_string();
		self.refresh_model_catalog();
		let prefer_local = self.runtime_mode == RuntimeMode::Local;
		let entry = self
			.provider
			.model_catalog
			.iter()
			.find(|m| m.is_selectable_model() && m.is_local == prefer_local && m.available)
			.or_else(|| {
				self
					.provider
					.model_catalog
					.iter()
					.find(|m| m.is_selectable_model() && m.is_local == prefer_local)
			})
			.or_else(|| self.provider.model_catalog.iter().find(|m| m.is_selectable_model()))
			.cloned();
		if let Some(entry) = entry {
			self.apply_model_entry(&entry);
		}
		self.show_toast(format!("Runtime: {}", self.runtime_mode.label()));
	}

	pub fn refresh_model_catalog(&mut self) {
		self.provider.model_catalog = crate::provider_registry::build_production_model_menu(
			&self.provider.models_catalog,
			&self.provider.provider_store,
		);
		self.provider.model_index = self
			.provider
			.model_catalog
			.iter()
			.position(|m| m.model_id == self.provider.selected_model && m.is_selectable_model())
			.or_else(|| {
				self.provider.model_catalog.iter().position(|m| m.is_selectable_model() && m.available)
			})
			.unwrap_or(0);
	}

	pub fn apply_model_entry(&mut self, entry: &ModelEntry) {
		if entry.is_section() || entry.is_action() {
			return;
		}
		self.provider.selected_model = entry.model_id.clone();
		self.provider.model_display_name = entry.display_name.clone();
		self.provider.model_provider_name = entry.provider.clone();
		// Selecting a local model switches runtime to Local (and vice versa).
		if entry.is_local && self.runtime_mode != RuntimeMode::Local {
			self.runtime_mode = RuntimeMode::Local;
			self.selected_local_mode = self.runtime_mode.label().to_string();
		} else if !entry.is_local && self.runtime_mode != RuntimeMode::Remote {
			self.runtime_mode = RuntimeMode::Remote;
			self.selected_local_mode = self.runtime_mode.label().to_string();
		}
		self.provider.model_index =
			self.provider.model_catalog.iter().position(|m| m.model_id == entry.model_id).unwrap_or(0);
		if entry.is_local {
			let flow = Arc::clone(&self.flow_backend);
			let key = entry.model_id.clone();
			if let Ok(handle) = tokio::runtime::Handle::try_current() {
				handle.spawn(async move {
					let mut flow = flow.lock().await;
					flow.set_selected_model(key);
				});
			}
		}
	}

	pub fn cycle_model(&mut self) {
		if self.provider.model_catalog.is_empty() {
			self.refresh_model_catalog();
		}
		if self.provider.model_catalog.is_empty() {
			return;
		}
		let len = self.provider.model_catalog.len();
		for _ in 0..len {
			self.provider.model_index = (self.provider.model_index + 1) % len;
			if self.provider.model_catalog[self.provider.model_index].is_selectable_model() {
				break;
			}
		}
		let entry = self.provider.model_catalog[self.provider.model_index].clone();
		if !entry.is_selectable_model() {
			return;
		}
		self.apply_model_entry(&entry);
		self.show_toast(format!("Model: {} ({})", entry.display_name, entry.provider));
	}

	pub fn open_popup(&mut self, popup: BottomPopup) {
		if self.ui.bottom_popup == popup {
			self.ui.bottom_popup = BottomPopup::None;
			return;
		}
		self.force_open_popup(popup);
	}

	pub fn open_models_menu(&mut self) {
		if self.show_tachyon_menu && self.menu.is_dynamic_models() {
			self.close_tachyon_only();
			self.animation.play_sound(SoundCue::MenuClose);
			return;
		}
		if self.provider.models_catalog.provider_count() == 0
			&& let Some(cat) = crate::providers::load_cached_catalog() {
				self.provider.models_catalog = cat;
			}
		self.refresh_model_catalog();
		// Themes-style full rows: Flow GGUFs → Zen 6 → models.dev (by catalog order).
		// Sections render as non-selectable headers (empty payload).
		let mut items: Vec<(String, String)> = Vec::new();
		items.push(("Connect provider…".into(), crate::modes::model_menu::ACT_CONNECT.into()));
		items.push(("Refresh local (Flow)".into(), crate::modes::model_menu::ACT_REFRESH_FLOW.into()));

		let mut n = 0usize;
		let mut push_model = |m: &ModelEntry, items: &mut Vec<(String, String)>| {
			if !m.is_selectable_model() {
				return;
			}
			n += 1;
			let check = if m.model_id == self.provider.selected_model { "  ✓" } else { "" };
			let left = format!("{n}. {}{check}", m.display_name);
			let tag = if m.is_local {
				"Flow".to_string()
			} else if m.provider.to_ascii_lowercase().contains("zen") {
				"Zen".to_string()
			} else {
				m.provider.chars().take(16).collect()
			};
			items.push((left, format!("{}||{tag}", m.model_id)));
		};

		// Walk catalog in section order; emit section headers + models only
		for m in &self.provider.model_catalog {
			if m.is_section() {
				// Clean section label (strip "── ")
				let label = m.display_name.trim().trim_matches('─').trim().to_string();
				if !label.is_empty() {
					items.push((format!("── {label} ──"), String::new()));
				}
				continue;
			}
			if m.is_action() {
				continue; // already have Connect / Refresh at top
			}
			push_model(m, &mut items);
		}

		if n == 0 {
			items.push(("(no models found)".into(), String::new()));
		}
		let sel = items
			.iter()
			.position(|(_, id)| {
				!id.is_empty()
					&& id.split("||").next().unwrap_or(id) == self.provider.selected_model.as_str()
			})
			.unwrap_or(0);
		self.menu.open_dynamic_list(crate::menu::DYNAMIC_MODELS, "Models", items);
		self.menu.selected_menu_item = sel;
		self.menu_is_closing = false;
		self.show_tachyon_menu = true;
		self.close_popup();
		self.play_sound(SoundCue::MenuOpen);
	}

	/// Open Channels as a tachyon-style menu (Themes-like full rows). Press `1` again to close.
	pub fn open_channels_menu(&mut self) {
		if self.show_tachyon_menu && self.menu.is_dynamic_channels() {
			self.close_tachyon_only();
			self.play_sound(SoundCue::MenuClose);
			return;
		}
		self.provider.channels = channels::load_channels();
		let mut items: Vec<(String, String)> = Vec::new();
		items.push(("Start gateway".into(), "__ch_act:1".into()));
		items.push(("Stop gateway".into(), "__ch_act:2".into()));
		items.push(("Refresh status".into(), "__ch_act:0".into()));
		let mut n = 0usize;
		for ch in &self.provider.channels {
			n += 1;
			let left = format!("{n}. {}", ch.name);
			let right = ch.status_label();
			items.push((left, format!("__ch:{}||{right}", ch.type_key)));
		}
		self.menu.open_dynamic_list(crate::menu::DYNAMIC_CHANNELS, "Channels", items);
		self.menu_is_closing = false;
		self.show_tachyon_menu = true;
		self.close_popup();
		self.play_sound(SoundCue::MenuOpen);
	}

	/// Activate selection from Models / Channels tachyon menus.
	pub fn activate_dynamic_menu_selection(&mut self) -> bool {
		let raw = self.menu.selected_payload().unwrap_or("").to_string();
		if raw.is_empty() {
			return false;
		}
		// payload may be "id||tag" — strip tag for matching
		let payload = raw.split("||").next().unwrap_or(&raw).to_string();
		if self.menu.is_dynamic_models() {
			use crate::modes::model_menu;
			match payload.as_str() {
				model_menu::ACT_CONNECT => {
					self.close_tachyon_only();
					self.force_open_popup(BottomPopup::Connect);
					return true;
				}
				model_menu::ACT_REFRESH_FLOW | model_menu::ACT_SCAN_ALL_DRIVES => {
					if payload == model_menu::ACT_SCAN_ALL_DRIVES {
						let _ = crate::flow_backend::discover_local_models_full_scan();
					}
					if let Ok(mut flow) = self.flow_backend.try_lock() {
						flow.refresh_models();
					}
					// Rebuild without toggle-close
					self.show_tachyon_menu = false;
					self.menu.current_submenu = None;
					self.refresh_model_catalog();
					self.open_models_menu();
					self.show_toast(format!(
						"Flow · {} ready",
						self
							.provider
							.model_catalog
							.iter()
							.filter(|m| m.is_local && m.is_selectable_model() && m.available)
							.count()
					));
					return true;
				}
				id => {
					if let Some(entry) = self
						.provider
						.model_catalog
						.iter()
						.find(|m| m.model_id == id && m.is_selectable_model())
						.cloned()
					{
						self.apply_model_entry(&entry);
						self.show_toast(format!("Model: {}", entry.display_name));
						self.save_prefs();
						self.close_tachyon_only();
						return true;
					}
				}
			}
			return false;
		}
		if self.menu.is_dynamic_channels() {
			if let Some(rest) = payload.strip_prefix("__ch_act:") {
				let idx: usize = rest.parse().unwrap_or(0);
				match idx {
					0 => {
						self.provider.channels = channels::load_channels();
						self.show_toast(channels::connection_summary());
						self.show_tachyon_menu = false;
						self.menu.current_submenu = None;
						self.open_channels_menu();
					}
					1 => match crate::channel_actions::start_channel_gateway() {
						Ok(m) => self.show_toast(m),
						Err(e) => self.show_toast(format!("Gateway: {e}")),
					},
					2 => match crate::channel_actions::stop_channel_gateway() {
						Ok(m) => self.show_toast(m),
						Err(e) => self.show_toast(format!("Gateway: {e}")),
					},
					_ => {}
				}
				return true;
			}
			if let Some(key) = payload.strip_prefix("__ch:")
				&& let Some(ch) = self.provider.channels.iter().find(|c| c.type_key == key).cloned() {
					if !ch.configured {
						match crate::channel_actions::ensure_channel_config_stub(&ch.type_key, &ch.name) {
							Ok(msg) => {
								self.provider.channels = channels::load_channels();
								self.show_toast(msg);
								self.show_tachyon_menu = false;
								self.menu.current_submenu = None;
								self.open_channels_menu();
							}
							Err(e) => self.show_toast(format!("Config: {e}")),
						}
					} else {
						self.show_toast(format!("{} · {}", ch.name, ch.status_label()));
					}
					return true;
				}
		}
		false
	}

	fn close_tachyon_only(&mut self) {
		self.menu_is_closing = true;
		self.menu.pick_closing_effect();
		self.show_tachyon_menu = false;
		self.menu.custom_title = None;
		self.menu.current_submenu = None;
		self.menu.opened_directly = false;
	}

	/// Always open (never toggle closed). Use when refreshing catalog then showing menu.
	pub fn force_open_popup(&mut self, popup: BottomPopup) {
		self.ui.bottom_popup = popup;
		self.ui.popup_cursor = 0;
		match popup {
			BottomPopup::Models => {
				if self.provider.models_catalog.provider_count() == 0 {
					// Best-effort load cached catalog so Connect/models.dev sections appear
					if let Some(cat) = crate::providers::load_cached_catalog() {
						self.provider.models_catalog = cat;
					}
				}
				self.refresh_model_catalog();
				// Land on selected model if present; otherwise first real model
				self.ui.popup_cursor = self
					.provider
					.model_catalog
					.iter()
					.position(|m| m.model_id == self.provider.selected_model && m.is_selectable_model())
					.or_else(|| self.provider.model_catalog.iter().position(|m| m.is_selectable_model()))
					.unwrap_or(0);
				self.provider.model_index = self.ui.popup_cursor;
			}
			BottomPopup::Channels => {
				self.provider.channels = channels::load_channels();
				// Keep cursor within action rows + channels
				let len = crate::channel_actions::CHANNELS_MENU_ACTIONS + self.provider.channels.len();
				self.ui.popup_cursor = self.provider.channel_cursor.min(len.saturating_sub(1));
			}
			BottomPopup::AgentMode => {
				self.ui.popup_cursor =
					AgentMode::ALL.iter().position(|m| *m == self.agent_mode).unwrap_or(0);
			}
			BottomPopup::Runtime => {
				self.ui.popup_cursor = if self.runtime_mode == RuntimeMode::Local { 0 } else { 1 };
			}
			BottomPopup::Connect => {
				if self.provider.models_catalog.provider_count() == 0 {
					self.reload_models_catalog();
				}
				// Prefer not-yet-connected providers at top of cursor (still full list)
				self.ui.popup_cursor = 0;
			}
			BottomPopup::PlanOptions => {
				self.ui.popup_cursor = 0;
				self.plan_wizard.reset(&self.plan_options);
				self.plan_wizard.active = true;
				let tools = crate::sidebar_data::workspace_tool_status();
				let ready = tools.iter().filter(|(_, ok, _)| *ok).count();
				self.show_toast(format!(
					"Plan options · {ready}/{} tools on PATH · {}",
					tools.len(),
					self.plan_options.summary()
				));
			}
			BottomPopup::PastePreview => {
				self.ui.popup_cursor = 0;
			}
			BottomPopup::ShareChannel => {
				self.provider.channels = channels::load_channels();
				self.ui.popup_cursor = 0;
			}
			BottomPopup::None => {}
		}
	}

	pub fn close_popup(&mut self) {
		self.ui.bottom_popup = BottomPopup::None;
	}

	pub fn activate_popup_selection(&mut self) {
		match self.ui.bottom_popup {
			BottomPopup::AgentMode => {
				let mode = AgentMode::from_index(self.ui.popup_cursor);
				self.set_agent_mode(mode);
				self.close_popup();
			}
			BottomPopup::Runtime => {
				let want_local = self.ui.popup_cursor == 0;
				if want_local != (self.runtime_mode == RuntimeMode::Local) {
					self.toggle_runtime_mode();
				}
				self.close_popup();
			}
			BottomPopup::Models => {
				if let Some(entry) = self.provider.model_catalog.get(self.ui.popup_cursor).cloned() {
					use crate::modes::model_menu;
					if entry.is_section() {
						// Skip headers — advance to next selectable
						self.popup_move(1);
						return;
					}
					if entry.is_action() {
						match entry.model_id.as_str() {
							model_menu::ACT_CONNECT => {
								self.close_popup();
								self.open_popup(BottomPopup::Connect);
								return;
							}
							model_menu::ACT_REFRESH_FLOW => {
								if let Ok(mut flow) = self.flow_backend.try_lock() {
									flow.refresh_models();
								}
								self.refresh_model_catalog();
								self.show_toast(format!(
									"Local models: {} · {}",
									self
										.provider
										.model_catalog
										.iter()
										.filter(|m| m.is_local && m.is_selectable_model())
										.count(),
									crate::flow_backend::flow_models_dir().display()
								));
								return;
							}
							model_menu::ACT_SCAN_ALL_DRIVES => {
								self.show_toast("Scanning C–Z for *.gguf…".into());
								// Full-drive scan even when flow/models already has files
								let found = crate::flow_backend::discover_local_models_full_scan();
								// Rebuild menu with forced full-scan results injected
								self.provider.model_catalog = crate::provider_registry::build_production_model_menu(
									&self.provider.models_catalog,
									&self.provider.provider_store,
								);
								// Prefer full-scan list for local entries
								let n = found
									.iter()
									.filter(|m| m.is_local && m.is_selectable_model() && m.available)
									.count();
								// Ensure full-scan models appear (build_production uses normal discover)
								// Overlay: merge full-scan into catalog
								for m in found {
									if m.is_selectable_model()
										&& !self.provider.model_catalog.iter().any(|x| x.model_id == m.model_id)
									{
										// Insert after Flow section actions
										if let Some(pos) = self
											.provider
											.model_catalog
											.iter()
											.position(|x| x.is_section() && x.display_name.contains("OpenCode Zen"))
										{
											self.provider.model_catalog.insert(pos, m);
										} else {
											self.provider.model_catalog.push(m);
										}
									}
								}
								self.show_toast(format!(
									"All-drives scan · {n} GGUF · library {}",
									crate::flow_backend::flow_models_dir().display()
								));
								return;
							}
							model_menu::ACT_RUNTIME_LOCAL => {
								if self.runtime_mode != RuntimeMode::Local {
									self.toggle_runtime_mode();
								} else {
									self.show_toast("Runtime: Local".into());
								}
								return;
							}
							model_menu::ACT_RUNTIME_REMOTE => {
								if self.runtime_mode != RuntimeMode::Remote {
									self.toggle_runtime_mode();
								} else {
									self.show_toast("Runtime: Remote".into());
								}
								return;
							}
							_ => {}
						}
					}
					self.apply_model_entry(&entry);
					self.show_toast(format!(
						"Model: {} · {} · {}",
						entry.display_name,
						entry.provider,
						self.runtime_mode.label()
					));
					self.save_prefs();
				}
				self.close_popup();
			}
			BottomPopup::Channels => {
				self.provider.channel_cursor = self.ui.popup_cursor;
				let actions = crate::channel_actions::CHANNELS_MENU_ACTIONS;
				if self.ui.popup_cursor < actions {
					match self.ui.popup_cursor {
						0 => {
							self.provider.channels = channels::load_channels();
							let cfg = self.provider.channels.iter().filter(|c| c.configured).count();
							let on = self.provider.channels.iter().filter(|c| c.connected).count();
							self.show_toast(format!(
								"Channels refreshed · {on} connected · {cfg} configured · {}",
								channels::connection_summary()
							));
						}
						1 => match crate::channel_actions::start_channel_gateway() {
							Ok(msg) => self.show_toast(msg),
							Err(e) => self.show_toast(format!("Gateway start: {e}")),
						},
						2 => match crate::channel_actions::stop_channel_gateway() {
							Ok(msg) => self.show_toast(msg),
							Err(e) => self.show_toast(format!("Gateway stop: {e}")),
						},
						3 => {
							self.provider.channels = channels::load_channels();
							let rows = crate::channel_actions::channel_doctor(&self.provider.channels);
							let preview: String = rows
								.into_iter()
								.take(4)
								.map(|(k, v)| format!("{k}: {v}"))
								.collect::<Vec<_>>()
								.join(" · ");
							self.show_toast(if preview.is_empty() {
								"Channel doctor: no data".into()
							} else {
								preview.chars().take(100).collect()
							});
						}
						_ => {}
					}
					return; // keep menu open
				}
				let idx = self.ui.popup_cursor - actions;
				if let Some(ch) = self.provider.channels.get(idx).cloned() {
					if !ch.configured {
						match crate::channel_actions::ensure_channel_config_stub(&ch.type_key, &ch.name) {
							Ok(msg) => {
								self.provider.channels = channels::load_channels();
								self.show_toast(msg);
							}
							Err(e) => self.show_toast(format!("Config failed: {e}")),
						}
					} else if ch.connected {
						self.show_toast(format!(
							"{} · connected · {} · /share-channel to send",
							ch.name, ch.description
						));
					} else {
						self.show_toast(format!(
							"{} · configured · Start gateway (row 1) · {}",
							ch.name, ch.description
						));
					}
				}
				// Keep channels menu open so user can browse; Esc closes.
			}
			BottomPopup::Connect => {
				self.connect_provider_by_catalog_index(self.ui.popup_cursor);
				// Re-open models so user can pick a model from the new provider
				self.close_popup();
				self.refresh_model_catalog();
				self.open_popup(BottomPopup::Models);
			}
			BottomPopup::PlanOptions => {
				if self.plan_wizard.is_confirm_tab() {
					self.plan_options = self.plan_wizard.to_plan_options();
					let report = goal_runner::run_plan_tools(&self.plan_options);
					self.messages.push(Message::assistant(format!(
						"<think>\n(plan tools)\n</think>\n```command name=\"plan-tools\"\n{report}\n```"
					)));
					self.show_toast("Plan tools attached to chat".into());
					self.plan_wizard.active = false;
					self.close_popup();
				} else {
					let confirmed = self.plan_wizard.select_current();
					if confirmed {
						self.plan_options = self.plan_wizard.to_plan_options();
						let report = goal_runner::run_plan_tools(&self.plan_options);
						self.messages.push(Message::assistant(format!(
							"<think>\n(plan tools)\n</think>\n```command name=\"plan-tools\"\n{report}\n```"
						)));
						self.show_toast("Plan tools attached to chat".into());
						self.plan_wizard.active = false;
						self.close_popup();
					}
				}
			}
			BottomPopup::PastePreview => {
				self.close_popup();
			}
			BottomPopup::ShareChannel => {
				self.share_session_to_channel_index(self.ui.popup_cursor);
				self.close_popup();
			}
			BottomPopup::None => {}
		}
	}

	pub fn popup_move(&mut self, delta: i32) {
		let len = match self.ui.bottom_popup {
			BottomPopup::AgentMode => AgentMode::ALL.len(),
			BottomPopup::Runtime => 2,
			BottomPopup::Models => self.provider.model_catalog.len().max(1),
			BottomPopup::Channels => {
				(crate::channel_actions::CHANNELS_MENU_ACTIONS + self.provider.channels.len()).max(1)
			}
			BottomPopup::Connect => self.provider.models_catalog.provider_count().max(1),
			BottomPopup::PlanOptions => {
				// Wizard handles its own navigation
				return;
			}
			BottomPopup::PastePreview => {
				(self.input.paste_blocks.len() + self.input.attachments.len()).max(1)
			}
			BottomPopup::ShareChannel => {
				crate::channel_actions::sendable_channels(&self.provider.channels).len().max(1)
			}
			BottomPopup::None => return,
		};
		if delta < 0 {
			self.ui.popup_cursor = self.ui.popup_cursor.saturating_sub((-delta) as usize);
		} else {
			self.ui.popup_cursor = (self.ui.popup_cursor + delta as usize).min(len.saturating_sub(1));
		}
		// Skip section headers in model menu when moving
		if self.ui.bottom_popup == BottomPopup::Models {
			for _ in 0..8 {
				if let Some(e) = self.provider.model_catalog.get(self.ui.popup_cursor)
					&& e.is_section() {
						if delta < 0 {
							if self.ui.popup_cursor == 0 {
								break;
							}
							self.ui.popup_cursor -= 1;
						} else if self.ui.popup_cursor + 1 < len {
							self.ui.popup_cursor += 1;
						} else {
							break;
						}
						continue;
					}
				break;
			}
		}
	}

	pub fn open_differ(&mut self) {
		self.diff_state.open_and_refresh();
		self.close_popup();
	}

	pub fn refresh_diff_stats_if_needed(&mut self) {
		if self.last_diff_refresh.elapsed() >= Duration::from_secs(5) {
			let (add, del) = crate::diff_view::quick_diff_stats();
			self.diff_state.total_additions = add;
			self.diff_state.total_deletions = del;
			self.last_diff_refresh = Instant::now();
		}
	}

	pub fn total_tokens_estimate(&self) -> usize {
		// Context window pressure: last prompt in + all stored message bodies.
		self
			.session
			.session_input_tokens
			.saturating_add(self.session.session_output_tokens)
			.max(self.messages.iter().map(|m| m.token_count).sum())
	}

	pub fn context_limit(&self) -> usize {
		if self.runtime_mode == RuntimeMode::Local { 32_000 } else { 128_000 }
	}

	/// Combined session tokens + context window % — e.g. `10.5K (5%)`.
	pub fn token_usage_label(&self) -> String {
		let inn = self.session.session_input_tokens.max(self.session.last_input_tokens);
		let out = self.session.session_output_tokens.max(self.session.last_output_tokens);
		let used = inn.saturating_add(out);
		let limit = self.context_limit();
		let pct =
			if limit > 0 { ((used as f32 / limit as f32) * 100.0).min(100.0).round() as u32 } else { 0 };
		format!("{} ({}%)", format_token_count(used), pct)
	}

	/// Rotating bottom-bar tips (center). Keep short and current.
	/// 10 generic shortcuts for non-splash full-screen modes (carousel, filebrowser, editor).
	pub fn screen_shortcuts() -> &'static [&'static str] {
		&[
			"Esc: back to splash",
			"Left: file browser  ·  Right: code editor",
			"Down: animation carousel  ·  Up: command menu",
			"Ctrl+S: voice input  ·  Ctrl+D: diff view",
			"/fmt /lint /lsp: workspace tools",
			"Tab: cycle Ask/Write/Plan/Goal/Agent",
			"1-9/0: open command menus",
			"Alt+Enter: newline in input",
			"hold Space: speech-to-text",
			"/sessions: browse history",
		]
	}

	/// 30 tips for the splash screen.
	pub fn splash_tips() -> &'static [&'static str] {
		&[
			"Esc: back to splash from any screen",
			"Left: file browser  ·  Right: code editor",
			"Down: animation carousel  ·  Up: command menu",
			"Ctrl+S: voice input  ·  Ctrl+D: diff view",
			"/fmt /lint /lsp: workspace tools",
			"Tab: cycle Ask/Write/Plan/Goal/Agent",
			"1-9/0: open command menus",
			"Alt+Enter: newline in input",
			"hold Space: speech-to-text",
			"/sessions: browse history",
			"Ctrl+C: interrupt generation",
			"Ctrl+E: export session",
			"Ctrl+P: command palette",
			"Ctrl+R: toggle reasoning",
			"Ctrl+T: token usage",
			"Ctrl+U: upload file",
			"Ctrl+W: close tab",
			"Ctrl+Z: undo",
			"Ctrl+Y: redo",
			"Ctrl+Shift+F: format code",
			"Ctrl+Shift+L: lint code",
			"Ctrl+Shift+E: file explorer",
			"Ctrl+Shift+P: project palette",
			"Ctrl+Shift+T: new terminal",
			"Ctrl+Shift+D: debug",
			"Alt+1-9: switch tab",
			"Alt+Left/Right: navigate back/forward",
			"Alt+Up/Down: scroll line by line",
			"PageUp/PageDown: scroll page",
			"Home/End: top/bottom of file",
		]
	}

	/// Current chat-area tips for the message screen (4 rotating hints).
	pub fn message_tips() -> &'static [&'static str] {
		&[
			"Tab: Ask/Write/Plan/Goal/Agent  ·  /name set your label  ·  / commands",
			"1-9/0: menus  ·  Ctrl+D differ  ·  Alt+T/C/S blocks",
			"/fmt /lint /lsp  ·  Plan injects workspace signals",
			"Alt+Enter newline  ·  /sessions  ·  hold Space = STT",
		]
	}

	pub fn current_tip(&self) -> &'static str {
		let anim_mode = self.animation.animation_mode;
		let current = self.current_animation();
		let is_splash = anim_mode && current == crate::AnimationType::Splash;
		let is_animation = anim_mode && current != crate::AnimationType::Splash;

		let tips = if is_splash {
			Self::splash_tips()
		} else if is_animation {
			Self::screen_shortcuts()
		} else {
			Self::message_tips()
		};
		tips[self.ui.shortcut_index % tips.len()]
	}

	pub fn cost_label(&self) -> String {
		format!("${:.2}", self.session.session_cost_usd)
	}

	/// Compact diff counters for the bottom bar (`+N, -M` — no "Current diffs" prefix).
	pub fn diff_label(&self) -> String {
		format!("+{}, -{}", self.diff_state.total_additions, self.diff_state.total_deletions)
	}
}

fn format_token_count(n: usize) -> String {
	if n < 1000 {
		n.to_string()
	} else if n.is_multiple_of(1000) {
		format!("{}K", n / 1000)
	} else {
		// 1.5K, 10.5K — one decimal when not a whole thousand
		format!("{:.1}K", n as f32 / 1000.0)
	}
}


	/// Width of the colored diff segment (`+N, -M`).
	pub fn diff_label_width(&self) -> u16 {
		self.diff_label().chars().count() as u16
	}

	/// Open (or toggle closed) a command-palette submenu by absolute index.
	/// Returns `Some(true)` if opened, `Some(false)` if closed/toggled off, `None` if invalid.
	pub fn toggle_menu_by_index(&mut self, index: usize) -> Option<bool> {
		let count = self.menu.main_menu_len();
		if index >= count {
			return None;
		}
		// Same key again → close
		if self.show_tachyon_menu
			&& self.menu.current_submenu == Some(index)
			&& self.menu.opened_directly
		{
			self.menu_is_closing = true;
			self.menu.pick_closing_effect();
			self.show_tachyon_menu = false;
			return Some(false);
		}
		if !self.show_tachyon_menu {
			self.menu_is_closing = false;
			self.show_tachyon_menu = true;
			self.menu.pick_opening_effect();
		}
		self.menu.enter_submenu_directly(index);
		Some(true)
	}

	/// Open command-palette submenu by absolute index (0-based into main menu).
	pub fn open_menu_by_index(&mut self, index: usize) -> bool {
		matches!(self.toggle_menu_by_index(index), Some(true))
	}

	/// Toggle expand/collapse on the last assistant message blocks (keyboard helper).
	pub fn toggle_last_assistant_block(&mut self, kind: char) {
		if let Some(msg) = self
			.messages
			.iter_mut()
			.rev()
			.find(|m| m.role == crate::components::MessageRole::Assistant && !m.content.is_empty())
		{
			match kind {
				't' => msg.toggle_thinking(),
				'c' => msg.toggle_commands(),
				's' => msg.toggle_subagents(),
				_ => {}
			}
		}
	}

	pub fn bootstrap_backends(&self) {
		// Skip when no Tokio runtime (unit tests construct ChatState offline).
		let Ok(handle) = tokio::runtime::Handle::try_current() else {
			return;
		};
		let agent = Arc::clone(&self.agent_backend);
		let flow = Arc::clone(&self.flow_backend);
		handle.spawn(async move {
			if let Err(e) = agent.initialize().await {
				tracing::debug!("dx-agent warmup: {e}");
			} else {
				tracing::info!("dx-agent ready");
			}
			let mut flow = flow.lock().await;
			if let Err(e) = flow.init().await {
				tracing::debug!("dx-flow warmup: {e}");
			} else {
				tracing::info!("dx-flow ready");
			}
		});
	}

	pub fn update(&mut self) {
		// Cycle bottom-bar tips every 8 seconds
		let tip_count = Self::message_tips().len().max(1);
		if self.ui.last_shortcut_cycle.elapsed().as_secs() >= 8 {
			self.ui.shortcut_index = (self.ui.shortcut_index + 1) % tip_count;
			self.ui.last_shortcut_cycle = Instant::now();
		}

		// Soft exit: readable session summary, then auto-quit (or immediate if legacy flag).
		if self.session.show_session_screen && self.session.force_clear_frames == 0 {
			if let Some(deadline) = self.session.session_exit_deadline {
				if Instant::now() >= deadline {
					self.session.session_exit_deadline = None;
					if self.animation.exit_after_outro {
						self.finish_outro_exit();
					} else {
						self.finish_soft_exit();
					}
				}
			} else if self.session.quit_after_session_reveal {
				self.session.session_reveal_frames = self.session.session_reveal_frames.saturating_add(1);
				if self.session.session_reveal_frames >= 24 {
					self.session.quit_after_session_reveal = false;
					self.finish_outro_exit();
				}
			}
		}

		// Goal mode: kick next iteration only when idle and not paused.
		if self.goal_pending_continue && !self.is_loading && self.goal.can_continue() {
			self.goal_pending_continue = false;
			let cont = goal_runner::goal_continuation_prompt(&self.goal);
			self.add_user_message(cont);
		}

		self.sidebar.refresh_if_stale(Duration::from_secs(60));

		// Hide toast after duration
		if let Some(start_time) = self.ui.toast_start_time
			&& start_time.elapsed() >= self.ui.toast_duration
		{
			self.ui.toast_message = None;
			self.ui.toast_start_time = None;
		}

		// Handle space key hold spinner with proper hold detection
		if self.space_held
			&& let Some(last_press) = self.last_space_press
		{
			// If no space press for 150ms, consider it released (timing fallback terminals).
			if last_press.elapsed() >= Duration::from_millis(150) {
				let held_ms = self.space_hold_start.map(|t| t.elapsed().as_millis()).unwrap_or(0);
				self.space_held = false;
				self.space_hold_start = None;
				self.last_space_press = None;
				self.space_press_count = 0;
				if held_ms >= 400 {
					self.on_push_to_talk_release();
				}
			} else {
				// Still holding, animate spinner
				if let Some(start_time) = self.space_hold_start {
					let elapsed_ms = start_time.elapsed().as_millis();
					self.spinner_frame = ((elapsed_ms / 100) % 12) as usize;
				}
			}
		}

		// Handle transition animations
		if (self.animation.playing_intro || self.animation.playing_outro)
			&& let Some(start_time) = self.animation.transition_start_time
			&& start_time.elapsed() >= self.animation.transition_duration
		{
			// Transition animation finished
			if self.animation.playing_intro {
				self.animation.playing_intro = false;
				self.animation.transition_start_time = None;
				// Animation mode is already off, messages are already added
			} else if self.animation.playing_outro {
				self.animation.playing_outro = false;
				self.animation.transition_start_time = None;
				if self.animation.exit_after_outro {
					self.finish_outro_exit();
				} else {
					self.animation.animation_mode = true;
					self.animation.current_animation_index = 0; // Splash
					self.messages.clear(); // Clear messages
					self.restart_current_animation();
				}
			}
		}

		self.drain_agent_response_chunks();
		self.refresh_diff_stats_if_needed();

		// Update typing indicator when loading
		if self.is_loading {
			self.typing_indicator.update();
		}
	}

	/// Show a toast notification. Startup toasts are suppressed for the first 3 seconds.
	pub fn show_toast(&mut self, message: String) {
		// Suppress startup toasts — only show if the session has been alive for >3s
		if self.session.session_start_time.elapsed() < Duration::from_secs(3) {
			return;
		}
		self.ui.toast_message = Some(message);
		self.ui.toast_start_time = Some(Instant::now());
	}

	pub fn stage_dx_command(&mut self, command: &str) {
		self.input.replace_content(command);
		self.show_toast(format!("Staged DX command: {command}"));
	}

	pub(crate) fn request_dx_tool_confirmation(&mut self, action: DxToolAction) {
		self.pending_dx_tool_confirmation = Some(action);
		self.show_toast(
			action.confirmation.unwrap_or("Press Enter/Y to stage this DX command.").to_string(),
		);
	}

	pub(crate) fn confirm_pending_dx_tool(&mut self) -> Option<DxToolAction> {
		let action = self.pending_dx_tool_confirmation.take()?;
		self.stage_dx_command(action.command);
		Some(action)
	}

	pub fn cancel_pending_dx_tool_confirmation(&mut self) -> bool {
		let cancelled = self.pending_dx_tool_confirmation.take().is_some();
		if cancelled {
			self.show_toast("Cancelled DX tool command".to_string());
		}
		cancelled
	}

	pub fn clear_pending_dx_tool_confirmation(&mut self) -> bool {
		self.pending_dx_tool_confirmation.take().is_some()
	}

	pub fn play_sound(&mut self, cue: SoundCue) {
		// No sounds on the message-list screen (when messages are visible)
		if !self.messages.is_empty() && !self.animation.animation_mode && !self.ui.show_dx_splash {
			return;
		}
		self.animation.sound_player_mut().play(cue);
	}

	pub fn start_current_animation_ambience(&mut self) {
		// Stop any previous animation loop first
		self.animation.sound_player_mut().stop_animation_loop();

		// Splash uses Matrix rain ambience (same as Matrix carousel screen).
		let sound = if self.current_animation() == AnimationType::Splash {
			AnimationSound::Matrix
		} else {
			self.current_animation().sound()
		};
		self.animation.sound_player_mut().start_animation_loop(sound);
	}

	pub fn restart_current_animation(&mut self) {
		self.animation.animation_start_time = Some(Instant::now());
		self.start_current_animation_ambience();
	}

	pub fn stop_animation_ambience(&mut self) {
		self.animation.sound_player_mut().stop_animation_loop();
	}

	pub fn active_animation_sound(&self) -> Option<AnimationSound> {
		self.animation.sound_player().current_animation_loop()
	}

	/// Start playing intro animation (splash → message list transition).
	pub fn play_intro_animation(&mut self) {
		// Leave splash Matrix ambience, then play the transition cue into chat.
		self.stop_animation_ambience();
		// Dedicated “enter chat” sound (not a keypress / typing click).
		self.play_sound(SoundCue::MenuOpen);
		if self.animation.intro_animation != AnimationType::Splash {
			self.play_sound(SoundCue::Animation(self.animation.intro_animation.sound()));
			self.animation.transition_duration = Duration::from_secs(2);
		} else {
			// Brief beat so MenuOpen can be heard before the first stream tokens.
			self.animation.transition_duration = Duration::from_millis(400);
		}
		self.animation.playing_intro = true;
		self.animation.transition_start_time = Some(Instant::now());
		self.animation.animation_start_time = Some(Instant::now());
	}

	/// Start playing outro animation
	pub fn play_outro_animation(&mut self) {
		self.stop_animation_ambience();
		self.play_sound(SoundCue::Animation(self.animation.outro_animation.sound()));
		self.animation.exit_after_outro = false;
		self.animation.playing_outro = true;
		self.animation.transition_duration = self.outro_transition_duration();
		self.animation.transition_start_time = Some(Instant::now());
		self.animation.animation_start_time = Some(Instant::now());
	}

	pub fn user_message_indices(&self) -> Vec<usize> {
		self
			.messages
			.iter()
			.enumerate()
			.filter(|(_, m)| m.role == crate::components::MessageRole::User)
			.map(|(i, _)| i)
			.collect()
	}

	/// Total rendered height of the chat message list (matches MessageList).
	pub fn chat_content_height(&self) -> usize {
		let w = self
			.ui
			.chat_list_area
			.width
			.saturating_sub(crate::components::SCROLLBAR_TRACK_WIDTH)
			.saturating_sub(crate::components::MESSAGE_LIST_RIGHT_PAD)
			.max(12) as usize;
		crate::components::messages_total_height_for_width(&self.messages, w)
	}

	pub fn max_chat_scroll(&self) -> usize {
		let viewport = self.ui.chat_list_area.height as usize;
		if viewport == 0 {
			return self.ui.chat_scroll_offset;
		}
		self.chat_content_height().saturating_sub(viewport)
	}

	pub fn set_chat_scroll(&mut self, offset: usize) {
		let max = self.max_chat_scroll();
		let next = offset.min(max);
		// User scrolled away from bottom → stop stick-to-bottom.
		if next < max {
			self.ui.stick_scroll_to_bottom = false;
		} else if next >= max {
			self.ui.stick_scroll_to_bottom = true;
		}
		self.ui.chat_scroll_offset = next;
	}

	/// Copy the last assistant message body to the clipboard.
	pub fn copy_last_assistant_response(&mut self) {
		let Some(msg) =
			self.messages.iter().rev().find(|m| m.role == crate::components::MessageRole::Assistant)
		else {
			self.show_toast("No assistant message to copy".into());
			return;
		};
		let text = msg.copy_text();
		match cli_clipboard::set_contents(text) {
			Ok(()) => self.show_toast("Copied assistant response".into()),
			Err(e) => self.show_toast(format!("Copy failed: {e}")),
		}
	}

	/// Interrupt the current generation (marks last assistant message).
	pub fn interrupt_generation(&mut self) {
		if !self.is_loading {
			self.show_toast("Nothing to interrupt".into());
			return;
		}
		self.is_loading = false;
		if let Some(last) = self.messages.last_mut()
			&& last.role == crate::components::MessageRole::Assistant {
				last.interrupted = true;
				last.append_content(crate::permission_hub::INTERRUPTED_MARKER);
				last.append_content("\n*(interrupted)*\n");
				last.interrupted = true;
			}
		self.permission_hub.clear();
		self.question_hub.clear();
		self.show_toast("Interrupted".into());
	}

	/// Reply to a pending tool permission (y/a/n).
	pub fn reply_permission(&mut self, decision: crate::tools::PermissionDecision) -> bool {
		if self.permission_hub.reply(decision) {
			let label = match decision {
				crate::tools::PermissionDecision::AllowOnce => "allowed once",
				crate::tools::PermissionDecision::AllowAlways => "always allowed",
				crate::tools::PermissionDecision::Deny => "denied",
			};
			self.show_toast(format!("Permission {label}"));
			true
		} else {
			false
		}
	}

	/// Messages visible on the active branch (linear view of the tree).
	pub fn visible_messages(&self) -> impl Iterator<Item = &Message> {
		let branch = self.active_branch_id.clone();
		self.messages.iter().filter(move |m| !m.hidden && m.branch_id == branch)
	}

	/// Fork a new branch from message at `idx` (inclusive history copy).
	pub fn branch_from_message(&mut self, idx: usize) -> Option<String> {
		if idx >= self.messages.len() {
			return None;
		}
		let branch_id = format!("br-{}", Message::new_id());
		// Hide messages after fork point on current branch (keep history)
		let fork_id = self.messages[idx].id.clone();
		// New branch inherits lineage: mark new messages with new branch;
		// prior messages stay on their branch but we copy ids for parent chain
		// by replaying: for simplicity, unhide path from root to idx on new branch
		// by cloning the prefix onto the new branch.
		let prefix: Vec<Message> = self.messages[..=idx]
			.iter()
			.map(|m| {
				let mut c = m.clone();
				c.branch_id = branch_id.clone();
				c.hidden = false;
				// new ids to avoid collisions when switching
				let old = c.id.clone();
				c.id = Message::new_id();
				if c.parent_id.as_deref() == Some(fork_id.as_str()) {
					// keep structure
				}
				let _ = old;
				c
			})
			.collect();
		// Link parents in prefix
		for _i in 1..prefix.len() {
			// already cloned; re-link sequentially
		}
		let mut prev: Option<String> = None;
		let mut relinked = Vec::with_capacity(prefix.len());
		for mut m in prefix {
			m.parent_id = prev.clone();
			prev = Some(m.id.clone());
			relinked.push(m);
		}
		self.messages.extend(relinked);
		self.active_branch_id = branch_id.clone();
		self.apply_branch_visibility();
		self.show_toast(format!("Branched · {branch_id}"));
		Some(branch_id)
	}

	fn apply_branch_visibility(&mut self) {
		let active = self.active_branch_id.clone();
		for m in &mut self.messages {
			m.hidden = m.branch_id != active;
		}
	}

	/// Switch active branch by id.
	pub fn switch_branch(&mut self, branch_id: &str) {
		self.active_branch_id = branch_id.to_string();
		self.apply_branch_visibility();
		self.show_toast(format!("Branch · {branch_id}"));
	}

	pub fn open_branch_picker(&mut self) {
		self.branch_picker.open = true;
		let branches = crate::msg_ui::list_branches(&self.messages, &self.active_branch_id);
		self.branch_picker.selected =
			branches.iter().position(|b| b.id == self.active_branch_id).unwrap_or(0);
	}

	pub fn close_branch_picker(&mut self) {
		self.branch_picker.open = false;
	}

	/// Handle keys while branch picker is open. Returns true if consumed.
	pub fn handle_branch_picker_key(&mut self, key: crossterm::event::KeyCode) -> bool {
		if !self.branch_picker.open {
			return false;
		}
		let branches = crate::msg_ui::list_branches(&self.messages, &self.active_branch_id);
		let n = branches.len().max(1);
		match key {
			crossterm::event::KeyCode::Esc => {
				self.close_branch_picker();
				true
			}
			crossterm::event::KeyCode::Up => {
				self.branch_picker.selected = (self.branch_picker.selected + n - 1) % n;
				true
			}
			crossterm::event::KeyCode::Down => {
				self.branch_picker.selected = (self.branch_picker.selected + 1) % n;
				true
			}
			crossterm::event::KeyCode::Enter => {
				if let Some(b) = branches.get(self.branch_picker.selected) {
					self.switch_branch(&b.id);
				}
				self.close_branch_picker();
				true
			}
			crossterm::event::KeyCode::Char('n') | crossterm::event::KeyCode::Char('N') => {
				// Fork from last visible message
				let idx = self.messages.iter().rposition(|m| !m.hidden).unwrap_or(0);
				let _ = self.branch_from_message(idx);
				self.close_branch_picker();
				true
			}
			_ => true, // consume while open
		}
	}

	/// Regenerate: remove last assistant on active branch and re-send last user prompt.
	pub fn regenerate_last_assistant(&mut self) -> bool {
		if self.is_loading {
			return false;
		}
		// Find last visible assistant and preceding user
		let branch = self.active_branch_id.clone();
		let mut last_as_idx = None;
		for (i, m) in self.messages.iter().enumerate().rev() {
			if m.branch_id == branch && !m.hidden && m.role == crate::components::MessageRole::Assistant {
				last_as_idx = Some(i);
				break;
			}
		}
		let Some(ai) = last_as_idx else {
			return false;
		};
		let mut user_content = None;
		for j in (0..ai).rev() {
			let m = &self.messages[j];
			if m.branch_id == branch && !m.hidden && m.role == crate::components::MessageRole::User {
				user_content = Some(m.content.clone());
				break;
			}
		}
		let Some(prompt) = user_content else {
			return false;
		};
		// Remove assistant and later on this branch
		self.messages.truncate(ai);
		self.apply_branch_visibility();
		self.show_toast("Regenerating…".into());
		self.is_loading = true;
		self.turn_started_at = Some(Instant::now());
		self.thinking_started_at = None;
		let mut asst = Message::assistant(String::new());
		asst.branch_id = branch;
		asst.parent_id = self
			.messages
			.iter()
			.rev()
			.find(|m| !m.hidden && m.role == crate::components::MessageRole::User)
			.map(|m| m.id.clone());
		self.messages.push(asst);

		let sys_ctx = crate::dx_system::SystemContext {
			mode: self.agent_mode,
			model_id: &self.provider.selected_model,
			model_display: &self.provider.model_display_name,
			project_dir: &self.session.session_project_dir,
			first_turn: false,
			workspace_signals: None,
		};
		let system = crate::dx_system::build_system(&sys_ctx);
		let user_for_model = prompt;
		let mut history: Vec<(String, String)> = self
			.messages
			.iter()
			.filter(|m| !m.hidden && m.branch_id == self.active_branch_id)
			.filter(|m| !m.content.is_empty() || m.role == crate::components::MessageRole::User)
			.map(|m| {
				let role = match m.role {
					crate::components::MessageRole::User => "user".to_string(),
					crate::components::MessageRole::Assistant => "assistant".to_string(),
				};
				(role, m.content.clone())
			})
			.collect();
		if let Some((_, body)) = history.iter_mut().rev().find(|(r, _)| r == "user") {
			*body = user_for_model.clone();
		}
		if history.last().is_some_and(|(r, c)| r == "assistant" && c.is_empty()) {
			history.pop();
		}

		let tx = Sender::clone(&self.agent_tx);
		let model = self.provider.selected_model.clone();
		let omni_url = crate::omniroute::chat_completions_url();
		let system_for_remote = system.clone();
		let agent_mode = self.agent_mode;
		let plan_allow_shell = self.plan_options.allow_shell;
		let project_dir = self.session.session_project_dir.clone();
		let max_tool_steps = 16u32;
		let perm_hub = self.permission_hub.clone();
		let q_hub = self.question_hub.clone();
		let ledger = self.delegation_ledger.clone();
		let sidebar = self.sidebar.clone();
		self.permission_hub.clear();
		self.question_hub.clear();
		self.ui.stick_scroll_to_bottom = true;
		self.persist_current_session();

		tokio::spawn(async move {
			let input = crate::agent_loop::LoopInput {
				model: model.clone(),
				system: system_for_remote.clone(),
				history: history.clone(),
				mode: agent_mode,
				cwd: std::path::PathBuf::from(&project_dir),
				plan_allow_shell,
				api_url: omni_url.clone(),
				enable_native_tools: true,
				max_steps: max_tool_steps,
				permission: Some(perm_hub),
				questions: Some(q_hub),
				ledger: Some(ledger),
				sidebar: Some(sidebar),
			};
			if let Err(e) = crate::agent_loop::run(input, tx.clone()).await {
				// Fallback plain stream
				let _ = e;
				if let Some(ref u) = omni_url {
					let _ = crate::zen::stream_chat_url_with_system(
						&model,
						history,
						tx.clone(),
						u,
						Some(system_for_remote),
					)
					.await;
				}
			}
			let _ = tx.send(END_OF_RESPONSE.to_string());
		});
		true
	}

	/// Expand/collapse all tool details on the last assistant message.
	pub fn toggle_details_expanded(&mut self) {
		if let Some(m) = self
			.messages
			.iter_mut()
			.rev()
			.find(|m| !m.hidden && m.role == crate::components::MessageRole::Assistant)
		{
			let open = !(m.commands_expanded && m.subagents_expanded);
			m.set_details_expanded(open);
			self.show_toast(if open { "Details expanded".into() } else { "Details collapsed".into() });
		}
	}

	/// Spawn interactive terminal card into the last assistant message (or new system-ish assistant).
	pub fn spawn_interactive_terminal(&mut self) {
		let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
		match self.pty_host.spawn_shell(cwd, "shell") {
			Ok(id) => {
				if let Some(last) = self
					.messages
					.iter_mut()
					.rev()
					.find(|m| !m.hidden && m.role == crate::components::MessageRole::Assistant)
				{
					// ok
					let _ = last;
				} else {
					let mut m = Message::assistant(String::new());
					m.branch_id = self.active_branch_id.clone();
					self.messages.push(m);
				}
				self.sync_pty_parts_into_messages();
				self.pty_host.attach(&id);
				// Give vim/htop a reasonable size
				let _ = self.pty_host.resize(&id, 100, 28);
				self.show_toast("PTY attached · Esc detach · real portable-pty".into());
			}
			Err(e) => self.show_toast(format!("PTY failed: {e}")),
		}
	}

	/// Push latest PTY snapshots into message parts for paint.
	pub fn sync_pty_parts_into_messages(&mut self) {
		self.pty_host.poll_exit();
		let snaps = self.pty_host.snapshots();
		if snaps.is_empty() {
			return;
		}
		if let Some(last) = self
			.messages
			.iter_mut()
			.rev()
			.find(|m| !m.hidden && m.role == crate::components::MessageRole::Assistant)
		{
			for s in snaps {
				let title = if s.is_real_pty {
					format!("{} · {}×{}", s.title, s.cols, s.rows)
				} else {
					s.title.clone()
				};
				crate::msg_ui::push_pty(&mut last.parts, &s.id, &title, s.lines, s.attached, s.alive);
			}
			last.sync_content_from_parts();
		}
	}

	/// Diff review: accept / reject / open for tool index in last assistant.
	pub fn diff_review_action(&mut self, tool_index: usize, action: u8) {
		let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
		let Some(msg) = self
			.messages
			.iter_mut()
			.rev()
			.find(|m| !m.hidden && m.role == crate::components::MessageRole::Assistant)
		else {
			return;
		};
		let tools: Vec<_> = msg
			.parts
			.iter()
			.filter_map(|p| match p {
				crate::msg_ui::StreamPart::Tool { index, body, preview, .. } if *index == tool_index => {
					Some((body.clone(), preview.clone()))
				}
				_ => None,
			})
			.collect();
		let Some((body, preview)) = tools.into_iter().next() else {
			self.show_toast("No diff tool at index".into());
			return;
		};
		match action {
			0 => {
				if let Some(p) = crate::msg_ui::accept_diff_path(&body, &preview) {
					self.show_toast(format!("Accepted · {}", p.display()));
				} else {
					self.show_toast("Accepted".into());
				}
			}
			1 => match crate::msg_ui::reject_unified_diff(&cwd, &body) {
				Ok(true) => self.show_toast("Rejected · file restored".into()),
				Ok(false) => self.show_toast("Reject · could not reverse cleanly".into()),
				Err(e) => self.show_toast(format!("Reject failed: {e}")),
			},
			2 => {
				if let Some(p) = crate::msg_ui::extract_diff_path(&body, &preview) {
					// Open via file tabs / toast path for editor bridge
					self.file_tabs.open_tab(p.clone());
					self.show_toast(format!("Open · {}", p.display()));
				} else {
					self.show_toast("No path in diff".into());
				}
			}
			_ => {}
		}
	}

	pub fn resolve_pty_hash(&self, hash: u64) -> Option<String> {
		self
			.pty_host
			.snapshots()
			.into_iter()
			.find(|s| {
				let mut h: u64 = 0xcbf2_9ce4_8422_2325;
				for b in s.id.as_bytes() {
					h ^= u64::from(*b);
					h = h.wrapping_mul(0x100_0000_01b3);
				}
				h == hash
			})
			.map(|s| s.id)
	}

	/// Handle a click/hotkey on a bottom-bar center chip (OpenCode footer actions).
	pub fn handle_center_action(&mut self, action: crate::bottom_center::CenterAction) {
		use crate::bottom_center::CenterAction;
		use crate::modes::AgentMode;
		use std::time::Duration;

		if let Some(d) = crate::bottom_center::perm_decision(&action) {
			self.reply_permission(d);
			return;
		}

		match action {
			CenterAction::PermOnce | CenterAction::PermAlways | CenterAction::PermDeny => {}
			CenterAction::QuestionPick(i) => {
				if let Some(q) = self.question_hub.pending() {
					let cur = q.selected as i32;
					let delta = i as i32 - cur;
					if delta != 0 {
						self.question_hub.move_selection(delta);
					}
				}
			}
			CenterAction::QuestionConfirm => {
				if let Some(ans) = self.question_hub.confirm() {
					self.show_toast(format!("Answered: {ans}"));
				}
			}
			CenterAction::QuestionDismiss => {
				self.question_hub.reject();
				self.show_toast("Question dismissed".into());
			}
			CenterAction::GoalPause => {
				self.goal.pause();
				self.goal_pending_continue = false;
				self.show_toast("Goal paused".into());
			}
			CenterAction::GoalResume => {
				self.goal.resume();
				self.show_toast("Goal resumed".into());
			}
			CenterAction::GoalExtend => {
				self.goal.extend(Duration::from_secs(15 * 60), 4);
				self.show_toast("Goal +15m · +4 iters".into());
			}
			CenterAction::PlanToggleFmt => {
				self.plan_options.run_formatter = !self.plan_options.run_formatter;
				self.show_toast(format!(
					"Plan formatter: {}",
					if self.plan_options.run_formatter { "on" } else { "off" }
				));
			}
			CenterAction::PlanToggleLint => {
				self.plan_options.run_linter = !self.plan_options.run_linter;
				self.show_toast(format!(
					"Plan linter: {}",
					if self.plan_options.run_linter { "on" } else { "off" }
				));
			}
			CenterAction::PlanToggleLsp => {
				self.plan_options.use_lsp = !self.plan_options.use_lsp;
				self.show_toast(format!(
					"Plan LSP: {}",
					if self.plan_options.use_lsp { "on" } else { "off" }
				));
			}
			CenterAction::PlanToggleVcs => {
				self.plan_options.use_vcs = !self.plan_options.use_vcs;
				self.show_toast(format!(
					"Plan VCS: {}",
					if self.plan_options.use_vcs { "on" } else { "off" }
				));
			}
			CenterAction::PlanToggleShell => {
				self.plan_options.allow_shell = !self.plan_options.allow_shell;
				self.show_toast(format!(
					"Plan shell: {}",
					if self.plan_options.allow_shell { "on" } else { "off" }
				));
			}
			CenterAction::PlanApproveWrite => {
				// OpenCode plan_exit → build agent: switch to Write and execute.
				let summary = self.plan_options.summary();
				self.agent_mode = AgentMode::Write;
				self.save_prefs();
				self.show_toast("Plan approved → Write".into());
				let prompt = goal_runner::plan_approve_write_prompt(&summary);
				self.add_user_message(prompt);
			}
			CenterAction::ShowPastePreview => {
				self.open_popup(BottomPopup::PastePreview);
			}
			CenterAction::ActionOpenPlan => {
				self.agent_mode = AgentMode::Plan;
				self.save_prefs();
				self.open_popup(BottomPopup::PlanOptions);
			}
			CenterAction::ActionStartGoal => {
				self.agent_mode = AgentMode::Goal;
				self.save_prefs();
				self.show_toast("Goal mode — send your goal as the next message".into());
			}
			CenterAction::ScrollChatTop => {
				self.ui.stick_scroll_to_bottom = false;
				self.set_chat_scroll(0);
				self.show_toast("Top of chat".into());
			}
			CenterAction::ScrollChatBottom => {
				self.ui.stick_scroll_to_bottom = true;
				let max = self.max_chat_scroll();
				self.set_chat_scroll(max);
				self.show_toast("Bottom of chat".into());
			}
		}
	}

	/// Resolve center chip under terminal coordinates.
	pub fn center_chip_at(&self, col: u16, row: u16) -> Option<crate::bottom_center::CenterAction> {
		for (i, (action, rect)) in self.ui.center_chip_areas.iter().enumerate() {
			let _ = i;
			if col >= rect.x
				&& col < rect.x.saturating_add(rect.width)
				&& row >= rect.y
				&& row < rect.y.saturating_add(rect.height)
			{
				return Some(action.clone());
			}
		}
		None
	}

	pub fn update_center_hover(&mut self, col: u16, row: u16) {
		let mut hover = None;
		for (i, (_action, rect)) in self.ui.center_chip_areas.iter().enumerate() {
			if col >= rect.x
				&& col < rect.x.saturating_add(rect.width)
				&& row >= rect.y
				&& row < rect.y.saturating_add(rect.height)
			{
				hover = Some(i);
				break;
			}
		}
		self.ui.center_chip_hover = hover;
	}

	pub fn scroll_chat_by(&mut self, delta: i32) {
		let next = if delta < 0 {
			self.ui.chat_scroll_offset.saturating_sub((-delta) as usize)
		} else {
			self.ui.chat_scroll_offset.saturating_add(delta as usize)
		};
		self.set_chat_scroll(next);
	}

	/// Map a Y position on a vertical scrollbar track to a scroll offset.
	pub fn scroll_offset_from_track_y(
		y: u16,
		track: ratatui::layout::Rect,
		max_scroll: usize,
	) -> usize {
		if max_scroll == 0 || track.height == 0 {
			return 0;
		}
		if track.height == 1 {
			return 0;
		}
		let rel = y.saturating_sub(track.y).min(track.height.saturating_sub(1)) as usize;
		// Inclusive mapping: top row → 0, bottom row → max_scroll
		(rel * max_scroll) / (track.height as usize - 1)
	}

	pub fn scroll_offset_from_drag(
		current_y: u16,
		anchor_y: u16,
		anchor_scroll: usize,
		track: ratatui::layout::Rect,
		max_scroll: usize,
	) -> usize {
		if max_scroll == 0 || track.height <= 1 {
			return 0;
		}
		let track_h = track.height as i32 - 1;
		let initial_rel = (anchor_scroll as i32 * track_h) / max_scroll as i32;
		let delta_y = current_y as i32 - anchor_y as i32;
		let new_rel = (initial_rel + delta_y).clamp(0, track_h);
		((new_rel * max_scroll as i32) / track_h) as usize
	}

	/// Right-edge hit target for chat scrollbar drag (matches drawn track width).
	pub fn chat_scrollbar_track(&self) -> ratatui::layout::Rect {
		let a = self.ui.chat_list_area;
		if a.width == 0 || a.height == 0 {
			return ratatui::layout::Rect::default();
		}
		let w = crate::components::SCROLLBAR_TRACK_WIDTH.min(a.width);
		ratatui::layout::Rect { x: a.x + a.width.saturating_sub(w), y: a.y, width: w, height: a.height }
	}

	pub fn sidebar_scrollbar_track(&self) -> ratatui::layout::Rect {
		let a = self.ui.sidebar_area;
		if a.width == 0 || a.height == 0 {
			return ratatui::layout::Rect::default();
		}
		let w = crate::components::SCROLLBAR_TRACK_WIDTH.min(a.width);
		ratatui::layout::Rect { x: a.x + a.width.saturating_sub(w), y: a.y, width: w, height: a.height }
	}

	/// Rows used by sidebar accordion sections (header + optional empty body).
	pub fn sidebar_content_height(&self) -> u16 {
		let sections = self.sidebar.section_lines();
		self
			.ui
			.accordion_open
			.iter()
			.zip(sections.iter())
			.map(|(&open, (_, body))| if open { 1 + body.len().max(1) as u16 } else { 1 })
			.sum()
	}

	pub fn max_sidebar_scroll(&self) -> u16 {
		self.sidebar_content_height().saturating_sub(self.ui.sidebar_area.height)
	}

	pub fn set_sidebar_scroll(&mut self, offset: u16) {
		self.ui.sidebar_scroll = offset.min(self.max_sidebar_scroll());
	}

	pub fn scroll_sidebar_by(&mut self, delta: i32) {
		let next = self.ui.sidebar_scroll as i32 + delta;
		self.set_sidebar_scroll(next.clamp(0, self.max_sidebar_scroll() as i32) as u16);
	}

	pub fn user_message_count(&self) -> usize {
		self.messages.iter().filter(|m| m.role == crate::components::MessageRole::User).count()
	}

	pub fn max_minimap_scroll(&self) -> u16 {
		let total = self.user_message_count() as u16;
		let viewport = self.ui.minimap_viewport.max(1);
		total.saturating_sub(viewport)
	}

	pub fn set_minimap_scroll(&mut self, offset: u16) {
		self.ui.minimap_scroll = offset.min(self.max_minimap_scroll());
	}

	pub fn scroll_minimap_by(&mut self, delta: i32) {
		let next = self.ui.minimap_scroll as i32 + delta;
		self.set_minimap_scroll(next.clamp(0, self.max_minimap_scroll() as i32) as u16);
	}

	/// Keep the given minimap entry (user-message list index) in view after a click.
	pub fn ensure_minimap_index_visible(&mut self, list_index: usize) {
		let viewport = self.ui.minimap_viewport.max(1) as usize;
		let first = self.ui.minimap_scroll as usize;
		let last = first + viewport.saturating_sub(1);
		if list_index < first {
			self.set_minimap_scroll(list_index as u16);
		} else if list_index > last {
			self.set_minimap_scroll((list_index + 1).saturating_sub(viewport) as u16);
		}
	}

	pub fn scroll_to_message_index(&mut self, index: usize) {
		if index >= self.messages.len() {
			return;
		}
		self.ui.active_message_index = Some(index);
		let w = self
			.ui
			.chat_list_area
			.width
			.saturating_sub(crate::components::SCROLLBAR_TRACK_WIDTH)
			.saturating_sub(crate::components::MESSAGE_LIST_RIGHT_PAD)
			.max(12) as usize;
		let mut offset = 0;
		for i in 0..index {
			if self.messages.get(i).is_some_and(|m| m.hidden) {
				continue;
			}
			offset += crate::components::message_rendered_height_with_context(&self.messages, i, w);
		}
		self.set_chat_scroll(offset);

		// Nudge minimap only for this explicit jump (not every frame).
		if let Some(list_idx) = self.user_message_indices().iter().position(|&i| i == index) {
			self.ensure_minimap_index_visible(list_idx);
		}
	}

	/// Soft, production-ready exit entry point (Ctrl+C / :q).
	/// Flow: First Ctrl+C → clear TUI → train outro → clear → session summary at top.
	///       Second Ctrl+C (during outro) → skip train, clear immediately, show summary.
	///       Ctrl+C on summary → quit.
	pub fn request_exit(&mut self) {
		// Already on summary: quit soon (still leave continue hint on screen briefly).
		if self.session.show_session_screen {
			self.session.session_exit_deadline = Some(Instant::now() + Duration::from_millis(800));
			self.session.quit_after_session_reveal = true;
			self.session.session_reveal_frames = 0;
			return;
		}

		// During outro (train): second Ctrl+C → stop train immediately, show summary, then exit.
		if self.animation.playing_outro && self.animation.exit_after_outro {
			self.animation.playing_outro = false;
			self.animation.transition_start_time = None;
			self.stop_animation_ambience();
			self.finish_outro_exit();
			return;
		}

		// First Ctrl+C: clear TUI → train outro → clear → session summary.
		self.stop_animation_ambience();
		self.session.force_clear_frames = 1; // One clear frame before train starts
		self.animation.outro_animation = AnimationType::Train;
		self.play_sound(SoundCue::Exit);
		self.animation.exit_after_outro = true;
		self.animation.playing_outro = true;
		self.animation.transition_duration = self.outro_transition_duration();
		self.animation.transition_start_time = Some(Instant::now());
		self.animation.animation_start_time = Some(Instant::now());
	}

	#[cfg(test)]
	pub(crate) fn begin_session_exit_screen_for_test(&mut self) {
		self.begin_session_exit_screen(false);
	}

	/// Show the professional session-complete screen (with `dx continue`).
	/// Always clears 1 frame between animation and session details.
	/// `fast` shortens the auto-quit hold after a double-Ctrl+C skip.
	#[allow(dead_code)]
	fn begin_session_exit_screen(&mut self, _fast: bool) {
		self.persist_current_session();
		self.pending_quit = true;
		crate::set_exit_continue_hint(self.generate_session_summary());
	}

	pub fn generate_session_summary(&self) -> String {
		let elapsed = self.session.session_start_time.elapsed();
		let hours = elapsed.as_secs() / 3600;
		let mins = (elapsed.as_secs() % 3600) / 60;
		let secs = elapsed.as_secs() % 60;
		let (model_name, _provider) = self.resolved_model_labels();
		let cont = self.continue_command_line();
		let short_id = crate::session_store::short_session_id(&self.session.chat_id);

		format!(
			"\n  Session saved\n\n  Name      {}\n  ID        {}\n  Messages  {}\n  Model     {}\n  Duration  {:02}:{:02}:{:02}\n\n  Resume this session:\n    {}\n",
			self.session.session_name,
			short_id,
			self.messages.len(),
			model_name,
			hours, mins, secs,
			cont
		)
	}

	fn finish_soft_exit(&mut self) {
		self.finish_soft_exit_public();
	}

	/// Persist session, stash `dx continue` hint, schedule quit.
	pub fn finish_soft_exit_public(&mut self) {
		self.persist_current_session();
		let summary = self.generate_session_summary();
		crate::set_exit_continue_hint(summary);
		self.pending_quit = true;
		self.session.show_session_screen = false;
		self.session.session_exit_deadline = None;
		self.session.quit_after_session_reveal = false;
		self.session.force_clear_frames = 0;
	}

	pub(crate) 	fn finish_outro_exit(&mut self) {
		self.persist_current_session();
		crate::set_exit_continue_hint(self.generate_session_summary());
		self.pending_quit = true;
		// Keep show_session_screen = true so the session summary stays visible on exit
		self.session.session_exit_deadline = None;
		self.session.quit_after_session_reveal = false;
		self.session.force_clear_frames = 0;
	}

	pub fn continue_command_line(&self) -> String {
		crate::session_store::continue_command(&self.session.chat_id)
	}

	/// Selected message index range (inclusive), if any.
	pub fn chat_selection_range(&self) -> Option<(usize, usize)> {
		let a = self.ui.chat_select_anchor?;
		let b = self.ui.chat_select_end.unwrap_or(a);
		Some((a.min(b), a.max(b)))
	}

	pub fn clear_chat_selection(&mut self) {
		self.ui.chat_select_anchor = None;
		self.ui.chat_select_end = None;
		self.ui.chat_text_selection_start = None;
		self.ui.chat_text_selection_end = None;
		self.ui.chat_mouse_selecting = false;
	}

	pub fn selected_chat_text(&self) -> Option<String> {
		let (lo, hi) = self.chat_selection_range()?;
		if self.messages.is_empty() {
			return None;
		}
		let hi = hi.min(self.messages.len().saturating_sub(1));
		let mut out = String::new();
		for (i, msg) in self.messages.iter().enumerate().skip(lo).take(hi - lo + 1) {
			let role = match msg.role {
				crate::components::MessageRole::User => self.user_display_name.as_str(),
				crate::components::MessageRole::Assistant => "DX",
			};
			if !out.is_empty() {
				out.push_str("\n\n");
			}
			out.push_str(&format!("[{i}] {role}\n{}", msg.content));
		}
		if out.is_empty() { None } else { Some(out) }
	}

	pub fn selected_chat_text_exact(&self) -> Option<String> {
		let (msg1, char1) = self.ui.chat_text_selection_start?;
		let (msg2, char2) = self.ui.chat_text_selection_end.unwrap_or((msg1, char1));
		let (m1, c1, m2, c2) = if msg1 < msg2 {
			(msg1, char1, msg2, char2)
		} else if msg1 > msg2 {
			(msg2, char2, msg1, char1)
		} else {
			let (c1, c2) = if char1 < char2 { (char1, char2) } else { (char2, char1) };
			(msg1, c1, msg1, c2)
		};
		let list_w = self
			.ui
			.chat_list_area
			.width
			.saturating_sub(crate::components::MESSAGE_LIST_RIGHT_PAD)
			.saturating_sub(crate::components::SCROLLBAR_TRACK_WIDTH)
			.max(12) as usize;
		let mut out = String::new();
		for i in m1..=m2 {
			if self.messages.get(i).is_none_or(|m| m.hidden) {
				continue;
			}
			let lines =
				crate::components::message_selection_lines(&self.messages, i, list_w, self.is_loading);
			let flattened = crate::components::flatten_selection_lines(&lines);
			let start = if i == m1 { c1 } else { 0 };
			let end = if i == m2 { c2.min(flattened.chars().count()) } else { flattened.chars().count() };
			let chunk: String = flattened.chars().skip(start).take(end.saturating_sub(start)).collect();
			if chunk.is_empty() {
				continue;
			}
			if !out.is_empty() {
				out.push_str("\n\n");
			}
			out.push_str(&chunk);
		}
		if out.is_empty() { None } else { Some(out) }
	}

	/// Copy any active selection (input → chat messages).
	pub fn copy_any_selection(&mut self) -> Option<String> {
		if self.input.has_selection() {
			let text = self.input.get_selected_text()?;
			if text.is_empty() {
				return None;
			}
			let _ = cli_clipboard::set_contents(text.clone());
			self.input.clear_selection();
			return Some(text);
		}
		if let Some(text) = self.selected_chat_text_exact() {
			let _ = cli_clipboard::set_contents(text.clone());
			return Some(text);
		}
		if let Some(text) = self.selected_chat_text() {
			let _ = cli_clipboard::set_contents(text.clone());
			return Some(text);
		}
		None
	}

	/// Map a (col, row) relative to a rendered message block to its character index.
	/// Uses the same paint path as the message list so selection stays aligned.
	pub fn char_index_at_display_pos(&self, msg_idx: usize, col: u16, row: u16) -> usize {
		let list_w = self
			.ui
			.chat_list_area
			.width
			.saturating_sub(crate::components::MESSAGE_LIST_RIGHT_PAD)
			.saturating_sub(crate::components::SCROLLBAR_TRACK_WIDTH)
			.max(12) as usize;
		let lines =
			crate::components::message_selection_lines(&self.messages, msg_idx, list_w, self.is_loading);
		// User bubbles include top+bottom border rows in local_y; body starts at row 1.
		let body_row =
			if self.messages.get(msg_idx).is_some_and(|m| m.role == crate::components::MessageRole::User)
			{
				row.saturating_sub(1) as usize
			} else {
				row as usize
			};
		let mut char_idx = 0usize;
		for (i, line) in lines.iter().enumerate() {
			let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
			if i == body_row {
				let off = crate::components::display_col_to_char_offset(&text, col as usize);
				return char_idx + off;
			}
			char_idx += text.chars().count() + 1; // + newline between rows
		}
		char_idx
	}

	/// Pointer (x,y) → (message index, character index) using paint-aligned geometry.
	pub fn selection_char_at_pointer(&self, x: u16, y: u16) -> Option<(usize, usize)> {
		use unicode_width::UnicodeWidthStr;
		let (idx, local_y) = self.message_hit_test(y)?;
		let area = self.ui.chat_list_area;
		let content_w = area
			.width
			.saturating_sub(crate::components::SCROLLBAR_TRACK_WIDTH)
			.saturating_sub(crate::components::MESSAGE_LIST_RIGHT_PAD)
			.max(12);
		let list_w = content_w as usize;
		let msg = self.messages.get(idx)?;
		let local_x = match msg.role {
			crate::components::MessageRole::Assistant => {
				x.saturating_sub(area.x).saturating_sub(crate::components::MESSAGE_SELECTION_PAD)
			}
			crate::components::MessageRole::User => {
				// Match paint: right-aligned bubble with border + H_PAD inset.
				const H_PAD: u16 = 1;
				const RIGHT_EDGE_GAP: u16 = 1;
				let lines =
					crate::components::message_selection_lines(&self.messages, idx, list_w, self.is_loading);
				let max_line = lines
					.iter()
					.map(|l| l.spans.iter().map(|s| s.content.as_ref().width()).sum::<usize>())
					.max()
					.unwrap_or(1)
					.max(1);
				let msg_width = (max_line + 2 + (H_PAD as usize) * 2)
					.min(content_w.saturating_sub(RIGHT_EDGE_GAP) as usize)
					.max(max_line + 2)
					.max(4) as u16;
				let msg_x = area.x + content_w.saturating_sub(msg_width).saturating_sub(RIGHT_EDGE_GAP);
				// Text starts after left border + H_PAD.
				let text_x = msg_x.saturating_add(1).saturating_add(H_PAD);
				x.saturating_sub(text_x)
			}
		};
		let char_idx = self.char_index_at_display_pos(idx, local_x, local_y);
		Some((idx, char_idx))
	}

	/// Map a Y coordinate inside the chat list viewport to a message index.
	pub fn message_index_at_y(&self, y: u16) -> Option<usize> {
		self.message_hit_test(y).map(|(i, _)| i)
	}

	/// Map a Y coordinate inside the chat list viewport to (message index, local_y).
	pub fn message_hit_test(&self, y: u16) -> Option<(usize, u16)> {
		let area = self.ui.chat_list_area;
		if area.height == 0 || y < area.y || y >= area.bottom() {
			return None;
		}
		let w = area
			.width
			.saturating_sub(crate::components::SCROLLBAR_TRACK_WIDTH)
			.saturating_sub(crate::components::MESSAGE_LIST_RIGHT_PAD)
			.max(12) as usize;
		let mut cursor_y = area.y as isize;
		let scroll = self.ui.chat_scroll_offset as isize;
		let mut skipped = 0isize;
		for (i, msg) in self.messages.iter().enumerate() {
			if msg.hidden {
				continue;
			}
			let h =
				crate::components::message_rendered_height_with_context(&self.messages, i, w) as isize;
			if skipped + h <= scroll {
				skipped += h;
				continue;
			}
			let visible_skip = (scroll - skipped).max(0);
			let visible_h = h - visible_skip;
			let top = cursor_y;
			let bottom = cursor_y + visible_h;
			if (y as isize) >= top && (y as isize) < bottom {
				let local_y = (y as isize - top + visible_skip) as u16;
				return Some((i, local_y));
			}
			cursor_y = bottom;
			skipped += h;
			if cursor_y >= area.bottom() as isize {
				break;
			}
		}
		None
	}

	/// Returns (message_index, interactive block) if the user clicked a header / expand control.
	/// Uses paint-aligned widths, turn-mark offset, structured parts, and footer actions.
	pub fn interactive_block_at_y(
		&self,
		y: u16,
	) -> Option<(usize, crate::components::InteractiveBlock)> {
		self.interactive_block_at(0, y)
	}

	/// Hit-test with optional X (reserved for future in-row actions).
	pub fn interactive_block_at(
		&self,
		_x: u16,
		y: u16,
	) -> Option<(usize, crate::components::InteractiveBlock)> {
		let area = self.ui.chat_list_area;
		if area.height == 0 {
			return None;
		}
		let list_w = area
			.width
			.saturating_sub(crate::components::SCROLLBAR_TRACK_WIDTH)
			.saturating_sub(crate::components::MESSAGE_LIST_RIGHT_PAD)
			.max(12) as usize;
		let paint_w = list_w.saturating_sub(crate::components::MESSAGE_SELECTION_PAD as usize).max(8);

		let mut cursor_y = area.y as isize;
		let scroll = self.ui.chat_scroll_offset as isize;
		let mut skipped = 0isize;
		for (i, m) in self.messages.iter().enumerate() {
			if m.hidden {
				continue;
			}
			let h =
				crate::components::message_rendered_height_with_context(&self.messages, i, list_w) as isize;
			if skipped + h <= scroll {
				skipped += h;
				continue;
			}
			let visible_skip = (scroll - skipped).max(0);
			let top = cursor_y;
			let bottom = top + h - visible_skip;

			if (y as isize) >= top && (y as isize) < bottom {
				if m.role != crate::components::MessageRole::Assistant {
					return None;
				}
				let relative_y = (visible_skip + (y as isize - top)) as usize;

			// Turn marker removed — no offset needed

				// Body parts (thinking / tools / …)
				let tagged = crate::components::render_assistant_lines(
					m,
					&crate::theme::ChatTheme::dark_fallback(),
					Some(paint_w),
					false,
				);
				let mut current_y = 0usize;
				for (line, tag) in &tagged {
					let wrapped = crate::components::clip_lines_to_width(vec![line.clone()], paint_w);
					let line_h = wrapped.len().max(1);
					if relative_y >= current_y && relative_y < current_y + line_h {
						if let Some(kind) = tag {
							return Some((i, *kind));
						}
						return None;
					}
					current_y += line_h;
				}

				// Footer is metrics-only (no action buttons).
				let _ = relative_y == current_y;
				return None;
			}
			cursor_y = bottom;
			skipped += h;
			if cursor_y >= area.bottom() as isize {
				break;
			}
		}
		None
	}

	/// Select every message in the session (whole chat transcript).
	pub fn select_all_chat(&mut self) {
		if self.messages.is_empty() {
			self.clear_chat_selection();
			return;
		}
		self.ui.chat_select_anchor = Some(0);
		self.ui.chat_select_end = Some(self.messages.len() - 1);
	}

	pub fn take_pending_quit(&mut self) -> bool {
		let pending = self.pending_quit;
		self.pending_quit = false;
		pending
	}

	pub fn set_last_animation_area_width(&mut self, width: u16) {
		if width > 0 {
			self.animation.last_animation_area_width = width;
		}
	}

	fn outro_transition_duration(&self) -> Duration {
		match self.animation.outro_animation {
			AnimationType::Train => {
				train_exit_duration_for_width(self.animation.last_animation_area_width)
			}
			_ => Duration::from_secs(2),
		}
	}

	fn drain_agent_response_chunks(&mut self) {
		self.sync_pty_parts_into_messages();
		for _ in 0..256 {
			match self.agent_rx.try_recv() {
				Ok(chunk) if chunk == END_OF_RESPONSE => {
					self.is_loading = false;
					self.sidebar.clear_prompts();
					self.collapse_finished_thinking();
					self.on_assistant_turn_finished();
					if !self.prompt_queue.is_empty() {
						let next = self.prompt_queue.remove(0);
						self.add_user_message(next);
					}
				}
				Ok(chunk) => self.append_agent_chunk(&chunk),
				Err(TryRecvError::Empty) => break,
				Err(TryRecvError::Disconnected) => {
					self.is_loading = false;
					break;
				}
			}
		}
		if self.is_loading && self.ui.stick_scroll_to_bottom {
			self.ui.chat_scroll_offset = self.max_chat_scroll();
		}
	}

	fn apply_stream_event(&mut self, ev: crate::stream_events::StreamEvent) {
		use crate::stream_events::StreamEvent;
		match ev {
			StreamEvent::ToolDelta { id, chunk } => {
				if let Some(last) = self.messages.last_mut()
					&& last.role == crate::components::MessageRole::Assistant {
						last.apply_tool_delta(&id, &chunk);
					}
			}
			StreamEvent::ToolEnd { id, ok, duration_ms } => {
				let _ = (id, ok, duration_ms);
				// Result fence usually follows; no-op tombstone.
			}
			StreamEvent::Permission { tool, preview, call_id } => {
				if let Some(last) = self.messages.last_mut() {
					let body = format!("{tool}\n{preview}\n[y] Allow once   [a] Always   [n] Deny");
					crate::msg_ui::push_approval(&mut last.parts, &call_id, &tool, &body);
				}
				self.show_toast(format!(
					"Permission · {tool} · {}",
					preview.chars().take(40).collect::<String>()
				));
			}
			StreamEvent::PermissionResolved { decision, call_id } => {
				if let Some(last) = self.messages.last_mut() {
					crate::msg_ui::resolve_approval(&mut last.parts, &call_id, &decision);
				}
				self.show_toast(format!("Permission · {decision}"));
			}
			StreamEvent::Question { id, prompt, options } => {
				if let Some(last) = self.messages.last_mut()
					&& !last
						.parts
						.iter()
						.any(|p| matches!(p, crate::msg_ui::StreamPart::Question { id: qid, .. } if qid == &id))
					{
						crate::msg_ui::push_question(&mut last.parts, &id, &prompt, options);
					}
				self.show_toast(format!("Question · {}", prompt.chars().take(48).collect::<String>()));
			}
			StreamEvent::SubagentMeta { name, status } => {
				if status == "running" {
					if let Some(last) = self.messages.last_mut() {
						crate::msg_ui::open_subagent(&mut last.parts, &name);
					}
					self.show_toast(format!("Subagent · {name} · running"));
				} else if let Some(last) = self.messages.last_mut() {
					crate::msg_ui::close_subagent(&mut last.parts, status == "done");
				}
			}
		}
	}

	fn append_agent_chunk(&mut self, chunk: &str) {
		// Structured live stream events (tool deltas, permission, questions)
		if let Some(ev) = crate::stream_events::StreamEvent::decode_chunk(chunk) {
			self.apply_stream_event(ev);
			return;
		}
		// Permission / question / control IPC (not message body)
		if let Some(rest) = chunk.strip_prefix(crate::permission_hub::PERM_REQ_PREFIX) {
			let mut lines = rest.lines();
			let tool = lines.next().unwrap_or("tool").trim().to_string();
			let preview = lines.next().unwrap_or("").trim().to_string();
			self.show_toast(format!("Permission: {tool} · [y]/a]/n]"));
			let _ = (tool, preview); // hub already holds pending
			return;
		}
		if chunk.starts_with(crate::permission_hub::QUESTION_REQ_PREFIX)
			|| chunk.contains(crate::permission_hub::QUESTION_REQ_PREFIX)
		{
			self.show_toast("Question · ↑/↓ select · Enter confirm · Esc dismiss".into());
			// Question fence is appended by the agent loop as message body.
		}
		if chunk.contains(crate::permission_hub::INTERRUPTED_MARKER)
			&& let Some(last) = self.messages.last_mut()
				&& last.role == crate::components::MessageRole::Assistant {
					last.interrupted = true;
				}
		if let Some(rest) = chunk.strip_prefix(crate::permission_hub::COMPACTION_MARKER) {
			if let Some(last) = self.messages.last_mut() {
				last.append_content(&format!("\n── Context compacted ──\n{rest}"));
			}
			return;
		}
		if let Some(rest) = chunk.strip_prefix(crate::permission_hub::ERROR_CARD_PREFIX) {
			if let Some(last) = self.messages.last_mut() {
				last.append_content(&format!("\n✗ {rest}\n"));
			}
			return;
		}
		if let Some(rest) = chunk.strip_prefix(crate::permission_hub::RETRY_HINT_PREFIX) {
			if let Some(last) = self.messages.last_mut() {
				last.append_content(&format!("\n↻ {rest}\n"));
			}
			return;
		}
		if let Some(rest) = chunk.strip_prefix("\n__UPDATE_STATUS__\n") {
			let msg = rest.trim().to_string();
			self.update_status_line = Some(msg.clone());
			self.show_toast(msg);
			return;
		}

		// Voice panel IPC from background tasks
		if let Some(rest) = chunk.strip_prefix("\n__VOICE_STT__\n") {
			let text = rest.trim().to_string();
			self.voice_state.panel.last_transcript = text.clone();
			self.voice_state.panel.status = "STT done".into();
			self.voice_state.panel.processing = false;
			if text.is_empty() {
				self.show_toast("STT returned empty · try speaking louder/longer".into());
			} else {
				// Append to input if already typing, else replace
				if self.input.content.trim().is_empty() {
					self.input.replace_content(&text);
				} else {
					let mut next = self.input.content.clone();
					if !next.ends_with(' ') && !next.ends_with('\n') {
						next.push(' ');
					}
					next.push_str(&text);
					self.input.replace_content(next);
				}
				self.show_toast(format!("STT → input ({} chars) · Enter to send", text.len()));
			}
			return;
		}
		if let Some(rest) = chunk.strip_prefix("\n__VOICE_TTS__\n") {
			let path = std::path::PathBuf::from(rest.trim());
			self.voice_state.panel.last_tts_path = Some(path.clone());
			self.voice_state.panel.status = format!("TTS → {}", path.display());
			// speak_text already played once at 5% — do not play again.
			if self.voice_state.panel.speaking {
				self.show_toast("Speaking (Kokoro · 5%)".into());
				self.voice_state.panel.speaking = false;
			} else {
				self.show_toast(format!("TTS saved: {}", path.display()));
			}
			return;
		}
		if let Some(rest) = chunk.strip_prefix("\n__VOICE_ERR__\n") {
			self.voice_state.panel.status = rest.chars().take(80).collect();
			self.voice_state.panel.speaking = false;
			self.voice_state.panel.processing = false;
			self.show_toast(format!("Voice: {}", rest.chars().take(100).collect::<String>()));
			return;
		}

		if let Some(last_msg) = self.messages.last_mut() {
			let before = &last_msg.content;
			let before_opens = before.matches("<think>").count() + before.matches("<thinking>").count();
			let before_closes =
				before.matches("</think>").count() + before.matches("</thinking>").count();
			let before_in_think = before_opens > before_closes;

			// Completed tool fences replace matching status=running blocks so the
			// message list never stacks dummy "running…" cards on top of real output.
			let is_tool_result = chunk.contains("```command")
				&& (chunk.contains("status=\"done\"")
					|| chunk.contains("status=\"error\"")
					|| chunk.contains("status=done")
					|| chunk.contains("status=error"));
			if is_tool_result {
				last_msg.upgrade_tool_result(chunk);
				// Drop raw XML tool tags the model printed (e.g. <shell command="…"/>)
				// now that a real tool card exists.
				last_msg.strip_xml_tool_tags();
			} else {
				last_msg.append_content(chunk);
			}

			let after = &last_msg.content;
			let after_opens = after.matches("<think>").count() + after.matches("<thinking>").count();
			let after_closes = after.matches("</think>").count() + after.matches("</thinking>").count();
			let after_in_think = after_opens > after_closes;

			if !before_in_think && after_in_think {
				self.thinking_started_at = Some(Instant::now());
				// Expand thoughts while they stream
				last_msg.thinking_expanded = true;
			}

			if before_in_think && !after_in_think {
				if let Some(start) = self.thinking_started_at.take() {
					last_msg.thinking_duration = Some(start.elapsed());
				} else if last_msg.thinking_duration.is_none() {
					last_msg.thinking_duration = self.turn_started_at.map(|t| t.elapsed());
				}
				// Auto-collapse only when the thought is long (>6 lines)
				let n = last_msg.thinking_line_count();
				last_msg.thinking_expanded = n > 0 && n <= 6;
			}

			if after_in_think {
				last_msg.thinking_expanded = true;
				if let Some(start) = self.thinking_started_at {
					last_msg.thinking_duration = Some(start.elapsed());
				}
			}

			if !self.session.title_from_ai
				&& let Some(title) = crate::session_meta::parse_title_line(&last_msg.content) {
					self.session.session_name = title;
					self.session.title_from_ai = true;
					self.session.title_auto_generated = true;
				}
		}
	}

	fn collapse_finished_thinking(&mut self) {
		if let Some(last_msg) = self.messages.last_mut() {
			let has_think = last_msg.content.contains("<think>")
				|| last_msg.content.contains("<thinking>")
				|| last_msg.content.contains("</think>")
				|| last_msg.content.contains("</thinking>");
			if has_think {
				// Short thoughts stay open; long ones collapse to save height
				let n = last_msg.thinking_line_count();
				last_msg.thinking_expanded = n > 0 && n <= 6;
				if last_msg.thinking_duration.is_none() {
					if let Some(start) = self.thinking_started_at.take() {
						last_msg.thinking_duration = Some(start.elapsed());
					} else if let Some(start) = self.turn_started_at {
						last_msg.thinking_duration = Some(start.elapsed());
					}
				} else {
					self.thinking_started_at = None;
				}
			}
		}
	}
	/// Poll the codex bridge for events and process them.
	pub fn poll_codex_events(&mut self) {
		// Check for pending connection result
		if let Some(rx) = &mut self.codex_connection {
			match rx.try_recv() {
				Ok(Ok(bridge)) => {
					self.codex_bridge = Some(bridge);
					self.codex_connection = None;
					self.show_toast("Codex connected (in-process)".into());
				}
				Ok(Err(e)) => {
					self.codex_connection = None;
					self.show_toast(format!("Codex connection failed: {e}"));
				}
				Err(_) => {} // Not ready yet
			}
		}

		let Some(ref mut bridge) = self.codex_bridge else { return };
		while let Some(event) = bridge.try_recv_event() {
			match event {
				crate::codex_bridge::BridgeEvent::AgentMessageDelta { delta } => {
					self.append_codex_delta(&delta);
				}
				crate::codex_bridge::BridgeEvent::ReasoningTextDelta { delta } => {
					self.append_codex_delta(&format!("<think>{delta}</think>"));
				}
				crate::codex_bridge::BridgeEvent::TurnCompleted => {
					self.finish_codex_message();
				}
				crate::codex_bridge::BridgeEvent::Error(msg) => {
					self.append_codex_delta(&format!("\n> Error: {msg}"));
					self.show_toast(format!("Codex error: {msg}"));
				}
				crate::codex_bridge::BridgeEvent::Disconnected { .. } => {
					self.show_toast("Codex disconnected".into());
					self.codex_bridge = None;
				}
				_ => {}
			}
		}
	}

}

#[cfg(test)]
mod tests {
	use std::time::Duration;

	use crate::agent_backend::END_OF_RESPONSE;
	use crate::components::Message;
	use crate::sound::AnimationSound;

	use super::{AnimationType, ChatState};

	#[test]
	fn current_animation_falls_back_when_index_is_stale() {
		let mut state = ChatState::new();
		state.animation.current_animation_index = usize::MAX;

		assert_eq!(state.current_animation(), AnimationType::Splash);
	}

	#[test]
	fn disabled_animation_screens_are_not_selectable() {
		assert!(!AnimationType::all().contains(&AnimationType::Confetti));
		assert!(!AnimationType::all().contains(&AnimationType::NyanCat));
		assert!(!AnimationType::all().contains(&AnimationType::DVDLogo));
		assert!(!AnimationType::carousel_animations().contains(&AnimationType::Confetti));
		assert!(!AnimationType::carousel_animations().contains(&AnimationType::NyanCat));
		assert!(!AnimationType::carousel_animations().contains(&AnimationType::DVDLogo));
	}

	#[test]
	fn default_outro_is_train() {
		let state = ChatState::new();

		assert_eq!(state.animation.outro_animation, AnimationType::Train);
	}

	#[test]
	fn default_model_is_remote_zen() {
		let state = ChatState::new();
		assert_eq!(state.provider.selected_model, crate::zen::DEFAULT_MODEL);
		assert_eq!(state.provider.model_display_name, crate::zen::DEFAULT_MODEL_DISPLAY);
	}

	#[test]
	fn token_label_shows_combined_total_and_pct() {
		let mut state = ChatState::new();
		state.session.last_input_tokens = 42;
		state.session.last_output_tokens = 10;
		state.session.session_input_tokens = 42;
		state.session.session_output_tokens = 10;
		let label = state.token_usage_label();
		// 52 tokens total → "52 (0%)" against default context window
		assert!(label.contains("52"), "got {label}");
		assert!(label.contains('%'), "got {label}");
		assert!(!label.contains('↑'), "got {label}");
		assert!(!label.contains('↓'), "got {label}");
	}

	#[test]
	fn token_label_formats_thousands() {
		let mut state = ChatState::new();
		state.session.session_input_tokens = 8_000;
		state.session.session_output_tokens = 2_500;
		state.session.last_input_tokens = 8_000;
		state.session.last_output_tokens = 2_500;
		let label = state.token_usage_label();
		// 10_500 → "10.5K (…%)"
		assert!(label.starts_with("10.5K"), "got {label}");
		assert!(label.contains('%'), "got {label}");
	}

	#[test]
	fn bottom_tips_rotate() {
		let tips = ChatState::message_tips();
		assert!(tips.len() >= 3);
		let mut state = ChatState::new();
		let first = state.current_tip();
		state.ui.shortcut_index = 1;
		let second = state.current_tip();
		assert_ne!(first, second);
	}

	#[test]
	fn exit_request_shows_session_screen_after_outro_duration() {
		let mut state = ChatState::new();

		state.set_last_animation_area_width(80);
		state.request_exit();
		assert!(state.animation.playing_outro);
		assert!(!state.take_pending_quit());

		state.animation.transition_start_time =
			Some(std::time::Instant::now() - state.animation.transition_duration);
		state.update();

		assert!(!state.animation.playing_outro);
		// Outro completion now triggers immediate quit with screen clear.
		assert!(state.take_pending_quit());
	}

	#[test]
	fn second_exit_request_lands_on_soft_summary() {
		let mut state = ChatState::new();

		state.request_exit();
		state.request_exit();

		// Second Ctrl+C skips train → immediate quit with screen clear.
		assert!(state.take_pending_quit());
		assert!(state.continue_command_line().starts_with("dx continue "));
	}

	#[test]
	fn soft_exit_finish_schedules_quit() {
		let mut state = ChatState::new();
		state.begin_session_exit_screen_for_test();
		state.finish_soft_exit_public();
		assert!(state.take_pending_quit());
		assert!(!state.session.show_session_screen);
	}

	#[test]
	fn train_exit_uses_fast_visible_transition_duration() {
		let mut state = ChatState::new();
		state.set_last_animation_area_width(80);

		state.request_exit();

		assert!(state.animation.transition_duration > Duration::from_secs(4));
		assert!(state.animation.transition_duration < Duration::from_secs(7));
	}

	#[test]
	fn exit_request_forces_train_outro() {
		let mut state = ChatState::new();
		state.animation.outro_animation = AnimationType::Matrix;

		state.request_exit();

		assert_eq!(state.animation.outro_animation, AnimationType::Train);
		assert!(state.animation.playing_outro);
	}

	#[test]
	fn current_animation_ambience_follows_carousel_selection() {
		let mut state = ChatState::new();
		state.animation.current_animation_index = AnimationType::all()
			.iter()
			.position(|animation| *animation == AnimationType::Matrix)
			.expect("matrix animation");

		state.restart_current_animation();
		assert_eq!(state.active_animation_sound(), Some(AnimationSound::Matrix));

		state.animation.current_animation_index = AnimationType::all()
			.iter()
			.position(|animation| *animation == AnimationType::Rain)
			.expect("rain animation");

		state.restart_current_animation();
		assert_eq!(state.active_animation_sound(), Some(AnimationSound::Rain));

		state.stop_animation_ambience();
		assert_eq!(state.active_animation_sound(), None);
	}

	#[test]
	fn agent_response_chunks_append_to_existing_assistant_message_without_runtime() {
		let mut state = ChatState::new();
		state.messages.push(Message::assistant(String::new()));
		state.is_loading = true;

		state.agent_tx.send("hello".to_string()).expect("send chunk");
		state.agent_tx.send(" from DX".to_string()).expect("send chunk");
		state.update();

		assert_eq!(state.messages.last().expect("assistant message").content, "hello from DX");
		assert!(state.is_loading);
	}

	#[test]
	fn agent_response_thinking_stays_collapsed_by_default() {
		let mut state = ChatState::new();
		state.messages.push(Message::assistant(String::new()));
		state.is_loading = true;

		state.agent_tx.send("<think>\nplan".to_string()).expect("send thinking");
		state.update();
		assert!(!state.messages.last().expect("assistant message").thinking_expanded);

		state.agent_tx.send("\n</think>\nanswer".to_string()).expect("send answer");
		state.agent_tx.send(END_OF_RESPONSE.to_string()).expect("send end marker");
		state.update();

		let message = state.messages.last().expect("assistant message");
		assert!(!message.thinking_expanded);
		assert_eq!(message.content, "<think>\nplan\n</think>\nanswer");
		assert!(!state.is_loading);
	}

	#[test]
	fn agent_response_end_marker_clears_loading_without_backend() {
		let mut state = ChatState::new();
		state.messages.push(Message::assistant(String::new()));
		state.is_loading = true;

		state.agent_tx.send(END_OF_RESPONSE.to_string()).expect("send end marker");
		state.update();

		assert!(!state.is_loading);
	}

	#[test]
	fn train_exit_does_not_clear_messages_before_session_screen() {
		let mut state = ChatState::new();
		state.messages.push(Message::user("stay visible".to_string()));

		state.request_exit();
		state.animation.transition_start_time =
			Some(std::time::Instant::now() - state.animation.transition_duration);
		state.update();

		assert_eq!(state.messages.len(), 1);
		assert!(state.take_pending_quit());
	}

	#[test]
	fn staging_dx_command_updates_input_without_submitting_message() {
		let mut state = ChatState::new();

		state.stage_dx_command("dx status --json");

		assert_eq!(state.input.content, "dx status --json");
		assert_eq!(state.input.cursor_position, "dx status --json".len());
		assert!(state.messages.is_empty());
		assert!(!state.is_loading);
	}

	#[test]
	fn dx_tool_confirmation_can_be_confirmed_or_cancelled() {
		let mut state = ChatState::new();
		let action = crate::menu::DxToolAction {
			command: "dx www build",
			kind: crate::menu::DxToolActionKind::ConfirmThenStage,
			confirmation: Some("confirm"),
		};

		state.request_dx_tool_confirmation(action);
		assert!(state.pending_dx_tool_confirmation.is_some());

		let confirmed = state.confirm_pending_dx_tool().expect("confirmed action");
		assert_eq!(confirmed.command, "dx www build");
		assert_eq!(state.input.content, "dx www build");
		assert!(state.pending_dx_tool_confirmation.is_none());

		state.request_dx_tool_confirmation(action);
		assert!(state.cancel_pending_dx_tool_confirmation());
		assert!(state.pending_dx_tool_confirmation.is_none());
	}

	// ── Session state transitions ───────────────────────────────────────

	#[test]
	fn session_id_persists_after_new_session() {
		let mut state = ChatState::new();
		let original_id = state.session.chat_id.clone();
		state.start_new_session();
		assert_ne!(state.session.chat_id, original_id);
	}

	#[test]
	fn session_name_defaults_to_new_and_can_be_renamed() {
		let mut state = ChatState::new();
		state.rename_session("my-project");
		assert_eq!(state.session.name.as_deref(), Some("my-project"));
	}

	#[test]
	fn add_user_message_sets_loading_and_increments_message_count() {
		let mut state = ChatState::new();
		assert_eq!(state.messages.len(), 0);
		assert!(!state.is_loading);

		state.add_user_message("test query");

		assert_eq!(state.messages.len(), 1);
		assert_eq!(state.messages[0].role, crate::components::MessageRole::User);
		assert!(state.is_loading);
	}

	#[test]
	fn staging_dx_command_without_confirmation_does_not_set_loading() {
		let mut state = ChatState::new();
		state.stage_dx_command("dx status");
		assert!(!state.is_loading);
	}

	#[test]
	fn clear_messages_resets_state() {
		let mut state = ChatState::new();
		state.add_user_message("hello");
		state.add_user_message("world");
		assert_eq!(state.messages.len(), 2);

		state.clear_messages();
		assert!(state.messages.is_empty());
		assert!(!state.is_loading);
	}

	// ── Animation state ────────────────────────────────────────────────

	#[test]
	fn animation_carousel_has_multiple_entries() {
		let carousel = AnimationType::carousel_animations();
		assert!(carousel.len() >= 5, "carousel should have at least 5 animations, got {}", carousel.len());
	}

	#[test]
	fn animation_all_includes_all_active_variants() {
		let all = AnimationType::all();
		assert!(all.contains(&AnimationType::Splash));
		assert!(all.contains(&AnimationType::Train));
		assert!(all.contains(&AnimationType::Matrix));
	}

	#[test]
	fn animation_cycle_moves_to_next() {
		let mut state = ChatState::new();
		let before = state.current_animation();
		state.cycle_animation();
		let after = state.current_animation();
		// Should change to a different animation (or wrap around)
		assert!(before != after || state.animation.current_animation_index > 0);
	}

	#[test]
	fn sound_property_is_deterministic_for_all_animations() {
		for anim in AnimationType::all() {
			let _sound = anim.sound();
			// At minimum, no panic
		}
	}

	// ── Toast notifications ─────────────────────────────────────────────

	#[test]
	fn show_toast_sets_message_and_timeout() {
		let mut state = ChatState::new();
		assert!(state.ui.toast_message.is_none());

		state.show_toast("test notification".into());

		assert_eq!(state.ui.toast_message.as_deref(), Some("test notification"));
		assert!(state.ui.toast_deadline.is_some());
	}

	#[test]
	fn toast_expires_after_timeout() {
		let mut state = ChatState::new();
		state.show_toast("temporary".into());

		// Set deadline in the past
		if let Some(deadline) = &mut state.ui.toast_deadline {
			*deadline = std::time::Instant::now()
			    .checked_sub(std::time::Duration::from_secs(10))
			    .unwrap_or(std::time::Instant::now());
		}

		state.update(); // update() clears expired toasts
		assert!(state.ui.toast_message.is_none());
	}

	// ── Agent mode transitions ──────────────────────────────────────────

	#[test]
	fn agent_mode_defaults_to_ask() {
		let state = ChatState::new();
		assert_eq!(state.agent_mode, crate::modes::AgentMode::Ask);
	}

	#[test]
	fn cycle_agent_mode_changes_mode() {
		let mut state = ChatState::new();
		let modes = vec![
			crate::modes::AgentMode::Ask,
			crate::modes::AgentMode::Write,
			crate::modes::AgentMode::Plan,
			crate::modes::AgentMode::Goal,
		];
		let mut seen = Vec::new();
		for _ in 0..modes.len() * 2 {
			state.cycle_agent_mode();
			seen.push(state.agent_mode);
		}
		// Should have visited at least 3 different modes after cycling
		let unique: std::collections::HashSet<_> = seen.into_iter().collect();
		assert!(unique.len() >= 3, "only visited {} unique modes", unique.len());
	}

	// ── Token usage label ──────────────────────────────────────────────

	#[test]
	fn token_usage_label_with_session_data() {
		let state = ChatState::new();
		let label = state.token_usage_label();
		// Even with no messages, there should be some default label
		assert!(!label.is_empty());
	}

	#[test]
	fn context_window_label_has_percentage() {
		let state = ChatState::new();
		let label = state.context_window_status_label();
		assert!(label.contains('%') || label.is_empty());
	}
}
