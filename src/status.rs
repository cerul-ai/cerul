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
    /// Episode-relative length of the primary stream, read from the sidecar.
    #[serde(default)]
    pub duration_us: Option<i64>,
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

/// One published annotation record, reduced to what a person reads in a list.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TimelineEntry {
    pub episode: String,
    pub stream: String,
    /// Full annotation name, for example `semantic.subtask`.
    pub annotation: String,
    pub start_us: i64,
    pub end_us: i64,
    /// The record's own fields written as one line. The record itself stays
    /// authoritative; this is a reading aid, not a new data format.
    pub summary: String,
    pub record: crate::annotations::Record,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TimelineEpisode {
    pub episode_id: String,
    pub media: PathBuf,
    pub sidecar: PathBuf,
    pub duration_us: Option<i64>,
    /// Annotation names present for this episode, in file order.
    pub annotations: Vec<String>,
    /// Records that matched the selection before `limit` was applied.
    pub total: usize,
    pub entries: Vec<TimelineEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Timeline {
    pub episodes: Vec<TimelineEpisode>,
}
#[derive(Debug, Clone)]
pub struct TimelineOptions {
    /// Semantic item or full annotation name; `None` reads every published item.
    pub kind: Option<String>,
    pub limit: usize,
}

/// Writes one record as a line, using the fields that item actually defines.
/// An unknown item falls back to its own JSON so nothing is silently dropped.
pub fn summarize_record(annotation: &str, record: &crate::annotations::Record) -> String {
    let text = |key: &str| {
        record
            .fields
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let item = annotation
        .rsplit('/')
        .next()
        .unwrap_or(annotation)
        .strip_prefix("semantic.")
        .unwrap_or(annotation);
    match item {
        "task" | "subtask" => text("text"),
        "event" => {
            let objects = record
                .fields
                .get("objects")
                .and_then(serde_json::Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let outcome = text("outcome");
            let mut line = text("verb");
            if !objects.is_empty() {
                line.push(' ');
                line.push_str(&objects);
            }
            if !outcome.is_empty() {
                line.push_str(" → ");
                line.push_str(&outcome);
            }
            line
        }
        "interaction" => {
            let contact = match record
                .fields
                .get("contact")
                .and_then(serde_json::Value::as_bool)
            {
                Some(true) => " · in contact",
                Some(false) => " · no contact",
                None => "",
            };
            format!("{} · {}{contact}", text("hand"), text("object"))
        }
        "state" => {
            let (before, after) = (text("before"), text("after"));
            let change = match (before.is_empty(), after.is_empty()) {
                (false, false) => format!("{before} → {after}"),
                (true, false) => after,
                (false, true) => before,
                (true, true) => String::new(),
            };
            let head = format!("{} · {}", text("object"), text("attribute"));
            if change.is_empty() {
                head
            } else {
                format!("{head}: {change}")
            }
        }
        "flag" => format!("{}: {}", text("kind"), text("note")),
        "progress" => {
            let value = record
                .fields
                .get("value")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_default();
            let done = record
                .fields
                .get("done")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or_default();
            format!("{:.0}%{}", value * 100., if done { " · done" } else { "" })
        }
        _ => serde_json::to_string(&record.fields).unwrap_or_default(),
    }
}

/// Reads published annotation records in time order. Local and read-only: it
/// opens sidecars only, makes no model call, and returns records unchanged.
pub fn timeline(
    workspace: &Path,
    path: Option<&Path>,
    options: &TimelineOptions,
) -> Result<Timeline> {
    let wanted = options.kind.as_ref().map(|kind| {
        let item = kind.strip_prefix("semantic.").unwrap_or(kind);
        format!("semantic.{item}")
    });
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
        let mut entries = Vec::new();
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
                if !name.starts_with("semantic.") || name.contains("conflicts") {
                    continue;
                }
                let file = AnnotationFile::read(&directory.join(format!("{name}.jsonl")))?;
                // A file whose inputs changed describes media that no longer
                // exists in that form; showing its records would mislead.
                if file.header.stream != stream.id()
                    || !crate::index::stations::has_current_input(&episode, &file)?
                {
                    continue;
                }
                // The same validation the summary applies. A sidecar is
                // authoritative, so a corrupt one is an error to report, not a
                // set of records to hand back as if they had been published.
                let coverage = episode
                    .video_coverage(stream.id())?
                    .context("annotation stream has no episode coverage")?;
                file.validate_in_range(coverage, None)?;
                anyhow::ensure!(
                    file.header.episode == episode.episode_id,
                    "annotation provenance does not match its episode"
                );
                annotations.push(name.clone());
                if wanted.as_ref().is_some_and(|kind| kind != &name) {
                    continue;
                }
                for record in file.records {
                    entries.push(TimelineEntry {
                        episode: episode.episode_id.clone(),
                        stream: stream.id().to_owned(),
                        annotation: name.clone(),
                        start_us: record.start_us,
                        end_us: record.end_us,
                        summary: summarize_record(&name, &record),
                        record,
                    });
                }
            }
        }
        if annotations.is_empty() {
            continue;
        }
        annotations.sort();
        annotations.dedup();
        entries.sort_by(|a, b| {
            (a.start_us, a.end_us, &a.annotation, &a.stream).cmp(&(
                b.start_us,
                b.end_us,
                &b.annotation,
                &b.stream,
            ))
        });
        let total = entries.len();
        entries.truncate(options.limit);
        episodes.push(TimelineEpisode {
            episode_id: entry.episode_id,
            duration_us: episode.duration_us().ok(),
            media: entry.media,
            sidecar: entry.sidecar,
            annotations,
            total,
            entries,
        });
    }
    Ok(Timeline { episodes })
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
                if file.header.stream != stream.id()
                    || !crate::index::stations::has_current_input(&episode, &file)?
                {
                    continue;
                }
                if file.header.name.starts_with("semantic.") {
                    let coverage = episode
                        .video_coverage(stream.id())?
                        .context("annotation stream has no episode coverage")?;
                    file.validate_in_range(coverage, None)?;
                } else {
                    file.validate(episode.duration_us()?, None)?;
                }
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
            duration_us: episode.duration_us().ok(),
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

/// Perception belongs to a later milestone, so its default endpoint is not a
/// service anyone can reach. Probing it would contact Cerul's servers on a plain
/// status check and report the resulting authentication failure as if the person
/// had misconfigured something. Only an endpoint someone actually pointed
/// elsewhere, or gave a key for, is theirs to check.
fn perception_configured(config: &crate::config::Config) -> bool {
    let default = crate::config::Config::default();
    config.perception.base_url.trim_end_matches('/')
        != default.perception.base_url.trim_end_matches('/')
        || std::env::var(&config.perception.api_key_env).is_ok_and(|key| !key.trim().is_empty())
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
    let mut endpoints = vec![
        ("embedding", &config.embedding, Capability::Embedding),
        ("vision", &config.vision, Capability::Vision),
        (
            "transcription",
            &config.transcription,
            Capability::Transcription,
        ),
    ];
    if perception_configured(config) {
        endpoints.push(("perception", &perception, Capability::Perception));
    }
    for (name, endpoint, capability) in endpoints {
        let mut result = ProviderStatus {
            base_url: endpoint.base_url.clone(),
            model: (name != "perception").then(|| endpoint.model.clone()),
            supported: None,
            checked_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            ),
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
    fn unreleased_perception_endpoint_is_not_contacted_by_default() {
        let mut config = crate::config::Config::default();
        assert!(
            !perception_configured(&config),
            "a plain status check must not reach Cerul's own servers"
        );
        config.perception.base_url = "http://127.0.0.1:9/".into();
        assert!(perception_configured(&config));
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
