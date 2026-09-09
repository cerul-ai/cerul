# Annotate a LeRobot dataset

Start with a local LeRobot video dataset whose license allows your intended
processing. Cerul reads the v3.0 and v3.1 metadata layouts and shared video
shards. See [compatibility and loader validation](lerobot.md) for the
exact upstream revision and the current v3.1 writeback limitation.

Install a complete bundle using the [installation guide](installation.md);
it includes FFmpeg, ffprobe, and OCR models. Configure model endpoints as
described in [configuration](configuration.md). Use a new workspace:

```sh
export CERUL_WORKSPACE="$PWD/robot-workspace"
cerul annotate ./dataset --only 0 --semantic subtask --dry-run
cerul annotate ./dataset --only 0 --semantic subtask
cerul status ./dataset
```

Annotation does not require an indexing step. These commands process episode 0
using the primary camera. The default primary
camera is the lexicographically first video feature. Use `--streams all` to
process every camera, or pass comma-separated feature keys. Model annotations
and vectors are stored per episode and stream; the original media, actions,
and state are preserved.

For all seven semantic modules, omit the item after `--semantic`:

```sh
cerul annotate ./dataset --semantic
cerul search --filter 'semantic.event.verb=regrasp' --in ./dataset
cerul search --filter 'semantic.event.verb=regrasp' --in ./dataset --count
```

The LeRobot default ontology validates event verbs against `cerul.verbs.v1`.
Time intervals use each episode's origin even when several episodes occupy the
same source MP4. A dataset UUID separates identically numbered episodes in
different datasets.

## Write subtasks to a new dataset

Writeback is opt-in and accepts only an input already identified as v3.1 with
the supported language-column contract. It does not upgrade v3.0. Check the
[compatibility note](lerobot.md) before using this operation: the pinned
upstream recorder still emits v3.0, and the v3.1 loader acceptance case is an
explicitly labeled compatibility fixture.

Use a new destination outside the source dataset:

```sh
cerul annotate ./dataset-v3.1 --semantic subtask \
  --write-lerobot --out ./dataset-with-subtasks
```

The complete output is staged and validated before publication. Only subtask
entries are added or replaced in `language_persistent`; existing actions,
state, other language entries, and unselected episodes are retained. Necessary
language feature metadata is added when a column is new. Event and flag
annotations stay in Cerul sidecars.

Every selected episode must have a successful primary-camera subtask result
before dataset writeback begins. A partial annotation result skips writeback.
Rerunning with intact completed sidecars reuses model results.

Omitting `--out` opts into in-place replacement with a recovery journal. Prefer
a separate output for initial use and validate it with your training loader.
The official-loader harness and reproduction commands are in
[developer validation](development/validation.md#official-lerobot-loader).

For action labels in ordinary videos or demonstrations, see the
[annotation guide](annotation.md).
