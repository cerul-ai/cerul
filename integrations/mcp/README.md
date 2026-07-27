# Cerul MCP

`cerul-mcp` is a thin remote MCP adapter. It discovers the selected runtime
through `GET /v1/capabilities`, maps each advertised capability to a tool, and
creates a normal Cerul Job when that tool is called.

It does not contain a planner, prompts, provider routing, or an alternative
business object model.

```sh
CERUL_API_KEY=... CERUL_BASE_URL=https://api.cerul.ai/v1 \
  corepack pnpm --filter @cerul/mcp start
```

For a local runtime, use its loopback URL and
`CERUL_INSTALLATION_TOKEN` instead.
