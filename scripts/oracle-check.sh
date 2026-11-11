#!/usr/bin/env bash
# Real-data checks (architecture A-TEST-1, A-TEST-2): round trips against the Python oracle,
# mutation test, and every other #[ignore] test of the workspace.
# Needs BENTWIRE_REFERENCE ($BENTWIRE_REFERENCE) and BENTWIRE_SOURCES (~/Sources/bentwire).
# BENTWIRE_PACKS defaults to the repo's characters/ (all packs found there).
# Optional: BENTWIRE_PROTO_COPY = an existing scratch copy of prototype (keeps its pickle
# cache between runs); without it a fresh copy is made in a temp dir and removed afterwards.
set -euo pipefail
for v in BENTWIRE_REFERENCE BENTWIRE_SOURCES; do
  if [ -z "${!v:-}" ]; then
    echo "NOT RUN: set $v"
    exit 77
  fi
done
export PATH="$HOME/.cargo/bin:$PATH" PYTHONDONTWRITEBYTECODE=1
cd "$(dirname "$0")/.."
# Packs for the daemon's ignored pack test (desktop): the repo's own packs by default.
export BENTWIRE_PACKS="${BENTWIRE_PACKS:-$PWD/characters}"
export BENTWIRE_PACKS="${BENTWIRE_PACKS:-$PWD/characters}"
if [ -z "${BENTWIRE_PROTO_COPY:-}" ]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "${tmp:?}"' EXIT
  rsync -a --exclude msadp32/ --exclude out/ --exclude __pycache__/ \
    "$BENTWIRE_REFERENCE/prototype/" "$tmp/proto/"
  export BENTWIRE_PROTO_COPY="$tmp/proto"
fi
cargo test --release --workspace -- --ignored --nocapture "$@"
