# Repository guidelines

Communicate with the repository owner in Chinese by default. Public docs,
code, identifiers, comments, commit messages, and API fields are English.

This repository is the public Cerul developer surface. Allowed content is
limited to sanitized generated OpenAPI, CLI, MCP and agent
integrations, examples, public documentation, and release metadata.

Do not add product Web, Desktop, API, Worker, storage, queue, model, prompt,
workflow, ranking, evaluation, billing-provider, or operational admin
implementation. Do not commit secrets, data, indexes, internal plans, or
production exports.

`openapi.json` and `integrations/mcp/src/generated/schema.ts` are generated
from the private platform contract. Do not hand-edit them. Contract changes must begin in the
private platform repository and pass the public-surface generator.

Use the existing boundaries:

- `apps/cli`
- `integrations/mcp`
- `integrations/claude-code`
- `examples`
- `docs`

MCP maps advertised capabilities and creates normal Cerul Jobs. It must not
implement a planner or invent business objects. All clients must support local
and cloud through the same contract and a configurable base URL.

Verify changes with:

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm check
cargo test --manifest-path apps/cli/Cargo.toml --locked
```

Use `main` as the only long-lived branch and `codex/` for agent branches.
Public merges go through a ready-for-review PR unless the user explicitly asks
for a draft.
