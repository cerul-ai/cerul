//! Sample-preserving contact proxies keep original PTS separate from encoding cadence.
use super::extract::SourceRange;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};
pub const RECIPE_VERSION: &str = "contact-proxy/1";
#[derive(Serialize, Deserialize)]
struct Manifest {
    key: String,
    output_sha256: String,
    times: Vec<i64>,
    times_hash: String,
}
fn valid_times(times: &[i64], duration: i64) -> bool {
    !times.is_empty()
        && times.len() <= 600
        && times.iter().all(|t| *t >= 0 && *t < duration)
        && times.windows(2).all(|p| p[0] < p[1])
}
/// Extract cached samples for rendering; returned times are original source offsets.
pub fn frames(
    source: &Path,
    hash: &str,
    range: SourceRange,
    fps: f64,
    workspace: &Path,
    destination: &Path,
) -> Result<Vec<(i64, PathBuf)>> {
    super::check_cancellation()?;
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid contact source hash"
    );
    ensure!(
        fps.is_finite() && fps > 0. && fps <= 10.,
        "invalid contact FPS"
    );
    SourceRange::new(range.start_us, range.end_us)?;
    let key = crate::storage::cache_key(&(
        RECIPE_VERSION,
        hash,
        range.start_us,
        range.end_us,
        fps,
        480,
        24,
    ))?;
    let dir = workspace.join("cache").join(hash).join("contact");
    let video = dir.join(format!("{key}.mp4"));
    let manifest_path = dir.join(format!("{key}.json"));
    for path in std::iter::once(video.as_path())
        .chain(std::iter::once(manifest_path.as_path()))
        .chain(dir.ancestors().take_while(|p| *p != workspace))
    {
        match fs::symlink_metadata(path) {
            Ok(meta) => ensure!(
                !meta.file_type().is_symlink(),
                "contact cache contains a symlink"
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    let cached = fs::read(&manifest_path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok());
    let manifest = match cached {
        Some(m)
            if m.key == key
                && valid_times(&m.times, range.end_us - range.start_us)
                && crate::storage::cache_key(&m.times)? == m.times_hash
                && video.is_file()
                && super::sha256(&video)? == m.output_sha256 =>
        {
            m
        }
        _ => {
            fs::create_dir_all(&dir)?;
            let stage = tempfile::tempdir_in(&dir)?;
            let sampled = super::extract::keyframes(source, range, stage.path(), fps)?;
            let times: Vec<_> = sampled.iter().map(|(t, _)| *t).collect();
            ensure!(
                valid_times(&times, range.end_us - range.start_us),
                "invalid contact sample timeline"
            );
            let encoded = stage.path().join("contact.mp4");
            super::run(crate::media::command("ffmpeg").args(["-v", "error", "-framerate", &fps.to_string(), "-i"])
                .arg(stage.path().join("frame-%08d.png"))
                .args(["-vf", "scale='min(480,iw)':'min(480,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2", "-c:v", "libx264", "-crf", "24", "-preset", "fast", "-pix_fmt", "yuv420p", "-an", "-movflags", "+faststart"])
                .arg(&encoded))?;
            ensure!(
                super::frame_pts_uncached(&encoded)?.len() == times.len(),
                "contact encoding changed frame count"
            );
            let m = Manifest {
                key: key.clone(),
                output_sha256: super::sha256(&encoded)?,
                times_hash: crate::storage::cache_key(&times)?,
                times,
            };
            super::check_cancellation()?;
            File::open(&encoded)?.sync_all()?;
            fs::rename(encoded, &video)?;
            File::open(&dir)?.sync_all()?;
            crate::storage::write_json(&manifest_path, &m)?;
            m
        }
    };
    fs::create_dir_all(destination)?;
    super::run(
        crate::media::command("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&video)
            .args(["-fps_mode", "passthrough"])
            .arg(destination.join("sample-%08d.png")),
    )?;
    manifest
        .times
        .into_iter()
        .enumerate()
        .map(|(i, time)| {
            let path = destination.join(format!("sample-{:08}.png", i + 1));
            ensure!(path.is_file(), "cached contact sample missing");
            Ok((time, path))
        })
        .collect()
}
