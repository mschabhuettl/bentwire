# Testing

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Plain `cargo test` uses only synthetic characters and the bundled font. It includes the
renderer's golden images (`crates/bentwire-render/tests/golden`; regenerate with
`BENTWIRE_BLESS=1`) and the D-Bus end-to-end tests on a private bus
(`cargo test -p bentwired --test e2e`).

Real-data tests compare against the original files and the research oracle. The files
are not public, so CI does not run these tests. They need `BENTWIRE_REFERENCE` (the research
tree), `BENTWIRE_SOURCES` (the owner's files) and the classics pack in `BENTWIRE_CLASSICS`
(default `../bentwire-classics`, a checkout of
[bentwire-classics](https://github.com/mschabhuettl/bentwire-classics) next to this one):

```sh
scripts/oracle-check.sh
```

The script builds a temporary `characters/` directory with `builtin` and `classics` and sets
`BENTWIRE_PACKS` to it, for the test that loads every character of every pack in the daemon.
Extra arguments filter the tests: `scripts/oracle-check.sh library` runs only that test,
`scripts/oracle-check.sh oracle` the daemon's playback path against interpreter B and the
B-FB-2 table.

With a variable unset or the pack missing, an ignored test fails with "NOT RUN", and
`oracle-check.sh` prints "NOT RUN" and exits 77. They never count as passed.

Desktop scenario (show, play, say, ask answered over D-Bus, moveto, hide, with signal and
screen checks). It also needs `dbus-run-session`, `busctl`, `python3` (with
`python-gobject` for the tray checks), `sway` and `grim`, or `kwin_wayland`:

```sh
cargo build -p bentwired -p bentwire && cargo build -p bentwired --examples
scripts/desktop/scenario.sh sway     # headless sway, screenshots with grim
scripts/desktop/scenario.sh kwin     # kwin_wayland --virtual
```

Under sway the script also checks a real virtual pointer (click, click-through, drag,
menu grab), output scales 2 and 1.5, output hot-plug and the tray against a minimal
StatusNotifierItem watcher. Screenshots are taken with grim and checked by colour and
position. Under KWin, screenshots are not possible on the build machine: KWin's
ScreenShot2 needs an OpenGL render node, and the VM has none. The character's geometry
is therefore read through KWin scripting (`workspace.windowList()`), and balloons and
menu through the backend's own record (`Test1.Snapshot`). The scale and hot-plug steps
run under sway only.

Client tests (shell integration and Claude Code hooks; need `dbus-run-session`, `gdbus`,
`jq`):

```sh
cargo build -p bentwired -p bentwire && clients/shell/test.sh && clients/claude-code/test.sh
```

Parser fuzzing: `BENTWIRE_FUZZ_ROUNDS=40000 cargo test --release -p agent-formats --test
malformed` (default 1500 rounds per fixture).
