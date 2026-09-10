# Architecture

Cerul is one Rust crate with a reusable library and a CLI. This page maps source
files and documentation to their responsibilities. [DESIGN.md](DESIGN.md) defines
the behavior, storage contracts, and invariants implementations must preserve.

## Module map

| Area | Entry points | Responsibility |
| --- | --- | --- |
| CLI | [main.rs](src/main.rs), [render.rs](src/render.rs), [guide.rs](src/guide.rs), [credentials.rs](src/credentials.rs) | Parse arguments, dispatch commands, render results, complete an under-specified command at a terminal, and manage interactive credential setup. |
| Library interface | [lib.rs](src/lib.rs), [config.rs](src/config.rs), [events.rs](src/events.rs) | Expose reusable operations, explicit configuration, and structured events. |
| Media and time | [episode.rs](src/episode.rs), [media/](src/media), [ocr.rs](src/ocr.rs) | Discover media properties, map episode time, prepare model inputs, and run embedded OCR. |
| Providers | [providers/](src/providers) | Call configured endpoints, validate capabilities, and resolve scoped credentials. |
| Indexing | [index/pipeline.rs](src/index/pipeline.rs), [index/](src/index) | Discover inputs, resume processing stations, publish sidecars, and build search indexes. |
| Annotations | [annotate/pipeline.rs](src/annotate/pipeline.rs), [annotations/](src/annotations) | Generate semantic modules, validate records, and publish annotations. |
| Retrieval | [search/](src/search) | Apply temporal filters, search vectors or text, and export clips. |
| Dataset writeback | [lerobot.rs](src/lerobot.rs), [lerobot/](src/lerobot) | Read datasets and stage, validate, publish, or recover subtask writeback. |
| Persistence and maintenance | [storage.rs](src/storage.rs), [status.rs](src/status.rs), [clean.rs](src/clean.rs) | Atomic writes and locks, status reporting, and explicit data or cache removal. |

## Processing flow

1. The CLI resolves configuration and credentials, then calls library operations.
2. Discovery establishes content identity, streams, and episode-relative time.
3. Processing stations reuse valid checkpoints or produce OCR, transcripts,
   semantic annotations, and embedding vectors.
4. Validated outputs are published atomically to sidecars. These are authoritative;
   the Lance indexes are projections that can be rebuilt without model calls.
5. Search applies scope and temporal filters before ranking, then returns
   structured matches and optional clip artifacts.

All stored intervals are half-open integer-microsecond episode time. Source and
model-relative times are converted at their boundaries. A workspace admits one
writer at a time; interrupted operations preserve completed work for recovery.

## Integration boundaries

Library code receives configuration and emits structured events without assuming
a terminal or process lifecycle. It does not parse process arguments, print
progress, or terminate the process. CLI subprocess consumers use the
[JSON/NDJSON and exit-code contract](docs/configuration.md#process-contract).

Library hosts can scope credentials with `providers::with_credentials` and supply
a lazy resolver with `providers::with_credential_resolver`. The library does not
read CLI credential files; environment values take precedence. Bundled media
tools are resolved by `media::command`, with explicit overrides taking precedence.

This crate implements local processing and endpoint clients. It does not
implement product UI, hosted inference, or HTTP/MCP serving. See
[scope and current limits](docs/development/scope.md).

## Documentation and supporting files

| Location | Purpose and editing rule |
| --- | --- |
| [README.md](README.md), [Simplified Chinese](README.zh-CN.md), [Traditional Chinese](README.zh-TW.md) | Product introduction, installation, first use, and links to detailed guides; keep the three entry pages aligned. |
| [docs/README.md](docs/README.md) | Navigation by reader and task. |
| [docs/](docs) | User installation, agent-assisted setup, video search, action annotation and LeRobot tutorials, configuration, and compatibility reference. |
| [docs/development/](docs/development) | Source builds, validation, releases, and scope boundaries for contributors and maintainers. |
| [ARCHITECTURE.md](ARCHITECTURE.md), [DESIGN.md](DESIGN.md) | Source/module orientation and normative implementation contracts, respectively. |
| [CONTRIBUTING.md](CONTRIBUTING.md), `AGENTS.md` | Human contribution workflow and repository automation instructions. |
| [SECURITY.md](SECURITY.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Vulnerability reporting and community conduct. |
| [LICENSE](LICENSE), [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), [packaging/licenses/](packaging/licenses) | Project license, bundled-component notices, and full third-party license texts. Preserve notices required by distributions. |
| [models/README.md](models/README.md), [models/LICENSE](models/LICENSE), [models/characters.txt](models/characters.txt) | Model provenance, license, and a runtime OCR dictionary. The dictionary is data; whitespace changes can alter class decoding. |
| [tests/fixtures/README.md](tests/fixtures/README.md), [docs/assets/README.md](docs/assets/README.md) | Fixture provenance and brand-asset usage notes next to their files. |
| [prompts/](prompts) | Runtime Markdown embedded in the binary: eight semantic annotation prompts, plus [skill.md](prompts/skill.md), the body of the agent skill. Edit and validate them as processing behavior. |
| [skills/](skills) | The generated agent skill exactly as `cerul skill --install claude` writes it. Regenerate with `cerul skill --print > skills/cerul/SKILL.md` after changing commands or the prompt. |
| [schemas/](schemas) | JSON contracts generated from Rust types by [generate_schemas.rs](examples/generate_schemas.rs); do not hand-edit them. |
| `.github/` | Issue forms, PR template, ownership, dependency automation, and workflow YAML. GitHub consumes these files at their designated locations. |
| [Cargo.toml](Cargo.toml), [Cargo.lock](Cargo.lock), `dist-workspace.toml`, `packaging/dist.toml` | Rust and distribution manifests and dependency resolution. These are executable build inputs, not user documentation. |
| `vercel.json` | Explicitly disables legacy Vercel Git deployment; it does not define a product website in this repository. |
| `.gitignore`, `.gitattributes` | Exclude local artifacts and preserve byte-sensitive asset behavior. |
| [examples/](examples), [scripts/](scripts), [tests/](tests) | Runnable library examples, build/verification tools, and tests. Keep executable examples with their Cargo entry points. |
| `.workspace/` | Ignored local plans, audits, and working artifacts; excluded from public source and packages. |

The [contribution policy](CONTRIBUTING.md#maintain-public-documentation) defines
publication boundaries. Cargo's explicit include list and CI package checks
verify that source-package documentation, linked assets, and runtime data travel
together. A source-package inventory differs from the Git checkout: repository
hosting configuration and local artifacts are not package contents.

## Dependency constraint

LanceDB 0.38.0 currently needs its `remote` Cargo feature to compile: its job
error conversion references a feature-gated HTTP error type. Cerul opens only
the configured local workspace directory. This compatibility feature does not
enable a cloud connection or implement HTTP or MCP serving.
