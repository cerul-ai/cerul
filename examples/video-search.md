# Search and annotate a video

Use a local video you are allowed to process. Model requests send sampled video,
audio, or text to your configured endpoints. The source media stays unchanged.

## Prepare

Build from this checkout with `cargo build --release --locked`, then put the
resulting `target/release/cerul` on your PATH. Install ffmpeg and ffprobe 6.0 or
later. Follow [configuration](../docs/configuration.md) to select models and make
their key environment variables available in your shell.

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
cerul clean --all-indexes --dry-run
cerul clean --all-indexes
cerul index ./demo.mp4
```

The final command restores indexes from intact sidecar vectors without model
calls. Cache/index cleaning preserves sidecars and source media. Deleting
sidecars requires an explicit `--sidecars PATH --yes` request.

For programmatic use, `--json` writes one final JSON object to stdout and NDJSON
progress/log events to stderr:

```sh
cerul --json search "A cup on a table" > hits.json 2> events.jsonl
```

See [schemas](../schemas) for generated contracts and [configuration](../docs/configuration.md)
for endpoint selection and exit codes.
