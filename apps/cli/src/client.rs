use anyhow::{anyhow, Context, Result};
use reqwest::{Method, StatusCode};
use serde_json::Value;

pub struct CerulClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl CerulClient {
    pub fn new(base_url: &str, token: String) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent(format!("cerul-cli/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .context("failed to build HTTP client")?,
            base_url: normalize_base_url(base_url)?,
            token,
        })
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        self.request(Method::GET, path, None, None).await
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        self.request(Method::DELETE, path, None, None).await
    }

    pub async fn post(
        &self,
        path: &str,
        body: Value,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        self.request(Method::POST, path, Some(body), idempotency_key)
            .await
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let mut request = self
            .http
            .request(method, format!("{}{}", self.base_url, path))
            .bearer_auth(&self.token)
            .header("Accept", "application/json")
            .header("X-Cerul-Client-Source", "cli");
        if let Some(key) = idempotency_key {
            request = request.header("Idempotency-Key", key);
        }
        if let Some(value) = body {
            request = request.json(&value);
        }
        let response = request
            .send()
            .await
            .context("failed to connect to Cerul API")?;
        let status = response.status();
        if status == StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let payload = response
            .json::<Value>()
            .await
            .context("failed to parse Cerul API response")?;
        if !status.is_success() {
            let code = payload
                .pointer("/error/code")
                .and_then(Value::as_str)
                .unwrap_or("api_error");
            let message = payload
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("Cerul API request failed");
            return Err(anyhow!("[{}] {}: {}", status.as_u16(), code, message));
        }
        Ok(payload)
    }
}

fn normalize_base_url(value: &str) -> Result<String> {
    let mut parsed = reqwest::Url::parse(value).context("invalid --base-url")?;
    let trimmed = parsed.path().trim_end_matches('/').to_string();
    let contract_root = trimmed.strip_suffix("/v1").unwrap_or(&trimmed).to_string();
    parsed.set_path(&contract_root);
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::normalize_base_url;

    #[test]
    fn one_client_normalizes_local_and_cloud_v1_urls() {
        assert_eq!(
            normalize_base_url("https://api.cerul.ai/v1").unwrap(),
            "https://api.cerul.ai"
        );
        assert_eq!(
            normalize_base_url("http://127.0.0.1:23785/v1/").unwrap(),
            "http://127.0.0.1:23785"
        );
    }
}
