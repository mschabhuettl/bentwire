#!/usr/bin/env bash
# Installs a bentwire Flatpak bundle in a fresh rootless podman container and checks it
# (docs/flatpak.md): `flatpak install --user` of the bundle (the runtime comes from
# Flathub), `bentwired --headless` started through `flatpak run` on a private session bus,
# the character surface mapped after `bentwire show`, a word balloon after `bentwire say hi`
# (both CLIs run inside the sandbox), and espeak-ng loadable in the sandbox.
# With --classics also `bentwire characters install classics` (downloads the pack).
#
# usage: scripts/flatpak-check.sh BUNDLE.flatpak [--classics]
# PODMAN_FLAGS: podman run flags (default below: the least that lets bubblewrap run nested).
# Needs podman and network access (Debian packages, the Flathub runtime).
set -euo pipefail
BUNDLE=$(realpath "${1:?usage: flatpak-check.sh BUNDLE.flatpak [--classics]}")
CLASSICS=${2:-}
IMAGE=${BENTWIRE_FLATPAK_IMAGE:-docker.io/library/debian:trixie}
# bubblewrap needs unprivileged user namespaces and mounts (seccomp, masked /proc paths).
read -r -a FLAGS <<<"${PODMAN_FLAGS:---security-opt seccomp=unconfined --security-opt unmask=ALL --security-opt label=disable}"

INNER=$(cat <<'EOF'
set -euo pipefail
APP=org.bentwire.Agent
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq flatpak dbus python3 libglib2.0-bin ca-certificates >/dev/null
# flatpak install of a bundle talks to the system bus, which every desktop has.
mkdir -p /run/dbus && dbus-daemon --system --fork
useradd -m tester
su tester -c 'bash -s' <<'USER'
set -euo pipefail
APP=org.bentwire.Agent
FAILS=0
check() { if "${@:2}"; then echo "  ok   $1"; else echo "  FAIL $1"; FAILS=$((FAILS + 1)); fi; }
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y --noninteractive /bundle.flatpak >/tmp/install.log 2>&1 || { cat /tmp/install.log; exit 1; }
echo "  ok   flatpak install --user of the bundle"
flatpak info --user "$APP" | sed -n 's/^ *\(Version\|Runtime\): /    \1: /p'
export XDG_RUNTIME_DIR=/tmp/run
mkdir -m 0700 -p "$XDG_RUNTIME_DIR"
cat >/tmp/session.sh <<'SESSION'
set -uo pipefail
APP=org.bentwire.Agent
FAILS=0
check() { if "${@:2}"; then echo "  ok   $1"; else echo "  FAIL $1"; FAILS=$((FAILS + 1)); fi; }
bw() { flatpak run --command=bentwire "$APP" "$@"; }
test1() { gdbus call --session -d org.bentwire.Agent -o /org/bentwire/Test -m "org.bentwire.Test1.$1" "${@:2}"; }
surface() { # surface KIND: a surface of that kind in Test1.Snapshot
    test1 Snapshot | python3 -I -c '
import ast, json, sys
s = json.loads(ast.literal_eval(sys.stdin.read().strip())[0])
sys.exit(0 if any(x["kind"] == sys.argv[1] and x["mapped"] for x in s["surfaces"]) else 1)' "$1"
}
flatpak run --command=bentwired "$APP" --headless --no-audio >/tmp/bentwired.log 2>&1 &
DAEMON=$!
for _ in $(seq 100); do bw status >/dev/null 2>&1 && break; sleep 0.2; done
check "bentwired --headless up in the sandbox (bentwire status)" bw status
# --wait needs RequestComplete, which crosses the D-Bus proxy (broadcast in a sandbox).
timeout 30 flatpak run --command=bentwire "$APP" show --wait >/tmp/show.log 2>&1 &
SHOW=$!
for _ in $(seq 50); do test1 Advance 200 >/dev/null; kill -0 "$SHOW" 2>/dev/null || break; sleep 0.2; done
check "bentwire show --wait completes (RequestComplete reaches the CLI)" wait "$SHOW"
check "character surface mapped after bentwire show" surface character
check "Wire (builtin, from /app/share) is the character" sh -c "flatpak run --command=bentwire $APP status | grep -qx 'character=wire'"
bw say hi >/dev/null
test1 Advance 500 >/dev/null
check "word balloon after bentwire say hi" surface agent_balloon
# espeak_Initialize(AUDIO_OUTPUT_SYNCHRONOUS, 0, NULL, 0) returns the sample rate, or -1
# without espeak-ng-data.
check "espeak-ng initialises in the sandbox" sh -c "flatpak run --command=python3 $APP -c \
    'import ctypes; print(ctypes.CDLL(\"libespeak-ng.so.1\").espeak_Initialize(2, 0, None, 0))' \
    | grep -qx 22050"
if [ -n "${CLASSICS:-}" ]; then
    check "bentwire characters install classics" bw characters install classics
    check "classics pack in the app's data dir" test -f "$HOME/.var/app/$APP/data/bentwire/characters/classics/pack.toml"
    check "daemon lists clippit" sh -c "flatpak run --command=bentwire $APP characters | grep -q '^clippit'"
fi
bw quit >/dev/null 2>&1 || true
for _ in $(seq 50); do kill -0 "$DAEMON" 2>/dev/null || break; sleep 0.2; done
if grep -q panicked /tmp/bentwired.log; then echo "  FAIL daemon panicked"; FAILS=$((FAILS + 1)); fi
[ "$FAILS" -eq 0 ] || { echo "--- bentwired.log"; cat /tmp/bentwired.log; exit 1; }
SESSION
CLASSICS=${CLASSICS:-} dbus-run-session -- bash /tmp/session.sh
USER
EOF
)

echo "flatpak-check: $IMAGE, podman flags: ${FLAGS[*]}"
podman run --rm "${FLAGS[@]}" -e CLASSICS="$CLASSICS" -v "$BUNDLE:/bundle.flatpak:ro" \
    "$IMAGE" bash -c "$INNER"
echo "flatpak-check: all checks passed"
