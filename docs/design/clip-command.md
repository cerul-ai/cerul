# `clip` command design (v1)

Status: implementation baseline for the first `cerul clip` release
(2026-10-15). Extends [DESIGN.md](../../DESIGN.md) section 3; the rules there
(library never parses arguments or prints, sidecars authoritative, integer
microseconds, atomic publication, zero-model-call rebuilds) apply unchanged.

## Purpose

`clip` turns one local long-form video into short clips chosen by intent, each
carrying its source. The user names an angle ("every time pricing comes up")
or picks from suggested angles; the command proposes moments as a numbered
list with a verbatim quote, a one-line reason and a time range; the user (or
an agent) selects numbers; the command renders the clips with captions and a
one-line source credit and writes a provenance sidecar next to each file.

Selection runs on the transcript (and cached screen text when present). No
embeddings, no vision calls, no index are required. The video never leaves the
machine: only sixty-second audio windows go to the configured ASR endpoint and
transcript text goes to the configured text endpoint.

## Contract

```sh
cerul clip <path> [QUERY]
    [--angle ANGLE]...            # preset name or free text; repeatable
    [--limit N]                   # proposals per angle, default 10, max 25
    [--min DURATION]              # default 15s
    [--max DURATION]              # default 90s
    [--export all|N[,N...]]       # render the listed proposals; default: propose only
    [--out DIR]                   # default ./clips
    [--aspect source|9:16]        # default source
    [--no-captions] [--no-credit]
    [--credit-text TEXT]          # overrides the generated credit line
    [--stream ID]                 # default primary
```

Global flags (`--json`, `--dry-run`, `--recompute`, `--jobs`, `--rpm`,
`--no-auto-deps`) keep their meaning. Exactly one video path is accepted in
v1; a folder returns `invalid_arguments`.

Two invocations are the normal agent flow:

```sh
cerul --json clip ./ep42.mp4 --angle numbers --angle stories   # propose
cerul --json clip ./ep42.mp4 --export 1,3,5                     # render
```

The second call reuses the cached proposals for the same video, angles, query
and limits; it makes no model call. `--export` without a matching cache
returns `invalid_arguments` naming the missing proposal run.

Preset angles: `numbers` (figures, prices, dates), `claims` (a stated position
or prediction), `stories` (a told anecdote), `howto` (a step someone could
act on), `questions` (a question asked and answered), `disagreements`
(two people differing), `definitions` (a term explained). Free text after
`--angle` or as `QUERY` is passed through verbatim. With no angle and no
query, the command uses the brief's suggested angles (below).

## Flow

1. **Preflight.** Media dependency check as in index. Resolve the episode as
   `index` does (single-stream episode, content identity by SHA-256).
2. **Prepare.** Ensure a current transcript sidecar exists; run the transcript
   station if it is missing or stale, with the same sixty-second windows,
   cache identity and resume behaviour as `index`. Do not run embeddings. Run
   OCR only if it is already configured to run locally and is missing; its
   absence is not an error. Progress events use station `transcript`.
3. **Utterances.** Derive sentence-level units from transcript records: split
   record text at sentence punctuation (`. ! ?` and `。！？`), keep order,
   assign ids `u00001…`. Timing: word-level when the transcript protocol is
   `native-word-phrases`, otherwise proportional by character count inside
   the record. Save as a checkpoint keyed by the transcript cache key. Screen
   text records, when present, become `s00001…` units with their own times.
4. **Brief.** One structured `generate` call on the text endpoint, input is
   the utterance list as `id [mm:ss] text`, chunked at 120,000 bytes with a
   merge call for long videos. Schema: `summary` (≤ 400 chars),
   `content_type` (`interview | lecture | webinar | solo | demo | other`),
   `topics` (≤ 8), `chapters[]` (`title`, `start_id`, `end_id`, ordered,
   non-overlapping), `suggested_angles[]` (≤ 6, `label`, `query`). Validate
   every id; reject and retry once on invalid output. Cached by transcript
   key, model and prompt version.
5. **Propose.** For each angle (preset, free text or suggested), one
   structured `generate` call with the utterance list and the brief. Schema:
   `moments[]` with `start_id`, `end_id`, `quote`, `reason` (≤ 140 chars),
   `title` (≤ 60 chars). Instructions: ids must exist; `quote` must be a
   verbatim contiguous excerpt of the listed utterances; spans between `--min`
   and `--max`; spans must not overlap. Then, locally:
   - snap bounds to utterance boundaries, pad 300 ms each side inside stream
     coverage;
   - enforce duration bounds;
   - **verify the quote**: normalise both sides (case, whitespace,
     punctuation) and require the quote to be a substring of the span text;
     otherwise drop the moment and record it under `dropped` with reason
     `quote_not_verbatim`;
   - drop spans overlapping an accepted one by more than 50 %;
   - number proposals: angle order, then LLM order, interleaved round-robin
     across chapters so one chapter does not take every slot.
   Cache the proposal run under a key of transcript key, angles, query,
   limits, model and prompt version.
6. **Present.** Human mode prints numbered cards using the search card
   renderer: still frame, time range, quote, reason, angle; then the two
   commands to export or re-run. `--json` prints one object: `brief`,
   `proposals[]` (number, `start_us`, `end_us`, `utterances[]`, `quote`,
   `title`, `reason`, `angle`, `preview` path), `dropped[]`, `cache_key`.
   Without `--export` the command exits 0 here.
7. **Export.** For each selected proposal, render with ffmpeg from the source
   range as `media::extract::video` does (accurate seek, libx264, aac,
   faststart), extended with a filter graph:
   - aspect `9:16`: `crop=min(iw\,ih*9/16):ih` centred, then scale to
     1080×1920; `source`: no crop;
   - loudness: `loudnorm=I=-16:TP=-1.5:LRA=11`;
   - captions: cues built from utterance (or word) timing, at most two lines
     and 32 characters per line, rendered by the CLI into PNG strips with a
     bundled OFL font (Latin) and a system CJK font when found; each cue is
     one `overlay` input with `enable='between(t,start,end)'`. This does not
     depend on libass or libfreetype in the ffmpeg build;
   - credit: one PNG strip for the whole clip at the bottom:
     `<file name> · mm:ss–mm:ss`, or `--credit-text`;
   - 100 ms audio fade in and out.
   Outputs: `<name>-<start>-<end>.mp4`, `.json` (sidecar, below), `.srt`.
   Publication is atomic (temp file, fsync, rename). Events: `published` per
   clip. Any failed clip leaves the others and returns exit 6.
8. **Dry run.** Reports the audio minutes that would go to ASR, the text bytes
   that would go to the text endpoint, the angles, and the output directory.
   Writes nothing, calls nothing.

## Provenance sidecar (`clip/1`)

```json
{
  "$cerul": "clip/1",
  "cerul_version": "0.0.19",
  "created": "2026-10-15T09:12:00Z",
  "source": {"path": "ep42.mp4", "sha256": "…", "episode": "…/0", "stream": "primary", "duration_us": 5400000000},
  "clip": {"start_us": 1830500000, "end_us": 1872100000},
  "utterances": ["u00137", "u00138", "u00139"],
  "quote": "we cut the price from 29 to 15 and renewals went from 60 to 74 percent",
  "title": "Cutting the price raised renewals",
  "reason": "The only place the episode gives concrete pricing numbers.",
  "angle": "numbers",
  "query": null,
  "aspect": "source",
  "captions": {"burned": true, "cues": [{"start_us": 0, "end_us": 2400000, "text": "we cut the price from 29 to 15"}]},
  "models": {"transcript": {"kind": "gemini", "model": "…"}, "selection": {"kind": "gemini", "model": "…"}}
}
```

Clip times are episode microseconds; cue times are relative to the clip
start. The sidecar is the public contract for anything that later reads
Cerul clips; its JSON schema is generated from the Rust type like the others.

## Skill

Add a "Clip a video" workflow to `skills/cerul/SKILL.md`: dry-run first and
tell the user audio will be sent to their ASR endpoint; run the proposal call
with `--json`; present the numbered proposals in plain language (quote,
reason, duration); ask which to export or which angle to try instead; run
`--export`; report file paths; offer the unused suggested angles. Never
present `reason` or `title` as fact beyond the quote; the quote is the only
verified text.

## Out of scope for v1

Voice activity detection, speaker labels, a review page, a hosted model
proxy, face-centred cropping, zip bundles, folders of videos, and any
embedding or vision call. These are listed so they are not added by accident.

## Verification

Unit tests: sentence splitting (Latin and CJK), proportional timing, quote
normalisation and verification, snapping and padding, overlap suppression,
caption cue grouping, filter-graph construction, sidecar round-trip and
schema generation. Documentation tests must parse every example above. One
integration test renders a synthetic video (tone plus colour bars) through
`--export` with a mocked transcript and text endpoint and checks the mp4,
json and srt exist and the sidecar validates. Live checks against a real
endpoint remain a release requirement, not a CI gate.
