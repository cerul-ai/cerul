---
name: cerul
description: Use Cerul when a user needs cited evidence from video or long-form media, or asks what was said, shown, or presented.
allowed-tools: Bash(cerul *)
---

# Cerul

Use the public `cerul` CLI when the answer depends on video or other indexed
long-form media. Do not guess what a speaker said.

Before a request, run `cerul capabilities` so unavailable capabilities are not
invented. For retrieval, use a user-authorized scope:

```bash
cerul search "query" --library-id library_...
```

For a grounded answer using the constrained Agent facade:

```bash
cerul ask "question" --library-id library_...
```

Never print, request in chat, or persist API keys or installation tokens. If
authentication is missing, tell the user to configure `CERUL_API_KEY`, or the
local `CERUL_BASE_URL` plus `CERUL_INSTALLATION_TOKEN`, outside the conversation.

Do not expand the requested library or asset scope. Do not change a
`local-only` execution policy. Present Evidence timestamps and Artifact links
from the response; do not fabricate citations.
