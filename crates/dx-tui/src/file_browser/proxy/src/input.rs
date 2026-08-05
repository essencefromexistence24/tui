use tokio::sync::mpsc;
use fb_config::popup::InputCfg;
use fb_macro::{emit, relay};
use fb_widgets::input::InputError;

pub struct InputProxy;

impl InputProxy {
	pub fn show(cfg: InputCfg) -> mpsc::UnboundedReceiver<Result<String, InputError>> {
		let (tx, rx) = mpsc::unbounded_channel();
		emit!(Call(relay!(input:show).with_any("tx", tx).with_any("cfg", cfg)));
		rx
	}
}

