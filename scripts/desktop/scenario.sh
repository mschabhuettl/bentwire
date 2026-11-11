#!/bin/bash
# Desktop scenario: runs bentwired on a headless compositor and
# drives show, play, say, ask (answered through org.bentwire.Test1), moveto and hide,
# checking D-Bus signals, the backend snapshot and the screen.
#
# usage: scripts/desktop/scenario.sh sway|kwin [BIN_DIR]
#   sway: WLR_BACKENDS=headless sway, screenshots with grim (pixels are checked).
#   kwin: kwin_wayland --virtual; KWin offers no screenshot without an OpenGL render
#         node, so geometry is checked through KWin scripting (workspace.windowList).
# Needs: dbus-run-session, python3, and sway+grim or kwin_wayland + busctl. Build first:
#   cargo build -p bentwired -p bentwire && cargo build -p bentwired --examples
# (the `vptr` example drives a real pointer under sway).
# Exit status 0 = all checks passed. Artefacts (logs, screenshots, signal log) are kept
# in the printed directory on failure or with BENTWIRE_SCENARIO_KEEP=1.
set -euo pipefail

COMP=${1:?usage: scenario.sh sway|kwin [BIN_DIR]}
REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN=${2:-$REPO/target/debug}
FIXTURES=${BENTWIRE_SCENARIO_CHARACTERS:-$REPO/crates/bentwired/tests/fixtures/characters}

if [ -z "${BENTWIRE_SCENARIO_INNER:-}" ]; then
    T=$(mktemp -d)
    echo "scenario $COMP: artefacts in $T"
    mkdir -m 0700 "$T/run"
    export XDG_RUNTIME_DIR=$T/run HOME=$T/home XDG_CONFIG_HOME=$T/home/cfg
    export XDG_STATE_HOME=$T/home/state XDG_DATA_HOME=$T/home/data XDG_CACHE_HOME=$T/home/cache
    export XDG_DATA_DIRS=$T/nonexistent BENTWIRE_SCENARIO_INNER=1 SCEN_DIR=$T
    mkdir -p "$HOME"
    rc=0
    dbus-run-session -- "$0" "$@" || rc=$?
    if [ "$rc" -eq 0 ] && [ -z "${BENTWIRE_SCENARIO_KEEP:-}" ]; then
        rm -rf "${T:?}"
    fi
    exit "$rc"
fi

T=$SCEN_DIR
PIDS=()
cleanup() {
    for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done
    wait 2>/dev/null || true
}
trap cleanup EXIT
trap 'echo "scenario aborted at line $LINENO: $BASH_COMMAND"' ERR

FAILS=0
check() { # check <description> <command...>
    local d=$1
    shift
    if "$@"; then echo "  ok   $d"; else echo "  FAIL $d"; FAILS=$((FAILS + 1)); fi
}

wait_for() { # wait_for <seconds> <command...>
    local n=$(($1 * 10))
    shift
    for _ in $(seq "$n"); do "$@" >/dev/null 2>&1 && return 0; sleep 0.1; done
    return 1
}

W=1280
H=720
case "$COMP" in
sway)
    export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
    printf 'output HEADLESS-1 resolution %dx%d position 0 0\n' $W $H >"$T/sway.conf"
    sway -c "$T/sway.conf" >"$T/compositor.log" 2>&1 &
    PIDS+=($!)
    wait_for 10 sh -c 'ls "$XDG_RUNTIME_DIR" | grep -Eq "^wayland-[0-9]+$"' || { echo "sway did not start"; exit 1; }
    WAYLAND_DISPLAY=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | head -1)
    ;;
kwin)
    export XDG_CONFIG_HOME=$T/home/cfg QT_LOGGING_RULES="kwin_scripting.debug=true;js.debug=true"
    mkdir -p "$XDG_CONFIG_HOME"
    KWIN_COMPOSE=Q kwin_wayland --virtual --socket kw0 --width $W --height $H \
        --no-lockscreen --no-global-shortcuts >"$T/compositor.log" 2>&1 &
    PIDS+=($!)
    wait_for 15 test -S "$XDG_RUNTIME_DIR/kw0" || { echo "kwin did not start"; exit 1; }
    wait_for 15 busctl --user status org.kde.KWin || { echo "KWin D-Bus not ready"; exit 1; }
    WAYLAND_DISPLAY=kw0
    ;;
*)
    echo "unknown compositor $COMP"
    exit 1
    ;;
esac
export WAYLAND_DISPLAY

# Tray (B-TRAY-1): a minimal StatusNotifierWatcher stands in for the panel.
TRAY=0
if python3 -I -c 'import gi' 2>/dev/null; then
    python3 -I -W ignore "$REPO/scripts/desktop/sni-watcher.py" >"$T/sni.log" 2>"$T/sni.err" &
    PIDS+=($!)
    wait_for 5 busctl --user status org.kde.StatusNotifierWatcher && TRAY=1
fi
TRAYARG=--no-tray
[ "$TRAY" = 1 ] && TRAYARG=
# One virtual pointer for the whole run (sway): a new device per command would make the
# seat's pointer capability come and go.
if [ "$COMP" = sway ] && [ -x "$BIN/examples/vptr" ]; then
    mkfifo "$T/vptr"
    "$BIN/examples/vptr" $W $H <"$T/vptr" >"$T/vptr.log" 2>&1 &
    PIDS+=($!)
    exec 7>"$T/vptr"
    sleep 0.5
fi
dbus-monitor --session "type='signal',interface='org.bentwire.Character1'" >"$T/signals.log" 2>&1 &
PIDS+=($!)
"$BIN/bentwired" --test-interface $TRAYARG --no-audio --characters "$FIXTURES" >"$T/bentwired.log" 2>&1 &
BW=$!
PIDS+=($BW)
wait_for 10 busctl --user status org.bentwire.Agent || { echo "bentwired did not start"; cat "$T/bentwired.log"; exit 1; }

TEST="busctl --user call org.bentwire.Agent /org/bentwire/Test org.bentwire.Test1"
# Snapshot JSON (busctl --json wraps the string reply).
snapshot_json() { busctl --user --json=short call org.bentwire.Agent /org/bentwire/Test org.bentwire.Test1 Snapshot \
    | python3 -I -c 'import sys, json; print(json.load(sys.stdin)["data"][0])'; }
snap() { # snap <python expression over s>
    snapshot_json | python3 -I -c '
import sys, json
s = json.load(sys.stdin)
sys.exit(0 if eval(sys.argv[1]) else 1)' "$1"
}
mapped() { snap 's["backend"]["character"] is not None and s["backend"]["character"]["buffer_attached"] and s["backend"]["character"]["configured"]'; }

SHOT=0
shot() { # shot <name>: screenshot (sway) or geometry dump (kwin) into $T
    SHOT=$((SHOT + 1))
    case "$COMP" in
    sway) grim -t ppm "$T/$SHOT-$1.ppm" ;;
    kwin)
        cat >"$T/dump.js" <<'EOF'
var l = workspace.windowList();
for (var i = 0; i < l.length; i++) { var w = l[i], g = w.frameGeometry;
  print("BWGEOM " + w.resourceClass + " " + g.x + " " + g.y + " " + g.width + " " + g.height + " " + w.hidden); }
print("BWGEOM end");
EOF
        local before
        before=$(grep -c 'BWGEOM end' "$T/compositor.log" || true)
        local id
        id=$(busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting loadScript ss "$T/dump.js" "bw$SHOT" | awk '{print $2}')
        busctl --user call org.kde.KWin "/Scripting/Script$id" org.kde.kwin.Script run >/dev/null
        wait_for 5 sh -c "[ \$(grep -c 'BWGEOM end' '$T/compositor.log') -gt $before ]" || true
        busctl --user call org.kde.KWin /Scripting org.kde.kwin.Scripting unloadScript s "bw$SHOT" >/dev/null || true
        awk -v n="$before" '/BWGEOM end/{c++; next} /BWGEOM/ && c==n {sub(/.*BWGEOM /,""); print}' "$T/compositor.log" >"$T/$SHOT-$1.geom"
        ;;
    esac
}

# Character pixels: the fixture's frames are opaque squares inset 4 px in a 32 px cell.
# shown_at <x> <y>: the character's square is on screen at top-left (x, y).
shown_at() {
    case "$COMP" in
    sway) python3 -I "$REPO/scripts/desktop/bbox.py" "$T/$SHOT-"*.ppm --expect "$(( $1 + 4 )),$(( $2 + 4 )),$(( $1 + 28 )),$(( $2 + 28 ))" ;;
    kwin) grep -Eq "^bentwired $1 $2 32 32 false$" "$T/$SHOT-"*.geom ;;
    esac
}
absent() {
    case "$COMP" in
    sway) python3 -I "$REPO/scripts/desktop/bbox.py" "$T/$SHOT-"*.ppm --expect none ;;
    kwin) ! grep -Eq '^bentwired ' "$T/$SHOT-"*.geom ;;
    esac
}
balloon_on_screen() { # balloon_on_screen agent|office
    case "$COMP" in
    sway) python3 -I "$REPO/scripts/desktop/bbox.py" "$T/$SHOT-"*.ppm --balloon "$1" ;;
    # KWin lists no subsurfaces in windowList and cannot take screenshots here, so this
    # is the backend's own record of the attached subsurface buffer.
    kwin) snap "any(b['buffer_attached'] and b['office'] == ('$1' == 'office') for b in s['backend']['balloons'])" ;;
    esac
}
status() { "$BIN/bentwire" status | sed -n "s/^$1=//p"; }

echo "1. startup and show"
check "show --wait succeeds" "$BIN/bentwire" show --wait
check "surface configured and buffer attached" wait_for 5 mapped
X=$(status left)
Y=$(status top)
check "first-run position (B-CFG-3)" test "$X,$Y" = "$((W - 48 - 32)),$((H - 96 - 32))"
check "layer surface keyboard_interactivity none" snap 's["backend"]["character"]["keyboard_interactivity"] == "none"'
sleep 0.5
shot show
check "character on screen at ($X,$Y)" shown_at "$X" "$Y"

echo "1a. real pointer (sway: zwlr_virtual_pointer_v1)"
if [ "$COMP" = sway ] && [ -x "$BIN/examples/vptr" ]; then
    vp() { printf '%s\n' "$@" >&7; sleep 0.3; }
    CX=$((X + 16)) CY=$((Y + 16))
    vp "move $CX $CY" "press 1" "release 1"
    check "click on an opaque pixel gives Click($CX, $CY) (B-UI-1)" wait_for 5 python3 -I -c '
import re, sys
log = open(sys.argv[1]).read()
sys.exit(0 if re.search(r"member=Click\n\s*uint32 1\n\s*uint32 0\n\s*int32 " + sys.argv[2] + r"\n\s*int32 " + sys.argv[3] + r"\n\s*boolean false", log) else 1)' "$T/signals.log" "$CX" "$CY"
    N=$(grep -c 'member=Click' "$T/signals.log")
    sleep 0.6 # past double_click_ms
    vp "move $((X + 1)) $((Y + 1))" "press 1" "release 1"
    sleep 0.5
    check "click on a transparent pixel goes through (B-WL-6, K5)" test "$(grep -c 'member=Click' "$T/signals.log")" = "$N"
    sleep 0.6
    STEPS=("move $CX $CY" "press 1")
    for i in 1 2 3 4 5 6 7 8 9 10; do STEPS+=("move $((CX - 5 * i)) $((CY - 3 * i))" "sleep 30"); done
    STEPS+=("release 1")
    vp "${STEPS[@]}"
    check "drag of (-50, -30) moves the character 1:1 (R-UI-2, B-WL-7, K6)" wait_for 5 sh -c "\"$BIN/bentwire\" status | grep -qx 'left=$((X - 50))' && \"$BIN/bentwire\" status | grep -qx 'top=$((Y - 30))'"
    "$BIN/bentwire" moveto "$X" "$Y" --speed 0 --wait >/dev/null
    sleep 0.6
    vp "move $CX $CY" "press 2" "release 2"
    check "right click opens the menu with a grab (B-WL-9)" wait_for 5 snap 's["backend"]["menu"] is not None and s["backend"]["menu"]["buffer_attached"]'
    vp "move 20 20" "press 1" "release 1"
    check "outside click closes it via popup_done (K3)" wait_for 5 snap 's["backend"]["menu"] is None and s["menu"] is None'
else
    echo "  NOT RUN real pointer checks (sway only; build with: cargo build -p bentwired --examples)"
fi

echo "1b. context menu (xdg_popup on the layer surface)"
$TEST Pointer suii press 2 "$((X + 16))" "$((Y + 16))" >/dev/null
$TEST Pointer suii release 2 "$((X + 16))" "$((Y + 16))" >/dev/null
check "menu popup configured with a buffer" wait_for 5 snap 's["backend"]["menu"] is not None and s["backend"]["menu"]["configured"] and s["backend"]["menu"]["buffer_attached"]'
sleep 0.3
shot menu
if [ "$COMP" = sway ]; then
    check "menu on screen" python3 -I "$REPO/scripts/desktop/bbox.py" "$T/$SHOT-menu.ppm" --menu x
fi
$TEST Key s Escape >/dev/null
check "Esc closes the menu" wait_for 5 snap 's["backend"]["menu"] is None'

echo "2. play"
check "play Wave --wait" "$BIN/bentwire" play Wave --wait

echo "3. say"
"$BIN/bentwire" say "It looks like your build failed." --wait >"$T/say.out" 2>&1 &
SAY=$!
check "agent balloon shown" wait_for 5 snap 'any(not b["office"] and b["buffer_attached"] for b in s["backend"]["balloons"])'
sleep 0.3
shot say
check "agent balloon on screen" balloon_on_screen agent
check "say completes" wait "$SAY"

echo "4. ask, answered through Test1"
"$BIN/bentwire" ask "Build failed." --button "Show log" --button "Ignore" >"$T/ask.out" 2>&1 &
ASK=$!
check "office balloon shown" wait_for 10 snap 's["office_balloon"] is not None and any(b["office"] and b["buffer_attached"] for b in s["backend"]["balloons"])'
check "keyboard on_demand while ask is open (B-WL-8)" snap 's["backend"]["character"]["keyboard_interactivity"] == "on_demand"'
sleep 0.3
shot ask
check "office balloon on screen" balloon_on_screen office
read -r BX BY < <(snapshot_json | python3 -I -c '
import sys, json
s = json.load(sys.stdin)
r = s["office_balloon"]["buttons"][1]["rect"]
print(r["x"] + r["w"] // 2, r["y"] + r["h"] // 2)')
$TEST Pointer suii press 1 "$BX" "$BY" >/dev/null
$TEST Pointer suii release 1 "$BX" "$BY" >/dev/null
check "ask completes" wait "$ASK"
check "ask prints button:2" grep -qx 'button:2' "$T/ask.out"
check "keyboard back to none" wait_for 5 snap 's["backend"]["character"]["keyboard_interactivity"] == "none"'

echo "5. moveto"
check "moveto fully off-screen" "$BIN/bentwire" moveto -100 -100 --speed 0 --wait
check "margins follow off-screen (B-WL-4, no clamping)" wait_for 5 snap 's["backend"]["character"]["margins_requested"] == {"left": -100, "top": -100}'
check "moveto 200 150 --wait" "$BIN/bentwire" moveto 200 150 --speed 300 --wait
check "status position 200,150" test "$(status left),$(status top)" = "200,150"
check "margins requested 200,150" wait_for 5 snap 's["backend"]["character"]["margins_requested"] == {"left": 200, "top": 150}'
sleep 0.5
shot moveto
check "character on screen at (200,150)" shown_at 200 150

echo "5b. tray (StatusNotifierItem + dbusmenu)"
if [ "$TRAY" = 1 ]; then
    check "item registered with the watcher" wait_for 5 test -s "$T/sni.log"
    read -r SNI SNIPATH <"$T/sni.log" || true
    prop() { busctl --user --json=short get-property "$SNI" "$SNIPATH" org.kde.StatusNotifierItem "$1" | python3 -I -c 'import sys, json; print(json.load(sys.stdin)["data"])'; }
    check "Id is bentwire" test "$(prop Id)" = bentwire
    check "Title is the character name" test "$(prop Title)" = Pebble
    MENU=$(prop Menu || true)
    check "dbusmenu has _Hide, _Options..., _Choose Assistant..." sh -c "busctl --user call '$SNI' '$MENU' com.canonical.dbusmenu GetLayout iias 0 10 0 \
        | grep -q '\"_Hide\"' && busctl --user call '$SNI' '$MENU' com.canonical.dbusmenu GetLayout iias 0 10 0 | grep -q '_Choose Assistant'"
    busctl --user call "$SNI" "$SNIPATH" org.kde.StatusNotifierItem Activate ii 7 9 >/dev/null
    check "Activate emits Click(1, 0, 7, 9, true)" wait_for 5 grep -q 'member=Click' "$T/signals.log"
else
    echo "  NOT RUN tray checks (python3-gi missing)"
fi

echo "5c. scale and output hot-plug (sway only: swaymsg changes outputs headless)"
if [ "$COMP" = sway ]; then
    export SWAYSOCK
    SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock | head -1)
    swaymsg -q output HEADLESS-1 scale 2
    check "buffer scale 2 on a scale-2 output (B-WL-6)" wait_for 5 snap 's["backend"]["character"]["buffer_scale"] == 2 and not s["backend"]["character"]["viewport"]'
    swaymsg -q output HEADLESS-1 scale 1.5
    check "viewporter on a scale-1.5 output (B-WL-6)" wait_for 5 snap 's["backend"]["character"]["viewport"] and s["backend"]["character"]["buffer_scale"] == 2'
    swaymsg -q output HEADLESS-1 scale 1
    check "back to buffer scale 1" wait_for 5 snap 's["backend"]["character"]["buffer_scale"] == 1'
    swaymsg -q output HEADLESS-1 transform 90
    check "rotated output keeps scale 1 (B-WL-6)" wait_for 5 snap "s['outputs'][0]['w'] == $H and s['outputs'][0]['scale'] == 1 and s['backend']['character']['buffer_scale'] == 1 and not s['backend']['character']['viewport']"
    swaymsg -q output HEADLESS-1 transform normal
    check "transform back to normal" wait_for 5 snap "s['outputs'][0]['w'] == $W and s['backend']['character']['buffer_scale'] == 1"
    swaymsg -q output HEADLESS-1 position 100 0
    check "margins follow a moved output origin (B-WL-2)" wait_for 5 snap 's["outputs"][0]["x"] == 100 and s["backend"]["character"]["margins_requested"] == {"left": s["left"] - 100, "top": s["top"]}'
    swaymsg -q output HEADLESS-1 position 0 0
    check "margins back with the origin" wait_for 5 snap 's["outputs"][0]["x"] == 0 and s["backend"]["character"]["margins_requested"] == {"left": s["left"], "top": s["top"]}'
    swaymsg -q create_output
    swaymsg -q output HEADLESS-2 resolution 640x480 position $W 0
    check "second output seen" wait_for 5 snap 'len(s["outputs"]) == 2'
    check "moveto onto the second output" "$BIN/bentwire" moveto $((W + 300)) 200 --speed 0 --wait
    check "surface recreated on the second output (B-WL-4)" wait_for 5 snap 's["backend"]["character"]["output"] != s["outputs"][0]["id"] and s["backend"]["character"]["buffer_attached"]'
    swaymsg -q output HEADLESS-2 unplug
    check "pulled back onto the first output (B-WL-4)" wait_for 5 snap 'len(s["outputs"]) == 1 and s["left"] + 32 <= s["outputs"][0]["w"] and s["backend"]["character"]["buffer_attached"]'
    check "Move cause 4 emitted" wait_for 5 grep -q 'int32 4' "$T/signals.log"
    "$BIN/bentwire" moveto 200 150 --speed 0 --wait >/dev/null
fi

echo "6. hide"
check "hide --wait" "$BIN/bentwire" hide --wait
check "surface unmapped" wait_for 5 snap 's["backend"]["character"] is None'
sleep 0.5
shot hide
check "character gone from screen" absent

echo "6b. tray menu click"
if [ "$TRAY" = 1 ]; then
    SHOWID=$(busctl --user --json=short call "$SNI" "$MENU" com.canonical.dbusmenu GetLayout iias 0 10 0 | python3 -I -c '
import json, sys
def walk(n):
    i, props, kids = n
    if props.get("label", {}).get("data") == "_Show":
        return i
    for k in kids:
        r = walk(k["data"])
        if r is not None:
            return r
print(walk(json.load(sys.stdin)["data"][1]))')
    check "hidden character: tray menu offers _Show" test "$SHOWID" != None
    busctl --user call "$SNI" "$MENU" com.canonical.dbusmenu Event isvu "$SHOWID" clicked s "" 0 >/dev/null
    check "tray Show shows the character with cause 2 (B-TRAY-1)" wait_for 5 python3 -I -c '
import re, sys
sys.exit(0 if re.search(r"member=Shown\n\s*uint32 2", open(sys.argv[1]).read()) else 1)' "$T/signals.log"
fi

echo "7. D-Bus signals"
"$BIN/bentwire" quit || true
wait_for 5 sh -c "! kill -0 $BW" || true
sleep 0.3
check "signal order" python3 -I "$REPO/scripts/desktop/signals.py" "$T/signals.log" \
    Shown RequestStart RequestComplete BalloonShown BalloonShown AskComplete RequestComplete \
    Move RequestComplete Hidden RequestComplete

if [ "$FAILS" -eq 0 ]; then
    echo "scenario $COMP: all checks passed"
else
    echo "scenario $COMP: $FAILS check(s) failed; see $T"
    exit 1
fi
