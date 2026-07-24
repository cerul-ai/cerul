# Cerul for Claude Code

This plugin teaches Claude Code when and how to call the public `cerul` CLI.
It does not include credentials or Cerul product implementation.

## Install

```text
/plugin marketplace add cerul-ai/cerul
/plugin install cerul@cerul-plugin
```

Install the CLI from this repository's release index, then provide either:

- `CERUL_API_KEY` for the cloud runtime; or
- `CERUL_BASE_URL` and `CERUL_INSTALLATION_TOKEN` for a local runtime.

The skill uses the unified `cerul search` and `cerul ask` commands. It never
asks an agent to print, copy, or persist a secret in chat.
