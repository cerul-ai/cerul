# Changelog

All notable changes to the Cerul CLI are recorded here. Release automation
publishes the section that matches the tagged version as the GitHub release
notes, so keep an entry under **Unreleased** for every user-visible change and
rename that heading when a version is cut.

## 0.0.16 - 2026-09-19

### Added
- `cerul config --show` prints the effective configuration, including its saved
  file path, and works with `--json`.
- `cerul config --set section.field=value` saves settings without prompting, so
  scripts and coding agents can configure speech transcription or search modes.
  `--dry-run` previews the write.
- `CHANGELOG.md` feeds release notes; releases no longer ship with only the
  generated install table.

### Changed
- `cerul status` and search results add parent directories to video names only
  when two indexed videos share a file name, so `video/1.mp4` and
  `raw_data/1.mp4` are distinguishable.
- Vector search skips screen-text vectors that hold fewer than two letters or
  digits and no Han character. A stray glyph read from one frame sits at the
  same distance from every query and used to outrank real evidence, while a
  single Han character is a word and is kept.
- The missing-processing-data notice in `cerul status` now also points to
  `cerul remove <video-path>` for forgetting a video whose sidecar is gone.
- Release builds abort on panic instead of unwinding. This removes the macOS
  linker warning about an oversized `__eh_frame` section and shrinks the
  binary by roughly a fifth.

## 0.0.15 - 2026-09-15
- Fix Chinese spacing in transcripts and search suggestions (#277).

## 0.0.14 - 2026-09-15
- Handle missing sidecars during `cerul upgrade` status checks and release
  0.0.14 (#276).

## 0.0.13 - 2026-09-15
- Update compatible dependencies (#273).
- Separate video analysis from indexing and accelerate embeddings (#267).

## 0.0.12 - 2026-09-14
- Fix media discovery and annotation errors (#266).

## 0.0.11 - 2026-09-14
- Annotation workflows and concurrent indexing (#265).

## 0.0.10 - 2026-09-14
- Improve indexing UX and enable hybrid video search by default (#263).
