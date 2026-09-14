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
Live indexing uses one progress bar across all selected videos, cameras and
stages. The filename and phase sit above a high-contrast block bar, which expands
to 60 cells on wide terminals. Elapsed time and remaining ETA sit together beside
the percentage; narrow terminals use `elapsed / remaining`. The current operation appears as a status label; changing stages does not
reset the percentage. The terminal labels its percentage with `~`: it eases large
completion jumps and estimates movement only within the current active work unit.
It never estimates completion of a future stage. A spinner and elapsed time remain
visible during slow requests. The success receipt waits for actual final publication;
partial runs never claim successful completion. JSON `index_progress.done` retains
confirmed weighted work, while `ceiling` bounds the terminal's active-work estimate.

Independent scene windows, speech windows, description vectors and overview groups
run concurrently. OCR and scene analysis start alongside speech. Description
vectors start as soon as scenes publish; the overview waits for scenes and text,
then overlaps vector work. All model stages share `--jobs` (default 4) and `--rpm`;
raising concurrency does not multiply the limit per stage. Frame extraction is
shared, and publication/index bookkeeping stays ordered.

Scene analysis and overview generation have separate weights. Summarizing reserves
at least a quarter of their combined work budget, rather than treating a potentially
slow overview as one more short scene window.

ETA covers the remaining invocation. Once the video inventory is known, a rough
prior based on work size and configured concurrency is available before the first
API response. Successful measured stages refine local timing hints under
`<workspace>/runtime/index-timing-<profile>.json`; profiles separate model/endpoints,
chunk settings and concurrency, and contain no credentials or media paths.
ETA follows the longest remaining dependency path, including parallel OCR, speech,
scene analysis and downstream work, instead of adding concurrent stage times. Unmeasured reuse and failed stages do
not train timings. Successful scene timings are saved at scene completion, independently of the
following overview. Timing files are optional caches, never authoritative data.
The estimate counts down between updates and is revised as work completes. If a
request exceeds the prediction, `ETA updating` indicates the estimate is overdue;
the approximate bar remains bounded within active work. API latency, retries, cache reuse and data
complexity can still make these approximate estimates inaccurate.

Stage labels describe the work: Screen text is local OCR; Speech transcribes
audio; Search index embeds video windows for semantic retrieval; Understanding
creates scene descriptions; Summarizing builds the overview and sections;
Descriptions embeds scene text; Saving
index prepares the final searchable records and local text index. Disabled or
inapplicable stages are omitted from the plan. `--json` includes an additive
`index_progress` event for the whole run and preserves individual stage events.
Optional search suggestions with invalid types or source references are discarded
before overview publication; summary and section evidence still require strict
validation. Invalid optional recommendations alone do not make an index partial.

Completed bars disappear before the final receipt is written. Routine model
notices and station summaries are hidden by default; `-v` enables diagnostics.
Warnings and errors remain visible. Redirected output contains a plain final
receipt; `--json` retains the structured progress and diagnostic events.

The completion message names the source file and shows up to three distinct,
copyable search commands. It prefers current visual suggestions from the overview,
then fills remaining places with descriptions of other observed scenes. Each
example keeps its source record and time range; extractive choices span the
available timeline and avoid repeating the same scene. No model call is needed to
select examples, including for an existing index.

Default examples describe actions or scenes. They do not mix in OCR logos, prices,
isolated words or short transcript fragments just to reach three commands. If
visual examples are unavailable, current subtask descriptions or substantive
transcript phrases provide a fallback. With no useful evidence, the CLI shows one
generic visual-search command. Original OCR and speech remain searchable; use
`--text` for literal words. The overview retains its original generated suggestions
for inspection; recommendation filtering does not rewrite authoritative records.

Long names are shortened; source files are never renamed. Commands preserve
workspace/model overrides and search the whole workspace. `--in` is optional and
only restricts an explicitly scoped search. Examples show supported content; they
do not guarantee a retrieval rank. Next-step hints put the description above the
command. Each command occupies its own line without a prompt prefix or trailing
explanation, so copying the line produces a runnable command.

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
Default search uses video, speech, screen text, and separately cached description
vectors, plus full-query matches from original OCR/ASR. Each track keeps its own
evidence and time range. Missing tracks are skipped. Use `--json` to inspect
`fusion_recipe` and `fusion_evidence`; scores are relevance values, not probabilities.
Set `[search] hybrid = false` to compare the previous three-track baseline.
See [configuration](configuration.md) for score and threshold semantics.

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

## Command help

Run `cerul help` for the command overview, `cerul help index` for indexing,
`cerul help search` for searching, or `cerul help annotate` for annotation modes.
`cerul <command> --help` provides the same command reference. Help includes
usage, examples, prerequisites, and relevant options, and works without media
tools, a key, or an initialized workspace.

All directories share the default `~/.cerul` workspace. After indexing, run
`cerul search "describe a moment"` from any directory to search that workspace;
there is no need to repeat the video's path. Use `--workspace DIR` when you
intentionally want a separate search workspace. Source media and authoritative
sidecars stay in their existing locations.
