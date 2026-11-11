#!/usr/bin/env bash
# Real-data checks (architecture A-TEST-1, A-TEST-2): round trips against the Python oracle,
# mutation test, and every other #[ignore] test of the workspace.
# Needs BENTWIRE_REFERENCE (the research tree) and BENTWIRE_SOURCES (the owner's files).
# BENTWIRE_CLASSICS: the classics pack checkout, default ../bentwire-classics next to this repo.
# BENTWIRE_PACKS defaults to a temp characters/ dir holding characters/builtin and the
# classics pack.
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
classics=${BENTWIRE_CLASSICS:-../bentwire-classics}
if [ ! -f "$classics/pack.toml" ]; then
  echo "NOT RUN: no classics pack at $classics (set BENTWIRE_CLASSICS)"
  exit 77
fi
export BENTWIRE_CLASSICS
BENTWIRE_CLASSICS=$(cd "$classics" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "${tmp:?}"' EXIT
if [ -z "${BENTWIRE_PACKS:-}" ]; then
  mkdir "$tmp/characters"
  ln -s "$PWD/characters/builtin" "$tmp/characters/builtin"
  ln -s "$BENTWIRE_CLASSICS" "$tmp/characters/classics"
  export BENTWIRE_PACKS="$tmp/characters"
fi
if [ -z "${BENTWIRE_PROTO_COPY:-}" ]; then
  rsync -a --exclude msadp32/ --exclude out/ --exclude __pycache__/ \
    "$BENTWIRE_REFERENCE/prototype/" "$tmp/proto/"
  export BENTWIRE_PROTO_COPY="$tmp/proto"
fi
cargo test --release --workspace -- --ignored --nocapture "$@"
