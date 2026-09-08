use crate::{annotations::AnnotationFile, episode::Episode, index::discover::read_registry};
use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Status {
    pub version: String,
    pub workspace: PathBuf,
    pub episodes: Vec<EpisodeStatus>,
    pub spaces: Vec<String>,
    pub space_details: BTreeMap<String, crate::config::SpaceMetadata>,
    pub capabilities: BTreeMap<String, Option<bool>>,
    pub providers: BTreeMap<String, ProviderStatus>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProviderStatus {
    pub base_url: String,
    pub model: Option<String>,
    pub supported: Option<bool>,
    pub checked_at: Option<u64>,
    pub error: Option<String>,
    pub advertised: Option<crate::providers::probes::PerceptionCapabilities>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EpisodeStatus {
    pub episode_id: String,
    pub media: PathBuf,
    pub sidecar: PathBuf,
    pub media_present: bool,
    pub annotations: Vec<String>,
    pub embedding_spaces: Vec<String>,
    pub embeddings: Vec<EmbeddingStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmbeddingStatus {
    pub stream: String,
    pub space_id: String,
    pub complete: bool,
    pub error: Option<String>,
}

fn file_names(directory: &Path, suffix: &str) -> Result<Vec<String>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        if let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.strip_suffix(suffix))
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Reads only local state: no dependency probe, model call, lock, or filesystem mutation.
pub fn inspect(workspace: &Path, path: Option<&Path>) -> Result<Status> {
    let selection = path
        .map(|p| {
            if p.exists() {
                fs::canonicalize(p)
            } else {
                Ok(p.to_owned())
            }
        })
        .transpose()?;
    let mut episodes = Vec::new();
    let mut space_details = BTreeMap::new();
    for entry in read_registry(workspace)? {
        if selection
            .as_ref()
            .is_some_and(|path| !entry.media.starts_with(path) && !entry.sidecar.starts_with(path))
        {
            continue;
        }
        let episode: Episode =
            serde_json::from_slice(&fs::read(entry.sidecar.join("episode.json"))?)
                .context("invalid registered episode")?;
        episode.validate()?;
        let mut annotations = Vec::new();
        for stream in &episode.streams {
            if !matches!(stream, crate::episode::Stream::Video { .. }) {
                continue;
            }
            let directory = crate::index::stations::stream_directory(
                &entry.sidecar,
                stream.id(),
                &episode.time.reference,
            );
            for name in file_names(&directory, ".jsonl")? {
                if name == "log" {
                    continue;
                }
                let file = AnnotationFile::read(&directory.join(format!("{name}.jsonl")))?;
                file.validate(episode.duration_us()?, None)?;
                anyhow::ensure!(
                    file.header.episode == episode.episode_id && file.header.stream == stream.id(),
                    "annotation provenance does not match its episode/stream"
                );
                annotations.push(if stream.id() == episode.time.reference {
                    name
                } else {
                    format!("{}/{name}", stream.id())
                });
            }
        }
        let products = file_names(&entry.sidecar.join("embeddings"), ".parquet")?;
        for id in file_names(&entry.sidecar.join("embeddings"), ".json")? {
            let metadata: crate::config::SpaceMetadata = serde_json::from_slice(&fs::read(
                entry.sidecar.join("embeddings").join(format!("{id}.json")),
            )?)?;
            anyhow::ensure!(
                metadata.id()? == id,
                "vector space metadata does not match its ID"
            );
            if let Some(old) = space_details.insert(id, metadata.clone()) {
                anyhow::ensure!(old == metadata, "conflicting vector space metadata");
            }
        }
        let mut embeddings = Vec::new();
        for stream in &episode.streams {
            if !matches!(stream, crate::episode::Stream::Video { .. }) {
                continue;
            }
            let directory = crate::index::stations::stream_directory(
                &entry.sidecar,
                stream.id(),
                &episode.time.reference,
            );
            for name in file_names(&directory, ".json")? {
                let Some(space_id) = name.strip_prefix("index.") else {
                    continue;
                };
                let state: crate::index::embed::State =
                    serde_json::from_slice(&fs::read(directory.join(format!("{name}.json")))?)?;
                embeddings.push(EmbeddingStatus {
                    stream: stream.id().to_owned(),
                    space_id: space_id.to_owned(),
                    complete: state.complete && products.iter().any(|id| id == space_id),
                    error: state.error,
                });
            }
        }
        // Only completed products belong in the usable-space inventory. An old
        // parquet file can remain while a replacement station is incomplete.
        let mut embedding_spaces: Vec<_> = embeddings
            .iter()
            .filter(|state| state.complete)
            .map(|state| state.space_id.clone())
            .collect();
        embedding_spaces.sort();
        embedding_spaces.dedup();
        episodes.push(EpisodeStatus {
            episode_id: entry.episode_id,
            media_present: entry.media.is_file(),
            media: entry.media,
            sidecar: entry.sidecar,
            annotations,
            embedding_spaces,
            embeddings,
        });
    }
    let mut spaces = Vec::new();
    match fs::read_dir(workspace.join("index")) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    spaces.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    spaces.sort();
    Ok(Status {
        version: env!("CARGO_PKG_VERSION").into(),
        workspace: workspace.into(),
        episodes,
        spaces,
        space_details,
        providers: BTreeMap::new(),
        capabilities: BTreeMap::from([
            ("ocr".into(), Some(true)),
            ("embedding".into(), None),
            ("vision".into(), None),
            ("transcription".into(), None),
            ("perception".into(), None),
        ]),
    })
}

/// Explicit remote inspection. A failure does not suppress results for other endpoints.
pub async fn check_providers(
    status: &mut Status,
    config: &crate::config::Config,
    cancel: tokio_util::sync::CancellationToken,
    notice: Option<crate::providers::RequestNotice>,
    force: bool,
) -> Result<bool> {
    use crate::providers::{
        Failure, Provider, ProviderError,
        probes::{self, Capability},
    };
    config.validate()?;
    let _lock = crate::storage::WorkspaceLock::acquire(&status.workspace)?;
    // Reuse the bounded bearer-auth HTTP transport; this is the Cerul GET contract,
    // not an OpenAI model call. No model is advertised for this endpoint.
    let perception = crate::config::Endpoint {
        kind: "openai".into(),
        model: "capabilities".into(),
        base_url: config.perception.base_url.clone(),
        api_key_env: config.perception.api_key_env.clone(),
        dims: None,
    };
    let mut partial = false;
    for (name, endpoint, capability) in [
        ("embedding", &config.embedding, Capability::Embedding),
        ("vision", &config.vision, Capability::Vision),
        (
            "transcription",
            &config.transcription,
            Capability::Transcription,
        ),
        ("perception", &perception, Capability::Perception),
    ] {
        let mut result = ProviderStatus {
            base_url: endpoint.base_url.clone(),
            model: (name != "perception").then(|| endpoint.model.clone()),
            supported: None,
            checked_at: None,
            error: None,
            advertised: None,
        };
        let outcome = async {
            let mut provider = Provider::from_env(endpoint.clone(), 1, None, cancel.clone())?;
            provider.request_notice = notice.clone();
            probes::check(&provider, capability, &status.workspace, force).await
        }
        .await;
        match outcome {
            Ok(probe) => {
                result.supported = Some(true);
                result.checked_at = Some(probe.checked_at);
                result.advertised = probe.perception;
            }
            Err(error) => {
                if cancel.is_cancelled()
                    || error
                        .downcast_ref::<ProviderError>()
                        .is_some_and(|e| e.kind == Failure::Cancelled)
                {
                    return Err(ProviderError {
                        kind: Failure::Cancelled,
                        message: "operation cancelled".into(),
                    }
                    .into());
                }
                result.supported = error
                    .downcast_ref::<ProviderError>()
                    .filter(|e| e.kind == Failure::Unsupported)
                    .map(|_| false);
                result.error = Some(error.to_string());
                partial = true;
            }
        }
        status.capabilities.insert(name.into(), result.supported);
        status.providers.insert(name.into(), result);
    }
    Ok(partial)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn explicit_provider_checks_cover_all_endpoints_cache_and_isolate_failure() {
        use serde_json::json;
        let generated = |value: serde_json::Value| json!({"candidates":[{"content":{"parts":[{"text":value.to_string()}]}}]});
        let (base, server) = crate::providers::tests::server(vec![
            (200, json!({"embedding":{"values":[0.5,0.5]}})),
            (200, json!({"embedding":{"values":[0.5,0.5]}})),
            (200, json!({"embedding":{"values":[0.5,0.5]}})),
            (200, generated(json!({"ok":true}))),
            (200, generated(json!({"segments":[]}))),
            (
                200,
                json!({"version":"1","tasks":["grounding.box"],"models":{"box":"fixture"}}),
            ),
            (415, json!({"error":"unsupported"})),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let mut config = crate::config::Config::default();
        config.embedding.base_url = base.clone();
        config.embedding.dims = Some(2);
        config.vision.base_url = base.clone();
        config.transcription.base_url = base.clone();
        config.perception.base_url = base;
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut status = inspect(dir.path(), None).unwrap();
        assert!(
            !check_providers(&mut status, &config, cancel.clone(), None, false)
                .await
                .unwrap()
        );
        assert_eq!(status.providers.len(), 4);
        assert!(status.providers.values().all(|p| p.supported == Some(true)));
        assert_eq!(
            status.providers["perception"]
                .advertised
                .as_ref()
                .unwrap()
                .tasks,
            ["grounding.box"]
        );
        let checked = status.providers["embedding"].checked_at;
        assert!(
            !check_providers(&mut status, &config, cancel.clone(), None, false)
                .await
                .unwrap()
        );
        assert_eq!(status.providers["embedding"].checked_at, checked);
        config.vision.model = "unsupported-model".into();
        assert!(
            check_providers(&mut status, &config, cancel, None, false)
                .await
                .unwrap()
        );
        assert_eq!(status.capabilities["vision"], Some(false));
        assert_eq!(status.capabilities["embedding"], Some(true));
        assert_eq!(status.capabilities["perception"], Some(true));
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 7);
        assert!(requests[5].0.starts_with("GET /v1beta/capabilities "));
        let local = inspect(dir.path(), None).unwrap();
        assert!(local.providers.is_empty());
        assert_eq!(local.capabilities["perception"], None);
    }
    #[test]
    fn fresh_status_does_not_create_workspace_or_assume_remote_capabilities() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("missing");
        let status = inspect(&workspace, None).unwrap();
        assert!(status.episodes.is_empty());
        assert!(!workspace.exists());
        assert_eq!(status.capabilities["embedding"], None);
    }
}
