# Using bentwire

The daemon's options and the full reference of the `bentwire` command line client. To
talk to the daemon from your own program, see the D-Bus reference,
[dbus.md](dbus.md).

## `bentwired` options

| Option | Meaning |
|---|---|
| `--characters DIR` | Prepend a character search root (repeatable) |
| `--no-tray` | Do not register a tray icon |
| `--no-audio` | Do not open an audio device |
| `--test-interface` | Export `org.bentwire.Test1` (`Pointer`, `Key`, `Snapshot`) on the Wayland backend |
| `--headless` | Headless backend with a fake clock (tests) |
| `--outputs WxH+X+Y[@S],...` | Virtual outputs in headless mode (default `1920x1080+0+0`) |
| `--seed N` / `--draws FILE` | Seed or scripted draws for the random source |
| `--trace FILE` | Headless: JSON lines of backend requests, sounds, signals and calls |
| `--dump-frames DIR` | Headless: a PNG of the composed scene per output whenever it changes |

`RUST_LOG=debug` gives more log output. The daemon owns the bus name
`org.bentwire.Agent`. A second instance exits with "already running". SIGTERM, SIGINT
and `bentwire quit` save the state and exit.

## CLI reference

The CLI is a thin D-Bus client. Without a running daemon it relies on D-Bus activation.
If that also fails, it prints `bentwired is not running` and exits 1. `bentwire help`
prints the usage.

| Command | Does | Output |
|---|---|---|
| `play <name> [--wait]` | Play an animation or state (missing names fall back by B-FB-1) | request id |
| `say <text> [--speak \| --no-speak] [--wait]` | Speak in the word balloon (`a\|b` picks one alternative; `\Mrk=n\` bookmarks); `--speak` / `--no-speak` speak it aloud or not, whatever the speech setting | request id |
| `ask [<text>] [--heading H] [--icon I] [--buttons SET \| --button CAPTION ...] [--label L ...] [--label-type T] [--checkbox C ...] [--mode M] [--animation NAME]` | Office balloon; always waits for the answer | line 1: the result; line 2 with check boxes: `checked:` and comma-separated 0/1 values |
| `moveto <x> <y> [--speed MS] [--wait]` | Slide to a position (default speed 1000) | request id |
| `gestureat <x> <y> [--wait]` | Gesture towards a point | request id |
| `show [--fast] [--wait]` | Show (animated unless `--fast`) | request id |
| `hide [--fast] [--wait]` | Hide | request id |
| `stop [<id>] [--all]` | Stop one request, the current one, or all (`--all`) | nothing |
| `characters` | List installed characters | `id<TAB>name` per line, the current one with a third column `*` |
| `addons [list]` | List installed add-ons | `name<TAB>kind<TAB>version<TAB>enabled` or `disabled` per line |
| `addons install <folder\|archive\|url> [--sha256 HEX]` | Install an add-on (see below) | the installed name, version and kind |
| `addons enable <name>` / `disable <name>` / `remove <name>` | Switch an add-on on or off, or delete it | nothing |
| `select <id>` | Switch character | nothing |
| `animations` | Animation names of the current character | one per line |
| `scale [<n>]` | Character size factor 1 to 4; without `n`, print it | the factor, or nothing |
| `speech [on \| off]` | Speech output on or off (persisted); without an argument, print it | `on` or `off`, or nothing |
| `smooth [on \| off]` | Smooth edges on or off (persisted); without an argument, print it | `on` or `off`, or nothing |
| `status` | Properties | `character`, `visible`, `left`, `top`, `idle` as `key=value` lines |
| `quit` | Stop the daemon | nothing |
| `convert <source> <dest> [--palette FILE] [--lang TAG] [--codepage LABEL]` | Convert a character file locally, no daemon | nothing |

`--wait` blocks until the request completes.

`ask` details:

- `--icon`: `none`, `alert`, `tip`, `info`, `warning`, `query`, `critical`.
- `--buttons SET`: `none`, `ok` (default), `cancel`, `ok-cancel`, `yes-no`,
  `yes-no-cancel`, `back-close`, `next-close`, `back-next-close`, `retry-cancel`,
  `abort-retry-ignore`, `search-close`, `back-next-snooze`, `tips-options-close`,
  `yes-all-no-cancel`. `--button CAPTION` (1 to 6 times) gives custom buttons; `&` marks
  the mnemonic letter.
- `--label` up to 5 times, `--label-type` `buttons` (clickable, default), `bullets` or
  `numbers`. `--checkbox` up to 5 times.
- `--mode` `modal` (default) or `autodown`, which also closes on a click on the character
  or the balloon and on the next client request.
- `--animation` is played before the balloon (default `Explain`, `""` for none).
- Body text markup: `{cf n}` colour (0..7, 248..255), `{ul 1}` / `{ul 0}` underline.
- Result line: `ok`, `cancel`, `yes`, `no`, `back`, `next`, `retry`, `abort`, `ignore`,
  `search`, `snooze`, `close`, `tips`, `options`, `yes-to-all`; `button:<n>` for custom
  buttons (1-based); `label:<n>` for a clicked label; `none` if dismissed or interrupted.

Exit status: 0 success; 1 usage, D-Bus or method error (error name and message on
stderr); 2 the request failed (for `ask`: nothing is printed, the reason goes to stderr);
3 interrupted, or `ask` result `none`.

## The builtin characters

`characters/builtin` holds bentwire's own characters (CC0), each in two renderings of the
same drawing. Wire, a piece of bent wire: `wire`, smooth and sharp on HiDPI outputs, which
the daemon shows when no other character is chosen or installed, and `wire-retro`, 12
colours and hard edges as in 1997 (`bentwire select wire-retro`). Quack, a rubber
debugging duck: `quack` and `quack-retro` (`bentwire select quack`); it wobbles on the
water when idle and squeaks on Alert and GetAttention. All have the Agent animation set
(`bentwire animations` lists it) and move their mouth with speech output.

## Smooth edges

The Office 97 and Microsoft Bob vector actors are drawn with hard edges, as Office 97 drew
them. `bentwire smooth on`, the tray menu's "Smooth Edges" item, the D-Bus property
`Smooth` or `smooth = true` in `~/.config/bentwire/config.toml` draws them anti-aliased
instead: the picture is taken from the character's 2x sheet and averaged down. It is off
by default; the first three are remembered and win over the config file. It works at size
1 on a 1x screen; at sizes 2 to 4 and on HiDPI screens these characters already use their
2x pictures and the setting changes nothing. Bitmap characters are not affected.

## Speech output

With speech output on, `say` texts and `ask` headings and texts are spoken with
[espeak-ng](https://github.com/espeak-ng/espeak-ng), and the character's mouth follows the
phonemes. It is off by default. Turn it on with `bentwire speech on`, the tray menu's
"Speech" item, the D-Bus property `Speech` or `speech = true` in
`~/.config/bentwire/config.toml`; the first three are remembered and win over the config
file. `bentwire say --speak` or `--no-speak` decides for one text.

- espeak-ng is optional: bentwired loads `libespeak-ng.so.1` when it starts (Arch:
  `pacman -S espeak-ng`, Debian and Ubuntu: `apt install libespeak-ng1`, Fedora: `dnf
  install espeak-ng`; restart bentwired after installing it). Without it, or without its
  data (`espeak-ng-data`), the log says `speech: espeak-ng not available` with the reason
  once, when it first would speak, and the text is shown word by word as without speech.
- Speech plays through the same audio backends as the character's sounds and is off when
  `sound_effects = false`.
- The voice comes from the character's voice data (language, gender, pitch, speed; Earl,
  Genie, Merlin and Peedy have one), otherwise espeak-ng's voice for the UI language
  (`language`). `speaking_speed` scales the speed. How gender and pitch are read is
  inferred, not verified (spec B-TTS-3 [INF]): the numbers are taken in the SAPI 4 sense
  (gender 1 female, 2 male; pitch in Hz), so a voice may not match the original.

## Bridges

- `bentwire-notify`: the character says desktop notifications, with a filter per app
  (`--only`, `--ignore`, `~/.config/bentwire/notify.toml`). Details in
  [`crates/bentwire-notify/README.md`](../crates/bentwire-notify/README.md).
- `bentwire-mqtt`: MQTT topics for say, play, show, hide and ask, with answers and Home
  Assistant discovery (`~/.config/bentwire/mqtt.toml`). Details in
  [`crates/bentwire-mqtt/README.md`](../crates/bentwire-mqtt/README.md).

Your own bridge: [dbus.md, "Writing a bridge"](dbus.md#writing-a-bridge).

## Add-ons

Add-ons are characters, balloon themes and bridges in a folder with an `addon.toml`
manifest. The format, and how to wrap `bentwire-notify` or `bentwire-mqtt` as one, is in
[addon-format.md](addon-format.md).

```sh
bentwire addons install ./my-characters            # a folder
bentwire addons install wire-1.0.tar.gz            # a .tar.gz or .zip archive
bentwire addons install https://example.org/door-bell-0.1.zip --sha256 3f2a...
bentwire addons list
bentwire addons enable door-bell                   # bridges only run once enabled
bentwire addons disable my-characters
bentwire addons remove door-bell
```

- Add-ons go to `~/.local/share/bentwire/addons/<name>/`; which ones are enabled is kept
  in `~/.config/bentwire/addons.toml`.
- Character and theme add-ons are enabled when installed; a disabled one is not used (its
  characters are not listed). Bridges are never enabled by an install, also not when they
  replace an enabled one.
- An enabled bridge is started by `bentwired` and runs as long as the daemon does; if it
  exits, it is restarted after 1 s, 2 s, 4 s and so on up to a minute.
- The CLI tells a running daemon about each change (D-Bus `Reload`) and waits up to 5 s
  for it; a daemon that does not answer in time gets a note on stderr (exit status 0),
  and it applies the change once it handles the call, or on its next start. After
  changing the files by hand, run `pkill -HUP -x bentwired` or restart the daemon.
