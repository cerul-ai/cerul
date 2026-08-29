# Cerul developer surface

**Help AI understand, remember, and access video.**

Video memory infrastructure for AI products.

Cerul turns video and other long-form media into searchable evidence and
structured artifacts. This repository is the public integration surface for
the Cerul platform.

The product implementation is private. This repository contains only:

- the sanitized [OpenAPI 3.1 contract](./openapi.json);
- the TypeScript and Python SDKs;
- the `cerul` command-line client;
- MCP and Claude Code integrations;
- public examples, compatibility notes, and release links.

Local and cloud runtimes use the same contract. Choose a base URL:

```text
Cloud: https://api.cerul.ai/v1
Local: http://127.0.0.1:<dynamic-port>/v1
```

The local runtime requires its installation token. The cloud runtime accepts a
Workspace API key or OAuth access token. Call `GET /v1/capabilities` before
requesting optional capabilities; clients must not assume local and cloud
offer identical execution.

## Packages

| Path | Purpose |
|---|---|
| `apps/cli` | CLI commands for search, ask, AgentSession, jobs, and export |
| `integrations/mcp` | Remote MCP projection of the Capability Registry |
| `integrations/claude-code` | Installable Claude Code integration |

## Verify

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm check
cargo test --manifest-path apps/cli/Cargo.toml --locked
```

## SDKs

The TypeScript and Python SDKs are retired while the `/v1` API is redesigned.
The published `cerul` packages on npm and PyPI are deprecated and receive no
updates. Use the REST API directly, or the MCP integration, until generated
SDKs return with the new contract.

The public OpenAPI and generated client surfaces are produced from the private
`cerul-platform/contracts/openapi.yaml`; they are not independently authored
here. See [ARCHITECTURE.md](./ARCHITECTURE.md) for the publication boundary.

No package publishing, binary release, or production deployment occurs from a
source change alone.
