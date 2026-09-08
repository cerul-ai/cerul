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
                let input = Input::LeRobot(fs::canonicalize(path)?);
                if !output.contains(&input) {
                    output.push(input);
                }
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

fn source_keys(episode: &Episode) -> Result<BTreeMap<String, String>> {
    episode
        .streams
        .iter()
        .filter(|stream| matches!(stream, Stream::Video { .. }))
        .map(|stream| {
            Ok((
                stream.id().to_owned(),
                super::stations::station_key(
                    episode,
                    stream.id(),
                    "source",
                    &serde_json::json!({}),
                )?,
            ))
        })
        .collect()
}
/// Withdraw old vectors before publishing a changed timeline/content descriptor.
/// A crash may leave an incomplete index, but cannot label old vectors as current.
fn write_episode(sidecar: &Path, episode: &Episode) -> Result<()> {
    let descriptor = sidecar.join("episode.json");
    let previous = if descriptor.is_file() {
        Some(serde_json::from_slice::<Episode>(&fs::read(&descriptor)?)?)
    } else {
        None
    };
    let old_keys = previous
        .as_ref()
        .map(source_keys)
        .transpose()?
        .unwrap_or_default();
    let new_keys = source_keys(episode)?;
    let changed = old_keys != new_keys;
    let vectors = sidecar.join("embeddings");
    if changed && vectors.is_dir() {
        let mut streams = std::collections::BTreeSet::new();
        let mut primaries = std::collections::BTreeSet::from([episode.time.reference.clone()]);
        for value in std::iter::once(episode).chain(previous.as_ref()) {
            primaries.insert(value.time.reference.clone());
            streams.extend(
                value
                    .streams
                    .iter()
                    .filter(|stream| matches!(stream, Stream::Video { .. }))
                    .map(|stream| stream.id().to_owned()),
            );
        }
        streams.retain(|stream| old_keys.get(stream) != new_keys.get(stream));
        for entry in fs::read_dir(vectors)? {
            let path = entry?.path();
            if path
                .extension()
                .is_none_or(|extension| extension != "parquet")
            {
                continue;
            }
            let space = path
                .file_stem()
                .and_then(|name| name.to_str())
                .context("invalid vector filename")?;
            let mut states = std::collections::BTreeSet::new();
            for stream in &streams {
                for primary in &primaries {
                    states.insert(super::embed::state_path(sidecar, stream, primary, space));
                }
            }
            for path in states {
                let mut state = if path.is_file() {
                    serde_json::from_slice::<super::embed::State>(&fs::read(&path)?)?
                } else {
                    super::embed::State {
                        input_hash: String::new(),
                        complete: false,
                        error: None,
                    }
                };
                state.complete = false;
                state.error = Some(
                    "episode content or timeline changed; run index to refresh vectors".into(),
                );
                storage::write_json(&path, &state)?;
            }
        }
    }
    storage::write_json(&descriptor, episode)
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
    let sidecar = match write_episode(&preferred, episode) {
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
            write_episode(&fallback, episode)?;
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
    #[tokio::test]
    async fn changed_dataset_publication_withdraws_only_affected_vectors_before_rebuild() {
        use super::super::{
            embed, lance,
            vectors::{self, Kind, VectorRow},
        };
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("dataset");
        crate::lerobot::tests::fixture(&root, "v3.1");
        let workspace = dir.path().join("workspace");
        let episode = crate::lerobot::read_with_workspace(&root, &workspace)
            .unwrap()
            .remove(0);
        let sidecar = publish_episode(&workspace, &episode, None).unwrap();
        let front = "observation.images.front";
        let wrist = "observation.images.wrist";
        let mut config = crate::config::Config::default();
        config.embedding.dims = Some(2);
        let space = config.space_id().unwrap();
        let rows = [front, wrist]
            .into_iter()
            .map(|stream| VectorRow {
                id: stream.into(),
                episode: episode.episode_id.clone(),
                stream: stream.into(),
                kind: Kind::Video,
                start_us: 0,
                end_us: 1_000_000,
                vector: vec![1., 0.],
                text: String::new(),
                still: false,
                space_id: space.clone(),
                params_hash: "fixture".into(),
            })
            .collect::<Vec<_>>();
        let parquet = sidecar.join("embeddings").join(format!("{space}.parquet"));
        vectors::write(&parquet, &rows, 2).unwrap();
        for stream in [front, wrist] {
            storage::write_json(
                &embed::state_path(&sidecar, stream, front, &space),
                &embed::State {
                    input_hash: embed::fingerprint(
                        &episode,
                        stream,
                        &space,
                        &embed::Options::default(),
                        None,
                        None,
                    )
                    .unwrap(),
                    complete: true,
                    error: None,
                },
            )
            .unwrap();
        }
        let bytes = fs::read(&parquet).unwrap();
        publish_episode(&workspace, &episode, None).unwrap();
        assert!(embed::usable(&sidecar, front, front, &space).unwrap());
        assert_eq!(
            lance::rebuild(&workspace, &space, 2)
                .await
                .unwrap()
                .count()
                .await
                .unwrap(),
            2
        );
        let Stream::Video { path, .. } = episode.video(front).unwrap() else {
            unreachable!()
        };
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:size=64x64:rate=2:duration=8",
                    "-c:v",
                    "libx264",
                ])
                .arg(root.join(path)),
        )
        .unwrap();
        let mut changed = crate::lerobot::read_with_workspace(&root, &workspace)
            .unwrap()
            .remove(0);
        assert_eq!(episode.episode_id, changed.episode_id);
        publish_episode(&workspace, &changed, None).unwrap();
        assert!(!embed::usable(&sidecar, front, front, &space).unwrap());
        assert!(embed::usable(&sidecar, wrist, front, &space).unwrap());
        assert_eq!(
            lance::rebuild(&workspace, &space, 2)
                .await
                .unwrap()
                .count()
                .await
                .unwrap(),
            1
        );
        let status = crate::status::inspect(&workspace, None).unwrap();
        assert!(
            !status.episodes[0]
                .embeddings
                .iter()
                .find(|state| state.stream == front)
                .unwrap()
                .complete
        );
        assert!(
            status.episodes[0]
                .embeddings
                .iter()
                .find(|state| state.stream == wrist)
                .unwrap()
                .complete
        );
        changed.time.mappings.get_mut(wrist).unwrap().b_us += 1;
        publish_episode(&workspace, &changed, None).unwrap();
        assert!(!embed::usable(&sidecar, wrist, front, &space).unwrap());
        assert_eq!(
            lance::rebuild(&workspace, &space, 2)
                .await
                .unwrap()
                .count()
                .await
                .unwrap(),
            0
        );
        let error = crate::search::run(
            &workspace,
            &config,
            &crate::search::Options {
                query: Some("cup".into()),
                ..Default::default()
            },
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<crate::providers::ProviderError>()
                .unwrap()
                .kind,
            crate::providers::Failure::Unsupported
        );
        assert!(
            crate::status::inspect(&workspace, None).unwrap().episodes[0]
                .embedding_spaces
                .is_empty()
        );
        assert_eq!(fs::read(parquet).unwrap(), bytes);
    }
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
    fn overlapping_dataset_inputs_are_canonicalized_and_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let dataset = dir.path().join("dataset");
        fs::create_dir_all(dataset.join("meta")).unwrap();
        fs::write(dataset.join("meta/info.json"), "{}").unwrap();
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&dataset, &alias).unwrap();
        assert_eq!(
            discover(&[dir.path().into(), dataset.clone(), alias, dataset.clone()]).unwrap(),
            vec![Input::LeRobot(fs::canonicalize(dataset).unwrap())]
        );
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
