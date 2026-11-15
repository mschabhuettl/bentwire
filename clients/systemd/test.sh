#!/bin/bash
# Scripted test of the systemd failure helper.
#
# Starts `bentwired --headless` with characters/builtin under dbus-run-session and runs
# bentwire-unit-failed directly with a unit name, as bentwire-failure@.service does (no
# running systemd needed). Checks the CLI calls, that the daemon accepted and started
# them (request ids, a matching trace call line with method and args, RequestStart after
# Test1.Advance), the exit status, the opt-out, and silence without a daemon.
#
# usage: clients/systemd/test.sh   (needs: cargo build -p bentwired -p bentwire)
# Exit status 0 = all checks passed.
set -u
# shellcheck source=../testlib.sh
. "$(dirname "$0")/../testlib.sh"
bw_setup "$@"
HELPER=$BW_REPO/clients/systemd/bentwire-unit-failed

bw_start_daemon
"$BW_BIN/bentwire" show >/dev/null

echo "a failed unit"
: >"$BW_LOG"
"$HELPER" backup.service >"$BW_DIR/helper.out" 2>&1
bw_check "exit status 0" test $? -eq 0
bw_check "printed nothing" test ! -s "$BW_DIR/helper.out"
bw_check "GetAttention" bw_has '^play GetAttention$'
bw_check "says which unit failed" bw_has '^say The unit backup\.service failed\.$'
bw_check "nothing else" test "$(wc -l <"$BW_LOG")" -eq 2
bw_check "daemon received each call (method, args, id)" bw_received '.'
bw_advance 5000
bw_check "daemon started them" bw_started '.'
"$HELPER" 'sync@-n.service'
bw_check "an instance name starting with - is text, not an option" bw_has '^say The unit sync@-n\.service failed\.$'

echo "usage and opt-out"
: >"$BW_LOG"
"$HELPER" 2>/dev/null
bw_check "no unit name: exit status 2" test $? -eq 2
BENTWIRE_SYSTEMD_OFF=1 "$HELPER" backup.service
bw_check "opt-out: exit status 0" test $? -eq 0
bw_check "no CLI call without a unit name or with the opt-out" bw_none

echo "silence without a daemon"
bw_stop_daemon
"$HELPER" backup.service
bw_check "exit status 0" test $? -eq 0
bw_check "no CLI call when bentwired is not running" bw_none
bw_finish
