//! Stage v3.1 language updates while preserving source columns and unrelated episodes.
use crate::{
    annotations::AnnotationFile,
    episode::{Episode, Stream},
    index::stations::has_current_input,
    lerobot::{json_rows, parquet_batches},
};
use anyhow::{Context, Result, ensure};
use arrow_array::{ArrayRef, BooleanArray, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

pub struct Assignment<'a> {
    pub episode: &'a Episode,
    pub subtasks: &'a AnnotationFile,
}
fn language_field() -> Field {
    Field::new(
        "language_persistent",
        DataType::List(Arc::new(Field::new(
            "item",
            DataType::Struct(
                vec![
                    Field::new("role", DataType::Utf8, false),
                    Field::new("content", DataType::Utf8, true),
                    Field::new("style", DataType::Utf8, true),
                    Field::new("timestamp", DataType::Float32, false),
                    Field::new("camera", DataType::Utf8, true),
                    Field::new(
                        "tool_calls",
                        DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                        true,
                    ),
                ]
                .into(),
            ),
            true,
        ))),
        true,
    )
}
fn integer(value: &Value, key: &str) -> Result<u64> {
    value[key]
        .as_u64()
        .with_context(|| format!("missing data integer {key}"))
}
fn time(value: &Value) -> Result<f64> {
    let time = value["timestamp"]
        .as_f64()
        .context("missing frame timestamp")?;
    ensure!(time.is_finite() && time >= 0., "invalid frame timestamp");
    Ok(time)
}
fn us(time: f64) -> i64 {
    (time * 1_000_000.).round() as i64
}
fn keys(batch: &RecordBatch) -> Result<Vec<Value>> {
    let columns = ["episode_index", "timestamp"]
        .iter()
        .map(|name| batch.schema().index_of(name))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    json_rows(&[batch.project(&columns)?])
}
fn language_rows(batch: &RecordBatch) -> Result<Vec<Value>> {
    if let Ok(index) = batch.schema().index_of("language_persistent") {
        json_rows(&[batch.project(&[index])?])
    } else {
        Ok(vec![json!({}); batch.num_rows()])
    }
}
fn concat(batches: &[RecordBatch], name: &str) -> Result<ArrayRef> {
    let arrays = batches
        .iter()
        .map(|batch| {
            batch
                .column_by_name(name)
                .map(|a| a.as_ref())
                .context("column missing during preservation check")
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(arrow_select::concat::concat(&arrays)?)
}
fn retained(value: &Value) -> Vec<Value> {
    value["language_persistent"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|r| r["style"] != "subtask")
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Write a staged shard. The caller publishes it only after dataset-level verification.
pub fn rewrite_shard(
    source: &Path,
    destination: &Path,
    assignments: &[Assignment<'_>],
) -> Result<()> {
    ensure!(
        source != destination,
        "writeback must stage a separate shard"
    );
    let batches = parquet_batches(source)?;
    ensure!(!batches.is_empty(), "empty source data shard");
    let schema = batches[0].schema();
    let all_keys = batches
        .iter()
        .map(keys)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut replacements = BTreeMap::new();
    let mut expected_active = BTreeMap::new();
    for assignment in assignments {
        let episode = assignment.episode;
        let annotation = assignment.subtasks;
        ensure!(
            episode.source.format == "lerobot/3.1",
            "subtask writeback requires LeRobot v3.1"
        );
        annotation.validate(episode.duration_us()?, None)?;
        ensure!(
            annotation.header.name == "semantic.subtask"
                && annotation.header.episode == episode.episode_id
                && annotation.header.stream == episode.time.reference,
            "writeback requires this episode's primary-camera subtasks"
        );
        ensure!(
            has_current_input(episode, annotation)?,
            "subtask annotation input changed before writeback"
        );
        let index = episode.local_id.parse::<u64>()?;
        let selected = all_keys
            .iter()
            .enumerate()
            .filter(|(_, row)| row["episode_index"].as_u64() == Some(index))
            .collect::<Vec<_>>();
        ensure!(!selected.is_empty(), "selected episode absent from shard");
        let state = episode
            .streams
            .iter()
            .find_map(|stream| match stream {
                Stream::Parquet {
                    path, row_range, ..
                } => Some((path, row_range)),
                _ => None,
            })
            .context("episode has no data stream")?;
        ensure!(
            fs::canonicalize(episode.source.root.join(state.0))? == fs::canonicalize(source)?,
            "assignment targets a different data shard"
        );
        ensure!(
            [
                selected[0].0 as u64,
                (selected.last().unwrap().0 + 1) as u64
            ] == *state.1,
            "episode row bounds changed before writeback"
        );
        let frame_times = selected
            .iter()
            .map(|(_, row)| time(row))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            frame_times.windows(2).all(|pair| pair[0] < pair[1]),
            "episode frame times are not strictly increasing"
        );
        let mut atoms = Vec::new();
        for ((position, _), timestamp) in selected.iter().zip(&frame_times) {
            let active = annotation
                .records
                .iter()
                .filter(|record| {
                    record.start_us <= us(*timestamp) && us(*timestamp) < record.end_us
                })
                .collect::<Vec<_>>();
            ensure!(
                active.len() == 1,
                "each dataset frame must have exactly one active subtask"
            );
            expected_active.insert(*position, active[0].fields["text"].clone());
        }
        for subtask in &annotation.records {
            let frame = frame_times
                .iter()
                .copied()
                .find(|t| us(*t) >= subtask.start_us && us(*t) < subtask.end_us)
                .context("subtask contains no dataset frame")?;
            // This is the original frame timestamp, not frame_index / fps.
            atoms.push(json!({"role":"assistant","content":subtask.fields["text"],"style":"subtask","timestamp":frame,"camera":null,"tool_calls":null}));
        }
        ensure!(
            replacements.insert(index, atoms).is_none(),
            "duplicate episode assignment"
        );
    }
    let existing = schema.index_of("language_persistent").ok();
    let field = existing
        .map(|index| schema.field(index).clone())
        .unwrap_or_else(language_field);
    let language_schema = Arc::new(Schema::new(vec![field.clone()]));
    let mut new_batches = Vec::new();
    for batch in &batches {
        let row_keys = keys(batch)?;
        let original = language_rows(batch)?;
        let mut changed = Vec::new();
        let mut mask = Vec::new();
        for (key, language) in row_keys.iter().zip(&original) {
            let index = integer(key, "episode_index")?;
            if let Some(atoms) = replacements.get(&index) {
                let mut keep = retained(language);
                keep.extend(atoms.clone());
                changed.push(json!({"language_persistent":keep}));
                mask.push(true);
            } else {
                changed.push(language.clone());
                mask.push(false);
            }
        }
        let mut decoder =
            arrow_json::ReaderBuilder::new(language_schema.clone()).build_decoder()?;
        decoder.serialize(&changed)?;
        let replacement = decoder
            .flush()?
            .context("missing language column output")?
            .column(0)
            .clone();
        let replacement = if let Some(index) = existing {
            arrow_select::zip::zip(
                &BooleanArray::from(mask),
                &replacement.as_ref(),
                &batch.column(index).as_ref(),
            )?
        } else {
            replacement
        };
        let mut fields = schema.fields().iter().cloned().collect::<Vec<_>>();
        let mut columns = batch.columns().to_vec();
        if let Some(index) = existing {
            columns[index] = replacement;
        } else {
            fields.push(Arc::new(field.clone()));
            columns.push(replacement);
        }
        new_batches.push(RecordBatch::try_new(
            Arc::new(Schema::new_with_metadata(fields, schema.metadata().clone())),
            columns,
        )?);
    }
    fs::create_dir_all(
        destination
            .parent()
            .context("shard destination has no parent")?,
    )?;
    let props = parquet::file::properties::WriterProperties::builder()
        .set_compression(parquet::basic::Compression::SNAPPY)
        .build();
    let mut writer = parquet::arrow::ArrowWriter::try_new(
        File::create(destination)?,
        new_batches[0].schema(),
        Some(props),
    )?;
    // Preserve an episode boundary as a row-group boundary for the official loader.
    let mut previous_episode = None;
    for batch in &new_batches {
        let row_keys = keys(batch)?;
        let mut start = 0;
        for end in 1..=batch.num_rows() {
            if end == batch.num_rows()
                || row_keys[end]["episode_index"] != row_keys[start]["episode_index"]
            {
                let episode = row_keys[start]["episode_index"].clone();
                if previous_episode
                    .as_ref()
                    .is_some_and(|previous| previous != &episode)
                {
                    writer.flush()?;
                }
                writer.write(&batch.slice(start, end - start))?;
                previous_episode = Some(episode);
                start = end;
            }
        }
    }
    writer.close()?;
    File::open(destination)?.sync_all()?;
    let after = parquet_batches(destination)?;
    for field in schema.fields() {
        if field.name() != "language_persistent" {
            ensure!(
                concat(&batches, field.name())? == concat(&after, field.name())?,
                "writeback changed protected column {}",
                field.name()
            );
        }
    }
    let before_language = batches
        .iter()
        .map(language_rows)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let after_language = after
        .iter()
        .map(language_rows)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    ensure!(
        before_language.len() == after_language.len(),
        "writeback changed row count"
    );
    for (position, ((key, before), after)) in all_keys
        .iter()
        .zip(&before_language)
        .zip(&after_language)
        .enumerate()
    {
        if let Some(atoms) = replacements.get(&integer(key, "episode_index")?) {
            ensure!(
                retained(before) == retained(after),
                "writeback changed existing non-subtask language"
            );
            let written = after["language_persistent"]
                .as_array()
                .context("missing persistent language")?
                .iter()
                .filter(|a| a["style"] == "subtask")
                .collect::<Vec<_>>();
            ensure!(
                written.len() == atoms.len(),
                "wrong number of subtask language atoms"
            );
            for (written, expected) in written.iter().zip(atoms) {
                ensure!(
                    written["content"] == expected["content"]
                        && written["role"] == expected["role"],
                    "subtask content changed during serialization"
                );
                ensure!(
                    (written["timestamp"]
                        .as_f64()
                        .context("missing atom timestamp")?
                        - expected["timestamp"].as_f64().unwrap())
                    .abs()
                        < 0.00001,
                    "subtask timestamp changed during serialization"
                );
            }
            let frame = time(key)?;
            let active = written
                .iter()
                .filter(|a| {
                    a["timestamp"]
                        .as_f64()
                        .is_some_and(|t| t <= frame + 0.000001)
                })
                .max_by(|a, b| {
                    a["timestamp"]
                        .as_f64()
                        .unwrap()
                        .total_cmp(&b["timestamp"].as_f64().unwrap())
                });
            ensure!(active.is_some(), "frame has no active subtask");
            ensure!(
                active.unwrap()["content"] == expected_active[&position],
                "wrong active subtask at dataset frame"
            );
        } else {
            ensure!(
                before == after,
                "writeback changed unselected episode language"
            );
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, target: &Path, root: &Path) -> Result<()> {
    crate::media::check_cancellation()?;
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        crate::media::check_cancellation()?;
        let entry = entry?;
        if matches!(entry.file_name().to_str(), Some(".cerul" | ".git")) {
            continue;
        }
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&path, &destination, root)?;
        } else {
            ensure!(
                fs::canonicalize(&path)?.starts_with(root),
                "dataset copy escapes root"
            );
            ensure!(fs::metadata(&path)?.is_file(), "unsupported dataset entry");
            let mut input = File::open(&path)?;
            let mut output = File::create(&destination)?;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                crate::media::check_cancellation()?;
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                output.write_all(&buffer[..count])?;
            }
            output.set_permissions(input.metadata()?.permissions())?;
            output.sync_all()?;
        }
    }
    Ok(())
}

/// Validate destination shape and containment without creating directories.
pub fn validate_output(root: &Path, destination: &Path) -> Result<()> {
    let root = fs::canonicalize(root)?;
    ensure!(
        destination.file_name().is_some(),
        "output must name a new dataset directory"
    );
    ensure!(
        !destination
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)),
        "output path must not contain parent traversal"
    );
    match fs::symlink_metadata(destination) {
        Ok(_) => anyhow::bail!("output destination already exists"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let ancestor = parent
        .ancestors()
        .find(|p| p.exists())
        .unwrap_or(Path::new("."));
    ensure!(
        fs::metadata(ancestor)?.is_dir(),
        "output parent is not a directory"
    );
    ensure!(
        !fs::canonicalize(ancestor)?.starts_with(&root),
        "output dataset must be outside the source dataset"
    );
    Ok(())
}

/// Build a complete output dataset and run the caller's validator before publication.
/// Native callers need no Python dependency; acceptance tests additionally use the official loader.
pub fn write_out(
    root: &Path,
    destination: &Path,
    assignments: &[Assignment<'_>],
    validate: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let root = fs::canonicalize(root)?;
    let _read_lock = super::transaction::read_lock(&root)?;
    write_out_locked(&root, destination, assignments, validate)
}

/// Caller holds a dataset read lock or the in-place writer's exclusive lock.
pub(super) fn write_out_locked(
    root: &Path,
    destination: &Path,
    assignments: &[Assignment<'_>],
    validate: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let root = fs::canonicalize(root)?;
    crate::media::check_cancellation()?;
    ensure!(!assignments.is_empty(), "no selected subtask annotations");
    let mut info: Value = serde_json::from_slice(&fs::read(root.join("meta/info.json"))?)?;
    ensure!(
        info["codebase_version"] == "v3.1",
        "writeback requires an existing v3.1 dataset"
    );
    validate_output(&root, destination)?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let parent = fs::canonicalize(parent)?;
    ensure!(
        !parent.starts_with(&root),
        "output dataset must be outside the source dataset"
    );
    let stage = tempfile::Builder::new()
        .prefix(".cerul-writeback-")
        .tempdir_in(&parent)?;
    copy_tree(&root, stage.path(), &root)?;
    let mut by_path: BTreeMap<std::path::PathBuf, Vec<Assignment<'_>>> = BTreeMap::new();
    for assignment in assignments {
        ensure!(
            fs::canonicalize(&assignment.episode.source.root)? == root,
            "writeback mixes source datasets"
        );
        let path = assignment
            .episode
            .streams
            .iter()
            .find_map(|stream| match stream {
                Stream::Parquet { path, .. } => Some(path.clone()),
                _ => None,
            })
            .context("episode has no parquet stream")?;
        by_path.entry(path).or_default().push(Assignment {
            episode: assignment.episode,
            subtasks: assignment.subtasks,
        });
    }
    for source in super::files(&root.join("data"))? {
        crate::media::check_cancellation()?;
        let relative = source.strip_prefix(&root)?;
        let selected = by_path.remove(relative).unwrap_or_default();
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            File::open(&source)?,
        )?;
        let has_column = reader.schema().index_of("language_persistent").is_ok();
        if !selected.is_empty() || !has_column {
            rewrite_shard(&source, &stage.path().join(relative), &selected)?;
        }
    }
    ensure!(by_path.is_empty(), "selected data shard was not found");
    let features = info["features"]
        .as_object_mut()
        .context("dataset features missing")?;
    features
        .entry("language_persistent")
        .or_insert(json!({"dtype":"language","shape":[1],"names":null}));
    crate::storage::write_json(&stage.path().join("meta/info.json"), &info)?;
    super::read(stage.path())?;
    validate(stage.path())?;
    crate::media::check_cancellation()?;
    ensure!(
        !destination.exists(),
        "output destination appeared during writeback"
    );
    fs::rename(stage.path(), destination)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{Header, Model, Record};
    #[test]
    fn staged_subtasks_preserve_protected_columns_and_other_episode_language() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("dataset");
        crate::lerobot::tests::fixture(&root, "v3.1");
        let source = root.join("data/chunk-000/file-000.parquet");
        let mut rows = json_rows(&parquet_batches(&source).unwrap()).unwrap();
        let without_language = rows.clone();
        for row in &mut rows {
            row["language_persistent"] = json!([
                {"role":"assistant","style":"plan","content":"Keep this existing plan","timestamp":0.0,"camera":null,"tool_calls":["{\"name\":\"existing\"}"],"extra":17},
                {"role":"assistant","style":"subtask","content":"Old subtask","timestamp":0.0,"camera":null,"tool_calls":null,"extra":null}
            ]);
            row["language_events"] = json!([{"role":"user","style":"vqa","content":"Where is the cup?","camera":"observation.images.front","tool_calls":null}]);
        }
        crate::lerobot::tests::write_rows(&source, &rows);
        let episodes = crate::lerobot::read(&root).unwrap();
        let episode = &episodes[0];
        let mut annotation = AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "semantic.subtask".into(),
                episode: episode.episode_id.clone(),
                stream: episode.time.reference.clone(),
                model: Model {
                    kind: "fixture".into(),
                    name: "test".into(),
                    base_url: None,
                },
                params: json!({}),
                created: "2026-09-08T00:00:00Z".into(),
                cerul_version: "0.0.3".into(),
                input_hash: "fixture".into(),
                record_schema: "semantic.subtask/1".into(),
            },
            records: ["Reach for cup", "Place cup"]
                .into_iter()
                .enumerate()
                .map(|(index, text)| Record {
                    id: format!("s{index}"),
                    start_us: index as i64 * 2_000_000,
                    end_us: (index + 1) as i64 * 2_000_000,
                    confidence: None,
                    fields: BTreeMap::from([
                        ("text".into(), json!(text)),
                        ("index".into(), json!(index)),
                    ]),
                })
                .collect(),
        };
        annotation.header.input_hash = crate::index::stations::station_key(
            episode,
            &annotation.header.stream,
            &annotation.header.name,
            &annotation.header.params,
        )
        .unwrap();
        let before = fs::read(&source).unwrap();
        let destination = dir.path().join("staged.parquet");
        rewrite_shard(
            &source,
            &destination,
            &[Assignment {
                episode,
                subtasks: &annotation,
            }],
        )
        .unwrap();
        assert_eq!(fs::read(&source).unwrap(), before);
        let output = json_rows(&parquet_batches(&destination).unwrap()).unwrap();
        let original_rows = json_rows(&parquet_batches(&source).unwrap()).unwrap();
        assert_eq!(output[8], original_rows[8]);
        let atoms = output[5]["language_persistent"].as_array().unwrap();
        assert!(
            atoms
                .iter()
                .any(|a| a["style"] == "plan" && a["extra"] == 17)
        );
        assert!(atoms.iter().any(|a| a["style"] == "subtask"
            && a["content"] == "Place cup"
            && a["timestamp"] == 2.0));
        let mut v3 = episode.clone();
        v3.source.format = "lerobot/3.0".into();
        assert!(
            rewrite_shard(
                &source,
                &dir.path().join("unsupported.parquet"),
                &[Assignment {
                    episode: &v3,
                    subtasks: &annotation
                }]
            )
            .is_err()
        );
        crate::lerobot::tests::write_rows(&source, &without_language);
        let added = dir.path().join("added-column.parquet");
        rewrite_shard(
            &source,
            &added,
            &[Assignment {
                episode,
                subtasks: &annotation,
            }],
        )
        .unwrap();
        let output = json_rows(&parquet_batches(&added).unwrap()).unwrap();
        assert!(output[0]["language_persistent"].is_array());
        assert!(output[8]["language_persistent"].is_null());
        let rejected = dir.path().join("rejected");
        let assignments = [Assignment {
            episode,
            subtasks: &annotation,
        }];
        assert!(
            write_out(&root, &rejected, &assignments, |_| anyhow::bail!(
                "validator rejected output"
            ))
            .is_err()
        );
        assert!(!rejected.exists());
        assert_eq!(
            json_rows(&parquet_batches(&source).unwrap()).unwrap(),
            without_language
        );
        let nested = root.join("new-parent/output");
        assert!(write_out(&root, &nested, &assignments, |_| Ok(())).is_err());
        assert!(!root.join("new-parent").exists());
        let cancelled = dir.path().join("cancelled");
        let token = tokio_util::sync::CancellationToken::new();
        let error = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(crate::media::with_cancellation(token.clone(), async {
                write_out(&root, &cancelled, &assignments, |_| {
                    token.cancel();
                    Ok(())
                })
            }))
            .unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<crate::providers::ProviderError>()
                .unwrap()
                .kind,
            crate::providers::Failure::Cancelled
        );
        assert!(!cancelled.exists());
        let published = dir.path().join("published");
        write_out(&root, &published, &assignments, |_| {
            // The source remains locked through validation and publication.
            assert!(super::super::transaction::recover(&root).is_err());
            Ok(())
        })
        .unwrap();
        let info: Value =
            serde_json::from_slice(&fs::read(published.join("meta/info.json")).unwrap()).unwrap();
        assert_eq!(info["features"]["language_persistent"]["dtype"], "language");
        assert!(write_out(&root, &published, &assignments, |_| Ok(())).is_err());
    }
}
