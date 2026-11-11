# bentwired on the desktop

How to build, install and run the daemon on KDE Plasma (Wayland), what the automated
desktop checks cover, and what could not be verified on the headless build machine.

## Build and install (user-local)

```sh
cargo build --release -p bentwired -p bentwire
install -Dm755 target/release/bentwired target/release/bentwire -t ~/.local/bin/
# characters: copy or symlink the packs (A-PACK-1)
mkdir -p ~/.local/share/bentwire/characters
cp -r characters/builtin characters/classics ~/.local/share/bentwire/characters/
# D-Bus activation, so `bentwire ...` starts the daemon on demand (B-RUN-1)
install -Dm644 packaging/dbus/org.bentwire.Agent.service -t ~/.local/share/dbus-1/services/
sed -i "s|^Exec=.*|Exec=$HOME/.local/bin/bentwired|" ~/.local/share/dbus-1/services/org.bentwire.Agent.service
```

Optional systemd user unit (start with the session instead of on demand):

```sh
install -Dm644 packaging/systemd/bentwired.service -t ~/.config/systemd/user/
sed -i "s|^ExecStart=.*|ExecStart=$HOME/.local/bin/bentwired|" ~/.config/systemd/user/bentwired.service
systemctl --user daemon-reload && systemctl --user enable --now bentwired
```

Arch packages needed at build time: `rust`, `wayland`, `libxkbcommon`, `alsa-lib`,
`pkgconf`. At run time the daemon needs a compositor with `zwlr_layer_shell_v1` version 4
or later (Plasma 6, sway, Hyprland); without it it exits with a message.

## Run

```sh
bentwired &                       # or rely on D-Bus activation
bentwire show
bentwire play Writing
bentwire say "It looks like your build failed."
bentwire ask "Build failed." --button "Show log" --button "Ignore"
```

Daemon options: `--characters DIR` (extra search root, repeatable), `--no-tray`,
`--no-audio`, `--test-interface` (exports `org.bentwire.Test1` with `Pointer`, `Key`,
`Snapshot`, A-TEST-3). `RUST_LOG=debug` gives more log output. Configuration:
`~/.config/bentwire/config.toml` (B-CFG-1; the menu's "Options..." creates it), reloaded
on `SIGHUP`. State: `~/.local/state/bentwire/state.toml`.

Headless mode for tests: `bentwired --headless [--outputs WxH+X+Y[@S],...] [--seed N |
--draws FILE] [--trace FILE] [--dump-frames DIR]` (T-4, format in spec 3.14 A-DESK-4).

## Automated checks

- `cargo test -p bentwired --test e2e`: D-Bus end to end on a private bus (T-5).
- `scripts/desktop/scenario.sh sway`: headless sway; show, menu, play, say, ask
  (answered through `Test1.Pointer`), moveto, tray (minimal SNI watcher), output scale
  2 and 1.5, output hot-plug pull-back, hide; checks D-Bus signals, what the backend
  requested, and grim screenshots (character, balloons and menu found by colour at the
  expected position).
- `scripts/desktop/scenario.sh kwin`: the same under `kwin_wayland --virtual`. KWin
  cannot take screenshots on this machine (its ScreenShot2 needs an OpenGL render node;
  the VM has none), so the character's geometry is read through KWin scripting
  (`workspace.windowList()`), and balloons and menu through the backend's own record
  (`Test1.Snapshot`). The scale and hot-plug steps run under sway only.

## Not verified headless (please check on Plasma)

The T-6 checklist items that need a real seat, a real panel or a GPU:

- K1 negative margins and positions partly off-output on KWin (sway accepted them).
- K2 the top layer below a full-screen window and above normal windows.
- K3 the menu's popup grab and `popup_done` on an outside click (tests open the menu
  without a grab, since synthetic clicks have no press serial).
- K4 keyboard focus of the ask balloon after a real click (`on_demand` is requested and
  checked, focus itself is not).
- K5 click-through on transparent pixels (the input region is set and checked).
- K6 drag with a real pointer staying 1:1 (margin commits are throttled to frame
  callbacks; only synthetic drags are tested).
- K7 the tray in Plasma's system tray (tested against a minimal watcher: registration,
  Id, Title, dbusmenu labels, Activate).
- K8 output hot-plug on KWin (tested on sway).
- Sound output through a real audio device (sounds are traced with their duration).
- KWin pixels: no screenshot comparison under KWin (see above).
