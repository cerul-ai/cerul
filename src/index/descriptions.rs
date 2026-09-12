//! Independently published description vectors. Default retrieval is gated by
//! an evaluated fusion recipe; these files do not replace the base index.
use super::{
    stations,
    understanding::{self, SourceRef},
    vectors::{self, Kind, VectorRow},
};
use crate::{
    annotations::AnnotationFile,
    config::Config,
    episode::Episode,
    events::{Event, EventSink},
    providers::{Input, Provider, probes},
    storage::{self, Checkpoints},
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const RECIPE: &str = "description-vectors/1";
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct State {
    pub schema: String,
    pub space_id: String,
    pub input_hash: String,
    pub scene_revision: String,
    pub complete: bool,
    pub source_refs: BTreeMap<String, SourceRef>,
}
fn state_path(sidecar: &Path, episode: &Episode, stream: &str, space: &str) -> PathBuf {
    stations::stream_directory(sidecar, stream, &episode.time.reference)
        .join(format!("description.{space}.json"))
}
fn product_path(sidecar: &Path, state: &State) -> Result<PathBuf> {
    ensure!(
        [&state.input_hash, &state.space_id]
            .iter()
            .all(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())),
        "invalid description product identity"
    );
    Ok(sidecar
        .join("embeddings/descriptions")
        .join(&state.space_id)
        .join(format!("{}.parquet", state.input_hash)))
}
/// Read a complete generation only while its original scene records still match.
pub fn read_current(
    sidecar: &Path,
    episode: &Episode,
    stream: &str,
    space: &str,
    dims: usize,
) -> Result<Option<(State, Vec<VectorRow>)>> {
    let path = state_path(sidecar, episode, stream, space);
    if !path.is_file() {
        return Ok(None);
    }
    let state: State = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        state.schema == "description-index/1" && state.space_id == space,
        "invalid description state"
    );
    if !state.complete {
        return Ok(None);
    }
    let directory = stations::stream_directory(sidecar, stream, &episode.time.reference);
    let scene_path = directory.join("semantic.scene.jsonl");
    if !scene_path.is_file() {
        return Ok(None);
    }
    let scenes = AnnotationFile::read(&scene_path)?;
    if !stations::has_current_input(episode, &scenes)?
        || storage::cache_key(&scenes.records)? != state.scene_revision
    {
        return Ok(None);
    }
    let rows = vectors::read(&product_path(sidecar, &state)?, dims)?;
    ensure!(
        rows.len() == state.source_refs.len(),
        "description provenance count mismatch"
    );
    for row in &rows {
        ensure!(
            row.kind == Kind::Description
                && row.space_id == space
                && row.episode == episode.episode_id
                && row.stream == stream
                && row.params_hash == state.input_hash,
            "description vector identity mismatch"
        );
        let reference = state
            .source_refs
            .get(&row.id)
            .context("description source reference missing")?;
        let record = scenes
            .records
            .iter()
            .find(|r| r.id == reference.record_id)
            .context("description source record missing")?;
        ensure!(
            reference.annotation == "semantic.scene"
                && reference.revision == understanding::revision(record)?
                && row.start_us == record.start_us
                && row.end_us == record.end_us
                && record
                    .fields
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    == Some(row.text.as_str()),
            "description source revision mismatch"
        );
    }
    Ok(Some((state, rows)))
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    episode: &Episode,
    stream: &str,
    sidecar: &Path,
    workspace: &Path,
    config: &Config,
    provider: &Provider,
    scenes: &AnnotationFile,
    recompute: bool,
    events: &mut dyn EventSink,
) -> Result<usize> {
    ensure!(
        scenes.header.name == "semantic.scene"
            && scenes.header.stream == stream
            && stations::has_current_input(episode, scenes)?,
        "description inputs are stale or mismatched"
    );
    scenes.validate_in_range(
        episode
            .video_coverage(stream)?
            .context("missing visual coverage")?,
        None,
    )?;
    let space = config.space_id()?;
    let dims = config
        .embedding
        .dims
        .context("missing embedding dimensions")?;
    let scene_revision = storage::cache_key(&scenes.records)?;
    let input_hash = stations::station_key(
        episode,
        stream,
        "description",
        &json!({"recipe":RECIPE,"space":space,"scene_revision":scene_revision,"document_template":crate::config::DOCUMENT_TEMPLATE}),
    )?;
    if !recompute
        && let Some((state, rows)) = read_current(sidecar, episode, stream, &space, dims)?
        && state.input_hash == input_hash
    {
        return Ok(rows.len());
    }
    let path = state_path(sidecar, episode, stream, &space);
    let mut state = State {
        schema: "description-index/1".into(),
        space_id: space.clone(),
        input_hash: input_hash.clone(),
        scene_revision,
        complete: false,
        source_refs: BTreeMap::new(),
    };
    // Keep the published manifest until every replacement row is ready. A
    // failed recompute must not withdraw a still-current description product.
    let checkpoints = Checkpoints::new(sidecar);
    let mut rows = Vec::new();
    events.emit(Event::Progress {
        episode: episode.episode_id.clone(),
        station: "description".into(),
        done: 0,
        total: scenes.records.len() as u64,
    });
    for (i, record) in scenes.records.iter().enumerate() {
        crate::media::check_cancellation()?;
        let text = record
            .fields
            .get("description")
            .and_then(serde_json::Value::as_str)
            .context("scene has no description")?;
        let key = stations::station_key(
            episode,
            stream,
            "description-row",
            &json!({"space":space,"recipe":RECIPE,"document_template":crate::config::DOCUMENT_TEMPLATE,"start_us":record.start_us,"end_us":record.end_us,"text":text}),
        )?;
        let cached = if recompute {
            None
        } else {
            checkpoints.load::<VectorRow>(&key)?
        };
        let mut row = if let Some(row) = cached {
            row
        } else {
            probes::check(provider, probes::Capability::Embedding, workspace, false).await?;
            let vector = provider.embed(Input::Text(text.into()), false).await?;
            VectorRow {
                id: key.clone(),
                episode: episode.episode_id.clone(),
                stream: stream.into(),
                kind: Kind::Description,
                start_us: record.start_us,
                end_us: record.end_us,
                vector,
                text: text.into(),
                still: false,
                space_id: space.clone(),
                params_hash: input_hash.clone(),
            }
        };
        row.validate(dims)?;
        ensure!(
            row.id == key
                && row.kind == Kind::Description
                && row.start_us == record.start_us
                && row.end_us == record.end_us
                && row.text == text
                && row.space_id == space
                && row.episode == episode.episode_id
                && row.stream == stream,
            "cached description identity mismatch"
        );
        row.params_hash = input_hash.clone();
        checkpoints.save(&key, &row)?;
        state.source_refs.insert(
            row.id.clone(),
            SourceRef {
                annotation: "semantic.scene".into(),
                record_id: record.id.clone(),
                revision: understanding::revision(record)?,
            },
        );
        rows.push(row);
        events.emit(Event::Progress {
            episode: episode.episode_id.clone(),
            station: "description".into(),
            done: (i + 1) as u64,
            total: scenes.records.len() as u64,
        });
    }
    vectors::write(&product_path(sidecar, &state)?, &rows, dims)?;
    state.complete = true;
    storage::write_json(&path, &state)?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        annotations::{Header, Model, Record},
        episode::TimeRange,
        index::{
            discover,
            understanding::{ContentKind, Scene, VisualEvidence},
        },
    };
    #[tokio::test]
    async fn description_updates_reembed_only_changed_text_and_keep_the_original_interval() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("red.mp4");
        crate::media::run(
            crate::media::command("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=red:size=64x64:duration=2",
                    "-c:v",
                    "libx264",
                ])
                .arg(&video),
        )
        .unwrap();
        let episode = discover::ordinary_episode(&video).unwrap();
        let workspace = dir.path().join("workspace");
        let sidecar = discover::publish_episode(&workspace, &episode, None).unwrap();
        let mut responses = vec![(200, json!({"embedding":{"values":[1.,0.]}})); 5];
        responses.push((400, json!({"error":"replacement rejected"})));
        let (base, server) = crate::providers::tests::server(responses);
        let mut config = Config::default();
        config.embedding.base_url = base;
        config.embedding.dims = Some(2);
        let provider = Provider::new(
            config.embedding.clone(),
            None,
            1,
            None,
            tokio_util::sync::CancellationToken::new(),
        )
        .unwrap();
        let scene = Scene {
            description: "A red screen.".into(),
            objects: vec![],
            actions: vec![],
            kind: ContentKind::Static,
            evidence: VisualEvidence {
                input_kind: "video".into(),
                input_start_us: 0,
                input_end_us: 2_000_000,
                sampling_fps: 1,
                sample_us: vec![0, 1_000_000],
                max_edge: 480,
                media_sha256: "a".repeat(64),
                input_sha256: "b".repeat(64),
                recipe: understanding::RECIPE.into(),
            },
            revision: "c".repeat(64),
            correction: None,
        };
        let params = json!({"fixture":true});
        let mut scenes = AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: "semantic.scene".into(),
                episode: episode.episode_id.clone(),
                stream: "primary".into(),
                model: Model {
                    kind: "fixture".into(),
                    name: "synthetic".into(),
                    base_url: None,
                },
                params: params.clone(),
                created: "now".into(),
                cerul_version: "test".into(),
                input_hash: stations::station_key(&episode, "primary", "semantic.scene", &params)
                    .unwrap(),
                record_schema: "semantic.scene/1".into(),
            },
            records: vec![Record {
                id: "scene-1".into(),
                start_us: 0,
                end_us: 2_000_000,
                confidence: None,
                fields: serde_json::from_value(json!(scene)).unwrap(),
            }],
        };
        let scene_path = sidecar.join("semantic.scene.jsonl");
        scenes
            .publish_in_range(&scene_path, TimeRange::new(0, 2_000_000).unwrap(), None)
            .unwrap();
        run(
            &episode,
            "primary",
            &sidecar,
            &workspace,
            &config,
            &provider,
            &scenes,
            false,
            &mut |_| {},
        )
        .await
        .unwrap();
        let space = config.space_id().unwrap();
        let (first, rows) = read_current(&sidecar, &episode, "primary", &space, 2)
            .unwrap()
            .unwrap();
        assert_eq!((rows[0].start_us, rows[0].end_us), (0, 2_000_000));
        assert!(
            !sidecar
                .join("embeddings")
                .join(format!("{space}.parquet"))
                .exists()
        );
        scenes.records[0]
            .fields
            .insert("objects".into(), json!(["screen"]));
        scenes
            .publish_in_range(&scene_path, TimeRange::new(0, 2_000_000).unwrap(), None)
            .unwrap();
        assert!(
            read_current(&sidecar, &episode, "primary", &space, 2)
                .unwrap()
                .is_none()
        );
        run(
            &episode,
            "primary",
            &sidecar,
            &workspace,
            &config,
            &provider,
            &scenes,
            false,
            &mut |_| {},
        )
        .await
        .unwrap();
        let (changed, same) = read_current(&sidecar, &episode, "primary", &space, 2)
            .unwrap()
            .unwrap();
        assert_ne!(first.scene_revision, changed.scene_revision);
        assert_eq!(rows[0].id, same[0].id);
        assert_eq!(rows[0].vector, same[0].vector);
        scenes.records[0]
            .fields
            .insert("description".into(), json!("A red color fills the screen."));
        scenes
            .publish_in_range(&scene_path, TimeRange::new(0, 2_000_000).unwrap(), None)
            .unwrap();
        run(
            &episode,
            "primary",
            &sidecar,
            &workspace,
            &config,
            &provider,
            &scenes,
            false,
            &mut |_| {},
        )
        .await
        .unwrap();
        let manifest_path = state_path(&sidecar, &episode, "primary", &space);
        let before_manifest = fs::read(&manifest_path).unwrap();
        let before_rows = read_current(&sidecar, &episode, "primary", &space, 2)
            .unwrap()
            .unwrap()
            .1;
        assert!(
            run(
                &episode,
                "primary",
                &sidecar,
                &workspace,
                &config,
                &provider,
                &scenes,
                true,
                &mut |_| {}
            )
            .await
            .is_err()
        );
        assert_eq!(fs::read(&manifest_path).unwrap(), before_manifest);
        assert_eq!(
            serde_json::to_value(
                read_current(&sidecar, &episode, "primary", &space, 2)
                    .unwrap()
                    .unwrap()
                    .1
            )
            .unwrap(),
            serde_json::to_value(before_rows).unwrap()
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 6);
        assert!(
            requests[4]
                .1
                .to_string()
                .contains("title: none | text: A red color fills the screen.")
        );
        run(
            &episode,
            "primary",
            &sidecar,
            &workspace,
            &config,
            &provider,
            &scenes,
            false,
            &mut |_| {},
        )
        .await
        .unwrap();
    }
}
