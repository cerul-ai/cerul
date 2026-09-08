//! Disposable annotation projection. Sidecar files remain authoritative.
use crate::{
    annotations::{AnnotationFile, Record},
    episode::{Episode, Stream},
    index::{discover::read_registry, lance::literal, stations::stream_directory},
};
use anyhow::{Context, Result, ensure};
use arrow_array::{Array, ArrayRef, Int64Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::{
    Table,
    query::{ExecutableQuery, QueryBase},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, sync::Arc};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Row {
    pub episode: String,
    pub stream: String,
    pub annotation: String,
    pub record: Record,
}
fn batch(rows: &[Row]) -> Result<RecordBatch> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("episode", DataType::Utf8, false),
        Field::new("stream", DataType::Utf8, false),
        Field::new("annotation", DataType::Utf8, false),
        Field::new("id", DataType::Utf8, false),
        Field::new("start_us", DataType::Int64, false),
        Field::new("end_us", DataType::Int64, false),
        Field::new("fields", DataType::Utf8, false),
    ]));
    let fields: Vec<_> = rows
        .iter()
        .map(|row| serde_json::to_string(&row.record))
        .collect::<Result<_, _>>()?;
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|r| r.episode.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|r| r.stream.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|r| r.annotation.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|r| r.record.id.as_str()),
        )),
        Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.record.start_us),
        )),
        Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.record.end_us),
        )),
        Arc::new(StringArray::from_iter_values(fields)),
    ];
    Ok(RecordBatch::try_new(schema, columns)?)
}
type BatchInput = RecordBatchIterator<
    std::vec::IntoIter<std::result::Result<RecordBatch, arrow_schema::ArrowError>>,
>;
fn input(rows: &[Row]) -> Result<Box<BatchInput>> {
    let batch = batch(rows)?;
    let schema = batch.schema();
    Ok(Box::new(RecordBatchIterator::new(
        vec![Ok(batch)].into_iter(),
        schema,
    )))
}
/// Validated annotation inventory also includes completed files with zero records.
pub fn sidecars(workspace: &Path) -> Result<Vec<AnnotationFile>> {
    let mut files = Vec::new();
    for entry in read_registry(workspace)? {
        let episode: Episode =
            serde_json::from_slice(&fs::read(entry.sidecar.join("episode.json"))?)?;
        episode.validate()?;
        ensure!(
            episode.episode_id == entry.episode_id,
            "registered episode identity mismatch"
        );
        for stream in &episode.streams {
            if !matches!(stream, Stream::Video { .. }) {
                continue;
            }
            let directory = stream_directory(&entry.sidecar, stream.id(), &episode.time.reference);
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            for entry in entries {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "jsonl")
                    || path.file_name().is_some_and(|name| name == "log.jsonl")
                {
                    continue;
                }
                let file = AnnotationFile::read(&path)?;
                if !crate::index::stations::has_current_input(&episode, &file)? {
                    continue;
                }
                if file.header.name.starts_with("semantic.") {
                    let coverage =
                        episode
                            .video_coverage(&file.header.stream)?
                            .ok_or_else(|| {
                                anyhow::anyhow!("annotation stream has no episode coverage")
                            })?;
                    file.validate_in_range(coverage, None)?;
                } else {
                    file.validate(episode.duration_us()?, None)?;
                }
                ensure!(
                    file.header.episode == episode.episode_id && file.header.stream == stream.id(),
                    "annotation provenance mismatch"
                );
                ensure!(
                    path.file_stem()
                        .is_some_and(|name| name == file.header.name.as_str()),
                    "annotation filename mismatch"
                );
                files.push(file);
            }
        }
    }
    files.sort_by(|a, b| {
        (&a.header.episode, &a.header.stream, &a.header.name).cmp(&(
            &b.header.episode,
            &b.header.stream,
            &b.header.name,
        ))
    });
    Ok(files)
}
pub struct RecordIndex {
    table: Table,
}
impl RecordIndex {
    pub async fn open(workspace: &Path, space: &str) -> Result<Self> {
        ensure!(
            space.len() == 64 && space.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid space ID"
        );
        let path = workspace.join("index").join(space);
        fs::create_dir_all(&path)?;
        let db = lancedb::connect(path.to_str().context("workspace path must be UTF-8")?)
            .execute()
            .await?;
        let table = if db
            .table_names()
            .execute()
            .await?
            .iter()
            .any(|name| name == "records")
        {
            db.open_table("records").execute().await?
        } else {
            db.create_empty_table("records", batch(&[])?.schema())
                .execute()
                .await?
        };
        Ok(Self { table })
    }
    pub async fn replace(&self, file: &AnnotationFile) -> Result<()> {
        let rows: Vec<_> = file
            .records
            .iter()
            .map(|record| Row {
                episode: file.header.episode.clone(),
                stream: file.header.stream.clone(),
                annotation: file.header.name.clone(),
                record: record.clone(),
            })
            .collect();
        let scope = format!(
            "episode = {} AND stream = {} AND annotation = {}",
            literal(&file.header.episode),
            literal(&file.header.stream),
            literal(&file.header.name)
        );
        self.merge(&rows, Some(scope)).await
    }
    async fn merge(&self, rows: &[Row], scope: Option<String>) -> Result<()> {
        if rows.is_empty() {
            self.table
                .delete(scope.as_deref().unwrap_or("true"))
                .await?;
        } else {
            let mut merge = self
                .table
                .merge_insert(&["episode", "stream", "annotation", "id"]);
            merge
                .when_matched_update_all(None)
                .when_not_matched_insert_all()
                .when_not_matched_by_source_delete(scope);
            merge.execute(input(rows)?).await?;
        }
        Ok(())
    }
    pub async fn rebuild(workspace: &Path, space: &str) -> Result<Self> {
        // Validate every file before modifying the table, preserving the old projection on invalid input.
        let files = sidecars(workspace)?;
        let rows: Vec<_> = files
            .into_iter()
            .flat_map(|file| {
                file.records.into_iter().map(move |record| Row {
                    episode: file.header.episode.clone(),
                    stream: file.header.stream.clone(),
                    annotation: file.header.name.clone(),
                    record,
                })
            })
            .collect();
        let index = Self::open(workspace, space).await?;
        index.merge(&rows, None).await?;
        Ok(index)
    }
    pub async fn read(&self, filter: Option<&str>) -> Result<Vec<Row>> {
        let mut query = self.table.query();
        if let Some(filter) = filter {
            query = query.only_if(filter);
        }
        let batches: Vec<_> = query.execute().await?.try_collect().await?;
        let mut rows = Vec::new();
        for batch in batches {
            let strings = |name: &str| -> Result<&StringArray> {
                let array = batch
                    .column_by_name(name)
                    .and_then(|a| a.as_any().downcast_ref::<StringArray>())
                    .context("invalid annotation column")?;
                ensure!(array.null_count() == 0, "null annotation column");
                Ok(array)
            };
            let episode = strings("episode")?;
            let stream = strings("stream")?;
            let annotation = strings("annotation")?;
            let fields = strings("fields")?;
            for i in 0..batch.num_rows() {
                rows.push(Row {
                    episode: episode.value(i).into(),
                    stream: stream.value(i).into(),
                    annotation: annotation.value(i).into(),
                    record: serde_json::from_str(fields.value(i))?,
                });
            }
        }
        rows.sort_by(|a, b| {
            (
                &a.episode,
                &a.stream,
                a.record.start_us,
                &a.annotation,
                &a.record.id,
            )
                .cmp(&(
                    &b.episode,
                    &b.stream,
                    b.record.start_us,
                    &b.annotation,
                    &b.record.id,
                ))
        });
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        annotations::{Header, Model},
        index::discover,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    #[tokio::test]
    async fn sidecar_rebuild_replaces_deleted_records_and_scoped_ids_do_not_collide() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        crate::media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=2:duration=2",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let workspace = dir.path().join("workspace");
        let episode = discover::ordinary_episode(&source).unwrap();
        let sidecar = discover::publish_episode(&workspace, &episode, None).unwrap();
        let mut file = AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "semantic.flag".into(),
                episode: episode.episode_id.clone(),
                stream: "primary".into(),
                model: Model {
                    kind: "fixture".into(),
                    name: "test".into(),
                    base_url: None,
                },
                params: json!({}),
                created: "2026-09-08T00:00:00Z".into(),
                cerul_version: "0.0.3".into(),
                input_hash: crate::index::stations::station_key(
                    &episode,
                    "primary",
                    "semantic.flag",
                    &json!({}),
                )
                .unwrap(),
                record_schema: "semantic.flag/1".into(),
            },
            records: (0..15)
                .map(|i| Record {
                    id: format!("flag{i}"),
                    start_us: 0,
                    end_us: 2_000_000,
                    confidence: Some(0.8),
                    fields: BTreeMap::from([
                        ("kind".into(), json!("review")),
                        ("note".into(), json!(format!("note {i}"))),
                    ]),
                })
                .collect(),
        };
        let path = sidecar.join("semantic.flag.jsonl");
        file.publish(&path, 2_000_000, None).unwrap();
        let space = "c".repeat(64);
        let index = RecordIndex::rebuild(&workspace, &space).await.unwrap();
        assert_eq!(index.read(None).await.unwrap().len(), 15);
        let mut unrelated = file.clone();
        unrelated.header.episode = "other/0".into();
        index.replace(&unrelated).await.unwrap();
        file.records.truncate(1);
        file.publish(&path, 2_000_000, None).unwrap();
        index.replace(&file).await.unwrap();
        assert_eq!(index.read(None).await.unwrap().len(), 16);
        assert_eq!(
            index.read(Some("episode = 'other/0'")).await.unwrap().len(),
            15
        );
        drop(index);
        fs::remove_dir_all(workspace.join("index")).unwrap();
        let index = RecordIndex::rebuild(&workspace, &space).await.unwrap();
        let rows = index.read(None).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].record.confidence, Some(0.8));
        assert_eq!(rows[0].record.fields["note"], "note 0");
        drop(index);
        fs::remove_file(path).unwrap();
        let index = RecordIndex::rebuild(&workspace, &space).await.unwrap();
        assert!(index.read(None).await.unwrap().is_empty());
    }
}
