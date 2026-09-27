# CLI interaction decisions

This record preserves terminal design principles. Current commands and defaults
are documented in the [user guides](../../../docs/README.md) and
[DESIGN.md](../../../DESIGN.md). The
[original screen proposal](https://github.com/cerul-ai/cerul/blob/cce52298b3b594699ed45b4ce972c48acb6672f0/design/cli-interaction/v2/COMMANDS.md)
is historical: its general-annotation defaults, guided flows and dependency
suggestions must not be treated as an implementation backlog.

## Shared grammar

Human output should make the operation, useful result, important details and
next action easy to identify. Keep successful, partial, cancelled and failed
outcomes distinct. Use compact receipts and timelines instead of exposing
every internal file. Display names can be shortened; copyable commands and
JSON retain complete paths and source/time provenance.

Status and search should distinguish videos with identical filenames without
making every display path verbose. Scores describe retrieval similarity, not
the probability that an answer is correct. Progress estimates remain estimates;
completion follows validated publication.

## Lightweight guidance

The CLI uses lightweight terminal guidance, not a full-screen TUI or an embedded
agent. Guided actions use the same operation arguments and defaults as typed
commands. Parameter parsing, prompts, player launching and terminal output
belong to the binary; the library receives configuration and emits events.

Machine mode must not enter a guide or prompt for credentials. Missing or
invalid inputs follow the documented error and exit-code contract. The
[agent mode record](AGENT-MODE.md) explains that boundary.

## Verification

Changes to guidance need PTY and redirected-output checks: paths with spaces,
cancellation, missing credentials, and equivalent guided/typed operations.
Presentation changes must preserve JSON/NDJSON streams, exit codes and shell
quoting. Check narrow terminals, dark/light backgrounds and `NO_COLOR` when
changing layout or color.

The [validation guide](../../../docs/development/validation.md) is the current
verification procedure. Splitting CLI modules or replacing prompt dependencies
requires a concrete maintenance need; it is not a prerequisite left over from
the original screen proposal.
