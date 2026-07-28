// Menu animation effects
use super::menu_data::Menu;
use crate::theme::ChatTheme;
use ratatui::style::Style;
use tachyonfx::{
	Duration, Effect,
	Interpolation::*,
	Motion, SimpleRng, color_from_hsl,
	fx::{self, ExpandDirection},
};

pub(super) struct EffectsRepository {
	opening_effects: Vec<(&'static str, Effect)>,
	closing_effects: Vec<(&'static str, Effect)>,
}

impl EffectsRepository {
	pub(super) fn new(theme: ChatTheme, rng: &mut SimpleRng) -> Self {
		let screen_bg = theme.bg;
		let slow = Duration::from_millis(2000);
		let medium = Duration::from_millis(1200);

		let mut random_color = || {
			let hue = (rng.r#gen() % 360) as f32;
			color_from_hsl(hue, 70.0, 60.0)
		};

		let color1 = random_color();
		let color2 = random_color();
		let color3 = random_color();
		let color4 = random_color();

		let opening_effects = vec![
			(
				"sweep in left to right",
				fx::sweep_in(Motion::LeftToRight, 30, 0, screen_bg, (slow, QuadOut)),
			),
			(
				"sweep in right to left",
				fx::sweep_in(Motion::RightToLeft, 30, 0, screen_bg, (slow, QuadOut)),
			),
			("sweep in top to bottom", fx::sweep_in(Motion::DownToUp, 30, 0, screen_bg, (slow, QuadOut))),
			("sweep in bottom to top", fx::sweep_in(Motion::UpToDown, 30, 0, screen_bg, (slow, QuadOut))),
			("slide in from bottom", fx::slide_in(Motion::UpToDown, 20, 0, screen_bg, (medium, QuadOut))),
			("slide in from top", fx::slide_in(Motion::DownToUp, 20, 0, screen_bg, (medium, QuadOut))),
			(
				"slide in from left",
				fx::slide_in(Motion::LeftToRight, 20, 0, screen_bg, (medium, QuadOut)),
			),
			(
				"slide in from right",
				fx::slide_in(Motion::RightToLeft, 20, 0, screen_bg, (medium, QuadOut)),
			),
			(
				"expand vertical",
				fx::expand(ExpandDirection::Vertical, Style::new().fg(color1).bg(screen_bg), 1200),
			),
			(
				"expand horizontal",
				fx::expand(ExpandDirection::Horizontal, Style::new().fg(color2).bg(screen_bg), 1200),
			),
			("coalesce", fx::coalesce((medium, CubicOut))),
		];

		let closing_effects = vec![
			("sweep out down to up", fx::sweep_out(Motion::DownToUp, 45, 0, color1, (slow, QuadOut))),
			("sweep out up to down", fx::sweep_out(Motion::UpToDown, 45, 0, color2, (slow, QuadOut))),
			(
				"sweep out left to right",
				fx::sweep_out(Motion::LeftToRight, 45, 0, color3, (slow, QuadOut)),
			),
			(
				"sweep out right to left",
				fx::sweep_out(Motion::RightToLeft, 45, 0, color4, (slow, QuadOut)),
			),
			(
				"slide out to right",
				fx::slide_out(Motion::LeftToRight, 80, 0, screen_bg, (medium, QuadIn)),
			),
			("slide out to left", fx::slide_out(Motion::RightToLeft, 80, 0, screen_bg, (medium, QuadIn))),
			("slide out to top", fx::slide_out(Motion::DownToUp, 80, 0, screen_bg, (medium, QuadIn))),
			("slide out to bottom", fx::slide_out(Motion::UpToDown, 80, 0, screen_bg, (medium, QuadIn))),
			(
				"shrink vertical",
				fx::expand(ExpandDirection::Vertical, Style::new().fg(color2).bg(screen_bg), 1200)
					.reversed(),
			),
			(
				"shrink horizontal",
				fx::expand(ExpandDirection::Horizontal, Style::new().fg(color3).bg(screen_bg), 1200)
					.reversed(),
			),
		];

		Self { opening_effects, closing_effects }
	}

	pub(super) fn get_random_opening_effect(&self, rng: &mut SimpleRng) -> (&'static str, Effect) {
		let idx = (rng.r#gen() % self.opening_effects.len() as u32) as usize;
		self.opening_effects[idx].clone()
	}

	pub(super) fn get_random_closing_effect(&self, rng: &mut SimpleRng) -> (&'static str, Effect) {
		let idx = (rng.r#gen() % self.closing_effects.len() as u32) as usize;
		self.closing_effects[idx].clone()
	}
}

impl Menu {
	pub fn pick_opening_effect(&mut self) {
		self.effects = EffectsRepository::new(self.theme.clone(), &mut self.rng);
		self.active_effect = self.effects.get_random_opening_effect(&mut self.rng);
		self.auto_cycle_timer = Duration::ZERO;
	}

	pub fn pick_closing_effect(&mut self) {
		self.effects = EffectsRepository::new(self.theme.clone(), &mut self.rng);
		self.active_effect = self.effects.get_random_closing_effect(&mut self.rng);
		self.auto_cycle_timer = Duration::ZERO;
	}
}
