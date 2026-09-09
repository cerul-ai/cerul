# Repository guidelines

Communicate with the repository owner in Chinese by default. Public docs,
code, identifiers, comments, commit messages, and API fields are English.

This repository is the open-source Cerul video processing core and CLI.
DESIGN.md is the implementation baseline for the CLI rewrite. The core owns
local media processing, endpoint clients, embedded OCR, annotations, sidecars,
and rebuildable indexes. Product UI and hosted services are outside this repository's scope.

Do not add product Web or Desktop UI, cloud Workers, billing-provider or
operational admin implementations. Do not commit secrets, user media, indexes,
or production exports. Embedded OCR weights and small licensed test fixtures
must include provenance and license information.

Legacy `openapi.json` and MCP generated schemas belong to the retired platform
client and must not be hand-edited. Remove obsolete client surfaces during the
rewrite. New core schemas are generated from Rust types. HTTP and MCP serving
are M2 work and must not be advertised as implemented in M1.

Use one root Rust crate with a library and a thin CLI. Library code must not
parse process arguments, print progress, or terminate the process. Model
endpoints are configurable. Sidecars are authoritative; indexes are caches.
Preserve integer-microsecond episode time, content-aware invalidation, atomic
publication, and zero-model-call index rebuilds.

Verify with `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`,
and `cargo test --locked`.
Each implementation step needs focused behavioral verification. M1 requires
macOS arm64 and Linux x86_64 builds, real OCR inference, model endpoint smoke
checks, interruption recovery, and official LeRobot loader round-trips.
Do not replace these gates with mock-only or schema-only evidence.

Retain LICENSE, Git history, existing tags, and all `ffmpeg-vendor-*` release
assets. Remove obsolete tracked files explicitly; never delete local worktrees
or untracked user artifacts as part of the rewrite.

Use `main` as the only long-lived branch and `codex/` for agent branches.
Public merges go through a ready-for-review PR unless the user explicitly asks
for a draft.
