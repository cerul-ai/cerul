//! Portable views shared by producers and read-only consumers.
use crate::{annotations::AnnotationFile, episode::Episode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Generator {
    pub name: String,
    pub version: String,
    pub website: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Bundle {
    #[serde(rename = "$cerul")]
    pub schema: String,
    pub generator: Generator,
    pub source: Episode,
    /// Hash of the exact exported annotation content, including provenance.
    pub generation: String,
    pub annotations: Vec<AnnotationFile>,
    /// Unpublished requested modules; a bundle may represent partial success.
    pub incomplete: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Export {
    pub episode: String,
    pub annotations: PathBuf,
    pub summary: PathBuf,
    pub records: usize,
    pub tracks: Vec<TrackCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TrackCount {
    pub stream: String,
    pub annotation: String,
    pub records: usize,
}

pub fn text(record: &crate::annotations::Record) -> String {
    if record.fields.contains_key("hands")
        && let Ok(frame) = super::hand::Frame::from_record(record)
    {
        return format!("{} detected hands", frame.hands.len());
    }
    if let Some(value) = record.fields.get("text").and_then(|value| value.as_str()) {
        return value.to_owned();
    }
    record
        .fields
        .iter()
        .map(|(key, value)| {
            let value = value
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| value.to_string());
            format!("{key}: {value}")
        })
        .collect::<Vec<_>>()
        .join("; ")
}
