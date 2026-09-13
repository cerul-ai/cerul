//! Human and portable views of published annotations. Sidecars remain authoritative.
use crate::{annotations::AnnotationFile, episode::Episode, storage};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

fn markdown(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace(['\n', '\r'], " ")
}

pub fn summary(bundle: &Bundle) -> String {
    let mut output = format!(
        "# Annotation summary\n\nEpisode: `{}`\n\n",
        markdown(&bundle.source.episode_id)
    );
    output.push_str("Generated annotations; not human-reviewed.\n\n");
    if !bundle.incomplete.is_empty() {
        output.push_str(&format!(
            "Incomplete: {}\n\n",
            markdown(&bundle.incomplete.join(", "))
        ));
    }
    output.push_str("| Time (seconds) | Camera | Type | Annotation |\n| --- | --- | --- | --- |\n");
    let mut rows = bundle
        .annotations
        .iter()
        .flat_map(|file| file.records.iter().map(move |record| (file, record)))
        .collect::<Vec<_>>();
    rows.sort_by_key(|(file, record)| {
        (
            record.start_us,
            record.end_us,
            &file.header.stream,
            &file.header.name,
        )
    });
    for (file, record) in rows {
        output.push_str(&format!(
            "| {:.3}–{:.3} | {} | {} | {} |\n",
            record.start_us as f64 / 1e6,
            record.end_us as f64 / 1e6,
            markdown(&file.header.stream),
            markdown(&file.header.name),
            markdown(&text(record))
        ));
    }
    if bundle
        .annotations
        .iter()
        .all(|file| file.records.is_empty())
    {
        output.push_str("\nNo applicable annotations were published.\n");
    }
    output.push_str(&format!(
        "\nGenerated with [Cerul](https://cerul.ai) · {}\n\nGeneration: `{}`\n",
        bundle.generator.version, bundle.generation
    ));
    output
}

pub(crate) fn canonicalize(annotations: &mut [AnnotationFile]) {
    annotations.sort_by(|a, b| {
        (&a.header.stream, &a.header.name).cmp(&(&b.header.stream, &b.header.name))
    });
}

pub fn publish(
    episode: &Episode,
    sidecar: &Path,
    modules: &[super::pipeline::ModuleResult],
) -> Result<Export> {
    let mut annotations = Vec::new();
    let mut incomplete = Vec::new();
    for module in modules
        .iter()
        .filter(|module| module.episode == episode.episode_id)
    {
        if module.complete {
            if let Some(path) = &module.path {
                annotations.push(AnnotationFile::read(path)?);
            }
        } else {
            incomplete.push(format!("{}: {}", module.stream, module.annotation));
        }
    }
    // Retain other current semantic types and reconciliation flags in the
    // portable view; a narrow rerun must not silently erase earlier results.
    for stream in &episode.streams {
        if !matches!(stream, crate::episode::Stream::Video { .. }) {
            continue;
        }
        let directory =
            crate::index::stations::stream_directory(sidecar, stream.id(), &episode.time.reference);
        for item in crate::annotations::SEMANTIC_ITEMS {
            let name = format!("semantic.{item}");
            if annotations
                .iter()
                .any(|file| file.header.stream == stream.id() && file.header.name == name)
                || incomplete.contains(&format!("{}: {name}", stream.id()))
            {
                continue;
            }
            let path = super::layout::annotation(&directory, &name);
            if path.is_file() {
                let file = AnnotationFile::read(&path)?;
                if crate::index::stations::has_current_input(episode, &file)? {
                    file.validate_in_range(
                        episode
                            .video_coverage(stream.id())?
                            .ok_or_else(|| anyhow::anyhow!("missing stream coverage"))?,
                        None,
                    )?;
                    annotations.push(file);
                }
            }
        }
    }
    canonicalize(&mut annotations);
    let bundle = Bundle {
        schema: "annotations/1".into(),
        generator: Generator {
            name: "cerul".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            website: "https://cerul.ai".into(),
        },
        source: episode.clone(),
        generation: storage::cache_key(&(&annotations, &incomplete))?,
        annotations,
        incomplete,
    };
    let export = Export {
        episode: episode.episode_id.clone(),
        annotations: sidecar.join("annotations.json"),
        summary: sidecar.join("summary.md"),
        records: bundle
            .annotations
            .iter()
            .map(|file| file.records.len())
            .sum(),
        tracks: bundle
            .annotations
            .iter()
            .map(|file| TrackCount {
                stream: file.header.stream.clone(),
                annotation: file.header.name.clone(),
                records: file.records.len(),
            })
            .collect(),
    };
    // Each view is atomic; the generation in both detects an interrupted pair.
    storage::write_json(&export.annotations, &bundle)?;
    storage::atomic_write(&export.summary, summary(&bundle).as_bytes())?;
    Ok(export)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summary_escapes_model_text_and_preserves_provenance() {
        assert_eq!(markdown("<script>|\n"), "&lt;script&gt;&#124; ");
        let generator = Generator {
            name: "cerul".into(),
            version: "test".into(),
            website: "https://cerul.ai".into(),
        };
        let value = serde_json::to_value(generator).unwrap();
        assert_eq!(value["name"], "cerul");
    }
}
