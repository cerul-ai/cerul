//! Useful search examples from current evidence, without new model calls.
use crate::{annotations::AnnotationFile, episode::Episode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Suggestion {
    pub query: String,
    /// Whether this example requires literal rather than semantic search.
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
    let excerpt = normalized.split(['。', '！', '？']).next()?.trim();
    let words = excerpt
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphabetic))
        .count();
    let cjk = excerpt
        .chars()
        .filter(|c| matches!(*c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff | 0xac00..=0xd7af))
        .count();
    // Require a phrase rather than promoting a logo, amount or isolated token.
    // This is a presentation filter, not a judgment of transcription accuracy.
    if !(4..=160).contains(&excerpt.chars().count())
        || excerpt.starts_with("http")
        || (words < 3 && cjk < 4)
    {
        return None;
    }
    Some(excerpt.to_owned())
}

fn from_file(file: &AnnotationFile, field: &str) -> Vec<Suggestion> {
    let mut items: Vec<_> = file
        .records
        .iter()
        .filter_map(|record| {
            Some(Suggestion {
                query: query(record.fields.get(field)?.as_str()?)?,
                exact: false,
                source: file.header.name.clone(),
                stream: file.header.stream.clone(),
                record_id: record.id.clone(),
                start_us: record.start_us,
                end_us: record.end_us,
                input_hash: file.header.input_hash.clone(),
            })
        })
        .collect();
    items.sort_by_key(|item| (item.start_us, item.end_us));
    items
}

fn distinct(items: &[Suggestion], candidate: &Suggestion) -> bool {
    !items.iter().any(|old| {
        old.query.to_lowercase() == candidate.query.to_lowercase()
            || (old.source == candidate.source
                && old.stream == candidate.stream
                && old.record_id == candidate.record_id)
    })
}

fn fill(items: &mut Vec<Suggestion>, candidates: Vec<Suggestion>) {
    let mut unique = Vec::new();
    for candidate in candidates {
        if distinct(items, &candidate) && distinct(&unique, &candidate) {
            unique.push(candidate);
        }
    }
    let count = (3 - items.len()).min(unique.len());
    for i in 0..count {
        // Spread examples across the remaining timeline, not just its opening.
        let index = if count == 1 {
            unique.len() / 2
        } else {
            i * (unique.len() - 1) / (count - 1)
        };
        items.push(unique[index].clone());
    }
}

fn visual_examples(
    scenes: &AnnotationFile,
    generated: &[super::understanding::QuerySuggestion],
) -> Vec<Suggestion> {
    let mut items = Vec::new();
    for suggestion in generated.iter().filter(|s| s.kind == "visual") {
        let Some(query) = query(&suggestion.query) else {
            continue;
        };
        if suggestion.source_refs.is_empty()
            || !suggestion.source_refs.iter().all(|reference| {
                reference.annotation == "semantic.scene"
                    && scenes.records.iter().any(|record| {
                        record.id == reference.record_id
                            && super::understanding::revision(record).ok().as_deref()
                                == Some(reference.revision.as_str())
                    })
            })
        {
            continue;
        }
        let record = scenes
            .records
            .iter()
            .find(|r| r.id == suggestion.source_refs[0].record_id)
            .expect("validated source reference");
        let item = Suggestion {
            query,
            exact: false,
            source: "semantic.scene".into(),
            stream: scenes.header.stream.clone(),
            record_id: record.id.clone(),
            start_us: record.start_us,
            end_us: record.end_us,
            input_hash: scenes.header.input_hash.clone(),
        };
        if distinct(&items, &item) {
            items.push(item);
        }
        if items.len() == 3 {
            break;
        }
    }
    fill(&mut items, from_file(scenes, "description"));
    items
}

pub(super) fn collect(
    episode: &Episode,
    sidecar: &Path,
    stream: &str,
    transcript: Option<&AnnotationFile>,
    _screen: Option<&AnnotationFile>,
) -> Vec<Suggestion> {
    let directory = super::stations::stream_directory(sidecar, stream, &episode.time.reference);
    let Ok(Some(coverage)) = episode.video_coverage(stream) else {
        return Vec::new();
    };
    let current = |file: &AnnotationFile| {
        file.header.episode == episode.episode_id
            && file.header.stream == stream
            && super::stations::has_current_input(episode, file).unwrap_or(false)
            && file.validate_in_range(coverage, None).is_ok()
    };
    let scenes = AnnotationFile::read(&directory.join("semantic.scene.jsonl"))
        .ok()
        .filter(|file| file.header.name == "semantic.scene" && current(file));
    if let Some(scenes) = scenes {
        let generated = AnnotationFile::read(&directory.join("semantic.summary.jsonl"))
            .ok()
            .filter(|file| {
                file.header.name == "semantic.summary"
                    && current(file)
                    && super::understanding::current_dependencies(&directory, file).unwrap_or(false)
            })
            .and_then(|file| {
                serde_json::from_value::<super::understanding::Summary>(serde_json::json!(
                    file.records.first()?.fields
                ))
                .ok()
            })
            .map(|summary| summary.suggestions)
            .unwrap_or_default();
        let items = visual_examples(&scenes, &generated);
        if !items.is_empty() {
            return items;
        }
    }
    let semantic = AnnotationFile::read(&crate::annotate::layout::annotation(
        &directory,
        "semantic.subtask",
    ))
    .ok()
    .filter(|file| file.header.name == "semantic.subtask" && current(file));
    // Fall back by modality, not by filling a visual list with stray words.
    // OCR remains searchable explicitly; it is not a default recommendation.
    for file in [
        semantic.as_ref(),
        transcript.filter(|file| file.header.name == "transcript" && current(file)),
    ]
    .into_iter()
    .flatten()
    {
        let mut items = Vec::new();
        fill(&mut items, from_file(file, "text"));
        if !items.is_empty() {
            return items;
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subtask_fallback_reads_current_and_legacy_layouts() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("demo.mp4");
        crate::media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=2:duration=1",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let episode = crate::index::discover::ordinary_episode(&source).unwrap();
        let sidecar = dir.path().join("sidecar");
        let key = crate::index::stations::station_key(
            &episode,
            "primary",
            "semantic.subtask",
            &serde_json::json!({}),
        )
        .unwrap();
        let file: AnnotationFile = serde_json::from_value(serde_json::json!({
            "header": {"$cerul":"annotation/1", "name":"semantic.subtask", "episode":episode.episode_id, "stream":"primary", "model":{"kind":"fixture", "name":"test"}, "params":{}, "created":"now", "cerul_version":"test", "input_hash":key, "record_schema":"semantic.subtask/1"},
            "records": [{"id":"subtask-0", "start_us":0, "end_us":1000000, "index":0, "text":"Place the cup on the table"}]
        })).unwrap();
        let current = crate::annotate::layout::annotation(&sidecar, "semantic.subtask");
        file.publish(&current, 1_000_000, None).unwrap();
        for legacy in [false, true] {
            if legacy {
                std::fs::rename(&current, sidecar.join("semantic.subtask.jsonl")).unwrap();
            }
            let suggestions = collect(&episode, &sidecar, "primary", None, None);
            assert_eq!(suggestions.len(), 1);
            assert_eq!(suggestions[0].query, "Place the cup on the table");
            assert_eq!(suggestions[0].input_hash, key);
        }
    }

    fn scenes(descriptions: &[&str]) -> AnnotationFile {
        serde_json::from_value(serde_json::json!({
            "header": {"$cerul":"annotation/1", "name":"semantic.scene", "episode":"ep", "stream":"primary", "model":{"kind":"fixture", "name":"test"}, "params":{}, "created":"now", "cerul_version":"test", "input_hash":"generation-1", "record_schema":"semantic.scene/1"},
            "records": descriptions.iter().enumerate().map(|(i, description)| serde_json::json!({
                "id":format!("scene-{i}"), "start_us":i * 1_000_000, "end_us":(i+1) * 1_000_000, "description":description
            })).collect::<Vec<_>>()
        })).unwrap()
    }

    #[test]
    fn visual_examples_replace_noisy_modalities_with_distinct_observed_actions() {
        let scenes = scenes(&[
            "A worker guides a panel into the machine.",
            "The worker retrieves another panel from the floor.",
            "The worker aligns the panel and pushes it forward.",
        ]);
        let reference = super::super::understanding::SourceRef {
            annotation: "semantic.scene".into(),
            record_id: scenes.records[0].id.clone(),
            revision: super::super::understanding::revision(&scenes.records[0]).unwrap(),
        };
        let mut generated: Vec<super::super::understanding::QuerySuggestion> = [
            ("visual", "feeding panel into machine"),
            ("screen", "INOVANCE"),
            ("speech", "1000円"),
        ]
        .into_iter()
        .map(
            |(kind, query)| super::super::understanding::QuerySuggestion {
                query: query.into(),
                kind: kind.into(),
                source_refs: vec![reference.clone()],
            },
        )
        .collect();
        let items = visual_examples(&scenes, &generated);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].query, "feeding panel into machine");
        assert_eq!(
            items[1].query,
            "The worker retrieves another panel from the floor."
        );
        assert_eq!(
            items[2].query,
            "The worker aligns the panel and pushes it forward."
        );
        assert!(
            items
                .iter()
                .all(|item| item.source == "semantic.scene" && !item.exact)
        );
        assert_eq!(items[1].record_id, "scene-1");
        assert_eq!(items[1].start_us, 1_000_000);
        assert_eq!(items[1].input_hash, "generation-1");
        // A second paraphrase of the same scene must not displace another action.
        generated[1].kind = "visual".into();
        generated[1].query = "worker feeding the machine".into();
        assert_eq!(visual_examples(&scenes, &generated).len(), 3);
        assert_eq!(visual_examples(&scenes, &generated)[1].record_id, "scene-1");
        // A stale generated query cannot override the current scene description.
        generated[0].source_refs[0].revision = "stale".into();
        generated.truncate(1);
        assert_eq!(
            visual_examples(&scenes, &generated)[0].query,
            "A worker guides a panel into the machine."
        );
    }

    #[test]
    fn fallback_examples_span_the_timeline_and_deduplicate_repeated_descriptions() {
        let scenes = scenes(&[
            "A worker opens the cabinet.",
            "A worker takes out a cup.",
            "A worker fills the cup.",
            "A worker fills the cup.",
            "A worker places the cup down.",
            "A worker closes the cabinet.",
        ]);
        let items = visual_examples(&scenes, &[]);
        assert_eq!(
            items
                .iter()
                .map(|item| item.record_id.as_str())
                .collect::<Vec<_>>(),
            ["scene-0", "scene-2", "scene-5"]
        );
        assert_eq!(
            visual_examples(&self::scenes(&["A worker opens a cabinet."]), &[]).len(),
            1
        );
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
        for noise in ["123456", "INOVANCE", "AUTOMATIC", "1000円", "hello world"] {
            assert!(query(noise).is_none(), "{noise}");
        }
    }
}
