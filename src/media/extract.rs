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
        command.args(["-an", "-vf", "scale='min(480,iw)':'min(480,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2,fps=1:round=up"]);
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

/// One still frame at a source timestamp, scaled for display. Input seeking keeps
/// the cost independent of recording length, unlike exact frame selection.
pub fn still(source: &Path, source_us: i64, destination: &Path, width: u32) -> Result<()> {
    ensure!((16..=4096).contains(&width), "invalid still width");
    let parent = destination.parent().context("still has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = tempfile::Builder::new()
        .suffix(".png")
        .tempfile_in(parent)?;
    let mut command = crate::media::command("ffmpeg");
    seek_input(
        &mut command,
        source,
        SourceRange::new(source_us, source_us.saturating_add(1))?,
    )?;
    command
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale='min({width},iw)':-2"),
        ])
        .arg(temp.path());
    run(&mut command)?;
    temp.as_file().sync_all()?;
    temp.persist(destination).map_err(|e| e.error)?;
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
/// Returned relative timestamps come from the same decode as the saved pixels.
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
    let interval = (1_000_000. / fps).round() as i64;
    // Keep integer microseconds through selection and emit the observed PTS on
    // stdout. This avoids a separate ffprobe -show_frames decode and a filter
    // expression that grows with video length. Clear the marker before setting
    // it so source metadata cannot inject lines into this private protocol.
    let filter = format!(
        "settb=1/1000000,select='gte(pts,{})*lt(pts,{})*if(isnan(prev_selected_pts),1,gte(pts-prev_selected_pts,{interval}))',scale='min(1080,iw)':'min(1080,ih)':force_original_aspect_ratio=decrease,metadata=mode=delete:key=cerul.sample,metadata=mode=add:key=cerul.sample:value=1,metadata=mode=print:key=cerul.sample:file='pipe\\:1'",
        source_range.start_us, source_range.end_us
    );
    let pattern = directory.join("frame-%08d.png");
    let origin = probe(source)?.start_us;
    let offset = source_range
        .start_us
        .checked_sub(origin)
        .context("seek time overflow")?;
    ensure!(offset >= 0, "clip starts before the source timeline");
    let mut command = crate::media::command("ffmpeg");
    command.args(["-nostdin", "-v", "error", "-y", "-copyts"]);
    if origin >= 0 {
        command.args([
            "-ss",
            &seconds(offset),
            "-t",
            &seconds(source_range.end_us - source_range.start_us),
        ]);
    } else {
        // Seeking even to offset zero can discard a Matroska stream's negative
        // PTS prefix. Decode from the beginning and let the source-PTS predicate
        // select the requested range. Shared indexing samples still need one pass.
        let duration = source_range
            .end_us
            .checked_sub(origin)
            .context("duration overflow")?;
        command.args(["-t", &seconds(duration)]);
    }
    command.arg("-i").arg(source);
    command
        .args([
            "-map",
            "0:v:0",
            "-vf",
            &filter,
            "-fps_mode",
            "passthrough",
            "-enc_time_base",
            "1:1000000",
            "-avoid_negative_ts",
            "disabled",
        ])
        .arg(&pattern);
    let output = run(&mut command)?;
    let text = std::str::from_utf8(&output.stdout).context("invalid frame timestamps")?;
    let lines: Vec<_> = text.lines().collect();
    let (pairs, remainder) = lines.as_chunks::<2>();
    ensure!(remainder.is_empty(), "incomplete frame timestamps");
    let mut frames = Vec::new();
    let mut previous = None;
    for (index, pair) in pairs.iter().enumerate() {
        let mut fields = pair[0].split_whitespace();
        ensure!(
            fields.next() == Some(format!("frame:{index}").as_str()) && pair[1] == "cerul.sample=1",
            "invalid frame timestamp sequence"
        );
        let time: i64 = fields
            .next()
            .and_then(|field| field.strip_prefix("pts:"))
            .context("missing frame timestamp")?
            .parse()
            .context("invalid frame timestamp")?;
        ensure!(
            time >= source_range.start_us
                && time < source_range.end_us
                && previous.is_none_or(|before| time > before),
            "frame timestamp outside ordered source interval"
        );
        previous = Some(time);
        let path = directory.join(format!("frame-{:08}.png", index + 1));
        ensure!(path.is_file(), "keyframe output missing");
        frames.push((time - source_range.start_us, path));
    }
    Ok(frames)
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
        assert_sampled_timeline("mp4", "libx264", "10", "5");
    }

    #[test]
    fn fractional_rates_preserve_negative_and_large_source_timestamps() {
        assert_sampled_timeline("mkv", "ffv1", "30000/1001", "-0.3");
        assert_sampled_timeline("mp4", "libx264", "30000/1001", "7200");
    }

    fn assert_sampled_timeline(extension: &str, codec: &str, rate: &str, offset: &str) {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(format!("vfr.{extension}"));
        run(crate::media::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("testsrc2=size=64x64:rate={rate}:duration=4"),
                "-vf",
                "select='not(eq(mod(n,3),1))'",
                "-fps_mode",
                "vfr",
                "-c:v",
                codec,
                "-output_ts_offset",
                offset,
                "-avoid_negative_ts",
                "disabled",
            ])
            .arg(&source))
        .unwrap();
        let pts = super::super::frame_pts(&source).unwrap();
        assert_eq!(pts[0] < 0, offset.starts_with('-'));
        let reference = dir.path().join("reference");
        fs::create_dir(&reference).unwrap();
        run(crate::media::command("ffmpeg")
            .args(["-v", "error", "-copyts", "-i"])
            .arg(&source)
            .args([
                "-fps_mode",
                "passthrough",
                "-enc_time_base",
                "1:1000000",
                "-avoid_negative_ts",
                "disabled",
            ])
            .arg(reference.join("%08d.png")))
        .unwrap();
        let range = SourceRange::new(pts[0] + 150_000, pts[0] + 2_050_000).unwrap();
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
