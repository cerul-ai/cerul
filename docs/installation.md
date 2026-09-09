# Install Cerul

[English](../README.md) · [简体中文](../README.zh-CN.md) · [繁體中文](../README.zh-TW.md) · [Agent instructions](agent-setup.md)

The current CLI is available from source. Published releases of the older
platform client are not installers for this CLI. Do not use an old release or
an unverified npm package as a shortcut. Shell, npm, and Homebrew installation
artifacts are configured, but their public distribution is not yet verified.

## Release bundle (when v0.0.3 is published)

Use the versioned shell installer linked in the README. It installs `cerul`,
`cerul-ffmpeg`, and `cerul-ffprobe` together. Keep them together when relocating
an installation. OCR models are embedded; no separate model download is needed.
Rust, Protobuf, Python, and a system FFmpeg are not required to run a bundle.

Supported platforms: macOS Apple Silicon and Linux x86_64 on Ubuntu 24.04 or
newer. Windows, Intel macOS, and Alpine/musl are not supported release targets.

Until this release exists, use the complete source build below. Do not use the
old `v0.0.03` client: its name is similar but it is a different implementation.

## Check before installing

Check existing tools first. A missing command means that tool needs installation;
you do not need to reinstall tools that already meet these requirements.

```sh
uname -s
uname -m
rustc --version
cargo --version
protoc --version
pkg-config --version
```

| Dependency | Used for | Needed after compilation? |
| --- | --- | --- |
| Stable Rust, Cargo, C/C++ toolchain | Building the CLI | No |
| Protobuf compiler and build libraries | Building dependencies | No |
| pkg-config, make, curl, tar | Building the bundled media tools | No |
| FFmpeg and ffprobe | Media processing | Included in a complete build |
| OCR models | Local text recognition | Included in the CLI |
| Python, CUDA, Ollama | Not required for the default workflow | No |

The current media pipeline uses the `libx264` encoder. A custom ffmpeg build
must provide it as well as the input decoders needed for your media. Check with
`ffmpeg -hide_banner -encoders` if using a nonstandard build.

## Install missing build dependencies

On macOS, use [Homebrew](https://brew.sh) if it is already installed:

```sh
brew install protobuf pkgconf
```

The Apple command-line developer tools are also required. If they are missing,
run `xcode-select --install` and complete the system dialog.

On Ubuntu 24.04:

```sh
sudo apt-get update
sudo apt-get install -y git build-essential pkg-config libssl-dev protobuf-compiler libprotobuf-dev curl xz-utils bzip2
```

Install a stable Rust toolchain using the instructions at [rustup.rs](https://rustup.rs)
if Rust is missing. Open a new terminal after installation so Cargo is on PATH.
Do not run Cargo or Cerul with sudo.

## Build and verify

```sh
git clone https://github.com/cerul-ai/cerul.git
cd cerul
bash scripts/build-distribution.sh
export PATH="$PWD/target/bundle:$PATH"
cerul --version
cerul --json status
```

These commands assume the checkout contains the root `Cargo.toml` and the
`index`, `search`, and `annotate` CLI. If you are testing an unmerged change,
use the branch or commit supplied by its author. Do not substitute the retired
`apps/cli` client. The PATH change above applies to this shell; save it in your
shell configuration only if you want a persistent installation at this location.

`status` does not call a provider. It verifies that the CLI starts; it does not
prove media processing or model connectivity. `--dry-run index` previews the
operation but does not establish that all runtime dependencies work.

## Run your first video

The first interactive `index` or semantic `search`/`annotate` using the default
Gemini endpoint prompts for a hidden API key, validates a small text embedding
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
| `ffmpeg` or `ffprobe` missing/too old | Install or update the media package, then verify both commands in the same terminal |
| Unknown encoder `libx264` | Use a build that includes this encoder |
| Missing model credential | Set the endpoint's configured key environment variable without exposing the value |
| Provider quota or rate limit | Lower `--jobs`, set an account-appropriate `--rpm`, and resume after quota is available |
| Exit 6 | Inspect `status` and the command's error output; rerun to fill incomplete work |
| Workspace busy | Let the other operation finish; do not delete its lock file |

Do not use `status --providers` as the default installation test: it probes all
configured endpoints, including optional ones that are unnecessary for your
first video. Processing commands probe the endpoints they actually need.

Continue with the [video tutorial](../examples/video-search.md) or the
[LeRobot tutorial](../examples/lerobot-subtasks.md).

## Custom media tools and development builds

Resolution order is `CERUL_FFMPEG` / `CERUL_FFPROBE`, then the bundled
`cerul-ffmpeg` / `cerul-ffprobe` next to the running CLI, then system PATH.
Overrides select an executable path, not a shell command. An invalid override
fails instead of silently selecting another tool. Source-only developers may
use `cargo build --release --locked` with system FFmpeg/ffprobe 6.0+ and libx264.
The bundled executables intentionally disable FFmpeg network input protocols;
Cerul processes local media files.
