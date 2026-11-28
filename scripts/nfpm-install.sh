#!/usr/bin/env bash
# Installs the pinned nfpm release (checked against its SHA-256) into DIR (CI).
# usage: scripts/nfpm-install.sh DIR
set -euo pipefail
VERSION=2.48.0
SHA256=6d7f9a28b2f397934403950fcbba83f598b08d02edbe79a0815b3b4897341750
DIR=${1:?usage: nfpm-install.sh DIR}
TMP=$(mktemp -d)
trap 'rm -rf "${TMP:?}"' EXIT
curl -fsSL -o "$TMP/nfpm.tar.gz" \
    "https://github.com/goreleaser/nfpm/releases/download/v$VERSION/nfpm_${VERSION}_Linux_x86_64.tar.gz"
echo "$SHA256  $TMP/nfpm.tar.gz" | sha256sum -c --quiet
tar -C "$TMP" -xzf "$TMP/nfpm.tar.gz" nfpm
install -Dm755 "$TMP/nfpm" -t "$DIR/"
