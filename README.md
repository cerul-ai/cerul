# Cerul

Open-source video processing core and CLI. Turn video into searchable,
annotated data using your own model endpoints, without a Cerul account.

Cerul indexes local videos and LeRobot datasets, searches moments using text or
images, and generates semantic annotations. Transcripts, annotations, and
embedding vectors live in sidecar files. The vector index is a disposable cache
that can be rebuilt without calling a model.

The v0.0.3 rewrite is undergoing release acceptance. The commands below use a
local source build; they do not assume that the new package has been published.

## Build and try it

M1 supports macOS arm64 and Linux x86_64. Install a current stable Rust toolchain,
ffmpeg/ffprobe 6.0 or later, and the build dependencies:

```sh
# macOS
brew install ffmpeg protobuf

# Ubuntu 24.04
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev protobuf-compiler libprotobuf-dev ffmpeg

cargo build --release --locked
./target/release/cerul --version
```

Make `target/release/cerul` available on your PATH. Configure `GEMINI_API_KEY` in
your shell, or choose other compatible endpoints in `cerul.toml` using the
[configuration guide](docs/configuration.md). Keep keys out of configuration
files and source control.

```sh
cerul index ./videos
cerul search "A person puts a cup on the table"
cerul search --image ./reference.png --save ./clips
cerul search --text "ECONNREFUSED"
cerul annotate ./videos --semantic
cerul status
```

OCR runs locally on CPU using embedded weights. Video embedding, transcription,
and semantic annotation send inputs to the configured model endpoints. Embedding
requires a model with text, image, and video support; text-only embedding APIs
are not substitutes. Cerul collects no telemetry.

## Work with results

- [Video search and annotation tutorial](examples/video-search.md)
- [LeRobot subtask tutorial](examples/lerobot-subtasks.md)
- [LeRobot format and writeback compatibility](docs/lerobot.md)
- [Configuration, capabilities, and exit codes](docs/configuration.md)
- [Generated JSON schemas](schemas)
- [Release artifacts and installer verification](docs/releases.md)

Repeated commands reuse completed work. Ctrl-C cancels a command; rerunning
completes missing work. `clean --all-indexes` and `clean --cache` preserve source
media and sidecars. Deleting sidecars is an explicit operation requiring `--yes`.

Ordinary videos default to task, subtask, and flag annotations. LeRobot datasets
also default to event, interaction, state, and progress. Writeback is opt-in,
limited to supported v3.1 subtask language entries, and can publish a separate
output dataset with `--out`.

## Integrate the core

The Rust library exposes processing functions independently of process
arguments and terminal output. Desktop applications can link it or run the CLI
with `--json`: stdout contains one final JSON object and stderr contains NDJSON
progress/log events. Annotation times are episode-relative integer microseconds.

HTTP/MCP serving, grounding, world annotations, and Windows support are later
milestones. Hosted tenancy, billing, and perception implementation remain in the
private product repository. See [DESIGN.md](DESIGN.md) and
[ARCHITECTURE.md](ARCHITECTURE.md) for boundaries.

## Development and licensing

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo run --locked --example generate_schemas -- --check
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for validation requirements. The Rust
source is licensed under Apache-2.0. Embedded OCR provenance is recorded in
[models/README.md](models/README.md); binary distributions include
[third-party notices](THIRD_PARTY_NOTICES.md).

Existing Git history, tags, and `ffmpeg-vendor-*` release assets are retained.
The retired platform API client and MCP projection are replaced by this core.
