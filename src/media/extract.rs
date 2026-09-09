use super::{probe, run, seconds};

use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    path::Path,
    process::Command,
};

/// Source PTS may be negative; episode intervals must remain nonnegative.
#[derive(Debug, Clone, Copy)]
pub struct SourceRange {
    pub start_us: i64,
    pub end_us: i64,
}
impl SourceRange {
    pub fn new(start_us: i64, end_us: i64) -> Result<Self> {
        ensure!(
            end_us > start_us && end_us.checked_sub(start_us).is_some(),
            "invalid source interval"
        );
        Ok(Self { start_us, end_us })
    }
}

fn input(command: &mut Command, source: &Path, source_range: SourceRange) -> Result<()> {
    seek_input(command, source, source_range)?;
    command.args(["-t", &seconds(source_range.end_us - source_range.start_us)]);
    Ok(())
}
fn seek_input(command: &mut Command, source: &Path, source_range: SourceRange) -> Result<()> {
    let origin = probe(source)?.start_us;
    let offset = source_range
        .start_us
        .checked_sub(origin)
        .context("seek time overflow")?;
    ensure!(offset >= 0, "clip starts before the source timeline");
    command
        .args(["-nostdin", "-v", "error", "-y", "-ss", &seconds(offset)])
        .arg("-i")
        .arg(source);
    Ok(())
}

/// Encode a proxy or returned clip without ever publishing a partial MP4.
pub fn video(
    source: &Path,
    source_range: SourceRange,
    destination: &Path,
    proxy: bool,
) -> Result<()> {
    video_quality(source, source_range, destination, proxy, 24)
}

pub fn video_quality(
    source: &Path,
    source_range: SourceRange,
    destination: &Path,
    proxy: bool,
    crf: u8,
) -> Result<()> {
    ensure!(crf <= 51, "invalid video quality");
    let parent = destination.parent().context("clip has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = tempfile::Builder::new()
        .suffix(".mp4")
        .tempfile_in(parent)?;
    let mut command = crate::media::command("ffmpeg");
    input(&mut command, source, source_range)?;
    command.args(["-map", "0:v:0"]);
    if proxy {
        command.args(["-an", "-vf", "scale='min(480,iw)':'min(480,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2,fps=2"]);
    } else {
        command.args(["-map", "0:a:0?", "-c:a", "aac"]);
    }
    command
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            &crf.to_string(),
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
        ])
        .arg(temp.path());
    run(&mut command)?;
    temp.as_file().sync_all()?;
    temp.persist(destination).map_err(|e| e.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn audio(source: &Path, source_range: SourceRange, destination: &Path) -> Result<()> {
    let parent = destination.parent().context("audio has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile_in(parent)?;
    let mut command = crate::media::command("ffmpeg");
    input(&mut command, source, source_range)?;
    command
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(temp.path());
    run(&mut command)?;
    temp.as_file().sync_all()?;
    temp.persist(destination).map_err(|e| e.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

/// Materialize sampled frames only inside the supplied temporary directory.
/// Returned episode timestamps are selected from actual source-frame PTS.
pub fn keyframes(
    source: &Path,
    source_range: SourceRange,
    directory: &Path,
    fps: f64,
) -> Result<Vec<(i64, std::path::PathBuf)>> {
    ensure!(
        fps.is_finite() && fps > 0. && fps <= 60.,
        "invalid keyframe rate"
    );
    fs::create_dir_all(directory)?;
    let pts = super::frame_pts(source)?;
    let interval = (1_000_000. / fps).round() as i64;
    let mut next = source_range.start_us;
    let mut selected = Vec::new();
    for (index, time) in pts.iter().enumerate() {
        if *time >= source_range.end_us {
            break;
        }
        if *time >= next {
            selected.push((index, *time));
            next = time.saturating_add(interval);
        }
    }
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    // Accurate input seeking discards frames before the requested start. Keep
    // ffprobe's actual selected frame numbers relative to that first retained frame.
    let first_frame = selected[0].0;
    // A flat sum exceeds ffmpeg's expression-parser recursion limit on long
    // recordings. Balance the tree and keep it in a file to avoid ARG_MAX.
    fn selection(frames: &[(usize, i64)], first: usize) -> String {
        if frames.len() == 1 {
            return format!("eq(n\\,{})", frames[0].0 - first);
        }
        let (left, right) = frames.split_at(frames.len() / 2);
        format!("({}+{})", selection(left, first), selection(right, first))
    }
    let expression = selection(&selected, first_frame);
    let filter = format!(
        "select='{expression}',scale='min(1080,iw)':'min(1080,ih)':force_original_aspect_ratio=decrease"
    );
    let pattern = directory.join("frame-%08d.png");
    let filter_path = directory.join("selection.filter");
    fs::write(&filter_path, &filter)?;
    // New FFmpeg releases removed filter_script in favor of file-valued options.
    // Detect the legacy option instead of parsing distributor-specific versions.
    let help = run(crate::media::command("ffmpeg").args(["-hide_banner", "-h", "full"]))?;
    let legacy = String::from_utf8_lossy(&help.stdout).contains("-filter_script");
    let filter_option = if legacy {
        "-filter_script:v"
    } else {
        "-/filter:v"
    };
    let mut command = crate::media::command("ffmpeg");
    seek_input(&mut command, source, source_range)?;
    command
        .args(["-map", "0:v:0", filter_option])
        .arg(&filter_path)
        .args(["-frames:v", &selected.len().to_string(), "-fps_mode", "vfr"])
        .arg(&pattern);
    run(&mut command)?;
    selected
        .into_iter()
        .enumerate()
        .map(|(index, (_, time))| {
            let path = directory.join(format!("frame-{:08}.png", index + 1));
            ensure!(path.is_file(), "keyframe output missing");
            Ok((time - source_range.start_us, path))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn long_selection_extracts_every_requested_frame_without_parser_overflow() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("long.mp4");
        run(crate::media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=32x32:rate=1:duration=320",
                "-c:v",
                "libx264",
            ])
            .arg(&source))
        .unwrap();
        let frames = keyframes(
            &source,
            SourceRange::new(0, 320_000_000).unwrap(),
            &dir.path().join("frames"),
            1.,
        )
        .unwrap();
        assert_eq!(frames.len(), 320);
        for (index, (time, path)) in frames.iter().enumerate() {
            assert_eq!(*time, index as i64 * 1_000_000);
            assert_eq!(image::open(path).unwrap().width(), 32);
        }
    }
    #[test]
    fn seeked_vfr_frames_match_full_decode_pixels_and_original_pts() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("vfr.mp4");
        run(crate::media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x64:rate=10:duration=4",
                "-vf",
                "select='not(eq(mod(n,3),1))'",
                "-fps_mode",
                "vfr",
                "-c:v",
                "libx264",
                "-output_ts_offset",
                "5",
            ])
            .arg(&source))
        .unwrap();
        let pts = super::super::frame_pts(&source).unwrap();
        assert!(pts[0] >= 5_000_000);
        let reference = dir.path().join("reference");
        fs::create_dir(&reference).unwrap();
        run(crate::media::command("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&source)
            .args(["-fps_mode", "vfr"])
            .arg(reference.join("%08d.png")))
        .unwrap();
        let range = SourceRange::new(5_150_000, 7_050_000).unwrap();
        let frames = keyframes(&source, range, &dir.path().join("selected"), 2.).unwrap();
        let mut next = range.start_us;
        let wanted: Vec<_> = pts
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                if **t >= next && **t < range.end_us {
                    next = **t + 500_000;
                    true
                } else {
                    false
                }
            })
            .collect();
        assert_eq!(frames.len(), wanted.len());
        for ((relative, actual), (index, timestamp)) in frames.iter().zip(wanted) {
            assert_eq!(*relative, timestamp - range.start_us);
            let expected = image::open(reference.join(format!("{:08}.png", index + 1)))
                .unwrap()
                .to_rgb8();
            assert_eq!(image::open(actual).unwrap().to_rgb8(), expected);
        }
    }
    #[test]
    fn extracted_clip_and_keyframes_respect_requested_source_interval() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        run(crate::media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x64:rate=10:duration=3",
                "-c:v",
                "libx264",
            ])
            .arg(&source))
        .unwrap();
        let range = SourceRange::new(1_000_000, 2_000_000).unwrap();
        let destination = dir.path().join("clip.mp4");
        video(&source, range, &destination, true).unwrap();
        let info = probe(&destination).unwrap();
        assert_eq!(info.duration_us, 1_000_000);
        let frames = keyframes(&source, range, &dir.path().join("frames"), 2.).unwrap();
        assert_eq!(
            frames.iter().map(|(time, _)| *time).collect::<Vec<_>>(),
            vec![0, 500_000]
        );
    }
}
