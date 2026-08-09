//! Bounded long-context RLM scratchpad tool.

use std::path::PathBuf;
use std::time::Duration;

use crate::types::output::{DynamicOutput, ToolOutput};
use crate::types::tool::{ToolKind, ToolNamespace};
use crate::types::tool_io::ToolInput;
use xai_tool_runtime::ToolCallContext;

const MAX_DOCUMENT_CHARS: usize = 4_000_000;
const MAX_ITERATIONS: usize = 32;
const MAX_DEPTH: usize = 4;
const DEFAULT_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RlmTask {
    #[default]
    Question,
    Summarize,
    AgentContext,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RlmProfile {
    LowMemory,
    #[default]
    Balanced,
    HighThroughput,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RlmToolInput {
    #[schemars(description = "Question, summary instruction, or agent-context goal.")]
    pub query: String,
    #[serde(default)]
    #[schemars(description = "Inline source text. Provide exactly one of document and file_path.")]
    pub document: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Path to a source document, relative to the current workspace when relative."
    )]
    pub file_path: Option<String>,
    #[serde(default)]
    pub task: RlmTask,
    #[serde(default)]
    #[schemars(range(min = 1, max = 32))]
    pub max_iterations: Option<usize>,
    #[serde(default)]
    #[schemars(range(min = 1, max = 4))]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub profile: RlmProfile,
    #[serde(default)]
    pub hint_keywords: Vec<String>,
}

impl From<RlmToolInput> for ToolInput {
    fn from(value: RlmToolInput) -> Self {
        Self::Dynamic(serde_json::to_value(value).expect("RlmToolInput serializes"))
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct RlmToolOutput {
    pub answer: String,
    pub document_id: String,
    pub source_path: Option<String>,
    pub provider: String,
    pub primary_model: String,
    pub fast_model: Option<String>,
    pub was_reduced: bool,
    pub final_context_chars: usize,
    pub evidence_excerpt: Option<String>,
    #[schemars(skip)]
    pub stats: serde_json::Value,
}

impl xai_tool_runtime::ToolOutput for RlmToolOutput {}

impl From<RlmToolOutput> for ToolOutput {
    fn from(value: RlmToolOutput) -> Self {
        Self::Dynamic(DynamicOutput {
            value: serde_json::to_value(value).expect("RlmToolOutput serializes"),
        })
    }
}

#[derive(Debug, Default)]
pub struct RlmTool;

impl crate::types::tool_metadata::ToolMetadata for RlmTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Other
    }
    fn tool_namespace(&self) -> ToolNamespace {
        ToolNamespace::GrokBuild
    }
    fn description_template(&self) -> &str {
        "Analyze oversized text with the bounded Recursive Language Model scratchpad. Use exactly one of `document` or `file_path`; it recursively reduces large context, searches it with a sandboxed Rhai loop, and returns an answer plus evidence and reduction statistics. It is read-only. Configure `RLM_API_KEY` (or `XAI_API_KEY`/`GROQ_API_KEY`), `RLM_MODEL`, and optionally `RLM_CHAT_COMPLETIONS_URL`. Use `task=agent_context` when preparing context for another agent."
    }
    fn is_read_only(&self) -> bool {
        true
    }
}

impl xai_tool_runtime::Tool for RlmTool {
    type Args = RlmToolInput;
    type Output = RlmToolOutput;

    fn id(&self) -> xai_tool_protocol::ToolId {
        xai_tool_protocol::ToolId::new("rlm").expect("valid tool id")
    }
    fn description(
        &self,
        _ctx: &xai_tool_runtime::ListToolsContext,
    ) -> xai_tool_types::ToolDescription {
        xai_tool_types::ToolDescription::new(
            "rlm",
            crate::types::tool_metadata::ToolMetadata::sanitized_description_template(self),
        )
    }
    fn capabilities(&self) -> xai_tool_protocol::ToolCapabilities {
        xai_tool_protocol::ToolCapabilities {
            is_read_only: true,
            tool_scope: Some(xai_tool_protocol::ToolScope::Read),
            ..Default::default()
        }
    }

    async fn run(
        &self,
        ctx: ToolCallContext,
        input: RlmToolInput,
    ) -> Result<RlmToolOutput, xai_tool_runtime::ToolError> {
        validate_input(&input)?;
        let (document, source_path) = load_document(&ctx, &input).await?;
        let model = std::env::var("RLM_MODEL")
            .map_err(|_| tool_error("configuration", "RLM_MODEL is required for the rlm tool"))?;
        let api_key =
            first_env(&["RLM_API_KEY", "XAI_API_KEY", "GROQ_API_KEY"]).ok_or_else(|| {
                tool_error(
                    "configuration",
                    "RLM_API_KEY, XAI_API_KEY, or GROQ_API_KEY is required for the rlm tool",
                )
            })?;
        let url = std::env::var("RLM_CHAT_COMPLETIONS_URL")
            .unwrap_or_else(|_| "https://api.x.ai/v1/chat/completions".to_owned());
        let provider = xai_rlm::LLMProviderConfig::openai_compatible(api_key, url);
        let rlm = xai_rlm::RLM::from_provider(provider, model)
            .with_max_iterations(input.max_iterations.unwrap_or(24).min(MAX_ITERATIONS))
            .with_max_depth(input.max_depth.unwrap_or(2).min(MAX_DEPTH))
            .with_profile(input.profile.clone().into());
        let doc_id = source_path
            .as_deref()
            .unwrap_or("inline-document")
            .to_owned();
        let mut doc = xai_rlm::RLMDocument::from_text(doc_id, document);
        if let Some(path) = source_path.clone() {
            doc = doc.with_source_path(path);
        }
        let request = match input.task {
            RlmTask::Question => xai_rlm::RLMRequest::question(input.query, doc),
            RlmTask::Summarize => xai_rlm::RLMRequest::summary(doc),
            RlmTask::AgentContext => xai_rlm::RLMRequest::agent_context(input.query, doc),
        }
        .with_hint_keywords(input.hint_keywords);
        let cancel = ctx.extensions.get::<xai_tool_runtime::Cancellation>();
        let run = async {
            rlm.complete_request_recursive(request, rlm.recommended_chunking_config())
                .await
                .map_err(|error| tool_error("execution", error.to_string()))
        };
        let response = tokio::time::timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS), async {
            if let Some(cancel) = cancel { tokio::select! { result = run => result, _ = cancel.0.cancelled() => Err(tool_error("cancelled", "rlm analysis was cancelled")) } }
            else { run.await }
        }).await.map_err(|_| tool_error("timeout", "rlm analysis exceeded the 300 second tool budget"))??;
        Ok(RlmToolOutput {
            answer: response.response.answer,
            document_id: response.response.document_id,
            source_path: response.response.source_path,
            provider: response.response.provider,
            primary_model: response.response.primary_model,
            fast_model: response.response.fast_model,
            was_reduced: response.was_reduced,
            final_context_chars: response.final_context_chars,
            evidence_excerpt: response.response.evidence_excerpt,
            stats: serde_json::to_value(response.aggregate_stats).unwrap_or_default(),
        })
    }
}

impl From<RlmProfile> for xai_rlm::RLMProfile {
    fn from(profile: RlmProfile) -> Self {
        match profile {
            RlmProfile::LowMemory => Self::LowMemory,
            RlmProfile::Balanced => Self::Balanced,
            RlmProfile::HighThroughput => Self::HighThroughput,
        }
    }
}

fn validate_input(input: &RlmToolInput) -> Result<(), xai_tool_runtime::ToolError> {
    if input.query.trim().is_empty() && !matches!(input.task, RlmTask::Summarize) {
        return Err(tool_error("invalid_arguments", "query must not be empty"));
    }
    let has_document = input.document.is_some();
    let has_file = input
        .file_path
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty());
    if has_document == has_file {
        return Err(tool_error(
            "invalid_arguments",
            "provide exactly one of document or file_path",
        ));
    }
    if input
        .max_iterations
        .is_some_and(|value| value == 0 || value > MAX_ITERATIONS)
        || input
            .max_depth
            .is_some_and(|value| value == 0 || value > MAX_DEPTH)
    {
        return Err(tool_error(
            "invalid_arguments",
            "max_iterations must be 1..=32 and max_depth must be 1..=4",
        ));
    }
    if input
        .document
        .as_deref()
        .is_some_and(|text| text.chars().count() > MAX_DOCUMENT_CHARS)
    {
        return Err(tool_error(
            "invalid_arguments",
            "inline document exceeds the 4,000,000 character limit",
        ));
    }
    Ok(())
}

async fn load_document(
    ctx: &ToolCallContext,
    input: &RlmToolInput,
) -> Result<(String, Option<String>), xai_tool_runtime::ToolError> {
    if let Some(document) = input.document.clone() {
        return Ok((document, None));
    }
    let raw_path = input.file_path.as_deref().unwrap_or_default();
    let resources = crate::types::tool_metadata::shared_resources(ctx)?;
    let cwd = crate::types::tool_metadata::resolve_cwd(ctx, &resources).await?;
    let path = {
        let path = PathBuf::from(raw_path);
        if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
    };
    let canonical = dunce::canonicalize(&path)
        .map_err(|error| tool_error("read", format!("cannot resolve file_path: {error}")))?;
    let canonical_cwd = dunce::canonicalize(&cwd).unwrap_or(cwd);
    if !canonical.starts_with(&canonical_cwd) {
        return Err(tool_error(
            "permission",
            "file_path must remain inside the current workspace",
        ));
    }
    let content = tokio::fs::read_to_string(&canonical)
        .await
        .map_err(|error| tool_error("read", format!("cannot read file_path: {error}")))?;
    if content.chars().count() > MAX_DOCUMENT_CHARS {
        return Err(tool_error(
            "invalid_arguments",
            "file exceeds the 4,000,000 character limit",
        ));
    }
    Ok((content, Some(canonical.to_string_lossy().into_owned())))
}

fn first_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    })
}
fn tool_error(kind: &str, detail: impl Into<String>) -> xai_tool_runtime::ToolError {
    xai_tool_runtime::ToolError::custom(kind, detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> RlmToolInput {
        RlmToolInput {
            query: "q".into(),
            document: Some("text".into()),
            file_path: None,
            task: RlmTask::Question,
            max_iterations: None,
            max_depth: None,
            profile: RlmProfile::Balanced,
            hint_keywords: vec![],
        }
    }
    #[test]
    fn requires_one_document_source() {
        let mut value = input();
        value.document = None;
        assert!(validate_input(&value).is_err());
    }
    #[test]
    fn rejects_unbounded_limits() {
        let mut value = input();
        value.max_iterations = Some(MAX_ITERATIONS + 1);
        assert!(validate_input(&value).is_err());
    }
}
