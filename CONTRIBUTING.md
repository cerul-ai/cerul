# Contributing

Read [DESIGN.md](DESIGN.md) and [AGENTS.md](AGENTS.md) before implementing changes.
Keep the reusable core independent of process arguments and terminal rendering.
Use small, reviewable pull requests with behavioral validation.

Run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and
`cargo test --locked`. Include real-media checks when changing media processing,
OCR, model adapters, or dataset writers. Never commit credentials or user data.
Fixtures and embedded models require clear provenance and compatible licenses.

Live default-provider checks run on this repository's `main` branch, never on
pull requests or fork code. Configure the `GEMINI_API_KEY` Actions secret before
merging; a missing credential fails the live check rather than silently passing.
To run the same checks locally, configure the key in your environment and run
`cargo run --locked --example verify_gemini`. This sends only synthetic probe
inputs: text, a small image, a two-second video, and silence. It checks endpoint
capabilities and response shapes, not retrieval quality or speech timestamp
accuracy; the remaining DESIGN.md acceptance checks are still required.

Use `main` as the only long-lived branch. Public changes require a ready-for-review
pull request. Package publishing and deployment are separate release operations.
