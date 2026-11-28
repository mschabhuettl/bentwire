#!/usr/bin/env bash
# Builds the Flatpak (packaging/flatpak/org.bentwire.Agent.yml) and exports a single-file
# bundle. Needs flatpak, flatpak-builder and the flathub remote in the user installation
# (the runtime, SDK and Rust extension are installed from it).
# usage: scripts/flatpak-build.sh [OUT.flatpak]
# BENTWIRE_FLATPAK_WORK: build, repo and builder state (default target/flatpak); the bundle
# defaults to WORK/bentwire-VERSION-x86_64.flatpak.
set -euo pipefail
REPO=$(cd "$(dirname "$0")/.." && pwd)
APP=org.bentwire.Agent
VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$REPO/Cargo.toml" | head -1)
WORK=${BENTWIRE_FLATPAK_WORK:-$REPO/target/flatpak}
OUT=${1:-$WORK/bentwire-$VERSION-x86_64.flatpak}
mkdir -p "$WORK"

flatpak-builder --user --install-deps-from=flathub --force-clean --disable-rofiles-fuse \
    --state-dir="$WORK/state" --repo="$WORK/repo" ${FLATPAK_BUILDER_JOBS:+--jobs="$FLATPAK_BUILDER_JOBS"} \
    "$WORK/build" "$REPO/packaging/flatpak/$APP.yml"
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$WORK/repo" "$OUT" "$APP"
echo "bundle: $OUT"
