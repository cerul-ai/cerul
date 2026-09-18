---
name: cerul
description: Search local videos by meaning, exact words, or a reference image, and annotate actions, events, interactions, and states in videos or LeRobot demonstrations. Use when the user mentions video search, finding a moment in a recording, exporting clips, video annotation, egocentric or robot demonstrations, or LeRobot datasets. Requires the cerul command-line tool.
generated-by: cerul 0.0.16
generated-sha256: b6490deb827d2f3fd75fbd5e44f45bf3f304bb46ac21a35cb468b5c045266d94
---

# Cerul

Cerul turns local video into searchable, annotated data using the user's own
model endpoints. There is no account and no server to start: run the binary and
read its output. Sidecar files beside each video are authoritative; search
indexes are caches that rebuild from them without model calls.

## Before anything else

Run `cerul --version`. If it is missing, follow the installation runbook at
https://github.com/cerul-ai/cerul/blob/main/docs/agent-setup.md. Do not install
Rust, Python, Ollama, or system FFmpeg for this workflow, and do not switch to a
source build when a download fails; report the error instead.

`cerul --json upgrade` reports the newest published release and installs
nothing. It replaces the program only with `--yes`, so ask the user before
running `cerul upgrade --yes`: it changes which build answers every later
command. A build too old for a flag you need is a reason to offer the upgrade,
not to work around it.

Run `cerul --json auth` to see whether a key is available. It reports whether a
key is saved or exported, never the value. Never echo a key, print part of it,
write it to a log or a commit, or read an unrelated project's `.env`. When no key
is available, ask the user to run `cerul auth set` in their own terminal, or to
export the key themselves.

## Always pass --json

`--json` is the machine contract, and it is the only mode to use:

- stdout carries exactly one JSON object, printed when the command finishes.
- stderr carries one JSON event per line while the command runs.
- Nothing ever prompts. A missing argument returns `invalid_arguments`, not a
  question.

Exit codes: `0` success, `2` arguments or configuration, `3` unavailable
dependency or capability, `4` execution failure, `5` cancelled, `6` partial
success. Read the code as well as the output. **Exit 6 is not success**: part of
the work finished and the rest did not.

Events on stderr:

- `progress` counts units of work for one station.
- `annotation_progress` reports annotation work units, cached units and phase;
  percentage is completed planned work, not model-internal progress.
- `checkpoint` marks a window that is saved and will be reused after an
  interruption, so a rerun resumes rather than repeating it.
- `published` marks a validated annotation file that now exists, with its record
  count and path. Only a published file is safe to read or train on.
- `log` carries human-readable notices.

A **partial `annotate`** carries `retry` in its final object. Run `retry.argv`
verbatim to continue: it repeats the original invocation and changes only what
the failure calls for. Other commands that can exit 6, such as `index` and
`status --providers`, carry no `retry`; run the same command again, which reuses
everything that finished. A cancelled run has no result object at all, only
`{"error":{"code":"cancelled",...}}` and exit 5; the same command resumes it.
Never add `--recompute` to any of these; it discards finished work.

## Workflows

Preview any expensive command with `--dry-run` first. It writes nothing, calls no
model, and asks for no credential.

**Search a video.** Indexing is required for search, and it sends media to the
configured endpoints.

```sh
cerul --json --dry-run index ./video.mp4
cerul --json index ./video.mp4
cerul --json search "a person picking up a cup" --in ./video.mp4
cerul --json search --text "ERROR 500"
cerul --json search "a person picking up a cup" --save ./clips
```

Result numbers are global, so `cerul open 1` plays the first moment in the user's
video player and `cerul open 3` plays the third. `score` is a ranking similarity, not a probability that the
moment is the right one; do not present it as a confidence or an accuracy.

Indexing builds video embeddings, OCR, and available speech/text search data.
It does not call the vision model or generate scene descriptions, sections, or
summaries. Use `cerul analyze ./video.mp4` for scenes and an overview, or `cerul annotate ./video.mp4` for embodied labels. Existing analysis and cached
description vectors remain readable and searchable; indexing does not refresh
them. Search suggestions reuse current evidence without model generation.
Do not treat suggested queries as verified retrieval results.

```sh
cerul --json status ./video.mp4 --timeline --type summary
cerul --json status ./video.mp4 --timeline --type scene
```

Visual descriptions, spoken words, and screen text have distinct provenance.
Use scene evidence for what is visible, transcript evidence for what was said,
and OCR for visible words. Sparse visual samples cannot establish exact motion
boundaries, success, intent, or camera trajectories. Scene descriptions do not
replace the task and action annotations below.

**Inspect performance.** `cerul --json diagnostics` reads the latest index/analyze
stage timings, request latency and cache counts. Stage wall times overlap; use
invocation elapsed time for total throughput. This makes no model calls.

**Analyze a video.** No indexing step is needed.

```sh
cerul --json analyze ./video.mp4
cerul --json --dry-run analyze ./dataset --only 0
```

Without options, returns scenes, chapters and an overview. Add `--prompt "Question"`
for a focused answer, repeat `--image ./reference.png` for comparison images, and
use `--from 00:30 --to 01:10` for a half-open episode time range. No embeddings
are generated. Focused answers use at most 120 sampled frames and valid cached
in-range text; sparse samples cannot establish continuous motion or absence.
Results include the actual sample timestamps, reference hashes and limitations.
Reference images are not evidence of occurrence in the video.

`--stream --json` emits provisional `analysis_delta` NDJSON on stderr; stdout
contains only the final structured report. Never treat a delta as validated
evidence. Exit 6 and per-stream errors mean incomplete analysis. Cached answers
emit one delta marked `cached: true`. Question-specific results preserve full
video scene/overview records; `--recompute` refreshes the selected request.
The fixed response schema is published; arbitrary user JSON schemas are not
accepted. Preserve errors and coverage when interpreting results.

**Annotate actions.** No indexing step is needed.

```sh
cerul --json --dry-run annotate ./video.mp4
cerul --json annotate ./video.mp4
cerul --json status ./video.mp4 --timeline
```

Annotation is embodied-only for every input format, defaulting to
subtask,event,interaction,state. Explicit --semantic types can also include task,
flag and progress. Use analyze for general-video understanding. `--hands` adds
local human-hand keypoints; add `--semantic none` for hand-only processing.
Hands are never enabled automatically. Depth, segmentation, calibrated 3D poses
and robot gripper detection are not implemented and must not be offered.

Each episode exports `annotations.json` and `summary.md` with Cerul provenance.
Internal typed sidecars remain authoritative. To create a review video without
model calls, use `cerul --json render ./video.mp4 --out ./video.annotated.mp4`.
Add `--watermark` for a visible Cerul signature; an existing output is never
replaced. Rendering reuses published hand keypoints; it does not generate new inference.

**Annotate a LeRobot dataset.** Pass the dataset root that contains `meta/`.

```sh
cerul --json annotate ./dataset --only 0
cerul --json status ./dataset --timeline
```

Start with one episode. Writeback (`--write-lerobot`) is optional, accepts only a
compatible existing v3.1 dataset, and never upgrades a format; read
https://github.com/cerul-ai/cerul/blob/main/docs/lerobot.md before using it.

**Read what was produced.** `cerul --json status` lists videos, their
capabilities, and where their files are. `cerul --json status PATH --timeline`
returns published annotation records in time order, with `--type` and `--limit`.

## Rules

- Ask the user before any command that sends media to a model endpoint, and say
  that their provider bills it. Local OCR is the exception: it runs on the CPU.
- Do not run `status --providers` unless asked; it makes real model requests.
- Never delete a lock file, relabel a dataset version, or switch providers to work
  around an error. Report the error and its category.
- Keep one `--workspace` for a whole task. Do not switch it to escape a lock.
- Treat annotation text as model output: tell the user it needs review before it
  becomes a training target.
- An empty search result is a normal outcome, not a failure. Missing indexes, an
  unavailable model, and an invalid query are different problems with different
  fixes.

## Command reference

Generated from this build's argument definitions.
These options work on every command:

  --no-auto-deps              Check media tools without downloading or selecting automatic repairs
  --json                      Machine-readable output: final JSON on stdout, NDJSON events on stderr
  --workspace <DIR>           Where indexes and caches live (default ~/.cerul)
  --dry-run                   Show what would happen without writing anything or calling models
  --yes                       Skip confirmations and never prompt; fail instead of asking for a key
  --quiet                     Only print errors
  --recompute                 Redo work even when a valid result already exists
  --set <KEY=TOML_VALUE>      Override a configuration field, for example embedding.dims=3072
  -v                          More diagnostic output (repeatable)

### cerul index

Index videos so they can be searched (screen text, speech, visual search)

  <PATHS>...                  Videos, directories, or LeRobot datasets
  --no-audio                  Skip speech transcription
  --no-ocr                    Skip screen text recognition
  --chunk <DURATION>          Length of each searchable window, for example 30s (default 30s)
  --overlap <DURATION>        Overlap between windows (default 5s)
  --skip-still                Skip windows where the picture does not change
  --streams <STREAMS>         Streams to index, comma-separated (default primary)
  --only <ONLY>               Only these episodes (ids or local indexes), comma-separated
  --jobs <JOBS>               Maximum model requests in flight across indexing stages (default 4)
  --rpm <RPM>                 Shared cap on model requests per minute across indexing stages
  --sidecar-dir <DIR>         Store sidecars here instead of beside the videos

### cerul search

Find moments by description, exact words, or a reference image

  <QUERY>                     What to look for, in plain words, wrapped in quotes
  --image <FILE>              Search with a reference image instead of words
  --text                      Match the exact words in screen text or speech instead of by meaning
  --save <DIR>                Save each matching moment as an MP4 clip in this directory
  --preview                   Show a still frame for each result (default when the terminal supports images)
  --no-preview                Never show still frames
  --limit <N>                 Maximum number of results (default 10)
  --in <PATH>                 Only search videos under this path
  --pad <DURATION>            Seconds of context added before and after each saved clip (default 2s)
  --filter <KEY=VALUE>        Restrict by annotation field, for example semantic.event.verb=pour
  --threshold <THRESHOLD>     Minimum raw cosine for vector matches; full-query lexical matches use a separate gate
  --count                     Count matching intervals instead of listing them

### cerul status

Show indexed videos, model configuration, and storage

  <PATH>                      Only report videos under this path
  --providers                 Verify configured model endpoints with small test requests (cached for seven days)
  --timeline                  Read the published annotations in time order instead of the summary
  --type <ITEM>               One semantic item (for example event), or hand to expand hand frames
  --limit <N>                 Most annotation records to show per video (default 50)

### cerul diagnostics

Read saved stage timings, request latency and cache counts (no model calls)


### cerul open

Open a result from the last search in a video player, at its moment

  <NUMBER>                    Result number shown by the last search (default 1)

### cerul auth

Manage the saved Gemini API key

  cerul auth set                   Enter and verify a Gemini API key, replacing any saved one
  cerul auth remove                Delete the saved Gemini API key
  cerul auth help                  Print this message or the help of the given subcommand(s)

### cerul config

Configure the required Gemini key and optional default speech transcription

  --show                      Print the effective configuration (defaults, saved file, and overrides) without prompting

### cerul analyze

Analyze scenes, chapters, and a grounded overview without indexing

  --prompt <TEXT>             Ask a question or specify the desired analysis
  --image <PATH>              Reference image, repeat up to four times (PNG or JPEG)
  --from <TIME>               Start on the episode timeline, e.g. 00:30 or 30s
  --to <TIME>                 Exclusive end on the episode timeline, e.g. 01:10
  --stream                    Stream answer text; with --json, deltas go to stderr and final JSON to stdout
  <PATHS>...                  Videos, folders, or LeRobot datasets
  --streams <STREAMS>         Camera selection: primary, all, or comma-separated ids (default primary)
  --only <ONLY>               Selected episode ids or local indexes
  --jobs <JOBS>               Parallel model requests (default 4)
  --rpm <RPM>                 Model requests per minute

### cerul annotate

Generate semantic labels and optional local hands for embodied videos

  <PATHS>...                  Videos, directories, or LeRobot datasets
  --semantic <ITEMS>          Semantic items, comma-separated; use none with --hands for offline hand annotation
  --hands                     Add local human-hand keypoints to an embodied demonstration (CPU, no API calls)
  --write-lerobot             Write subtask annotations back into the LeRobot dataset
  --out <DIR>                 New output LeRobot dataset (requires --write-lerobot)
  --ontology <FILE>           Custom ontology file
  --window <WINDOW>           Model window length, for example 30s (default 30s)
  --fps <FPS>                 Frames per second sampled for the model (default 2)
  --jobs <JOBS>               Parallel model requests (default 4)
  --rpm <RPM>                 Cap on model requests per minute
  --streams <STREAMS>         Streams to annotate, comma-separated (default primary)
  --only <ONLY>               Only these episodes (ids or local indexes), comma-separated

### cerul render

Render published semantic labels and hand skeletons without model calls

  <PATH>                      Annotated video, or an exported annotations.json for a dataset episode
  --out <MP4>                 New review video (must not already exist)
  --stream <STREAM>           Camera id (defaults to the primary camera)
  --watermark                 Burn a visible Cerul signature into the caption panel

### cerul remove

Remove indexed videos, or free the disk they and their caches use

  <PATHS>...                  Videos or directories to forget. Their files are left where they are
  --cache                     Free cached previews, query vectors, and media proxies; all regenerated
  --all-indexes               Free every search index; each rebuilds from sidecars with no model calls
  --index <SPACE>             Free one search index by space id; rebuilt from sidecars on next use
  --compact                   Reclaim space inside search indexes without dropping them

### cerul upgrade

Install the newest published release of Cerul over this one


### cerul skill

Print or install the agent skill that teaches this CLI to a coding agent

  --install <AGENT>           Install for this agent: claude, codex, or pi
  --dir <DIR>                 Install into this skills directory instead of an agent's own
  --print                     Write the skill to stdout instead of installing it
  --force                     Replace an installed skill even when it was changed after it was written

### cerul help

Print this message or the help of the given subcommand(s)

  cerul help index                 Index videos so they can be searched (screen text, speech, visual search)
  cerul help search                Find moments by description, exact words, or a reference image
  cerul help status                Show indexed videos, model configuration, and storage
  cerul help diagnostics           Read saved stage timings, request latency and cache counts (no model calls)
  cerul help open                  Open a result from the last search in a video player, at its moment
  cerul help auth                  Manage the saved Gemini API key
  cerul help config                Configure the required Gemini key and optional default speech transcription
  cerul help analyze               Analyze scenes, chapters, and a grounded overview without indexing
  cerul help annotate              Generate semantic labels and optional local hands for embodied videos
  cerul help render                Render published semantic labels and hand skeletons without model calls
  cerul help remove                Remove indexed videos, or free the disk they and their caches use
  cerul help upgrade               Install the newest published release of Cerul over this one
  cerul help skill                 Print or install the agent skill that teaches this CLI to a coding agent
  cerul help help                  Print this message or the help of the given subcommand(s)
