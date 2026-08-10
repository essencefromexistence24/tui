//! Core RLM implementation and library-first long-context APIs.
//!
//! This module exposes both the low-level `complete(...)` loop and the higher
//! level document/task abstractions needed by host applications such as DX,
//! Zed forks, Codex forks, and ZeroClaw forks.

use crate::error::{RLMError, Result};
use crate::llm::{LLMClient, LLMProviderConfig, Message};
use crate::parser::{extract_final, is_final};
use crate::repl::REPLExecutor;
use regex::Regex;
use rhai::Scope;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLMStats {
    pub llm_calls: usize,
    pub iterations: usize,
    pub elapsed_ms: u128,
    pub ast_cache_hits: usize,
    pub ast_cache_misses: usize,
    pub llm_cache_hits: usize,
    pub llm_cache_misses: usize,
    pub fast_model_calls: usize,
    pub smart_model_calls: usize,
}

impl RLMStats {
    pub fn zero() -> Self {
        Self {
            llm_calls: 0,
            iterations: 0,
            elapsed_ms: 0,
            ast_cache_hits: 0,
            ast_cache_misses: 0,
            llm_cache_hits: 0,
            llm_cache_misses: 0,
            fast_model_calls: 0,
            smart_model_calls: 0,
        }
    }

    pub fn combine(&self, other: &Self) -> Self {
        Self {
            llm_calls: self.llm_calls + other.llm_calls,
            iterations: self.iterations + other.iterations,
            elapsed_ms: self.elapsed_ms + other.elapsed_ms,
            ast_cache_hits: self.ast_cache_hits + other.ast_cache_hits,
            ast_cache_misses: self.ast_cache_misses + other.ast_cache_misses,
            llm_cache_hits: self.llm_cache_hits + other.llm_cache_hits,
            llm_cache_misses: self.llm_cache_misses + other.llm_cache_misses,
            fast_model_calls: self.fast_model_calls + other.fast_model_calls,
            smart_model_calls: self.smart_model_calls + other.smart_model_calls,
        }
    }

    pub fn cache_hit_rate(&self) -> f64 {
        let total_ast = self.ast_cache_hits + self.ast_cache_misses;
        let total_llm = self.llm_cache_hits + self.llm_cache_misses;

        if total_ast + total_llm == 0 {
            return 0.0;
        }

        let hits = self.ast_cache_hits + self.llm_cache_hits;
        let total = total_ast + total_llm;
        ((hits as f64 / total as f64) * 10_000.0).round() / 100.0
    }

    pub fn cost_savings(&self) -> f64 {
        let total_calls = self.fast_model_calls + self.smart_model_calls;
        if total_calls == 0 {
            return 0.0;
        }

        let baseline_cost = total_calls as f64;
        let actual_cost = (self.fast_model_calls as f64 * 0.1) + self.smart_model_calls as f64;
        ((baseline_cost - actual_cost) / baseline_cost * 10_000.0).round() / 100.0
    }

    pub fn average_iteration_latency_ms(&self) -> f64 {
        if self.iterations == 0 {
            return 0.0;
        }

        ((self.elapsed_ms as f64 / self.iterations as f64) * 100.0).round() / 100.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RLMProfile {
    LowMemory,
    Balanced,
    HighThroughput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RLMTaskKind {
    QuestionAnswering,
    SummarizeDocument,
    BuildAgentContext,
    ExtractEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RLMRunMode {
    Standard,
    Streaming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLMDocument {
    pub id: String,
    pub title: Option<String>,
    pub content: String,
    pub source_path: Option<String>,
    pub mime_type: Option<String>,
    pub tags: Vec<String>,
}

impl RLMDocument {
    pub fn from_text(id: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            id: id.into(),
            title: infer_title(&content),
            content,
            source_path: None,
            mime_type: Some("text/plain".to_string()),
            tags: Vec::new(),
        }
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("document")
            .to_string();

        Ok(Self {
            id: file_name.clone(),
            title: infer_title(&content).or(Some(file_name.clone())),
            content,
            source_path: Some(path.display().to_string()),
            mime_type: Some("text/plain".to_string()),
            tags: Vec::new(),
        })
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_source_path(mut self, source_path: impl Into<String>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }

    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn shared_content(&self) -> Arc<String> {
        Arc::new(self.content.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLMRequest {
    pub task: RLMTaskKind,
    pub query: String,
    pub document: RLMDocument,
    pub mode: RLMRunMode,
    pub max_iterations_override: Option<usize>,
    pub hint_keywords: Vec<String>,
}

impl RLMRequest {
    pub fn question(query: impl Into<String>, document: RLMDocument) -> Self {
        Self {
            task: RLMTaskKind::QuestionAnswering,
            query: query.into(),
            document,
            mode: RLMRunMode::Standard,
            max_iterations_override: None,
            hint_keywords: Vec::new(),
        }
    }

    pub fn summary(document: RLMDocument) -> Self {
        Self {
            task: RLMTaskKind::SummarizeDocument,
            query: "Summarize the document with concrete facts, major sections, and actionable takeaways.".to_string(),
            document,
            mode: RLMRunMode::Standard,
            max_iterations_override: None,
            hint_keywords: Vec::new(),
        }
    }

    pub fn agent_context(goal: impl Into<String>, document: RLMDocument) -> Self {
        Self {
            task: RLMTaskKind::BuildAgentContext,
            query: goal.into(),
            document,
            mode: RLMRunMode::Standard,
            max_iterations_override: None,
            hint_keywords: Vec::new(),
        }
    }

    pub fn with_mode(mut self, mode: RLMRunMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations_override = Some(max_iterations.max(1));
        self
    }

    pub fn with_hint_keywords(mut self, hint_keywords: Vec<String>) -> Self {
        self.hint_keywords = hint_keywords;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLMResponse {
    pub answer: String,
    pub stats: RLMStats,
    pub task: RLMTaskKind,
    pub mode: RLMRunMode,
    pub document_id: String,
    pub document_title: Option<String>,
    pub source_path: Option<String>,
    pub provider: String,
    pub primary_model: String,
    pub fast_model: Option<String>,
    pub evidence_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLMChunkingConfig {
    pub recursive_threshold_chars: usize,
    pub target_chunk_chars: usize,
    pub overlap_chars: usize,
    pub max_reduce_passes: usize,
    pub max_chunks_per_pass: usize,
    pub per_chunk_max_iterations: usize,
}

impl Default for RLMChunkingConfig {
    fn default() -> Self {
        Self {
            recursive_threshold_chars: 18_000,
            target_chunk_chars: 8_000,
            overlap_chars: 500,
            max_reduce_passes: 3,
            max_chunks_per_pass: 24,
            per_chunk_max_iterations: 12,
        }
    }
}

impl RLMChunkingConfig {
    pub fn for_profile(profile: RLMProfile) -> Self {
        match profile {
            RLMProfile::LowMemory => Self::low_memory(),
            RLMProfile::Balanced => Self::default(),
            RLMProfile::HighThroughput => Self::high_throughput(),
        }
    }

    pub fn low_memory() -> Self {
        Self {
            recursive_threshold_chars: 12_000,
            target_chunk_chars: 4_500,
            overlap_chars: 300,
            max_reduce_passes: 3,
            max_chunks_per_pass: 16,
            per_chunk_max_iterations: 8,
        }
    }

    pub fn high_throughput() -> Self {
        Self {
            recursive_threshold_chars: 28_000,
            target_chunk_chars: 12_000,
            overlap_chars: 750,
            max_reduce_passes: 4,
            max_chunks_per_pass: 32,
            per_chunk_max_iterations: 14,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLMChunk {
    pub index: usize,
    pub start_char: usize,
    pub end_char: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLMReductionPass {
    pub level: usize,
    pub input_chars: usize,
    pub output_chars: usize,
    pub chunk_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLMRecursiveResponse {
    pub response: RLMResponse,
    pub aggregate_stats: RLMStats,
    pub reduction_passes: Vec<RLMReductionPass>,
    pub final_context_chars: usize,
    pub was_reduced: bool,
}

impl RLMRecursiveResponse {
    pub fn reduction_ratio(&self, original_context_chars: usize) -> f64 {
        if original_context_chars == 0 {
            return 0.0;
        }

        let reduced = 1.0 - (self.final_context_chars as f64 / original_context_chars as f64);
        (reduced.clamp(0.0, 1.0) * 10_000.0).round() / 100.0
    }
}

pub struct RLMBuilder {
    provider: LLMProviderConfig,
    model: String,
    fast_model: Option<String>,
    max_iterations: usize,
    max_depth: usize,
    current_depth: usize,
    profile: RLMProfile,
    temperature: f32,
    max_tokens: u32,
    cache_limit: usize,
}

impl RLMBuilder {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: LLMProviderConfig::groq(api_key.into()),
            model: model.into(),
            fast_model: None,
            max_iterations: 24,
            max_depth: 5,
            current_depth: 0,
            profile: RLMProfile::Balanced,
            temperature: 0.2,
            max_tokens: 1024,
            cache_limit: 500,
        }
    }

    pub fn openai_compatible(
        api_key: impl Into<String>,
        chat_completions_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            provider: LLMProviderConfig::openai_compatible(api_key, chat_completions_url),
            model: model.into(),
            fast_model: None,
            max_iterations: 24,
            max_depth: 5,
            current_depth: 0,
            profile: RLMProfile::Balanced,
            temperature: 0.2,
            max_tokens: 1024,
            cache_limit: 500,
        }
    }

    pub fn with_provider(mut self, provider: LLMProviderConfig) -> Self {
        self.provider = provider;
        self
    }

    pub fn with_fast_model(mut self, fast_model: impl Into<String>) -> Self {
        self.fast_model = Some(fast_model.into());
        self
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations.max(1);
        self
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth.max(1);
        self
    }

    pub fn with_depth(mut self, current_depth: usize) -> Self {
        self.current_depth = current_depth;
        self
    }

    pub fn with_profile(mut self, profile: RLMProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.clamp(0.0, 2.0);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens.max(64);
        self
    }

    pub fn with_cache_limit(mut self, cache_limit: usize) -> Self {
        self.cache_limit = cache_limit.max(1);
        self
    }

    pub fn build_checked(self) -> Result<RLM> {
        if self.provider.api_key.trim().is_empty() {
            return Err(RLMError::MissingConfiguration(
                "provider api_key cannot be empty".to_string(),
            ));
        }

        if self.provider.chat_completions_url.trim().is_empty() {
            return Err(RLMError::ConfigurationError(
                "provider chat_completions_url cannot be empty".to_string(),
            ));
        }

        Ok(self.build())
    }

    pub fn build(self) -> RLM {
        let llm_client = LLMClient::from_provider(self.provider, self.model)
            .with_temperature(self.temperature)
            .with_max_tokens(self.max_tokens)
            .with_cache_limit(self.cache_limit);

        let llm_client = match self.fast_model {
            Some(fast_model) => llm_client.with_fast_model(fast_model),
            None => llm_client,
        };

        RLM {
            llm_client,
            repl: REPLExecutor::new(),
            max_iterations: self.max_iterations,
            max_depth: self.max_depth,
            current_depth: self.current_depth,
            profile: self.profile,
        }
    }
}

pub struct RLM {
    llm_client: LLMClient,
    repl: REPLExecutor,
    max_iterations: usize,
    max_depth: usize,
    current_depth: usize,
    profile: RLMProfile,
}

impl Clone for RLM {
    fn clone(&self) -> Self {
        Self {
            llm_client: self.llm_client.clone(),
            repl: REPLExecutor::new(),
            max_iterations: self.max_iterations,
            max_depth: self.max_depth,
            current_depth: self.current_depth,
            profile: self.profile,
        }
    }
}

impl RLM {
    pub fn builder(api_key: impl Into<String>, model: impl Into<String>) -> RLMBuilder {
        RLMBuilder::new(api_key, model)
    }

    pub fn openai_compatible(
        api_key: impl Into<String>,
        chat_completions_url: impl Into<String>,
        model: impl Into<String>,
    ) -> RLMBuilder {
        RLMBuilder::openai_compatible(api_key, chat_completions_url, model)
    }

    pub fn new(api_key: String, model: String) -> Self {
        RLMBuilder::new(api_key, model).build()
    }

    pub fn from_provider(provider: LLMProviderConfig, model: impl Into<String>) -> Self {
        let model = model.into();
        RLMBuilder::new(String::new(), model)
            .with_provider(provider)
            .build()
    }

    pub fn from_env_groq(model: impl Into<String>) -> Result<Self> {
        let provider = LLMProviderConfig::groq_from_env("RLM_API_KEY")
            .or_else(|_| LLMProviderConfig::groq_from_env("GROQ_API_KEY"))?;
        Ok(Self::from_provider(provider, model))
    }

    pub fn from_env_openai_compatible(
        model: impl Into<String>,
        chat_completions_url_env: &str,
        fallback_chat_completions_url: &str,
    ) -> Result<Self> {
        let provider = LLMProviderConfig::openai_compatible_from_env(
            "RLM_API_KEY",
            chat_completions_url_env,
            fallback_chat_completions_url,
        )
        .or_else(|_| {
            LLMProviderConfig::openai_compatible_from_env(
                "GROQ_API_KEY",
                chat_completions_url_env,
                fallback_chat_completions_url,
            )
        })?;

        Ok(Self::from_provider(provider, model))
    }

    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations.max(1);
        self
    }

    pub fn with_max_depth(mut self, max_depth: usize) -> Self {
        self.max_depth = max_depth.max(1);
        self
    }

    pub fn with_depth(mut self, depth: usize) -> Self {
        self.current_depth = depth;
        self
    }

    pub fn with_profile(mut self, profile: RLMProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn with_fast_model(mut self, fast_model: String) -> Self {
        self.llm_client = self.llm_client.with_fast_model(fast_model);
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.llm_client = self.llm_client.with_temperature(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.llm_client = self.llm_client.with_max_tokens(max_tokens);
        self
    }

    pub fn with_cache_limit(mut self, cache_limit: usize) -> Self {
        self.llm_client = self.llm_client.with_cache_limit(cache_limit);
        self
    }

    pub fn with_provider(mut self, provider: LLMProviderConfig) -> Self {
        self.llm_client = self.llm_client.with_provider(provider);
        self
    }

    pub fn provider_config(&self) -> &LLMProviderConfig {
        self.llm_client.provider_config()
    }

    pub fn provider_label(&self) -> &str {
        self.llm_client.provider_label()
    }

    pub fn execution_profile(&self) -> RLMProfile {
        self.profile
    }

    pub fn recommended_chunking_config(&self) -> RLMChunkingConfig {
        RLMChunkingConfig::for_profile(self.profile)
    }

    pub fn should_reduce_document(&self, document: &RLMDocument, chunking: &RLMChunkingConfig) -> bool {
        document.content.len() > chunking.recursive_threshold_chars
    }

    pub fn model_stats(&self) -> (usize, usize) {
        self.llm_client.model_stats()
    }

    pub async fn complete_parallel(
        &self,
        queries: Vec<(&str, Arc<String>)>,
    ) -> Result<Vec<Result<(String, RLMStats)>>> {
        let futures = queries.into_iter().map(|(query, context)| {
            let rlm = self.clone();
            let query = query.to_string();
            async move { rlm.complete_with_arc(&query, context).await }
        });

        Ok(futures::future::join_all(futures).await)
    }

    pub async fn complete(&self, query: &str, context: &str) -> Result<(String, RLMStats)> {
        self.complete_with_arc(query, Arc::new(context.to_string())).await
    }

    pub async fn complete_with_arc(
        &self,
        query: &str,
        context: Arc<String>,
    ) -> Result<(String, RLMStats)> {
        self.run_loop(query, context, RLMRunMode::Standard, None).await
    }

    pub async fn complete_streaming(
        &self,
        query: &str,
        context: Arc<String>,
    ) -> Result<(String, RLMStats)> {
        self.run_loop(query, context, RLMRunMode::Streaming, None).await
    }

    pub async fn complete_request(&self, request: RLMRequest) -> Result<RLMResponse> {
        let query = decorate_query(&request);
        let context = request.document.shared_content();
        let (answer, stats) = self
            .run_loop(
                &query,
                context,
                request.mode,
                request.max_iterations_override,
            )
            .await?;

        let evidence_excerpt =
            build_evidence_excerpt(&request.document.content, &request.query, &request.hint_keywords);
        let document = request.document;
        let document_id = document.id;
        let document_title = document.title;
        let source_path = document.source_path;

        Ok(RLMResponse {
            answer,
            stats,
            task: request.task,
            mode: request.mode,
            document_id,
            document_title,
            source_path,
            provider: self.provider_label().to_string(),
            primary_model: self.llm_client.model_name().to_string(),
            fast_model: self.llm_client.fast_model_name().map(str::to_string),
            evidence_excerpt,
        })
    }

    pub async fn complete_document(
        &self,
        query: impl Into<String>,
        document: RLMDocument,
    ) -> Result<RLMResponse> {
        self.complete_request(RLMRequest::question(query, document)).await
    }

    pub async fn complete_document_recursive(
        &self,
        query: impl Into<String>,
        document: RLMDocument,
        chunking: RLMChunkingConfig,
    ) -> Result<RLMRecursiveResponse> {
        self.complete_request_recursive(RLMRequest::question(query, document), chunking)
            .await
    }

    pub async fn summarize_document(&self, document: RLMDocument) -> Result<RLMResponse> {
        self.complete_request(RLMRequest::summary(document)).await
    }

    pub async fn summarize_document_recursive(
        &self,
        document: RLMDocument,
        chunking: RLMChunkingConfig,
    ) -> Result<RLMRecursiveResponse> {
        self.complete_request_recursive(RLMRequest::summary(document), chunking)
            .await
    }

    pub async fn build_agent_context(
        &self,
        goal: impl Into<String>,
        document: RLMDocument,
    ) -> Result<RLMResponse> {
        self.complete_request(RLMRequest::agent_context(goal, document)).await
    }

    pub async fn build_agent_context_recursive(
        &self,
        goal: impl Into<String>,
        document: RLMDocument,
        chunking: RLMChunkingConfig,
    ) -> Result<RLMRecursiveResponse> {
        self.complete_request_recursive(RLMRequest::agent_context(goal, document), chunking)
            .await
    }

    pub async fn complete_file(
        &self,
        query: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Result<RLMResponse> {
        self.complete_document(query, RLMDocument::from_file(path)?).await
    }

    pub async fn summarize_file(&self, path: impl AsRef<Path>) -> Result<RLMResponse> {
        self.summarize_document(RLMDocument::from_file(path)?).await
    }

    pub async fn complete_document_auto(
        &self,
        query: impl Into<String>,
        document: RLMDocument,
    ) -> Result<RLMRecursiveResponse> {
        self.complete_document_recursive(query, document, self.recommended_chunking_config())
            .await
    }

    pub async fn summarize_document_auto(
        &self,
        document: RLMDocument,
    ) -> Result<RLMRecursiveResponse> {
        self.summarize_document_recursive(document, self.recommended_chunking_config())
            .await
    }

    pub async fn build_agent_context_auto(
        &self,
        goal: impl Into<String>,
        document: RLMDocument,
    ) -> Result<RLMRecursiveResponse> {
        self.build_agent_context_recursive(goal, document, self.recommended_chunking_config())
            .await
    }

    pub async fn complete_file_recursive(
        &self,
        query: impl Into<String>,
        path: impl AsRef<Path>,
        chunking: RLMChunkingConfig,
    ) -> Result<RLMRecursiveResponse> {
        self.complete_document_recursive(query, RLMDocument::from_file(path)?, chunking)
            .await
    }

    pub async fn summarize_file_recursive(
        &self,
        path: impl AsRef<Path>,
        chunking: RLMChunkingConfig,
    ) -> Result<RLMRecursiveResponse> {
        self.summarize_document_recursive(RLMDocument::from_file(path)?, chunking)
            .await
    }

    pub fn chunk_document(&self, document: &RLMDocument, chunking: &RLMChunkingConfig) -> Vec<RLMChunk> {
        let text = &document.content;
        let text_len = text.len();

        if text_len == 0 {
            return Vec::new();
        }

        let target = chunking.target_chunk_chars.max(1);
        let overlap = chunking.overlap_chars.min(target.saturating_sub(1));
        let step = target.saturating_sub(overlap).max(1);

        let mut chunks = Vec::new();
        let mut start = 0usize;
        let mut index = 0usize;

        while start < text_len {
            start = align_boundary_backward(text, start);
            let provisional_end = (start + target).min(text_len);
            let mut end = if provisional_end < text_len {
                align_chunk_end(text, provisional_end)
            } else {
                text_len
            };

            if end <= start {
                end = provisional_end;
            }

            chunks.push(RLMChunk {
                index,
                start_char: start,
                end_char: end,
                text: text[start..end].to_string(),
            });

            if end >= text_len {
                break;
            }

            start = align_boundary_backward(text, end.saturating_sub(overlap));
            if start < end && (end - start) > step {
                start = align_boundary_backward(text, end - step);
            }
            index += 1;
        }

        chunks
    }

    pub async fn complete_request_recursive(
        &self,
        request: RLMRequest,
        chunking: RLMChunkingConfig,
    ) -> Result<RLMRecursiveResponse> {
        if request.document.content.len() <= chunking.recursive_threshold_chars {
            let original_context_chars = request.document.content.len();
            let response = self.complete_request(request).await?;
            let aggregate_stats = response.stats.clone();
            return Ok(RLMRecursiveResponse {
                response,
                aggregate_stats,
                reduction_passes: Vec::new(),
                final_context_chars: original_context_chars,
                was_reduced: false,
            });
        }

        let (reduced_document, reduction_passes, reduction_stats) =
            self.reduce_request_document(&request, &chunking).await?;

        let final_request = RLMRequest {
            document: reduced_document.clone(),
            ..request
        };
        let final_response = self.complete_request(final_request).await?;
        let aggregate_stats = reduction_stats.combine(&final_response.stats);

        Ok(RLMRecursiveResponse {
            final_context_chars: reduced_document.content.len(),
            was_reduced: true,
            response: final_response,
            aggregate_stats,
            reduction_passes,
        })
    }

    async fn run_loop(
        &self,
        query: &str,
        context: Arc<String>,
        mode: RLMRunMode,
        max_iterations_override: Option<usize>,
    ) -> Result<(String, RLMStats)> {
        let start = Instant::now();

        if self.current_depth >= self.max_depth {
            return Err(RLMError::MaxDepth(self.max_depth));
        }

        let max_iterations = max_iterations_override.unwrap_or(self.max_iterations);
        let system_prompt =
            build_system_prompt(context.len(), self.current_depth, self.profile);

        let mut messages = vec![
            Message {
                role: "system".to_string(),
                content: system_prompt,
            },
            Message {
                role: "user".to_string(),
                content: query.to_string(),
            },
        ];

        let mut scope = Scope::new();
        scope.push("context", (*context).clone());
        scope.push("query", query.to_string());

        let (start_ast_hits, start_ast_misses) = self.repl.cache_stats();
        let (start_llm_hits, start_llm_misses) = self.llm_client.cache_stats();
        let (start_fast_calls, start_smart_calls) = self.llm_client.model_stats();

        let mut llm_calls = 0;

        for iteration in 0..max_iterations {
            let iterations = iteration + 1;
            llm_calls += 1;

            let response = match mode {
                RLMRunMode::Standard => self.llm_client.complete(messages.clone()).await?,
                RLMRunMode::Streaming => {
                    let mut receiver = self.llm_client.stream(messages.clone()).await?;
                    let mut response = String::new();

                    while let Some(token) = receiver.recv().await {
                        response.push_str(&token);

                        if is_final(&response) {
                            break;
                        }
                    }

                    response
                }
            };

            if is_final(&response) {
                if let Some(answer) = extract_final(&response) {
                    return Ok((
                        answer,
                        self.build_stats(
                            llm_calls,
                            iterations,
                            &start,
                            (start_ast_hits, start_ast_misses),
                            (start_llm_hits, start_llm_misses),
                            (start_fast_calls, start_smart_calls),
                        ),
                    ));
                }
            }

            let exec_result = match self.repl.execute(&response, &mut scope) {
                Ok(result) => result,
                Err(err) => format!("Error: {err}"),
            };

            messages.push(Message {
                role: "assistant".to_string(),
                content: response,
            });
            messages.push(Message {
                role: "user".to_string(),
                content: exec_result,
            });
        }

        Err(RLMError::MaxIterations(max_iterations))
    }

    fn build_stats(
        &self,
        llm_calls: usize,
        iterations: usize,
        start: &Instant,
        ast_baseline: (usize, usize),
        llm_baseline: (usize, usize),
        model_baseline: (usize, usize),
    ) -> RLMStats {
        let elapsed_ms = start.elapsed().as_millis();
        let (ast_hits_now, ast_misses_now) = self.repl.cache_stats();
        let (llm_hits_now, llm_misses_now) = self.llm_client.cache_stats();
        let (fast_calls_now, smart_calls_now) = self.llm_client.model_stats();

        RLMStats {
            llm_calls,
            iterations,
            elapsed_ms,
            ast_cache_hits: ast_hits_now.saturating_sub(ast_baseline.0),
            ast_cache_misses: ast_misses_now.saturating_sub(ast_baseline.1),
            llm_cache_hits: llm_hits_now.saturating_sub(llm_baseline.0),
            llm_cache_misses: llm_misses_now.saturating_sub(llm_baseline.1),
            fast_model_calls: fast_calls_now.saturating_sub(model_baseline.0),
            smart_model_calls: smart_calls_now.saturating_sub(model_baseline.1),
        }
    }

    async fn reduce_request_document(
        &self,
        request: &RLMRequest,
        chunking: &RLMChunkingConfig,
    ) -> Result<(RLMDocument, Vec<RLMReductionPass>, RLMStats)> {
        let mut current_document = request.document.clone();
        let mut passes = Vec::new();
        let mut aggregate_stats = RLMStats::zero();

        for level in 0..chunking.max_reduce_passes {
            if current_document.content.len() <= chunking.recursive_threshold_chars {
                break;
            }

            let chunks = self.chunk_document(&current_document, chunking);
            if chunks.len() <= 1 {
                break;
            }

            let mut reduced_sections = Vec::new();
            let mut processed_chunks = 0usize;

            for chunk in chunks.into_iter().take(chunking.max_chunks_per_pass) {
                processed_chunks += 1;

                let chunk_document = RLMDocument::from_text(
                    format!("{}#chunk-{}-{}", current_document.id, level, chunk.index),
                    chunk.text,
                )
                .with_mime_type(
                    current_document
                        .mime_type
                        .clone()
                        .unwrap_or_else(|| "text/plain".to_string()),
                )
                .with_tags(current_document.tags.clone());

                let chunk_query = build_chunk_query(
                    request.task,
                    &request.query,
                    chunk.index,
                    &request.hint_keywords,
                );

                let chunk_request = RLMRequest {
                    task: match request.task {
                        RLMTaskKind::QuestionAnswering | RLMTaskKind::ExtractEvidence => {
                            RLMTaskKind::ExtractEvidence
                        }
                        other => other,
                    },
                    query: chunk_query,
                    document: chunk_document,
                    mode: request.mode,
                    max_iterations_override: Some(
                        request
                            .max_iterations_override
                            .unwrap_or(chunking.per_chunk_max_iterations)
                            .min(chunking.per_chunk_max_iterations.max(1)),
                    ),
                    hint_keywords: request.hint_keywords.clone(),
                };

                let chunk_response = self.complete_request(chunk_request).await?;
                aggregate_stats = aggregate_stats.combine(&chunk_response.stats);
                reduced_sections.push(format!(
                    "[chunk {}]\n{}",
                    chunk.index, chunk_response.answer
                ));
            }

            let reduced_text = reduced_sections.join("\n\n");
            let output_chars = reduced_text.len();

            passes.push(RLMReductionPass {
                level,
                input_chars: current_document.content.len(),
                output_chars,
                chunk_count: processed_chunks,
            });

            if output_chars == 0 || output_chars >= current_document.content.len() {
                break;
            }

            current_document = RLMDocument::from_text(
                format!("{}#reduced-{}", request.document.id, level),
                reduced_text,
            )
            .with_title(
                request
                    .document
                    .title
                    .clone()
                    .unwrap_or_else(|| request.document.id.clone()),
            )
            .with_mime_type(
                request
                    .document
                    .mime_type
                    .clone()
                    .unwrap_or_else(|| "text/plain".to_string()),
            )
            .with_tags(request.document.tags.clone());
        }

        Ok((current_document, passes, aggregate_stats))
    }
}

fn build_system_prompt(context_size: usize, depth: usize, profile: RLMProfile) -> String {
    let profile_note = match profile {
        RLMProfile::LowMemory => {
            "Prefer narrow searches, short excerpts, and early FINAL answers."
        }
        RLMProfile::Balanced => {
            "Balance search breadth with synthesis quality and avoid redundant scans."
        }
        RLMProfile::HighThroughput => {
            "Optimize for fast exploration, reuse helper functions, and summarize incrementally."
        }
    };

    format!(
        r#"You are a Recursive Language Model operating through a constrained Rhai REPL.

The document is available in variable `context`. The user question is in `query`.
Document size: {context_size} characters.
Recursion depth: {depth}.
Execution profile: {profile_note}

You cannot read the document directly outside code execution. You must inspect it using Rhai.

Available fast functions:
- fast_find(text, pattern) -> i64
- fast_rfind(text, pattern) -> i64
- fast_contains(text, pattern) -> bool
- fast_find_all(text, pattern) -> array
- fast_count(text, pattern) -> i64
- window(text, start, len) -> string
- head(text, len) -> string
- tail(text, len) -> string

Rules:
- Search first, answer second.
- Prefer fast_* helpers over naive string scanning.
- Print evidence snippets before concluding.
- Never guess.
- End only with FINAL("...") or FINAL("""...""").
"#
    )
}

fn build_chunk_query(
    task: RLMTaskKind,
    query: &str,
    chunk_index: usize,
    hint_keywords: &[String],
) -> String {
    let hint_text = if hint_keywords.is_empty() {
        String::new()
    } else {
        format!("\nPriority keywords: {}", hint_keywords.join(", "))
    };

    match task {
        RLMTaskKind::QuestionAnswering => format!(
            "Question: {query}\nWork only from chunk {chunk_index}. Extract only concrete evidence that helps answer the question. If the chunk has no relevant evidence, say so briefly.{hint_text}"
        ),
        RLMTaskKind::SummarizeDocument => format!(
            "Summarize chunk {chunk_index}. Keep only high-signal facts, APIs, dates, metrics, decisions, and unique details. Remove repetition.{hint_text}"
        ),
        RLMTaskKind::BuildAgentContext => format!(
            "Goal: {query}\nFrom chunk {chunk_index}, extract only the context an autonomous coding or ops agent needs: files, commands, APIs, constraints, risks, and next steps.{hint_text}"
        ),
        RLMTaskKind::ExtractEvidence => format!(
            "Extract evidence from chunk {chunk_index} relevant to: {query}. Return only concise evidence-backed findings.{hint_text}"
        ),
    }
}

fn decorate_query(request: &RLMRequest) -> String {
    match request.task {
        RLMTaskKind::QuestionAnswering => request.query.clone(),
        RLMTaskKind::SummarizeDocument => format!(
            "{}\nFocus on high-signal sections, metrics, dates, APIs, and decisions.",
            request.query
        ),
        RLMTaskKind::BuildAgentContext => format!(
            "{}\nExtract the minimum context an autonomous coding or ops agent would need: key files, commands, risks, APIs, and open questions.",
            request.query
        ),
        RLMTaskKind::ExtractEvidence => format!(
            "{}\nReturn only evidence-backed findings and keep each finding tied to a concrete snippet.",
            request.query
        ),
    }
}

fn infer_title(content: &str) -> Option<String> {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches('#').trim().to_string())
        .filter(|title| !title.is_empty())
}

fn align_boundary_backward(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn align_chunk_end(text: &str, proposed_end: usize) -> usize {
    let mut end = align_boundary_backward(text, proposed_end);

    let search_start = end.saturating_sub(200);
    if let Some((relative, marker)) = text[search_start..end]
        .match_indices(|character: char| matches!(character, '\n' | '.' | '!' | '?'))
        .last()
    {
        let candidate = search_start + relative + marker.len();
        if candidate > search_start {
            end = candidate;
        }
    }

    align_boundary_backward(text, end)
}

fn build_evidence_excerpt(
    content: &str,
    query: &str,
    hint_keywords: &[String],
) -> Option<String> {
    let mut keywords = hint_keywords.to_vec();
    keywords.extend(extract_keywords(query));

    for keyword in keywords {
        if let Some(index) = find_case_insensitive(content, &keyword) {
            let start = index.saturating_sub(120);
            let end = (index + keyword.len() + 200).min(content.len());
            return Some(content[start..end].trim().to_string());
        }
    }

    None
}

fn extract_keywords(query: &str) -> Vec<String> {
    const STOP_WORDS: &[&str] = &[
        "what", "which", "where", "when", "with", "from", "that", "this", "into", "than",
        "have", "does", "about", "there", "their", "your", "will", "would", "could", "should",
        "use", "using", "please", "find", "show", "give", "make",
    ];

    let mut keywords = Vec::new();

    for token in query
        .split(|character: char| !character.is_alphanumeric() && character != '_' && character != '-')
        .map(|token| token.trim().to_lowercase())
        .filter(|token| token.len() >= 4 && !STOP_WORDS.contains(&token.as_str()))
    {
        if !keywords.contains(&token) {
            keywords.push(token);
        }

        if keywords.len() >= 8 {
            break;
        }
    }

    keywords
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let pattern = regex::escape(needle);
    Regex::new(&format!("(?i){pattern}"))
        .ok()
        .and_then(|regex| regex.find(haystack))
        .map(|match_| match_.start())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_keywords_without_common_fillers() {
        let keywords = extract_keywords("Please find the AI market growth rate in 2024");
        assert!(keywords.contains(&"market".to_string()));
        assert!(keywords.contains(&"growth".to_string()));
        assert!(!keywords.contains(&"please".to_string()));
    }

    #[test]
    fn infers_title_from_first_non_empty_line() {
        let document = RLMDocument::from_text("demo", "\n# Sample Title\n\nBody");
        assert_eq!(document.title.as_deref(), Some("Sample Title"));
    }

    #[test]
    fn request_builder_sets_summary_task() {
        let request = RLMRequest::summary(RLMDocument::from_text("demo", "body"));
        assert_eq!(request.task, RLMTaskKind::SummarizeDocument);
        assert_eq!(request.mode, RLMRunMode::Standard);
    }

    #[test]
    fn chunking_config_can_follow_profile() {
        let config = RLMChunkingConfig::for_profile(RLMProfile::LowMemory);
        assert!(config.target_chunk_chars < RLMChunkingConfig::default().target_chunk_chars);
    }

    #[test]
    fn chunk_document_respects_small_input() {
        let rlm = RLM::new("demo".to_string(), "model".to_string());
        let document = RLMDocument::from_text("demo", "short text");
        let chunks = rlm.chunk_document(&document, &RLMChunkingConfig::default());
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "short text");
    }

    #[test]
    fn recursive_response_reports_reduction_ratio() {
        let response = RLMRecursiveResponse {
            response: RLMResponse {
                answer: "ok".to_string(),
                stats: RLMStats::zero(),
                task: RLMTaskKind::SummarizeDocument,
                mode: RLMRunMode::Standard,
                document_id: "demo".to_string(),
                document_title: None,
                source_path: None,
                provider: "test".to_string(),
                primary_model: "model".to_string(),
                fast_model: None,
                evidence_excerpt: None,
            },
            aggregate_stats: RLMStats::zero(),
            reduction_passes: Vec::new(),
            final_context_chars: 500,
            was_reduced: true,
        };

        assert_eq!(response.reduction_ratio(1000), 50.0);
    }
}
