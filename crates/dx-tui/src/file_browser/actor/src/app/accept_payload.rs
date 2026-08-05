use anyhow::Result;
use mlua::IntoLua;
use tracing::error;
use fb_actor::lives::Lives;
use fb_binding::runtime_scope;
use fb_dds::{LOCAL, Payload, REMOTE};
use fb_macro::succ;
use fb_plugin::LUA;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct AcceptPayload;

impl Actor for AcceptPayload {
	type Options = Payload<'static>;

	const NAME: &str = "accept_payload";

	fn act(cx: &mut Ctx, payload: Payload) -> Result<Data> {
		let kind = payload.body.kind();
		let lock = if payload.receiver == 0 || payload.receiver != payload.sender {
			REMOTE.read()
		} else {
			LOCAL.read()
		};

		let Some(handlers) = lock.get(kind).filter(|&m| !m.is_empty()).cloned() else { succ!() };
		drop(lock);

		let kind = kind.to_owned();
		succ!(Lives::scope(cx.core, || {
			let body = payload.body.into_lua(&LUA)?;
			for (id, cb) in handlers {
				if let Err(e) = runtime_scope!(LUA, &id, cb.call::<()>(body.clone())) {
					error!("Failed to run `{kind}` event handler in your `{id}` plugin: {e}");
				}
			}
			Ok(())
		})?);
	}
}

