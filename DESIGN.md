# Cerul CLI

Public design reference for the local video processing core.

**Purpose:** `cerul` turns video into searchable, annotated data suitable for downstream training workflows. Users supply their own model keys. No Cerul account is required.

## 1. Decisions

| Area | Decision |
| --- | --- |
| Language | Rust, one crate with a library and one binary. ffmpeg subprocesses, four endpoint types, LanceDB, and embedded OCR. No PyTorch, GPU runtime, Python runtime, or plugins. |
| Commands | M1: `index`, `search`, `status`, `open`, `auth`, `annotate`, `remove`. HTTP and MCP `serve` belong to M2. |
| Structure | Logic lives in library modules exposed through `lib.rs`. `main.rs` parses arguments, calls the library, and prints results. Desktop can link the library or consume subprocess JSON events without `serve`. |
| Source of truth | One sidecar directory per episode, using JSONL and Parquet, **including embedding vectors**. Indexes are caches rebuildable from sidecars without model calls. |
| Indexes | One LanceDB directory per `space_id`, the hash of provider kind, base URL, model, dimensions, and query instruction template. The same model name at different endpoints is a different space. Queries must match exactly. |
| Endpoints | `embedding`, `vision`, and `transcription` support `kind = gemini \| openai` with user keys. `perception` is reserved and processing is not implemented. Missing perception must not block indexing/search. |
| Default models | One Gemini key: `gemini-embedding-2` at 1536 dimensions and `gemini-3.8-flash`. |
| OCR | Embedded PP-OCRv6 small, approximately 31 MB of weights, CPU inference through `tract-onnx`, enabled by default. This is the only embedded model and the only exception to endpoint-based inference. |
| Retrieval | One multimodal space. Each 30-second unit has up to three independently embedded rows: video, transcript, screen text. The embedding endpoint must accept video and images; unsupported endpoints fail, without a text-only fallback. No BM25 or fusion. Exact strings use `--text` substring matching. Annotation filtering happens before vector search. |
| Annotations | Three fixed families: `semantic`, `grounding`, `world`. Subtypes may grow. All derived records are annotations. |
| Time | Integer microseconds, half-open intervals, episode-relative time derived from PTS. |
| Cameras | Process the primary camera by default; `--streams all` expands selection. |
| Language | Annotation text fields are English. |
| LeRobot | Read v3.0 and v3.1 and produce sidecars. Writeback accepts only an existing compatible v3.1 dataset and writes subtask language entries. v3.0 writeback is unsupported; Cerul does not upgrade formats. Events and flags remain in sidecars. |
| Platforms | M1: macOS arm64 and Linux x86_64. Windows is M2. |
| Versions | Increment `v0.0.x` without alpha/beta suffixes. The old npm package reached 0.0.2; the first rewritten release is **v0.0.3**, with the same version on crates.io. Milestones M1/M2/M3 are not fixed version numbers. |
| Distribution | cargo-dist shell installer, Homebrew formula, and npm `cerul` wrapper. |
| Telemetry | None. |
| Scope | M1 is a complete video retrieval and semantic annotation CLI requiring only third-party model credentials. `status --providers` probes only the endpoints M1 uses; the later-milestone perception endpoint is contacted solely when configured explicitly. Pose, depth, and segmentation depend on later perception services and must not be advertised as available. |

## 2. Usage

The intended release installation entry point is shown below. It must be connected and verified before being advertised as live.

~~~sh
curl -fsSL https://cerul.ai/install.sh | sh
export GEMINI_API_KEY=...

cerul index ./videos
cerul search "Pick up the cup, then put it on the table"
cerul search --image ref.png --save ./clips
cerul search --text "ECONNREFUSED"
cerul annotate ./videos --semantic
cerul search --filter semantic.event.verb=regrasp
cerul status
~~~

Complete distributions install `cerul-ffmpeg` and `cerul-ffprobe` beside the CLI. Source-only builds can use ffmpeg/ffprobe 6.0+ on PATH; explicit `CERUL_FFMPEG` and `CERUL_FFPROBE` overrides take precedence. The default workspace is `~/.cerul/`, overridden by `--workspace` or `CERUL_WORKSPACE`. Source-build instructions are in README until publication is verified.

## 3. Commands

Global options: `--json`, `--workspace`, `--recompute`, `--dry-run`, `--yes`, `-q`, `-v`. Configuration overrides use repeated `--set KEY=TOML_VALUE`.

Without `--json` the CLI renders text for people: a start page for a bare `cerul` that walks through setup until the first video is searchable, live progress on a terminal (one line per finished station when redirected), one card per matched video listing its moments, and errors with a recovery hint on stderr. Cards carry the file name, match percentage, time range, excerpt, and an OSC 8 link; terminals with the iTerm2 or Kitty graphics protocol also show a still frame. When a player that accepts a start time is installed the link opens the moment rather than the file. Model requests whose duration cannot be predicted show a spinner. `index` prints the stations it will run before the first request, and collapses each finished video into one line so the live area stays the size of the video being worked on. Colour, links, and images follow the terminal and `NO_COLOR`, so redirected output stays plain. `cerul auth [set|remove]` manages the saved Gemini key. Rendering lives in the binary only; the library never touches a terminal.

In JSON mode, stdout contains exactly one final JSON object. Each stderr line is a JSON event, for example:

~~~json
{"event":"progress","episode":"dataset/12","station":"embed","done":37,"total":120}
{"event":"log","level":"info","msg":"Indexing started"}
~~~

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
with no model calls, which is what acceptance criterion 7 exercises. Status
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
| `--grounding` | M2: box and affordance. Mask, track, and keypoint require perception and are later capabilities. |
| `--world` | Camera, hand, object, depth, points; all require perception, M2 onward. |
| `--streams`, `--only`, `--ontology FILE`, `--window 30s`, `--fps 2` | LeRobot defaults to `cerul.verbs.v1` with verb validation. Ordinary videos have unrestricted verbs unless an ontology is supplied. |
| `--write-lerobot`, `--out DIR` | No dataset mutation by default. Writeback accepts only an existing compatible v3.1 dataset. |

Each subtype is a module: frames → timestamped contact sheet → schema-constrained model response → staging → validation → annotation publication → records index update. Invalid modules publish nothing; independent modules can still succeed.

### `search [query]`

| Options | Behavior/default |
| --- | --- |
| `--image PATH` | Supply a text query, image, or filter. Filters alone skip embeddings and return records in time order. |
| `--limit 10`, `--threshold X` | No default score threshold in M1. |
| Repeated `--filter k=v` | Keys: `<annotation>.<field>`, episode, stream, kind. Operators: `= != > < ~`. Convert filters to temporal ranges before vector ranking; do not filter only the top-k results. |
| `--in PATH` | Restrict the searched input scope. |
| `--count` | Filters only: return exact-label record and episode counts. No natural-language probe counting. |
| `--save DIR`, `--pad 2s` | Save matched MP4 clips. |
| `--preview`, `--no-preview` | Cache one still frame per hit under workspace `cache/previews/` and expose it as `preview`. Stills use input seeking, so their cost does not grow with recording length. Default: on when the terminal can draw images. |

Query embeddings are cached under workspace `cache/queries/` keyed by embedding space and query, so repeating or refining a search needs no further model call. Both caches are disposable and freed by `remove --cache`.
| `--rerank` | M2: vision reranking of the first 20 candidates. |
| `--text` | Substring match over transcript and screen text without vectors. |

Results: `hits[]` containing episode, stream, start_us, end_us, optional frame_range, score, matched (video/speech/screen), excerpt, and annotations.

### `status [path]`

Return a workspace overview or an episode's annotation inventory. Running `cerul` without a command is equivalent to status.

Ordinary status does not probe remote endpoints; remote capabilities are null (unknown). `--providers` probes all four endpoints, caching successful checks for seven days. `--recompute` refreshes checks; `--dry-run` performs none. Explicit probes report endpoint, model, check time, and error. Unsupported is false; missing keys and network errors remain null. Preserve other endpoint results when one fails and return exit code 6. The JSON root contains `capabilities`. Perception's advertised tasks do not imply that M1 implements grounding/world.

## 4. Configuration and models

Precedence: CLI overrides → environment → current-directory `cerul.toml` → `~/.cerul/config.toml` → defaults. Configuration stores key environment variable names, not secret values.

~~~toml
[embedding]
kind = "gemini"
model = "gemini-embedding-2"
dims = 1536

[vision]
kind = "gemini"
model = "gemini-3.8-flash"

[transcription]
kind = "gemini"
model = "gemini-3.8-flash"

# Example local or third-party endpoints:
# [vision]
# kind = "openai"
# base_url = "http://localhost:11434/v1"
# model = "qwen3-vl"
#
# [transcription]
# kind = "openai"
# base_url = "https://api.groq.com/openai/v1"
# model = "whisper-large-v3-turbo"
# api_key_env = "GROQ_API_KEY"
#
# [perception]  # Defaults; unnecessary for M1 processing.
# base_url = "https://api.cerul.ai"
# api_key_env = "CERUL_API_KEY"
~~~

| Capability | Gemini | OpenAI-compatible |
| --- | --- | --- |
| Embedding | `embedContent`, outputDimensionality=1536. Text query instruction: `task: search result \| query: {query}`. | `/v1/embeddings` with text, image, and video support. Reject text-only endpoints. |
| Vision | `generateContent` with responseSchema. | `/v1/chat/completions` with image_url and json_schema. |
| Transcription | Audio `generateContent` with a segment schema. | `/v1/audio/transcriptions`, verbose_json, segment timestamps. |

Probe the remote calls actually needed, not command names. Index probes embedding only for pending vectors and transcription only for pending audio. Annotate probes selected vision/perception capabilities. Search probes only when computing a query vector. Filters alone, exact text, sidecar rebuilds, ordinary status, and cleanup make no probe calls.

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
    grounding.box.jsonl              # Later milestone.
    world.hand.parquet              # Later, dense per-frame data.
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

M1 publication and recovery rules:

1. Write outputs to temporary staging files, validate, then atomically rename to their final location. No incomplete artifact occupies a final path.
2. Index and annotate are idempotent: skip completed stations and resume missing work. If sidecars exist but Lance rows are absent, restore the index with zero model calls. `--recompute` rewrites complete artifacts. Persist validated, cache-keyed per-unit checkpoints so successful units survive interruptions. Replace indexed rows by episode, stream, and artifact version, including removal of stale rows. An index update failure preserves valid sidecars; subsequent commands resynchronize the corresponding projection.


## 6. Data model

**Episode:** one recording, one or more streams, and a common timeline. M1 recognizes LeRobot episodes from metadata; each other video is a single-stream episode. User-authored multi-camera episode directories are M2.

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
| grounding | Coordinates normalized to [0,1] with frame_w/h. box{t_us,label,xyxy,track_id?}; affordance{t_us,label,points,action_hint}; trace{points[[t_us,x,y]],subject,label}; keypoint; mask{rle}. |
| world | Frames use T_<a>_from_<b>, units m or relative, xyzw quaternions, valid flags, null for missing values. camera{t_us,T_world_from_camera[7],intrinsics?,scale,valid}; hand{t_us,side,joints[21][3],valid}; object; depth{t_us,path,scale}; points. |

**Index unit:** a 30-second video-stream interval in episode time, with up to three rows of kind video, speech, or screen. Write vectors to sidecars before indexing.

Chunks columns: id, episode, stream, kind, start_us, end_us, vector[1536] for the default space, text, still, space_id, params_hash. Records columns: episode, stream, annotation, id, start_us, end_us, fields. Both tables are rebuildable from sidecars.

Default `cerul.verbs.v1` vocabulary: reach, grasp, regrasp, lift, carry, place, release, push, pull, open, close, insert, rotate, pour, wipe.

## 7. Required behavior

### Index stations

- **Transcript:** audio segments no longer than ten minutes; records contain start_us, end_us, text, lang.
- **Screen text:** PP-OCRv6 detection at 0.5 fps on source-resolution frames, with a 1080-pixel maximum long edge. Recognition runs only on detected boxes. Rust implements DBNet postprocessing and CTC decoding; repeated consecutive text is merged. Do not use a 480p proxy for OCR.
- **Frames and proxies:** 0.5 fps source keyframes (maximum 1080-pixel long edge) are temporary and removed after use. Cache 480p H.264 proxies for embeddings and contact sheets. Embedding uses 2 fps. Contact sheets default to 2 fps but honor --fps, sampling source frames before encoding and preserving their original PTS separately.
- **Embedding:** up to three independent calls per unit. Measure the actual encoded body against the endpoint limit; reduce bitrate then split, failing explicitly if still oversized. Persist vectors before Lance. Log failures and retry only missing units.
- **Cache identity:** episode_id, stream, stream_sha256, range_us, time mapping, station, space_id or model, params_hash, station version. Identity distinguishes datasets; content hashes detect replacement; ranges distinguish episodes sharing a shard.

### Semantic annotation

Sample at --fps (default 2), use a five-column contact grid, and burn **clip-relative** timestamps into frames. Each window starts at zero. Add clip_start_us exactly once to returned model times.

Subtask annotation describes first, then segments. Define boundaries using holding, release, arrival, and state change. Windows overlap by five seconds; conflicting outputs produce flags. Snap boundaries to real frame PTS, require continuous subtask coverage, validate ontology verbs where applicable, and keep normalized coordinates within [0,1].

### LeRobot writeback

M1 writes only subtask language entries to an already compatible v3.1 dataset. v3.0 returns code 3 and directs users to official format tooling; Cerul performs no upgrade. The pinned upstream recorder/compatibility-fixture distinction is documented in docs/lerobot.md and must not be misrepresented as an available official upgrade command.

Use language_persistent with style=subtask, exactly one active subtask per frame, and timestamps from the source Parquet frame values without recalculation. Preserve action, state, tasks, language_events, and all non-subtask language_persistent entries. Add or replace only subtask entries. Retain info.json content, updating only necessary language feature metadata when adding a column. Events/flags stay in sidecars because they have no defined official style.

Prefer --out. Stage a complete dataset, perform native field/timeline validation, then publish. Release acceptance additionally uses the official Python loader to read all sample frames; Python is an acceptance dependency, not a runtime dependency. In-place writeback requires recoverable file replacement records and must preserve unselected episodes in shared shards. Compare all affected shards' protected fields and existing annotations, not just one sampled frame.

### Search

Parse filters into episode/stream half-open time intervals, then apply them as Lance prefilters to intersecting chunks before vector ranking. Group the same interval by highest score and retain matched provenance. Exact-text hits merge when adjacent; ranked vector hits merge only when their windows mostly coincide, so the small overlap between neighbouring index windows never chains a whole video into one hit. Reranking is M2.

Repeated filters are AND. Conditions on one annotation must match the same record. Different annotations join by temporal intersection. Episode/stream/kind constrain scope. A chunk is eligible when its interval intersects the event interval; the hit time is that intersection.

Without a filter, search the whole selected space. Filters alone skip vectors and return time-ordered records. A query space mismatch returns code 3. Missing required annotations return a capability error, not an empty result.

## 8. Repository structure

One root Cargo.toml defines lib and bin. Modules cover configuration, providers, media, OCR, annotation schemas/I/O, episodes, LeRobot, indexing, semantic annotation, search, status, and cleanup. Serve modules are M2.

- models/: embedded detection/recognition ONNX and dictionary, with provenance, versions, and Apache-2.0 license.
- prompts/: one embedded English Markdown prompt per module.
- schemas/: generated with schemars and committed. Shared by CLI and library consumers.
- tests/: short, clearly licensed fixtures (at most ten seconds) and recorded endpoint-response replay.
- examples/: ordinary-video and LeRobot tutorials in M1; authored multi-camera directory tutorial in M2.
- dist-workspace.toml and the generated npm wrapper.
- README.md, DESIGN.md, LICENSE, and third-party notices.

Dependencies include clap, tokio, reqwest, serde, serde_json, schemars, lancedb 0.38, arrow, parquet, image, sha2, indicatif, tracing, tract-onnx. axum/utoipa and optional rerun belong to the later serving/visualization milestones. Invoke ffmpeg as a subprocess.

The initial macOS arm64 release binary measured approximately 228 MiB uncompressed on 2026-09-08. Final archive size and CPU throughput require release acceptance; the original 60 MB estimate is not a promise.

Every PR runs offline tests and two-platform builds. Protected-branch validation includes small real Gemini calls; never inject model keys into fork PRs.

## 9. Acceptance

| Milestone | Scope |
| --- | --- |
| **M1**, first release v0.0.3 | Index (transcript, OCR, embedding, still detection); search (vectors, images, prefilters, filter counts, saving clips, exact text); status; removal and disk reclamation; semantic annotation; LeRobot reading and subtask writeback; macOS/Linux distribution; replace the old npm cerul wrapper. |

Acceptance requirements:

1. On fresh macOS/Linux, an installation command plus GEMINI_API_KEY enables indexing a ten-minute video with speech in under ten minutes and finding the correct moment.
2. Ask ten manually chosen questions, including at least three each about visuals, speech, and screen text. Recall@5 is at least 8/10. Speech questions match speech rows; screen-text questions match screen rows. No separate large gold-set project is required.
3. Annotate five LeRobot episodes: subtasks cover the full timeline without gaps and use real frame boundaries. Official-loader readback of --write-lerobot --out verifies subtask content/timestamps and preserves all original action/state/annotation fields.
3a. Adjacent episodes sharing one MP4 do not mix timestamps. Two datasets with episode 000012 do not overwrite each other.
4. Repeating unchanged commands makes zero model calls. Changing only the embedding model recomputes only embeddings.
5. search/status JSON conforms to schemas. JSON-mode stderr parses line by line.
6. Gemini performs semantic annotation. Ollama and alternative vision endpoints are optional configurations, not M1 acceptance gates. An embedding endpoint without image support fails probing with code 3 and creates no index.
7. Deleting the workspace index permits zero-model-call rebuild from sidecar vectors. Ctrl-C during indexing preserves completed units; resumption fills gaps and leaves no incomplete final artifacts.
7a. A filtered event can be returned even when its unfiltered vector rank is below the first ten candidates.
8. Offline index --no-audio completes local probing, frames, and OCR, returns code 6 with embedding incomplete, and status reports the episode unindexed. Reconnection resumes embedding only.
9. Embedded OCR on twenty sample clips is no worse than a documented reference implementation.
10. The binary requires no Python, CUDA, or dynamic ML libraries; FFmpeg and ffprobe run as separately licensed, bundled subprocesses.

## 10. Validation considerations

- Verify the current gemini-3.8-flash model ID, pricing, and segment-level transcription timestamp quality. If timestamp quality fails acceptance, use a Whisper-compatible transcription default without changing the configuration shape.
- Verify tract-onnx operator coverage for PP-OCRv6 detection/recognition. If unsupported, investigate statically linked ort. Measure both platforms' CPU throughput and final binary size. Earlier throughput/size estimates remain unproven until measured.
- Verify lancedb 0.38 Rust APIs, particularly vector prefilter behavior.
- Use consented or licensed recordings for screen-text acceptance and a small, appropriately licensed public LeRobot dataset. Keep private recordings and derived acceptance outputs out of the public repository.
- Review the fifteen-verb default vocabulary.

## 11. Out of scope

DAG/profile/Run state machines, SSE, idempotency keys, plugins, bundle formats, ask/export/init/rm commands, SQLite, Python SDK/PyO3, embedded inference beyond OCR, Temporal, multi-tenant serve, knowledge graphs, chat UI, an action annotation family, retargeting, and training.

Do not add speculative interface layers before a fifth station, second embedding provider, or fourth annotation family creates a concrete need.
