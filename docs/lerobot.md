# LeRobot compatibility

The adapter reads v3.0 and v3.1 video datasets. Metadata paths and per-episode
video ranges are resolved from `meta/info.json` and `meta/episodes/**/*.parquet`.
Data row ranges are local to their Parquet shard and are checked against the
episode's global index bounds and length. Camera IDs retain the official feature
keys; the lexicographically first video feature is the default primary camera.
Use `--streams all` or explicit feature keys to select other cameras.

A dataset receives a persistent UUID in `.cerul/dataset.json` when first published.
Read-only discovery and dry runs do not create this file. Each episode has a
separate `.cerul/episodes/<episode_index>/` sidecar, even when source videos are shared.
When the dataset cannot be written, its identity is saved in the workspace's
`datasets/` directory under a key derived from its canonical source path. Sidecars
then use `<workspace>/sidecars/<dataset_id>/<episode_index>/`. Repeated runs reuse
that identity and existing registered sidecar. Two read-only datasets at different
paths receive different IDs, even if their media is identical. Moving a read-only
dataset without its local identity creates a new identity in that workspace.
An explicit `--sidecar-dir` uses the same dataset/episode separation.

The format implementation is checked against Hugging Face LeRobot commit
[`2774d9bddcbbda50e697e162e89e7eaada8d7105`](https://github.com/huggingface/lerobot/tree/2774d9bddcbbda50e697e162e89e7eaada8d7105).
Relevant primary references:

- [Dataset metadata paths](https://github.com/huggingface/lerobot/blob/2774d9bddcbbda50e697e162e89e7eaada8d7105/src/lerobot/datasets/dataset_metadata.py)
- [Dataset writer and shard boundaries](https://github.com/huggingface/lerobot/blob/2774d9bddcbbda50e697e162e89e7eaada8d7105/src/lerobot/datasets/dataset_writer.py)
- [Language column types](https://github.com/huggingface/lerobot/blob/2774d9bddcbbda50e697e162e89e7eaada8d7105/src/lerobot/datasets/language.py)

The staged `write_out` library path has passed the pinned official loader on a
five-episode, 40-frame fixture, both with existing language columns and with no
language columns. The harness compares every original frame field, including
decoded images, actions, and state; it also checks untouched episodes and active
subtask timestamps. A rejected validator leaves the destination unpublished.
`annotate --semantic subtask --write-lerobot --out DIR` publishes a complete
output dataset. Without `--out`, the command retains rollback copies and a durable
replacement journal under `.cerul/writeback`. On interruption, the next writeback
restores the complete original group before retrying. Cerul readers reject a
pending replacement journal. Recovery checks all hashes before restoring files
and refuses to overwrite a change made outside the transaction.

Writeback requires successful primary-camera subtasks for every selected episode
in that dataset. Incomplete subtasks leave the dataset unchanged and produce a
partial result. `--out` accepts one source dataset and a new destination outside
the source. Dry runs publish nothing. Native checks guard each runtime write;
the official Python loader is an additional acceptance-test validator.

The pinned official recorder emits `codebase_version: v3.0`, even though this
source revision includes the language-column API. The acceptance fixture adds
those canonical columns and explicitly labels its synthetic compatibility case
v3.1. This is not evidence that an official v3.1 recorder or upgrade command
exists. Cerul does not upgrade user datasets.

To reproduce the existing-language case, install the pinned LeRobot checkout
with its dataset and PyAV dependencies in a separate validation environment:

```sh
python tests/lerobot_roundtrip.py create /tmp/cerul-acceptance-source
HF_HOME=/tmp/cerul-acceptance-cache cargo run --locked --example verify_lerobot_writeback -- \
  /tmp/cerul-acceptance-source /tmp/cerul-acceptance-output /absolute/path/to/validation/python
```

Use fresh source and output paths. Python and the official loader are acceptance
test dependencies, not Cerul runtime dependencies.

Dataset reads and output copies take a shared advisory lock on the source
folder; in-place replacement and recovery take an exclusive lock on that same
folder. This coordinates separate workspaces without requiring writes to a
read-only source dataset. Lock conflicts fail immediately. The supported M1
filesystems must implement POSIX advisory locks; unrelated tools that ignore
those locks still require external coordination.

Copying a new output checks cancellation between file blocks and before
publication. A cancelled output stays unpublished and its temporary directory
is removed. Recovery of an already interrupted in-place transaction completes
its rollback before another operation can observe the dataset.
