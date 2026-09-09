#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
bash "$root/scripts/build-media.sh" --sources-only
cp "$root/THIRD_PARTY_NOTICES.md" "$root/target/bundle/media-source/"
tar -czf "$root/cerul-media-source.tar.gz" -C "$root/target/bundle" media-source
