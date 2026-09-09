# Build Cerul from source

Run these commands from a checkout of this repository. For a release bundle,
use the [installation guide](../installation.md).

The following build tools are only needed when compiling Cerul yourself.

## Check build dependencies

Check existing tools first. A missing command means that tool needs installation;
you do not need to reinstall tools that already meet these requirements.

```sh
uname -s
uname -m
rustc --version
cargo --version
protoc --version
pkg-config --version
```

| Dependency | Used for | Needed after compilation? |
| --- | --- | --- |
| Stable Rust, Cargo, C/C++ toolchain | Building the CLI | No |
| Protobuf compiler and build libraries | Building dependencies | No |
| pkg-config, make, curl, tar | Building the bundled media tools | No |
| FFmpeg and ffprobe | Media processing | Included in a complete build |
| OCR models | Local text recognition | Included in the CLI |
| Python 3.9+ | Contributor documentation tests; Python 3.12 for the official loader | No |
| CUDA, Ollama | Not required for the default workflow | No |

The current media pipeline uses the `libx264` encoder. A custom ffmpeg build
must provide it as well as the input decoders needed for your media. Check with
`ffmpeg -hide_banner -encoders` if using a nonstandard build.

## Install missing build dependencies

On macOS, use [Homebrew](https://brew.sh) if it is already installed:

```sh
brew install protobuf pkgconf
```

The Apple command-line developer tools are also required. If they are missing,
run `xcode-select --install` and complete the system dialog.

On Ubuntu 24.04:

```sh
sudo apt-get update
sudo apt-get install -y git build-essential nasm pkg-config libssl-dev protobuf-compiler libprotobuf-dev curl xz-utils bzip2
```

Install a stable Rust toolchain using the instructions at [rustup.rs](https://rustup.rs)
if Rust is missing. Open a new terminal after installation so Cargo is on PATH.
Do not run Cargo or Cerul with sudo.

## Build and verify

```sh
git clone https://github.com/cerul-ai/cerul.git
cd cerul
bash scripts/build-distribution.sh
export PATH="$PWD/target/bundle:$PATH"
cerul --version
cerul --json status
```

To test an unmerged change, check out its branch or commit before building.
The PATH change above applies to this shell; save it in your shell configuration
only if you want a persistent installation at this location.

`status` does not call a provider. It verifies that the CLI starts; it does not
prove media processing or model connectivity. `--dry-run index` previews the
operation but does not establish that all runtime dependencies work.

For contributor checks, see [validation](validation.md).
