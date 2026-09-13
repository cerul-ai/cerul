//! Shared timestamped 1 FPS samples. Consumers choose their own resolution.
use super::extract::SourceRange;
use crate::storage;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const RECIPE: &str = "index-frames/2";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Recipe {
    version: String,
    source_sha256: String,
    start_us: i64,
    end_us: i64,
    fps: u32,
    max_edge: u32,
}
#[derive(Debug, Serialize, Deserialize)]
struct Frame {
    relative_us: i64,
    sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    recipe: Recipe,
    frames: Vec<Frame>,
}
fn filename(frame: &Frame) -> String {
    format!("{}-{}.png", frame.relative_us, frame.sha256)
}
fn selected(
    manifest: &Manifest,
    directory: &Path,
    range: SourceRange,
) -> Result<Vec<(i64, PathBuf)>> {
    let mut previous = None;
    let mut output = Vec::new();
    for frame in &manifest.frames {
        ensure!(
            frame.relative_us >= 0
                && frame.relative_us < manifest.recipe.end_us - manifest.recipe.start_us
                && previous.is_none_or(|time| time < frame.relative_us),
            "invalid cached frame time"
        );
        ensure!(
            frame.sha256.len() == 64 && frame.sha256.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid cached frame hash"
        );
        previous = Some(frame.relative_us);
        let source_us = manifest.recipe.start_us + frame.relative_us;
        if source_us < range.start_us || source_us >= range.end_us {
            continue;
        }
        let path = directory.join(filename(frame));
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "frame cache contains a symlink"
        );
        ensure!(
            super::sha256(&path)? == frame.sha256,
            "cached frame changed"
        );
        output.push((frame.relative_us, path));
    }
    Ok(output)
}

/// Return sample times relative to `source_range`, retaining actual source PTS.
/// Only requested images are hashed on cache reads, not the whole recording.
pub fn get(
    source: &Path,
    source_sha256: &str,
    source_range: SourceRange,
    requested: SourceRange,
    workspace: &Path,
) -> Result<Vec<(i64, PathBuf)>> {
    super::check_cancellation()?;
    ensure!(
        source_sha256.len() == 64 && source_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid frame source hash"
    );
    ensure!(
        requested.start_us >= source_range.start_us && requested.end_us <= source_range.end_us,
        "frame request outside source range"
    );
    let recipe = Recipe {
        version: RECIPE.into(),
        source_sha256: source_sha256.into(),
        start_us: source_range.start_us,
        end_us: source_range.end_us,
        fps: 1,
        max_edge: 1080,
    };
    let key = storage::cache_key(&recipe)?;
    let directory = workspace
        .join("cache")
        .join(source_sha256)
        .join("frames")
        .join(key);
    for path in directory.ancestors().take_while(|path| *path != workspace) {
        if let Ok(meta) = fs::symlink_metadata(path) {
            ensure!(
                !meta.file_type().is_symlink(),
                "frame cache contains a symlink"
            );
        }
    }
    let manifest_path = directory.join("frames.json");
    if let Ok(bytes) = fs::read(&manifest_path)
        && let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes)
        && manifest.recipe == recipe
        && let Ok(frames) = selected(&manifest, &directory, requested)
    {
        return Ok(frames);
    }
    fs::create_dir_all(&directory)?;
    let stage = tempfile::tempdir_in(&directory)?;
    let extracted = super::extract::keyframes(source, source_range, stage.path(), 1.)?;
    let mut frames = Vec::new();
    for (relative_us, path) in extracted {
        super::check_cancellation()?;
        let frame = Frame {
            relative_us,
            sha256: super::sha256(&path)?,
        };
        let target = directory.join(filename(&frame));
        ensure!(
            fs::symlink_metadata(&target).map_or(true, |m| !m.file_type().is_symlink()),
            "frame cache contains a symlink"
        );
        fs::rename(path, target)?;
        frames.push(frame);
    }
    let manifest = Manifest { recipe, frames };
    storage::write_json(&manifest_path, &manifest)?;
    selected(&manifest, &directory, requested)
}

/// A proxy is encoded from already sampled pixels; the original source is not
/// decoded again for each overlapping embedding/understanding window.
pub fn encode_proxy(
    frames: &[(i64, PathBuf)],
    source_start_us: i64,
    range: SourceRange,
    destination: &Path,
    crf: u8,
) -> Result<()> {
    ensure!(!frames.is_empty(), "proxy has no observed samples");
    let stage = tempfile::tempdir()?;
    let mut list = String::from("ffconcat version 1.0\n");
    for (i, (relative_us, source)) in frames.iter().enumerate() {
        let name = format!("frame-{i:08}.png");
        let target = stage.path().join(&name);
        if fs::hard_link(source, &target).is_err() {
            fs::copy(source, &target)?;
        }
        let start = if i == 0 {
            range.start_us
        } else {
            source_start_us + relative_us
        };
        let end = frames
            .get(i + 1)
            .map(|(t, _)| source_start_us + t)
            .unwrap_or(range.end_us);
        list.push_str(&format!(
            "file '{name}'\nduration {}\n",
            super::seconds(end - start)
        ));
    }
    // Concat needs a final file to honor the last image's duration. Output is
    // explicitly bounded to the requested half-open interval.
    list.push_str(&format!("file 'frame-{:08}.png'\n", frames.len() - 1));
    let manifest = stage.path().join("input.ffconcat");
    fs::write(&manifest, list)?;
    super::run(super::command("ffmpeg").args(["-nostdin", "-v", "error", "-y", "-f", "concat", "-safe", "1", "-i"])
        .arg(&manifest).args(["-an", "-vf", "scale='min(480,iw)':'min(480,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2,fps=1:round=up",
            "-frames:v", &((range.end_us - range.start_us + 999_999) / 1_000_000).to_string(),
            "-c:v", "libx264", "-preset", "fast", "-crf", &crf.to_string(), "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(destination))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_samples_encode_overlapping_windows_without_decoding_source_again() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        super::super::run(
            super::super::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=640x480:rate=10:duration=4",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let hash = super::super::sha256(&source).unwrap();
        let range = SourceRange::new(0, 4_000_000).unwrap();
        let frames = get(&source, &hash, range, range, dir.path()).unwrap();
        assert_eq!(frames.len(), 4);
        assert_eq!(image::open(&frames[0].1).unwrap().width(), 640);
        fs::rename(&source, dir.path().join("unavailable.mp4")).unwrap();
        for (start, end) in [(0, 3_000_000), (1_000_000, 4_000_000)] {
            let proxy = super::super::proxy::get_with_samples(
                &source,
                &hash,
                SourceRange::new(start, end).unwrap(),
                range,
                dir.path(),
                24,
            )
            .unwrap();
            let probe = super::super::probe(&proxy).unwrap();
            assert_eq!(probe.duration_us, 3_000_000);
            assert_eq!(probe.width, 480);
            assert_eq!(probe.fps, "1/1");
        }
    }
}
