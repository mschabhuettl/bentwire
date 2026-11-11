#!/bin/bash
# Scripted test of the shell integration (BRIEF "Done when" 7) for bash and zsh.
#
# Starts `bentwired --headless` with characters/builtin under dbus-run-session, feeds
# command lines to an interactive bash and zsh that source clients/shell/bentwire.sh, and
# checks which CLI calls the integration made, that the daemon accepted them (request
# ids, a matching trace call line with method and args, RequestStart after Test1.Advance), that the prompt never waits for the
# CLI, and that nothing is sent when the daemon is not running.
#
# usage: clients/shell/test.sh        (needs: cargo build -p bentwired -p bentwire)
# Exit status 0 = all checks passed.
set -u
# shellcheck source=../testlib.sh
. "$(dirname "$0")/../testlib.sh"
bw_setup "$@"
INTEGRATION=$BW_REPO/clients/shell/bentwire.sh
export BENTWIRE_LONG_SECONDS=1

# session <shell> <command line>...: one interactive shell, one prompt per line
session() {
    local sh=$1
    shift
    case $sh in
        bash) printf '%s\n' ". $INTEGRATION" "$@" exit | bash --norc --noprofile -i >"$BW_DIR/$sh.out" 2>&1 ;;
        zsh) printf '%s\n' ". $INTEGRATION" "$@" exit | zsh -f -i >"$BW_DIR/$sh.out" 2>&1 ;;
    esac
}

bw_start_daemon
"$BW_BIN/bentwire" show >/dev/null
cat >"$BW_DIR/delay" <<EOF
#!/bin/sh
[ "\$*" = "play Processing" ] && sleep 2
exec "$BENTWIRE_CLI" "\$@"
EOF
chmod +x "$BW_DIR/delay"

for sh in bash zsh; do
    command -v $sh >/dev/null || { echo "  skip $sh (not installed)"; continue; }
    echo "$sh: failed commands"
    : >"$BW_LOG"
    session $sh false 'cargo() { return 101; }' 'cargo build' '(exit 130)' \
        'BENTWIRE_SHELL_OFF=1' false 'unset BENTWIRE_SHELL_OFF' true
    bw_wait 10 bw_has '^say ' 2
    sleep 1.5 # let stray timers fire (they must not send anything)
    bw_check "failure animation twice" bw_has '^play GetAttention$' 2
    bw_check "generic failure text" bw_has '^say It looks like that command failed \(exit status 1\)\.$'
    bw_check "build failure text" bw_has '^say It looks like your build failed\.$'
    bw_check "nothing for Ctrl-C, opt-out or success" test "$(wc -l <"$BW_LOG")" -eq 4
    bw_check "no Processing for short commands" test "$(bw_count '^play Processing')" -eq 0
    bw_check "daemon received each call (method, args, id)" bw_received '.'

    echo "$sh: long-running command"
    : >"$BW_LOG"
    session $sh 'sleep 2.5' 'BENTWIRE_SHELL_IGNORE=sleep' 'sleep 2.5'
    bw_wait 10 bw_has '^say Done'
    sleep 1.5
    bw_check "Processing while running" bw_has '^play Processing$'
    bw_check "stopped when done" bw_has '^stop --all$'
    bw_check "Congratulate when done" bw_has '^play Congratulate$'
    bw_check "done text" bw_has '^say Done after [0-9]+ s: sleep$'
    bw_check "ignored command not watched" test "$(bw_count '^play Processing')" -eq 1
    bw_check "call order" test "$(cut -f1 "$BW_LOG" | cut -d' ' -f1-2 | tr '\n' '|')" = \
        "play Processing|stop --all|play Congratulate|say Done|"
    bw_check "daemon received each call (method, args, id)" bw_received '.'
    bw_advance 5000
    bw_check "daemon started Congratulate and the done text" bw_started '^(play Congratulate|say Done)'

    echo "$sh: long-running command ended by Ctrl-C (status 130)"
    : >"$BW_LOG"
    session $sh 'sleep 2.5; (exit 130)'
    bw_wait 10 bw_has '^stop --all$'
    sleep 1.5
    bw_check "Processing, then only stop (no success)" test "$(cut -f1 "$BW_LOG" | tr '\n' '|')" = \
        "play Processing|stop --all|"
    bw_check "daemon received each call (method, args, id)" bw_received '.'

    # The Processing call of the timer and the calls at the end of the command run
    # detached; a slow Processing call must still reach the daemon before the stop.
    echo "$sh: slow Processing call, command ends meanwhile"
    : >"$BW_LOG"
    BENTWIRE_CLI=$BW_DIR/delay session $sh 'sleep 2'
    bw_wait 15 bw_has '^say Done'
    bw_check "client order Processing, stop, Congratulate, Done" \
        test "$(cut -f1 "$BW_LOG" | cut -d' ' -f1-2 | tr '\n' '|')" = "play Processing|stop --all|play Congratulate|say Done|"
    bw_check "daemon received them in that order" \
        test "$(jq -r 'select(.type == "call") | .method' "$BW_TRACE" | tail -4 | tr '\n' ' ')" = "Play StopAll Play Speak "
done

echo "bash: the user's PROMPT_COMMAND still sees the exit status"
printf '%s\n' "PROMPT_COMMAND='echo rc=\$? >>$BW_DIR/pc.log'" ". $INTEGRATION" true false '(exit 7)' exit |
    bash --norc --noprofile -i >"$BW_DIR/pc.out" 2>&1
bw_check "statuses 0, 1, 7 reach the existing PROMPT_COMMAND" \
    test "$(tail -3 "$BW_DIR/pc.log" | tr '\n' ' ')" = "rc=0 rc=1 rc=7 "

echo "prompt never waits for the CLI"
cat >"$BW_DIR/slow" <<'EOF'
#!/bin/sh
sleep 5
EOF
chmod +x "$BW_DIR/slow"
# Compared with the same session and a CLI that returns at once, so machine load does
# not matter: a waiting prompt would add at least 5 s.
elapsed_ms() { # <cli> <shell>
    local start
    start=$(date +%s%N)
    BENTWIRE_CLI=$1 session "$2" false false false 'sleep 1.5'
    echo $((($(date +%s%N) - start) / 1000000))
}
for sh in bash zsh; do
    command -v $sh >/dev/null || continue
    base=$(elapsed_ms true $sh)
    ms=$(elapsed_ms "$BW_DIR/slow" $sh)
    bw_check "$sh: 4 prompts with a 5 s CLI took ${ms} ms, ${base} ms with an instant one (< +2000)" \
        test "$ms" -lt $((base + 2000))
done

echo "silent without a daemon"
bw_stop_daemon
: >"$BW_LOG"
for sh in bash zsh; do
    command -v $sh >/dev/null || continue
    session $sh false 'sleep 1.5'
done
sleep 2
bw_check "no CLI call when bentwired is not running" bw_none
bw_finish
