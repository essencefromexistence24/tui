use ratatui::style::Color;
use std::time::{Duration, Instant};

pub use crate::theme::RgbColor;

impl From<Color> for RgbColor {
	fn from(color: Color) -> Self {
		match color {
			Color::Rgb(r, g, b) => Self { r, g, b },
			_ => Self { r: 255, g: 255, b: 255 },
		}
	}
}

#[derive(Debug, Clone)]
pub struct ShimmerEffect {
	colors: Vec<Color>,
	start_time: Instant,
	duration: Duration,
}

impl ShimmerEffect {
	pub fn new(colors: Vec<Color>) -> Self {
		Self { colors, start_time: Instant::now(), duration: Duration::from_millis(1500) }
	}

	/// Get shimmer color at a specific position (0.0 to 1.0)
	/// This creates a moving gradient effect across the text
	pub fn shimmer_color_at(&self, position: f32) -> Color {
		let elapsed = self.start_time.elapsed().as_millis() as f32;
		let cycle = (elapsed % self.duration.as_millis() as f32) / self.duration.as_millis() as f32;

		// Create a moving wave: the shimmer moves from -1.0 to 2.0 across the text
		// This creates the "sweep" effect
		let wave_position = -1.0 + (cycle * 3.0);

		// Calculate distance from the wave center
		let distance = (position - wave_position).abs();

		// Create a gradient: bright in the center, fading to base color
		if distance < 0.3 {
			// In the shimmer zone - interpolate between base and highlight
			let t = 1.0 - (distance / 0.3);
			// Bright white/light color at the center
			let highlight = Color::Rgb(255, 255, 255);
			let base = self.colors[0];
			self.interpolate_color(base, highlight, t * 0.7) // 0.7 for subtle effect
		} else {
			// Outside shimmer zone - use base color
			self.colors[0]
		}
	}

	pub fn current_color(&self) -> Color {
		// For backward compatibility - return color at center position
		self.shimmer_color_at(0.5)
	}

	fn interpolate_color(&self, c1: Color, c2: Color, t: f32) -> Color {
		match (c1, c2) {
			(Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
				let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8;
				let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8;
				let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8;
				Color::Rgb(r, g, b)
			}
			_ => c1,
		}
	}

	pub fn reset(&mut self) {
		self.start_time = Instant::now();
	}
}

#[derive(Debug, Clone)]
pub struct TypingIndicator {
	dots: usize,
	last_update: Instant,
	interval: Duration,
}

impl TypingIndicator {
	pub fn new() -> Self {
		Self { dots: 0, last_update: Instant::now(), interval: Duration::from_millis(500) }
	}

	pub fn update(&mut self) {
		if self.last_update.elapsed() >= self.interval {
			self.dots = (self.dots + 1) % 4;
			self.last_update = Instant::now();
		}
	}

	pub fn text(&self, is_visible: bool) -> String {
		if is_visible {
			match self.dots {
				0 => "".to_string(),
				1 => ".".to_string(),
				2 => "..".to_string(),
				_ => "...".to_string(),
			}
		} else {
			String::new()
		}
	}

	pub fn is_visible(&self) -> bool {
		// Blink every 500ms
		(self.last_update.elapsed().as_millis() / 500).is_multiple_of(2)
	}
}

impl Default for TypingIndicator {
	fn default() -> Self {
		Self::new()
	}
}

#[derive(Debug, Clone)]
pub struct RainbowEffect {
	start_time: Instant,
	speed: f32,
}

impl RainbowEffect {
	pub fn new() -> Self {
		Self {
			start_time: Instant::now(),
			speed: 0.5, // 0.5 cycles per second (slower)
		}
	}

	pub fn elapsed(&self) -> f32 {
		self.start_time.elapsed().as_secs_f32()
	}

	pub fn current_color(&self) -> Color {
		let elapsed = self.start_time.elapsed().as_secs_f32();
		let hue = (elapsed * self.speed * 360.0) % 360.0;

		Self::hsl_to_rgb(hue, 0.8, 0.6)
	}

	pub fn color_at(&self, index: usize) -> Color {
		let elapsed = self.start_time.elapsed().as_secs_f32();
		let hue = ((elapsed * self.speed * 360.0) + (index as f32 * 10.0)) % 360.0;
		Self::hsl_to_rgb(hue, 0.8, 0.6)
	}

	pub fn rgb_color_at(&self, index: usize) -> RgbColor {
		self.color_at(index).into()
	}

	fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Color {
		let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
		let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
		let m = l - c / 2.0;

		let (r, g, b) = if h < 60.0 {
			(c, x, 0.0)
		} else if h < 120.0 {
			(x, c, 0.0)
		} else if h < 180.0 {
			(0.0, c, x)
		} else if h < 240.0 {
			(0.0, x, c)
		} else if h < 300.0 {
			(x, 0.0, c)
		} else {
			(c, 0.0, x)
		};

		Color::Rgb(((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8)
	}
}

impl Default for RainbowEffect {
	fn default() -> Self {
		Self::new()
	}
}
