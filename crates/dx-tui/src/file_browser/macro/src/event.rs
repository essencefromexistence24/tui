#[macro_export]
macro_rules! emit {
	(Call($action:expr)) => {
		fb_shared::event::Event::Call(fb_shared::event::ActionCow::from($action)).emit();
	};
	(Seq($actions:expr)) => {
		fb_shared::event::Event::Seq($actions).emit();
	};
	($event:ident) => {
		fb_shared::event::Event::$event.emit();
	};
}

#[macro_export]
macro_rules! relay {
	($layer:ident : $name:ident) => {
		fb_shared::event::Action::new_relay(concat!(stringify!($layer), ":", stringify!($name)))
	};
	($layer:ident : $name:ident, $args:expr) => {
		fb_shared::event::Action::new_relay_args(
			concat!(stringify!($layer), ":", stringify!($name)),
			$args,
		)
	};
}

#[macro_export]
macro_rules! succ {
	($data:expr) => {
		return Ok(fb_shared::data::Data::from($data))
	};
	() => {
		return Ok(fb_shared::data::Data::Nil)
	};
}
