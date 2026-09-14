# Index performance

As of 2026-09-15, CLI indexing runs OCR, speech, video/text embedding and local
publication only. It does not call vision or generate descriptions and overviews.
The local-API regression below now verifies bounded speech/embedding concurrency,
zero vision requests, cache reuse, and preservation of failed legacy analysis.
The earlier analysis pipeline and benchmark below are historical, not the current
CLI workload or a current throughput claim.

## Current execution and diagnostics

Video embedding now starts alongside OCR and speech. It saves reusable row
checkpoints; the combined Parquet/Lance generation publishes after text is ready.
A complete cached index still rebuilds with zero model calls. Failed recomputes
retain prior files but withdraw the incomplete search projection.

Text documents are cached by exact content, endpoint/model, dimensions and document
template. Rows retain their independent time ranges and provenance. Identical
concurrent documents coalesce through ordered locks. Gemini and OpenAI-compatible endpoints receive synchronous batches of up to
32 distinct documents. Response counts, dimensions and OpenAI response indexes
are validated before publication.
The shared jobs/RPM limit still applies. An explicit unsupported HTTP response falls back to bounded individual requests
and disables batching for that provider instance. Malformed successful batches
fail validation rather than silently assigning vectors to the wrong text.

Default analyze retains a full-video overview. Repeated evidence content is stored
once per request, and short wire references resolve locally to exact source IDs
and revisions. Generation is bounded to 12 coarse sections, 800 summary characters,
100-character titles and 16 references per claim. Duplicate references are removed;
unique references are never silently truncated. An over-limit response gets one
bounded regeneration attempt, then remains a reported failure if still invalid.

```sh
cerul diagnostics
cerul diagnostics --json
```

The command reads the latest index/analyze measurements under
`runtime/diagnostics/<command>-latest.json` in the workspace, without model calls.
Each stage reports elapsed wall time, HTTP attempt count, summed/max request
latency, retries and cache hits/misses by category. Request latency excludes
rate-limit queues and retry backoff; stage wall time includes its waits. The
invocation stage spans the entire run, while nested/parallel stages overlap:
**do not add stage durations to obtain total time**. Counts include capability
probes and attempts that failed or were cancelled. Cache counts measure lookups,
not unique frames or seconds; a zero-model rebuild is confirmed by zero requests.

Diagnostics contain no prompts, API keys or model response bodies. They are
best-effort local records and cannot fail a published operation; dry runs do not
write them. A new run replaces the corresponding latest report. These are actual
measurements, separate from estimated progress/ETA history.

## Historical analysis-inclusive pipeline

Cerul keeps visual evidence, the whole-video overview, and description retrieval
in the default indexing workflow. These products serve different purposes:

| Product | Input | Purpose |
| --- | --- | --- |
| Scene analysis | Silent 30-second video windows | Produce timestamped descriptions of visible actions and objects with source evidence. |
| Overview | Scene, transcript and screen-text records | Generate a title, summary, chapters and grounded query suggestions. Large inputs use a bounded hierarchy. |
| Description vectors | Published scene descriptions | Embed text in the retrieval space to improve natural-language scene search. This does not analyze the video again. |

## Scheduling

Within each video stream, OCR, speech and scene analysis start independently.
Speech windows, scene windows, description rows, and independent groups within
one overview level run concurrently. Video/text embedding waits for OCR and
speech; overview waits for scenes plus OCR and speech; description embedding
starts when scenes publish. The final overview level must wait for its partial
summaries. Authoritative publication, registry updates, stream/episode traversal,
and search projection updates remain ordered.

`--jobs` is a shared upper bound for in-flight model requests across these stages,
not a separate multiplier for each stage. `--rpm` is also shared. The default is
four model requests. OCR uses its existing bounded CPU workers. Shared frame
extraction is serialized and cached so consumers cannot decode the same cold
source concurrently. Successful units checkpoint independently; results are
sorted by source time before publication. Retry reuses completed units, including
those that finished out of order.

ETA follows the longest remaining dependency path. Cold estimates use work size
and configured concurrency; completed stages calibrate a configuration-specific
local timing cache. API queuing, rate limits, retries and shared-resource
contention can still make ETA inaccurate. Changing timing profiles does not
invalidate video annotations or model checkpoints.

## Reproducible concurrency check

Build the CLI, then run the opt-in integration check:

```sh
cargo build --locked
CERUL_TEST_BINARY="$PWD/target/debug/cerul" python3 -m unittest discover -s tests -p test_index_concurrency.py
```

The test generates a 301-second 64x64 synthetic video with audio and uses a local
HTTP server. The original version verified the shared limit, overlapping scene/speech work,
overlapping overview/vector work, concurrent groups, sorted publication,
zero-call cached replay, and recovery of one invalid scene window without
repeating completed speech or scene requests. No external model is called.

A macOS arm64 release-to-release check on 2026-09-14 used identical input bytes,
`--jobs 4`, no OCR, and fixed local response delays (scene 450 ms, speech 350 ms,
overview 600 ms, embedding 100 ms). Total invocation time was **30.10 seconds
before and 16.58 seconds after** (1.82x throughput, approximately 45% less time).
Both runs made 175 requests with peak concurrency four. Published scene,
transcript, summary and section records were identical. These are one-run local
scheduling measurements, not hosted-provider latency guarantees.

Fixed local delays isolate scheduling overhead. Such measurements do not predict
Gemini or another hosted provider's wall time. A real speed comparison should use
the same media, endpoints, models, jobs/rate limits, cache state and requested
outputs, and separately report upload, time to searchable data, and time to all
requested products completing. Model request events under `--json` expose actual
request latency, status and retry attempts without response payloads.

## Provider comparison (checked 2026-09-14)

[Twelve Labs describes Marengo](https://www.twelvelabs.io/marengo) as a native
multimodal embedding model. Its [Avid integration page](https://www.twelvelabs.io/integrations/twelvelabs-panel-for-avid-media-composer)
claims indexing at approximately 50 times real time, and separates Marengo search
from Pegasus summaries and chapters. This is a vendor claim for that integration,
not a controlled benchmark against Cerul or an end-to-end SLA including upload,
queueing, detailed scene records, OCR transcripts and every generated artifact.

[Azure Video Indexer's FAQ](https://learn.microsoft.com/en-us/azure/azure-video-indexer/faq)
does not promise one fixed indexing duration: it names media length, quality and
insight count as factors and recommends measuring representative user media.
Its [scale guidance](https://learn.microsoft.com/en-us/azure/azure-video-indexer/considerations-when-use-at-scale)
also identifies upload conditions and resolution as performance factors.

Further work includes benchmarking faster vision or native video embedding models. Changes to scene granularity,
sampling or retrieval products require quality evaluation; scheduler speedups
alone do not establish equivalent recall to a different provider.
