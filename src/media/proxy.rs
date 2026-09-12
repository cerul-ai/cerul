//! Disposable, validated proxies keyed by source content, interval, and encoding recipe.
use super::extract::SourceRange;
pub const RECIPE_VERSION: &str = "proxy/4";
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Recipe {
    version: String,
    source_sha256: String,
    start_us: i64,
    end_us: i64,
    sample_start_us: i64,
    sample_end_us: i64,
    crf: u8,
    max_edge: u32,
    fps: u32,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    recipe: Recipe,
    output_sha256: String,
}
/// `source_sha256` is the content hash already verified during episode discovery.
pub fn get(
    source: &Path,
    source_sha256: &str,
    range: SourceRange,
    workspace: &Path,
    crf: u8,
) -> Result<PathBuf> {
    get_with_samples(source, source_sha256, range, range, workspace, crf)
}

pub fn get_with_samples(
    source: &Path,
    source_sha256: &str,
    range: SourceRange,
    sample_range: SourceRange,
    workspace: &Path,
    crf: u8,
) -> Result<PathBuf> {
    super::check_cancellation()?;
    ensure!(
        source_sha256.len() == 64 && source_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid proxy source hash"
    );
    ensure!(crf <= 51, "invalid proxy quality");
    SourceRange::new(range.start_us, range.end_us)?;
    let recipe = Recipe {
        version: RECIPE_VERSION.into(),
        source_sha256: source_sha256.into(),
        start_us: range.start_us,
        end_us: range.end_us,
        sample_start_us: sample_range.start_us,
        sample_end_us: sample_range.end_us,
        crf,
        max_edge: 480,
        fps: 1,
    };
    let key = crate::storage::cache_key(&recipe)?;
    let directory = workspace.join("cache").join(source_sha256).join("proxies");
    let video = directory.join(format!("{key}.mp4"));
    let manifest_path = directory.join(format!("{key}.json"));
    for path in std::iter::once(video.as_path())
        .chain(std::iter::once(manifest_path.as_path()))
        .chain(directory.ancestors().take_while(|path| *path != workspace))
    {
        match fs::symlink_metadata(path) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "proxy cache contains a symlink"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    if video.is_file()
        && let Ok(bytes) = fs::read(&manifest_path)
        && let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes)
        && manifest.recipe == recipe
        && super::sha256(&video)? == manifest.output_sha256
    {
        return Ok(video);
    }
    fs::create_dir_all(&directory)?;
    let stage = tempfile::tempdir_in(&directory)?;
    let staged_video = stage.path().join("proxy.mp4");
    let frames = super::frames::get(source, source_sha256, sample_range, range, workspace)?;
    if frames.is_empty() {
        // A subsecond tail can fall between shared samples. Decode that exact
        // interval instead of borrowing a frame from outside its evidence.
        super::extract::video_quality(source, range, &staged_video, true, crf)?;
    } else {
        super::frames::encode_proxy(&frames, sample_range.start_us, range, &staged_video, crf)?;
    }
    let probe = super::probe(&staged_video)?;
    ensure!(
        probe.width <= 480
            && probe.height <= 480
            && probe.codec == "h264"
            && probe.fps == "1/1"
            && !probe.has_audio,
        "invalid encoded proxy"
    );
    let duration = range.end_us - range.start_us;
    ensure!(
        probe.duration_us.abs_diff(duration) < 1_000_000,
        "proxy duration differs from requested interval"
    );
    let manifest = Manifest {
        recipe,
        output_sha256: super::sha256(&staged_video)?,
    };
    super::check_cancellation()?;
    fs::rename(&staged_video, &video)?;
    File::open(&directory)?.sync_all()?;
    crate::storage::write_json(&manifest_path, &manifest)?;
    Ok(video)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proxies_reuse_and_invalidate_by_recipe_content_and_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        super::super::run(
            crate::media::command("ffmpeg")
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
        let workspace = dir.path().join("workspace");
        let range = SourceRange::new(0, 2_000_000).unwrap();
        let first = get(&source, &hash, range, &workspace, 24).unwrap();
        let probe = super::super::probe(&first).unwrap();
        assert_eq!(probe.fps, "1/1");
        assert!(!probe.has_audio);
        // A 1 FPS proxy must still contain a frame for a subsecond tail and
        // preserve the requested source interval within one sampling period.
        for (start, end) in [(0, 200_000), (500_000, 1_800_000), (3_200_000, 4_000_000)] {
            let clip = get(
                &source,
                &hash,
                SourceRange::new(start, end).unwrap(),
                &workspace,
                24,
            )
            .unwrap();
            let tail = super::super::probe(&clip).unwrap();
            assert!(tail.duration_us > 0);
            assert!(tail.duration_us.abs_diff(end - start) < 1_000_000);
        }
        let modified = fs::metadata(&first).unwrap().modified().unwrap();
        // The cache hit needs neither the source file nor a new ffmpeg invocation.
        let moved = dir.path().join("moved.mp4");
        fs::rename(&source, &moved).unwrap();
        assert_eq!(get(&source, &hash, range, &workspace, 24).unwrap(), first);
        assert_eq!(fs::metadata(&first).unwrap().modified().unwrap(), modified);
        fs::rename(&moved, &source).unwrap();
        assert_ne!(get(&source, &hash, range, &workspace, 45).unwrap(), first);
        assert_ne!(
            get(
                &source,
                &hash,
                SourceRange::new(1_000_000, 3_000_000).unwrap(),
                &workspace,
                24
            )
            .unwrap(),
            first
        );
        fs::write(&first, b"corrupt cache").unwrap();
        assert_eq!(get(&source, &hash, range, &workspace, 24).unwrap(), first);
        assert!(super::super::probe(&first).is_ok());
        let mut bytes = fs::read(&source).unwrap();
        bytes.extend_from_slice(b"trailing metadata");
        fs::write(&source, bytes).unwrap();
        let changed = super::super::sha256(&source).unwrap();
        assert_ne!(
            get(&source, &changed, range, &workspace, 24).unwrap(),
            first
        );
    }
}
