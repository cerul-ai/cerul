//! Human configuration lives in the binary; processing stays non-interactive.
use anyhow::{Context, Result, ensure};
use cerul::config::{Config, Endpoint};
use std::{fs, io::Write, path::PathBuf};
use tokio_util::sync::CancellationToken;

fn path() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is required to save configuration")?)
            .join(".cerul/config.toml"),
    )
}

pub fn available(endpoint: &Endpoint) -> bool {
    let state = crate::credentials::state(endpoint);
    state.env_set || state.saved
}

/// Resolve automatic speech without changing an explicit provider or opt-out.
pub fn automatic(endpoint: &mut Endpoint, key_available: bool) {
    if endpoint.enabled.is_none() && key_available {
        endpoint.enabled = Some(true);
    }
}

fn preset(name: &str) -> Endpoint {
    let mut endpoint = Config::default().transcription;
    endpoint.enabled = Some(true);
    match name {
        "openai" => {
            endpoint.kind = "openai".into();
            endpoint.base_url = "https://api.openai.com/v1".into();
            endpoint.api_key_env = "OPENAI_API_KEY".into();
            endpoint.model = "whisper-1".into();
        }
        "groq" => {
            endpoint.kind = "openai".into();
            endpoint.base_url = "https://api.groq.com/openai/v1".into();
            endpoint.api_key_env = "GROQ_API_KEY".into();
            endpoint.model = "whisper-large-v3-turbo".into();
        }
        "custom" => {
            endpoint.kind = "openai".into();
            endpoint.api_key_env = "ASR_API_KEY".into();
            endpoint.model.clear();
            endpoint.base_url.clear();
        }
        _ => {}
    }
    endpoint
}

fn field(label: &str, default: &str) -> Result<String> {
    let term = console::Term::stderr();
    term.write_str(&format!("{label} [{}]: ", default))?;
    let value = term.read_line()?;
    let value = value.trim();
    Ok(if value.is_empty() {
        default.into()
    } else {
        value.into()
    })
}

fn save(endpoint: &Endpoint) -> Result<()> {
    let path = path()?;
    let mut document: toml::Table = match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
        Err(e) => return Err(e.into()),
    };
    document.insert("transcription".into(), toml::Value::try_from(endpoint)?);
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(toml::to_string_pretty(&document)?.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(&path).map_err(|e| e.error)?;
    Ok(())
}

pub async fn configure(config: &Config, cancel: CancellationToken) -> Result<bool> {
    ensure!(
        crate::guide::asks(&[]),
        "cerul config requires an interactive terminal; edit ~/.cerul/config.toml for scripted configuration"
    );
    if !available(&config.embedding) {
        crate::credentials::set(&config.embedding, cancel.clone()).await?;
    }
    let palette = crate::render::Palette::new(console::colors_enabled_stderr());
    use crate::guide::Choice;
    let selection = crate::guide::select(
        &palette,
        "Speech transcription",
        &[
            Choice::new("Gemini", "Reuse the Gemini key", "gemini"),
            Choice::new("Groq", "Whisper transcription", "groq"),
            Choice::new("OpenAI", "Whisper transcription", "openai"),
            Choice::new("Custom", "OpenAI-compatible transcription API", "custom"),
            Choice::new(
                "Disabled",
                "Index video without a separate transcript",
                "disabled",
            ),
        ],
    )?;
    let Some(selection) = selection else {
        return Ok(false);
    };
    let mut endpoint = preset(selection);
    if selection == "disabled" {
        endpoint.enabled = Some(false);
    } else {
        if selection == "custom" {
            endpoint.base_url = field("Base URL (for example https://host/v1)", "")?;
        }
        endpoint.model = field("ASR model", &endpoint.model)?;
        let mut candidate = config.clone();
        candidate.transcription = endpoint.clone();
        candidate.validate()?;
        let key_state = crate::credentials::state(&endpoint);
        if key_state.env_set {
            eprintln!(
                "Using {} from the environment; change that variable to replace it.",
                endpoint.api_key_env
            );
        } else if !key_state.saved {
            crate::credentials::set(&endpoint, cancel.clone()).await?;
        }
        // Even a shared or environment key must be checked against the selected ASR model.
        let provider = cerul::providers::Provider::from_env(endpoint.clone(), 1, None, cancel)?;
        let temporary = tempfile::tempdir()?;
        cerul::providers::probes::check(
            &provider,
            cerul::providers::probes::Capability::Transcription,
            temporary.path(),
            true,
        )
        .await?;
    }
    save(&endpoint)?;
    eprintln!("Saved transcription settings.");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_speech_preserves_explicit_settings() {
        let mut endpoint = preset("gemini");
        endpoint.enabled = None;
        automatic(&mut endpoint, false);
        assert_eq!(endpoint.enabled, None);
        automatic(&mut endpoint, true);
        assert_eq!(endpoint.enabled, Some(true));
        endpoint.enabled = Some(false);
        automatic(&mut endpoint, true);
        assert_eq!(endpoint.enabled, Some(false));
        let mut custom = preset("groq");
        automatic(&mut custom, true);
        assert_eq!(custom.model, "whisper-large-v3-turbo");
    }
    #[test]
    fn presets_share_protocol_not_credentials() {
        let groq = preset("groq");
        let openai = preset("openai");
        assert_eq!(groq.kind, openai.kind);
        assert_ne!(
            cerul::providers::credential_scope(&groq),
            cerul::providers::credential_scope(&openai)
        );
        assert_eq!(
            preset("gemini").api_key_env,
            Config::default().embedding.api_key_env
        );
        assert_eq!(preset("custom").api_key_env, "ASR_API_KEY");
    }
}
