# Annotate actions and demonstrations

Use Cerul to describe what happens in ordinary videos, egocentric recordings,
or LeRobot robot demonstrations. You can annotate media directly: **indexing is
not required**. These are semantic labels, not pose, depth, or 3D trajectories.

## Start with one video

Save your Gemini API key with `cerul auth set`, or configure your own
[vision endpoint](configuration.md). Sampled frames are sent to that endpoint;
model processing may incur API charges.

For action steps, events, interactions, and state changes:

```sh
cerul annotate ./video.mp4 --semantic subtask,event,interaction,state
```

Replace the path with your recording, or pass a directory to process its videos.
To see the plan before making model calls or writing results:

```sh
cerul annotate ./video.mp4 --semantic subtask,event,interaction,state --dry-run
```

For a smaller default set of task, subtask, and quality flag labels:

```sh
cerul annotate ./video.mp4 --semantic
```

The `--semantic` flag selects annotation processing. Giving it no value selects
the defaults; giving it a comma-separated list selects those types only.

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

Pass the dataset root, containing its `meta/` directory. Unlike ordinary videos,
LeRobot datasets default to all seven semantic types:

```sh
cerul annotate ./dataset --semantic --only 0
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

## Find and inspect the results

```sh
cerul status ./video.mp4
cerul status ./dataset
```

Status shows each registered episode's sidecar location. Files are named
`semantic.<type>.jsonl`, with a metadata header followed by annotation records.

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
