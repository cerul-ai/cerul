# Clip a video by intent

`clip` turns one long local video into short clips chosen by an angle, each
carrying its source. Name what you are after ("every time pricing comes up"),
or pick a preset; the command proposes moments as a numbered list with a
verbatim quote, a one-line reason and a time range; you (or an agent) choose
numbers; the command renders those clips with captions and a one-line source
credit and writes a provenance sidecar next to each file.

```sh
cerul clip ./ep42.mp4 --angle numbers
cerul clip ./ep42.mp4 "every time pricing comes up"
cerul clip ./ep42.mp4
cerul clip ./ep42.mp4 --export 1,3,5
cerul clip ./ep42.mp4 --export all --aspect 9:16 --out ./shorts
cerul clip ./ep42.mp4 --dry-run
```

Selection runs on the transcript and, when present, cached screen text. No
embeddings, no vision calls and no search index are involved, and the video
never leaves the machine: sixty-second audio windows go to the configured
speech endpoint and transcript text goes to the configured vision endpoint as
plain text. One video file per invocation; a folder returns
`invalid_arguments`.

## Angles

Presets: `numbers` (figures, prices, dates), `claims` (a stated position or
prediction), `stories` (a told anecdote), `howto` (a step someone could act
on), `questions` (a question asked and answered), `disagreements` (two people
differing) and `definitions` (a term explained). Anything else after `--angle`,
or the positional query, is passed to the model verbatim as its own angle. With
no angle and no query, the command uses the angles its brief suggests for the
video.

## What happens

1. **Prepare.** The media tools are checked as for `index`, the file gets the
   same content identity and sidecar directory, and a current transcript is
   ensured: missing or stale speech is transcribed in sixty-second windows with
   the same cache identity and resume behaviour as `index`. Embeddings are
   never computed. Screen text is used when a current `screen_text.jsonl`
   exists; `--ocr` recognises it locally when it is missing. Its absence is not
   an error.
2. **Utterances.** Transcript records are split into sentence-level units at
   `. ! ?` and `。！？`, timed proportionally by character count inside each
   record, and numbered `u00001…`; screen-text records become `s00001…`. The
   derivation is checkpointed under the transcript cache key.
3. **Brief.** One structured call summarises the video: `summary`,
   `content_type`, `topics`, ordered non-overlapping `chapters` and up to six
   `suggested_angles`. Long transcripts are sent in 120 kB chunks and merged.
   Every id is validated; invalid output is retried once. The brief is cached
   by transcript key, model and prompt version.
4. **Propose.** One structured call per angle returns moments as utterance
   spans with a quote, reason and title. Locally, each span snaps to utterance
   boundaries, gains 300 ms of padding inside stream coverage, must fall within
   `--min` and `--max` (15 s and 90 s by default), must quote the span
   verbatim after normalising case, whitespace and punctuation, and is dropped
   when more than half of it overlaps an accepted moment. Numbering follows
   angle order and model order, interleaved round-robin across chapters so one
   chapter cannot take every slot. The run is cached by transcript key, angles,
   query, limits, model and prompt version, and remembered as the latest run
   for the video.
5. **Export.** `--export` renders the numbered proposals from the latest run
   for the video, making no model call. Naming different angles with
   `--export`, or exporting before a proposal run exists, returns
   `invalid_arguments`.

`--dry-run` reports the audio minutes that would go to the speech endpoint,
the transcript bytes that would go to the text endpoint when a current
transcript exists to measure, the angles and the output directory. It writes
nothing and calls nothing.

## Rendered clips

Each selected proposal is cut from the source with an accurate seek and
encoded as H.264/AAC with `faststart`. `--aspect 9:16` centre-crops to
portrait and scales to 1080×1920; `source` keeps the frame. Audio is
normalised with `loudnorm=I=-16:TP=-1.5:LRA=11` with a 100 ms fade in and out.
Captions come from utterance timing, at most two lines of 32 characters, and
are rasterised by the CLI into PNG strips composited with ffmpeg `overlay`,
so burned-in text does not depend on libass or libfreetype in the ffmpeg
build. Latin text uses the bundled Inter font (SIL Open Font License); CJK
text uses a font found on the system — `CERUL_CJK_FONT`, then PingFang,
Hiragino Sans GB, Arial Unicode or Noto Sans CJK. Without one, Latin still
renders and missing characters appear as the font's missing-glyph box.
`--no-captions` skips the burned captions (the `.srt` is still written),
`--no-credit` skips the credit line and `--credit-text` replaces it.

Three files are published per clip under `--out` (default `./clips`), named
`<source stem>-<start>-<end>`, for example `ep42-30m30s500-31m12s100.mp4`:
the MP4, an `.srt` with the same cues, and a `.json` provenance sidecar. The
MP4 is written through a temporary file and renamed, then the SRT, then the
sidecar, so a sidecar's presence means the clip is complete. A clip that fails
to render leaves the others in place and the command exits 6.

The sidecar is the public contract for anything that later reads Cerul clips.
Clip times are episode microseconds; cue times are relative to the clip start.
Its schema is generated from the Rust type as
[`schemas/clip-sidecar.json`](../schemas/clip-sidecar.json).

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
  "models": {"transcript": {"kind": "gemini", "name": "…"}, "selection": {"kind": "gemini", "name": "…"}}
}
```

## Agents and JSON

Two invocations are the normal agent flow: propose, then export the chosen
numbers.

```sh
cerul --json clip ./ep42.mp4 --angle numbers --angle stories
cerul --json clip ./ep42.mp4 --export 1,3,5
```

`--json` prints one object conforming to
[`schemas/clip-result.json`](../schemas/clip-result.json): the `brief`, the
`angles`, `proposals[]` (`number`, `start_us`, `end_us`, `utterances[]`,
`quote`, `title`, `reason`, `angle`, `query`, `chapter`, optional `preview`
still), `dropped[]` with a reason (`quote_not_verbatim`, `unknown_id`,
`invalid_span`, `duration`, `overlap`), the `cache_key` of the proposal run,
and after an export `exports[]` with each clip's `video`, `subtitles` and
`sidecar` paths or its `error`. Dry runs put their plan under `plan`. On
stderr, `progress` events use stations `transcript`, `propose` and `export`,
and each rendered clip emits a `published` event with its MP4 path.

The quote is the only model text verified against the transcript. Present
`title` and `reason` as the model's hints, not as facts. Exit codes follow the
rest of the CLI: 2 for arguments and configuration, including `--export`
without a cached run, and 6 when some clips failed to render.

## Not in this version

Voice activity detection, speaker labels, a review page, a hosted model proxy,
face-centred cropping, zip bundles, folders of videos, and any embedding or
vision call. The [design record](design/clip-command.md) lists them so they
are not added by accident.
