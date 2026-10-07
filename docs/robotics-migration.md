# Robotics migration

Cerul owns the reusable video engine and its general video CLI. Robot demonstration
workflows now live in [cerul-robotics](https://github.com/cerul-ai/cerul-robotics).
The source change does not modify existing installed binaries or user data.

| Capability | Owner |
| --- | --- |
| Media probing, frame/audio preparation, microsecond timelines, CPU OCR | Cerul |
| Endpoint clients, scene analysis, indexing, search, clips, storage and recovery | Cerul |
| Embodied task/subtask/event/interaction/state/flag/progress generation | Cerul Robotics |
| CPU human-hand inference, model weights, tracking and review-video rendering | Cerul Robotics |
| LeRobot readers, dataset identity and recoverable subtask writeback | Cerul Robotics |
| Robotics skill, semantic response schemas, prompts and loader acceptance | Cerul Robotics |
| Existing episode and annotation formats, validation and read compatibility | Cerul |

Use the separately installed `cerul-robotics` binary for annotate, render, and
LeRobot index/analyze operations. The hidden retired Cerul command names return
exit 3 with migration guidance; they do not install software or dispatch silently.
Existing sidecars are not deleted or rewritten by migration. Search, timeline
inspection and index rebuilds still read compatible previously published records.
Pass the existing `--workspace DIR` when deliberately reusing a previous registry.

Robotics links the Cerul Rust library at an exact Git revision. Its dataset adapter
enters through `index::discover::DatasetAdapter`, `index::pipeline::run_with_adapter`
and `analyze::run_with_adapter`. The core default rejects dataset roots rather than
mistaking shared dataset MP4 shards for independent ordinary videos.

Core has no Robotics dependency or embedded hand models. Shared record schemas and
legacy dataset sidecar placement stay in core for compatibility. Adding a robotics
feature must not add it back to the general CLI. Git history, existing tags and
`ffmpeg-vendor-*` release assets are retained. Product UI and hosted services remain
outside both processing repositories.
