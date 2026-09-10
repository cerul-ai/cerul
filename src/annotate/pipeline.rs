//! Command orchestration keeps independent semantic modules recoverable.
use super::semantic;
use crate::{
    annotations::{AnnotationFile, Header, Model, Record, SEMANTIC_ITEMS, VERBS},
    config::Config,
    episode::Episode,
    events::{Event, EventSink},
    index::{
        discover::{self, Input},
        pipeline::{selected, streams},
        records::RecordIndex,
        stations::stream_directory,
    },
    providers::{Failure, Provider, ProviderError, RequestNotice},
    storage,
};
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Options {
    pub items: Vec<String>,
    pub write_lerobot: bool,
    pub out: Option<PathBuf>,
    pub streams: String,
    pub only: Option<String>,
    pub ontology: Option<PathBuf>,
    pub window_us: i64,
    pub fps: f64,
    pub recompute: bool,
    pub dry_run: bool,
    pub jobs: usize,
    pub rpm: Option<u32>,
    pub request_notice: Option<RequestNotice>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            write_lerobot: false,
            out: None,
            streams: "primary".into(),
            only: None,
            ontology: None,
            window_us: 30_000_000,
            fps: 2.,
            recompute: false,
            dry_run: false,
            jobs: 4,
            rpm: None,
            request_notice: None,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.out.is_none() || self.write_lerobot,
            "--out requires --write-lerobot"
        );
        ensure!(
            !self.write_lerobot
                || self.items.is_empty()
                || self.items.iter().any(|i| i == "subtask"),
            "writeback requires semantic subtask"
        );
        ensure!(
            self.items
                .iter()
                .all(|item| SEMANTIC_ITEMS.contains(&item.as_str())),
            "unknown semantic item"
        );
        ensure!(
            self.window_us > 5_000_000 && self.window_us <= 300_000_000,
            "semantic window must be in (5s,5m]"
        );
        ensure!(
            self.fps.is_finite() && self.fps > 0. && self.fps <= 10.,
            "semantic FPS must be in (0,10]"
        );
        ensure!(
            (self.window_us as f64 / 1_000_000. * self.fps).ceil() <= 600.,
            "semantic window and FPS must produce at most 600 contact-sheet frames"
        );
        ensure!(
            self.jobs > 0 && self.rpm != Some(0),
            "jobs and RPM must be positive"
        );
        ontology(self, false)?;
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModuleResult {
    pub episode: String,
    pub stream: String,
    pub annotation: String,
    pub records: usize,
    pub complete: bool,
    pub error: Option<String>,
    /// What this result belongs to: the video file, or the dataset root when the
    /// episode came from one. Episodes of a dataset share their MP4 shards, so a
    /// media file name alone cannot tell two results apart; the dataset and the
    /// episode index can.
    #[serde(default)]
    pub source: PathBuf,
    /// True when the episode came from a LeRobot dataset.
    #[serde(default)]
    pub dataset: bool,
    /// Where the published annotation file is, once validation and publication
    /// succeeded. A checkpoint on disk is not a published file, so an incomplete
    /// or dry-run module has no path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WritebackResult {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub complete: bool,
    pub error: Option<String>,
}
/// Why a run stopped early, in the terms that decide what to change about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetryReason {
    /// A provider limited the request rate, so the retry lowers it.
    RateLimit,
    /// Work remains for another reason; the retry repeats the command unchanged.
    Incomplete,
}
/// The command that continues unfinished work: the original invocation with
/// only the failure's fix applied, so running it is always safe. Published work
/// is reused and nothing is recomputed on purpose.
///
/// The result carries this so that a machine reader finds recovery in the same
/// object as the failure. Only the binary can fill it in, because only the
/// binary sees an argument list.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Retry {
    pub argv: Vec<String>,
    pub reason: RetryReason,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub modules: Vec<ModuleResult>,
    pub writebacks: Vec<WritebackResult>,
    pub partial: bool,
    pub dry_run: bool,
    /// Present when work remains. Absent on a complete or dry run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<Retry>,
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
fn ontology(options: &Options, dataset: bool) -> Result<Option<BTreeSet<String>>> {
    if let Some(path) = &options.ontology {
        let text = fs::read_to_string(path)?;
        let words = if path.extension().is_some_and(|ext| ext == "json")
            || text.trim_start().starts_with(['[', '{'])
        {
            serde_json::from_str::<Vec<String>>(&text)?
        } else {
            text.lines()
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.starts_with('#'))
                .map(str::to_owned)
                .collect()
        };
        ensure!(
            !words.is_empty() && words.iter().all(|word| !word.trim().is_empty()),
            "ontology must contain nonempty verbs"
        );
        return Ok(Some(words.into_iter().collect()));
    }
    Ok(dataset.then(|| VERBS.iter().map(|s| s.to_string()).collect()))
}
/// Generated conflicts are projected alongside model flags without becoming model input.
pub(crate) fn publish_conflicts(episode: &Episode, stream: &str, sidecar: &Path) -> Result<()> {
    let directory = stream_directory(sidecar, stream, &episode.time.reference);
    let mut conflicts: Vec<Record> = Vec::new();
    for item in SEMANTIC_ITEMS {
        let path = directory.join(format!("semantic.{item}.conflicts.json"));
        let annotation_path = directory.join(format!("semantic.{item}.jsonl"));
        if !path.is_file() || !annotation_path.is_file() {
            continue;
        }
        let product: semantic::Product = serde_json::from_slice(&fs::read(path)?)?;
        let annotation = AnnotationFile::read(&annotation_path)?;
        if annotation.header.stream != stream
            || !crate::index::stations::has_current_input(episode, &annotation)?
        {
            continue;
        }
        ensure!(
            annotation.header.episode == episode.episode_id && annotation.header.stream == stream,
            "conflict source provenance mismatch"
        );
        let mut model_annotation = annotation.clone();
        model_annotation
            .records
            .retain(|record| !record.id.starts_with("window-conflict-"));
        if storage::cache_key(&model_annotation)? != storage::cache_key(&product.annotation)? {
            // A torn module publication must not overwrite the last coherent flags.
            // Re-running that module repairs its pair from completed checkpoints.
            return Ok(());
        }
        for mut conflict in product.conflicts {
            conflict.id = format!("window-conflict-{item}-{}", conflicts.len());
            conflicts.push(conflict);
        }
    }
    let path = directory.join("semantic.flag.jsonl");
    let mut existing = if path.is_file() {
        Some(AnnotationFile::read(&path)?)
    } else {
        None
    };
    if let Some(file) = &existing
        && (file.header.stream != stream
            || !crate::index::stations::has_current_input(episode, file)?)
    {
        existing = None;
    }
    let replace_stale = existing.is_none();
    let mut file = if let Some(file) = existing {
        file
    } else {
        if conflicts.is_empty() {
            return Ok(());
        }
        AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "semantic.flag".into(),
                episode: episode.episode_id.clone(),
                stream: stream.into(),
                model: Model {
                    kind: "cerul".into(),
                    name: "window-reconciliation/1".into(),
                    base_url: None,
                },
                params: json!({}),
                created: chrono::Utc::now().to_rfc3339(),
                cerul_version: env!("CARGO_PKG_VERSION").into(),
                input_hash: crate::index::stations::station_key(
                    episode,
                    stream,
                    "semantic.flag",
                    &json!({}),
                )?,
                record_schema: "semantic.flag/1".into(),
            },
            records: Vec::new(),
        }
    };
    ensure!(
        file.header.episode == episode.episode_id && file.header.stream == stream,
        "flag provenance mismatch"
    );
    file.validate(episode.duration_us()?, None)?;
    let before = serde_json::to_vec(&file.records)?;
    file.records
        .retain(|record| !record.id.starts_with("window-conflict-"));
    file.records.extend(conflicts);
    file.records
        .sort_by(|a, b| (a.start_us, a.end_us, &a.id).cmp(&(b.start_us, b.end_us, &b.id)));
    if before != serde_json::to_vec(&file.records)? || replace_stale {
        if file.header.model.kind == "cerul" {
            file.header.input_hash = crate::index::stations::station_key(
                episode,
                stream,
                "semantic.flag",
                &file.header.params,
            )?;
        }
        file.publish(&path, episode.duration_us()?, None)?;
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
    options.validate()?;
    config.validate()?;
    interrupted(&cancel)?;
    let mut episodes = Vec::new();
    for input in discover::discover(paths)? {
        match input {
            Input::Video(path) => {
                let episode = discover::ordinary_episode(&path)?;
                if selected(&episode, options.only.as_deref()) {
                    episodes.push(episode);
                }
            }
            Input::LeRobot(root) => {
                if options.write_lerobot
                    && !options.dry_run
                    && root.join(".cerul/writeback").exists()
                {
                    crate::lerobot::transaction::recover(&root)?;
                }
                episodes.extend(
                    crate::lerobot::read_with_workspace(&root, workspace)?
                        .into_iter()
                        .filter(|episode| selected(episode, options.only.as_deref())),
                );
            }
        }
    }
    ensure!(!episodes.is_empty(), "no matching episodes");
    let mut expected = std::collections::BTreeMap::<PathBuf, usize>::new();
    if options.write_lerobot {
        for episode in &episodes {
            if episode.source.format != "lerobot/3.1" {
                return Err(ProviderError { kind: Failure::Unsupported, message: "writeback requires an existing LeRobot v3.1 dataset; Cerul does not upgrade datasets".into() }.into());
            }
            ensure!(
                streams(episode, &options.streams)?.contains(&episode.time.reference),
                "writeback requires the primary camera"
            );
            *expected.entry(episode.source.root.clone()).or_default() += 1;
        }
        ensure!(
            options.out.is_none() || expected.len() == 1,
            "--out requires exactly one source dataset"
        );
        if let Some(out) = &options.out {
            crate::lerobot::writer::validate_output(expected.keys().next().unwrap(), out)?;
        }
    }
    let mut ready = std::collections::BTreeMap::<PathBuf, Vec<(Episode, AnnotationFile)>>::new();
    let _lock = if options.dry_run {
        None
    } else {
        Some(storage::WorkspaceLock::acquire(workspace)?)
    };
    let mut provider = Provider::from_env(
        config.vision.clone(),
        options.jobs,
        options.rpm,
        cancel.clone(),
    )?;
    provider.request_notice = options.request_notice.clone();
    let mut report = Report {
        modules: Vec::new(),
        writebacks: Vec::new(),
        partial: false,
        dry_run: options.dry_run,
        retry: None,
    };
    for episode in episodes {
        let dataset = episode.source.format.starts_with("lerobot/");
        let vocabulary = ontology(options, dataset)?;
        let mut items = if options.items.is_empty() {
            if dataset {
                SEMANTIC_ITEMS.iter().map(|s| s.to_string()).collect()
            } else {
                vec!["task".into(), "subtask".into(), "flag".into()]
            }
        } else {
            options.items.clone()
        };
        items.sort();
        items.dedup();
        let selected_streams = streams(&episode, &options.streams)?;
        let sidecar = if options.dry_run {
            PathBuf::new()
        } else {
            discover::publish_episode(workspace, &episode, None)?
        };
        for stream in selected_streams {
            for item in &items {
                interrupted(&cancel)?;
                let mut result = ModuleResult {
                    episode: episode.episode_id.clone(),
                    stream: stream.clone(),
                    annotation: format!("semantic.{item}"),
                    records: 0,
                    complete: false,
                    error: None,
                    // Stream paths are relative to the episode's root, and two
                    // directories can hold the same file name, so the root has to
                    // stay on the front of it.
                    source: match dataset {
                        true => episode.source.root.clone(),
                        false => episode
                            .video(&episode.time.reference)
                            .ok()
                            .and_then(|stream| match stream {
                                crate::episode::Stream::Video { path, .. } => {
                                    Some(episode.source.root.join(path))
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| episode.source.root.clone()),
                    },
                    dataset,
                    path: None,
                };
                if !options.dry_run {
                    match semantic::run(
                        &episode,
                        &stream,
                        item,
                        &sidecar,
                        workspace,
                        &provider,
                        &semantic::Options {
                            window_us: options.window_us,
                            fps: options.fps,
                            recompute: options.recompute,
                            ontology: vocabulary.clone(),
                        },
                        events,
                    )
                    .await
                    {
                        Ok(product) => {
                            result.complete = true;
                            result.records = product.annotation.records.len();
                            let published =
                                stream_directory(&sidecar, &stream, &episode.time.reference)
                                    .join(format!("semantic.{item}.jsonl"));
                            events.emit(Event::Published {
                                episode: episode.episode_id.clone(),
                                stream: stream.clone(),
                                annotation: result.annotation.clone(),
                                records: result.records as u64,
                                path: published.clone(),
                            });
                            result.path = Some(published);
                            if options.write_lerobot
                                && item == "subtask"
                                && stream == episode.time.reference
                            {
                                ready
                                    .entry(episode.source.root.clone())
                                    .or_default()
                                    .push((episode.clone(), product.annotation));
                            }
                        }
                        Err(error) => {
                            interrupted(&cancel)?;
                            if error.downcast_ref::<ProviderError>().is_some_and(|e| {
                                matches!(e.kind, Failure::MissingKey | Failure::Unsupported)
                            }) {
                                return Err(error);
                            }
                            report.partial = true;
                            result.error = Some(error.to_string());
                            events.emit(Event::Log {
                                level: "error".into(),
                                msg: format!(
                                    "{} {stream} semantic.{item}: {error}",
                                    episode.episode_id
                                ),
                            });
                        }
                    }
                }
                report.modules.push(result);
            }
            if !options.dry_run {
                publish_conflicts(&episode, &stream, &sidecar)?;
            }
        }
    }
    for (root, count) in expected {
        interrupted(&cancel)?;
        let destination = options.out.clone().unwrap_or_else(|| root.clone());
        let mut result = WritebackResult {
            source: root.clone(),
            destination: destination.clone(),
            complete: false,
            error: None,
        };
        if !options.dry_run {
            let products = ready.remove(&root).unwrap_or_default();
            let outcome = if products.len() != count {
                Err(anyhow::anyhow!(
                    "writeback skipped because selected primary-camera subtasks are incomplete"
                ))
            } else {
                let assignments: Vec<_> = products
                    .iter()
                    .map(|(episode, subtasks)| crate::lerobot::writer::Assignment {
                        episode,
                        subtasks,
                    })
                    .collect();
                if options.out.is_some() {
                    crate::lerobot::writer::write_out(&root, &destination, &assignments, |_| {
                        interrupted(&cancel)
                    })
                } else {
                    crate::lerobot::transaction::write_in_place(&root, &assignments, |_| {
                        interrupted(&cancel)
                    })
                }
            };
            match outcome {
                Ok(()) => result.complete = true,
                Err(error) => {
                    interrupted(&cancel)?;
                    report.partial = true;
                    result.error = Some(error.to_string());
                }
            }
        }
        report.writebacks.push(result);
    }
    if !options.dry_run {
        let mut spaces = BTreeSet::from([config.space_id()?]);
        if let Ok(entries) = fs::read_dir(workspace.join("index")) {
            for entry in entries {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type()?.is_dir()
                    && name.len() == 64
                    && name.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    spaces.insert(name);
                }
            }
        }
        for space in spaces {
            RecordIndex::rebuild(workspace, &space).await?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_contact_sheet_overflow_before_processing() {
        let mut options = Options {
            window_us: 300_000_000,
            fps: 10.,
            ..Default::default()
        };
        assert!(options.validate().is_err());
        options.fps = 2.;
        options.validate().unwrap();
        options.fps = 2.001;
        assert!(options.validate().is_err());
    }
    fn response(value: serde_json::Value) -> serde_json::Value {
        json!({"candidates":[{"content":{"parts":[{"text":value.to_string()}]}}]})
    }
    #[tokio::test]
    async fn dataset_subtasks_write_out_and_cached_in_place_without_new_calls() {
        let (base, server) = crate::providers::tests::server(vec![
            (200, response(json!({"ok":true}))),
            (200, response(json!({"description":"Move the cup."}))),
            (
                200,
                response(
                    json!({"records":[{"start_us":0,"end_us":4000000,"confidence":0.9,"text":"Move the cup","index":0}]}),
                ),
            ),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("dataset");
        crate::lerobot::tests::fixture(&root, "v3.1");
        let workspace = dir.path().join("workspace");
        let output = dir.path().join("output");
        let source = root.join("data/chunk-000/file-000.parquet");
        let before = fs::read(&source).unwrap();
        let mut config = Config::default();
        config.vision.base_url = base;
        let mut options = Options {
            items: vec!["subtask".into()],
            only: Some("0".into()),
            write_lerobot: true,
            out: Some(output.clone()),
            ..Default::default()
        };
        let valid_output = options.out.clone();
        options.out = Some(root.join("new-parent/output"));
        let error = run(
            std::slice::from_ref(&root),
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("outside the source"));
        assert!(!workspace.exists());
        assert!(!root.join("new-parent").exists());
        options.out = valid_output;
        let report = run(
            std::slice::from_ref(&root),
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(!report.partial, "{:?}", report.writebacks);
        assert!(report.writebacks[0].complete);
        assert_eq!(fs::read(&source).unwrap(), before);
        assert_eq!(server.join().unwrap().len(), 3);
        options.out = None;
        let report = run(
            std::slice::from_ref(&root),
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(!report.partial, "{:?}", report.writebacks);
        assert!(report.writebacks[0].complete);
        assert_ne!(fs::read(&source).unwrap(), before);
        assert!(!root.join(".cerul/writeback").exists());
        let rows =
            crate::lerobot::json_rows(&crate::lerobot::parquet_batches(&source).unwrap()).unwrap();
        assert_eq!(rows[0]["language_persistent"][0]["content"], "Move the cup");
        assert!(rows[8]["language_persistent"].is_null());
        let info_path = root.join("meta/info.json");
        let mut info: serde_json::Value =
            serde_json::from_slice(&fs::read(&info_path).unwrap()).unwrap();
        info["codebase_version"] = json!("v3.0");
        storage::write_json(&info_path, &info).unwrap();
        let error = run(
            &[root],
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
    }
    #[tokio::test]
    async fn independent_modules_publish_and_resume_without_repeating_success() {
        let (base, server) = crate::providers::tests::server(vec![
            (200, response(json!({"ok":true}))),
            (
                200,
                response(
                    json!({"records":[{"start_us":0,"end_us":2000000,"confidence":null,"kind":"","note":"invalid flag"}]}),
                ),
            ),
            (
                200,
                response(
                    json!({"records":[{"start_us":0,"end_us":2000000,"confidence":null,"text":"Move the cup"}]}),
                ),
            ),
            (200, response(json!({"records":[]}))),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        crate::media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=2:duration=2",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let workspace = dir.path().join("workspace");
        let mut config = Config::default();
        config.vision.base_url = base;
        let options = Options {
            items: vec!["task".into(), "flag".into()],
            ..Default::default()
        };
        let paths = vec![source];
        let first = run(
            &paths,
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(first.partial);
        assert!(
            first
                .modules
                .iter()
                .any(|m| m.annotation == "semantic.task" && m.complete)
        );
        assert!(
            first
                .modules
                .iter()
                .any(|m| m.annotation == "semantic.flag" && !m.complete)
        );
        assert_eq!(
            RecordIndex::open(&workspace, &config.space_id().unwrap())
                .await
                .unwrap()
                .read(None)
                .await
                .unwrap()
                .len(),
            1
        );
        let second = run(
            &paths,
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(!second.partial);
        assert!(second.modules.iter().all(|m| m.complete));
        assert_eq!(server.join().unwrap().len(), 4);
        let third = run(
            &paths,
            &workspace,
            &config,
            &options,
            CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(!third.partial);
    }
}
