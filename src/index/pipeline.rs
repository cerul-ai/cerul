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
pub struct StreamResult {
    pub stream: String,
    pub indexed: bool,
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
    for input in inputs {
        match input {
            Input::Video(path) => {
                let episode = discover::ordinary_episode(&path)?;
                if selected(&episode, options.only.as_deref()) {
                    episodes.push(episode);
                }
            }
            Input::LeRobot(root) => {
                episodes.extend(
                    crate::lerobot::read_with_workspace(&root, workspace)?
                        .into_iter()
                        .filter(|episode| selected(episode, options.only.as_deref())),
                );
            }
        }
    }
    ensure!(!episodes.is_empty(), "no matching videos");
    let registry = discover::read_registry(workspace)?;
    if options.dry_run {
        let mut result = Vec::new();
        for episode in episodes {
            let selected = streams(&episode, &options.streams)?;
            let sidecar =
                discover::sidecar_path(&episode, &registry, options.sidecar_dir.as_deref())?;
            result.push(EpisodeResult {
                episode_id: episode.episode_id,
                sidecar,
                streams: selected
                    .into_iter()
                    .map(|stream| StreamResult {
                        stream,
                        indexed: false,
                        vector_rows: 0,
                        errors: Vec::new(),
                    })
                    .collect(),
            });
        }
        return Ok(Report {
            episodes: result,
            partial: false,
            dry_run: true,
        });
    }
    let _lock = storage::WorkspaceLock::acquire(workspace)?;
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
        let mut blocked = None;
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
                            .is_some_and(|e| e.kind == Failure::Unavailable) =>
                    {
                        blocked = Some(error.to_string());
                        break;
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
            let screen = if options.no_ocr {
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
            let transcript = if options.no_audio {
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
                    Err(error) => {
                        errors.push(error.to_string());
                        None
                    }
                }
            };
            if let Some(error) = &blocked {
                errors.push(error.clone());
            }
            let count = if errors.is_empty() {
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
                    Ok(count) => count,
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
                storage::write_json(
                    &embed::state_path(&sidecar, &stream, &episode.time.reference, &space),
                    &embed::State {
                        input_hash: embed::fingerprint(
                            &episode,
                            &stream,
                            &space,
                            &options.embedding,
                            transcript.as_ref(),
                            screen.as_ref(),
                        )?,
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
                events.emit(Event::Log {
                    level: "error".into(),
                    msg: format!("{} {stream}: {}", episode.episode_id, errors.join("; ")),
                });
            }
            result.streams.push(StreamResult {
                stream,
                indexed: errors.is_empty(),
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
    async fn offline_index_preserves_local_ocr_and_reports_incomplete_embedding() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("screen.mp4");
        crate::media::run(
            std::process::Command::new("ffmpeg")
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
