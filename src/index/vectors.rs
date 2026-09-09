use anyhow::{Context, Result, ensure};
use arrow_array::{
    Array, ArrayRef, BooleanArray, FixedSizeListArray, Float32Array, Int64Array, RecordBatch,
    StringArray, types::Float32Type,
};
use arrow_schema::{DataType, Field, Schema};
use parquet::{
    arrow::{ArrowWriter, arrow_reader::ParquetRecordBatchReaderBuilder},
    basic::Compression,
    file::properties::WriterProperties,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::Path,
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Video,
    Speech,
    Screen,
}
impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Speech => "speech",
            Self::Screen => "screen",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        match value {
            "video" => Ok(Self::Video),
            "speech" => Ok(Self::Speech),
            "screen" => Ok(Self::Screen),
            _ => anyhow::bail!("invalid embedding kind"),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VectorRow {
    pub id: String,
    pub episode: String,
    pub stream: String,
    pub kind: Kind,
    pub start_us: i64,
    pub end_us: i64,
    pub vector: Vec<f32>,
    pub text: String,
    pub still: bool,
    pub space_id: String,
    pub params_hash: String,
}
impl VectorRow {
    pub fn validate(&self, dims: usize) -> Result<()> {
        crate::episode::TimeRange::new(self.start_us, self.end_us)?;
        ensure!(
            !self.id.is_empty() && !self.episode.is_empty() && !self.stream.is_empty(),
            "incomplete vector identity"
        );
        ensure!(
            self.vector.len() == dims && self.vector.iter().all(|v| v.is_finite()),
            "invalid vector dimensions or values"
        );
        ensure!(
            self.vector.iter().any(|v| *v != 0.),
            "zero embedding vector"
        );
        Ok(())
    }
}

pub fn batch(rows: &[VectorRow], dims: usize) -> Result<RecordBatch> {
    ensure!(
        dims > 0 && dims <= i32::MAX as usize,
        "invalid embedding dimensions"
    );
    let mut ids = std::collections::BTreeSet::new();
    for row in rows {
        row.validate(dims)?;
        ensure!(ids.insert(&row.id), "duplicate embedding row ID");
    }
    let vector = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        rows.iter()
            .map(|row| Some(row.vector.iter().map(|v| Some(*v)))),
        dims as i32,
    );
    let strings = |f: fn(&VectorRow) -> &str| {
        Arc::new(StringArray::from_iter_values(rows.iter().map(f))) as ArrayRef
    };
    let arrays: Vec<ArrayRef> = vec![
        strings(|r| &r.id),
        strings(|r| &r.episode),
        strings(|r| &r.stream),
        strings(|r| r.kind.as_str()),
        Arc::new(Int64Array::from_iter_values(
            rows.iter().map(|r| r.start_us),
        )),
        Arc::new(Int64Array::from_iter_values(rows.iter().map(|r| r.end_us))),
        Arc::new(vector),
        strings(|r| &r.text),
        Arc::new(BooleanArray::from_iter(rows.iter().map(|r| Some(r.still)))),
        strings(|r| &r.space_id),
        strings(|r| &r.params_hash),
    ];
    let fields = vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("episode", DataType::Utf8, false),
        Field::new("stream", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("start_us", DataType::Int64, false),
        Field::new("end_us", DataType::Int64, false),
        Field::new("vector", arrays[6].data_type().clone(), false),
        Field::new("text", DataType::Utf8, false),
        Field::new("still", DataType::Boolean, false),
        Field::new("space_id", DataType::Utf8, false),
        Field::new("params_hash", DataType::Utf8, false),
    ];
    Ok(RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)?)
}

fn column<'a, T: Array + 'static>(batch: &'a RecordBatch, name: &str) -> Result<&'a T> {
    let column = batch
        .column_by_name(name)
        .with_context(|| format!("missing column {name}"))?;
    ensure!(
        column.null_count() == 0,
        "null values in required column {name}"
    );
    column
        .as_any()
        .downcast_ref::<T>()
        .with_context(|| format!("invalid column type {name}"))
}
pub fn rows(batch: &RecordBatch, dims: usize) -> Result<Vec<VectorRow>> {
    let vectors = column::<FixedSizeListArray>(batch, "vector")?;
    ensure!(
        vectors.value_length() == dims as i32,
        "embedding dimension mismatch"
    );
    let mut result = Vec::new();
    for i in 0..batch.num_rows() {
        let string =
            |name| -> Result<String> { Ok(column::<StringArray>(batch, name)?.value(i).into()) };
        let value = vectors.value(i);
        let values = value
            .as_any()
            .downcast_ref::<Float32Array>()
            .context("invalid vector scalar type")?;
        ensure!(values.null_count() == 0, "null vector component");
        let row = VectorRow {
            id: string("id")?,
            episode: string("episode")?,
            stream: string("stream")?,
            kind: Kind::parse(&string("kind")?)?,
            start_us: column::<Int64Array>(batch, "start_us")?.value(i),
            end_us: column::<Int64Array>(batch, "end_us")?.value(i),
            vector: values.values().to_vec(),
            text: string("text")?,
            still: column::<BooleanArray>(batch, "still")?.value(i),
            space_id: string("space_id")?,
            params_hash: string("params_hash")?,
        };
        row.validate(dims)?;
        result.push(row);
    }
    Ok(result)
}

pub fn write(path: &Path, rows: &[VectorRow], dims: usize) -> Result<()> {
    let batch = batch(rows, dims)?;
    let parent = path.parent().context("embedding file has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .build();
    let mut writer = ArrowWriter::try_new(temp.reopen()?, batch.schema(), Some(properties))?;
    writer.write(&batch)?;
    writer.close()?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn read(path: &Path, dims: usize) -> Result<Vec<VectorRow>> {
    let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?.build()?;
    let mut result = Vec::new();
    for batch in reader {
        result.extend(rows(&batch?, dims)?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn fixture() -> VectorRow {
        VectorRow {
            id: "one".into(),
            episode: "dataset/12".into(),
            stream: "front".into(),
            kind: Kind::Video,
            start_us: 0,
            end_us: 30_000_000,
            vector: vec![0.2, 0.8],
            text: "cup".into(),
            still: false,
            space_id: "test-space".into(),
            params_hash: "params".into(),
        }
    }
    #[test]
    fn vectors_roundtrip_without_model_calls_and_invalid_replacement_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vectors.parquet");
        let row = fixture();
        write(&path, std::slice::from_ref(&row), 2).unwrap();
        let restored = read(&path, 2).unwrap();
        assert_eq!(restored[0].vector, row.vector);
        assert_eq!(restored[0].episode, row.episode);
        let mut invalid = row;
        invalid.vector[0] = f32::NAN;
        assert!(write(&path, &[invalid], 2).is_err());
        assert_eq!(read(&path, 2).unwrap()[0].vector, restored[0].vector);
        assert!(read(&path, 3).is_err());
    }
}
