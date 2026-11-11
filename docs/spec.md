# bentwire behaviour specification, version 1

Status: normative for the first build, 2026-11-11. This file consolidates the research
reports. It does not add reverse engineering. A build session implements against it and
turns every rule into a test.

## 0. How to read this

**Inputs.** The build session gets this file together with:
- `docs/idea-de.md`: decisions and scope, in German;
- `docs/character-format.md`: the character format, cited as **character-format**;
- the research reports in `docs/research/`, cited by file name without `.md`:
  agent-runtime, office-assistant, acs-format, acf-aaf-format, agent15-format,
  act-format.

Byte-level detail lives in those files. This spec points at them and does not copy them.

**Normative words.** MUST, MUST NOT and SHOULD carry their usual meaning. Everything not
marked as a note is normative.

**Rule IDs.**
- `R-…` and `R-OB-…` are rule IDs from the reports. They are reused only where the
  semantics are unchanged, written as "(= agent-runtime R-IDLE-1)".
- `B-<AREA>-n` are new rules. They are used for a consolidation, an adaptation to
  Linux/Wayland ("adapts R-…") or a decision.
- Sections of reports that have no rule IDs are cited by section, for example
  "(= acs-format 5.2)".

**Evidence tags** are carried over from the reports:
- [OBS]: observed in a shipped binary (static analysis);
- [DOC]: Microsoft documentation;
- [INF]: inferred.

A rule that rests only on [INF], or that chooses where the evidence is silent, is marked
**bentwire decision** with its reason. The build MUST NOT reopen such a rule without new
evidence.

**Character format status.** character-format calls itself a draft. However, bentwire.md
records its recommendations D1 to D8 as accepted on 2026-11-11. This spec treats D1 to D8
as decided and refers to the character-format sections by number. Its audio section
(character-format 6) may still be changing; this spec depends only on "the daemon decodes
FLAC and PCM WAVE" (D1 B).

**Units and coordinates.**
- Time is in milliseconds.
- Positions are desktop logical pixels: the compositor's global logical coordinate space,
  as given by xdg-output logical positions. These play the role of Agent's screen
  coordinates.
- "Frame rectangle" means the character's on-screen rectangle at its display size.

## 1. Scope

### 1.1 In v1 (from bentwire.md "Umfang der ersten Version")

- `bentwired`, a Wayland overlay daemon (layer-shell, KDE Plasma first). It provides:
  - animations with both interpreter families;
  - states and idle;
  - sound effects;
  - drag, click and double-click;
  - the right-click menu as in the original;
  - a tray icon.
- Two balloons:
  - the Agent speech balloon (`say`);
  - the Office balloon (`ask`), with buttons, labels, check boxes and an icon.
- D-Bus API and CLI: `play`, `say`, `ask`, `moveto`, `gestureat`, `show`, `hide`, `stop`.
  Added because they are needed to run the system:
  - character listing and selection;
  - animation listing;
  - status;
  - quit;
  - `convert`.
- Character selection through the menu.
- The daemon loads only the bentwire character format. Parsers and the converter are in
  scope as separate work, specified by character-format.
- Two demo clients (shell integration, Claude Code hooks). They only use the CLI or the
  D-Bus API defined here.

### 1.2 Out of v1

- **Speech:**
  - TTS and speech output;
  - `.WAV`/`.LWV` URLs in Speak;
  - speech recognition (Listen, Listening/Hearing driven by voice, the Voice Commands
    window).
- **Methods and features dropped from the API:**
  - Think;
  - Wait and Interrupt (they need several characters);
  - Activate and the multi-client input model;
  - Get/Prepare (HTTP loading);
  - client-defined menu commands (Commands collection);
  - Balloon.Style setting by clients;
  - Width/Height scaling by clients;
  - help mode.
- **Characters and desktop integration:**
  - several characters at once;
  - docking to the active window;
  - AT-SPI triggers ("It looks like you're writing a letter");
  - tip collections and the tip bulb.
- **Office features:**
  - the Office search balloon and its Answer Wizard;
  - the Office balloon's modeless mode;
  - pictures in balloon markup (`{bmp}`, `{wmf}`, `{pict}`, `{dlx}`, `{cbmp}`).
- **Settings UI:** a graphical options dialog (v1 opens the config file instead,
  B-MENU-3).
- **Platforms:** GNOME (no layer-shell), X11, Windows, macOS.

## 2. Execution model

**B-CORE-1 Deterministic core.** All behaviour in section 3 lives in a core that has no
I/O. The core gets these inputs:
- a clock;
- a random source;
- input events (pointer, outputs, D-Bus calls).

It produces these outputs:
- scene state (surfaces, positions, pictures, input shapes, balloons, menu);
- sound start and stop commands;
- D-Bus replies and signals.

Backends:
- the Wayland backend;
- the audio backend (rodio);
- the D-Bus backend (zbus);
- the tray backend (ksni);
- a headless backend for tests (section 6).

**B-CORE-2 Random source.** Every random draw goes through one injectable source. It
returns a uniform integer in [0, n) (character-format 7.1). Tests inject scripted draws.
Reproducing msvcrt `rand()` bit for bit is not required (= agent-runtime R-ANIM-4).

The draw sites are:
- branch draws (agent family: one per frame visit; office97 family: one per random
  branch);
- state member choice;
- the Speak `|` alternative;
- the office97 group variant;
- the Animate! shuffle.

**B-CORE-3 Clock.** All timers use one monotonic clock that tests can replace with a fake
clock. Timer resolution is 1 ms.

## 3. Rules

### 3.1 Characters: discovery, loading, selection

**B-CHAR-1 Discovery.**
- Characters are found in `$XDG_DATA_HOME/bentwire/characters/`, then in each
  `$XDG_DATA_DIRS` entry plus `/bentwire/characters/`, in that order.
- An entry is either a folder containing `character.toml`, or a `*.bwc` file.
- The character id is the entry name without `.bwc`, ASCII-lowercased. When two entries
  have the same id, the first one found wins.
- Entries that fail the cheap check are skipped with a log line. The cheap check is: the
  manifest parses, `format = 1`, and `family` is known.

**B-CHAR-2 Loading.**
- Loading follows character-format 4 to 7. An unknown `format` major or `family` is
  rejected.
- Animations named in `source.missing_animations` count as absent (B-FB-1 applies).
- A picture or sound that fails to decode makes only the animations using it
  unavailable. It does not make the whole character fail. This matches agent15-format 8.2.

**B-CHAR-3 Display names.** The name and description are chosen from `character.info`
for the UI language L (B-L10N-1), adapting the ACS rule (= acs-format 3.2.6) to BCP 47
tags:
1. the first entry whose tag equals L and whose field is non-empty;
2. otherwise the last entry with the same primary language;
3. otherwise the last entry with primary language `en`;
4. otherwise an empty string.

**B-CHAR-4 Startup selection.** The character is the first of these that loads:
1. the state file's character;
2. the config file's `character`;
3. `clippit`;
4. the first id in sorted order.

If none loads, the daemon still runs and the tray shows no character. Character methods
then fail with `org.bentwire.Error.NoCharacter`.

**B-CHAR-5 Switching (`Select`, gallery).**
1. Every queued, running or in-progress request of the old character ends with status 3
   and the reason `org.bentwire.Error.CharacterChanged`. An open client ask balloon
   closes (AskComplete result 0). The gallery balloon is the exception (B-GAL-1).
2. The new character takes the old top-left position.
3. If the old character was visible, the new one is shown with an animated Show
   (cause 4).
4. The switch is persisted (B-CFG-2).

### 3.2 Animation playback

#### 3.2.1 Common to both families

**R-ANIM-1 Duration unit** (= agent-runtime R-ANIM-1) [OBS] [DOC].
- An ACS duration is 1/100 s.
- The manifest already stores ms (character-format 4: `ms` = ACS value x 10; ACT value
  unchanged).
- The player uses `ms` unchanged.

**B-ANIM-2 Completion.** (Adapts agent-runtime R-ANIM-3 [OBS].) **bentwire decision**
for which sound counts.
- An animation completes when its last shown frame's duration has expired, or when the
  interpreter reports END.
- If a sound effect started by this animation is still playing at that point, completion
  waits until that sound ends. The sound may have been started by an earlier frame.
- The report says "that frame's sound". bentwire waits for whichever sound of the
  animation is playing. Reason: the server waits for the wave-out "done" notification,
  and only one sound plays at a time (R-ANIM-8). That notification does not tell which
  frame started the sound.
- Test vector: two frames of 100 ms, where frame 0 starts a 500 ms sound. The animation
  completes at t = 500, not at t = 200.

**R-ANIM-6 Exit and return on switch** (= agent-runtime R-ANIM-6) [OBS] [DOC].
- The animation most recently played (completed or stopped) is the "previous animation".
- Every new animation request (Play, a state, the MoveTo or Gesture animation, Speakable,
  Hiding, idle) first runs the family's switch rule (B-SW-A or B-SW-O) against it.
- Exception [OBS]: a Showing request drops the previous animation, so no exit or return
  animation is played before Showing.

**R-ANIM-7 / B-ANIM-1 Stop cuts, exit happens on the next request.** (Adapts
agent-runtime R-ANIM-7 [OBS] and closes U-3 [INF].) **bentwire decision.**
- Stopping a running animation does three things immediately:
  - kills its frame timer;
  - stops its sound;
  - leaves the current picture on screen.
- The stopped animation stays the "previous animation", together with its current frame
  index.
- The next animation request runs the switch rule. That rule resumes the stopped
  animation in exit mode from that frame; the first exit step is taken at once.
- A request that plays no animation does not consume the previous animation and does not
  trigger the exit. Such requests are:
  - Hide(fast);
  - the MoveTo slide phase;
  - a missing state.
- Reason: this is the only reading consistent with R-ANIM-7 [OBS] together with the
  provider holding the previous animation [OBS]. The SDK says exit branches run "when the
  animation is stopped, before the next animation plays" ([DOC], docs page
  `creating-animations`).

**R-ANIM-8 Sound effects** (= agent-runtime R-ANIM-8) [OBS].
- When a frame becomes current and has a `sound`, that sound starts, provided that both:
  - config `sound_effects` is true;
  - the character is visible.
- Starting a sound stops the previous one. There is at most one sound per character.
- Whether a sound starts on frames that are not shown is decided per family:
  - R-ANIM-5: agent, not started;
  - B-O97-2: office97, started.
- If the audio device fails, the sound is skipped and logged, and completion does not
  wait for it.

**R-ANIM-9 Hidden character** (= agent-runtime R-ANIM-9) [OBS] [DOC].
- Fail at once with `org.bentwire.Error.Hidden`, without playing:
  - Play;
  - the MoveTo animation phase and the MoveTo end phase.
- GestureAt and the Speak Speakable phase run invisibly. Frames are stepped with normal
  timing but not drawn, and no sounds start.
- Every animation that completes while the character is hidden completes with failure
  `org.bentwire.Error.Hidden`.

**R-ANIM-10 One player.** The core has one frame stepper per character
(= agent-runtime R-ANIM-10).

#### 3.2.2 Family `agent`

**R-ANIM-2 Frame clock** (= agent-runtime R-ANIM-2) [OBS].
- The player keeps an absolute deadline. When frame N is shown, the next deadline is the
  previous deadline plus N's `ms`.
- If that deadline is already past, the next deadline becomes now plus `ms`. The player
  does not catch up and does not drop frames.
- Test vector, fake clock, frames of 100/200/300 ms:
  - frames are shown at t = 0, 100 and 300;
  - the animation completes at 600;
  - if frame 2 is shown late at t = 800, frame 3's deadline is 1000.

**R-ANIM-4 / B-SEQ-A Next frame.** (= acs-format 5.2, character-format 7.2 `next()`.)
- Draw: one draw r in [0, 100) per frame visit. Take the first branch with r < p, with a
  cumulative walk.
- This equals agentdp2's `rand() % 100 + 1 <= p` exactly within the constraints of
  character-format 7.2.
- Test vector: branches [[5, 30], [7, 50]]:
  - r = 29 gives 5;
  - r = 30 gives 7;
  - r = 79 gives 7;
  - r = 80 gives cur + 1.

**R-ANIM-5 Invisible frames** (= agent-runtime R-ANIM-5, acs-format 5.2) [OBS].
- A frame with no layers and `ms = 0` is an invisible branch point:
  - it is walked through, with at most 32 hops;
  - it is never shown;
  - its sound is not started. This closes character-format 14 item 5 (2 frames, F1 and
    ROCKY) as a **bentwire decision** following agentdp2's skip logic [OBS].
- A frame with layers and `ms = 0` becomes current: its sound starts and its shape is
  applied. The next frame then follows at the same deadline.

**B-SW-A Switch rule, agent** (= acs-format 5.2 "entering exit mode", 5.3) [OBS].

When a request needs a new animation X and a previous animation P exists, the first case
that applies is used:
1. **Exit mode.** P's current frame is valid and its `exit` is not `"end"`, and at least
   one of these holds: P is not at its end, or P has `transition = "exit"`. Then P
   continues in exit mode (`next(cur, exiting = true)`) until END, and then X starts.
2. **Return animation.** P is at its end and has a `return` animation R with R != X.
   Then R plays (normally, not in exit mode), and then X starts.
3. **Otherwise** X replaces P immediately.

Notes:
- agentdp2 has a third condition for case 1, an internal "queued" flag (acs-format 5.2).
  Its meaning is [INF] in acs-format and agent15-format 5. bentwire treats it as false
  for an animation that reached its end, which leaves only the two conditions above.
  **bentwire decision.** Reason: if the flag were always true, a finished animation with
  a return animation would never play that return, which contradicts the SDK ("the
  server plays the Return animation of the previous animation", [DOC] `play-method`). No
  real data depends on the flag: all 14 `transition = "return"` animations in the ACS
  files (courtney, earl) have exit -1 on every frame (checked with `acs.py`).
- 1.5 characters (`exit = "end"` on every frame) therefore never use exit mode. They
  play their return animation after a finished animation (agent15-format 5).
- Test vectors, with a synthetic two-animation character:
  - a loop with exits that is stopped mid-loop resumes through its exit frames;
  - an animation with `transition = "return"` that has finished plays its return first;
  - Showing after any P plays no exit.

**B-PICK-A Speakable** (= acs-format 3.2.5, agent-runtime R-SPEAK-1) [OBS].
- The pseudo-state Speakable keeps the current animation if its current frame has
  `mouths`.
- Otherwise Speakable is the Speaking state.

#### 3.2.3 Family `office97`

**B-O97-1 Frame sequencing.** Frames run as specified in character-format 7.3
(= act-format 4.11, 5.3, 5.4):
- one draw per random branch, in n/32768;
- `p = 0` is never taken in normal play;
- `daypart` uses the local hour (act-format 4.11 periods);
- `host` branches are never taken in v1, because bentwire has no host value.

**B-O97-2 Zero-duration frames.** A frame with `ms = 0`:
- is never drawn;
- still starts its sound;
- updates the logical pose (= act-format 5.2).

A frame without `pose` and with `ms > 0` blanks the character (= act-format 5.2).

**B-O97-3 Frame clock with coalescing** (= act-format 5.2) [OBS].
- The player keeps an absolute due time. On a tick that is late, the due time is reset
  to now.
- The player then consumes frames, adding each frame's `ms` to the due time, until the
  due time passes now + 14. Only the last consumed frame with `ms > 0` is drawn.
- The next tick is scheduled at `max(due - now, 14)`.
- Test vector: frames of 5, 5 and 100 ms at t = 0 draw only the 100 ms frame, and the
  next tick is at t = 110.

**B-SW-O Switch rule, office97** (= act-format 5.4, 5.5) [OBS].

When a new animation X is requested and the previous animation P has not ended (it was
stopped or is still running):
1. P runs in exit mode from its current frame:
   - the first step is scheduled 100 ms later;
   - frames longer than 99 ms last 100 ms;
   - every branch is resolved as `max(next, target)`: forward branches are taken and
     backward branches are skipped.
2. When P ends, the transition check runs against X's start pose. X's start pose is the
   pose of X's first frame, or none if the first frame has `ms = 0` and the next frame
   is blank.
3. If the last logical pose differs from X's start pose, the first entry of
   `office97.transitions` matching `[last, start, clip]` or `[last, rest_pose, clip]` is
   used. Its clip plays first, and the check repeats after the clip.
4. Then X starts.

Note: the one-slot queue of MSO97 is not used (see B-FAM-1).

**B-O97-4 Variants.**
- In an `office97` manifest an Office animation id is a state (a group). For example
  `Greeting = ["Greeting.1", "Greeting.2", "Greeting.3"]`, as found in
  `prototype/out/genius-act/character.toml`.
- A Play by name resolves the name to that state and picks a variant uniformly
  (= act-format 4.10 "rand() % count"). B-FB-1 step 2 does this.

**B-O97-5 Derived states.** When an `office97` manifest does not define one of the Agent
states, the daemon uses this map (from act-format 6, Agent state table):

| Agent state | office97 group, by Office 97 name / Office XP name |
|---|---|
| Showing | Appear / Show |
| Hiding | Disappear / Hide |
| GesturingLeft, GesturingRight, GesturingUp, GesturingDown | GestureLeft, GestureRight, GestureUp, GestureDown |
| Hearing | ListensToComputer / Hearing_1 |
| IdlingLevel1..3 | not used; idle is B-IDLE-O |
| Speaking, Listening, Moving* | none (missing state, R-STATE-2) |

**bentwire decision.** Reason: the prototype names ACT groups by the Office 97 type
library, while D6 (accepted) asks for Office XP names plus aliases. Accepting both
spellings makes the daemon independent of which converter version produced the
character.

#### 3.2.4 Which layer wins for each family

**B-FAM-1 Family split.** bentwire is an Agent-style server for both families. The
family key changes only the rows marked as differing. **bentwire decision** for every
row. Reason: D-Bus clients expect one queue and one event model whatever the character's
source.

| Aspect | `agent` | `office97` |
|---|---|---|
| Frame sequencing and branch draws | B-SEQ-A | B-O97-1 |
| Frame timing | R-ANIM-2 | B-O97-3 |
| Switching between animations | B-SW-A (exit, return) | B-SW-O (exit, pose transitions) |
| Request queue | R-Q-* (Agent FIFO) | same; MSO97's one-slot queue is not used |
| Idle | R-IDLE-1..5 (4 s, three levels) | B-IDLE-O (immediate replay of `office97.idle`) |
| States | manifest | manifest, else B-O97-5 |
| Sound gating | R-ANIM-8 | same |
| Show, Hide, MoveTo, GestureAt, Speak, Ask | section 3 | same |

#### 3.2.5 Pictures, shape and mouths

**B-PIX-1 Compositing.**
- The picture of a frame is composited exactly as in character-format 5.3. This includes
  office97 poses scaled from logical units.
- The picture is in RGBA at the display size, times the buffer scale (B-WL-6).

**B-PIX-2 Mouth overlays are clipped to the base frame.** [DOC] **bentwire decision**,
closing character-format 14 item 3.
- When a mouth variant is displayed, mouth-layer pixels are drawn only where the base
  picture has alpha > 0. The base picture is the frame's layers without the mouth.
- A mouth with `base` (the "replace top image" or "replace base frame" case) is not
  clipped. Its shape is the base list plus the mouth, intersected with the canvas
  (= acs-format 5.1, acf-aaf-format 8.1).
- Evidence: the Agent SDK states that the overlay is clipped to the base frame outline
  unless "Replace Base Frame Top Image" is set
  (https://learn.microsoft.com/en-us/windows/win32/lwef/animations and
  `creating-animations`). This agrees with agentdp2 using only the frame region when the
  replace flag is 0 (acs-format 5.1).
- This is a presentation step after compositing. The round-trip hashes of
  character-format 11 stay unclipped.
- Test vector: a synthetic frame with an opaque 10x10 square, plus a mouth layer that
  overlaps it by 5 px. Only the overlapping pixels change.
- Note: in v1 (no TTS) mouth variants are never displayed (B-LIP-2). The rule matters for
  later TTS and for tests.

**R-WIN-2 / B-PIX-3 Shape and input.**
- The visible and clickable shape of the character is the set of pixels with alpha > 0
  in the displayed picture (adapts agent-runtime R-WIN-2 [OBS]).
- For every converted ACS character this equals the stored region (character-format 9.1).

### 3.3 States and automatic state playback

**R-STATE-1 Random member.** A state plays one of its n members, chosen uniformly
(= agent-runtime R-STATE-1) [OBS].

**R-STATE-2 Missing state.**
- A state with no members, or with no available members, completes the request phase
  with success and plays nothing (= agent-runtime R-STATE-2) [OBS].
- Idle failure is handled by R-IDLE-5.
- Names are compared ignoring ASCII case.

**R-SHOW-1 Show** (= agent-runtime R-SHOW-1) [OBS] [DOC].
- If the character is hidden:
  1. map the surface;
  2. emit `Shown(cause)`;
  3. play Showing, unless `fast`.
- If it is already visible, the request completes with no animation and no signal.

**R-SHOW-2 Hide** (= agent-runtime R-SHOW-2) [OBS].
- If the character is visible:
  1. play Hiding, unless `fast`;
  2. unmap the character and every balloon;
  3. emit `Hidden(cause)`.
- If it is already hidden, the request completes with no signal.

**B-SHOW-3 Fast Show picture.** (Adapts agent-runtime R-SHOW-3.) **bentwire decision.**
- `Show(fast)` displays the first of:
  1. frame 0 of `RestPose`; for office97 this is the pose `office97.rest_pose`;
  2. the last frame of the first Showing member;
  3. an empty picture.
- Reason: the SDK warns that an empty frame may appear. Showing animations usually start
  empty and end neutral ([DOC] `agent-states`). The report's "first frame of Showing"
  would therefore usually be empty.

**B-SHOW-4 Cause codes.** (Adapts agent-runtime R-SHOW-1/2; D-Bus signals are broadcast
and cannot tell "own client" from "other client".) **bentwire decision.**

| Signal | Causes |
|---|---|
| `Shown` | 2 user (tray, menu), 4 program (including startup, character switch and gallery preview) |
| `Hidden` | 1 user via tray, 3 program, 7 user via character menu |
| `Move` | 1 user drag, 2 program (MoveTo, balloon fit B-OB-4), 4 display change |

### 3.4 Request queue

**R-Q-1 One FIFO per character** (= agent-runtime R-Q-1) [DOC] [OBS].
- These methods append a request and return its id at once: Show, Hide, Play, Speak,
  Ask, MoveTo and GestureAt.
- Requests run strictly in order.

**R-Q-3 Immediate calls** (= agent-runtime R-Q-3) [DOC].
- These act at once and return no request: Stop, StopAll, property access, Select and
  Quit.

**B-Q-1 Request ids and the method reply.**
- Ids are u32. They start at 1 and increase for the life of the daemon; 0 means "none".
- The reply to the method call MUST be sent before any signal about that request.

**B-Q-2 Events per request.**
- A request that starts gets exactly one RequestStart and one RequestComplete.
- A request removed before it starts gets only RequestComplete, with status 3.
- Statuses (= agent-runtime R-Q-5):
  - 0 complete;
  - 1 failed;
  - 3 interrupted.
- The reason string of RequestComplete is `""` on success, otherwise one of section 5.4.
- RequestStart, RequestComplete, AskComplete and Bookmark are sent only to the D-Bus
  unique name that made the call, because the SDK sends them only to the request's
  holder (= R-Q-5) [DOC].

**B-Q-3 Multi-phase requests.** (Closes agent-runtime U-13 [INF].) **bentwire decision.**
- MoveTo has the phases animation, slide and end (R-MOVE-1..3).
- Speak has the phases Speakable and output (R-SPEAK-1).
- Ask has the phases animation, display and answer (B-ASK-4).
- RequestStart is sent when the first phase begins. RequestComplete is sent after the
  last phase.
- A failing phase does not cancel the later phases (agent-runtime R-MOVE-4: the frame is
  still moved). The final status is failed with the reason of the first failing phase.
- An interrupted phase (Stop, StopAll, user Hide, character switch) cancels the
  remaining phases. The request ends with status 3. A stopped MoveTo leaves the frame
  where the slide stopped.
- Reason: this is the report's best reading, and it reproduces R-MOVE-4.

**R-Q-4 / B-Q-4 Stop.** (Adapts agent-runtime R-Q-4 [OBS] [DOC].)
- `Stop(0)` removes every queued request and stops the running request (both status 3,
  `org.bentwire.Error.Interrupted`). Exception: a Show or Hide that is running continues;
  its queued successors are still removed.
- `Stop(id)` removes or stops only that request. An unknown or finished id does nothing.
- Stopping also closes an open ask balloon (B-ASK-7). The ask then completes with result
  0 and status 3.

**B-Q-5 StopAll(types).**
- `types` is a subset of `play` (Play, GestureAt), `move` (MoveTo phases), `speak` (Speak
  phases) and `ask` (Ask). Unknown names give `org.freedesktop.DBus.Error.InvalidArgs`.
- An empty list stops everything, including a running Show or Hide
  (= agent-runtime R-Q-4) [DOC].

**R-Q-6 Looping animations** (= agent-runtime R-Q-6) [DOC] [OBS].
- An animation that never reaches END blocks the queue until it is stopped.
- Test vector: a 100 % backward branch never completes.
- `Stop(0)` then lets the next request run, after the loop's exit (B-SW-A).

### 3.5 Idle

**R-IDLE-1 Arming** (= agent-runtime R-IDLE-1) [OBS]. The idle timer is armed with
4000 ms when all of these hold:
- the queue becomes empty;
- IdleOn is true;
- the character is visible;
- the timer is not already armed.

**R-IDLE-2 Firing** (= agent-runtime R-IDLE-2) [OBS].
1. Disarm the timer.
2. If n = 0, emit `IdleStart`.
3. Pick the level: n < 10 gives IdlingLevel1; n < 20 gives IdlingLevel2; otherwise
   IdlingLevel3.
4. Play that state through the switch rule. This plays the return animation before idle
   ([DOC]).
5. On success, increment n.

**R-IDLE-3 Re-arming.** When an idle animation completes, the timer is armed again with
4000 ms (= agent-runtime R-IDLE-3) [OBS].

**R-IDLE-4 Reset** (= agent-runtime R-IDLE-4) [OBS]. Any newly queued request does the
following:
1. disarms the timer;
2. stops a running idle animation (B-ANIM-1);
3. emits `IdleComplete` if `IdleStart` was emitted;
4. sets n = 0.

Setting IdleOn to false does the same. Clicks, drags and balloons do not reset idle.

**R-IDLE-5 Missing idle level** (= agent-runtime R-IDLE-5) [OBS].
- If the chosen level has no members: n becomes 1 if it was 0, and the timer is re-armed.
- n is otherwise not advanced.

**B-IDLE-T Idle test vector** (agent-runtime 2.5, test).
- The queue is empty at t = 0, and every idle animation lasts 1000 ms.
- Idle picks happen at t = 4000, 9000, 14000 and so on.
- Picks 1 to 10 are level 1, picks 11 to 20 level 2, and picks from 21 on level 3.
- A Play at t = 9500:
  - stops the idle animation;
  - emits `IdleComplete`;
  - the next pick is level 1, at the Play's completion + 4000.

**B-IDLE-O Idle, office97.** (= act-format 5.6 [OBS], adapted by B-FAM-1.)
**bentwire decision.**
- The conditions of R-IDLE-1 apply, but the delay is 0 ms and there are no levels. When
  they hold, the state `office97.idle` is played.
- When it completes, it is replayed at once with a new variant. The replay waits at least
  100 ms after the previous idle animation began, so a variant made only of `ms = 0`
  frames cannot spin.
- If `office97.idle` is missing or has no available variant, R-IDLE-5 applies: the
  4000 ms retry, with n set to 1.
- `IdleStart` and `IdleComplete` follow R-IDLE-2 and R-IDLE-4.
- Reason: this is the original Office 97 behaviour inside the Agent event model.

### 3.6 Moving and gesturing

**R-DIR-1 Direction of a point relative to a rectangle** (= agent-runtime R-DIR-1) [OBS].
- Formula as in the report. Note: truncation is toward zero.
- Test vectors for the rectangle (100,100)-(228,228), computed with the report's formula:

| Point | Result |
|---|---|
| (0,164) | ScreenLeft |
| (400,164) | ScreenRight |
| (164,0) | Up |
| (164,400) | Down |
| (101,164) | ScreenLeft |
| (220,110) | ScreenRight |
| (164,227) | Down |
| (164,101) | Up |

**R-MOVE-1 Animation phase** (= agent-runtime R-MOVE-1, the quirk confirmed in
agentsvr!0x0101a2f7..0x0101a30f, where the point-in-rectangle test is half-open) [OBS].
- If the target equals the current top-left, there is no animation.
- If the target is outside the frame rectangle:
  - ScreenLeft gives MovingRight;
  - Up gives MovingUp;
  - ScreenRight gives MovingLeft;
  - Down gives MovingDown.
- If the target is inside (left <= x < right, top <= y < bottom): ScreenLeft gives
  MovingDown, and anything else gives MovingLeft.
- Vectors, frame (100,100)-(228,228):

| Target | Result |
|---|---|
| (0,164) | MovingRight |
| (400,164) | MovingLeft |
| (164,0) | MovingUp |
| (164,400) | MovingDown |
| (101,164) | MovingDown |
| (220,110) | MovingLeft |
| (150,150) | MovingLeft |
| (100,100) | none |

- Characters without Moving states play nothing (R-STATE-2). The Moving animation's last
  frame stays shown during the slide ([DOC] `agent-states`).

**R-MOVE-2 / B-MOVE-1 Slide phase** (= agent-runtime R-MOVE-2 [OBS], with one decision).
- Speed 0, or a hidden character, places the frame at once.
- Otherwise:
  - the timer period is `max(Speed / 25, 10)` ms (integer division);
  - each tick advances the major axis by `max(major_distance / 25, 25)` px, without
    overshoot;
  - the minor axis is on the straight line from start to target, truncated toward zero;
  - the target is reached exactly on the last tick.
- The 10 ms floor is a **bentwire decision**. Reason: Win32 SetTimer clamps periods below
  USER_TIMER_MINIMUM (10 ms); a 0 ms period is not meaningful.
- The balloons hide when the position actually changes (R-BAL-10, B-ASK-7).
- A drag that starts during the slide cancels it with `org.bentwire.Error.Dragged`.
- There is no clamping to the screen (R-WIN-4).
- Vectors, Speed 1000:
  - (0,0) to (100,0): (25,0) at t = 40, (50,0) at 80, (75,0) at 120, (100,0) at 160;
  - (0,0) to (1000,500): 25 ticks, (40,20) at 40, (80,40) at 80, target at t = 1000;
  - (0,0) to (30,200): (3,25), (7,50), (11,75), (15,100), (18,125), (22,150), (26,175),
    (30,200), at 40 ms steps.

**R-MOVE-3 End phase** (= agent-runtime R-MOVE-3) [OBS].
- Play the Speaking state, then emit `Move(x, y, 2)`.
- With Speed 0, the slide phase emits `Move` itself.

**R-MOVE-4 Hidden MoveTo** (= agent-runtime R-MOVE-4) [DOC] [OBS]. Phase 1 fails, the
frame still moves instantly, and the request ends failed (`Hidden`).

**R-GEST-1 GestureAt** (= agent-runtime R-GEST-1) [OBS].
- R-DIR-1 of the point against the frame rectangle:
  - ScreenLeft gives GesturingRight;
  - Up gives GesturingUp;
  - ScreenRight gives GesturingLeft;
  - Down gives GesturingDown.
- With the rectangle above, the points (0,164), (400,164), (164,0) and (164,400) give
  GesturingRight, GesturingLeft, GesturingUp and GesturingDown.
- No visibility pre-check (R-ANIM-9).

### 3.7 `say`: Speak and the Agent balloon

**R-SPEAK-1 Two phases** (= agent-runtime R-SPEAK-1).
- Phase 1 plays Speakable (B-PICK-A).
- Phase 2 shows the balloon and paces the text.

**R-SPEAK-2 Visibility.**
- Phase 2 on a hidden character fails with `Hidden`.
- Empty text completes at once with success (= agent-runtime R-SPEAK-2) [OBS].

**R-SPEAK-3 Alternatives.** Text with `|` is split, and one part is chosen uniformly
(= agent-runtime R-SPEAK-3) [OBS] [DOC].

**B-SPEAK-1 Tags.** [DOC]
- A tag is `\Name...\`, with a case-insensitive name. The known names are Chr, Ctx, Emp,
  Lst, Map, Mrk, Pau, Pit, Rst, Spd and Vol; inside a tag, `\\` is a literal backslash.
- Only Map and Mrk have an effect in v1, as on the text-only path:
  - `\Map="spoken"="balloon"\` shows the balloon part;
  - `\Mrk=n\` emits `Bookmark(n)` when the pacer reaches it.
- The other known tags are removed. A backslash that does not start a known tag is
  literal text.
- n must be in 1..2147483645. 0x7FFFFFFE and 0x7FFFFFFF are reserved
  (= agent-runtime R-Q-7). Violations give a method error `InvalidArgs`.

**B-SPEAK-2 Status.** Speak always completes with success in v1.
- Reason: R-SPEAK-4 gives 0 whenever "Play spoken audio" is on and there is no TTS
  voice. bentwire has no TTS and no such option.

**R-BAL-1 Style.**
- The balloon style comes from the manifest:
  - `balloon.enabled` (ACS flag 0x200 on, 0x100 off);
  - `size_to_text` (0x10000);
  - `auto_hide` (inverse of 0x20000);
  - `auto_pace` (inverse of 0x40000).
- The ACS flags are exactly the server's internal style bits (agent-runtime R-BAL-1 [OBS],
  acs-format 3.2.1).
- Lines 0 means 2 and chars per line 0 means 28 (= agent-runtime R-BAL-2, acs-format
  3.2.3).

**B-BAL-1 No balloon.**
- If `balloon.enabled` is false, or config `balloon = false`, no balloon is shown. The
  text is still paced, and BalloonShown is not emitted.
- Example: earl.acs has no balloon ([DOC] "Balloon.Enabled always False").

**B-BAL-2 Font.** **bentwire decision.**
- The font is the character's `balloon.font`, or config `balloon_font` when that is set
  (Agent option "a single font for all characters").
- It is resolved through fontconfig. If the family is missing, `sans-serif` is used.
- Size: em height = |font_height| px.
- TM.height is the font's ascent + descent in px, rounded.
- TM.aveCharWidth is OS/2 xAvgCharWidth scaled and rounded. Without OS/2, it is the
  advance of `x`.
- Reason: "MS Sans Serif" (all ACS files except qmark, which uses Arial -15) is not
  available on Linux. Metrics must come from the font actually drawn.

**R-BAL-3 Body size** (= agent-runtime R-BAL-3) [OBS].
- Width: W = CharsPerLine x aveCharWidth.
- Height: H = lines x (TM.height + 2) + 18.
- 10 px text inset.
- With SizeToText, lines = the wrapped line count, at most 128, reduced until H fits the
  output height.

**R-BAL-4 Shape.** A rounded rectangle with a 16 x 20 corner ellipse and a triangular
tail: tailLen 16, tailHalf 4. The fill is the background colour, with a 1 px border
(= agent-runtime R-BAL-4) [OBS]. The thought balloon is out of v1.

**B-BAL-3 Placement.** (Adapts agent-runtime R-BAL-5: the monitor rectangle M is the
logical rectangle of the output that holds the character surface, B-WL-2.)
- The rules are R-BAL-5 steps 1 to 5 unchanged:
  - above if there is room;
  - otherwise beside, left of the character if C.left > M.width / 2;
  - never below;
  - the anchor is the mouth point of the lowest mouth type present on the current frame,
    otherwise the frame centre, clamped into M;
  - the tail range limits apply.
- Test vectors (= R-BAL-5): M = 1024 x 768, character 128 x 128, W = 200, H = 54:
  - character at (448,400): balloon window (412,330)-(612,400), tail at the bottom;
  - character at (900,10): left 684, top 10, tail on the right;
  - character at (10,10): left 138, tail on the left.

**B-BAL-4 Mouth point without anchor.**
- A mouth without `anchor` (ACA and 1.5 characters,
  `prototype/out/merlin-web/character.toml`) uses the centre of the mouth layer's
  opaque bounding box, floored, in canvas coordinates.
- **bentwire decision**, closing character-format 14 item 4. Reason: in ACS, 275 of 291
  stored mouth points equal that centre within 1 px (acs-format 3.4).

**R-BAL-6 Word wrap.**
- Words are separated by spaces. A word that does not fit starts a new line. A word wider
  than a line is broken at the longest prefix that fits (= agent-runtime R-BAL-6) [OBS].
- Tab also breaks ([DOC]). RTL is out of v1.

**R-BAL-7 Showing.** The balloon appears empty, at its full size, before the first word,
and `BalloonShown` is emitted (= agent-runtime R-BAL-7). During a drag it is not shown.

**R-BAL-8 Pacing** (= agent-runtime R-BAL-8) [OBS].
- The first word appears at once. Each following word appears after
  `interval = 352 x 170 / clamp(170 x f, 50, 250)` ms (truncated), where f is config
  `speaking_speed` (0.5 to 1.5, default 1.0).
- The report's defaultSpeed is the voice's speed when the character has a TTS voice, else
  170. bentwire always uses 170. **bentwire decision.** Reason: without a TTS engine
  there is no voice on Linux, which is the server's "no TTS voice" case. A `[voice]`
  table in the manifest (earl, merlin) does not change this in v1.
- The output phase ends one interval after the last word, so N words last N x interval.
- With AutoPace off, the whole text appears at once and the end time is unchanged.
  **bentwire decision** for U-10. Reason: the report's reading; no other evidence.
- Test vector: "Hello big world" at f = 1:
  - words appear at 0, 352 and 704;
  - the request completes at 1056;
  - with AutoHide, the balloon hides at 2556.

**R-BAL-9 Scroll** (= agent-runtime R-BAL-9).
- When NumberOfLines lines are full, the content scrolls up by one line pitch and the
  oldest line is dropped.
- The scroll MUST be finished `60000 / effectiveSpeed` ms after it starts. The step
  pattern is not normative.

**R-BAL-10 Hiding** (= agent-runtime R-BAL-10) [OBS] [DOC].
- With AutoHide, the balloon hides 1500 ms after the end of speech.
- Without AutoHide, it stays until one of:
  - the next Speak;
  - Hide;
  - a click or drag on a character that is not speaking.
- In both cases it hides at once:
  - when a MoveTo actually moves;
  - at drag start;
  - when the character is hidden.
- `BalloonHidden` is emitted.

### 3.8 Lip sync without TTS

**R-LIP-1 Table.**
- Phoneme to mouth uses the 116-entry table in acs-format 5.4. An unknown phoneme keeps
  the previous mouth; the initial mouth is none (= agent-runtime R-LIP-1) [OBS].
- Implement it as a pure function, tested against the table.
- Vectors:
  - U+0020 gives 0;
  - `m` gives 0;
  - `f` gives 1;
  - `n` gives 1;
  - `t` gives 2;
  - U+0259 (ə) gives 3;
  - `a` gives 4;
  - U+0283 (ʃ) gives 5;
  - `o` gives 6;
  - `w` gives 6;
  - `Q` keeps the previous mouth.

**B-LIP-1 Amplitude quantisation** (= agent-runtime R-LIP-2) [OBS].
- Implement as a pure function for later TTS: `<0x10` gives `b`, `<0x30` gives `f`,
  `<0x50` gives `e`, `<0x70` gives `ɑ`, otherwise `ɜ`; silence gives `m`.
- Not driven in v1.

**B-LIP-2 Text-only Speak moves no mouth.** (Closes agent-runtime U-1 and R-LIP-3 [INF].)
**bentwire decision.**
- During text-only Speak the character holds its speaking frame without a mouth overlay.
- Reason: no mouth-setting call exists on the server's text pacer path. bentwire.md
  limits the table to optional TTS, and "Originaltreue vor Features" applies.

### 3.9 `ask`: Ask and the Office balloon

**B-ASK-1 Contract.** `Ask(a{sv})` takes the keys below, which follow the Office balloon
typelib (office-assistant 2, "What the balloon can contain"). All keys are optional except
that one of `heading`, `text`, `labels` must be present.

| Key | Type | Meaning |
|---|---|---|
| `heading` | s | bold heading |
| `text` | s | body text with markup B-ASK-3 |
| `icon` | s | `none` (default), `alert`, `tip`, `info`, `warning`, `query`, `critical` (MsoIconType 0, 2..7) |
| `labels` | as | up to 5 |
| `label_type` | s | `buttons` (default, clickable), `bullets`, `numbers` (display only) |
| `checkboxes` | as | up to 5, initially unchecked |
| `buttons` | s or as | s: a set name from B-ASK-2 (default `ok`); as: 1 to 6 custom captions |
| `mode` | s | `modal` (default) or `autodown`; `modeless` gives `org.bentwire.Error.NotSupported` |
| `animation` | s | played before the balloon; default `Explain`, resolved by B-FB-1; `""` = none |

That labels of type bullets and numbers are not clickable is a **bentwire decision**.
Reason: the typelib's BalloonType distinguishes clickable label buttons from list
display, and the public docs describe clicking a label only for button labels.

**B-ASK-2 Buttons and results** (= office-assistant R-OB-32) [OBS].
- Set names map to MsoButtonSetType 0..14:

| Set name | Type |
|---|---|
| `none` | 0 |
| `ok` | 1 |
| `cancel` | 2 |
| `ok-cancel` | 3 |
| `yes-no` | 4 |
| `yes-no-cancel` | 5 |
| `back-close` | 6 |
| `next-close` | 7 |
| `back-next-close` | 8 |
| `retry-cancel` | 9 |
| `abort-retry-ignore` | 10 |
| `search-close` | 11 |
| `back-next-snooze` | 12 |
| `tips-options-close` | 13 |
| `yes-all-no-cancel` | 14 |

- Button order is as in R-OB-32.
- Results:
  - button ids 0..14 return -1..-15 (OK -1, Cancel -2, Yes -3, No -4, Back -5, Next -6,
    Retry -7, Abort -8, Ignore -9, Search -10, Snooze -11, Close -12, Tips -13,
    Options -14, YesToAll -15);
  - custom buttons return -25, -26 and so on, in order;
  - a clicked label returns its 1-based index;
  - 0 means dismissed or interrupted (a **bentwire decision**; Office's value for this
    case was not observed).
- Esc activates Cancel or Close if the set has one (R-OB-32). Otherwise Esc does nothing
  in modal mode and dismisses in autodown mode.
- Enter activates the focused control. Focus starts on the first button.
- Tab cycles through labels, check boxes and buttons. Space toggles a check box.
- A mnemonic letter, with or without Alt, activates its control.

**B-ASK-3 Markup** (= office-assistant R-OB-16) [OBS].
- `{cf n}` sets the colour for n in {0..7, 248..255}, with the table in R-OB-16. Other
  values leave the colour unchanged.
- `{ul 1}` and `{ul 0}` toggle underline.
- `{bmp …}`, `{wmf …}`, `{pict …}`, `{dlx …}` and `{cbmp …}` are removed in v1.
- Any other `{…}` is literal text.

**B-ASK-4 Phases.**
1. If the character is hidden, the request fails with `Hidden` and the balloon does not
   appear (consistent with R-SPEAK-2).
2. Play `animation`, if it resolves.
3. Lay out the balloon (B-OB-1..4). If even its minimum size does not fit the output,
   the request fails with `org.bentwire.Error.BalloonTooBig` (Office TooBig).
4. Show the balloon. The ask now stays "in progress" outside the queue, and **the queue
   continues**, so idle resumes (R-IDLE-1).
5. On an answer:
   - hide the balloon;
   - emit `AskComplete(id, result, checked)`;
   - then emit `RequestComplete(id, 0, "")`.

Phase 4 is a **bentwire decision**. Reason: Office never put its balloon into Agent's
queue (office-assistant 1: MSO never calls Speak) and kept IdleOn on. The original
character therefore idled while its balloon waited.

**B-ASK-5 Validation.** These are method errors, `org.bentwire.Error.InvalidBalloon`, with
the Office error name in the message:
- more than 5 labels or check boxes: `TooManyControls`;
- modal with no buttons and no clickable labels: `ButtonlessModal`;
- check boxes with no buttons: `CheckboxesNeedButton` ([pub] "checkboxes need a button";
  this name is bentwire's);
- more than 6 custom buttons: `TooManyControls`.

**B-ASK-6 Modes.**
- `modal` closes only through a button, a clickable label or Esc (B-ASK-2).
- `autodown` also closes, with result 0, on:
  - a click on the balloon outside its controls;
  - a click on the character;
  - Esc;
  - a later client request (Show, Hide, Play, Speak, Ask, MoveTo, GestureAt) reaching
    execution. Requests the daemon makes itself (idle, Animate!, gallery preview) do not
    close it.
- A dismissed autodown ask completes with result 0 and status 0.
- Clicks elsewhere on the desktop cannot be seen on Wayland (B-WL-5). This is a
  documented deviation from Office's "closes on the next click elsewhere"
  (office-assistant 2, [inf] from memory).

**B-ASK-7 Interaction matrix.** While an ask balloon is open:

| Event | `modal` | `autodown` |
|---|---|---|
| Play, GestureAt (client) | run; the balloon stays | the ask is dismissed (B-ASK-6), then the request runs |
| MoveTo (client) | the balloon hides when the slide moves the frame, and is placed again and shown at the end of the slide | dismissed, then MoveTo runs |
| Speak, a second Ask | wait at the head of the queue until the open ask closes | dismissed, then the request runs |
| Hide request reaching execution | the ask closes: AskComplete result 0, status 3, reason `Hidden`; then Hide runs | same |
| User Hide (menu, tray) | as Hide, reason `HiddenByUser` (B-UI-4) | same |
| Drag | the balloon hides at drag start and is placed again at drag end | dismissed at drag start (a press on the character) |
| `Stop(0)`, `Stop(ask id)`, `StopAll([])`, `StopAll(["ask"])` | the ask closes, result 0, status 3, `Interrupted` | same |
| `StopAll` without `ask` | no effect on the ask | same |
| Character switch by a client or the menu | the ask closes, `CharacterChanged` (B-CHAR-5) | same |
| Idle, Animate! | run; the balloon stays | same |
| Output change | the balloon is placed again | same |

The gallery balloon (B-GAL-1) is internal and has the exceptions stated there.

**B-OB-1 Look and layout.** Content follows office-assistant R-OB-1..16 and R-OB-31..35:
- #FFFFCC background, text colours R-OB-2;
- Tahoma 13 px, or 16 px when the output's logical width is 1024 or more, resolved
  through fontconfig with fallback `sans-serif` (B-BAL-2 applies);
- 6 px inset;
- 32 x 32 icon with a 37 px heading indent;
- G spacing;
- label prefix column with a 17 px floor;
- uniform button width (widest caption rounded up to 5, plus 22), with 10 px gaps,
  centred, 22 or 24 px high;
- size by area with the 0.618 ratio.

Art: the code repo MUST NOT ship Microsoft bitmaps. Buttons, label bullets and icons are
drawn by bentwire with the stated sizes and states:
- the button is a 1 px rounded outline on the balloon colour with 2 px corner cuts,
  colours from R-OB-11;
- bullets are 15 x 15;
- icons are 32 x 32 own artwork.

**B-OB-2 Frame** (= office-assistant R-OB-20..27) [OBS].
- 8 px corner radius;
- 1 px black outline around the body and tail;
- #FFFFCC fill;
- the tail is a right triangle with a 10 px base and 16 px length, at 70 % (bottom) or
  min(70 % H, 30 % C.height) (side), clamped as specified;
- no shadow;
- a 4 px gap.

**B-OB-3 Placement.** (Adapts office-assistant R-OB-17 and R-OB-34; the work area is the
output rectangle, B-WL-1.)
- Candidates in R-OB-34 order: above on the preferred side, above on the other side,
  beside on the preferred side, beside on the other side. The preferred side is "balloon
  left of the character" when the character's centre is in the right half of the output.
- Score = the balloon pixels outside the output.
- The first candidate with score 0 wins, otherwise the lowest score, with ties going to
  the earlier candidate.
- Of the two size shapes, the landscape shape is tried first. The portrait shape is used
  only if the landscape shape has no score-0 candidate.
- **bentwire decision** for the scoring. Reason: the report infers "overflow in pixels";
  the exact formula (0x324A809E) is unknown.

**B-OB-4 Fit fallback.** **bentwire decision** for the amount.
- If no candidate scores 0, the lowest-score candidate is used. The character is moved
  by the smallest vector that brings that balloon inside the output, with `Move(…, 2)`.
- This follows R-OB-17 [inf]; the exact amount is unknown (office-assistant Unknowns).

### 3.10 User interaction

**R-UI-1 / B-UI-1 Click and double-click.** (Adapts agent-runtime R-UI-1 [OBS].)
- A press then release with no drag gives `Click(button, modifiers, x, y, false)`.
- A press within `double_click_ms` (config, default 500, the Windows default) of the
  previous press gives `DblClick` instead, and its release gives nothing. There is no
  distance check [OBS].
- Buttons: 1 left, 2 right, 4 middle.
- Modifiers: 1 Shift, 2 Ctrl, 4 Alt.
- A right click also opens the menu (AutoPopupMenu is always true in v1).
- Clicking does not stop output ([DOC]).
- Double-click has no built-in action besides the signal. **bentwire decision.** Reason:
  the Agent server defines none [OBS]; Office's DblClick handler was not analysed; the
  Office single-click search balloon is out of v1.

**R-UI-2 Drag** (= agent-runtime R-UI-2) [OBS].
1. A drag starts when the pointer moves at least 2 px on either axis while the left or
   right button is held.
2. At drag start:
   - balloons hide;
   - a running slide is cancelled (R-MOVE-2);
   - `DragStart(button, modifiers, x, y)` is emitted.
3. The top-left follows the pointer 1:1. There is no clamping, and the animation is not
   changed.
4. On release:
   - `DragComplete(…)` is emitted;
   - `Move(x, y, 1)` is emitted.
5. A right-button drag does not open the menu.

**R-UI-3 / B-MENU-1 Character menu.** (Adapts agent-runtime R-UI-3 [OBS] and
office-assistant 4 [obs].)
- The menu is built fresh for each opening. Only one exists at a time.
- The chosen item runs after the menu closes.
- Items, in this order:
  1. "&Hide" (character visible). There is no voice item, because there is no speech
     input.
  2. A separator ([DOC]: a separator only if both server and client entries exist).
  3. "&Options...".
  4. "&Choose Assistant...".
  5. "&Animate!", only if B-MENU-4's list is not empty.
- That Hide comes first and the Office items after it is a **bentwire decision** resting
  on R-UI-3's build order (item 3 there is [INF]).

**B-MENU-2 Menu strings.** The UI language is chosen by B-L10N-1.

| Item | English | German | Provenance |
|---|---|---|---|
| Hide | `&Hide` | `&Ausblenden` | verbatim, agt0409/agt0407 string 182 |
| Show (tray, hidden) | `&Show` | `&Einblenden` | verbatim, string 183 |
| Options | `&Options...` | `&Optionen...` | verbatim, MSOINTL 10.0.2609 (1033/1031) string 0x60015 |
| Choose Assistant | `&Choose Assistant...` | `Assistenten &wählen...` | verbatim, MSOINTL 0x60016 |
| Animate | `&Animate!` | `A&nimation!` | verbatim, MSOINTL 0x60017 |

**B-MENU-3 Options...** writes a default config file if none exists, then opens it with
`xdg-open`. **bentwire decision.** Reason: the options dialog is out of v1, but the item
belongs to the original menu.

**B-MENU-4 Animate!** (office-assistant 1 [obs] for XP's shuffled list; act-format 5.8
[OBS] for the list.)
- The list is the Office 97 Animate! ids mapped to XP names, filtered to animations or
  office97 groups the character has, using B-FB-1 steps 1 to 3 (name, group, alias) but
  not the fallback chain, so Office 97 spellings match too: Greeting, Explain,
  Congratulate, GetAttention, Wave, Searching, Print, GestureRight, Writing, Processing,
  Thinking, SendMail, Hearing_1, Show, GetArtsy, GetTechy, GetWizardy, CheckingSomething,
  Save, GestureDown, GestureLeft, GestureUp, EmptyTrash.
- The list is shuffled (B-CORE-2). Each choice queues a Play of the next entry. When all
  are used, the list is shuffled again.
- XP's 5 s Idle timer is not reproduced; R-IDLE applies.

**B-GAL-1 Choose Assistant (gallery).** **bentwire decision.**
- The gallery is an Office balloon, mode modal, next to the character:
  - heading = the candidate's name;
  - text = its description;
  - custom buttons `<&Back`, `&Next>`, `OK`, `Cancel`. German: `<&Zurück`, `&Weiter>`.
    Both verbatim: Back and Next from the MSOINTL 10.0.2609 (1033/1031) Assistant dialog
    template (type 221, id 12), OK and Cancel from OBALLOON 112, 113.
- Candidates are the installed characters in id order. Back and Next are disabled at the
  ends.
- Back and Next switch the live character (B-CHAR-5) and queue Play `Wave` (B-FB-1) as
  the preview.
- Exceptions to the general Ask rules, which apply only to the gallery balloon:
  - Back and Next do not close the balloon (unlike B-ASK-2);
  - disabled buttons are drawn in the disabled state and ignore clicks and mnemonics;
  - the balloon survives the gallery's own character switches (unlike B-CHAR-5 and
    B-ASK-7) and is re-anchored to the new character;
  - it is not a client request: no request id and no signals other than `Shown`,
    `Hidden`, `Move` and the balloon signals.
- Opening the gallery while a client ask is open first closes that ask with result 0,
  status 3, `Interrupted`. While the gallery is open, client Speak and Ask wait as in
  B-ASK-7 (modal).
- OK keeps and persists the choice. Cancel or Esc switches back to the character that was
  shown when the gallery opened. If the user hides the character, the gallery closes as
  Cancel.
- Reason: Office's gallery previewed Wave and then IdlingLevel1 in a dialog. The live
  character is the closest preview available without a dialog toolkit.

**B-UI-4 User Hide.** **bentwire decision.**
- Hide from the menu (cause 7) or tray (cause 1) first interrupts every request of the
  character with `org.bentwire.Error.HiddenByUser` (Agent 0x8004210D, agent-runtime
  R-Q-4). It then runs an animated Hide.
- Reason: a queued Hide behind a looping animation would never run. That the cause code
  exists implies this flush.

**R-UI-4 / B-TRAY-1 Tray icon.** (Adapts agent-runtime R-UI-4 to StatusNotifierItem.)
- One item for the loaded character, with:
  - Id `bentwire`;
  - Title and ToolTip = the character name, cut to 60 characters + "..." when it has 64
    or more;
  - IconPixmap = `icon.png` of the character, or bentwire's own icon.
- `Activate` (left click) shows the character (cause 2) and emits
  `Click(1, 0, x, y, true)`.
- `SecondaryActivate` (middle click) emits `Click(4, 0, x, y, true)`.
- The context menu is supplied as dbusmenu:
  - when visible, the items of B-MENU-1;
  - when hidden, only `&Show` ([DOC] `pop-up-menu-support`).
- SNI hosts show the menu themselves, so a right click on the tray emits no `Click`, and
  tray double-click is not available. These are documented deviations.
- The 0x1000 / 0x2000 flag is replaced by `from_tray` (section 4, item 6).

### 3.11 Window behaviour on Wayland

All rules here describe requests to the compositor. The headless backend records them so
tests can assert them (section 6).

**B-WL-1 Coordinate space and areas.**
- Positions are global logical coordinates (xdg-output).
- An output rectangle M is its logical position and size.
- The "work area" of the Office balloon is also M: Wayland gives clients no way to learn
  panel areas. This is a deviation.

**B-WL-2 Character surface.** (Adapts agent-runtime R-WIN-1.)
- One `zwlr_layer_surface_v1` per character, with:
  - layer `top`;
  - namespace `bentwire`;
  - anchor top|left;
  - `exclusive_zone = -1` ("would not like to be moved to accommodate for other
    surfaces", so margins count from the output edge);
  - margins = top-left minus the output origin;
  - size = the display size;
  - `keyboard_interactivity = none`.
- The output is the one containing the frame centre, else the nearest.
- It is not in the taskbar and has no decorations.
- Requires `zwlr_layer_shell_v1` version 4 or later (`on_demand`, B-WL-8). If layer-shell
  is missing, `bentwired` exits with a clear message.

**B-WL-3 Z-order.** Layer `top` sits above normal windows. Reason: agentsvr drops topmost
while a full-screen application runs (R-WIN-1 [INF]); compositors usually draw full-screen
windows above the top layer. KWin verification: section 6, item K2.

**B-WL-4 Positions are not clamped.** (= agent-runtime R-WIN-4 [OBS] for the model.)
- Left and Top are never clamped by MoveTo or drag.
- The backend requests exactly that position. Parts beyond the surface's output are not
  shown, because a surface belongs to one output.
- The output is chosen again (by recreating the surface) when the centre lies on another
  output after a drag or slide ends. Never during a drag, because recreating would break
  the implicit pointer grab.
- On an output change (added, removed or resized), the frame is pulled inside M of the
  output holding its centre, or else the first output: right and bottom edges first, then
  left and top. Then `Move(x, y, 4)` is emitted (= R-WIN-4).

**B-WL-5 No global pointer.** Wayland reports pointer positions only on bentwire's own
surfaces. Consequences:
- event coordinates are the surface position plus the local position;
- the menu opens at the local pointer position;
- clicks outside bentwire are not observable (B-ASK-6).

Nothing else in v1 needs the global pointer.

**B-WL-6 Shape, scale, mapping.**
- The input region is the shape (B-PIX-3) of the current picture and is updated with
  every displayed frame. A hidden character is unmapped (null buffer).
- Buffers are wl_shm ARGB8888 with straight-to-premultiplied conversion.
- Buffer scale = ceil(output scale), with nearest-neighbour integer upscaling, or the
  `hidpi` sheet when present. A fractional scale is reached through viewporter. The
  logical size stays the display size.

**B-WL-7 Drag implementation.**
- Drags use the implicit grab of the pressed button.
- The new position is computed from the grab offset. The backend MUST compensate for
  margin changes that the compositor has not yet applied, so the motion stays 1:1.

**B-WL-8 Balloon surfaces.**
- Balloons are `wl_subsurface`s of the character surface, placed above it and positioned
  relative to it. Their input region is the balloon outline.
- Clicks on the Agent balloon do nothing.
- While an ask balloon is open, the layer surface uses `keyboard_interactivity =
  on_demand`, so the balloon gets keyboard focus after the user clicks it. Otherwise
  `none`.
- **bentwire decision.** Reason: a subsurface guarantees z-order and moves with the
  character. A second layer surface has no stacking guarantee within the layer.

**B-WL-9 Menu surface.**
- The menu is an `xdg_popup` attached with `zwlr_layer_surface_v1.get_popup`. It is
  grabbed with the serial of the right-button press and positioned at the pointer, with
  flip and slide constraint adjustment (Agent clamps to the virtual screen).
- `popup_done` closes it without a choice.
- Drawn by bentwire, XP style. Keyboard: arrows, Enter, Esc and mnemonics.

**R-WIN-5 Focus.** The character and balloons never take focus or activate, except
B-WL-8 (= agent-runtime R-WIN-5).

### 3.12 Missing-animation fallback

**B-FB-1 Name resolution for Play** (and for `animation` in Ask, Animate! and the
gallery). The first step that finds something wins:
1. An animation with that name, ignoring ASCII case.
2. office97: a state (group) with that name; pick a variant (B-O97-4).
3. The alias table: for each pair below, try the other spelling (steps 1 and 2). Pairs
   are Office 97 type-library name / Office XP name, from office-assistant 1 and
   act-format 6:
   - Idle/Idle1_1
   - BeginSpeaking/Explain
   - CharacterSuccessMajor/Congratulate
   - GetAttentionMajor/GetAttention
   - GetAttentionMinor/Wave
   - Printing/Print
   - WritingNotingSomething/Writing
   - WorkingAtSomething/Processing
   - SendingMail/SendMail
   - ListensToComputer/Hearing_1
   - Disappear/Hide
   - Appear/Show
   - Saving/Save
4. The fallback chain below, each candidate through steps 1 to 3. `state:X` plays state X.
5. Nothing found gives the method error `org.bentwire.Error.UnknownAnimation`. The
   message names the request and lists the chain that was tried.

The Agent behaviour for an unknown name is a raised error ([DOC] `play-method`). Steps 3
and 4 are a **bentwire decision** (bentwire.md, office-assistant 1: "fallback map"), so
that clients written for Office names work with every character.

| Requested (XP name) | Fallback chain |
|---|---|
| Idle1_1 | Idle, Idle (1), state:IdlingLevel1 |
| Greeting | Greet, Wave, GetAttentionMinor, Welcome |
| Goodbye | Wave, GetAttentionMinor |
| Explain | Announce, Acknowledge, Suggest |
| Congratulate | CharacterSucceeds, Pleased |
| GetAttention | GetAttentionMinor, Wave, Alert |
| Wave | GetAttentionMinor, Greet |
| Searching | Search, ImageSearching |
| Print | Printing, Processing, Writing |
| Writing | Write |
| Processing | Process, Thinking, Searching |
| Thinking | Think |
| SendMail | Writing, Write |
| Hearing_1 | ListensToComputer, state:Hearing |
| Show | state:Showing |
| Hide | Disappear, state:Hiding |
| GetArtsy | Writing, Write |
| GetTechy | Processing, Process, Thinking |
| GetWizardy | DoMagic1, Thinking, Think |
| CheckingSomething | Searching, Search, Thinking |
| LookDown, LookUp, LookLeft, LookRight | the same name + `Blink` |
| LookDownLeft, LookDownRight, LookUpLeft, LookUpRight | vertical look, its Blink, horizontal look, its Blink (e.g. LookUpLeft: LookUp, LookUpBlink, LookLeft, LookLeftBlink) |
| Save | Writing, Processing |
| GestureRight, GestureLeft, GestureUp, GestureDown | state:GesturingX, PointX |
| EmptyTrash | Processing, Thinking |
| RestPose | (none) |

Goodbye does not fall back to Hide. Reason: playing Hide through Play ends on Hide's last
frame while the window stays mapped, which looks like a vanished but clickable
character.

**B-FB-2 Test vectors.** Computed from the names and states of the user's ACS files with
the acs-format parser:

| Character | Requested | Resolved |
|---|---|---|
| rover | Greeting | Greet |
| rover | Explain | Acknowledge |
| rover | Goodbye | GetAttentionMinor |
| rover | Print | Writing |
| rover | Hearing_1 | error |
| rover | GestureUp | state:GesturingUp |
| courtney | Idle1_1 | Idle (1) |
| courtney | Processing | Thinking |
| courtney | LookUpRight | LookUp |
| earl | Print | Printing |
| earl | Hearing_1 | ListensToComputer |
| earl | GetAttention | GetAttentionMinor |
| merlin | Goodbye | Wave |
| merlin | GetWizardy | DoMagic1 |
| merlin | EmptyTrash | Processing |
| merlin | LookDownLeft | LookDown |
| qmark | Greeting | Welcome |
| qmark | GestureLeft | state:GesturingLeft |
| qmark | Hide | state:Hiding |
| qmark | Wave | error |
| qmark | Show | error |
| any Office ACS | any of the 35 names | itself |

### 3.13 Localization, configuration, persistence

**B-L10N-1 UI language.**
- Config `language` (`auto`, `de`, `en`). `auto` takes the first of `LC_ALL`,
  `LC_MESSAGES`, `LANG`: German if it starts with `de`, otherwise English.
- It applies to the menu, tray, balloon buttons and character names.

**B-L10N-2 Office balloon button captions.**
- German: verbatim from OBALLOON.DLL 10.0.2607 RT_STRING 112..126. These strings were
  dumped for this spec and match R-OB-11:
  - `OK`
  - `Abbrechen`
  - `&Ja`
  - `&Nein`
  - `&Zurück`
  - `&Weiter`
  - `&Wiederholen`
  - `&Abbrechen`
  - `&Ignorieren`
  - `&Suchen`
  - `&Erneut erinnern`
  - `Schließen`
  - `&Tipps`
  - `&Optionen`
  - `Ja, &alle`
- English: verbatim from the English (1033) OBALLOON.DLL 10.0.2607 RT_STRING 112..126
  (Office XP CD, public archive X09-23849, OFFICE1.CAB):
  - `OK`
  - `Cancel`
  - `&Yes`
  - `&No`
  - `&Back`
  - `&Next`
  - `&Retry`
  - `&Abort`
  - `&Ignore`
  - `&Search`
  - `&Snooze`
  - `Close`
  - `&Tips`
  - `&Options`
  - `Yes To &All`
- CLI result names: `ok`, `cancel`, `yes`, `no`, `back`, `next`, `retry`, `abort`,
  `ignore`, `search`, `snooze`, `close`, `tips`, `options`, `yes-to-all`.

**B-CFG-1 Config file.** `$XDG_CONFIG_HOME/bentwire/config.toml`. All keys are optional:

| Key | Default | Source |
|---|---|---|
| `character` | unset | B-CHAR-4 |
| `language` | `"auto"` | B-L10N-1 |
| `sound_effects` | `true` | Agent "Play character sound effects", Office "Make sounds" |
| `balloon` | `true` | Agent "Display spoken output in word balloon" |
| `balloon_font` | `""` (each character's font) | Agent "A single font for all characters" |
| `speaking_speed` | `1.0` (0.5 to 1.5) | Agent speed slider factor (R-UI-5) |
| `idle` | `true` | initial IdleOn |
| `double_click_ms` | `500` | B-UI-1 |

The daemon reads the config file at start and when it receives SIGHUP.

**B-CFG-2 State file.** `$XDG_STATE_HOME/bentwire/state.toml`:
- `character`, `left`, `top`, `visible`;
- written at most once per second after a change, and at exit.

**B-CFG-3 First run.** **bentwire decision.**
- Position: right edge 48 px and bottom edge 96 px inside the first output (clears a
  typical panel).
- Visible.
- At start, a visible character is shown with an animated Show (cause 4).

**B-RUN-1 Daemon lifecycle.**
- `bentwired` requests the bus name `org.bentwire.Agent` with DO_NOT_QUEUE. If the name
  is taken, it exits with status 1 and the message "already running".
- A D-Bus activation file (`org.bentwire.Agent.service`) lets the CLI start it.
- SIGTERM and `Quit` save the state and exit 0 without animation.

### 3.14 Decisions added during the build

Each item is a **bentwire decision** made while building v1, where the docs above left a
gap. Details and reasons are in `docs/architecture.md` section 2 where marked.

- **A-PACK-1** Packs and search roots extend B-CHAR-1 (`pack.toml`, config
  `character_paths`, `--characters`). Reason: the brief requires packs; B-CHAR-1 only
  knows flat character entries.
- **A-ID-1** Unique ids in `classics` (`clippit` = Office XP Clippit; suffixes `-web`,
  `-15`, `-97`, `-bob`, `-act`). Reason: several sources carry the same character name.
- **A-PACK-2** `classics` holds a source iff it converts and `source.missing_animations`
  is empty. Reason: D8 "only complete characters", decided in code, not by list.
- **A-CHAR-1** With only `builtin` installed, B-CHAR-4 step 4 selects
  `bentwire-placeholder`.
- **A-THEME-1** The Office balloon theme is pack data with a generic loader; the
  look-alike of B-OB-1 is the fallback.
- **A-TEST-3** `bentwired --test-interface` exports a reduced `org.bentwire.Test1` on the
  Wayland backend. Reason: the desktop scenario must answer `ask` without a human.
- **B-REN-1 Text.** Glyphs are drawn advance-only (no kerning or shaping), each advance
  rounded to whole logical px, so measured and drawn widths agree; antialiased greyscale
  coverage, no hinting. Ascent and descent are OS/2 usWinAscent/usWinDescent (GDI
  tmAscent/tmDescent; hhea if there is no OS/2 table). A positive LOGFONT height is the
  cell height (em = h x unitsPerEm / (winAscent + winDescent)), a negative one the em.
  A missing italic face is sheared by 0.2. fontconfig is queried with `fc-match`; when
  the returned family differs from the requested one, `sans-serif` is used; without
  fontconfig, the bundled Tahoma. Text is rasterised at the buffer scale. Reason: GDI
  draws Latin text without kerning; B-BAL-2 leaves the details open.
- **B-REN-2 Office look-alike art.** Buttons: outline #868686 (normal), black (hot, with
  white/#C0C0C0 inner bevel; pressed, #E7E7D6 fill, caption +1 px; focused, plus a dotted
  focus rectangle inset 3 px), #C0C0C0 with #808080 caption (disabled). Check boxes in
  the XP look (#1C5180 border, green tick, orange inner ring when hot). Hot clickable
  labels are drawn underlined (R-OB-4). `alert` uses the same exclamation-triangle icon
  as `warning` (R-OB-5 lists only four stock icons for five alert kinds). Reason:
  R-OB-11 gives colours but only a guessed state order; no bitmaps may be shipped.
- **B-REN-3 Menu metrics** (XP flat menu look): Tahoma em 11 px, item height 17,
  separator 9 (1 px #ACA899 line), 1 px #ACA899 border plus 2 px padding, white
  background, text at x = 21 with a 21 px right margin, minimum width 100, highlight
  #316AC5 with white text; mnemonics are always underlined. Reason: B-WL-9 says "XP
  style" without metrics.
- **B-REN-4 HiDPI shape.** For a picture above 1x, a logical pixel belongs to the input
  region when any buffer pixel it covers has alpha > 0. Reason: B-WL-6 keeps logical
  sizes; for 1x pictures this equals B-PIX-3.
- **B-REN-5 Headless frames** (T-4 `--dump-frames`): one RGBA PNG per output at
  ceil(output scale), transparent background, z-order character, Agent balloon, Office
  balloon, menu. Reason: the dump shows what bentwire draws, not a desktop.
- **B-REN-6 Balloon outlines.** Body and tail are GDI-style regions (rounded rectangle
  sampled per row at the pixel centre; polygon with right and bottom edges excluded);
  the border is the region's inner 1 px frame (FrameRgn). The same region is the
  balloon's input region (B-WL-8).
- **B-REN-7 Theme format** (A-THEME-1): `balloon-themes/<id>/theme.toml` with
  `format = 1`, `[buttons.small]` / `[buttons.large]` (`left`, `middle`, `right` PNG
  strips, `frames` naming normal, default, hot, pressed, disabled), `[bullets]` (`label`,
  `label_frames`) and `[icons]` (alert, tip, info, warning, query, critical; 32 x 32).
  Masks are folded into alpha. A button strip is used when its height equals the button
  height; anything missing falls back to B-REN-2.
- **A-FMT-1 Reader limits** (agent-formats). Decompressed ACF and ACA/AAF bodies above
  256 MiB, images and regions above 10,000,000 bytes (the runtime's image cap), and
  strings above 0x1000000 units (the 2.0 limit, also applied to 1.5 data) are errors.
  Counts are checked against the remaining bytes before allocating, and all bytes decoded
  or copied out of one file (images, sounds, bodies, streams) are capped at 1 GiB in total,
  so table entries that all point at one large record cannot multiply memory. Reason:
  hostile input must not exhaust memory; real files are far below these limits (largest
  sample about 20 MB decoded).
- **A-FMT-2 Strict records.** Every ACS block and record and every ACF and ACA/AAF body
  must be consumed exactly; decoded pixels must be exactly `stride * height` and regions
  exactly their declared size (the runtime ignores the decoder's result and can show stale
  bytes). ACA/AAF image and overlay sizes must equal `stride * height` of their picture.
  Regions must be RDH_RECTANGLES with `32 + 16 * nCount == size`. Bytes after the
  compressed block of an ACF or ACA file are ignored. Reason: every sample satisfies
  these rules and the research parsers enforce them; anything else is corrupt, not a
  variant to guess about.
- **A-FMT-3 Edge cases without samples.** ACS image kind 0 is "no image" and nothing after
  the kind byte is read; other kinds and compression flags other than 0/1 are errors. Raw
  ACS images use `stride * height` bytes and ignore the data size (as the runtime does).
  Mouth type above 6 is an error in ACA/AAF too (the ACS rule). Strings that are not valid
  UTF-16 are errors. Reason: follow the runtime where the research traced it, fail loudly
  where it did not.
- **A-FMT-4 Per-animation failure.** In Agent 1.5 OLE files each animation stream is parsed
  on its own and a missing or damaged stream is reported for that animation only; the
  web loader reports missing files, unparsable files and stale checksums per animation.
  OLE streams' checksums are not compared (the runtime does not). Reason: the runtime
  loads animations lazily (agent15-format 8.2).
- **B-REN-8 Office font size** (clarifies B-OB-1, ruled by the lead): the Office balloon's
  13 / 16 px are the Tahoma cell height (positive lfHeight, R-OB-4), so TM.height is 13 / 16
  and G is 6 / 8. B-BAL-2's em rule applies to the Agent balloon's negative ACS heights.
- **A-CORE-1 Office balloon sizing details** (R-OB-31 leaves them [inf]). Area terms:
  heading block (37 + heading width with an icon) x max(32, heading height); each text line
  unwrapped width x TM.height; each label and check-box row (text width + prefix column +
  17) x max(TM.height, 15); button row (width + 12) x button height. Minimum body: width
  12 + max(button row, 47 with an icon, prefix column + 15); height 12 + 32 (icon) +
  button height, at least 12 + TM.height. A shape grows until its content area (w - 12) x
  (h - 12) reaches the area. The shape fixes only the width: the body height is the
  laid-out content height + 12. When neither shape has a score-0 candidate, the lowest
  score over both shapes wins, ties to the landscape shape and the earlier candidate.
  Reason: the report gives the method, not these constants.
- **A-CORE-2 Office balloon controls.** Check-box rows use the label prefix column (13 x 13
  glyph centred) and start their text at the column + 5 px, like labels. `&` mnemonics
  apply to button and check-box captions; label text is plain. Markup (B-ASK-3) applies
  to `text` and labels, not to the heading or check boxes. Space only toggles a focused
  check box. Unknown `icon`, `label_type`, `mode` or set names and an empty custom
  button list are `InvalidArgs`. Reason: native check boxes and buttons take mnemonics;
  labels are owner-drawn rows (R-OB-7).
- **A-CORE-3 Small engine choices.** Move(..., 4) after an output change only when the
  position changed. An interrupted Speak hides its Agent balloon at once. A double-click
  is any second press within `double_click_ms`, whatever the button; DragStart carries
  the pointer position at drag start. Select of the current id reloads it (B-CHAR-5
  applies). A program Hide that reaches execution closes the gallery as Cancel; Stop and
  StopAll do not touch the gallery. A state member choice always takes a draw, also for
  a single member (`rand() % n`). The Speak alternative is drawn when the request starts.
  A Hide reaching execution closes an autodown ask with status 3 (the B-ASK-7 row wins
  over B-ASK-6). A drag that cancels a slide (or is running when the slide would start)
  fails it with `Dragged`; the end phase still plays Speaking but emits no Move, and the
  output and the ask balloon wait for the drag end. Hiding while the gallery shows
  another character switches back without a Show, so the Hide runs on the original.
  The Animate! list is shuffled on first use, not at load. An Ask interrupted before its
  balloon opens also gets AskComplete (result 0) before RequestComplete, and opening the
  gallery interrupts an Ask still in its animation phase. Re-laying out an open Office
  balloon after hover, focus or check-box changes does not move the character again
  (B-OB-4 applies once per placement); an ask placed during a drag appears at drag end.
  Reason: the spec is silent;
  these are the simplest consistent readings.
- **A-CORE-4 office97 switch details.** The transition check (B-SW-O steps 2 to 4) also
  runs when the previous animation ended normally, not after Showing, and chains at most
  8 clips per switch. Both steppers consume at most 1000 frames per timer callback; a
  cycle of 0 ms frames then yields (agent: 10 ms, office97: the 14 ms minimum tick).
  Reason: MSO97 checks when the queued animation starts; the caps prevent cyclic data
  from hanging the daemon.
- **A-CORE-5 Agent balloon tail and scroll.** The tail tip lies on the window edge at the
  clamped anchor coordinate, and its base (2 x tailHalf) is centred on the tip coordinate.
  The scroll (R-BAL-9) moves one pixel at a time, evenly over `60000 / effectiveSpeed` ms;
  the outgoing top line stays drawn and slides out, and is dropped when the scroll ends.
  Reason: R-BAL-4/R-BAL-9 give lengths and duration, not these shapes.
- **A-CORE-6 Exit runs end on a revisit.** In exit mode (B-SW-A case 1) a frame without an
  `exit` index continues normal sequencing; when that would show a frame the same exit
  run already showed, the animation ends instead. Reason: a loop without exit branches
  (allowed by the format) would otherwise block the queue forever after Stop or user
  Hide (R-Q-6, B-UI-4); real exit paths are forward chains and are not affected.
- **A-CHAR-2 Loader strictness.** `Character::load` rejects a manifest with type, range or
  cross-reference errors (the validator's rules); unknown non-`x-` keys and tolerated ZIP
  deviations (media entry not stored, manifest not first) are warnings; a sheet, sound or
  icon that fails to decode only disables the animations using it (B-CHAR-2). Besides
  8-bit indexed and RGBA, the loader accepts any PNG colour type and depth (indexed stays
  indexed, others become RGBA). Reason: character-format 4 makes unknown keys a warning,
  and editors export other PNG variants.
- **A-CHAR-3 HiDPI file choice.** Per layer, the file (base = key 1, or a `hidpi` key) with
  the smallest key whose picture (base size x key) covers the destination rectangle of
  character-format 5.3 step 3, else the densest decoded file; `cell` and `sizes` are pixels
  of the base file and are multiplied by the key; a picture's canvas size is its base size
  divided by `scale`, truncated. Reason: character-format 8 defines the files, not the
  choice; deciding per destination rectangle also works for office97 canvases in logical
  units.
- **A-CHAR-4 Sanity limits.** The parser rejects canvas, display, cell and `sizes` sides
  above 16384, explicit layer sizes above 262144, more than 2^24 pictures per sheet and
  `scale` above 64; `compose` returns an empty picture for outputs above 2^28 pixels and
  clamps the density to 1..64. A PNG declaring more than 131072 px per side or 2^28
  pixels is rejected from its header, before decoding. Reason: character-format 8 sets no limits, but a hostile
  file must not crash or exhaust the daemon; the largest real character is 2880 logical
  units wide.
- **A-CONV-1 Vector actor pictures (D2).** Each used pose is rasterised by the bentwire
  WMF rasteriser at the display size (1x) and at 2x (`hidpi = { 2 = ... }`), aliased,
  indexed. Palette: index 0 is a transparent key colour not painted by the actor, then the
  exact distinct colours the metafiles paint, in first-use order (not quantised to the
  Office palette). Reason: D2 asks for the exact WMF colours; act-format 4.6 accepts the
  small deviation from Office 97's palette snapping.
- **A-CONV-2 Sounds in converted packages.** Sounds are deduplicated by PCM; the id is
  `sNN` of the first source index (ACS, ACT) or of the first occurrence over all
  animations (ACF/ACA, 1.5). Unplayable ACF sounds are listed as `<animation>:<index>`.
  Reason: character-format 6 identifies sounds by their PCM hash.
- **A-CONV-3 ACT conversion options.** `character.info.lang` (default `en`) and the
  codepage of the item name and of v1 strings (default windows-1252) are converter
  options; `characters/classics/options.txt` sets `ja` and `shift_jis` for the two Japanese actors
  (OFFLADY, DOLPHIN). The D6 state aliases are added only for Office actors (kind 7) and
  only to `[states]`, not to `office97.ids`. The Office palette for bitmap actors must be
  given (an Office ACS or MSO97.DLL); there is no default. Reason: ACT carries no language
  or codepage (character-format 9.4), and the palette must not be embedded.
- **A-CONV-4 Agent 1.5 animation set.** The 1.5 formats store no animation set version;
  `source.animation_set` is then omitted rather than filled with the runtime's 1.50.
  Reason: `[source]` records the file, not runtime defaults.
- **A-THEME-2 Theme extraction.** OBALLOON.DLL bitmaps are vertical strips; masks are
  folded into alpha (black mask pixel opaque). Buttons 105 to 110 become 5-frame strips
  named normal, default, hot, pressed, disabled; bullet 101 becomes the label bullet with
  normal, hot, pressed, disabled; icon group 100 (32 x 32, deepest depth) becomes `tip`.
  Bullet 102 and arrows 103/104 are extracted but not referenced by `theme.toml`. Reason:
  office-assistant R-OB-8/11 give ids and frame counts; the state order is [inf] there.
- **A-CONV-5 Ids of qualified file names.** A-ID-1 uses the file stem; a parenthesised
  qualifier (`WILL(Bob).ACT`, `ROCKY(Greetings).ACT`) is dropped unless another source would
  then get the same id, in which case it is kept as `-qualifier` (`rocky-greetings-act`).
  Characters outside `[a-z0-9]` become `-`. The source list is the pack's (earlier byte-identical
  copies win, so duplicate copies of a file never name an id).
  Byte-identical sources are converted once. Reason: A-ID-1 does not say how to spell such
  stems; the rule keeps ids short and still never guesses between two different files.
- **A-CONV-6 Out-of-range source data.** As for ACS (character-format 10), an ACF/ACA image or
  sound index beyond its list means no layer or no sound; a state without members is
  dropped; palette indices beyond a short palette (fewer than 256 entries) are written as the
  transparent index, which is how the colour-key compositing of the source runtimes draws
  them. A transparent index beyond the palette is an error. Reason: no sample has these
  cases; the agent-fixtures files do, and the rules keep packages valid and
  playback-equivalent.
- **A-PACK-3 Helper actors.** A source whose every frame is fully transparent (the Bob helper
  ZVISIBLE.ACT, whose metafile draws nothing) is excluded from `classics` with that reason.
  Reason: a character without a visible picture has no use as a desktop character.
- **W-RAST-1 WMF geometry (D2, act-format 4.6).** Logical points go through
  MM_ANISOTROPIC (viewport = destination rectangle, window = placeable bbox until the
  metafile's own SetWindowOrg/SetWindowExt) in floating point and are rounded to whole
  device pixels. A pixel is covered iff its centre lies inside the shape: polygons with
  ALTERNATE (even-odd) or WINDING (nonzero), ellipses and round rectangles analytically.
  The pen width is scaled by the x factor and rounded to whole pixels; pens of at most
  1 px after rounding are cosmetic one-pixel lines (last point of a polyline excluded);
  wider pens cover every pixel within half the rounded width of a segment (round caps
  and joins). Drawing is clipped to the canvas, not to the
  viewport. Composite children are placed by character-format 5.3's truncating map at the
  output size, also for the 2x sheet (computed from logical units, not by doubling 1x).
  Unknown record types fail the conversion; SetMapMode is taken as MM_ANISOTROPIC (the only
  value in the samples) and Escape is ignored. Reason: GDI's exact rasterisation rules are
  not documented in the research; pixel-centre sampling is simple and reproducible.
- **W-RAST-2 Ellipse and RoundRect pens.** The brush fills the box shape; the pen paints
  a ring of its width centred on the outline, or inside the box for PS_INSIDEFRAME and for
  cosmetic pens. GDI's one-pixel shrink of shapes drawn with a null pen is not modelled.
  Reason: as W-RAST-1.
- **W-COL-1 WMF colours.** The rasteriser paints into a caller-given palette: every
  colour goes to the nearest entry (squared RGB distance, lowest index on ties) other
  than the transparent one; COLORREF 0xF5FAF5 paints transparent (act-format 4.6). With
  the A-CONV-1 palette every stored colour matches exactly. Reason: indexed output (D2).
- **W-ROP-1 R2_NOTXORPEN and other ROP2 codes.** Evaluated per RGB channel on the
  destination pixel's palette colour, then snapped by W-COL-1; operations that read the
  destination leave transparent pixels transparent. TextOut glyphs and their opaque
  background ignore ROP2, as in GDI. Only 4 Hoverbot records use a code other than
  R2_COPYPEN; they fill with a white brush and a null pen, which is the identity under
  any reading (0 pixels change in 197 poses). Reason: Office 97 applied it to palette
  indices of its own back buffer, which bentwire does not have.
- **W-TEXT-1 TextOut.** Every face name (Albertus Medium, Courier New in the samples) is
  drawn with the bundled Wine Tahoma (the bold file for weight >= 600), bytes as cp1252,
  glyph outlines filled aliased with the nonzero rule. A negative lfHeight is the em
  height, a positive one the cell height (ascender minus descender), 0 means 12 px;
  heights scale by the y factor. SetTextAlign left/right/centre and top/baseline/bottom are honoured; an opaque
  background fills advance width x (ascender to descender). Reason: the original fonts
  are not shipped; act-format 4.6 asks for a fallback for these few frames.

- **A-DESK-1 Ask keys.** `Ask` rejects keys outside B-ASK-1 with
  `org.freedesktop.DBus.Error.InvalidArgs`; a value wrapped in a second variant is
  unwrapped. Reason: a typo such as `button` would otherwise be dropped silently.
- **A-DESK-2 CLI details (5.5).** `characters` prints `id<TAB>name`, and the current
  character gets a third column `*`, so `cut -f1` still yields ids. `ask` whose request
  fails (status 1, e.g. `Hidden`, `BalloonTooBig`) prints nothing and exits 2 with the
  reason; an interrupted ask prints its result line (`none`) and exits 3. `convert` also
  takes `--palette FILE`, `--lang TAG`, `--codepage LABEL` (ConvertOptions, for ACT
  actors). Reason: the table leaves these cases open.
- **A-DESK-3 Test1 details (T-4, A-TEST-3).** `Pointer(kind, button, x, y)` takes `press`,
  `release`, `motion`, `leave` at global coordinates and is hit-tested like a compositor:
  menu, Office balloon, Agent balloon, then the character's shape; a press keeps an
  implicit grab until its release; a press outside an open menu acts as `popup_done`.
  `Key(name)` takes `Escape`, `Enter`/`Return`, `Tab`, `space`, `Up`, `Down`, `Left`,
  `Right` or one character, with optional `Shift+`, `Ctrl+`, `Alt+` prefixes.
  `AddOutput(s WxH+X+Y[@scale]) -> u id`, `RemoveOutput(u id)`. `Snapshot() -> s` exists in
  headless mode too; on Wayland it adds `backend`, what the backend actually requested
  (configure acked, margins, buffer attached, keyboard interactivity, balloons, menu).
  Reason: the spec names the members but not their arguments.
- **A-DESK-4 Headless trace format (T-4).** One JSON object per line, each with `t` (fake
  ms): compositor requests as `{"req": "layer_surface" | "keyboard_interactivity" |
  "buffer" | "subsurface" | "popup" | "destroy" | "unmap", ...}` (buffers and input
  regions as SHA-256 prefixes), `{"type": "signal", "member", "args", "dest"}` (`dest`
  null = broadcast), `{"type": "sound_start", "sound", "duration_ms"}`, `sound_stop`,
  `output_added`, `output_removed`, and `{"type": "call", "method", "args", "sender"}` for
every Agent1/Character1 call the core answered, with `id` (request id) or `error`.
`--dump-frames DIR` writes
  `<seq>-t<ms>-out<id>.png` per output whenever the drawn scene changes. Headless mode
  uses only the bundled fonts (A-FONT-1), opens no audio device and no tray item.
  Reason: the spec lists the contents, not the encoding.
- **A-DESK-5 Wayland details.** Each margin commit is followed by a wl_display.sync;
  its `done` marks the position as applied, so pointer events are mapped with the
  position the compositor used for them. During an implicit grab the global position is
  the press position plus zwp_relative_pointer_v1 deltas, because compositors differ in
  how they report positions on a surface that moves under the grab (sway: relative to
  the press-time position) (B-WL-7). A fractional output scale is the current mode width
  over the xdg-output logical width (no fractional-scale protocol). The Agent and the
  Office balloon are separate subsurfaces when both are open, the Office one above. A
  menu opened without a real press serial (Test1) is shown without a grab. Reason:
  implementation choices the rules leave open.
- **A-DESK-6 Daemon flags and files.** `bentwired --no-tray`, `--no-audio` switch those
  backends off. Unknown config keys are ignored; a config file that fails to parse on
  SIGHUP keeps the previous config (logged). SIGTERM and SIGINT run the core's `Quit`
  (which saves the state) and exit 0. The tray's dbusmenu labels turn the `&` mnemonic
  into dbusmenu's `_`. Reason: robustness; dbusmenu has its own mnemonic syntax.
- **A-ACT-1 ACT parser strictness.** `agent_formats::act` reads only the first pack item
  (every sample has one; MSO97 also takes the first). Beyond the runtime's checks
  (act-format 4) it rejects: payload index entries that share a blob unless they are
  distinct sub-images of one `MNAK` pack; a group index entry referenced twice; group
  blobs whose total size exceeds their data section; `MNAK` packs with 0 or more than 64
  images; a declared LZ size above 256 MiB; LZ streams whose bits run past the blob end
  (the runtime reads past it). Leniencies kept from the runtime: `mtSize` and the
  placeable checksum are ignored, RLE output is truncated to stride x height, a RIFF blob
  may be one pad byte longer than its size field. Reason: no sample needs any of the
  rejected forms (all 55 parse and match the research parser), and each could make a
  small file allocate gigabytes or take minutes.

## 4. Reconciliation

| # | Topic and sources | Finding | Resolution and reason |
|---|---|---|---|
| 1 | Lip-sync table: acs-format 5.4 (characters), acf-aaf-format 8.1 (code points), agent-runtime R-LIP-1 (points to acs), agent15-format 5 ("same table") | Compared by script, mouth type by mouth type: both tables hold the same 116 code points, with no difference and no duplicate. | Agree. One table (R-LIP-1). The SDK's descriptive phoneme groups (docs findings: "Closed = m b f", "wide2 = n d t") disagree for `f` and `n`; the binary table [OBS] wins over the prose [DOC]. |
| 2 | Image/audio "checksum": acs-format 8 (unknown, not a CRC of stored bytes) vs acf-aaf-format 11 ("CRC-32 per image and sound") and 6.2 (ACE computes CRC-32 of the editor's animation text) | `acs.py` reports `crc_match: 0` and `aud_crc_match: 0` for all 12 files (`all.txt`). Byte-identical records carry different values. | acf-aaf-format 11's table row is loose wording. Most likely an editor-side CRC over source data, as ACE does for ACA. It is never read by any runtime; dropped by the converter (character-format 9.1). No runtime rule. |
| 3 | ACS duration time base: acs-format 3.4 ("not traced") vs agent-runtime R-ANIM-1 | The server multiplies by 10 [OBS]; SDK "hundredths of a second" [DOC]. | 1/100 s, settled. Manifest `ms` = value x 10. |
| 4 | Balloon style bits: acs-format 3.2.1 (0x10000 size-to-text unverified; Lebeau 0x20000, 0x40000) vs agent-runtime R-BAL-1 | The provider returns `flags & 0xF0F00`; the server's internal style uses 0x200/0x100 on/off, 0x10000 size-to-text, 0x20000 no auto-hide, 0x40000 no auto-pace [OBS]. | The ACS flags are the internal style bits, confirmed. The manifest keys carry them (R-BAL-1). |
| 5 | Mouth point: acs-format 3.4 ("consumer not traced") vs agent-runtime R-BAL-5.4 | The server uses it as the balloon tail anchor [OBS]. | Anchor = tail tip (B-BAL-3). ACA and 1.5 have none: B-BAL-4 (bentwire decision). |
| 6 | Tray flag: agent-runtime R-UI-4 (binary 0x1000) vs [DOC] click-event (bit 13, 0x2000); U-7 | Both may be right at different layers. | Neither constant is exposed: boolean `from_tray` in Click/DblClick (B-TRAY-1). |
| 7 | Mouth pixels outside the frame shape: character-format 14 item 3 (50 of 291 ACS overlays, 1,080 pixels) | SDK: the overlay is clipped to the base frame outline unless the replace flag is set [DOC]; agentdp2 builds the region from frame images only when replace = 0 [OBS]. | Clip (B-PIX-2), as presentation, so the round-trip hashes are unchanged. |
| 8 | Exit branches after Stop: agent-runtime R-ANIM-7, U-3 | Stop cuts [OBS]; the provider keeps the previous animation [OBS]; the SDK says exits play before the next animation [DOC]. | B-ANIM-1: the exit plays at the next animation-playing request; requests without an animation do not consume it (bentwire decision). |
| 9 | RequestStart/Complete for MoveTo/Speak/Ask: agent-runtime U-13 | Unread enqueue path. | B-Q-3: start at the first phase, complete after the last, status of the first failure (bentwire decision). |
| 10 | Mouth movement in Speak without TTS: agent-runtime R-LIP-3, U-1 | No mouth call on the pacer path [INF]. | No mouth overlay (B-LIP-2, bentwire decision). |
| 11 | Branch draw: acs-format 5.2 (r in 1..100, r <= p) vs acf-aaf-format 8.2 (p >= r) vs character-format 7.2 (r in [0,100), r < p) | Equivalent under the constraints of character-format 7.2. | B-SEQ-A. |
| 12 | Exit semantics in ACF 2.0 (acf-aaf-format 8.2: "exit not -1 enters exiting") vs acs-format 5.2 (extra conditions: not at end, or transition 1, or queued) | The ACF wording is a summary; the ACS conditions are [OBS] in the shared logic. | B-SW-A uses the acs-format conditions for every agent-family character. |
| 13 | Agent server vs MSO97 engine (act-format 5.4 to 5.6: one-slot queue, immediate idle, pose transitions, 14 ms coalescing) | Two runtimes with different event models. | B-FAM-1 table (bentwire decisions). |
| 14 | Office balloon area: agent balloon uses the monitor rectangle (R-BAL-5), Office uses the work area (R-OB-17, R-OB-34); BLNMGR clips C to the monitor (R-OB-20) | Different on Windows. | Both use the output rectangle on Wayland (B-WL-1, B-BAL-3, B-OB-3). |
| 15 | Office placement: R-OB-17 candidate rectangles vs R-OB-27 window positions | Checked: identical geometry (window = body + 16 on the tail side, 4 px gap, 12 px reach). | Agree. |
| 16 | Snooze caption: office-assistant R-OB-11 ("&Erneut erinnern", strings 112 to 127) vs R-OB-32 ("Er&neut erinnern", ids 112 to 126) | OBALLOON.DLL string 122 is `&Erneut erinnern`. 127 is `&Hilfe`, which is not a button set member. | R-OB-11 is right. B-L10N-2 uses the verbatim string. |
| 17 | Office animation names in office97 manifests: D6 (XP names, accepted) vs the prototype output (Office 97 type-library names, e.g. `Greeting = ["Greeting.1", …]`) | The converter is still to be built. | Daemon accepts both through the alias table (B-FB-1 step 3, B-O97-5). |
| 18 | Character-format status "draft, nothing decided" vs bentwire.md "D1 to D8 accepted" | bentwire.md is newer. | D1 to D8 decided (section 0). |
| 19 | Office report's name scan ("courtney, earl, rover lack about 20 each; qmark showed no names") | A rescan with the ACS parser: courtney lacks 24, earl 20, rover 24, merlin 14, qmark 34 of the 35 names (qmark has 13 animations, e.g. Welcome, PointLeft). | Scan replaced; B-FB-2 vectors come from it. office-assistant's open qmark item is closed. |
| 20 | Scope: agent-runtime 7.2/7.3 "keep" list (Think, Wait, Load/Unload, Commands, Balloon.Style) vs bentwire.md v1 scope (8 commands) | bentwire.md is the owner's decision. | Out of v1 (1.2); Load/Unload become Select (one character at a time). |
| 21 | R-MOVE-2 timer period `Speed / 25` gives 0 for Speed < 25 | Win32 timers have a 10 ms minimum. | B-MOVE-1 floor of 10 ms. |
| 22 | Error code to string pairings (agent-runtime U-14, by meaning only) | Not proven. | No numeric codes are exposed; section 5.4 uses names. Their meaning is the documented one, not the code. |
| 23 | R-UI-3 menu order (item 3 [INF]) and the separator ([DOC] "only if both exist") | Partly inferred. | B-MENU-1 (bentwire decision). |
| 24 | Show Fast picture: agent-runtime R-SHOW-3 suggests "first frame of Showing" | Showing usually starts empty ([DOC] agent-states). | B-SHOW-3 uses RestPose frame 0 first. |
| 25 | Sound of an invisible frame with a sound (character-format 14 item 5; 2 ACS frames, F1 and ROCKY) | agentdp2 skips such frames [OBS]; the start of their sound was not observed. | Not started (R-ANIM-5, bentwire decision). office97 starts sounds on 0 ms frames (act-format 5.2, B-O97-2). |
| 26 | Completion and sounds: agent-runtime R-ANIM-3 says "that frame's sound" | The server waits for the wave-out done message, and only one sound plays at a time. | B-ANIM-2 waits for any sound the animation started (bentwire decision). |
| 27 | Queued flag in the agentdp2 switch (acs-format 5.2, agent15-format 5, both [INF]) | If it were always true, return animations would never play. | B-SW-A treats it as false at end (bentwire decision). All 14 return-type ACS animations have exit -1 on every frame, so real data does not depend on it. |

## 5. D-Bus API and CLI

### 5.1 Objects

- Bus: session bus.
- Name: `org.bentwire.Agent`.
- Objects:
  - `/org/bentwire/Agent` with interface `org.bentwire.Agent1`;
  - `/org/bentwire/Character` with interface `org.bentwire.Character1`. This is the
    current character; one exists at a time in v1.
- Properties use `org.freedesktop.DBus.Properties` and emit `PropertiesChanged`.

### 5.2 `org.bentwire.Agent1`

| Member | Signature | Behaviour |
|---|---|---|
| `ListCharacters` | `() -> a(sss)` | id, name, description (B-CHAR-1, B-CHAR-3), sorted by id |
| `Select` | `(s id) -> ()` | B-CHAR-5. Errors: `NoSuchCharacter`, `InvalidCharacter` (with the load error) |
| `Quit` | `() -> ()` | B-RUN-1 |
| property `Character` | `s`, read | current id, `""` if none |
| property `Version` | `s`, read | daemon version |

### 5.3 `org.bentwire.Character1`

Methods. Every `u` return value is a request id (B-Q-1).

| Method | Signature | Rules |
|---|---|---|
| `Show` | `(b fast) -> u` | R-SHOW-1, B-SHOW-3, cause 4 |
| `Hide` | `(b fast) -> u` | R-SHOW-2, cause 3 |
| `Play` | `(s name) -> u` | B-FB-1 at call time; error `UnknownAnimation` |
| `Speak` | `(s text) -> u` | R-SPEAK-*, R-BAL-*, B-SPEAK-* |
| `Ask` | `(a{sv} spec) -> u` | B-ASK-*, B-OB-*; errors `InvalidBalloon`, `NotSupported` |
| `MoveTo` | `(i x, i y, u speed_ms) -> u` | R-MOVE-*, B-MOVE-1 |
| `GestureAt` | `(i x, i y) -> u` | R-GEST-1 |
| `Stop` | `(u id) -> ()` | B-Q-4, 0 = all |
| `StopAll` | `(as types) -> ()` | B-Q-5 |

Properties:

| Property | Type | Access | Meaning |
|---|---|---|---|
| `Id` | s | read | character id |
| `Name` | s | read | localized name |
| `Description` | s | read | localized description |
| `Visible` | b | read | |
| `Left` | i | read | top-left position |
| `Top` | i | read | top-left position |
| `Width` | u | read | display size |
| `Height` | u | read | display size |
| `IdleOn` | b | read and write | R-IDLE-4 |
| `MoveCause` | u | read | last cause (B-SHOW-4) |
| `VisibilityCause` | u | read | last cause (B-SHOW-4); 0 = never shown |
| `BalloonEnabled` | b | read | B-BAL-1 |
| `AnimationNames` | as | read | manifest names, then office97 group names |

Signals. "U" marks a signal sent only to the caller's unique name (B-Q-2); the others
are broadcast.

| Signal | Signature | Sent | Rule |
|---|---|---|---|
| `RequestStart` | `(u id)` | U | B-Q-2 |
| `RequestComplete` | `(u id, u status, s reason)` | U | B-Q-2, B-Q-3 |
| `AskComplete` | `(u id, i result, ab checked)` | U, before RequestComplete | B-ASK-2, B-ASK-4 |
| `Bookmark` | `(i n)` | U | B-SPEAK-1 |
| `Click` | `(u button, u modifiers, i x, i y, b from_tray)` | broadcast | B-UI-1, B-TRAY-1 |
| `DblClick` | `(u button, u modifiers, i x, i y, b from_tray)` | broadcast | B-UI-1 |
| `DragStart` | `(u button, u modifiers, i x, i y)` | broadcast | R-UI-2 |
| `DragComplete` | `(u button, u modifiers, i x, i y)` | broadcast | R-UI-2 |
| `Move` | `(i x, i y, u cause)` | broadcast | B-SHOW-4 |
| `Shown` | `(u cause)` | broadcast | R-SHOW-1 |
| `Hidden` | `(u cause)` | broadcast | R-SHOW-2 |
| `IdleStart` | `()` | broadcast | R-IDLE-2 |
| `IdleComplete` | `()` | broadcast | R-IDLE-4 |
| `BalloonShown` | `()` | broadcast | R-BAL-7, B-ASK-4 |
| `BalloonHidden` | `()` | broadcast | R-BAL-10, B-ASK-4 |

### 5.4 Error names

Method errors (the call fails, no request is created):

| Name | When |
|---|---|
| `org.bentwire.Error.NoCharacter` | no character loaded |
| `org.bentwire.Error.NoSuchCharacter` | `Select` with an unknown id |
| `org.bentwire.Error.InvalidCharacter` | `Select` of a character that fails to load |
| `org.bentwire.Error.UnknownAnimation` | B-FB-1 step 5 |
| `org.bentwire.Error.InvalidBalloon` | B-ASK-5 |
| `org.bentwire.Error.NotSupported` | `mode = modeless` |
| `org.freedesktop.DBus.Error.InvalidArgs` | wrong types, unknown `StopAll` types, reserved bookmark numbers |

Request reasons (in `RequestComplete`):

| Reason | Status | Agent equivalent |
|---|---|---|
| `org.bentwire.Error.Hidden` | 1 (3 for an ask closed by Hide) | 0x8004200A |
| `org.bentwire.Error.Interrupted` | 3 | 0x8004210C |
| `org.bentwire.Error.HiddenByUser` | 3 | 0x8004210D |
| `org.bentwire.Error.Dragged` | 1 | 0x80042010 |
| `org.bentwire.Error.CharacterChanged` | 3 | none |
| `org.bentwire.Error.BalloonTooBig` | 1 | Office TooBig |
| `org.bentwire.Error.AnimationUnavailable` | 1 | 0x80070570 on a damaged animation (B-CHAR-2) |

### 5.5 CLI `bentwire`

The CLI is a thin client of the API. Without a running daemon it relies on D-Bus
activation. If that also fails, it prints `bentwired is not running` and exits 1.

| Command | Maps to | Output |
|---|---|---|
| `play <name> [--wait]` | `Play` | request id |
| `say <text> [--wait]` | `Speak` | request id |
| `ask [<text>] [--heading H] [--icon I] [--buttons SET \| --button CAPTION ...] [--label L ...] [--label-type T] [--checkbox C ...] [--mode M] [--animation NAME]` | `Ask`, always waits | line 1: result name (B-L10N-2), `button:<n>` (custom, 1-based), `label:<n>` or `none`; line 2, if there are check boxes: `checked:` followed by comma-separated 0 or 1 values |
| `moveto <x> <y> [--speed MS] [--wait]` | `MoveTo`, default speed 1000 | request id |
| `gestureat <x> <y> [--wait]` | `GestureAt` | request id |
| `show [--fast] [--wait]` | `Show` | request id |
| `hide [--fast] [--wait]` | `Hide` | request id |
| `stop [<id>] [--all]` | `Stop(id or 0)`, `--all` = `StopAll([])` | nothing |
| `characters` | `ListCharacters` | `id<TAB>name` per line, current marked with `*` |
| `select <id>` | `Select` | nothing |
| `animations` | `AnimationNames` | one per line |
| `status` | properties | `character`, `visible`, `left`, `top`, `idle` as `key=value` lines |
| `quit` | `Quit` | nothing |
| `convert <source> <dest>` | local, no daemon | per character-format 10 |

Exit status:
- 0 success;
- 1 usage, D-Bus or method error, with the error name and message on stderr;
- 2 the request failed;
- 3 interrupted, or `ask` result `none`.

`--wait` blocks until `RequestComplete` for the printed id.

## 6. Test strategy

**T-0 Repo boundary.**
- The code repo holds no Microsoft-derived data: no characters, no frames and no golden
  hashes made from them (bentwire.md).
- CI tests use synthetic characters written for the tests (manifests plus tiny PNGs and
  WAVs) and a bundled free font for every metric vector (B-BAL-2, B-OB-1). Pick one font
  and pin it by hash.
- Oracle tests against the real files run only when `BENTWIRE_SAMPLES` (the user's files)
  and `BENTWIRE_REFERENCE` (`$BENTWIRE_REFERENCE`) are set. They write results outside the
  repo.
- When the variables are unset, the oracle suite reports "not run" as a separate target.
  It must never count as passed.

**T-1 Unit tests per rule.** Every rule with a vector gets a table-driven test on the core
with a fake clock and scripted draws. In particular:
- R-ANIM-2, B-ANIM-2, B-O97-3;
- B-SEQ-A, B-SW-A, B-SW-O;
- R-DIR-1, R-MOVE-1, B-MOVE-1, R-GEST-1;
- B-IDLE-T, R-IDLE-5;
- R-BAL-8, B-BAL-3, R-BAL-6;
- R-LIP-1, B-LIP-1;
- B-FB-1/B-FB-2 (synthetic characters with the same name sets);
- B-ASK-2, B-ASK-5, B-ASK-7;
- B-Q-2..5, B-PIX-2.

**T-2 Interpreter traces against the oracle.**
- For each converted real character, the Rust interpreters run every animation twice: in
  normal mode, and with exit mode entered at a scripted step. They use the same draw
  scripts as `prototype/oracle.py` interpreter A, the source runtime's rules over the
  research parsers.
- The sequences of (picture hash, ms, sound hash) must be equal (character-format 11.3).

**T-3 Golden frames.**
- Rust compositing of converted characters must equal frames rendered from the source
  files by the research Python parsers. The Python side is the independent oracle:
  - `render.py` and `prototype/oracle.py` for ACS, ACF and 1.5;
  - `render.py` for ACT bitmap actors.
- Equality is compared on canonical RGBA hashes at display size, before B-PIX-2.

**T-4 Headless render mode.** `bentwired --headless` uses the headless backend:
- virtual outputs (`--outputs 1920x1080+0+0[,…]`, with scale);
- a fake clock;
- `--draws FILE` or `--seed N`;
- no audio device: sounds are logged with their decoded duration;
- `--trace FILE`: JSON lines of every compositor request a Wayland backend would make
  (layer, anchor, margins, exclusive zone, input region hash, buffer scale, popup and
  grab serial, keyboard interactivity), every sound start and stop, and every signal;
- `--dump-frames DIR`: PNG of the composed scene per output.

It also exports `org.bentwire.Test1`, only in headless mode:
- `Advance(u ms)`;
- `Pointer(s kind, u button, i x, i y)`;
- `Key(s key)`;
- `SetDraws(au)`;
- `AddOutput` and `RemoveOutput`.

Balloon and menu goldens (synthetic character, bundled font) are committed PNGs.

**T-5 D-Bus end to end.**
- Under `dbus-run-session`, start `bentwired --headless`, drive it with the CLI and
  `Test1`, and assert:
  - signals, their order and their unicast addressing;
  - the reply-before-signal ordering (B-Q-1);
  - CLI output and exit codes (5.5);
  - the B-ASK-7 matrix;
  - user Hide (B-UI-4);
  - gallery switching (B-GAL-1).

**T-6 KWin verification checklist** (manual or nested `kwin_wayland --virtual`, recorded
in the build log; not a spec decision):
- K1: negative margins and positions partly off-output (B-WL-4);
- K2: top layer below a full-screen window, above normal windows (B-WL-3);
- K3: `get_popup` menu with grab and `popup_done` on an outside click (B-WL-9);
- K4: `on_demand` keyboard focus for the ask balloon (B-WL-8);
- K5: input region click-through on transparent pixels (B-WL-6);
- K6: drag stays 1:1 without jitter (B-WL-7);
- K7: SNI Activate, SecondaryActivate and dbusmenu in the Plasma tray (B-TRAY-1);
- K8: output hot-plug pull-back (B-WL-4).

If K1 fails on KWin, the backend clamps only the requested margins so the surface stays
on its output, leaves the model position unchanged, and logs this once.

**T-7 Faithfulness review.** No recordings of the original exist (bentwire.md). The
behaviour is checked against this spec only. Each bentwire decision is listed in the
build's README so it can be revisited when evidence appears.

## 7. Open points

Only points that cannot be decided now (MS ADPCM rounding is closed, see
character-format section 6):

None. English Office captions and mnemonics (B-MENU-2, B-L10N-2, B-GAL-1) are
confirmed from English (1033) MSOINTL.DLL 10.0.2609 and OBALLOON.DLL 10.0.2607.

All other unknowns in the reports are either out of v1 or closed by a bentwire decision
above.
