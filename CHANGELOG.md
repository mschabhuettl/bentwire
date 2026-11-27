# Changelog

All notable changes to bentwire are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and bentwire uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). bentwire stays at 0.x until the
D-Bus API (`org.bentwire.Agent1`) and the character format (`format = 1`) are frozen.

## [Unreleased]

## [0.6.0] - 2026-11-27

### Changed

- The Office balloon follows the original more closely, now proven from Office XP's
  balloon library (docs/research/office-assistant.md): its size comes from the proven area
  terms, minimum and growth steps, a refused width widens until the longest word fits (up
  to the free space), and portrait and landscape are chosen by the original tie rule. Most
  balloons get somewhat narrower.
- Office balloon theme: buttons and bullets use the proven state order (normal, hot,
  pressing, pressed, disabled); a focused button keeps its picture and shows a focus
  rectangle around its caption; a disabled caption is grey #B2B2B2. Themes extracted by
  older converters still load. `bentwire-convert --theme` writes the arrows as 5 frames of
  17x9.
- The converter writes smaller PNGs (the smallest indexed bit depth, libdeflate at level
  12) with the same pixels; the builtin characters shrink by 4 to 11 %, the classics pack
  2026.11.3 from 91 MB to 71 MB installed.
- `addons`: no warning when the daemon is slow to answer `Reload`; a warning when the
  session bus itself does not answer.
- The "espeak-ng not available" reason stays on one log line.
- `scripts/screenshots.sh` also draws the README picture of Quack.

### Fixed

- `addons install`: the tar header limit is 1 MiB (pax data of real archives can exceed
  64 KiB), and the error names the limit.

## [0.5.1] - 2026-11-27

### Fixed

- The test for the 5 s `Reload` timeout no longer counts the first build of the CLI, which
  made `check()` of the AUR source package fail for 0.5.0.

## [0.5.0] - 2026-11-27

### Added

- Quack, a rubber debugging duck, bentwire's second own character (CC0-1.0), drawn by
  code like Wire: `quack` (smooth, 1x and 2x) and `quack-retro` (12 colours, hard edges).
  Same animation set as Wire; it wobbles on the water when idle and squeaks on Alert and
  GetAttention. Wire stays the default.
- Smooth edges (off by default): vector characters such as the Office 97 and Bob actors
  are drawn anti-aliased from their 2x pictures, at size 1 on a 1x screen. Tray item
  "Smooth Edges", `bentwire smooth on|off`, config key `smooth`, D-Bus property `Smooth`;
  the choice is remembered.

### Changed

- A position saved for a smaller character (the 96 px placeholder of 0.3) or on an
  output that is gone is pulled inside an output at start.
- `addons`: the CLI waits at most 5 s for the daemon's `Reload` and says so when it does
  not answer.
- A missing espeak-ng-data no longer prints espeak-ng's error on every daemon start; the
  reason is part of the once-per-run "espeak-ng not available" line.
- A hand-copied `bentwire-placeholder` is reported once in the log as removable.

### Fixed

- `addons install`: the headers of a tar entry (GNU long names, pax extensions) are
  limited to 64 KiB, so a crafted archive cannot make the CLI hold up to 1 GiB in memory.

## [0.4.0] - 2026-11-27

### Added

- Wire, bentwire's own character (CC0-1.0): a piece of bent wire with a face, drawn by
  code from one vector rig. Two renderings: `wire` (smooth, 1x and 2x, the default) and
  `wire-retro` (11 colours, hard edges). It has the Agent states and animation set, the
  Office animation names, return animations and exit branches, and mouths for speech
  output.
- D-Bus method `org.bentwire.Agent1.Reload`, which does what SIGHUP does. After an add-on
  change the CLI calls it instead of signalling a looked-up process id.

### Changed

- Wire replaces `bentwire-placeholder` in `builtin`. With only `builtin` installed, the
  default character is `wire`.
- `addons install`: archives and downloads are limited to 256 MiB packed, 1 GiB unpacked
  and 10000 entries, checked while unpacking.
- espeak-ng is loaded and initialised at daemon start, before any other thread, because
  it sets the process locale. After installing espeak-ng, restart the daemon.
- Docs: the SAPI 4 reading of voice gender and pitch is marked as inferred.

### Fixed

- Quit, SIGTERM and the end of the session bus stop speech and sounds before the daemon
  waits for its bridges.

## [0.3.0] - 2026-11-27

### Added

- Speech output with espeak-ng: `say` texts and `ask` headings and texts are spoken and
  the mouth follows the phonemes. Off by default; `bentwire speech on`, the tray item
  "Speech", the D-Bus property `Speech` or `speech = true`. `bentwire say --speak` /
  `--no-speak` decides per text (`SpeakWith`). espeak-ng is optional and loaded at run
  time.
- Add-ons: `bentwire addons list|install|enable|disable|remove` installs characters,
  balloon themes and bridges from a folder, a `.zip` or `.tar.gz`, or an HTTPS URL
  (`--sha256`). Enabled bridges run as their own processes, are restarted with backoff
  when they exit and stop with the daemon; an install never enables a bridge. Format in
  `docs/addon-format.md`.

### Fixed

- Programs the daemon starts (`xdg-open` for Options...) no longer inherit blocked
  signals.

## [0.2.2] - 2026-11-27

### Changed

- Faster start: the daemon keeps the characters it found in
  `~/.cache/bentwire/discovery.json` and reads only manifests that changed since the last
  start (about 120 ms less with the classics pack).

## [0.2.1] - 2026-11-21

### Fixed

- After Animate! a notification or any other client request no longer waits forever
  behind a looping animation: the client request ends the Animate! play.
- Gallery: Microsoft Bob greetings with a user name placeholder and the licence string of
  Agent 1.5 characters are no longer shown as intro phrases.
- Gallery: the Choose Assistant balloon no longer jumps while browsing when the character
  stands in the middle or upper part of the screen or at a size factor of 3 or 4, no longer
  moves when the pointer touches a button, reopens where it was closed, and after a size
  change moves once and then stays.

## [0.2.0] - 2026-11-15

### Added

- The Choose Assistant gallery shows one of the character's intro phrases (its ExtraData,
  or the greeting lines of an Office 97 actor) instead of the description, a new one each
  time the character is shown. Characters without them keep the description.
- `bentwire-notify`, an optional notification bridge: the character says desktop
  notifications, filtered per app with `--only` and `--ignore` or
  `~/.config/bentwire/notify.toml`, with an optional systemd user unit.
- `bentwire-mqtt`, an MQTT bridge: topics for say, play, show, hide and ask, answers
  (`button:<n>`) and the visibility state published back, Home Assistant discovery and
  example automations, an optional systemd user unit.
- Faster first frame: decoded character sheets are cached in `~/.cache/bentwire/sheets/`
  (at most 512 MiB) and mapped on load, and the startup character loads while the daemon
  starts.
- `docs/dbus.md`: a developer reference of the D-Bus API with the introspection XML
  (`docs/dbus/org.bentwire.Agent.xml`, kept current by a test), examples in shell,
  Python, Rust and JavaScript, and a template for your own bridge.
- More ready-made clients: an opt-in command-not-found handler for bash and zsh
  (`BENTWIRE_SHELL_NOT_FOUND=1`; keeps the usual message and status 127 and chains to an
  existing handler), git hooks in `clients/git/` and a systemd `OnFailure=` template in
  `clients/systemd/` that says which unit failed.

### Fixed

- Dragging across an output edge keeps the character under the cursor when KWin's edge
  barrier holds the cursor for part of a mouse movement. Before, it ended up to 20 px off
  after a horizontal crossing and up to the barrier's width after a slightly diagonal
  one. The same holds at the layout edge under sway.
- D-Bus: the introspection names every argument as the spec does; `Set` of an unknown
  property fails with `UnknownProperty` and `GetAll` of an unknown interface with
  `UnknownInterface`.
- Sound: a PipeWire stream that reports an error after its format was negotiated falls
  back to ALSA instead of staying silent.

## [0.1.3] - 2026-11-15

### Fixed

- Sound works on PulseAudio and PipeWire desktops without pipewire-alsa or
  pulseaudio-alsa. bentwired now plays through the running sound server: the PulseAudio
  protocol first (PulseAudio, or PipeWire with pipewire-pulse), then PipeWire's own
  protocol, then ALSA's default device. The client libraries are loaded at run time and
  are not package dependencies.
- The log says once which audio backend is used (`audio: pulse`, `audio: pipewire`,
  `audio: alsa <device>`), and once when one fails and the next is tried.
- ALSA's device list is read only when the default device fails, so the journal no longer
  fills with JACK, OSS and dsnoop errors on every sound.
- Drag: a drag that starts on a fractionally scaled output (for example 1.5) no longer
  lags behind the cursor. The character is drawn once per batch of pointer events and
  only moved while its picture is unchanged.
- Choose Assistant: the balloon keeps its size and place while browsing, so Back and Next
  stay under the mouse, also in a screen corner and at size 3x.
- First start: the "Install more characters" hint stays until you click the character.
- `bentwire characters install classics` no longer prints curl progress bars; it prints
  one line for the download.

## [0.1.2] - 2026-11-12

### Added

- Arch Linux packages on the AUR, with the PKGBUILDs in `packaging/aur/`: `bentwire` (from
  source), `bentwire-bin` (the release binaries) and `bentwire-git` (`main`). They install
  the binaries, the placeholder character under `/usr/share/bentwire/characters/`, the D-Bus
  activation file and the systemd user unit (not enabled).
- `bentwire characters install classics` downloads the latest bentwire-classics release,
  checks it against its `sha256sums.txt` and installs it into
  `~/.local/share/bentwire/characters/classics`. Run again, it updates; a git checkout there
  is left alone.
- On its first start with only the placeholder installed, the placeholder says once how to
  get the classic characters.
- `bentwire select` with an unknown id suggests close ids, and the install command when the
  classics are missing.
- Character names come with their product, for example "Clippit (Office 97)" and
  "Merlin (Microsoft Agent 1.5)", the same in `bentwire characters`, Choose Assistant, the
  tray menu and the classics gallery. A name is never empty (A-LABEL-1).
- Character size: 1x, 2x, 3x or 4x, from the new "Size" submenu of the character menu and
  the tray, `bentwire scale <n>`, the config key `scale`, or the D-Bus property
  `org.bentwire.Agent1.Scale`. Bitmap characters scale pixel-exact; the Office 97 vector
  actors use their 2x drawings. Input region, drag, `moveto`, `gestureat` and balloon
  placement follow the size; balloons keep their size. A change keeps the character's
  bottom centre, applies at once and is remembered across restarts. `bentwire scale`
  without an argument prints the current factor.
- The converter can take a palette from a sibling character with the same pictures
  (`palette-from` in the classics `options.txt`, A-CONV-7).

### Changed

- Characters installed while the daemon runs are found without a restart (`bentwire
  characters`, `select`, Choose Assistant).
- Until you choose a character (`select` or Choose Assistant's OK), the default follows what
  is installed: `clippit` when the classics are there, else the placeholder. A choice is
  kept. State files from 0.1.1 keep their character unless it is the placeholder.
- Without a remembered position, the character first appears on the primary output (KDE
  Plasma), else on the output the compositor picks for new surfaces, else on the leftmost
  output.
- Sound: the audio device is opened only while a sound plays, so PipeWire can suspend the
  sink, and PipeWire's ALSA device is used when PipeWire runs.
- CI uses `actions/checkout` v7 (v4 ran on the deprecated Node 20).

### Fixed

- Switching characters (`select`, Choose Assistant's Back and Next) keeps the character
  standing on the same spot instead of keeping its top-left corner, so it no longer jumps.
  The gallery balloon stays in place while it still fits next to the new character.
- Dragging the character from one monitor to another on KDE Plasma no longer stalls and then
  jumps ahead of the cursor: relative pointer motion held back by KWin's edge barrier is
  ignored.
- When the session bus goes away (logout), bentwired saves its state and exits instead of
  running on; the tray no longer panics on exit ("all futures in select! were completed").
- Sound recovers after PipeWire restarts or the audio device fails.
- Without pipewire-alsa, bentwired no longer holds the sound card and so no longer silences
  other programs.
- A broken ALSA stream (after an underrun, suspend or device removal) no longer spins a CPU
  core and floods the journal with "audio stream error" lines.
- A stuck audio device can no longer delay bentwired's exit on SIGTERM.

## [0.1.1] - 2026-11-11

A cleanup release: no new daemon features. The matching classics pack is
[bentwire-classics 2026.11.1](https://github.com/mschabhuettl/bentwire-classics/releases/tag/2026.11.1).

### Added

- [CONTRIBUTING.md](https://github.com/mschabhuettl/bentwire/blob/v0.1.1/CONTRIBUTING.md):
  build, tests, the real-data test setup, the desktop scenario, screenshots and conventions.
- A gallery: `scripts/gallery.sh` renders every character in its rest pose at 1x on a
  transparent background, byte-identical on every run; `docs/gallery/` holds the `builtin`
  pack's image.
- Animated GIFs: `scripts/screenshots.sh --gifs` (or `scripts/gifs.sh`) records the
  character appearing, `say`, `ask` with a click, the menu, a drag and a tour, and `--all`
  makes PNGs and GIFs.
- `docs/roadmap.md` is now the full public roadmap: the principles (made with AI, no AI
  inside; a small daemon whose open interface is the D-Bus API and the CLI) and the plan
  for packaging, 0.1.2, 0.2, 0.3, 0.4 and own characters. The README links it and says
  "Built with AI assistance. Contains no AI."
- `docs/sources.md` lists the sources of the new classics characters Genie, Peedy, Robby
  (Agent 2.0) and Ozzar (Agent 1.5).

### Changed

- **Breaking for the real-data test setup:** the real-data tests, `scripts/oracle-check.sh`
  and the classics `regenerate.sh` take the original files from `BENTWIRE_SOURCES` (one or
  more directories with your own copies, in any layout, found by file name and checked by
  SHA-256) and the reference parsers from `BENTWIRE_REFERENCE`. The two variables of 0.1.0
  are gone.
- The repository history was rewritten once before republishing: private traces and
  internal planning files were removed from every commit. `docs/sources.md` and
  `docs/research/` now give product, version, file name, size and SHA-256 of each source
  file, and no download locations.
- `scripts/screenshots.sh` runs a real KDE Plasma 6 session headless (KWin nested in
  headless sway, the default Breeze look and wallpaper, the tray icon in the panel)
  instead of a plain sway desktop. `docs/credits.md` credits the KDE artwork.
- The README follows a new layout (header with badges, features, quick start, contents),
  with a short note to Microsoft.

### Fixed

- Animations listed in a character's `source.missing_animations` were removed from its
  animations but stayed members of its states, so the daemon treated such a character
  differently from one that never had them. They are now dropped from the states too, and
  a state left empty is dropped.
- Dragging the character works when the compositor delivers no relative-pointer events
  (seen under nested KWin): `bentwired` falls back to absolute pointer motion.

## [0.1.0] - 2026-11-11

First release.

### Added

- `bentwired`, a daemon that shows one animated character above the windows of a Wayland
  desktop (layer shell, Plasma 6, sway, Hyprland): animations, idle behaviour, the Agent word
  balloon, the Office balloon with buttons, labels and check boxes, the right-click menu
  (Hide, Options..., Choose Assistant..., Animate!), dragging, a tray icon, sound, HiDPI and
  fractional scaling, English and German.
- D-Bus API `org.bentwire.Agent1` on the bus name `org.bentwire.Agent`, with D-Bus
  activation and an optional systemd user unit.
- `bentwire`, a command line client for the D-Bus API (`show`, `hide`, `play`, `say`, `ask`,
  `moveto`, `gestureat`, `stop`, `characters`, `select`, `animations`, `status`, `quit`)
  and `bentwire convert` for local conversion.
- An open character format (`docs/character-format.md`): a folder with `character.toml`
  and PNG sheets, or a single `.bwc` file.
- Converters from Agent 2.0 ACS, Agent 2.0 web characters (ACF + ACA), Agent 1.5 (OLE ACS,
  ACF + AAF) and Office 97 / Microsoft Bob actors (ACT, bitmap and vector).
- Character packs (`pack.toml`) and search roots; the `builtin` pack with a CC0 placeholder
  character. Microsoft's characters are in the separate `bentwire-classics` repository.
- Shell integration for bash and zsh, and Claude Code hooks (`clients/`).
- Configuration file `~/.config/bentwire/config.toml`, reloaded on `SIGHUP`.
- Documentation: behaviour spec, format research, sources, credits, decisions, roadmap.

[Unreleased]: https://github.com/mschabhuettl/bentwire/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/mschabhuettl/bentwire/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/mschabhuettl/bentwire/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/mschabhuettl/bentwire/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/mschabhuettl/bentwire/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/mschabhuettl/bentwire/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/mschabhuettl/bentwire/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/mschabhuettl/bentwire/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/mschabhuettl/bentwire/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/mschabhuettl/bentwire/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/mschabhuettl/bentwire/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/mschabhuettl/bentwire/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/mschabhuettl/bentwire/releases/tag/v0.1.0
