//! Offline candidate export. Query vectors must already be cached by `search`.
use anyhow::{Context, Result, ensure};
use cerul::{
    config::Config,
    episode::{Episode, Stream},
    index::{
        descriptions, discover, embed, lance, lexical, records,
        vectors::{self, Kind},
    },
    storage,
};
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeMap, fs, path::PathBuf, time::Instant};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    id: String,
    query: String,
    split: String,
    #[serde(default)]
    episodes: Option<Vec<String>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 4,
        "usage: retrieval_diagnostics WORKSPACE CONFIG.toml QUERIES.jsonl OUTPUT.jsonl"
    );
    let workspace = PathBuf::from(&args[0]);
    let config = Config::load(&[PathBuf::from(&args[1])])?;
    let space = config.space_id()?;
    let dims = config.embedding.dims.context("missing dimensions")?;
    let queries: Vec<Query> = fs::read_to_string(&args[2])?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(!queries.is_empty(), "no queries");
    let mut ids = std::collections::BTreeSet::new();
    let mut query_vectors = Vec::new();
    for query in &queries {
        ensure!(
            ids.insert(&query.id) && !query.id.is_empty() && !query.query.trim().is_empty(),
            "invalid or duplicate query ID"
        );
        ensure!(
            matches!(query.split.as_str(), "diagnostic" | "tune" | "holdout"),
            "invalid split"
        );
        let key = storage::cache_key(&(&space, &query.query))?;
        let vector: Vec<f32> = serde_json::from_slice(
            &fs::read(workspace.join("cache/queries").join(format!("{key}.json"))).with_context(
                || {
                    format!(
                        "missing cached vector for {}; this tool never calls a model",
                        query.id
                    )
                },
            )?,
        )?;
        ensure!(
            vector.len() == dims
                && vector.iter().all(|x| x.is_finite())
                && vector.iter().any(|x| *x != 0.),
            "invalid cached query vector"
        );
        query_vectors.push(vector);
    }
    let _lock = storage::WorkspaceLock::acquire(&workspace)?;
    // Isolate experimental description and lexical projections from production.
    let temporary = tempfile::tempdir()?;
    let index = lance::VectorIndex::open(temporary.path(), &space, dims, true).await?;
    let mut available_episodes = std::collections::BTreeSet::new();
    for entry in discover::read_registry(&workspace)? {
        let episode: Episode =
            serde_json::from_slice(&fs::read(entry.sidecar.join("episode.json"))?)?;
        episode.validate()?;
        available_episodes.insert(episode.episode_id.clone());
        let base = entry
            .sidecar
            .join("embeddings")
            .join(format!("{space}.parquet"));
        let base_rows = if base.is_file() {
            vectors::read(&base, dims)?
        } else {
            Vec::new()
        };
        ensure!(
            base_rows
                .iter()
                .all(|row| row.episode == episode.episode_id && row.space_id == space),
            "invalid vector identity"
        );
        for stream in &episode.streams {
            if !matches!(stream, Stream::Video { .. }) {
                continue;
            }
            let mut rows = Vec::new();
            if embed::usable(&entry.sidecar, stream.id(), &episode.time.reference, &space)? {
                rows.extend(
                    base_rows
                        .iter()
                        .filter(|row| row.stream == stream.id())
                        .cloned(),
                );
            }
            if let Some((_, descriptions)) =
                descriptions::read_current(&entry.sidecar, &episode, stream.id(), &space, dims)?
            {
                rows.extend(descriptions);
            }
            index
                .replace(&episode.episode_id, stream.id(), &rows)
                .await?;
        }
    }
    let files = records::sidecars(&workspace)?;
    let lexical = lexical::LexicalIndex::prepare(temporary.path(), &files).await?;
    let count = index.count().await?;
    let kinds = [Kind::Video, Kind::Speech, Kind::Screen, Kind::Description];
    let mut output = String::new();
    for (query, vector) in queries.iter().zip(&query_vectors) {
        let episodes = query
            .episodes
            .clone()
            .unwrap_or_else(|| available_episodes.iter().cloned().collect());
        ensure!(
            !episodes.is_empty() && episodes.iter().all(|id| available_episodes.contains(id)),
            "query {} has an empty or unknown corpus scope",
            query.id
        );
        let predicate = format!(
            "episode IN ({})",
            episodes
                .iter()
                .map(|id| lance::literal(id))
                .collect::<Vec<_>>()
                .join(",")
        );
        let started = Instant::now();
        let lists = index.search_tracks(vector, &predicate, &kinds, 100).await?;
        let vector_ms = started.elapsed().as_secs_f64() * 1000.;
        let mut tracks = BTreeMap::new();
        for (kind, rows) in kinds.iter().zip(lists) {
            tracks.insert(kind.as_str(), rows.into_iter().enumerate().map(|(rank, (row, score))| json!({
                "id":row.id,"episode":row.episode,"stream":row.stream,"start_us":row.start_us,"end_us":row.end_us,
                "text":row.text,"raw_score":score,"rank":rank+1
            })).collect::<Vec<_>>());
        }
        let started = Instant::now();
        let lexical_rows = lexical.search(&query.query, Some(&predicate), 100).await?;
        let lexical_ms = started.elapsed().as_secs_f64() * 1000.;
        tracks.insert("lexical", lexical_rows.into_iter().enumerate().map(|(rank, (row, score))| json!({
            "id":row.record.id,"episode":row.episode,"stream":row.stream,"start_us":row.record.start_us,"end_us":row.record.end_us,
            "annotation":row.annotation,"text":row.record.fields.get("text"),"raw_score":score,"rank":rank+1
        })).collect());
        output.push_str(&serde_json::to_string(&json!({
            "schema":"retrieval-diagnostics/1","query_id":query.id,"query":query.query,"split":query.split,
            "space_id":space,"dims":dims,"vector_rows":count,"candidate_limit":100,"episodes":episodes,
            "vector_ms":vector_ms,"lexical_ms":lexical_ms,"model_calls":0,"tracks":tracks
        }))?);
        output.push('\n');
    }
    let destination = PathBuf::from(&args[3]);
    ensure!(!destination.exists(), "output already exists");
    fs::write(destination, output)?;
    Ok(())
}
