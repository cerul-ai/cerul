//! Recoverable semantic modules with window-local validation and atomic publication.
use super::{contact, schema};
use crate::{
    annotations::{AnnotationFile, Header, Model, Record},
    episode::{Episode, Stream, TimeRange},
    events::{Event, EventSink},
    index::stations::{station_key, stream_directory},
    media,
    providers::{Failure, Input, Provider, ProviderError, probes},
    storage::{self, Checkpoints},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Clone)]
pub struct Options {
    pub window_us: i64,
    pub fps: f64,
    pub recompute: bool,
    pub ontology: Option<BTreeSet<String>>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            window_us: 30_000_000,
            fps: 2.,
            recompute: false,
            ontology: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub annotation: AnnotationFile,
    pub conflicts: Vec<Record>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Unit {
    window: TimeRange,
    records: Vec<Record>,
}
fn prompt(item: &str) -> Result<&'static str> {
    Ok(match item {
        "task" => include_str!("../../prompts/task.md"),
        "subtask" => include_str!("../../prompts/subtask.md"),
        "event" => include_str!("../../prompts/event.md"),
        "interaction" => include_str!("../../prompts/interaction.md"),
        "state" => include_str!("../../prompts/state.md"),
        "flag" => include_str!("../../prompts/flag.md"),
        "progress" => include_str!("../../prompts/progress.md"),
        _ => anyhow::bail!("unknown semantic item"),
    })
}
fn interrupted(provider: &Provider) -> Result<()> {
    if provider.cancel.is_cancelled() {
        return Err(ProviderError {
            kind: Failure::Cancelled,
            message: "operation cancelled".into(),
        }
        .into());
    }
    Ok(())
}
fn equivalent(a: &Record, b: &Record) -> bool {
    a.fields
        .iter()
        .filter(|(key, _)| key.as_str() != "index")
        .eq(b.fields.iter().filter(|(key, _)| key.as_str() != "index"))
}
fn normalize(
    item: &str,
    unit: &Unit,
    header: &Header,
    pts: &[i64],
    ontology: Option<&BTreeSet<String>>,
) -> Result<Vec<Record>> {
    let duration = unit.window.end_us - unit.window.start_us;
    AnnotationFile {
        header: header.clone(),
        records: unit.records.clone(),
    }
    .validate(duration, ontology)?;
    let mut records = unit.records.clone();
    for record in &mut records {
        record.start_us = if record.start_us == 0 {
            unit.window.start_us
        } else {
            contact::snap(unit.window, record.start_us, pts)?
        };
        record.end_us = contact::snap(unit.window, record.end_us, pts)?;
        ensure!(
            record.end_us > record.start_us,
            "{item} interval collapsed while snapping to frame PTS"
        );
    }
    Ok(records)
}
fn reconcile(
    item: &str,
    units: &[Unit],
    header: &Header,
    pts: &[i64],
    coverage: TimeRange,
    ontology: Option<&BTreeSet<String>>,
) -> Result<Product> {
    let normalized = units
        .iter()
        .map(|unit| normalize(item, unit, header, pts, ontology))
        .collect::<Result<Vec<_>>>()?;
    let mut cuts = vec![coverage.start_us];
    for pair in units.windows(2) {
        let midpoint =
            pair[1].window.start_us + (pair[0].window.end_us - pair[1].window.start_us) / 2;
        let boundary = pts
            .iter()
            .copied()
            .filter(|t| *t >= pair[1].window.start_us && *t <= pair[0].window.end_us)
            .min_by_key(|t| t.abs_diff(midpoint))
            .ok_or_else(|| anyhow::anyhow!("no actual frame PTS in annotation window overlap"))?;
        ensure!(
            boundary > *cuts.last().unwrap(),
            "annotation windows have no ordered frame boundaries"
        );
        cuts.push(boundary);
    }
    cuts.push(coverage.end_us);
    let mut conflicts = Vec::new();
    let mut conflict_keys = BTreeSet::new();
    for pair in normalized.windows(2) {
        for a in &pair[0] {
            for b in &pair[1] {
                if !equivalent(a, b)
                    && let Some(overlap) = a.range()?.intersection(b.range()?)
                {
                    let key = (overlap.start_us, overlap.end_us);
                    if conflict_keys.insert(key) {
                        conflicts.push(Record{id:format!("{item}-conflict-{}",conflicts.len()),start_us:overlap.start_us,end_us:overlap.end_us,confidence:None,fields:BTreeMap::from([("kind".into(),json!("window_conflict")),("note".into(),json!(format!("Overlapping semantic.{item} windows disagree; the earlier window owns the first half of the overlap and the later window owns the second half.")))])});
                    }
                }
            }
        }
    }
    let mut records: Vec<Record> = Vec::new();
    for (index, unit) in normalized.into_iter().enumerate() {
        let owner = TimeRange::new(cuts[index], cuts[index + 1])?;
        for mut record in unit {
            if let Some(range) = record.range()?.intersection(owner) {
                record.start_us = range.start_us;
                record.end_us = range.end_us;
                if let Some(previous) = records.last_mut()
                    && previous.end_us == record.start_us
                    && equivalent(previous, &record)
                {
                    previous.end_us = record.end_us;
                    previous.confidence = previous
                        .confidence
                        .zip(record.confidence)
                        .map(|(a, b)| a.min(b));
                } else {
                    records.push(record);
                }
            }
        }
    }
    records.sort_by_key(|r| (r.start_us, r.end_us));
    for (index, record) in records.iter_mut().enumerate() {
        record.id = format!("{item}-{index}");
        if item == "subtask" {
            record.fields.insert("index".into(), json!(index));
        }
    }
    let annotation = AnnotationFile {
        header: header.clone(),
        records,
    };
    annotation.validate_in_range(coverage, ontology)?;
    Ok(Product {
        annotation,
        conflicts,
    })
}
#[allow(clippy::too_many_arguments)]
pub async fn run(
    episode: &Episode,
    stream: &str,
    item: &str,
    sidecar: &Path,
    workspace: &Path,
    provider: &Provider,
    options: &Options,
    events: &mut dyn EventSink,
) -> Result<Product> {
    interrupted(provider)?;
    ensure!(
        options.window_us > 5_000_000 && options.window_us <= 300_000_000,
        "semantic window must be in (5s,5m]"
    );
    ensure!(
        options.fps.is_finite() && options.fps > 0. && options.fps <= 10.,
        "invalid semantic FPS"
    );
    let common = include_str!("../../prompts/semantic-common.md");
    let instruction = prompt(item)?;
    let name = format!("semantic.{item}");
    let ontology = (item == "event").then_some(&options.ontology);
    let ontology = ontology.and_then(|value| value.as_ref());
    let params = json!({"stream_coverage_recipe":1,"contact_recipe":media::contact_proxy::RECIPE_VERSION,"window_us":options.window_us,"overlap_us":5_000_000,"fps":options.fps,"ontology":ontology,"prompt_hash":storage::cache_key(&(common,instruction,1))?,"kind":provider.endpoint.kind,"base_url":provider.endpoint.base_url,"model":provider.endpoint.model});
    let key = station_key(episode, stream, &name, &params)?;
    let directory = stream_directory(sidecar, stream, &episode.time.reference);
    let path = directory.join(format!("{name}.jsonl"));
    let product_path = directory.join(format!("{name}.conflicts.json"));
    let coverage = episode
        .video_coverage(stream)?
        .ok_or_else(|| anyhow::anyhow!("no stream coverage in episode"))?;
    if !options.recompute && path.is_file() && product_path.is_file() {
        let annotation = AnnotationFile::read(&path)?;
        let saved: Product = serde_json::from_slice(&fs::read(&product_path)?)?;
        if annotation.header.input_hash == key
            && saved.annotation.header.input_hash == key
            && storage::cache_key(&annotation)? == storage::cache_key(&saved.annotation)?
        {
            annotation.validate_in_range(coverage, ontology)?;
            return Ok(Product {
                annotation,
                conflicts: saved.conflicts,
            });
        }
    }
    let header = Header {
        schema: "annotation/1".into(),
        name: name.clone(),
        episode: episode.episode_id.clone(),
        stream: stream.into(),
        model: Model {
            kind: provider.endpoint.kind.clone(),
            name: provider.endpoint.model.clone(),
            base_url: Some(provider.endpoint.base_url.clone()),
        },
        params,
        created: chrono::Utc::now().to_rfc3339(),
        cerul_version: env!("CARGO_PKG_VERSION").into(),
        input_hash: key.clone(),
        record_schema: format!("{name}/1"),
    };
    let Stream::Video { path: source, .. } = episode.video(stream)? else {
        unreachable!()
    };
    let pts = media::frame_pts(&episode.source.root.join(source))?
        .into_iter()
        .map(|t| episode.source_to_episode(stream, t))
        .collect::<Result<Vec<_>>>()?;
    let windows = media::chunks(
        coverage.end_us - coverage.start_us,
        options.window_us,
        5_000_000,
    )?
    .into_iter()
    .map(|window| TimeRange::from_clip(coverage.start_us, window))
    .collect::<Result<Vec<_>>>()?;
    let checkpoints = Checkpoints::new(sidecar);
    let mut units = Vec::new();
    for (index, window) in windows.iter().enumerate() {
        interrupted(provider)?;
        let unit_key = storage::cache_key(&(&key, window))?;
        if !options.recompute
            && let Some(unit) = checkpoints.load::<Unit>(&unit_key)?
            && unit.window == *window
            && normalize(item, &unit, &header, &pts, ontology).is_ok()
        {
            units.push(unit);
            continue;
        }
        probes::check(provider, probes::Capability::Vision, workspace, false).await?;
        let sheet = contact::build_cached(episode, stream, *window, options.fps, workspace)?;
        let input = Input::Image(sheet.jpeg, "image/jpeg".into());
        let description = if item == "subtask" {
            let description_key = storage::cache_key(&(&unit_key, "description"))?;
            if !options.recompute
                && let Some(text) = checkpoints.load::<String>(&description_key)?
            {
                text
            } else {
                let value=provider.generate(&format!("{common}\nDescribe the sequence of visible actions in this window before segmenting it. Return an English description."),std::slice::from_ref(&input),json!({"type":"object","properties":{"description":{"type":"string"}},"required":["description"],"additionalProperties":false})).await?;
                let text = value["description"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| anyhow::anyhow!("missing subtask description"))?
                    .to_owned();
                checkpoints.save(&description_key, &text)?;
                text
            }
        } else {
            String::new()
        };
        let request = format!(
            "{common}\n{instruction}\nwindow_duration_us: {}\nallowed_verbs: {}\nfirst_pass_description: {}",
            window.end_us - window.start_us,
            serde_json::to_string(&ontology)?,
            serde_json::to_string(&description)?
        );
        let response = provider
            .generate(&request, &[input], schema::response_schema(item)?)
            .await?;
        let unit = Unit {
            window: *window,
            records: schema::records(item, response)?,
        };
        normalize(item, &unit, &header, &pts, ontology)?;
        checkpoints.save(&unit_key, &unit)?;
        units.push(unit);
        events.emit(Event::Progress {
            episode: episode.episode_id.clone(),
            station: name.clone(),
            done: index as u64 + 1,
            total: windows.len() as u64,
        });
    }
    let product = reconcile(item, &units, &header, &pts, coverage, ontology)?;
    // Keep the conflict provenance recoverable if publication is interrupted.
    storage::write_json(&product_path, &product)?;
    product
        .annotation
        .publish_in_range(&path, coverage, ontology)?;
    Ok(product)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(value: serde_json::Value) -> serde_json::Value {
        json!({"candidates":[{"content":{"parts":[{"text":value.to_string()}]}}]})
    }
    fn subtask(text: &str, start: i64) -> serde_json::Value {
        json!({"records":[{"start_us":start,"end_us":6000000,"confidence":0.8,"text":text,"index":0}]})
    }
    #[tokio::test]
    async fn short_offset_camera_annotates_only_its_coverage_and_roundtrips_records() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=2:duration=7",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let mut episode = crate::index::discover::ordinary_episode(&source).unwrap();
        let mut camera = episode.streams[0].clone();
        if let Stream::Video {
            id,
            primary,
            range_us,
            ..
        } = &mut camera
        {
            *id = "short".into();
            *primary = false;
            *range_us = [1_000_000, 3_000_000];
        }
        episode.streams.push(camera);
        episode.time.mappings.insert(
            "short".into(),
            crate::episode::TimeMapping {
                a: 1.,
                b_us: 2_000_000,
                status: crate::episode::MappingStatus::Calibrated,
            },
        );
        let coverage = TimeRange::new(2_000_000, 4_000_000).unwrap();
        assert_eq!(episode.video_coverage("short").unwrap(), Some(coverage));
        let sheet =
            contact::build(&episode, "short", TimeRange::new(0, 7_000_000).unwrap(), 2.).unwrap();
        assert_eq!(
            sheet.frame_times_us,
            vec![2_000_000, 2_500_000, 3_000_000, 3_500_000]
        );
        assert!(
            contact::build(
                &episode,
                "short",
                TimeRange::new(4_000_000, 7_000_000).unwrap(),
                2.
            )
            .is_err()
        );
        let (base, server) = crate::providers::tests::server(vec![
            (200, response(json!({"ok":true}))),
            (200, response(json!({"description":"Hold the cup."}))),
            (
                200,
                response(
                    json!({"records":[{"start_us":0,"end_us":2000000,"confidence":0.8,"text":"Hold the cup","index":0}]}),
                ),
            ),
        ]);
        let workspace = dir.path().join("workspace");
        let sidecar = crate::index::discover::publish_episode(&workspace, &episode, None).unwrap();
        let mut endpoint = crate::config::Config::default().vision;
        endpoint.base_url = base;
        let provider = Provider::new(
            endpoint,
            None,
            1,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        let product = run(
            &episode,
            "short",
            "subtask",
            &sidecar,
            &workspace,
            &provider,
            &Options::default(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(product.annotation.records[0].range().unwrap(), coverage);
        product
            .annotation
            .validate_in_range(coverage, None)
            .unwrap();
        assert_eq!(
            crate::index::records::sidecars(&workspace).unwrap().len(),
            1
        );
        assert_eq!(server.join().unwrap().len(), 3);
        assert!(
            crate::status::inspect(&workspace, None).unwrap().episodes[0]
                .annotations
                .contains(&"short/semantic.subtask".into())
        );
        let config = crate::config::Config::default();
        let report = crate::search::run(
            &workspace,
            &config,
            &crate::search::Options {
                filters: vec!["stream=short".into()],
                save: Some(dir.path().join("clips")),
                ..Default::default()
            },
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(report.hits.len(), 1);
        assert_eq!(
            (report.hits[0].start_us, report.hits[0].end_us),
            (2_000_000, 4_000_000)
        );
        assert_eq!(
            media::probe(report.hits[0].clip.as_ref().unwrap())
                .unwrap()
                .duration_us,
            2_000_000
        );
        let annotation_path =
            stream_directory(&sidecar, "short", "primary").join("semantic.subtask.jsonl");
        let old_bytes = fs::read(&annotation_path).unwrap();
        for change in ["range", "mapping"] {
            let mut changed = episode.clone();
            if change == "mapping" {
                changed.time.mappings.get_mut("short").unwrap().b_us += 1;
            } else if let Stream::Video { range_us, .. } = &mut changed.streams[1] {
                range_us[1] -= 1;
            }
            assert!(
                !crate::index::stations::has_current_input(&changed, &product.annotation).unwrap()
            );
        }
        media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=blue:size=64x64:rate=2:duration=7",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let new_hash = media::sha256(&source).unwrap();
        for stream in &mut episode.streams {
            if let Stream::Video { sha256, .. } = stream {
                *sha256 = new_hash.clone();
            }
        }
        storage::write_json(&sidecar.join("episode.json"), &episode).unwrap();
        assert!(
            crate::index::records::sidecars(&workspace)
                .unwrap()
                .is_empty()
        );
        assert!(
            crate::status::inspect(&workspace, None).unwrap().episodes[0]
                .annotations
                .is_empty()
        );
        let index =
            crate::index::records::RecordIndex::rebuild(&workspace, &config.space_id().unwrap())
                .await
                .unwrap();
        assert!(index.read(None).await.unwrap().is_empty());
        drop(index);
        let error = crate::search::run(
            &workspace,
            &config,
            &crate::search::Options {
                filters: vec!["stream=short".into(), "semantic.subtask.index=0".into()],
                ..Default::default()
            },
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("has not been generated"));
        assert_eq!(fs::read(&annotation_path).unwrap(), old_bytes);
        episode.time.mappings.get_mut("short").unwrap().b_us = 10_000_000;
        let error = run(
            &episode,
            "short",
            "subtask",
            &sidecar,
            &workspace,
            &provider,
            &Options::default(),
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("no stream coverage"));
    }
    #[tokio::test]
    async fn subtask_resumes_failed_window_and_preserves_whole_episode_coverage() {
        let (base, server) = crate::providers::tests::server(vec![
            (200, response(json!({"ok":true}))),
            (
                200,
                response(json!({"description":"Reach toward the cup."})),
            ),
            (200, response(subtask("Reach for the cup", 0))),
            (200, response(json!({"description":"Hold the cup."}))),
            (200, response(subtask("Hold the cup", 1))),
            (200, response(subtask("Hold the cup", 0))),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=2:duration=7",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let episode = crate::index::discover::ordinary_episode(&source).unwrap();
        let workspace = dir.path().join("workspace");
        let sidecar = crate::index::discover::publish_episode(&workspace, &episode, None).unwrap();
        let mut endpoint = crate::config::Config::default().vision;
        endpoint.base_url = base;
        let provider = Provider::new(
            endpoint,
            None,
            1,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        let options = Options {
            window_us: 6_000_000,
            ..Default::default()
        };
        let old = AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "semantic.subtask".into(),
                episode: episode.episode_id.clone(),
                stream: "primary".into(),
                model: Model {
                    kind: "fixture".into(),
                    name: "old".into(),
                    base_url: None,
                },
                params: json!({}),
                created: "2026-09-08T00:00:00Z".into(),
                cerul_version: "0.0.3".into(),
                input_hash: "old".into(),
                record_schema: "semantic.subtask/1".into(),
            },
            records: vec![Record {
                id: "old".into(),
                start_us: 0,
                end_us: 7_000_000,
                confidence: None,
                fields: BTreeMap::from([
                    ("text".into(), json!("Existing annotation")),
                    ("index".into(), json!(0)),
                ]),
            }],
        };
        let path = sidecar.join("semantic.subtask.jsonl");
        old.publish(&path, 7_000_000, None).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(
            run(
                &episode,
                "primary",
                "subtask",
                &sidecar,
                &workspace,
                &provider,
                &options,
                &mut |_| {}
            )
            .await
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        let product = run(
            &episode,
            "primary",
            "subtask",
            &sidecar,
            &workspace,
            &provider,
            &options,
            &mut |_| {},
        )
        .await
        .unwrap();
        product.annotation.validate(7_000_000, None).unwrap();
        assert_eq!(product.annotation.records.len(), 2);
        assert_eq!(product.annotation.records[0].end_us, 3_500_000);
        assert_eq!(product.annotation.records[1].start_us, 3_500_000);
        assert_eq!(product.annotation.records[1].end_us, 7_000_000);
        assert!(!product.conflicts.is_empty());
        super::super::pipeline::publish_conflicts(&episode, "primary", &sidecar).unwrap();
        let flag_path = sidecar.join("semantic.flag.jsonl");
        let flags = AnnotationFile::read(&flag_path).unwrap();
        assert_eq!(flags.records[0].fields["kind"], "window_conflict");
        let bytes = fs::read(&flag_path).unwrap();
        super::super::pipeline::publish_conflicts(&episode, "primary", &sidecar).unwrap();
        assert_eq!(fs::read(&flag_path).unwrap(), bytes);
        assert_eq!(server.join().unwrap().len(), 6);
        // Closed endpoint proves a completed module is reused without probing.
        let again = run(
            &episode,
            "primary",
            "subtask",
            &sidecar,
            &workspace,
            &provider,
            &options,
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            again.annotation.header.input_hash,
            product.annotation.header.input_hash
        );
        // Simulate interruption after the new product is published but before
        // its annotation replaces the previous generation (same input hash).
        let mut torn = product.clone();
        torn.annotation.records[0]
            .fields
            .insert("text".into(), json!("Different generation"));
        torn.conflicts.clear();
        storage::write_json(&sidecar.join("semantic.subtask.conflicts.json"), &torn).unwrap();
        super::super::pipeline::publish_conflicts(&episode, "primary", &sidecar).unwrap();
        assert_eq!(fs::read(&flag_path).unwrap(), bytes);
        let repaired = run(
            &episode,
            "primary",
            "subtask",
            &sidecar,
            &workspace,
            &provider,
            &options,
            &mut |_| {},
        )
        .await
        .unwrap();
        assert!(!repaired.conflicts.is_empty());
        let saved: Product = serde_json::from_slice(
            &fs::read(sidecar.join("semantic.subtask.conflicts.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            storage::cache_key(&saved.annotation).unwrap(),
            storage::cache_key(&AnnotationFile::read(&path).unwrap()).unwrap()
        );
    }
}
