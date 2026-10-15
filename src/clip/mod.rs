//! Intent-driven short clips from one long video, selected on the transcript.
//!
//! See `docs/design/clip-command.md` for the behavioral baseline. Selection
//! reads the transcript (and cached screen text); no embeddings, vision calls
//! or index are involved. Only sixty-second audio windows reach the ASR
//! endpoint and only transcript text reaches the text endpoint.
pub mod brief;
pub mod captions;
pub mod export;
pub mod propose;
pub mod raster;
pub mod utterances;

use crate::{
    annotations::AnnotationFile,
    config::Config,
    episode::{Episode, Stream, TimeRange},
    events::{Event, EventSink},
    index::{discover, stations},
    providers::{Provider, RequestNotice},
    storage::{self, Checkpoints},
};
use anyhow::{Context, Result, ensure};
use brief::Brief;
use propose::{Angle, Dropped, Proposal};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

pub const DEFAULT_LIMIT: usize = 10;
pub const DEFAULT_MIN_US: i64 = 15_000_000;
pub const DEFAULT_MAX_US: i64 = 90_000_000;

/// A request the command cannot act on; the CLI reports it as invalid arguments.
#[derive(Debug)]
pub struct UsageError(pub String);
impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for UsageError {}
fn usage(message: impl Into<String>) -> anyhow::Error {
    UsageError(message.into()).into()
}

/// Which proposals `--export` renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    All,
    Numbers(Vec<usize>),
}
impl Selection {
    /// `all`, or comma-separated one-based numbers.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("all") {
            return Ok(Self::All);
        }
        let mut numbers = Vec::new();
        for part in text.split(',') {
            let number: usize = part
                .trim()
                .parse()
                .ok()
                .filter(|n| *n >= 1)
                .ok_or_else(|| {
                    usage(format!(
                        "--export takes all or proposal numbers such as 1,3,5; got {text}"
                    ))
                })?;
            if !numbers.contains(&number) {
                numbers.push(number);
            }
        }
        Ok(Self::Numbers(numbers))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    /// No crop; the clip keeps the source frame.
    Source,
    /// Centre crop to 9:16, scaled to 1080×1920.
    #[serde(rename = "9:16")]
    Portrait,
}
impl Aspect {
    pub fn parse(text: &str) -> Result<Self> {
        match text.trim() {
            "source" => Ok(Self::Source),
            "9:16" => Ok(Self::Portrait),
            other => Err(usage(format!(
                "--aspect must be source or 9:16; got {other}"
            ))),
        }
    }
}

#[derive(Clone)]
pub struct Options {
    /// Preset names or free text, in the order given.
    pub angles: Vec<String>,
    /// Free text passed through verbatim as one more angle.
    pub query: Option<String>,
    pub limit: usize,
    pub min_us: i64,
    pub max_us: i64,
    /// `None` proposes only.
    pub export: Option<Selection>,
    pub out: PathBuf,
    pub aspect: Aspect,
    pub captions: bool,
    pub credit: bool,
    pub credit_text: Option<String>,
    pub stream: String,
    /// Recognise screen text locally when it is missing; otherwise cached
    /// screen text is used when present and its absence is not an error.
    pub ocr: bool,
    /// Cache a still frame per proposal for terminal cards.
    pub preview: bool,
    pub jobs: usize,
    pub rpm: Option<u32>,
    pub recompute: bool,
    pub dry_run: bool,
    pub request_notice: Option<RequestNotice>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            angles: Vec::new(),
            query: None,
            limit: DEFAULT_LIMIT,
            min_us: DEFAULT_MIN_US,
            max_us: DEFAULT_MAX_US,
            export: None,
            out: PathBuf::from("clips"),
            aspect: Aspect::Source,
            captions: true,
            credit: true,
            credit_text: None,
            stream: "primary".into(),
            ocr: false,
            preview: false,
            jobs: 4,
            rpm: None,
            recompute: false,
            dry_run: false,
            request_notice: None,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.jobs > 0 && self.rpm != Some(0),
            usage("jobs and RPM must be positive")
        );
        ensure!(
            (1..=propose::MAX_LIMIT).contains(&self.limit),
            usage(format!(
                "--limit must be between 1 and {}",
                propose::MAX_LIMIT
            ))
        );
        ensure!(
            self.min_us >= 1_000_000 && self.min_us < self.max_us,
            usage("--min must be at least 1s and shorter than --max")
        );
        ensure!(
            self.angles.iter().all(|a| !a.trim().is_empty())
                && self.query.as_ref().is_none_or(|q| !q.trim().is_empty()),
            usage("angles and the query must not be empty")
        );
        ensure!(
            self.credit_text.as_ref().is_none_or(|t| {
                !t.trim().is_empty() && t.chars().count() <= 120 && !t.contains('\n')
            }),
            usage("--credit-text must be one line of at most 120 characters")
        );
        ensure!(
            !self.stream.trim().is_empty(),
            usage("--stream must name a stream")
        );
        Ok(())
    }
    /// Angles named on the command line, in order: presets and free text first,
    /// then the positional query.
    pub fn named_angles(&self) -> Vec<Angle> {
        let mut angles: Vec<Angle> = self.angles.iter().map(|a| Angle::parse(a)).collect();
        if let Some(query) = &self.query {
            angles.push(Angle::parse(query));
        }
        angles.dedup();
        angles
    }
}

/// What a dry run would send and where it would write.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    /// Audio that would go to the ASR endpoint; zero when the transcript is current.
    pub audio_minutes: f64,
    /// Transcript bytes that would go to the text endpoint across the brief and
    /// proposal calls, when a current transcript exists to measure.
    pub text_bytes: Option<u64>,
    pub transcript_cached: bool,
    /// Angle labels, or empty when the brief's suggestions would be used.
    pub angles: Vec<String>,
    pub out: PathBuf,
}

/// One rendered clip, or why it was not.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Export {
    pub number: usize,
    pub video: Option<PathBuf>,
    pub sidecar: Option<PathBuf>,
    pub subtitles: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub source: PathBuf,
    pub episode: String,
    pub stream: String,
    pub sidecar: PathBuf,
    pub dry_run: bool,
    pub plan: Option<Plan>,
    pub brief: Option<Brief>,
    pub angles: Vec<Angle>,
    pub proposals: Vec<Proposal>,
    pub dropped: Vec<Dropped>,
    pub cache_key: Option<String>,
    /// The proposals came from a saved run; no selection call was made.
    pub cached: bool,
    pub exports: Vec<Export>,
    pub out: Option<PathBuf>,
    pub partial: bool,
}

/// One structured generation, parsed and checked by the caller's rule. Invalid
/// output earns exactly one repair attempt that names what was wrong; a second
/// failure is the caller's error.
pub(crate) async fn generate_validated<T: DeserializeOwned>(
    provider: &Provider,
    prompt: &str,
    schema: &Value,
    validate: impl Fn(&T) -> Result<()>,
) -> Result<T> {
    let validate = &validate;
    let attempt = |prompt: String| async move {
        let value = provider.generate(&prompt, &[], schema.clone()).await?;
        let parsed: T = serde_json::from_value(value)?;
        validate(&parsed)?;
        Ok::<T, anyhow::Error>(parsed)
    };
    match attempt(prompt.to_owned()).await {
        Ok(value) => Ok(value),
        Err(error) => {
            if let Some(provider_error) = error.downcast_ref::<crate::providers::ProviderError>()
                && provider_error.kind != crate::providers::Failure::InvalidResponse
            {
                return Err(error);
            }
            crate::media::check_cancellation()?;
            let repair = format!(
                "{prompt}\n\nYour previous response was rejected: {error:#}. Return only valid output that follows every rule above."
            );
            attempt(repair).await
        }
    }
}

/// Where briefs and proposal runs live: derived from the sidecar, removed with it.
pub fn cache_directory(sidecar: &Path, stream: &str, episode: &Episode) -> PathBuf {
    stations::stream_directory(sidecar, stream, &episode.time.reference)
        .join(".internal")
        .join("clip")
}

/// A published annotation that is current for this episode and stream.
fn current_annotation(
    directory: &Path,
    name: &str,
    episode: &Episode,
    stream: &str,
    coverage: TimeRange,
) -> Option<AnnotationFile> {
    let file = AnnotationFile::read(&directory.join(format!("{name}.jsonl"))).ok()?;
    (file.header.name == name
        && file.header.episode == episode.episode_id
        && file.header.stream == stream
        && stations::has_current_input(episode, &file).unwrap_or(false)
        && file.validate_in_range(coverage, None).is_ok())
    .then_some(file)
}

/// One still frame at a proposal's start, cached like search previews.
fn preview(
    episode: &Episode,
    stream: &str,
    source: &Path,
    start_us: i64,
    workspace: &Path,
) -> Result<PathBuf> {
    let key = storage::cache_key(&(&episode.episode_id, stream, start_us, "preview"))?;
    let destination = workspace
        .join("cache")
        .join("previews")
        .join(format!("{key}.png"));
    if !destination.is_file() {
        crate::media::extract::still(
            source,
            episode.episode_to_source(stream, start_us)?,
            &destination,
            480,
        )?;
    }
    Ok(destination)
}

pub async fn run(
    path: &Path,
    workspace: &Path,
    config: &Config,
    options: &Options,
    cancel: CancellationToken,
    events: &mut dyn EventSink,
) -> Result<Report> {
    crate::diagnostics::run(
        workspace,
        "clip",
        options.dry_run,
        crate::media::with_cancellation(
            cancel.clone(),
            run_inner(path, workspace, config, options, cancel, events),
        ),
    )
    .await
}

async fn run_inner(
    path: &Path,
    workspace: &Path,
    config: &Config,
    options: &Options,
    cancel: CancellationToken,
    events: &mut dyn EventSink,
) -> Result<Report> {
    options.validate()?;
    config.validate()?;
    ensure!(
        path.is_file(),
        usage(format!(
            "clip takes exactly one video file; {} is not one",
            path.display()
        ))
    );
    crate::media::check_dependencies()?;
    let inputs = discover::discover(std::slice::from_ref(&path.to_path_buf()))?;
    let source = match inputs.as_slice() {
        [discover::Input::Video(source)] => source.clone(),
        _ => return Err(usage("clip takes exactly one ordinary video file")),
    };
    let episode = discover::ordinary_episode(&source)?;
    let stream = options.stream.clone();
    ensure!(
        matches!(episode.video(&stream), Ok(Stream::Video { .. })),
        usage(format!(
            "stream {stream} is not a video stream of this file"
        ))
    );
    let coverage = episode
        .video_coverage(&stream)?
        .context("stream has no coverage")?;
    let registry = discover::read_registry_all(workspace)?;
    ensure!(
        !registry
            .iter()
            .any(|r| r.pending_deletion && r.episode_id == episode.episode_id),
        "episode has pending cleanup; finish remove before clipping it again"
    );
    let _lock = if options.dry_run {
        None
    } else {
        Some(storage::WorkspaceLock::acquire(workspace)?)
    };
    let sidecar = if options.dry_run {
        discover::sidecar_path(&episode, &registry, None)?
    } else {
        discover::publish_episode(workspace, &episode, None)?
    };
    let directory = stations::stream_directory(&sidecar, &stream, &episode.time.reference);
    let cache = cache_directory(&sidecar, &stream, &episode);
    let mut asr = Provider::from_env(
        config.transcription.clone(),
        options.jobs,
        options.rpm,
        cancel.clone(),
    )?;
    asr.request_notice = options.request_notice.clone();
    let mut text_model = Provider::from_env(
        config.vision.clone(),
        options.jobs,
        options.rpm,
        cancel.clone(),
    )?;
    text_model.share_limits(&asr);
    text_model.request_notice = options.request_notice.clone();
    let mut report = Report {
        source: source.clone(),
        episode: episode.episode_id.clone(),
        stream: stream.clone(),
        sidecar: sidecar.clone(),
        dry_run: options.dry_run,
        plan: None,
        brief: None,
        angles: options.named_angles(),
        proposals: Vec::new(),
        dropped: Vec::new(),
        cache_key: None,
        cached: false,
        exports: Vec::new(),
        out: None,
        partial: false,
    };

    if options.dry_run {
        let pending = stations::transcript_pending_windows(
            &episode,
            &stream,
            &sidecar,
            &asr,
            options.recompute,
        )?;
        let transcript = current_annotation(&directory, "transcript", &episode, &stream, coverage);
        let screen = current_annotation(&directory, "screen_text", &episode, &stream, coverage);
        let text_bytes = match (&transcript, pending.is_empty() && !options.recompute) {
            (Some(transcript), true) => {
                let units = utterances::derive(Some(transcript), screen.as_ref())?;
                let listing = utterances::listing(&units).len() as u64;
                let chunks = brief::chunks(&units, brief::CHUNK_BYTES).len() as u64;
                let angles = report.angles.len().max(1) as u64;
                Some(listing * (chunks + angles))
            }
            _ => None,
        };
        report.plan = Some(Plan {
            audio_minutes: pending
                .iter()
                .map(|w| (w.end_us - w.start_us) as f64)
                .sum::<f64>()
                / 60_000_000.,
            text_bytes,
            transcript_cached: transcript.is_some() && pending.is_empty() && !options.recompute,
            angles: report.angles.iter().map(|a| a.label.clone()).collect(),
            out: options.out.clone(),
        });
        return Ok(report);
    }

    // Prepare: a current transcript, plus screen text when present or requested.
    ensure!(
        config.transcription.enabled != Some(false)
            || current_annotation(&directory, "transcript", &episode, &stream, coverage).is_some(),
        "speech transcription is disabled and no current transcript exists; clip selects on the transcript"
    );
    let transcript = stations::transcript(
        &episode,
        &stream,
        &sidecar,
        workspace,
        &asr,
        options.recompute,
        events,
    )
    .await?;
    let screen = if options.ocr {
        Some(stations::screen_text_with_workspace(
            &episode,
            &stream,
            &sidecar,
            workspace,
            options.recompute,
            options.jobs,
            events,
            &cancel,
        )?)
    } else {
        current_annotation(&directory, "screen_text", &episode, &stream, coverage)
    };
    let utterance_key = utterances::key(Some(&transcript), screen.as_ref())?;
    let units = utterances::load_or_derive(
        &Checkpoints::new(&sidecar),
        &utterance_key,
        Some(&transcript),
        screen.as_ref(),
        true,
    )?;
    ensure!(
        units.iter().any(|u| u.kind == utterances::Kind::Speech),
        "the transcript contains no speech to select from"
    );

    if let Some(selection) = &options.export {
        let run = propose::last(&cache).ok_or_else(|| {
            usage(format!(
                "no cached proposals for {}; run cerul clip on it without --export first",
                source.display()
            ))
        })?;
        ensure!(
            run.utterance_key == utterance_key,
            usage(format!(
                "the cached proposals for {} were made from an earlier transcript; run cerul clip on it without --export first",
                source.display()
            ))
        );
        let named = options.named_angles();
        ensure!(
            named.is_empty() || named == run.angles,
            usage(format!(
                "the cached proposals were made for angles {}; run cerul clip without --export to propose for other angles",
                run.angles
                    .iter()
                    .map(|a| a.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        );
        let numbers: Vec<usize> = match selection {
            Selection::All => run.proposals.iter().map(|p| p.number).collect(),
            Selection::Numbers(numbers) => numbers.clone(),
        };
        for number in &numbers {
            ensure!(
                run.proposals.iter().any(|p| p.number == *number),
                usage(format!(
                    "proposal {number} does not exist; the cached run has {} proposals",
                    run.proposals.len()
                ))
            );
        }
        report.brief = brief::cached(&cache, &run.brief_key, &units);
        report.angles = run.angles.clone();
        report.cache_key = Some(run.key.clone());
        report.cached = true;
        report.proposals = run.proposals.clone();
        report.dropped = run.dropped.clone();
        report.out = Some(options.out.clone());
        std::fs::create_dir_all(&options.out)
            .with_context(|| format!("create {}", options.out.display()))?;
        let mut fonts = raster::Fonts::load()?;
        events.emit(Event::Progress {
            episode: episode.episode_id.clone(),
            station: "export".into(),
            done: 0,
            total: numbers.len() as u64,
        });
        for (index, number) in numbers.iter().enumerate() {
            crate::media::check_cancellation()?;
            let proposal = run
                .proposals
                .iter()
                .find(|p| p.number == *number)
                .expect("numbers were checked");
            let job = export::Job {
                episode: &episode,
                stream: &stream,
                source: &source,
                proposal,
                units: &units,
                out: &options.out,
                aspect: options.aspect,
                captions: options.captions,
                credit: options.credit,
                credit_text: options.credit_text.as_deref(),
                transcript_model: transcript.header.model.clone(),
                selection_model: run.model.clone(),
            };
            let mut export = Export {
                number: *number,
                video: None,
                sidecar: None,
                subtitles: None,
                error: None,
            };
            match export::render(&job, &mut fonts) {
                Ok(published) => {
                    events.emit(Event::Published {
                        episode: episode.episode_id.clone(),
                        stream: stream.clone(),
                        annotation: "clip".into(),
                        records: 1,
                        path: published.video.clone(),
                    });
                    export.video = Some(published.video);
                    export.sidecar = Some(published.sidecar);
                    export.subtitles = Some(published.subtitles);
                }
                Err(error) => {
                    crate::media::check_cancellation()?;
                    report.partial = true;
                    export.error = Some(format!("{error:#}"));
                    events.emit(Event::Log {
                        level: "warn".into(),
                        msg: format!("clip {number} failed: {error:#}"),
                    });
                }
            }
            report.exports.push(export);
            events.emit(Event::Progress {
                episode: episode.episode_id.clone(),
                station: "export".into(),
                done: index as u64 + 1,
                total: numbers.len() as u64,
            });
        }
        if let Some(path) = fonts.cjk_path.as_deref() {
            events.emit(Event::Log {
                level: "info".into(),
                msg: format!("CJK captions use {path}"),
            });
        }
        crate::diagnostics::partial(report.partial);
        return Ok(report);
    }

    let brief_key = brief::key(&utterance_key, &text_model, brief::CHUNK_BYTES)?;
    let (brief, _) = brief::generate(
        &text_model,
        &units,
        &cache,
        &brief_key,
        brief::CHUNK_BYTES,
        options.recompute,
    )
    .await?;
    let mut angles = options.named_angles();
    if angles.is_empty() {
        angles = brief
            .suggested_angles
            .iter()
            .map(|a| Angle::suggested(&a.label, &a.query))
            .collect();
        angles.dedup();
    }
    ensure!(
        !angles.is_empty(),
        "the brief suggested no angles; name one with --angle or a query"
    );
    let key = propose::key(
        &utterance_key,
        &brief_key,
        &angles,
        options.limit,
        options.min_us,
        options.max_us,
        &text_model,
    )?;
    let (mut run, cached) = propose::run(
        &text_model,
        &units,
        &brief,
        &angles,
        options.limit,
        options.min_us,
        options.max_us,
        coverage,
        &cache,
        (&key, &utterance_key, &brief_key),
        options.recompute,
        events,
        &episode.episode_id,
    )
    .await?;
    if options.preview {
        for proposal in &mut run.proposals {
            crate::media::check_cancellation()?;
            // A missing preview must never fail a proposal; it is decoration.
            proposal.preview =
                preview(&episode, &stream, &source, proposal.start_us, workspace).ok();
        }
    }
    events.emit(Event::Log {
        level: "info".into(),
        msg: format!(
            "{} proposal{} for {}",
            run.proposals.len(),
            if run.proposals.len() == 1 { "" } else { "s" },
            source.display()
        ),
    });
    report.brief = Some(brief);
    report.angles = angles;
    report.cache_key = Some(key);
    report.cached = cached;
    report.proposals = run.proposals;
    report.dropped = run.dropped;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selections_and_aspects_parse_and_options_validate() {
        assert_eq!(Selection::parse("all").unwrap(), Selection::All);
        assert_eq!(
            Selection::parse("1, 3,5,3").unwrap(),
            Selection::Numbers(vec![1, 3, 5])
        );
        for bad in ["", "0", "1,x", "-1"] {
            let error = Selection::parse(bad).unwrap_err();
            assert!(error.downcast_ref::<UsageError>().is_some(), "{bad}");
        }
        assert_eq!(Aspect::parse("9:16").unwrap(), Aspect::Portrait);
        assert!(Aspect::parse("4:3").is_err());
        assert_eq!(
            serde_json::to_value(Aspect::Portrait).unwrap(),
            serde_json::json!("9:16")
        );
        let mut options = Options::default();
        options.validate().unwrap();
        options.limit = 26;
        assert!(options.validate().is_err());
        options.limit = 5;
        options.min_us = 100_000_000;
        assert!(options.validate().is_err());
        options.min_us = 15_000_000;
        options.angles = vec!["numbers".into(), "numbers".into()];
        options.query = Some("pricing".into());
        let angles = options.named_angles();
        assert_eq!(angles.len(), 2);
        assert_eq!(angles[1].query.as_deref(), Some("pricing"));
    }
}
