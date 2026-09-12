# Cerul CLI

Implementation reference for the local video processing core. User commands and
installation instructions are in the [documentation](docs/README.md).

**Purpose:** `cerul` turns video into searchable, annotated data suitable for downstream training workflows. Users supply their own model keys. No Cerul account is required.

## 1. Decisions

| Area | Decision |
| --- | --- |
| Language | Rust, one crate with a library and one binary. ffmpeg subprocesses, four endpoint types, LanceDB, and embedded OCR. No PyTorch, GPU runtime, Python runtime, or plugins. |
| Commands | `index`, `search`, `status`, `open`, `auth`, `annotate`, `remove`. HTTP and MCP serving are not implemented. |
| Structure | Logic lives in library modules exposed through `lib.rs`. `main.rs` parses arguments, calls the library, and prints results. Desktop can link the library or consume subprocess JSON events without `serve`. |
| Source of truth | One sidecar directory per episode, using JSONL and Parquet, **including embedding vectors**. Indexes are caches rebuildable from sidecars without model calls. |
| Indexes | One LanceDB directory per `space_id`, the hash of provider kind, base URL, model, dimensions, and query instruction template. The same model name at different endpoints is a different space. Queries must match exactly. |
| Endpoints | Embedding defaults to Gemini Embedding 2 at 3072 dimensions. Embedding, vision, and optional transcription support configurable `kind = gemini \| openai` endpoints with user keys. `perception` is reserved and processing is not implemented. Missing perception must not block indexing/search. |
| Default models | Required Gemini embedding: `gemini-embedding-2` at 3072 dimensions. Vision: `gemini-3.8-flash`. Optional ASR preset: `gemini-3.5-transcribe`. |
| OCR | Embedded PP-OCRv6 small, approximately 31 MB of weights, CPU inference through `tract-onnx`, enabled by default. This is the only embedded model and the only exception to endpoint-based inference. |
| Retrieval | One compatible multimodal space with independent video, transcript, and screen-text rows. Per-track candidates use separate budgets, then raw maximum similarity per interval. The embedding endpoint must accept video and images; unsupported endpoints fail without a text-only fallback. Description vectors and a separate lexical cache are available for evaluation; their default fusion remains gated on held-out evidence. Exact strings use `--text` substring matching. Annotation filtering happens before vector search. |
| Annotations | Three fixed families: `semantic`, `grounding`, `world`. Subtypes may grow. All derived records are annotations. |
| Time | Integer microseconds, half-open intervals, episode-relative time derived from PTS. |
| Cameras | Process the primary camera by default; `--streams all` expands selection. |
| Language | Annotation text fields are English. |
| LeRobot | Read v3.0 and v3.1 and produce sidecars. Writeback accepts only an existing compatible v3.1 dataset and writes subtask language entries. v3.0 writeback is unsupported; Cerul does not upgrade formats. Events and flags remain in sidecars. |
| Platforms | macOS arm64 and Linux x86_64. Windows is not supported. |
| Versions | Increment `v0.0.x` without alpha/beta suffixes. Cargo.toml, packaging/dist.toml, and release tags must agree. Version numbers identify releases, not roadmap milestones. |
| Distribution | GitHub Releases with a cargo-dist shell installer. Homebrew formula and npm wrapper artifacts are generated; registry/tap publication is separate. See [release artifacts](docs/development/releases.md). |
| Telemetry | None. |
| Scope | Video retrieval and semantic annotation require only third-party model credentials. `status --providers` probes only the implemented endpoints; the reserved perception endpoint is contacted solely when configured explicitly. Pose, depth, and segmentation depend on later perception services and must not be advertised as available. |

## 2. Reader guides

Use the [installation guide](docs/installation.md) and
[video tutorial](docs/video-search.md) for runnable workflows, and
[configuration](docs/configuration.md) for endpoint setup, credentials, and
runtime settings. [ARCHITECTURE.md](ARCHITECTURE.md) maps the source modules.
The command sections below define behavioral contracts; use `cerul --help`
and command-specific help for the executable argument reference.

## 3. Command contracts

Global options: `--json`, `--workspace`, `--recompute`, `--dry-run`, `--yes`, `-q`, `-v`. Configuration overrides use repeated `--set KEY=TOML_VALUE`.

The start page exposes both search and annotation workflows, even after setup. An annotation invocation without arguments displays usage with concrete video and LeRobot examples, supported types, and output locations; JSON argument errors remain machine-readable. See the [annotation guide](docs/annotation.md).

Without `--json` the CLI renders text for people: a start page for a bare `cerul` that walks through setup until the first video is searchable, live progress on a terminal (one line per finished station when redirected), one card per matched video listing its moments, and errors with a recovery hint on stderr. Cards carry the file name, match percentage, time range, excerpt, and an OSC 8 link; terminals with the iTerm2 or Kitty graphics protocol also show a still frame. When a player that accepts a start time is installed the link opens the moment rather than the file. Model requests whose duration cannot be predicted show a spinner. `index` prints the stations it will run before the first request, and collapses each finished video into one line so the live area stays the size of the video being worked on. Colour, links, and images follow the terminal and `NO_COLOR`, so redirected output stays plain. `cerul auth` reports the saved Gemini key status; `cerul auth set` and `cerul auth remove` manage it. Rendering lives in the binary only; the library never touches a terminal.

In JSON mode, stdout contains exactly one final JSON object. Each stderr line is a JSON event, for example:

~~~json
{"event":"progress","episode":"dataset/12","station":"embed","done":37,"total":120}
{"event":"log","level":"info","msg":"Indexing started"}
~~~

JSON-mode `model_request` events capture per-attempt HTTP status, duration,
retry identity, and provider-reported Gemini token usage, without request or
response payloads. Missing usage remains unknown. The library exposes an
opt-in observer; the CLI emits these diagnostics only in JSON mode. See the
[agent event contract](docs/agent.md#events).

Exit codes: 0 success; 2 arguments/configuration; 3 missing dependency or unsupported capability; 4 execution failure; 5 cancellation; 6 partial success.

### `index <path>...`

Accept files, directories, and automatically detected LeRobot v3 datasets.

| Options | Behavior/default |
| --- | --- |
| `--chunk 30s`, `--overlap 5s` | Default units stay within 32 seconds to retain Gemini's 1 fps sampling density. This is not a hard endpoint duration limit. |
| `--no-audio`, `--no-ocr`, `--skip-still` | Transcribe when an audio track is present; OCR is enabled by default. Skip embedding stationary units when requested. |
| `--streams primary\|all\|a,b` | Default: primary. |
| `--only SEL` | Default: all episodes. |
| `--jobs 4`, `--rpm N` | Bound remote concurrency and request rate. Local OCR uses at most --jobs workers and the available CPU count; results remain ordered by source time. |
| `--sidecar-dir DIR` | Override sidecar placement. Unwritable ordinary media use workspace `sidecars/<sha256>/`. Unwritable LeRobot datasets use `sidecars/<dataset_id>/<episode_index>/`; their stable identity is cached under workspace `datasets/` using the canonical source path. |

Pipeline: discover → SHA-256 → ffprobe → transcript/screen_text/embed stations → Lance publication. Skip completed outputs when content, model, and parameters match.

### `remove [path...] [--cache] [--all-indexes] [--index SPACE_ID] [--compact]`

One command gets rid of things, because from a person's side forgetting a video
and reclaiming disk are the same verb. Named paths delete those episodes'
sidecars and the projections built from them, leaving the media untouched;
interactive runs confirm first, `--yes` skips the question, and non-terminal
callers must pass it. A path that was never indexed is reported, not an error.
The flags free only regenerable artifacts and never ask, because nothing is
lost: caches are recreated on demand and search indexes rebuild from sidecars
with no model calls, which is covered by the [release acceptance checks](docs/development/validation.md#release-acceptance). Status
lists space IDs. Everything supports `--dry-run`. Sidecar deletion records
durable intent before removing index rows and files; if interrupted, repeating
the command resumes even when the sidecar is partially removed or absent. Dry
runs never record intent or delete data.

### `completions <shell>`

Prints a completion script for bash, zsh, fish, elvish, or powershell to stdout.

### `open [N]`

Plays result `N` (default 1) from the last search, starting at its moment. The
last search is remembered in workspace `cache/last-search.json`. Players are
tried in order: mpv, IINA, VLC, ffplay, then the system handler, which cannot
start at a timestamp and says so.

### `annotate <path>... --semantic [items] --grounding [items] --world [items]`

| Options | Behavior/default |
| --- | --- |
| `--semantic` | LeRobot: task, subtask, event, interaction, state, flag, progress. Ordinary video: task, subtask, flag; the other four are explicit selections. |
| `--grounding` | Reserved; rejected as unsupported. |
| `--world` | Reserved; rejected as unsupported. |
| `--streams`, `--only`, `--ontology FILE`, `--window 30s`, `--fps 2` | LeRobot defaults to `cerul.verbs.v1` with verb validation. Ordinary videos have unrestricted verbs unless an ontology is supplied. |
| `--write-lerobot`, `--out DIR` | No dataset mutation by default. Writeback accepts only an existing compatible v3.1 dataset. |

Each subtype is a module: frames → timestamped contact sheet → schema-constrained model response → staging → validation → annotation publication → records index update. Invalid modules publish nothing; independent modules can still succeed.

### `search [query]`

| Options | Behavior/default |
| --- | --- |
| `--image PATH` | Supply a text query, image, or filter. Filters alone skip embeddings and return records in time order. |
| `--limit 10`, `--threshold X` | No default score threshold. |
| Repeated `--filter k=v` | Keys: `<annotation>.<field>`, episode, stream, kind. Operators: `= != > < ~`. Convert filters to temporal ranges before vector ranking; do not filter only the top-k results. |
| `--in PATH` | Restrict the searched input scope. |
| `--count` | Filters only: return exact-label record and episode counts. No natural-language probe counting. |
| `--save DIR`, `--pad 2s` | Save matched MP4 clips. |
| `--preview`, `--no-preview` | Cache one still frame per hit under workspace `cache/previews/` and expose it as `preview`. Stills use input seeking, so their cost does not grow with recording length. Default: on when the terminal can draw images. |
| `--rerank` | Reserved; rejected as unsupported. |
| `--text` | Substring match over transcript and screen text without vectors. |

Query embeddings are cached under workspace `cache/queries/` keyed by embedding space and query, so repeating the exact query in the same space reuses its vector; changing filters or the result limit does not embed the query again. Changing the query text requires a new embedding. An expired capability cache can still trigger a provider probe. Both caches are disposable and freed by `remove --cache`.

Results: `hits[]` containing episode, stream, start_us, end_us, optional frame_range, score, matched (video/speech/screen), excerpt, and annotations.

### `status [path]`

Return a workspace overview or an episode's annotation inventory. Running `cerul` without a command is equivalent to status.

Ordinary status does not probe remote endpoints; remote capabilities are null (unknown). `--providers` probes embedding, vision, and enabled transcription, plus perception only when explicitly configured, caching successful checks for seven days. `--recompute` refreshes checks; `--dry-run` performs none. Explicit probes report endpoint, model, check time, and error. Unsupported is false; missing keys and network errors remain null. Preserve other endpoint results when one fails and return exit code 6. The JSON root contains `capabilities`. Perception's advertised tasks do not imply that this version implements grounding/world.

## 4. Configuration and models

Configuration resolution and examples are maintained in the
[configuration guide](docs/configuration.md). Credentials must be scoped to the
selected endpoint; configuration stores key environment variable names, not
secret values. Provider adapters must preserve these request contracts:

| Capability | Gemini | OpenAI-compatible |
| --- | --- | --- |
| Embedding | `embedContent`, outputDimensionality=3072. Text query instruction: `task: search result \| query: {query}`. | `/v1/embeddings` with text, image, and video support. Reject text-only endpoints. |
| Vision | `generateContent` with responseSchema. | `/v1/chat/completions` with image_url and json_schema. |
| Transcription | Native word timestamps for `gemini-3.5-transcribe`; prompted segments for other Gemini models. | `/v1/audio/transcriptions`, verbose_json, segment timestamps. |

Probe the remote calls actually needed, not command names. Index probes embedding only for pending vectors and transcription only for pending audio. Annotate probes selected vision/perception capabilities. Semantic search can refresh an expired embedding capability probe even when its query vector is cached. Filters alone, exact text, sidecar rebuilds, ordinary status, and cleanup make no probe calls.

Embedding probes send a sentence, image, and two-second video and check dimensions/modalities. Vision requests `{"ok":true}` from a 64×64 image. Transcription uses one second of silence. Perception reads `/capabilities`. Explicitly unsupported required capabilities fail with code 3 before processing media. Temporary network failures allow local probe/frame/OCR stations to finish, mark remote work incomplete, and return code 6; a later run fills the gaps.

Print a notice before the first media request to each endpoint; `--yes` suppresses it. Requests are inline. Check the actual encoded request byte size against endpoint limits (Gemini: 20 MB); reduce bitrate, then split if needed.

## 5. Storage

~~~text
videos/
  demo1.mp4
  demo1.mp4.cerul/
    episode.json
    transcript.jsonl
    screen_text.jsonl
    semantic.subtask.jsonl
    embeddings/<space_id>.parquet    # Authoritative vectors and ranges.
    embeddings/<space_id>.json       # Public space metadata, no keys.
    streams/<stream_id>/             # Non-primary stream annotations.
    staging/
    log.jsonl

my_dataset/
  meta/ data/ videos/
  .cerul/dataset.json                # UUID, root, info.json hash.
  .cerul/episodes/000012/             # Per-episode sidecar.

~/.cerul/
  config.toml
  providers.json
  runtime/lock
  registry.jsonl                    # Input paths and media hash -> sidecar.
  cache/<sha256>/proxies/<recipe_hash>.mp4
  cache/<sha256>/proxies/<recipe_hash>.json
  cache/<sha256>/contact/<recipe_hash>.mp4
  cache/<sha256>/contact/<recipe_hash>.json
  datasets/                         # Read-only dataset identity cache.
  sidecars/<sha256>/                # Ordinary-video fallback.
  sidecars/<dataset_id>/<local_id>/ # LeRobot fallback.
  index/<space_id>/chunks.lance
  index/<space_id>/records.lance
~~~

Embedding Parquet rows contain stream, kind, start_us, end_us, vector, and params_hash. The adjacent JSON records kind/base_url/model/dims/query_template for status inspection. Primary annotations remain at the sidecar root; non-primary annotations cannot overwrite them.

Proxy metadata stores the recipe and output hash; missing/corrupt files are rebuilt. Contact proxy metadata additionally preserves original source-relative PTS for each sampled frame. It must not substitute the proxy encoder's frame clock.

One workspace writer holds `runtime/lock`; contention returns code 4. LeRobot directory locks coordinate different workspaces: shared for reads and output copies, exclusive for in-place replacement and recovery. Read-only dataset directories need no new lock file.

Publication and recovery rules:

1. Write outputs to temporary staging files, validate, then atomically rename to their final location. No incomplete artifact occupies a final path.
2. Index and annotate are idempotent: skip completed stations and resume missing work. If sidecars exist but Lance rows are absent, restore the index with zero model calls. `--recompute` rewrites complete artifacts. Persist validated, cache-keyed per-unit checkpoints so successful units survive interruptions. Replace indexed rows by episode, stream, and artifact version, including removal of stale rows. An index update failure preserves valid sidecars; subsequent commands resynchronize the corresponding projection.


## 6. Data model

**Episode:** one recording, one or more streams, and a common timeline. The CLI recognizes LeRobot episodes from metadata; each other video is a single-stream episode. User-authored multi-camera episode directories are not implemented.

**Identity:** `episode_id = <dataset_id>/<local_id>`. For ordinary video, dataset_id is the first 16 characters of video SHA-256 and local_id is 0. LeRobot uses the persistent UUID in .cerul/dataset.json and the original episode_index. Identically numbered episodes in different datasets cannot collide.

**Time:** source time `t_src` is MP4 PTS. For the primary stream, `t_ep = t_src - range_us[0]`; other streams also use their affine mapping a,b_us. Model clip time maps as `t_ep = clip_start_us + t_clip`. Annotations and chunks store only episode time. Example: an episode occupies source seconds 120–150; a model timestamp of 5 seconds becomes episode second 5, corresponding to source second 125.

An episode schema includes:

- $cerul: episode/1; episode_id, dataset_id, local_id.
- Video streams: id, kind=video, primary, SHA-256, path, source range_us, and probe (duration_us, fps, width, height, has_audio, codec).
- Parquet streams: id, kind=parquet, path, selected columns such as observation.state/action, and row_range.
- Time reference and mappings: a, b_us, status (calibrated/estimated/unknown).
- Task and source format/root.

An annotation JSONL file starts with a header containing $cerul=annotation/1, name, episode, stream, model kind/name, params, created, cerul_version, input_hash, and record_schema. Remaining lines are records. Common fields are id, start_us, end_us, and confidence (uncalibrated model self-report). Readers accept older schema versions and treat absent optional fields as null. Breaking changes increment the major schema version and use a new filename.

| Family | Record subtypes and fields |
| --- | --- |
| semantic | task{text}; subtask{text,index}, with continuous coverage; event{verb,objects[],actor,outcome}; interaction{hand,object,contact}; state{object,attribute,before,after}; flag{kind,note}; progress{value,done}. |
| grounding (reserved; not generated) | Coordinates normalized to [0,1] with frame_w/h. box{t_us,label,xyxy,track_id?}; affordance{t_us,label,points,action_hint}; trace{points[[t_us,x,y]],subject,label}; keypoint; mask{rle}. |
| world (reserved; not generated) | Frames use T_<a>_from_<b>, units m or relative, xyzw quaternions, valid flags, null for missing values. camera{t_us,T_world_from_camera[7],intrinsics?,scale,valid}; hand{t_us,side,joints[21][3],valid}; object; depth{t_us,path,scale}; points. |

**Index unit:** a 30-second video-stream interval in episode time, with up to three rows of kind video, speech, or screen. Write vectors to sidecars before indexing.

Chunks columns: id, episode, stream, kind, start_us, end_us, vector[3072] for the default space, text, still, space_id, params_hash. Records columns: episode, stream, annotation, id, start_us, end_us, fields. Both tables are rebuildable from sidecars.

Default `cerul.verbs.v1` vocabulary: reach, grasp, regrasp, lift, carry, place, release, push, pull, open, close, insert, rotate, pour, wipe.

## 7. Required behavior

### Index stations

- **Transcript:** sixty-second audio windows, with a shorter final window; records contain start_us, end_us, text, lang.
- **Screen text:** PP-OCRv6 detection on shared 1 fps source samples, with a 1080-pixel maximum long edge. A conservative image-change gate skips nearly identical frames, with recognition at least every five seconds. Recognition runs only on detected boxes. Rust implements DBNet postprocessing and CTC decoding; repeated consecutive text is merged. Do not use a 480p proxy for OCR.
- **Frames and proxies:** Indexing caches content-addressed 1 fps source samples (maximum 1080-pixel long edge), shared by OCR, embedding proxies, and understanding. Sampling emits pixels and integer-microsecond source PTS from one decode, without a preliminary full-frame timestamp scan. Streams with negative origins decode from the beginning because input seeking can discard their negative prefix. Cache 480p, 1 fps H.264 proxies with no audio. Original source timestamps are retained in the sample manifest. Contact sheets default to 2 fps but honor --fps, sampling source frames before encoding and preserving their original PTS separately. Sampling and both proxy recipes version these changes; the next index or annotation run refreshes dependent checkpoints. Existing published sidecars remain available for offline search and index rebuilds.
- **Scheduling:** OCR and ASR run concurrently; base embeddings wait for both. A failed remote station retains completed local evidence.
- **Embedding:** one video row and bounded text rows per unit. Deduplicate whitespace-equivalent OCR lines only in derived retrieval text; preserve case, punctuation, and raw sidecars. Split oversized text without inventing finer timestamps. Gemini Embedding 2 text documents use `title: none | text: {text}`. Measure the actual encoded body against the endpoint limit; reduce bitrate then split, failing explicitly if still oversized. Persist vectors before Lance. Log failures and retry only missing units.
- **Understanding:** default-on 30-second silent windows produce typed scenes, then a bounded hierarchical overview produces sections, title, coverage, and at most three grounded suggestions. Gemini uses video; compatible image-only vision endpoints receive timestamped JPEG frames. Actual sample times are retained. `--no-understanding` or `vision.enabled = false` records an explicit skip. Missing or failed vision does not discard searchable base vectors. Scene IDs identify source/stream/interval; generation revisions are separate. Corrections are a separate revision layer, never overwritten by generation.
- **Derived storage:** scene, section, and summary records use `annotation/1`; dependent references pin record revisions. Description vectors are separately published under `embeddings/descriptions/<space>/<generation>.parquet` with per-stream `description.<space>.json` manifests. They preserve each scene's original interval and do not enter default retrieval yet. Workspace `lexical/` is a rebuildable FTS cache of original OCR/ASR with a tokenizer recipe independent of embedding space. Both diagnostic projections rebuild without model calls.
- **Cache identity:** episode_id, stream, stream_sha256, range_us, time mapping, station, space_id or model, params_hash, station version. Identity distinguishes datasets; content hashes detect replacement; ranges distinguish episodes sharing a shard.

### Semantic annotation

Sample at --fps (default 2), use a five-column contact grid, and burn **clip-relative** timestamps into frames. Each window starts at zero. Add clip_start_us exactly once to returned model times.

Subtask annotation describes first, then segments. Define boundaries using holding, release, arrival, and state change. Windows overlap by five seconds; conflicting outputs produce flags. Snap boundaries to real frame PTS, require continuous subtask coverage, validate ontology verbs where applicable, and keep normalized coordinates within [0,1].

### LeRobot writeback

The CLI writes only subtask language entries to an already compatible v3.1 dataset. v3.0 returns code 3 and directs users to official format tooling; Cerul performs no upgrade. The pinned upstream recorder/compatibility-fixture distinction is documented in docs/lerobot.md and must not be misrepresented as an available official upgrade command.

Use language_persistent with style=subtask, exactly one active subtask per frame, and timestamps from the source Parquet frame values without recalculation. Preserve action, state, tasks, language_events, and all non-subtask language_persistent entries. Add or replace only subtask entries. Retain info.json content, updating only necessary language feature metadata when adding a column. Events/flags stay in sidecars because they have no defined official style.

Prefer --out. Stage a complete dataset, perform native field/timeline validation, then publish. Release acceptance additionally uses the official Python loader to read all sample frames; Python is an acceptance dependency, not a runtime dependency. In-place writeback requires recoverable file replacement records and must preserve unselected episodes in shared shards. Compare all affected shards' protected fields and existing annotations, not just one sampled frame.

### Search

Parse filters into episode/stream half-open time intervals, then apply them as Lance prefilters to intersecting chunks before vector ranking. Each present base track receives its own candidate budget. Group the same interval by highest score and retain raw per-track scores, ranks, IDs, and original intervals in JSON. These are cosine scores, not confidence probabilities. Exact-text hits merge when adjacent; ranked vector hits merge only when their windows mostly coincide, so the small overlap between neighbouring index windows never chains a whole video into one hit. Reranking is not implemented. Search reuses saved Lance projections and reads authoritative annotation files once; it does not rescan Parquet or rebuild an annotation projection on every query. A valid query cache hit makes no capability probe or embedding call.

Repeated filters are AND. Conditions on one annotation must match the same record. Different annotations join by temporal intersection. Episode/stream/kind constrain scope. A chunk is eligible when its interval intersects the event interval; the hit time is that intersection.

Without a filter, search the whole selected space. Filters alone skip vectors and return time-ordered records. A query space mismatch returns code 3. Missing required annotations return a capability error, not an empty result.

## 8. Repository structure

The [architecture map](ARCHITECTURE.md) assigns module responsibilities and
explains where guides, runtime prompts, generated schemas, assets, and build
configuration belong. Keep one root Cargo.toml with a library and a binary.

Cargo.toml declares the supported dependency configuration; Cargo.lock pins the
resolved versions. FFmpeg runs as a separately licensed subprocess.

Measure binary and archive sizes and CPU throughput on both targets during release acceptance.

Every PR runs offline tests and two-platform builds. Maintainers run small real Gemini checks locally with their own credentials as part of the [release checklist](docs/development/releases.md). CI does not require a model key.

## 9. Validation and scope

[Developer validation](docs/development/validation.md) defines behavioral gates,
real OCR and provider checks, official-loader round-trips, and release acceptance.
The [release guide](docs/development/releases.md) describes packaging and publication.

[Scope and current limits](docs/development/scope.md) separates unavailable
capabilities from work excluded from this repository.

Do not add speculative interface layers before a fifth station, second embedding
provider, or fourth annotation family creates a concrete need.

## Optional speech configuration

`cerul config` selects Gemini, Groq, OpenAI, a custom OpenAI-compatible ASR, or
Disabled. The CLI resolves unspecified ASR to enabled when its configured key is
available (Gemini by default), without a model or credential chooser during index.
Explicit endpoints and Disabled are preserved. Compatible cached transcripts remain
usable without credentials; new unconfigured speech is skipped in machine mode. Credentials are
private and separate from model configuration. ASR failure retains searchable
video/OCR vectors, records a diagnostic, and returns partial success. Repeating
index resumes missing speech windows without recomputing completed OCR.

## CLI index presentation

Selecting Index and a source in the interactive guide starts work directly;
`--dry-run` remains an explicit flag. The library emits initial and incremental
station progress; only the binary renders bars and stage estimates. Estimates
exclude an initial cached position and remain indeterminate without sufficient
measured work. Completion output keeps full paths in copyable commands and JSON,
while shortening display names.

Index reports may include up to three source-referenced suggestions per episode.
They use the current generated summary when available and extractive evidence
otherwise; collecting saved suggestions makes no model calls. Missing speech,
disabled ASR, no audio, successful empty transcription, and failures remain
distinguishable. The [hybrid retrieval plan](docs/design/hybrid-video-search.md)
records the remaining evaluation gates; calibrated fusion, description retrieval,
and automatic ANN selection are not yet default behavior.
