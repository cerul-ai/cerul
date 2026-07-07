<div align="center">
  <br />
  <a href="https://cerul.ai">
    <img src="./assets/logo.png" alt="Cerul" width="80" />
  </a>
  <h1>Cerul</h1>
  <p><strong>Where video becomes citable.</strong></p>
  <p>Stop scrubbing. Start searching. The video search API for agents — meaning-based, grounded, timestamped.</p>

  <p>
    <a href="https://cerul.ai/docs"><strong>Docs</strong></a> &middot;
    <a href="https://cerul.ai/docs/search-api"><strong>API Reference</strong></a> &middot;
    <a href="https://cerul.ai/pricing"><strong>Pricing</strong></a> &middot;
    <a href="https://x.com/cerul_hq"><img src="https://img.shields.io/badge/follow-%40cerul__hq-000?style=flat-square&logo=x" alt="Follow on X" /></a> &middot;
    <a href="https://discord.gg/qHDEMQB9vN"><img src="https://img.shields.io/badge/join-Discord-5865F2?style=flat-square&logo=discord&logoColor=white" alt="Join Discord" /></a> &middot;
    <a href="#wechat-group"><img src="https://img.shields.io/badge/WeChat-Group-07C160?style=flat-square&logo=wechat&logoColor=white" alt="Join WeChat Group" /></a>
  </p>

  <p>
    <a href="./LICENSE"><img alt="License" src="https://img.shields.io/badge/license-Apache_2.0-3b82f6?style=flat-square" /></a>
    <a href="https://www.npmjs.com/package/cerul"><img alt="npm" src="https://img.shields.io/npm/v/cerul?style=flat-square&color=22c55e" /></a>
    <a href="https://pypi.org/project/cerul"><img alt="PyPI" src="https://img.shields.io/pypi/v/cerul?style=flat-square&color=22c55e" /></a>
  </p>
</div>

<br />

<div align="center">
  <img src="./assets/screenshot-home.png" alt="Cerul — Copy, paste, run. Get video results in seconds." width="840" />
</div>

<br />

<div align="center">
  <video src="https://github.com/user-attachments/assets/e0982048-3b40-42b9-80b6-a79b401d2355" width="720" autoplay loop muted playsinline></video>
</div>

<br />

## Why Cerul

Web pages are easy for AI agents to search. **Video is not.**

Most video search today is limited to transcripts — what was *said*. Cerul goes further by indexing what is *shown*: slides, charts, product demos, code walkthroughs, whiteboards, and other visual evidence.

> [!NOTE]
> Cerul is in active development. The API is live at [cerul.ai](https://cerul.ai) — sign up to get a free API key.

## Quickstart

### Claude Code plugin

Install the official Cerul plugin in Claude Code:

```bash
/plugin marketplace add cerul-ai/cerul-plugin-cc
/plugin install cerul@cerul-plugin
```

Source: [cerul-ai/cerul-plugin-cc](https://github.com/cerul-ai/cerul-plugin-cc).

### Use with your AI agent

Copy this and send it to your agent (Claude Code, Codex, Cursor, etc.):

```
Install the Cerul video search skill: mkdir -p ~/.claude/skills/cerul && curl -fsSL https://raw.githubusercontent.com/cerul-ai/cerul/main/skills/cerul/SKILL.md -o ~/.claude/skills/cerul/SKILL.md && cat ~/.claude/skills/cerul/SKILL.md
```

Your agent will install the CLI, set up credentials, and start searching videos as a tool.

<div align="center">
  <img src="./assets/agent-skill-search.png" alt="Claude Code using Cerul skill to search videos" width="720" />
  <img src="./assets/agent-skill-result.png" alt="Agent synthesizing video evidence into an answer" width="720" />
</div>

Or install via the skills CLI:

```bash
npx skills add cerul-ai/cerul
```

<details>
<summary><strong>Install for specific agents</strong></summary>

```bash
# Claude Code
mkdir -p ~/.claude/skills/cerul && curl -fsSL https://raw.githubusercontent.com/cerul-ai/cerul/main/skills/cerul/SKILL.md -o ~/.claude/skills/cerul/SKILL.md

# Windsurf
mkdir -p ~/.codeium/windsurf/skills/cerul && curl -fsSL https://raw.githubusercontent.com/cerul-ai/cerul/main/skills/cerul/SKILL.md -o ~/.codeium/windsurf/skills/cerul/SKILL.md

# OpenCode
mkdir -p ~/.config/opencode/skills/cerul && curl -fsSL https://raw.githubusercontent.com/cerul-ai/cerul/main/skills/cerul/SKILL.md -o ~/.config/opencode/skills/cerul/SKILL.md

# Cline / Cursor
mkdir -p .claude/skills/cerul && curl -fsSL https://raw.githubusercontent.com/cerul-ai/cerul/main/skills/cerul/SKILL.md -o .claude/skills/cerul/SKILL.md
```

</details>

### CLI

```bash
curl -fsSL https://cli.cerul.ai/install.sh | bash
cerul search "Sam Altman on AI video generation tools"
```

<div align="center">
  <img src="./assets/cli-search.png" alt="cerul search CLI output" width="720" />
</div>

> Inline video frame previews are supported in iTerm2, WezTerm, and Kitty. Enable with `cerul config` and toggle **Images** on. Other terminals show text-only results.

### MCP (Model Context Protocol)

Connect any MCP-compatible client to the hosted endpoint (replace with your API key):

```bash
# Claude Code
claude mcp add --transport streamable-http "https://api.cerul.ai/mcp?apiKey=YOUR_API_KEY" cerul

# Codex
codex mcp add --url "https://api.cerul.ai/mcp?apiKey=YOUR_API_KEY" cerul
```

Also works with Claude Desktop, Cursor, Windsurf, and other MCP clients.

<details>
<summary><strong>SDK & API</strong></summary>

**Python**

```bash
pip install cerul
```

```python
from cerul import Cerul

client = Cerul(api_key="YOUR_API_KEY")
results = client.search(query="Sam Altman on AGI timeline", max_results=5)

for r in results:
    print(r.title, r.url)
```

**JavaScript**

```bash
npm install cerul
```

```javascript
import { cerul } from "cerul";

const client = cerul({ apiKey: "YOUR_API_KEY" });
const result = await client.search({ query: "Sam Altman on AGI timeline", max_results: 5 });

for (const r of result.results) {
  console.log(r.title, r.url);
}
```

**cURL**

```bash
curl "https://api.cerul.ai/v1/search" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"query": "Sam Altman on AGI timeline", "max_results": 5}'
```

Full API spec: [`openapi.yaml`](./openapi.yaml) | [cerul.ai/docs](https://cerul.ai/docs/search-api)

</details>

## License

Licensed under [Apache 2.0](./LICENSE).

<h2 id="wechat-group">Community</h2>

Join our WeChat group to chat with the team and other users. Scan the QR code below (refreshes weekly):

<div align="center">
  <img src="./assets/wechat-group.jpg" alt="Cerul.ai WeChat Group" width="280" />
</div>

Or join us on [Discord](https://discord.gg/qHDEMQB9vN) for English discussion.

<div align="center">
  <br />

  [![Star History Chart](https://api.star-history.com/svg?repos=cerul-ai/cerul&type=Date)](https://star-history.com/#cerul-ai/cerul&Date)

  <br />
  <sub>Built by <a href="https://github.com/JessyTsui">@JessyTsui</a></sub>
</div>
