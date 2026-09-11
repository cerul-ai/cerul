use super::{
    discover::{self, Input},
    embed, stations,
};
use crate::{
    annotations::AnnotationFile,
    config::Config,
    episode::{Episode, Stream},
    events::{Event, EventSink},
    providers::{Failure, Provider, ProviderError, probes},
    storage,
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Options {
    pub embedding: embed::Options,
    pub no_audio: bool,
    pub no_ocr: bool,
    pub streams: String,
    pub only: Option<String>,
    pub jobs: usize,
    pub rpm: Option<u32>,
    pub sidecar_dir: Option<PathBuf>,
    pub dry_run: bool,
    pub request_notice: Option<crate::providers::RequestNotice>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            embedding: Default::default(),
            no_audio: false,
            no_ocr: false,
            streams: "primary".into(),
            only: None,
            jobs: 4,
            rpm: None,
            sidecar_dir: None,
            dry_run: false,
            request_notice: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub episodes: Vec<EpisodeResult>,
    pub partial: bool,
    pub dry_run: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EpisodeResult {
    pub episode_id: String,
    pub sidecar: PathBuf,
    pub streams: Vec<StreamResult>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpeechStatus {
    Disabled,
    NotConfigured,
    NoAudio,
    Complete,
    Failed,
    Planned,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StreamResult {
    pub stream: String,
    pub indexed: bool,
    pub speech: SpeechStatus,
    pub vector_rows: usize,
    pub errors: Vec<String>,
}

pub(crate) fn selected(episode: &Episode, selector: Option<&str>) -> bool {
    let Some(selector) = selector else {
        return true;
    };
    selector.split(',').any(|item| {
        item == episode.episode_id
            || item == episode.local_id
            || item
                .parse::<u64>()
                .ok()
                .zip(episode.local_id.parse::<u64>().ok())
                .is_some_and(|(a, b)| a == b)
    })
}
pub(crate) fn streams(episode: &Episode, selection: &str) -> Result<Vec<String>> {
    let all: Vec<_> = episode
        .streams
        .iter()
        .filter_map(|stream| match stream {
            Stream::Video { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    let selected = match selection {
        "primary" => vec![episode.time.reference.clone()],
        "all" => all.clone(),
        other => other.split(',').map(str::to_owned).collect(),
    };
    ensure!(
        !selected.is_empty() && selected.iter().all(|id| all.contains(id)),
        "unknown video stream selection"
    );
    let mut selected = selected;
    selected.sort();
    selected.dedup();
    Ok(selected)
}
fn existing_annotation(
    sidecar: &Path,
    episode: &Episode,
    stream: &str,
    name: &str,
    disabled: bool,
) -> Result<Option<AnnotationFile>> {
    if disabled {
        return Ok(None);
    }
    let path = stations::stream_directory(sidecar, stream, &episode.time.reference)
        .join(format!("{name}.jsonl"));
    if path.is_file() {
        Ok(Some(AnnotationFile::read(&path)?))
    } else {
        Ok(None)
    }
}
fn retained_annotation(
    sidecar: &Path,
    episode: &Episode,
    stream: &str,
    name: &str,
) -> Result<Option<AnnotationFile>> {
    let Some(file) = existing_annotation(sidecar, episode, stream, name, false)? else {
        return Ok(None);
    };
    if file.header.name != name
        || file.header.stream != stream
        || !stations::has_current_input(episode, &file)?
    {
        return Ok(None);
    }
    file.validate(episode.duration_us()?, None)?;
    Ok(Some(file))
}

fn speech_plan(
    episode: &Episode,
    stream: &str,
    disabled: bool,
    enabled: Option<bool>,
) -> Result<SpeechStatus> {
    Ok(if disabled {
        SpeechStatus::Disabled
    } else if enabled.is_none() {
        SpeechStatus::NotConfigured
    } else if matches!(episode.video(stream)?, Stream::Video {probe, ..} if !probe.has_audio) {
        SpeechStatus::NoAudio
    } else {
        SpeechStatus::Planned
    })
}

fn interrupted(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(ProviderError {
            kind: Failure::Cancelled,
            message: "operation cancelled".into(),
        }
        .into());
    }
    Ok(())
}

/// Read-only planning for a CLI deciding whether missing speech settings are relevant.
/// A fully cached transcript never requires a key just to rebuild an index.
pub fn pending_transcription(
    paths: &[PathBuf],
    workspace: &Path,
    config: &Config,
    options: &Options,
) -> Result<bool> {
    let registry = discover::read_registry(workspace)?;
    let provider = Provider::new(
        config.transcription.clone(),
        None,
        1,
        None,
        CancellationToken::new(),
    )?;
    for input in discover::discover(paths)? {
        let episodes = match input {
            Input::Video(path) => vec![discover::ordinary_episode(&path)?],
            Input::LeRobot(path) => crate::lerobot::read(&path)?,
        };
        for episode in episodes {
            if !selected(&episode, options.only.as_deref()) {
                continue;
            }
            let sidecar =
                discover::sidecar_path(&episode, &registry, options.sidecar_dir.as_deref())?;
            for stream in streams(&episode, &options.streams)? {
                if stations::transcript_pending(
                    &episode,
                    &stream,
                    &sidecar,
                    &provider,
                    options.embedding.recompute,
                )? {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

pub async fn run(
    paths: &[PathBuf],
    workspace: &Path,
    config: &Config,
    options: &Options,
    cancel: CancellationToken,
    events: &mut dyn EventSink,
) -> Result<Report> {
    crate::media::with_cancellation(
        cancel.clone(),
        run_inner(paths, workspace, config, options, cancel, events),
    )
    .await
}

async fn run_inner(
    paths: &[PathBuf],
    workspace: &Path,
    config: &Config,
    options: &Options,
    cancel: CancellationToken,
    events: &mut dyn EventSink,
) -> Result<Report> {
    config.validate()?;
    let mut effective = options.clone();
    let explicitly_disabled = options.no_audio || config.transcription.enabled == Some(false);
    effective.no_audio |= config.transcription.enabled != Some(true);
    let options = &effective;
    ensure!(
        options.jobs > 0 && options.rpm != Some(0),
        "jobs and RPM must be positive"
    );
    crate::media::chunks(1, options.embedding.chunk_us, options.embedding.overlap_us)?;
    ensure!(
        options.embedding.chunk_us <= 32_000_000,
        "embedding chunk exceeds 32 seconds"
    );
    crate::media::check_dependencies()?;
    let inputs = discover::discover(paths)?;
    let mut episodes = Vec::new();
    let mut datasets = Vec::new();
    for input in inputs {
        match input {
            Input::Video(path) => {
                let episode = discover::ordinary_episode(&path)?;
                if selected(&episode, options.only.as_deref()) {
                    episodes.push(episode);
                }
            }
            Input::LeRobot(root) => {
                let members = crate::lerobot::read_with_workspace(&root, workspace)?;
                datasets.push((root, members.iter().map(|e| e.episode_id.clone()).collect()));
                episodes.extend(
                    members
                        .into_iter()
                        .filter(|episode| selected(episode, options.only.as_deref())),
                );
            }
        }
    }
    ensure!(
        !episodes.is_empty() || !datasets.is_empty(),
        "no matching videos"
    );
    let registry = discover::read_registry(workspace)?;
    if options.dry_run {
        let mut result = Vec::new();
        for episode in episodes {
            let selected = streams(&episode, &options.streams)?;
            let sidecar =
                discover::sidecar_path(&episode, &registry, options.sidecar_dir.as_deref())?;
            result.push(EpisodeResult {
                episode_id: episode.episode_id.clone(),
                sidecar,
                streams: selected
                    .into_iter()
                    .map(|stream| {
                        Ok(StreamResult {
                            speech: speech_plan(
                                &episode,
                                &stream,
                                explicitly_disabled,
                                config.transcription.enabled,
                            )?,
                            stream,
                            indexed: false,
                            vector_rows: 0,
                            errors: Vec::new(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            });
        }
        return Ok(Report {
            episodes: result,
            partial: false,
            dry_run: true,
        });
    }
    let _lock = storage::WorkspaceLock::acquire(workspace)?;
    for (root, members) in datasets {
        discover::reconcile_dataset(workspace, &root, &members)?;
    }
    let pending = discover::read_registry_all(workspace)?;
    for episode in &episodes {
        ensure!(
            !pending
                .iter()
                .any(|entry| entry.pending_deletion && entry.episode_id == episode.episode_id),
            "episode has pending cleanup; finish clean --sidecars before indexing it again"
        );
    }
    let mut embedding = Provider::from_env(
        config.embedding.clone(),
        options.jobs,
        options.rpm,
        cancel.clone(),
    )?;
    let mut transcription = Provider::from_env(
        config.transcription.clone(),
        options.jobs,
        options.rpm,
        cancel.clone(),
    )?;
    embedding.request_notice = options.request_notice.clone();
    transcription.request_notice = options.request_notice.clone();
    let space = config.space_id()?;
    let dims = config
        .embedding
        .dims
        .context("missing embedding dimensions")?;
    let mut report = Report {
        episodes: Vec::new(),
        partial: false,
        dry_run: false,
    };
    for episode in episodes {
        interrupted(&cancel)?;
        let selected = streams(&episode, &options.streams)?;
        let planned = discover::sidecar_path(
            &episode,
            &discover::read_registry(workspace)?,
            options.sidecar_dir.as_deref(),
        )?;
        let mut speech_blocked = std::collections::BTreeMap::new();
        // Probe only uncached audio work, before OCR. Offline endpoints may still
        // leave useful local output; unsupported protocols and credentials are hard errors.
        if !options.no_audio {
            for stream in &selected {
                if !stations::transcript_pending(
                    &episode,
                    stream,
                    &planned,
                    &transcription,
                    options.embedding.recompute,
                )? {
                    continue;
                }
                match probes::check(
                    &transcription,
                    probes::Capability::Transcription,
                    workspace,
                    false,
                )
                .await
                {
                    Ok(_) => {}
                    Err(error) if cancel.is_cancelled() => return Err(error),
                    Err(error) => {
                        speech_blocked.insert(
                            stream.clone(),
                            format!(
                                "model {} at {}: {error}",
                                transcription.endpoint.model, transcription.endpoint.base_url
                            ),
                        );
                    }
                }
            }
        }
        let mut blocked = std::collections::BTreeMap::new();
        // Preflight before expensive local processing, but never for a complete cached product.
        for stream in &selected {
            let transcript =
                existing_annotation(&planned, &episode, stream, "transcript", options.no_audio)?;
            let screen =
                existing_annotation(&planned, &episode, stream, "screen_text", options.no_ocr)?;
            let expected = embed::fingerprint(
                &episode,
                stream,
                &space,
                &options.embedding,
                transcript.as_ref(),
                screen.as_ref(),
            )?;
            let state = embed::state_path(&planned, stream, &episode.time.reference, &space);
            let complete = if state.is_file() && !options.embedding.recompute {
                let state: embed::State = serde_json::from_slice(&fs::read(state)?)?;
                state.complete
                    && state.input_hash == expected
                    && planned
                        .join("embeddings")
                        .join(format!("{space}.parquet"))
                        .is_file()
            } else {
                false
            };
            if !complete && !options.embedding.skip_still {
                match probes::check(&embedding, probes::Capability::Embedding, workspace, false)
                    .await
                {
                    Ok(_) => {}
                    Err(error)
                        if error
                            .downcast_ref::<ProviderError>()
                            .is_some_and(|e| matches!(e.kind, Failure::Unavailable)) =>
                    {
                        blocked.insert(stream.clone(), error.to_string());
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        let sidecar =
            discover::publish_episode(workspace, &episode, options.sidecar_dir.as_deref())?;
        let mut result = EpisodeResult {
            episode_id: episode.episode_id.clone(),
            sidecar: sidecar.clone(),
            streams: Vec::new(),
        };
        for stream in selected {
            interrupted(&cancel)?;
            let mut errors = Vec::new();
            let mut screen = if options.no_ocr {
                None
            } else {
                match stations::screen_text(
                    &episode,
                    &stream,
                    &sidecar,
                    options.embedding.recompute,
                    options.jobs,
                    events,
                    &cancel,
                ) {
                    Ok(file) => Some(file),
                    Err(error) => {
                        errors.push(error.to_string());
                        None
                    }
                }
            };
            let mut transcript = if options.no_audio {
                None
            } else if let Some(error) = speech_blocked.get(&stream) {
                errors.push(format!("speech: {error}"));
                None
            } else {
                match stations::transcript(
                    &episode,
                    &stream,
                    &sidecar,
                    workspace,
                    &transcription,
                    options.embedding.recompute,
                    events,
                )
                .await
                {
                    Ok(file) => Some(file),
                    Err(error) if cancel.is_cancelled() => return Err(error),
                    Err(error) => {
                        errors.push(error.to_string());
                        None
                    }
                }
            };
            let speech_completed = transcript.is_some();
            // Failed recomputation leaves the authoritative annotation intact. Reuse it
            // only when it still belongs to this exact media input and stream.
            if screen.is_none() && !options.no_ocr {
                screen = retained_annotation(&sidecar, &episode, &stream, "screen_text")?;
            }
            if transcript.is_none() && !options.no_audio {
                transcript = retained_annotation(&sidecar, &episode, &stream, "transcript")?;
            }
            if let Some(error) = blocked.get(&stream) {
                errors.push(error.clone());
            }
            let mut embedding_succeeded = false;
            let count = if !blocked.contains_key(&stream) {
                match embed::run(
                    &episode,
                    &stream,
                    &sidecar,
                    workspace,
                    config,
                    &embedding,
                    &options.embedding,
                    transcript.as_ref(),
                    screen.as_ref(),
                    events,
                )
                .await
                {
                    Ok(count) => {
                        embedding_succeeded = true;
                        count
                    }
                    Err(error)
                        if error.downcast_ref::<ProviderError>().is_some_and(|e| {
                            matches!(
                                e.kind,
                                Failure::Unsupported | Failure::MissingKey | Failure::Cancelled
                            )
                        }) =>
                    {
                        return Err(error);
                    }
                    Err(error) => {
                        interrupted(&cancel)?;
                        errors.push(error.to_string());
                        0
                    }
                }
            } else {
                0
            };
            if !errors.is_empty() {
                report.partial = true;
                let state_path =
                    embed::state_path(&sidecar, &stream, &episode.time.reference, &space);
                let saved_transcript = existing_annotation(
                    &sidecar,
                    &episode,
                    &stream,
                    "transcript",
                    options.no_audio,
                )?;
                let saved_screen = existing_annotation(
                    &sidecar,
                    &episode,
                    &stream,
                    "screen_text",
                    options.no_ocr,
                )?;
                let input_hash = embed::fingerprint(
                    &episode,
                    &stream,
                    &space,
                    &options.embedding,
                    saved_transcript.as_ref(),
                    saved_screen.as_ref(),
                )?;
                let preserve = if state_path.is_file() {
                    let state: embed::State = serde_json::from_slice(&fs::read(&state_path)?)?;
                    embedding_succeeded
                        || (state.complete
                            && state.input_hash == input_hash
                            && sidecar
                                .join("embeddings")
                                .join(format!("{space}.parquet"))
                                .is_file())
                } else {
                    false
                };
                if preserve {
                    let mut state: embed::State = serde_json::from_slice(&fs::read(&state_path)?)?;
                    state.error = Some(errors.join("; "));
                    storage::write_json(&state_path, &state)?;
                } else {
                    storage::write_json(
                        &state_path,
                        &embed::State {
                            input_hash,
                            complete: false,
                            error: Some(errors.join("; ")),
                        },
                    )?;
                    if workspace.join("index").join(&space).is_dir() {
                        super::lance::VectorIndex::open(workspace, &space, dims, true)
                            .await?
                            .replace(&episode.episode_id, &stream, &[])
                            .await?;
                    }
                }
                events.emit(Event::Log {
                    level: "error".into(),
                    msg: format!("{} {stream}: {}", episode.episode_id, errors.join("; ")),
                });
            }
            if errors.is_empty() {
                let path = embed::state_path(&sidecar, &stream, &episode.time.reference, &space);
                if path.is_file() {
                    let mut state: embed::State = serde_json::from_slice(&fs::read(&path)?)?;
                    if state.error.take().is_some() {
                        storage::write_json(&path, &state)?;
                    }
                }
            }
            let speech = match speech_plan(
                &episode,
                &stream,
                explicitly_disabled,
                config.transcription.enabled,
            )? {
                SpeechStatus::Planned if speech_completed => SpeechStatus::Complete,
                SpeechStatus::Planned => SpeechStatus::Failed,
                status => status,
            };
            result.streams.push(StreamResult {
                speech,
                stream,
                indexed: embedding_succeeded,
                vector_rows: count,
                errors,
            });
        }
        report.episodes.push(result);
    }
    if workspace.join("index").join(&space).is_dir() {
        super::records::RecordIndex::rebuild(workspace, &space).await?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn skip_still_keeps_zero_calls_but_rejects_unsupported_moving_video() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.embedding.dims = Some(2);
        let options = Options {
            no_audio: true,
            no_ocr: true,
            embedding: embed::Options {
                skip_still: true,
                ..Default::default()
            },
            ..Default::default()
        };
        for (name, filter) in [
            ("static", "color=size=64x64:rate=2:duration=4"),
            ("moving", "testsrc2=size=64x64:rate=2:duration=4"),
        ] {
            let source = dir.path().join(format!("{name}.mp4"));
            crate::media::run(
                crate::media::command("ffmpeg")
                    .args([
                        "-v", "error", "-f", "lavfi", "-i", filter, "-c:v", "libx264",
                    ])
                    .arg(&source),
            )
            .unwrap();
            let workspace = dir.path().join(name);
            if name == "static" {
                // No credentials: any model request would fail this complete run.
                config.embedding.base_url = "http://127.0.0.1:9".into();
                let report = run(
                    &[source],
                    &workspace,
                    &config,
                    &options,
                    CancellationToken::new(),
                    &mut |_| {},
                )
                .await
                .unwrap();
                assert!(!report.partial);
            } else {
                let (base, server) = crate::providers::tests::server(vec![(
                    400,
                    serde_json::json!({"error":"unsupported"}),
                )]);
                config.embedding.base_url = base;
                let error = run(
                    &[source],
                    &workspace,
                    &config,
                    &options,
                    CancellationToken::new(),
                    &mut |_| {},
                )
                .await
                .unwrap_err();
                assert_eq!(
                    error.downcast_ref::<ProviderError>().unwrap().kind,
                    Failure::Unsupported
                );
                assert_eq!(server.join().unwrap().len(), 1);
            }
        }
    }

    #[tokio::test]
    async fn offline_pending_camera_does_not_block_cached_camera_rebuild() {
        use crate::index::vectors::{self, Kind, VectorRow};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("dataset");
        crate::lerobot::tests::fixture(&root, "v3.1");
        let workspace = dir.path().join("workspace");
        let episode = crate::lerobot::read(&root).unwrap().remove(0);
        let sidecar = discover::publish_episode(&workspace, &episode, None).unwrap();
        let mut config = Config::default();
        config.embedding.base_url = "http://127.0.0.1:9".into();
        config.embedding.dims = Some(2);
        let options = Options {
            no_audio: true,
            no_ocr: true,
            streams: "all".into(),
            only: Some("0".into()),
            ..Default::default()
        };
        let space = config.space_id().unwrap();
        let stream = "observation.images.wrist";
        vectors::write(
            &sidecar.join("embeddings").join(format!("{space}.parquet")),
            &[VectorRow {
                id: "cached-wrist".into(),
                episode: episode.episode_id.clone(),
                stream: stream.into(),
                kind: Kind::Video,
                start_us: 0,
                end_us: 4_000_000,
                vector: vec![1., 0.],
                text: "cached".into(),
                still: false,
                space_id: space.clone(),
                params_hash: "fixture".into(),
            }],
            2,
        )
        .unwrap();
        storage::write_json(
            &embed::state_path(&sidecar, stream, &episode.time.reference, &space),
            &embed::State {
                input_hash: embed::fingerprint(
                    &episode,
                    stream,
                    &space,
                    &options.embedding,
                    None,
                    None,
                )
                .unwrap(),
                complete: true,
                error: None,
            },
        )
        .unwrap();
        assert!(!workspace.join("index").exists());
        let report = run(
            &[root],
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(report.partial);
        let camera = report.episodes[0]
            .streams
            .iter()
            .find(|s| s.stream == stream)
            .unwrap();
        assert!(camera.indexed, "{:?}", camera.errors);
        assert_eq!(camera.vector_rows, 1);
        assert_eq!(
            super::super::lance::VectorIndex::open(&workspace, &space, 2, false)
                .await
                .unwrap()
                .count()
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn failed_recompute_retains_speech_vectors_and_reports_failure() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("audio.mp4");
        crate::media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=64x64:rate=2:duration=1",
                    "-f",
                    "lavfi",
                    "-i",
                    "anullsrc=r=16000:cl=mono",
                    "-t",
                    "1",
                    "-c:v",
                    "libx264",
                    "-c:a",
                    "aac",
                ])
                .arg(&source),
        )
        .unwrap();
        use serde_json::json;
        let generated = |value: serde_json::Value| json!({"candidates":[{"content":{"parts":[{"text":value.to_string()}]}}]});
        let (base, server) = crate::providers::tests::server(vec![
            (200, generated(json!({"segments":[]}))),
            (
                200,
                generated(
                    json!({"segments":[{"start":0.1,"end":0.9,"text":"retained speech","lang":"en"}]}),
                ),
            ),
            (200, json!({"promptFeedback":{"blockReason":"OTHER"}})),
        ]);
        let (embedding_base, embedding_server) =
            crate::providers::tests::server(vec![
                (200, json!({"embedding":{"values":[1.,0.]}}));
                7
            ]);
        let mut config = Config::default();
        config.embedding.base_url = embedding_base;
        config.embedding.dims = Some(2);
        config.transcription.base_url = base;
        config.transcription.model = "gemini-3.8-flash".into();
        config.transcription.enabled = Some(true);
        let workspace = dir.path().join("workspace");
        let mut options = Options {
            no_ocr: true,
            ..Default::default()
        };
        // Dry-run must describe the same effective speech choice without requests.
        options.dry_run = true;
        for (enabled, no_audio, expected) in [
            (None, false, "not_configured"),
            (Some(false), false, "disabled"),
            (Some(true), true, "disabled"),
            (Some(true), false, "planned"),
        ] {
            config.transcription.enabled = enabled;
            options.no_audio = no_audio;
            let result = run(
                std::slice::from_ref(&source),
                &workspace,
                &config,
                &options,
                CancellationToken::new(),
                &mut |_| {},
            )
            .await
            .unwrap();
            assert_eq!(
                serde_json::to_value(&result.episodes[0].streams[0].speech).unwrap(),
                expected
            );
        }
        let mut silent_episode = discover::ordinary_episode(&source).unwrap();
        if let Stream::Video { probe, .. } = &mut silent_episode.streams[0] {
            probe.has_audio = false;
        }
        assert!(matches!(
            speech_plan(&silent_episode, "primary", false, Some(true)).unwrap(),
            SpeechStatus::NoAudio
        ));
        assert!(!workspace.exists());
        config.transcription.enabled = Some(true);
        options.no_audio = false;
        options.dry_run = false;
        let first = run(
            std::slice::from_ref(&source),
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(!first.partial);
        assert_eq!(first.episodes[0].streams[0].vector_rows, 2);
        let sidecar = &first.episodes[0].sidecar;
        let transcript = fs::read(sidecar.join("transcript.jsonl")).unwrap();
        options.embedding.recompute = true;
        let second = run(
            std::slice::from_ref(&source),
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(second.partial);
        assert!(matches!(
            second.episodes[0].streams[0].speech,
            SpeechStatus::Failed
        ));
        assert_eq!(second.episodes[0].streams[0].vector_rows, 2);
        assert_eq!(
            fs::read(sidecar.join("transcript.jsonl")).unwrap(),
            transcript
        );
        let space = config.space_id().unwrap();
        let rows = super::super::vectors::read(
            &sidecar.join("embeddings").join(format!("{space}.parquet")),
            2,
        )
        .unwrap();
        assert!(rows.iter().any(|row| row.text == "retained speech"));
        assert_eq!(
            super::super::lance::VectorIndex::open(&workspace, &space, 2, false)
                .await
                .unwrap()
                .count()
                .await
                .unwrap(),
            2
        );
        let state: embed::State = serde_json::from_slice(
            &fs::read(embed::state_path(sidecar, "primary", "primary", &space)).unwrap(),
        )
        .unwrap();
        assert!(state.complete && state.error.is_some());
        assert_eq!(server.join().unwrap().len(), 3);
        assert_eq!(embedding_server.join().unwrap().len(), 7);
    }

    #[tokio::test]
    async fn unsupported_transcription_keeps_video_searchable() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("audio.mp4");
        crate::media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=64x64:rate=2:duration=1",
                    "-f",
                    "lavfi",
                    "-i",
                    "anullsrc=r=16000:cl=mono",
                    "-t",
                    "1",
                    "-c:v",
                    "libx264",
                    "-c:a",
                    "aac",
                ])
                .arg(&source),
        )
        .unwrap();
        let (base, server) = crate::providers::tests::server(vec![(
            400,
            serde_json::json!({"error":"unsupported"}),
        )]);
        let mut config = Config::default();
        config.transcription.base_url = base;
        config.transcription.enabled = Some(true);
        config.transcription.model = "gemini-3.8-flash".into();
        let (embedding_base, embedding_server) = crate::providers::tests::server(vec![
            (
                200,
                serde_json::json!({"embedding":{"values":[1.,0.]}})
            );
            4
        ]);
        config.embedding.base_url = embedding_base;
        config.embedding.dims = Some(2);
        let workspace = dir.path().join("workspace");
        let report = run(
            std::slice::from_ref(&source),
            &workspace,
            &config,
            &Options {
                no_ocr: true,
                ..Default::default()
            },
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(report.partial);
        assert!(report.episodes[0].streams[0].indexed);
        assert_eq!(report.episodes[0].streams[0].vector_rows, 1);
        assert_eq!(server.join().unwrap().len(), 1);
        assert_eq!(embedding_server.join().unwrap().len(), 4);
        let state: embed::State = serde_json::from_slice(
            &fs::read(embed::state_path(
                &report.episodes[0].sidecar,
                "primary",
                "primary",
                &config.space_id().unwrap(),
            ))
            .unwrap(),
        )
        .unwrap();
        assert!(state.complete);
        assert!(state.error.is_some());
        assert_eq!(
            super::super::lance::VectorIndex::open(
                &workspace,
                &config.space_id().unwrap(),
                2,
                false
            )
            .await
            .unwrap()
            .count()
            .await
            .unwrap(),
            1
        );
        // Disabling or leaving ASR unconfigured reuses the completed video index offline.
        for enabled in [Some(false), None] {
            config.transcription.enabled = enabled;
            let report = run(
                std::slice::from_ref(&source),
                &workspace,
                &config,
                &Options {
                    no_ocr: true,
                    ..Default::default()
                },
                CancellationToken::new(),
                &mut |_| {},
            )
            .await
            .unwrap();
            assert!(!report.partial);
            let stream = &report.episodes[0].streams[0];
            assert!(stream.indexed);
            assert_eq!(stream.vector_rows, 1);
            assert!(matches!(
                (&stream.speech, enabled),
                (SpeechStatus::Disabled, Some(false)) | (SpeechStatus::NotConfigured, None)
            ));
        }
    }
    #[tokio::test]
    async fn offline_index_preserves_local_ocr_and_reports_incomplete_embedding() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("screen.mp4");
        crate::media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-loop",
                    "1",
                    "-i",
                    "tests/fixtures/ocr-text.png",
                    "-t",
                    "2",
                    "-r",
                    "2",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(&source),
        )
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let mut config = Config::default();
        config.embedding.base_url = format!("http://{address}/");
        let workspace = dir.path().join("workspace");
        let mut events = Vec::new();
        let report = run(
            &[source],
            &workspace,
            &config,
            &Options {
                no_audio: true,
                ..Default::default()
            },
            CancellationToken::new(),
            &mut |event| events.push(event),
        )
        .await
        .unwrap();
        assert!(report.partial);
        assert!(!report.episodes[0].streams[0].indexed);
        let transcript = report.episodes[0].sidecar.join("transcript.jsonl");
        assert!(!transcript.exists());
        let screen =
            AnnotationFile::read(&report.episodes[0].sidecar.join("screen_text.jsonl")).unwrap();
        assert!(!screen.records.is_empty());
        let status = crate::status::inspect(&workspace, None).unwrap();
        assert!(
            status.episodes[0]
                .annotations
                .contains(&"screen_text".to_owned())
        );
        assert!(status.episodes[0].embedding_spaces.is_empty());
        assert!(!status.episodes[0].embeddings[0].complete);
        assert!(!workspace.join("index").exists());
    }
}
