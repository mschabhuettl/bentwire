# Roadmap

What is planned for bentwire, in rough order. Plans change; the GitHub milestones and issues
track the current state.

## Principles

- **Made with AI, no AI inside.** bentwire is developed with AI help but contains none: no
  model, no network service, nothing generative at runtime. Clients that drive bentwire from
  outside (for example coding-assistant hooks) are fine.
- **Keep it simple.** The daemon stays small and does one thing. There is no plugin code in
  the daemon process. The open interface is the D-Bus API and the CLI; integrations are
  separate programs that use only that API.
- **Versatile and scriptable.** bentwire should be usable for anything, so interfaces and
  integrations come before polish.

## Packaging (right after 0.1.1)

- AUR packages `bentwire` (from source) and `bentwire-bin` (from the release), maybe
  `bentwire-git`. The PKGBUILDs live in `packaging/aur/`.

## 0.1.2

- Fixes from testing on a real KDE Plasma desktop: real mouse under KWin, tray, sound,
  HiDPI, fullscreen windows, multiple screens.
- Keep an eye on the shell-integration test that failed once.

## 0.2 "Everyday use"

- `bentwire characters install classics`, which downloads the classics release.
- **Notification bridge:** optionally reads desktop notifications and lets the character
  say them, with a per-app filter.
- **MQTT bridge `bentwire-mqtt`:** maps topics to say, play, ask, show and hide, and
  publishes answers (`button:<n>`) back, with Home Assistant MQTT discovery. Example Home
  Assistant automations go into the docs. It works with Node-RED and others too.
- **Developer docs:** a D-Bus reference with the introspection XML, every signal
  documented, short examples in shell, Python, Rust and JavaScript, and a template for your
  own bridge.
- **More ready-made clients:** a shell hook on failure or "command not found", git hooks,
  systemd failures.
- **Faster first frame:** cache the decoded sheets in `~/.cache/bentwire/` and load them
  with mmap, preload the default character at daemon start, and put the frames of the Show
  animation first in the sheet.
- A test for the time-of-day query of Office 97 animations.

## 0.3 "It speaks"

- **Speech output with lip sync** through espeak-ng, which is small, has no AI and sounds
  like 1999. The mouth is driven by the existing lip-sync table.
- **Add-ons:** an add-on manifest (`addon.toml`: name, kind, version, command) and
  `bentwire addons list/install/enable/disable`. Add-ons are characters, balloon themes
  and bridges. Enabled bridges run as their own processes and stop with the daemon.

## 0.4 "Polish and reach"

- The Office balloon theme proven from the original: the order of the button states and
  the balloon size constants.
- Optional smoothing for vector characters.
- A slimmer classics pack.
- Flatpak.

## Own characters (after 0.2)

- Free (CC0) characters made by code, with no image generators. Each character is
  described once as a vector rig and rendered in two styles: retro pixel art (palette, hard
  edges) and a modern smooth look (anti-aliased, alpha, 1x and 2x).
- The first is **Wire**, a piece of bent wire. It replaces the placeholder in `builtin`.
- Ideas after that: Quack (a debugging duck), Hermit (a hermit crab, "shell"), Greybeard
  (an old sysadmin), Blink (a cursor), Cron (an alarm clock), Floppy.
- Later, a separate `mascots` pack with free mascots of free-software projects (for example
  Ferris, Xenia, Tux, Konqi, the Go gopher, Pepper & Carrot), each under its own licence.

## Deliberately not

- Speech recognition.
- AI in the core.
- X11.
- A GNOME extension comes last (GNOME has no layer-shell).
