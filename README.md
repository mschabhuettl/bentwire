<div align="center">

# bentwire

**The animated desktop characters of Microsoft Agent and the Office Assistant, on Linux Wayland.**

[![CI](https://github.com/mschabhuettl/bentwire/actions/workflows/ci.yml/badge.svg)](https://github.com/mschabhuettl/bentwire/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/mschabhuettl/bentwire)](https://github.com/mschabhuettl/bentwire/releases/latest)
[![Licence: MIT OR Apache-2.0](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue)](#licences)

<img src="https://raw.githubusercontent.com/mschabhuettl/bentwire-classics/main/screenshots/ask.png" alt="Clippit asks &quot;Build failed&quot; in the Office balloon, with the buttons Show log and Ignore">

</div>

bentwire brings the animated desktop characters of Microsoft Agent and the Office Assistant
to the Linux Wayland desktop. A small daemon shows one character above your windows that
plays its animations, speaks in a word balloon and asks questions in an Office-style
balloon. Scripts, your shell and Claude Code drive it over D-Bus or through the `bentwire`
command.

bentwire reads the original character files (Agent 2.0 ACS, ACF/ACA web characters, Agent
1.5, Office 97 and Microsoft Bob ACT actors) and converts them into its own open format. It
ships no Microsoft characters: they live in the separate
[bentwire-classics](https://github.com/mschabhuettl/bentwire-classics) repository.

## Features

- **The original characters.** Reads Agent 2.0 ACS, ACF/ACA web characters, Agent 1.5,
  Office 97 and Microsoft Bob ACT actors and converts them into an open format (a folder
  or a single `.bwc` file).
- **Faithful playback.** Branching animations, idle behaviour, sounds, the Agent word
  balloon, and the Office balloon with buttons, labels, check boxes and the Office XP
  theme.
- **At home on the desktop.** Wayland layer shell (Plasma 6, sway, Hyprland), click-through
  on transparent pixels, dragging, the right-click menu with Choose Assistant..., a tray
  icon, HiDPI and fractional scaling, English and German.
- **Scriptable.** A D-Bus API (`org.bentwire.Agent1`) with D-Bus activation, and the
  `bentwire` command line client.
- **Integrations.** Reacts to your bash or zsh commands and to Claude Code sessions.
- **Clean room.** Written from documented formats and observed runtime behaviour; no
  Microsoft code or data in this repository.

## Quick start

```sh
git clone https://github.com/mschabhuettl/bentwire && cd bentwire
cargo build --release -p bentwired -p bentwire
git clone https://github.com/mschabhuettl/bentwire-classics ~/.local/share/bentwire/characters/classics
target/release/bentwired &
target/release/bentwire show
target/release/bentwire say "It looks like you're writing a letter."
```

For a permanent install with D-Bus activation, see [Install](#install).

## Contents

- [Install](#install)
- [Usage](#usage)
- [Integration](#integration)
- [Configuration](#configuration)
- [Own character files](#own-character-files)
- [The characters, and a note to Microsoft](#the-characters-and-a-note-to-microsoft)
- [Known gaps](#known-gaps)
- [Documentation](#documentation)
- [Licences](#licences)

## Install

bentwire needs a Wayland compositor with `zwlr_layer_shell_v1` version 4 or later (Plasma 6,
sway, Hyprland; without it the daemon exits with a message) and a session bus. The tray
icon needs a StatusNotifierItem host (Plasma's system tray).

### Arch Linux

There is no package yet. Install the build dependencies and build from source as below:

```sh
sudo pacman -S --needed rust base-devel pkgconf wayland libxkbcommon alsa-lib git
```

(`rustup` works instead of `rust`.)

### From source

```sh
git clone https://github.com/mschabhuettl/bentwire
cd bentwire
cargo build --release -p bentwired -p bentwire
install -Dm755 target/release/bentwired target/release/bentwire -t ~/.local/bin/

# The placeholder character.
mkdir -p ~/.local/share/bentwire/characters
cp -r characters/builtin ~/.local/share/bentwire/characters/

# D-Bus activation, so that `bentwire ...` starts the daemon on demand.
install -Dm644 packaging/dbus/org.bentwire.Agent.service -t ~/.local/share/dbus-1/services/
sed -i "s|^Exec=.*|Exec=$HOME/.local/bin/bentwired|" ~/.local/share/dbus-1/services/org.bentwire.Agent.service
```

Optional: start the daemon with the session instead of on demand.

```sh
install -Dm644 packaging/systemd/bentwired.service -t ~/.config/systemd/user/
sed -i "s|^ExecStart=.*|ExecStart=$HOME/.local/bin/bentwired|" ~/.config/systemd/user/bentwired.service
systemctl --user daemon-reload && systemctl --user enable --now bentwired
```

Release binaries for x86_64 Linux are attached to each
[GitHub release](https://github.com/mschabhuettl/bentwire/releases).

### The classics pack

The classic characters (Clippit, Merlin, Rocky, the Office 97 and Microsoft Bob actors and
more, 61 in all, most of them Microsoft's) and the Office balloon theme come as a separate
pack:

```sh
git clone https://github.com/mschabhuettl/bentwire-classics ~/.local/share/bentwire/characters/classics
```

See the [gallery](https://github.com/mschabhuettl/bentwire-classics#gallery) of all of them.
bentwire grants no licence for these files; see the pack's README. With only `builtin`
installed, the daemon shows `bentwire-placeholder`
(<img src="docs/gallery/bentwire-placeholder.png" height="24" alt="the placeholder character">,
[docs/gallery/](docs/gallery/)). With `classics` installed, the default
is `clippit`.

<details>
<summary>Where the daemon looks for characters</summary>

Characters are searched in these roots, in order: each entry of the config key
`character_paths`, `$XDG_DATA_HOME/bentwire` (normally `~/.local/share/bentwire`), then
each `$XDG_DATA_DIRS` entry plus `/bentwire`. Inside a root, `characters/` holds
characters (a folder with `character.toml`, or a `.bwc` file) and packs (a folder with
`pack.toml`). The first character with a given id wins.

</details>

## Usage

```sh
bentwire show
bentwire play Writing
bentwire say "It looks like your build failed."
bentwire ask "Build failed." --button "Show log" --button "Ignore"
bentwire characters            # installed characters
bentwire select rocky
bentwire hide
```

Right-click the character for its menu: Hide, Options... (creates and opens the config
file), Choose Assistant... (a gallery balloon to browse and pick a character) and
Animate!. Drag it with the left or right mouse button. A left click on the tray icon shows
a hidden character. Every command and option is in [docs/usage.md](docs/usage.md).

## Integration

- **Shell** (bash and zsh): the character reacts to failed and long-running commands. Add
  `. /path/to/bentwire/clients/shell/bentwire.sh` to `~/.bashrc` or `~/.zshrc`. Details in
  [clients/shell/README.md](clients/shell/README.md).
- **Claude Code hooks**: thinking, writing, attention and done animations. Details in
  [clients/claude-code/README.md](clients/claude-code/README.md).

Both use only the `bentwire` CLI and never start the daemon themselves.

## Configuration

`~/.config/bentwire/config.toml` (`$XDG_CONFIG_HOME`). All keys are optional. The
menu's Options... writes a commented default file if none exists. Send `SIGHUP` to
`bentwired` to reload it. A file that fails to parse keeps the previous config, and
unknown keys are ignored.

| Key | Default | Meaning |
|---|---|---|
| `character` | unset | Character id to start with (after the state file) |
| `language` | `"auto"` | `auto`, `de` or `en` (menu, tray, balloon buttons, names); `auto` reads `LC_ALL`, `LC_MESSAGES`, `LANG` |
| `sound_effects` | `true` | Play character sounds |
| `balloon` | `true` | Show the word balloon for `say` |
| `balloon_font` | `""` | One font for every character's balloon; empty = each character's own |
| `speaking_speed` | `1.0` | Word pacing factor, 0.5 to 1.5 |
| `idle` | `true` | Idle animations on at start |
| `double_click_ms` | `500` | Double-click interval |
| `character_paths` | `[]` | Extra character search roots, searched first |

State (character, position, visibility) is kept in `~/.local/state/bentwire/state.toml`.

**Balloon font.** The word balloon of `say` uses the character's own font: "MS Sans Serif"
13 px for the Microsoft Agent characters; Office 97 actors name no font and get the same
default. bentwire looks that family up through fontconfig, which falls back to `sans-serif`
(DejaVu Sans on most Linux systems) when it is not installed. The Office balloon (`ask`)
and the menu always use the bundled Tahoma (Wine's free replacement), so mixed fonts are
expected. To make the word balloon look like the original, set one font for all
characters: `balloon_font = "Tahoma"` uses the bundled font and needs no installed fonts
(any fontconfig family works too). The daemon rereads the file on `SIGHUP` or at the next
start.

**Requests run one after another.** `show`, `hide`, `play`, `say`, `ask`, `moveto` and
`gestureat` go into one queue per character, as in Microsoft Agent. An `ask` sent while a
`say` is still speaking waits until the text has been fully shown, then plays the ask's
animation (default `Explain`, `--animation ""` for none) and opens the Office balloon. The
word balloon hides on its own 1.5 s after the text ends, so both balloons can be visible
for a moment. While the Office balloon is open the queue goes on, but a later `say` or
`ask` waits until the balloon is answered (with `--mode autodown` the open balloon is
dismissed instead). `bentwire stop --all` drops everything still queued.

## Own character files

```sh
bentwire convert MERLIN.ACS ~/.local/share/bentwire/characters/merlin.bwc
```

The format is detected from the file content. A destination ending in `.bwc` gives a
single package file; any other destination gives a folder. Supported sources:

| Source | Give `convert` | Notes |
|---|---|---|
| Agent 2.0 character (`.acs`) | the `.acs` | |
| Agent 2.0 web character (`.acf` + `.aca`) | the `.acf` | its `.aca` animation files must sit next to it (names matched case-insensitively); missing ones are listed, not invented |
| Agent 1.5 character (OLE `.acs`) | the `.acs` | damaged animation streams are reported per animation |
| Agent 1.5 web character (`.acf` + `.aaf`) | the `.acf` | as for 2.0 web characters |
| Office 97 / Microsoft Bob actor (`.act`), vector | the `.act` | rasterised at 1x and 2x |
| Office 97 / Microsoft Bob actor (`.act`), bitmap | the `.act` plus `--palette FILE` | the Office palette is read from an Office `.acs` file or `MSO97.DLL` |

For ACT actors, `--lang TAG` sets the language of the name (default `en`) and
`--codepage LABEL` the codepage of names and strings (default `windows-1252`; for example
`shift_jis` for Japanese actors). Conversion fails loudly on anything it cannot represent
instead of guessing. The converted character keeps its source file name and SHA-256 in
`[source]`.

To rebuild the whole `classics` pack from the original files, see the
[bentwire-classics README](https://github.com/mschabhuettl/bentwire-classics#regenerate).

## The characters, and a note to Microsoft

bentwire's code is its own; the classic characters are not, and they live in the separate
[bentwire-classics](https://github.com/mschabhuettl/bentwire-classics) repository.

> **Microsoft, please don't sue me ❤️**
>
> bentwire is a private, non-commercial fun project made out of affection for Clippy and
> his friends. I make no money from it and have no commercial interest in it. bentwire is
> not affiliated with or endorsed by Microsoft. The full note, with the trademarks and how
> to ask for a removal, is at the top of the
> [bentwire-classics README](https://github.com/mschabhuettl/bentwire-classics#readme).

## Known gaps

Not verified on a real Plasma desktop yet. Under headless sway a virtual pointer checked: a click on an opaque pixel, click-through
on a transparent one, a drag staying 1:1, and the right-click menu with a real popup
grab and its dismissal by an outside click. Under KWin these ran only through `Test1`.
Please check on Plasma:

<details>
<summary>The open points</summary>


- Negative margins and positions partly off the output (requested and recorded, not
  seen on screen under KWin).
- The character's layer below a full-screen window and above normal windows.
- Clicks, drags and the menu grab with a real mouse on KWin.
- Keyboard focus of the ask balloon after a real click (`on_demand` is requested and
  checked, the focus itself is not).
- The tray in Plasma's system tray (tested only against a minimal watcher: registration,
  Id, Title, menu labels, a menu click, Activate).
- Output hot-plug on KWin (tested on sway).
- Sound through a real audio device (headless runs log sounds with their duration).
- Pixels under KWin: no screenshot comparison was possible there.
- Fractional scale (1.5, through viewporter) and buffer scale 2: only the requests were
  checked, and sway accepted them. The 2x output pixels were not compared, and these
  steps did not run under KWin.
- Multi-monitor setups: covered only by a second headless output under sway and its
  removal.
- `hidpi` sheets on a real 2x output.
- Options... opening the config file through a real `xdg-open` handler (tested with a
  fake `xdg-open`, which shows that the file is written).
- The tray icon image as Plasma draws it (registration and properties were tested, not
  the picture).

</details>

## Documentation

- [docs/usage.md](docs/usage.md): daemon options and the full CLI reference.
- [docs/desktop.md](docs/desktop.md): the daemon on the desktop, and what the automated
  desktop checks cover.
- [docs/testing.md](docs/testing.md): tests, real-data tests and the desktop scenario.
- [docs/spec.md](docs/spec.md): the behaviour, derived from clean-room research of the
  original runtimes.
- [docs/character-format.md](docs/character-format.md): the bentwire character format.
- [docs/research/](docs/research/): the format and runtime research.
- [docs/architecture.md](docs/architecture.md): crates, ownership, clean-room rules.
- [docs/decisions.md](docs/decisions.md): the choices bentwire made where the research left
  a gap.
- [docs/sources.md](docs/sources.md): every source file and analysed binary.
- [docs/credits.md](docs/credits.md): prior work, the bundled font, crate licences.
- [docs/roadmap.md](docs/roadmap.md): what is planned next.
- [CONTRIBUTING.md](CONTRIBUTING.md): build, tests, real-data test setup, conventions.
- [CHANGELOG.md](CHANGELOG.md).

## Licences

- Code: MIT OR Apache-2.0 ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
- `characters/builtin`: CC0-1.0, generated by bentwire's own code.
- `assets/fonts`: Wine's Tahoma replacement, LGPL-2.1 or later
  ([assets/fonts/README.md](assets/fonts/README.md)).
- The classics pack is not part of this repository. bentwire grants no licence for it.
