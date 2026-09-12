# Developer validation

Run commands from the repository root. Use a separate workspace and synthetic
or licensed fixtures; keep credentials, customer media, benchmark outputs, and
one-off review notes out of the public repository.

## Local checks

Follow [source build setup](building.md), then run:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo run --locked --example generate_schemas -- --check
python3 scripts/check-docs.py
python3 -m unittest discover -s tests -p "test_documentation.py"
python3 -m unittest discover -s tests -p "test_retrieval_evaluation.py"
CERUL_TEST_BINARY="$PWD/target/debug/cerul" python3 -m unittest discover -s tests -p "test_setup.py"
cargo package --list --locked > /tmp/cerul-package-files.txt
python3 scripts/check-docs.py --package-list /tmp/cerul-package-files.txt
```

The documentation test parses fenced and inline Cerul command examples with
the same Clap parser as the CLI; it does not execute those commands or contact a provider.
The documentation checker validates local links and anchors, common personal
paths and credential patterns in public documents, and the source-package
inventory. This is a targeted guard, not a full secret scanner.
Review new binary fixtures and extend the explicit asset list only after
recording provenance and licensing. During local editing, add
`--allow-dirty` to the Cargo package command to inspect uncommitted changes.

Core CI runs on Linux x86_64 and macOS arm64. Tests include embedded OCR on real
pixels, cancellation, restart recovery, sidecar invalidation, and index rebuilds.
The Linux job also checks the official LeRobot loader. CI does not use a model
key. A successful CI run is evidence for its exact commit and fixtures, not a
measurement of retrieval quality on arbitrary videos.

## Official LeRobot loader

The implementation is tested against Hugging Face LeRobot commit
[`2774d9bddcbbda50e697e162e89e7eaada8d7105`](https://github.com/huggingface/lerobot/tree/2774d9bddcbbda50e697e162e89e7eaada8d7105).
Its recorder emits v3.0 while exposing the language-column types used by the
synthetic v3.1 compatibility fixture. These tests do not establish the existence
of an official v3.1 recorder or an upgrade tool. See the
[user-facing compatibility limits](../lerobot.md).

On Linux, install the same isolated Python environment used by Core CI:

```sh
python3.12 -m venv /tmp/cerul-loader-env
/tmp/cerul-loader-env/bin/python -m pip install 'torch==2.11.0' 'torchvision==0.26.0' --index-url https://download.pytorch.org/whl/cpu
/tmp/cerul-loader-env/bin/python -m pip install 'lerobot[dataset,av-dep] @ git+https://github.com/huggingface/lerobot.git@2774d9bddcbbda50e697e162e89e7eaada8d7105'
```

Use fresh source and output paths for both cases:

```sh
export HF_HUB_OFFLINE=1 HF_DATASETS_OFFLINE=1
/tmp/cerul-loader-env/bin/python tests/lerobot_roundtrip.py create /tmp/cerul-source
cargo run --locked --example verify_lerobot_writeback -- /tmp/cerul-source /tmp/cerul-output /tmp/cerul-loader-env/bin/python
/tmp/cerul-loader-env/bin/python tests/lerobot_roundtrip.py create /tmp/cerul-source-no-language --without-language
cargo run --locked --example verify_lerobot_writeback -- /tmp/cerul-source-no-language /tmp/cerul-output-no-language /tmp/cerul-loader-env/bin/python
```

The five-episode, 40-frame fixture covers existing and absent language columns.
The harness compares original fields, decoded images, actions, state, untouched
episodes, and active subtask timestamps. A rejected validator must leave the
output unpublished. Python is a test dependency, not a Cerul runtime dependency.

## Live model checks

Maintainers run synthetic endpoint checks locally with an authorized key set
securely in `GEMINI_API_KEY`:

```sh
cargo run --locked --example verify_gemini
```

This checks text, image, and video embeddings, structured vision output, and
transcription response shapes. It does not measure semantic recall or speech
alignment. When changing provider defaults, verify the model IDs, availability,
and timestamp quality against real requests before publication.

## OCR and performance

Run embedded OCR on the repository's synthetic fixture:

```sh
cargo run --release --locked --example verify_ocr -- tests/fixtures/ocr-text.png
```

For OCR/runtime changes, also compare twenty licensed sample clips against a
documented reference, recording versions, output quality, hardware, CPU time,
and peak memory. Measure both supported platforms. Real inference is required;
model schema inspection alone cannot establish operator compatibility.

Record binary/archive sizes and throughput with the release artifacts. A
previous benchmark or an estimate must not be presented as a new release result.
Review the semantic verb vocabulary against the selected dataset when changing
annotation behavior.

## Release acceptance

The following are acceptance requirements, not a claim that every requirement
has passed for the current checkout. Record the commit, platform, fixture source,
and result for each check. Review them before the [release process](releases.md).

### Installation and runtime

- [ ] On fresh macOS arm64 and Linux x86_64, install using the public command.
  With `GEMINI_API_KEY`, index a ten-minute video with speech in under ten minutes
  and find the correct moment.
- [ ] Verify that the CLI needs no Python, CUDA, or dynamic ML libraries at
  runtime. FFmpeg and ffprobe run as separately licensed bundled subprocesses.

### Retrieval and output contracts

- [ ] Ask ten manually chosen questions, with at least three each about visuals,
  speech, and screen text. Recall@5 must be at least 8/10. Speech questions match
  speech rows; screen-text questions match screen rows.
- [ ] Return a filtered event whose unfiltered vector rank is below the first
  ten candidates, proving filters apply before ranking.
- [ ] Check search/status JSON against schemas and parse JSON-mode stderr line
  by line as NDJSON.

### Dataset integrity

- [ ] Annotate five LeRobot episodes. Subtasks cover each full timeline without
  gaps and use real frame boundaries. Official-loader readback of
  `--write-lerobot --out` verifies content and timestamps while preserving all
  original action, state, and annotation fields.
- [ ] Check adjacent episodes sharing an MP4 for timestamp isolation, and two
  datasets containing episode `000012` for distinct identities and outputs.

### Cache and recovery

- [ ] Repeating unchanged processing reuses completed outputs without model
  calls. Changing only the embedding model recomputes only embeddings.
- [ ] Repeating the same search text reuses its query vector without a capability
  probe, including when the probe cache has expired. A query cache miss may
  probe the endpoint. Distinguish processing, query embeddings, and capability
  probes when counting calls.
- [ ] Delete workspace indexes and rebuild from sidecar vectors with zero model
  calls. Interrupt indexing with Ctrl-C; resumption fills gaps, preserves
  completed units, and leaves no incomplete final artifacts.
- [ ] Offline `index --no-audio` completes local probing, frames, and OCR, returns
  exit 6 with embeddings incomplete, and reports the episode unindexed in status.
  Reconnection resumes embedding only.

### Model capabilities and OCR

- [ ] Verify Gemini semantic annotation. Ollama and alternative vision endpoints
  are optional configurations, not required default-provider acceptance gates.
- [ ] Reject an embedding endpoint without image support during probing with
  exit 3 and no index creation.
- [ ] Run embedded OCR on twenty licensed sample clips and compare it with a
  documented reference implementation; output quality must be no worse.

LanceDB or Arrow changes must preserve prefilter behavior, sidecar round-trips,
and zero-model-call rebuilds. Use behavioral tests and the official loader in
addition to compilation. Keep a copy of this checklist with the commit,
platforms, fixture provenance, measured results, and remaining failures in the
release evidence; empty checkboxes here do not describe a particular release.
