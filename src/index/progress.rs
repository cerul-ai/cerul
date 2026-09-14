//! Whole-run indexing progress. Timing history is an optional, rebuildable hint;
//! counters only advance on observed work, never merely because time passed.
use super::pipeline::{Options, streams};
use crate::{
    config::Config,
    episode::{Episode, Stream},
    events::{Event, EventSink},
    storage,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};

struct Stage {
    episode: String,
    stream: String,
    station: &'static str,
    group: usize,
    dependencies: Option<Vec<usize>>,
    units: f64,
    weight: f64,
    fraction: f64,
    step: f64,
    started: Option<Instant>,
    measured_end: Option<Instant>,
    observed: bool,
    sample_elapsed: f64,
    finished: bool,
}
#[derive(Default, Serialize, Deserialize)]
struct History {
    seconds_per_unit: BTreeMap<String, f64>,
}

pub(super) struct RunProgress<'a> {
    sink: &'a mut dyn EventSink,
    stages: Vec<Stage>,
    history: History,
    history_path: PathBuf,
    sources: BTreeMap<String, PathBuf>,
    latest: String,
    latest_phase: String,
    high_water: u64,
}
impl<'a> RunProgress<'a> {
    pub fn new(
        episodes: &[Episode],
        workspace: &Path,
        config: &Config,
        options: &Options,
        sink: &'a mut dyn EventSink,
    ) -> Result<Self> {
        // Store only a hash: no credentials, URLs, media paths, or request bodies.
        let profile = storage::cache_key(&(
            "index-timing/3",
            std::env::consts::ARCH,
            std::env::consts::OS,
            (
                options.no_ocr,
                options.no_audio,
                options.no_understanding,
                options.embedding.skip_still,
            ),
            options.jobs,
            options.rpm,
            options.embedding.chunk_us,
            options.embedding.overlap_us,
            [&config.embedding, &config.vision, &config.transcription]
                .map(|e| (&e.kind, &e.model, &e.base_url, e.dims)),
        ))?;
        let history_path = workspace
            .join("runtime")
            .join(format!("index-timing-{profile}.json"));
        let history: History = std::fs::read(&history_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let mut result = Self {
            sink,
            stages: Vec::new(),
            history,
            history_path,
            sources: episodes
                .iter()
                .map(|episode| {
                    Ok((
                        episode.episode_id.clone(),
                        episode.video(&episode.time.reference)?.path().to_owned(),
                    ))
                })
                .collect::<Result<_>>()?,
            latest: String::new(),
            latest_phase: "preparing".into(),
            high_water: 0,
        };
        let mut group = 0;
        for episode in episodes {
            result.add(&episode.episode_id, "", "prepare", group, 1., 8.);
            group += 1;
            for stream in streams(episode, &options.streams)? {
                let Stream::Video {
                    range_us, probe, ..
                } = episode.video(&stream)?
                else {
                    unreachable!()
                };
                let duration = range_us[1] - range_us[0];
                let seconds = duration as f64 / 1_000_000.;
                if !options.no_ocr {
                    result.add(
                        &episode.episode_id,
                        &stream,
                        "screen_text",
                        group,
                        (seconds / 5.).ceil().max(1.),
                        0.8,
                    );
                }
                if !options.no_audio && probe.has_audio {
                    result.add(
                        &episode.episode_id,
                        &stream,
                        "transcript",
                        group,
                        (seconds / 60.).ceil().max(1.),
                        20. / (options.jobs as f64).min((seconds / 60.).ceil().max(1.)),
                    );
                }
                group += 1;
                let windows = crate::media::chunks(
                    duration,
                    options.embedding.chunk_us,
                    options.embedding.overlap_us,
                )?
                .len()
                .max(1) as f64;
                result.add(
                    &episode.episode_id,
                    &stream,
                    "embed",
                    group,
                    windows,
                    12. / (options.jobs as f64).min(windows),
                );
                group += 1;
                if !options.no_understanding && config.vision.enabled != Some(false) {
                    let windows = (seconds / 30.).ceil().max(1.);
                    result.add(
                        &episode.episode_id,
                        &stream,
                        "understanding",
                        group,
                        windows,
                        25. / (options.jobs as f64).min(windows),
                    );
                    group += 1;
                    result.add(
                        &episode.episode_id,
                        &stream,
                        "overview",
                        group,
                        1.,
                        30. + windows * 3.,
                    );
                    // A long summary is not just one more short scene request.
                    let len = result.stages.len();
                    result.stages[len - 1].weight = result.stages[len - 1]
                        .weight
                        .max(result.stages[len - 2].weight / 3.);
                    group += 1;
                    result.add(
                        &episode.episode_id,
                        &stream,
                        "description",
                        group,
                        windows,
                        4. / (options.jobs as f64).min(windows),
                    );
                    group += 1;
                }
            }
        }
        result.add("", "", "finalize", group, 1., 3.);
        result.link_dependencies();
        result.emit_overall(false, false);
        Ok(result)
    }
    fn add(
        &mut self,
        episode: &str,
        stream: &str,
        station: &'static str,
        group: usize,
        units: f64,
        fallback: f64,
    ) {
        let rate = self
            .history
            .seconds_per_unit
            .get(station)
            .copied()
            .filter(|v| v.is_finite() && (0.05..=600.).contains(v))
            .unwrap_or(fallback);
        self.stages.push(Stage {
            episode: episode.into(),
            stream: stream.into(),
            station,
            group,
            dependencies: None,
            units,
            weight: (rate * units).max(1.),
            fraction: 0.,
            step: 1. / units.max(1.),
            started: None,
            measured_end: None,
            observed: false,
            sample_elapsed: 0.,
            finished: false,
        });
    }
    fn link_dependencies(&mut self) {
        for i in 0..self.stages.len() {
            let stage = &self.stages[i];
            let dependencies = self.stages[..i]
                .iter()
                .enumerate()
                .filter_map(|(j, before)| {
                    let same = before.episode == stage.episode && before.stream == stage.stream;
                    let required = match stage.station {
                        "prepare" | "finalize" => true,
                        "embed" => same && matches!(before.station, "screen_text" | "transcript"),
                        "overview" => {
                            same && matches!(
                                before.station,
                                "screen_text" | "transcript" | "understanding"
                            )
                        }
                        "description" => same && before.station == "understanding",
                        _ => false,
                    };
                    // Different streams/episodes publish in order. Within a stream,
                    // only actual inputs constrain when work can start.
                    (required || before.episode != stage.episode || before.stream != stage.stream)
                        .then_some(j)
                })
                .collect();
            self.stages[i].dependencies = Some(dependencies);
        }
    }
    pub fn begin(&mut self, episode: &str, stream: &str, station: &str) {
        if let Some(stage) = self.stages.iter_mut().find(|s| {
            s.episode == episode && s.stream == stream && s.station == station && !s.finished
        }) {
            stage.started = Some(Instant::now());
            self.latest = episode.into();
            self.latest_phase = station.into();
            self.emit_overall(false, false);
        }
    }
    pub fn end(&mut self, episode: &str, stream: &str, station: &str, successful: bool) {
        if let Some(stage) = self.stages.iter_mut().find(|s| {
            s.episode == episode
                && s.stream == stream
                && s.station == station
                && (!s.finished || (successful && s.observed && s.measured_end.is_some()))
        }) {
            // Unmeasured reuse, failed and cancelled work must not train estimates.
            if successful
                && stage.observed
                && let Some(started) = stage.started
            {
                let seconds = stage
                    .measured_end
                    .unwrap_or_else(Instant::now)
                    .duration_since(started)
                    .as_secs_f64();
                if seconds >= 1. {
                    let sample = (seconds / stage.units).clamp(0.05, 600.);
                    let old = self
                        .history
                        .seconds_per_unit
                        .get(station)
                        .copied()
                        .filter(|v| v.is_finite() && (0.05..=600.).contains(v))
                        .unwrap_or(sample);
                    self.history
                        .seconds_per_unit
                        .insert(station.into(), old * 0.7 + sample * 0.3);
                }
            }
            if successful {
                stage.observed = false; // A deferred successful end may train this stage only once.
            }
            stage.fraction = 1.;
            stage.finished = true;
            self.emit_overall(false, false);
        }
    }
    fn snapshot(&self) -> (u64, f64, String) {
        let total: f64 = self.stages.iter().map(|s| s.weight).sum();
        let done: f64 = self.stages.iter().map(|s| s.weight * s.fraction).sum();
        let mut groups: BTreeMap<usize, f64> = BTreeMap::new();
        let mut active = Vec::new();
        let mut ends = vec![0_f64; self.stages.len()];
        for (index, stage) in self.stages.iter().enumerate() {
            if stage.finished {
                continue;
            }
            let elapsed = stage
                .started
                .map(|t| t.elapsed().as_secs_f64())
                .unwrap_or(0.);
            let predicted = if stage.fraction > 0. && stage.sample_elapsed >= 0.5 {
                stage.sample_elapsed / stage.fraction
            } else {
                self.history
                    .seconds_per_unit
                    .get(stage.station)
                    .copied()
                    .filter(|v| v.is_finite() && (0.05..=600.).contains(v))
                    .map(|rate| (rate * stage.units).max(1.))
                    .unwrap_or(stage.weight)
            };
            // Measured work revises the rough prior; time alone never advances
            // the completion bar. Parallel OCR/ASR use their maximum.
            let left = if stage.started.is_some() {
                (predicted - elapsed).max(predicted * 0.15)
            } else {
                predicted
            };
            if let Some(dependencies) = &stage.dependencies {
                ends[index] = left + dependencies.iter().map(|&i| ends[i]).fold(0_f64, f64::max);
            }
            groups
                .entry(stage.group)
                .and_modify(|n| *n = n.max(left))
                .or_insert(left);
            if stage.started.is_some() {
                active.push(stage.station);
            }
        }
        let phase = if active.is_empty() {
            self.latest_phase.clone()
        } else {
            active.join("+")
        };
        (
            ((done / total.max(1.) * 10_000.).floor() as u64).min(9999),
            if self.stages.iter().all(|s| s.dependencies.is_some()) {
                ends.into_iter().fold(1_f64, f64::max)
            } else {
                groups.values().sum::<f64>().max(1.)
            },
            phase,
        )
    }
    fn emit_overall(&mut self, finished: bool, partial: bool) {
        let (done, eta_seconds, phase) = self.snapshot();
        self.high_water = self.high_water.max(done);
        let total: f64 = self.stages.iter().map(|s| s.weight).sum();
        let ceiling: f64 = self
            .stages
            .iter()
            .map(|s| {
                s.weight
                    * if s.started.is_some() && !s.finished {
                        (s.fraction + s.step).min(1.)
                    } else {
                        s.fraction
                    }
            })
            .sum();
        self.sink.emit(Event::IndexProgress {
            episode: self.latest.clone(),
            source: self.sources.get(&self.latest).cloned(),
            phase,
            done: if finished && !partial {
                10_000
            } else {
                self.high_water
            },
            total: 10_000,
            ceiling: if finished && !partial {
                10_000
            } else {
                ((ceiling / total.max(1.) * 10_000.) as u64)
                    .max(self.high_water)
                    .min(9999)
            },
            eta_seconds: if finished { 0. } else { eta_seconds },
            finished,
        });
    }
    pub fn finish(&mut self, partial: bool) {
        self.emit_overall(true, partial);
        // Timing hints may be lost without affecting authoritative products.
        if !self.history.seconds_per_unit.is_empty() {
            let _ = storage::write_json(&self.history_path, &self.history);
        }
    }
}
impl EventSink for RunProgress<'_> {
    fn emit(&mut self, event: Event) {
        if let Event::Checkpoint {
            episode,
            station,
            window,
            total,
        } = &event
            && station == "understanding"
            && let Some(stage) = self.stages.iter_mut().find(|s| {
                s.episode == *episode && s.station == station && s.started.is_some() && !s.finished
            })
        {
            // Only wholly fresh scene work can calibrate model latency.
            stage.observed = *window == *total && *total > 0;
        }
        if let Event::Progress {
            episode,
            station,
            done: 0,
            ..
        } = &event
            && station == "overview"
            && let Some(stage) = self.stages.iter().find(|s| {
                s.episode == *episode
                    && s.station == "understanding"
                    && !s.finished
                    && s.started.is_some()
            })
        {
            let stream = stage.stream.clone();
            // Only the whole product can confirm scene success, so do not train
            // timings from this boundary (it can also follow retained failures).
            self.end(episode, &stream, "understanding", false);
            self.begin(episode, &stream, "overview");
        }
        if let Event::Progress {
            episode,
            station,
            done,
            total,
        } = &event
            && let Some(stage) = self.stages.iter_mut().find(|s| {
                s.episode == *episode && s.station == station && s.started.is_some() && !s.finished
            })
        {
            if station != "understanding" {
                stage.observed = true;
            }
            let total = if station == "understanding" {
                total.saturating_sub(1).max(1)
            } else {
                (*total).max(1)
            };
            stage.step = 1. / total as f64;
            let fraction = if total > 0 {
                *done as f64 / total as f64
            } else {
                1.
            }
            .min(0.99);
            if fraction > stage.fraction {
                stage.fraction = fraction;
                stage.sample_elapsed = stage
                    .started
                    .map(|time| time.elapsed().as_secs_f64())
                    .unwrap_or(0.);
            }
            if *done >= total {
                stage.measured_end = Some(Instant::now());
            }
        }
        self.sink.emit(event);
        self.emit_overall(false, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn empty<'a>(sink: &'a mut dyn EventSink, path: PathBuf) -> RunProgress<'a> {
        RunProgress {
            sink,
            stages: Vec::new(),
            history: History::default(),
            history_path: path,
            sources: BTreeMap::new(),
            latest: String::new(),
            latest_phase: "preparing".into(),
            high_water: 0,
        }
    }
    #[test]
    fn aggregate_is_monotonic_across_streams_and_parallel_eta_uses_the_maximum() {
        let dir = tempfile::tempdir().unwrap();
        let mut events = Vec::new();
        let mut sink = |e| events.push(e);
        let mut run = empty(&mut sink, dir.path().join("history.json"));
        run.add("one", "front", "screen_text", 0, 1., 10.);
        run.add("one", "front", "transcript", 0, 1., 20.);
        run.add("one", "side", "embed", 1, 1., 12.);
        run.add("two", "primary", "embed", 2, 1., 12.);
        run.add("", "", "finalize", 3, 1., 3.);
        assert_eq!(run.snapshot().1, 47.); // 20 + 12 + 12 + 3, not 10 + 20 + ...
        run.begin("one", "front", "screen_text");
        run.begin("one", "front", "transcript");
        run.emit(Event::Progress {
            episode: "one".into(),
            station: "screen_text".into(),
            done: 1,
            total: 2,
        });
        run.end("one", "front", "screen_text", true);
        run.end("one", "front", "transcript", true);
        run.begin("one", "side", "embed");
        run.emit(Event::Progress {
            episode: "one".into(),
            station: "embed".into(),
            done: 0,
            total: 1,
        });
        run.end("one", "side", "embed", true);
        run.begin("two", "primary", "embed");
        run.end("two", "primary", "embed", true);
        run.begin("", "", "finalize");
        run.end("", "", "finalize", true);
        assert!(run.snapshot().0 < 10_000);
        run.finish(false);
        drop(run);
        let progress: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::IndexProgress {
                    done,
                    eta_seconds,
                    finished,
                    ..
                } => Some((*done, *eta_seconds, *finished)),
                _ => None,
            })
            .collect();
        assert!(progress.windows(2).all(|p| p[1].0 >= p[0].0));
        assert!(
            progress
                .iter()
                .filter(|p| !p.2)
                .all(|p| p.1.is_finite() && p.1 > 0.)
        );
        assert_eq!(progress.last().unwrap(), &(10_000, 0., true));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Progress { done: 1, .. }))
        );
        assert!(!dir.path().join("history.json").exists()); // Cache-only / unmeasured work.
    }
    #[test]
    fn history_uses_successful_measurements_and_partial_runs_do_not_claim_completion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut events = Vec::new();
        let mut sink = |e| events.push(e);
        let mut run = empty(&mut sink, path.clone());
        for (i, name) in ["embed", "transcript", "understanding"].iter().enumerate() {
            run.add("one", "front", name, i, 2., 12.);
        }
        run.add("two", "front", "embed", 3, 2., 12.);
        run.begin("one", "front", "embed");
        run.stages[0].started = Some(Instant::now() - Duration::from_secs(8));
        let prior = run.snapshot().1;
        run.emit(Event::Progress {
            episode: "one".into(),
            station: "embed".into(),
            done: 1,
            total: 2,
        });
        assert!(run.snapshot().1 < prior);
        run.emit(Event::Progress {
            episode: "one".into(),
            station: "embed".into(),
            done: 2,
            total: 2,
        });
        run.end("one", "front", "embed", true);
        run.begin("one", "front", "transcript");
        run.stages[1].started = Some(Instant::now() - Duration::from_secs(8));
        run.emit(Event::Progress {
            episode: "one".into(),
            station: "transcript".into(),
            done: 2,
            total: 2,
        });
        run.end("one", "front", "transcript", false);
        run.begin("one", "front", "understanding");
        run.stages[2].started = Some(Instant::now() - Duration::from_secs(8));
        run.end("one", "front", "understanding", true); // No work events: do not train from reuse.
        // The next video's ETA learns immediately; its percentage weight stays fixed.
        assert!((run.snapshot().1 - 8.).abs() < 0.2);
        assert_eq!(run.stages[3].weight, 24.);
        run.finish(true);
        drop(run);
        let history: History = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(history.seconds_per_unit.len(), 1);
        assert!((history.seconds_per_unit["embed"] - 4.).abs() < 0.1);
        assert!(
            matches!(events.last(), Some(Event::IndexProgress { done, finished: true, .. }) if *done < 10_000)
        );
        let mut sink = |_: Event| {};
        let mut next = empty(&mut sink, path);
        next.history = history;
        next.add("other", "front", "embed", 0, 2., 12.);
        assert!((next.snapshot().1 - 8.).abs() < 0.2);
    }
    #[test]
    fn scene_completion_reserves_overview_work_and_bounds_estimates() {
        let dir = tempfile::tempdir().unwrap();
        let mut events = Vec::new();
        let mut sink = |e| events.push(e);
        let mut run = empty(&mut sink, dir.path().join("timings.json"));
        run.add("mori", "primary", "understanding", 0, 9., 25.);
        run.add("mori", "primary", "overview", 1, 1., 75.);
        run.add("mori", "primary", "description", 2, 9., 4.);
        run.add("", "", "finalize", 3, 1., 3.);
        run.begin("mori", "primary", "understanding");
        run.emit(Event::Progress {
            episode: "mori".into(),
            station: "understanding".into(),
            done: 9,
            total: 10,
        });
        run.emit(Event::Progress {
            episode: "mori".into(),
            station: "overview".into(),
            done: 0,
            total: 1,
        });
        assert!(run.stages[0].finished);
        assert!(run.stages[1].started.is_some());
        drop(run);
        let Some(Event::IndexProgress {
            phase,
            done,
            ceiling,
            total,
            ..
        }) = events.last()
        else {
            panic!("missing aggregate progress")
        };
        assert_eq!(phase, "overview");
        assert!(*done < total * 3 / 4);
        assert!(*ceiling > *done && *ceiling < *total);
    }

    #[test]
    fn successful_product_trains_scene_time_after_overview_boundary_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = |_: Event| {};
        let mut run = empty(&mut sink, dir.path().join("history.json"));
        run.add("one", "front", "understanding", 0, 2., 25.);
        run.add("one", "front", "overview", 1, 1., 30.);
        run.begin("one", "front", "understanding");
        run.stages[0].started = Some(Instant::now() - Duration::from_secs(8));
        run.emit(Event::Checkpoint {
            episode: "one".into(),
            station: "understanding".into(),
            window: 2,
            total: 2,
        });
        run.emit(Event::Progress {
            episode: "one".into(),
            station: "understanding".into(),
            done: 2,
            total: 3,
        });
        run.emit(Event::Progress {
            episode: "one".into(),
            station: "overview".into(),
            done: 0,
            total: 1,
        });
        assert!(run.history.seconds_per_unit.is_empty());
        // Publication confirms success later; only the scene interval is learned.
        run.end("one", "front", "understanding", true);
        let learned = run.history.seconds_per_unit["understanding"];
        assert!((learned - 4.).abs() < 0.2);
        run.end("one", "front", "understanding", true);
        assert_eq!(run.history.seconds_per_unit["understanding"], learned);
        run.add("two", "front", "understanding", 2, 2., 25.);
        assert!((run.stages[2].weight - 8.).abs() < 0.4);
    }

    #[test]
    fn cached_scene_progress_does_not_train_model_latency() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = |_: Event| {};
        let mut run = empty(&mut sink, dir.path().join("history.json"));
        run.add("one", "front", "understanding", 0, 2., 25.);
        run.begin("one", "front", "understanding");
        run.stages[0].started = Some(Instant::now() - Duration::from_secs(8));
        run.emit(Event::Progress {
            episode: "one".into(),
            station: "understanding".into(),
            done: 2,
            total: 3,
        });
        run.end("one", "front", "understanding", true);
        assert!(run.history.seconds_per_unit.is_empty());
    }

    #[test]
    fn parallel_eta_follows_dependencies_instead_of_summing_stages() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = |_: Event| {};
        let mut run = empty(&mut sink, dir.path().join("history.json"));
        for (i, (stage, seconds)) in [
            ("prepare", 8.),
            ("screen_text", 10.),
            ("transcript", 20.),
            ("embed", 12.),
            ("understanding", 40.),
            ("overview", 30.),
            ("description", 10.),
        ]
        .into_iter()
        .enumerate()
        {
            run.add(
                "one",
                if stage == "prepare" { "" } else { "front" },
                stage,
                i,
                1.,
                seconds,
            );
        }
        run.add("", "", "finalize", 7, 1., 3.);
        run.link_dependencies();
        assert_eq!(run.snapshot().1, 81.); // prepare + max(text->embed, scenes->overview/description) + save
        run.end("one", "", "prepare", true);
        run.end("one", "front", "screen_text", true);
        run.end("one", "front", "transcript", true);
        assert_eq!(run.snapshot().1, 73.);
    }

    #[test]
    fn plan_omits_disabled_work_and_namespaces_history_by_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("video.mp4");
        crate::media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=16x16:rate=1:duration=1",
                    "-c:v",
                    "libx264",
                ])
                .arg(&video),
        )
        .unwrap();
        let episode = super::super::discover::ordinary_episode(&video).unwrap();
        let mut sink = |_: Event| {};
        let mut config = Config::default();
        let options = Options {
            no_ocr: true,
            no_audio: true,
            no_understanding: true,
            ..Default::default()
        };
        let first = RunProgress::new(
            std::slice::from_ref(&episode),
            dir.path(),
            &config,
            &options,
            &mut sink,
        )
        .unwrap();
        assert_eq!(
            first.stages.iter().map(|s| s.station).collect::<Vec<_>>(),
            ["prepare", "embed", "finalize"]
        );
        assert!(first.snapshot().1 > 0.);
        let first_path = first.history_path.clone();
        drop(first);
        config.embedding.model = "different-model".into();
        let second =
            RunProgress::new(&[episode], dir.path(), &config, &options, &mut sink).unwrap();
        assert_ne!(first_path, second.history_path);
        assert!(!second.history_path.exists()); // Planning itself does not save timings.
    }
}
