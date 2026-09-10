# Driving Cerul from an agent

Cerul is a command-line program, and that is its whole interface for automation.
There is no HTTP server, no MCP server, and no account: an agent runs the binary,
reads one JSON object, and reads the exit code. This page is the contract. The
[installation runbook](agent-setup.md) covers getting the binary onto a machine.

## Install the skill

```sh
cerul skill --install claude    # ~/.claude/skills/cerul/SKILL.md
cerul skill --install codex     # ~/.codex/skills/cerul/SKILL.md
cerul skill --install pi        # ~/.pi/agent/skills/cerul/SKILL.md
cerul skill --dir ./skills      # anywhere else
cerul skill --print             # read it, or pipe it somewhere
```

The file is written in the Agent Skills format: YAML front matter with a `name`
and a `description`, then Markdown. Its command reference is generated from this
build's own argument definitions, so it always matches `cerul --help`. A copy of
the same file lives at [`skills/cerul/SKILL.md`](../skills/cerul/SKILL.md) for
people who would rather read it on GitHub or vendor it into a repository.

Cerul records a digest of the file it wrote in the front matter, so installing
can tell its own untouched copy from one somebody changed, whichever build wrote
it. An untouched copy is replaced, which is how an upgrade works. Anything else
is left alone and the command fails, so a hand-edited skill is never overwritten;
`--force` replaces it when that is what you want.

## The `--json` contract

`--json` is agent mode. There is no second flag to remember.

| Stream | Contents |
| --- | --- |
| stdout | Exactly one JSON object, written when the command finishes. |
| stderr | One JSON event per line, written while it runs. |

Prompts never appear. A missing or invalid argument returns
`{"error":{"code":"invalid_arguments","message":"…"}}` and exit 2 instead of a
question. `cerul auth set` refuses in this mode rather than reading a key from a
pipe.

Exit codes:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 2 | Arguments or configuration |
| 3 | Unavailable dependency or capability |
| 4 | Execution failure |
| 5 | Cancelled |
| 6 | Partial success |

Exit 6 is not success. Some work finished and the rest did not; read `retry`.

## Events

```jsonl
{"event":"progress","episode":"demo","station":"semantic.subtask","done":3,"total":5}
{"event":"checkpoint","episode":"demo","station":"semantic.subtask","window":3,"total":5}
{"event":"published","episode":"demo","stream":"video","annotation":"semantic.subtask","records":12,"path":"/v/demo.mp4.cerul/semantic.subtask.jsonl"}
{"event":"log","level":"info","msg":"Indexing started"}
```

- `progress` counts units of work for one station. Its units differ by station:
  frames for screen text, segments for speech, batches for embedding, windows for
  annotation.
- `checkpoint` marks a window that is saved and will be reused after an
  interruption. A rerun resumes from there instead of paying for it again.
- `published` marks a validated annotation file that now exists on disk, with its
  record count and path. **A checkpoint is not a published file.** Only a
  published module is safe to read or train on.
- `log` carries a human-readable notice, including the one-per-endpoint notice
  before media is sent to a model.

The schema for these lives in [`schemas/event.json`](../schemas/event.json).

## Final objects

`index`, `search`, `status`, `status --timeline`, `annotate`, and `remove`
each have a generated schema in [`schemas/`](../schemas/), produced from the Rust
result types. `auth`, `open`, `skill`, and `upgrade` return small objects with no
generated schema; their shapes are:

```json
{"provider":"gemini","endpoint":"…","env":"GEMINI_API_KEY","env_set":false,"saved":true,"credentials_path":"…","action":"set"}
{"media":"/v/demo.mp4","start_us":12000000,"player":"mpv","seeks":true,"opened":true}
{"version":"0.0.7","installed":"…/SKILL.md","targets":{"claude":"…"},"skill":"---\nname: cerul\n…"}
{"current":"0.0.6","latest":"0.0.7","newer":true,"installer":"https://…/cerul-installer.sh","upgraded":false}
```

`upgrade` reports in `--json` and installs nothing; replacing the program needs
an explicit `--yes`, which is a decision to put to the user rather than take.

`auth` never reports a key's value, or any part of one. Two fields on the
generated results matter most to an agent.

**`modules[].path` on an annotate result** is where a published annotation file
is. It is absent while a module is incomplete, because there is no file to point
at yet.

**`retry`** appears on a partial **annotate** result and carries the command that
continues the work. It is the only command that offers one: `index` and
`status --providers` can also exit 6, and there the recovery is to run the same
command again. A cancelled run has no result object at all, only an error with
code `cancelled` and exit 5; the same command run again resumes it.

```json
{
  "retry": {
    "argv": ["cerul", "annotate", "/v/demo.mp4", "--semantic", "subtask,event", "--rpm", "6"],
    "reason": "rate_limit"
  }
}
```

`argv` repeats the original invocation and changes only what the failure calls
for; run it verbatim. `reason` is `rate_limit` when a provider limited the run and
`incomplete` otherwise. Never add `--recompute` to a retry: it discards finished
work and pays for it again.

## Reading results back

```sh
cerul --json status                      # every video, with what each one has
cerul --json status ./video.mp4          # one video and its files
cerul --json status ./video.mp4 --timeline --type event --limit 100
```

`--timeline` returns published annotation records in time order, each with the
original record plus a one-line `summary`. It reads sidecars only: no model call,
no network, no lock. Use it instead of parsing JSONL directly.

`cerul --json auth` reports whether a key is saved or exported, never its value.

## Rules that keep a run honest

- Ask before any command that sends media to a model endpoint, and say the
  user's provider bills it. Local OCR is the exception; it runs on the CPU.
- Preview with `--dry-run` first. It writes nothing, calls no model, probes no
  endpoint, and asks for no credential.
- Do not run `status --providers` unless asked. It makes real model requests.
- Never delete a lock file, relabel a dataset version, or switch providers to get
  past an error.
- Keep one `--workspace` for a whole task.
- `score` on a search hit is a ranking similarity, not a probability that the
  moment is the right one. Do not present it as confidence or accuracy.
