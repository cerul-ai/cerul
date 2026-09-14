//! Optional embodied-only local hand annotation with durable temporal checkpoints.
use super::hand_model::{Detector, HAND_HASH, PALM_HASH};
use crate::{
    annotations::{AnnotationFile, Header, Model, Record},
    episode::{Episode, Stream, TimeRange},
    events::{Event, EventSink},
    index::stations::{station_key, stream_directory},
    media::{self, extract::SourceRange},
    storage::{self, Checkpoints},
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const NAME: &str = "grounding.hand";
const RECIPE: &str = "mediapipe-onnx-hands/1";
const CHUNK_US: i64 = 5_000_000;

pub(crate) fn draw(image: &mut image::RgbaImage, frame: &Frame, width: u32, height: u32) {
    const BONES: [(usize, usize); 21] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 4),
        (0, 5),
        (5, 6),
        (6, 7),
        (7, 8),
        (5, 9),
        (9, 10),
        (10, 11),
        (11, 12),
        (9, 13),
        (13, 14),
        (14, 15),
        (15, 16),
        (13, 17),
        (0, 17),
        (17, 18),
        (18, 19),
        (19, 20),
    ];
    for hand in &frame.hands {
        let color = match hand.handedness {
            Side::Left => image::Rgba([42, 210, 230, 255]),
            Side::Right => image::Rgba([255, 193, 70, 255]),
        };
        let points = hand
            .keypoints
            .map(|p| p.map(|[x, y]| (x * (width - 1) as f32, y * (height - 1) as f32)));
        for (a, b) in BONES {
            if let (Some(a), Some(b)) = (points[a], points[b]) {
                imageproc::drawing::draw_line_segment_mut(image, a, b, color);
            }
        }
        for (x, y) in points.into_iter().flatten() {
            imageproc::drawing::draw_filled_circle_mut(
                image,
                (x.round() as i32, y.round() as i32),
                3,
                color,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hand {
    pub track_id: u64,
    pub handedness: Side,
    /// Classification confidence for the side, not keypoint accuracy.
    pub handedness_score: f32,
    /// Model hand-presence confidence; per-joint visibility is unavailable.
    pub confidence: f32,
    /// MediaPipe order: wrist, thumb, index, middle, ring, little finger.
    /// Display-image normalized XY. Out-of-frame/nonfinite predictions are null.
    pub keypoints: [Option<[f32; 2]>; 21],
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub frame_width: u32,
    pub frame_height: u32,
    /// Empty means no accepted detection in this observed frame.
    pub hands: Vec<Hand>,
}
impl Frame {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.frame_width > 0 && self.frame_height > 0 && self.hands.len() <= 2,
            "invalid hand frame"
        );
        let mut ids = BTreeSet::new();
        for hand in &self.hands {
            ensure!(
                hand.track_id > 0 && ids.insert(hand.track_id),
                "invalid hand track ID"
            );
            ensure!(
                [hand.confidence, hand.handedness_score]
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "invalid hand confidence"
            );
            ensure!(
                hand.keypoints
                    .iter()
                    .flatten()
                    .flatten()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "invalid hand keypoint"
            );
        }
        Ok(())
    }
    pub fn from_record(record: &Record) -> Result<Self> {
        let frame: Self = serde_json::from_value(serde_json::to_value(&record.fields)?)?;
        frame.validate()?;
        Ok(frame)
    }
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Tracker {
    next_id: u64,
    previous: Vec<(i64, Hand)>,
}
impl Tracker {
    fn assign(&mut self, time: i64, hands: &mut [Hand]) {
        self.previous
            .retain(|(t, _)| time >= *t && time - *t <= 500_000);
        let mut pairs = Vec::new();
        for (i, hand) in hands.iter().enumerate() {
            for (j, (_, old)) in self.previous.iter().enumerate() {
                if let (Some(a), Some(b)) = (hand.keypoints[0], old.keypoints[0]) {
                    let distance = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
                    if distance < 0.2 {
                        let cost = distance
                            + if hand.handedness == old.handedness {
                                0.
                            } else {
                                0.1
                            };
                        pairs.push((cost, i, j));
                    }
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut used = BTreeSet::new();
        for (_, i, j) in pairs {
            if hands[i].track_id == 0 && used.insert(j) {
                hands[i].track_id = self.previous[j].1.track_id;
            }
        }
        for hand in hands {
            if hand.track_id == 0 {
                self.next_id += 1;
                hand.track_id = self.next_id;
            }
            self.previous
                .retain(|(_, old)| old.track_id != hand.track_id);
            self.previous.push((time, hand.clone()));
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Checkpoint {
    records: Vec<Record>,
    tracker: Tracker,
    digest: String,
}

pub struct Plan {
    pub source: PathBuf,
    pub times: Vec<i64>,
    pub coverage: TimeRange,
}
pub fn plan(episode: &Episode, stream: &str) -> Result<Plan> {
    let Stream::Video { path, .. } = episode.video(stream)? else {
        unreachable!()
    };
    let source = episode.source.root.join(path);
    let coverage = episode
        .video_coverage(stream)?
        .context("no hand stream coverage")?;
    let start = episode.episode_to_source(stream, coverage.start_us)?;
    let end = episode.episode_to_source(stream, coverage.end_us)?;
    ensure!(
        end - start == coverage.end_us - coverage.start_us,
        "hand annotation does not support scaled camera timelines"
    );
    let times: Vec<_> = media::frame_pts(&source)?
        .into_iter()
        .filter(|t| *t >= start && *t < end)
        .collect();
    ensure!(
        !times.is_empty() && times.windows(2).all(|p| p[0] < p[1]),
        "hand annotation requires strictly ordered video timestamps"
    );
    Ok(Plan {
        source,
        times,
        coverage,
    })
}
pub fn run(
    episode: &Episode,
    stream: &str,
    sidecar: &Path,
    recompute: bool,
    events: &mut dyn EventSink,
) -> Result<AnnotationFile> {
    let plan = plan(episode, stream)?;
    let directory = stream_directory(sidecar, stream, &episode.time.reference);
    let params = json!({"recipe":RECIPE,"palm_sha256":PALM_HASH,"hand_sha256":HAND_HASH,"max_hands":2,"sampling":"every_observed_frame","max_edge":1080,"coordinates":"display_normalized_xy","handedness":"model_unmirrored_display","chunk_us":CHUNK_US});
    let hash = station_key(episode, stream, NAME, &params)?;
    let total = plan.times.len() as u64 + 1;
    let mut done = 0;
    let mut cached = 0;
    let emit = |events: &mut dyn EventSink, phase: &str, done, cached| {
        events.emit(Event::AnnotationProgress {
            episode: episode.episode_id.clone(),
            phase: format!("{stream} · hands · {phase}"),
            done,
            total,
            cached,
        })
    };
    emit(events, "preparing", done, cached);
    if !recompute {
        let path = super::layout::annotation(&directory, NAME);
        if let Ok(file) = AnnotationFile::read(&path)
            && file.header.input_hash == hash
            && file.header.params == params
            && file.header.name == NAME
            && file.header.stream == stream
            && file.header.episode == episode.episode_id
            && file.records.len() == plan.times.len()
            && file.validate_in_range(plan.coverage, None).is_ok()
            && file
                .records
                .iter()
                .zip(&plan.times)
                .all(|(r, t)| episode.source_to_episode(stream, *t).ok() == Some(r.start_us))
        {
            emit(events, "cached publication", total, total - 1);
            return Ok(file);
        }
    }
    let checkpoints = Checkpoints::semantic(&directory);
    let mut tracker = Tracker::default();
    let mut detector = None;
    let mut records = Vec::new();
    let source_end = episode.episode_to_source(stream, plan.coverage.end_us)?;
    let mut index = 0;
    while index < plan.times.len() {
        media::check_cancellation()?;
        let first = index;
        let start = plan.times[first];
        while index < plan.times.len() && plan.times[index] < start + CHUNK_US {
            index += 1;
        }
        let end = plan.times.get(index).copied().unwrap_or(source_end);
        let key = storage::cache_key(&(&hash, start, end, &tracker))?;
        let previous_count = records.len();
        let loaded = if recompute {
            None
        } else {
            checkpoints.load::<Checkpoint>(&key)?
        };
        if let Some(checkpoint) = loaded.filter(|c| {
            c.records.len() == index - first
                && storage::cache_key(&(&c.records, &c.tracker)).ok().as_ref() == Some(&c.digest)
                && c.records
                    .iter()
                    .zip(&plan.times[first..index])
                    .all(|(r, t)| {
                        episode.source_to_episode(stream, *t).ok() == Some(r.start_us)
                            && Frame::from_record(r).is_ok()
                    })
        }) {
            tracker = checkpoint.tracker;
            records.extend(checkpoint.records);
            done += (index - first) as u64;
            cached += (index - first) as u64;
            emit(events, "cached frames", done, cached);
            continue;
        }
        emit(events, "decoding", done, cached);
        let stage = tempfile::tempdir()?;
        let frames = media::extract::observed_frames(
            &plan.source,
            SourceRange::new(start, end)?,
            stage.path(),
        )?;
        ensure!(
            frames.len() == index - first
                && frames
                    .iter()
                    .zip(&plan.times[first..index])
                    .all(|((t, _), p)| start + t == *p),
            "decoded hand frames differ from source timestamps"
        );
        if detector.is_none() {
            detector = Some(Detector::new()?);
        }
        for (offset, (_, path)) in frames.iter().enumerate() {
            media::check_cancellation()?;
            let image = image::open(path)?.to_rgb8();
            let time = episode.source_to_episode(stream, plan.times[first + offset])?;
            let end = episode.source_to_episode(
                stream,
                plan.times
                    .get(first + offset + 1)
                    .copied()
                    .unwrap_or(source_end),
            )?;
            let mut hands = detector.as_ref().unwrap().infer(&image)?;
            tracker.assign(time, &mut hands);
            let frame = Frame {
                frame_width: image.width(),
                frame_height: image.height(),
                hands,
            };
            frame.validate()?;
            records.push(Record {
                id: format!("hand-frame-{time}"),
                start_us: time,
                end_us: end,
                confidence: None,
                fields: serde_json::from_value(serde_json::to_value(frame)?)?,
            });
            done += 1;
            emit(events, "local CPU", done, cached);
        }
        let chunk = records[previous_count..].to_vec();
        let digest = storage::cache_key(&(&chunk, &tracker))?;
        checkpoints.save(
            &key,
            &Checkpoint {
                records: chunk,
                tracker: tracker.clone(),
                digest,
            },
        )?;
    }
    let file = AnnotationFile {
        header: Header {
            schema: "annotation/1".into(),
            name: NAME.into(),
            episode: episode.episode_id.clone(),
            stream: stream.into(),
            model: Model {
                kind: "local".into(),
                name: "mediapipe-handpose-opencv-2023feb".into(),
                base_url: None,
            },
            params,
            created: chrono::Utc::now().to_rfc3339(),
            cerul_version: env!("CARGO_PKG_VERSION").into(),
            input_hash: hash,
            record_schema: format!("{NAME}/1"),
        },
        records,
    };
    file.publish_in_range(
        &directory.join(".internal/annotations/grounding.hand.jsonl"),
        plan.coverage,
        None,
    )?;
    emit(events, "published", total, cached);
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hand(x: f32, side: Side) -> Hand {
        Hand {
            track_id: 0,
            handedness: side,
            handedness_score: 0.9,
            confidence: 0.9,
            keypoints: [Some([x, 0.5]); 21],
        }
    }
    #[test]
    fn association_survives_detection_order_changes_but_expires_missing_tracks() {
        let mut tracker = Tracker::default();
        let mut first = vec![hand(0.2, Side::Left), hand(0.8, Side::Right)];
        tracker.assign(0, &mut first);
        let mut second = vec![hand(0.78, Side::Right), hand(0.23, Side::Left)];
        tracker.assign(33_333, &mut second);
        assert_eq!(second[0].track_id, first[1].track_id);
        assert_eq!(second[1].track_id, first[0].track_id);
        let mut late = vec![hand(0.23, Side::Left)];
        tracker.assign(1_000_000, &mut late);
        assert_ne!(late[0].track_id, first[0].track_id);
    }

    #[tokio::test]
    async fn real_cpu_hands_resume_publish_render_and_rebuild_without_endpoints() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("hand.mp4");
        media::run(
            media::command("ffmpeg")
                .args(["-v", "error", "-loop", "1", "-framerate", "1", "-i"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hand-opencv.png"))
                .args([
                    "-t",
                    "7",
                    "-vf",
                    "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(&source),
        )
        .unwrap();
        let workspace = directory.path().join("workspace");
        let mut config = crate::config::Config::default();
        config.vision.base_url = "http://127.0.0.1:1".into();
        config.vision.api_key_env = "CERUL_HAND_TEST_NO_KEY".into();
        let options = super::super::pipeline::Options {
            embodied: true,
            hands: true,
            no_semantic: true,
            ..Default::default()
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let trigger = cancel.clone();
        let result = super::super::pipeline::run(
            std::slice::from_ref(&source),
            &workspace,
            &config,
            &options,
            cancel,
            &mut |event| {
                if let Event::AnnotationProgress { done: 6, .. } = event {
                    trigger.cancel();
                }
            },
        )
        .await;
        assert!(result.is_err());
        let sidecar = source.with_extension("mp4.cerul");
        assert!(!super::super::layout::annotation(&sidecar, NAME).exists());
        let mut events = Vec::new();
        let report = super::super::pipeline::run(
            std::slice::from_ref(&source),
            &workspace,
            &config,
            &options,
            tokio_util::sync::CancellationToken::new(),
            &mut |e| events.push(e),
        )
        .await
        .unwrap();
        assert!(!report.partial);
        assert_eq!(report.modules[0].records, 7);
        assert!(
            events
                .iter()
                .all(|e| !matches!(e, Event::ModelRequest { .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::AnnotationProgress { cached: 5, .. }))
        );
        let file = AnnotationFile::read(report.modules[0].path.as_ref().unwrap()).unwrap();
        let frame = Frame::from_record(&file.records[0]).unwrap();
        assert!(
            !frame.hands.is_empty(),
            "real hand fixture must be detected"
        );
        let hand = &frame.hands[0];
        assert!(hand.keypoints.iter().all(Option::is_some));
        let wrist = hand.keypoints[0].unwrap();
        let middle = hand.keypoints[12].unwrap();
        assert!(
            middle[1] < wrist[1],
            "upright hand must retain image orientation"
        );
        let before = std::fs::read(&report.exports[0].annotations).unwrap();
        let mut cached_events = Vec::new();
        super::super::pipeline::run(
            std::slice::from_ref(&source),
            &workspace,
            &config,
            &options,
            tokio_util::sync::CancellationToken::new(),
            &mut |e| cached_events.push(e),
        )
        .await
        .unwrap();
        assert_eq!(
            before,
            std::fs::read(&report.exports[0].annotations).unwrap()
        );
        assert!(
            cached_events
                .iter()
                .any(|e| matches!(e, Event::AnnotationProgress { cached: 7, .. }))
        );
        let bundle = super::super::video::load(&source, &workspace).unwrap();
        assert_eq!(bundle.annotations.len(), 1);
        let output = directory.path().join("rendered.mp4");
        let rendered = super::super::video::render(
            &source,
            &workspace,
            &output,
            None,
            true,
            false,
            &tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(rendered.generation, bundle.generation);
        assert_eq!(
            media::frame_pts(&source).unwrap(),
            media::frame_pts(&output).unwrap()
        );
        assert_eq!(
            media::probe(&output).unwrap().duration_us,
            media::probe(&source).unwrap().duration_us,
            "the last hand frame must retain its display duration"
        );
        assert!(
            crate::index::records::sidecars(&workspace)
                .unwrap()
                .is_empty()
        );
        // Identical input instances must each export one copy of the hand track.
        let duplicate = directory.path().join("duplicate.mp4");
        std::fs::copy(&source, &duplicate).unwrap();
        let repeated = super::super::pipeline::run(
            &[source.clone(), duplicate],
            &workspace,
            &config,
            &options,
            tokio_util::sync::CancellationToken::new(),
            &mut |_: Event| {},
        )
        .await
        .unwrap();
        assert_eq!(repeated.exports.len(), 2);
        for export in &repeated.exports {
            let bundle: super::super::export::Bundle =
                serde_json::from_slice(&std::fs::read(&export.annotations).unwrap()).unwrap();
            assert_eq!(bundle.annotations.len(), 1);
            assert_eq!(bundle.annotations[0].records.len(), 7);
        }
        // A missing semantic key must not discard completed local hands.
        let combined = super::super::pipeline::Options {
            no_semantic: false,
            items: vec!["event".into()],
            ..options
        };
        let partial = super::super::pipeline::run(
            std::slice::from_ref(&source),
            &workspace,
            &config,
            &combined,
            tokio_util::sync::CancellationToken::new(),
            &mut |_: Event| {},
        )
        .await
        .unwrap();
        assert!(partial.partial);
        assert_eq!(partial.exports.len(), 1);
        let bundle: super::super::export::Bundle =
            serde_json::from_slice(&std::fs::read(&partial.exports[0].annotations).unwrap())
                .unwrap();
        assert_eq!(bundle.annotations.len(), 1);
        assert_eq!(bundle.annotations[0].header.name, NAME);
        assert_eq!(bundle.incomplete.len(), 1);
        let mut corrupt = frame.clone();
        corrupt.hands[0].keypoints[0] = Some([1.1, 0.]);
        assert!(corrupt.validate().is_err());
    }

    #[tokio::test]
    async fn rotated_vfr_hands_keep_display_coordinates_and_observed_frame_times() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.mp4");
        media::run(
            media::command("ffmpeg")
                .args(["-v", "error", "-loop", "1", "-framerate", "3", "-i"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hand-opencv.png"))
                .args([
                    "-t",
                    "1",
                    "-vf",
                    "pad=ceil(iw/2)*2:ceil(ih/2)*2,select='not(eq(n,1))'",
                    "-fps_mode",
                    "vfr",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                ])
                .arg(&base),
        )
        .unwrap();
        let source = dir.path().join("rotated.mp4");
        media::run(
            media::command("ffmpeg")
                .args(["-v", "error", "-display_rotation:v:0", "90", "-i"])
                .arg(&base)
                .args(["-c", "copy"])
                .arg(&source),
        )
        .unwrap();
        let workspace = dir.path().join("workspace");
        let options = super::super::pipeline::Options {
            embodied: true,
            hands: true,
            no_semantic: true,
            ..Default::default()
        };
        let report = super::super::pipeline::run(
            std::slice::from_ref(&source),
            &workspace,
            &crate::config::Config::default(),
            &options,
            tokio_util::sync::CancellationToken::new(),
            &mut |_| {},
        )
        .await
        .unwrap();
        let file = AnnotationFile::read(report.modules[0].path.as_ref().unwrap()).unwrap();
        assert_eq!(file.records.len(), 2);
        let frame = Frame::from_record(&file.records[0]).unwrap();
        assert_eq!((frame.frame_width, frame.frame_height), (366, 490));
        assert!(!frame.hands.is_empty());
        let times = media::frame_pts(&source).unwrap();
        assert_eq!(
            file.records.iter().map(|r| r.start_us).collect::<Vec<_>>(),
            times
        );
        let output = dir.path().join("rendered.mp4");
        super::super::video::render(
            &source,
            &workspace,
            &output,
            None,
            false,
            false,
            &tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        assert_eq!(media::frame_pts(&output).unwrap(), times);
        assert_eq!(media::probe(&output).unwrap().width, 366);
    }
}
