# Shell integration (bash and zsh)

The character reacts to your terminal:

- **A command fails:** it plays `GetAttention` and says "It looks like your build
  failed." (for `make`, `cargo`, `npm`, `gcc` and other build tools), "It looks like some
  tests failed." (command lines with `test`, `check`, `pytest`, `ctest`) or "It looks like
  that command failed (exit status N)." Ctrl-C (status 130) and Ctrl-Z (148) are ignored.
- **A command runs long** (10 s by default): it plays `Processing` while the command
  runs. When the command ends, the character stops, plays `Congratulate` and says
  "Done after N s: <command>", or reports the failure as above.
- **A command is not found** (opt-in, `BENTWIRE_SHELL_NOT_FOUND=1`): it plays `Searching`
  and says 'I can't find the command "<name>".' The shell still prints its usual message
  and the status stays 127; that miss is not reported a second time as a failure.

## Command not found

Set the variable before the integration is sourced:

```sh
export BENTWIRE_SHELL_NOT_FOUND=1
. /path/to/bentwire/clients/shell/bentwire.sh
```

It defines `command_not_found_handle` (bash) or `command_not_found_handler` (zsh). A
handler that already exists, such as the one of `pkgfile` or `command-not-found` from your
distribution, is kept and called after the character's reaction: it prints its own
suggestions and decides the exit status. So source the distribution's script first.
Sourcing the integration again does not chain the handler to itself.

## Install

Add one line to `~/.bashrc` or `~/.zshrc`:

```sh
. /path/to/bentwire/clients/shell/bentwire.sh
```

It needs the `bentwire` CLI on `PATH` and `busctl` (systemd) or `dbus-send` to tell whether
the daemon runs.

## Behaviour

- Only the `bentwire` CLI is used (`play`, `say`, `stop --all`).
- The prompt never waits: every call runs detached, with a 3 s `timeout`.
- Nothing is sent when bentwired is not running. The check asks the bus whether
  `org.bentwire.Agent` has an owner, so the shell never starts the daemon through D-Bus
  activation.
- Interactive programs (`vim`, `less`, `ssh`, `top`, `tmux`, ...) never count as
  long-running.
- bash: it uses the `DEBUG` trap and `PROMPT_COMMAND` (string or array). With
  [bash-preexec](https://github.com/rcaloras/bash-preexec) loaded first it uses
  `preexec_functions` instead. If another `DEBUG` trap is already set, nothing is
  reported.
- zsh: it uses the `preexec`, `precmd` and `zshexit` hooks.

## Settings

| Variable | Default | Meaning |
|---|---|---|
| `BENTWIRE_SHELL_OFF` | unset | `1` disables everything (takes effect at the next prompt) |
| `BENTWIRE_LONG_SECONDS` | `10` | threshold for long-running commands |
| `BENTWIRE_SHELL_IGNORE` | `exit logout vi vim nvim nano emacs less more most man info ssh mosh telnet top htop btop watch tmux screen fg bg jobs bentwire` | commands that are never watched as long-running |
| `BENTWIRE_CLI` | `bentwire` | the CLI to call |
| `BENTWIRE_SHELL_NOT_FOUND` | unset | `1` when the file is sourced installs the command-not-found handler |

## Test

```sh
cargo build -p bentwired -p bentwire
clients/shell/test.sh
```

The test runs `bentwired --headless` with `characters/builtin` under `dbus-run-session`,
with an isolated `HOME`. It drives an interactive bash and an interactive zsh and checks:

- the CLI calls and their order;
- that the daemon accepted them (request ids) and started them: the `--trace` call line for each request id names the same method and
  arguments, and `RequestStart` follows after `Test1.Advance`;
- that four prompts take well under the 5 s a slow CLI would need;
- the command-not-found handler: one reaction per miss, status 127, the usual message,
  chaining to an earlier handler, no reaction with `BENTWIRE_SHELL_OFF=1`;
- that nothing is sent without a daemon.

It needs `dbus-run-session`, `gdbus`, `jq`, bash and (optionally) zsh.
`BENTWIRE_TEST_KEEP=1` keeps the artefacts (call log, trace, shell output).
