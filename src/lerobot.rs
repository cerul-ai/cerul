//! LeRobot v3 metadata adapter. Official contract reference is documented in docs/lerobot.md.
use crate::{
    episode::{Episode, MappingStatus, Source, Stream, TimeMapping, Timeline},
    media,
    providers::{Failure, ProviderError},
    storage,
};
use anyhow::{Context, Result, ensure};
use arrow_array::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    path::{Path, PathBuf},
};
pub mod transaction;
pub mod writer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub dataset_id: String,
    pub root: PathBuf,
    pub info_hash: String,
}
pub fn parquet_batches(path: &Path) -> Result<Vec<RecordBatch>> {
    ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?
        .build()?
        .map(|batch| Ok(batch?))
        .collect()
}
pub fn json_rows(batches: &[RecordBatch]) -> Result<Vec<Value>> {
    let mut writer = arrow_json::ArrayWriter::new(Vec::new());
    writer.write_batches(&batches.iter().collect::<Vec<_>>())?;
    writer.finish()?;
    Ok(serde_json::from_slice(&writer.into_inner())?)
}
fn data_metadata(path: &Path) -> Result<(Vec<String>, Vec<Value>)> {
    let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?;
    let columns: Vec<_> = builder
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect();
    let indexes: Vec<_> = columns
        .iter()
        .enumerate()
        .filter(|(_, name)| {
            matches!(
                name.as_str(),
                "index" | "episode_index" | "frame_index" | "timestamp"
            )
        })
        .map(|(index, _)| index)
        .collect();
    let projection = parquet::arrow::ProjectionMask::roots(builder.parquet_schema(), indexes);
    let batches = builder
        .with_projection(projection)
        .build()?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok((columns, json_rows(&batches)?))
}
fn number(row: &Value, key: &str) -> Result<u64> {
    row.get(key)
        .and_then(Value::as_u64)
        .with_context(|| format!("missing LeRobot integer {key}"))
}
fn micros(row: &Value, key: &str) -> Result<i64> {
    let value = row
        .get(key)
        .and_then(Value::as_f64)
        .with_context(|| format!("missing LeRobot timestamp {key}"))?;
    ensure!(
        value.is_finite() && value >= 0. && value < i64::MAX as f64 / 1_000_000.,
        "invalid dataset timestamp"
    );
    Ok((value * 1_000_000.).round() as i64)
}
fn files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut result = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            result.extend(files(&entry.path())?);
        } else if entry.path().extension().is_some_and(|e| e == "parquet") {
            result.push(entry.path());
        }
    }
    result.sort();
    Ok(result)
}
fn relative(
    root: &Path,
    template: &str,
    chunk: u64,
    file: u64,
    video: Option<&str>,
) -> Result<PathBuf> {
    let mut value = template.to_owned();
    for (key, number) in [("chunk_index", chunk), ("file_index", file)] {
        value = value
            .replace(&format!("{{{key}:03d}}"), &format!("{number:03}"))
            .replace(&format!("{{{key}:04d}}"), &format!("{number:04}"))
            .replace(&format!("{{{key}}}"), &number.to_string());
    }
    if let Some(video) = video {
        value = value.replace("{video_key}", video);
    }
    ensure!(
        !value.contains(['{', '}']),
        "unsupported dataset path template"
    );
    let path = PathBuf::from(value);
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
        "dataset path must be relative without traversal"
    );
    ensure!(
        fs::canonicalize(root.join(&path))?.starts_with(root),
        "dataset path escapes root"
    );
    Ok(path)
}
/// Read-only discovery: the persistent UUID is published under the writer lock later.
pub fn read(root: &Path) -> Result<Vec<Episode>> {
    read_inner(root, None)
}
/// Resolve a read-only dataset's stable identity from this workspace without writing.
pub fn read_with_workspace(root: &Path, workspace: &Path) -> Result<Vec<Episode>> {
    read_inner(root, Some(workspace))
}
fn identity_cache(root: &Path, workspace: &Path) -> Result<PathBuf> {
    Ok(workspace.join("datasets").join(format!(
        "{}.json",
        storage::cache_key(&fs::canonicalize(root)?)?
    )))
}
fn read_inner(root: &Path, workspace: Option<&Path>) -> Result<Vec<Episode>> {
    let root = fs::canonicalize(root)?;
    let _read_lock = transaction::read_lock(&root)?;
    let info: Value = serde_json::from_slice(&fs::read(root.join("meta/info.json"))?)?;
    let version = info["codebase_version"]
        .as_str()
        .context("dataset has no codebase_version")?;
    if !matches!(version, "v3.0" | "v3.1") {
        return Err(ProviderError {
            kind: Failure::Unsupported,
            message: format!("LeRobot {version} is unsupported; expected v3.0 or v3.1"),
        }
        .into());
    }
    let local_identity = root.join(".cerul/dataset.json");
    let identity_path = if local_identity.is_file() {
        local_identity
    } else if let Some(workspace) = workspace {
        identity_cache(&root, workspace)?
    } else {
        local_identity
    };
    let dataset_id = if identity_path.is_file() {
        let identity: Identity = serde_json::from_slice(&fs::read(identity_path)?)?;
        uuid::Uuid::parse_str(&identity.dataset_id)?;
        identity.dataset_id
    } else {
        uuid::Uuid::new_v4().to_string()
    };
    let data_template = info["data_path"]
        .as_str()
        .unwrap_or("data/chunk-{chunk_index:03d}/file-{file_index:03d}.parquet");
    let video_template = info["video_path"]
        .as_str()
        .context("LeRobot dataset has no video path")?;
    let features = info["features"]
        .as_object()
        .context("dataset has no features")?;
    let cameras: Vec<_> = features
        .iter()
        .filter(|(_, feature)| feature["dtype"] == "video")
        .map(|(key, _)| key.clone())
        .collect();
    ensure!(
        !cameras.is_empty(),
        "dataset contains no video camera features"
    );
    let mut metadata = Vec::new();
    for path in files(&root.join("meta/episodes"))? {
        metadata.extend(json_rows(&parquet_batches(&path)?)?);
    }
    metadata.sort_by_key(|row| row["episode_index"].as_u64().unwrap_or(u64::MAX));
    let mut seen = BTreeSet::new();
    let mut data_cache: BTreeMap<PathBuf, (Vec<String>, Vec<Value>)> = BTreeMap::new();
    let mut video_cache = BTreeMap::new();
    let mut episodes = Vec::new();
    for row in metadata {
        let index = number(&row, "episode_index")?;
        ensure!(seen.insert(index), "duplicate dataset episode_index");
        let data = relative(
            &root,
            data_template,
            number(&row, "data/chunk_index")?,
            number(&row, "data/file_index")?,
            None,
        )?;
        if !data_cache.contains_key(&data) {
            data_cache.insert(data.clone(), data_metadata(&root.join(&data))?);
        }
        let (columns, data_rows) = &data_cache[&data];
        let selected: Vec<_> = data_rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r["episode_index"].as_u64() == Some(index))
            .collect();
        ensure!(
            !selected.is_empty(),
            "episode has no rows in its data shard"
        );
        let start = selected[0].0;
        let end = selected.last().unwrap().0 + 1;
        ensure!(
            end - start == selected.len() && selected.len() as u64 == number(&row, "length")?,
            "episode rows must be contiguous and match metadata length"
        );
        ensure!(
            number(selected[0].1, "index")? == number(&row, "dataset_from_index")?
                && number(selected.last().unwrap().1, "index")? + 1
                    == number(&row, "dataset_to_index")?,
            "episode global data bounds disagree with shard"
        );
        let mut streams = Vec::new();
        let mut mappings = BTreeMap::new();
        for (camera_index, camera) in cameras.iter().enumerate() {
            let prefix = format!("videos/{camera}");
            let path = relative(
                &root,
                video_template,
                number(&row, &format!("{prefix}/chunk_index"))?,
                number(&row, &format!("{prefix}/file_index"))?,
                Some(camera),
            )?;
            if !video_cache.contains_key(&path) {
                video_cache.insert(
                    path.clone(),
                    (
                        media::probe(&root.join(&path))?,
                        media::sha256(&root.join(&path))?,
                    ),
                );
            }
            let (probe, hash) = &video_cache[&path];
            let range = [
                micros(&row, &format!("{prefix}/from_timestamp"))?,
                micros(&row, &format!("{prefix}/to_timestamp"))?,
            ];
            ensure!(
                range[1] > range[0] && range[1] <= probe.start_us + probe.duration_us + 1_000,
                "episode video range exceeds source shard"
            );
            streams.push(Stream::Video {
                id: camera.clone(),
                primary: camera_index == 0,
                path,
                sha256: hash.clone(),
                range_us: range,
                probe: probe.clone(),
            });
            if camera_index != 0 {
                mappings.insert(
                    camera.clone(),
                    TimeMapping {
                        a: 1.,
                        b_us: 0,
                        status: MappingStatus::Calibrated,
                    },
                );
            }
        }
        streams.push(Stream::Parquet {
            id: "state".into(),
            path: data,
            columns: columns.clone(),
            row_range: [start as u64, end as u64],
        });
        let local_id = format!("{index:06}");
        let episode = Episode {
            schema: "episode/1".into(),
            episode_id: format!("{dataset_id}/{local_id}"),
            dataset_id: dataset_id.clone(),
            local_id,
            streams,
            time: Timeline {
                reference: cameras[0].clone(),
                mappings,
            },
            task: row["tasks"]
                .as_array()
                .and_then(|tasks| tasks.first())
                .and_then(Value::as_str)
                .map(str::to_owned),
            source: Source {
                format: format!("lerobot/{}", version.trim_start_matches('v')),
                root: root.clone(),
            },
        };
        episode.validate()?;
        episodes.push(episode);
    }
    ensure!(!episodes.is_empty(), "dataset contains no episode metadata");
    Ok(episodes)
}
pub(crate) fn permission_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<std::io::Error>().is_some_and(|e| {
        matches!(
            e.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
        )
    })
}
/// Caller holds the workspace lock. Cache identity before attempting a local sidecar.
pub fn publish_identity(episode: &Episode, workspace: &Path) -> Result<()> {
    let local = episode.source.root.join(".cerul/dataset.json");
    let cache = identity_cache(&episode.source.root, workspace)?;
    for path in [&local, &cache] {
        if path.is_file() {
            let identity: Identity = serde_json::from_slice(&fs::read(path)?)?;
            ensure!(
                identity.dataset_id == episode.dataset_id,
                "dataset identity changed during discovery; retry command"
            );
        }
    }
    let identity = Identity {
        dataset_id: episode.dataset_id.clone(),
        root: episode.source.root.clone(),
        info_hash: media::sha256(&episode.source.root.join("meta/info.json"))?,
    };
    storage::write_json(&cache, &identity)?;
    if !local.is_file() {
        match storage::write_json(&local, &identity) {
            Ok(()) => (),
            Err(error) if permission_error(&error) => (),
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    pub fn write_rows(path: &Path, rows: &[Value]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let schema =
            arrow_json::reader::infer_json_schema_from_iterator(rows.iter().map(Ok)).unwrap();
        let mut decoder = arrow_json::ReaderBuilder::new(Arc::new(schema))
            .build_decoder()
            .unwrap();
        decoder.serialize(rows).unwrap();
        let batch = decoder.flush().unwrap().unwrap();
        let props = parquet::file::properties::WriterProperties::builder()
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            File::create(path).unwrap(),
            batch.schema(),
            Some(props),
        )
        .unwrap();
        writer.write(&batch).unwrap();
        writer.close().unwrap();
    }
    pub fn fixture(root: &Path, version: &str) {
        fs::create_dir_all(root.join("meta")).unwrap();
        let cameras = ["observation.images.front", "observation.images.wrist"];
        let mut features = serde_json::Map::new();
        for camera in cameras {
            features.insert(
                camera.into(),
                json!({"dtype":"video","shape":[64,64,3],"names":["height","width","channels"]}),
            );
        }
        features.insert(
            "action".into(),
            json!({"dtype":"float32","shape":[1],"names":null}),
        );
        let info = json!({"codebase_version":version,"fps":2,"total_episodes":2,"total_frames":16,"features":features,"data_path":"data/chunk-{chunk_index:03d}/file-{file_index:03d}.parquet","video_path":"videos/{video_key}/chunk-{chunk_index:03d}/file-{file_index:03d}.mp4"});
        storage::write_json(&root.join("meta/info.json"), &info).unwrap();
        for camera in cameras {
            let path = root.join(format!("videos/{camera}/chunk-000/file-000.mp4"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            media::run(
                std::process::Command::new("ffmpeg")
                    .args([
                        "-v",
                        "error",
                        "-f",
                        "lavfi",
                        "-i",
                        "testsrc2=size=64x64:rate=2:duration=8",
                        "-c:v",
                        "libx264",
                    ])
                    .arg(path),
            )
            .unwrap();
        }
        let data:Vec<_>=(0..16).map(|i|json!({"episode_index":i/8,"index":100+i,"frame_index":i%8,"timestamp":(i%8) as f64/2.,"action":[i as f64],"observation.state":[i as f64+0.5],"task_index":0})).collect();
        write_rows(&root.join("data/chunk-000/file-000.parquet"), &data);
        let episodes:Vec<_>=(0..2).map(|i|{
            let mut row=json!({"episode_index":i,"length":8,"dataset_from_index":100+i*8,"dataset_to_index":108+i*8,"data/chunk_index":0,"data/file_index":0,"tasks":["Move the object"]});
            for camera in cameras {for (field,value) in [("chunk_index",json!(0)),("file_index",json!(0)),("from_timestamp",json!(i*4)),("to_timestamp",json!((i+1)*4))]{row[format!("videos/{camera}/{field}")]=value;}}
            row
        }).collect();
        write_rows(
            &root.join("meta/episodes/chunk-000/file-000.parquet"),
            &episodes,
        );
    }
    #[cfg(unix)]
    #[test]
    fn read_only_datasets_keep_stable_distinct_workspace_sidecars() {
        use std::os::unix::fs::PermissionsExt;
        struct Restore(PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        let mut all_sidecars = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for name in ["first", "second"] {
            let root = dir.path().join(name);
            fixture(&root, "v3.1");
            fs::set_permissions(&root, fs::Permissions::from_mode(0o555)).unwrap();
            let _restore = Restore(root.clone());
            assert!(
                fs::create_dir(root.join("permission-check")).is_err(),
                "test requires an unprivileged user"
            );
            let episodes = read_with_workspace(&root, &workspace).unwrap();
            assert!(!workspace.exists() || name == "second");
            ids.insert(episodes[0].dataset_id.clone());
            let _lock = storage::WorkspaceLock::acquire(&workspace).unwrap();
            for episode in &episodes {
                let path =
                    crate::index::discover::publish_episode(&workspace, episode, None).unwrap();
                assert!(path.starts_with(workspace.join("sidecars")));
                assert_eq!(path.file_name().unwrap(), episode.local_id.as_str());
                all_sidecars.insert(path.clone());
                let saved: Episode =
                    serde_json::from_slice(&fs::read(path.join("episode.json")).unwrap()).unwrap();
                assert_eq!(saved.episode_id, episode.episode_id);
            }
            assert!(!root.join(".cerul").exists());
            let again = read_with_workspace(&root, &workspace).unwrap();
            assert_eq!(again[0].dataset_id, episodes[0].dataset_id);
            for episode in &again {
                let path =
                    crate::index::discover::publish_episode(&workspace, episode, None).unwrap();
                assert!(all_sidecars.contains(&path));
            }
            let explicit = dir.path().join("explicit");
            let path =
                crate::index::discover::publish_episode(&workspace, &again[0], Some(&explicit))
                    .unwrap();
            assert_eq!(
                path,
                explicit.join(&again[0].dataset_id).join(&again[0].local_id)
            );
        }
        assert_eq!(ids.len(), 2);
        assert_eq!(all_sidecars.len(), 4);
        assert_eq!(
            crate::index::discover::read_registry(&workspace)
                .unwrap()
                .len(),
            4
        );
    }
    #[test]
    fn v3_shared_shards_keep_episode_ranges_and_dataset_identities_separate() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        let mut identities = BTreeSet::new();
        for version in ["v3.0", "v3.1"] {
            let root = dir.path().join(version);
            fixture(&root, version);
            let episodes = read(&root).unwrap();
            assert_eq!(episodes.len(), 2);
            assert!(!root.join(".cerul").exists());
            identities.insert(episodes[0].dataset_id.clone());
            let paths: Vec<_> = episodes
                .iter()
                .map(|episode| {
                    crate::index::discover::publish_episode(&workspace, episode, None).unwrap()
                })
                .collect();
            assert_ne!(paths[0], paths[1]);
            assert_eq!(read(&root).unwrap()[0].dataset_id, episodes[0].dataset_id);
            assert_eq!(
                episodes[1]
                    .source_to_episode(&episodes[1].time.reference, 5_000_000)
                    .unwrap(),
                1_000_000
            );
            assert_eq!(episodes[1].duration_us().unwrap(), 4_000_000);
            let Stream::Parquet { row_range, .. } = &episodes[1].streams[2] else {
                panic!()
            };
            assert_eq!(*row_range, [8, 16]);
            assert_eq!(
                episodes[1].time.mappings["observation.images.wrist"].b_us,
                0
            );
        }
        assert_eq!(identities.len(), 2);
        assert_eq!(
            crate::index::discover::read_registry(&workspace)
                .unwrap()
                .len(),
            4
        );
    }
}
