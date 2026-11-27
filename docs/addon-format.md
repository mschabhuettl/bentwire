# bentwire add-on format

An add-on is a folder with a manifest, `addon.toml`, at its top. It holds characters, a
balloon theme, or a bridge: a separate program that talks to the daemon over D-Bus. The
rules are in `docs/spec.md` B-ADDON-1 to B-ADDON-5; this file is the guide for authors.

## 1. `addon.toml`

```toml
name = "wire-characters"   # one path component: [a-z0-9][a-z0-9._-]*, no ".."
kind = "character"         # "character", "balloon-theme" or "bridge"
version = "1.0.0"          # free text, not empty
# command = ["./run.sh", "--flag"]   # bridges only, and required for them
```

| Key | Required | Meaning |
|---|---|---|
| `name` | yes | The add-on's name. It is also the folder name the add-on is installed under, so it must be one safe path component: a lowercase ASCII letter or digit, then letters, digits, `.`, `_` or `-`, and no `..` anywhere. |
| `kind` | yes | `character`, `balloon-theme` or `bridge`. |
| `version` | yes | Shown by `bentwire addons list`; not compared. |
| `command` | bridges only | The argv of the bridge, a list of strings with a non-empty first element. Not allowed for the other kinds. |

Other keys are errors (a typo such as `comand` is caught at install).

## 2. Characters and balloon themes

An installed, enabled add-on of kind `character` or `balloon-theme` is a search root and is
read like a pack (A-PACK-1): its direct children are characters (a folder with
`character.toml`, or a `.bwc` file), and `balloon-themes/<id>/theme.toml` is a balloon
theme (A-THEME-1, B-REN-7). A `pack.toml` is optional; when present it gives the display
labels (A-LABEL-1). A folder with `character.toml` next to `addon.toml` is a single
character whose id is the add-on's name.

```
wire-characters/
  addon.toml
  pack.toml            (optional)
  wire/character.toml
  wire/sheets/...
  balloon-themes/plain/theme.toml   (a theme add-on, or a pack with one)
```

The `kind` tells users what to expect; both kinds are searched the same way. Add-on roots
come after `bentwired --characters` and `character_paths`, and before
`$XDG_DATA_HOME/bentwire`, in name order. So an enabled add-on's character wins over an
installed pack's character with the same id, and its theme over the pack's theme;
disabling the add-on brings the others back.

## 3. Bridges

A bridge is a program that uses the D-Bus API (`docs/dbus.md`, "Writing a bridge"). When
it is enabled, the daemon starts it once it owns its bus name and supervises it:

- The program is `command[0]`: a relative path with a `/` (`./run.sh`, `bin/bridge`) is
  taken from the add-on folder, a bare name (`bentwire-notify`) is looked up in `PATH`,
  an absolute path is used as it is. The other elements are its arguments. It runs in the
  add-on folder, with the daemon's environment (so `DBUS_SESSION_BUS_ADDRESS` is set),
  stdin from `/dev/null`, and stdout and stderr shared with the daemon (the journal under
  systemd).
- It runs in its own process group. If it exits, for any reason, it is started again after
  1 s, then 2 s, 4 s and so on up to 60 s; after a run of 60 s or more the wait starts at
  1 s again.
- It is stopped with SIGTERM to its process group when it is disabled or removed, and when
  the daemon quits (Quit, SIGTERM, SIGINT, the end of the session bus); a bridge still
  running 2 s after that gets SIGKILL. If the daemon dies without stopping it, the bridge
  gets SIGTERM from the kernel (`PR_SET_PDEATHSIG`). That signal reaches only the bridge
  process itself: a wrapper script should `exec` the real program, as below.
- Installing never enables a bridge, also not when it replaces an enabled one: only
  `bentwire addons enable <name>` makes its command run.

### Wrapping `bentwire-notify` or `bentwire-mqtt`

The two bridges that come with bentwire are separate programs with their own systemd
units. Either can run as a bridge add-on instead, for example the notification bridge with
a filter:

```
notify/
  addon.toml
```

```toml
name = "notify"
kind = "bridge"
version = "1"
command = ["bentwire-notify", "--ignore", "Spotify"]
```

```sh
bentwire addons install ./notify
bentwire addons enable notify
```

Use one way or the other: with both the add-on and `bentwire-notify.service` enabled, every
notification is said twice. The same works for `bentwire-mqtt` (`command =
["bentwire-mqtt"]`, configured in `~/.config/bentwire/mqtt.toml`).

A bridge of your own with a wrapper script:

```toml
name = "door-bell"
kind = "bridge"
version = "0.1"
command = ["./run.sh"]
```

```sh
#!/bin/sh
# run.sh (mode 0755): settings, then exec the real program in place of the shell.
export DOOR_BELL_TOPIC=home/door
exec python3 -I ./bridge.py
```

## 4. Packaging and installing

`bentwire addons install` takes a folder, a `.zip` or a `.tar.gz` archive (recognised by
its content, not its name), or an `https://` URL of such an archive (`--sha256 HEX` checks
it; a `file://` URL is accepted as a local file). In an archive, `addon.toml` is at the top
or in its only top-level folder (as GitHub release tarballs have it). Archives and folders
may hold only folders and regular files: links, devices and paths that are absolute or
contain `..` make the install fail. Files keep only their executable bit (0755, else
0644); times are those of the install.

| Path | What |
|---|---|
| `$XDG_DATA_HOME/bentwire/addons/<name>/` | an installed add-on (`~/.local/share/bentwire/addons/`) |
| `$XDG_CONFIG_HOME/bentwire/addons.toml` | the enabled list, `enabled = ["name", ...]`, written by the CLI |

A running daemon reads both again on SIGHUP, which the CLI sends after each change.
