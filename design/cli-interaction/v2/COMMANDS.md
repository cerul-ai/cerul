# Cerul CLI interaction design, v2

Status: design for review. Supersedes the 64-scene catalog in the parent
directory by narrowing it to the screens a person actually meets. No Rust
changes are part of this document. DESIGN.md stays the implementation baseline;
everything here is presentation and TTY-only guidance layered on the existing
command contracts.

Direction confirmed by the owner: command line with lightweight guidance.
No full-screen TUI and no embedded agent in this iteration. Agents call the
CLI through `--json` and a shipped skill; see [AGENT-MODE.md](AGENT-MODE.md).

## 1. Shared grammar

Every human-facing screen is built from the same five parts, in this order.
Not every screen needs all five, but the order never changes.

| Part | Rule |
| --- | --- |
| Title | One line: state glyph, verb, subject, and the one number that matters (`✓ Annotated demo.mp4 in 01:48`). |
| Facts | Two-space indented `label  value` rows, labels left-aligned in one column. Real paths, real counts. |
| Live area | stderr only, TTY only. One line per unit of work with a glyph. Collapses to one line when the unit finishes. |
| Receipt | stdout. What was produced and where, as paths the shell can use. |
| Next | Heading `Next`, one to three full commands with a short reason. Never more than three. |

Glyphs carry state without color: `✓` done, `◐` in progress, `·` queued or
not applicable, `!` partial, `✗` failed, `❯` current choice. Color is one
accent for commands and the current choice, green for done, amber for partial
and cancelled, red for failed, dim for metadata. No emoji in default output;
the current search card glyphs (🔍 📊 🕐 🎬) go.

Width: 80 columns is the layout target. Below 80, fact rows stack; paths and
commands wrap at word boundaries and are never truncated. Times are `mm:ss`
or `hh:mm:ss` for people; microseconds stay in JSON.

Streams: results on stdout, progress and prompts on stderr. Redirected output
is append-only, no cursor movement, no images. `NO_COLOR` removes color;
a non-TTY removes control sequences independently.

Guidance gate: interactive prompts appear only when stdin and stderr are TTYs
and none of `--json`, `--yes`, `-q`, `--dry-run` is set. This is the predicate
`confirm()` already uses in `main.rs`; the guided screens reuse it. Every
guided flow ends by printing the exact equivalent command, shell-quoted, so the
second run never needs the guide.

## 2. Home: `cerul`

TTY:

```text
cerul 0.0.7  ·  Search and annotate your videos

Workspace  ~/.cerul     8 videos · 6 searchable · 3 annotated
Models     Gemini       key saved · endpoints not checked this run

What do you want to do?
❯ Index videos so they can be searched
  Search indexed videos
  Annotate actions in a video or LeRobot dataset
  Show status
  Quit

↑↓ move · Enter select · Esc quit                or: cerul index ./video.mp4
```

Selecting an item runs the matching guided flow below. Esc or Quit exits 0
with nothing written. Redirected or `--json`: the existing read-only home
(status summary plus three example commands), no menu, no numbering that looks
pressable.

First run (no key, nothing indexed) keeps the current four-step "Get started"
list; the menu is offered underneath it once a key exists.

## 3. Annotate

### 3.1 Guided: `cerul annotate` with no path

Three questions, then run. Bare `annotate` in a non-TTY keeps today's
`invalid_arguments` error and example block.

```text
Annotate  ·  which video or dataset?
❯ demo.mp4              02:30
  kitchen.mp4           04:12
  egodemo/              LeRobot v3.1 · 24 episodes
  Type a path…

Only this directory is listed. Nothing is read until you start.
```

Directory listing is non-recursive, media files and LeRobot roots only,
capped at 12 entries, most recent first. Typing a path validates existence
and layout before continuing.

```text
Annotate  ·  demo.mp4  ·  what is in it?
❯ A robot or first-person demonstration     subtask, event, interaction, state
  A general video                            task, subtask, flag
  Everything                                 all seven semantic types
```

Presets are named by what the user has, not by label names. They expand to
existing `--semantic` lists; no new flags. For a LeRobot root an extra step
picks episodes (`0`, a list, or all; default `0`) and cameras (primary or all)
from the real discovery result, and the third preset is the default.

```text
Annotate  ·  ready

  Input     demo.mp4 · 02:30 · primary camera
  Labels    subtask, event, interaction, state
  Model     Gemini gemini-3.8-flash · frames are sent to it, billed to your key
  Output    ./demo.mp4.cerul/semantic.<type>.jsonl

  cerul annotate ./demo.mp4 --semantic subtask,event,interaction,state

❯ Start
  Preview only (--dry-run)
  Cancel
```

No cost or time estimate is invented. Start executes exactly the printed
command through the normal argument path.

### 3.2 Running

```text
Annotating demo.mp4   subtask · event · interaction · state
  Gemini gemini-3.8-flash · output ./demo.mp4.cerul/

✓ subtask        5/5 windows   12 records   published
◐ event          3/5 windows
◐ interaction    2/5 windows
· state          queued

00:42 elapsed · Ctrl+C stops after the current window; finished work is kept
```

Window counts appear only when known. "published" appears only after
validation and atomic publication, never for checkpoints. For several inputs
the finished ones collapse to one line each and the live block shows the
current input, as `index` does today.

### 3.3 Receipt

```text
✓ Annotated demo.mp4 in 01:48

  subtask        12 records   ./demo.mp4.cerul/semantic.subtask.jsonl
  event          18 records   ./demo.mp4.cerul/semantic.event.jsonl
  interaction     9 records   ./demo.mp4.cerul/semantic.interaction.jsonl
  state          14 records   ./demo.mp4.cerul/semantic.state.jsonl

Next
  cerul status ./demo.mp4 --timeline              read the labels in order
  cerul search --filter semantic.event.verb=grasp  find one action across videos

Labels are model-generated; review them before training on them.
```

LeRobot receipts group by episode and camera and end with
`cerul status ./dataset --timeline`.

### 3.4 Partial, cancelled, failed

```text
! Annotated demo.mp4 partially                                       exit 6

  ✓ subtask        12 records   published
  ✓ event          18 records   published
  ✗ interaction    stopped at window 3/5 · Gemini rate limit (429)
  · state          not started

Finished windows are saved and reused. Retry with a lower request rate:
  cerul annotate ./demo.mp4 --semantic subtask,event,interaction,state --rpm 6
```

The retry command repeats every original argument and changes only what the
error suggests. `--recompute` is never added automatically. Ctrl+C prints the
same block with `cancelled` and exit 5 and the unchanged command as the retry.

## 4. Timeline: `cerul status <path> --timeline`

New read-only view over sidecars. It is the "did it work?" screen and the
cheapest way to judge label quality without opening JSONL. `--json` returns the
records unchanged from the sidecar schema.

```text
demo.mp4  02:30  ·  subtask event interaction state  ·  ./demo.mp4.cerul/

  00:00 – 00:14   subtask       reach for the cup
  00:03           event         grasp cup
  00:14 – 00:31   subtask       lift cup to mouth
  00:20           state         cup · on table → in hand
  00:31 – 00:52   subtask       drink
  00:52           interaction   hand releases cup

showing 6 of 53 records · --type event · --limit 100
```

## 5. Index

`cerul index` with no path asks one question (which video or folder, same
picker as annotate) and runs. With arguments it prints the plan line first,
as today.

```text
Indexing 3 videos   screen text (local) · speech (Gemini) · search index (Gemini)

✓ cup-demo.mp4      02:30   screen text · speech · search ready
◐ silent.mp4        02:10
    screen text     180/260 frames
    speech          no audio track
    search          2/5 windows
· assembly.mp4      04:02   queued

01:18 elapsed
```

Receipt:

```text
✓ 3 videos ready to search   08:42 of media in 03:05

  screen text    3 videos          speech    2 videos · 1 had no audio
  indexes        ./videos/*.cerul  vectors   ~/.cerul/index/

Next
  cerul search "a person picking up a cup"
  cerul search --text "ERROR 500"
```

Offline or missing-key runs report local stations as done and remote ones as
`not indexed · no Gemini key`, exit 6, and recommend only `--text` search.

## 6. Search

`cerul search` with no query asks `Search for:` on one line and runs.
Image and filter searches stay flag-only. Results:

```text
3 moments for "a person picking up a cup"   in 2 videos

  cup-demo.mp4
  [1]  00:12 → 00:19   similarity 82%    visual
       A person lifts a ceramic cup from the table.
  [2]  01:04 → 01:10   similarity 77%    visual
       A hand picks up the cup beside a notebook.

  kitchen.mp4
  [3]  00:42 → 00:48   similarity 71%    speech
       "Pick up the cup and place it here."

Next
  cerul open 1                                  play the first moment
  cerul search "a person picking up a cup" --save ./clips
```

Changes from today: the box-drawing card frame and emoji go; `similarity
82%` replaces `82% match` (a label change only; the number and the ranking
stay as they are, and README needs no edit); numbering is
global and identical to `open N` even when grouped by video; still frames stay
where the terminal supports them, placed under the `[N]` line. `--text`
results show `exact · screen text` or `exact · speech` and no similarity.
`--count` keeps its one-line answer.

Empty:

```text
No matches for "a person juggling"   in 6 searchable videos

  cerul search --text "juggling"      exact words on screen or in speech
  cerul status                        which videos are searchable
```

Missing indexes, unavailable model, and invalid query are separate errors
(section 10), never an empty result.

## 7. Open

```text
▶ Playing cup-demo.mp4 from 00:12 in mpv                          result 1 of 3
```

Fallback: `▶ Opening cup-demo.mp4 in the system player · it cannot start at
00:12, seek there yourself`. No search yet: `error: no search to open from`
with `cerul search "..."` as the hint. Out of range: names the valid range.

## 8. Status

```text
Workspace ~/.cerul   ·   cerul 0.0.7

  Video               Length   Search    Text   Speech     Annotations
  cup-demo.mp4        02:30    ready     ✓      ✓          subtask event
  silent.mp4          02:10    ready     ✓      no audio   –
  assembly.mp4        04:02    partial   ✓      ✗          –
  egodemo/ · 24 ep    18:40    –         –      –          ep 0 · 4 types

Models    Gemini    gemini-embedding-2 (1536d) · gemini-3.8-flash · key saved
Storage   3 indexes beside videos · 1 vector space · OCR runs locally

cerul status <path> for files and the annotation timeline
```

Below 80 columns the table becomes one block per video. `status <path>`
lists every sidecar file with counts and offers `--timeline`. `--providers`
adds a `Checked` line with the probe time per endpoint.

## 9. Auth and remove

```text
$ cerul auth set
Gemini API key (hidden):
Checking  ✓ embedding · ✓ vision · ✓ transcription
Saved to ~/.cerul/credentials.toml (0600) · GEMINI_API_KEY in the environment overrides it

Next
  cerul index ./video.mp4                  make it searchable
  cerul annotate ./video.mp4 --semantic    label actions
```

Failed verification keeps the previous key and says which check failed and
why (rejected, timeout, quota, unsupported). Removal says the local copy is
gone, environment keys still apply, and nothing was revoked remotely.

```text
$ cerul remove ./demo.mp4
Forget demo.mp4?
  deletes   ./demo.mp4.cerul/          annotations and vectors · regenerating costs model calls
  deletes   its rows in 1 search index
  keeps     ./demo.mp4

Remove? [y/N] y
✓ Forgot demo.mp4 · freed 14 MB
```

`--cache`, `--all-indexes`, `--index`, `--compact` each print the same
`deletes / keeps` block with their own scope and do not ask, as today. The
receipt for `--cache` says a later query may need one embedding call; for
indexes it says rebuild needs no model calls.

## 10. Errors

Four lines, always in this order: what failed, what it affected, the safe
next action as a full command, what was preserved. Exit code right-aligned on
the last line.

```text
✗ Workspace is busy
  ~/.cerul is locked by another cerul process started 00:04:12 ago.
  Wait for it, or use a separate workspace:
    cerul --workspace ./ws annotate ./demo.mp4 --semantic
  Nothing was changed.                                                 exit 4
```

Categories keep their current codes: 2 arguments and configuration, 3 missing
dependency or unsupported capability, 4 execution, 5 cancelled, 6 partial.
Missing key, invalid config, unsupported capability, rate limit, connection
failure, and lock contention each have their own message; none suggests
deleting a lock file or switching providers.

## 11. Help

Root help groups commands by task and lists global options in full:

```text
Process    index      make videos searchable
           annotate   label actions, events, interactions, and states
Explore    search     find moments or annotation records
           open       play a result from the last search
           status     videos, files, timeline, and model setup
Maintain   auth       manage the saved Gemini key
           remove     free caches, indexes, or a video's data
           skill      print or install the agent skill
```

`completions` stays hidden. Command help keeps the current
"examples, then input and output, then grouped options" order. Help never
starts a guide.

## 12. Implementation notes

- Binary only. A `guide` module beside `render.rs`, declared from `main.rs`;
  the library never sees a terminal. This is also the moment to split the
  1,200-line `main.rs` into `cli/args.rs`, `cli/run.rs`, `cli/guide.rs`.
- Prompts use `dialoguer` (same author as the existing `indicatif` and
  `console`, no new terminal backend). Select, Input, Confirm only.
- `paths` becomes optional at the clap level for `index` and `annotate`;
  the dispatch layer either runs the guide or returns today's error, so the
  JSON `invalid_arguments` tests keep passing.
- Guided choices build the same `Args` structs the parser produces; the
  printed command is derived from them with a small shell-quoting helper.
- Receipt, partial, timeline, status table, search card, and error changes
  are pure `render.rs` work and ship first, before any prompt.
- Acceptance: PTY tests (guide appears, Esc leaves nothing behind, paths
  with spaces quote correctly, guided run equals typed command); pipe tests
  (no prompt, no ANSI, streams and exit codes unchanged); visual check at
  60, 80, 100 columns, dark, light, `NO_COLOR`.

## 13. Order of work

| Step | Scope | New dependency |
| --- | --- | --- |
| 1 | Receipts, partial and cancel blocks, error grammar, search card, status table, timeline view | none |
| 2 | Agent skill command and JSON events ([AGENT-MODE.md](AGENT-MODE.md)) | none |
| 3 | Guided `annotate`, `index`, `search`; home menu | dialoguer |
| 4 | LeRobot episode and camera picker | none |

Owner decisions recorded: the ordinary-video `--semantic` default stays
`task,subtask,flag` (the guide's first preset is an explicit choice, not a new
default); the search score is shown as `similarity 82%`; the home menu lists
index and search before annotate.
