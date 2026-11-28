#!/usr/bin/env bash
# Installs a bentwire .deb or .rpm in a fresh rootless podman container of the
# distribution and checks it: `apt-get install ./x.deb` or `dnf install ./x.rpm` (so the
# dependencies resolve, the recommended espeak-ng included), `bentwired --headless` on a
# private session bus as a fresh user with no XDG_DATA_DIRS, so Wire can only come from
# /usr/share/bentwire, the character surface mapped after `bentwire show`, a word balloon
# after `bentwire say hi`, espeak-ng loadable. Then `apt-get purge` or `dnf remove` of
# bentwire alone must leave the file list exactly as after the install minus the package's
# own paths (paths that existed before the install, such as /usr/bin, stay), and nothing
# with "bentwire" in its path may remain after the unused dependencies are removed too.
# The file lists leave out package manager state, caches, logs, /home, /tmp and /run.
#
# usage: scripts/package-check.sh PACKAGE.deb|PACKAGE.rpm [IMAGE]
# IMAGE defaults to docker.io/library/debian:trixie (.deb) or docker.io/library/fedora:43
# (.rpm). Needs podman (CONTAINER=docker works too) and network access.
set -euo pipefail
PKG=$(realpath "${1:?usage: package-check.sh PACKAGE.deb|PACKAGE.rpm [IMAGE]}")
case "$PKG" in
*.deb) KIND=deb IMAGE=${2:-docker.io/library/debian:trixie} ;;
*.rpm) KIND=rpm IMAGE=${2:-docker.io/library/fedora:43} ;;
*) echo "package-check: not a .deb or .rpm: $PKG" >&2; exit 2 ;;
esac

INNER=$(cat <<'EOF'
set -euo pipefail
FAILS=0
check() { if "${@:2}"; then echo "  ok   $1"; else echo "  FAIL $1"; FAILS=$((FAILS + 1)); fi; }
# Every path on the root file system except package manager state, caches, logs and
# per-user or runtime data.
snapshot() {
    find / -xdev \( -path /proc -o -path /sys -o -path /dev -o -path /run -o -path /tmp \
        -o -path /home -o -path /root -o -path /var/cache -o -path /var/log \
        -o -path /var/lib/apt -o -path /var/lib/dpkg -o -path /var/lib/dnf \
        -o -path /var/lib/rpm -o -path /usr/lib/sysimage -o -path /etc/ld.so.cache \
        -o -path /pkg.deb -o -path /pkg.rpm \) -prune -o -print | sort
}
if [ "$KIND" = deb ]; then
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq
    apt-get install -y -qq dbus python3 libglib2.0-bin >/dev/null
else
    dnf install -y -q dbus-daemon python3 glib2 findutils util-linux shadow-utils >/dev/null 2>&1
fi
# Before the first file list: useradd leaves backup files in /etc.
useradd -m tester
snapshot >/tmp/before
if [ "$KIND" = deb ]; then
    apt-get install -y -qq /pkg.deb >/tmp/install.log 2>&1 || { cat /tmp/install.log; exit 1; }
    echo "  ok   apt-get install ./$PKGNAME"
    dpkg-query -W -f '    ${Package} ${Version}\n    Depends: ${Depends}\n    Recommends: ${Recommends}\n' bentwire
    dpkg -L bentwire | sort >/tmp/files
else
    dnf install -y /pkg.rpm >/tmp/install.log 2>&1 || { cat /tmp/install.log; exit 1; }
    echo "  ok   dnf install ./$PKGNAME"
    rpm -q bentwire | sed 's/^/    /'
    echo "    Requires: $(rpm -q --requires bentwire | grep -v '^rpmlib(' | paste -sd ' ')"
    echo "    Recommends: $(rpm -q --recommends bentwire | paste -sd ' ')"
    rpm -ql bentwire | sort >/tmp/files
fi
snapshot >/tmp/installed
echo "    $(wc -l </tmp/files) paths in the package, $(grep -c '^/usr/bin/' /tmp/files) in /usr/bin"
grep -q '^/usr/share/bentwire/characters/builtin/wire/' /tmp/files

cat >/tmp/session.sh <<'SESSION'
set -uo pipefail
FAILS=0
check() { if "${@:2}"; then echo "  ok   $1"; else echo "  FAIL $1"; FAILS=$((FAILS + 1)); fi; }
test1() { gdbus call --session -d org.bentwire.Agent -o /org/bentwire/Test -m "org.bentwire.Test1.$1" "${@:2}"; }
surface() { # surface KIND: a mapped surface of that kind in Test1.Snapshot
    test1 Snapshot | python3 -I -c '
import ast, json, sys
s = json.loads(ast.literal_eval(sys.stdin.read().strip())[0])
sys.exit(0 if any(x["kind"] == sys.argv[1] and x["mapped"] for x in s["surfaces"]) else 1)' "$1"
}
unset XDG_DATA_DIRS XDG_DATA_HOME
check "no user character dir" test ! -e "$HOME/.local/share/bentwire"
bentwired --headless --no-audio >/tmp/bentwired.log 2>&1 &
DAEMON=$!
# Wait for the name without calling it: a call would D-Bus-activate /usr/bin/bentwired.
check "bentwired --headless owns org.bentwire.Agent" gdbus wait --session --timeout 30 org.bentwire.Agent
check "bentwire status" sh -c "bentwire status >/dev/null"
timeout 30 bentwire show --wait >/tmp/show.log 2>&1 &
SHOW=$!
for _ in $(seq 50); do test1 Advance 200 >/dev/null; kill -0 "$SHOW" 2>/dev/null || break; sleep 0.2; done
check "bentwire show --wait completes" wait "$SHOW"
check "character surface mapped after bentwire show" surface character
check "Wire (builtin, from /usr/share/bentwire) is the character" sh -c "bentwire status | grep -qx 'character=wire'"
check "builtin characters listed" sh -c "bentwire characters | cut -f1 | tr '\n' ' ' | grep -q 'hermit .*quack .*wire '"
bentwire say hi >/dev/null
test1 Advance 500 >/dev/null
check "word balloon after bentwire say hi" surface agent_balloon
# espeak_Initialize(AUDIO_OUTPUT_SYNCHRONOUS, 0, NULL, 0) returns the sample rate, or -1
# without espeak-ng-data.
check "espeak-ng (recommended) initialises" sh -c "python3 -I -c \
    'import ctypes; print(ctypes.CDLL(\"libespeak-ng.so.1\").espeak_Initialize(2, 0, None, 0))' \
    | grep -qx 22050"
bentwire quit >/dev/null 2>&1 || true
for _ in $(seq 50); do kill -0 "$DAEMON" 2>/dev/null || break; sleep 0.2; done
check "daemon exited after bentwire quit" sh -c "! kill -0 $DAEMON 2>/dev/null"
if grep -q panicked /tmp/bentwired.log; then echo "  FAIL daemon panicked"; FAILS=$((FAILS + 1)); fi
[ "$FAILS" -eq 0 ] || { echo "--- bentwired.log"; cat /tmp/bentwired.log; exit 1; }
SESSION
runuser -u tester -- dbus-run-session -- bash /tmp/session.sh || FAILS=$((FAILS + 1))

if [ "$KIND" = deb ]; then
    apt-get purge -y -qq bentwire >/dev/null
    check "apt-get purge bentwire" sh -c "! dpkg -s bentwire >/dev/null 2>&1"
else
    dnf remove -y -q --noautoremove bentwire >/dev/null 2>&1
    check "dnf remove bentwire" sh -c "! rpm -q bentwire >/dev/null 2>&1"
fi
snapshot >/tmp/removed
# Paths of the package that stay must have been there before the install.
{ comm -12 /tmp/files /tmp/removed | comm -23 - /tmp/before; comm -13 /tmp/installed /tmp/removed; } >/tmp/left
check "no path of the package left, nothing new (file lists after install and removal)" \
    test ! -s /tmp/left
sed 's/^/    left: /' /tmp/left
# Parent directories of package paths that are new since before the install and owned by
# no package: created by the install but not owned by the package.
while read -r f; do d=$f; while d=$(dirname "$d") && [ "$d" != / ]; do echo "$d"; done; done \
    </tmp/files | sort -u | comm -12 - /tmp/removed | comm -23 - /tmp/before | while read -r f; do
    if [ "$KIND" = deb ]; then dpkg -S "$f" >/dev/null 2>&1 || echo "$f"
    else rpm -qf "$f" >/dev/null 2>&1 || echo "$f"; fi
done >/tmp/unowned
check "no unowned parent directory of a package path left" test ! -s /tmp/unowned
sed 's/^/    unowned: /' /tmp/unowned
if [ "$KIND" = deb ]; then
    apt-get autoremove --purge -y -qq >/dev/null
else
    dnf autoremove -y -q >/dev/null 2>&1
fi
snapshot >/tmp/after
comm -13 /tmp/before /tmp/after >/tmp/deps
check "nothing named bentwire left after removing the dependencies too" \
    sh -c "! grep -qi bentwire /tmp/deps"
sed 's/^/    left by the removed dependencies (not package paths): /' /tmp/deps
[ "$FAILS" -eq 0 ]
EOF
)

echo "package-check: $(basename "$PKG") in $IMAGE"
"${CONTAINER:-podman}" run --rm -e KIND="$KIND" -e PKGNAME="$(basename "$PKG")" \
    -v "$PKG:/pkg.$KIND:ro" "$IMAGE" bash -c "$INNER"
echo "package-check: all checks passed"
