//! Explicit video analysis, independent of search indexing and embodied labels.
pub mod focused;

use crate::{
    annotations::{AnnotationFile, Record},
    config::Config,
    events::{Event, EventSink},
    index::{discover, pipeline, stations, understanding},
    providers::{Provider, RequestNotice},
    storage,
};
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Options {
    pub prompt: Option<String>,
    pub images: Vec<PathBuf>,
    pub from_us: Option<i64>,
    pub to_us: Option<i64>,
    pub stream: bool,
    pub streams: String,
    pub only: Option<String>,
    pub jobs: usize,
    pub rpm: Option<u32>,
    pub recompute: bool,
    pub dry_run: bool,
    pub request_notice: Option<RequestNotice>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            prompt: None,
            images: Vec::new(),
            from_us: None,
            to_us: None,
            stream: false,
            streams: "primary".into(),
            only: None,
            jobs: 4,
            rpm: None,
            recompute: false,
            dry_run: false,
            request_notice: None,
        }
    }
}
impl Options {
    pub fn focused(&self) -> bool {
        self.prompt.is_some()
            || !self.images.is_empty()
            || self.from_us.is_some()
            || self.to_us.is_some()
            || self.stream
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.prompt
                .as_ref()
                .is_none_or(|p| !p.trim().is_empty() && p.len() <= 32_000),
            "prompt must contain 1..32000 bytes"
        );
        ensure!(
            self.images.len() <= 4,
            "at most four reference images are supported"
        );
        ensure!(
            self.from_us.is_none_or(|t| t >= 0) && self.to_us.is_none_or(|t| t >= 0),
            "timestamps must be nonnegative"
        );
        ensure!(
            self.from_us.zip(self.to_us).is_none_or(|(a, b)| a < b),
            "from must precede to"
        );
        for path in &self.images {
            let metadata = std::fs::metadata(path)?;
            ensure!(
                metadata.is_file() && metadata.len() <= 10 * 1024 * 1024,
                "reference image must be a file no larger than 10 MB"
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StreamResult {
    pub episode: String,
    pub source: PathBuf,
    pub stream: String,
    pub sidecar: PathBuf,
    pub range_us: Option<[i64; 2]>,
    pub response: Option<focused::Response>,
    pub cached: bool,
    pub scenes: Vec<Record>,
    pub sections: Vec<Record>,
    pub summary: Option<Record>,
    pub errors: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub streams: Vec<StreamResult>,
    pub partial: bool,
    pub dry_run: bool,
    pub streaming: bool,
}

pub async fn run(
    paths: &[PathBuf],
    workspace: &Path,
    config: &Config,
    options: &Options,
    cancel: CancellationToken,
    events: &mut dyn EventSink,
) -> Result<Report> {
    crate::diagnostics::run(
        workspace,
        "analyze",
        options.dry_run,
        crate::media::with_cancellation(cancel.clone(), async {
            options.validate()?;
            config.validate()?;
            ensure!(
                options.jobs > 0 && options.rpm != Some(0),
                "jobs and RPM must be positive"
            );
            ensure!(
                config.vision.enabled != Some(false),
                "analysis requires an enabled vision endpoint"
            );
            crate::media::check_dependencies()?;
            let mut episodes = Vec::new();
            for input in discover::discover(paths)? {
                match input {
                    discover::Input::Video(path) => {
                        episodes.push(discover::ordinary_episode(&path)?)
                    }
                    discover::Input::LeRobot(root) => {
                        episodes.extend(crate::lerobot::read_with_workspace(&root, workspace)?)
                    }
                }
            }
            episodes.retain(|e| pipeline::selected(e, options.only.as_deref()));
            ensure!(!episodes.is_empty(), "no matching videos");
            if options.focused() {
                for episode in &episodes {
                    for stream in pipeline::streams(episode, &options.streams)? {
                        focused::scope(episode, &stream, options)?;
                    }
                }
            }
            let _lock = if options.dry_run {
                None
            } else {
                Some(storage::WorkspaceLock::acquire(workspace)?)
            };
            let registry = discover::read_registry_all(workspace)?;
            ensure!(
                !registry
                    .iter()
                    .any(|r| r.pending_deletion
                        && episodes.iter().any(|e| e.episode_id == r.episode_id)),
                "episode has pending cleanup; finish clean before analyzing it again"
            );
            let mut provider = Provider::from_env(
                config.vision.clone(),
                options.jobs,
                options.rpm,
                cancel.clone(),
            )?;
            provider.request_notice = options.request_notice.clone();
            let mut report = Report {
                streams: Vec::new(),
                partial: false,
                dry_run: options.dry_run,
                streaming: options.stream,
            };
            for episode in episodes {
                crate::media::check_cancellation()?;
                let streams = pipeline::streams(&episode, &options.streams)?;
                let sidecar = if options.dry_run {
                    discover::sidecar_path(&episode, &registry, None)?
                } else {
                    discover::publish_episode(workspace, &episode, None)?
                };
                for stream in streams {
                    let mut result = StreamResult {
                        episode: episode.episode_id.clone(),
                        source: episode.source.root.clone(),
                        stream: stream.clone(),
                        sidecar: sidecar.clone(),
                        range_us: if options.focused() {
                            Some(focused::scope(&episode, &stream, options)?)
                        } else {
                            None
                        },
                        response: None,
                        cached: false,
                        scenes: Vec::new(),
                        sections: Vec::new(),
                        summary: None,
                        errors: Vec::new(),
                    };
                    if let crate::episode::Stream::Video { path, .. } = episode.video(&stream)? {
                        result.source = episode.source.root.join(path);
                    }
                    if !options.dry_run {
                        let directory =
                            stations::stream_directory(&sidecar, &stream, &episode.time.reference);
                        let evidence = |name: &str| -> Option<AnnotationFile> {
                            let file =
                                AnnotationFile::read(&directory.join(format!("{name}.jsonl")))
                                    .ok()?;
                            let coverage = episode.video_coverage(&stream).ok()??;
                            (file.header.name == name
                                && file.header.episode == episode.episode_id
                                && file.header.stream == stream
                                && stations::has_current_input(&episode, &file).unwrap_or(false)
                                && file.validate_in_range(coverage, None).is_ok())
                            .then_some(file)
                        };
                        let transcript = evidence("transcript");
                        let screen = evidence("screen_text");
                        // Detailed station events remain available to JSON clients; human output
                        // keeps one spinner until the complete stream result is ready.
                        events.emit(Event::Log {
                            level: "info".into(),
                            msg: format!("Analyzing {}", result.source.display()),
                        });
                        if options.focused() {
                            match focused::run(
                                &episode,
                                &stream,
                                &sidecar,
                                workspace,
                                &provider,
                                options,
                                transcript.as_ref(),
                                screen.as_ref(),
                                events,
                            )
                            .await
                            {
                                Ok((response, cached)) => {
                                    result.response = Some(response);
                                    result.cached = cached;
                                }
                                Err(error) => {
                                    crate::media::check_cancellation()?;
                                    result.errors.push(format!("{error:#}"));
                                }
                            }
                        } else {
                            match understanding::run(
                                &episode,
                                &stream,
                                &sidecar,
                                workspace,
                                &provider,
                                options.recompute,
                                transcript.as_ref(),
                                screen.as_ref(),
                                events,
                            )
                            .await
                            {
                                Ok(product) => {
                                    result.scenes = product.scenes.records;
                                    result.summary =
                                        product.summary.and_then(|f| f.records.into_iter().next());
                                    result.errors = product.errors;
                                    if result.summary.is_some() {
                                        result.sections = AnnotationFile::read(
                                            &directory.join("semantic.section.jsonl"),
                                        )?
                                        .records;
                                    }
                                }
                                Err(error) => {
                                    crate::media::check_cancellation()?;
                                    result.errors.push(format!("{error:#}"));
                                }
                            }
                        }
                        report.partial |= !result.errors.is_empty();
                    }
                    report.streams.push(result);
                }
            }
            crate::diagnostics::partial(report.partial);
            Ok(report)
        }),
    )
    .await
}
