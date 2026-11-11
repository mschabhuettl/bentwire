# shellcheck shell=bash
# Sourced by scripts/screenshots.sh: a real KDE Plasma 6 Wayland session, headless.
#
# Outer compositor: sway with the headless backend and the pixman renderer (the VM has no
# render node). Inside it, kwin_wayland runs nested as a full-screen sway window
# (`--wayland-display`); KWin falls back to its QPainter compositor because the outer
# sway offers no dmabuf. plasmashell, kded6 (StatusNotifierWatcher) and kactivitymanagerd
# run on KWin's socket with the default Breeze look and the default wallpaper. Pictures
# are taken from the outer sway with grim, the virtual pointer (vptr example) is a sway
# pointer that sway forwards to the nested KWin.
# plasmashell and bentwired run under libfaketime with a frozen wall clock (the panel
# clock and the Office 97 daypart); timers use CLOCK_MONOTONIC, which stays real.
#
# Expects: T (work dir), BIN, W, H, PY, wait_for. Sets: SWAYD, SWAYSOCK, KWD, SCALE.

FAKETIME_LIB=${FAKETIME_LIB:-/usr/lib/x86_64-linux-gnu/faketime/libfaketimeMT.so.1}
FROZEN="2026-11-11 10:00:00"
KAMD=${KAMD:-/usr/lib/x86_64-linux-gnu/libexec/kactivitymanagerd}
SESSION_PIDS=()
KWD=kw0

frozen() { LD_PRELOAD=$FAKETIME_LIB FAKETIME="$FROZEN" FAKETIME_DONT_FAKE_MONOTONIC=1 "$@"; }
sgrim() { WAYLAND_DISPLAY=$SWAYD grim "$@"; }

desktop_settled() { # three identical full grabs in a row, 0.3 s apart, not black
    sgrim -t ppm "$T/settle0.ppm" || return 1
    for _ in 1 2 3; do
        sleep 0.3
        sgrim -t ppm "$T/settle.ppm" || return 1
        cmp -s "$T/settle0.ppm" "$T/settle.ppm" || return 1
    done
    ! $PY pixel "$T/settle.ppm" 000000
}

start_session() { # start_session <scale>: outer sway, vptr, KWin, Plasma
    SCALE=$1
    # Every session process carries this tag, so stop_session also finds the children
    # that outlive their parents (kioworkers and the like).
    export BW_SESSION_TAG=$T/session-$SCALE-$RANDOM
    rm -rf "${XDG_CONFIG_HOME:?}" "${XDG_CACHE_HOME:?}" "${XDG_DATA_HOME:?}"
    mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"
    # Only what a headless run needs; everything else is Plasma's default.
    # The top-left hot corner (Overview) would fire when the pointer is parked there.
    printf '[Effect-overview]\nBorderActivate=9\n' >"$XDG_CONFIG_HOME/kwinrc"
    # No yearly donation notification popping up during a run.
    printf '[Module-donationmessage]\nautoload=false\n' >"$XDG_CONFIG_HOME/kded6rc"

    cat >"$T/sway.conf" <<EOF
output HEADLESS-1 resolution $((W * SCALE))x$((H * SCALE)) position 0 0 scale $SCALE
default_border none
for_window [title=".*"] fullscreen enable
seat * hide_cursor 0
EOF
    WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
        sway -c "$T/sway.conf" >"$T/sway.log" 2>&1 &
    SESSION_PIDS+=($!)
    wait_for 10 sh -c 'ls "$XDG_RUNTIME_DIR" | grep -Eq "^wayland-[0-9]+$"'
    # shellcheck disable=SC2010
    SWAYD=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | head -1)
    wait_for 10 sh -c 'ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock'
    SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock | head -1)
    export SWAYSOCK

    rm -f "$T/vptr"
    mkfifo "$T/vptr"
    WAYLAND_DISPLAY=$SWAYD "$BIN/examples/vptr" $W $H <"$T/vptr" >"$T/vptr.log" 2>&1 &
    SESSION_PIDS+=($!)
    exec 7>"$T/vptr"
    wait_for 10 sh -c 'swaymsg -t get_inputs | grep -q "\"type\": \"pointer\""'

    local kscale=()
    [ "$SCALE" != 1 ] && kscale=(--scale "$SCALE")
    WAYLAND_DISPLAY=$SWAYD kwin_wayland --wayland-display "$SWAYD" --socket $KWD \
        --width $W --height $H "${kscale[@]}" --no-lockscreen --no-global-shortcuts \
        >"$T/kwin.log" 2>&1 &
    SESSION_PIDS+=($!)
    wait_for 20 test -S "$XDG_RUNTIME_DIR/$KWD"
    wait_for 20 busctl --user status org.kde.KWin

    export WAYLAND_DISPLAY=$KWD QT_QPA_PLATFORM=wayland
    "$KAMD" >"$T/kamd.log" 2>&1 &
    SESSION_PIDS+=($!)
    kded6 >"$T/kded.log" 2>&1 &
    SESSION_PIDS+=($!)
    wait_for 20 busctl --user status org.kde.kded6
    wait_for 20 busctl --user call org.kde.kded6 /kded org.kde.kded6 loadModule s statusnotifierwatcher
    wait_for 20 busctl --user introspect org.kde.StatusNotifierWatcher /StatusNotifierWatcher
    LIBGL_ALWAYS_SOFTWARE=1 frozen plasmashell --no-respawn >"$T/plasma.log" 2>&1 &
    SESSION_PIDS+=($!)
    wait_for 30 busctl --user status org.kde.plasmashell
    vp "move $PARK"
    wait_for 60 desktop_settled
    sgrim -t ppm "$T/desktop.ppm"
    SESSION_TAG=$BW_SESSION_TAG
    unset BW_SESSION_TAG
}

stop_session() {
    local TAG=$SESSION_TAG
    exec 7>&-
    for p in "${SESSION_PIDS[@]}"; do kill "$p" 2>/dev/null || true; done
    for p in "${SESSION_PIDS[@]}"; do wait "$p" 2>/dev/null || true; done
    SESSION_PIDS=()
    local p
    for p in $(pgrep -u "$(id -u)"); do
        tr '\0' '\n' 2>/dev/null </proc/"$p"/environ | grep -qx "BW_SESSION_TAG=$TAG" \
            && kill "$p" 2>/dev/null
    done
    return 0
}
