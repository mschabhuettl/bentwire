#!/bin/sh
# bentwire D-Bus example in POSIX sh with gdbus (GLib) and dbus-monitor (dbus), docs/dbus.md.
# Speaks, plays an animation, asks a question with two buttons and prints the answer,
# then prints Click, DblClick, Shown and Hidden until Ctrl+C.
set -eu

DEST=org.bentwire.Agent
OBJ=/org/bentwire/Character
IFACE=org.bentwire.Character1

# call METHOD ARGS...: a Character1 method; prints the reply, e.g. "(uint32 3,)".
# The first call D-Bus-activates bentwired if it is not running.
call() {
    m=$1
    shift
    gdbus call --session --dest "$DEST" --object-path "$OBJ" --method "$IFACE.$m" "$@"
}
# "(uint32 3,)" -> 3
request_id() { sed -n 's/^(uint32 \([0-9]*\),)$/\1/p'; }

call Show false >/dev/null
call Speak 'Hello from the shell!' >/dev/null
call Play Wave >/dev/null

# RequestComplete and AskComplete go only to the connection that made the call (B-Q-2),
# and `gdbus call` disconnects after the reply. A bus monitor (BecomeMonitor) sees them
# anyway. It also sees the broadcast signals.
dir=$(mktemp -d) mon=''
trap 'kill "$mon" 2>/dev/null; rm -r "$dir"' EXIT
trap 'exit 0' INT TERM
mkfifo "$dir/signals"
dbus-monitor --session --monitor "type='signal',path='$OBJ',interface='$IFACE'" >"$dir/signals" &
mon=$!
exec 3<"$dir/signals"
# The monitor is ready once the bus has taken its unique name away (NameLost).
while read -r line <&3; do
    case $line in *member=NameLost*) break ;; esac
done

ask=$(call Ask "{'heading': <'Shell example'>, 'text': <'Coffee or tea?'>,
                 'buttons': <['&Coffee', '&Tea']>}" | request_id)

# dbus-monitor prints a header line per message, then one line per argument.
member='' args=''
# shellcheck disable=SC2086 # $args is split into the signal's arguments on purpose
show() {
    case $member in
    AskComplete)
        set -- $args
        if [ "$1" = "$ask" ]; then
            case $2 in
            0) echo 'answer: none (dismissed or interrupted)' ;;
            -25) echo 'answer: Coffee (button 1)' ;;
            -26) echo 'answer: Tea (button 2)' ;;
            *) echo "answer: result $2" ;;
            esac
        fi
        ;;
    RequestComplete)
        set -- $args
        if [ "$1" = "$ask" ]; then
            [ "$2" = 0 ] || echo "ask ended with status $2 $3"
            echo 'Listening for Click, DblClick, Shown, Hidden (Ctrl+C to quit)'
        fi
        ;;
    Click | DblClick | Shown | Hidden) echo "$member$args" ;;
    esac
}
while read -r line <&3; do
    case $line in
    signal\ *) member=${line##*member=} args='' ;;
    uint32\ * | int32\ * | boolean\ * | string\ *) args="$args ${line#* }" ;;
    esac
    # Act on the last argument line of each signal used here.
    case $member:$line in
    AskComplete:\] | RequestComplete:string\ * | Click:boolean\ * | DblClick:boolean\ * | \
        Shown:uint32\ * | Hidden:uint32\ *) show ;;
    esac
done
