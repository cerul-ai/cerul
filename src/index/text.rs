//! Derived retrieval text. Original annotation records remain authoritative.
use crate::{annotations::AnnotationFile, episode::TimeRange};
use anyhow::{Context, Result};
use std::collections::BTreeSet;

pub const RECIPE: &str = "retrieval-text/1";
// A conservative UTF-8 byte budget leaves room for the provider's document
// instruction. Split long records without inventing finer timestamp precision.
pub const MAX_BYTES: usize = 6_000;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub range: TimeRange,
    pub text: String,
}

pub fn chunks(file: Option<&AnnotationFile>, range: TimeRange, screen: bool) -> Result<Vec<Chunk>> {
    let Some(file) = file else {
        return Ok(Vec::new());
    };
    let mut output = Vec::new();
    let mut text = String::new();
    let mut seen = BTreeSet::new();
    for record in &file.records {
        if record.range()?.intersection(range).is_none() {
            continue;
        }
        let original = record
            .fields
            .get("text")
            .and_then(serde_json::Value::as_str)
            .context("annotation text missing")?;
        for line in original.lines() {
            // Whitespace-only differences in prose are redundant. Preserve
            // every distinct digit, punctuation mark and letter case.
            let key = line.split_whitespace().collect::<Vec<_>>().join(" ");
            if line.trim().is_empty() || (screen && !seen.insert(key)) {
                continue;
            }
            let mut remaining = line;
            while !remaining.is_empty() {
                if !text.is_empty() {
                    if text.len() + 1 >= MAX_BYTES {
                        output.push(Chunk {
                            range,
                            text: std::mem::take(&mut text),
                        });
                    } else {
                        text.push('\n');
                    }
                }
                let mut end = remaining.len().min(MAX_BYTES - text.len());
                while !remaining.is_char_boundary(end) {
                    end -= 1;
                }
                if end == 0 {
                    output.push(Chunk {
                        range,
                        text: std::mem::take(&mut text),
                    });
                    continue;
                }
                text.push_str(&remaining[..end]);
                remaining = &remaining[end..];
                if !remaining.is_empty() {
                    output.push(Chunk {
                        range,
                        text: std::mem::take(&mut text),
                    });
                }
            }
        }
    }
    if !text.is_empty() {
        output.push(Chunk { range, text });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{Header, Model, Record};
    use std::collections::BTreeMap;
    fn file(lines: &[&str]) -> AnnotationFile {
        AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "screen_text".into(),
                episode: "e".into(),
                stream: "s".into(),
                model: Model {
                    kind: "embedded".into(),
                    name: "fixture".into(),
                    base_url: None,
                },
                params: serde_json::json!({}),
                created: "fixture".into(),
                cerul_version: "fixture".into(),
                input_hash: "fixture".into(),
                record_schema: "screen_text/1".into(),
            },
            records: lines
                .iter()
                .enumerate()
                .map(|(i, t)| Record {
                    id: i.to_string(),
                    start_us: 0,
                    end_us: 10,
                    confidence: None,
                    fields: BTreeMap::from([("text".into(), serde_json::json!(t))]),
                })
                .collect(),
        }
    }
    #[test]
    fn dedup_preserves_identifiers_and_original_evidence() {
        let f = file(&[
            "Speed 10\nError_A",
            "Speed 10\nError_A",
            "Speed 11\nerror_A",
        ]);
        let original = serde_json::to_vec(&f).unwrap();
        let chunks = chunks(Some(&f), TimeRange::new(0, 10).unwrap(), true).unwrap();
        assert_eq!(chunks[0].text, "Speed 10\nError_A\nSpeed 11\nerror_A");
        assert_eq!(serde_json::to_vec(&f).unwrap(), original);
        let spoken = super::chunks(Some(&f), TimeRange::new(0, 10).unwrap(), false).unwrap();
        assert_eq!(spoken[0].text.matches("Speed 10").count(), 2);
    }
    #[test]
    fn oversized_unicode_record_is_lossless_and_does_not_invent_timestamps() {
        let text = "水温92°C".repeat(2000);
        let f = file(&[&text]);
        let range = TimeRange::new(0, 10).unwrap();
        let chunks = chunks(Some(&f), range, true).unwrap();
        assert!(chunks.len() > 1);
        assert!(
            chunks
                .iter()
                .all(|c| c.text.len() <= MAX_BYTES && c.range == range)
        );
        assert_eq!(
            chunks.iter().map(|c| c.text.as_str()).collect::<String>(),
            text
        );
        assert!(
            super::chunks(Some(&f), TimeRange::new(10, 20).unwrap(), true)
                .unwrap()
                .is_empty()
        );
    }
}
