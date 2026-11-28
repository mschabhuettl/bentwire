# Releasing bentwire

A release is a `vX.Y.Z` tag on `main`. The tag starts `.github/workflows/release.yml`,
which builds everything, checks the packages and creates the GitHub release. Everything
below the tag is in the order it is done.

## Before the tag

Run on the commit that gets the tag, with the commands from [CONTRIBUTING.md](../CONTRIBUTING.md)
and the repository's `CLAUDE.md`:

1. `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`.
2. Real-data tests: `scripts/oracle-check.sh`, `scripts/oracle-check.sh library` and
   `scripts/oracle-check.sh oracle` (need the original files, the reference parsers and the
   classics pack; "NOT RUN" is a failure here).
3. Client tests: `clients/shell/test.sh`, `clients/claude-code/test.sh`,
   `clients/git/test.sh`, `clients/systemd/test.sh`; MQTT: `scripts/mqtt-check.sh`.
4. Desktop scenarios: `scripts/desktop/scenario.sh sway` and `scripts/desktop/scenario.sh kwin`.
5. Flatpak: `scripts/flatpak-build.sh`, then `scripts/flatpak-check.sh BUNDLE` and
   `scripts/flatpak-check.sh BUNDLE --classics` in a fresh container.
6. Arch: `makepkg -s` of the PKGBUILDs in `packaging/aur/` in a fresh Arch container as a
   normal user, with `check()` (the default), for example `bentwire-git` against the branch.
7. Debian and Fedora packages: build the release binaries
   (`cargo build --release --locked -p bentwired -p bentwire -p bentwire-convert -p
   bentwire-notify -p bentwire-mqtt`), then `scripts/package-build.sh X.Y.Z target/release
   DIR` (needs [nfpm](https://nfpm.goreleaser.com/), `scripts/nfpm-install.sh` installs the
   pinned version), then `scripts/package-check.sh` on the results in
   `docker.io/library/debian:trixie`, `docker.io/library/ubuntu:24.04` and
   `docker.io/library/fedora:43` (and the oldest supported Fedora, 41):

   ```sh
   scripts/package-check.sh DIR/bentwire_X.Y.Z-1_amd64.deb docker.io/library/debian:trixie
   scripts/package-check.sh DIR/bentwire_X.Y.Z-1_amd64.deb docker.io/library/ubuntu:24.04
   scripts/package-check.sh DIR/bentwire-X.Y.Z-1.x86_64.rpm docker.io/library/fedora:43
   ```

   CI (`.github/workflows/ci.yml`, jobs `packages` and `packages-check`) runs the same on
   every push.
8. CI on `main` is green (CI and Flatpak workflows).

## Version bump

One commit, `Version X.Y.Z`:

- `version` in `[workspace.package]` of `Cargo.toml`.
- `Cargo.lock` (`cargo build` updates it), then `scripts/flatpak-cargo-sources.sh`, which
  regenerates the Flatpak's cargo sources (CI checks them with `--check`).
- `CHANGELOG.md`: rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`, add a new empty
  `## [Unreleased]`, and update the compare links at the end:
  `[Unreleased]: .../compare/vX.Y.Z...HEAD` and `[X.Y.Z]: .../compare/vPREV...vX.Y.Z`. The
  release notes are taken from this section by `scripts/changelog-notes.sh`.
- `packaging/flatpak/org.bentwire.Agent.metainfo.xml`: a new `<release version="X.Y.Z"
  date="YYYY-MM-DD"/>` first in `<releases>`.
- The version in the README examples that name a release file (Flatpak, .deb, .rpm).

The `pkgver` of the PKGBUILDs follows after the release (below), because their checksums
come from the release.

## Release assets

The workflow attaches exactly these files to the release (job `publish` fails on any
other set):

| Asset | Built by |
|---|---|
| `bentwire-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz` | job `build` (binaries, builtin characters, packaging, clients, docs, licences) |
| `bentwire-X.Y.Z-x86_64.flatpak` | `.github/workflows/flatpak.yml` (renamed in `publish`) |
| `bentwire_X.Y.Z-1_amd64.deb` | job `build`, `scripts/package-build.sh` from the same binaries |
| `bentwire-X.Y.Z-1.x86_64.rpm` | job `build`, `scripts/package-build.sh` from the same binaries |
| `sha256sums.txt` | job `publish` |

`sha256sums.txt` must list every other asset, one `<sha256>  <name>` line each. Only the
tarball's name may contain `tar.gz`: the AUR update reads its line with `grep tar.gz`.
After the release, check it against the downloaded assets:

```sh
gh release download vX.Y.Z -D rel && (cd rel && sha256sum -c sha256sums.txt)
```

## After the release

1. AUR: update `pkgver`, `pkgrel=1` and the checksums of the three packages
   (`bentwire` from the tag archive, `bentwire-bin` from the tarball line of
   `sha256sums.txt`, `bentwire-git` unchanged), regenerate `.SRCINFO`, commit them in
   `packaging/aur/` and push each to its AUR repository. Then, in a fresh Arch container,
   install each with `yay -S` and check the installed version with `pacman -Q`.
2. Install the published assets in fresh containers: the Flatpak bundle with
   `scripts/flatpak-check.sh` (with and without `--classics`), the .deb and .rpm with
   `scripts/package-check.sh` as above, but on the downloaded files.
