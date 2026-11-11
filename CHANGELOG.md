# Changelog

All notable changes to bentwire are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and bentwire uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). bentwire stays at 0.x until the
D-Bus API (`org.bentwire.Agent1`) and the character format (`format = 1`) are frozen.

## [Unreleased]

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

[Unreleased]: https://github.com/mschabhuettl/bentwire/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/mschabhuettl/bentwire/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/mschabhuettl/bentwire/releases/tag/v0.1.0
