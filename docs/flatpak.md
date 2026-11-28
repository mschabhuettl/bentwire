# Flatpak

bentwire as a Flatpak: app ID `org.bentwire.Agent`, runtime `org.freedesktop.Platform`
26.08, built from source by `packaging/flatpak/org.bentwire.Agent.yml`. Every release has a
bundle `bentwire-VERSION-x86_64.flatpak`; it is not on Flathub (see the end of this file).

The Flatpak contains the daemon `bentwired`, the CLI `bentwire`, the builtin characters
(Wire and Quack, in `/app/share/bentwire/characters/builtin`, found through `XDG_DATA_DIRS`)
and espeak-ng 1.52 with its data for speech output.

**Compositors.** KDE Plasma (KWin 6.6) works from the sandbox. sway does not: Flatpak
connects the app through `wp_security_context_v1`, and sway hides `zwlr_layer_shell_v1`
from every client with a security context. The daemon then exits with "the compositor has
no wlr-layer-shell". See [sway](#sway).

## Install and use

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user bentwire-0.7.0-x86_64.flatpak     # pulls the runtime from Flathub
flatpak run --command=bentwire org.bentwire.Agent say hello
flatpak run --command=bentwire org.bentwire.Agent characters install classics
```

The app exports the D-Bus activation file for `org.bentwire.Agent`, so any `bentwire`
call (from the Flatpak or from the host) starts the sandboxed daemon on demand (as with a
host install, the session bus's activation environment must contain `WAYLAND_DISPLAY`,
which desktop sessions set at login). The launcher "bentwire" in the application menu
starts it as well; `flatpak run org.bentwire.Agent` runs `bentwired` with options
(`flatpak run org.bentwire.Agent --help`). To start it with the session, put the launcher into your desktop's autostart
(Plasma: System Settings, Autostart; others: copy
`~/.local/share/flatpak/exports/share/applications/org.bentwire.Agent.desktop` to
`~/.config/autostart/`). A Flatpak cannot install an autostart entry itself.

## Permissions

| Permission | Why |
|---|---|
| `--socket=wayland` | The character, balloons and menus are Wayland surfaces (wlr-layer-shell, `wl_shm`). No X11: bentwire is Wayland only. Under sway this socket does not offer layer-shell, see [sway](#sway). |
| `--socket=pulseaudio` | Character sounds and speech. The daemon plays through libpulse-simple (PulseAudio or pipewire-pulse); the runtime's alsa-lib also goes through its pulse plugin. |
| `--talk-name=org.kde.StatusNotifierWatcher` | The tray icon (B-TRAY-1): the item registers with the panel's watcher. In the sandbox it registers under its unique bus name, so no `--own-name=org.kde.StatusNotifierItem-*` is needed. |
| `--share=network` | `bentwire characters install classics` and `bentwire addons install URL` download packs and add-ons (curl from the runtime). The daemon itself makes no network connections. |

Not requested, and why:

- **Owning `org.bentwire.Agent`**: the app ID equals the bus name, and Flatpak lets an app
  own its own ID without `--own-name`. The name stays public, so host clients reach the
  daemon.
- **Notifications** (`org.freedesktop.Notifications`): bentwired sends none.
- **Files** (`--filesystem`): the daemon reads characters, config and state from the app's
  own directories. See "Host files" below for the cases that need more.
- **`--device=dri`, `--share=ipc`, X11**: the renderer draws in software into shared memory
  buffers.

## D-Bus signals from the sandbox

The daemon's `RequestStart`, `RequestComplete`, `AskComplete` and `Bookmark` are unicast
signals to the caller ([dbus.md](dbus.md)). The Flatpak D-Bus proxy drops signals that a
sandboxed app addresses to a unique name, so inside a sandbox (`/.flatpak-info` exists)
bentwired broadcasts these four instead. `bentwire` (0.7 and later) and `bentwire-mqtt`
add a match rule for them and filter by request id. Older CLIs and own clients that rely on
the unicast delivery without a match rule wait forever for `--wait` or an `ask` answer from
the Flatpak daemon; add the match rule
`type='signal',sender='org.bentwire.Agent',interface='org.bentwire.Character1'`.

Known consequence: inside the sandbox every client on the session bus can see these four
signals, that is the request ids, completion status and `ask` answers of other clients
(outside the sandbox only the caller receives them). The session bus is the user's own, so
this exposes nothing to other users.

## Where things live

Inside the sandbox the XDG directories are the app's own:

| What | Host path |
|---|---|
| Config (`config.toml`) | `~/.var/app/org.bentwire.Agent/config/bentwire/` |
| State | `~/.var/app/org.bentwire.Agent/.local/state/bentwire/` |
| Cache (converted sheets) | `~/.var/app/org.bentwire.Agent/cache/bentwire/` |
| Installed packs (`characters install`) | `~/.var/app/org.bentwire.Agent/data/bentwire/characters/` |
| Add-ons | `~/.var/app/org.bentwire.Agent/data/bentwire/addons/` |

## A host `bentwire` and the client hooks

- The Flatpak ships its own CLI: `flatpak run --command=bentwire org.bentwire.Agent ...`.
- A `bentwire` installed on the host (0.7 or later, see above) also works with the
  sandboxed daemon for everything that goes over D-Bus (`say`, `play`, `ask`, `status`,
  ...): the bus name is public, and a host call starts the sandboxed daemon on demand.
- But `characters install` and `addons install` of a host CLI write into the host's
  `~/.local/share/bentwire`, which the Flatpak daemon does not read. Install packs and
  add-ons with the Flatpak's CLI.
- The client hooks (`clients/shell`, `clients/claude-code`, `clients/git`, `clients/systemd`)
  call `bentwire` from `PATH`. Without a host bentwire, give them a wrapper:

  ```sh
  mkdir -p ~/.local/bin
  printf '#!/bin/sh\nexec flatpak run --command=bentwire org.bentwire.Agent "$@"\n' >~/.local/bin/bentwire
  chmod +x ~/.local/bin/bentwire
  ```

  Each `flatpak run` adds about 0.1 s to a call.

## Host files

The sandbox sees no host files. Where bentwire needs one, grant it for that run:

- `bentwire convert` and `bentwire addons install ./folder`:
  `flatpak run --filesystem="$PWD" --command=bentwire org.bentwire.Agent convert clippit.acs`
  (or use a host `bentwire convert`, then install the result with `--filesystem=...:ro`).
- `character_paths` in `config.toml` and `bentwired --characters DIR`: `flatpak override
  --user --filesystem=DIR:ro org.bentwire.Agent`, or put the characters into
  `~/.var/app/org.bentwire.Agent/data/bentwire/characters/`.

## sway

Tested with sway 1.11. In sway's `sway/server.c`, `is_privileged()` lists the layer-shell
global among the privileged ones, and `filter_global()` offers those only to clients
without a `wp_security_context_v1` state. This is built in; sway's configuration has no
setting for it. The sandboxed daemon's registry under sway lacks `zwlr_layer_shell_v1`
(and the other privileged globals) while a host client sees it. So the Flatpak does not
run on sway as shipped, and `scripts/desktop/scenario.sh sway` refuses
`BENTWIRE_SCENARIO_FLATPAK` ("NOT RUN").

The only way around it is to bypass the filtered socket. A user override
that hides Flatpak's filtered Wayland socket and exposes the compositor's own socket, with
`WAYLAND_DISPLAY` set inside the sandbox (Flatpak clears it without `--socket=wayland`):

```sh
flatpak run --nosocket=wayland --filesystem=xdg-run/$WAYLAND_DISPLAY \
    --command=sh org.bentwire.Agent -c "WAYLAND_DISPLAY=$WAYLAND_DISPLAY exec bentwired"
```

This gives the app the same Wayland access as an unsandboxed client (all privileged
protocols sway offers), which is the reason sway filters them; a host install is the
plainer choice on sway. D-Bus activation does not use this command line; start the daemon
with it from sway's config (`exec ...`) instead.

## Left out

- **`bentwire-notify`** (the notification bridge) is not in the Flatpak. It watches the
  session bus with `BecomeMonitor`, and the Flatpak D-Bus proxy does not allow monitoring
  (a sandboxed app may not read other apps' messages). Run it from a host install; it talks
  to the sandboxed daemon over the public bus name.
- **`bentwire-mqtt`** and **`bentwire-convert`** are not shipped either (the MQTT bridge can
  run on the host against the Flatpak daemon; `bentwire convert` covers the converter).
- Bridge add-ons run inside the sandbox: their programs must exist there (a program in the
  add-on folder, or one from the runtime).

## Building

```sh
scripts/flatpak-build.sh                  # target/flatpak/bentwire-VERSION-x86_64.flatpak
scripts/flatpak-cargo-sources.sh          # regenerate cargo-sources.json after Cargo.lock changes
scripts/flatpak-cargo-sources.sh --check  # CI: fails when cargo-sources.json is stale
scripts/flatpak-check.sh BUNDLE.flatpak   # install and run the bundle in a fresh podman container
```

`flatpak-build.sh` needs flatpak, flatpak-builder and the flathub remote in the user
installation; `flatpak-cargo-sources.sh` needs python3 with aiohttp and tomlkit (it fetches
a pinned flatpak-cargo-generator). Every `Cargo.lock` change needs a regenerated
`cargo-sources.json`, otherwise CI fails.

CI (`.github/workflows/flatpak.yml`) checks `cargo-sources.json`, validates the metainfo and
the desktop file and builds the bundle on pushes to `main` and on pull requests; the
release workflow builds it again and attaches it to the release.

## What was tested

- `scripts/desktop/scenario.sh kwin` with `BENTWIRE_SCENARIO_FLATPAK=org.bentwire.Agent`
  (the daemon through `flatpak run`, the CLI on the host): all checks pass, including the
  layer surfaces on two outputs with fractional scaling, popups, the tray (StatusNotifierItem
  and dbusmenu through the watcher) and the session bus loss.
- sway 1.11: the sandboxed daemon sees no `zwlr_layer_shell_v1`; with the command in
  [sway](#sway) (in the real `/run/user/UID`) `show --wait` completes and the character is
  mapped and on screen.
- D-Bus activation under KWin: a host `bentwire say --wait` and a sandboxed `flatpak run
  --command=bentwire ... say --wait` each start the sandboxed daemon and complete.
- Sound and speech under KWin with a private PipeWire and pipewire-pulse (null sink): the
  sandboxed daemon logs `audio: pulse` for its sound and its speech output, and its stream
  (`application.process.binary = "bentwired"`) appears on the server while it speaks.
- `scripts/flatpak-check.sh` in a fresh Debian container: install of the bundle,
  `bentwired --headless`, `show --wait`, `say`, espeak-ng initialisation and
  `characters install classics`.
- The host scenarios (sway and KWin, without the Flatpak) still pass.
- Not tested here: the tray in a real Plasma panel (the scenario uses a minimal watcher),
  a real PulseAudio server (only pipewire-pulse), and "Edit configuration" from the menu,
  which runs `xdg-open` inside the sandbox (the runtime's xdg-open goes through the portal).

The container check runs rootless `podman run --security-opt unmask=ALL`: bubblewrap mounts
a new `/proc`, which podman's masked `/proc` paths forbid (`bwrap: Can't mount proc on
/proc`); `seccomp=unconfined` alone does not help, and nothing more is needed (SELinux hosts
may also need `label=disable`). It starts a system bus in the container, which `flatpak
install` of a bundle uses.

## Flathub

Not submitted. What a submission still needs:

- **App ID and domain.** Flathub verifies that the publisher controls the domain of the app
  ID, here `bentwire.org`. Without it the ID would be `io.github.mschabhuettl.bentwire`,
  and the daemon's bus name `org.bentwire.Agent` would then need `--own-name`, which
  Flathub reviews.
- **Sources.** The manifest builds from the local checkout (`type: dir`); Flathub needs a
  `type: git` source with tag and commit (or the release archive with its SHA-256), in a
  repository `flathub/<app id>` with `cargo-sources.json` next to it.
- **Metainfo.** Screenshots of the character on a desktop (the current one is the gallery
  picture), a `<releases>` entry per version, and the screenshots mirrored by Flathub.
- **Licences.** The bundle contains espeak-ng (GPL-3.0-or-later) next to bentwire
  (MIT OR Apache-2.0), the font (LGPL-2.1-or-later) and the builtin characters (CC0);
  `project_license` lists them all.
- **Review of the permissions** above, in particular `--share=network`.
