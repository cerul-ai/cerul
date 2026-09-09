# Contributing

Start with the [architecture and module map](ARCHITECTURE.md), then read
[DESIGN.md](DESIGN.md) for the behavior and data invariants your change affects.
Keep the reusable core independent of process arguments and terminal rendering.

## Make and validate a change

1. Follow the [source build guide](docs/development/building.md).
2. Make a small, reviewable change with focused behavioral validation.
3. Run the [local checks](docs/development/validation.md#local-checks), including
   formatting, Clippy, tests, schemas, documentation, and package checks.
4. Describe the behavior change, evidence, and remaining checks in the PR.

Media, OCR, provider, and dataset changes need the corresponding real-input
checks in the [validation guide](docs/development/validation.md). It also defines
maintainer endpoint checks and release acceptance. CI does not require a model
key. Publishing follows the separate [release process](docs/development/releases.md).

Use `main` as the only long-lived branch. Public changes require a ready-for-review
pull request. Follow the [code of conduct](CODE_OF_CONDUCT.md) and report
vulnerabilities through the [security policy](SECURITY.md).

## Maintain public documentation

Use the [documentation map](ARCHITECTURE.md#documentation-and-supporting-files)
to choose where content belongs. Keep procedural steps in their relevant guide;
link to them from entry pages instead of maintaining duplicate instructions.
Keep the English, Simplified Chinese, and Traditional Chinese READMEs aligned
when changing installation, command examples, or supported features.
The [agent setup guide](docs/agent-setup.md) is for end-user installation;
`AGENTS.md` contains repository contribution instructions.

Credentials, personal machine paths, customer media, production exports, private
operating procedures, and one-off review notes do not belong in public source.
Use synthetic examples and placeholders. Local working notes belong in the
ignored `.workspace/` directory. CI's documentation and package guards supplement
human review; passing those checks does not prove content is safe to publish.

Embedded models and small licensed fixtures require provenance and license
information beside the assets. Committed media fixtures must be at most ten
seconds. Keep model dictionaries byte-exact, regenerate schemas from Rust types,
and validate prompt changes as processing changes.

For library consumers, see [integration boundaries](ARCHITECTURE.md#integration-boundaries).
