# Annotate actions and demonstrations

Use Cerul to describe what happens in ordinary videos, egocentric recordings,
or LeRobot robot demonstrations. You can annotate media directly: **indexing is
not required**. These are semantic labels, not pose, depth, or 3D trajectories.

## Start with one video

Save your Gemini API key with `cerul auth set`, or configure your own
[vision endpoint](configuration.md). Sampled frames are sent to that endpoint;
model processing may incur API charges.

For embodied action steps, events, interactions, and state changes:

```sh
cerul annotate ./video.mp4 --embodied
```

Replace the path with your recording, or pass a directory to process its videos.
To see the plan before making model calls or writing results:

```sh
cerul annotate ./video.mp4 --embodied --dry-run
```

For a smaller default set of task, subtask, and quality flag labels:

```sh
cerul annotate ./video.mp4
```

No mode flag means general video, regardless of storage format. `--embodied`
selects demonstration-specific prompts and four default labels. Optional
`--semantic task,event` overrides the labels without changing the mode. Mode,
prompt recipe and ontology are included in cache identity.

## Optional local hands for embodied demonstrations

General video annotation never runs hand inference. `--embodied` changes semantic
prompts only; add `--hands` explicitly for videos containing human hands. Human
hands are not robot grippers. `--hands` without `--embodied` is an argument error.

```sh
cerul annotate ./video.mp4 --embodied --hands
cerul annotate ./video.mp4 --embodied --hands --semantic none
```

The first command combines cloud semantic labels with local hands. The second
runs only local CPU inference and needs no API key or network. Both hand models
are bundled; no Python, OpenCV, GPU runtime or model download is required.
`--fps` controls semantic sampling only; hands use every observed source frame.

The `grounding.hand` track stores normalized display-image XY in MediaPipe's
21-joint order, hand-presence confidence and handedness with a separate score.
Handedness is the model prediction, not guaranteed identity. Missing/out-of-frame
points are null; an empty hands array records a frame without an accepted hand.
Track IDs use temporal association and may change after occlusion; they are not
persistent person identifiers. Hand-centered 3D, depth and grippers are excluded.

Frame times retain integer-microsecond source PTS, including variable-frame-rate
and rotated media. Completed five-second chunks are checkpointed with model,
source and tracker identity. A rerun resumes them; index rebuilding never runs
hand inference. `annotations.json` includes the hand track; `summary.md` reports
coverage without printing every coordinate. `render` overlays available skeletons
and does not call inference. Cyan means predicted left; amber means predicted right.

## Choose the labels you need

| Type | Describes |
| --- | --- |
| `task` | The overall activity or goal |
| `subtask` | Steps within the task, with temporal boundaries |
| `event` | Actions and events at particular moments |
| `interaction` | Interactions between people, objects, and the environment |
| `state` | Observed states and state changes |
| `flag` | Quality issues and noteworthy conditions |
| `progress` | Progress through the task |

To request all seven types for an ordinary video, list them explicitly:

```sh
cerul annotate ./video.mp4 --semantic task,subtask,event,interaction,state,flag,progress
```

Model-generated labels should be inspected before use as training targets.
See [annotation schemas](../schemas/annotation-record.json) for the record format.

## Start with one LeRobot episode

Pass the dataset root, containing its `meta/` directory. Select embodied mode
explicitly, just as for an ordinary demonstration video:

```sh
cerul annotate ./dataset --embodied --only 0
```

`--only 0` selects episode index 0. Omit it to process all episodes. The default
camera selection is `--streams primary`; use `--streams all` to annotate every
video stream. Annotations for each stream are stored separately.

For subtask annotations only:

```sh
cerul annotate ./dataset --semantic subtask --only 0
```

This normally creates sidecars without changing the dataset's original action
or state data. Optional subtask writeback has additional compatibility rules:
follow the [LeRobot subtask tutorial](lerobot-subtasks.md) and
[writeback compatibility guide](lerobot.md) before using `--write-lerobot`.

## Read the labels

```sh
cerul status ./video.mp4 --timeline
```

This prints the published semantic records in time order, one line each, so you can judge
a run without opening a JSONL file. Narrow it with `--type event` and lengthen it
with `--limit 200`. It reads sidecars only: no model call and no network.

Hand tracks remain listed, but their per-frame records do not consume the default
timeline limit. Expand them explicitly with `--type hand` (aliases: `hands` and
`grounding.hand`):

```sh
cerul status ./video.mp4 --timeline --type hand --limit 200
```

## Find and inspect the results

```sh
cerul status ./video.mp4
cerul status ./dataset
```

The completion receipt shows two files in the episode's sidecar:

- `annotations.json`: one portable bundle of current semantic tracks, source
  hashes, integer-microsecond times, per-track model provenance and incomplete
  modules. `$cerul: "annotations/1"` and `generator` identify Cerul. Its schema
  is generated from Rust: [annotations.json](../schemas/annotations.json).
- `summary.md`: the same annotations as a readable timeline, with a Cerul footer
  and the exact export generation. It is a derived view, not edited ground truth.

Typed `semantic.<type>.jsonl` records remain authoritative under
`.internal/annotations/`; recovery products and new semantic checkpoints live
under `.internal/recovery/` and `.internal/checkpoints/`. Non-primary cameras
have their own `streams/<camera>/.internal/` directories. Legacy annotations
remain readable and migrate after successful publication. Old checkpoints are
still accepted and retained; unrelated user files are never swept away.
The episode descriptor and index-generated evidence keep their existing paths.
Search, timeline and index rebuilds read both layouts without model calls.

- Ordinary videos normally use a sibling directory such as `video.mp4.cerul/`.
- LeRobot uses `<dataset>/.cerul/episodes/<episode_index>/`, with separate
  subdirectories for additional streams.
- If the source changes or its directory is not writable, the sidecar location
  can differ. Use the path reported by status.

`--out` is **not** a directory for exporting arbitrary JSONL files. It requires
`--write-lerobot` and selects a new output LeRobot dataset, leaving the source
intact. Writeback is limited to compatible datasets; annotation support alone
does not imply writeback support.

Rerun the same annotation command after an interruption to resume completed
work. Use `cerul annotate --help` for window length, frame sampling, camera
selection, concurrency, and other options.


## Render a review video

```sh
cerul render ./video.mp4 --out ./video.annotated.mp4
cerul render ./video.mp4 --out ./video.branded.mp4 --watermark
```

Rendering reads published annotations without contacting a model. It adds a
caption panel below the original image, preserving the original file, source
frame timing and audio alignment. Subtask captions take priority; other semantic
records fill gaps. The compact built-in font renders English ASCII captions;
long captions are truncated in the preview, while the JSON and summary retain
full text. A new MP4 is published atomically and an existing output is never
overwritten. Cerul version and annotation generation are recorded in MP4
metadata; the visible signature is opt-in.

For a dataset episode, pass its exported bundle and optionally a camera:

```sh
cerul render ./dataset/.cerul/episodes/0/annotations.json --stream observation.images.front --out ./episode-0.mp4
```

Camera timelines with non-unit time scaling are rejected. Source hashes and
annotation provenance must still match. A rendered MP4 is for inspection and
sharing; raw annotations remain available for training workflows. Hands require explicit `--embodied --hands` annotation before rendering.
Depth is not implemented. `--embodied` alone never enables hand inference.

## Progress and resuming

The terminal shows the completed percentage, elapsed time, current phase and an
approximate remaining time after enough work has been measured. Percent counts
planned work units, including the subtask description pass and final publication;
it is not a prediction of the model's internal completion. Cached work is counted
separately and excluded from throughput estimates. Partial runs never report
successful 100% completion. `--json` emits `annotation_progress` on stderr while
stdout remains one final result object.

Compatibility change: LeRobot no longer implicitly selects all seven types or
enforces the manipulation verb list. Use an explicit seven-type `--semantic`
selection to retain that selection, and `--ontology FILE` for strict verbs.
