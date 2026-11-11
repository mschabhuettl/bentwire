# Claude Code hooks

The character shows what Claude Code is doing:

| Claude Code event | Hook argument | The character |
|---|---|---|
| `UserPromptSubmit` (thinking) | `thinking` | plays `Thinking` |
| `PreToolUse` on `Edit`, `MultiEdit`, `Write`, `NotebookEdit` (writing) | `writing` | plays `Writing` |
| `Notification` (needs permission, waiting for input) | `attention` | plays `GetAttention` and says the notification text, e.g. "Claude needs your permission to use Bash" |
| `Stop` (done) | `done` | plays `Congratulate` |

Each event first stops what the character is still playing (`bentwire stop --all`).
Notifications are shown only for `permission_prompt`, `idle_prompt`, `agent_needs_input`
or when no `notification_type` is given; `auth_success` and similar are ignored. A
character without one of these animations falls back by the daemon's rules (B-FB-1).

## Install

1. Copy the script to the path the snippet uses (or edit the paths in the snippet):

   ```sh
   mkdir -p ~/.local/share/bentwire/claude-code
   cp clients/claude-code/bentwire-hook.sh ~/.local/share/bentwire/claude-code/
   ```

2. Merge the `hooks` object of [`settings.snippet.json`](settings.snippet.json) into
   `~/.claude/settings.json` (all projects) or `.claude/settings.json` (one project). If
   you already have hooks for these events, add the entries to the existing arrays.

The hook needs the `bentwire` CLI on `PATH`, `jq` or `python3` to read the hook input,
and `busctl` or `dbus-send`.

## Behaviour

- The hook reads its JSON input, prints nothing, and exits 0 at once. Output matters:
  stdout of a `UserPromptSubmit` hook would be added to Claude's context. The CLI calls
  run detached with a 3 s `timeout`, so Claude Code never waits for the character.
- Nothing is sent when bentwired is not running. The check asks the bus for the owner of
  `org.bentwire.Agent`, so the daemon is never started through D-Bus activation.
- `BENTWIRE_HOOKS_OFF=1` in Claude Code's environment disables the hooks.
  `BENTWIRE_CLI` names another CLI.

## Test

```sh
cargo build -p bentwired -p bentwire
clients/claude-code/test.sh
```

The test runs `bentwired --headless` with `characters/builtin` under
`dbus-run-session`, with an isolated `HOME`. It runs each hook command from the snippet
through `sh -c`, with the JSON input Claude Code sends, and checks:

- the CLI calls, and that the daemon accepted and started them (request ids; the `--trace` call line for each id names the same method
  and arguments; `RequestStart` after `Test1.Advance`);
- that the hooks print nothing;
- that four hooks with a 5 s CLI return in milliseconds;
- the python3 fallback for reading the hook input (on a `PATH` without `jq`);
- the notification filter, the opt-out, and silence without a daemon.

It needs `dbus-run-session`, `gdbus` and `jq`. `BENTWIRE_TEST_KEEP=1` keeps the
artefacts.
