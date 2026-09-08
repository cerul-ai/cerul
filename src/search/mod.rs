pub mod filter;

use crate::{
    annotations::AnnotationFile,
    config::Config,
    episode::{Episode, Stream, TimeRange},
    index::{
        discover, lance,
        records::{self, RecordIndex, Row},
        vectors::Kind,
    },
    media,
    providers::{Failure, Input, Provider, ProviderError, RequestNotice, probes},
    storage,
};
use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Options {
    pub query: Option<String>,
    pub image: Option<PathBuf>,
    pub text: bool,
    pub filters: Vec<String>,
    pub within: Option<PathBuf>,
    pub limit: usize,
    pub threshold: Option<f32>,
    pub count: bool,
    pub save: Option<PathBuf>,
    pub pad_us: i64,
    pub dry_run: bool,
    pub request_notice: Option<RequestNotice>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            query: None,
            image: None,
            text: false,
            filters: Vec::new(),
            within: None,
            limit: 10,
            threshold: None,
            count: false,
            save: None,
            pad_us: 2_000_000,
            dry_run: false,
            request_notice: None,
        }
    }
}
impl Options {
    pub fn validate(&self) -> Result<Vec<filter::Filter>> {
        ensure!(
            self.query.as_ref().is_none_or(|q| !q.trim().is_empty()),
            "query cannot be empty"
        );
        ensure!(
            self.query.is_some() || self.image.is_some() || !self.filters.is_empty(),
            "query, image, or filter is required"
        );
        ensure!(
            !(self.query.is_some() && self.image.is_some()),
            "choose a text query or an image query"
        );
        ensure!(
            !self.text || (self.query.is_some() && self.image.is_none()),
            "--text requires a text query"
        );
        ensure!(
            self.limit > 0 && self.pad_us >= 0,
            "limit must be positive and pad nonnegative"
        );
        ensure!(
            self.threshold.is_none_or(|n| n.is_finite()),
            "threshold must be finite"
        );
        let filters: Vec<_> = self
            .filters
            .iter()
            .map(|f| filter::Filter::parse(f))
            .collect::<Result<_>>()?;
        ensure!(
            !self.count
                || (self.query.is_none()
                    && self.image.is_none()
                    && self.save.is_none()
                    && filters.iter().any(|f| f.annotation().is_some())),
            "--count requires annotation filters without a query, image, or save"
        );
        ensure!(
            self.threshold.is_none()
                || (!self.text && (self.query.is_some() || self.image.is_some())),
            "threshold requires vector search"
        );
        ensure!(
            self.query.is_some()
                || self.image.is_some()
                || !filters.iter().any(|f| f.key == "kind"),
            "kind requires vector or text search"
        );
        Ok(filters)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Hit {
    pub episode: String,
    pub stream: String,
    pub start_us: i64,
    pub end_us: i64,
    pub frame_range: Option<[u64; 2]>,
    pub score: Option<f32>,
    pub matched: Option<Kind>,
    pub excerpt: String,
    pub annotations: Vec<Row>,
    pub clip: Option<PathBuf>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Counts {
    pub records: usize,
    pub episodes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Report {
    pub hits: Vec<Hit>,
    pub counts: Option<Counts>,
    pub dry_run: bool,
}
fn unavailable(message: impl Into<String>) -> anyhow::Error {
    ProviderError {
        kind: Failure::Unsupported,
        message: message.into(),
    }
    .into()
}
fn cancelled(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(ProviderError {
            kind: Failure::Cancelled,
            message: "operation cancelled".into(),
        }
        .into());
    }
    Ok(())
}
fn kind_allowed(filters: &[filter::Filter], kind: Kind) -> bool {
    filters
        .iter()
        .filter(|f| f.key == "kind")
        .all(|f| f.matches(Some(&Value::String(kind.as_str().into()))))
}
fn annotations(rows: &[Row], episode: &str, stream: &str, range: TimeRange) -> Vec<Row> {
    rows.iter()
        .filter(|r| {
            r.episode == episode
                && r.stream == stream
                && r.record.start_us < range.end_us
                && r.record.end_us > range.start_us
        })
        .cloned()
        .collect()
}
fn hit(episode: &str, stream: &str, range: TimeRange, rows: &[Row]) -> Hit {
    Hit {
        episode: episode.into(),
        stream: stream.into(),
        start_us: range.start_us,
        end_us: range.end_us,
        frame_range: None,
        score: None,
        matched: None,
        excerpt: String::new(),
        annotations: annotations(rows, episode, stream, range),
        clip: None,
    }
}
fn merge_hits(mut hits: Vec<Hit>, vector: bool) -> Vec<Hit> {
    hits.sort_by(|a, b| {
        (&a.episode, &a.stream, a.start_us, a.end_us)
            .cmp(&(&b.episode, &b.stream, b.start_us, b.end_us))
    });
    let mut merged: Vec<Hit> = Vec::new();
    for mut next in hits {
        if let Some(previous) = merged.last_mut()
            && previous.episode == next.episode
            && previous.stream == next.stream
            && next.start_us <= previous.end_us
        {
            previous.end_us = previous.end_us.max(next.end_us);
            if next.score > previous.score {
                previous.score = next.score;
                previous.matched = next.matched;
                previous.excerpt = next.excerpt.clone();
            }
            if !vector && !next.excerpt.is_empty() && !previous.excerpt.contains(&next.excerpt) {
                if !previous.excerpt.is_empty() {
                    previous.excerpt.push('\n');
                }
                previous.excerpt.push_str(&next.excerpt);
            }
            previous.annotations.append(&mut next.annotations);
            previous
                .annotations
                .sort_by(|a, b| (&a.annotation, &a.record.id).cmp(&(&b.annotation, &b.record.id)));
            previous
                .annotations
                .dedup_by(|a, b| a.annotation == b.annotation && a.record.id == b.record.id);
        } else {
            merged.push(next);
        }
    }
    if vector {
        merged.sort_by(|a, b| {
            b.score
                .unwrap_or(f32::NEG_INFINITY)
                .total_cmp(&a.score.unwrap_or(f32::NEG_INFINITY))
                .then_with(|| {
                    (&a.episode, &a.stream, a.start_us).cmp(&(&b.episode, &b.stream, b.start_us))
                })
        });
    }
    merged
}

pub async fn run(
    workspace: &Path,
    config: &Config,
    options: &Options,
    cancel: CancellationToken,
) -> Result<Report> {
    crate::media::with_cancellation(
        cancel.clone(),
        run_inner(workspace, config, options, cancel),
    )
    .await
}

async fn run_inner(
    workspace: &Path,
    config: &Config,
    options: &Options,
    cancel: CancellationToken,
) -> Result<Report> {
    let filters = options.validate()?;
    config.validate()?;
    cancelled(&cancel)?;
    if options.dry_run {
        return Ok(Report {
            hits: Vec::new(),
            counts: None,
            dry_run: true,
        });
    }
    if options.save.is_some() {
        media::check_dependencies().map_err(|error| unavailable(error.to_string()))?;
    }
    let vector = !options.text && (options.query.is_some() || options.image.is_some());
    let space = config.space_id()?;
    let selection = options.within.as_ref().map(fs::canonicalize).transpose()?;
    let _lock = storage::WorkspaceLock::acquire(workspace)?;
    let registry = discover::read_registry(workspace)?;
    let mut episodes = BTreeMap::new();
    for entry in &registry {
        if selection
            .as_ref()
            .is_some_and(|p| !entry.media.starts_with(p) && !entry.sidecar.starts_with(p))
        {
            continue;
        }
        let episode: Episode =
            serde_json::from_slice(&fs::read(entry.sidecar.join("episode.json"))?)?;
        episode.validate()?;
        episodes.insert(episode.episode_id.clone(), episode);
    }
    if vector {
        let dims = config
            .embedding
            .dims
            .context("embedding dimensions missing")?;
        let mut available = false;
        for entry in registry
            .iter()
            .filter(|entry| episodes.contains_key(&entry.episode_id))
        {
            let path = entry
                .sidecar
                .join("embeddings")
                .join(format!("{space}.parquet"));
            if !path.is_file() {
                continue;
            }
            let episode = &episodes[&entry.episode_id];
            for row in crate::index::vectors::read(&path, dims)? {
                if row.episode == entry.episode_id
                    && row.space_id == space
                    && kind_allowed(&filters, row.kind)
                    && filters.iter().all(|filter| match filter.key.as_str() {
                        "episode" => filter.matches(Some(&Value::String(row.episode.clone()))),
                        "stream" => filter.matches(Some(&Value::String(row.stream.clone()))),
                        _ => true,
                    })
                    && episode.video(&row.stream).is_ok()
                    && crate::index::embed::usable(
                        &entry.sidecar,
                        &row.stream,
                        &episode.time.reference,
                        &space,
                    )?
                {
                    available = true;
                    break;
                }
            }
            if available {
                break;
            }
        }
        if !available {
            return Err(unavailable(
                "no usable saved vectors in the selected scope match the configured embedding space; run index with this model",
            ));
        }
    }
    let record_index = RecordIndex::rebuild(workspace, &space).await?;
    let rows = record_index.read(None).await?;
    let mut files: Vec<AnnotationFile> = records::sidecars(workspace)?;
    for file in &mut files {
        file.records = rows
            .iter()
            .filter(|row| {
                row.episode == file.header.episode
                    && row.stream == file.header.stream
                    && row.annotation == file.header.name
            })
            .map(|row| row.record.clone())
            .collect();
    }
    let mut scopes = BTreeMap::new();
    for (id, episode) in &episodes {
        for stream in &episode.streams {
            if !matches!(stream, Stream::Video { .. }) {
                continue;
            }
            let Some(coverage) = episode.video_coverage(stream.id())? else {
                continue;
            };
            let intervals =
                filter::intervals(&files, &filters, id, stream.id(), episode.duration_us()?)
                    .map_err(|e| unavailable(e.to_string()))?
                    .into_iter()
                    .filter_map(|mut matched| {
                        matched.range = matched.range.intersection(coverage)?;
                        Some(matched)
                    })
                    .collect::<Vec<_>>();
            if !intervals.is_empty() {
                scopes.insert((id.clone(), stream.id().to_owned()), intervals);
            }
        }
    }
    let mut hits = Vec::new();
    let mut counted = BTreeSet::new();
    if vector && !scopes.is_empty() {
        let dims = config
            .embedding
            .dims
            .context("embedding dimensions missing")?;
        let index = lance::rebuild(workspace, &space, dims).await?;
        let allowed: Vec<_> = [Kind::Video, Kind::Speech, Kind::Screen]
            .into_iter()
            .filter(|kind| kind_allowed(&filters, *kind))
            .collect();
        if !allowed.is_empty() {
            let predicate = format!(
                "({}) AND kind IN ({})",
                scopes
                    .iter()
                    .map(|((episode, stream), ranges)| filter::sql_predicate(
                        episode, stream, ranges
                    ))
                    .collect::<Vec<_>>()
                    .join(" OR "),
                allowed
                    .iter()
                    .map(|kind| lance::literal(kind.as_str()))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            if index.count().await? > 0 {
                let mut provider =
                    Provider::from_env(config.embedding.clone(), 1, None, cancel.clone())?;
                provider.request_notice = options.request_notice.clone();
                probes::check(&provider, probes::Capability::Embedding, workspace, false).await?;
                let input = if let Some(path) = &options.image {
                    let bytes = fs::read(path)?;
                    let mime = match image::guess_format(&bytes)? {
                        image::ImageFormat::Png => "image/png",
                        image::ImageFormat::Jpeg => "image/jpeg",
                        image::ImageFormat::WebP => "image/webp",
                        _ => return Err(unavailable("query image must be PNG, JPEG, or WebP")),
                    };
                    Input::Image(bytes, mime.into())
                } else {
                    Input::Text(options.query.clone().unwrap())
                };
                let query = provider.embed(input, true).await?;
                let total = index.count().await?;
                let mut budget = options.limit.saturating_mul(3).max(32).min(total);
                loop {
                    cancelled(&cancel)?;
                    let candidates = index.search(&query, Some(&predicate), budget).await?;
                    let exhausted = candidates.len() < budget || budget == total;
                    let below_threshold = candidates.last().is_some_and(|(_, score)| {
                        options
                            .threshold
                            .is_some_and(|threshold| *score < threshold)
                    });
                    hits.clear();
                    let mut unique = BTreeSet::new();
                    for (row, score) in candidates {
                        if options.threshold.is_some_and(|threshold| score < threshold) {
                            continue;
                        }
                        for scope in &scopes[&(row.episode.clone(), row.stream.clone())] {
                            if let Some(range) = scope
                                .range
                                .intersection(TimeRange::new(row.start_us, row.end_us)?)
                            {
                                unique.insert((
                                    row.episode.clone(),
                                    row.stream.clone(),
                                    range.start_us,
                                    range.end_us,
                                ));
                                let mut found = hit(&row.episode, &row.stream, range, &rows);
                                found.score = Some(score);
                                found.matched = Some(row.kind);
                                found.excerpt = row.text.clone();
                                hits.push(found);
                            }
                        }
                    }
                    if unique.len() >= options.limit || exhausted || below_threshold {
                        break;
                    }
                    budget = budget.saturating_mul(2).min(total);
                }
            }
        }
    } else if !vector && options.text {
        let query = options.query.as_ref().unwrap();
        for row in &rows {
            let kind = match row.annotation.as_str() {
                "transcript" => Kind::Speech,
                "screen_text" => Kind::Screen,
                _ => continue,
            };
            if !kind_allowed(&filters, kind) {
                continue;
            }
            let Some(text) = row
                .record
                .fields
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| text.contains(query))
            else {
                continue;
            };
            let Some(ranges) = scopes.get(&(row.episode.clone(), row.stream.clone())) else {
                continue;
            };
            for scope in ranges {
                if let Some(range) = scope.range.intersection(row.record.range()?) {
                    let mut found = hit(&row.episode, &row.stream, range, &rows);
                    found.excerpt = text.into();
                    found.matched = Some(kind);
                    hits.push(found);
                }
            }
        }
    } else if !vector {
        for ((episode, stream), ranges) in &scopes {
            for scope in ranges {
                for (annotation, id) in &scope.records {
                    counted.insert((
                        episode.clone(),
                        stream.clone(),
                        annotation.clone(),
                        id.clone(),
                    ));
                }
                let mut found = hit(episode, stream, scope.range, &rows);
                if !scope.records.is_empty() {
                    found.annotations.retain(|row| {
                        scope
                            .records
                            .contains(&(row.annotation.clone(), row.record.id.clone()))
                    });
                }
                hits.push(found);
            }
        }
    }
    if options.count {
        return Ok(Report {
            hits: Vec::new(),
            counts: Some(Counts {
                records: counted.len(),
                episodes: counted.iter().map(|r| &r.0).collect::<BTreeSet<_>>().len(),
            }),
            dry_run: false,
        });
    }
    if vector {
        let mut intervals: BTreeMap<(String, String, i64, i64), Hit> = BTreeMap::new();
        for found in hits {
            let key = (
                found.episode.clone(),
                found.stream.clone(),
                found.start_us,
                found.end_us,
            );
            match intervals.get_mut(&key) {
                Some(old) if found.score > old.score => *old = found,
                None => {
                    intervals.insert(key, found);
                }
                _ => {}
            }
        }
        hits = intervals.into_values().collect();
        hits.sort_by(|a, b| b.score.unwrap().total_cmp(&a.score.unwrap()));
        // Choose the top unique intervals before joining their adjacent windows.
        hits.truncate(options.limit);
    }
    let mut hits = merge_hits(hits, vector);
    hits.truncate(options.limit);
    if let Some(directory) = &options.save {
        for found in &mut hits {
            cancelled(&cancel)?;
            let episode = &episodes[&found.episode];
            let Stream::Video { path, .. } = episode.video(&found.stream)? else {
                unreachable!()
            };
            let coverage = episode
                .video_coverage(&found.stream)?
                .context("hit stream has no episode coverage")?;
            let range = TimeRange::new(
                found.start_us.saturating_sub(options.pad_us).max(0),
                found.end_us.saturating_add(options.pad_us),
            )?
            .intersection(coverage)
            .context("saved clip has no stream coverage")?;
            let name =
                storage::cache_key(&(&found.episode, &found.stream, range.start_us, range.end_us))?;
            let destination = directory.join(format!("{name}.mp4"));
            media::extract::video(
                &episode.source.root.join(path),
                media::extract::SourceRange::new(
                    episode.episode_to_source(&found.stream, range.start_us)?,
                    episode.episode_to_source(&found.stream, range.end_us)?,
                )?,
                &destination,
                false,
            )?;
            found.clip = Some(destination);
        }
    }
    Ok(Report {
        hits,
        counts: None,
        dry_run: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        annotations::{Header, Model, Record},
        index::vectors::{self, VectorRow},
    };
    use serde_json::json;
    fn file(episode: &Episode, name: &str, fields: Value) -> AnnotationFile {
        AnnotationFile {
            header: Header {
                schema: "annotation/1".into(),
                name: name.into(),
                episode: episode.episode_id.clone(),
                stream: "primary".into(),
                model: Model {
                    kind: "fixture".into(),
                    name: "test".into(),
                    base_url: None,
                },
                params: json!({}),
                created: "2026-09-08T00:00:00Z".into(),
                cerul_version: "0.0.3".into(),
                input_hash: crate::index::stations::station_key(
                    episode,
                    "primary",
                    name,
                    &json!({}),
                )
                .unwrap(),
                record_schema: format!("{name}/1"),
            },
            records: vec![Record {
                id: "record".into(),
                start_us: 26_000_000,
                end_us: 27_000_000,
                confidence: None,
                fields: serde_json::from_value(fields).unwrap(),
            }],
        }
    }
    #[tokio::test]
    async fn busy_workspace_precedes_vector_availability_errors() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let options = Options {
            query: Some("cup".into()),
            ..Default::default()
        };
        let writer = storage::WorkspaceLock::acquire(dir.path()).unwrap();
        let error = run(dir.path(), &config, &options, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("already being written"));
        assert!(error.downcast_ref::<ProviderError>().is_none());
        drop(writer);
        let error = run(dir.path(), &config, &options, CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProviderError>().unwrap().kind,
            Failure::Unsupported
        );
    }
    #[tokio::test]
    async fn prefilter_recovers_low_ranked_event_and_offline_modes_count_text_and_save() {
        let ok = json!({"embedding":{"values":[1.0,0.0]}});
        let (base, server) = crate::providers::tests::server(vec![
            (200, ok.clone()),
            (200, ok.clone()),
            (200, ok.clone()),
            (200, ok.clone()),
            (200, ok),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=64x64:rate=2:duration=30",
                    "-c:v",
                    "libx264",
                ])
                .arg(&source),
        )
        .unwrap();
        let workspace = dir.path().join("workspace");
        let episode = discover::ordinary_episode(&source).unwrap();
        let sidecar = discover::publish_episode(&workspace, &episode, None).unwrap();
        file(&episode, "semantic.event", json!({"verb":"regrasp"}))
            .publish(&sidecar.join("semantic.event.jsonl"), 30_000_000, None)
            .unwrap();
        file(&episode, "screen_text", json!({"text":"ECONNREFUSED"}))
            .publish(&sidecar.join("screen_text.jsonl"), 30_000_000, None)
            .unwrap();
        let mut config = Config::default();
        config.embedding.base_url = base;
        config.embedding.dims = Some(2);
        let space = config.space_id().unwrap();
        let rows: Vec<_> = (0..15)
            .map(|i| VectorRow {
                id: format!("v{i}"),
                episode: episode.episode_id.clone(),
                stream: "primary".into(),
                kind: Kind::Video,
                start_us: i * 2_000_000,
                end_us: (i + 1) * 2_000_000,
                vector: if i == 13 {
                    vec![0.0, 1.0]
                } else {
                    vec![1.0, 0.0]
                },
                text: format!("unit {i}"),
                still: false,
                space_id: space.clone(),
                params_hash: "fixture".into(),
            })
            .collect();
        vectors::write(
            &sidecar.join("embeddings").join(format!("{space}.parquet")),
            &rows,
            2,
        )
        .unwrap();
        let options = Options {
            query: Some("regrasp cup".into()),
            filters: vec!["semantic.event.verb=regrasp".into()],
            limit: 1,
            ..Default::default()
        };
        let report = run(&workspace, &config, &options, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(report.hits.len(), 1);
        assert_eq!(report.hits[0].start_us, 26_000_000);
        assert_eq!(report.hits[0].end_us, 27_000_000);
        assert_eq!(report.hits[0].excerpt, "unit 13");
        let image_report = run(
            &workspace,
            &config,
            &Options {
                image: Some(
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ocr-text.png"),
                ),
                filters: options.filters.clone(),
                limit: 1,
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(image_report.hits[0].start_us, 26_000_000);
        assert_eq!(server.join().unwrap().len(), 5);
        // The endpoint has stopped. These modes must not probe or embed.
        fs::remove_dir_all(workspace.join("index")).unwrap();
        let counted = run(
            &workspace,
            &config,
            &Options {
                filters: options.filters.clone(),
                count: true,
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(counted.counts.unwrap().records, 1);
        let text = run(
            &workspace,
            &config,
            &Options {
                query: Some("ECONNREFUSED".into()),
                text: true,
                save: Some(dir.path().join("clips")),
                pad_us: 0,
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(text.hits.len(), 1);
        assert!(matches!(text.hits[0].matched, Some(Kind::Screen)));
        assert_eq!(
            media::probe(text.hits[0].clip.as_ref().unwrap())
                .unwrap()
                .duration_us,
            1_000_000
        );
        let missing = run(
            &workspace,
            &config,
            &Options {
                filters: vec!["semantic.flag.kind=review".into()],
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            missing.downcast_ref::<ProviderError>().unwrap().kind,
            Failure::Unsupported
        );
        let unindexed = dir.path().join("unindexed.mp4");
        media::run(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=size=64x64:rate=2:duration=1",
                    "-c:v",
                    "libx264",
                ])
                .arg(&unindexed),
        )
        .unwrap();
        let other = discover::ordinary_episode(&unindexed).unwrap();
        discover::publish_episode(&workspace, &other, None).unwrap();
        let error = run(
            &workspace,
            &config,
            &Options {
                query: Some("cup".into()),
                within: Some(unindexed),
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProviderError>().unwrap().kind,
            Failure::Unsupported
        );
        storage::write_json(
            &crate::index::embed::state_path(&sidecar, "primary", "primary", &space),
            &crate::index::embed::State {
                input_hash: "fixture".into(),
                complete: false,
                error: None,
            },
        )
        .unwrap();
        let error = run(
            &workspace,
            &config,
            &Options {
                query: Some("cup".into()),
                within: Some(source),
                ..Default::default()
            },
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ProviderError>().unwrap().kind,
            Failure::Unsupported
        );
    }
}
