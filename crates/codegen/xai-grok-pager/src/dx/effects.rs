//! Visual effects ported directly from `crates/dx-tui/src/effects.rs`.

use ratatui::style::Color;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<Color> for RgbColor {
    fn from(color: Color) -> Self {
        match color {
            Color::Rgb(r, g, b) => Self { r, g, b },
            _ => Self {
                r: 255,
                g: 255,
                b: 255,
            },
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
        Self {
            colors,
            start_time: Instant::now(),
            duration: Duration::from_millis(1500),
        }
    }

    pub fn shimmer_color_at(&self, position: f32) -> Color {
        let elapsed = self.start_time.elapsed().as_millis() as f32;
        let cycle = (elapsed % self.duration.as_millis() as f32) / self.duration.as_millis() as f32;
        let wave_position = -1.0 + (cycle * 3.0);
        let distance = (position - wave_position).abs();

        if distance < 0.3 {
            let t = 1.0 - (distance / 0.3);
            self.interpolate_color(self.colors[0], Color::Rgb(255, 255, 255), t * 0.7)
        } else {
            self.colors[0]
        }
    }

    pub fn current_color(&self) -> Color {
        self.shimmer_color_at(0.5)
    }

    fn interpolate_color(&self, c1: Color, c2: Color, t: f32) -> Color {
        match (c1, c2) {
            (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => Color::Rgb(
                (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8,
                (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8,
                (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8,
            ),
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
        Self {
            dots: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(500),
        }
    }

    pub fn update(&mut self) {
        if self.last_update.elapsed() >= self.interval {
            self.dots = (self.dots + 1) % 4;
            self.last_update = Instant::now();
        }
    }

    pub fn text(&self, is_visible: bool) -> String {
        if !is_visible {
            return String::new();
        }
        match self.dots {
            0 => "",
            1 => ".",
            2 => "..",
            _ => "...",
        }
        .to_string()
    }

    pub fn is_visible(&self) -> bool {
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
            speed: 0.5,
        }
    }

    pub fn elapsed(&self) -> f32 {
        self.start_time.elapsed().as_secs_f32()
    }

    pub fn current_color(&self) -> Color {
        let hue = (self.elapsed() * self.speed * 360.0) % 360.0;
        Self::hsl_to_rgb(hue, 0.8, 0.6)
    }

    pub fn color_at(&self, index: usize) -> Color {
        let hue = ((self.elapsed() * self.speed * 360.0) + (index as f32 * 10.0)) % 360.0;
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
        Color::Rgb(
            ((r + m) * 255.0) as u8,
            ((g + m) * 255.0) as u8,
            ((b + m) * 255.0) as u8,
        )
    }
}

impl Default for RainbowEffect {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rainbow_returns_rgb_colors() {
        assert!(matches!(
            RainbowEffect::new().color_at(0),
            Color::Rgb(_, _, _)
        ));
    }

    #[test]
    fn hidden_typing_indicator_is_empty() {
        assert_eq!(TypingIndicator::new().text(false), "");
    }
}
