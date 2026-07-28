//! Production message-stream UI: structured parts, professional cards, clean copy.
//!
//! **Paint source of truth:** `Message.parts` (`Vec<StreamPart>`), kept in sync
//! with wire `Message.content` for session persistence. Stream events and
//! fence appends update both.

mod ansi;
mod branch_ui;
mod copy;
mod diff_review;
mod live;
mod parse;
mod pty_host;
mod render;
mod vt_grid;

pub use ansi::ansi_line;
pub use branch_ui::{BranchPickerState, list_branches, render_branch_picker, render_branch_rail};
pub use copy::clean_copy_text;
pub use diff_review::{accept_diff_path, extract_diff_path, reject_unified_diff};
pub use live::{
	append_text_part, append_thinking_part, append_tool_body, close_subagent, open_subagent,
	parts_to_wire, push_approval, push_pty, push_question, rebuild_parts, resolve_approval,
};
pub use parse::{PlanStep, StreamPart, extract_attr};
pub use pty_host::PtyHost;
pub use render::{RenderCtx, render_parts_list, render_parts_tagged};
