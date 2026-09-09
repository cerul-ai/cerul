//! Endpoint adapters: explicit capabilities, bounded requests, retries and cancellation.
use crate::config::{Endpoint, QUERY_TEMPLATE};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use reqwest::{
    Client,
    header::{CONTENT_TYPE, HeaderValue},
};
use serde_json::{Value, json};
use std::{fmt, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, Semaphore},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

pub mod probes;

tokio::task_local! {
    static CREDENTIALS: std::collections::BTreeMap<String, String>;
    static CREDENTIAL_RESOLVER: CredentialResolver;
}
/// Scope host-supplied credentials to one operation; environment values take precedence.
/// The library does not read credential files or prompt the user.
pub async fn with_credentials<T>(
    credentials: std::collections::BTreeMap<String, String>,
    future: impl std::future::Future<Output = T>,
) -> T {
    CREDENTIALS.scope(credentials, future).await
}
/// Optional host-owned credential acquisition, called only before remote work.
pub type CredentialResolver = Arc<
    dyn Fn(
            Endpoint,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send>>
        + Send
        + Sync,
>;
pub async fn with_credential_resolver<T>(
    resolver: CredentialResolver,
    future: impl std::future::Future<Output = T>,
) -> T {
    CREDENTIAL_RESOLVER.scope(resolver, future).await
}
/// Bind stored credentials to the exact endpoint origin/path and environment name.
pub fn credential_scope(endpoint: &Endpoint) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                &endpoint.kind,
                endpoint.base_url.trim_end_matches('/'),
                &endpoint.api_key_env
            ))
            .expect("string tuple serializes")
        )
    )
}

pub const MAX_REQUEST_BYTES: usize = 20_000_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Cancelled,
    MissingKey,
    Unsupported,
    Unavailable,
    TooLarge,
    InvalidResponse,
    Rejected,
}
#[derive(Debug)]
pub struct ProviderError {
    pub kind: Failure,
    pub message: String,
}
impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ProviderError {}
fn failure(kind: Failure, message: impl Into<String>) -> anyhow::Error {
    ProviderError {
        kind,
        message: message.into(),
    }
    .into()
}

#[derive(Clone)]
pub struct Provider {
    pub endpoint: Endpoint,
    key: Option<HeaderValue>,
    acquired_key: Arc<tokio::sync::OnceCell<Option<HeaderValue>>>,
    client: Client,
    permits: Arc<Semaphore>,
    jobs: usize,
    next: Arc<Mutex<Instant>>,
    interval: Duration,
    pub cancel: CancellationToken,
    pub request_notice: Option<RequestNotice>,
}
/// Optional host notification immediately before an endpoint request is sent.
pub type RequestNotice = Arc<dyn Fn(&Endpoint) + Send + Sync>;
#[derive(Debug, Clone)]
pub enum Input {
    Text(String),
    Image(Vec<u8>, String),
    Video(Vec<u8>, String),
    Audio(Vec<u8>, String),
}
impl Input {
    fn gemini(&self) -> Value {
        match self {
            Self::Text(text) => json!({"text":text}),
            Self::Image(bytes, mime) | Self::Video(bytes, mime) | Self::Audio(bytes, mime) => {
                json!({"inlineData":{"mimeType":mime,"data":STANDARD.encode(bytes)}})
            }
        }
    }
    fn openai(&self) -> Value {
        match self {
            Self::Text(text) => json!({"type":"text","text":text}),
            Self::Image(bytes, mime) => {
                json!({"type":"image_url","image_url":{"url":format!("data:{mime};base64,{}",STANDARD.encode(bytes))}})
            }
            Self::Video(bytes, mime) => {
                json!({"type":"video_url","video_url":{"url":format!("data:{mime};base64,{}",STANDARD.encode(bytes))}})
            }
            Self::Audio(bytes, _) => {
                json!({"type":"input_audio","input_audio":{"data":STANDARD.encode(bytes),"format":"wav"}})
            }
        }
    }
}

impl Provider {
    pub fn new(
        endpoint: Endpoint,
        key: Option<String>,
        jobs: usize,
        rpm: Option<u32>,
        cancel: CancellationToken,
    ) -> Result<Self> {
        ensure!(jobs > 0 && rpm != Some(0), "jobs and RPM must be positive");
        let url = url::Url::parse(&endpoint.base_url)?;
        ensure!(
            matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
            "invalid provider URL"
        );
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "provider URL must not contain credentials or query"
        );
        ensure!(
            matches!(endpoint.kind.as_str(), "gemini" | "openai"),
            "unsupported provider protocol"
        );
        ensure!(
            !endpoint.model.is_empty() && !endpoint.model.chars().any(char::is_control),
            "invalid model name"
        );
        let key = key
            .filter(|key| !key.is_empty())
            .map(|value| {
                let mut header = HeaderValue::from_str(&value)
                    .map_err(|_| failure(Failure::MissingKey, "invalid API key header"))?;
                header.set_sensitive(true);
                Ok::<_, anyhow::Error>(header)
            })
            .transpose()?;
        let builder = if matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) {
            Client::builder().no_proxy()
        } else {
            Client::builder()
        };
        let client = builder
            .timeout(Duration::from_secs(180))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("cerul/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            endpoint,
            key,
            acquired_key: Arc::new(tokio::sync::OnceCell::new()),
            client,
            permits: Arc::new(Semaphore::new(jobs)),
            jobs,
            next: Arc::new(Mutex::new(Instant::now())),
            interval: rpm
                .map(|n| Duration::from_secs_f64(60. / f64::from(n)))
                .unwrap_or_default(),
            cancel,
            request_notice: None,
        })
    }
    fn key_header(&self) -> Option<&HeaderValue> {
        self.key
            .as_ref()
            .or_else(|| self.acquired_key.get().and_then(Option::as_ref))
    }
    async fn resolve_key(&self) -> Result<()> {
        if self.key.is_some() {
            return Ok(());
        }
        self.acquired_key
            .get_or_try_init(|| async {
                let Some(resolver) = CREDENTIAL_RESOLVER.try_with(Clone::clone).ok() else {
                    return Ok(None);
                };
                let value = resolver(self.endpoint.clone()).await?;
                value
                    .filter(|key| !key.is_empty())
                    .map(|value| {
                        let mut header = HeaderValue::from_str(&value)
                            .map_err(|_| failure(Failure::MissingKey, "invalid API key header"))?;
                        header.set_sensitive(true);
                        Ok::<_, anyhow::Error>(header)
                    })
                    .transpose()
            })
            .await?;
        Ok(())
    }
    pub fn concurrency(&self) -> usize {
        self.jobs
    }
    pub fn from_env(
        endpoint: Endpoint,
        jobs: usize,
        rpm: Option<u32>,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let key = std::env::var(&endpoint.api_key_env).ok().or_else(|| {
            CREDENTIALS
                .try_with(|keys| keys.get(&credential_scope(&endpoint)).cloned())
                .ok()
                .flatten()
        });
        Self::new(endpoint, key, jobs, rpm, cancel)
    }
    fn route(&self, action: &str) -> Result<url::Url> {
        let mut url = url::Url::parse(&self.endpoint.base_url)?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("invalid provider base URL"))?;
            segments.pop_if_empty();
            if self.endpoint.kind == "gemini" {
                segments.push("models").push(&format!(
                    "{}:{action}",
                    self.endpoint
                        .model
                        .strip_prefix("models/")
                        .unwrap_or(&self.endpoint.model)
                ));
            } else {
                for segment in action.split('/') {
                    segments.push(segment);
                }
            }
        }
        Ok(url)
    }
    async fn delay(&self, duration: Duration) -> Result<()> {
        tokio::select! {biased;_ = self.cancel.cancelled()=>Err(failure(Failure::Cancelled,"operation cancelled")),_ = tokio::time::sleep(duration)=>Ok(())}
    }
    async fn rate_limit(&self) -> Result<()> {
        let when = {
            let mut next = self.next.lock().await;
            let when = (*next).max(Instant::now());
            *next = when + self.interval;
            when
        };
        self.delay(when.saturating_duration_since(Instant::now()))
            .await
    }
    async fn post(&self, action: &str, content_type: &str, bytes: Vec<u8>) -> Result<Value> {
        self.request(action, content_type, bytes, false).await
    }
    async fn retry_wait(&self, mut response: reqwest::Response, attempt: u32) -> Result<Duration> {
        if let Some(seconds) = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
        {
            return Ok(Duration::from_secs(seconds.min(120)));
        }
        let rate_limited = response.status().as_u16() == 429;
        if rate_limited && self.endpoint.kind == "gemini" {
            // Google commonly supplies RetryInfo in JSON instead of Retry-After.
            // Bound the error body and never expose it: it may echo input or keys.
            let mut bytes = Vec::new();
            loop {
                let chunk = tokio::select! {biased;
                    _ = self.cancel.cancelled() => return Err(failure(Failure::Cancelled,"operation cancelled")),
                    chunk = response.chunk() => chunk,
                };
                let Ok(Some(chunk)) = chunk else { break };
                if bytes.len() + chunk.len() > 16_384 {
                    break;
                }
                bytes.extend_from_slice(&chunk);
            }
            if let Ok(body) = serde_json::from_slice::<Value>(&bytes)
                && let Some(details) = body["error"]["details"].as_array()
            {
                for detail in details {
                    if detail["@type"] != "type.googleapis.com/google.rpc.RetryInfo" {
                        continue;
                    }
                    if let Some(seconds) = detail["retryDelay"]
                        .as_str()
                        .and_then(|v| v.strip_suffix('s'))
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite() && *v >= 0.)
                    {
                        return Ok(Duration::from_secs_f64(seconds.min(120.)));
                    }
                }
            }
        }
        Ok(Duration::from_secs(if rate_limited {
            30 << attempt
        } else {
            1 << attempt
        }))
    }
    /// GET the configured Cerul endpoint's advertised capabilities.
    pub async fn capabilities(&self) -> Result<Value> {
        self.request("capabilities", "application/json", Vec::new(), true)
            .await
    }
    async fn request(
        &self,
        action: &str,
        content_type: &str,
        bytes: Vec<u8>,
        get: bool,
    ) -> Result<Value> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(failure(
                Failure::TooLarge,
                "encoded model request exceeds the inline byte limit",
            ));
        }
        let url = self.route(action)?;
        let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !loopback && !get {
            self.resolve_key().await?;
        }
        if self.key_header().is_none() && !loopback && !get {
            return Err(failure(
                Failure::MissingKey,
                format!("set {} for this endpoint", self.endpoint.api_key_env),
            ));
        }
        let _permit = tokio::select! {biased;_ = self.cancel.cancelled()=>return Err(failure(Failure::Cancelled,"operation cancelled")),permit=self.permits.acquire()=>permit?};
        if let Some(notice) = &self.request_notice {
            notice(&self.endpoint);
        }
        for attempt in 0..3u32 {
            self.rate_limit().await?;
            let mut request = self
                .client
                .request(
                    if get {
                        reqwest::Method::GET
                    } else {
                        reqwest::Method::POST
                    },
                    url.clone(),
                )
                .header(CONTENT_TYPE, content_type)
                .body(bytes.clone());
            if let Some(key) = self.key_header() {
                request = if self.endpoint.kind == "gemini" {
                    request.header("x-goog-api-key", key.clone())
                } else {
                    let key = key
                        .to_str()
                        .map_err(|_| failure(Failure::MissingKey, "invalid API key"))?;
                    request.bearer_auth(key)
                };
            }
            let response = tokio::select! {biased;_ = self.cancel.cancelled()=>return Err(failure(Failure::Cancelled,"operation cancelled")),response=request.send()=>response};
            let response = match response {
                Ok(response) => response,
                Err(_) => {
                    if attempt < 2 {
                        self.delay(Duration::from_secs(1 << attempt)).await?;
                        continue;
                    }
                    return Err(failure(
                        Failure::Unavailable,
                        "model endpoint could not be reached",
                    ));
                }
            };
            let status = response.status();
            if status.is_success() {
                if response
                    .content_length()
                    .is_some_and(|n| n > MAX_REQUEST_BYTES as u64)
                {
                    return Err(failure(
                        Failure::InvalidResponse,
                        "model response exceeds the size limit",
                    ));
                }
                let mut response = response;
                let mut data = Vec::new();
                loop {
                    let chunk = tokio::select! {biased;_ = self.cancel.cancelled()=>return Err(failure(Failure::Cancelled,"operation cancelled")),chunk=response.chunk()=>chunk.map_err(|_|failure(Failure::Unavailable,"model response interrupted"))?};
                    let Some(chunk) = chunk else {
                        break;
                    };
                    if data.len() + chunk.len() > MAX_REQUEST_BYTES {
                        return Err(failure(
                            Failure::InvalidResponse,
                            "model response exceeds the size limit",
                        ));
                    }
                    data.extend_from_slice(&chunk);
                }
                return serde_json::from_slice(&data)
                    .map_err(|_| failure(Failure::InvalidResponse, "model returned invalid JSON"));
            }
            if (status.as_u16() == 429 || status.is_server_error()) && attempt < 2 {
                let wait = self.retry_wait(response, attempt).await?;
                self.delay(wait).await?;
                continue;
            }
            let kind = match status.as_u16() {
                400 | 404 | 405 | 415 | 422 => Failure::Unsupported,
                413 => Failure::TooLarge,
                429 | 500..=599 => Failure::Unavailable,
                _ => Failure::Rejected,
            };
            // Provider bodies may echo credentials or media. Expose only the HTTP status.
            return Err(failure(
                kind,
                format!("model endpoint returned HTTP {}", status.as_u16()),
            ));
        }
        unreachable!()
    }
    async fn json(&self, action: &str, body: Value) -> Result<Value> {
        self.post(action, "application/json", serde_json::to_vec(&body)?)
            .await
    }
    pub async fn embed(&self, input: Input, query: bool) -> Result<Vec<f32>> {
        let dims = self
            .endpoint
            .dims
            .context("embedding dimensions are not configured")?;
        let input = match input {
            Input::Text(text) if query => Input::Text(QUERY_TEMPLATE.replace("{query}", &text)),
            other => other,
        };
        let response = if self.endpoint.kind == "gemini" {
            self.json("embedContent",json!({"content":{"parts":[input.gemini()]},"embedContentConfig":{"outputDimensionality":dims}})).await?
        } else {
            let content = match &input {
                Input::Text(text) => Value::String(text.clone()),
                _ => json!([input.openai()]),
            };
            self.json("embeddings",json!({"model":self.endpoint.model,"input":content,"dimensions":dims,"encoding_format":"float"})).await?
        };
        let values = if self.endpoint.kind == "gemini" {
            &response["embedding"]["values"]
        } else {
            &response["data"][0]["embedding"]
        };
        let vector: Vec<f32> = serde_json::from_value(values.clone()).map_err(|_| {
            failure(
                Failure::InvalidResponse,
                "model returned no valid embedding",
            )
        })?;
        if vector.len() != dims
            || vector.iter().any(|n| !n.is_finite())
            || !vector.iter().any(|n| *n != 0.)
        {
            return Err(failure(
                Failure::InvalidResponse,
                "model embedding has wrong dimensions or invalid values",
            ));
        }
        Ok(vector)
    }
    pub async fn generate(&self, prompt: &str, inputs: &[Input], schema: Value) -> Result<Value> {
        let response = if self.endpoint.kind == "gemini" {
            let mut parts = vec![json!({"text":prompt})];
            parts.extend(inputs.iter().map(Input::gemini));
            self.json("generateContent",json!({"contents":[{"role":"user","parts":parts}],"generationConfig":{"responseMimeType":"application/json","responseJsonSchema":schema}})).await?
        } else {
            let mut content = vec![json!({"type":"text","text":prompt})];
            content.extend(inputs.iter().map(Input::openai));
            self.json("chat/completions",json!({"model":self.endpoint.model,"messages":[{"role":"user","content":content}],"response_format":{"type":"json_schema","json_schema":{"name":"cerul_output","strict":true,"schema":schema}}})).await?
        };
        let text = if self.endpoint.kind == "gemini" {
            response["candidates"][0]["content"]["parts"]
                .as_array()
                .map(|parts| {
                    parts
                        .iter()
                        .filter(|p| p["thought"] != true)
                        .filter_map(|p| p["text"].as_str())
                        .collect::<String>()
                })
                .unwrap_or_default()
        } else {
            response["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .into()
        };
        serde_json::from_str(&text).map_err(|_| {
            failure(
                Failure::InvalidResponse,
                "model returned no structured output",
            )
        })
    }
    pub async fn transcribe(&self, audio: Vec<u8>, duration_us: i64) -> Result<Value> {
        ensure!(duration_us > 0, "invalid transcription duration");
        let response = if self.endpoint.kind == "gemini" {
            let prompt = format!(
                "Transcribe this {}-second audio clip accurately. Return chronological speech segments with start and end measured in seconds from the beginning of THIS clip. Include silence in the timeline; do not reset time at speech or pauses. Every segment must have end greater than start. If there is no speech, return an empty segments array, not a placeholder segment. Keep spoken text in its original language; lang is an ISO language code. Do not invent speech in silence.",
                crate::media::seconds(duration_us)
            );
            let timestamp =
                json!({"type":"number","minimum":0,"maximum":duration_us as f64 / 1_000_000.});
            self.generate(&prompt,&[Input::Audio(audio,"audio/wav".into())],json!({"type":"object","properties":{"segments":{"type":"array","items":{"type":"object","properties":{"start":timestamp,"end":timestamp,"text":{"type":"string"},"lang":{"type":"string"}},"required":["start","end","text","lang"],"additionalProperties":false}}},"required":["segments"],"additionalProperties":false})).await?
        } else {
            let boundary = format!("cerul-{}", crate::storage::cache_key(&audio)?);
            let mut body = Vec::new();
            for (name, value) in [
                ("model", self.endpoint.model.as_str()),
                ("response_format", "verbose_json"),
                ("timestamp_granularities[]", "segment"),
            ] {
                body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
            }
            body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
            body.extend_from_slice(&audio);
            body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            self.post(
                "audio/transcriptions",
                &format!("multipart/form-data; boundary={boundary}"),
                body,
            )
            .await?
        };
        let segments = response["segments"].as_array().ok_or_else(|| {
            failure(
                Failure::Unsupported,
                "transcription endpoint does not provide segment timestamps",
            )
        })?;
        let mut normalized = Vec::new();
        for segment in segments {
            let timestamp = |field: &str| -> Result<i64> {
                let value = segment[field]
                    .as_f64()
                    .context("invalid transcription timestamp")?;
                ensure!(
                    value.is_finite() && (0. ..1e12).contains(&value),
                    "invalid transcription timestamp"
                );
                Ok((value * 1_000_000.).round() as i64)
            };
            let lang = if self.endpoint.kind == "gemini" {
                &segment["lang"]
            } else {
                &response["language"]
            };
            normalized.push(json!({"start_us":timestamp("start")?,"end_us":timestamp("end")?,"text":segment["text"],"lang":lang}));
        }
        Ok(json!({"segments":normalized}))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        thread,
    };
    pub(crate) fn server(
        responses: Vec<(u16, Value)>,
    ) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
        server_with_retry_header(responses, true)
    }
    fn server_with_retry_header(
        responses: Vec<(u16, Value)>,
        retry_header: bool,
    ) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let start = std::time::Instant::now();
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                start.elapsed() < Duration::from_secs(15),
                                "request was not received"
                            );
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(socket.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                requests.push((
                    first,
                    serde_json::from_slice(&bytes)
                        .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned())),
                ));
                let body = body.to_string();
                let retry = if retry_header {
                    "Retry-After: 0\r\n"
                } else {
                    ""
                };
                write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            requests
        });
        (format!("http://{address}/v1beta"), handle)
    }
    fn provider(base: String, kind: &str) -> Provider {
        Provider::new(
            Endpoint {
                kind: kind.into(),
                model: "fixture".into(),
                base_url: base,
                api_key_env: "TEST_KEY".into(),
                dims: Some(2),
            },
            None,
            2,
            None,
            CancellationToken::new(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn gemini_json_retry_hint_is_observed_and_default_backoff_is_cancellable() {
        let (base, server) = server_with_retry_header(
            vec![
                (
                    429,
                    json!({"error":{"details":[{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"0.1s"}]}}),
                ),
                (200, json!({"embedding":{"values":[0.2,0.8]}})),
            ],
            false,
        );
        let model = provider(base, "gemini");
        let start = std::time::Instant::now();
        assert_eq!(
            model
                .embed(Input::Text("fixture".into()), false)
                .await
                .unwrap(),
            vec![0.2, 0.8]
        );
        assert!(start.elapsed() >= Duration::from_millis(100));
        assert_eq!(server.join().unwrap().len(), 2);

        let (base, server) =
            server_with_retry_header(vec![(429, json!({"error":{"message":"busy"}}))], false);
        let model = provider(base, "gemini");
        let cancel = model.cancel.clone();
        let request =
            tokio::spawn(async move { model.embed(Input::Text("fixture".into()), false).await });
        // Completion of the first request proves the provider reached backoff.
        tokio::task::spawn_blocking(move || server.join().unwrap())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!request.is_finished());
        cancel.cancel();
        let error = tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProviderError>().unwrap().kind,
            Failure::Cancelled
        );
    }
    #[tokio::test]
    async fn gemini_inline_media_request_retries_transient_error_and_checks_dimensions() {
        let (base, server) = server(vec![
            (429, json!({})),
            (200, json!({"embedding":{"values":[0.5,0.5]}})),
            (200, json!({"embedding":{"values":[0.5]}})),
        ]);
        let provider = provider(base, "gemini");
        assert_eq!(
            provider
                .embed(Input::Video(vec![1, 2, 3], "video/mp4".into()), false)
                .await
                .unwrap(),
            vec![0.5, 0.5]
        );
        let error = provider
            .embed(Input::Text("query".into()), true)
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProviderError>().unwrap().kind,
            Failure::InvalidResponse
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].0.contains("models/fixture:embedContent"));
        assert_eq!(
            requests[0].1["content"]["parts"][0]["inlineData"]["data"],
            "AQID"
        );
        assert_eq!(
            requests[0].1["embedContentConfig"]["outputDimensionality"],
            2
        );
        assert!(
            requests[2].1["content"]["parts"][0]["text"]
                .as_str()
                .unwrap()
                .contains("query: query")
        );
    }
    #[tokio::test]
    async fn openai_vision_and_transcription_use_distinct_wire_contracts() {
        let (base, server) = server(vec![
            (
                200,
                json!({"choices":[{"message":{"content":"{\"ok\":true}"}}]}),
            ),
            (
                200,
                json!({"language":"en","segments":[{"start":0.25,"end":1.5,"text":"hello"}]}),
            ),
        ]);
        let provider = provider(base, "openai");
        assert_eq!(
            provider
                .generate(
                    "check",
                    &[Input::Image(vec![0], "image/png".into())],
                    json!({"type":"object"})
                )
                .await
                .unwrap(),
            json!({"ok":true})
        );
        assert_eq!(
            provider.transcribe(vec![1, 2, 3], 1_000_000).await.unwrap()["segments"][0]["start_us"],
            250000
        );
        let requests = server.join().unwrap();
        assert!(requests[0].0.contains("chat/completions"));
        assert_eq!(
            requests[0].1["messages"][0]["content"][1]["type"],
            "image_url"
        );
        assert!(requests[1].0.contains("audio/transcriptions"));
        assert!(
            requests[1]
                .1
                .as_str()
                .unwrap()
                .contains("timestamp_granularities[]")
        );
    }
    #[tokio::test]
    async fn text_only_embedding_endpoint_fails_multimodal_probe_without_caching_success() {
        let (base, server) = server(vec![
            (200, json!({"data":[{"embedding":[0.5,0.5]}]})),
            (400, json!({"error":"images unsupported"})),
        ]);
        let provider = provider(base, "openai");
        let workspace = tempfile::tempdir().unwrap();
        let error = probes::check(
            &provider,
            probes::Capability::Embedding,
            workspace.path(),
            false,
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProviderError>().unwrap().kind,
            Failure::Unsupported
        );
        assert!(!workspace.path().join("providers.json").exists());
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].1["input"][0]["type"], "image_url");
    }
    #[tokio::test]
    async fn cancelled_and_oversized_requests_never_connect() {
        let provider = provider("http://127.0.0.1:9/v1beta".into(), "gemini");
        provider.cancel.cancel();
        assert_eq!(
            provider
                .embed(Input::Text("query".into()), true)
                .await
                .unwrap_err()
                .downcast_ref::<ProviderError>()
                .unwrap()
                .kind,
            Failure::Cancelled
        );
        assert_eq!(
            provider
                .embed(
                    Input::Image(vec![0; MAX_REQUEST_BYTES], "image/png".into()),
                    false
                )
                .await
                .unwrap_err()
                .downcast_ref::<ProviderError>()
                .unwrap()
                .kind,
            Failure::TooLarge
        );
    }
}

#[cfg(test)]
mod credential_scope_tests {
    use super::*;
    #[tokio::test]
    async fn host_resolution_is_lazy_and_missing_credentials_are_resolved_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let resolver: CredentialResolver = Arc::new(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(None) })
        });
        with_credential_resolver(resolver, async {
            let provider = Provider::new(
                crate::config::Config::default().embedding,
                None,
                1,
                None,
                CancellationToken::new(),
            )
            .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            for _ in 0..2 {
                let error = provider
                    .embed(Input::Text("test".into()), true)
                    .await
                    .unwrap_err();
                assert_eq!(
                    error.downcast_ref::<ProviderError>().unwrap().kind,
                    Failure::MissingKey
                );
            }
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        })
        .await;
    }
    #[tokio::test]
    async fn scoped_credentials_do_not_escape_to_other_endpoints_or_tasks() {
        let mut endpoint = crate::config::Config::default().embedding;
        endpoint.api_key_env = "CERUL_TEST_SCOPED_KEY_UNSET".into();
        let keys =
            std::collections::BTreeMap::from([(credential_scope(&endpoint), "test-key".into())]);
        with_credentials(keys, async {
            let provider =
                Provider::from_env(endpoint.clone(), 1, None, CancellationToken::new()).unwrap();
            assert!(provider.key.is_some());
            let mut other = endpoint.clone();
            other.base_url = "https://other.example/v1".into();
            assert!(
                Provider::from_env(other, 1, None, CancellationToken::new())
                    .unwrap()
                    .key
                    .is_none()
            );
        })
        .await;
        assert!(
            Provider::from_env(endpoint, 1, None, CancellationToken::new())
                .unwrap()
                .key
                .is_none()
        );
    }
}
