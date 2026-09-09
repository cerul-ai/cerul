# Configuration

Cerul uses model endpoints selected by the user. It does not require a Cerul
account for M1 indexing, searching, or semantic annotation. OCR runs locally on
CPU with embedded weights; embedding, vision, and transcription use HTTP.

Configuration priority, from lowest to highest:

1. Built-in defaults.
2. `~/.cerul/config.toml`.
3. `cerul.toml` in the current directory.
4. `CERUL_<ENDPOINT>_<FIELD>` environment variables.
5. Repeated `--set KEY=TOML_VALUE` arguments.

The default embedding model is `gemini-embedding-2` with 1536 dimensions; vision
and transcription default to `gemini-3.8-flash`. These defaults use
`GEMINI_API_KEY` and the Gemini v1beta endpoint. The [Gemini model reference](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash)
and [embedding guide](https://ai.google.dev/gemini-api/docs/embeddings) describe
these model IDs. Text queries use the embedding guide's `task: search result | query: {query}`
instruction. Account access and timestamp quality require real-request acceptance.
Store only the key's environment variable name in configuration, never the key.

The CLI can save a validated default Gemini key in `~/.cerul/credentials.json`
(mode 0600). Stored keys are scoped to endpoint kind, service URL, and key
variable name. Environment values override saved keys. The first interactive
processing command offers hidden setup when needed; JSON, quiet, yes, dry-run,
and non-terminal calls never prompt. See [installation](installation.md) for
storage, removal, and agent setup. Library callers do not read this file.

```toml
[embedding]
kind = "gemini"
model = "gemini-embedding-2"
dims = 1536
api_key_env = "GEMINI_API_KEY"

[vision]
kind = "openai"
base_url = "http://localhost:11434/v1"
model = "qwen3-vl"

[transcription]
kind = "openai"
base_url = "https://api.groq.com/openai/v1"
model = "whisper-large-v3-turbo"
api_key_env = "GROQ_API_KEY"
```

An OpenAI-compatible endpoint must implement the specific request and response
features Cerul uses. Vision requires image input and structured JSON output;
transcription requires timestamped segments. Embedding requires text, image,
and video input in one matching space: a text-only embeddings endpoint is not a
fallback. Compatibility is established by capability probes, not by its URL.

Endpoint URLs cannot contain embedded credentials, query parameters, or
fragments. For a command-line string override, include the TOML quotes:

```sh
cerul --set 'vision.model="qwen3-vl"' status
```

## Capabilities and network calls

Ordinary `status`, exact text search, annotation-only filters, index rebuilds,
and cleanup do not probe providers. Commands probe only endpoints needed for
pending work. Successful probes are cached for seven days.

`cerul status --providers` explicitly probes all four configured endpoints,
including the future perception endpoint. It may return a partial result when
one endpoint is unavailable even if the endpoints needed for M1 are usable.
Use `--recompute` to refresh successful probes and `--dry-run` to avoid probes.
Capability values are true for supported, false for explicitly unsupported,
and null for unknown (including missing credentials and network errors).
Perception's advertised tasks do not enable M2 features in this CLI.

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

Gemini transcription uses sixty-second audio windows and converts the model's
clip-relative seconds to episode-relative integer microseconds. These are
model-estimated speech boundaries; inspect the source video when exact word
alignment matters.

## Workspace and vector spaces

`--workspace DIR` overrides `CERUL_WORKSPACE`, then the default `~/.cerul`.
Each embedding space includes provider kind, base URL, model, dimensions, and
query template in its identity. Changing any of these requires compatible new
vectors. `status` lists space IDs and public model metadata. A query cannot
silently search a different space.

Changing the embedding configuration invalidates embedding artifacts, while
unchanged transcripts and OCR results remain reusable. Sidecar vectors are
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
perception processing are outside M1.

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
