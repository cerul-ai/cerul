# Scope and current limits

This page separates unavailable capabilities from the repository's boundaries.
It does not set a delivery schedule. Use the [user guides](../README.md) for
supported workflows and the [core design](../../DESIGN.md) for implementation
constraints.

## Not implemented

- HTTP and MCP serving.
- Windows distributions and additional target architectures.
- Grounding, depth, pose, segmentation, and other perception processing.
- Vision reranking and user-authored multi-camera episode directories.

Reserved configuration, CLI flags, and design sketches do not implement these
capabilities. Adding them requires an implementation and behavioral validation.

## Outside the repository's scope

Product Web/Desktop UI, hosted services, billing, operational administration,
training, and multi-tenant serving do not belong in this repository.

Other excluded interfaces and implementation choices include DAG/profile/Run
state machines, SSE, idempotency keys, bundle formats, ask/export/init/rm
commands, SQLite, a Python SDK/PyO3, embedded inference beyond OCR, Temporal,
knowledge graphs, chat UI, an action annotation family, and retargeting.
Do not introduce new orchestration frameworks, plugin systems, or storage engines
without a concrete requirement and a reviewed design.
