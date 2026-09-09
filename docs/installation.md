# Install Cerul

[English](../README.md) · [简体中文](../README.zh-CN.md) · [繁體中文](../README.zh-TW.md) · [Agent instructions](agent-setup.md)

## Install

Supports macOS Apple Silicon and Linux x86_64 (Ubuntu 24.04 or newer).
The bundle includes FFmpeg, ffprobe, and OCR models.

```sh
curl -fsSL https://cerul.ai/install.sh | sh
```

Follow the installer's PATH instructions or open a new terminal, then run:

```sh
cerul --version
cerul index ./demo.mp4
cerul search "A person puts a cup on the table" --save ./clips
```

On first use, enter your [Gemini API key](https://aistudio.google.com/apikey)
when prompted. Model processing sends inputs to Gemini and may incur API charges.

You can also download an archive from [GitHub Releases](https://github.com/cerul-ai/cerul/releases/latest).
Keep `cerul`, `cerul-ffmpeg`, and `cerul-ffprobe` together when moving them.

## Run your first video

The first interactive `index` or semantic `search`/`annotate` using the default
Gemini endpoint prompts for a hidden API key only when remote work is pending,
validates a small text embedding
request, and writes `~/.cerul/credentials.json` with mode 0600. This is private
plaintext storage on your computer, not an encrypted vault. Remove the file to
forget saved keys. Credentials are scoped to the service URL and key variable;
changing the service does not forward saved keys to a new provider.

An environment variable such as `GEMINI_API_KEY` takes precedence. Custom
providers use their configured key environment variables. `--json`, `--yes`,
`--quiet`, `--dry-run`, and non-terminal calls never prompt. Agents should use
an existing credential or guide the user to enter it in their own terminal,
never ask for the secret in chat. Provider config stores names, not key values.
See [configuration](configuration.md) for alternative endpoints.

Choose a short video and a search phrase that describes something in it:

```sh
cerul --dry-run index ./demo.mp4
cerul index ./demo.mp4 --jobs 1
cerul search "A person puts a cup on the table" --in ./demo.mp4
cerul status ./demo.mp4
```

OCR stays local. Embedding and transcription send sampled media or text to your
configured endpoints and may incur provider charges. Use a video you intend to
process with those endpoints. The default workspace is `~/.cerul`; pass
`--workspace DIR` to use another one consistently across commands.

For a local-only first check without credentials:

```sh
cerul index ./demo.mp4 --no-audio --jobs 1
cerul status ./demo.mp4
```

When no model credentials are configured, local OCR can complete and the index
command returns **6 (partial)** for missing embeddings. This is not a complete
semantic-search setup. Configure a key and rerun the same command to finish.

## If something fails

| Symptom | Next step |
| --- | --- |
| An older Cerul version runs after installation | Run `type -a cerul` to find competing installations, then put the new installer's directory first on PATH and open a new terminal |
| `ffmpeg` or `ffprobe` missing/too old | Reinstall the complete Cerul bundle and keep all three executables together; for custom tools, check the overrides below |
| Unknown encoder `libx264` | Use a build that includes this encoder |
| Missing model credential | Set the endpoint's configured key environment variable without exposing the value |
| Provider quota or rate limit | Lower `--jobs`, set an account-appropriate `--rpm`, and resume after quota is available |
| Exit 6 | Inspect `status` and the command's error output; rerun to fill incomplete work |
| Workspace busy | Let the other operation finish; do not delete its lock file |

Do not use `status --providers` as the default installation test: it probes all
configured endpoints, including optional ones that are unnecessary for your
first video. Processing commands probe the endpoints they actually need.

Continue with the [video tutorial](video-search.md) or the
[LeRobot tutorial](lerobot-subtasks.md).

## Custom media tools and development builds

Resolution order is `CERUL_FFMPEG` / `CERUL_FFPROBE`, then the bundled
`cerul-ffmpeg` / `cerul-ffprobe` next to the running CLI, then system PATH.
Overrides select an executable path, not a shell command. An invalid override
fails instead of silently selecting another tool. Source-only developers may
use `cargo build --release --locked` with system FFmpeg/ffprobe 6.0+ and libx264.
The bundled executables intentionally disable FFmpeg network input protocols;
Cerul processes local media files.

## Build from source

Contributors and users testing an unreleased change can follow the
[source build guide](development/building.md).
