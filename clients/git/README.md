# Git hooks

The character reacts to your git work:

| Hook | The character |
|---|---|
| `post-commit` | plays `Congratulate` and says "Committed: <subject>" |
| `post-merge` (also after `git pull`) | plays `Congratulate` and says "Merge done." |
| `pre-push` | plays `SendMail` and says "Pushing to <remote>." |
| any hook, when the repository's own hook of that name fails (a lint `pre-commit`, a `commit-msg` check, a `pre-push` test run) | plays `GetAttention` and says "The <hook> hook failed." |

`hooks/bentwire-git-hook` is the one script; the other files in `hooks/` are symbolic
links to it, one per client-side hook name (`pre-commit`, `commit-msg`, `post-checkout`,
`post-rewrite`, ...).

## Install

**All repositories** (`core.hooksPath`):

```sh
git config --global core.hooksPath /path/to/bentwire/clients/git/hooks
```

With `core.hooksPath` set, git no longer runs the hooks in a repository's `.git/hooks/`.
These hooks run them for you: each one first runs `.git/hooks/<name>` (of the main
working tree for linked worktrees) with the same arguments and stdin, and exits with its
status, so a failing `pre-commit` still stops the commit. Hook names that are not in
`hooks/` (`reference-transaction`, `fsmonitor-watchman`, server-side hooks) are not
chained. Tools that install their own hooks into `.git/hooks/` keep working this way;
tools that set `core.hooksPath` themselves replace this setting for their repository.

**One repository**, without a hook of that name yet: link the script under the hook name.

```sh
ln -s /path/to/bentwire/clients/git/hooks/bentwire-git-hook .git/hooks/post-commit
```

**One repository with an existing hook**: add one line to it. Called under its own name
with the hook name as first argument, the script only lets the character react and does
not chain.

```sh
/path/to/bentwire/clients/git/hooks/bentwire-git-hook post-commit "$@"
```

The hooks need the `bentwire` CLI on `PATH` and `busctl` (systemd) or `dbus-send`.

## Behaviour

- Only the `bentwire` CLI is used (`play`, `say`).
- git never waits: the calls run detached with a 3 s `timeout`, read no stdin and print
  nothing. The exit status is that of the repository's own hook (0 without one).
- Nothing is sent when bentwired is not running (no D-Bus activation).
- `BENTWIRE_GIT_OFF=1` disables the reactions; chaining to the repository's hooks stays.
  `BENTWIRE_CLI` names another CLI.

## Test

```sh
cargo build -p bentwired -p bentwire
clients/git/test.sh
```

The test runs `bentwired --headless` with `characters/builtin` under `dbus-run-session`,
with an isolated `HOME`, and a temporary repository with `core.hooksPath` set to
`hooks/`. It commits, merges and pushes to a local bare repository and checks the CLI
calls and that the daemon accepted them; that the repository's own `post-commit`,
`commit-msg` and `pre-push` hooks run with their arguments and stdin; that a failing own
`pre-commit` or `pre-push` still stops git and is reported; the call from an existing
hook; the opt-out; and silence without a daemon.
