---
name: cerul
description: Search local videos by meaning, exact words, or a reference image, and annotate actions, events, interactions, and states in videos or LeRobot demonstrations. Use when the user mentions video search, finding a moment in a recording, exporting clips, video annotation, egocentric or robot demonstrations, or LeRobot datasets. Requires the cerul command-line tool.
generated-by: cerul 0.0.6
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
- `checkpoint` marks a window that is saved and will be reused after an
  interruption, so a rerun resumes rather than repeating it.
- `published` marks a validated annotation file that now exists, with its record
  count and path. Only a published file is safe to read or train on.
- `log` carries human-readable notices.

When a run ends partial or cancelled, the final object carries `retry`. Run
`retry.argv` verbatim to continue: it repeats the original invocation and changes
only what the failure calls for. Never add `--recompute` to a retry; it discards
finished work.

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

**Annotate actions.** No indexing step is needed.

```sh
cerul --json --dry-run annotate ./video.mp4 --semantic subtask,event,interaction,state
cerul --json annotate ./video.mp4 --semantic subtask,event,interaction,state
cerul --json status ./video.mp4 --timeline
```

Ordinary videos default to `task,subtask,flag`; list the items explicitly to get
the rest. Available items: task, subtask, event, interaction, state, flag,
progress. Pose, depth, segmentation, and 3D trajectories are not implemented and
must not be offered.

**Annotate a LeRobot dataset.** Pass the dataset root that contains `meta/`.

```sh
cerul --json annotate ./dataset --semantic --only 0
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

  --json                      Machine-readable output: final JSON on stdout, NDJSON events on stderr
  --workspace <DIR>           Where indexes and caches live (default ~/.cerul)
  --dry-run                   Show what would happen without writing anything or calling models
  --yes                       Skip confirmations and never prompt; fail instead of asking for a key
  --quiet                     Only print errors
  --recompute                 Redo work even when a valid result already exists
  --set <KEY=TOML_VALUE>      Override a configuration field, for example embedding.dims=1536
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
  --jobs <JOBS>               Parallel model requests (default 4)
  --rpm <RPM>                 Cap on model requests per minute
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
  --threshold <THRESHOLD>     Minimum similarity score for semantic matches
  --count                     Count matching intervals instead of listing them

### cerul status

Show indexed videos, model configuration, and storage

  <PATH>                      Only report videos under this path
  --providers                 Verify configured model endpoints with small test requests (cached for seven days)
  --timeline                  Read the published annotations in time order instead of the summary
  --type <ITEM>               Only one semantic item, for example event
  --limit <N>                 Most annotation records to show per video (default 50)

### cerul open

Open a result from the last search in a video player, at its moment

  <NUMBER>                    Result number shown by the last search (default 1)

### cerul auth

Manage the saved Gemini API key

  cerul auth set                   Enter and verify a Gemini API key, replacing any saved one
  cerul auth remove                Delete the saved Gemini API key

### cerul annotate

Generate semantic annotations (tasks, events, states) for videos

  <PATHS>...                  Videos, directories, or LeRobot datasets
  --semantic <ITEMS>          Semantic items to generate, comma-separated (default set when no value is given)
  --write-lerobot             Write subtask annotations back into the LeRobot dataset
  --out <DIR>                 New output LeRobot dataset (requires --write-lerobot)
  --ontology <FILE>           Custom ontology file
  --window <WINDOW>           Model window length, for example 30s (default 30s)
  --fps <FPS>                 Frames per second sampled for the model (default 2)
  --jobs <JOBS>               Parallel model requests (default 4)
  --rpm <RPM>                 Cap on model requests per minute
  --streams <STREAMS>         Streams to annotate, comma-separated (default primary)
  --only <ONLY>               Only these episodes (ids or local indexes), comma-separated

### cerul remove

Remove indexed videos, or free the disk they and their caches use

  <PATHS>...                  Videos or directories to forget. Their files are left where they are
  --cache                     Free cached previews, query vectors, and media proxies; all regenerated
  --all-indexes               Free every search index; each rebuilds from sidecars with no model calls
  --index <SPACE>             Free one search index by space id; rebuilt from sidecars on next use
  --compact                   Reclaim space inside search indexes without dropping them

### cerul skill

Print or install the agent skill that teaches this CLI to a coding agent

  --install <AGENT>           Install for this agent: claude, codex, or pi
  --dir <DIR>                 Install into this skills directory instead of an agent's own
  --print                     Write the skill to stdout instead of installing it
