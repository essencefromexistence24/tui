use ratatui::style::Color;
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeVariant {
	Dark,
	Light,
}

impl ThemeVariant {
	pub fn toggle(self) -> Self {
		match self {
			Self::Dark => Self::Light,
			Self::Light => Self::Dark,
		}
	}
}

#[derive(Debug, Deserialize, Clone)]
pub struct RgbColor {
	pub r: u8,
	pub g: u8,
	pub b: u8,
}

impl From<RgbColor> for Color {
	fn from(rgb: RgbColor) -> Self {
		Color::Rgb(rgb.r, rgb.g, rgb.b)
	}
}

#[derive(Debug, Deserialize, Clone)]
struct ThemeColors {
	background: RgbColor,
	foreground: RgbColor,
	#[serde(default)]
	card: Option<RgbColor>,
	#[serde(default)]
	primary: Option<RgbColor>,
	#[serde(default)]
	secondary: Option<RgbColor>,
	#[serde(default)]
	muted: Option<RgbColor>,
	#[serde(default)]
	muted_foreground: Option<RgbColor>,
	#[serde(default)]
	accent: Option<RgbColor>,
	#[serde(default)]
	destructive: Option<RgbColor>,
	border: RgbColor,
	// Accept extra JSON keys without failing
	#[serde(flatten)]
	_extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ThemeDefinition {
	pub name: String,
	pub title: String,
	dark: ThemeColors,
	light: ThemeColors,
}

#[derive(Debug, Deserialize)]
pub struct ThemeRegistry {
	pub themes: Vec<ThemeDefinition>,
}

static THEME_REGISTRY: OnceLock<ThemeRegistry> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct ChatTheme {
	pub variant: ThemeVariant,
	pub bg: Color,
	pub fg: Color,
	pub accent: Color,
	pub border: Color,
	/// Theme `muted` / card surface (suggestion list bg).
	pub muted: Color,
	/// Theme `muted_foreground` — secondary labels (Remote, model name, tips).
	pub muted_fg: Color,
	/// Theme `card` surface.
	pub card: Color,
	/// Theme `destructive` (errors / deletions when preferred).
	pub destructive: Color,
	/// Theme `primary` (often same as accent).
	pub primary: Color,
}

impl ChatTheme {
	pub fn load_themes() -> &'static ThemeRegistry {
		THEME_REGISTRY.get_or_init(|| {
			let themes_json = include_str!("../themes.json");
			serde_json::from_str(themes_json).unwrap_or_else(|error| {
				tracing::warn!(?error, "Failed to parse embedded theme registry; using fallback theme");
				ThemeRegistry { themes: Vec::new() }
			})
		})
	}

	pub fn available_themes() -> Vec<(String, String)> {
		Self::load_themes().themes.iter().map(|t| (t.name.clone(), t.title.clone())).collect()
	}

	pub fn from_definition(def: &ThemeDefinition, variant: ThemeVariant) -> Self {
		let colors = match variant {
			ThemeVariant::Dark => &def.dark,
			ThemeVariant::Light => &def.light,
		};
		let fg: Color = colors.foreground.clone().into();
		let border: Color = colors.border.clone().into();
		let accent: Color = colors
			.accent
			.clone()
			.or_else(|| colors.primary.clone())
			.map(Into::into)
			.unwrap_or(Color::Rgb(0, 255, 42));
		let primary: Color = colors.primary.clone().map(Into::into).unwrap_or(accent);
		let muted: Color = colors
			.muted
			.clone()
			.or_else(|| colors.card.clone())
			.or_else(|| colors.secondary.clone())
			.map(Into::into)
			.unwrap_or(border);
		let muted_fg: Color =
			colors.muted_foreground.clone().map(Into::into).unwrap_or(Color::Rgb(155, 155, 155));
		let card: Color = colors.card.clone().map(Into::into).unwrap_or(muted);
		let destructive: Color =
			colors.destructive.clone().map(Into::into).unwrap_or(Color::Rgb(203, 154, 151));

		Self {
			variant,
			bg: colors.background.clone().into(),
			fg,
			accent,
			border,
			muted,
			muted_fg,
			card,
			destructive,
			primary,
		}
	}

	pub fn by_name(name: &str, variant: ThemeVariant) -> Option<Self> {
		Self::load_themes()
			.themes
			.iter()
			.find(|t| t.name == name)
			.map(|def| Self::from_definition(def, variant))
	}

	pub fn dark_fallback() -> Self {
		Self {
			variant: ThemeVariant::Dark,
			bg: Color::Rgb(0, 0, 0),
			fg: Color::Rgb(255, 255, 255),
			accent: Color::Rgb(0, 255, 42),
			border: Color::Rgb(36, 36, 36),
			muted: Color::Rgb(29, 29, 29),
			muted_fg: Color::Rgb(155, 155, 155),
			card: Color::Rgb(14, 14, 14),
			destructive: Color::Rgb(203, 154, 151),
			primary: Color::Rgb(0, 255, 42),
		}
	}

	/// Semantic green for additions (tinted from accent when possible).
	pub fn success(&self) -> Color {
		match self.accent {
			Color::Rgb(r, g, b) => {
				Color::Rgb(r.saturating_mul(1).min(80), g.max(180), b.saturating_mul(1).min(100))
			}
			_ => Color::Rgb(0x22, 0xc5, 0x5e),
		}
	}

	/// Pure red for diff deletions (`-N`).
	pub fn danger(&self) -> Color {
		Color::Rgb(0xff, 0x00, 0x00)
	}

	/// Warning / amber from accent shift.
	pub fn warning(&self) -> Color {
		match self.accent {
			Color::Rgb(r, g, b) => Color::Rgb(r.max(200), g.clamp(160, 220), b.saturating_div(3)),
			_ => Color::Rgb(0xf5, 0xa6, 0x23),
		}
	}
}
