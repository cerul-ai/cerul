//! Media operations preserve source PTS and explicitly map them to episode time.
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::Path,
    process::{Command, Output},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Probe {
    pub duration_us: i64,
    pub start_us: i64,
    pub width: u32,
    pub height: u32,
    pub fps: String,
    pub has_audio: bool,
    pub codec: String,
}

/// Parse ffprobe decimal seconds without a floating-point round trip.
pub fn seconds_to_us(value: &str) -> Result<i64> {
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    ensure!(
        !whole.is_empty()
            && whole.bytes().all(|c| c.is_ascii_digit())
            && fraction.bytes().all(|c| c.is_ascii_digit()),
        "invalid media timestamp"
    );
    let whole: i64 = whole.parse()?;
    let digits = &fraction[..fraction.len().min(6)];
    let fractional = if digits.is_empty() {
        0
    } else {
        digits.parse::<i64>()? * 10_i64.pow((6 - digits.len()) as u32)
    };
    let us = whole
        .checked_mul(1_000_000)
        .and_then(|n| n.checked_add(fractional))
        .context("media timestamp overflow")?;
    Ok(if negative { -us } else { us })
}

#[derive(Clone)]
struct CachedFrames {
    length: u64,
    modified: Option<std::time::SystemTime>,
    points: Vec<i64>,
}
tokio::task_local! {
    static FRAME_CACHE: std::cell::RefCell<std::collections::BTreeMap<std::path::PathBuf, CachedFrames>>;
    static CANCELLATION: tokio_util::sync::CancellationToken;
}
/// Scope cancellation to one operation, including synchronous nested media work.
/// Independent tasks never share a process-global cancellation flag.
pub async fn with_cancellation<T>(
    cancel: tokio_util::sync::CancellationToken,
    future: impl std::future::Future<Output = T>,
) -> T {
    FRAME_CACHE
        .scope(Default::default(), CANCELLATION.scope(cancel, future))
        .await
}
/// Carry an operation's cancellation token into scoped CPU worker threads.
pub(crate) fn with_sync_cancellation<T>(
    cancel: tokio_util::sync::CancellationToken,
    operation: impl FnOnce() -> T,
) -> T {
    CANCELLATION.sync_scope(cancel, operation)
}
pub fn check_cancellation() -> Result<()> {
    if CANCELLATION
        .try_with(|token| token.is_cancelled())
        .unwrap_or(false)
    {
        return Err(crate::providers::ProviderError {
            kind: crate::providers::Failure::Cancelled,
            message: "operation cancelled".into(),
        }
        .into());
    }
    Ok(())
}
pub fn run(command: &mut Command) -> Result<Output> {
    let cancel = CANCELLATION.try_with(Clone::clone).unwrap_or_default();
    run_cancellable(command, &cancel)
}
/// Drain both pipes while waiting, and kill/reap the direct media child on cancellation.
pub fn run_cancellable(
    command: &mut Command,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Output> {
    let cancelled = || crate::providers::ProviderError {
        kind: crate::providers::Failure::Cancelled,
        message: "operation cancelled".into(),
    };
    if cancel.is_cancelled() {
        return Err(cancelled().into());
    }
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("start media subprocess; install ffmpeg and ffprobe >= 6.0")?;
    let drain = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes)?;
            Ok::<_, std::io::Error>(bytes)
        })
    };
    let stdout = drain(Box::new(child.stdout.take().unwrap()));
    let stderr = drain(Box::new(child.stderr.take().unwrap()));
    let status = (|| -> Result<std::process::ExitStatus> {
        loop {
            if cancel.is_cancelled() {
                return Err(cancelled().into());
            }
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    })();
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = stdout
        .join()
        .map_err(|_| anyhow::anyhow!("media stdout reader failed"))??;
    let stderr = stderr
        .join()
        .map_err(|_| anyhow::anyhow!("media stderr reader failed"))??;
    let output = Output {
        status: status?,
        stdout,
        stderr,
    };
    ensure!(
        output.status.success(),
        "media subprocess failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

pub fn check_dependencies() -> Result<()> {
    for executable in ["ffmpeg", "ffprobe"] {
        let output = run(Command::new(executable).arg("-version"))?;
        let version = String::from_utf8_lossy(&output.stdout);
        let major = version
            .split_whitespace()
            .nth(2)
            .unwrap_or("")
            .trim_start_matches('n')
            .split('.')
            .next()
            .unwrap_or("")
            .parse::<u32>();
        ensure!(
            major.is_ok_and(|major| major >= 6),
            "{executable} >= 6.0 is required"
        );
    }
    Ok(())
}

pub fn probe(path: &Path) -> Result<Probe> {
    let output = run(Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path))?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let streams = value["streams"]
        .as_array()
        .context("ffprobe returned no streams")?;
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .context("no video stream")?;
    let duration = video["duration"]
        .as_str()
        .or_else(|| value["format"]["duration"].as_str())
        .context("unknown video duration")?;
    let duration_us = seconds_to_us(duration)?;
    ensure!(duration_us > 0, "empty video");
    Ok(Probe {
        duration_us,
        start_us: seconds_to_us(video["start_time"].as_str().unwrap_or("0"))?,
        width: u32::try_from(video["width"].as_u64().context("missing width")?)?,
        height: u32::try_from(video["height"].as_u64().context("missing height")?)?,
        fps: video["avg_frame_rate"].as_str().unwrap_or("0/1").into(),
        has_audio: streams.iter().any(|s| s["codec_type"] == "audio"),
        codec: video["codec_name"].as_str().unwrap_or("unknown").into(),
    })
}

pub fn sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        check_cancellation()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn frame_pts(path: &Path) -> Result<Vec<i64>> {
    check_cancellation()?;
    let path = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&path)?;
    let modified = metadata.modified().ok();
    if let Ok(Some(found)) = FRAME_CACHE.try_with(|cache| cache.borrow().get(&path).cloned())
        && modified.is_some()
        && found.length == metadata.len()
        && found.modified == modified
    {
        return Ok(found.points);
    }
    let points = frame_pts_uncached(&path)?;
    let _ = FRAME_CACHE.try_with(|cache| {
        let mut cache = cache.borrow_mut();
        // Bound the number of retained source timelines for multi-camera datasets.
        if cache.len() >= 4 {
            cache.clear();
        }
        cache.insert(
            path,
            CachedFrames {
                length: metadata.len(),
                modified,
                points: points.clone(),
            },
        );
    });
    Ok(points)
}
fn frame_pts_uncached(path: &Path) -> Result<Vec<i64>> {
    let output = run(Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_frames",
            "-show_entries",
            "frame=best_effort_timestamp_time",
            "-of",
            "json",
        ])
        .arg(path))?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    value["frames"]
        .as_array()
        .context("missing frame timestamps")?
        .iter()
        .map(|frame| {
            seconds_to_us(
                frame["best_effort_timestamp_time"]
                    .as_str()
                    .context("frame has no PTS")?,
            )
        })
        .collect()
}

/// Formatting is exact even at nonintegral frame rates.
pub fn seconds(us: i64) -> String {
    format!(
        "{}{}.{:06}",
        if us < 0 { "-" } else { "" },
        us.unsigned_abs() / 1_000_000,
        us.unsigned_abs() % 1_000_000
    )
}

pub fn chunks(
    duration_us: i64,
    chunk_us: i64,
    overlap_us: i64,
) -> Result<Vec<crate::episode::TimeRange>> {
    ensure!(
        duration_us > 0 && chunk_us > 0 && overlap_us >= 0 && overlap_us < chunk_us,
        "invalid chunk duration or overlap"
    );
    let mut chunks = Vec::new();
    let mut start: i64 = 0;
    loop {
        let end = start.saturating_add(chunk_us).min(duration_us);
        chunks.push(crate::episode::TimeRange::new(start, end)?);
        if end == duration_us {
            break;
        }
        start = end - overlap_us;
    }
    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn operation_frame_cache_invalidates_when_source_changes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        with_cancellation(tokio_util::sync::CancellationToken::new(), async {
            for rate in [10, 2] {
                run(Command::new("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-y",
                        "-f",
                        "lavfi",
                        "-i",
                        &format!("testsrc2=size=64x64:rate={rate}:duration=1"),
                        "-c:v",
                        "libx264",
                    ])
                    .arg(&source))
                .unwrap();
                let first = frame_pts(&source).unwrap();
                assert_eq!(first.len(), rate);
                assert_eq!(frame_pts(&source).unwrap(), first);
            }
        })
        .await;
    }
    #[test]
    fn cancellation_kills_a_running_ffmpeg_and_drains_large_outputs() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let trigger = cancel.clone();
        let signal = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            trigger.cancel();
        });
        let started = std::time::Instant::now();
        let error = run_cancellable(
            Command::new("ffmpeg").args([
                "-v",
                "error",
                "-re",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x64:rate=10:duration=60",
                "-f",
                "null",
                "-",
            ]),
            &cancel,
        )
        .unwrap_err();
        signal.join().unwrap();
        assert_eq!(
            error
                .downcast_ref::<crate::providers::ProviderError>()
                .unwrap()
                .kind,
            crate::providers::Failure::Cancelled
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        let output = run(Command::new("ffmpeg").args([
            "-v",
            "debug",
            "-debug_ts",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x64:rate=240:duration=3",
            "-f",
            "rawvideo",
            "-",
        ]))
        .unwrap();
        assert!(output.stdout.len() > 65536);
        assert!(output.stderr.len() > 65536);
    }
    #[tokio::test]
    async fn scoped_cancellation_does_not_leak_to_other_operations() {
        let stopped = tokio_util::sync::CancellationToken::new();
        stopped.cancel();
        let (first, second) = tokio::join!(
            with_cancellation(stopped, async {
                run(Command::new("ffprobe").arg("-version"))
            }),
            with_cancellation(tokio_util::sync::CancellationToken::new(), async {
                run(Command::new("ffprobe").arg("-version"))
            }),
        );
        assert!(first.is_err());
        assert!(second.is_ok());
        assert!(check_cancellation().is_ok());
    }
    #[test]
    fn timestamps_are_integer_and_chunks_cover_tail_without_duplicate_tail() {
        assert_eq!(seconds_to_us("120.000001").unwrap(), 120_000_001);
        assert_eq!(seconds_to_us("-0.033333").unwrap(), -33333);
        assert!(seconds_to_us("NaN").is_err());
        let ranges = chunks(60_000_001, 30_000_000, 5_000_000).unwrap();
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges.last().unwrap().end_us, 60_000_001);
        assert!(chunks(10, 5, 5).is_err());
    }
    #[test]
    fn real_video_probe_hash_and_frame_pts() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.mp4");
        run(Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x64:rate=10:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&path))
        .unwrap();
        let info = probe(&path).unwrap();
        assert_eq!(
            (info.width, info.height, info.duration_us),
            (64, 64, 1_000_000)
        );
        assert!(!info.has_audio);
        assert_eq!(
            frame_pts(&path).unwrap(),
            (0..10).map(|n| n * 100_000).collect::<Vec<_>>()
        );
        assert_eq!(sha256(&path).unwrap().len(), 64);
    }
}

pub mod extract;

pub mod proxy;

pub mod contact_proxy;
