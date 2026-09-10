# Search and annotate a video

Use a local video you are allowed to process. Model requests send sampled video,
audio, or text to your configured endpoints. The source media stays unchanged.

## Prepare

Install a complete bundle or build this checkout following the
[installation guide](installation.md). Complete bundles include FFmpeg,
ffprobe, and OCR weights. The first interactive run guides default Gemini key
setup; an existing `GEMINI_API_KEY` also works. Use [configuration](configuration.md)
for other endpoints.

Choose a dedicated workspace so this example does not mix with other indexes:

```sh
export CERUL_WORKSPACE="$PWD/video-workspace"
cerul --dry-run index ./demo.mp4
cerul index ./demo.mp4
cerul status ./demo.mp4
```

The default pipeline runs screen OCR, transcribes an audio track when present,
and embeds video, speech, and screen text. Each completed artifact is stored in
`demo.mp4.cerul/`; the workspace holds disposable indexes and proxy caches.
Media directories that cannot be written use workspace sidecars instead.

## Find moments

```sh
cerul search "A person puts a cup on the table" --in ./demo.mp4
cerul search --text "ECONNREFUSED" --in ./demo.mp4
cerul search --image ./reference.png --save ./clips
```

Text and image queries use the configured multimodal embedding space. Exact
`--text` searches use substring matching over speech and screen text and do not
call a model. Hits contain episode-relative integer microseconds and identify
whether video, speech, or screen text matched. `--save` extracts clips with two
seconds of padding, clamped to the episode's source range.

In a terminal the hits are grouped into one card per video, each moment carrying
its match percentage, time range, excerpt, and a link. With IINA installed the
link opens the video at the matched moment; otherwise it opens the file from the
beginning. Every moment is numbered, and `cerul open 2` plays the second moment with
mpv, IINA, VLC, or ffplay when one of them is installed. Terminals implementing the iTerm2 or Kitty graphics
protocol (iTerm2, Ghostty, Kitty, WezTerm) also draw a still frame from the start
of the hit; `--preview` forces the frames and `--no-preview` suppresses them.
Frames are cached under workspace `cache/previews/`, query embeddings under
`cache/queries/`, and both are freed by `cerul remove --cache`. Repeating the
same query in the same embedding space reuses its cached vector, including
when you change filters or the result limit. Changing the query text requires
a new embedding request. Expired capability checks may still contact the
provider. Visual hits cover a whole index window; re-index with a shorter
`--chunk` when you need finer moments.

## Add semantic annotations

```sh
cerul annotate ./demo.mp4 --semantic
cerul annotate ./demo.mp4 --semantic event
cerul search --filter 'semantic.event.verb=place'
cerul search --filter 'semantic.event.verb=place' --count
```

Ordinary videos default to task, subtask, and flag. Event annotations are
requested explicitly here. Without an ontology, ordinary-video event verbs are
free text: use a verb actually present in your generated annotations. A filter
on an annotation that has not been generated returns a capability error.
Repeated filters combine with AND; filters apply before vector ranking.

## Resume and rebuild

Running the same indexing or annotation command again reuses completed work.
Ctrl-C cancels processing; rerun the command to complete missing units. A partial
result uses exit code 6 and reports the incomplete stations.

```sh
cerul remove --all-indexes --dry-run
cerul remove --all-indexes
cerul index ./demo.mp4
```

The final command, `cerul index ./demo.mp4`, restores indexes from intact
sidecar vectors without model calls. Cache/index cleaning preserves sidecars
and source media.

## Remove a video's derived data

`cerul remove ./demo.mp4` deletes the video's sidecar and the index rows
projected from it. The source video is preserved. An interactive run confirms
before deleting authoritative data; a path that was never indexed is reported
rather than treated as an error.

For a noninteractive removal, use `cerul --yes remove ./demo.mp4`.
After removing sidecars, indexing that video again must regenerate the deleted
outputs and can call your model endpoints. Use the index-cleaning commands
above when you want to preserve those outputs for a zero-model-call rebuild.

## Use structured output

For programmatic use, `--json` writes one final JSON object to stdout and NDJSON
progress/log events to stderr:

```sh
cerul --json search "A cup on a table" > hits.json 2> events.jsonl
```

See [schemas](../schemas) for generated contracts and [configuration](configuration.md)
for endpoint selection and exit codes.

For action labels in ordinary videos or demonstrations, see the
[annotation guide](annotation.md).
