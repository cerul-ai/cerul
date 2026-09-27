# Agent mode decisions

An agent operates the same Cerul binary as a person. The supported integration
is subprocess JSON, with a generated skill teaching that contract. Cerul does
not embed an agent or implement HTTP/MCP serving.

The [agent guide](../../../docs/agent.md) is the maintained contract for streams,
events, final objects, exit codes and retry instructions. The
[original proposal](https://github.com/cerul-ai/cerul/blob/cce52298b3b594699ed45b4ce972c48acb6672f0/design/cli-interaction/v2/AGENT-MODE.md)
preserves the initial event sketches and implementation plan; it is not another
schema or command reference.

## Machine contract

`--json` selects machine behavior without a separate agent flag. Final output
goes to stdout, structured events to stderr, and exit codes distinguish success,
failure, cancellation and partial completion. This path never asks interactive
questions. A checkpoint is durable work, not a published annotation; consumers
must distinguish the two.

Event types and result schemas originate in Rust. Regenerate `schemas/` through
the documented validation command rather than copying examples from a proposal.
Human renderers consume results and events without changing library behavior.

## Skill source of truth

The skill body is [prompts/skill.md](../../../prompts/skill.md); its command
reference comes from the current argument definitions. The generated
[skill](../../../skills/cerul/SKILL.md) must match `cerul skill --print`.

An installed generated skill carries a digest so upgrades can recognize their
own unchanged output. User-edited copies are preserved unless replacement is
explicitly requested. Agent-specific destinations do not introduce separate
command contracts. The [installation runbook](../../../docs/agent-setup.md)
covers setup without requiring repository knowledge.

## Verification

Pipe tests check JSON stdout, NDJSON stderr, exit codes and reparsable retry
arguments. Schema checks compare generated files with Rust types. Skill checks
verify generated content and command references. A real short-video agent run
on both supported platforms is separate acceptance evidence; parser and schema
checks alone do not establish processing success.
