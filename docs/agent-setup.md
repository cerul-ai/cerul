# Install and teach Cerul with an agent

This is an end-user runbook for a terminal-capable coding agent. It does not
require MCP, a Cerul account, or an agent-specific plugin. Repository contributor
instructions are separate from this installation workflow.

## Copy this prompt

```text
Help me install Cerul from https://github.com/cerul-ai/cerul and use it to
search a short local video. Read docs/agent-setup.md and docs/installation.md
from the checkout I am using, and follow their supported installation path.
Prefer a verified new CLI release bundle; otherwise build the complete source
distribution. Check my OS and existing dependencies; install only what is missing, within
my permissions. Build and verify the CLI, then ask me for a video if I have
not selected one. Help me configure model credentials without displaying
or asking me to paste their values into chat. Explain which inputs are sent
to model endpoints before the first processing run. Run one indexing and
search example, then teach me how to repeat it. Report actual results and
anything incomplete. Respond in my language.
```

## Agent workflow

### 1. Identify the correct source

Use an existing user-selected checkout when supplied; record its commit and
preserve local changes. Otherwise clone `https://github.com/cerul-ai/cerul`
into an unused directory. Read its README and installation guide. Before
building, confirm the root Cargo package exposes the new video core commands.
If the default branch still contains the old client, ask for the intended
branch/ref. Do not silently pick an arbitrary PR, reset a checkout, or install
an older same-named package.

The verified path in this version is a source build. Generated installer
configuration is not proof of a published package. Do not invent a Homebrew
tap, use `npm install -g cerul`, or execute `cerul.ai/install.sh` unless the
checked-out release documentation and actual matching release establish that
route. A historical ffmpeg release is not a Cerul CLI release.

### 2. Check and install dependencies

Use `uname -s` and `uname -m`. Support macOS arm64 and Linux x86_64; Ubuntu 24.04
is the tested Linux environment. Follow the installation guide for Rust,
the compiler toolchain, Protobuf, and pkg-config for a source build. Release
bundles already include FFmpeg and ffprobe. Check existing dependencies first;
install only missing build tools. Do not install system FFmpeg for a bundle.

Use the user's package manager and the host's normal permission flow. If a
system installer needs interaction, explain the specific remaining step and
resume afterward. Do not install Python, CUDA, Ollama, optional perception
services, or unrelated model servers for the default Gemini workflow.

### 3. Build and establish an offline baseline

Run `bash scripts/build-distribution.sh` in the checkout. Wait for its actual exit
status; compilation output alone is not a successful build. Use the absolute
path to `target/bundle/cerul` until PATH is verified in the relevant shell.
Report where it is installed and whether the PATH setup is temporary.

Run `cerul --version`, `cerul --help`, and `cerul --json status`. Do not claim
model or media validation from these commands. Do not use `status --providers`
for initial verification because it probes optional endpoints too.

### 4. Configure credentials and choose a video

Reuse an explicitly configured model endpoint and credentials. Otherwise guide
the user to run their first `cerul index` in their own interactive terminal for
hidden key entry, or set `GEMINI_API_KEY` locally through their secret manager.
Saved keys are reused by noninteractive calls. JSON/agent calls never prompt.
Check presence only; never echo a key, dump the environment, include credentials
in logs, or copy them to the repository. Do not source an unrelated project's
entire `.env` file. If the user supplies a credential file, parse only the named
value as data and scope it to the child process that needs it.

Ask for a video only if none was selected. Prefer a 30–60 second clip for the
first run. State that OCR is local and that embedding/transcription send inputs
to the selected endpoints with possible provider charges. Use the user's
authorization for that clip and those endpoints; do not choose private media
from their disk automatically.

With no key available, a local OCR trial using `index --no-audio` can return
exit 6 with missing embeddings. Label it partial; do not claim semantic search
works. Do not call paid endpoints until configured and authorized.

### 5. Complete one real workflow

Use one workspace consistently. Substitute the actual quoted video path in:

```sh
cerul --dry-run index ./demo.mp4
cerul --json index ./demo.mp4 --jobs 1
cerul --json search "A description of a moment in this video" --in ./demo.mp4
cerul --json status ./demo.mp4
```

Choose the query from the user's description or the resulting transcript/OCR;
do not assume a cup or another example object exists in their video. The dry
run previews the operation and does not prove dependency or endpoint readiness.
For JSON commands, stdout is the final result and stderr carries NDJSON events.
Retain the exit code and inspect both streams. Exit 6 means partial work, not
success. Rerun after fixing the reported problem; completed work is reused.

Show returned time ranges and explain the match. A zero-hit query is not proof
of successful retrieval; explain what was indexed and help choose a grounded
query. If requested, use `search ... --save DIR` to export matching clips.
Keep the first walkthrough to one video; dataset writeback and sidecar deletion
need their own user request. Ctrl-C and rerunning are supported for recovery.

### 6. Teach and report

Give the user their actual executable path, source commit/version, workspace,
dependency versions, completed/partial stations, and one real search result.
Do not include credentials or unnecessary local file contents. Finish with
three copyable commands using their chosen path: index, search, and status.
Link the video tutorial and explain how to rerun with another video. Distinguish
installation success, local media processing, and remote model success.
