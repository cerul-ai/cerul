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

The default pipeline runs screen OCR and available ASR concurrently, embeds
video, speech, and screen text, then produces visual scene descriptions and a
grounded overview. OCR, embedding proxies, and understanding reuse timestamped
samples; each consumer selects its own resolution. Each completed artifact is stored in
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
a new embedding request. A valid cached query does not probe the provider. Visual hits cover a whole index window; re-index with a shorter
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

## Optional speech transcription

Use `cerul config` to choose Gemini, Groq, OpenAI, or a custom transcription
service. Choose Disabled to keep video embeddings and local OCR without a separate
transcript. With a Gemini key already available, the CLI automatically uses
Gemini transcription without another chooser. Explicit settings are preserved;
non-interactive runs never prompt. See [configuration](configuration.md).

## Diagnose speech failures

If Gemini blocks an audio request, Cerul reports the transcription window and
`promptFeedback.blockReason` or `finishReason`. In prompt feedback,
`blockReason=OTHER` is an unspecified provider rejection, not evidence of
malformed JSON. A candidate's `finishReason=OTHER` instead means an unknown
completion reason; it does not establish that the request was blocked. Blocked responses are not
automatically retried, and no successful transcript is published for them.
A truncated response and an empty or invalid JSON response have separate errors.
Provider response text, credentials, and free-form rejection messages are not
included in these diagnostics.

Completed screen text and checkpoints remain available. Video/OCR vectors can
remain searchable even when speech fails. Inspect the saved diagnostic with
`cerul status ./video.mp4 --json`; a complete embedding state may carry a speech
error. Capture the full command report and events for additional context:

```sh
cerul index ./video.mp4 --json > result.json 2> events.jsonl
```

Cerul does not automatically save a complete request/response log. Retrying can
make new billable model calls. If speech is not needed, `cerul index ./video.mp4
--no-audio` explicitly skips transcription; visual indexing still uses the
configured embedding endpoint.

## Index progress and search examples

After choosing Index and a path in the interactive guide, processing starts
immediately. Explicit `--dry-run` remains available for inspecting planned work.
Live terminal output shows the current stage, a progress bar, and an estimated
remaining time for that stage after enough work has completed. Preparation and
single-request stages show an indeterminate estimate rather than a fabricated ETA.
Redirected output uses plain lines; `--json` keeps structured progress events.

The completion message uses a generated title when available and shortens long
file names otherwise. It never renames your files. Each episode stores zero to
three generated suggestions with source record IDs and revisions. Visual
suggestions cite scene records; speech and screen suggestions cite their own
tracks. Commands preserve workspace/model overrides and scope to that video.
The examples show supported content; they do not guarantee a retrieval rank.

If understanding is unavailable or explicitly skipped, Cerul uses extractive
examples from current annotations, speech, or OCR. Extractive OCR examples use
`--text`. With no usable evidence, it shows one generic visual-search command.
A partial result keeps completed work and prints the original command to retry.

## Inspect video understanding

```sh
cerul status ./demo.mp4 --timeline --type summary
cerul status ./demo.mp4 --timeline --type scene
cerul status ./demo.mp4 --timeline --type section
cerul status ./demo.mp4 --timeline --type summary --json
```

The sidecar stores `semantic.scene.jsonl`, `semantic.section.jsonl`, and
`semantic.summary.jsonl` using the existing `annotation/1` envelope. Scenes
contain visible descriptions, objects, actions, a content kind, and input sample
times. Lighting and camera movement may appear in the description when visible;
these are observations, not metric camera trajectories. Sections are coarse
navigation. The summary contains a short title, overview, coverage, dependencies,
and up to three grounded search suggestions. Original OCR and transcript text
remain in their own files. Sparse samples do not establish continuous observation
or exact action boundaries.

`cerul status ./demo.mp4 --json` also reports per-stream understanding status,
including a deliberate skip, incomplete work, and observed/failed windows.
Descriptions have separate cached vectors for evaluation; default search still
uses video, speech, and screen text. The [evaluation guide](development/retrieval-evaluation.md)
explains the gate before description and lexical scores enter default ranking.

Without ASR, visual scenes and suggestions still work. Adding or correcting ASR
invalidates the dependent overview; compatible visual generation is reused.
A failed refresh preserves valid previous evidence for unchanged inputs. Source
revisions are retained under the sidecar's `revisions/` directory.

Manual scene edits belong in `corrections/semantic.scene.json`, using the
[generated correction schema](../schemas/scene-corrections.json). Each edit pins
the original record ID and its `base_revision` (the timeline entry's `revision`
from `status --timeline --type scene --json`). Re-indexing applies the edits without
changing their IDs or intervals. Generation never writes this correction file.
When a new generation conflicts or re-segments the scene, Cerul reports that the
edit needs rebasing and withholds the disputed replacement.
