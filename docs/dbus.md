# D-Bus API reference

bentwired is controlled only over D-Bus. The `bentwire` CLI, the shell, git, systemd and
Claude Code clients, and the bridges `bentwire-notify` and `bentwire-mqtt` all use the API
described here. This page is the developer view: names, types, what each call returns and
how completion is reported. The behaviour behind every member is in
[spec.md](spec.md) section 5 and the rules it names (B-Q-2, B-ASK-4, ...); where this page
and the spec differ, the spec wins.

- Introspection XML: [dbus/org.bentwire.Agent.xml](dbus/org.bentwire.Agent.xml).
- Examples in shell, Python, Rust and JavaScript: [dbus/examples/](dbus/examples/).
- A bridge template: [dbus/bridge-template/main.rs](dbus/bridge-template/main.rs).

## Bus, name and objects

| | |
|---|---|
| Bus | session bus |
| Name | `org.bentwire.Agent` |
| `/org/bentwire/Agent` | `org.bentwire.Agent1`: characters, scale, speech, quit |
| `/org/bentwire/Character` | `org.bentwire.Character1`: the current character (one at a time) |

Both objects also implement `org.freedesktop.DBus.Introspectable`, `Peer` and
`Properties`. Properties emit `org.freedesktop.DBus.Properties.PropertiesChanged` with the
changed values (no invalidated list).

**Activation.** [packaging/dbus/org.bentwire.Agent.service](../packaging/dbus/org.bentwire.Agent.service)
lets the bus start bentwired on the first method call to `org.bentwire.Agent` (B-RUN-1).
Every example below relies on it. To find out whether the daemon runs without starting
it, call `org.freedesktop.DBus.NameHasOwner("org.bentwire.Agent")`, or send your calls
with the NO_AUTO_START flag. Follow `NameOwnerChanged` with `arg0='org.bentwire.Agent'`
to see it come and go (`Quit`, SIGTERM, a crash, a restart). A second instance exits with
"already running"; the name is never queued.

## Requests and completion

The methods `Show`, `Hide`, `Play`, `Speak`, `Ask`, `MoveTo` and `GestureAt` of
`Character1` queue a **request** and return its id (`u`) at once (R-Q-1). Ids start at 1
and only grow; 0 means "none" (B-Q-1). The request runs later, in queue order, and reports
through signals:

1. `RequestStart(id)` when its first phase begins.
2. For an ask, `AskComplete(id, result, checked)` when the balloon closes.
3. `RequestComplete(id, status, reason)` after its last phase. Always the last signal of a
   request. A request removed before it starts gets only this one, with status 3.

`status` is 0 complete, 1 failed, 3 interrupted; `reason` is `""` on success, otherwise an
error name from [the table below](#request-reasons) (B-Q-2, B-Q-3).

`RequestStart`, `AskComplete`, `RequestComplete` and `Bookmark` are **unicast**: they go
only to the unique name of the connection that made the call (B-Q-2). Two consequences
for clients:

- Wait on the same connection that made the call, and set up your signal handling before
  the call, so that nothing is missed. The method reply is always sent before the first
  signal of its request (B-Q-1).
- A command line tool such as `gdbus call` disconnects after the reply and never sees
  them. The shell example uses a bus monitor (`dbus-monitor --monitor`) instead.

Unicast signals reach their destination without a match rule; a match rule such as
`type='signal',interface='org.bentwire.Character1'` is needed for the broadcast ones.

A method **error** (the call fails, see [Errors](#errors)) creates no request and no
signals.

## `org.bentwire.Agent1` at `/org/bentwire/Agent`

| Method | Signature | Meaning | Errors |
|---|---|---|---|
| `ListCharacters` | `() -> a(sss)` | (id, name, description) of every installed character, sorted by id; also rescans the search roots (B-CHAR-1, B-CHAR-3) | |
| `Select` | `(s id) -> ()` | switch to that character (B-CHAR-5); open requests end with `CharacterChanged` | `NoSuchCharacter`, `InvalidCharacter` (message: the load error) |
| `Quit` | `() -> ()` | save the state and exit (B-RUN-1) | |

| Property | Type | Access | Meaning |
|---|---|---|---|
| `Character` | `s` | read | current character id, `""` if none |
| `Version` | `s` | read | daemon version |
| `Scale` | `u` | read, write | size factor 1 to 4 (B-SCALE-1); a write is persisted; other values give `InvalidArgs` |
| `Speech` | `b` | read, write | speech output with espeak-ng (B-TTS-1); a write is persisted and wins over the config key `speech` |

## `org.bentwire.Character1` at `/org/bentwire/Character`

### Methods

Every `u` return value is a request id.

| Method | Signature | Meaning | Method errors |
|---|---|---|---|
| `Show` | `(b fast) -> u` | show, animated unless `fast` (R-SHOW-1, B-SHOW-3); `Shown` cause 4 | `NoCharacter` |
| `Hide` | `(b fast) -> u` | hide (R-SHOW-2); `Hidden` cause 3 | `NoCharacter` |
| `Play` | `(s name) -> u` | play an animation or state; a missing name falls back by B-FB-1 | `NoCharacter`, `UnknownAnimation` |
| `Speak` | `(s text) -> u` | say `text` in the word balloon; `a\|b` picks one alternative, `\Mrk=n\` sends `Bookmark(n)` (R-SPEAK-*, B-SPEAK-1); spoken aloud when `Speech` is on (B-TTS-1) | `NoCharacter`, `InvalidArgs` (reserved bookmark number) |
| `SpeakWith` | `(s text, a{sv} options) -> u` | as `Speak`; option `speech` (`b`) speaks the text aloud or not, whatever `Speech` says (B-TTS-1) | as `Speak`; `InvalidArgs` for an unknown option or a wrong type |
| `Ask` | `(a{sv} spec) -> u` | Office balloon with buttons, labels, check boxes; answer in `AskComplete` (B-ASK-*) | `NoCharacter`, `InvalidBalloon`, `NotSupported`, `InvalidArgs` |
| `MoveTo` | `(i x, i y, u speed_ms) -> u` | slide the top-left corner to (x, y) in `speed_ms` (R-MOVE-*, B-MOVE-1) | `NoCharacter` |
| `GestureAt` | `(i x, i y) -> u` | gesture towards a point (R-GEST-1) | `NoCharacter` |
| `Stop` | `(u id) -> ()` | stop one request; 0 stops all of them (B-Q-4) | |
| `StopAll` | `(as types) -> ()` | stop by type: `play`, `move`, `speak`, `ask`; `[]` = everything (B-Q-5) | `InvalidArgs` (unknown type) |

`Stop` and `StopAll` act at once and return no request. An unknown or finished id does
nothing.

**`Ask` keys** (B-ASK-1, all optional, one of `heading`, `text`, `labels` required). An
unknown key or a wrong type is `InvalidArgs` (A-DESK-1); a value wrapped in a second
variant is accepted.

| Key | Type | Values |
|---|---|---|
| `heading` | `s` | bold heading |
| `text` | `s` | body, with `{cf n}` colour and `{ul 1}` / `{ul 0}` underline (B-ASK-3) |
| `icon` | `s` | `none`, `alert`, `tip`, `info`, `warning`, `query`, `critical` |
| `labels` | `as` | up to 5 |
| `label_type` | `s` | `buttons` (clickable, default), `bullets`, `numbers` |
| `checkboxes` | `as` | up to 5 |
| `buttons` | `s` or `as` | a set name (`ok` default, `yes-no`, `ok-cancel`, ... B-ASK-2), or 1 to 6 custom captions (`&` marks the mnemonic) |
| `mode` | `s` | `modal` (default) or `autodown` (B-ASK-6); `modeless` gives `NotSupported` |
| `animation` | `s` | played before the balloon, default `Explain`, `""` for none |

**Ask results** in `AskComplete` (B-ASK-2): -1 OK, -2 Cancel, -3 Yes, -4 No, -5 Back,
-6 Next, -7 Retry, -8 Abort, -9 Ignore, -10 Search, -11 Snooze, -12 Close, -13 Tips,
-14 Options, -15 Yes to all; custom buttons -25, -26, ... in order; a clicked label its
1-based index; 0 dismissed or interrupted. `checked` has one boolean per check box. The CLI
and `bentwire-mqtt` print these as `ok`, `yes`, `button:<n>`, `label:<n>`, `none`.

An ask whose request fails (status 1: `Hidden`, `BalloonTooBig`) has no `AskComplete`;
one closed by `Stop`, `Hide` or a character switch has `AskComplete` with result 0 and
status 3 (B-ASK-4, B-ASK-7). While an ask balloon is open the queue keeps running.

### Properties

All read-only except `IdleOn`.

| Property | Type | Meaning |
|---|---|---|
| `Id` | `s` | character id |
| `Name` | `s` | localized name |
| `Description` | `s` | localized description |
| `Visible` | `b` | shown |
| `Left`, `Top` | `i` | top-left position |
| `Width`, `Height` | `u` | display size times `Scale` |
| `IdleOn` | `b` | idle animations on (R-IDLE-4); read and write |
| `MoveCause` | `u` | cause of the last move (B-SHOW-4) |
| `VisibilityCause` | `u` | cause of the last Shown or Hidden; 0 = never shown |
| `BalloonEnabled` | `b` | the character has a word balloon (B-BAL-1) |
| `AnimationNames` | `as` | animation names, then office97 group names |

### Signals

"Unicast" signals go only to the caller of the request; the others are broadcast to every
matching connection.

| Signal | Arguments | Sent | When |
|---|---|---|---|
| `RequestStart` | `u id` | unicast | a request's first phase begins (B-Q-2) |
| `RequestComplete` | `u id, u status, s reason` | unicast | a request ended: 0 complete, 1 failed, 3 interrupted (B-Q-2, B-Q-3) |
| `AskComplete` | `u id, i result, ab checked` | unicast | an ask balloon closed, just before its `RequestComplete` (B-ASK-2, B-ASK-4) |
| `Bookmark` | `i n` | unicast | a `Speak` reached `\Mrk=n\` in its text (B-SPEAK-1) |
| `Click` | `u button, u modifiers, i x, i y, b from_tray` | broadcast | press and release on the character without a drag; tray left click (button 1) and middle click (4) with `from_tray` true (B-UI-1, B-TRAY-1) |
| `DblClick` | `u button, u modifiers, i x, i y, b from_tray` | broadcast | a second press within `double_click_ms` (B-UI-1) |
| `DragStart` | `u button, u modifiers, i x, i y` | broadcast | the user starts dragging the character (R-UI-2) |
| `DragComplete` | `u button, u modifiers, i x, i y` | broadcast | the drag ended (R-UI-2) |
| `Move` | `i x, i y, u cause` | broadcast | the character moved: cause 1 drag, 2 program, 4 display change (B-SHOW-4) |
| `Shown` | `u cause` | broadcast | the character became visible: 2 user, 4 program, startup, switch (R-SHOW-1, B-SHOW-4) |
| `Hidden` | `u cause` | broadcast | the character was hidden: 1 tray, 3 program, 7 character menu (R-SHOW-2, B-SHOW-4) |
| `IdleStart` | | broadcast | idle begins (R-IDLE-2) |
| `IdleComplete` | | broadcast | idle ends: a new request is queued or `IdleOn` is set to false (R-IDLE-4) |
| `BalloonShown` | | broadcast | a word or ask balloon appeared (R-BAL-7, B-ASK-4) |
| `BalloonHidden` | | broadcast | it disappeared (R-BAL-10, B-ASK-4) |

Buttons: 1 left, 2 right, 4 middle. Modifiers: 1 Shift, 2 Ctrl, 4 Alt (bits). A right
click also opens the character's menu.

## Errors

Method errors (the call fails; spec 5.4):

| Name | When |
|---|---|
| `org.bentwire.Error.NoCharacter` | a request method while no character is loaded |
| `org.bentwire.Error.NoSuchCharacter` | `Select` with an unknown id |
| `org.bentwire.Error.InvalidCharacter` | `Select` of a character that fails to load |
| `org.bentwire.Error.UnknownAnimation` | `Play` of a name that has no fallback (B-FB-1 step 5) |
| `org.bentwire.Error.InvalidBalloon` | `Ask` that breaks B-ASK-5; the message names the Office error (`TooManyControls`, `ButtonlessModal`, `CheckboxesNeedButton`) |
| `org.bentwire.Error.NotSupported` | `Ask` with `mode = modeless` |
| `org.freedesktop.DBus.Error.InvalidArgs` | wrong argument types, unknown `Ask` keys and `SpeakWith` options, unknown `StopAll` types, reserved bookmark numbers, `Scale` outside 1 to 4 |

Standard errors: `org.freedesktop.DBus.Error.UnknownMethod` and `UnknownObject` for
unknown members and paths, `UnknownProperty` for `Get` of an unknown property, and
`PropertyReadOnly` for every other `Set` of a known property, `UnknownProperty` for an
unknown one. `GetAll` of an unknown interface fails with `UnknownInterface`.

<a id="request-reasons"></a>Request reasons (the `reason` of `RequestComplete`):

| Reason | Status | When |
|---|---|---|
| `org.bentwire.Error.Hidden` | 1 (3 for an ask closed by `Hide`) | the request needs a visible character |
| `org.bentwire.Error.Interrupted` | 3 | `Stop`, `StopAll` |
| `org.bentwire.Error.HiddenByUser` | 3 | the user hid the character (menu, tray) |
| `org.bentwire.Error.Dragged` | 1 | a drag started during a `MoveTo` slide (R-MOVE-2) |
| `org.bentwire.Error.CharacterChanged` | 3 | `Select` or the gallery switched character |
| `org.bentwire.Error.BalloonTooBig` | 1 | the ask balloon does not fit the output |
| `org.bentwire.Error.AnimationUnavailable` | 1 | the animation is damaged (B-CHAR-2) |

## `org.bentwire.Test1`

`/org/bentwire/Test` exports `org.bentwire.Test1` (synthetic pointer and key events, a
JSON snapshot of the scene and, in headless mode, the fake clock). It exists only with
`bentwired --headless` or `--test-interface` and is meant for tests (spec T-4, A-TEST-3,
A-DESK-3); it is not part of the API and is left out of the XML.

## Introspection XML

[dbus/org.bentwire.Agent.xml](dbus/org.bentwire.Agent.xml) holds the two interfaces as the
daemon introspects them, without the standard interfaces, in one `<node>` so that code
generators such as `gdbus-codegen` take it as it is. The
test `spec_5_1_introspection_matches_docs_dbus_xml` in `crates/bentwired/tests/e2e.rs`
(part of `cargo test --workspace`) starts `bentwired --headless` on a private bus, calls
`Introspect` on both objects and fails if the file differs. After an API change,
regenerate it with

```sh
BENTWIRE_BLESS=1 cargo test -p bentwired --test e2e spec_5_1_introspection
```

and update this page. The argument names in the introspection follow the spec.

To look at a running daemon: `gdbus introspect --session --dest org.bentwire.Agent
--object-path /org/bentwire/Character` or `busctl --user introspect org.bentwire.Agent
/org/bentwire/Character`.

## Examples

Each one shows the character, speaks, plays `Wave`, asks "Coffee or tea?" with two custom
buttons and prints the answer, then prints `Click`, `Shown` and `Hidden` (and `DblClick`)
until Ctrl+C.

| File | Needs | Run |
|---|---|---|
| [example.sh](dbus/examples/example.sh) | `gdbus` (GLib), `dbus-monitor` (dbus) | `sh docs/dbus/examples/example.sh` |
| [example.py](dbus/examples/example.py) | PyGObject: `python3-gi` (Debian, Ubuntu), `python-gobject` (Arch), `python3-gobject` (Fedora) | `python3 docs/dbus/examples/example.py` |
| [example.rs](dbus/examples/example.rs) | `zbus = "5"` | `cargo run -p bentwire --example dbus-client` |
| [example.js](dbus/examples/example.js) | Node.js and `npm install dbus-next` | `node docs/dbus/examples/example.js` |

The Rust example is a cargo example of the `bentwire` crate, so `cargo build --workspace
--all-targets` and `cargo test --workspace` compile it. The shell example needs a bus
that supports `BecomeMonitor`; it was tested with dbus-daemon, not with dbus-broker.

One-off calls from a shell:

```sh
gdbus call --session --dest org.bentwire.Agent --object-path /org/bentwire/Character \
  --method org.bentwire.Character1.Speak 'Hello!'
busctl --user call org.bentwire.Agent /org/bentwire/Character org.bentwire.Character1 \
  Ask a{sv} 2 text s 'Ready?' buttons s yes-no
busctl --user get-property org.bentwire.Agent /org/bentwire/Agent org.bentwire.Agent1 Character
```

For shell scripts that wait for an answer the CLI is simpler: `bentwire ask --buttons
yes-no 'Ready?'` prints `yes` or `no` ([usage.md](usage.md)).

## Writing a bridge

A bridge maps events from somewhere else (a broker, a socket, a log) to calls, and
results back. [dbus/bridge-template/main.rs](dbus/bridge-template/main.rs) is a small
Rust skeleton (stdin as the event source, `cargo run -p bentwire --example
bridge-template`) with the structure of the real bridges:

- One thread owns the session bus connection. It creates its message stream and adds the
  match rules (`NameOwnerChanged` for `org.bentwire.Agent`, and
  `interface='org.bentwire.Character1'` for broadcast signals) before any call, then
  forwards signals to the main loop. When the connection ends it reports that and
  reconnects every 2 s.
- Calls go to `org.bentwire.Agent`; a call while the daemon does not run starts it
  through activation. Method errors are logged with their D-Bus error name; the bridge
  keeps running.
- Each `Ask` id is remembered until its `RequestComplete`; the `AskComplete` before it
  holds the answer. A failed ask has no `AskComplete`. When the daemon loses its name or
  the bus connection drops, open asks get no more signals: answer them as dismissed.
- Exit cleanly when the source ends or on SIGTERM / SIGINT. A bridge with external state
  blocks both signals and cleans up after `sigwait`, as `bentwire-mqtt` does.

Full bridges:
[crates/bentwire-notify](../crates/bentwire-notify/README.md) (B-NOTIFY-1: a bus monitor
for notifications, `Speak` on a second connection) and
[crates/bentwire-mqtt](../crates/bentwire-mqtt/README.md) (B-MQTT-1: MQTT commands,
answers, state and Home Assistant discovery).
