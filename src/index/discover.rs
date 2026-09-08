use crate::{
    episode::{Episode, Source, Stream, Timeline},
    media, storage,
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RegistryEntry {
    pub episode_id: String,
    pub sha256: String,
    pub media: PathBuf,
    pub sidecar: PathBuf,
    /// Durable intent lets an explicit cleanup resume a partially removed directory.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pending_deletion: bool,
}

pub fn read_registry(workspace: &Path) -> Result<Vec<RegistryEntry>> {
    let text = match fs::read_to_string(workspace.join("registry.jsonl")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    text.lines()
        .enumerate()
        .map(|(n, line)| {
            serde_json::from_str(line).with_context(|| format!("invalid registry line {}", n + 1))
        })
        .collect()
}

/// Caller holds the workspace write lock; registry publication is atomic.
pub fn register(workspace: &Path, entry: RegistryEntry) -> Result<()> {
    register_inner(workspace, entry, false)
}
fn register_inner(workspace: &Path, entry: RegistryEntry, replace_media: bool) -> Result<()> {
    let mut entries = read_registry(workspace)?;
    // LeRobot episodes may share one media shard, but never one sidecar.
    entries.retain(|existing| {
        existing.episode_id != entry.episode_id
            && existing.sidecar != entry.sidecar
            && !(replace_media && existing.media == entry.media)
    });
    entries.push(entry);
    entries.sort_by(|a, b| a.episode_id.cmp(&b.episode_id));
    let mut bytes = Vec::new();
    for entry in entries {
        serde_json::to_writer(&mut bytes, &entry)?;
        bytes.push(b'\n');
    }
    storage::atomic_write(&workspace.join("registry.jsonl"), &bytes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Video(PathBuf),
    LeRobot(PathBuf),
}

pub fn discover(paths: &[PathBuf]) -> Result<Vec<Input>> {
    fn visit(path: &Path, explicit: bool, output: &mut Vec<Input>) -> Result<()> {
        let metadata =
            fs::symlink_metadata(path).with_context(|| format!("inspect {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            if explicit {
                return visit(&fs::canonicalize(path)?, true, output);
            }
            return Ok(());
        }
        if metadata.is_dir() {
            if path.join("meta/info.json").is_file() {
                output.push(Input::LeRobot(fs::canonicalize(path)?));
                return Ok(());
            }
            let mut children = fs::read_dir(path)?
                .map(|entry| entry.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            children.sort();
            for child in children {
                let name = child.file_name().unwrap().to_string_lossy();
                if name.starts_with('.') || name.ends_with(".cerul") {
                    continue;
                }
                visit(&child, false, output)?;
            }
        } else if metadata.is_file() {
            let supported = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                [
                    "mp4", "mov", "mkv", "webm", "avi", "m4v", "mpeg", "mpg", "mts", "m2ts",
                ]
                .contains(&e.to_ascii_lowercase().as_str())
            });
            ensure!(
                !explicit || supported,
                "unsupported video extension: {}",
                path.display()
            );
            if supported {
                let input = Input::Video(fs::canonicalize(path)?);
                if !output.contains(&input) {
                    output.push(input);
                }
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    for path in paths {
        visit(path, true, &mut output)?;
    }
    Ok(output)
}

pub fn ordinary_episode(path: &Path) -> Result<Episode> {
    let path = fs::canonicalize(path)?;
    let hash = media::sha256(&path)?;
    let probe = media::probe(&path)?;
    let dataset_id = hash[..16].to_owned();
    let end = probe
        .start_us
        .checked_add(probe.duration_us)
        .context("media end time overflow")?;
    let episode = Episode {
        schema: "episode/1".into(),
        episode_id: format!("{dataset_id}/0"),
        dataset_id,
        local_id: "0".into(),
        streams: vec![Stream::Video {
            id: "primary".into(),
            primary: true,
            path: path.file_name().context("video has no filename")?.into(),
            sha256: hash,
            range_us: [probe.start_us, end],
            probe,
        }],
        time: Timeline {
            reference: "primary".into(),
            mappings: BTreeMap::new(),
        },
        task: None,
        source: Source {
            format: "video".into(),
            root: path.parent().context("video has no parent")?.into(),
        },
    };
    episode.validate()?;
    Ok(episode)
}

/// Resolve a previous sidecar by full content hash after a media move.
pub fn sidecar_path(
    episode: &Episode,
    registry: &[RegistryEntry],
    override_dir: Option<&Path>,
) -> Result<PathBuf> {
    let absolute_override = override_dir.map(std::path::absolute).transpose()?;
    let override_dir = absolute_override.as_deref();
    if episode.source.format.starts_with("lerobot/") {
        if override_dir.is_none()
            && let Some(existing) = registry
                .iter()
                .find(|entry| entry.episode_id == episode.episode_id && entry.sidecar.is_dir())
        {
            return Ok(existing.sidecar.clone());
        }
        return Ok(match override_dir {
            Some(directory) => directory.join(&episode.dataset_id).join(&episode.local_id),
            None => episode
                .source
                .root
                .join(".cerul/episodes")
                .join(&episode.local_id),
        });
    }
    let Stream::Video { path, sha256, .. } = episode.video(&episode.time.reference)? else {
        unreachable!()
    };
    if let Some(directory) = override_dir {
        return Ok(directory.join(sha256));
    }
    if let Some(existing) = registry
        .iter()
        .find(|entry| entry.sha256 == *sha256 && entry.sidecar.is_dir())
    {
        return Ok(existing.sidecar.clone());
    }
    let media = episode.source.root.join(path);
    let mut name = media.as_os_str().to_os_string();
    name.push(".cerul");
    let preferred = PathBuf::from(name);
    if preferred.join("episode.json").is_file() {
        let previous: Episode = serde_json::from_slice(&fs::read(preferred.join("episode.json"))?)?;
        if previous.episode_id != episode.episode_id {
            // Preserve annotations belonging to the replaced content. Publishing
            // a new episode over them would make provenance validation fail.
            let mut versioned = media.as_os_str().to_os_string();
            versioned.push(format!(".{sha256}.cerul"));
            return Ok(versioned.into());
        }
    }
    Ok(preferred)
}

/// Publish only local metadata here. Remote station work follows separately.
pub fn publish_episode(
    workspace: &Path,
    episode: &Episode,
    override_dir: Option<&Path>,
) -> Result<PathBuf> {
    episode.validate()?;
    if episode.source.format.starts_with("lerobot/") {
        crate::lerobot::publish_identity(episode, workspace)?;
    }
    let registry = read_registry(workspace)?;
    let Stream::Video { path, sha256, .. } = episode.video(&episode.time.reference)? else {
        unreachable!()
    };
    let media = episode.source.root.join(path);
    let preferred = sidecar_path(episode, &registry, override_dir)?;
    let sidecar = match storage::write_json(&preferred.join("episode.json"), episode) {
        Ok(()) => preferred,
        Err(error) if override_dir.is_none() && crate::lerobot::permission_error(&error) => {
            let fallback = if episode.source.format.starts_with("lerobot/") {
                workspace
                    .join("sidecars")
                    .join(&episode.dataset_id)
                    .join(&episode.local_id)
            } else {
                workspace.join("sidecars").join(sha256)
            };
            storage::write_json(&fallback.join("episode.json"), episode)?;
            fallback
        }
        Err(error) => return Err(error),
    };
    let sidecar = std::path::absolute(sidecar)?;
    register_inner(
        workspace,
        RegistryEntry {
            episode_id: episode.episode_id.clone(),
            sha256: sha256.clone(),
            media,
            sidecar: sidecar.clone(),
            pending_deletion: false,
        },
        episode.source.format == "video",
    )?;
    Ok(sidecar)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_replaces_shared_sidecar_but_preserves_shared_media_episodes() {
        let dir = tempfile::tempdir().unwrap();
        for (id, sidecar) in [
            ("old/0", "video.cerul"),
            ("new/0", "video.cerul"),
            ("dataset/1", "episode1"),
        ] {
            register(
                dir.path(),
                RegistryEntry {
                    episode_id: id.into(),
                    sha256: id.into(),
                    media: "shared.mp4".into(),
                    sidecar: sidecar.into(),
                    pending_deletion: false,
                },
            )
            .unwrap();
        }
        let entries = read_registry(dir.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.episode_id != "old/0"));
    }
    #[test]
    fn discovery_skips_sidecars_and_recognizes_dataset_once() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("clip.MP4"), b"fixture").unwrap();
        fs::create_dir_all(dir.path().join("clip.MP4.cerul")).unwrap();
        fs::write(dir.path().join("clip.MP4.cerul/proxy.mp4"), b"cache").unwrap();
        fs::create_dir_all(dir.path().join("dataset/meta")).unwrap();
        fs::write(dir.path().join("dataset/meta/info.json"), b"{}").unwrap();
        fs::write(dir.path().join("dataset/video.mp4"), b"shared").unwrap();
        let found = discover(&[dir.path().into()]).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|i| matches!(i, Input::LeRobot(_))));
    }
    #[test]
    fn media_move_reuses_sidecar_and_preserves_identity() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        let first = dir.path().join("first.mp4");
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=64x64:rate=2:duration=1",
                    "-c:v",
                    "libx264",
                ])
                .arg(&first),
        )
        .unwrap();
        let episode = ordinary_episode(&first).unwrap();
        let sidecar = publish_episode(&workspace, &episode, None).unwrap();
        let second = dir.path().join("second.mp4");
        fs::rename(first, &second).unwrap();
        let moved = ordinary_episode(&second).unwrap();
        assert_eq!(episode.episode_id, moved.episode_id);
        assert_eq!(publish_episode(&workspace, &moved, None).unwrap(), sidecar);
        let registry = read_registry(&workspace).unwrap();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry[0].media, fs::canonicalize(&second).unwrap());
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=red:size=64x64:rate=2:duration=1",
                    "-c:v",
                    "libx264",
                ])
                .arg(&second),
        )
        .unwrap();
        let replacement = ordinary_episode(&second).unwrap();
        assert_ne!(replacement.episode_id, moved.episode_id);
        let replacement_sidecar = publish_episode(&workspace, &replacement, None).unwrap();
        // Replacing the same path again must preserve the prior content sidecar.
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:size=64x64:rate=2:duration=1",
                    "-c:v",
                    "libx264",
                ])
                .arg(&second),
        )
        .unwrap();
        let newest = ordinary_episode(&second).unwrap();
        assert_ne!(
            publish_episode(&workspace, &newest, None).unwrap(),
            replacement_sidecar
        );
        let preserved: Episode =
            serde_json::from_slice(&fs::read(replacement_sidecar.join("episode.json")).unwrap())
                .unwrap();
        assert_eq!(preserved.episode_id, replacement.episode_id);
        let registry = read_registry(&workspace).unwrap();
        assert_eq!(registry.len(), 1);
        assert!(
            !registry
                .iter()
                .any(|entry| entry.episode_id == replacement.episode_id)
        );
        for entry in registry {
            let stored: Episode =
                serde_json::from_slice(&fs::read(entry.sidecar.join("episode.json")).unwrap())
                    .unwrap();
            assert_eq!(stored.episode_id, entry.episode_id);
        }
        let cwd = std::env::current_dir().unwrap();
        let override_root = tempfile::tempdir_in(&cwd).unwrap();
        let relative = override_root
            .path()
            .strip_prefix(&cwd)
            .unwrap()
            .join("not-created-yet");
        let published = publish_episode(&workspace, &newest, Some(&relative)).unwrap();
        assert!(published.is_absolute());
        assert_eq!(read_registry(&workspace).unwrap()[0].sidecar, published);
        assert!(published.join("episode.json").is_file());
    }
}
