//! Versioned annotation files. Validate a complete module before publishing it.
use crate::{episode::TimeRange, storage};
use anyhow::{Context, Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

pub const VERBS: &[&str] = &[
    "reach", "grasp", "regrasp", "lift", "carry", "place", "release", "push", "pull", "open",
    "close", "insert", "rotate", "pour", "wipe",
];
pub const SEMANTIC_ITEMS: &[&str] = &[
    "task",
    "subtask",
    "event",
    "interaction",
    "state",
    "flag",
    "progress",
];

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Model {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub base_url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Header {
    #[serde(rename = "$cerul")]
    pub schema: String,
    pub name: String,
    pub episode: String,
    pub stream: String,
    pub model: Model,
    pub params: Value,
    pub created: String,
    pub cerul_version: String,
    pub input_hash: String,
    pub record_schema: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Record {
    pub id: String,
    pub start_us: i64,
    pub end_us: i64,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}
impl Record {
    pub fn range(&self) -> Result<TimeRange> {
        TimeRange::new(self.start_us, self.end_us)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AnnotationFile {
    pub header: Header,
    pub records: Vec<Record>,
}

fn required_text<'a>(record: &'a Record, key: &str) -> Result<&'a str> {
    let value = record
        .fields
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("annotation requires string field {key}"))?;
    ensure!(!value.trim().is_empty(), "annotation field {key} is empty");
    Ok(value)
}
fn optional_string(record: &Record, key: &str) -> Result<()> {
    if let Some(value) = record.fields.get(key) {
        ensure!(
            value.is_null() || value.is_string(),
            "{key} must be string or null"
        );
    }
    Ok(())
}
impl AnnotationFile {
    pub fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        let mut lines = text.lines();
        let header = serde_json::from_str(lines.next().context("empty annotation file")?)?;
        let records = lines
            .enumerate()
            .map(|(index, line)| {
                serde_json::from_str(line)
                    .with_context(|| format!("invalid annotation at line {}", index + 2))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { header, records })
    }
    pub fn validate(&self, duration_us: i64, ontology: Option<&BTreeSet<String>>) -> Result<()> {
        let h = &self.header;
        ensure!(
            h.schema == "annotation/1" && h.record_schema == format!("{}/1", h.name),
            "unsupported annotation schema"
        );
        ensure!(
            matches!(h.name.as_str(), "transcript" | "screen_text")
                || h.name
                    .strip_prefix("semantic.")
                    .is_some_and(|name| SEMANTIC_ITEMS.contains(&name)),
            "unsupported annotation name"
        );
        ensure!(
            !h.episode.is_empty() && !h.stream.is_empty() && !h.input_hash.is_empty(),
            "incomplete annotation provenance"
        );
        ensure!(duration_us > 0, "invalid episode duration");
        let mut ids = BTreeSet::new();
        let mut expected_subtask_start = 0;
        for (index, record) in self.records.iter().enumerate() {
            let range = record.range()?;
            ensure!(
                range.end_us <= duration_us,
                "annotation extends beyond episode"
            );
            ensure!(
                !record.id.is_empty() && ids.insert(&record.id),
                "duplicate or empty annotation ID"
            );
            ensure!(
                record
                    .confidence
                    .is_none_or(|n| n.is_finite() && (0.0..=1.0).contains(&n)),
                "invalid confidence"
            );
            for key in ["id", "start_us", "end_us", "confidence"] {
                ensure!(
                    !record.fields.contains_key(key),
                    "reserved record field collision"
                );
            }
            match h.name.as_str() {
                "transcript" => {
                    required_text(record, "text")?;
                    optional_string(record, "lang")?;
                }
                "screen_text" | "semantic.task" => {
                    required_text(record, "text")?;
                }
                "semantic.subtask" => {
                    required_text(record, "text")?;
                    ensure!(
                        record.fields.get("index").and_then(Value::as_u64) == Some(index as u64),
                        "subtask indexes must be contiguous and zero based"
                    );
                    ensure!(
                        range.start_us == expected_subtask_start,
                        "subtask coverage has gap, overlap, or out-of-order records"
                    );
                    expected_subtask_start = range.end_us;
                }
                "semantic.event" => {
                    let verb = required_text(record, "verb")?;
                    if let Some(ontology) = ontology {
                        ensure!(ontology.contains(verb), "event verb is outside ontology");
                    }
                    if let Some(value) = record.fields.get("objects") {
                        ensure!(
                            value.is_null()
                                || value
                                    .as_array()
                                    .is_some_and(|items| items.iter().all(Value::is_string)),
                            "objects must be an array of strings"
                        );
                    }
                    optional_string(record, "actor")?;
                    optional_string(record, "outcome")?;
                }
                "semantic.interaction" => {
                    required_text(record, "hand")?;
                    required_text(record, "object")?;
                    if let Some(value) = record.fields.get("contact") {
                        ensure!(
                            value.is_null() || value.is_boolean(),
                            "contact must be boolean"
                        );
                    }
                }
                "semantic.state" => {
                    required_text(record, "object")?;
                    required_text(record, "attribute")?;
                    optional_string(record, "before")?;
                    optional_string(record, "after")?;
                }
                "semantic.flag" => {
                    required_text(record, "kind")?;
                    optional_string(record, "note")?;
                }
                "semantic.progress" => {
                    let value = record
                        .fields
                        .get("value")
                        .and_then(Value::as_f64)
                        .context("progress requires value")?;
                    ensure!(
                        value.is_finite() && (0.0..=1.0).contains(&value),
                        "progress must be in [0,1]"
                    );
                    if let Some(value) = record.fields.get("done") {
                        ensure!(
                            value.is_null() || value.is_boolean(),
                            "done must be boolean"
                        );
                    }
                }
                _ => bail!("unsupported annotation"),
            }
        }
        if h.name == "semantic.subtask" {
            ensure!(
                expected_subtask_start == duration_us,
                "subtasks do not cover the whole episode"
            );
        }
        Ok(())
    }
    pub fn publish(
        &self,
        path: &Path,
        duration_us: i64,
        ontology: Option<&BTreeSet<String>>,
    ) -> Result<()> {
        self.validate(duration_us, ontology)?;
        let mut bytes = serde_json::to_vec(&self.header)?;
        bytes.push(b'\n');
        for record in &self.records {
            serde_json::to_writer(&mut bytes, record)?;
            bytes.push(b'\n');
        }
        storage::atomic_write(path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> AnnotationFile {
        AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "semantic.subtask".into(),
                episode: "dataset/12".into(),
                stream: "front".into(),
                model: Model {
                    kind: "gemini".into(),
                    name: "test".into(),
                    base_url: None,
                },
                params: serde_json::json!({}),
                created: "2026-09-08T00:00:00Z".into(),
                cerul_version: env!("CARGO_PKG_VERSION").into(),
                input_hash: "sha256:fixture".into(),
                record_schema: "semantic.subtask/1".into(),
            },
            records: vec![Record {
                id: "one".into(),
                start_us: 0,
                end_us: 100,
                confidence: None,
                fields: BTreeMap::from([
                    ("text".into(), Value::String("Pick up the cup".into())),
                    ("index".into(), Value::from(0)),
                ]),
            }],
        }
    }
    #[test]
    fn invalid_module_cannot_replace_valid_sidecar() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("semantic.subtask.jsonl");
        let mut file = sample();
        file.publish(&path, 100, None).unwrap();
        let before = fs::read(&path).unwrap();
        file.records[0].start_us = 1;
        assert!(file.publish(&path, 100, None).is_err());
        assert_eq!(before, fs::read(&path).unwrap());
        let reloaded = AnnotationFile::read(&path).unwrap();
        reloaded.validate(100, None).unwrap();
        assert_eq!(reloaded.records[0].start_us, 0);
    }
    #[test]
    fn ordinary_video_verbs_are_open_and_dataset_ontology_is_enforced() {
        let mut file = sample();
        file.header.name = "semantic.event".into();
        file.header.record_schema = "semantic.event/1".into();
        file.records[0].fields = BTreeMap::from([("verb".into(), Value::String("present".into()))]);
        file.validate(100, None).unwrap();
        assert!(
            file.validate(100, Some(&VERBS.iter().map(|v| v.to_string()).collect()))
                .is_err()
        );
    }
}
