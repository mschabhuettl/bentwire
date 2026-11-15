# systemd unit failures

When a systemd user unit fails, the character plays `GetAttention` and says "The unit
backup.service failed."

- `bentwire-failure@.service`: a template for `OnFailure=`; the instance is the name of
  the failed unit.
- `bentwire-unit-failed <unit>`: the helper it runs.

## Install

```sh
mkdir -p ~/.local/share/bentwire/systemd ~/.config/systemd/user
cp clients/systemd/bentwire-unit-failed ~/.local/share/bentwire/systemd/
cp clients/systemd/bentwire-failure@.service ~/.config/systemd/user/
systemctl --user daemon-reload
```

A distribution package may already ship both (the template in `/usr/lib/systemd/user/`).

Then add `OnFailure=` to the units to watch, with a drop-in
(`systemctl --user edit backup.service`, or the file
`~/.config/systemd/user/backup.service.d/bentwire.conf`):

```ini
[Unit]
OnFailure=bentwire-failure@%n.service
```

**All user services**: put the same drop-in in `~/.config/systemd/user/service.d/bentwire.conf`,
and turn it off for the template itself, or a failing handler would start another one
(`~/.config/systemd/user/bentwire-failure@.service.d/bentwire.conf`):

```ini
[Unit]
OnFailure=
```

The helper needs the `bentwire` CLI on the user manager's `PATH` (`systemctl --user
show-environment`); otherwise set `Environment=BENTWIRE_CLI=/path/to/bentwire` in a drop-in
for `bentwire-failure@.service`. It also needs `busctl` or `dbus-send`.

## System units

The character lives on your session bus, so a failed system unit has to reach it as you.
Copy the template to `/etc/systemd/system/` and give the system copy a drop-in
(`sudo systemctl edit bentwire-failure@.service`; user `alice` with UID 1000 here, helper
in `/usr/local/bin`):

```ini
[Service]
User=alice
Environment=XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
ExecStart=
ExecStart=/usr/local/bin/bentwire-unit-failed %i
```

Then add `OnFailure=bentwire-failure@%n.service` to the system units as above
(`sudo systemctl edit nightly-backup.service`). When you are not logged in, the bus is
not there and nothing is said.

## Behaviour

- Only the `bentwire` CLI is used (`play`, `say`), each call with a 3 s `timeout`.
- Nothing is sent when bentwired is not running (no D-Bus activation).
- The helper exits 0 (2 without a unit name), so the handler unit does not fail.
- `BENTWIRE_SYSTEMD_OFF=1` (in a drop-in: `Environment=BENTWIRE_SYSTEMD_OFF=1`) disables
  it. `BENTWIRE_CLI` names another CLI.

## Test

```sh
cargo build -p bentwired -p bentwire
clients/systemd/test.sh
```

The test runs `bentwired --headless` with `characters/builtin` under `dbus-run-session`,
with an isolated `HOME`, and calls the helper directly with a unit name, as the template
does; no running systemd is needed. It checks the CLI calls, that the daemon accepted and
started them, the exit status, the opt-out and silence without a daemon.
