//! Local/transcript stations publish complete annotations and resume validated units.
use crate::{
    annotations::{AnnotationFile, Header, Model, Record},
    episode::{Episode, Stream},
    events::{Event, EventSink},
    media::{self, extract::SourceRange},
    providers::{Provider, probes},
    storage::{self, Checkpoints},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub fn stream_directory(sidecar: &Path, stream: &str, primary: &str) -> PathBuf {
    if stream == primary {
        sidecar.into()
    } else {
        sidecar.join("streams").join(stream)
    }
}
pub fn station_key(
    episode: &Episode,
    stream: &str,
    station: &str,
    params: &Value,
) -> Result<String> {
    let Stream::Video {
        sha256, range_us, ..
    } = episode.video(stream)?
    else {
        unreachable!()
    };
    storage::cache_key(&(
        &episode.episode_id,
        stream,
        sha256,
        range_us,
        &episode.time,
        station,
        params,
        1,
    ))
}
/// Retain historical sidecars on disk, but never project records from old inputs.
pub(crate) fn has_current_input(episode: &Episode, file: &AnnotationFile) -> Result<bool> {
    if file.header.episode != episode.episode_id || episode.video(&file.header.stream).is_err() {
        return Ok(false);
    }
    Ok(file.header.input_hash
        == station_key(
            episode,
            &file.header.stream,
            &file.header.name,
            &file.header.params,
        )?)
}
fn header(
    episode: &Episode,
    stream: &str,
    name: &str,
    model: Model,
    params: Value,
    input_hash: String,
) -> Header {
    Header {
        schema: "annotation/1".into(),
        name: name.into(),
        episode: episode.episode_id.clone(),
        stream: stream.into(),
        model,
        params,
        created: chrono::Utc::now().to_rfc3339(),
        cerul_version: env!("CARGO_PKG_VERSION").into(),
        input_hash,
        record_schema: format!("{name}/1"),
    }
}
fn existing(
    path: &Path,
    key: &str,
    duration: i64,
    recompute: bool,
) -> Result<Option<AnnotationFile>> {
    if !recompute && path.is_file() {
        let file = AnnotationFile::read(path)?;
        if file.header.input_hash == key {
            file.validate(duration, None)?;
            return Ok(Some(file));
        }
    }
    Ok(None)
}
pub fn screen_text(
    episode: &Episode,
    stream: &str,
    sidecar: &Path,
    recompute: bool,
    jobs: usize,
    events: &mut dyn EventSink,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<AnnotationFile> {
    ensure!(jobs > 0, "OCR jobs must be positive");
    media::with_sync_cancellation(cancel.clone(), media::check_cancellation)?;
    let params = json!({"fps":0.5,"max_edge":1080,"postprocess_version":2,"min_text_confidence":crate::ocr::MIN_TEXT_CONFIDENCE,"model":"PP-OCRv6-small","det":"d73e0058b7a8086bbd57f3d10b8bcd4ff95363f67e06e2762b5e814fe9c9410e","rec":"5435fd747c9e0efe15a96d0b378d5bd157e9492ed8fd80edf08f30d02fa24634"});
    let key = station_key(episode, stream, "screen_text", &params)?;
    let duration = episode.duration_us()?;
    let directory = stream_directory(sidecar, stream, &episode.time.reference);
    let path = directory.join("screen_text.jsonl");
    if let Some(file) = existing(&path, &key, duration, recompute)? {
        return Ok(file);
    }
    let Stream::Video {
        path: source,
        range_us,
        ..
    } = episode.video(stream)?
    else {
        unreachable!()
    };
    let source = episode.source.root.join(source);
    let temporary = tempfile::tempdir()?;
    let frames = media::extract::keyframes(
        &source,
        SourceRange::new(range_us[0], range_us[1])?,
        temporary.path(),
        0.5,
    )?;
    let checkpoints = Checkpoints::new(sidecar);
    let total = frames.len() as u64;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers_cancel = cancel.child_token();
    let worker_count = jobs
        .min(frames.len())
        .min(std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get));
    let texts = std::thread::scope(|scope| -> Result<Vec<String>> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(worker_count.max(1));
        let mut workers = Vec::new();
        for _ in 0..worker_count {
            let sender = sender.clone();
            let worker_cancel = workers_cancel.clone();
            let (frames, key, checkpoints, next) = (&frames, &key, &checkpoints, &next);
            workers.push(scope.spawn(move || {
                media::with_sync_cancellation(worker_cancel.clone(), || {
                    let mut ocr = crate::ocr::Ocr::default();
                    while !worker_cancel.is_cancelled() {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((relative, path)) = frames.get(index) else {
                            break;
                        };
                        let result = (|| -> Result<String> {
                            media::check_cancellation()?;
                            let checkpoint = storage::cache_key(&(key, relative))?;
                            if !recompute
                                && let Some(text) = checkpoints.load::<String>(&checkpoint)?
                            {
                                return Ok(text);
                            }
                            let pixels = image::open(path)?.to_rgb8();
                            let text = ocr
                                .read(&pixels)?
                                .iter()
                                .map(|b| b.text.as_str())
                                .collect::<Vec<_>>()
                                .join("\n");
                            media::check_cancellation()?;
                            checkpoints.save(&checkpoint, &text)?;
                            Ok(text)
                        })();
                        let failed = result.is_err();
                        if sender.send((index, result)).is_err() || failed {
                            worker_cancel.cancel();
                            break;
                        }
                    }
                });
            }));
        }
        drop(sender);
        let mut texts = vec![None; frames.len()];
        let mut failure = None;
        let mut done = 0;
        for (index, result) in receiver {
            match result {
                Ok(text) => {
                    texts[index] = Some(text);
                    done += 1;
                    events.emit(Event::Progress {
                        episode: episode.episode_id.clone(),
                        station: "screen_text".into(),
                        done,
                        total,
                    });
                }
                Err(error) => {
                    failure.get_or_insert(error);
                    workers_cancel.cancel();
                }
            }
        }
        for worker in workers {
            if worker.join().is_err() {
                failure.get_or_insert_with(|| anyhow::anyhow!("OCR worker panicked"));
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        media::with_sync_cancellation(cancel.clone(), media::check_cancellation)?;
        texts
            .into_iter()
            .map(|text| text.context("OCR frame was not processed"))
            .collect()
    })?;
    let mut records: Vec<Record> = Vec::new();
    for (index, ((relative, _), text)) in frames.iter().zip(texts).enumerate() {
        let start = episode
            .source_to_episode(stream, range_us[0] + relative)?
            .max(0);
        let next = frames
            .get(index + 1)
            .map(|(time, _)| *time)
            .unwrap_or(range_us[1] - range_us[0]);
        let end = episode
            .source_to_episode(stream, range_us[0] + next)?
            .min(duration);
        if !text.trim().is_empty() && end > start {
            if let Some(previous) = records.last_mut()
                && previous.end_us == start
                && previous.fields.get("text").and_then(Value::as_str) == Some(&text)
            {
                previous.end_us = end;
            } else {
                records.push(Record {
                    id: storage::cache_key(&(&key, start, &text))?,
                    start_us: start,
                    end_us: end,
                    confidence: None,
                    fields: BTreeMap::from([("text".into(), Value::String(text))]),
                });
            }
        }
    }
    let file = AnnotationFile {
        header: header(
            episode,
            stream,
            "screen_text",
            Model {
                kind: "embedded".into(),
                name: "PP-OCRv6-small".into(),
                base_url: None,
            },
            params,
            key,
        ),
        records,
    };
    media::with_sync_cancellation(cancel.clone(), media::check_cancellation)?;
    file.publish(&path, duration, None)?;
    Ok(file)
}

const WINDOW_US: i64 = 60_000_000;
fn transcript_params(provider: &Provider) -> serde_json::Value {
    json!({"kind":provider.endpoint.kind,"model":provider.endpoint.model,"base_url":provider.endpoint.base_url,"window_us":WINDOW_US,"timestamp_protocol":"seconds/1"})
}
fn transcript_key(episode: &Episode, stream: &str, provider: &Provider) -> Result<String> {
    // Short audio windows avoid long-context timestamp drift and keep inline
    // mono 16 kHz PCM requests well below the encoded request-size limit.
    let params = transcript_params(provider);
    station_key(episode, stream, "transcript", &params)
}

pub fn transcript_pending(
    episode: &Episode,
    stream: &str,
    sidecar: &Path,
    provider: &Provider,
    recompute: bool,
) -> Result<bool> {
    let Stream::Video {
        probe, range_us, ..
    } = episode.video(stream)?
    else {
        unreachable!()
    };
    if !probe.has_audio {
        return Ok(false);
    }
    let key = transcript_key(episode, stream, provider)?;
    let path = stream_directory(sidecar, stream, &episode.time.reference).join("transcript.jsonl");
    if existing(&path, &key, episode.duration_us()?, recompute)?.is_some() {
        return Ok(false);
    }
    if recompute {
        return Ok(true);
    }
    let checkpoints = Checkpoints::new(sidecar);
    for window in media::chunks(range_us[1] - range_us[0], WINDOW_US, 0)? {
        if checkpoints
            .load::<Vec<Record>>(&storage::cache_key(&(&key, window))?)?
            .is_none()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub async fn transcript(
    episode: &Episode,
    stream: &str,
    sidecar: &Path,
    workspace: &Path,
    provider: &Provider,
    recompute: bool,
    events: &mut dyn EventSink,
) -> Result<AnnotationFile> {
    let key = transcript_key(episode, stream, provider)?;
    let params = transcript_params(provider);
    let duration = episode.duration_us()?;
    let directory = stream_directory(sidecar, stream, &episode.time.reference);
    let path = directory.join("transcript.jsonl");
    if let Some(file) = existing(&path, &key, duration, recompute)? {
        return Ok(file);
    }
    let Stream::Video {
        path: source,
        range_us,
        probe,
        ..
    } = episode.video(stream)?
    else {
        unreachable!()
    };
    let mut records = Vec::new();
    if probe.has_audio {
        let source = episode.source.root.join(source);
        let checkpoints = Checkpoints::new(sidecar);
        let windows = media::chunks(range_us[1] - range_us[0], WINDOW_US, 0)?;
        for (index, window) in windows.iter().enumerate() {
            let checkpoint = storage::cache_key(&(&key, window))?;
            let cached = if !recompute {
                checkpoints.load::<Vec<Record>>(&checkpoint)?
            } else {
                None
            };
            let mut segment_records = match cached {
                Some(records) => records,
                None => {
                    probes::check(
                        provider,
                        probes::Capability::Transcription,
                        workspace,
                        false,
                    )
                    .await?;

                    let temporary = tempfile::tempdir()?;
                    let audio = temporary.path().join("audio.wav");
                    media::extract::audio(
                        &source,
                        SourceRange::new(
                            range_us[0] + window.start_us,
                            range_us[0] + window.end_us,
                        )?,
                        &audio,
                    )?;
                    let response = provider
                        .transcribe(fs::read(audio)?, window.end_us - window.start_us)
                        .await?;
                    let records = probes::segments(response, window.end_us - window.start_us)?;
                    checkpoints.save(&checkpoint, &records)?;
                    records
                }
            };
            for record in &mut segment_records {
                record.start_us = episode
                    .source_to_episode(stream, range_us[0] + window.start_us + record.start_us)?
                    .max(0);
                record.end_us = episode
                    .source_to_episode(stream, range_us[0] + window.start_us + record.end_us)?
                    .min(duration);
                record.id =
                    storage::cache_key(&(&key, record.start_us, record.end_us, &record.fields))?;
            }
            records.extend(
                segment_records
                    .into_iter()
                    .filter(|r| r.end_us > r.start_us),
            );
            events.emit(Event::Progress {
                episode: episode.episode_id.clone(),
                station: "transcript".into(),
                done: index as u64 + 1,
                total: windows.len() as u64,
            });
        }
    }
    let file = AnnotationFile {
        header: header(
            episode,
            stream,
            "transcript",
            Model {
                kind: provider.endpoint.kind.clone(),
                name: provider.endpoint.model.clone(),
                base_url: Some(provider.endpoint.base_url.clone()),
            },
            params,
            key,
        ),
        records,
    };
    file.publish(&path, duration, None)?;
    Ok(file)
}

pub fn text_for_range(
    file: Option<&AnnotationFile>,
    range: crate::episode::TimeRange,
) -> Result<String> {
    let mut text = Vec::new();
    if let Some(file) = file {
        for record in &file.records {
            if record.range()?.intersection(range).is_some() {
                text.push(
                    record
                        .fields
                        .get("text")
                        .and_then(Value::as_str)
                        .context("annotation text missing")?,
                );
            }
        }
    }
    Ok(text.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn ten_minute_audio_uses_bounded_requests_and_offsets_segment_times_once() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("speech.mp4");
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=32x32:rate=1:duration=600",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=16000:duration=600",
                    "-c:v",
                    "libx264",
                    "-c:a",
                    "aac",
                    "-shortest",
                ])
                .arg(&video),
        )
        .unwrap();
        let reply = |segments: Value| {
            (
                200,
                json!({"candidates":[{"content":{"parts":[{
                    "text":json!({"segments":segments}).to_string()
                }]}}]}),
            )
        };
        let segment = json!([{"start":1.234567,"end":2.345678,"text":"Test speech","lang":"en"}]);
        let mut responses = vec![reply(json!([]))];
        responses.extend((0..10).map(|_| reply(segment.clone())));
        let (base, server) = crate::providers::tests::server(responses);
        let mut endpoint = crate::config::Config::default().transcription;
        endpoint.base_url = base;
        let provider = Provider::new(
            endpoint,
            None,
            1,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        let episode = crate::index::discover::ordinary_episode(&video).unwrap();
        let workspace = dir.path().join("workspace");
        let sidecar = crate::index::discover::publish_episode(&workspace, &episode, None).unwrap();
        let file = transcript(
            &episode,
            "primary",
            &sidecar,
            &workspace,
            &provider,
            false,
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            file.records
                .iter()
                .map(|r| (r.start_us, r.end_us))
                .collect::<Vec<_>>(),
            (0..10)
                .map(|n| (n * 60_000_000 + 1_234_567, n * 60_000_000 + 2_345_678))
                .collect::<Vec<_>>()
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 11);
        for (_, request) in &requests[1..] {
            assert!(
                serde_json::to_vec(request).unwrap().len() < crate::providers::MAX_REQUEST_BYTES
            );
            let data = request["contents"][0]["parts"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|part| part.get("inlineData"))
                .unwrap()["data"]
                .as_str()
                .unwrap();
            use base64::Engine;
            let audio = base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap();
            assert_eq!(&audio[..4], b"RIFF");
            assert!((1_920_000..1_930_000).contains(&audio.len()));
        }
        // The server has closed; intact output must still be reusable.
        let cached = transcript(
            &episode,
            "primary",
            &sidecar,
            &workspace,
            &provider,
            false,
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(file.records).unwrap(),
            serde_json::to_value(cached.records).unwrap()
        );
    }
    #[tokio::test]
    async fn local_stations_merge_text_and_resume_without_model_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("screen.mp4");
        media::run(
            std::process::Command::new("ffmpeg")
                .args(["-v", "error", "-loop", "1", "-i"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-text.png"))
                .args([
                    "-t", "4", "-r", "2", "-c:v", "libx264", "-pix_fmt", "yuv420p",
                ])
                .arg(&video),
        )
        .unwrap();
        let episode = crate::index::discover::ordinary_episode(&video).unwrap();
        let workspace = dir.path().join("workspace");
        let sidecar = crate::index::discover::publish_episode(&workspace, &episode, None).unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut events = Vec::new();
        let file = screen_text(
            &episode,
            "primary",
            &sidecar,
            false,
            2,
            &mut |event| events.push(event),
            &cancel,
        )
        .unwrap();
        assert_eq!(file.records.len(), 1);
        assert_eq!(file.records[0].start_us, 0);
        assert_eq!(file.records[0].end_us, 4_000_000);
        assert_eq!(
            file.records[0].fields["text"]
                .as_str()
                .unwrap()
                .replace(' ', ""),
            "CERULVIDEO123"
        );
        let before = fs::read(sidecar.join("screen_text.jsonl")).unwrap();
        let mut unexpected_events = Vec::new();
        screen_text(
            &episode,
            "primary",
            &sidecar,
            false,
            2,
            &mut |event| unexpected_events.push(event),
            &cancel,
        )
        .unwrap();
        assert!(unexpected_events.is_empty());
        assert_eq!(before, fs::read(sidecar.join("screen_text.jsonl")).unwrap());
        let serial = screen_text(
            &episode,
            "primary",
            &dir.path().join("serial"),
            false,
            1,
            &mut |_| {},
            &cancel,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&file.records).unwrap(),
            serde_json::to_value(&serial.records).unwrap()
        );

        // A cached later frame completes before the earlier frame's real OCR.
        // Publication must still use source time, not worker completion order.
        let reordered = dir.path().join("reordered");
        let later_key = storage::cache_key(&(&file.header.input_hash, 2_000_000i64)).unwrap();
        Checkpoints::new(&reordered)
            .save(&later_key, &"Cached second frame")
            .unwrap();
        let ordered = screen_text(
            &episode,
            "primary",
            &reordered,
            false,
            2,
            &mut |_| {},
            &cancel,
        )
        .unwrap();
        assert_eq!(ordered.records.len(), 2);
        assert_eq!(
            (ordered.records[0].start_us, ordered.records[0].end_us),
            (0, 2_000_000)
        );
        assert_eq!(
            ordered.records[0].fields["text"],
            file.records[0].fields["text"]
        );
        assert_eq!(ordered.records[1].fields["text"], "Cached second frame");

        let interrupted = dir.path().join("interrupted");
        let stopped = tokio_util::sync::CancellationToken::new();
        let error = screen_text(
            &episode,
            "primary",
            &interrupted,
            false,
            2,
            &mut |_| stopped.cancel(),
            &stopped,
        )
        .unwrap_err();
        assert!(
            matches!(error.downcast_ref::<crate::providers::ProviderError>(), Some(e) if e.kind == crate::providers::Failure::Cancelled)
        );
        assert!(!interrupted.join("screen_text.jsonl").exists());
        let saved: Vec<_> = fs::read_dir(interrupted.join("staging/checkpoints"))
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        assert!(!saved.is_empty());
        let resumed = screen_text(
            &episode,
            "primary",
            &interrupted,
            false,
            2,
            &mut |_| {},
            &cancel,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&file.records).unwrap(),
            serde_json::to_value(&resumed.records).unwrap()
        );
        for (path, bytes) in saved {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        let provider = Provider::new(
            crate::config::Config::default().transcription,
            None,
            1,
            None,
            cancel,
        )
        .unwrap();
        assert!(
            transcript(
                &episode,
                "primary",
                &sidecar,
                &workspace,
                &provider,
                false,
                &mut |_| {}
            )
            .await
            .unwrap()
            .records
            .is_empty()
        );
        assert!(!workspace.join("providers.json").exists());
        assert_ne!(
            stream_directory(&sidecar, "primary", "primary"),
            stream_directory(&sidecar, "wrist", "primary")
        );
    }
}
