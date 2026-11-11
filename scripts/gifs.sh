#!/bin/bash
# Animated GIFs on the Plasma desktop: scripts/screenshots.sh in its GIF mode.
# usage: scripts/gifs.sh [--classics DIR] [BIN_DIR]
set -euo pipefail
here=$(dirname "$0")
if [ "${1:-}" = --classics ]; then
    exec "$here/screenshots.sh" --classics "${2:?--classics needs a directory}" --gifs "${@:3}"
fi
exec "$here/screenshots.sh" --gifs "$@"
