---
name: cerul
description: Search local videos by meaning, exact words, or a reference image, and annotate actions, events, interactions, and states in videos or LeRobot demonstrations. Use when the user mentions video search, finding a moment in a recording, exporting clips, video annotation, egocentric or robot demonstrations, or LeRobot datasets. Requires the cerul command-line tool.
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

Indexing also runs visual understanding by default. It saves visible scene
descriptions, coarse sections, and a grounded overview with up to three search
suggestions. This uses the configured vision endpoint in addition to embedding
and any enabled ASR. `--no-understanding` explicitly skips that work. Missing ASR
does not prevent visual descriptions; failed understanding can leave base search
usable while returning partial success. Do not treat suggested queries as
verified retrieval results.

```sh
cerul --json status ./video.mp4 --timeline --type summary
cerul --json status ./video.mp4 --timeline --type scene
```

Visual descriptions, spoken words, and screen text have distinct provenance.
Use scene evidence for what is visible, transcript evidence for what was said,
and OCR for visible words. Sparse visual samples cannot establish exact motion
boundaries, success, intent, or camera trajectories. Scene descriptions do not
replace the task and action annotations below.

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
