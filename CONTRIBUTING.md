# Contributing

Read [DESIGN.md](DESIGN.md) and [ARCHITECTURE.md](ARCHITECTURE.md) before implementing changes.
Keep the reusable core independent of process arguments and terminal rendering.
Use small, reviewable pull requests with behavioral validation.

Keep the English, Simplified Chinese, and Traditional Chinese READMEs aligned
when changing installation steps, command examples, or supported features.
The end-user agent setup runbook is `docs/agent-setup.md`; it should use only
verified installation paths and must never contain credentials or local user paths.

Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and
`cargo test --locked`. Include real-media checks when changing media processing,
OCR, model adapters, or dataset writers. Never commit credentials or user data.
Fixtures and embedded models require clear provenance and compatible licenses.

Core CI runs offline checks on Linux and macOS, including generated-schema and
source-package checks. Linux also runs official LeRobot loader round-trips.
Live provider checks are a maintainer release check, run locally with your own
`GEMINI_API_KEY` using `cargo run --locked --example verify_gemini`.
The release workflow does not require a model credential. See the
[release checklist](docs/releases.md) for the full publication process.
The local provider check sends only synthetic probe
inputs: text, a small image, a two-second video, and silence. It checks endpoint
capabilities and response shapes, not retrieval quality or speech timestamp
accuracy; the remaining DESIGN.md acceptance checks are still required.

Use `main` as the only long-lived branch. Public changes require a ready-for-review
pull request. Package publishing and deployment are separate release operations.

## Developer integration

The Rust library is independent of process arguments and terminal output.
`cerul --json` writes one final JSON value to stdout and NDJSON progress events
to stderr. Times use episode-relative integer microseconds. See
[ARCHITECTURE.md](ARCHITECTURE.md) and [configuration](docs/configuration.md).
HTTP/MCP serving, grounding, world annotations, and Windows are not implemented
in this version. They must not be advertised as available capabilities.

CLI onboarding owns credential files and prompts. Library hosts may scope
credentials with `providers::with_credentials` and provide a lazy resolver
with `providers::with_credential_resolver`; the library itself never reads
CLI credential files. Environment values take precedence. Bundled media tools
are resolved by `media::command`; an explicit tool override takes precedence.
