# Contributing to bentwire

Thanks for your interest. This file covers building, testing and the conventions of the
repository. Read [docs/architecture.md](docs/architecture.md) first for the crates, the
cross-cutting decisions and the clean-room rules, then [docs/spec.md](docs/spec.md) (the
behaviour, with rule IDs) and [docs/character-format.md](docs/character-format.md).

## Contents

- [Setup](#setup)
- [Build, test, lint](#build-test-lint)
- [Real-data tests](#real-data-tests)
- [Client tests](#client-tests)
- [Desktop scenario](#desktop-scenario)
- [Screenshots, GIFs and gallery](#screenshots-gifs-and-gallery)
- [Generated files](#generated-files)
- [Conventions](#conventions)

## Setup

bentwire is a Rust workspace. The minimum Rust version is 1.90 (`rust-version` in
`Cargo.toml`); CI builds with Rust 1.99.1 (`.github/workflows/ci.yml`). Install Rust with
[rustup](https://rustup.rs/) or your distribution's package.

System packages for the build:

```sh
# Debian, Ubuntu
sudo apt-get install libwayland-dev libxkbcommon-dev libasound2-dev pkg-config dbus
# Arch Linux
sudo pacman -S --needed rust base-devel pkgconf wayland libxkbcommon alsa-lib git
```

```sh
git clone https://github.com/mschabhuettl/bentwire
cd bentwire
```

## Build, test, lint

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

These four are what CI runs (with `--locked`), and every commit should pass them. Plain
`cargo test --workspace` needs no original files: it uses synthetic characters from the
`agent-fixtures` crate and the bundled font. It includes:

- the renderer's golden images in `crates/bentwire-render/tests/golden`. After an
  intended rendering change, regenerate them with
  `BENTWIRE_BLESS=1 cargo test -p bentwire-render` and review the changed images before
  committing them;
- the D-Bus end-to-end tests (`cargo test -p bentwired --test e2e`). Each test starts a
  private `dbus-daemon` and builds the `bentwire` CLI itself, so they need `dbus-daemon`
  installed but never touch your session bus.

Parser fuzzing over the malformed-input fixtures (default 1500 rounds per fixture):

```sh
BENTWIRE_FUZZ_ROUNDS=40000 cargo test --release -p agent-formats --test malformed
```

## Real-data tests

Some tests compare bentwire against the original character files and against reference
parsers from the format research. Neither is public, so CI does not run these tests, and
without them you can run the rest of the suite as above. They are `#[ignore]` tests run by
one script:

```sh
BENTWIRE_SOURCES=<dir>[:<dir>...] BENTWIRE_REFERENCE=<dir> scripts/oracle-check.sh
```

The script reads these variables:

| Variable | Meaning |
|---|---|
| `BENTWIRE_SOURCES` | One or more directories, separated by `:`, with your own copies of the original character files and DLLs, in any layout. Files are found by file name and checked by SHA-256 against [docs/sources.md](docs/sources.md). |
| `BENTWIRE_REFERENCE` | A directory with the Python reference parsers and the prototype from the format research, used as test oracle (not public). |
| `BENTWIRE_CLASSICS` | A checkout of [bentwire-classics](https://github.com/mschabhuettl/bentwire-classics). Default: `../bentwire-classics`, next to this checkout. |
| `BENTWIRE_PACKS` | A `characters/` directory with the packs to load. Set by the script (see below); set it yourself only when running the tests without the script. |
| `BENTWIRE_PROTO_COPY` | Optional: an existing scratch copy of the prototype, which keeps its cache between runs. Without it the script makes a fresh copy in a temporary directory and removes it afterwards. |

The script builds a temporary `characters/` directory with `builtin` and `classics` and
points `BENTWIRE_PACKS` at it, for the test that loads every character of every pack through
the daemon's loader and the core. Extra arguments filter the tests:

```sh
scripts/oracle-check.sh library   # every pack character through the daemon's loader and the core
scripts/oracle-check.sh oracle    # the daemon's playback path against interpreter B (T-2) and the B-FB-2 table
```

A missing variable or pack never counts as a pass: an ignored test then fails with
"NOT RUN", and `oracle-check.sh` prints "NOT RUN" and exits with status 77.

The source directories are only read. The real-data tests write a lot of temporary data;
if `/tmp` is a small tmpfs on your machine, point `TMPDIR` at a larger disk, for example
`TMPDIR=~/.cache/bentwire/tmp`.

## Client tests

The shell integration and the Claude Code hooks have their own tests. They need
`dbus-run-session`, `gdbus` and `jq`:

```sh
cargo build -p bentwired -p bentwire && clients/shell/test.sh && clients/claude-code/test.sh
```

## Desktop scenario

`scripts/desktop/scenario.sh` runs the daemon on a real headless compositor: show, play,
say, ask answered over D-Bus, moveto and hide, with signal and screen checks. It needs
`dbus-run-session`, `busctl`, `python3` (with `python3-gi` for the tray checks), and `sway`
with `grim`, or `kwin_wayland`:

```sh
cargo build -p bentwired -p bentwire && cargo build -p bentwired --examples
scripts/desktop/scenario.sh sway
scripts/desktop/scenario.sh kwin
```

Under sway the script also drives a virtual pointer (click, click-through, drag, menu grab),
output scales 2 and 1.5, output hot-plug and the tray against a minimal StatusNotifierItem
watcher. `kwin_wayland --virtual` without an OpenGL render node cannot take screenshots, so
under KWin the scenario reads the character's geometry through KWin scripting and the
balloons and menu through the backend's own record (`Test1.Snapshot`).

On failure the artefacts stay in the temporary directory the script prints;
`BENTWIRE_SCENARIO_KEEP=1` keeps them always. [docs/testing.md](docs/testing.md) and
[docs/desktop.md](docs/desktop.md) describe in detail what the scenario covers.

When stopping a stray daemon by hand, anchor the pattern so that `pkill` does not match
the shell running it: `pkill -f "^$PWD/target/debug/bentwired"`.

## Screenshots, GIFs and gallery

Screenshots and animated GIFs are made by `scripts/screenshots.sh` on a real KDE Plasma 6
desktop, run headless: KWin nested in headless sway, plasmashell with the default Breeze
look and wallpaper, and the bentwire tray icon in the panel. The output is a fixed
1280x720, with seed 1 and a frozen wall clock:

```sh
cargo build -p bentwired -p bentwire && cargo build -p bentwired --examples
scripts/screenshots.sh                                   # builtin only, into docs/screenshots/
scripts/screenshots.sh --classics ../bentwire-classics   # classics, into that checkout's screenshots/
scripts/screenshots.sh --classics ../bentwire-classics --gifs   # GIFs only (same as scripts/gifs.sh)
scripts/screenshots.sh --classics ../bentwire-classics --all    # PNGs and GIFs
```

It needs `sway`, `grim`, `wf-recorder`, `ffmpeg`, `kwin-wayland`, `plasma-desktop`,
`plasma-discover`, `dolphin`, `systemsettings` and `falkon` (they fill the default panel
launchers), `breeze-icon-theme`, `fonts-noto-core`, `kded6`, `faketime`,
`dbus-run-session`, `busctl` and `python3` with PIL. On Ubuntu, the `falkon` package's
install script fails unless `/usr/share/qt5` exists (`sudo mkdir -p /usr/share/qt5`).
Set `TMPDIR` to a directory on disk, since the session and the recordings do not fit a
small tmpfs.

The PNGs are byte-identical on every run. The GIFs are recorded in real time; two runs
give the same frames, but a single frame time can differ by 50 ms.

Images that show Microsoft characters are committed only to bentwire-classics; this
repository's README embeds them by URL.

The gallery images (every character in its rest pose, 1x, transparent background) are
made by `scripts/gallery.sh`, byte-identical on every run. It writes `docs/gallery/` for
the `builtin` pack and `gallery/` of the classics checkout in `BENTWIRE_CLASSICS` (default
`../bentwire-classics`; skipped with a note when it is not there):

```sh
scripts/gallery.sh
```

## Generated files

- `characters/builtin` is generated by `cargo run -p bentwire-convert --bin gen-builtin`.
  A test fails when the committed files are stale, so rerun the generator after changing
  the placeholder character and commit the result.
- The classics pack is generated by `regenerate.sh` in bentwire-classics (see its
  [README](https://github.com/mschabhuettl/bentwire-classics#regenerate)). Test
  `classics_real` requires the regenerated pack to be byte-identical to the checkout, so a
  converter change that alters output needs a regenerated pack in the same release.

## Conventions

- **Rule IDs.** Behaviour is specified in [docs/spec.md](docs/spec.md) with rule IDs such as
  `R-ANIM-2`. Every behaviour test names its rule in the test name or a comment, for
  example `#[test] fn r_anim_2_frame_clock()`. Where the spec leaves a gap, choose a
  sensible behaviour and record it as a bentwire decision in `docs/spec.md` 3.14 and
  [docs/decisions.md](docs/decisions.md), with the reason.
- **Clean room.** bentwire is written from the documented formats and behaviour, not from
  Microsoft code. Follow the clean-room rules in
  [docs/architecture.md](docs/architecture.md): no disassembly or decompilation of Microsoft
  binaries, and no Microsoft or third-party Agent source code.
- **No Microsoft data in this repository.** Character files, images, sounds, palettes, tip
  texts, and golden hashes or images of converted output live only in bentwire-classics.
  Tests use synthetic fixtures (`agent-fixtures`) or hand-made characters. Allowed: the
  provenance SHA-256 of source files in [docs/sources.md](docs/sources.md) and
  `docs/research/`, and the exceptions listed in `docs/architecture.md`.
- **Naming.** Use the product's own names: Microsoft Agent, Office Assistant, Clippit.
- **Commits.** Small commits that each build and pass the checks above. The subject starts
  with the area it touches (`core:`, `wayland:`, `docs:`, `README:`, a crate or a file) and
  says what changed and why, in English. Stage explicit paths. Do not rewrite published
  history.
- **Changelog.** Add user-visible changes under `## [Unreleased]` in
  [CHANGELOG.md](CHANGELOG.md) ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)).
  The release workflow takes the release notes from the `## [x.y.z]` section.
- **Licence.** Contributions are dual-licensed under MIT OR Apache-2.0, like the rest of the
  code ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
