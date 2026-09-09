<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/cerul-logo-dark.png">
    <img src="docs/assets/cerul-logo-light.png" alt="Cerul" width="280">
  </picture>
</p>

<p align="center"><strong>Help AI understand, remember, and access video.</strong></p>
<p align="center">Open-source video processing core and CLI. Use your own model endpoints, without a Cerul account.</p>

<p align="center">
  <a href="https://cerul.ai">Website</a> ·
  <a href="examples/video-search.md">Quickstart</a> ·
  <a href="docs/agent-setup.md">Install with an agent</a> ·
  <a href="docs/configuration.md">Configuration</a> ·
  <a href="https://x.com/cerul_hq">X / Twitter</a> ·
  <a href="https://discord.gg/qHDEMQB9vN">Discord</a>
</p>

<p align="center">
  <a href="https://github.com/cerul-ai/cerul/actions/workflows/ci.yml"><img src="https://github.com/cerul-ai/cerul/actions/workflows/ci.yml/badge.svg" alt="Build and tests"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="License: Apache-2.0"></a>
</p>

<p align="center">English · <a href="README.zh-CN.md">简体中文</a> · <a href="README.zh-TW.md">繁體中文</a></p>

## Search your videos with words

Cerul turns local videos into a searchable library. Describe a moment, find text
spoken or shown on screen, and save matching clips. It works with ordinary video
folders and LeRobot datasets, using your own model API key.

- **Search by meaning or example.** Use a sentence or a reference image.
- **Find speech and screen text.** Transcription plus local, embedded OCR.
- **Keep useful results.** Export clips and structured semantic annotations.
- **Resume where you left off.** Completed work is cached; rerun after interruption.

## Getting started

Supports **macOS Apple Silicon** and **Linux x86_64 (Ubuntu 24.04 or newer)**.
Release bundles contain Cerul, FFmpeg, ffprobe, and OCR weights. No Python,
Ollama, or separately installed FFmpeg is needed for these bundles.

> The rewritten CLI's first bundle is awaiting release. Older releases contain a
> different client. For this checkout, use the source instructions below or ask
> your agent to follow the installation guide. The command below becomes available
> when `v0.0.3` is published.

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/cerul-ai/cerul/releases/download/v0.0.3/cerul-installer.sh | sh
```

<details>
<summary>Build this checkout before the release</summary>

Install [Rust](https://rustup.rs), a C compiler, Protobuf, and pkg-config.
On macOS: `brew install protobuf pkgconf`. On Ubuntu, see the
[installation guide](docs/installation.md).

```sh
git clone https://github.com/cerul-ai/cerul.git
cd cerul
bash scripts/build-distribution.sh
export PATH="$PWD/target/bundle:$PATH"
cerul --version
```

Use the author-provided branch for an unmerged PR. This builds the media tools
from pinned sources too; the first build takes longer than a release install.

</details>

### 1. Index a video

```sh
cerul index ./demo.mp4
```

On your first interactive run, Cerul asks for a [Gemini API key](https://aistudio.google.com/apikey),
validates it, and saves it privately on your computer. Input is hidden. You can
also set `GEMINI_API_KEY` yourself; environment values take precedence.

OCR runs locally. Video embeddings, transcription, and semantic annotation send
inputs to your configured model endpoints and may incur provider charges.
Cerul requires no account and collects no telemetry.

### 2. Find a moment

```sh
cerul search "A person puts a cup on the table"
cerul search --text "connection refused"
cerul search --image ./reference.png
```

### 3. Save matching clips

```sh
cerul search "A person puts a cup on the table" --save ./clips
```

Start with a short video. Follow the [complete walkthrough](examples/video-search.md)
for more examples, progress checks, and recovery.

## Let your agent do the setup

Copy this prompt into an agent that can use a terminal:

```text
Install Cerul from https://github.com/cerul-ai/cerul. Read docs/agent-setup.md
and docs/installation.md from the selected version. Prefer a verified release
bundle; if none exists for the new CLI, build the current source with its bundled
media tools. Verify installation, help me configure my API key without displaying
or logging it, and ask which local video I want to try. Explain model data transfer,
index one short video, search it, and teach me how to repeat the commands in my
language. Report actual results, including any incomplete stations.
```

[Agent setup guide →](docs/agent-setup.md)

## More things to try

| Goal | Command |
| --- | --- |
| Index a folder | `cerul index ./videos` |
| Preview work | `cerul --dry-run index ./videos` |
| Generate semantic annotations | `cerul annotate ./videos --semantic` |
| Check progress and results | `cerul status` |
| Search one video | `cerul search "opening a door" --in ./demo.mp4` |

Sidecar files preserve transcripts, annotations, and vectors alongside your media.
Search indexes can be rebuilt from them without model calls. LeRobot subtask
writeback is available as an explicit opt-in.

## Learn more

- [Installation and troubleshooting](docs/installation.md)
- [Video search tutorial](examples/video-search.md)
- [LeRobot tutorial](examples/lerobot-subtasks.md)
- [Model endpoints and configuration](docs/configuration.md)
- [Contributing and developer integration](CONTRIBUTING.md)

## License

Cerul's Rust code is Apache-2.0. Bundles also contain separately licensed media
tools and OCR weights; see [third-party notices](THIRD_PARTY_NOTICES.md).
The [Cerul name and logo](docs/assets/README.md) identify the project and do not
imply endorsement of third-party products.
