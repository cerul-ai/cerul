# Release artifacts

The v0.0.3 rewrite is not published by building or reviewing this branch.
Package publishing is a separate release operation after M1 acceptance.
Existing tags and `ffmpeg-vendor-*` assets must be preserved: Desktop releases
still depend on those assets.

## Build configuration

`dist-workspace.toml` pins cargo-dist 0.32.0 and defines exactly two targets:

| Target | Build runner |
| --- | --- |
| `aarch64-apple-darwin` | macOS 14 |
| `x86_64-unknown-linux-gnu` | Ubuntu 24.04 |

The `dist` Cargo profile inherits the optimized release profile. OCR weights
are embedded; archives include Apache-2.0 and third-party license notices. ffmpeg and
ffprobe 6.0 or later are runtime dependencies. Homebrew's generated formula
includes ffmpeg as a dependency. Shell and npm users must install it separately.
Linux binaries target glibc; musl/Alpine and other CPU architectures are not M1
targets. Minimum runtime versions must be checked against the final binary's
linkage manifest, rather than inferred from its filename.

The first macOS release build measured approximately 228 MiB uncompressed.
Archive size and final platform requirements are recorded during release
acceptance; the previous 60 MB estimate is not a size guarantee.

## Generate and inspect

Use the pinned cargo-dist version, then run:

```sh
dist generate --check
dist plan --output-format=json
dist build --artifacts=local --target=aarch64-apple-darwin
dist build --artifacts=global
```

Build Linux local artifacts on Linux with its target instead. Global artifacts
combine the local build manifests; generating them without local artifacts
cannot prove final checksums or runtime requirements. Generated files are
written under `target/distrib/`, including:

- `cerul-<target>.tar.xz` and SHA-256 files.
- `cerul-installer.sh`.
- `cerul.rb`, a Homebrew formula.
- `cerul-npm-package.tar.gz`, the `cerul` 0.0.3 npm wrapper.
- A source archive and aggregate checksums.

The source archive uses committed Git content. Regenerate it after committing;
an archive produced while this rewrite is uncommitted is not the release source.
Do not hand-edit generated release workflows or installers. Change the dist
configuration and regenerate them.

## Cargo source package

Cargo.toml explicitly lists public source, documentation, embedded models, and
acceptance fixtures. Check the actual registry package as well as the GitHub
source archive; local legacy build directories must not enter either artifact.

```sh
cargo package --list --locked
cargo package --locked
```

The initial cleaned package measured 25.1 MiB compressed (93 files). Embedded
OCR weights account for most of its size. This exceeds the default 10 MB limit
documented in the [Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).
Before publishing to crates.io, confirm package ownership and an upload limit
that accepts the final archive. No size exception has been verified. Keep the
embedded OCR runtime contract; do not silently omit the weights or replace
them with a first-run download to make the upload fit.

## Installation acceptance

Test both target archives on their corresponding operating systems. Check
installed executable hashes, `cerul --version`, empty-workspace `status --json`,
real OCR, and a real video indexing/search flow. Verify licenses in both native
archives and the npm package. Exercise npm's wrapper as well as its postinstall
step; merely inspecting package.json is insufficient.

For shell testing against a local mirror, set `CERUL_DOWNLOAD_URL` and use
`CERUL_UNMANAGED_INSTALL` pointing to a temporary directory. This avoids changing
shell profiles or the user's normal installation. A temporary npm package copy
can point `artifactDownloadUrls` at the same mirror. Keep production metadata
pointing to the versioned GitHub release.

Local mirror tests verify installation mechanics. They do not prove a public
release URL exists or a registry publication succeeded. Perform those checks
separately only as part of an authorized release.

## Publishing boundary

The generated GitHub workflow plans artifacts on pull requests and builds and
hosts releases for matching version tags. This branch does not push a version
tag. npm and Homebrew artifacts are generated, but registry/tap publishing is
not configured: no Homebrew tap or registry credential is assumed.

Before an authorized publication, finish all DESIGN.md M1 acceptance gates,
verify the committed source archive, confirm npm package ownership and the
chosen Homebrew distribution destination, and configure the corresponding
publisher. The desired `https://cerul.ai/install.sh` entry point also needs to
be connected to the verified generated installer in the product website. Do
not advertise these installation endpoints as live before they are verified.
