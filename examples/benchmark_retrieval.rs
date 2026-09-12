//! Reproducible engine benchmark, not a semantic-retrieval quality benchmark.
use anyhow::{Context, Result, ensure};
use arrow_array::{Array, RecordBatch, StringArray};
use cerul::index::{
    lance::VectorIndex,
    vectors::{Kind, VectorRow},
};
use clap::Parser;
use futures::TryStreamExt;
use lancedb::{
    DistanceType, Table,
    index::{Index, vector::IvfFlatIndexBuilder},
    query::{ExecutableQuery, QueryBase},
};
use serde_json::json;
use std::{collections::BTreeSet, time::Instant};

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 100_000)]
    rows: usize,
    #[arg(long, default_value_t = 3072)]
    dims: usize,
    #[arg(long, default_value_t = 20)]
    queries: usize,
    #[arg(long, default_value_t = 64)]
    partitions: u32,
    #[arg(long, default_value_t = 16)]
    probes: usize,
}
fn random(seed: &mut u64) -> f32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 40) as f32 / 16_777_216.) - 0.5
}
fn vector(seed: &mut u64, center: &[f32]) -> Vec<f32> {
    center.iter().map(|x| x + random(seed) * 0.35).collect()
}
fn percentile(values: &[f64], fraction: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((values.len() as f64 * fraction).ceil() as usize).saturating_sub(1)]
}
async fn query(
    table: &Table,
    vector: &[f32],
    flat: bool,
    probes: usize,
    filter: Option<&str>,
) -> Result<Vec<String>> {
    let mut query = table
        .vector_search(vector)?
        .distance_type(DistanceType::Cosine)
        .limit(10)
        .nprobes(probes);
    if flat {
        query = query.bypass_vector_index();
    }
    if let Some(filter) = filter {
        query = query.only_if(filter);
    }
    let batches: Vec<RecordBatch> = query.execute().await?.try_collect().await?;
    let mut ids = Vec::new();
    for batch in batches {
        let column = batch
            .column_by_name("id")
            .and_then(|a| a.as_any().downcast_ref::<StringArray>())
            .context("missing ID")?;
        ensure!(column.null_count() == 0, "null ID");
        ids.extend((0..column.len()).map(|i| column.value(i).into()));
    }
    Ok(ids)
}
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        args.rows >= 100
            && args.dims >= 8
            && args.queries >= 2
            && args.partitions > 0
            && args.probes > 0
            && args.probes <= args.partitions as usize,
        "invalid benchmark size"
    );
    let mut seed = 20260912;
    let centers: Vec<Vec<f32>> = (0..64)
        .map(|_| (0..args.dims).map(|_| random(&mut seed)).collect())
        .collect();
    let queries: Vec<_> = (0..args.queries)
        .map(|i| vector(&mut seed, &centers[i % centers.len()]))
        .collect();
    let workspace = tempfile::tempdir()?;
    let space = "f".repeat(64);
    let index = VectorIndex::open(workspace.path(), &space, args.dims, true).await?;
    let kinds = [Kind::Video, Kind::Speech, Kind::Screen, Kind::Description];
    let rows: Vec<_> = (0..args.rows)
        .map(|i| VectorRow {
            id: format!("v{i}"),
            episode: "synthetic".into(),
            stream: "primary".into(),
            kind: kinds[i % 4],
            start_us: (i as i64 / 4) * 25_000_000,
            end_us: (i as i64 / 4) * 25_000_000 + 30_000_000,
            vector: vector(&mut seed, &centers[(i / 4) % centers.len()]),
            text: String::new(),
            still: false,
            space_id: space.clone(),
            params_hash: "synthetic-clusters/1".into(),
        })
        .collect();
    let started = Instant::now();
    index.replace("synthetic", "primary", &rows).await?;
    let write_s = started.elapsed().as_secs_f64();
    drop(rows);
    let connection = lancedb::connect(
        workspace
            .path()
            .join("index")
            .join(&space)
            .to_str()
            .context("invalid path")?,
    )
    .execute()
    .await?;
    let table = connection.open_table("chunks").execute().await?;
    let mut reports = Vec::new();
    let started = Instant::now();
    let first = index
        .search_tracks(&queries[0], "true", &kinds, 100)
        .await?;
    let first_ms = started.elapsed().as_secs_f64() * 1000.;
    ensure!(first.len() == 4, "missing evidence track");
    for filter in [None, Some("kind = 'video' AND start_us < 100000000000")] {
        let mut times = Vec::new();
        let mut expected = Vec::new();
        for q in &queries {
            let started = Instant::now();
            expected.push(query(&table, q, true, args.probes, filter).await?);
            times.push(started.elapsed().as_secs_f64() * 1000.);
        }
        reports.push(json!({"method":"flat","filter":filter,"p50_ms":percentile(&times,0.5),"p95_ms":percentile(&times,0.95),"exact_recall_at_10":1.0}));
        // Create once after recording a true unindexed baseline.
        if filter.is_none() {
            let started = Instant::now();
            table
                .create_index(
                    &["vector"],
                    Index::IvfFlat(
                        IvfFlatIndexBuilder::default()
                            .distance_type(DistanceType::Cosine)
                            .num_partitions(args.partitions)
                            .sample_rate(128)
                            .max_iterations(20),
                    ),
                )
                .execute()
                .await?;
            reports
                .push(json!({"method":"ivf_flat_build","seconds":started.elapsed().as_secs_f64()}));
        }
        let mut times = Vec::new();
        let mut recalls = Vec::new();
        for (q, expected) in queries.iter().zip(expected) {
            let started = Instant::now();
            let actual = query(&table, q, false, args.probes, filter).await?;
            times.push(started.elapsed().as_secs_f64() * 1000.);
            let found: BTreeSet<_> = actual.into_iter().collect();
            recalls.push(
                expected.iter().filter(|id| found.contains(*id)).count() as f64
                    / expected.len().max(1) as f64,
            );
        }
        reports.push(json!({"method":"ivf_flat","filter":filter,"p50_ms":percentile(&times,0.5),"p95_ms":percentile(&times,0.95),"exact_recall_at_10":recalls.iter().sum::<f64>()/recalls.len() as f64}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":"retrieval-engine-benchmark/1","fixture":"synthetic-clusters/1","seed":20260912,"rows":args.rows,"dims":args.dims,"queries":args.queries,"partitions":args.partitions,"probes":args.probes,"build_mode":if cfg!(debug_assertions){"debug"}else{"release"},"vector_bytes":args.rows*args.dims*4,"projection_write_s":write_s,"first_four_track_query_ms":first_ms,"first_query_cache_state":"new index object; OS cache uncontrolled","model_calls":0,"reports":reports,"limitation":"Synthetic geometry measures engine behavior only; it cannot justify semantic quality claims or default ANN activation."})
        )?
    );
    Ok(())
}
