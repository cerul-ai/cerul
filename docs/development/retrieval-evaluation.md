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
choosing gates/weights, and held-out videos once for acceptance. Parameterized
fusion and replay are implemented independently of corpus preparation; fit and
select their parameters only after measurement.
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

## Offline fusion replay

The shared Rust module `search::fusion` implements three recipes over the same
saved candidate lists: the existing three-track raw-max baseline, affine
calibration plus capped agreement, and grouped weighted RRF. There is no model
client, automatic calibration fitting, or default-search activation in replay.
The [recipe schema](../../schemas/fusion-recipe.json) and
[moment schema](../../schemas/fusion-moment.json) are generated from Rust.

Create a recipe suite with `schema: "retrieval-fusion-recipes/1"`, the exact
`space_id`, `parameter_split` (`diagnostic` or `tune`), `parameter_episodes`, a
`source_groups` mapping from episode IDs to original source-video identities,
and a `recipes` object mapping experiment names to recipe objects. Every
parameter episode and searched episode must have a source group. Keep clips
from the same original video together even when their episode IDs differ.

Each recipe has explicit parameters; there are no recommended numeric defaults:

| Method | Required fields | Score meaning |
| --- | --- | --- |
| `raw_max` | `threshold` (number or null) | Maximum cosine over video/speech/screen with identical intervals |
| `calibrated_max` | `channels`, `agreement_bonus`, `agreement_cap` | Per-channel `raw_min`, positive `scale`, and `offset`; affine relevance clamped to [0, 1] |
| `grouped_rrf` | `channels`, `rank_constant` | Per-channel `raw_min` and positive `weight` up to 1; original rank, before gating |

Only channels present in a recipe are enabled. BM25's gate and calibration are
separate from cosine's; supplying a cosine gate as BM25's is not a calibration.
The calibrated candidate takes the strongest group score, adds the lesser of
the agreement cap and `agreement_bonus` times the weaker group score, and caps
the result at 1. The RRF candidate sums each group's strongest
`weight / (rank_constant + original_rank)` contribution. Neither score is a
probability. Weights are fixed across candidates, including silent sources.

The initial grouping is deliberately conservative: video/description form one
visual group; speech/screen/lexical form one text group. Duplicate rows,
generated descriptions, or lexical echoes cannot multiply votes. This caps
distinct text evidence too, which must be measured when choosing the recipe.
Description and lexical intervals attach to at most one admitted base anchor,
by greatest IoU, only when they cover half of both intervals and are no longer
than the anchor. Otherwise they retain their own interval. Ties use source time
and stable identity; there is no transitive temporal merging in this experiment.

```sh
cargo run --locked --example retrieval_fusion -- /tmp/candidates.jsonl /tmp/recipes.json /tmp/fused.jsonl
python3 scripts/evaluate-retrieval.py /tmp/candidates.jsonl /tmp/labels.jsonl --fusions /tmp/fused.jsonl --k 5 > /tmp/fusion-metrics.json
```

Replay needs no labels and can inspect already cached diagnostic queries. Its
output pins the exact input-line hash, the algorithm version, the full recipe
suite and their combined hash, all
admitted evidence with original scores/ranks/times, actual contributing votes,
and fusion-only latency. It validates the entire run before publishing a new
file and refuses to overwrite earlier output. The evaluator still requires
reviewed, exhaustive labels and validates replay hashes and source evidence.
Do not edit or reformat the candidate export after producing a replay.

Diagnostic parameters cannot score tuning or held-out queries. Tuning sources
cannot overlap held-out source groups, and all three data splits must be
source-disjoint. These guards check declared provenance; they do not certify
that judgments are human-reviewed or parameters were selected correctly.
The cosine `--threshold` affects original cosine tracks only; replay recipes
carry their own gates. Fused recall requires both a matching returned interval
and correctly attributed original contributing evidence. Carrying an unrelated
record cannot turn speech into visual evidence. A good visual match accompanied
by an incorrect text vote can earn recall while still increasing the incorrect
evidence count.

The single-video [pilot replay](retrieval-pilot.md#offline-fusion-replay) uses
explicit exploratory parameters, not fitted or accepted ones. Joint
concatenation remains an unmeasured ablation: separate saved embeddings cannot
reconstruct the embedding of concatenated inputs without another model call.

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
