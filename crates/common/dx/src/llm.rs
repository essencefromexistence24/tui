#[cfg(feature = "llm")]
mod real {

	use anyhow::{Context, Result};
	use llama_cpp_2::context::params::LlamaContextParams;
	use llama_cpp_2::llama_backend::LlamaBackend;
	use llama_cpp_2::llama_batch::LlamaBatch;
	use llama_cpp_2::model::params::LlamaModelParams;
	use llama_cpp_2::model::{AddBos, LlamaModel};
	use llama_cpp_2::sampling::LlamaSampler;
	use std::env;
	use std::num::NonZeroU32;
	use std::path::PathBuf;
	use std::sync::{Arc, Mutex};
	use std::time::{SystemTime, UNIX_EPOCH};
	use sysinfo::System;

	const MODEL_PATH_ENV: &str = "DX_TUI_MODEL_PATH";
	const DEFAULT_MODEL_PATH: &str = "models/llm/model.gguf";

	const SYSTEM_PROMPT: &str = "\
# IDENTITY
You are Dx — the AI core of DX, the world's fastest development experience platform. \
You are built in Rust, you run locally, and you are free. You are not a cloud chatbot. \
You are a precision engineering tool that lives on the developer's own machine.

# VOICE
- You speak like a senior staff engineer: direct, technically precise, zero filler.
- Short sentences for clarity. Longer sentences only when technical depth demands it.
- NEVER use corporate buzzwords: \"leverage\", \"synergy\", \"revolutionize\", \"delve\", \"I'd be happy to\".
- NEVER start responses with \"Great question\" or \"That's a great question\" or any sycophantic opener.
- NEVER apologize unless you made a factual error. \"Sorry\" is not a filler word.
- First word of your response should be substantive content, not pleasantries.
";

	const INFERENCE_CONTEXT_TOKENS: u32 = 32_768;
	const PROMPT_BATCH_SIZE: usize = 512;
	const SAMPLER_TEMPERATURE: f32 = 0.7;
	const SAMPLER_TOP_P: f32 = 0.92;
	const SAMPLER_TOP_K: i32 = 40;
	const SAMPLER_MIN_P: f32 = 0.05;
	const SAMPLER_REPEAT_LAST_N: i32 = 256;
	const SAMPLER_REPEAT_PENALTY: f32 = 1.10;

	#[derive(Clone)]
	struct Message {
		role: String,
		content: String,
	}

	struct LocalLlmInner {
		backend: LlamaBackend,
		model: LlamaModel,
		history: Vec<Message>,
	}

	#[derive(Clone)]
	pub struct LocalLlm {
		inner: Arc<Mutex<Option<LocalLlmInner>>>,
	}

	impl LocalLlm {
		pub fn new() -> Self {
			Self { inner: Arc::new(Mutex::new(None)) }
		}

		pub async fn initialize(&self) -> Result<()> {
			let mut inner = self.inner.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

			if inner.is_some() {
				return Ok(());
			}

			let mut backend = LlamaBackend::init().context("Failed to initialize llama backend")?;
			backend.void_logs();

			let model_params = LlamaModelParams::default().with_n_gpu_layers(999);
			let model_path = Self::model_path();
			let model =
				LlamaModel::load_from_file(&backend, &model_path, &model_params).with_context(|| {
					format!(
						"Failed to load model from path: {}. Set {MODEL_PATH_ENV} to a local GGUF model path.",
						model_path.display()
					)
				})?;

			*inner = Some(LocalLlmInner { backend, model, history: Vec::new() });

			Ok(())
		}

		pub async fn generate(&self, prompt: &str) -> Result<String> {
			let mut inner_guard = self.inner.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
			let inner = inner_guard.as_mut().ok_or_else(|| anyhow::anyhow!("LLM not initialized"))?;

			inner.history.push(Message { role: "user".to_string(), content: prompt.to_string() });

			let full_prompt = Self::build_prompt(&inner.history);

			let n_threads = Self::optimal_thread_count();
			let ctx_params = LlamaContextParams::default()
				.with_n_ctx(NonZeroU32::new(INFERENCE_CONTEXT_TOKENS))
				.with_n_batch(PROMPT_BATCH_SIZE as u32)
				.with_n_threads(n_threads)
				.with_n_threads_batch(n_threads)
				.with_flash_attention_policy(1);

			let mut ctx = inner
				.model
				.new_context(&inner.backend, ctx_params.clone())
				.or_else(|e| {
					#[cfg(debug_assertions)]
					eprintln!(
						"Warning: Flash attention context creation failed, falling back to standard attention ({})",
						e
					);
					let fallback_params = LlamaContextParams::default()
						.with_n_ctx(NonZeroU32::new(INFERENCE_CONTEXT_TOKENS))
						.with_n_batch(PROMPT_BATCH_SIZE as u32)
						.with_n_threads(n_threads)
						.with_n_threads_batch(n_threads)
						.with_flash_attention_policy(0);
					inner.model.new_context(&inner.backend, fallback_params)
				})
				.context("Failed to create inference context")?;

			ctx.clear_kv_cache();

			let tokens =
				inner.model.str_to_token(&full_prompt, AddBos::Always).context("Tokenization failed")?;

			let available = (INFERENCE_CONTEXT_TOKENS as usize).saturating_sub(tokens.len());
			let max_tokens = available.min(4096);

			// Batched prompt evaluation
			let mut pos: i32 = 0;
			let total = tokens.len();
			let mut offset = 0;

			while offset < total {
				let end = (offset + PROMPT_BATCH_SIZE).min(total);
				let chunk = &tokens[offset..end];
				let is_last_chunk = end == total;

				let mut batch = LlamaBatch::new(chunk.len(), 1);
				for (i, &token) in chunk.iter().enumerate() {
					let logits = is_last_chunk && i == chunk.len() - 1;
					batch.add(token, pos, &[0], logits)?;
					pos += 1;
				}
				ctx.decode(&mut batch)?;
				offset = end;
			}

			// Sampler chain
			let mut sampler = LlamaSampler::chain_simple([
				LlamaSampler::penalties(SAMPLER_REPEAT_LAST_N, SAMPLER_REPEAT_PENALTY, 0.0, 0.0),
				LlamaSampler::top_k(SAMPLER_TOP_K),
				LlamaSampler::top_p(SAMPLER_TOP_P, 1),
				LlamaSampler::min_p(SAMPLER_MIN_P, 1),
				LlamaSampler::temp(SAMPLER_TEMPERATURE),
				LlamaSampler::dist(Self::sampler_seed()),
			]);
			sampler.accept_many(tokens.iter().copied());

			// Generation loop
			let mut n_cur = tokens.len() as i32;
			let mut generated_text = String::with_capacity(max_tokens * 4);
			let mut gen_batch = LlamaBatch::new(1, 1);

			let mut hit_limit = false;
			let mut extra_tokens = 0;
			let max_loop = max_tokens + 50;

			for i in 0..max_loop {
				if i >= max_tokens {
					hit_limit = true;
				}
				if n_cur >= INFERENCE_CONTEXT_TOKENS as i32 {
					break;
				}

				let token = sampler.sample(&ctx, -1);

				if inner.model.is_eog_token(token) {
					break;
				}

				#[allow(deprecated)]
				let piece_bytes = inner.model.token_to_bytes(token, llama_cpp_2::model::Special::Tokenize)?;
				let piece = String::from_utf8_lossy(&piece_bytes);
				generated_text.push_str(&piece);

				gen_batch.clear();
				gen_batch.add(token, n_cur, &[0], true)?;
				n_cur += 1;

				ctx.decode(&mut gen_batch)?;

				if hit_limit {
					let last_char = piece.chars().last().unwrap_or(' ');
					if last_char == '.' || last_char == '?' || last_char == '!' || piece.contains('\n') {
						break;
					}
					extra_tokens += 1;
					if extra_tokens >= 50 {
						generated_text.push_str("...");
						break;
					}
				}
			}

			let answer = generated_text.trim().to_string();
			if !answer.is_empty() {
				inner.history.push(Message { role: "assistant".to_string(), content: answer.clone() });
			}

			Ok(answer)
		}

		pub async fn generate_stream<F>(&self, prompt: &str, callback: F) -> Result<()>
		where
			F: Fn(String) + Send + 'static,
		{
			let mut inner_guard = self.inner.lock().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
			let inner = inner_guard.as_mut().ok_or_else(|| anyhow::anyhow!("LLM not initialized"))?;

			inner.history.push(Message { role: "user".to_string(), content: prompt.to_string() });

			let full_prompt = Self::build_prompt(&inner.history);

			let n_threads = Self::optimal_thread_count();
			let ctx_params = LlamaContextParams::default()
				.with_n_ctx(NonZeroU32::new(INFERENCE_CONTEXT_TOKENS))
				.with_n_batch(PROMPT_BATCH_SIZE as u32)
				.with_n_threads(n_threads)
				.with_n_threads_batch(n_threads)
				.with_flash_attention_policy(1);

			let mut ctx = inner
				.model
				.new_context(&inner.backend, ctx_params.clone())
				.or_else(|e| {
					#[cfg(debug_assertions)]
					eprintln!(
						"Warning: Flash attention context creation failed, falling back to standard attention ({})",
						e
					);
					let fallback_params = LlamaContextParams::default()
						.with_n_ctx(NonZeroU32::new(INFERENCE_CONTEXT_TOKENS))
						.with_n_batch(PROMPT_BATCH_SIZE as u32)
						.with_n_threads(n_threads)
						.with_n_threads_batch(n_threads)
						.with_flash_attention_policy(0);
					inner.model.new_context(&inner.backend, fallback_params)
				})
				.context("Failed to create inference context")?;

			ctx.clear_kv_cache();

			let tokens =
				inner.model.str_to_token(&full_prompt, AddBos::Always).context("Tokenization failed")?;

			let available = (INFERENCE_CONTEXT_TOKENS as usize).saturating_sub(tokens.len());
			let max_tokens = available.min(4096);

			// Batched prompt evaluation
			let mut pos: i32 = 0;
			let total = tokens.len();
			let mut offset = 0;

			while offset < total {
				let end = (offset + PROMPT_BATCH_SIZE).min(total);
				let chunk = &tokens[offset..end];
				let is_last_chunk = end == total;

				let mut batch = LlamaBatch::new(chunk.len(), 1);
				for (i, &token) in chunk.iter().enumerate() {
					let logits = is_last_chunk && i == chunk.len() - 1;
					batch.add(token, pos, &[0], logits)?;
					pos += 1;
				}
				ctx.decode(&mut batch)?;
				offset = end;
			}

			// Sampler chain
			let mut sampler = LlamaSampler::chain_simple([
				LlamaSampler::penalties(SAMPLER_REPEAT_LAST_N, SAMPLER_REPEAT_PENALTY, 0.0, 0.0),
				LlamaSampler::top_k(SAMPLER_TOP_K),
				LlamaSampler::top_p(SAMPLER_TOP_P, 1),
				LlamaSampler::min_p(SAMPLER_MIN_P, 1),
				LlamaSampler::temp(SAMPLER_TEMPERATURE),
				LlamaSampler::dist(Self::sampler_seed()),
			]);
			sampler.accept_many(tokens.iter().copied());

			// Generation loop with streaming
			let mut n_cur = tokens.len() as i32;
			let mut generated_text = String::with_capacity(max_tokens * 4);
			let mut gen_batch = LlamaBatch::new(1, 1);

			let mut hit_limit = false;
			let mut extra_tokens = 0;
			let max_loop = max_tokens + 50;

			for i in 0..max_loop {
				if i >= max_tokens {
					hit_limit = true;
				}
				if n_cur >= INFERENCE_CONTEXT_TOKENS as i32 {
					break;
				}

				let token = sampler.sample(&ctx, -1);

				if inner.model.is_eog_token(token) {
					break;
				}

				#[allow(deprecated)]
				let piece_bytes = inner.model.token_to_bytes(token, llama_cpp_2::model::Special::Tokenize)?;
				let piece = String::from_utf8_lossy(&piece_bytes);

				// Stream each token as it's generated
				callback(piece.to_string());
				generated_text.push_str(&piece);

				gen_batch.clear();
				gen_batch.add(token, n_cur, &[0], true)?;
				n_cur += 1;

				ctx.decode(&mut gen_batch)?;

				if hit_limit {
					let last_char = piece.chars().last().unwrap_or(' ');
					if last_char == '.' || last_char == '?' || last_char == '!' || piece.contains('\n') {
						break;
					}
					extra_tokens += 1;
					if extra_tokens >= 50 {
						callback("...".to_string());
						generated_text.push_str("...");
						break;
					}
				}
			}

			let answer = generated_text.trim().to_string();
			if !answer.is_empty() {
				inner.history.push(Message { role: "assistant".to_string(), content: answer });
			}

			Ok(())
		}

		pub fn is_initialized(&self) -> bool {
			self.inner.lock().map(|guard| guard.is_some()).unwrap_or(false)
		}

		pub fn get_model_name(&self) -> String {
			"Local LLM".to_string()
		}

		fn build_prompt(history: &[Message]) -> String {
			let mut prompt = String::with_capacity(4096);
			prompt.push_str("<|im_start|>system\n");
			prompt.push_str(SYSTEM_PROMPT);
			prompt.push_str("<|im_end|>\n");

			for msg in history {
				prompt.push_str("<|im_start|>");
				prompt.push_str(&msg.role);
				prompt.push('\n');
				prompt.push_str(&msg.content);
				prompt.push_str("<|im_end|>\n");
			}

			prompt.push_str("<|im_start|>assistant\n");
			prompt
		}

		fn optimal_thread_count() -> i32 {
			let _sys = System::new_all();
			let physical = sysinfo::System::physical_core_count().unwrap_or(1).max(1);
			if physical > 4 { (physical - 1) as i32 } else { physical as i32 }
		}

		fn model_path() -> PathBuf {
			env::var_os(MODEL_PATH_ENV)
				.map(PathBuf::from)
				.unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_PATH))
		}

		fn sampler_seed() -> u32 {
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.map(|d| d.as_nanos() as u32)
				.unwrap_or(0xDEAD_BEEF)
		}
	}

	impl Default for LocalLlm {
		fn default() -> Self {
			Self::new()
		}
	}
}

#[cfg(feature = "llm")]
pub use real::LocalLlm;
