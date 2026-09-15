# Analyze videos

`analyze` explicitly generates timestamped visual scenes, a grounded overview and
chronological chapters. It runs directly on local videos, folders or LeRobot
datasets; search indexing is not a prerequisite.

```sh
cerul analyze ./video.mp4
cerul analyze ./video.mp4 --json
cerul analyze ./dataset --only 0
cerul analyze ./video.mp4 --dry-run
```

The configured vision endpoint handles silent video windows (or sampled frames
for compatible image-only endpoints). Current, validated OCR and transcript
sidecars are reused as evidence for the overview when available. This command
does not run transcription, OCR, embeddings or create a search index. Fresh
analysis is visual-only unless existing text evidence is available.

`--streams primary|all|a,b`, `--only`, `--jobs`, `--rpm`, and global `--recompute`
control selection and execution. Independent windows use bounded concurrency.
Cached successful work is reused; partial failures return exit code 6. Retrying
the same command repairs missing work. Cancellation preserves completed windows.
`--dry-run` lists selected streams without publishing analysis or calling models.

The sidecar retains `semantic.scene.jsonl`, `semantic.section.jsonl`, and
`semantic.summary.jsonl`, including source revisions and integer-microsecond
intervals. Overview and chapter publication share a validity fence. Invalid
references are rejected, not silently invented or truncated. Generation schemas
bound overview and section references before the response is validated locally.
Repeated content and wire references are compacted; public source IDs and revisions
are restored exactly. Overview output is limited to 12 sections and 800 summary
characters. Duplicate references are removed, and excessive unique references
trigger one bounded regeneration attempt. See `cerul diagnostics` for timings.

Human output contains scene/chapter counts, the overview, the sidecar location,
and elapsed processing time. `--json` returns records and errors conforming to
[the generated result schema](../schemas/analyze-result.json); stderr contains
progress and request diagnostics. An agent can inspect these results and select
relevant evidence, but this command does not implement an agent loop.

```sh
cerul status ./video.mp4 --timeline --type scene
cerul status ./video.mp4 --timeline --type summary
```

## Questions, references and time ranges

Any of `--prompt`, `--image`, `--from`, `--to` or `--stream` selects a focused
answer instead of the default scene/chapter/overview workflow.

```sh
cerul analyze ./video.mp4 --prompt "What happens after the door opens?"
cerul analyze ./video.mp4 --prompt "Does this object appear?" --image ./object.png
cerul analyze ./video.mp4 --from 00:30 --to 01:10 --prompt "Describe the interaction."
cerul analyze ./video.mp4 --prompt "Summarize the visible actions." --stream
cerul analyze ./video.mp4 --prompt "Find the target." --stream --json
```

`--question` aliases `--prompt`. Without a question, focused analysis describes
visible events and compares supplied references. Questions may contain up to
32,000 bytes. Repeat `--image` for up to four images, each at most 10 MB. Images
are decoded with memory/dimension limits and resized before sending. References
are comparison material, never proof that their contents appeared in the video.

Times accept seconds, duration units, `MM:SS` or `HH:MM:SS`, with fractional
seconds. The range includes `--from` and excludes `--to`; omitted boundaries use
camera coverage. Both boundaries must lie within the selected camera's coverage.
Results retain integer-microsecond episode timestamps: selecting 30–70 seconds
still cites 30–70, not 0–40. For LeRobot this is the episode timeline, not the
backing MP4 shard's absolute timeline.

Focused analysis uses at most 120 evenly distributed visual frames, selected
from approximately one-frame-per-second local extraction. Long ranges have
sparser coverage. The CLI measures the serialized model request (including base64,
references and protocol fields) and reduces sample count evenly when needed to
fit the 20 MB request budget. Returned timestamps describe the frames actually
sent. Brief events, continuous motion and absence cannot be reliably
established. Video samples are resized to 480 pixels on the longest edge;
references to 768. Valid cached OCR/transcript records fully inside the interval
are included up to 48 KB. No fresh transcription or OCR is run. Narrow the range
for more detailed visual evidence.

Structured responses include the answer, evidence at actual sampled timestamps,
limitations, selected range, sample timestamps, source/reference hashes and model
identity. Evidence timestamps are validated against the samples. This checks
provenance and format, not factual accuracy. The response has a fixed schema;
arbitrary user-supplied JSON schemas are not implemented.

### Streaming and saved results

`--stream` uses real SSE generation with Gemini or compatible OpenAI Chat
Completions endpoints. Human output prints decoded answer text as it arrives,
with an episode/camera heading and a blank line between different sources.
With `--json`, provisional `analysis_delta` events go to stderr and stdout
receives one complete final report. Unsupported streaming endpoints fail
explicitly; there is no simulated streaming.

Incremental text is provisional until generation finishes normally and passes
local validation. Interrupted, refused or malformed responses are not published
or cached as successful answers. Streaming is not retried after response data
begins, avoiding duplicated output.

Focused responses are atomically saved in the selected stream's
`analysis/requests/<cache-key>.json` directory. Cache identity includes source,
prompt, range, reference content, model endpoint, text evidence and sampling
recipe. Changes create separate answers; whole-video scene/section/summary files
remain intact. Repeating a request reuses its saved answer; streaming a cache hit
emits one delta with `cached: true`. Use `--recompute` to refresh the answer.

## Command boundaries

- `index`: build video, OCR and speech retrieval data.
- `search`: retrieve moments using available saved tracks.
- `analyze`: generate scenes/overview or answer a question about selected footage.
- `annotate`: produce embodied semantic labels, optionally with local hands.

Previously saved analysis remains compatible. Existing description vectors remain
searchable, but neither ordinary `index` nor `analyze` regenerates that optional
legacy projection. Old general annotation records are preserved; new annotation
uses embodied defaults and a distinct prompt/cache identity.

## Comparison with TwelveLabs Analyze

Checked against official documentation on 2026-09-15. This compares interfaces,
not measured accuracy, speed or cost. No TwelveLabs calls were made.

| Capability | Cerul analyze in this change | TwelveLabs Analyze |
| --- | --- | --- |
| Entry point | Local CLI and Rust library | Hosted API and SDKs |
| Source | Local files, folders and LeRobot cameras | Asset ID, media URL or base64 video |
| Model | Configurable vision endpoint, Gemini by default | Pegasus 1.5 in the documented API |
| Request | Scenes/overview, custom questions and reference images | User prompt, including questions; reference-image prompts |
| Selected time range | Episode-relative start/end | Start/end timestamps |
| Response | Real incremental answer text and fixed structured JSON with evidence | Generated text, optional structured JSON, streaming |
| Long-running work | Foreground command, local checkpoints and retry | Synchronous, asynchronous tasks and batch endpoints |
| Custom segmentation | Not implemented | Asynchronous segmentation with custom definitions |
| Search prerequisite | None | Analysis can accept video directly, without search indexing |
| Embodied dataset output | Separate annotate command | Not an equivalent LeRobot annotation workflow in the cited API |

TwelveLabs documents synchronous analysis for videos up to one hour and async
analysis up to two hours. Cerul has no equivalent benchmarked service limit or
SLA; supported media are processed in bounded windows and latency depends on the
configured endpoint. Do not equate windowing with unlimited supported duration.

Sources: [Analyze endpoints](https://docs.twelvelabs.io/api-reference/analyze-videos),
[synchronous analysis](https://docs.twelvelabs.io/api-reference/analyze-videos/analyze),
and [analysis guide](https://docs.twelvelabs.io/docs/guides/analyze-videos).
