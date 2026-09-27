# Hybrid video retrieval decisions

This page records the rationale and remaining evaluation work for hybrid
retrieval. [DESIGN.md](../../DESIGN.md) defines current behavior, and the
[search guide](../video-search.md) describes supported commands.
The [original proposal](https://github.com/cerul-ai/cerul/blob/cce52298b3b594699ed45b4ce972c48acb6672f0/docs/design/hybrid-video-search.md)
preserves historical baselines, research references and dated cost estimates.
Those estimates are not current prices or measured bills.

## Decisions

| Area | Current decision | Reason or limit |
| --- | --- | --- |
| Embedding space | Default 3072 dimensions; configurable endpoints and dimensions | Space identity separates incompatible models, templates and dimensions. |
| Visual route | Native video embeddings of silent 1 FPS, 480 px proxy clips | Frame embeddings remain an evaluation baseline, not an automatic fallback. |
| Retrieval windows | 30 seconds with five-second overlap | An engineering baseline, not a demonstrated optimum. |
| Evidence | Independent video, speech and screen rows, plus existing description vectors and lexical records | Preserve evidence modality and original intervals rather than concatenating tracks. |
| Fusion | Fixed affine max with capped agreement | Initial relevance scales, not fitted probabilities. Raw-max fallback remains available. |
| Analysis | Explicit `analyze`, separate from CLI indexing | Indexing does not generate scenes, overviews or description vectors. |

## Indexing and updates

The CLI builds visual, speech and OCR retrieval data. OCR and ASR can run
concurrently; embedding waits for their required inputs. Shared frame sampling
preserves timestamps while allowing each consumer to use its own resolution.

Text hygiene applies to derived retrieval inputs, never by rewriting original
OCR or ASR evidence. Each vector track carries its own dependency fingerprint,
so a speech correction does not invalidate unchanged video vectors. Cache
identity includes source content and processing recipes.

Explicit analysis writes scenes, sections and summaries without embeddings.
The library retains an opt-in legacy understanding workflow; this is not the
CLI default. Existing analysis and description vectors remain readable and
searchable. A cleanup must not delete them or silently regenerate them.

## Authoritative storage and consumers

JSONL annotations and Parquet vectors in sidecars are authoritative. Rust types
generate their schemas. Publish validated sidecars atomically before updating
the rebuildable search projections.

Description vectors preserve each scene's original interval and use a separate
projection. Lexical search indexes original speech and screen records; its
tokenizer recipe is independent of embedding space. Both projections rebuild
without model calls. Source revisions and generation identities prevent stale
records from being reused after content or recipe changes.

## Retrieval

Scope and temporal annotation filters apply before candidate ranking. Each
present vector track has its own candidate budget, and lexical retrieval
requires every query token. Hybrid search expands candidate budgets until the
scoped candidates are exhausted; collecting `limit` hits alone cannot prove
the fused top-k. This prioritizes correctness and can cost latency and memory
at large scope.

Default fusion maps cosine to `0.5 * cosine + 0.5` and BM25 to
`0.8 + 0.01 * BM25`, clamped to [0, 1]. Video/description and
speech/screen/lexical form two groups, each contributing its strongest evidence,
with agreement capped at 0.02. Missing evidence contributes nothing. These
constants do not establish calibrated confidence.

Original cosine thresholds and the full-query lexical gate retain their
separate meanings. Image queries omit lexical retrieval. Exact-text search
retains substring matching. Duplicate suppression preserves the strongest
moment's boundaries rather than widening them. Disjoint filter intervals are
fused independently and results stay within their selected interval.

Query embeddings are cached per compatible space. A valid cache hit requires
neither a capability probe nor an embedding call. JSON retains raw scores,
evidence intervals and the fusion recipe so results can be inspected and
replayed. See [default hybrid search](../configuration.md#default-hybrid-search).

## Evaluation

The [initial live pilot](../development/retrieval-pilot.md) is diagnostic evidence
for its recorded runtime and single video. The
[evaluation guide](../development/retrieval-evaluation.md) provides candidate
export, offline replay and an engine-only benchmark. Neither replaces a
representative quality benchmark.

Remaining work, in order:

1. Build reviewed labels across ordinary clips, lectures, screen recordings,
   silent video and first-person demonstrations. Include visual-only,
   speech-only, literal-text, state-change and no-answer queries. Keep tuning
   videos separate from acceptance videos.
2. Measure per-track relevant and irrelevant score distributions before
   fitting calibration. Compare the fixed default, raw max, calibrated max
   and grouped RRF on the same held-out set.
3. Measure Recall@K, nDCG, temporal localization, duplicate hits, incorrect
   evidence attribution and no-answer false positives. Remove modalities
   explicitly and include stale-sidecar and interrupted-rebuild cases.
4. Compare window sizes with an explicit overlap policy. Record actual model
   usage and retries; changed overlap can change cost as well as localization.
5. Measure cold/warm P50/P95, memory and exact-neighbor recall at representative
   scale before enabling ANN or changing candidate stopping rules. Synthetic
   engine measurements do not establish real-video retrieval quality.

## Out of scope, recorded for later

Desktop/cloud unification, a second vector backend, query planning and mandatory
vision reranking are separate requirements. Do not introduce those abstractions
as repository cleanup. Any future cross-surface comparison needs matching
evidence types, intervals and filter semantics, with explicit recall tolerances.
