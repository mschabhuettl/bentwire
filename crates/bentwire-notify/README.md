# bentwire-notify

Notification bridge (spec B-NOTIFY-1): the character says your desktop notifications.

It watches the session bus for `Notify` calls to `org.freedesktop.Notifications` (as a
bus monitor, so your notification daemon keeps showing them as usual) and calls
`Speak` on bentwired with `summary: body`, or just the summary when the body is empty.
Body markup (`<b>`, `<i>`, `<u>`, `<a href>`, `<img>`) is removed, entities are decoded,
whitespace is collapsed and the text is cut at 300 characters. `|` and `\` become `/`,
so a notification cannot pick Speak alternatives or use Speak tags.

```sh
bentwire-notify                          # every app
bentwire-notify --only Thunderbird --only Signal
bentwire-notify --ignore Spotify
```

## Filter

App names are compared case-insensitively with the notification's `app_name`
(`dbus-monitor "interface='org.freedesktop.Notifications'"` shows it).

- `--only APP` (repeatable): say only notifications of these apps.
- `--ignore APP` (repeatable): never say notifications of these apps. It wins over
  `--only`.

The same lists can go into `$XDG_CONFIG_HOME/bentwire/notify.toml`
(`~/.config/bentwire/notify.toml`):

```toml
only = ["thunderbird", "signal"]
ignore = []
```

A flag replaces the file's list of the same key; the other key still comes from the
file. The file is read at start.

## Daemon

The bridge needs no running bentwired at start. Each notification calls `Speak`, which
starts bentwired through D-Bus activation when it is installed. If that fails, the
bridge prints one line on stderr and drops notifications until a `Speak` succeeds.
SIGTERM and SIGINT exit 0; so does the end of the session bus.

## systemd

`packaging/systemd/bentwire-notify.service` is a user unit, not enabled by default:

```sh
cp packaging/systemd/bentwire-notify.service ~/.config/systemd/user/
systemctl --user enable --now bentwire-notify
```

Adjust `ExecStart` if the binary is not in `/usr/bin`, or to add filter flags.
