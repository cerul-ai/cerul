# Release artifacts

Building or reviewing a branch never publishes a release.
Package publishing is a separate release operation after the [release acceptance checks](validation.md#release-acceptance).
Existing tags and `ffmpeg-vendor-*` assets must be preserved for downstream compatibility.

## Release checklist

Public installation uses GitHub Releases and the permanent website installer
redirect. npm and Homebrew publication are separate from generating their artifacts.

1. Verify PR checks and acceptance results, then merge the release PR into `main`.
2. From the merged `main`, confirm Cargo.toml and packaging/dist.toml agree on
   the version being released, then create and push the matching tag. The
   commands below use `X.Y.Z` as a placeholder; replace it with the version in
   both manifests. Existing release tags must not be moved or reused.

   ```sh
   git switch main
   git pull --ff-only origin main
   git tag -a vX.Y.Z -m "Release Cerul X.Y.Z"
   git push origin vX.Y.Z
   ```

3. Wait for the Release workflow to publish both platform archives, the shell
   installer, checksums, and corresponding media sources to GitHub Releases.
4. Nothing to configure. `https://cerul.ai/install.sh` redirects to
   `https://github.com/cerul-ai/cerul/releases/latest/download/cerul-installer.sh`,
   which GitHub resolves to the newest published, non-prerelease release, so
   step 3 is what makes a release live. Verify that following the redirect
   returns the shell script for this version, not an HTML page or an older one.
5. Confirm the default model endpoints still answer, using your own key so no
   credential is stored in the repository:

   ```sh
   cargo run --locked --example verify_gemini
   ```

   Set `GEMINI_API_KEY` securely in your local environment before running this
   command. The CLI's defaults name specific Gemini models, so this catches an
   upstream model being retired or changed before users do. CI does not run
   this check or require a model credential.
6. On each supported platform, install from the public README command and verify
   `cerul --version`, video indexing, search, and clip export.
7. Confirm the README installation command works from a clean terminal.

These are maintainer instructions; creating the tag publishes the release.
No server deployment is needed for the CLI.

## Build configuration

`dist-workspace.toml` pins cargo-dist 0.32.0 and defines exactly two targets:

| Target | Build runner |
| --- | --- |
| `aarch64-apple-darwin` | macOS 14 |
| `x86_64-unknown-linux-gnu` | Ubuntu 24.04 |

A generic cargo-dist package wraps the single Rust crate. Its native build script
compiles the CLI and pinned FFmpeg/x264 sources, then packages three executables:
`cerul`, `cerul-ffmpeg`, and `cerul-ffprobe`. Shell, npm, and Homebrew installers
install them together. No runtime system FFmpeg dependency is required.
OCR weights are embedded. Each binary archive includes third-party notices. The same release publishes
`cerul-media-source.tar.gz`, containing the exact FFmpeg, x264, and zlib source
tarballs (including licenses) and build script in `media-source/`.
FFmpeg with libx264 is GPL-2.0-or-later; the separately executed Rust CLI retains
its Apache-2.0 license. Do not advertise the entire bundle as Apache-only.

Linux binaries target glibc; musl/Alpine and other CPU architectures are not supported
targets. Minimum runtime versions must be checked against the final binary's
linkage manifest, rather than inferred from its filename.

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
- `cerul-npm-package.tar.gz`, the `cerul` npm wrapper at the manifest version.
- A source archive and aggregate checksums.

The source archive uses committed Git content. Regenerate it after committing;
uncommitted changes are not included in the release source.
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

Embedded OCR weights account for much of the package size. Measure the final
`.crate` archive and confirm package ownership and the registry's upload limit
before publishing to crates.io; see the
[Cargo publishing guide](https://doc.rust-lang.org/cargo/reference/publishing.html).
Registry publication is not configured by the release workflow. Keep the
embedded OCR runtime contract; do not omit the weights or replace them with
a first-run download to make the upload fit.

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

With `pr-run-mode = "upload"`, pull requests plan and build both platform
archives and upload CI artifacts for inspection. Matching version tags also
publish those artifacts as a GitHub Release. The permanent website redirect tracks
the latest published release; it needs no per-release website change.

npm and Homebrew artifacts are generated, but registry/tap publishing is not
configured. Before enabling those channels, confirm npm package ownership and
the Homebrew distribution destination, configure their publishers, and verify
installation through each public channel. A downloadable wrapper or formula
alone does not establish registry/tap availability.
