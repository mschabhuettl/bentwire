# Shared helpers for clients/*/test.sh (bash). Source it, then call bw_setup "$@" first.
#
# bw_setup re-executes the calling script inside dbus-run-session with an isolated HOME and
# XDG directories, so the test never touches the user's session or configuration.
# Binaries come from $BENTWIRE_BIN (default: <repo>/target/debug); build them first with
# `cargo build -p bentwired -p bentwire`.

BW_REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
BW_BIN=${BENTWIRE_BIN:-$BW_REPO/target/debug}
BW_FAILS=0

bw_setup() {
    if [ -z "${BW_INNER:-}" ]; then
        for b in bentwired bentwire; do
            [ -x "$BW_BIN/$b" ] || { echo "missing $BW_BIN/$b (cargo build -p bentwired -p bentwire)"; exit 1; }
        done
        command -v dbus-run-session >/dev/null || { echo "dbus-run-session not found"; exit 1; }
        local t rc=0
        t=$(mktemp -d)
        mkdir -m 0700 "$t/run"
        export BW_INNER=1 BW_DIR=$t XDG_RUNTIME_DIR=$t/run HOME=$t/home
        export XDG_CONFIG_HOME=$t/home/cfg XDG_STATE_HOME=$t/home/state XDG_DATA_HOME=$t/home/data
        export XDG_CACHE_HOME=$t/home/cache XDG_DATA_DIRS=$t/nonexistent
        mkdir -p "$HOME"
        dbus-run-session -- bash "$0" "$@" || rc=$?
        if [ "$rc" -eq 0 ] && [ -z "${BENTWIRE_TEST_KEEP:-}" ]; then
            rm -rf "${t:?}"
        else
            echo "artefacts in $t"
        fi
        exit "$rc"
    fi
    trap bw_stop_daemon EXIT
    # The CLI the clients call: logs "args<TAB>status<TAB>output" per call, then the real
    # CLI's result is passed through. The output of play/say is the daemon's request id.
    BW_LOG=$BW_DIR/calls.log
    : >"$BW_LOG"
    cat >"$BW_DIR/bentwire" <<EOF
#!/bin/bash
out=\$("$BW_BIN/bentwire" "\$@" 2>&1)
rc=\$?
printf '%s\t%s\t%s\n' "\$*" "\$rc" "\$(printf '%s' "\$out" | tr '\n' ' ')" >>"$BW_LOG"
printf '%s\n' "\$out"
exit \$rc
EOF
    chmod +x "$BW_DIR/bentwire"
    export BENTWIRE_CLI=$BW_DIR/bentwire
}

bw_start_daemon() {
    BW_TRACE=$BW_DIR/trace.jsonl
    # Only builtin is installed: no first-start install hint (B-CHAR-6) in front of the calls.
    mkdir -p "$XDG_STATE_HOME/bentwire"
    echo 'install_hint_shown = true' >"$XDG_STATE_HOME/bentwire/state.toml"
    "$BW_BIN/bentwired" --headless --no-audio --characters "$BW_REPO/characters/builtin" \
        --trace "$BW_TRACE" >"$BW_DIR/daemon.log" 2>&1 &
    BW_PID=$!
    bw_wait 10 "$BW_BIN/bentwire" status || { echo "bentwired did not start"; cat "$BW_DIR/daemon.log"; exit 1; }
}

bw_stop_daemon() {
    if [ -n "${BW_PID:-}" ]; then
        kill "$BW_PID" 2>/dev/null
        wait "$BW_PID" 2>/dev/null
        BW_PID=
    fi
    return 0
}

bw_wait() { # bw_wait <seconds> <command...>: retry every 0.1 s
    local n=$(($1 * 10))
    shift
    for _ in $(seq "$n"); do "$@" >/dev/null 2>&1 && return 0; sleep 0.1; done
    return 1
}

bw_check() { # bw_check <description> <command...>
    local d=$1
    shift
    if "$@"; then echo "  ok   $d"; else echo "  FAIL $d"; BW_FAILS=$((BW_FAILS + 1)); fi
}

# Calls whose arguments match the extended regex $1 and that the daemon accepted (status 0).
bw_calls() {
    re=$1 awk -F'\t' '$1 ~ ENVIRON["re"] && $2 == 0' "$BW_LOG"
}

bw_count() { bw_calls "$1" | wc -l; }

# bw_has <regex> [n]: at least n (default 1) accepted calls match, each answered with a
# request id where the command returns one.
bw_has() {
    [ "$(bw_count "$1")" -ge "${2:-1}" ] || return 1
    case $1 in
        ^play*|^say*) bw_calls "$1" | awk -F'\t' '$3 !~ /^[0-9]+ *$/ {bad=1} END {exit bad}' ;;
    esac
}

bw_none() { [ "$(wc -l <"$BW_LOG")" -eq 0 ]; }

# Advances the daemon's fake clock and returns the scene snapshot (JSON).
bw_advance() {
    gdbus call --session -d org.bentwire.Agent -o /org/bentwire/Test -m org.bentwire.Test1.Advance "$1" >/dev/null
}

bw_snapshot() {
    gdbus call --session -d org.bentwire.Agent -o /org/bentwire/Test -m org.bentwire.Test1.Snapshot |
        sed -e "s/^('//" -e "s/',)$//"
}

# bw_started <regex>: every accepted call matching the regex got a RequestStart for its
# request id in the daemon trace (needs bw_advance first; the clock is fake).
bw_started() {
    local id ids
    ids=$(bw_calls "$1" | cut -f3 | tr -d ' ')
    [ -n "$ids" ] || return 1
    for id in $ids; do
        jq -e --argjson id "$id" 'select(.type == "signal" and .member == "RequestStart" and .args[0] == $id)' \
            "$BW_TRACE" >/dev/null || return 1
    done
}

# bw_received <regex>: for every accepted play/say call matching the regex, the daemon
# trace has a call line with the request id the CLI printed, the method (Play, Speak) and
# the same argument. Calls without a request id (stop --all) must appear as StopAll.
bw_received() {
    local line verb rest id method n=0
    while IFS=$'\t' read -r line _ id; do
        verb=${line%% *}
        rest=${line#* }
        id=${id// /}
        case $verb in
            play) method=Play ;;
            say) method=Speak ;;
            stop)
                jq -e 'select(.type == "call" and .method == "StopAll")' "$BW_TRACE" >/dev/null || return 1
                n=$((n + 1))
                continue
                ;;
            *) return 1 ;;
        esac
        jq -e --argjson id "$id" --arg m "$method" --arg a "$rest" \
            'select(.type == "call" and .id == $id and .method == $m and .args == [$a])' \
            "$BW_TRACE" >/dev/null || return 1
        n=$((n + 1))
    done < <(bw_calls "$1")
    [ "$n" -gt 0 ]
}

bw_finish() {
    bw_stop_daemon
    if [ "$BW_FAILS" -eq 0 ]; then echo "all checks passed"; else echo "$BW_FAILS check(s) failed"; exit 1; fi
}
