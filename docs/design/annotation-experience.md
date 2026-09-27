# Annotation experience decisions

This page keeps the rationale for the shipped annotation workflow.
[DESIGN.md](../../DESIGN.md) is the implementation baseline; the
[annotation guide](../annotation.md) describes current commands.
The [original proposal](https://github.com/cerul-ai/cerul/blob/cce52298b3b594699ed45b4ce972c48acb6672f0/docs/design/annotation-experience.md)
records earlier alternatives, including the retired general annotation mode.

## Explicit domain selection

Annotation produces embodied labels. General-video scenes and overviews belong
to [analyze](../analyze.md). The old `--embodied` option remains a compatibility
no-op; it does not select a second mode. Dataset detection controls reading,
camera selection and writeback compatibility, not semantic intent.

The interactive guide asks for a path and uses the same defaults as a typed
command. Semantic types, hands, episode selection and cameras are explicit
options. Hand inference is opt-in and does not follow from choosing annotation.

Mode, prompt recipe and ontology participate in cache identity. Old general
records remain readable, but their checkpoints must not be reused as embodied
labels. Labels describe observable actions and state changes, not inferred
robot actions or sensor state. An explicit ontology governs verb validation.

## Honest progress and estimates

Count model passes and publication as work, including initial and cached units.
Cached units count as complete but do not establish processing throughput.
Partial publication must remain distinguishable from successful completion.

An in-flight model request cannot supply a measured completion percentage.
Human estimates must remain approximate; JSON events carry structured progress.
Keep machine output separate from terminal presentation. See the
[process contract](../configuration.md#process-contract) and
[progress and resuming](../annotation.md#progress-and-resuming).

## A readable result over durable data

The receipt, timeline and regenerated `summary.md` are human views. Typed
sidecars remain authoritative. The semantic layout separates published records
from private checkpoints and recovery snapshots under `.internal/`.

Recovery snapshots can contain complete annotations needed to reproduce
conflict flags. Neither a hidden directory nor a successful publication makes
those records disposable. Preserve legacy reads and interruption recovery
when changing storage. Index projections must rebuild without model calls.

Portable `annotations.json` and rendered videos help inspection and sharing;
they do not replace authoritative sidecars. Narrow semantic reruns retain valid
hand tracks. See [reading labels](../annotation.md#read-the-labels).

## Local hands and rendered video

Optional human-hand annotation uses the bundled OpenCV Zoo palm and landmark
ONNX models through CPU inference. Model provenance and limitations belong in
the [model documentation](../../models/hands/README.md).

Hands use observed source frames and integer-microsecond timestamps, including
rotation and variable frame rate. Completed chunks preserve matching tracker
state for resumption. Publication is atomic, and failed processing preserves
the previous published track. Missing detections remain empty; invalid points
remain null. Image-space keypoints are not calibrated world poses or grippers.

Rendering reads published tracks and adds hand skeletons and semantic captions.
The output is a review artifact, not evidence that labels were human-validated.
Depth and generic perception processing remain outside the implemented feature
set; see [scope and current limits](../development/scope.md).

## Delivery and acceptance

Storage and presentation changes require behavioral checks for cached resume,
partial and interrupted publication, legacy reads, JSON separation and
zero-model-call rebuilds. Hand changes additionally need real CPU inference,
per-frame timestamp checks and both supported platforms.

Occlusion, handedness and tracking stability require representative video
evaluation; fixture success alone does not establish accuracy. The
[release acceptance gates](../development/validation.md#release-acceptance),
including official LeRobot loader round-trips, still apply.
