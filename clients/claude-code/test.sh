#!/bin/bash
# Scripted test of the Claude Code hooks (BRIEF "Done when" 7).
#
# Starts `bentwired --headless` with characters/builtin under dbus-run-session and runs
# the hook commands from settings.snippet.json with the JSON input Claude Code sends for
# each event. Checks the CLI calls, that the daemon accepted and started them (request
# ids, RequestStart in the trace after Test1.Advance), that a hook prints nothing and
# returns at once even with a slow CLI, the opt-out, and silence without a daemon.
#
# usage: clients/claude-code/test.sh   (needs: cargo build -p bentwired -p bentwire; jq)
# Exit status 0 = all checks passed.
set -u
# shellcheck source=../testlib.sh
. "$(dirname "$0")/../testlib.sh"
bw_setup "$@"
command -v jq >/dev/null || { echo "jq not found"; exit 1; }
HERE=$BW_REPO/clients/claude-code
HOOK=$HERE/bentwire-hook.sh
SESSION='"session_id":"test","transcript_path":"/dev/null","cwd":"/tmp"'

# The hook command of <event> in settings.snippet.json, with the path pointing here.
hook_cmd() {
    jq -r --arg e "$1" '.hooks[$e][0].hooks[0].command' "$HERE/settings.snippet.json" |
        sed "s|^~/.local/share/bentwire/claude-code/bentwire-hook.sh|$HOOK|"
}

# fire <event> <json>: runs the hook as Claude Code would (sh -c, input on stdin);
# stdout goes to $BW_DIR/hook.out
fire() {
    printf '%s' "$2" | sh -c "$(hook_cmd "$1")" >>"$BW_DIR/hook.out"
}

bw_start_daemon
"$BW_BIN/bentwire" show >/dev/null
: >"$BW_DIR/hook.out"

echo "events"
: >"$BW_LOG"
fire UserPromptSubmit "{$SESSION,\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"fix the build\"}"
bw_wait 10 bw_has '^play Thinking$'
bw_check "UserPromptSubmit plays Thinking" bw_has '^play Thinking$'
fire PreToolUse "{$SESSION,\"hook_event_name\":\"PreToolUse\",\"tool_name\":\"Edit\",\"tool_input\":{\"file_path\":\"/tmp/x\"}}"
bw_wait 10 bw_has '^play Writing$'
bw_check "PreToolUse on Edit plays Writing" bw_has '^play Writing$'
fire Notification "{$SESSION,\"hook_event_name\":\"Notification\",\"message\":\"Claude needs your permission to use Bash\"}"
bw_wait 10 bw_has '^say '
bw_check "Notification plays GetAttention" bw_has '^play GetAttention$'
bw_check "Notification says the message" bw_has '^say Claude needs your permission to use Bash$'
fire Stop "{$SESSION,\"hook_event_name\":\"Stop\",\"stop_hook_active\":false}"
bw_wait 10 bw_has '^play Congratulate$'
bw_check "Stop plays Congratulate" bw_has '^play Congratulate$'
bw_check "each event stops what was playing first" test "$(bw_count '^stop --all$')" -eq 4
bw_check "matcher covers Edit and Write" sh -c \
    "jq -r '.hooks.PreToolUse[0].matcher' '$HERE/settings.snippet.json' | grep -Eq '(^|\\|)Edit(\\||$)' &&
     jq -r '.hooks.PreToolUse[0].matcher' '$HERE/settings.snippet.json' | grep -Eq '(^|\\|)Write(\\||$)'"
bw_advance 3000
bw_check "daemon started Congratulate" bw_started '^play Congratulate$'
bw_check "hooks print nothing" test ! -s "$BW_DIR/hook.out"

echo "events stay in order"
cat >"$BW_DIR/delay" <<EOF
#!/bin/sh
[ "\$*" = "play Thinking" ] && sleep 1
exec "$BENTWIRE_CLI" "\$@"
EOF
chmod +x "$BW_DIR/delay"
: >"$BW_LOG"
BENTWIRE_CLI=$BW_DIR/delay fire UserPromptSubmit "{$SESSION,\"hook_event_name\":\"UserPromptSubmit\",\"prompt\":\"x\"}"
BENTWIRE_CLI=$BW_DIR/delay fire Stop "{$SESSION,\"hook_event_name\":\"Stop\"}"
bw_wait 10 bw_has '^play Congratulate$'
sleep 2
bw_check "a slow earlier event never overtakes a later one" \
    test "$(grep -E '^play ' "$BW_LOG" | cut -f1 | tail -1)" = "play Congratulate"

echo "hooks return at once"
cat >"$BW_DIR/slow" <<'EOF'
#!/bin/sh
sleep 5
EOF
chmod +x "$BW_DIR/slow"
start=$(date +%s%N)
for e in UserPromptSubmit PreToolUse Notification Stop; do
    BENTWIRE_CLI=$BW_DIR/slow fire $e "{$SESSION,\"hook_event_name\":\"$e\",\"message\":\"x\"}"
done
ms=$((($(date +%s%N) - start) / 1000000))
bw_check "4 hooks with a 5 s CLI took ${ms} ms (< 1500)" test "$ms" -lt 1500

echo "opt-out and no daemon"
: >"$BW_LOG"
BENTWIRE_HOOKS_OFF=1 fire Stop "{$SESSION,\"hook_event_name\":\"Stop\"}"
sleep 1
bw_check "BENTWIRE_HOOKS_OFF=1 sends nothing" bw_none
fire Notification "{$SESSION,\"hook_event_name\":\"Notification\",\"notification_type\":\"auth_success\",\"message\":\"ok\"}"
sleep 1
bw_check "auth_success notification sends nothing" bw_none
fire Notification "{$SESSION,\"hook_event_name\":\"Notification\",\"notification_type\":\"idle_prompt\",\"message\":\"Claude is waiting for your input\"}"
bw_wait 10 bw_has '^say '
bw_check "idle_prompt notification says the message" bw_has '^say Claude is waiting for your input$'
bw_stop_daemon
: >"$BW_LOG"
fire Stop "{$SESSION,\"hook_event_name\":\"Stop\"}"
fire Notification "{$SESSION,\"hook_event_name\":\"Notification\",\"message\":\"x\"}"
sleep 1
bw_check "no CLI call when bentwired is not running" bw_none
bw_finish
