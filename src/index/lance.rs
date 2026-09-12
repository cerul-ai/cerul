//! Local Lance tables are disposable projections of authoritative sidecars.
use crate::index::{
    discover::read_registry,
    vectors::{self, Kind, VectorRow},
};
use anyhow::{Context, Result, ensure};
use arrow_array::{Array, Float32Array, RecordBatch, RecordBatchIterator};
use futures::TryStreamExt;
use lancedb::{
    DistanceType, Table,
    query::{ExecutableQuery, QueryBase},
};
use std::{fs, path::Path};
use tokio_util::sync::CancellationToken;

pub struct VectorIndex {
    table: Table,
    dims: usize,
    space: String,
}
fn input(
    batch: RecordBatch,
) -> Box<RecordBatchIterator<std::vec::IntoIter<Result<RecordBatch, arrow_schema::ArrowError>>>> {
    let schema = batch.schema();
    Box::new(RecordBatchIterator::new(
        vec![Ok(batch)].into_iter(),
        schema,
    ))
}
pub fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

impl VectorIndex {
    pub async fn open(workspace: &Path, space: &str, dims: usize, create: bool) -> Result<Self> {
        ensure!(
            space.len() == 64 && space.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid embedding space ID"
        );
        let directory = workspace.join("index").join(space);
        if create {
            fs::create_dir_all(&directory)?;
        } else {
            ensure!(
                directory.is_dir(),
                "embedding index is missing; rebuild it with index"
            );
        }
        let connection =
            lancedb::connect(directory.to_str().context("workspace path must be UTF-8")?)
                .execute()
                .await?;
        let table = if connection
            .table_names()
            .execute()
            .await?
            .iter()
            .any(|name| name == "chunks")
        {
            connection.open_table("chunks").execute().await?
        } else {
            ensure!(create, "embedding index is missing; rebuild it with index");
            connection
                .create_empty_table("chunks", vectors::batch(&[], dims)?.schema())
                .execute()
                .await?
        };
        let schema = table.schema().await?;
        ensure!(
            schema.field_with_name("vector")?.data_type()
                == vectors::batch(&[], dims)?
                    .schema()
                    .field_with_name("vector")?
                    .data_type(),
            "index embedding dimensions differ from configured model"
        );
        Ok(Self {
            table,
            dims,
            space: space.into(),
        })
    }

    /// Replace a complete episode/stream projection in one Lance commit, removing stale rows.
    pub async fn replace(&self, episode: &str, stream: &str, rows: &[VectorRow]) -> Result<()> {
        ensure!(
            rows.iter().all(|row| row.episode == episode
                && row.stream == stream
                && row.space_id == self.space),
            "replacement rows do not match projection identity"
        );
        let filter = format!(
            "episode = {} AND stream = {}",
            literal(episode),
            literal(stream)
        );
        if rows.is_empty() {
            self.table.delete(&filter).await?;
            return Ok(());
        }
        let mut merge = self.table.merge_insert(&["id"]);
        merge
            .when_matched_update_all(None)
            .when_not_matched_insert_all()
            .when_not_matched_by_source_delete(Some(filter));
        merge
            .execute(input(vectors::batch(rows, self.dims)?))
            .await?;
        Ok(())
    }
    pub async fn count(&self) -> Result<usize> {
        Ok(self.table.count_rows(None).await?)
    }
    pub async fn count_matching(&self, predicate: &str) -> Result<usize> {
        Ok(self.table.count_rows(Some(predicate.into())).await?)
    }
    /// Independent budgets prevent one evidence kind from consuming another's
    /// candidates. These are separate prefiltered SDK queries, not a claim that
    /// the engine executes one shared physical scan.
    pub async fn search_tracks(
        &self,
        query: &[f32],
        filter: &str,
        kinds: &[Kind],
        limit: usize,
    ) -> Result<Vec<Vec<(VectorRow, f32)>>> {
        use futures::StreamExt;
        futures::stream::iter(kinds.iter().copied())
            .map(|kind| async move {
                let filter = format!("({filter}) AND kind = {}", literal(kind.as_str()));
                self.search(query, Some(&filter), limit).await
            })
            .buffered(4)
            .try_collect()
            .await
    }
    /// Remove only projections whose authoritative embedding state is no longer usable.
    /// This keeps ordinary searches incremental while an interrupted upstream station
    /// refresh cannot leave stale Lance rows queryable.
    pub async fn prune_incomplete(
        &self,
        workspace: &Path,
        cancel: &CancellationToken,
    ) -> Result<()> {
        for entry in read_registry(workspace)? {
            check_cancelled(cancel)?;
            let metadata = entry.sidecar.join("episode.json");
            if !metadata.is_file() {
                continue;
            }
            let episode: crate::episode::Episode = serde_json::from_slice(&fs::read(metadata)?)?;
            for stream in &episode.streams {
                check_cancelled(cancel)?;
                if !matches!(stream, crate::episode::Stream::Video { .. }) {
                    continue;
                }
                if !entry
                    .sidecar
                    .join("embeddings")
                    .join(format!("{}.parquet", self.space))
                    .is_file()
                    || !super::embed::usable(
                        &entry.sidecar,
                        stream.id(),
                        &episode.time.reference,
                        &self.space,
                    )?
                {
                    self.replace(&episode.episode_id, stream.id(), &[]).await?;
                    check_cancelled(cancel)?;
                }
            }
        }
        Ok(())
    }
    pub async fn search(
        &self,
        query: &[f32],
        filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(VectorRow, f32)>> {
        ensure!(
            query.len() == self.dims
                && query.iter().all(|n| n.is_finite())
                && query.iter().any(|n| *n != 0.),
            "invalid query vector"
        );
        ensure!(limit > 0, "search limit must be positive");
        let mut search = self
            .table
            .vector_search(query)?
            .distance_type(DistanceType::Cosine)
            .limit(limit);
        if let Some(filter) = filter {
            search = search.only_if(filter);
        }
        // Lance 0.38 defaults to prefilter=true. Do not call postfilter().
        let batches: Vec<RecordBatch> = search.execute().await?.try_collect().await?;
        let mut result = Vec::new();
        for batch in batches {
            let distances = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
                .context("missing vector distances")?;
            ensure!(distances.null_count() == 0, "invalid vector distances");
            result.extend(
                vectors::rows(&batch, self.dims)?
                    .into_iter()
                    .enumerate()
                    .map(|(i, row)| (row, 1. - distances.value(i))),
            );
        }
        Ok(result)
    }
}
fn check_cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(crate::providers::ProviderError {
            kind: crate::providers::Failure::Cancelled,
            message: "operation cancelled".into(),
        }
        .into());
    }
    Ok(())
}

/// Rebuild only from saved vectors; this module has no model endpoint dependency.
pub async fn rebuild(workspace: &Path, space: &str, dims: usize) -> Result<VectorIndex> {
    let index = VectorIndex::open(workspace, space, dims, true).await?;
    let mut restored = Vec::new();
    for entry in read_registry(workspace)? {
        let path = entry
            .sidecar
            .join("embeddings")
            .join(format!("{space}.parquet"));
        if path.is_file() {
            let rows = vectors::read(&path, dims)?;
            ensure!(
                rows.iter()
                    .all(|row| row.episode == entry.episode_id && row.space_id == space),
                "sidecar vector identity mismatch"
            );
            let metadata = entry.sidecar.join("episode.json");
            let primary = if metadata.is_file() {
                let episode: crate::episode::Episode =
                    serde_json::from_slice(&fs::read(metadata)?)?;
                episode.time.reference
            } else {
                "primary".into()
            };
            for row in rows {
                if super::embed::usable(&entry.sidecar, &row.stream, &primary, space)? {
                    restored.push(row);
                }
            }
        }
    }
    // Whole-table merge removes rows belonging to sources no longer registered.
    if restored.is_empty() {
        index.table.delete("true").await?;
    } else {
        let mut merge = index.table.merge_insert(&["id"]);
        merge
            .when_matched_update_all(None)
            .when_not_matched_insert_all()
            .when_not_matched_by_source_delete(None);
        merge
            .execute(input(vectors::batch(&restored, dims)?))
            .await?;
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::vectors::Kind;
    fn fixture(id: &str, episode: &str, vector: Vec<f32>, space: &str) -> VectorRow {
        VectorRow {
            id: id.into(),
            episode: episode.into(),
            stream: "front".into(),
            kind: Kind::Video,
            start_us: 0,
            end_us: 30_000_000,
            vector,
            text: String::new(),
            still: false,
            space_id: space.into(),
            params_hash: "params".into(),
        }
    }
    #[tokio::test]
    async fn per_track_budgets_preserve_visual_candidates_behind_text_distractors() {
        let dir = tempfile::tempdir().unwrap();
        let space = "c".repeat(64);
        let index = VectorIndex::open(dir.path(), &space, 2, true)
            .await
            .unwrap();
        let mut rows: Vec<_> = (0..50)
            .map(|n| {
                let mut row = fixture(&format!("speech-{n}"), "episode", vec![1., 0.], &space);
                row.kind = Kind::Speech;
                row
            })
            .collect();
        rows.push(fixture("visual", "episode", vec![0.1, 0.9], &space));
        index.replace("episode", "front", &rows).await.unwrap();
        assert!(
            index
                .search(&[1., 0.], None, 5)
                .await
                .unwrap()
                .iter()
                .all(|(r, _)| r.kind == Kind::Speech)
        );
        let tracks = index
            .search_tracks(
                &[1., 0.],
                "episode = 'episode'",
                &[Kind::Video, Kind::Speech],
                5,
            )
            .await
            .unwrap();
        assert_eq!(tracks[0][0].0.id, "visual");
        assert_eq!(tracks[1].len(), 5);
    }
    #[tokio::test]
    async fn deleted_index_rebuilds_from_sidecar_and_drops_removed_rows() {
        let dir = tempfile::tempdir().unwrap();
        let space = "b".repeat(64);
        let sidecar = dir.path().join("sidecar");
        let vector_file = sidecar.join("embeddings").join(format!("{space}.parquet"));
        let row = fixture("saved", "dataset/12", vec![0.2, 0.8], &space);
        vectors::write(&vector_file, &[row], 2).unwrap();
        crate::index::discover::register(
            dir.path(),
            crate::index::discover::RegistryEntry {
                episode_id: "dataset/12".into(),
                sha256: "fixture".into(),
                media: dir.path().join("video.mp4"),
                sidecar,
                pending_deletion: false,
            },
        )
        .unwrap();
        let index = rebuild(dir.path(), &space, 2).await.unwrap();
        assert_eq!(index.count().await.unwrap(), 1);
        drop(index);
        fs::remove_dir_all(dir.path().join("index")).unwrap();
        let index = rebuild(dir.path(), &space, 2).await.unwrap();
        assert_eq!(
            index.search(&[0.2, 0.8], None, 1).await.unwrap()[0].0.id,
            "saved"
        );
        drop(index);
        vectors::write(&vector_file, &[], 2).unwrap();
        assert_eq!(
            rebuild(dir.path(), &space, 2)
                .await
                .unwrap()
                .count()
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn prefilter_finds_low_ranked_event_and_replacement_deletes_only_its_scope() {
        let dir = tempfile::tempdir().unwrap();
        let space = "a".repeat(64);
        let index = VectorIndex::open(dir.path(), &space, 2, true)
            .await
            .unwrap();
        let distractors: Vec<_> = (0..20)
            .map(|n| fixture(&format!("other-{n}"), "other/0", vec![1., 0.], &space))
            .collect();
        index
            .replace("other/0", "front", &distractors)
            .await
            .unwrap();
        let row = fixture("target", "dataset/12", vec![0., 1.], &space);
        index.replace("dataset/12", "front", &[row]).await.unwrap();
        let hits = index
            .search(
                &[1., 0.],
                Some("episode = 'dataset/12' AND start_us < 13000000 AND end_us > 12000000"),
                1,
            )
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.id, "target");
        let new = fixture("new", "dataset/12", vec![0.5, 0.5], &space);
        index.replace("dataset/12", "front", &[new]).await.unwrap();
        assert_eq!(index.count().await.unwrap(), 21);
        index.replace("dataset/12", "front", &[]).await.unwrap();
        assert_eq!(index.count().await.unwrap(), 20);
    }
}
