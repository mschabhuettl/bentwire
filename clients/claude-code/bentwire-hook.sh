#!/bin/bash
# Claude Code hook: lets the bentwire character show what Claude Code is doing.
#
# usage (from a hook command): bentwire-hook.sh thinking|writing|attention|done
#   thinking   UserPromptSubmit         play Thinking
#   writing    PreToolUse (Edit, Write) play Writing
#   attention  Notification             play GetAttention and say the notification text
#                                       ("Claude needs your permission to use Bash");
#                                       only for permission_prompt, idle_prompt,
#                                       agent_needs_input or an untyped notification
#   done       Stop                     play Congratulate
# The hook input (JSON on stdin) is read; the hook prints nothing, always exits 0 and
# returns at once: the `bentwire` CLI runs detached with a short timeout. Nothing is sent
# when bentwired is not running (no D-Bus activation).
#
# Environment: BENTWIRE_HOOKS_OFF=1 disables it; BENTWIRE_CLI names the CLI (default
# `bentwire`).

event=${1:-}
input=$(cat)
[ "${BENTWIRE_HOOKS_OFF:-}" = 1 ] && exit 0

# The value of a top-level string field of the hook input.
field() {
    if command -v jq >/dev/null 2>&1; then
        printf '%s' "$input" | jq -r --arg k "$1" '.[$k] // empty | strings' 2>/dev/null
    elif command -v python3 >/dev/null 2>&1; then
        printf '%s' "$input" | python3 -I -c 'import json,sys
try:
    v = json.load(sys.stdin).get(sys.argv[1])
except Exception:
    v = None
print(v if isinstance(v, str) else "")' "$1" 2>/dev/null
    fi
}

running() {
    if command -v busctl >/dev/null 2>&1; then
        busctl --user --no-pager status org.bentwire.Agent >/dev/null 2>&1
    elif command -v dbus-send >/dev/null 2>&1; then
        dbus-send --session --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus \
            org.freedesktop.DBus.NameHasOwner string:org.bentwire.Agent 2>/dev/null | grep -q true
    else
        return 0
    fi
}

cli() {
    if command -v timeout >/dev/null 2>&1; then
        timeout 3 "${BENTWIRE_CLI:-bentwire}" "$@"
    else
        "${BENTWIRE_CLI:-bentwire}" "$@"
    fi
}

case $event in
    thinking) calls=("stop --all" "play Thinking") ;;
    writing) calls=("stop --all" "play Writing") ;;
    attention)
        # only notifications that want the user (others: auth_success, agent_completed)
        case $(field notification_type) in
            "" | permission_prompt | idle_prompt | agent_needs_input) ;;
            *) exit 0 ;;
        esac
        msg=$(field message)
        calls=("stop --all" "play GetAttention")
        ;;
    done) calls=("stop --all" "play Congratulate") ;;
    *) exit 0 ;;
esac

run() {
    running || return 0
    local c
    for c in "${calls[@]}"; do
        # shellcheck disable=SC2086 # fixed words, split on purpose
        cli $c
    done
    [ -n "${msg:-}" ] && cli say "$msg"
    return 0
}

# Events must reach the daemon in order although each hook runs detached: every event
# takes a sequence number now, and the detached part runs under a lock and drops itself
# when a newer event is already waiting (the newest event wins).
state=${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}/bentwire-claude-hook-$(id -u)
ordered() {
    local n=$1
    exec 9>"$state.run"
    flock -w 15 9 || return 0
    [ "$(cat "$state.seq" 2>/dev/null)" = "$n" ] || return 0
    run
}
if command -v flock >/dev/null 2>&1; then
    n=$(
        exec 8>"$state.lock"
        flock -w 2 8
        n=$(($(cat "$state.seq" 2>/dev/null || echo 0) + 1))
        echo "$n" >"$state.seq"
        echo "$n"
    )
    ( ordered "$n" </dev/null >/dev/null 2>&1 & )
else
    ( run </dev/null >/dev/null 2>&1 & )
fi
exit 0
