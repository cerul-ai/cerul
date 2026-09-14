# Index performance

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
HTTP server. It verifies the shared limit, overlapping scene/speech work,
overlapping overview/vector work, concurrent groups, sorted publication,
zero-call cached replay, and recovery of one invalid scene window without
repeating completed speech or scene requests. No external model is called.

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

Potential further optimizations include exact-input embedding reuse, compacting
redundant overview context while preserving source references, and benchmarking
faster vision or native video embedding models. Changes to scene granularity,
sampling or retrieval products require quality evaluation; scheduler speedups
alone do not establish equivalent recall to a different provider.
