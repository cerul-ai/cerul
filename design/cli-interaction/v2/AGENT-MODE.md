# Agent mode: letting other people's agents drive Cerul

Goal: a coding agent (Claude Code, Codex, pi, Cursor, or a hand-written loop)
should be able to install, configure, and operate Cerul without reading the
repository, and without Cerul embedding an agent of its own. Three pieces
deliver that: a machine contract that already mostly exists (`--json`), a
shipped skill that teaches the contract, and a command that installs the skill
where each agent looks for it.

Nothing here adds HTTP or MCP serving. DESIGN.md keeps those unimplemented and
unadvertised; subprocess JSON is the sanctioned integration path.

## 1. The contract is `--json`

There is no separate agent flag. `--json` is agent mode:

- stdout: exactly one final JSON object, schema per command under `schemas/`.
- stderr: one NDJSON event per line.
- exit codes: 0 ok, 2 arguments or configuration, 3 dependency or capability,
  4 execution, 5 cancelled, 6 partial.
- never prompts, never draws, never asks for a key. Missing arguments return
  `{"error":"invalid_arguments", ...}` with an example command.

### 1.1 Event vocabulary

Today `events.rs` has `progress` and `log`. Agents waiting on a long
`annotate` need to know when something usable appeared and how to recover.
Add three events; all fields are stable and documented in `schemas/event.json`.

```jsonl
{"event":"progress","episode":"demo","station":"semantic.subtask","done":3,"total":5}
{"event":"published","episode":"demo","annotation":"semantic.subtask","records":12,"path":"/v/demo.mp4.cerul/semantic.subtask.jsonl"}
{"event":"checkpoint","episode":"demo","station":"semantic.event","window":3,"total":5}
{"event":"log","level":"info","msg":"Indexing started"}
```

`published` fires only after validation and atomic publication. `checkpoint`
fires when a window is durably saved and will be reused by a rerun.

### 1.2 Final object additions

The annotate result gains `retry`, and each module gains the path of the file
it published:

```json
{
  "modules": [{"episode":"demo","annotation":"semantic.subtask","records":12,
               "source":"/v/demo.mp4","dataset":false,"path":"…/semantic.subtask.jsonl"}],
  "retry":   {"argv":["cerul","annotate","/v/demo.mp4","--semantic","subtask,event","--rpm","6"],
              "reason":"rate_limit"}
}
```

`retry` is present on a partial **annotate** result, and it is part of the
generated schema rather than a field added beside it, so a client built from
`schemas/` knows the recovery exists. It is the only command that offers one:
`index` and `status --providers` can also exit 6, and there the recovery is to
run the same command again. It repeats every original argument, including any completed by
guidance, and changes only what the failure suggests; `--recompute` is never
added. An agent can run `retry.argv` verbatim. A cancelled run has no result
object at all, only an error with code `cancelled`; the same command run again
resumes it.

The draft proposed a separate `outputs` array. It was dropped during
implementation: `modules[]` already is that list once each entry carries its own
`path`, and two copies of one fact in one object is a defect waiting to happen.

### 1.3 Read-only views agents need

- `cerul --json status [path]`: unchanged, plus `--timeline` returning
  sidecar records in time order with `--type` and `--limit`. This is how an
  agent answers "what did you find" without parsing JSONL itself.
- `cerul --json auth`: reports `saved`, `env_set`, `credentials_path`; never
  a value or prefix.
- `cerul --json --dry-run <anything>`: plan without writes, probes, or model
  calls.

## 2. The skill

One file in the Agent Skills format (YAML front matter plus Markdown body),
readable by Claude Code, Codex, pi, and any tool that follows the same
convention. It is the only document an agent needs.

### 2.1 Source of truth

`prompts/skill.md` holds the hand-written parts (when to use, workflow,
safety rules). The command reference section is generated from the clap
definitions at build time, so it can never drift from `--help`. A test asserts
`cerul skill --print` equals the committed copy at `skills/cerul/SKILL.md`,
which exists so people can install from GitHub without running anything.

### 2.2 Structure

```markdown
---
name: cerul
description: Search local videos by meaning, exact words, or a reference image, and
  annotate actions, events, interactions, and states in videos or LeRobot datasets.
  Use when the user mentions video search, clips, video annotation, egocentric or
  robot demonstrations, or LeRobot. Requires the cerul CLI.
---

# Cerul

## Check first
`cerul --version`; if missing, follow docs/agent-setup.md (install.sh, no Rust).
`cerul --json auth` for key presence. Never echo, log, or copy a key.

## Always use --json
stdout final object, stderr NDJSON events, exit codes 0/2/3/4/5/6.
Exit 6 is partial, not success. Run `retry.argv` from the final object.

## Workflows
Search:   cerul --dry-run index P → cerul --json index P → cerul --json search "Q" --in P
Annotate: cerul --json --dry-run annotate P --semantic subtask,event,interaction,state
          → same without --dry-run → cerul --json status P --timeline
LeRobot:  cerul --json annotate DATASET --semantic --only 0 → status DATASET --timeline
Clips:    cerul --json search "Q" --save DIR

## Rules
- Ask before any command that sends media to a model endpoint; say it is billed.
- Do not run status --providers unless asked; it makes model requests.
- Do not delete lock files, change dataset version labels, or add --recompute.
- Use one --workspace consistently.

## Command reference
<generated from clap: every command, every option, defaults>
```

### 2.3 `cerul skill`

```text
cerul skill                   where it can go, and where it already is
cerul skill --print           write SKILL.md to stdout
cerul skill --install claude  ~/.claude/skills/cerul/SKILL.md
cerul skill --install codex   ~/.codex/skills/cerul/SKILL.md
cerul skill --install pi      ~/.pi/agent/skills/cerul/SKILL.md
cerul skill --dir DIR         any other skills directory
cerul skill --install claude --force   replace a skill that was changed
```

Receipt: `✓ Wrote ~/.claude/skills/cerul/SKILL.md (cerul 0.0.6)`. The file records
the CLI version in its front matter. Installing replaces a file an older build
wrote, because that is an upgrade. It records a digest of what it wrote, so it can tell its own untouched
copy from one somebody changed whichever build wrote it, and refuses the latter;
`--force` is the way past that. A file with no digest at all is never replaced.
`--install` and `--dir` are two destinations and cannot both be given. Printing the skill, or writing it to a named directory, needs neither a
workspace nor a home directory.

## 3. Documentation

- `docs/agent.md`: the contract in one page (streams, exit codes, events,
  final-object fields, schemas, `retry`). Linked from README's agent section.
- `docs/agent-setup.md`: unchanged runbook, plus one line: install the skill
  first with `cerul skill --install claude`.
- `llms.txt` on cerul.ai pointing at `docs/agent.md` and the skill is a
  website change, outside this repository.

## 4. Not in scope

MCP or HTTP serving, an embedded agent, tool-call adapters per vendor, or a
plugin marketplace listing. If a vendor-specific package (a Claude Code
plugin, a pi package) becomes worthwhile, it wraps the same SKILL.md and the
same `--json` contract.

## 5. Verification

- Pipe tests for each `--json` command: single stdout object, NDJSON stderr,
  documented exit code, `retry.argv` re-parses with clap.
- Schema test: `schemas/event.json` and per-command final objects regenerate
  from Rust types with no diff.
- Skill test: `cerul skill --print` equals `skills/cerul/SKILL.md`; front
  matter parses; every command in the reference exists in clap.
- One real run: an agent given only SKILL.md completes annotate on a short
  clip and reads the timeline, on macOS arm64 and Linux x86_64.
