# Changelog

All notable changes to bentwire are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and bentwire uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). bentwire stays at 0.x until the
D-Bus API (`org.bentwire.Agent1`) and the character format (`format = 1`) are frozen.

## [Unreleased]

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

[Unreleased]: https://github.com/mschabhuettl/bentwire/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mschabhuettl/bentwire/releases/tag/v0.1.0
