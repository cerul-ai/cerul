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

Cerul is a command-line tool for searching videos and saving clips. The download
includes the media tools and OCR models it needs.

Supports **macOS Apple Silicon** and **Linux x86_64 (Ubuntu 24.04 or newer)**.

### 1. Install

```sh
curl -fsSL https://cerul.ai/install.sh | sh
```

### 2. Add a video

```sh
cerul index ./demo.mp4
```

On first use, follow the prompt to enter your [Gemini API key](https://aistudio.google.com/apikey).
Cerul saves it on your computer for future runs. Model processing sends data to
Gemini and may incur API charges.

### 3. Search and save clips

```sh
cerul search "A person puts a cup on the table"
cerul search "A person puts a cup on the table" --save ./clips
```

[More examples →](examples/video-search.md)

## Let your agent do the setup

Copy this prompt into an agent that can use a terminal:

```text
Install Cerul by following https://github.com/cerul-ai/cerul/blob/main/docs/agent-setup.md. Help me set up my Gemini API key securely, search a local video, and save a matching clip. Teach me the commands in my language.
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
