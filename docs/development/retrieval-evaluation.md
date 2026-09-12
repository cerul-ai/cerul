# Retrieval evaluation

The first experiment measures per-track score distributions before choosing a
fusion formula. The shipped baseline retains raw maximum cosine across video,
speech, and screen-text candidates. Description vectors and lexical candidates
are exported independently; neither is silently mixed into this baseline.

Use a dedicated workspace with synthetic or licensed media. Create diagnostic,
tuning, and held-out video groups. Cover visible-only, spoken-only, identifiers,
state changes, mentioned-versus-demonstrated, silent, and no-answer queries.
Keep source licenses and human judgments with the private experiment outputs;
do not commit user media, cached vectors, or exported search text.

Prepare a JSONL query file with one item per line:

```json
{"id":"q1","query":"A hand opens the drawer","split":"holdout","episodes":["episode-id"]}
```

Populate query caches through normal authorized searches using the experiment's
configuration and workspace. This preparation can make paid model calls. The
exporter itself requires cached vectors, fails if any are absent, and never
contacts an endpoint:

```sh
cargo run --locked --example retrieval_diagnostics -- /tmp/cerul-eval /tmp/cerul-eval.toml /tmp/queries.jsonl /tmp/candidates.jsonl
```

It builds temporary projections from current sidecars, including separately
stored descriptions, without modifying the production search projection. Each
track returns at most 100 candidates. Output includes source, stream, original
interval, raw cosine or BM25, rank, space, row count, query split, and separate
vector/lexical latency. Cosine and BM25 scores are never directly compared.
Record hardware, build mode, and cache conditions separately. These timings
exclude query encoding and index construction and are not end-to-end latency.

The [initial live pilot](retrieval-pilot.md) records one public video and four
diagnostic queries. It checks pipeline behavior; it does not satisfy the
representative or held-out acceptance gates below.

Review all relevant intervals for every query, then create labels:

```json
{"query_id":"q1","split":"holdout","reviewed":true,"exhaustive":true,"scope":["episode-id"],"answers":[{"episode":"episode-id","stream":"primary","start_us":4000000,"end_us":9000000,"kinds":["video","description"]}]}
```

An empty `answers` list explicitly labels a no-answer query. Mark `reviewed`
and `exhaustive` only after checking the source video and its evidence. Diagnostic,
tuning, and held-out videos must be disjoint, including videos with no-answer queries;
the evaluator checks the entire explicit corpus scope, including no-answer
videos, and rejects export/label scope mismatches. The exporter prefilters both
vector and lexical candidates to the query's `episodes`; omitting it searches
all registered episodes, which must then all appear in the label's `scope`.
The experiment inventory must also keep clips from the same original source
video in one split. Do not relabel model output as ground truth.

```sh
python3 scripts/evaluate-retrieval.py /tmp/candidates.jsonl /tmp/labels.jsonl --k 5 > /tmp/metrics.json
```

The evaluator reports interval Recall@K and binary nDCG with a temporal IoU
threshold of 0.5, best temporal IoU, duplicate rate, incorrect evidence counts,
no-answer false positives, relevant/irrelevant score percentiles, and latency
P50/P95. Duplicate answers cannot earn repeated nDCG gains. The raw-max diagnostic
groups identical intervals; it does not simulate production temporal merging.
`--threshold` applies only to cosine channels; BM25 requires its own future gate.
No default calibration constants or retrieval-quality claims follow from the
synthetic unit tests.

Use the diagnostic split for inspecting score distributions, tuning videos for
choosing gates/weights, and held-out videos once for acceptance. Only after that
measurement should calibrated max and grouped RRF be implemented and compared.
Record call counts, actual provider usage/cost, full search P50/P95, modality
ablations, and 100k-row flat/ANN recall and latency in the experiment report.
Activation requires an explicit recipe version and reviewed acceptance evidence.

Capture JSON-mode stderr during authorized model runs to retain `model_request`
events. Count attempts separately from logical `request_id` values; include
capability probes, and flag missing `usage` instead of treating it as free work.
The adapter reads Gemini's [embedding usage](https://ai.google.dev/api/embeddings#EmbeddingUsageMetadata)
and [generation usage](https://ai.google.dev/api/generate-content#UsageMetadata)
fields, including their distinct input-modality field names. These are reported
tokens, not billed dollars or a provider-side frame count. Unsupported adapters
and interrupted/error responses can lack usage even when charges occur. Keep
the price schedule and coverage of available usage with any cost estimate.
Cached diagnostic exports never create synthetic usage receipts.

## Engine-only benchmark

Run the synthetic index benchmark independently of model evaluation:

```sh
cargo run --release --locked --example benchmark_retrieval -- --rows 100000 --dims 3072 --queries 20 > /tmp/cerul-engine-benchmark.json
```

It creates and removes its own temporary index, uses no credentials or model
calls, and does not touch a configured workspace. It compares exact cosine
Top-10 with IVF_FLAT Top-10, including a prefiltered query. The seed controls
the generated vectors; it does not control the SDK's index-training randomness.

One run on 2026-09-12, Apple M3 Max (14 physical cores, 36 GiB RAM), release
build, LanceDB 0.38.0:

| Query shape | Flat P50 / P95 | IVF_FLAT P50 / P95 | ANN recall against exact Top-10 |
| --- | --- | --- | --- |
| All 100,000 rows | 166.3 / 189.8 ms | 15.3 / 19.9 ms | 1.00 |
| Prefilter to video rows starting before 100,000 s (4,000 rows) | 184.7 / 246.5 ms | 4.4 / 6.0 ms | 1.00 |

Configuration: 3,072 dimensions, 64 IVF partitions, 16 probes, 20 queries,
64 synthetic clusters, four equally sized evidence kinds. Raw vector payload
was 1,228,800,000 bytes; this is not peak memory or on-disk index size. Writing
the projection took 5.20 s and IVF_FLAT construction took 4.86 s. The first
four-track request, with 100 candidates per track, took 2.13 s. Its query shape
differs from the single Top-10 queries above and must not be compared as the
same workload. It used a new index object with uncontrolled OS cache state,
so it is not a reproducible cold-start measurement.

These are warmed engine timings from a single run, excluding query encoding,
sidecar validation, temporal fusion, and CLI startup. The deliberately separated
clusters make ANN recall optimistic. The result motivates a real-vector
benchmark; it does not establish semantic quality, full-search latency, or a
threshold at which production should automatically enable ANN. Production
continues to use the flat projection pending representative measurements.
