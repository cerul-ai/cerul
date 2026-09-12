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
{"id":"q1","query":"A hand opens the drawer","split":"holdout"}
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

Review all relevant intervals for every query, then create labels:

```json
{"query_id":"q1","split":"holdout","reviewed":true,"exhaustive":true,"answers":[{"episode":"episode-id","stream":"primary","start_us":4000000,"end_us":9000000,"kinds":["video","description"]}]}
```

An empty `answers` list explicitly labels a no-answer query. Mark `reviewed`
and `exhaustive` only after checking the source video and its evidence. Tuning
and held-out videos must be disjoint, including videos with no-answer queries;
the evaluator checks answer-video overlap, while the experiment inventory must
also establish disjoint source groups. Do not relabel model output as ground truth.

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
