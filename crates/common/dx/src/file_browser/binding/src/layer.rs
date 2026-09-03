use mlua::{MetaMethod, UserData};

#[derive(Clone, Copy)]
pub struct Layer(fb_shared::Layer);

impl From<fb_shared::Layer> for Layer {
	fn from(event: fb_shared::Layer) -> Self { Self(event) }
}

impl UserData for Layer {
	fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
		methods.add_meta_method(MetaMethod::ToString, |_, me, ()| Ok(me.0.to_string()));
	}
}

