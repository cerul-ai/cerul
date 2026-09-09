//! Probe only stations with pending remote calls. Cache successful checks for seven days.
use super::{Input, Provider};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Cursor,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Embedding,
    Vision,
    Transcription,
    Perception,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CachedProbe {
    pub key: String,
    pub capability: Capability,
    pub checked_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perception: Option<PerceptionCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PerceptionCapabilities {
    pub version: String,
    pub tasks: Vec<String>,
    pub models: std::collections::BTreeMap<String, Value>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn key(provider: &Provider, capability: Capability) -> Result<String> {
    let endpoint = &provider.endpoint;
    // Only the digest is persisted. A rotated credential cannot inherit a probe.
    let credential = provider
        .key_header()
        .map(|key| crate::storage::sha256_hex(key.as_bytes()));
    crate::storage::cache_key(&(
        &endpoint.kind,
        &endpoint.base_url,
        &endpoint.model,
        endpoint.dims,
        &endpoint.api_key_env,
        capability,
        credential,
        2,
    ))
}
fn read(path: &Path) -> Result<Vec<CachedProbe>> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}
fn image() -> Result<Vec<u8>> {
    let image = image::RgbImage::from_pixel(64, 64, image::Rgb([128, 128, 128]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    Ok(bytes.into_inner())
}
fn video() -> Result<Vec<u8>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("probe.mp4");
    crate::media::run(
        crate::media::command("ffmpeg")
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=size=64x64:rate=1:duration=2",
                "-an",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&path),
    )?;
    Ok(fs::read(path)?)
}
fn ffmpeg_available() -> Result<bool> {
    match crate::media::command("ffmpeg").arg("-version").output() {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn silence() -> Vec<u8> {
    let data_size = 32_000u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.extend_from_slice(&32000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    bytes.resize(44 + data_size as usize, 0);
    bytes
}

static CACHE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn check(
    provider: &Provider,
    capability: Capability,
    workspace: &Path,
    force: bool,
) -> Result<CachedProbe> {
    crate::media::with_cancellation(
        provider.cancel.clone(),
        check_inner(provider, capability, workspace, force),
    )
    .await
}

async fn check_inner(
    provider: &Provider,
    capability: Capability,
    workspace: &Path,
    force: bool,
) -> Result<CachedProbe> {
    provider.resolve_key().await?;
    let _guard = tokio::select! {biased; _=provider.cancel.cancelled()=>return Err(super::failure(super::Failure::Cancelled,"operation cancelled")),guard=CACHE_LOCK.lock()=>guard};
    let url = url::Url::parse(&provider.endpoint.base_url)?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if provider.key_header().is_none() && !loopback && capability != Capability::Perception {
        return Err(super::failure(
            super::Failure::MissingKey,
            format!("set {} for this endpoint", provider.endpoint.api_key_env),
        ));
    }
    let path = workspace.join("providers.json");
    let mut cached = read(&path)?;
    let key = key(provider, capability)?;
    let timestamp = now();
    if !force
        && let Some(found) = cached.iter().find(|probe| {
            probe.key == key
                && probe.capability == capability
                && probe.checked_at <= timestamp
                && timestamp - probe.checked_at < 7 * 24 * 60 * 60
        })
    {
        return Ok(found.clone());
    }
    if force {
        cached.retain(|entry| entry.key != key);
        crate::storage::write_json(&path, &cached)?;
    }
    let mut perception = None;
    match capability {
        Capability::Perception => {
            let advertised: PerceptionCapabilities =
                serde_json::from_value(provider.capabilities().await?).map_err(|_| {
                    super::failure(
                        super::Failure::InvalidResponse,
                        "invalid perception capabilities contract",
                    )
                })?;
            ensure!(
                !advertised.version.is_empty()
                    && advertised.tasks.iter().all(|task| !task.is_empty()),
                "invalid perception capability names"
            );
            perception = Some(advertised);
        }
        Capability::Embedding => {
            provider
                .embed(Input::Text("Capability check".into()), true)
                .await?;
            provider
                .embed(Input::Image(image()?, "image/png".into()), false)
                .await?;
            if !ffmpeg_available()? {
                return Err(super::failure(
                    super::Failure::Unsupported,
                    "ffmpeg is required for the embedding video capability probe",
                ));
            }
            let probe_video = video()?;
            provider
                .embed(Input::Video(probe_video, "video/mp4".into()), false)
                .await?;
        }
        Capability::Vision => {
            let response=provider.generate("Return {\"ok\":true}.",&[Input::Image(image()?,"image/png".into())],json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false})).await?;
            ensure!(
                response["ok"] == true,
                "vision endpoint did not return the requested structured output"
            );
        }
        Capability::Transcription => {
            let response = provider.transcribe(silence(), 1_000_000).await?;
            let segments = response["segments"]
                .as_array()
                .context("transcription has no segment output")?;
            for segment in segments {
                ensure!(
                    segment["start_us"].as_i64().is_some()
                        && segment["end_us"].as_i64().is_some()
                        && segment["text"].is_string(),
                    "transcription endpoint lacks timestamped segments"
                );
            }
        }
    }
    let probe = CachedProbe {
        key: key.clone(),
        capability,
        checked_at: timestamp,
        perception,
    };
    cached.retain(|entry| entry.key != key);
    cached.push(probe.clone());
    crate::storage::write_json(&path, &cached)?;
    Ok(probe)
}

/// Interpret semantic/transcription JSON separately from transport success.
pub fn segments(response: Value, duration_us: i64) -> Result<Vec<crate::annotations::Record>> {
    let segments = response["segments"]
        .as_array()
        .context("model returned no segments")?;
    let mut records = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let start_us = segment["start_us"]
            .as_i64()
            .context("missing segment start")?;
        let end_us = segment["end_us"].as_i64().context("missing segment end")?;
        let text = segment["text"]
            .as_str()
            .context("missing segment text")?
            .trim();
        if text.is_empty() {
            continue;
        }
        crate::episode::TimeRange::new(start_us, end_us)?;
        ensure!(
            end_us <= duration_us,
            "transcript segment exceeds clip duration"
        );
        records.push(crate::annotations::Record {
            id: index.to_string(),
            start_us,
            end_us,
            confidence: None,
            fields: std::collections::BTreeMap::from([
                ("text".into(), Value::String(text.into())),
                ("lang".into(), segment["lang"].clone()),
            ]),
        });
    }
    records.sort_by_key(|record| record.start_us);
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn credentials_scope_success_cache_and_missing_key_is_not_supported() {
        let dir = tempfile::tempdir().unwrap();
        let create = |key| {
            Provider::new(
                crate::config::Config::default().embedding,
                key,
                1,
                None,
                tokio_util::sync::CancellationToken::new(),
            )
            .unwrap()
        };
        let first = create(Some("fixture-first-key".into()));
        let second = create(Some("fixture-second-key".into()));
        let missing = create(None);
        assert_ne!(
            key(&first, Capability::Embedding).unwrap(),
            key(&second, Capability::Embedding).unwrap()
        );
        crate::storage::write_json(
            &dir.path().join("providers.json"),
            &vec![CachedProbe {
                key: key(&missing, Capability::Embedding).unwrap(),
                capability: Capability::Embedding,
                checked_at: now(),
                perception: None,
            }],
        )
        .unwrap();
        let error = check(&missing, Capability::Embedding, dir.path(), false)
            .await
            .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<super::super::ProviderError>()
                .unwrap()
                .kind,
            super::super::Failure::MissingKey
        );
    }
    #[test]
    fn transcript_validation_rejects_time_outside_input() {
        assert!(
            segments(
                json!({"segments":[{"start_us":0,"end_us":200,"text":"hi","lang":"en"}]}),
                100
            )
            .is_err()
        );
        assert_eq!(segments(json!({"segments":[]}), 100).unwrap().len(), 0);
        assert!(
            segments(
                json!({"segments":[{"start_us":0,"end_us":0,"text":" ","lang":"en"}]}),
                100
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            segments(
                json!({"segments":[{"start_us":0,"end_us":0,"text":"speech","lang":"en"}]}),
                100
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn fresh_cached_probe_does_not_contact_an_unreachable_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let mut endpoint = crate::config::Config::default().embedding;
        endpoint.base_url = "http://127.0.0.1:9".into();
        let provider = Provider::new(
            endpoint,
            None,
            1,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        let probe = CachedProbe {
            key: key(&provider, Capability::Embedding).unwrap(),
            capability: Capability::Embedding,
            checked_at: now(),
            perception: None,
        };
        crate::storage::write_json(&dir.path().join("providers.json"), &vec![probe]).unwrap();
        check(&provider, Capability::Embedding, dir.path(), false)
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn forced_failed_probe_evicts_an_old_success() {
        let dir = tempfile::tempdir().unwrap();
        let mut endpoint = crate::config::Config::default().embedding;
        endpoint.base_url = "http://127.0.0.1:9".into();
        let provider = Provider::new(
            endpoint,
            None,
            1,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        let old = CachedProbe {
            key: key(&provider, Capability::Embedding).unwrap(),
            capability: Capability::Embedding,
            checked_at: now(),
            perception: None,
        };
        crate::storage::write_json(&dir.path().join("providers.json"), &vec![old]).unwrap();
        assert!(
            check(&provider, Capability::Embedding, dir.path(), true)
                .await
                .is_err()
        );
        assert!(read(&dir.path().join("providers.json")).unwrap().is_empty());
    }
}
