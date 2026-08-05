use mlua::{FromLua, IntoLua, Lua, Value};

use crate::{spark::SparkKind, try_from_spark};

#[derive(Debug)]
pub enum Spark<'a> {
	// Void
	Void(fb_parser::VoidOpt),

	// App
	AppAcceptPayload(crate::Payload<'a>),
	AppBootstrap(fb_parser::VoidOpt),
	AppDeprecate(fb_parser::app::DeprecateOpt),
	AppFocus(fb_parser::VoidOpt),
	AppMouse(fb_parser::app::MouseOpt),
	AppPlugin(fb_parser::app::PluginOpt),
	AppPluginDo(fb_parser::app::PluginOpt),
	AppQuit(fb_parser::app::QuitOpt),
	AppReflow(fb_parser::app::ReflowOpt),
	AppResize(fb_parser::app::ReflowOpt),
	AppResume(fb_parser::app::ResumeOpt),
	AppStop(fb_parser::app::StopOpt),
	AppTitle(fb_parser::app::TitleOpt),
	AppUpdateProgress(fb_parser::app::UpdateProgressOpt),

	// Mgr
	Arrow(fb_parser::ArrowOpt),
	Back(fb_parser::VoidOpt),
	BulkRename(fb_parser::VoidOpt),
	Cd(fb_parser::mgr::CdOpt),
	Close(fb_parser::mgr::CloseOpt),
	Copy(fb_parser::mgr::CopyOpt),
	Create(fb_parser::mgr::CreateOpt),
	Displace(fb_parser::VoidOpt),
	DisplaceDo(fb_parser::mgr::DisplaceDoOpt),
	Download(fb_parser::mgr::DownloadOpt),
	Enter(fb_parser::VoidOpt),
	Escape(fb_parser::mgr::EscapeOpt),
	EscapeFilter(fb_parser::VoidOpt),
	EscapeFind(fb_parser::VoidOpt),
	EscapeSearch(fb_parser::VoidOpt),
	EscapeSelect(fb_parser::VoidOpt),
	EscapeVisual(fb_parser::VoidOpt),
	Filter(fb_parser::mgr::FilterOpt),
	FilterDo(fb_parser::mgr::FilterOpt),
	Find(fb_parser::mgr::FindOpt),
	FindArrow(fb_parser::mgr::FindArrowOpt),
	FindDo(fb_parser::mgr::FindDoOpt),
	Follow(fb_parser::VoidOpt),
	Forward(fb_parser::VoidOpt),
	Hardlink(fb_parser::mgr::HardlinkOpt),
	Hidden(fb_parser::mgr::HiddenOpt),
	Hover(fb_parser::mgr::HoverOpt),
	Leave(fb_parser::VoidOpt),
	Linemode(fb_parser::mgr::LinemodeOpt),
	Link(fb_parser::mgr::LinkOpt),
	Open(fb_parser::mgr::OpenOpt),
	OpenDo(fb_parser::mgr::OpenDoOpt),
	ParentArrow(fb_parser::ArrowOpt),
	Paste(fb_parser::mgr::PasteOpt),
	Peek(fb_parser::mgr::PeekOpt),
	Quit(fb_parser::app::QuitOpt),
	Refresh(fb_parser::VoidOpt),
	Remove(fb_parser::mgr::RemoveOpt),
	RemoveDo(fb_parser::mgr::RemoveOpt),
	Rename(fb_parser::mgr::RenameOpt),
	Reveal(fb_parser::mgr::RevealOpt),
	Search(fb_parser::mgr::SearchOpt),
	SearchDo(fb_parser::mgr::SearchOpt),
	SearchStop(fb_parser::VoidOpt),
	Seek(fb_parser::mgr::SeekOpt),
	Shell(fb_parser::mgr::ShellOpt),
	Sort(fb_parser::mgr::SortOpt),
	Spot(fb_parser::mgr::SpotOpt),
	Stash(fb_parser::mgr::StashOpt),
	Suspend(fb_parser::VoidOpt),
	TabClose(fb_parser::mgr::TabCloseOpt),
	TabCreate(fb_parser::mgr::TabCreateOpt),
	TabRename(fb_parser::mgr::TabRenameOpt),
	TabSwap(fb_parser::ArrowOpt),
	TabSwitch(fb_parser::mgr::TabSwitchOpt),
	Toggle(fb_parser::mgr::ToggleOpt),
	ToggleAll(fb_parser::mgr::ToggleAllOpt),
	Unyank(fb_parser::VoidOpt),
	UpdateFiles(fb_parser::mgr::UpdateFilesOpt),
	UpdateMimes(fb_parser::mgr::UpdateMimesOpt),
	UpdatePaged(fb_parser::mgr::UpdatePagedOpt),
	UpdatePeeked(fb_parser::mgr::UpdatePeekedOpt),
	UpdateSpotted(fb_parser::mgr::UpdateSpottedOpt),
	UpdateYanked(fb_parser::mgr::UpdateYankedOpt<'a>),
	Upload(fb_parser::mgr::UploadOpt),
	VisualMode(fb_parser::mgr::VisualModeOpt),
	Watch(fb_parser::VoidOpt),
	Yank(fb_parser::mgr::YankOpt),

	// Cmp
	CmpArrow(fb_parser::ArrowOpt),
	CmpClose(fb_parser::cmp::CloseOpt),
	CmpShow(fb_parser::cmp::ShowOpt),
	CmpTrigger(fb_parser::cmp::TriggerOpt),

	// Confirm
	ConfirmArrow(fb_parser::ArrowOpt),
	ConfirmClose(fb_parser::confirm::CloseOpt),
	ConfirmShow(Box<fb_parser::confirm::ShowOpt>),

	// Help
	HelpArrow(fb_parser::ArrowOpt),
	HelpEscape(fb_parser::VoidOpt),
	HelpFilter(fb_parser::VoidOpt),
	HelpToggle(fb_parser::help::ToggleOpt),

	// Input
	InputBackspace(fb_widgets::input::parser::BackspaceOpt),
	InputBackward(fb_widgets::input::parser::BackwardOpt),
	InputClose(fb_parser::input::CloseOpt),
	InputComplete(fb_widgets::input::parser::CompleteOpt),
	InputDelete(fb_widgets::input::parser::DeleteOpt),
	InputEscape(fb_parser::VoidOpt),
	InputForward(fb_widgets::input::parser::ForwardOpt),
	InputInsert(fb_widgets::input::parser::InsertOpt),
	InputKill(fb_widgets::input::parser::KillOpt),
	InputMove(fb_widgets::input::parser::MoveOpt),
	InputPaste(fb_widgets::input::parser::PasteOpt),
	InputShow(fb_parser::input::ShowOpt),

	// Notify
	NotifyPush(fb_parser::notify::PushOpt),
	NotifyTick(fb_parser::notify::TickOpt),

	// Pick
	PickArrow(fb_parser::ArrowOpt),
	PickClose(fb_parser::pick::CloseOpt),
	PickShow(fb_parser::pick::ShowOpt),

	// Spot
	SpotArrow(fb_parser::ArrowOpt),
	SpotClose(fb_parser::VoidOpt),
	SpotCopy(fb_parser::spot::CopyOpt),
	SpotSwipe(fb_parser::ArrowOpt),

	// Tasks
	TasksArrow(fb_parser::ArrowOpt),
	TasksCancel(fb_parser::VoidOpt),
	TasksClose(fb_parser::VoidOpt),
	TasksInspect(fb_parser::VoidOpt),
	TasksOpenShellCompat(fb_parser::tasks::ProcessOpenOpt),
	TasksProcessOpen(fb_parser::tasks::ProcessOpenOpt),
	TasksShow(fb_parser::VoidOpt),
	TasksUpdateSucceed(fb_parser::tasks::UpdateSucceedOpt),

	// Which
	WhichActivate(fb_parser::which::ActivateOpt),
	WhichDismiss(fb_parser::VoidOpt),
}

impl<'a> Spark<'a> {
	pub fn from_lua(lua: &Lua, kind: SparkKind, value: Value) -> mlua::Result<Self> {
		use SparkKind::*;

		Ok(match kind {
			// app:title
			IndAppTitle => Self::AppTitle(<_>::from_lua(value, lua)?),

			// mgr:hidden
			KeyHidden => Self::Hidden(<_>::from_lua(value, lua)?),
			IndHidden => Self::Hidden(<_>::from_lua(value, lua)?),
			// mgr:sort
			KeySort => Self::Sort(<_>::from_lua(value, lua)?),
			IndSort => Self::Sort(<_>::from_lua(value, lua)?),
			// mgr:stash
			IndStash => Self::Stash(<_>::from_lua(value, lua)?),
			RelayStash => Self::Stash(<_>::from_lua(value, lua)?),
			// mgr:quit
			KeyQuit => Self::Quit(<_>::from_lua(value, lua)?),

			// which:activate
			IndWhichActivate => Self::WhichActivate(<_>::from_lua(value, lua)?),

			// notify:push
			RelayNotifyPush => Self::NotifyPush(<_>::from_lua(value, lua)?),
		})
	}
}

impl<'a> IntoLua for Spark<'a> {
	fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
		match self {
			// Void
			Self::Void(b) => b.into_lua(lua),

			// App
			Self::AppAcceptPayload(b) => b.into_lua(lua),
			Self::AppBootstrap(b) => b.into_lua(lua),
			Self::AppDeprecate(b) => b.into_lua(lua),
			Self::AppFocus(b) => b.into_lua(lua),
			Self::AppMouse(b) => b.into_lua(lua),
			Self::AppPlugin(b) => b.into_lua(lua),
			Self::AppPluginDo(b) => b.into_lua(lua),
			Self::AppQuit(b) => b.into_lua(lua),
			Self::AppReflow(b) => b.into_lua(lua),
			Self::AppResize(b) => b.into_lua(lua),
			Self::AppResume(b) => b.into_lua(lua),
			Self::AppStop(b) => b.into_lua(lua),
			Self::AppTitle(b) => b.into_lua(lua),
			Self::AppUpdateProgress(b) => b.into_lua(lua),

			// Mgr
			Self::Arrow(b) => b.into_lua(lua),
			Self::Back(b) => b.into_lua(lua),
			Self::BulkRename(b) => b.into_lua(lua),
			Self::Cd(b) => b.into_lua(lua),
			Self::Close(b) => b.into_lua(lua),
			Self::Copy(b) => b.into_lua(lua),
			Self::Create(b) => b.into_lua(lua),
			Self::Displace(b) => b.into_lua(lua),
			Self::DisplaceDo(b) => b.into_lua(lua),
			Self::Download(b) => b.into_lua(lua),
			Self::Enter(b) => b.into_lua(lua),
			Self::Escape(b) => b.into_lua(lua),
			Self::EscapeFilter(b) => b.into_lua(lua),
			Self::EscapeFind(b) => b.into_lua(lua),
			Self::EscapeSearch(b) => b.into_lua(lua),
			Self::EscapeSelect(b) => b.into_lua(lua),
			Self::EscapeVisual(b) => b.into_lua(lua),
			Self::Filter(b) => b.into_lua(lua),
			Self::FilterDo(b) => b.into_lua(lua),
			Self::Find(b) => b.into_lua(lua),
			Self::FindArrow(b) => b.into_lua(lua),
			Self::FindDo(b) => b.into_lua(lua),
			Self::Follow(b) => b.into_lua(lua),
			Self::Forward(b) => b.into_lua(lua),
			Self::Hardlink(b) => b.into_lua(lua),
			Self::Hidden(b) => b.into_lua(lua),
			Self::Hover(b) => b.into_lua(lua),
			Self::Leave(b) => b.into_lua(lua),
			Self::Linemode(b) => b.into_lua(lua),
			Self::Link(b) => b.into_lua(lua),
			Self::Open(b) => b.into_lua(lua),
			Self::OpenDo(b) => b.into_lua(lua),
			Self::ParentArrow(b) => b.into_lua(lua),
			Self::Paste(b) => b.into_lua(lua),
			Self::Peek(b) => b.into_lua(lua),
			Self::Quit(b) => b.into_lua(lua),
			Self::Refresh(b) => b.into_lua(lua),
			Self::Remove(b) => b.into_lua(lua),
			Self::RemoveDo(b) => b.into_lua(lua),
			Self::Rename(b) => b.into_lua(lua),
			Self::Reveal(b) => b.into_lua(lua),
			Self::Search(b) => b.into_lua(lua),
			Self::SearchDo(b) => b.into_lua(lua),
			Self::SearchStop(b) => b.into_lua(lua),
			Self::Seek(b) => b.into_lua(lua),
			Self::Shell(b) => b.into_lua(lua),
			Self::Sort(b) => b.into_lua(lua),
			Self::Spot(b) => b.into_lua(lua),
			Self::Stash(b) => b.into_lua(lua),
			Self::Suspend(b) => b.into_lua(lua),
			Self::TabClose(b) => b.into_lua(lua),
			Self::TabCreate(b) => b.into_lua(lua),
			Self::TabRename(b) => b.into_lua(lua),
			Self::TabSwap(b) => b.into_lua(lua),
			Self::TabSwitch(b) => b.into_lua(lua),
			Self::Toggle(b) => b.into_lua(lua),
			Self::ToggleAll(b) => b.into_lua(lua),
			Self::Unyank(b) => b.into_lua(lua),
			Self::UpdateFiles(b) => b.into_lua(lua),
			Self::UpdateMimes(b) => b.into_lua(lua),
			Self::UpdatePaged(b) => b.into_lua(lua),
			Self::UpdatePeeked(b) => b.into_lua(lua),
			Self::UpdateSpotted(b) => b.into_lua(lua),
			Self::UpdateYanked(b) => b.into_lua(lua),
			Self::Upload(b) => b.into_lua(lua),
			Self::VisualMode(b) => b.into_lua(lua),
			Self::Watch(b) => b.into_lua(lua),
			Self::Yank(b) => b.into_lua(lua),

			// Cmp
			Self::CmpArrow(b) => b.into_lua(lua),
			Self::CmpClose(b) => b.into_lua(lua),
			Self::CmpShow(b) => b.into_lua(lua),
			Self::CmpTrigger(b) => b.into_lua(lua),

			// Confirm
			Self::ConfirmArrow(b) => b.into_lua(lua),
			Self::ConfirmClose(b) => b.into_lua(lua),
			Self::ConfirmShow(b) => b.into_lua(lua),

			// Help
			Self::HelpArrow(b) => b.into_lua(lua),
			Self::HelpEscape(b) => b.into_lua(lua),
			Self::HelpFilter(b) => b.into_lua(lua),
			Self::HelpToggle(b) => b.into_lua(lua),

			// Input
			Self::InputBackspace(b) => b.into_lua(lua),
			Self::InputBackward(b) => b.into_lua(lua),
			Self::InputClose(b) => b.into_lua(lua),
			Self::InputComplete(b) => b.into_lua(lua),
			Self::InputDelete(b) => b.into_lua(lua),
			Self::InputEscape(b) => b.into_lua(lua),
			Self::InputForward(b) => b.into_lua(lua),
			Self::InputInsert(b) => b.into_lua(lua),
			Self::InputKill(b) => b.into_lua(lua),
			Self::InputMove(b) => b.into_lua(lua),
			Self::InputPaste(b) => b.into_lua(lua),
			Self::InputShow(b) => b.into_lua(lua),

			// Notify
			Self::NotifyPush(b) => b.into_lua(lua),
			Self::NotifyTick(b) => b.into_lua(lua),

			// Pick
			Self::PickArrow(b) => b.into_lua(lua),
			Self::PickClose(b) => b.into_lua(lua),
			Self::PickShow(b) => b.into_lua(lua),

			// Spot
			Self::SpotArrow(b) => b.into_lua(lua),
			Self::SpotClose(b) => b.into_lua(lua),
			Self::SpotCopy(b) => b.into_lua(lua),
			Self::SpotSwipe(b) => b.into_lua(lua),

			// Tasks
			Self::TasksArrow(b) => b.into_lua(lua),
			Self::TasksCancel(b) => b.into_lua(lua),
			Self::TasksClose(b) => b.into_lua(lua),
			Self::TasksInspect(b) => b.into_lua(lua),
			Self::TasksOpenShellCompat(b) => b.into_lua(lua),
			Self::TasksProcessOpen(b) => b.into_lua(lua),
			Self::TasksShow(b) => b.into_lua(lua),
			Self::TasksUpdateSucceed(b) => b.into_lua(lua),

			// Which
			Self::WhichActivate(b) => b.into_lua(lua),
			Self::WhichDismiss(b) => b.into_lua(lua),
		}
	}
}

try_from_spark!(
	fb_parser::VoidOpt,
	app:bootstrap,
	app:focus,
	mgr:back,
	mgr:bulk_rename,
	mgr:enter,
	mgr:escape_filter,
	mgr:escape_find,
	mgr:escape_search,
	mgr:escape_select,
	mgr:escape_visual,
	mgr:follow,
	mgr:forward,
	mgr:leave,
	mgr:refresh,
	mgr:search_stop,
	mgr:suspend,
	mgr:unyank,
	mgr:watch,
	which:dismiss
);

// App
try_from_spark!(fb_parser::ArrowOpt, mgr:arrow, mgr:parent_arrow, mgr:tab_swap);
try_from_spark!(fb_parser::app::DeprecateOpt, app:deprecate);
try_from_spark!(fb_parser::app::MouseOpt, app:mouse);
try_from_spark!(fb_parser::app::PluginOpt, app:plugin, app:plugin_do);
try_from_spark!(fb_parser::app::QuitOpt, app:quit, mgr:quit);
try_from_spark!(fb_parser::app::ReflowOpt, app:reflow, app:resize);
try_from_spark!(fb_parser::app::ResumeOpt, app:resume);
try_from_spark!(fb_parser::app::StopOpt, app:stop);
try_from_spark!(fb_parser::app::TitleOpt, app:title);
try_from_spark!(fb_parser::app::UpdateProgressOpt, app:update_progress);
try_from_spark!(fb_parser::cmp::CloseOpt, cmp:close);
try_from_spark!(fb_parser::cmp::ShowOpt, cmp:show);
try_from_spark!(fb_parser::cmp::TriggerOpt, cmp:trigger);
try_from_spark!(fb_parser::confirm::CloseOpt, confirm:close);
try_from_spark!(fb_parser::confirm::ShowOpt, confirm:show);
try_from_spark!(fb_parser::help::ToggleOpt, help:toggle);
try_from_spark!(fb_parser::input::CloseOpt, input:close);
try_from_spark!(fb_parser::input::ShowOpt, input:show);
try_from_spark!(fb_parser::mgr::CdOpt, mgr:cd);
try_from_spark!(fb_parser::mgr::CloseOpt, mgr:close);
try_from_spark!(fb_parser::mgr::CopyOpt, mgr:copy);
try_from_spark!(fb_parser::mgr::CreateOpt, mgr:create);
try_from_spark!(fb_parser::mgr::DisplaceDoOpt, mgr:displace_do);
try_from_spark!(fb_parser::mgr::DownloadOpt, mgr:download);
try_from_spark!(fb_parser::mgr::EscapeOpt, mgr:escape);
try_from_spark!(fb_parser::mgr::FilterOpt, mgr:filter, mgr:filter_do);
try_from_spark!(fb_parser::mgr::FindArrowOpt, mgr:find_arrow);
try_from_spark!(fb_parser::mgr::FindDoOpt, mgr:find_do);
try_from_spark!(fb_parser::mgr::FindOpt, mgr:find);
try_from_spark!(fb_parser::mgr::HardlinkOpt, mgr:hardlink);
try_from_spark!(fb_parser::mgr::HiddenOpt, mgr:hidden);
try_from_spark!(fb_parser::mgr::HoverOpt, mgr:hover);
try_from_spark!(fb_parser::mgr::LinemodeOpt, mgr:linemode);
try_from_spark!(fb_parser::mgr::LinkOpt, mgr:link);
try_from_spark!(fb_parser::mgr::OpenDoOpt, mgr:open_do);
try_from_spark!(fb_parser::mgr::OpenOpt, mgr:open);
try_from_spark!(fb_parser::mgr::PasteOpt, mgr:paste);
try_from_spark!(fb_parser::mgr::PeekOpt, mgr:peek);
try_from_spark!(fb_parser::mgr::RemoveOpt, mgr:remove, mgr:remove_do);
try_from_spark!(fb_parser::mgr::RenameOpt, mgr:rename);
try_from_spark!(fb_parser::mgr::RevealOpt, mgr:reveal);
try_from_spark!(fb_parser::mgr::SearchOpt, mgr:search, mgr:search_do);
try_from_spark!(fb_parser::mgr::SeekOpt, mgr:seek);
try_from_spark!(fb_parser::mgr::ShellOpt, mgr:shell);
try_from_spark!(fb_parser::mgr::SortOpt, mgr:sort);
try_from_spark!(fb_parser::mgr::SpotOpt, mgr:spot);
try_from_spark!(fb_parser::mgr::StashOpt, mgr:stash);
try_from_spark!(fb_parser::mgr::TabCloseOpt, mgr:tab_close);
try_from_spark!(fb_parser::mgr::TabCreateOpt, mgr:tab_create);
try_from_spark!(fb_parser::mgr::TabRenameOpt, mgr:tab_rename);
try_from_spark!(fb_parser::mgr::TabSwitchOpt, mgr:tab_switch);
try_from_spark!(fb_parser::mgr::ToggleAllOpt, mgr:toggle_all);
try_from_spark!(fb_parser::mgr::ToggleOpt, mgr:toggle);
try_from_spark!(fb_parser::mgr::UpdateFilesOpt, mgr:update_files);
try_from_spark!(fb_parser::mgr::UpdateMimesOpt, mgr:update_mimes);
try_from_spark!(fb_parser::mgr::UpdatePagedOpt, mgr:update_paged);
try_from_spark!(fb_parser::mgr::UpdatePeekedOpt, mgr:update_peeked);
try_from_spark!(fb_parser::mgr::UpdateSpottedOpt, mgr:update_spotted);
try_from_spark!(fb_parser::mgr::UpdateYankedOpt<'a>, mgr:update_yanked);
try_from_spark!(fb_parser::mgr::UploadOpt, mgr:upload);
try_from_spark!(fb_parser::mgr::VisualModeOpt, mgr:visual_mode);
try_from_spark!(fb_parser::mgr::YankOpt, mgr:yank);
try_from_spark!(fb_parser::notify::PushOpt, notify:push);
try_from_spark!(fb_parser::notify::TickOpt, notify:tick);
try_from_spark!(fb_parser::pick::CloseOpt, pick:close);
try_from_spark!(fb_parser::pick::ShowOpt, pick:show);
try_from_spark!(fb_parser::spot::CopyOpt, spot:copy);
try_from_spark!(fb_parser::tasks::ProcessOpenOpt, tasks:process_open);
try_from_spark!(fb_parser::tasks::UpdateSucceedOpt, tasks:update_succeed);
try_from_spark!(fb_parser::which::ActivateOpt, which:activate);
try_from_spark!(fb_widgets::input::parser::BackspaceOpt, input:backspace);
try_from_spark!(fb_widgets::input::parser::BackwardOpt, input:backward);
try_from_spark!(fb_widgets::input::parser::CompleteOpt, input:complete);
try_from_spark!(fb_widgets::input::parser::DeleteOpt, input:delete);
try_from_spark!(fb_widgets::input::parser::ForwardOpt, input:forward);
try_from_spark!(fb_widgets::input::parser::InsertOpt, input:insert);
try_from_spark!(fb_widgets::input::parser::KillOpt, input:kill);
try_from_spark!(fb_widgets::input::parser::MoveOpt, input:move);
try_from_spark!(fb_widgets::input::parser::PasteOpt, input:paste);

