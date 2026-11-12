# Using bentwire

The daemon's options and the full reference of the `bentwire` command line client.

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
| `say <text> [--wait]` | Speak in the word balloon (`a\|b` picks one alternative; `\Mrk=n\` bookmarks) | request id |
| `ask [<text>] [--heading H] [--icon I] [--buttons SET \| --button CAPTION ...] [--label L ...] [--label-type T] [--checkbox C ...] [--mode M] [--animation NAME]` | Office balloon; always waits for the answer | line 1: the result; line 2 with check boxes: `checked:` and comma-separated 0/1 values |
| `moveto <x> <y> [--speed MS] [--wait]` | Slide to a position (default speed 1000) | request id |
| `gestureat <x> <y> [--wait]` | Gesture towards a point | request id |
| `show [--fast] [--wait]` | Show (animated unless `--fast`) | request id |
| `hide [--fast] [--wait]` | Hide | request id |
| `stop [<id>] [--all]` | Stop one request, the current one, or all (`--all`) | nothing |
| `characters` | List installed characters | `id<TAB>name` per line, the current one with a third column `*` |
| `select <id>` | Switch character | nothing |
| `animations` | Animation names of the current character | one per line |
| `scale [<n>]` | Character size factor 1 to 4; without `n`, print it | the factor, or nothing |
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
