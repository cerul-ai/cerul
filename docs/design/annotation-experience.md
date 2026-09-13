# Annotation experience proposal

Status: design rationale. Explicit modes, aggregate progress, portable exports,
private semantic storage and semantic review-video rendering are implemented.
Perception integration remains proposed. DESIGN.md is the implementation baseline.

## Observed problems

The current guide's demonstration choice only selects semantic subtypes; it
does not select a domain-specific processing mode. Default types and verb
validation depend on whether the input is LeRobot, conflating storage format
with annotation intent.

A locally inspected 10.77-second sample has four published semantic files,
four recovery products named `*.conflicts.json`, one episode manifest, and five
checkpoints. The recovery products include complete annotations, not just
conflicts. They participate in recovery and must not be deleted as clutter.

Semantic progress is emitted after a completed uncached window, with no initial
zero event and no event for reused windows. A short video has one window per
subtype; subtask uses an extra description request. Per-subtype progress can
therefore appear only when processing is already finished. The renderer has
stage estimates, but this event stream gives it insufficient observations.

## Explicit domain selection

Proposed invocations:

```sh
cerul annotate ./video.mp4
cerul annotate ./demo.mp4 --embodied
cerul annotate ./demo.mp4 --embodied --semantic subtask,event
```

Without `--embodied`, default to general semantic annotation: task, subtask,
flag. With it, default to subtask, event, interaction, state. Explicit
`--semantic` selections override types, not the domain. Keep existing explicit
subtype invocations valid. Do not let the flag imply hand or depth inference,
dataset conversion, or LeRobot writeback.

Apply the explicit-mode rule to LeRobot inputs too; announce the change from
the previous seven-type default in release notes. Dataset detection still
controls reading, camera selection, and writeback compatibility. The guide
defaults to general video, offers an embodied toggle, displays the resolved
mode/types, and starts directly after input selection. Keep dry-run explicit.

Version domain-specific prompts and include mode, ontology, and recipe in
cache identity. General prompts must allow videos without a task or action;
empty annotations are valid and should explain their lack of applicability.
Embodied prompts target observable actions, actors, objects, contacts, and
state changes, without inferring unavailable robot actions or sensor state.
Do not automatically enforce the current manipulation verb list: human demos
may contain actions such as cutting and washing. Explicit ontology files
remain the source of strict vocabulary validation.

## Honest progress and estimates

Plan episodes, streams, windows, model passes, and publication before work.
Emit initial state and cached completions. Display one aggregate line and the
active phase, not one accumulating line for every subtype.

Percent means completed planned work units, not elapsed-time fraction or
annotation quality. Include both subtask passes and final publication units;
do not show successful completion until validation and atomic publication
finish. Show failures separately from successful units on partial completion.

An in-flight model request has a spinner and elapsed time. Never animate
fictional model percentages. Estimate remaining time using measured comparable
requests; before sufficient evidence, show `Estimating…`. Optionally use local
historical timings keyed by provider/model/recipe and input-size bucket,
clearly marked approximate. Exclude cached work from throughput; account for
rate-limit waits and retries without enlarging the successful-work count.
Keep JSON stdout as one final object and progress events on stderr.

## A readable result over durable data

First ship a compact receipt and an explicit timeline view using the existing
`status <path> --timeline` interface. Show total records, subtype counts, a
single result location, and meaningful warnings; reserve file inventories for
detail/JSON modes. Successful publication does not imply human-reviewed quality.

An illustrative receipt for the inspected sample:

```text
✓ Annotated 1.mp4 · 13 records
  4 subtasks · 2 events · 5 interactions · 2 state changes
  Results: 1.mp4.cerul/
  Timeline: cerul status ./1.mp4 --timeline
```

Implemented semantic layout:

```text
1.mp4.cerul/
  summary.md
  episode.json
  annotations.json
  .internal/
    annotations/
      semantic.subtask.jsonl
      semantic.event.jsonl
      semantic.interaction.jsonl
      semantic.state.jsonl
    recovery/
    checkpoints/
```

`summary.md` is a regenerated human view; typed sidecars remain authoritative.
Keep recovery snapshots necessary to reproduce conflict flags even after
successful publication. A hidden directory is organization, not a promise that
its content is disposable. Preserve old layout reads, use a layout version and
recoverable migration, update every reader/index-rebuilder/writeback path, and
test interruption boundaries before moving files. Do not change existing
sample data during the design phase. An optional single-file export may ease
sharing but is not a new competing source of truth.

## Perception and rendered video

Proposed future workflow:

```text
cerul annotate ./demo.mp4 --embodied --grounding keypoint --world depth
cerul render ./demo.mp4 --overlay subtask,hand --depth-panel --out ./demo.annotated.mp4
```

Hand/depth inference and the perception composition options above are currently
unavailable. Semantic-only `render` is implemented. Define and implement a concrete
perception endpoint contract before exposing the options. The Rust CLI should
orchestrate configurable endpoints and local composition without bundling a
Python/PyTorch runtime. Missing perception must not block semantic-only work.
Explicitly requested unavailable perception must fail preflight before billing.

Start with 2D hand keypoints, stable tracks, handedness, confidence, visibility,
and source timestamps. Human-hand detection does not cover robot grippers.
Treat occluded or missing joints as missing, not zero-valued positions. Add
relative video depth with validity masks and temporally consistent scaling.
Persist numerical depth independently of its colorized preview. Metric depth
requires an explicit scale/unit contract and separate validation. Hand-local
3D points are not automatically camera/world-frame robot coordinates.

Rendering reads published annotations without model calls. First deliver
semantic labels on video; later add skeletons and a depth panel. Preserve source
PTS, episode offsets, audio synchronization, and variable-frame-rate behavior.
Write a new video atomically; retain original video and raw annotations. A
burned-in MP4 is a review artifact, not a replacement for training data. Camera
calibration, robot action/state, and synchronized sensors cannot be recovered
merely by compositing an overlay.

Candidate references, not chosen integrations:

- [MediaPipe hand landmarks](https://ai.google.dev/edge/api/mediapipe/python/mp/tasks/vision/HandLandmarkerResult)
- [Video Depth Anything](https://github.com/DepthAnything/Video-Depth-Anything)

## Delivery and acceptance

1. Explicit modes, initial/aggregate progress, honest ETA, compact receipt and
   readable timeline. Test argument precedence across video/LeRobot, mode cache
   invalidation, one-window and multi-window work, cached resume, retries,
   partial publication, redirected output, and JSON separation.
2. Semantic video rendering and readable summary; then storage migration with
   legacy-read, interrupted-publication and zero-model-call rebuild checks.
3. Endpoint-backed hands/depth, then composition. Evaluate real first-person
   hand occlusion, left/right and track stability, temporal depth flicker,
   valid scale, and audiovisual synchronization. Verify per-frame timestamp
   alignment rather than relying on visually plausible examples alone.

Implementation changes require repository fmt/clippy/test gates. Perception
release acceptance additionally requires both supported platforms and real
endpoint inference; schema-only evidence is insufficient. Existing release
gates, including official LeRobot loader round-trips, still apply.
