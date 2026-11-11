#!/bin/bash
# Reproducible screenshots (release brief 0.1, section 4): bentwired under headless sway
# (pixman renderer), grim, a fixed 1280x720 output, seed 1, a plain desktop colour and
# fresh XDG directories on a private session bus. No timing guesses: every shot waits for
# the request to complete (--wait) or for the surface to appear in Test1.Snapshot, then
# grabs until two consecutive crops are identical. Crops come from the Snapshot geometry.
#
# usage: scripts/screenshots.sh [--classics DIR] [BIN_DIR]
#   without --classics: only characters/builtin; writes the placeholder shots to
#     docs/screenshots/ (no side-by-side image: builtin has one character).
#   --classics DIR: the classics pack in DIR; writes to DIR/screenshots/.
# Build first: cargo build -p bentwired -p bentwire && cargo build -p bentwired --examples
# Needs: sway, swaybg, grim, dbus-run-session, busctl, python3 with PIL.
# The config pins balloon_font = "Tahoma" (bundled), so the Agent balloon does not depend
# on the host's fontconfig (B-BAL-2).
set -euo pipefail

REPO=$(cd "$(dirname "$0")/.." && pwd)
CLASSICS=
if [ "${1:-}" = --classics ]; then
    CLASSICS=$(cd "${2:?--classics needs a directory}" && pwd)
    shift 2
fi
BIN=$(cd "${1:-$REPO/target/debug}" && pwd)

if [ -z "${BENTWIRE_SHOTS_INNER:-}" ]; then
    T=$(mktemp -d)
    echo "screenshots: work dir $T"
    mkdir -m 0700 "$T/run"
    # Nothing from the caller's environment (BENTWIRE_*, XDG dirs, locale) reaches the run.
    for v in $(env | sed -n 's/^\(BENTWIRE_[A-Z_]*\)=.*/\1/p'); do unset "$v"; done
    export XDG_RUNTIME_DIR=$T/run HOME=$T/home XDG_CONFIG_HOME=$T/home/cfg
    export XDG_STATE_HOME=$T/home/state XDG_DATA_HOME=$T/home/data XDG_CACHE_HOME=$T/home/cache
    export XDG_DATA_DIRS=$T/nonexistent LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC
    export BENTWIRE_SHOTS_INNER=1 SHOTS_DIR=$T
    mkdir -p "$HOME"
    rc=0
    if [ -n "$CLASSICS" ]; then
        dbus-run-session -- "$0" --classics "$CLASSICS" "$BIN" || rc=$?
    else
        dbus-run-session -- "$0" "$BIN" || rc=$?
    fi
    [ "$rc" -eq 0 ] && rm -rf "${T:?}"
    exit "$rc"
fi

T=$SHOTS_DIR
PY="python3 -I $REPO/scripts/screenshots/shot.py"
W=1280 H=720 BG=3a6ea5 MARGIN=12 X=600 Y=330
if [ -n "$CLASSICS" ]; then
    OUT=$CLASSICS/screenshots
    ROOTS=(--characters "$CLASSICS")
    MAIN=clippit
else
    OUT=$REPO/docs/screenshots
    ROOTS=(--characters "$REPO/characters/builtin")
    MAIN=bentwire-placeholder
fi
mkdir -p "$OUT"

PIDS=()
cleanup() {
    for p in "${PIDS[@]}" $(jobs -p); do kill "$p" 2>/dev/null || true; done
    wait 2>/dev/null || true
}
trap cleanup EXIT
trap 'echo "screenshots aborted at line $LINENO: $BASH_COMMAND"' ERR

wait_for() { # wait_for <seconds> <command...>: polls a condition, not a timing guess
    local n=$(($1 * 20))
    shift
    for _ in $(seq "$n"); do "$@" >/dev/null 2>&1 && return 0; sleep 0.05; done
    echo "timed out: $*"
    return 1
}

export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
cat >"$T/sway.conf" <<EOF
output HEADLESS-1 resolution ${W}x${H} position 0 0 scale 1 bg #$BG solid_color
seat * hide_cursor 0
EOF
sway -c "$T/sway.conf" >"$T/sway.log" 2>&1 &
PIDS+=($!)
wait_for 10 sh -c 'ls "$XDG_RUNTIME_DIR" | grep -Eq "^wayland-[0-9]+$"'
WAYLAND_DISPLAY=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | head -1)
export WAYLAND_DISPLAY
wait_for 10 sh -c 'ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock'
SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock | head -1)
export SWAYSOCK
bg_ready() { grim -g "0,0 1x1" -t ppm "$T/px.ppm" && $PY pixel "$T/px.ppm" "$BG"; }
wait_for 10 bg_ready

# One virtual pointer for the whole run; it is parked in the top-left corner for shots.
mkfifo "$T/vptr"
"$BIN/examples/vptr" $W $H <"$T/vptr" >"$T/vptr.log" 2>&1 &
PIDS+=($!)
exec 7>"$T/vptr"
wait_for 10 sh -c 'swaymsg -t get_inputs | grep -q "\"type\": \"pointer\""'
vp() { printf '%s\n' "$@" >&7; }

TEST="busctl --user call org.bentwire.Agent /org/bentwire/Test org.bentwire.Test1"
snapshot() { # snapshot FILE
    busctl --user --json=short call org.bentwire.Agent /org/bentwire/Test org.bentwire.Test1 Snapshot \
        | python3 -I -c 'import sys, json; print(json.load(sys.stdin)["data"][0])' >"$1"
}
has() { snapshot "$T/s.json" && $PY has "$T/s.json" "$@"; }
no_office() { ! has office_balloon; }
scale2() { snapshot "$T/s.json" && python3 -I -c 'import json, sys; sys.exit(json.load(open(sys.argv[1]))["backend"]["character"]["buffer_scale"] != 2)' "$T/s.json"; }

BW=
start_daemon() { # start_daemon <character> <language>: fresh config and state, seed 1
    rm -rf "${XDG_CONFIG_HOME:?}" "${XDG_STATE_HOME:?}"
    mkdir -p "$XDG_CONFIG_HOME/bentwire" "$XDG_STATE_HOME/bentwire"
    cat >"$XDG_CONFIG_HOME/bentwire/config.toml" <<EOF
character = "$1"
language = "$2"
idle = false
sound_effects = false
balloon_font = "Tahoma"
EOF
    printf 'character = "%s"\nleft = %d\ntop = %d\nvisible = true\n' "$1" $X $Y \
        >"$XDG_STATE_HOME/bentwire/state.toml"
    "$BIN/bentwired" --seed 1 --test-interface --no-tray --no-audio "${ROOTS[@]}" \
        >>"$T/bentwired.log" 2>&1 &
    BW=$!
    wait_for 10 busctl --user status org.bentwire.Agent
    "$BIN/bentwire" show --wait >/dev/null
    test "$("$BIN/bentwire" status | sed -n 's/^character=//p')" = "$1" \
        || { echo "character $1 not selected"; exit 1; }
    wait_for 10 has character
}
stop_daemon() {
    "$BIN/bentwire" quit >/dev/null || true
    wait "$BW" 2>/dev/null || true
}

SCALE=1
capture() { # capture <out.png> <surface kind...>: grab until two crops agree
    local out=$1 prev=
    shift
    for i in $(seq 100); do
        snapshot "$T/a.json"
        grim -t ppm "$T/g.ppm"
        snapshot "$T/b.json"
        if cmp -s "$T/a.json" "$T/b.json" && $PY has "$T/a.json" "$@"; then
            # shellcheck disable=SC2046
            $PY crop "$T/g.ppm" "$T/c$i.png" "$SCALE" $($PY rect "$T/a.json" "$MARGIN" "$@")
            if [ -n "$prev" ] && cmp -s "$prev" "$T/c$i.png"; then
                cp "$T/c$i.png" "$out"
                echo "  $(basename "$out")"
                return 0
            fi
            prev=$T/c$i.png
        else
            prev=
        fi
    done
    echo "no stable frame for $out"
    return 1
}

shoot_set() { # shoot_set <character> <prefix>: idle, say, ask, menu (en)
    local c=$1 p=$2
    start_daemon "$c" en
    vp "move 0 0"
    capture "$OUT/${p}idle.png" character
    "$BIN/bentwire" say "It looks like your build failed. Want me to show the log?" --wait >/dev/null
    capture "$OUT/${p}say.png" character agent_balloon
    "$BIN/bentwire" ask "The build failed in 3 crates." --heading "Build failed" \
        --button "Show log" --button "Ignore" >"$T/ask.out" &
    local ask=$!
    wait_for 10 has office_balloon
    capture "$OUT/${p}ask.png" character office_balloon
    $TEST Key s Return >/dev/null
    wait "$ask" || true
    wait_for 10 no_office
    stop_daemon
}

menu_shot() { # menu_shot <character> <language> <out.png>: real right click (vptr)
    start_daemon "$1" "$2"
    vp "move 0 0"
    capture "$T/menu-idle.png" character >/dev/null # leaves a settled grab in g.ppm
    # shellcheck disable=SC2046
    read -r px py < <($PY opaque "$T/g.ppm" "$BG" $($PY rect "$T/a.json" 0 character))
    vp "move $px $py" "press 2" "release 2"
    wait_for 10 has menu
    vp "move 0 0"
    capture "$3" character menu
    $TEST Key s Escape >/dev/null
    stop_daemon
}

echo "screenshots into $OUT"
shoot_set "$MAIN" ""
menu_shot "$MAIN" en "$OUT/menu-en.png"
menu_shot "$MAIN" de "$OUT/menu-de.png"

if [ -n "$CLASSICS" ]; then
    # Agent 2.0, Agent 1.5, Office 97 bitmap, Office 97 vector; one daemon each.
    PARTS=()
    for c in merlin merlin-15 genius-97 clippit-97; do
        start_daemon "$c" en
        vp "move 0 0"
        capture "$T/part-$c.png" character
        PARTS+=("$T/part-$c.png")
        stop_daemon
    done
    $PY compose "$OUT/characters.png" "$BG" 0 "${PARTS[@]}"
    echo "  characters.png"
fi

echo "HiDPI (output scale 2)"
swaymsg -q output HEADLESS-1 resolution $((W * 2))x$((H * 2)) scale 2
hidpi_ready() { grim -t ppm "$T/px.ppm" && python3 -I -c 'from PIL import Image; import sys; sys.exit(Image.open(sys.argv[1]).width != int(sys.argv[2]))' "$T/px.ppm" $((W * 2)); }
wait_for 10 hidpi_ready
SCALE=2
HI=$MAIN
[ -n "$CLASSICS" ] && HI=clippit-97
start_daemon "$HI" en
vp "move 0 0"
wait_for 10 scale2
"$BIN/bentwire" say "Sharp on a 2x output." --wait >/dev/null
capture "$OUT/hidpi.png" character agent_balloon
stop_daemon

echo "done"
