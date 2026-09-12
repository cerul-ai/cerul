//! Opt-in request accounting. Never retain request bodies or free-form responses.
use crate::config::Endpoint;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::time::Instant;

tokio::task_local! {
    static OBSERVER: Option<RequestObserver>;
}

/// Receives one report per attempted HTTP request, including retries and probes.
pub type RequestObserver = Arc<dyn Fn(RequestReport) + Send + Sync>;

/// Providers created in this scope retain the observer across worker tasks.
pub async fn with_request_observer<T>(
    observer: Option<RequestObserver>,
    future: impl std::future::Future<Output = T>,
) -> T {
    OBSERVER.scope(observer, future).await
}

pub(super) fn current_observer() -> Option<RequestObserver> {
    OBSERVER.try_with(Clone::clone).ok().flatten()
}

pub(super) fn next_request_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RequestReport {
    /// Process-local logical request ID, shared by that request's retries.
    pub request_id: u64,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub action: String,
    /// One-based attempt number. Cache hits do not produce reports.
    pub attempt: u32,
    /// Network/response time, excluding rate-limit waits and retry backoff.
    pub elapsed_ms: u64,
    /// None when no HTTP response was received, including cancellation.
    pub http_status: Option<u16>,
    /// Unknown is not zero: a failed or unsupported response may still be billed.
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub thinking_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub tool_input_tokens: Option<u64>,
    /// Provider-reported total; never calculated by adding potentially overlapping fields.
    pub total_tokens: Option<u64>,
    pub input_tokens_by_modality: Option<Vec<ModalityTokens>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModalityTokens {
    pub modality: Modality,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Video,
    Audio,
    Document,
}

fn modality_tokens(value: &Value) -> Option<Vec<ModalityTokens>> {
    let entries = value.as_array()?;
    if entries.len() > 32 {
        return None;
    }
    entries
        .iter()
        .map(|entry| {
            let modality = match entry["modality"].as_str()? {
                "TEXT" => Modality::Text,
                "IMAGE" => Modality::Image,
                "VIDEO" => Modality::Video,
                "AUDIO" => Modality::Audio,
                "DOCUMENT" => Modality::Document,
                _ => return None,
            };
            Some(ModalityTokens {
                modality,
                tokens: entry["tokenCount"].as_u64()?,
            })
        })
        .collect()
}

fn reported_usage(provider: &str, action: &str, response: &Value) -> Option<TokenUsage> {
    if provider != "gemini" {
        return None;
    }
    let usage = response.get("usageMetadata")?;
    usage.as_object()?;
    let tokens = TokenUsage {
        input_tokens: usage["promptTokenCount"].as_u64(),
        output_tokens: usage["candidatesTokenCount"].as_u64(),
        thinking_tokens: usage["thoughtsTokenCount"].as_u64(),
        cached_input_tokens: usage["cachedContentTokenCount"].as_u64(),
        tool_input_tokens: usage["toolUsePromptTokenCount"].as_u64(),
        total_tokens: usage["totalTokenCount"].as_u64(),
        // Gemini's embedding and generation APIs use different field names.
        input_tokens_by_modality: modality_tokens(
            &usage[if action == "embedContent" {
                "promptTokenDetails"
            } else {
                "promptTokensDetails"
            }],
        ),
    };
    (tokens.input_tokens.is_some()
        || tokens.output_tokens.is_some()
        || tokens.thinking_tokens.is_some()
        || tokens.cached_input_tokens.is_some()
        || tokens.tool_input_tokens.is_some()
        || tokens.total_tokens.is_some()
        || tokens.input_tokens_by_modality.is_some())
    .then_some(tokens)
}

/// Drop also records interrupted requests; no report is created while queued.
pub(super) struct Attempt {
    observer: Option<RequestObserver>,
    started: Instant,
    report: RequestReport,
}
impl Attempt {
    pub(super) fn new(
        observer: Option<RequestObserver>,
        endpoint: &Endpoint,
        action: &str,
        request_id: u64,
        attempt: u32,
    ) -> Self {
        Self {
            observer,
            started: Instant::now(),
            report: RequestReport {
                request_id,
                provider: endpoint.kind.clone(),
                base_url: endpoint.base_url.clone(),
                model: endpoint.model.clone(),
                action: action.into(),
                attempt,
                elapsed_ms: 0,
                http_status: None,
                usage: None,
            },
        }
    }
    pub(super) fn status(&mut self, status: u16) {
        self.report.http_status = Some(status);
    }
    pub(super) fn response(&mut self, value: &Value) {
        self.report.usage = reported_usage(&self.report.provider, &self.report.action, value);
    }
}
impl Drop for Attempt {
    fn drop(&mut self) {
        if let Some(observer) = &self.observer {
            self.report.elapsed_ms = self.started.elapsed().as_millis().min(u64::MAX.into()) as u64;
            observer(self.report.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generation_and_embedding_usage_keep_reported_counts_and_modality_names() {
        let generation = reported_usage(
            "gemini",
            "generateContent",
            &json!({
                "usageMetadata": {"promptTokenCount":100,"candidatesTokenCount":20,
                    "thoughtsTokenCount":30,"cachedContentTokenCount":40,"totalTokenCount":150,
                    "promptTokensDetails":[{"modality":"VIDEO","tokenCount":100}],
                    "secret":"never copy arbitrary metadata"}
            }),
        )
        .unwrap();
        assert_eq!(generation.total_tokens, Some(150));
        assert_eq!(generation.output_tokens, Some(20));
        assert_eq!(generation.thinking_tokens, Some(30));
        assert_eq!(generation.cached_input_tokens, Some(40));
        assert!(
            !serde_json::to_string(&generation)
                .unwrap()
                .contains("secret")
        );
        let embedding = reported_usage(
            "gemini",
            "embedContent",
            &json!({
                "usageMetadata":{"promptTokenCount":1980,"promptTokenDetails":[
                    {"modality":"VIDEO","tokenCount":1980}]}
            }),
        )
        .unwrap();
        assert_eq!(embedding.input_tokens, Some(1980));
        assert!(embedding.total_tokens.is_none());
        assert!(embedding.output_tokens.is_none());
        assert_eq!(embedding.input_tokens_by_modality.unwrap()[0].tokens, 1980);
        assert!(matches!(
            generation.input_tokens_by_modality.unwrap()[0].modality,
            Modality::Video
        ));
    }

    #[test]
    fn missing_and_malformed_usage_is_unknown_while_explicit_zero_is_preserved() {
        for usage in [
            Value::Null,
            json!({}),
            json!({"promptTokenCount":-1}),
            json!({"promptTokenCount":"secret"}),
            json!({"promptTokenCount":1.5}),
        ] {
            assert!(
                reported_usage("gemini", "embedContent", &json!({"usageMetadata":usage})).is_none()
            );
        }
        let response = json!({"usageMetadata":{"promptTokenCount":0,"promptTokenDetails":[{"modality":"secret","tokenCount":5}]}});
        let tokens = reported_usage("gemini", "embedContent", &response).unwrap();
        assert_eq!(tokens.input_tokens, Some(0));
        assert!(tokens.input_tokens_by_modality.is_none());
        assert!(reported_usage("openai", "embeddings", &response).is_none());
    }
}
