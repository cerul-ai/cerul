# Install and teach Cerul with an agent

This is an end-user runbook for a terminal-capable coding agent. It does not
require MCP, a Cerul account, or an agent-specific plugin. Repository contributor
instructions are separate from this installation workflow.

## Copy this prompt

```text
Install Cerul by following https://github.com/cerul-ai/cerul/blob/main/docs/agent-setup.md.
Help me configure my Gemini API key securely, search a local video, and save a
matching clip. Teach me the commands in my language.
```

## Agent workflow

### 1. Check the platform

Use `uname -s` and `uname -m`. Supported bundles are macOS arm64 and Linux
x86_64 (Ubuntu 24.04 or newer). Check whether `cerul` is already installed.
Preserve existing configuration and user files.

### 2. Install the release bundle

Follow [installation](installation.md) and run:

```sh
curl -fsSL https://cerul.ai/install.sh | sh
```

The bundle includes media tools and OCR models. Do not install Rust, Python,
Ollama, or system FFmpeg for this workflow. If the download fails, report the
error; do not silently switch to a source build or a different package.
Source builds are for users who explicitly choose them.

### 3. Verify installation

Follow the installer's PATH instructions. Run `cerul --version`, `cerul --help`,
and `cerul --json status`. Report the executable path and version. Do not use
`status --providers`: optional providers are unnecessary for the first video.

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
