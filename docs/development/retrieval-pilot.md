# Initial live retrieval pilot

On 2026-09-12, an authorized single-video run completed indexing, OCR, ASR,
video understanding, independent description embeddings, and four searches.
It verified the live pipeline, while exposing remaining ranking and temporal
localization problems. This is diagnostic evidence, not retrieval acceptance.

## Reproduction context

- Runtime commit: `4c1a009f4b3a48c85b270d49b97602fc4c478914`, release build,
  Apple M3 Max, macOS arm64.
- Public research fixture: QVHighlights clip
  `eYTTYnFgKQc_360.0_510.0`, SHA-256
  `04c9ee78a0074564cb790da545ae320f7e1a7917e4f5a00523ebfb00877b1839`.
  Clip-local duration is 150 seconds; all intervals below are clip-local.
- Official query: QVHighlights train QID 1157, with relevant interval 66–86 s.
  Dataset labels were used as published; no new human modality-attribution
  judgments were added.
- Gemini Embedding 2, 3,072 dimensions; silent 1 FPS/480 px video proxies;
  30-second retrieval windows with five-second overlap.
- Gemini 3.5 Transcribe; Gemini 3.8 Flash for understanding. Official Gemini
  endpoint, isolated workspace, two concurrent requests, configured 20 RPM.
- Raw-max production search; description and lexical candidates exported
  separately through the offline diagnostic example.

Media, keys, sidecars, vectors, and full search exports remain outside the
repository. Only aggregate observations and short diagnostic queries appear
here. The run makes no claim about this fixture's redistribution rights.

## Observed output

| Product | Count |
| --- | ---: |
| OCR sampling points | 150 |
| Published screen-text records | 14 |
| Published transcript records | 34 |
| Base vector rows: video / speech / screen | 6 / 6 / 3 |
| Visual scene records and independent description vectors | 18 / 18 |
| Coarse sections | 5 |
| Episode overviews | 1 |
| Model-generated search suggestions: visual / speech | 1 / 1 |

All five understanding windows and the overview completed with no reported
stage errors. Coverage recorded no failed windows. Total index wall time was
approximately 158 seconds, inferred from workspace creation and final-output
file timestamps, not from instrumented stage timings.

The original CLI added a third, long transcript excerpt after the two generated
suggestions. The follow-up change preserves the current overview's zero-to-three
selection; extractive suggestions remain a fallback when a current overview is
unavailable. A full cached CLI re-index with the changed debug build returned
exactly the two generated suggestions, with no endpoint request notice or
partial failure.

## Search observations

| Query | Production Top-1, winning track and raw cosine | Independent description Top-1 |
| --- | --- | --- |
| A family eats dinner together. | 50–80 s, speech, 0.6349 | 65.16–85.04 s, 0.8148 |
| blue and gold elephant figurine | 125–150 s, video, 0.6437 | 141–150 s, 0.7918 |
| खीर से शुरुआत कर रहे हैं | 50–80 s, speech, 0.7236 | 56.2–60 s, 0.6066 |
| A rocket launches from a snowy mountain | 0–30 s, screen, 0.5856 | 106–108.5 s, 0.5694 |

The first query has an official temporal label. Production Top-1 has temporal
IoU 0.389 against 66–86 s; the description candidate has IoU 0.914. Finding an
overlapping window is not equivalent to passing localization at IoU 0.5. One
example does not establish an average improvement from descriptions.

The middle two queries came from the generated overview. Their source references
validate against current records, but these are not independent quality labels.
Restricting the visual query to `kind=video` retains its same Top-1 result;
this confirms that result does not require speech candidates. It does not test
an entire fresh run with ASR disabled.

The final query is an unreviewed negative probe, not an exhaustive no-answer
label. Production returns OCR fragments rather than abstaining. Lexical search
also finds an OCR record containing a common word. These observations motivate
track-specific relevance checks and lexical noise handling; they do not justify
choosing a numerical gate from this one clip.

The four first searches, including query encoding, took 1.24–1.58 seconds each.
Offline candidate extraction used a debug-built diagnostic executable and a
temporary 33-row projection; its vector phase took 16.9–18.7 ms. These are
different workloads, and four queries are insufficient for a stable P95 claim.

## Storage and cost checks

Moving the rebuildable vector index aside, preserving query caches, and repeating
the official query rebuilt the index in approximately 0.40 seconds. Results were
identical, authoritative files retained their checksums, and no model request
notice was emitted. The original index was retained as a local backup.

Provider usage metadata was not retained by this runtime. Actual billed tokens,
retry counts, and charged cost are therefore unavailable. The pre-run budget
estimate was USD 0.20–0.30; this is not an observed bill. At the
[published Standard price](https://ai.google.dev/gemini-api/docs/pricing#gemini-embedding-2),
175 assumed processed video frames cost approximately USD 0.138 for visual
embedding alone. That calculation assumes one processed frame per proxy second
and excludes understanding, ASR, text embeddings, retries, and free quotas.
The follow-up runtime adds JSON-mode per-attempt usage events for future runs;
it cannot recover the receipts discarded by this pilot. Actual usage must be
captured before validating the proposal's cost table.

## Offline fusion replay

On 2026-09-13, the new Rust fusion module replayed the same four cached candidate
exports without an endpoint or new embeddings. Three deliberately exploratory
recipes were used: the three-track raw-max baseline; identity calibration over
the four cosine tracks with zero agreement bonus; and grouped RRF over all five
tracks with equal weights and rank constant 60. Cosine gates were -1 and the
BM25 gate was 0, effectively admitting all exported rows. These settings expose
behavior; they are not fitted thresholds or production recommendations.

| Query | Raw max Top-1 | Identity max including descriptions | Grouped RRF, no effective gate |
| --- | --- | --- | --- |
| Official dinner query | 50–80 s | 65.16–85.04 s | 50–80 s |
| Generated figurine query | 125–150 s | 141–150 s | 125–150 s |
| Generated speech query | 50–80 s | 50–80 s | 50–80 s |
| Unreviewed negative probe | 0–30 s | 0–30 s | 25–55 s |

The identity candidate can retain a short scene as its own result. Grouped RRF
still selects the wider dinner window, and all three ungated recipes return a
result for the negative probe. Rank fusion alone has not solved localization or
abstention. No aggregate quality metric was generated from these unreviewed
probes. The replay records source intervals, raw scores, original ranks, actual
contributors, recipe provenance, and fusion-only timings; production ranking
and authoritative sidecars are unchanged.

## Remaining acceptance work

The sample contains one original source, one officially labeled query, two
self-generated suggestions, and one unreviewed negative probe. It provides
neither the required twenty-to-thirty representative queries nor disjoint
tuning and held-out sets. Per-track calibration, fusion competition, description
and lexical quality, and production ANN still need broader evaluation. The owner
subsequently requested default hybrid activation after lightweight acceptance;
that decision supersedes the earlier activation gate, without turning this pilot
into a representative benchmark.

Generated scene boundaries sometimes have sub-second precision despite 1 FPS
input. Integer-microsecond storage does not establish microsecond localization
accuracy. Boundary uncertainty and sampling-aware validation still need to be
addressed; the original model estimates and submitted sample timestamps were
preserved for inspection.

## Default activation: lightweight acceptance (2026-09-13)

The owner requested connecting hybrid retrieval to ordinary search immediately,
using simple effect acceptance rather than waiting for a larger benchmark.
`hybrid/affine-max-full-query-lexical/1` is now the default. The constants are
fixed engineering choices, not fitted calibration or a claimed benchmark winner.
`search.hybrid = false` retains the previous baseline.

Replaying the four existing diagnostic exports through the production recipe
made **zero model calls** and preserved the original embedding-space identity:

| Cached query | Previous raw-max top interval | Default top interval and evidence |
| --- | --- | --- |
| Family eating dinner | 50–80 s | 65.16–85.04 s, visual description |
| Blue and gold elephant figurine | 125–150 s | 141–150 s, visual description |
| Exact Hindi spoken phrase | 50–80 s | 61.2–66.1 s, original transcript full-text match |
| Unreviewed rocket negative probe | 0–30 s | Still returns a candidate without a threshold; empty with raw cosine threshold 0.6 and the full-query lexical gate |

This is candidate-fusion replay, not a new end-to-end model run. It neither
relabels old vectors into a new space nor supplies representative quality metrics.
The official dinner interval is 66–86 seconds; the generated suggestions and
negative probe retain their original diagnostic status.

Reproduce the comparison from a private diagnostic export:

```sh
cargo run --locked --example retrieval_default < /tmp/candidates.jsonl > /tmp/default-comparison.jsonl
```

Focused behavioral tests additionally exercise the real default search entry:
description-only retrieval without ASR, cached query reuse, zero-call projection
rebuild, invalidation after scene changes, lexical identifier recovery, kind
prefilters, and the legacy configuration switch. Original cosine and BM25 units
remain separate in JSON evidence. These local checks do not replace release
platform, real-inference, or large-corpus acceptance.
