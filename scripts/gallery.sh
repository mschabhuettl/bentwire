#!/usr/bin/env bash
# Gallery images: every character in its rest pose (B-SHOW-3 picture), 1x, transparent
# background, one PNG per character, byte-identical on every run.
#   builtin  -> docs/gallery/<id>.png
#   classics -> <classics pack>/gallery/<id>.png (BENTWIRE_CLASSICS, default
#               ../bentwire-classics; skipped with a note if the pack is not there)
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
cargo build --release -q -p bentwired --example gallery
gallery=${CARGO_TARGET_DIR:-target}/release/examples/gallery
run() { # pack dir, output dir
  mkdir -p "$2"
  find "$2" -maxdepth 1 -name '*.png' -delete
  "$gallery" "$1" "$2" > /dev/null
  echo "$2: $(find "$2" -maxdepth 1 -name '*.png' | wc -l) images"
}
run characters/builtin docs/gallery
classics=${BENTWIRE_CLASSICS:-../bentwire-classics}
if [ -f "$classics/pack.toml" ]; then
  run "$classics" "$classics/gallery"
else
  echo "no classics pack at $classics (set BENTWIRE_CLASSICS); classics gallery not made"
fi
