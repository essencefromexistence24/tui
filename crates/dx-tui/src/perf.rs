use std::time::{Duration, Instant};

pub struct PerfMonitor {
	operation_start: Option<Instant>,
	pub stats: PerfStats,
}

#[derive(Debug, Clone, Default)]
pub struct PerfStats {
	pub avg_input_render_ms: f64,
	pub max_input_render_ms: f64,
	pub min_input_render_ms: f64,
	pub avg_keystroke_latency_ms: f64,
	pub avg_frame_render_ms: f64,
	pub total_samples: usize,
}

impl PerfMonitor {
	pub fn new() -> Self {
		Self { operation_start: None, stats: PerfStats::default() }
	}

	pub fn start_timing(&mut self) {
		self.operation_start = Some(Instant::now());
	}

	pub fn record_input_render(&mut self) -> Duration {
		let duration = self.operation_start.map(|start| start.elapsed()).unwrap_or_default();
		self.operation_start = None;
		self.stats.total_samples += 1;
		let ms = duration.as_secs_f64() * 1000.0;
		if self.stats.total_samples == 1 {
			self.stats.avg_input_render_ms = ms;
			self.stats.max_input_render_ms = ms;
			self.stats.min_input_render_ms = ms;
		} else {
			self.stats.avg_input_render_ms = self.stats.avg_input_render_ms
				+ (ms - self.stats.avg_input_render_ms) / self.stats.total_samples as f64;
			self.stats.max_input_render_ms = self.stats.max_input_render_ms.max(ms);
			self.stats.min_input_render_ms = self.stats.min_input_render_ms.min(ms);
		}
		duration
	}

	pub fn get_stats(&self) -> &PerfStats {
		&self.stats
	}

	pub fn is_meeting_targets(&self) -> bool {
		self.stats.avg_input_render_ms < 16.0
	}
}

impl Default for PerfMonitor {
	fn default() -> Self {
		Self::new()
	}
}
