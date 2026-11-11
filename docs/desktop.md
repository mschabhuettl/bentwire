# bentwired on the desktop

How to build, install and run the daemon on KDE Plasma (Wayland), what the automated
desktop checks cover, and what could not be verified on the headless build machine.

## Build and install (user-local)

```sh
cargo build --release -p bentwired -p bentwire
install -Dm755 target/release/bentwired target/release/bentwire -t ~/.local/bin/
# characters: copy or symlink the packs (A-PACK-1); classics is its own repository
mkdir -p ~/.local/share/bentwire/characters
cp -r characters/builtin ~/.local/share/bentwire/characters/
git clone https://github.com/mschabhuettl/bentwire-classics ~/.local/share/bentwire/characters/classics
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
- `scripts/desktop/scenario.sh sway`: headless sway; real-pointer click, click-through,
  drag and menu grab (virtual pointer), show, menu, play, say, ask
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

Checked under headless sway with a virtual pointer (`zwlr_virtual_pointer_v1`, the
`vptr` example): click on an opaque pixel, click-through on a transparent one (K5), a
drag staying 1:1 (K6), the right-click menu with a real popup grab and its dismissal by
an outside click (K3). Under KWin these run only through `Test1` (no virtual pointer
there), so on Plasma please check:

- K1 negative margins and positions partly off-output (requested and recorded, not seen
  on screen under KWin).
- K2 the top layer below a full-screen window and above normal windows.
- K3 and K6 with a real mouse on KWin (drags use relative-pointer deltas during the
  implicit grab, so the result does not depend on how the compositor reports positions
  on a moving surface).
- K4 keyboard focus of the ask balloon after a real click (`on_demand` is requested and
  checked, focus itself is not).
- K7 the tray in Plasma's system tray (tested against a minimal watcher: registration,
  Id, Title, dbusmenu labels, a menu click, Activate).
- K8 output hot-plug on KWin (tested on sway).
- Sound output through a real audio device (sounds are traced with their duration).
- KWin pixels: no screenshot comparison under KWin (see above).
