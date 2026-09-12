//! Independent full-text projection of original OCR and transcript records.
//! No embedding model, dimensions, or credentials participate in this cache.
use super::records::Row;
use crate::{annotations::AnnotationFile, storage};
use anyhow::{Context, Result, ensure};
use arrow_array::{
    Array, ArrayRef, Float32Array, Int64Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::{
    Table,
    index::{
        Index,
        scalar::{FtsIndexBuilder, FullTextSearchQuery},
    },
    query::{ExecutableQuery, QueryBase},
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, sync::Arc};

pub const RECIPE: &str = "lexical/simple-cjk-bigrams-no-stem-no-stopwords/1";

/// FTS supplies candidates, but a shared/common word alone cannot earn a
/// default lexical vote. CJK phrases may occur within an unsegmented sentence.
pub fn covers_query(query: &str, text: &str) -> bool {
    let query = query.to_lowercase();
    let text = text.to_lowercase();
    let tokens: Vec<_> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let words: std::collections::BTreeSet<_> = text.split(|c: char| !c.is_alphanumeric()).collect();
    !tokens.is_empty() && tokens.iter().all(|token| {
        if token.chars().any(|c| matches!(c as u32, 0x3400..=0x9fff | 0x20000..=0x3134f | 0x3040..=0x30ff | 0xac00..=0xd7af)) {
            text.contains(token)
        } else { words.contains(token) }
    })
}
#[derive(Serialize, Deserialize)]
struct State {
    fingerprint: String,
    table_version: u64,
}
pub struct LexicalIndex {
    table: Table,
}

// Keep the projection self-contained: dictionary tokenizers would require a
// separate model download. CJK bigrams supplement the original text only in
// the disposable index; result excerpts always come from the untouched Row.
fn searchable(text: &str) -> String {
    let cjk = |c: char| matches!(c as u32, 0x3400..=0x9fff | 0x20000..=0x3134f | 0x3040..=0x30ff | 0xac00..=0xd7af);
    let mut result = text.to_owned();
    for run in text.split(|c| !cjk(c)) {
        let chars: Vec<_> = run.chars().collect();
        if chars.len() <= 2 {
            continue;
        }
        for pair in chars.windows(2) {
            result.push(' ');
            result.extend(pair);
        }
    }
    result
}

fn batch(rows: &[Row]) -> Result<RecordBatch> {
    let ids: Vec<_> = rows
        .iter()
        .map(|r| storage::cache_key(&(&r.episode, &r.stream, &r.annotation, &r.record.id)))
        .collect::<Result<_>>()?;
    let payloads: Vec<_> = rows
        .iter()
        .map(serde_json::to_string)
        .collect::<std::result::Result<_, _>>()?;
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("episode", DataType::Utf8, false),
        Field::new("stream", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("start_us", DataType::Int64, false),
        Field::new("end_us", DataType::Int64, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("record", DataType::Utf8, false),
    ]));
    let texts: Vec<_> = rows
        .iter()
        .map(|r| {
            r.record
                .fields
                .get("text")
                .and_then(serde_json::Value::as_str)
                .map(searchable)
                .context("lexical record has no text")
        })
        .collect::<Result<_>>()?;
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from_iter_values(ids)),
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|r| r.episode.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(
            rows.iter().map(|r| r.stream.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(rows.iter().map(|r| {
            if r.annotation == "transcript" {
                "speech"
            } else {
                "screen"
            }
        }))),
        Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.record.start_us),
        )),
        Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.record.end_us),
        )),
        Arc::new(StringArray::from_iter_values(texts)),
        Arc::new(StringArray::from_iter_values(payloads)),
    ];
    Ok(RecordBatch::try_new(schema, arrays)?)
}
impl LexicalIndex {
    /// Call under the workspace writer lock. Only changed records rewrite the
    /// projection; the manifest is published after both data and FTS succeed.
    pub async fn prepare(workspace: &Path, files: &[AnnotationFile]) -> Result<Self> {
        let rows: Vec<_> = files
            .iter()
            .filter(|f| matches!(f.header.name.as_str(), "transcript" | "screen_text"))
            .flat_map(|f| {
                f.records.iter().map(|record| Row {
                    episode: f.header.episode.clone(),
                    stream: f.header.stream.clone(),
                    annotation: f.header.name.clone(),
                    record: record.clone(),
                })
            })
            .collect();
        let fingerprint = storage::cache_key(&(RECIPE, &rows))?;
        let directory = workspace.join("lexical");
        fs::create_dir_all(&directory)?;
        let db = lancedb::connect(directory.to_str().context("workspace must be UTF-8")?)
            .execute()
            .await?;
        let table = if db
            .table_names()
            .execute()
            .await?
            .iter()
            .any(|n| n == "records")
        {
            db.open_table("records").execute().await?
        } else {
            db.create_empty_table("records", batch(&[])?.schema())
                .execute()
                .await?
        };
        let state_path = directory.join("state.json");
        if let Ok(bytes) = fs::read(&state_path)
            && let Ok(state) = serde_json::from_slice::<State>(&bytes)
            && state.fingerprint == fingerprint
            && state.table_version == table.version().await?
        {
            return Ok(Self { table });
        }
        if rows.is_empty() {
            table.delete("true").await?;
        } else {
            let batch = batch(&rows)?;
            let schema = batch.schema();
            let input = Box::new(RecordBatchIterator::new(
                vec![Ok(batch)].into_iter(),
                schema,
            ));
            let mut merge = table.merge_insert(&["id"]);
            merge
                .when_matched_update_all(None)
                .when_not_matched_insert_all()
                .when_not_matched_by_source_delete(None);
            merge.execute(input).await?;
            table
                .create_index(
                    &["text"],
                    Index::FTS(
                        FtsIndexBuilder::default()
                            .base_tokenizer("simple".into())
                            .stem(false)
                            .remove_stop_words(false),
                    ),
                )
                .replace(true)
                .execute()
                .await?;
        }
        storage::write_json(
            &state_path,
            &State {
                fingerprint,
                table_version: table.version().await?,
            },
        )?;
        Ok(Self { table })
    }
    pub async fn search(
        &self,
        text: &str,
        predicate: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(Row, f32)>> {
        ensure!(
            limit > 0 && !text.trim().is_empty(),
            "lexical search requires text and a positive limit"
        );
        if self.table.count_rows(None).await? == 0 {
            return Ok(Vec::new());
        }
        let mut query = self
            .table
            .query()
            .full_text_search(FullTextSearchQuery::new(searchable(text)))
            .limit(limit);
        if let Some(predicate) = predicate {
            query = query.only_if(predicate);
        }
        let batches: Vec<_> = query.execute().await?.try_collect().await?;
        let mut rows = Vec::new();
        for batch in batches {
            let records = batch
                .column_by_name("record")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .context("missing lexical record")?;
            let scores = batch
                .column_by_name("_score")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
                .context("missing lexical score")?;
            ensure!(
                records.null_count() == 0 && scores.null_count() == 0,
                "null lexical output"
            );
            for i in 0..batch.num_rows() {
                rows.push((serde_json::from_str(records.value(i))?, scores.value(i)));
            }
        }
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn source(episode: &str, annotation: &str, text: &str) -> AnnotationFile {
        serde_json::from_value(json!({
            "header":{"$cerul":"annotation/1","name":annotation,"episode":episode,"stream":"primary","model":{"kind":"fixture","name":"local"},"params":{},"created":"now","cerul_version":"test","input_hash":"source","record_schema":format!("{annotation}/1")},
            "records":[{"id":"same-local-id","start_us":1_000_000,"end_us":2_000_000,"text":text}]
        })).unwrap()
    }
    #[test]
    fn full_query_gate_rejects_common_word_only_and_preserves_identifiers() {
        assert!(!covers_query(
            "A rocket launches from a snowy mountain",
            "a family"
        ));
        assert!(!covers_query("car", "scar"));
        assert!(covers_query("ERROR_A", "reported error_a today"));
        assert!(covers_query("红色杯子", "桌上有红色杯子"));
        assert!(!covers_query("...", "anything"));
    }
    #[tokio::test]
    async fn lexical_cache_is_incremental_scoped_and_preserves_original_text() {
        let dir = tempfile::tempdir().unwrap();
        let first = source("first", "screen_text", "Error_A: drawer is open 抽屉打开");
        let second = source("second", "transcript", "A drawer is closed.");
        let files = vec![first, second.clone()];
        let index = LexicalIndex::prepare(dir.path(), &files).await.unwrap();
        let version = index.table.version().await.unwrap();
        let found = index
            .search("drawer", Some("episode = 'first'"), 10)
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].0.record.fields["text"],
            files[0].records[0].fields["text"]
        );
        assert_eq!(found[0].0.record.start_us, 1_000_000);
        assert_eq!(index.search("抽屉", None, 10).await.unwrap().len(), 1);
        let same = LexicalIndex::prepare(dir.path(), &files).await.unwrap();
        assert_eq!(same.table.version().await.unwrap(), version);
        // Changing the library removes stale documents. There is no embedding
        // configuration or model key to copy, truncate, or regenerate.
        let changed = LexicalIndex::prepare(dir.path(), &[second]).await.unwrap();
        assert!(changed.search("Error", None, 10).await.unwrap().is_empty());
        assert!(
            changed
                .search("drawer", Some("episode = 'first'"), 10)
                .await
                .unwrap()
                .is_empty()
        );
        let empty = LexicalIndex::prepare(dir.path(), &[]).await.unwrap();
        assert!(empty.search("drawer", None, 10).await.unwrap().is_empty());
    }
}
