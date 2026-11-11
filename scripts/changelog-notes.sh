#!/usr/bin/env bash
# Print the body of the section `## [VERSION] ...` of a Keep a Changelog file (release notes).
# Fails when the section is missing or empty.
#   scripts/changelog-notes.sh 0.1.0 CHANGELOG.md
set -euo pipefail
version=$1
file=${2:-CHANGELOG.md}
notes=$(awk -v v="$version" '
  /^## \[/ { if (on) exit; on = index($0, "## [" v "]") == 1; next }
  on' "$file")
if [ -z "${notes//[[:space:]]/}" ]; then
  echo "error: no notes for $version in $file" >&2
  exit 1
fi
printf '%s\n' "$notes"
