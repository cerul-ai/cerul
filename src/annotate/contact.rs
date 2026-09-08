//! Five-column contact sheets with explicit window-relative, microsecond timestamps.
use crate::{
    episode::{Episode, Stream, TimeRange},
    media::{self, extract::SourceRange},
};
use anyhow::{Result, ensure};
use image::{Rgb, RgbImage};

pub struct Sheet {
    pub jpeg: Vec<u8>,
    pub frame_times_us: Vec<i64>,
}
fn glyph(character: char) -> [u8; 7] {
    match character {
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        's' => [0, 0, 15, 16, 14, 1, 30],
        _ => [0; 7],
    }
}
/// Fixed glyphs avoid a runtime font dependency and make labels deterministic.
fn label(image: &mut RgbImage, x: u32, y: u32, time_us: i64) {
    let text = format!("{}.{:06}s", time_us / 1_000_000, time_us % 1_000_000);
    for (index, character) in text.chars().enumerate() {
        for (row, bits) in glyph(character).iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) == 0 {
                    continue;
                }
                for dy in 0..3 {
                    for dx in 0..3 {
                        image.put_pixel(
                            x + index as u32 * 18 + column * 3 + dx,
                            y + row as u32 * 3 + dy,
                            Rgb([255, 255, 255]),
                        );
                    }
                }
            }
        }
    }
}
pub fn build(episode: &Episode, stream: &str, window: TimeRange, fps: f64) -> Result<Sheet> {
    build_inner(episode, stream, window, fps, None)
}
pub fn build_cached(
    episode: &Episode,
    stream: &str,
    window: TimeRange,
    fps: f64,
    workspace: &std::path::Path,
) -> Result<Sheet> {
    build_inner(episode, stream, window, fps, Some(workspace))
}
fn build_inner(
    episode: &Episode,
    stream: &str,
    window: TimeRange,
    fps: f64,
    workspace: Option<&std::path::Path>,
) -> Result<Sheet> {
    ensure!(
        window.end_us <= episode.duration_us()?,
        "contact window exceeds episode"
    );
    ensure!(
        window.end_us - window.start_us <= 300_000_000,
        "contact window exceeds five minutes"
    );
    ensure!(
        fps.is_finite() && fps > 0. && fps <= 10.,
        "contact FPS must be in (0,10]"
    );
    let Stream::Video { path, sha256, .. } = episode.video(stream)? else {
        unreachable!()
    };
    let coverage = episode
        .video_coverage(stream)?
        .and_then(|coverage| coverage.intersection(window))
        .ok_or_else(|| anyhow::anyhow!("no stream coverage in annotation window"))?;
    let source = episode.source.root.join(path);
    let range = SourceRange::new(
        episode.episode_to_source(stream, coverage.start_us)?,
        episode.episode_to_source(stream, coverage.end_us)?,
    )?;
    let directory = tempfile::tempdir()?;
    let frames = if let Some(workspace) = workspace {
        media::contact_proxy::frames(&source, sha256, range, fps, workspace, directory.path())?
    } else {
        media::extract::keyframes(&source, range, directory.path(), fps)?
    };
    ensure!(!frames.is_empty(), "no video frames in annotation window");
    ensure!(
        frames.len() <= 600,
        "contact sheet has too many frames; reduce window or FPS"
    );
    let width = 480;
    let height = 302;
    let mut canvas = RgbImage::from_pixel(
        width * 5,
        height * (frames.len() as u32).div_ceil(5),
        Rgb([0, 0, 0]),
    );
    let mut times = Vec::new();
    for (index, (relative, path)) in frames.iter().enumerate() {
        let time = episode.source_to_episode(stream, range.start_us + relative)? - window.start_us;
        ensure!(
            time >= 0 && time < window.end_us - window.start_us,
            "contact frame outside window"
        );
        times.push(time);
        let image = image::open(path)?.thumbnail(480, 270).to_rgb8();
        let x = index as u32 % 5 * width;
        let y = index as u32 / 5 * height;
        image::imageops::replace(
            &mut canvas,
            &image,
            (x + (480 - image.width()) / 2) as i64,
            (y + (270 - image.height()) / 2) as i64,
        );
        label(&mut canvas, x + 8, y + 275, time);
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&canvas)?;
    Ok(Sheet {
        jpeg,
        frame_times_us: times,
    })
}
/// Model boundaries are snapped on the episode timeline after adding the window start once.
pub fn snap(window: TimeRange, relative_us: i64, frame_pts: &[i64]) -> Result<i64> {
    ensure!(
        relative_us >= 0 && relative_us <= window.end_us - window.start_us,
        "model time outside input window"
    );
    let absolute = window.start_us + relative_us;
    // The final half-open endpoint is a boundary, not the PTS of a frame.
    if absolute == window.end_us {
        return Ok(absolute);
    }
    frame_pts
        .iter()
        .copied()
        .filter(|t| *t >= window.start_us && *t < window.end_us)
        .min_by_key(|t| t.abs_diff(absolute))
        .ok_or_else(|| anyhow::anyhow!("no frame boundaries in window"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_contact_preserves_vfr_times_and_rebuilds_corrupt_metadata() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=10:duration=3",
                    "-vf",
                    "select='not(eq(mod(n,3),1))'",
                    "-fps_mode",
                    "vfr",
                    "-c:v",
                    "libx264",
                    "-output_ts_offset",
                    "5",
                ])
                .arg(&source),
        )
        .unwrap();
        let episode = crate::index::discover::ordinary_episode(&source).unwrap();
        let window = TimeRange::new(150_000, 2_050_000).unwrap();
        let workspace = dir.path().join("workspace");
        let original = build(&episode, "primary", window, 2.).unwrap();
        let cached = build_cached(&episode, "primary", window, 2., &workspace).unwrap();
        assert_eq!(cached.frame_times_us, original.frame_times_us);
        let moved = dir.path().join("moved.mp4");
        fs::rename(&source, &moved).unwrap();
        assert_eq!(
            build_cached(&episode, "primary", window, 2., &workspace)
                .unwrap()
                .jpeg,
            cached.jpeg
        );
        assert!(build_cached(&episode, "primary", window, 10., &workspace).is_err());
        fs::rename(&moved, &source).unwrap();
        let dense = build_cached(&episode, "primary", window, 10., &workspace).unwrap();
        assert_eq!(
            dense.frame_times_us,
            build(&episode, "primary", window, 10.)
                .unwrap()
                .frame_times_us
        );
        assert!(dense.frame_times_us.len() > cached.frame_times_us.len());
        let Stream::Video { sha256, .. } = episode.video("primary").unwrap() else {
            unreachable!()
        };
        for entry in fs::read_dir(workspace.join("cache").join(sha256).join("contact")).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "json") {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                value["times"][0] = serde_json::json!(1);
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
        }
        assert_eq!(
            build_cached(&episode, "primary", window, 2., &workspace)
                .unwrap()
                .frame_times_us,
            original.frame_times_us
        );
    }
    #[test]
    fn contact_labels_and_boundaries_use_window_time_once() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=10:duration=4",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let episode = crate::index::discover::ordinary_episode(&source).unwrap();
        let window = TimeRange::new(2_000_000, 4_000_000).unwrap();
        let sheet = build(&episode, "primary", window, 2.).unwrap();
        assert_eq!(sheet.frame_times_us, vec![0, 500_000, 1_000_000, 1_500_000]);
        let decoded = image::load_from_memory(&sheet.jpeg).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2400, 302));
        assert_eq!(
            snap(
                window,
                550_000,
                &[2_000_000, 2_500_000, 2_600_000, 3_000_000]
            )
            .unwrap(),
            2_500_000
        );
        assert_eq!(
            snap(window, 2_000_000, &[2_000_000, 3_000_000]).unwrap(),
            4_000_000
        );
        assert!(snap(window, 2_000_001, &[2_000_000]).is_err());
        let mut sliced = episode;
        if let Stream::Video { range_us, .. } = &mut sliced.streams[0] {
            *range_us = [1_000_000, 4_000_000];
        }
        let sheet = build(
            &sliced,
            "primary",
            TimeRange::new(1_000_000, 3_000_000).unwrap(),
            2.,
        )
        .unwrap();
        assert_eq!(sheet.frame_times_us, vec![0, 500_000, 1_000_000, 1_500_000]);
    }
}
