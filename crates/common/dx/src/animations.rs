//! Animation rendering functions adapted for Buffer rendering

use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Modifier, Style},
	text::{Line, Span},
	widgets::{Paragraph, Widget},
};

use super::state::ChatState;

const TRAIN: &[&str] = &[
	"      ====        ________                ___________",
	"  _D _|  |_______/        \\__I_I_____===__|_________|",
	"   |(_)---  |   H\\________/ |   |        =|___ ___|",
	"   /     |  |   H  |  |     |   |         ||_| |_||",
	"  |      |  |   H  |__--------------------| [___] |",
	"  | ________|___H__/__|_____/[][]~\\_______|       |",
	"  |/ |   |-----------I_____I [][] []  D   |=======|",
	"__/ =| o |=-~~\\  /~~\\  /~~\\  /~~\\ ____Y___________|",
	" |/-=|___|=O=====O=====O=====O   |_____/~\\___/",
	"  \\_/      \\__/  \\__/  \\__/  \\__/      \\_/",
];
const SMOKE: &[&[&str]] = &[
	&["    (  )", "   (    )", "  (      )"],
	&["   (   )", "  (     )", " (       )"],
	&["  (    )", " (      )", "(        )"],
];
const TRAIN_COLUMN_MS: u64 = 35;
const TRAIN_START_RIGHT_GUTTER_COLUMNS: u64 = 6;
const TRAIN_LOOP_GUTTER_COLUMNS: u64 = 12;
const ANIMATION_TICK_MS: u64 = 50;

pub(crate) fn train_exit_duration_for_width(width: u16) -> std::time::Duration {
	let visible_columns = u64::from(width) + train_width_columns() + TRAIN_START_RIGHT_GUTTER_COLUMNS;
	std::time::Duration::from_millis((visible_columns * TRAIN_COLUMN_MS) + (2 * ANIMATION_TICK_MS))
}

fn train_width_columns() -> u64 {
	TRAIN.iter().map(|line| line.chars().count() as u64).max().unwrap_or(0)
}

fn train_loop_progress(width: u16, elapsed_ms: usize) -> i32 {
	let travel = u64::from(width) + train_width_columns() + TRAIN_LOOP_GUTTER_COLUMNS;
	((elapsed_ms as u64 / TRAIN_COLUMN_MS) % travel.max(1)) as i32
}

fn train_exit_progress(width: u16, elapsed_ms: usize) -> i32 {
	let visible_columns = u64::from(width) + train_width_columns() + TRAIN_START_RIGHT_GUTTER_COLUMNS;
	let duration_ms = train_exit_duration_for_width(width).as_millis().max(1);
	((elapsed_ms as u128).min(duration_ms) * u128::from(visible_columns) / duration_ms) as i32
}

impl ChatState {
	/// Helper: get a rainbow color as ratatui Color
	pub fn rainbow_color(&self, index: usize) -> ratatui::style::Color {
		let c = self.animation.rainbow_animation.rgb_color_at(index);
		ratatui::style::Color::Rgb(c.r, c.g, c.b)
	}

	/// Solid theme background so theme switches repaint the full TUI surface.
	pub fn theme_bg_color(&self) -> ratatui::style::Color {
		self.theme.bg
	}

	fn ensure_frame_buffer(&mut self, area: Rect) {
		let w = area.width as usize;
		let h = area.height as usize;
		if w != self.animation.frame_buffer_width as usize
			|| h != self.animation.frame_buffer_height as usize
		{
			self.animation.frame_buffer = vec![vec![(' ', ratatui::style::Color::Reset); w]; h];
			self.animation.frame_buffer_width = area.width;
			self.animation.frame_buffer_height = area.height;
		}
	}

	pub fn render_matrix_animation_in_area(&mut self, area: Rect, buf: &mut Buffer) {
		use ratatui::style::Color;

		let bg_color = self.theme_bg_color();

		// Authentic Matrix characters
		let chars = [
			'ﾊ', 'ﾐ', 'ﾋ', 'ｰ', 'ｳ', 'ｼ', 'ﾅ', 'ﾓ', 'ﾆ', 'ｻ', 'ﾜ', 'ﾂ', 'ｵ', 'ﾘ', 'ｱ', 'ﾎ', 'ﾃ', 'ﾏ',
			'ｹ', 'ﾒ', 'ｴ', 'ｶ', 'ｷ', 'ﾑ', 'ﾕ', 'ﾗ', 'ｾ', 'ﾈ', 'ｽ', 'ﾀ', 'ﾇ', 'ﾍ', '0', '1', '2', '3',
			'4', '5', '6', '7', '8', '9', ':', '.', '"', '=', '*', '+', '-', '<', '>', '¦', '|', 'Z',
		];

		let elapsed_ms =
			self.animation.animation_start_time.map(|t| t.elapsed().as_millis() as usize).unwrap_or(0);

		self.ensure_frame_buffer(area);

		let w = area.width as usize;
		let h = area.height as usize;
		let green_g = 255;

		// Reset buffer with background color
		for row in self.animation.frame_buffer.iter_mut().take(h) {
			for cell in row.iter_mut().take(w) {
				*cell = (' ', bg_color);
			}
		}

		for x in 0..area.width {
			if (x * 7) % 3 != 0 {
				continue;
			}

			let column_speed = 1 + ((x * 11) % 2) as usize;
			let column_length = 8 + ((x * 13) % 12);
			let column_offset = (x * 17) % 40;

			let fall_progress = ((elapsed_ms / (150 / column_speed)) + column_offset as usize) as i32;
			let head_y = (fall_progress % (area.height as i32 + 30)) - 10;

			for trail_pos in 0..column_length {
				let y = head_y - trail_pos as i32;

				if y >= 0 && y < area.height as i32 {
					let char_idx = (x as usize * 31 + y as usize * 17 + elapsed_ms / 200) % chars.len();

					let color = if trail_pos == 0 {
						Color::Rgb(200, 255, 200)
					} else {
						let fade = 1.0 - (trail_pos as f32 / column_length as f32) * 0.85;
						Color::Rgb(0, (green_g as f32 * fade) as u8, 0)
					};

					self.animation.frame_buffer[y as usize][x as usize] = (chars[char_idx], color);
				}
			}
		}

		let mut lines = Vec::new();
		for y in 0..h {
			let mut spans = Vec::new();
			for x in 0..w {
				let (ch, color) = self.animation.frame_buffer[y][x];
				spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}

	pub fn render_train_animation_in_area(&self, area: Rect, buf: &mut Buffer) {
		if area.width == 0 || area.height == 0 {
			return;
		}

		let bg_color = self.theme_bg_color();
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				buf[(x, y)].reset();
				buf[(x, y)].set_bg(bg_color);
			}
		}

		let elapsed_ms = if self.animation.playing_outro {
			self
				.animation
				.animation_start_time
				.map(|start| start.elapsed().as_millis() as usize)
				.unwrap_or(0)
		} else {
			(self.animation.rainbow_animation.elapsed() * 1000.0) as usize
		};
		let progress = if self.animation.playing_outro {
			train_exit_progress(area.width, elapsed_ms)
		} else {
			train_loop_progress(area.width, elapsed_ms)
		};
		let train_x = area.width as i32 + 6 - progress;
		let content_height = (SMOKE[0].len() + TRAIN.len() + 2) as i32;
		let top = ((area.height as i32 - content_height) / 2).max(0);
		let smoke = SMOKE[(elapsed_ms / 240) % SMOKE.len()];

		for (line_idx, line) in smoke.iter().enumerate() {
			self.render_train_line(area, buf, top + line_idx as i32, train_x + 6, line, elapsed_ms / 200);
		}

		let train_top = top + smoke.len() as i32 + 1;
		for (line_idx, line) in TRAIN.iter().enumerate() {
			self.render_train_line(
				area,
				buf,
				train_top + line_idx as i32,
				train_x,
				line,
				line_idx * 3 + elapsed_ms / 150,
			);
		}

		let track_y = train_top + TRAIN.len() as i32;
		if track_y >= 0 && track_y < area.height as i32 {
			for x in 0..area.width {
				let ch = if (x as usize + elapsed_ms / 300).is_multiple_of(4) { '+' } else { '=' };
				let color = self.rainbow_color((x as usize + elapsed_ms / 300) % 50);
				let cell = &mut buf[(area.x + x, area.y + track_y as u16)];
				cell.set_char(ch);
				cell.set_style(Style::default().fg(color).bg(bg_color));
			}
		}
	}

	fn render_train_line(
		&self,
		area: Rect,
		buf: &mut Buffer,
		y: i32,
		start_x: i32,
		line: &str,
		color_seed: usize,
	) {
		if y < 0 || y >= area.height as i32 {
			return;
		}

		let bg_color = self.theme_bg_color();
		for (char_index, ch) in line.chars().enumerate() {
			let x = start_x + char_index as i32;
			if x < 0 || x >= area.width as i32 {
				continue;
			}

			let color = self.rainbow_color((color_seed + char_index) % 50);
			let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
			cell.set_char(ch);
			cell.set_style(Style::default().fg(color).bg(bg_color));
		}
	}
}

impl ChatState {
	pub fn render_confetti_animation_in_area(&mut self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let elapsed_ms = (elapsed * 1000.0) as u64;
		let bg_color = self.theme_bg_color();

		let confetti_chars =
			['*', '+', 'o', '~', '#', '@', '%', '&', '!', '^', '▪', '▫', '●', '◆', '◇'];

		let w = area.width as usize;
		let h = area.height as usize;

		self.ensure_frame_buffer(area);

		// Reset buffer to empty
		for row in self.animation.frame_buffer.iter_mut().take(h) {
			for cell in row.iter_mut().take(w) {
				*cell = (' ', bg_color);
			}
		}

		// Multiple explosion sources that trigger periodically
		let num_explosions = 3;
		let explosion_cycle_ms: u64 = 5000;

		for explosion_id in 0..num_explosions {
			// Each explosion has a different center and timing
			let explosion_offset = explosion_id * (explosion_cycle_ms / num_explosions);
			let local_time = (elapsed_ms.wrapping_add(explosion_offset)) % explosion_cycle_ms;
			let age = local_time as f64 / 1000.0; // seconds since this explosion

			// Explosion center - varies per explosion
			let seed = explosion_id;
			let center_x = match explosion_id {
				0 => w as f64 / 2.0,
				1 => w as f64 / 4.0,
				_ => 3.0 * w as f64 / 4.0,
			};
			let center_y = match explosion_id {
				0 => h as f64 / 3.0,
				1 => h as f64 / 2.0,
				_ => h as f64 / 4.0,
			};

			let num_particles = 80;
			let gravity = 12.0;
			let air_drag: f64 = 0.97;

			for i in 0..num_particles {
				let particle_seed = seed * 1000 + i as u64;

				// Explosion: particles burst outward from center in all directions
				// Use golden angle for even distribution
				let angle = (i as f64 * 2.39996322) + (seed as f64 * 1.7);
				let speed_base = 8.0 + ((particle_seed * 7919) % 200) as f64 / 10.0; // 8-28 units/s
				let speed_variation = 1.0 + ((particle_seed * 3571) % 100) as f64 / 200.0;
				let speed = speed_base * speed_variation;

				let vx = angle.cos() * speed * 1.8; // wider horizontal spread for terminal
				let vy = angle.sin() * speed * 0.8 - 5.0; // bias upward initially

				// Physics with drag: approximate position
				// With drag factor d per second: v(t) = v0 * d^t, x(t) = v0 * (d^t - 1) / ln(d)
				let drag_t = air_drag.powf(age * 30.0); // approximate drag over frames
				let px = center_x + vx * age * drag_t;
				let py = center_y + vy * age * drag_t + 0.5 * gravity * age * age;

				let px_i = px as i32;
				let py_i = py as i32;

				if px_i >= 0 && px_i < w as i32 && py_i >= 0 && py_i < h as i32 {
					// Fade out over time
					let fade = (1.0 - age / (explosion_cycle_ms as f64 / 1000.0)).max(0.0);
					if fade > 0.05 {
						let char_idx =
							(particle_seed as usize + (elapsed_ms / 150) as usize) % confetti_chars.len();
						let color_idx = (i * 7 + explosion_id as usize * 13 + (elapsed_ms / 60) as usize) % 50;
						let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
						let color = ratatui::style::Color::Rgb(
							(c.r as f64 * fade) as u8,
							(c.g as f64 * fade) as u8,
							(c.b as f64 * fade) as u8,
						);

						// Spinning character effect based on particle rotation
						let spin_chars = ['|', '/', '─', '\\'];
						let spin_idx = ((age * 8.0) as usize + i) % spin_chars.len();
						let ch = if i % 3 == 0 { spin_chars[spin_idx] } else { confetti_chars[char_idx] };

						self.animation.frame_buffer[py_i as usize][px_i as usize] = (ch, color);
					}
				}
			}
		}

		// Render sparkle effects at explosion centers during initial burst
		for explosion_id in 0..num_explosions {
			let explosion_offset = explosion_id * (explosion_cycle_ms / num_explosions);
			let local_time = (elapsed_ms.wrapping_add(explosion_offset)) % explosion_cycle_ms;

			if local_time < 300 {
				let center_x = match explosion_id {
					0 => w / 2,
					1 => w / 4,
					_ => 3 * w / 4,
				};
				let center_y = match explosion_id {
					0 => h / 3,
					1 => h / 2,
					_ => h / 4,
				};

				// Bright flash at center
				let flash_chars = ['✦', '✧', '★', '☆', '✴', '✵'];
				let flash_radius = (local_time as f64 / 100.0) as i32 + 1;
				for dy in -flash_radius..=flash_radius {
					for dx in -flash_radius..=flash_radius {
						let fx = (center_x as i32 + dx) as usize;
						let fy = (center_y as i32 + dy) as usize;
						if fx < w && fy < h && (dx * dx + dy * dy) <= flash_radius * flash_radius {
							let flash_idx = ((dx.unsigned_abs() + dy.unsigned_abs()) as usize
								+ (elapsed_ms / 50) as usize)
								% flash_chars.len();
							let brightness = 1.0 - (local_time as f64 / 300.0);
							let color_idx = (fx + fy + (elapsed_ms / 40) as usize) % 50;
							let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
							let color = ratatui::style::Color::Rgb(
								(c.r as f64 * brightness + 255.0 * (1.0 - brightness) * brightness) as u8,
								(c.g as f64 * brightness + 255.0 * (1.0 - brightness) * brightness) as u8,
								(c.b as f64 * brightness + 255.0 * (1.0 - brightness) * brightness) as u8,
							);
							self.animation.frame_buffer[fy][fx] = (flash_chars[flash_idx], color);
						}
					}
				}
			}
		}

		let mut lines = Vec::new();
		for y in 0..h {
			let mut spans = Vec::new();
			for x in 0..w {
				let (ch, color) = self.animation.frame_buffer[y][x];
				spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}

	pub fn render_gameoflife_animation_in_area(&mut self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let bg_color = self.theme_bg_color();

		let w = area.width as usize;
		let h = area.height as usize;

		if w == 0 || h == 0 {
			return;
		}

		// Use a simple deterministic hash to create evolving patterns
		// Instead of recomputing from seed each frame, we compute generation N
		// using a fast cellular automaton that's computed incrementally
		let generation = ((elapsed * 1000.0) / 150.0) as usize; // ~6.6 fps evolution

		// Initialize grid from a deterministic seed
		let mut grid = vec![vec![false; w]; h];

		// Seed with a pseudo-random but deterministic pattern
		// Use different seed patterns that create interesting evolution
		let seed_gen = generation / 200; // restart every 200 generations for variety
		let local_gen = generation % 200;

		// Create initial pattern based on seed_gen for variety
		let seed_hash = seed_gen.wrapping_mul(2654435761);
		for (y, row) in grid.iter_mut().enumerate().take(h) {
			for (x, cell) in row.iter_mut().enumerate().take(w) {
				let hash = (x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263) ^ seed_hash)
					.wrapping_mul(2246822519);
				// ~25% fill rate for interesting patterns
				*cell = (hash % 100) < 25;
			}
		}

		// Place some classic patterns for visual interest
		let place = |grid: &mut Vec<Vec<bool>>, cx: usize, cy: usize, pattern: &[(i32, i32)]| {
			for &(dx, dy) in pattern {
				let x = (cx as i32 + dx).rem_euclid(w as i32) as usize;
				let y = (cy as i32 + dy).rem_euclid(h as i32) as usize;
				grid[y][x] = true;
			}
		};

		// R-pentomino creates chaos
		let r_pentomino = [(0, -1), (1, -1), (-1, 0), (0, 0), (0, 1)];
		place(&mut grid, w / 2, h / 2, &r_pentomino);

		// Evolve for local_gen steps (capped for performance)
		let actual_steps = local_gen.min(60);
		for _ in 0..actual_steps {
			let mut new_grid = vec![vec![false; w]; h];
			for y in 0..h {
				for x in 0..w {
					let mut neighbors = 0u8;
					for dy in [h - 1, 0, 1] {
						for dx in [w - 1, 0, 1] {
							if dy == 0 && dx == 0 {
								continue;
							}
							if grid[(y + dy) % h][(x + dx) % w] {
								neighbors += 1;
							}
						}
					}
					new_grid[y][x] = if grid[y][x] { matches!(neighbors, 2 | 3) } else { neighbors == 3 };
				}
			}
			grid = new_grid;
		}

		// Count neighbors for glow effect
		let mut neighbor_count = vec![vec![0u8; w]; h];
		for y in 0..h {
			for x in 0..w {
				let mut count = 0u8;
				for dy in [h - 1, 0, 1] {
					for dx in [w - 1, 0, 1] {
						if dy == 0 && dx == 0 {
							continue;
						}
						if grid[(y + dy) % h][(x + dx) % w] {
							count += 1;
						}
					}
				}
				neighbor_count[y][x] = count;
			}
		}

		// Pulsing effect based on elapsed time
		let pulse = (elapsed * 3.0).sin() * 0.3 + 0.7;

		self.ensure_frame_buffer(area);

		// Write GoL output to pre-allocated frame buffer
		for y in 0..h {
			for x in 0..w {
				if grid[y][x] {
					let color_idx = (x * 3 + y * 7 + (elapsed * 5.0) as usize) % 50;
					let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
					let color = ratatui::style::Color::Rgb(
						(c.r as f32 * pulse) as u8,
						(c.g as f32 * pulse) as u8,
						(c.b as f32 * pulse) as u8,
					);
					let ch = match neighbor_count[y][x] {
						0 | 1 => '·',
						2 => '●',
						3 => '◉',
						_ => '★',
					};
					self.animation.frame_buffer[y][x] = (ch, color);
				} else if neighbor_count[y][x] > 0 {
					let glow_intensity = neighbor_count[y][x] as f32;
					let glow_pulse = (elapsed * 4.0 + x as f32 * 0.1 + y as f32 * 0.1).sin() * 0.5 + 0.5;
					let color_idx = (x + y + (elapsed * 2.0) as usize) % 50;
					let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
					let dim = 0.12 * glow_intensity * glow_pulse;
					let glow_color = ratatui::style::Color::Rgb(
						(c.r as f32 * dim) as u8,
						(c.g as f32 * dim) as u8,
						(c.b as f32 * dim) as u8,
					);
					let glow_ch = if glow_pulse > 0.6 { '·' } else { '.' };
					self.animation.frame_buffer[y][x] = (glow_ch, glow_color);
				} else {
					self.animation.frame_buffer[y][x] = (' ', bg_color);
				}
			}
		}

		let mut lines = Vec::new();
		for y in 0..h {
			let mut spans = Vec::new();
			for x in 0..w {
				let (ch, color) = self.animation.frame_buffer[y][x];
				spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}

	pub fn render_starfield_animation_in_area(&self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let elapsed_ms = (elapsed * 1000.0) as u64;
		let bg_color = self.theme_bg_color();

		let center_x = area.width as f64 / 2.0;
		let center_y = area.height as f64 / 2.0;

		let num_stars = 120;
		let mut star_positions: Vec<(u16, u16, f64, usize)> = Vec::new();

		for i in 0..num_stars {
			let angle = (i as f64 * 2.39996) % (2.0 * std::f64::consts::PI);
			let speed = 0.5 + (i % 5) as f64 * 0.4;
			let birth = (i * 300) % 5000;

			let age = (elapsed_ms.wrapping_sub(birth as u64) % 5000) as f64;
			let dist = age * speed / 100.0;

			let sx = center_x + angle.cos() * dist * 3.0;
			let sy = center_y + angle.sin() * dist;

			if sx >= 0.0 && sx < area.width as f64 && sy >= 0.0 && sy < area.height as f64 {
				let brightness = (dist / 15.0).min(1.0);
				star_positions.push((sx as u16, sy as u16, brightness, i));
			}
		}

		let mut lines = Vec::new();
		for y in 0..area.height {
			let mut spans = Vec::new();
			for x in 0..area.width {
				let mut found = false;
				for &(sx, sy, brightness, idx) in &star_positions {
					if sx == x && sy == y {
						let ch = if brightness > 0.7 {
							'★'
						} else if brightness > 0.4 {
							'*'
						} else {
							'·'
						};
						let color_idx = (idx * 3 + (elapsed * 2.0) as usize) % 50;
						let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
						let r = (c.r as f32 * brightness as f32) as u8;
						let g = (c.g as f32 * brightness as f32) as u8;
						let b = (c.b as f32 * brightness as f32) as u8;
						spans.push(Span::styled(
							ch.to_string(),
							Style::default().fg(ratatui::style::Color::Rgb(r, g, b)),
						));
						found = true;
						break;
					}
				}
				if !found {
					spans.push(Span::raw(" "));
				}
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}

	pub fn render_rain_animation_in_area(&mut self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let elapsed_ms = (elapsed * 1000.0) as u64;
		let bg_color = self.theme_bg_color();

		let w = area.width as usize;
		let h = area.height as usize;
		if w == 0 || h == 0 {
			return;
		}

		self.ensure_frame_buffer(area);

		// Reset buffer to empty
		for row in self.animation.frame_buffer.iter_mut().take(h) {
			for cell in row.iter_mut().take(w) {
				*cell = (' ', bg_color);
			}
		}

		for col in 0..area.width {
			for drop_id in 0..3u64 {
				let seed = col as u64 * 31 + drop_id * 997;
				let speed = 80 + (seed % 60);
				let drop_len = 2 + (seed % 3) as i32;
				let offset = (seed * 13) % (h as u64 * 3);

				let head_y =
					((elapsed_ms + offset * speed) / speed) as i32 % (h as i32 * 2) - (h as i32 / 2);

				for t in 0..drop_len {
					let y = head_y - t;
					if y >= 0 && y < h as i32 {
						let brightness = 1.0 - (t as f32 / drop_len as f32) * 0.6;

						// Rainbow animated colors - each drop gets a color that shifts over time
						let color_idx =
							(col as usize * 3 + drop_id as usize * 11 + (elapsed_ms / 100) as usize) % 50;
						let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
						let r = (c.r as f32 * brightness) as u8;
						let g = (c.g as f32 * brightness) as u8;
						let b = (c.b as f32 * brightness) as u8;
						let ch = if t == 0 { '|' } else { '│' };
						self.animation.frame_buffer[y as usize][col as usize] =
							(ch, ratatui::style::Color::Rgb(r, g, b));
					}
				}
			}

			// Splash at bottom with rainbow colors
			let splash_seed = col as u64 * 37;
			let splash_time = (elapsed_ms + splash_seed * 50) % 2000;
			if splash_time < 200 && h > 0 {
				let bottom = h - 1;
				let color_idx = (col as usize * 5 + (elapsed_ms / 80) as usize) % 50;
				let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
				self.animation.frame_buffer[bottom][col as usize] =
					('~', ratatui::style::Color::Rgb(c.r, c.g, c.b));
			}
		}

		let mut lines = Vec::new();
		for y in 0..h {
			let mut spans = Vec::new();
			for x in 0..w {
				let (ch, color) = self.animation.frame_buffer[y][x];
				spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}
}

impl ChatState {
	pub fn render_nyancat_animation_in_area(&self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let elapsed_ms = (elapsed * 1000.0) as u64;

		let cat_speed = 80;
		let total_width = area.width as i32 + 40;
		let x_pos = ((elapsed_ms / cat_speed) as i32 % total_width) - 20;

		// Nyan cat ASCII art (just the cat, no rectangle)
		let cat_art = [r#" /\_/\"#, r#"( o.o )"#, r#" > ^ < "#, r#"/|   |\"#, r#"(_|   |)"#];

		let cat_height = cat_art.len() as u16;
		let cat_width = cat_art.iter().map(|l| l.len()).max().unwrap_or(0) as i32;

		// Rainbow trail colors - animated
		let rainbow_band_count = 6;

		// Vertical bob
		let bob = ((elapsed_ms / 200) % 4) as i16;
		let bob_offset: i16 = match bob {
			0 => 0,
			1 => -1,
			2 => 0,
			3 => 1,
			_ => 0,
		};

		let y_center = (area.height as i16 / 2) - (cat_height as i16 / 2) + bob_offset;

		let mut lines = Vec::new();

		for y in 0..area.height {
			let mut spans = Vec::new();
			let row_y = y as i16;

			let cat_top = y_center;
			let cat_row = row_y - cat_top;

			// Which rainbow band is this row?
			let rainbow_row_offset = row_y - (cat_top - 1);

			for x in 0..area.width {
				let xi = x as i32;

				// Check if we're in the cat area
				if cat_row >= 0
					&& (cat_row as usize) < cat_art.len()
					&& xi >= x_pos
					&& xi < x_pos + cat_width
				{
					let line = cat_art[cat_row as usize];
					let char_offset = (xi - x_pos) as usize;
					let ch = line.chars().nth(char_offset).unwrap_or(' ');
					if ch != ' ' {
						// Animated rainbow colors for the cat
						let color_idx = (char_offset + cat_row as usize * 3 + (elapsed_ms / 100) as usize) % 50;
						let cat_color = self.rainbow_color(color_idx);
						spans.push(Span::styled(ch.to_string(), Style::default().fg(cat_color)));
					} else {
						// Transparent background - just space
						spans.push(Span::styled(" ", Style::default()));
					}
				} else if rainbow_row_offset >= 0
					&& (rainbow_row_offset as usize) < rainbow_band_count
					&& xi < x_pos
					&& xi >= x_pos.saturating_sub(30)
				{
					// Rainbow trail behind cat - animated
					let wave = ((xi + elapsed_ms as i32 / 50) % 2) == 0;
					let trail_ch = if wave { '=' } else { '-' };
					let trail_color_idx =
						(rainbow_row_offset as usize * 8 + (elapsed_ms / 100) as usize) % 50;
					let trail_color = self.rainbow_color(trail_color_idx);
					spans.push(Span::styled(trail_ch.to_string(), Style::default().fg(trail_color)));
				} else {
					// Transparent background with subtle twinkling stars
					let star_seed = (x as u64 * 31 + y as u64 * 17) % 200;
					if star_seed < 3 {
						let twinkle = (elapsed_ms / 300 + x as u64 + y as u64) % 2;
						let ch = if twinkle == 0 { '.' } else { '*' };
						let star_color_idx = (x as usize + y as usize) % 50;
						let c = self.animation.rainbow_animation.rgb_color_at(star_color_idx);
						spans.push(Span::styled(
							ch.to_string(),
							Style::default().fg(ratatui::style::Color::Rgb(c.r / 3, c.g / 3, c.b / 3)),
						));
					} else {
						spans.push(Span::styled(" ", Style::default()));
					}
				}
			}
			lines.push(Line::from(spans));
		}

		// Render with transparent background
		Paragraph::new(lines).render(area, buf);
	}

	pub fn render_dvdlogo_animation_in_area(&self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let elapsed_ms = (elapsed * 1000.0) as u64;
		let bg_color = self.theme_bg_color();

		let logo = [" DDDD  X   X", " D   D  X X ", " D   D   X  ", " D   D  X X ", " DDDD  X   X"];

		let logo_height = logo.len() as i32;
		let logo_width = 13i32;

		let max_x = (area.width as i32 - logo_width).max(1);
		let max_y = (area.height as i32 - logo_height).max(1);

		let speed_x: i32 = 3;
		let speed_y: i32 = 1;
		let tick = (elapsed_ms / 100) as i32;

		let raw_x = tick * speed_x;
		let raw_y = tick * speed_y;

		let cycle_x = max_x * 2;
		let cycle_y = max_y * 2;

		let pos_in_cycle_x = ((raw_x % cycle_x) + cycle_x) % cycle_x;
		let pos_in_cycle_y = ((raw_y % cycle_y) + cycle_y) % cycle_y;

		let x_pos = if pos_in_cycle_x < max_x { pos_in_cycle_x } else { cycle_x - pos_in_cycle_x };

		let y_pos = if pos_in_cycle_y < max_y { pos_in_cycle_y } else { cycle_y - pos_in_cycle_y };

		// Color changes on bounce - use rainbow colors
		let bounce_count_x = raw_x / max_x.max(1);
		let bounce_count_y = raw_y / max_y.max(1);
		let color_index = ((bounce_count_x + bounce_count_y).unsigned_abs() as usize * 7) % 50;
		let logo_color = self.rainbow_color(color_index);

		let mut lines = Vec::new();

		for row in 0..area.height as i32 {
			let mut spans = Vec::new();

			if row >= y_pos && row < y_pos + logo_height {
				let logo_line = logo[(row - y_pos) as usize];

				for col in 0..area.width as i32 {
					if col >= x_pos && col < x_pos + logo_width {
						let char_offset = (col - x_pos) as usize;
						let ch = logo_line.as_bytes().get(char_offset).map(|&b| b as char).unwrap_or(' ');
						if ch != ' ' {
							spans.push(Span::styled(ch.to_string(), Style::default().fg(logo_color)));
						} else {
							spans.push(Span::raw(" "));
						}
					} else {
						spans.push(Span::raw(" "));
					}
				}
			} else {
				for _ in 0..area.width {
					spans.push(Span::raw(" "));
				}
			}

			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}

	pub fn render_fire_animation_in_area(&self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let elapsed_ms = (elapsed * 1000.0) as u64;
		let bg_color = self.theme_bg_color();

		let fire_chars = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
		let w = area.width as usize;
		let h = area.height as usize;

		let mut lines = Vec::new();
		for y in 0..h {
			let mut spans = Vec::new();
			for x in 0..w {
				let heat = if y > h.saturating_sub(3) {
					7 + ((x * 7 + elapsed_ms as usize / 50) % 3)
				} else {
					let base_heat = 10 - (y * 10 / h.max(1));
					let noise = (elapsed_ms as usize / 100 + x + y) % 3;
					(base_heat + noise).min(9)
				};

				let ch = fire_chars[heat.min(fire_chars.len() - 1)];
				let color = match heat {
					0..=2 => ratatui::style::Color::Rgb(50, 0, 0),
					3..=4 => ratatui::style::Color::Rgb(150, 50, 0),
					5..=6 => ratatui::style::Color::Rgb(255, 100, 0),
					7..=8 => ratatui::style::Color::Rgb(255, 200, 0),
					_ => ratatui::style::Color::Rgb(255, 255, 100),
				};

				spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}

	pub fn render_plasma_animation_in_area(&self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let t = elapsed * 10.0;
		let bg_color = self.theme_bg_color();

		let w = area.width as usize;
		let h = area.height as usize;

		let mut lines = Vec::new();
		for y in 0..h {
			let mut spans = Vec::new();
			for x in 0..w {
				let fx = x as f32 / 10.0;
				let fy = y as f32 / 10.0;

				let value = (fx + t).sin() + (fy + t).sin() + ((fx + fy) / 2.0 + t).sin();

				let r = ((value + 1.0) * 127.5) as u8;
				let g = ((value.sin() + 1.0) * 127.5) as u8;
				let b = ((value.cos() + 1.0) * 127.5) as u8;

				let ch = if value > 1.0 {
					'█'
				} else if value > 0.0 {
					'▓'
				} else if value > -1.0 {
					'▒'
				} else {
					'░'
				};

				spans.push(Span::styled(
					ch.to_string(),
					Style::default().fg(ratatui::style::Color::Rgb(r, g, b)),
				));
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}

	pub fn render_spinners_animation_in_area(&self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let frame_num = (elapsed * 10.0) as usize;

		let spinners: Vec<Vec<char>> = vec![
			vec!['|', '/', '-', '\\'],
			// vec!['◐', '◓', '◑', '◒'],  // Dots - commented out due to width issues
			vec!['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
			vec!['▁', '▃', '▄', '▅', '▆', '▇', '█', '▇', '▆', '▅', '▄', '▃'],
			// vec!['◜', '◠', '◝', '◞', '◡', '◟'],  // Arc - commented out due to width issues
			vec!['⣾', '⣽', '⣻', '⢿', '⡿', '⣟', '⣯', '⣷'],
		];

		let spinner_names = [
			"Classic",
			// "Dots",  // Commented out
			"Braille",
			"Blocks",
			// "Arc",  // Commented out
			"Braille Dots",
		];

		let mut lines = Vec::new();

		for (i, (spinner_set, name)) in spinners.iter().zip(spinner_names.iter()).enumerate() {
			let char_idx = frame_num % spinner_set.len();

			let color_idx = (i * 7 + frame_num) % 50;
			let color = self.rainbow_color(color_idx);

			let spinner_char = spinner_set[char_idx];

			// Use consistent padding for all spinners to prevent width changes
			// All spinners get the same spacing regardless of character width
			let spinner_line = Line::from(vec![
				Span::raw("    "), // Fixed leading padding (4 spaces)
				Span::styled(
					spinner_char.to_string(),
					Style::default().fg(color).add_modifier(Modifier::BOLD),
				),
				Span::raw("    "), // Fixed trailing padding (4 spaces)
				Span::styled(*name, Style::default().fg(self.theme.fg)),
			]);

			lines.push(spinner_line);
			lines.push(Line::from("")); // Empty line between spinners
		}

		// Calculate vertical centering
		let content_height = lines.len() as u16;
		let vertical_offset =
			if area.height > content_height { (area.height - content_height) / 2 } else { 0 };

		// Create a centered area
		let centered_area = Rect {
			x: area.x,
			y: area.y + vertical_offset,
			width: area.width,
			height: content_height.min(area.height),
		};

		Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center).render(centered_area, buf);
	}

	pub fn render_waves_animation_in_area(&self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let bg_color = self.theme_bg_color();

		let w = area.width as usize;
		let h = area.height as usize;

		let mut lines = Vec::new();
		for y in 0..h {
			let mut spans = Vec::new();
			for x in 0..w {
				let fx = x as f32 / 8.0;
				let _fy = y as f32 / 4.0;
				let t = elapsed * 2.0;

				// Multiple sine waves for ocean effect
				let wave1 = (fx + t).sin();
				let wave2 = (fx * 0.5 - t * 0.7).sin();
				let wave3 = (fx * 1.5 + t * 0.5).sin();
				let combined = (wave1 + wave2 + wave3) / 3.0;

				let wave_height = (h as f32 * 0.5 + combined * h as f32 * 0.3) as usize;

				let ch = if y < wave_height {
					' '
				} else if y == wave_height {
					'~'
				} else if y == wave_height + 1 {
					'≈'
				} else {
					'·'
				};

				// Blue gradient for water
				let depth = (y.saturating_sub(wave_height)) as f32 / h as f32;
				let blue_intensity = (200.0 - depth * 150.0) as u8;
				let color = ratatui::style::Color::Rgb(0, blue_intensity / 3, blue_intensity);

				spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}

	pub fn render_fireworks_animation_in_area(&mut self, area: Rect, buf: &mut Buffer) {
		let elapsed = self.animation.rainbow_animation.elapsed();
		let elapsed_ms = (elapsed * 1000.0) as u64;
		let bg_color = self.theme_bg_color();

		let w = area.width as usize;
		let h = area.height as usize;

		if w == 0 || h == 0 {
			return;
		}

		self.ensure_frame_buffer(area);

		// Reset buffer to empty
		for row in self.animation.frame_buffer.iter_mut().take(h) {
			for cell in row.iter_mut().take(w) {
				*cell = (' ', bg_color);
			}
		}

		// Multiple firework bursts
		let num_fireworks = 4;
		let cycle_ms: u64 = 4000;

		for fw_id in 0..num_fireworks {
			let offset = fw_id * (cycle_ms / num_fireworks);
			let local_time = (elapsed_ms.wrapping_add(offset)) % cycle_ms;
			let age = local_time as f64 / 1000.0;

			// Launch position
			let center_x = match fw_id {
				0 => w / 4,
				1 => w / 2,
				2 => 3 * w / 4,
				_ => w / 3,
			};
			let launch_y = h.saturating_sub(1);

			// Rocket launch phase (first 0.5 seconds)
			if age < 0.5 {
				let rocket_offset = (age * h as f64 * 2.0) as usize;
				if rocket_offset <= launch_y {
					let rocket_y = launch_y - rocket_offset;
					if rocket_y < h {
						let color_idx = (fw_id as usize * 13 + (elapsed_ms / 50) as usize) % 50;
						let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
						let color = ratatui::style::Color::Rgb(c.r, c.g, c.b);
						self.animation.frame_buffer[rocket_y][center_x] = ('|', color);
						if rocket_y + 1 < h {
							self.animation.frame_buffer[rocket_y + 1][center_x] = ('·', color);
						}
					}
				}
			} else {
				// Explosion phase
				let explosion_age = age - 0.5;
				let explosion_offset = (0.5 * h as f64 * 2.0) as usize;
				if explosion_offset <= launch_y {
					let explosion_y = launch_y - explosion_offset;

					let num_particles = 60;
					for i in 0..num_particles {
						let angle = (i as f64 * 2.0 * std::f64::consts::PI) / num_particles as f64;
						let speed = 15.0 + (i % 5) as f64 * 2.0;
						let vx = angle.cos() * speed;
						let vy = angle.sin() * speed * 0.6 - 3.0;

						let px = center_x as f64 + vx * explosion_age;
						let py = explosion_y as f64 + vy * explosion_age + 5.0 * explosion_age * explosion_age;

						let px_i = px as i32;
						let py_i = py as i32;

						if px_i >= 0 && px_i < w as i32 && py_i >= 0 && py_i < h as i32 {
							let fade = (1.0 - explosion_age / 2.0).max(0.0);
							if fade > 0.1 {
								let color_idx = (fw_id as usize * 13 + i * 3) % 50;
								let c = self.animation.rainbow_animation.rgb_color_at(color_idx);
								let color = ratatui::style::Color::Rgb(
									(c.r as f64 * fade) as u8,
									(c.g as f64 * fade) as u8,
									(c.b as f64 * fade) as u8,
								);

								let chars = ['*', '·', '+', '×'];
								let ch = chars[i % chars.len()];
								self.animation.frame_buffer[py_i as usize][px_i as usize] = (ch, color);
							}
						}
					}
				}
			}
		}

		let mut lines = Vec::new();
		for y in 0..h {
			let mut spans = Vec::new();
			for x in 0..w {
				let (ch, color) = self.animation.frame_buffer[y][x];
				spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
			}
			lines.push(Line::from(spans));
		}

		Paragraph::new(lines).style(Style::default().bg(bg_color)).render(area, buf);
	}
}

impl ChatState {
	pub fn render_menu_in_area(&mut self, area: Rect, buf: &mut Buffer) {
		use tachyonfx::EffectRenderer;

		// Render menu if visible OR if closing (to show close animation)
		if self.show_tachyon_menu || self.menu_is_closing {
			// Only render the menu content if it's visible (not closing)
			if self.show_tachyon_menu {
				self.menu.render_in_area(area, buf, &self.theme_mode);
			}

			// Apply the active effect to the content area (for both open and close)
			let duration = self.menu.last_tick;
			if self.menu.active_effect.1.running() {
				// Calculate the centered content area (MUST match render_in_area)
				let content_width = (area.width * 7 / 10).min(80); // Match reduced size
				let content_height = (area.height * 75 / 100).min(32); // Match reduced size

				// Calculate centered position
				let x_offset = (area.width - content_width) / 2;
				let y_offset = (area.height - content_height) / 2;

				let content_area = Rect {
					x: area.x + x_offset,
					y: area.y + y_offset,
					width: content_width,
					height: content_height,
				};

				// Apply effect to the entire content area (including border)
				buf.render_effect(&mut self.menu.active_effect.1, content_area, duration);
			} else if self.menu_is_closing {
				// Animation finished, stop showing closing animation
				self.menu_is_closing = false;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::train_exit_duration_for_width;
	use crate::state::ChatState;
	use ratatui::{buffer::Buffer, layout::Rect};

	#[test]
	fn rain_animation_ignores_zero_height_area() {
		let mut state = ChatState::new();
		let mut buffer = Buffer::empty(Rect { x: 0, y: 0, width: 1, height: 1 });

		state.render_rain_animation_in_area(Rect { x: 0, y: 0, width: 1, height: 0 }, &mut buffer);
	}

	#[test]
	fn train_animation_handles_tiny_areas() {
		let mut state = ChatState::new();
		let mut buffer = Buffer::empty(Rect { x: 0, y: 0, width: 1, height: 1 });

		state.render_train_animation_in_area(Rect { x: 0, y: 0, width: 0, height: 1 }, &mut buffer);
		state.render_train_animation_in_area(Rect { x: 0, y: 0, width: 1, height: 0 }, &mut buffer);
		state.render_train_animation_in_area(Rect { x: 0, y: 0, width: 1, height: 1 }, &mut buffer);
	}

	#[test]
	fn train_exit_duration_scales_with_terminal_width() {
		assert!(train_exit_duration_for_width(120) > train_exit_duration_for_width(80));
		assert!(train_exit_duration_for_width(80) > std::time::Duration::from_secs(4));
		assert!(train_exit_duration_for_width(80) < std::time::Duration::from_secs(7));
	}

	#[test]
	fn train_exit_duration_is_bounded_for_tiny_and_wide_widths() {
		let tiny = train_exit_duration_for_width(1);
		let wide = train_exit_duration_for_width(240);

		assert!(tiny >= std::time::Duration::from_secs(2));
		assert!(tiny <= std::time::Duration::from_secs(4));
		assert!(wide <= std::time::Duration::from_secs(12));
	}
}
