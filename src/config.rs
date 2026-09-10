use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const QUERY_TEMPLATE: &str = "task: search result | query: {query}";

/// Public vector-space identity. Credentials and key environment names are excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpaceMetadata {
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub dims: usize,
    pub query_template: String,
}
impl SpaceMetadata {
    pub fn id(&self) -> Result<String> {
        validate_url(&self.base_url)?;
        ensure!(
            matches!(self.kind.as_str(), "gemini" | "openai")
                && !self.model.is_empty()
                && self.dims > 0
                && !self.query_template.is_empty(),
            "invalid vector space metadata"
        );
        Ok(crate::storage::sha256_hex(serde_json::to_vec(&(
            &self.kind,
            self.base_url.trim_end_matches('/'),
            &self.model,
            Some(self.dims),
            &self.query_template,
        ))?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub kind: String,
    pub model: String,
    pub base_url: String,
    pub api_key_env: String,
    pub dims: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Perception {
    pub base_url: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub embedding: Endpoint,
    pub vision: Endpoint,
    pub transcription: Endpoint,
    pub perception: Perception,
}

impl Default for Config {
    fn default() -> Self {
        let endpoint = |model: &str, dims| Endpoint {
            kind: "gemini".into(),
            model: model.into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".into(),
            api_key_env: "GEMINI_API_KEY".into(),
            dims,
        };
        Self {
            embedding: endpoint("gemini-embedding-2", Some(1536)),
            vision: endpoint("gemini-3.8-flash", None),
            transcription: endpoint("gemini-3.8-flash", None),
            perception: Perception {
                base_url: "https://api.cerul.ai".into(),
                api_key_env: "CERUL_API_KEY".into(),
            },
        }
    }
}

fn merge(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                if let Some(old) = base.get_mut(&key) {
                    merge(old, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn validate_url(value: &str) -> Result<()> {
    let url = url::Url::parse(value).context("invalid endpoint URL")?;
    ensure!(
        !url.path().ends_with("//"),
        "endpoint URL must not end with repeated slashes"
    );
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "endpoint must use HTTP(S) with a host"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "endpoint URL must not contain credentials, query or fragment"
    );
    Ok(())
}
fn validate_key_name(value: &str) -> Result<()> {
    let mut chars = value.chars();
    ensure!(
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "invalid API key environment variable name"
    );
    Ok(())
}

impl Config {
    pub fn load(paths: &[PathBuf]) -> Result<Self> {
        Self::resolve(
            paths,
            &BTreeMap::new(),
            toml::Value::Table(Default::default()),
        )
    }

    /// CLI values override environment values, then project/global configuration.
    /// Provider credentials are deliberately never copied into configuration.
    pub fn resolve(
        paths: &[PathBuf],
        environment: &BTreeMap<String, String>,
        cli: toml::Value,
    ) -> Result<Self> {
        let mut explicit = toml::Value::Table(Default::default());
        for path in paths {
            match fs::read_to_string(path) {
                Ok(text) => merge(
                    &mut explicit,
                    toml::from_str(&text)
                        .with_context(|| format!("invalid config {}", path.display()))?,
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| format!("read {}", path.display()));
                }
            }
        }
        for section in ["embedding", "vision", "transcription", "perception"] {
            for field in ["kind", "model", "base_url", "api_key_env", "dims"] {
                if section == "perception" && !matches!(field, "base_url" | "api_key_env") {
                    continue;
                }
                let name = format!(
                    "CERUL_{}_{}",
                    section.to_ascii_uppercase(),
                    field.to_ascii_uppercase()
                );
                if let Some(raw) = environment.get(&name) {
                    let value = if field == "dims" {
                        toml::Value::Integer(
                            raw.parse()
                                .with_context(|| format!("{name} must be an integer"))?,
                        )
                    } else {
                        toml::Value::String(raw.clone())
                    };
                    let table = explicit
                        .as_table_mut()
                        .unwrap()
                        .entry(section)
                        .or_insert_with(|| toml::Value::Table(Default::default()));
                    table
                        .as_table_mut()
                        .context("endpoint config must be a table")?
                        .insert(field.into(), value);
                }
            }
        }
        merge(&mut explicit, cli);
        let mut defaults = toml::Value::try_from(Self::default())?;
        for section in ["embedding", "vision", "transcription"] {
            if explicit
                .get(section)
                .and_then(|v| v.get("kind"))
                .and_then(toml::Value::as_str)
                == Some("openai")
            {
                let table = defaults[section].as_table_mut().unwrap();
                table.insert(
                    "base_url".into(),
                    toml::Value::String("https://api.openai.com/v1".into()),
                );
                table.insert(
                    "api_key_env".into(),
                    toml::Value::String("OPENAI_API_KEY".into()),
                );
                // No universal multimodal OpenAI-compatible default exists.
                ensure!(
                    explicit[section].get("model").is_some(),
                    "openai endpoint {section} requires an explicit model"
                );
            }
        }
        merge(&mut defaults, explicit);
        let config: Self = defaults.try_into()?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        for endpoint in [&self.embedding, &self.vision, &self.transcription] {
            ensure!(
                matches!(endpoint.kind.as_str(), "gemini" | "openai"),
                "unsupported endpoint kind"
            );
            ensure!(!endpoint.model.trim().is_empty(), "empty model name");
            validate_url(&endpoint.base_url)?;
            validate_key_name(&endpoint.api_key_env)?;
        }
        validate_url(&self.perception.base_url)?;
        validate_key_name(&self.perception.api_key_env)?;
        ensure!(
            self.embedding.dims.is_some_and(|n| n > 0),
            "embedding dimensions must be positive"
        );
        Ok(())
    }

    pub fn space_metadata(&self) -> Result<SpaceMetadata> {
        Ok(SpaceMetadata {
            kind: self.embedding.kind.clone(),
            base_url: self.embedding.base_url.trim_end_matches('/').into(),
            model: self.embedding.model.clone(),
            dims: self
                .embedding
                .dims
                .context("embedding dimensions missing")?,
            query_template: QUERY_TEMPLATE.into(),
        })
    }
    pub fn space_id(&self) -> Result<String> {
        self.space_metadata()?.id()
    }
}

pub fn config_paths(home: &Path, cwd: &Path) -> [PathBuf; 2] {
    [home.join(".cerul/config.toml"), cwd.join("cerul.toml")]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_and_provider_defaults_do_not_copy_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("global.toml");
        let project = dir.path().join("project.toml");
        fs::write(
            &global,
            "[embedding]\ndims=768\n[vision]\nkind='openai'\nmodel='remote'\n",
        )
        .unwrap();
        fs::write(&project, "[vision]\nmodel='project'\n").unwrap();
        let env = BTreeMap::from([
            ("CERUL_VISION_MODEL".into(), "environment".into()),
            ("OPENAI_API_KEY".into(), "do-not-serialize-me".into()),
        ]);
        let config = Config::resolve(
            &[global, project],
            &env,
            toml::from_str("[vision]\nmodel='cli'\n").unwrap(),
        )
        .unwrap();
        assert_eq!(config.vision.model, "cli");
        assert_eq!(config.embedding.dims, Some(768));
        assert_eq!(config.vision.base_url, "https://api.openai.com/v1");
        assert_eq!(config.vision.api_key_env, "OPENAI_API_KEY");
        assert!(
            !serde_json::to_string(&config)
                .unwrap()
                .contains("do-not-serialize-me")
        );
    }
    #[test]
    fn space_is_endpoint_specific_but_not_key_specific() {
        let mut config = Config::default();
        let original = config.space_id().unwrap();
        config.embedding.api_key_env = "OTHER_KEY".into();
        assert_eq!(original, config.space_id().unwrap());
        config.embedding.base_url = "https://another.example/v1".into();
        assert_ne!(original, config.space_id().unwrap());
    }
    #[test]
    fn reject_secret_bearing_urls_and_unknown_config() {
        let mut config = Config::default();
        config.vision.base_url = "https://user:password@example.com/v1".into();
        assert!(config.validate().is_err());
        assert!(
            Config::resolve(
                &[],
                &BTreeMap::new(),
                toml::from_str("[vision]\napi_key='secret'\n").unwrap()
            )
            .is_err()
        );
        assert!(
            Config::resolve(
                &[],
                &BTreeMap::new(),
                toml::from_str("[vision]\nkind='openai'\n").unwrap()
            )
            .is_err()
        );
    }
}
