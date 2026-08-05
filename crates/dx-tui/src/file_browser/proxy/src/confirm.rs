use fb_config::popup::ConfirmCfg;
use fb_macro::{emit, relay};
use fb_shared::CompletionToken;

pub struct ConfirmProxy;

impl ConfirmProxy {
	pub async fn show(cfg: ConfirmCfg) -> bool { Self::show_sync(cfg).future().await }

	pub fn show_sync(cfg: ConfirmCfg) -> CompletionToken {
		let token = CompletionToken::default();
		emit!(Call(relay!(confirm:show).with_any("cfg", cfg).with_any("token", token.clone())));
		token
	}
}

