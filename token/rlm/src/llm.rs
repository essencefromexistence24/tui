use crate::error::{RLMError, Result};
use futures::stream::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LLMAuthScheme {
    Bearer,
    XApiKey,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMProviderConfig {
    pub provider_label: String,
    pub chat_completions_url: String,
    pub api_key: String,
    pub auth_scheme: LLMAuthScheme,
    pub extra_headers: HashMap<String, String>,
    pub timeout_secs: u64,
}

impl LLMProviderConfig {
    pub fn groq(api_key: impl Into<String>) -> Self {
        Self {
            provider_label: "groq".to_string(),
            chat_completions_url: "https://api.groq.com/openai/v1/chat/completions".to_string(),
            api_key: api_key.into(),
            auth_scheme: LLMAuthScheme::Bearer,
            extra_headers: HashMap::new(),
            timeout_secs: 120,
        }
    }

    pub fn openai_compatible(
        api_key: impl Into<String>,
        chat_completions_url: impl Into<String>,
    ) -> Self {
        Self {
            provider_label: "openai-compatible".to_string(),
            chat_completions_url: chat_completions_url.into(),
            api_key: api_key.into(),
            auth_scheme: LLMAuthScheme::Bearer,
            extra_headers: HashMap::new(),
            timeout_secs: 120,
        }
    }

    pub fn groq_from_env(api_key_env: &str) -> Result<Self> {
        let api_key = std::env::var(api_key_env).map_err(|_| {
            RLMError::MissingConfiguration(format!(
                "environment variable {api_key_env} is required"
            ))
        })?;

        Ok(Self::groq(api_key))
    }

    pub fn openai_compatible_from_env(
        api_key_env: &str,
        chat_completions_url_env: &str,
        fallback_chat_completions_url: &str,
    ) -> Result<Self> {
        let api_key = std::env::var(api_key_env).map_err(|_| {
            RLMError::MissingConfiguration(format!(
                "environment variable {api_key_env} is required"
            ))
        })?;

        let chat_completions_url = std::env::var(chat_completions_url_env)
            .unwrap_or_else(|_| fallback_chat_completions_url.to_string());

        Ok(Self::openai_compatible(api_key, chat_completions_url))
    }

    pub fn with_provider_label(mut self, provider_label: impl Into<String>) -> Self {
        self.provider_label = provider_label.into();
        self
    }

    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs.max(1);
        self
    }

    pub fn with_header(
        mut self,
        header_name: impl Into<String>,
        header_value: impl Into<String>,
    ) -> Self {
        self.extra_headers.insert(header_name.into(), header_value.into());
        self
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Delta,
}

#[derive(Debug, Deserialize)]
struct Delta {
    content: Option<String>,
}

pub struct LLMClient {
    client: Client,
    provider: LLMProviderConfig,
    model: String,
    fast_model: Option<String>,
    temperature: f32,
    max_tokens: u32,
    cache_limit: usize,
    response_cache: Arc<Mutex<HashMap<u64, String>>>,
    cache_hits: Arc<Mutex<usize>>,
    cache_misses: Arc<Mutex<usize>>,
    fast_model_calls: Arc<Mutex<usize>>,
    smart_model_calls: Arc<Mutex<usize>>,
}

impl Clone for LLMClient {
    fn clone(&self) -> Self {
        Self::from_parts(
            self.provider.clone(),
            self.model.clone(),
            self.fast_model.clone(),
            self.temperature,
            self.max_tokens,
            self.cache_limit,
            self.response_cache.clone(),
            self.cache_hits.clone(),
            self.cache_misses.clone(),
            self.fast_model_calls.clone(),
            self.smart_model_calls.clone(),
        )
    }
}

impl LLMClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self::from_provider(LLMProviderConfig::groq(api_key), model)
    }

    pub fn from_provider(provider: LLMProviderConfig, model: impl Into<String>) -> Self {
        Self::from_parts(
            provider,
            model.into(),
            None,
            0.2,
            1024,
            500,
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(0)),
            Arc::new(Mutex::new(0)),
            Arc::new(Mutex::new(0)),
            Arc::new(Mutex::new(0)),
        )
    }

    fn from_parts(
        provider: LLMProviderConfig,
        model: String,
        fast_model: Option<String>,
        temperature: f32,
        max_tokens: u32,
        cache_limit: usize,
        response_cache: Arc<Mutex<HashMap<u64, String>>>,
        cache_hits: Arc<Mutex<usize>>,
        cache_misses: Arc<Mutex<usize>>,
        fast_model_calls: Arc<Mutex<usize>>,
        smart_model_calls: Arc<Mutex<usize>>,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(provider.timeout_secs.max(1)))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            provider,
            model,
            fast_model,
            temperature,
            max_tokens,
            cache_limit,
            response_cache,
            cache_hits,
            cache_misses,
            fast_model_calls,
            smart_model_calls,
        }
    }

    pub fn with_fast_model(mut self, fast_model: String) -> Self {
        self.fast_model = Some(fast_model);
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

    pub fn with_provider(mut self, provider: LLMProviderConfig) -> Self {
        let timeout = provider.timeout_secs.max(1);
        self.client = Client::builder()
            .timeout(Duration::from_secs(timeout))
            .build()
            .unwrap_or_else(|_| Client::new());
        self.provider = provider;
        self
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    pub fn fast_model_name(&self) -> Option<&str> {
        self.fast_model.as_deref()
    }

    pub fn provider_config(&self) -> &LLMProviderConfig {
        &self.provider
    }

    pub fn provider_label(&self) -> &str {
        &self.provider.provider_label
    }

    pub fn model_stats(&self) -> (usize, usize) {
        let fast = *self.fast_model_calls.lock().unwrap();
        let smart = *self.smart_model_calls.lock().unwrap();
        (fast, smart)
    }

    fn select_model(&self, messages: &[Message]) -> String {
        let fast_model = match &self.fast_model {
            Some(model) => model,
            None => return self.model.clone(),
        };

        if let Some(last_msg) = messages.iter().rev().find(|message| message.role == "user") {
            let content = last_msg.content.to_lowercase();

            if content.contains("fast_find")
                || content.contains("fast_contains")
                || content.contains("fast_find_all")
                || content.contains("fast_count")
                || content.contains("window(")
                || content.contains("head(")
                || content.contains("tail(")
                || content.contains("search")
                || content.contains("find")
                || content.contains("extract")
                || content.contains("locate")
            {
                return fast_model.clone();
            }

            if content.contains("final(")
                || content.contains("summarize")
                || content.contains("summary")
                || content.contains("analyze")
                || content.contains("compare")
                || content.contains("conclude")
                || content.contains("synthesize")
            {
                return self.model.clone();
            }
        }

        fast_model.clone()
    }

    fn hash_messages(&self, selected_model: &str, messages: &[Message]) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.provider.chat_completions_url.hash(&mut hasher);
        selected_model.hash(&mut hasher);
        for message in messages {
            message.role.hash(&mut hasher);
            message.content.hash(&mut hasher);
        }
        hasher.finish()
    }

    pub fn cache_stats(&self) -> (usize, usize) {
        let hits = *self.cache_hits.lock().unwrap();
        let misses = *self.cache_misses.lock().unwrap();
        (hits, misses)
    }

    pub fn clear_cache(&self) {
        self.response_cache.lock().unwrap().clear();
        *self.cache_hits.lock().unwrap() = 0;
        *self.cache_misses.lock().unwrap() = 0;
    }

    pub async fn complete(&self, messages: Vec<Message>) -> Result<String> {
        let selected_model = self.select_model(&messages);
        let cache_key = self.hash_messages(&selected_model, &messages);

        {
            let cache = self.response_cache.lock().unwrap();
            if let Some(cached_response) = cache.get(&cache_key) {
                *self.cache_hits.lock().unwrap() += 1;
                return Ok(cached_response.clone());
            }
        }

        *self.cache_misses.lock().unwrap() += 1;
        self.record_model_usage(&selected_model);

        let request = ChatRequest {
            model: selected_model,
            messages,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            stream: None,
        };

        let response = self.send_request(&request).await?;
        let parsed: ChatResponse = response.json().await?;

        let result = parsed
            .choices
            .first()
            .map(|choice| choice.message.content.clone())
            .ok_or_else(|| RLMError::LLMError("No response choices returned by provider".to_string()))?;

        self.store_cached_response(cache_key, result.clone());
        Ok(result)
    }

    pub async fn stream(&self, messages: Vec<Message>) -> Result<mpsc::Receiver<String>> {
        let (tx, rx) = mpsc::channel(100);

        let selected_model = self.select_model(&messages);
        let cache_key = self.hash_messages(&selected_model, &messages);
        {
            let cache = self.response_cache.lock().unwrap();
            if let Some(cached_response) = cache.get(&cache_key) {
                *self.cache_hits.lock().unwrap() += 1;
                let cached_response = cached_response.clone();
                tokio::spawn(async move {
                    let _ = tx.send(cached_response).await;
                });
                return Ok(rx);
            }
        }

        *self.cache_misses.lock().unwrap() += 1;
        self.record_model_usage(&selected_model);

        let request = ChatRequest {
            model: selected_model,
            messages,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            stream: Some(true),
        };

        let response = self.send_request(&request).await?;
        let response_cache = self.response_cache.clone();
        let cache_limit = self.cache_limit;

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut pending = String::new();
            let mut collected = String::new();

            while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(bytes) => bytes,
                    Err(_) => break,
                };

                pending.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(newline_index) = pending.find('\n') {
                    let line = pending[..newline_index].trim_end_matches('\r').to_string();
                    pending = pending[newline_index + 1..].to_string();

                    if let Some(token) = parse_stream_line(&line) {
                        collected.push_str(&token);
                        if tx.send(token).await.is_err() {
                            return;
                        }
                    } else if line.trim() == "data: [DONE]" {
                        break;
                    }
                }
            }

            if !collected.is_empty() {
                let mut cache = response_cache.lock().unwrap();
                if cache.len() < cache_limit {
                    cache.insert(cache_key, collected);
                }
            }
        });

        Ok(rx)
    }

    async fn send_request(&self, request: &ChatRequest) -> Result<reqwest::Response> {
        if self.provider.chat_completions_url.trim().is_empty() {
            return Err(RLMError::ConfigurationError(
                "provider chat_completions_url cannot be empty".to_string(),
            ));
        }

        let mut builder = self
            .client
            .post(&self.provider.chat_completions_url)
            .header("Content-Type", "application/json");

        builder = match self.provider.auth_scheme {
            LLMAuthScheme::Bearer => {
                builder.header("Authorization", format!("Bearer {}", self.provider.api_key))
            }
            LLMAuthScheme::XApiKey => builder.header("x-api-key", &self.provider.api_key),
            LLMAuthScheme::None => builder,
        };

        for (header_name, header_value) in &self.provider.extra_headers {
            builder = builder.header(header_name, header_value);
        }

        let response = builder.json(request).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(RLMError::LLMError(format!(
                "Provider {} returned {}: {}",
                self.provider.provider_label, status, error_text
            )));
        }

        Ok(response)
    }

    fn record_model_usage(&self, selected_model: &str) {
        if self.fast_model.as_deref() == Some(selected_model) {
            *self.fast_model_calls.lock().unwrap() += 1;
        } else {
            *self.smart_model_calls.lock().unwrap() += 1;
        }
    }

    fn store_cached_response(&self, cache_key: u64, result: String) {
        let mut cache = self.response_cache.lock().unwrap();
        if cache.len() < self.cache_limit {
            cache.insert(cache_key, result);
        }
    }
}

fn parse_stream_line(line: &str) -> Option<String> {
    if !line.starts_with("data: ") || line.trim() == "data: [DONE]" {
        return None;
    }

    let payload = &line[6..];
    serde_json::from_str::<StreamChunk>(payload)
        .ok()
        .and_then(|chunk| chunk.choices.into_iter().next())
        .and_then(|choice| choice.delta.content)
}
