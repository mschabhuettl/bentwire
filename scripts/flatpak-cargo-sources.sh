#!/usr/bin/env bash
# Regenerates packaging/flatpak/cargo-sources.json from Cargo.lock with flatpak-builder-tools'
# flatpak-cargo-generator (pinned below; needs python3 with aiohttp and tomlkit).
# usage: scripts/flatpak-cargo-sources.sh [--check]
#   --check: fail (exit 1) when the committed file does not match Cargo.lock.
set -euo pipefail
REPO=$(cd "$(dirname "$0")/.." && pwd)
GEN_COMMIT=9d86d336a39b9af38b86f114b6ce5820fe260f91
GEN_URL=https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$GEN_COMMIT/cargo/flatpak-cargo-generator.py
DEST=$REPO/packaging/flatpak/cargo-sources.json

T=$(mktemp -d)
trap 'rm -rf "${T:?}"' EXIT
mkdir "$T/gen"
curl -fsSL "$GEN_URL" -o "$T/gen/flatpak-cargo-generator.py"
(cd "$T" && python3 -I gen/flatpak-cargo-generator.py "$REPO/Cargo.lock" -o "$T/cargo-sources.json")

if [ "${1:-}" = --check ]; then
    if ! diff -q "$DEST" "$T/cargo-sources.json" >/dev/null; then
        echo "packaging/flatpak/cargo-sources.json is stale: run scripts/flatpak-cargo-sources.sh" >&2
        exit 1
    fi
    echo "cargo-sources.json matches Cargo.lock"
else
    cp "$T/cargo-sources.json" "$DEST"
fi
