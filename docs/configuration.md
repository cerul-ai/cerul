# Configuration

Cerul uses model endpoints selected by the user. It does not require a Cerul
account for indexing, searching, or semantic annotation. OCR runs locally on
CPU with embedded weights; embedding, vision, and transcription use HTTP.

Configuration priority, from lowest to highest:

1. Built-in defaults.
2. `~/.cerul/config.toml`.
3. `cerul.toml` in the current directory.
4. `CERUL_<ENDPOINT>_<FIELD>` environment variables.
5. Repeated `--set KEY=TOML_VALUE` arguments.

Multimodal embedding defaults to `gemini-embedding-2`, 3072 dimensions, at the
Google Gemini endpoint. A Gemini key is required for new vectors and semantic
queries. Providers, models, dimensions, and URLs remain configurable; changing
an embedding space requires compatible saved vectors or re-indexing. Offline
status, cleanup, and rebuilding cached vectors need no key.
Vision defaults to `gemini-3.8-flash` and runs during indexing. Use
`cerul index ./video.mp4 --no-understanding` for a deliberate one-run skip, or
set `enabled = false` in `[vision]` to disable automatic understanding.
Scene generation, overview generation, and their source references are cached
independently. A failed understanding step leaves completed base vectors usable.

The CLI automatically enables the configured transcription endpoint when its key
is already available. With defaults, this reuses the Gemini key for
`gemini-3.5-transcribe`; indexing does not open a provider/model/credential chooser.
Use `cerul config` to choose Gemini, Groq, OpenAI, Custom, or Disabled. Saved keys
are reused; `cerul auth set` replaces the Gemini key. Explicit provider settings
and `enabled = false` take precedence over automatic selection.

Defaults are saved in `~/.cerul/config.toml`. JSON, quiet, yes, dry-run, and redirected
invocations never prompt. Without credentials, new unconfigured ASR is skipped;
complete compatible transcripts can still be reused offline. An enabled ASR that
fails is reported as a partial result, not silently disabled.

The presets are Gemini `gemini-3.5-transcribe`, Groq `whisper-large-v3-turbo`, and
OpenAI `whisper-1`. Model names can be edited. Custom services need a base URL,
model, and key, and must implement OpenAI's timestamped transcription contract.
The setup check uses a silent audio clip; it checks connectivity and the response
contract, not recognition accuracy. Use a real recording to evaluate accuracy.

Keys are stored separately in `~/.cerul/credentials.json` (mode 0600), scoped to
protocol, service URL, and environment variable name. Environment values override
saved keys. Configuration files contain only the environment variable name,
never the secret. `cerul auth set` and `cerul auth remove` continue to manage the
Gemini key. Library callers do not read CLI credential storage.

For non-interactive setup, export the selected key variable and configure ASR:

```toml
[embedding]
kind = "gemini"
model = "gemini-embedding-2"
dims = 3072
api_key_env = "GEMINI_API_KEY"

[vision]
kind = "openai"
base_url = "http://localhost:11434/v1"
model = "qwen3-vl"

[transcription]
enabled = true
kind = "openai"
base_url = "https://api.groq.com/openai/v1"
model = "whisper-large-v3-turbo"
api_key_env = "GROQ_API_KEY"
```

To disable ASR permanently, set `enabled = false` in `[transcription]`. To use
Gemini, set `kind = "gemini"`, `model = "gemini-3.5-transcribe"`,
`base_url = "https://generativelanguage.googleapis.com/v1beta"`, and
`api_key_env = "GEMINI_API_KEY"`. OpenAI uses `kind = "openai"`,
`base_url = "https://api.openai.com/v1"`, `model = "whisper-1"`, and
`api_key_env = "OPENAI_API_KEY"`.

OpenAI-compatible ASR uses `/audio/transcriptions` with `verbose_json` and segment
timestamps. A model returning text without timestamps is unsupported for video
localization. Gemini's dedicated model uses native word annotations; other Gemini
models retain the prompted segment-JSON compatibility path. No Responses API or
cross-provider fallback is used. All results become the same integer-microsecond
transcript sidecar format.

Endpoint URLs cannot contain embedded credentials, query parameters, or
fragments. For a command-line string override, include the TOML quotes:

```sh
cerul --set 'vision.model="qwen3-vl"' status
```

## Capabilities and network calls

Ordinary `status`, exact text search, annotation-only filters, index rebuilds,
and cleanup do not probe providers. Commands probe only endpoints needed for
pending work. Successful probes are cached for seven days.

`cerul status --providers` explicitly probes embedding, vision, and enabled transcription.
Perception processing is not supported in this version. Its reserved endpoint
is probed only when explicitly configured; otherwise its capability stays null
and no request is sent to it. The command may return a partial result when one
endpoint is unavailable even if the endpoints needed for your operation work.
Use `--recompute` to refresh successful probes and `--dry-run` to avoid probes.
Capability values are true for supported, false for explicitly unsupported,
and null for unknown (including missing credentials and network errors).
A perception endpoint advertising tasks does not add processing support to this CLI.

Before the first media request to each endpoint in a command, Cerul prints a
notice. `--yes` suppresses that notice. This is a notice, not an interactive
confirmation prompt. Configure endpoints before running a processing command.

## Provider rate limits and transcription

Use `--rpm` on indexing or annotation commands to pace requests when an endpoint
returns HTTP 429. For example:

```sh
cerul index ./recording.mp4 --jobs 4 --rpm 12
```

Choose a limit appropriate for your provider account. Cerul honors bounded
`Retry-After` and Gemini retry-delay hints, but retries cannot guarantee that
account quotas are available. A partial index retains completed work; repeat
the same command after quota becomes available to fill the remaining units.

Transcription uses sixty-second audio windows and converts clip-relative times
to episode-relative integer microseconds. Inspect the source when exact alignment
matters. A failed ASR does not prevent video/OCR embedding publication. Repeat
`cerul index ./recording.mp4` after fixing ASR to resume missing speech windows;
completed OCR and compatible embedding checkpoints are reused. Changing the ASR
model invalidates its transcript and the derived text embeddings.

Skipping ASR omits the separate transcript. Video embedding proxies contain no
audio; speech retrieval depends on the separate transcript. JSON stream results include `speech`
(`disabled`, `not_configured`, `no_audio`, `no_speech`, `complete`, `planned`, or `failed`). A partial
failure returns exit 6 and retains a diagnostic in the stream's index state.

## Workspace and vector spaces

`--workspace DIR` overrides `CERUL_WORKSPACE`, then the default `~/.cerul`.
Each embedding space includes provider kind, base URL, model, dimensions, and
query template in its identity. Changing any of these requires compatible new
vectors. `status` lists space IDs and public model metadata. A query cannot
silently search a different space.

The CLI defaults to the Gemini embedding space. Compatible endpoints and dimensions can be configured; different configurations create separate spaces. Sidecar vectors are
retained by cache/index cleaning and can rebuild the corresponding index.

## Process contract

`--json` writes one final object to stdout and newline-delimited JSON events to
stderr. Human-readable mode uses stderr for progress and logs. Annotation text
is English. All annotation intervals are half-open episode-relative integer
microseconds; source PTS and clip offsets are mapped internally.

| Exit | Meaning |
| --- | --- |
| 0 | Success |
| 2 | Invalid arguments or configuration |
| 3 | Missing dependency or unsupported capability |
| 4 | Execution failure, including a busy workspace lock |
| 5 | Cancelled |
| 6 | Partial result; inspect incomplete stations or endpoint results |

Only one writer may operate in a workspace at a time. A lock conflict fails;
commands do not silently forward to a background service. HTTP/MCP serving and
perception processing are not supported in this version.

## Local OCR concurrency

`index --jobs N` also bounds local OCR workers, capped by the available CPU
count. Each worker owns its model plans, so higher concurrency uses more memory.
Use `--jobs 1` on memory-constrained machines. Completed frame checkpoints survive
cancellation; changing the worker count does not invalidate OCR results. Worker
completion order never changes annotation timestamps or adjacent-text merging.

OCR keeps recognized lines with confidence of at least 0.75. This suppresses
low-confidence output from motion-blurred frames; it does not guarantee that
every retained character is correct. The threshold is part of the station's
cache identity, so results produced with an older threshold are recomputed.

When a video is replaced at the same path, Cerul registers only its current
content identity. If the adjacent sidecar belongs to the previous content, the
new sidecar uses `<media>.<sha256>.cerul`; the previous annotations are preserved
on disk and are not included in the current registry. Shared LeRobot video
shards continue to keep one registry entry and sidecar per episode.
