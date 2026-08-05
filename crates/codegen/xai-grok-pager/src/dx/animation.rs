//! Presentation-only adapter for DX's train animation.
//!
//! The artwork, timing, motion, smoke frames, rainbow seeding, and track
//! rendering are transplanted from `crates/dx-tui/src/animations.rs`. This
//! adapter replaces only the original `ChatState` dependency so Grok remains
//! the sole application-state owner.

use std::time::Instant;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

use super::effects::RainbowEffect;

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
const TRAIN_LOOP_GUTTER_COLUMNS: u64 = 12;
const TRAIN_START_RIGHT_GUTTER_COLUMNS: u64 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationKind {
    Splash,
    Train,
    Matrix,
    GameOfLife,
    Starfield,
    Rain,
    Fire,
    Plasma,
    Waves,
    Fireworks,
}

impl AnimationKind {
    pub const ALL: [Self; 10] = [
        Self::Splash,
        Self::Train,
        Self::Matrix,
        Self::GameOfLife,
        Self::Starfield,
        Self::Rain,
        Self::Fire,
        Self::Plasma,
        Self::Waves,
        Self::Fireworks,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Splash => "Splash",
            Self::Matrix => "Matrix",
            Self::Train => "Train",
            Self::GameOfLife => "Game of Life",
            Self::Starfield => "Starfield",
            Self::Rain => "Rain",
            Self::Fire => "Fire",
            Self::Plasma => "Plasma",
            Self::Waves => "Waves",
            Self::Fireworks => "Fireworks",
        }
    }

    pub fn sound(self) -> super::sound::AnimationSound {
        match self {
            Self::Splash | Self::Matrix => super::sound::AnimationSound::Matrix,
            Self::Train => super::sound::AnimationSound::Train,
            Self::GameOfLife => super::sound::AnimationSound::GameOfLife,
            Self::Starfield => super::sound::AnimationSound::Starfield,
            Self::Rain => super::sound::AnimationSound::Rain,
            Self::Fire => super::sound::AnimationSound::Fire,
            Self::Plasma => super::sound::AnimationSound::Plasma,
            Self::Waves => super::sound::AnimationSound::Waves,
            Self::Fireworks => super::sound::AnimationSound::Fireworks,
        }
    }
}

fn train_width_columns() -> u64 {
    TRAIN
        .iter()
        .map(|line| line.chars().count() as u64)
        .max()
        .unwrap_or(0)
}

fn train_loop_progress(width: u16, elapsed_ms: usize) -> i32 {
    let travel = u64::from(width) + train_width_columns() + TRAIN_LOOP_GUTTER_COLUMNS;
    ((elapsed_ms as u64 / TRAIN_COLUMN_MS) % travel.max(1)) as i32
}

fn scale_color(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        other => other,
    }
}

pub struct AnimationSurface {
    started_at: Instant,
    rainbow: RainbowEffect,
    current: usize,
    pub intro: AnimationKind,
    pub outro: AnimationKind,
    exiting: bool,
    last_width: u16,
    splash_font_index: usize,
    last_font_change: Instant,
}

impl Default for AnimationSurface {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            rainbow: RainbowEffect::new(),
            current: 0,
            intro: AnimationKind::Matrix,
            outro: AnimationKind::Train,
            exiting: false,
            last_width: 120,
            splash_font_index: 0,
            last_font_change: Instant::now(),
        }
    }
}

impl AnimationSurface {
    pub fn restart(&mut self) {
        self.started_at = Instant::now();
        self.rainbow = RainbowEffect::new();
        self.exiting = false;
    }

    pub fn current(&self) -> AnimationKind {
        AnimationKind::ALL[self.current]
    }

    pub fn next(&mut self) {
        self.current = (self.current + 1) % AnimationKind::ALL.len();
        self.restart();
    }

    pub fn previous(&mut self) {
        self.current = self
            .current
            .checked_sub(1)
            .unwrap_or(AnimationKind::ALL.len() - 1);
        self.restart();
    }

    pub fn select_intro(&mut self) {
        self.intro = self.current();
    }

    /// Jump to the configured intro animation and restart its timeline for a
    /// welcome→chat intro playback.
    pub fn begin_intro(&mut self) {
        self.current = AnimationKind::ALL
            .iter()
            .position(|kind| *kind == self.intro)
            .unwrap_or(0);
        self.restart();
    }

    /// Start the default welcome surface at the Splash carousel item.
    ///
    /// This is deliberately separate from `begin_intro`: the configurable
    /// intro animation is useful for explicit intro playback, while a new
    /// workspace should always enter through the named Splash screen.
    pub fn begin_splash(&mut self) {
        self.current = AnimationKind::ALL
            .iter()
            .position(|kind| *kind == AnimationKind::Splash)
            .unwrap_or(0);
        self.restart();
    }

    pub fn select_outro(&mut self) {
        self.outro = self.current();
    }

    pub fn begin_outro(&mut self) -> std::time::Duration {
        self.current = AnimationKind::ALL
            .iter()
            .position(|kind| *kind == self.outro)
            .unwrap_or_else(|| {
                AnimationKind::ALL
                    .iter()
                    .position(|kind| *kind == AnimationKind::Train)
                    .unwrap_or(0)
            });
        self.started_at = Instant::now();
        self.exiting = true;
        match self.outro {
            AnimationKind::Train => {
                let columns = u64::from(self.last_width)
                    + train_width_columns()
                    + TRAIN_START_RIGHT_GUTTER_COLUMNS;
                std::time::Duration::from_millis(columns * TRAIN_COLUMN_MS + 100)
            }
            _ => std::time::Duration::from_millis(1800),
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer, theme: &crate::theme::Theme) {
        let background = theme.bg_base;
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.last_width = area.width;

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].reset();
                buf[(x, y)].set_bg(background);
            }
        }

        let elapsed_ms = self.started_at.elapsed().as_millis() as usize;
        match self.current() {
            AnimationKind::Splash => {
                self.render_matrix(area, buf, background, elapsed_ms);
                if self.last_font_change.elapsed() >= std::time::Duration::from_secs(3) {
                    self.splash_font_index =
                        (self.splash_font_index + 1) % super::splash::splash_font_count();
                    self.last_font_change = Instant::now();
                }
                super::splash::render(
                    area,
                    buf,
                    &crate::theme::ChatTheme::from(theme),
                    self.splash_font_index,
                    &self.rainbow,
                );
                return;
            }
            AnimationKind::Matrix => {
                self.render_matrix(area, buf, background, elapsed_ms);
                return;
            }
            AnimationKind::GameOfLife => {
                self.render_game_of_life(area, buf, background, elapsed_ms);
                return;
            }
            AnimationKind::Starfield => {
                self.render_starfield(area, buf, background, elapsed_ms);
                return;
            }
            AnimationKind::Rain => {
                self.render_rain(area, buf, background, elapsed_ms);
                return;
            }
            AnimationKind::Fire => {
                self.render_fire(area, buf, background, elapsed_ms);
                return;
            }
            AnimationKind::Plasma => {
                self.render_plasma(area, buf, background, elapsed_ms);
                return;
            }
            AnimationKind::Waves => {
                self.render_waves(area, buf, background, elapsed_ms);
                return;
            }
            AnimationKind::Fireworks => {
                self.render_fireworks(area, buf, background, elapsed_ms);
                return;
            }
            AnimationKind::Train => {}
        }
        let progress = if self.exiting {
            (elapsed_ms as u64 / TRAIN_COLUMN_MS) as i32
        } else {
            train_loop_progress(area.width, elapsed_ms)
        };
        let train_x = area.width as i32 + 6 - progress;
        // Skip the last undercarriage row so the track sits right under the
        // wheel row — no gap between tyres and railway.
        let visible_rows = TRAIN.len().saturating_sub(1);
        let content_height = (SMOKE[0].len() + visible_rows + 1) as i32;
        let top = ((area.height as i32 - content_height) / 2).max(0);
        let smoke = SMOKE[(elapsed_ms / 240) % SMOKE.len()];
        let train_top = top + smoke.len() as i32;

        for (line_idx, line) in smoke.iter().enumerate() {
            self.render_train_line(
                area,
                buf,
                top + line_idx as i32,
                train_x + 6,
                line,
                elapsed_ms / 200,
                background,
            );
        }

        for (line_idx, line) in TRAIN.iter().take(visible_rows).enumerate() {
            self.render_train_line(
                area,
                buf,
                train_top + line_idx as i32,
                train_x,
                line,
                line_idx * 3 + elapsed_ms / 150,
                background,
            );
        }

        // Track drawn right after the last rendered train row (the wheel
        // row), so the wheels sit directly on the railway.
        let track_y = train_top + visible_rows as i32;
        if track_y >= 0 && track_y < area.height as i32 {
            for x in 0..area.width {
                let ch = if (x as usize + elapsed_ms / 300).is_multiple_of(4) {
                    '+'
                } else {
                    '='
                };
                let color = self.rainbow.color_at((x as usize + elapsed_ms / 300) % 50);
                let cell = &mut buf[(area.x + x, area.y + track_y as u16)];
                cell.set_char(ch);
                cell.set_style(Style::default().fg(color).bg(background));
            }
        }
        // Controls removed: the carousel fills the full left panel and
        // navigation is handled via the keyboard shortcut layer (←/→).
    }

    fn render_matrix(&self, area: Rect, buf: &mut Buffer, background: Color, elapsed_ms: usize) {
        let chars = [
            'ﾊ', 'ﾐ', 'ﾋ', 'ｰ', 'ｳ', 'ｼ', 'ﾅ', 'ﾓ', 'ﾆ', 'ｻ', 'ﾜ', 'ﾂ', 'ｵ', 'ﾘ', 'ｱ', 'ﾎ', 'ﾃ',
            'ﾏ', 'ｹ', 'ﾒ', 'ｴ', 'ｶ', 'ｷ', 'ﾑ', 'ﾕ', 'ﾗ', 'ｾ', 'ﾈ', 'ｽ', 'ﾀ', 'ﾇ', 'ﾍ', '0', '1',
            '2', '3', '4', '5', '6', '7', '8', '9', ':', '.', '"', '=', '*', '+', '-', '<', '>',
            '¦', '|', 'Z',
        ];
        for x in 0..area.width {
            if (x * 7) % 3 != 0 {
                continue;
            }
            let speed = 1 + ((x * 11) % 2) as usize;
            let length = 8 + ((x * 13) % 12);
            let head = (((elapsed_ms / (150 / speed)) + ((x * 17) % 40) as usize)
                % (area.height as usize + 30)) as i32
                - 10;
            for trail in 0..length {
                let y = head - trail as i32;
                if y < 0 || y >= area.height as i32 {
                    continue;
                }
                let ch =
                    chars[(x as usize * 31 + y as usize * 17 + elapsed_ms / 200) % chars.len()];
                let color = if trail == 0 {
                    Color::Rgb(200, 255, 200)
                } else {
                    let fade = 1.0 - (trail as f32 / length as f32) * 0.85;
                    Color::Rgb(0, (255.0 * fade) as u8, 0)
                };
                let cell = &mut buf[(area.x + x, area.y + y as u16)];
                cell.set_char(ch);
                cell.set_style(Style::default().fg(color).bg(background));
            }
        }
    }

    fn render_game_of_life(
        &self,
        area: Rect,
        buf: &mut Buffer,
        background: Color,
        elapsed_ms: usize,
    ) {
        let (w, h) = (area.width as usize, area.height as usize);
        if w == 0 || h == 0 {
            return;
        }
        let generation = elapsed_ms / 150;
        let seed_gen = generation / 200;
        let mut grid = vec![vec![false; w]; h];
        let seed_hash = seed_gen.wrapping_mul(2_654_435_761);
        for (y, row) in grid.iter_mut().enumerate() {
            for (x, cell) in row.iter_mut().enumerate() {
                let hash = (x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed_hash)
                    .wrapping_mul(2_246_822_519);
                *cell = hash % 100 < 25;
            }
        }
        for (dx, dy) in [(0i32, -1i32), (1, -1), (-1, 0), (0, 0), (0, 1)] {
            let x = (w as i32 / 2 + dx).rem_euclid(w as i32) as usize;
            let y = (h as i32 / 2 + dy).rem_euclid(h as i32) as usize;
            grid[y][x] = true;
        }
        for _ in 0..(generation % 200).min(60) {
            let mut next = vec![vec![false; w]; h];
            for y in 0..h {
                for x in 0..w {
                    let mut neighbors = 0u8;
                    for dy in [h - 1, 0, 1] {
                        for dx in [w - 1, 0, 1] {
                            if dy == 0 && dx == 0 {
                                continue;
                            }
                            neighbors += u8::from(grid[(y + dy) % h][(x + dx) % w]);
                        }
                    }
                    next[y][x] = if grid[y][x] {
                        matches!(neighbors, 2 | 3)
                    } else {
                        neighbors == 3
                    };
                }
            }
            grid = next;
        }
        let pulse = ((elapsed_ms as f32 / 1000.0) * 3.0).sin() * 0.3 + 0.7;
        for (y, row) in grid.iter().enumerate() {
            for (x, alive) in row.iter().copied().enumerate() {
                let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
                if alive {
                    let base = self
                        .rainbow
                        .color_at((x * 3 + y * 7 + elapsed_ms / 200) % 50);
                    let color = scale_color(base, pulse);
                    cell.set_char('●');
                    cell.set_style(Style::default().fg(color).bg(background));
                }
            }
        }
    }

    fn render_starfield(&self, area: Rect, buf: &mut Buffer, background: Color, elapsed_ms: usize) {
        let center_x = area.width as f64 / 2.0;
        let center_y = area.height as f64 / 2.0;
        for i in 0..120usize {
            let angle = (i as f64 * 2.39996) % (2.0 * std::f64::consts::PI);
            let speed = 0.5 + (i % 5) as f64 * 0.4;
            let birth = (i * 300) % 5000;
            let age = elapsed_ms.wrapping_sub(birth) % 5000;
            let distance = age as f64 * speed / 100.0;
            let x = center_x + angle.cos() * distance * 3.0;
            let y = center_y + angle.sin() * distance;
            if x < 0.0 || x >= area.width as f64 || y < 0.0 || y >= area.height as f64 {
                continue;
            }
            let brightness = (distance / 15.0).min(1.0) as f32;
            let ch = if brightness > 0.7 {
                '★'
            } else if brightness > 0.4 {
                '*'
            } else {
                '·'
            };
            let color = scale_color(
                self.rainbow.color_at((i * 3 + elapsed_ms / 500) % 50),
                brightness,
            );
            let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
            cell.set_char(ch);
            cell.set_style(Style::default().fg(color).bg(background));
        }
    }

    fn render_rain(&self, area: Rect, buf: &mut Buffer, background: Color, elapsed_ms: usize) {
        let height = area.height as i32;
        for column in 0..area.width {
            for drop_id in 0..3u64 {
                let seed = column as u64 * 31 + drop_id * 997;
                let speed = 80 + seed % 60;
                let length = 2 + (seed % 3) as i32;
                let offset = (seed * 13) % (area.height.max(1) as u64 * 3);
                let head = ((elapsed_ms as u64 + offset * speed) / speed) as i32
                    % (height.max(1) * 2)
                    - height / 2;
                for trail in 0..length {
                    let y = head - trail;
                    if y < 0 || y >= height {
                        continue;
                    }
                    let brightness = 1.0 - trail as f32 / length as f32 * 0.6;
                    let color = scale_color(
                        self.rainbow.color_at(
                            (column as usize * 3 + drop_id as usize * 11 + elapsed_ms / 100) % 50,
                        ),
                        brightness,
                    );
                    let cell = &mut buf[(area.x + column, area.y + y as u16)];
                    cell.set_char(if trail == 0 { '|' } else { '│' });
                    cell.set_style(Style::default().fg(color).bg(background));
                }
            }
            if (elapsed_ms + column as usize * 1850) % 2000 < 200 && area.height > 0 {
                let cell = &mut buf[(area.x + column, area.bottom() - 1)];
                cell.set_char('~');
                cell.set_style(
                    Style::default()
                        .fg(self
                            .rainbow
                            .color_at((column as usize * 5 + elapsed_ms / 80) % 50))
                        .bg(background),
                );
            }
        }
    }

    fn render_fire(&self, area: Rect, buf: &mut Buffer, background: Color, elapsed_ms: usize) {
        let chars = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
        let h = area.height as usize;
        for y in 0..h {
            for x in 0..area.width as usize {
                let heat = if y > h.saturating_sub(3) {
                    7 + ((x * 7 + elapsed_ms / 50) % 3)
                } else {
                    (10usize.saturating_sub(y * 10 / h.max(1)) + (elapsed_ms / 100 + x + y) % 3)
                        .min(9)
                };
                let color = match heat {
                    0..=2 => Color::Rgb(50, 0, 0),
                    3..=4 => Color::Rgb(150, 50, 0),
                    5..=6 => Color::Rgb(255, 100, 0),
                    7..=8 => Color::Rgb(255, 200, 0),
                    _ => Color::Rgb(255, 255, 100),
                };
                let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
                cell.set_char(chars[heat]);
                cell.set_style(Style::default().fg(color).bg(background));
            }
        }
    }

    fn render_plasma(&self, area: Rect, buf: &mut Buffer, background: Color, elapsed_ms: usize) {
        let t = elapsed_ms as f32 / 100.0;
        for y in 0..area.height {
            for x in 0..area.width {
                let fx = x as f32 / 10.0;
                let fy = y as f32 / 10.0;
                let value = (fx + t).sin() + (fy + t).sin() + ((fx + fy) / 2.0 + t).sin();
                let color = Color::Rgb(
                    ((value + 1.0) * 127.5) as u8,
                    ((value.sin() + 1.0) * 127.5) as u8,
                    ((value.cos() + 1.0) * 127.5) as u8,
                );
                let cell = &mut buf[(area.x + x, area.y + y)];
                cell.set_char(if value > 1.0 {
                    '█'
                } else if value > 0.0 {
                    '▓'
                } else if value > -1.0 {
                    '▒'
                } else {
                    '░'
                });
                cell.set_style(Style::default().fg(color).bg(background));
            }
        }
    }

    fn render_waves(&self, area: Rect, buf: &mut Buffer, background: Color, elapsed_ms: usize) {
        let h = area.height as usize;
        let t = elapsed_ms as f32 / 500.0;
        for y in 0..h {
            for x in 0..area.width as usize {
                let fx = x as f32 / 8.0;
                let combined =
                    ((fx + t).sin() + (fx * 0.5 - t * 0.7).sin() + (fx * 1.5 + t * 0.5).sin())
                        / 3.0;
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
                let depth = y.saturating_sub(wave_height) as f32 / h.max(1) as f32;
                let blue = (200.0 - depth * 150.0) as u8;
                let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
                cell.set_char(ch);
                cell.set_style(
                    Style::default()
                        .fg(Color::Rgb(0, blue / 3, blue))
                        .bg(background),
                );
            }
        }
    }

    fn render_fireworks(&self, area: Rect, buf: &mut Buffer, background: Color, elapsed_ms: usize) {
        let (w, h) = (area.width as usize, area.height as usize);
        if w == 0 || h == 0 {
            return;
        }
        for id in 0..4usize {
            let local = (elapsed_ms + id * 1000) % 4000;
            let age = local as f64 / 1000.0;
            let center_x = [w / 4, w / 2, 3 * w / 4, w / 3][id];
            if age < 0.5 {
                let offset = (age * h as f64 * 2.0) as usize;
                let y = h.saturating_sub(1).saturating_sub(offset);
                let cell = &mut buf[(area.x + center_x as u16, area.y + y as u16)];
                cell.set_char('|');
                cell.set_style(
                    Style::default()
                        .fg(self.rainbow.color_at((id * 13 + elapsed_ms / 50) % 50))
                        .bg(background),
                );
                continue;
            }
            let burst_age = age - 0.5;
            let center_y = h / 3 + id % 3;
            for particle in 0..64usize {
                let angle = particle as f64 * 2.39996322 + id as f64;
                let speed = 4.0 + (particle * 7919 % 100) as f64 / 10.0;
                let x = center_x as f64 + angle.cos() * speed * burst_age * 1.8;
                let y = center_y as f64
                    + angle.sin() * speed * burst_age * 0.7
                    + 2.5 * burst_age * burst_age;
                if x < 0.0 || x >= w as f64 || y < 0.0 || y >= h as f64 {
                    continue;
                }
                let fade = (1.0 - burst_age / 3.5).max(0.0) as f32;
                let color = scale_color(
                    self.rainbow
                        .color_at((particle * 7 + id * 13 + elapsed_ms / 60) % 50),
                    fade,
                );
                let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
                cell.set_char(['✦', '·', '*', '+'][particle % 4]);
                cell.set_style(Style::default().fg(color).bg(background));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_train_line(
        &self,
        area: Rect,
        buf: &mut Buffer,
        y: i32,
        start_x: i32,
        line: &str,
        color_seed: usize,
        background: Color,
    ) {
        if y < 0 || y >= area.height as i32 {
            return;
        }
        for (char_index, ch) in line.chars().enumerate() {
            let x = start_x + char_index as i32;
            if x < 0 || x >= area.width as i32 {
                continue;
            }
            let color = self.rainbow.color_at((color_seed + char_index) % 50);
            let cell = &mut buf[(area.x + x as u16, area.y + y as u16)];
            cell.set_char(ch);
            cell.set_style(Style::default().fg(color).bg(background));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn train_renderer_handles_empty_area() {
        let mut surface = AnimationSurface::default();
        let mut buffer = Buffer::empty(Rect::new(0, 0, 0, 0));
        surface.render(
            Rect::new(0, 0, 0, 0),
            &mut buffer,
            &crate::theme::Theme::current(),
        );
    }

    #[test]
    fn train_renderer_paints_supplied_background() {
        let mut surface = AnimationSurface::default();
        let area = Rect::new(0, 0, 80, 24);
        let mut buffer = Buffer::empty(area);
        let theme = crate::theme::Theme::current();
        surface.render(area, &mut buffer, &theme);

        assert_eq!(buffer[(0, 0)].bg, theme.bg_base);
        assert!(
            buffer
                .content
                .iter()
                .any(|cell| cell.symbol() != " " && cell.fg != Color::Reset)
        );
    }

    #[test]
    fn every_dx_carousel_screen_renders() {
        let area = Rect::new(0, 0, 96, 30);
        for (index, kind) in AnimationKind::ALL.into_iter().enumerate() {
            let mut surface = AnimationSurface::default();
            surface.current = index;
            let mut buffer = Buffer::empty(area);
            surface.render(area, &mut buffer, &crate::theme::Theme::current());
            assert_eq!(surface.current(), kind);
            assert!(
                buffer.content.iter().any(|cell| cell.symbol() != " "),
                "{} must paint visible content",
                kind.name()
            );
        }
    }

    #[test]
    fn native_dx_splash_has_the_embedded_font_catalog() {
        assert!(
            crate::dx::splash::splash_font_count() >= 100,
            "DX FIGlet font bundle must be embedded in the pager"
        );
    }

    #[test]
    fn carousel_navigation_and_intro_outro_selection_are_stable() {
        let mut surface = AnimationSurface::default();
        assert_eq!(surface.current(), AnimationKind::Splash);
        surface.next();
        assert_eq!(surface.current(), AnimationKind::Train);
        surface.next();
        assert_eq!(surface.current(), AnimationKind::Matrix);
        surface.select_intro();
        assert_eq!(surface.intro, AnimationKind::Matrix);
        surface.next();
        assert_eq!(surface.current(), AnimationKind::GameOfLife);
        surface.previous();
        surface.previous();
        assert_eq!(surface.current(), AnimationKind::Train);
        surface.select_outro();
        assert_eq!(surface.outro, AnimationKind::Train);
        surface.previous();
        assert_eq!(surface.current(), AnimationKind::Splash);
    }

    #[test]
    fn train_outro_duration_covers_the_whole_terminal() {
        let mut surface = AnimationSurface::default();
        surface.last_width = 190;
        surface.outro = AnimationKind::Train;
        let duration = surface.begin_outro();
        assert!(duration > std::time::Duration::from_secs(8));
        assert!(surface.exiting);
        assert_eq!(surface.current(), AnimationKind::Train);
    }
}
