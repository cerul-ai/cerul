# Public integration architecture

This repository is a generated and reviewable projection of the private Cerul
platform contract. It contains no model routing, prompts, workflow internals,
storage implementation, queue implementation, desktop source, Web source,
Worker source, provider secrets, or operational admin endpoints.

## Contract flow

```text
private contracts/openapi.yaml
  -> remove operations marked x-cerul-visibility: internal
  -> retain only transitively referenced public components
  -> openapi.json
  -> TypeScript schema and Python operation registry
  -> SDK, CLI, MCP, and examples
```

`openapi.json` is generated output. Contract changes begin in the private
platform repository. Public CI verifies that internal paths and metadata are
absent and that every client operation exists in the generated manifest.

## Runtime rules

- Cloud and local clients differ only in base URL, authentication, advertised
  capabilities, and execution location.
- A local installation token is not a Cerul Account credential.
- Login never implies upload. `local_only` requests may not cause cloud content
  egress.
- MCP maps advertised capabilities to remote tools and creates platform Jobs;
  it does not implement a second planner.
- Artifact, Evidence, AgentSession, Response, Job, Usage, and error shapes come
  from the same OpenAPI contract used by the product runtimes.
