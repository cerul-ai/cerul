//! Grounded search examples from a current overview, with extractive fallbacks.
//! Collection itself makes no model calls.
use crate::{annotations::AnnotationFile, episode::Episode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Suggestion {
    pub query: String,
    /// OCR examples use literal search; other examples use semantic search.
    pub exact: bool,
    pub source: String,
    pub stream: String,
    pub record_id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub input_hash: String,
}

fn query(text: &str) -> Option<String> {
    if text.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return None;
    }
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // Keep an actual excerpt, including CJK text. Do not manufacture paraphrases.
    let excerpt = normalized.split(['\n', '。', '！', '？']).next()?.trim();
    if !(4..=160).contains(&excerpt.chars().count())
        || excerpt.starts_with("http")
        || !excerpt.chars().any(char::is_alphabetic)
    {
        return None;
    }
    Some(excerpt.to_owned())
}

fn from_file(file: &AnnotationFile) -> Option<Suggestion> {
    // Prefer a useful description over a one-word title or an entire transcript.
    let (record, query) = file
        .records
        .iter()
        .filter_map(|record| {
            let text = record.fields.get("text")?.as_str()?;
            let excerpt = if file.header.name == "screen_text" {
                text.lines().find_map(|line| {
                    let candidate = query(line)?;
                    line.contains(&candidate).then_some(candidate)
                })?
            } else {
                query(text)?
            };
            Some((record, excerpt))
        })
        .max_by_key(|(_, text)| text.chars().count().min(80))?;
    Some(Suggestion {
        query,
        exact: file.header.name == "screen_text",
        source: file.header.name.clone(),
        stream: file.header.stream.clone(),
        record_id: record.id.clone(),
        start_us: record.start_us,
        end_us: record.end_us,
        input_hash: file.header.input_hash.clone(),
    })
}

pub(super) fn collect(
    episode: &Episode,
    sidecar: &Path,
    stream: &str,
    transcript: Option<&AnnotationFile>,
    screen: Option<&AnnotationFile>,
) -> Vec<Suggestion> {
    let directory = super::stations::stream_directory(sidecar, stream, &episode.time.reference);
    let semantic = AnnotationFile::read(&directory.join("semantic.subtask.jsonl")).ok();
    let mut suggestions: Vec<Suggestion> = Vec::new();
    let Ok(duration) = episode.duration_us() else {
        return suggestions;
    };
    if let Ok(file) = AnnotationFile::read(&directory.join("semantic.summary.jsonl"))
        && file.header.stream == stream
        && file.header.episode == episode.episode_id
        && super::stations::has_current_input(episode, &file).unwrap_or(false)
        && super::understanding::current_dependencies(&directory, &file).unwrap_or(false)
        && let Ok(Some(coverage)) = episode.video_coverage(stream)
        && file.validate_in_range(coverage, None).is_ok()
        && let Some(record) = file.records.first()
        && let Ok(summary) = serde_json::from_value::<super::understanding::Summary>(
            serde_json::json!(record.fields),
        )
    {
        for suggestion in summary.suggestions {
            let expected = match suggestion.kind.as_str() {
                "visual" => "semantic.scene",
                "speech" => "transcript",
                "screen" => "screen_text",
                _ => continue,
            };
            let Ok(source) = AnnotationFile::read(&directory.join(format!("{expected}.jsonl")))
            else {
                continue;
            };
            let valid = !suggestion.source_refs.is_empty()
                && suggestion.source_refs.iter().all(|reference| {
                    reference.annotation == expected
                        && source.records.iter().any(|record| {
                            record.id == reference.record_id
                                && super::understanding::revision(record).ok().as_deref()
                                    == Some(reference.revision.as_str())
                        })
                });
            if !valid {
                continue;
            }
            let Some(record) = source
                .records
                .iter()
                .find(|r| r.id == suggestion.source_refs[0].record_id)
            else {
                continue;
            };
            suggestions.push(Suggestion {
                query: suggestion.query,
                exact: false,
                source: expected.into(),
                stream: stream.into(),
                record_id: record.id.clone(),
                start_us: record.start_us,
                end_us: record.end_us,
                input_hash: source.header.input_hash.clone(),
            });
        }
        // A current overview already chose zero to three useful examples.
        // Extractive fallbacks are for missing overviews, not padding the list.
        return suggestions;
    }
    for file in [semantic.as_ref(), transcript, screen]
        .into_iter()
        .flatten()
    {
        if suggestions.len() == 3 {
            break;
        }
        if file.header.stream != stream
            || !super::stations::has_current_input(episode, file).unwrap_or(false)
            || (if file.header.name.starts_with("semantic.") {
                episode
                    .video_coverage(stream)
                    .ok()
                    .flatten()
                    .is_none_or(|coverage| file.validate_in_range(coverage, None).is_err())
            } else {
                file.validate(duration, None).is_err()
            })
        {
            continue;
        }
        if let Some(item) = from_file(file)
            && !suggestions
                .iter()
                .any(|old| old.query.to_lowercase() == item.query.to_lowercase())
        {
            suggestions.push(item);
        }
    }
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ocr_examples_remain_literal_and_keep_source_references() {
        let file: AnnotationFile = serde_json::from_value(serde_json::json!({
            "header": {"$cerul":"annotation/1", "name":"screen_text", "episode":"ep", "stream":"primary", "model":{"kind":"local", "name":"ocr"}, "params":{}, "created":"now", "cerul_version":"test", "input_hash":"generation-1", "record_schema":"screen_text/1"},
            "records": [{"id":"ocr-4", "start_us":4000000, "end_us":6000000, "text":"Too   many   spaces\nTRAFFIC FLOW"}]
        })).unwrap();
        let example = from_file(&file).unwrap();
        assert!(
            file.records[0].fields["text"]
                .as_str()
                .unwrap()
                .contains(&example.query)
        );
        assert_eq!(example.query, "TRAFFIC FLOW");
        assert!(example.exact);
        assert_eq!(example.record_id, "ocr-4");
        assert_eq!(example.start_us, 4000000);
        assert_eq!(example.input_hash, "generation-1");
    }
    #[test]
    fn excerpts_are_bounded_and_do_not_invent_content() {
        assert_eq!(
            query("Cars   moving through an intersection"),
            Some("Cars moving through an intersection".into())
        );
        assert_eq!(query("打开抽屉。然后放入杯子"), Some("打开抽屉".into()));
        assert!(query("\x1b[31munsafe").is_none());
        assert!(query("https://example.org").is_none());
        assert!(query(&"a".repeat(161)).is_none());
        assert!(query("123456").is_none());
    }
}
