#!/bin/bash
# Scripted test of the git hooks.
#
# Starts `bentwired --headless` with characters/builtin under dbus-run-session, points
# `core.hooksPath` of a temporary repository at clients/git/hooks and commits, merges and
# pushes there. Checks the CLI calls, that the daemon accepted them (request ids, a
# matching trace call line with method and args), that the repository's own hooks still
# run (arguments, stdin, exit status), the call from an existing hook, and silence without
# a daemon.
#
# usage: clients/git/test.sh   (needs: cargo build -p bentwired -p bentwire; git)
# Exit status 0 = all checks passed.
set -u
# shellcheck source=../testlib.sh
. "$(dirname "$0")/../testlib.sh"
bw_setup "$@"
command -v git >/dev/null || { echo "git not found"; exit 1; }
HOOKS=$BW_REPO/clients/git/hooks
export GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=Test GIT_AUTHOR_EMAIL=test@example.invalid
export GIT_COMMITTER_NAME=Test GIT_COMMITTER_EMAIL=test@example.invalid
R=$BW_DIR/repo
OWN=$BW_DIR/own.log

# own_hook <repo> <name> <body>: the repository's own hook
own_hook() {
    printf '#!/bin/sh\n%s\n' "$3" >"$1/.git/hooks/$2"
    chmod +x "$1/.git/hooks/$2"
}

bw_start_daemon
"$BW_BIN/bentwire" show >/dev/null
git init -q -b main "$R"
git -C "$R" config core.hooksPath "$HOOKS"
: >"$OWN"

echo "post-commit (core.hooksPath)"
: >"$BW_LOG"
own_hook "$R" post-commit "echo \"post-commit \$#\" >>$OWN"
git -C "$R" commit -q --allow-empty -m '-n looks like an option' >"$BW_DIR/git.out" 2>&1
bw_wait 10 bw_has '^say Committed'
bw_check "Congratulate" bw_has '^play Congratulate$'
bw_check "says the subject" bw_has '^say Committed: -n looks like an option$'
bw_check "daemon received each call (method, args, id)" bw_received '.'
bw_check "the repository's own post-commit ran" grep -qx 'post-commit 0' "$OWN"
bw_check "the hook printed nothing" test ! -s "$BW_DIR/git.out"

echo "a failing pre-commit of the repository"
: >"$BW_LOG"
own_hook "$R" pre-commit 'echo "lint failed" >&2; exit 1'
bw_check "the commit is refused" bash -c "! git -C '$R' commit -q --allow-empty -m refused 2>/dev/null"
bw_check "no commit was made" test "$(git -C "$R" log -1 --format=%s)" = '-n looks like an option'
bw_wait 10 bw_has '^say The pre-commit hook failed\.$'
bw_check "GetAttention" bw_has '^play GetAttention$'
bw_check "says which hook failed" bw_has '^say The pre-commit hook failed\.$'
bw_check "no post-commit reaction" test "$(bw_count '^say Committed')" -eq 0
rm "$R/.git/hooks/pre-commit"

echo "commit-msg gets its argument"
own_hook "$R" commit-msg "echo \"commit-msg \$1\" >>$OWN"
git -C "$R" commit -q --allow-empty -m second
bw_check "the repository's own commit-msg got the message file" grep -q '^commit-msg .*COMMIT_EDITMSG$' "$OWN"

echo "post-merge"
git -C "$R" checkout -q -b topic
git -C "$R" commit -q --allow-empty -m topic
git -C "$R" checkout -q main
bw_wait 10 bw_has '^say Committed: topic$'
: >"$BW_LOG"
git -C "$R" merge -q topic
bw_wait 10 bw_has '^say Merge done\.$'
bw_check "Congratulate" bw_has '^play Congratulate$'
bw_check "says the merge is done" bw_has '^say Merge done\.$'
bw_check "daemon received each call (method, args, id)" bw_received '.'

echo "pre-push"
git init -q --bare "$BW_DIR/remote.git"
git -C "$R" remote add origin "$BW_DIR/remote.git"
own_hook "$R" pre-push "echo \"pre-push \$1\" >>$OWN; cat >>$OWN"
: >"$BW_LOG"
git -C "$R" push -q origin main 2>"$BW_DIR/git.out"
bw_wait 10 bw_has '^say Pushing'
bw_check "SendMail" bw_has '^play SendMail$'
bw_check "says the remote" bw_has '^say Pushing to origin\.$'
bw_check "daemon received each call (method, args, id)" bw_received '.'
bw_check "the repository's own pre-push got remote and refs on stdin" \
    grep -q '^refs/heads/main [0-9a-f]* refs/heads/main ' "$OWN"
bw_check "pushed" test "$(git -C "$BW_DIR/remote.git" log -1 --format=%s main)" = topic
own_hook "$R" pre-push 'exit 1'
: >"$BW_LOG"
git -C "$R" commit -q --allow-empty -m third
bw_check "a failing own pre-push still stops the push" bash -c "! git -C '$R' push -q origin main 2>/dev/null"
bw_wait 10 bw_has '^say The pre-push hook failed\.$'
bw_check "says which hook failed" bw_has '^say The pre-push hook failed\.$'
bw_check "no Pushing" test "$(bw_count '^say Pushing')" -eq 0

echo "called from an existing hook (no core.hooksPath)"
R2=$BW_DIR/repo2
git init -q -b main "$R2"
own_hook "$R2" post-commit "$HOOKS/bentwire-git-hook post-commit"
: >"$BW_LOG"
git -C "$R2" commit -q --allow-empty -m 'from an existing hook'
bw_wait 10 bw_has '^say Committed'
sleep 1
bw_check "says the subject once" test "$(bw_count '^say Committed: from an existing hook$')" -eq 1
bw_check "nothing else" test "$(wc -l <"$BW_LOG")" -eq 2

echo "BENTWIRE_GIT_OFF=1 and silence without a daemon"
: >"$BW_LOG"
BENTWIRE_GIT_OFF=1 git -C "$R" commit -q --allow-empty -m off
sleep 1.5
bw_check "no CLI call with the opt-out" bw_none
bw_stop_daemon
git -C "$R" commit -q --allow-empty -m 'no daemon'
sleep 1.5
bw_check "no CLI call when bentwired is not running" bw_none
bw_finish
