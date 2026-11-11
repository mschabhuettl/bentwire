#!/usr/bin/env bash
# Real-data checks (architecture A-TEST-1, A-TEST-2): round trips against the Python oracle,
# mutation test, and every other #[ignore] test of the workspace.
# Needs BENTWIRE_SOURCES (one or more directories, colon-separated, with your own copies of
# the original character files and DLLs, in any layout; found by file name and SHA-256, see
# docs/sources.md) and BENTWIRE_REFERENCE (a directory with the Python reference parsers and
# prototype used as test oracle, not public).
# BENTWIRE_CLASSICS: the classics pack checkout, default ../bentwire-classics next to this repo.
# BENTWIRE_PACKS defaults to a temp characters/ dir holding characters/builtin and the
# classics pack.
# Optional: BENTWIRE_PROTO_COPY = an existing scratch copy of the prototype (keeps its pickle
# cache between runs); without it a fresh copy is made in a temp dir and removed afterwards.
set -euo pipefail
for v in BENTWIRE_SOURCES BENTWIRE_REFERENCE; do
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
  # The prototype is the directory below BENTWIRE_REFERENCE holding oracle.py and reader.py.
  proto=$(find "$BENTWIRE_REFERENCE" -name oracle.py -execdir test -f reader.py \; -printf '%h\n' |
    LC_ALL=C sort | head -n 1)
  if [ -z "$proto" ]; then
    echo "NOT RUN: no prototype (oracle.py, reader.py) below BENTWIRE_REFERENCE"
    exit 77
  fi
  rsync -a --exclude msadp32/ --exclude out/ --exclude __pycache__/ "$proto/" "$tmp/proto/"
  export BENTWIRE_PROTO_COPY="$tmp/proto"
fi
cargo test --release --workspace -- --ignored --nocapture "$@"
