#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
host=$(rustc -vV | sed -n 's/^host: //p')
target=${CARGO_DIST_TARGET:-$host}
if [[ "$target" != "$host" ]]; then echo 'Distribution builds must run on the target platform.' >&2; exit 1; fi
bash scripts/build-media.sh
cargo build --release --locked --bin cerul
cp target/release/cerul target/bundle/cerul
if [[ "${CI:-}" == true ]]; then
  python3 scripts/test-bundle.py target/bundle
fi
