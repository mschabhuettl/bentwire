#!/bin/bash
# Reproducible screenshots and GIFs on a real KDE Plasma 6 desktop, headless: KWin nested
# in headless sway, plasmashell with the default Breeze look and wallpaper, the bentwire
# tray icon in the panel (scripts/screenshots/session.sh has the details). Fixed
# 1280x720 output, seed 1, frozen wall clock, fresh XDG directories on a private session
# bus. No timing guesses for stills: every shot waits for the request to complete
# (--wait) or for the surface to appear in Test1.Snapshot, then grabs until three
# consecutive frames agree and differ from the frame before the action.
#
# usage: scripts/screenshots.sh [--classics DIR] [--gifs | --all] [BIN_DIR]
#   without --classics: only characters/builtin; writes to docs/screenshots/.
#   --classics DIR: the classics pack in DIR; writes to DIR/screenshots/.
#   default PNGs only, --gifs GIFs only (also scripts/gifs.sh), --all both.
# Build first: cargo build -p bentwired -p bentwire && cargo build -p bentwired --examples
# Needs: sway, grim, wf-recorder, ffmpeg, kwin-wayland, plasma-desktop, plasma-workspace,
# breeze, breeze-icon-theme, fonts-noto-core, kded6, faketime, dbus-run-session, busctl,
# python3 with PIL and numpy; and the apps behind the default panel launchers (systemsettings,
# plasma-discover, dolphin, falkon as the web browser), else they show other icons.
# The config pins balloon_font = "Tahoma" (bundled), so the Agent balloon does not depend
# on the host's fontconfig (B-BAL-2).
# GIFs are recorded at output scale 2 around the character (GFRAME) in real time
# (wf-recorder at the 120 Hz headless output; the fake clock exists only in --headless),
# then identical frames are merged, states shorter than 35 ms dropped, frame times rounded
# to 50 ms and capped at 1 s. Two runs mostly give byte-identical GIFs; otherwise the
# frames are the same and single frame times differ by 50 ms, or one run keeps an extra
# short frame (around the right click in menu.gif, between characters in tour.gif).
set -euo pipefail

REPO=$(cd "$(dirname "$0")/.." && pwd)
CLASSICS=
if [ "${1:-}" = --classics ]; then
    CLASSICS=$(cd "${2:?--classics needs a directory}" && pwd)
    shift 2
fi
MODE=shots
case "${1:-}" in
--gifs) MODE=gifs; shift ;;
--all) MODE=all; shift ;;
esac
BIN=$(cd "${1:-${CARGO_TARGET_DIR:-$REPO/target}/debug}" && pwd)

if [ -z "${BENTWIRE_SHOTS_INNER:-}" ]; then
    T=$(mktemp -d)
    echo "screenshots: work dir $T"
    mkdir -m 0700 "$T/run"
    # Nothing from the caller's environment (BENTWIRE_*, XDG dirs, locale) reaches the run.
    for v in $(env | sed -n 's/^\(BENTWIRE_[A-Z_]*\)=.*/\1/p'); do unset "$v"; done
    unset WAYLAND_DISPLAY DISPLAY SWAYSOCK XDG_CONFIG_DIRS XDG_SESSION_TYPE XDG_CURRENT_DESKTOP
    export XDG_RUNTIME_DIR=$T/run HOME=$T/home XDG_CONFIG_HOME=$T/home/cfg
    export XDG_STATE_HOME=$T/home/state XDG_DATA_HOME=$T/home/data XDG_CACHE_HOME=$T/home/cache
    export XDG_DATA_DIRS=/usr/local/share:/usr/share XDG_MENU_PREFIX=plasma- XDG_CURRENT_DESKTOP=KDE LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC
    export BENTWIRE_SHOTS_INNER=1 SHOTS_DIR=$T
    mkdir -p "$HOME"
    rc=0
    args=("$BIN")
    [ "$MODE" != shots ] && args=("--$MODE" "$BIN")
    [ -n "$CLASSICS" ] && args=(--classics "$CLASSICS" "${args[@]}")
    dbus-run-session -- "$0" "${args[@]}" || rc=$?
    # anything the run left behind (also after an abort) still has our runtime dir
    for p in $(pgrep -u "$(id -u)"); do
        [ "$p" = $$ ] && continue
        tr '\0' '\n' 2>/dev/null </proc/"$p"/environ | grep -qx "XDG_RUNTIME_DIR=$T/run" \
            && kill "$p" 2>/dev/null
    done
    [ "$rc" -eq 0 ] && rm -rf "${T:?}"
    exit "$rc"
fi

T=$SHOTS_DIR
PY="python3 -I $REPO/scripts/screenshots/shot.py"
W=1280 H=720
# Character position, the frame of the cropped shots (bottom right of the desktop, with
# the panel's system tray and clock) and where the pointer rests.
X=1010 Y=470
FRAME="480 270 800 450"
# GIFs: a tighter frame around the character, its balloon or menu and the tray, recorded
# at output scale 2 (1280x720 pixels, nothing upscaled), so the character fills about a
# third of the height.
GFRAME="640 360 640 360"
PARK="300 200"
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
    stop_session 2>/dev/null || true
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
vp() { printf '%s\n' "$@" >&7; }

# shellcheck source=scripts/screenshots/session.sh
. "$REPO/scripts/screenshots/session.sh"

TEST="busctl --user call org.bentwire.Agent /org/bentwire/Test org.bentwire.Test1"
snapshot() { # snapshot FILE
    busctl --user --json=short call org.bentwire.Agent /org/bentwire/Test org.bentwire.Test1 Snapshot \
        | python3 -I -c 'import sys, json; print(json.load(sys.stdin)["data"][0])' >"$1"
}
has() { snapshot "$T/s.json" && $PY has "$T/s.json" "$@"; }
no_office() { ! has office_balloon; }
no_menu() { ! has menu; }
no_agent() { ! has agent_balloon; }
no_surfaces() { snapshot "$T/s.json" && python3 -I -c 'import json, sys; sys.exit(len(json.load(open(sys.argv[1]))["surfaces"]) != 0)' "$T/s.json"; }
scale2() { snapshot "$T/s.json" && python3 -I -c 'import json, sys; sys.exit(json.load(open(sys.argv[1]))["backend"]["character"]["buffer_scale"] != 2)' "$T/s.json"; }
tray_item() { busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher \
    org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems | grep -q '"'; }
no_tray_item() { ! tray_item; }

BW=
start_daemon() { # start_daemon <character> <language> [visible] [x y]: fresh config and state
    rm -rf "${T:?}/home/bwcfg" "${XDG_STATE_HOME:?}"
    mkdir -p "$T/home/bwcfg/bentwire" "$XDG_STATE_HOME/bentwire"
    cat >"$T/home/bwcfg/bentwire/config.toml" <<EOF
character = "$1"
language = "$2"
idle = false
sound_effects = false
balloon_font = "Tahoma"
EOF
    # Always start hidden: with visible = true the daemon starts the Show itself and
    # `show --wait` would return before it ends.
    printf 'character = "%s"\nleft = %d\ntop = %d\nvisible = false\n' "$1" "${4:-$X}" "${5:-$Y}" \
        >"$XDG_STATE_HOME/bentwire/state.toml"
    XDG_CONFIG_HOME=$T/home/bwcfg frozen "$BIN/bentwired" --seed 1 --test-interface --no-audio \
        "${ROOTS[@]}" >>"$T/bentwired.log" 2>&1 &
    BW=$!
    wait_for 10 busctl --user status org.bentwire.Agent
    test "$("$BIN/bentwire" status | sed -n 's/^character=//p')" = "$1" \
        || { echo "character $1 not selected"; exit 1; }
    wait_for 20 tray_item
    if [ "${3:-true}" = true ]; then
        "$BIN/bentwire" show --wait >/dev/null
        wait_for 10 has character
    fi
    vp "move $PARK"
    wait_for 30 desktop_settled
}
stop_daemon() {
    "$BIN/bentwire" quit >/dev/null || true
    wait "$BW" 2>/dev/null || true
    wait_for 20 no_tray_item
}

capture() { # capture <out.png> <x y w h> <surface kind...>: three agreeing grabs of the
    # rect, with an unchanged Snapshot around each, that differ from the bare desktop
    local out=$1 rect=$2 x y w h prev="" same=0 i
    shift 2
    read -r x y w h <<<"$rect"
    # shellcheck disable=SC2086
    $PY crop "$T/desktop.ppm" "$T/ref.png" "$SCALE" $rect
    for i in $(seq 300); do
        $TEST Snapshot >"$T/a.txt"
        sgrim -g "$x,$y ${w}x$h" -t ppm "$T/c$i.ppm"
        $TEST Snapshot >"$T/b.txt"
        if cmp -s "$T/a.txt" "$T/b.txt" && [ -n "$prev" ] && cmp -s "$prev" "$T/c$i.ppm"; then
            same=$((same + 1))
        else
            same=0
        fi
        prev=$T/c$i.ppm
        if [ "$same" -ge 2 ]; then
            snapshot "$T/a.json"
            $PY crop "$prev" "$T/cand.png" 1 0 0 $((w * SCALE)) $((h * SCALE))
            # shellcheck disable=SC2086
            if $PY only "$T/a.json" "$@" && $PY inside "$T/a.json" $rect "$@" \
                && ! cmp -s "$T/cand.png" "$T/ref.png"; then
                cp "$T/cand.png" "$out"
                echo "  $(basename "$out")"
                return 0
            fi
            same=0
        fi
    done
    echo "no stable frame for $out"
    return 1
}

shoot_set() { # shoot_set <character>: idle (whole desktop), say, ask
    local c=$1
    start_daemon "$c" en
    capture "$OUT/idle.png" "0 0 $W $H" character
    "$BIN/bentwire" say "It looks like your build failed. Want me to show the log?" --wait >/dev/null
    capture "$OUT/say.png" "$FRAME" character agent_balloon
    wait_for 10 no_agent
    "$BIN/bentwire" ask "The build failed in 3 crates." --heading "Build failed" \
        --button "Show log" --button "Ignore" >"$T/ask.out" &
    local ask=$!
    wait_for 10 has office_balloon
    capture "$OUT/ask.png" "$FRAME" character office_balloon
    $TEST Key s Return >/dev/null
    wait "$ask" || true
    wait_for 10 no_office
    stop_daemon
}

right_click_character() { # a real right click on a drawn pixel of the character
    snapshot "$T/a.json"
    sgrim -t ppm "$T/g.ppm"
    # shellcheck disable=SC2046
    read -r px py < <($PY opaque "$T/g.ppm" "$T/desktop.ppm" "$SCALE" $($PY rect "$T/a.json" 0 character))
    vp "move $px $py"
    sleep 0.3
    vp "press 2" "release 2"
    wait_for 10 has menu
}

menu_shot() { # menu_shot <character> <language> <out.png>
    start_daemon "$1" "$2"
    right_click_character
    vp "move $PARK"
    capture "$3" "$FRAME" character menu
    $TEST Key s Escape >/dev/null
    wait_for 10 no_menu
    stop_daemon
}

# ---------------------------------------------------------------------------- GIFs
REC=
record_start() { # record_start <name> <x y w h>: lossless recording of that rect
    local x y w h
    read -r x y w h <<<"$2"
    REC=$1
    rm -rf "${T:?}/rec-$REC"
    mkdir -p "$T/rec-$REC"
    WAYLAND_DISPLAY=$SWAYD wf-recorder -D -g "$x,$y ${w}x$h" -c libx264rgb -p crf=0 \
        -p preset=ultrafast -f "$T/rec-$REC/rec.mkv" >"$T/rec-$REC/log" 2>&1 &
    RECPID=$!
    # the first frame is the scene before the action
    wait_for 10 grep -q 'Output #0' "$T/rec-$REC/log"
    sleep 0.5
}
record_stop() { # record_stop <out.gif> [first-hold-ms] [last-hold-ms]
    local d=$T/rec-$REC
    sleep 0.3
    kill -INT "$RECPID"
    wait "$RECPID" || true
    ffmpeg -v error -i "$d/rec.mkv" -fps_mode passthrough "$d/%05d.png"
    ffprobe -v error -select_streams v -show_entries frame=pts_time -of csv=p=0 "$d/rec.mkv" >"$d/pts"
    local n
    n=$($PY frames "$d" "$d/pts" "$d/keep" "${2:-500}" "${3:-1500}" 50)
    $PY gif "$d/keep" "$1" "$d/keep/durations"
    echo "  $(basename "$1") ($n frames, $(($(stat -c %s "$1") / 1024)) KiB)"
}

anim() { # anim <name...>: the first of these animations the character has
    local a
    for a in "$@"; do
        "$BIN/bentwire" animations | grep -qx "$a" && { echo "$a"; return 0; }
    done
    return 1
}

gif_appear() { # character appears and waves
    start_daemon "$1" en false
    record_start appear "$GFRAME"
    "$BIN/bentwire" show --wait >/dev/null
    "$BIN/bentwire" play "$(anim Wave Greet Greeting)" --wait >/dev/null
    record_stop "$OUT/appear.gif" 800 1500
    stop_daemon
}

gif_hero() { # gif_hero <character> <out.gif>: appears, waves and says hello
    start_daemon "$1" en false
    record_start hero "$GFRAME"
    "$BIN/bentwire" show --wait >/dev/null
    "$BIN/bentwire" play "$(anim Wave Greet Greeting)" --wait >/dev/null
    "$BIN/bentwire" say "Hi! I live on your Plasma desktop." --wait >/dev/null
    record_stop "$2" 800 2500
    stop_daemon
}

gif_say() {
    start_daemon "$1" en
    record_start say "$GFRAME"
    "$BIN/bentwire" say "It looks like your build failed. Want me to show the log?" --wait >/dev/null
    record_stop "$OUT/say.gif" 500 2500
    stop_daemon
}

gif_ask() { # Office balloon, a button clicked with the virtual pointer
    start_daemon "$1" en
    record_start ask "$GFRAME"
    "$BIN/bentwire" ask "The build failed in 3 crates." --heading "Build failed" \
        --button "Show log" --button "Ignore" >"$T/ask.out" &
    local ask=$!
    wait_for 10 has office_balloon
    wait_for 10 desktop_settled
    snapshot "$T/a.json"
    local bx by i
    read -r bx by < <($PY button "$T/a.json" "Show log")
    # few, slow pointer steps: each state lasts long enough to be recorded every time
    for i in 0 1 2 3; do vp "move $((bx - 60 + i * 20)) $((by + 60 - i * 20))"; sleep 0.3; done
    vp "press 1"
    sleep 0.25
    vp "release 1"
    wait_for 10 no_office
    wait "$ask" || true
    grep -qx "button:1" "$T/ask.out" || { echo "ask answered: $(cat "$T/ask.out")"; exit 1; }
    vp "move $PARK"
    record_stop "$OUT/ask.gif" 500 1500
    stop_daemon
}

gif_menu() {
    start_daemon "$1" en
    record_start menu "$GFRAME"
    right_click_character
    snapshot "$T/a.json"
    # hover down the first few items, then dismiss
    python3 -I -c '
import json, sys
m = json.load(open(sys.argv[1]))["menu"]
for e in [e for e in m if "label" in e][:4]:
    print(e["x"] + 30, e["y"] + e["h"] // 2)' "$T/a.json" >"$T/items"
    while read -r ix iy; do vp "move $ix $iy"; sleep 0.45; done <"$T/items"
    sleep 0.3
    $TEST Key s Escape >/dev/null
    vp "move $PARK"
    sleep 0.3
    record_stop "$OUT/menu.gif" 500 800
    stop_daemon
}

gif_drag() { # press on the character and drag it across the frame with the pointer
    start_daemon "$1" en
    snapshot "$T/a.json"
    sgrim -t ppm "$T/g.ppm"
    # shellcheck disable=SC2046
    read -r px py < <($PY opaque "$T/g.ppm" "$T/desktop.ppm" "$SCALE" $($PY rect "$T/a.json" 0 character))
    record_start drag "$GFRAME"
    vp "move $px $py"
    sleep 0.3
    vp "press 1"
    local i nx ny
    # each step stays on screen for several recorder frames (see "frames" in shot.py)
    for i in $(seq 1 30); do
        nx=$((px - i * 10)) ny=$((py - (i * (30 - i)) / 3))
        vp "move $nx $ny"
        sleep 0.08
    done
    vp "release 1"
    sleep 0.5
    snapshot "$T/a.json"
    python3 -I -c 'import json, sys; s = json.load(open(sys.argv[1])); sys.exit(s["left"] > int(sys.argv[2]) - 250)' "$T/a.json" "$X" \
        || { echo "the drag did not move the character"; exit 1; }
    record_stop "$OUT/drag.gif" 500 1500
    stop_daemon
}

gif_tour() { # gif_tour <character...>: each appears, greets and leaves
    record_start tour "$GFRAME"
    for c in "$@"; do
        start_daemon "$c" en false
        "$BIN/bentwire" show --wait >/dev/null
        "$BIN/bentwire" play "$(anim Greet Wave Greeting)" --wait >/dev/null
        "$BIN/bentwire" hide --wait >/dev/null
        wait_for 10 no_surfaces
        stop_daemon
    done
    record_stop "$OUT/tour.gif" 500 500
}

# ---------------------------------------------------------------------------- run
echo "output into $OUT"
if [ "$MODE" != gifs ]; then
    start_session 1
    shoot_set "$MAIN"
    menu_shot "$MAIN" en "$OUT/menu-en.png"
    menu_shot "$MAIN" de "$OUT/menu-de.png"
    if [ -n "$CLASSICS" ]; then
        # Agent 2.0, Agent 1.5, Office 97 bitmap, Office 97 vector: one daemon each, each
        # in its own quarter of the same desktop; the strip takes quarter i from grab i.
        GRABS=()
        i=0
        for c in merlin merlin-15 genius-97 clippit-97; do
            start_daemon "$c" en true $((i * 320 + 100)) 480
            capture "$T/grab-$c.png" "0 0 $W $H" character
            GRABS+=("$T/grab-$c.png")
            stop_daemon
            i=$((i + 1))
        done
        $PY strip "$OUT/characters.png" 0 400 $W 320 "${GRABS[@]}"
        echo "  characters.png"
    fi
    stop_session

    echo "HiDPI (output scale 2)"
    start_session 2
    HI=$MAIN
    [ -n "$CLASSICS" ] && HI=clippit-97
    start_daemon "$HI" en
    wait_for 10 scale2
    "$BIN/bentwire" say "Sharp on a 2x output." --wait >/dev/null
    capture "$OUT/hidpi.png" "$FRAME" character agent_balloon
    stop_daemon
    stop_session
fi

if [ "$MODE" != shots ]; then
    echo "GIFs (output scale 2)"
    start_session 2
    gif_hero "$MAIN" "$OUT/hero.gif"
    gif_appear "$MAIN"
    gif_say "$MAIN"
    gif_ask "$MAIN"
    gif_menu "$MAIN"
    gif_drag "$MAIN"
    if [ -n "$CLASSICS" ]; then
        gif_tour clippit merlin genie peedy rover
    else
        mapfile -t ALL < <(find "$REPO/characters/builtin" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)
        [ "${#ALL[@]}" -gt 1 ] && gif_tour "${ALL[@]}"
    fi
    stop_session
fi

echo "done"
