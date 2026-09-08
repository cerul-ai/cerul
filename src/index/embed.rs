use super::{
    stations,
    vectors::{self, Kind, VectorRow},
};
use crate::{
    annotations::AnnotationFile,
    config::Config,
    episode::{Episode, Stream, TimeRange},
    events::{Event, EventSink},
    media::{self, extract::SourceRange},
    providers::{Failure, Input, Provider, ProviderError, probes},
    storage::{self, Checkpoints},
};
use anyhow::{Context, Result, ensure};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub input_hash: String,
    pub complete: bool,
    #[serde(default)]
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    pub chunk_us: i64,
    pub overlap_us: i64,
    pub skip_still: bool,
    pub recompute: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            chunk_us: 30_000_000,
            overlap_us: 5_000_000,
            skip_still: false,
            recompute: false,
        }
    }
}

pub fn state_path(sidecar: &Path, stream: &str, primary: &str, space: &str) -> PathBuf {
    stations::stream_directory(sidecar, stream, primary).join(format!("index.{space}.json"))
}
pub fn usable(sidecar: &Path, stream: &str, primary: &str, space: &str) -> Result<bool> {
    let path = state_path(sidecar, stream, primary, space);
    if !path.is_file() {
        return Ok(true);
    }
    Ok(serde_json::from_slice::<State>(&fs::read(path)?)?.complete)
}
/// An upstream station changed. Keep its vectors on disk for recovery, but never
/// project them until embeddings have been regenerated against the new input.
pub fn invalidate_stream_states(
    sidecar: &Path,
    stream: &str,
    primary: &str,
    message: &str,
) -> Result<()> {
    let directory = stations::stream_directory(sidecar, stream, primary);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("index.") || !name.ends_with(".json") {
            continue;
        }
        let mut state: State = serde_json::from_slice(&fs::read(&path)?)?;
        if state.complete {
            state.complete = false;
            state.error = Some(message.into());
            storage::write_json(&path, &state)?;
        }
    }
    Ok(())
}
pub fn fingerprint(
    episode: &Episode,
    stream: &str,
    space: &str,
    options: &Options,
    transcript: Option<&AnnotationFile>,
    screen: Option<&AnnotationFile>,
) -> Result<String> {
    stations::station_key(
        episode,
        stream,
        "embed",
        &json!({"proxy_recipe":media::proxy::RECIPE_VERSION,"space":space,"chunk_us":options.chunk_us,"overlap_us":options.overlap_us,"skip_still":options.skip_still,"transcript":transcript.map(|f|&f.records),"screen":screen.map(|f|&f.records)}),
    )
}
fn cancelled(provider: &Provider) -> Result<()> {
    if provider.cancel.is_cancelled() {
        return Err(ProviderError {
            kind: Failure::Cancelled,
            message: "operation cancelled".into(),
        }
        .into());
    }
    Ok(())
}
fn still(path: &Path) -> Result<bool> {
    let output = media::run(
        Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args([
                "-vf",
                "scale=32:32",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "-",
            ]),
    )?;
    let (frames, _) = output.stdout.as_chunks::<1024>();
    if frames.len() < 2 {
        return Ok(false);
    }
    Ok(frames.windows(2).all(|pair| {
        pair[0]
            .iter()
            .zip(pair[1].iter())
            .map(|(a, b)| i32::from(*a).abs_diff(i32::from(*b)) as u64)
            .sum::<u64>()
            < 1024
    }))
}

struct EmbeddingContext<'a> {
    episode: &'a Episode,
    stream: &'a str,
    sidecar: &'a Path,
    workspace: &'a Path,
    provider: &'a Provider,
    space: &'a str,
    options: &'a Options,
    transcript: Option<&'a AnnotationFile>,
    screen: Option<&'a AnnotationFile>,
    fingerprint: &'a str,
}
impl EmbeddingContext<'_> {
    async fn vector(
        &self,
        kind: Kind,
        range: TimeRange,
        input: Input,
        excerpt: String,
    ) -> Result<VectorRow> {
        let key = stations::station_key(
            self.episode,
            self.stream,
            "embedding-row",
            &json!({"space":self.space,"kind":kind,"range":range,"text":excerpt,"proxy_recipe":if matches!(kind, Kind::Video) { Some(media::proxy::RECIPE_VERSION) } else { None }}),
        )?;
        let checkpoints = Checkpoints::new(self.sidecar);
        if !self.options.recompute
            && let Some(mut row) = checkpoints.load::<VectorRow>(&key)?
        {
            row.validate(self.provider.endpoint.dims.context("missing dimensions")?)?;
            row.params_hash = self.fingerprint.into();
            return Ok(row);
        }
        cancelled(self.provider)?;
        probes::check(
            self.provider,
            probes::Capability::Embedding,
            self.workspace,
            false,
        )
        .await?;
        let vector = self.provider.embed(input, false).await?;
        let row = VectorRow {
            id: storage::cache_key(&(
                &self.episode.episode_id,
                self.stream,
                kind,
                range,
                self.space,
            ))?,
            episode: self.episode.episode_id.clone(),
            stream: self.stream.into(),
            kind,
            start_us: range.start_us,
            end_us: range.end_us,
            vector,
            text: excerpt,
            still: false,
            space_id: self.space.into(),
            params_hash: self.fingerprint.into(),
        };
        checkpoints.save(&key, &row)?;
        Ok(row)
    }
    async fn unit(&self, range: TimeRange) -> Result<Vec<VectorRow>> {
        let root_key = storage::cache_key(&(self.fingerprint, "unit", range))?;
        let checkpoints = Checkpoints::new(self.sidecar);
        if !self.options.recompute
            && let Some(rows) = checkpoints.load::<Vec<VectorRow>>(&root_key)?
        {
            for row in &rows {
                row.validate(self.provider.endpoint.dims.context("missing dimensions")?)?;
            }
            return Ok(rows);
        }
        cancelled(self.provider)?;
        let Stream::Video {
            path: source,
            sha256,
            range_us,
            ..
        } = self.episode.video(self.stream)?
        else {
            unreachable!()
        };
        let source = self.episode.source.root.join(source);
        let start = self
            .episode
            .episode_to_source(self.stream, range.start_us)?
            .max(range_us[0]);
        let end = self
            .episode
            .episode_to_source(self.stream, range.end_us)?
            .min(range_us[1]);
        if end <= start {
            checkpoints.save(&root_key, &Vec::<VectorRow>::new())?;
            return Ok(Vec::new());
        }
        let range = TimeRange::new(
            self.episode.source_to_episode(self.stream, start)?.max(0),
            self.episode
                .source_to_episode(self.stream, end)?
                .min(self.episode.duration_us()?),
        )?;
        let clip = media::proxy::get(
            &source,
            sha256,
            SourceRange::new(start, end)?,
            self.workspace,
            24,
        )?;
        if self.options.skip_still && still(&clip)? {
            checkpoints.save(&root_key, &Vec::<VectorRow>::new())?;
            return Ok(Vec::new());
        }
        let first = self
            .vector(
                Kind::Video,
                range,
                Input::Video(fs::read(&clip)?, "video/mp4".into()),
                String::new(),
            )
            .await;
        let video = match first {
            Ok(row) => row,
            Err(error)
                if error
                    .downcast_ref::<ProviderError>()
                    .is_some_and(|e| e.kind == Failure::TooLarge) =>
            {
                let clip = media::proxy::get(
                    &source,
                    sha256,
                    SourceRange::new(start, end)?,
                    self.workspace,
                    45,
                )?;
                match self
                    .vector(
                        Kind::Video,
                        range,
                        Input::Video(fs::read(&clip)?, "video/mp4".into()),
                        String::new(),
                    )
                    .await
                {
                    Ok(row) => row,
                    Err(error)
                        if error
                            .downcast_ref::<ProviderError>()
                            .is_some_and(|e| e.kind == Failure::TooLarge)
                            && range.end_us - range.start_us > 1_000_000 =>
                    {
                        let midpoint = range.start_us + (range.end_us - range.start_us) / 2;
                        let mut rows =
                            Box::pin(self.unit(TimeRange::new(range.start_us, midpoint)?)).await?;
                        rows.extend(
                            Box::pin(self.unit(TimeRange::new(midpoint, range.end_us)?)).await?,
                        );
                        checkpoints.save(&root_key, &rows)?;
                        return Ok(rows);
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        let mut rows = vec![video];
        for (kind, file) in [(Kind::Speech, self.transcript), (Kind::Screen, self.screen)] {
            let text = stations::text_for_range(file, range)?;
            if !text.is_empty() {
                rows.push(
                    self.vector(kind, range, Input::Text(text.clone()), text)
                        .await?,
                );
            }
        }
        checkpoints.save(&root_key, &rows)?;
        Ok(rows)
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    episode: &Episode,
    stream: &str,
    sidecar: &Path,
    workspace: &Path,
    config: &Config,
    provider: &Provider,
    options: &Options,
    transcript: Option<&AnnotationFile>,
    screen: Option<&AnnotationFile>,
    events: &mut dyn EventSink,
) -> Result<usize> {
    ensure!(
        options.chunk_us <= 32_000_000,
        "embedding chunks must be at most 32 seconds"
    );
    let ranges = media::chunks(episode.duration_us()?, options.chunk_us, options.overlap_us)?;
    let space = config.space_id()?;
    let dims = provider
        .endpoint
        .dims
        .context("embedding dimensions missing")?;
    storage::write_json(
        &sidecar.join("embeddings").join(format!("{space}.json")),
        &config.space_metadata()?,
    )?;
    let path = sidecar.join("embeddings").join(format!("{space}.parquet"));
    let state_path = state_path(sidecar, stream, &episode.time.reference, &space);
    let input_hash = fingerprint(episode, stream, &space, options, transcript, screen)?;
    let index = super::lance::VectorIndex::open(workspace, &space, dims, true).await?;
    let previous_complete = if path.is_file() && state_path.is_file() {
        let state: State = serde_json::from_slice(&fs::read(&state_path)?)?;
        state.complete && state.input_hash == input_hash
    } else {
        false
    };
    if previous_complete && !options.recompute {
        let rows: Vec<_> = vectors::read(&path, dims)?
            .into_iter()
            .filter(|row| row.stream == stream)
            .collect();
        index.replace(&episode.episode_id, stream, &rows).await?;
        return Ok(rows.len());
    }
    // A failed refresh of identical inputs must not withdraw a valid generation.
    if !previous_complete {
        storage::write_json(
            &state_path,
            &State {
                input_hash: input_hash.clone(),
                complete: false,
                error: None,
            },
        )?;
        index.replace(&episode.episode_id, stream, &[]).await?;
    }
    let context = EmbeddingContext {
        episode,
        stream,
        sidecar,
        workspace,
        provider,
        space: &space,
        options,
        transcript,
        screen,
        fingerprint: &input_hash,
    };
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut pending = futures::stream::iter(ranges.iter().copied())
        .map(|range| {
            let context = &context;
            async move { (range, context.unit(range).await) }
        })
        .buffer_unordered(provider.concurrency());
    let mut done = 0;
    while let Some((range, result)) = pending.next().await {
        match result {
            Ok(unit) => rows.extend(unit),
            Err(error) => {
                if error
                    .downcast_ref::<ProviderError>()
                    .is_some_and(|e| e.kind == Failure::Cancelled)
                {
                    return Err(error);
                }
                events.emit(Event::Log {
                    level: "error".into(),
                    msg: format!(
                        "{} {stream} {}..{}: {error}",
                        episode.episode_id, range.start_us, range.end_us
                    ),
                });
                errors.push(error);
            }
        }
        done += 1;
        events.emit(Event::Progress {
            episode: episode.episode_id.clone(),
            station: "embed".into(),
            done,
            total: ranges.len() as u64,
        });
    }
    drop(pending);
    if let Some(error) = errors.into_iter().next() {
        if !previous_complete {
            storage::write_json(
                &state_path,
                &State {
                    input_hash,
                    complete: false,
                    error: Some(error.to_string()),
                },
            )?;
        }
        return Err(error);
    }
    rows.sort_by(|a, b| {
        a.start_us
            .cmp(&b.start_us)
            .then(a.kind.as_str().cmp(b.kind.as_str()))
    });
    let count = rows.len();
    let mut all = if path.is_file() {
        vectors::read(&path, dims)?
    } else {
        Vec::new()
    };
    all.retain(|row| row.stream != stream);
    all.extend(rows.clone());
    vectors::write(&path, &all, dims)?;
    index.replace(&episode.episode_id, stream, &rows).await?;
    // The Lance projection and its state are one publication unit.  A state that
    // advertises these rows as complete must never become visible before Lance
    // has committed its replacement.
    storage::write_json(
        &state_path,
        &State {
            input_hash,
            complete: true,
            error: None,
        },
    )?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_annotation_invalidation_withdraws_every_saved_space() {
        let dir = tempfile::tempdir().unwrap();
        let first = "a".repeat(64);
        let second = "b".repeat(64);
        fs::create_dir_all(dir.path()).unwrap();
        for space in [&first, &second] {
            let path = state_path(dir.path(), "front", "front", space);
            storage::write_json(
                &path,
                &State {
                    input_hash: "old".into(),
                    complete: true,
                    error: None,
                },
            )
            .unwrap();
        }
        invalidate_stream_states(dir.path(), "front", "front", "upstream changed").unwrap();
        for space in [&first, &second] {
            let state: State = serde_json::from_slice(
                &fs::read(state_path(dir.path(), "front", "front", space)).unwrap(),
            )
            .unwrap();
            assert!(!state.complete);
            assert_eq!(state.error.as_deref(), Some("upstream changed"));
        }
    }
    #[tokio::test]
    async fn interrupted_unit_resumes_only_missing_call_and_rebuild_uses_zero_calls() {
        let ok = json!({"embedding":{"values":[0.5,0.5]}});
        let (base, server) = crate::providers::tests::server(vec![
            (200, ok.clone()),
            (200, ok.clone()),
            (200, ok.clone()),
            (200, ok.clone()),
            (400, json!({"error":"fixture failure"})),
            (200, ok),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("video.mp4");
        media::run(
            Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=2:duration=4",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let episode = super::super::discover::ordinary_episode(&source).unwrap();
        let workspace = dir.path().join("workspace");
        let sidecar = super::super::discover::publish_episode(&workspace, &episode, None).unwrap();
        let mut config = Config::default();
        config.embedding.base_url = base;
        config.embedding.dims = Some(2);
        let provider = Provider::new(
            config.embedding.clone(),
            None,
            2,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        let options = Options {
            chunk_us: 2_000_000,
            overlap_us: 0,
            ..Default::default()
        };
        assert!(
            run(
                &episode,
                "primary",
                &sidecar,
                &workspace,
                &config,
                &provider,
                &options,
                None,
                None,
                &mut |_| {}
            )
            .await
            .is_err()
        );
        assert!(!usable(&sidecar, "primary", "primary", &config.space_id().unwrap()).unwrap());
        let status = crate::status::inspect(&workspace, None).unwrap();
        assert!(status.episodes[0].embedding_spaces.is_empty());
        assert!(!status.episodes[0].embeddings[0].complete);
        assert!(status.episodes[0].embeddings[0].error.is_some());
        assert_eq!(
            run(
                &episode,
                "primary",
                &sidecar,
                &workspace,
                &config,
                &provider,
                &options,
                None,
                None,
                &mut |_| {}
            )
            .await
            .unwrap(),
            2
        );
        assert_eq!(server.join().unwrap().len(), 6);
        let status = crate::status::inspect(&workspace, None).unwrap();
        assert_eq!(
            status.episodes[0].embedding_spaces,
            vec![config.space_id().unwrap()]
        );
        assert!(status.episodes[0].embeddings[0].complete);
        assert_eq!(
            status.space_details[&config.space_id().unwrap()],
            config.space_metadata().unwrap()
        );
        assert!(
            !serde_json::to_string(&status.space_details)
                .unwrap()
                .contains("api_key")
        );
        let space = config.space_id().unwrap();
        let state = state_path(&sidecar, "primary", "primary", &space);
        let parquet = sidecar.join("embeddings").join(format!("{space}.parquet"));
        let before_state = fs::read(&state).unwrap();
        let before_parquet = fs::read(&parquet).unwrap();
        let refresh = Options {
            recompute: true,
            ..options.clone()
        };
        assert!(
            run(
                &episode,
                "primary",
                &sidecar,
                &workspace,
                &config,
                &provider,
                &refresh,
                None,
                None,
                &mut |_| {}
            )
            .await
            .is_err()
        );
        assert_eq!(before_state, fs::read(&state).unwrap());
        assert_eq!(before_parquet, fs::read(&parquet).unwrap());
        assert!(usable(&sidecar, "primary", "primary", &space).unwrap());
        assert_eq!(
            super::super::lance::VectorIndex::open(&workspace, &space, 2, false)
                .await
                .unwrap()
                .count()
                .await
                .unwrap(),
            2
        );
        // The server has exited: another model request would fail this successful rebuild.
        fs::remove_dir_all(workspace.join("index")).unwrap();
        let without_index = crate::status::inspect(&workspace, None).unwrap();
        assert_eq!(without_index.space_details, status.space_details);
        assert!(!workspace.join("index").exists());
        assert_eq!(
            run(
                &episode,
                "primary",
                &sidecar,
                &workspace,
                &config,
                &provider,
                &options,
                None,
                None,
                &mut |_| {}
            )
            .await
            .unwrap(),
            2
        );
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
            2
        );
    }
}
