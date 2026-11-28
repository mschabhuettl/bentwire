#!/usr/bin/env bash
# Builds bentwire_VERSION-1_amd64.deb and bentwire-VERSION-1.x86_64.rpm into OUTDIR from
# release binaries in BINDIR and this checkout (packaging/nfpm/nfpm.yaml, needs nfpm).
# The staged layout follows packaging/aur/bentwire/PKGBUILD package().
#
# usage: scripts/package-build.sh VERSION BINDIR OUTDIR
set -euo pipefail
VERSION=${1:?usage: package-build.sh VERSION BINDIR OUTDIR}
BINDIR=$(realpath "${2:?usage: package-build.sh VERSION BINDIR OUTDIR}")
OUTDIR=${3:?usage: package-build.sh VERSION BINDIR OUTDIR}
mkdir -p "$OUTDIR"
OUTDIR=$(realpath "$OUTDIR")
cd "$(dirname "$0")/.."
STAGE=$(mktemp -d)
trap 'rm -rf "${STAGE:?}"' EXIT

install -Dm755 "$BINDIR"/{bentwired,bentwire,bentwire-convert,bentwire-notify,bentwire-mqtt} \
    -t "$STAGE/usr/bin/"
install -d "$STAGE/usr/share/bentwire/characters"
cp -r --no-preserve=ownership characters/builtin "$STAGE/usr/share/bentwire/characters/"
install -Dm644 packaging/dbus/org.bentwire.Agent.service -t "$STAGE/usr/share/dbus-1/services/"
install -Dm644 packaging/systemd/{bentwired,bentwire-notify,bentwire-mqtt}.service \
    -t "$STAGE/usr/lib/systemd/user/"
install -d "$STAGE/usr/share/bentwire/clients"
cp -a --no-preserve=ownership clients/{shell,claude-code,git,systemd} \
    "$STAGE/usr/share/bentwire/clients/"
rm "$STAGE"/usr/share/bentwire/clients/*/test.sh
sed 's|%h/.local/share/bentwire/systemd/|/usr/share/bentwire/clients/systemd/|' \
    clients/systemd/bentwire-failure@.service >"$STAGE/usr/lib/systemd/user/bentwire-failure@.service"
chmod 644 "$STAGE/usr/lib/systemd/user/bentwire-failure@.service"
install -Dm644 README.md -t "$STAGE/usr/share/doc/bentwire/"
install -Dm644 LICENSE-MIT LICENSE-APACHE assets/fonts/LICENSE-LGPL-2.1.txt \
    -t "$STAGE/usr/share/licenses/bentwire/"

# The checkout's umask may leave group-writable modes.
chmod -R u=rwX,go=rX "$STAGE"
# nfpm reads the contents relative to the working directory.
CONFIG=$PWD/packaging/nfpm/nfpm.yaml
export VERSION
for p in deb rpm; do
    (cd "$STAGE" && nfpm package --config "$CONFIG" --packager "$p" --target "$OUTDIR/")
done
