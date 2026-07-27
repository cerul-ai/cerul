# Contributing

Thank you for improving Cerul's public developer surface.

## Scope

Good contributions improve the SDKs, CLI, MCP/agent integrations, examples,
public documentation, compatibility checks, or release metadata. Product
implementation and internal contracts are maintained privately.

Do not hand-edit generated OpenAPI or generated SDK files. Open an issue when a
contract change is needed; maintainers will update the private source and
regenerate this repository.

## Development

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm check
python3 -m unittest discover -s packages/python/tests
cargo test --manifest-path apps/cli/Cargo.toml --locked
```

Keep changes focused, add tests for behavior changes, and do not include
credentials, private data, provider details, model routing, prompts, or
internal evaluation material.

Use a short-lived branch and open a ready-for-review pull request against
`main`. Include the affected package, verification commands, and any public
compatibility impact.
