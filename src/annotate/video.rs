//! Offline composition of published semantic annotations into a new review video.
use super::export::{Bundle, Generator};
use crate::{
    annotations::{AnnotationFile, SEMANTIC_ITEMS},
    episode::{Episode, Stream},
    index::{discover, stations::stream_directory},
    media, storage,
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub output: PathBuf,
    pub generation: String,
    pub stream: String,
    pub rendered: bool,
}

/// Existing portable exports can represent dataset episodes. Ordinary media are
/// resolved by content identity, including sidecars relocated through the registry.
pub fn load(input: &Path, workspace: &Path) -> Result<Bundle> {
    if input.extension().is_some_and(|value| value == "json") {
        let bundle: Bundle = serde_json::from_slice(&fs::read(input)?)?;
        ensure!(
            bundle.schema == "annotations/1",
            "unsupported annotation bundle"
        );
        ensure!(
            bundle.generation == storage::cache_key(&(&bundle.annotations, &bundle.incomplete))?,
            "annotation bundle generation mismatch"
        );
        bundle.source.validate()?;
        return Ok(bundle);
    }
    let episode = discover::ordinary_episode(input)?;
    let sidecar = discover::sidecar_path(&episode, &discover::read_registry(workspace)?, None)?;
    let saved: Episode = serde_json::from_slice(
        &fs::read(sidecar.join("episode.json"))
            .context("no published annotations; run cerul annotate first")?,
    )?;
    ensure!(
        saved.episode_id == episode.episode_id,
        "source changed since annotation; annotate the current video first"
    );
    let mut annotations = Vec::new();
    for name in SEMANTIC_ITEMS
        .iter()
        .map(|item| format!("semantic.{item}"))
        .chain(std::iter::once(super::hands::NAME.to_owned()))
    {
        let path = super::layout::annotation(
            &stream_directory(&sidecar, "primary", &episode.time.reference),
            &name,
        );
        if path.is_file() {
            let file = AnnotationFile::read(&path)?;
            if crate::index::stations::has_current_input(&episode, &file)? {
                annotations.push(file);
            }
        }
    }
    ensure!(!annotations.is_empty(), "no published annotations");
    super::export::canonicalize(&mut annotations);
    // A completed export also records failed modules. Keep that exact view
    // while its included tracks match current sidecars; a failed recompute may
    // intentionally leave an older authoritative file for an excluded track.
    if let Ok(mut published) = load(&sidecar.join("annotations.json"), workspace)
        && published.source.episode_id == episode.episode_id
    {
        let included = annotations
            .iter()
            .filter(|file| {
                !published
                    .incomplete
                    .contains(&format!("{}: {}", file.header.stream, file.header.name))
            })
            .collect::<Vec<_>>();
        if storage::cache_key(&included)? == storage::cache_key(&published.annotations)? {
            // A video may have moved since the export. Its content/timeline
            // identity was verified above; render from the current location.
            published.source = episode;
            return Ok(published);
        }
    }

    Ok(Bundle {
        schema: "annotations/1".into(),
        generator: Generator {
            name: "cerul".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            website: "https://cerul.ai".into(),
        },
        source: episode,
        generation: storage::cache_key(&(&annotations, Vec::<String>::new()))?,
        annotations,
        incomplete: Vec::new(),
    })
}

// Decode one frame with the same autorotation behavior used by composition.
// Coded ffprobe dimensions do not describe rotated display-matrix inputs.
fn display_dimensions(source: &Path) -> Result<(u32, u32)> {
    let output = media::run(
        media::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(source)
            .args([
                "-map",
                "0:V:0",
                "-frames:v",
                "1",
                "-an",
                "-c:v",
                "png",
                "-f",
                "image2pipe",
                "pipe:1",
            ]),
    )?;
    Ok(image::ImageReader::new(std::io::Cursor::new(output.stdout))
        .with_guessed_format()?
        .into_dimensions()?)
}

fn interrupted(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(crate::providers::ProviderError {
            kind: crate::providers::Failure::Cancelled,
            message: "render cancelled".into(),
        }
        .into());
    }
    Ok(())
}

pub fn render(
    input: &Path,
    workspace: &Path,
    output: &Path,
    stream: Option<&str>,
    watermark: bool,
    dry_run: bool,
    cancel: &CancellationToken,
) -> Result<Report> {
    media::with_sync_cancellation(cancel.clone(), || {
        render_inner(input, workspace, output, stream, watermark, dry_run, cancel)
    })
}

fn render_inner(
    input: &Path,
    workspace: &Path,
    output: &Path,
    stream: Option<&str>,
    watermark: bool,
    dry_run: bool,
    cancel: &CancellationToken,
) -> Result<Report> {
    interrupted(cancel)?;
    let bundle = load(input, workspace)?;
    let stream = stream.unwrap_or(&bundle.source.time.reference);
    let episode = &bundle.source;
    let Stream::Video {
        path,
        sha256,
        probe,
        ..
    } = episode.video(stream)?
    else {
        unreachable!()
    };
    let source = episode.source.root.join(path);
    ensure!(
        media::sha256(&source)? == *sha256,
        "source video changed since annotation"
    );
    let coverage = episode
        .video_coverage(stream)?
        .context("no video coverage")?;
    let source_start = episode.episode_to_source(stream, coverage.start_us)?;
    let source_end = episode.episode_to_source(stream, coverage.end_us)?;
    ensure!(
        source_end - source_start == coverage.end_us - coverage.start_us,
        "rendering a scaled camera timeline is unsupported"
    );
    ensure!(
        output
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4")),
        "render output must be an MP4"
    );
    ensure!(
        !output.exists(),
        "output already exists; choose a new --out path"
    );
    let files = bundle
        .annotations
        .iter()
        .filter(|file| {
            file.header.stream == stream
                && (file.header.name.starts_with("semantic.")
                    || file.header.name == super::hands::NAME)
        })
        .collect::<Vec<_>>();
    ensure!(
        !files.is_empty(),
        "no published annotations for selected stream"
    );
    for file in &files {
        ensure!(
            file.header.episode == episode.episode_id,
            "annotation episode mismatch"
        );
        ensure!(
            crate::index::stations::has_current_input(episode, file)?,
            "annotation inputs no longer match source"
        );
        file.validate_in_range(coverage, None)?;
    }
    let mut report = Report {
        output: output.to_owned(),
        generation: bundle.generation.clone(),
        stream: stream.into(),
        rendered: false,
    };
    if dry_run {
        return Ok(report);
    }
    media::check_dependencies()?;
    let records = files
        .iter()
        .filter(|file| file.header.name.starts_with("semantic."))
        .flat_map(|file| {
            file.records
                .iter()
                .map(move |record| (file.header.name.as_str(), record))
        })
        .collect::<Vec<_>>();
    let mut boundaries = BTreeSet::from([coverage.start_us, coverage.end_us]);
    for (_, record) in &records {
        boundaries.insert(record.start_us);
        boundaries.insert(record.end_us);
    }
    let hand_frames = files
        .iter()
        .find(|file| file.header.name == super::hands::NAME)
        .map(|file| {
            file.records
                .iter()
                .map(|r| Ok((r.start_us, r.end_us, super::hands::Frame::from_record(r)?)))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    for (start, end, _) in &hand_frames {
        boundaries.insert(*start);
        boundaries.insert(*end);
    }
    let boundaries = boundaries.into_iter().collect::<Vec<_>>();
    let directory = tempfile::tempdir()?;
    let mut concat = "ffconcat version 1.0\n".to_owned();
    let (display_width, display_height) = display_dimensions(&source)?;
    let width = display_width.max(32).div_ceil(2) * 2;
    let height = display_height.div_ceil(2) * 2;
    let mut panel_height = 0;
    for (index, times) in boundaries.windows(2).enumerate() {
        interrupted(cancel)?;
        let active = records
            .iter()
            .filter(|(_, record)| record.start_us <= times[0] && times[0] < record.end_us)
            .collect::<Vec<_>>();
        // Subtask is the readable primary caption; other types remain available
        // in the structured export and fill gaps when no subtask is present.
        let subtask = active.iter().find(|(name, _)| *name == "semantic.subtask");
        let caption = match subtask {
            Some((_, record)) => super::export::text(record),
            None => active
                .iter()
                .map(|(name, record)| {
                    format!(
                        "{}: {}",
                        name.trim_start_matches("semantic."),
                        super::export::text(record)
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        };
        let hand_index = hand_frames.partition_point(|(start, _, _)| *start <= times[0]);
        let hand_frame = hand_index
            .checked_sub(1)
            .and_then(|i| hand_frames.get(i))
            .filter(|(_, end, _)| times[0] < *end);
        let caption = if caption.is_empty() && !hand_frames.is_empty() {
            format!(
                "Human hands: {} detected",
                hand_frame.map_or(0, |(_, _, f)| f.hands.len())
            )
        } else {
            caption
        };
        let panel = super::caption::panel(width, &caption, watermark);
        panel_height = panel.height();
        let path = directory.path().join(format!("{index}.png"));
        if hand_frames.is_empty() {
            panel.save(path)?;
        } else {
            let mut overlay = image::RgbaImage::new(width, height + panel_height);
            let panel = image::DynamicImage::ImageRgb8(panel).to_rgba8();
            image::imageops::replace(&mut overlay, &panel, 0, height as i64);
            if let Some((_, _, frame)) = hand_frame {
                super::hands::draw(&mut overlay, frame, display_width, display_height);
            }
            overlay.save(path)?;
        }
        concat.push_str(&format!(
            "file '{index}.png'\noption framerate 1000000\nduration {:.6}\n",
            (times[1] - times[0]) as f64 / 1e6
        ));
    }
    // A final packet gives the last duration an explicit endpoint.
    concat.push_str(&format!(
        "file '{}.png'\noption framerate 1000000\n",
        boundaries.len() - 2
    ));
    fs::write(directory.path().join("captions.ffconcat"), concat)?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = tempfile::Builder::new()
        .suffix(".mp4")
        .tempfile_in(parent)?;
    let start = source_start as f64 / 1e6;
    let duration = (source_end - source_start) as f64 / 1e6;
    let overlay_y = if hand_frames.is_empty() { height } else { 0 };
    let filter = format!(
        "[0:V:0]trim=start=0:end={duration:.6},pad={width}:{}:0:0[video];[video][1:v:0]overlay=x=0:y={overlay_y}:eof_action=repeat:shortest=0[out]",
        height + panel_height
    );
    let mut command = media::command("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-copyts",
            // Shift both streams at input. FFmpeg 6/7 setpts clears frame
            // durations, which can make the MP4 muxer discard the last frame.
            "-itsoffset",
            &format!("{:.6}", -start),
            "-i",
        ])
        .arg(&source)
        .args(["-f", "concat", "-safe", "0", "-i"])
        .arg(directory.path().join("captions.ffconcat"))
        .args([
            "-filter_complex",
            &filter,
            "-map",
            "[out]",
            "-map",
            "0:a:0?",
            "-c:v",
            "libx264",
            "-crf",
            "20",
            "-preset",
            "fast",
            "-pix_fmt",
            "yuv420p",
            "-fps_mode",
            "vfr",
            "-enc_time_base",
            "1:1000000",
            "-video_track_timescale",
            "1000000",
        ]);
    if probe.has_audio {
        command.args([
            "-af",
            &format!("atrim=start=0:end={duration:.6}"),
            "-c:a",
            "aac",
        ]);
    }
    command
        .args([
            "-t",
            &format!("{:.6}", (source_end - source_start) as f64 / 1e6),
            "-metadata",
            &format!(
                "comment=Annotated with Cerul {}; generation={}",
                env!("CARGO_PKG_VERSION"),
                bundle.generation
            ),
            "-movflags",
            "+faststart",
        ])
        .arg(temp.path());
    media::run_cancellable(&mut command, cancel)?;
    let result = media::probe(temp.path())?;
    ensure!(
        result.width == width && result.height == height + panel_height,
        "render dimensions differ from plan"
    );
    temp.as_file().sync_all()?;
    temp.persist_noclobber(output)
        .map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()?;
    report.rendered = true;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{Header, Model, Record};
    use std::collections::BTreeMap;

    fn annotate_fixture(source: &Path, workspace: &Path) -> Episode {
        let episode = discover::ordinary_episode(source).unwrap();
        let sidecar = discover::publish_episode(workspace, &episode, None).unwrap();
        let coverage = episode.video_coverage("primary").unwrap().unwrap();
        let file = AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "semantic.task".into(),
                episode: episode.episode_id.clone(),
                stream: "primary".into(),
                model: Model {
                    kind: "test".into(),
                    name: "fixture".into(),
                    base_url: None,
                },
                params: serde_json::json!({}),
                created: "2026-01-01T00:00:00Z".into(),
                cerul_version: "test".into(),
                input_hash: crate::index::stations::station_key(
                    &episode,
                    "primary",
                    "semantic.task",
                    &serde_json::json!({}),
                )
                .unwrap(),
                record_schema: "semantic.task/1".into(),
            },
            records: vec![Record {
                id: "task-0".into(),
                start_us: coverage.start_us,
                end_us: coverage.end_us,
                confidence: None,
                fields: BTreeMap::from([("text".into(), "Review a visible action.".into())]),
            }],
        };
        file.publish_in_range(&sidecar.join("semantic.task.jsonl"), coverage, None)
            .unwrap();
        episode
    }

    #[test]
    fn render_preserves_offset_vfr_timeline_audio_and_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a quote's 视频.mp4");
        media::run(
            media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=128x96:rate=10:duration=1.5",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=1.5",
                    "-vf",
                    "select='not(eq(n,2)+eq(n,3))',setpts=PTS+2/TB",
                    "-af",
                    "asetpts=PTS+2/TB",
                    "-fps_mode",
                    "vfr",
                    "-c:v",
                    "libx264",
                    "-c:a",
                    "aac",
                ])
                .arg(&source),
        )
        .unwrap();
        let original = media::sha256(&source).unwrap();
        let workspace = dir.path().join("workspace");
        let episode = annotate_fixture(&source, &workspace);
        let output = dir.path().join("review.mp4");
        let plan = render(
            &source,
            &workspace,
            &output,
            None,
            false,
            true,
            &CancellationToken::new(),
        )
        .unwrap();
        assert!(!plan.rendered && !output.exists());
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(render(&source, &workspace, &output, None, false, false, &cancelled).is_err());
        assert!(!output.exists());
        render(
            &source,
            &workspace,
            &output,
            None,
            true,
            false,
            &CancellationToken::new(),
        )
        .unwrap();
        let expected = media::frame_pts(&source)
            .unwrap()
            .into_iter()
            .map(|pts| episode.source_to_episode("primary", pts).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(media::frame_pts(&output).unwrap(), expected);
        assert!(media::probe(&output).unwrap().has_audio);
        assert_eq!(media::sha256(&source).unwrap(), original);
        let source_before = fs::read(&source).unwrap();
        assert!(
            render(
                &source,
                &workspace,
                &source,
                None,
                false,
                false,
                &CancellationToken::new()
            )
            .is_err()
        );
        assert_eq!(fs::read(&source).unwrap(), source_before);
    }
    #[test]
    fn rotated_phone_videos_render_at_display_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("base.mp4");
        media::run(
            media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=128x96:rate=2:duration=1",
                    "-c:v",
                    "libx264",
                ])
                .arg(&base),
        )
        .unwrap();
        for rotation in [90, 270] {
            let source = dir.path().join(format!("phone-{rotation}.mp4"));
            media::run(
                media::command("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-display_rotation:v:0",
                        &rotation.to_string(),
                        "-i",
                    ])
                    .arg(&base)
                    .args(["-c", "copy"])
                    .arg(&source),
            )
            .unwrap();
            assert_eq!(display_dimensions(&source).unwrap(), (96, 128));
            let workspace = dir.path().join(format!("workspace-{rotation}"));
            annotate_fixture(&source, &workspace);
            let output = dir.path().join(format!("review-{rotation}.mp4"));
            render(
                &source,
                &workspace,
                &output,
                None,
                false,
                false,
                &CancellationToken::new(),
            )
            .unwrap();
            let expected = (
                96,
                128 + super::super::caption::panel(96, "", false).height(),
            );
            let probe = media::probe(&output).unwrap();
            assert_eq!((probe.width, probe.height), expected);
            assert_eq!(
                display_dimensions(&output).unwrap(),
                expected,
                "rotation must not be applied a second time on playback"
            );
            assert_eq!(
                media::frame_pts(&source).unwrap(),
                media::frame_pts(&output).unwrap()
            );
        }
    }

    #[test]
    fn cancellation_during_hashing_uses_the_library_call_token() {
        use std::{io::Write, sync::mpsc, time::Duration};
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("slow.mp4");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&source)
                .status()
                .unwrap()
                .success()
        );
        let cancel = CancellationToken::new();
        let ambient = CancellationToken::new();
        let (tx, rx) = mpsc::channel();
        let worker_cancel = cancel.clone();
        let worker_ambient = ambient.clone();
        let input = source.clone();
        let root = dir.path().to_owned();
        let worker = std::thread::spawn(move || {
            let result = media::with_sync_cancellation(worker_ambient, || {
                render(
                    &input,
                    &root,
                    &root.join("out.mp4"),
                    None,
                    false,
                    true,
                    &worker_cancel,
                )
            });
            tx.send(result).unwrap();
        });
        // FIFO open pairs with File::open in sha256: cancellation happens after
        // render's entry check, while the source-reading phase is active.
        let mut writer = fs::OpenOptions::new().write(true).open(&source).unwrap();
        cancel.cancel();
        let _ = writer.write_all(b"first chunk");
        let result = rx.recv_timeout(Duration::from_secs(1));
        let timely = result.is_ok();
        let result = match result {
            Ok(result) => result,
            Err(_) => {
                // Release a regressed implementation through its ambient token,
                // so this test never leaks a blocked worker after asserting.
                ambient.cancel();
                let _ = writer.write_all(b"wake reader");
                rx.recv_timeout(Duration::from_secs(2)).unwrap()
            }
        };
        worker.join().unwrap();
        assert!(
            timely,
            "render ignored its own cancellation token while hashing"
        );
        assert!(matches!(
            result
                .unwrap_err()
                .downcast_ref::<crate::providers::ProviderError>()
                .unwrap()
                .kind,
            crate::providers::Failure::Cancelled
        ));
        assert!(!dir.path().join("out.mp4").exists());
    }
}
