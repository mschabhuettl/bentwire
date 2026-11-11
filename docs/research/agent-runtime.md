# Microsoft Agent 2.0 runtime: behaviour and API

This is the behaviour spec for the bentwire player and D-Bus service. It covers
what the Agent 2.0 server does at run time. The file formats are covered in the
sibling files `acs-format.md` and `office-assistant.md`.

## 0. Sources, conventions and evidence

**Binaries.** Microsoft Agent 2.0 runtime, version 2.00.0.3427:

| Binary | Role here |
|---|---|
| `agentsvr.exe` | The server. It runs characters, windows, the balloon, idle, the queue, the menu and the tray. Image base 0x01000000. |
| `agentdp2.dll` | Data provider for `.acs` files. It does frame sequencing, branching and exit branching, state-to-animation choice, and holds the phoneme-to-mouth table. Image base 0x71200000. |
| `agentdpv.dll` | Data provider for `.acf`/`.aca` (HTTP). Its logic matches agentdp2. |
| `agentanm.dll` | Standalone animation player behind `CreateAgentAnimW`. It is probably used for previews. Its frame timing matches the server. |
| `mslwvtts.dll` | The "Labelled Wave File" (.LWV) pseudo TTS engine. |
| `intl/agt0409.dll`, `intl/agt0407.dll` | English and German UI strings and dialogs. |
| `agtctl15.tlb` | Type library of the 1.5 control. |
| `agentctl.dll` | Embeds the 2.0 control type library. |

**Method.**
- REA/Ghidra 12.1.4 decompiled every function of these files to the scratchpad.
- Capstone disassembled the places where the decompiler hid x87 maths.
- `pefile` parsed the string tables and dialog templates.
- Nothing was executed.

The REA Evidence IDs are:
- agentsvr decompile: `ev_a4c6f19521ec716f...`
- agentdp2 decompile: `ev_1d2bd3adb3a210ff...`
- agentanm decompile: `ev_600944cd7bff1e18...`
- mslwvtts decompile: `ev_30073a0a9bb8c993...`
- agentdpv decompile: `ev_e40a52829394900a...`

**Documentation.**
- Archived copies of the old MSDN pages were not reachable in this environment.
- The official SDK text is still published by Microsoft at
  `https://learn.microsoft.com/en-us/windows/win32/lwef/` ("Legacy Windows
  Environment Features", Microsoft Agent). It is the same SDK text as the old MSDN
  pages.
- Below, `[L:page]` means `https://learn.microsoft.com/en-us/windows/win32/lwef/page`.
  For example, `[L:agent-states]` is
  https://learn.microsoft.com/en-us/windows/win32/lwef/agent-states.
- A research subagent collected those pages. The page slugs are as it reported them.

**Evidence tags** on every rule:
- **[OBS]**: observed in the binary at the cited address. These are static
  analysis facts, not runtime observation.
- **[DOC]**: stated in the Microsoft documentation.
- **[INF]**: inferred from code structure or naming. Treat these as the best
  reading, not as fact.

**Addresses.** Addresses are written as `binary!0xADDRESS` (virtual address in that
image). A function address is its entry point.

**Clean room.** Everything below is a description in our own words. No
decompiled code is reproduced.

---

## 1. Animation playback

The frame-level algorithms are specified in `acs-format.md` sections 5.1 to 5.4:
- frame compositing;
- next frame, branching and exit;
- switching animations;
- mouth selection.

This section adds the timing and server-side rules and does not repeat those.

**R-ANIM-1 Frame duration unit.** [OBS] [DOC]
- A frame's stored duration is in hundredths of a second.
- The player multiplies it by 10 to get milliseconds.
- Evidence: agentsvr!0x0100add8 (server frame stepper, `duration*10`),
  agentanm!0x71253b5f (same), [L:animations] ("hundredths of a second").

**R-ANIM-2 Frame clock (drift-free with resync).** [OBS]
- The player keeps an absolute deadline.
- When frame N is shown, the next deadline is the previous deadline plus N's duration.
- If that deadline is already in the past, the player does not catch up or drop
  frames. The next deadline becomes now plus the duration.
- There is one Win32 timer per character window.
- Evidence: agentsvr!0x0100add8, agentanm!0x71253cb0.

Test: with a fake clock, a 3-frame animation of 10/20/30 (1/100 s) shows its
frames at t = 0, 100 and 300 ms, and completes at 600 ms. If frame 2 is delayed by
500 ms, frame 3 is scheduled 200 ms after the late frame 2 was shown. The
schedule is not compressed.

**R-ANIM-3 Last frame and completion.** [OBS]
- The stepper uses one timer id for ordinary frames and another for the last
  frame.
- When the last frame's duration expires, the animation completes, but only once
  that frame's sound has finished.
- If the sound is still playing, completion waits for the wave-out "done"
  notification (message 0x3BD).
- Completion is then posted to the character's request queue (internal message
  0x701), which completes the Play/state request.
- Evidence: agentsvr!0x0100add8 (timer ids 0x10000000 and 0x80000000), the
  character window procedure agentsvr!0x0100c6a3 (case 0x3BD),
  [L:creating-animations] (server waits for the sound).

**R-ANIM-4 Branching and RNG.** [OBS]
- Branch choice is `r = rand() % 100 + 1`, with the walk described in
  acs-format 5.2.
- `rand()` is the msvcrt LCG (seed*214013+2531011, output bits 16..30).
- agentdp2 seeds it with `GetTickCount()` when the DLL loads (agentdp2!0x71204440).
- agentdpv seeds with `time(NULL)` (agentdpv!0x5a964375).
- Seeding happens at process attach, and msvcrt keeps RNG state per thread. So the
  sequence a given frame sees is not reproducible in practice.
- Recommendation [INF]: bentwire uses a seedable PRNG behind a trait. Tests inject
  a scripted sequence. Reproducing msvcrt bit for bit is optional.

**R-ANIM-5 Empty frames.** [OBS]
- A frame with no images and a duration of 0 is never displayed. Sequencing walks
  through it, with a guard of at most 32 hops.
- Evidence: agentdp2!0x71204fee.
- Note: [L:creating-animations] uses such frames as invisible branch points.

**R-ANIM-6 Exit branches and return animations.** [OBS] (agentdp2) + [DOC] + [INF] (server sequencing)
- When a new animation request starts while the previous animation is still the
  current one, the provider first lets the old animation play out.
  - If the current frame can enter exit mode (acs-format 5.2: exit index not -1,
    and either not at end, or transition type 1), it plays through its exit indices.
  - Otherwise, if it is at its end and has a return animation, the return
    animation plays first.
- The server passes the "previous animation" object along with every Play and
  state request (agentsvr!0x01019f17, 0x01019f9a, 0x0101a09e). The provider does
  the switch (agentdp2!0x71206595, acs-format 5.3).
- Exception [OBS]: for the **Showing** state the previous animation is dropped,
  so no exit or return animation is played before Showing (agentsvr!0x0101a09e).
- [DOC] [L:creating-animations]: the server also plays the Return animation
  before Idling animations.

**R-ANIM-7 Stop cuts immediately.** [OBS] [INF]
- The server's "stop current animation" path kills the frame timer, the move
  timer and the sound, ends speech and tells the provider to end (agentsvr!0x0100c1b5,
  internal message 0x60B).
- No exit frames are played at that moment.
- [INF] The graceful exit path (R-ANIM-6) happens on the next animation that is
  started, because the provider still holds the old animation as "previous".
- See unknown U-3.

**R-ANIM-8 Sounds.** [OBS] [DOC]
- When a frame that has a sound becomes current, its sound is started. This
  happens only if both:
  - sound effects are enabled for the character (the server-wide "Play character
    sound effects" option and `SoundEffectsOn`);
  - the character window is visible.
- Starting a new sound stops the previous one.
- Evidence: agentsvr!0x0100add8 calls agentsvr!0x010098c2 only when the
  sound-effects flag (window object offset 0x9C) and the visible flag (offset 0x1C)
  are both set.
- The options code (agentsvr!0x0101b4e1) clears the sound-effects flag with
  message 0x611.
- Message 0x612 clears a different flag (offset 0xA0), the "Play spoken audio"
  option. See R-SPEAK-4.
- [L:soundeffectson-property]: SoundEffectsOn applies to all clients.

**R-ANIM-9 Animations on a hidden character.** [OBS] [DOC]
- **Play** and the MoveTo animation and end phases check visibility up front. On a
  hidden character they fail at once with code 0x8004200A, without playing.
  - Evidence: agentsvr!0x01019f9a, 0x0101a22f and 0x0101a373 (check through
    message 0x626).
  - [INF] The string for 0x8004200A is probably 10123, "The specified method
    failed because the character is hidden". It was matched by meaning; the code
    to string table was not located.
- **GestureAt** and the Speak "Speakable" phase have no up-front check
  (agentsvr!0x0101a14a, 0x0101a739). Their animation runs invisibly: frames are
  stepped with normal timing but not drawn.
- When any animation completes while the window is not visible, the frame stepper
  reports completion with failure code 0x8004200A instead of success.
  - Evidence: agentsvr!0x0100add8 (end-of-animation path) and the case 0x3BD in
    agentsvr!0x0100c6a3. Both choose 0x8004200A when the visible flag is clear.
- So every animation request on a hidden character ends with Status 1 (failed).
  Some fail immediately and some only after the animation's normal duration.
  - This matches [L:play-method] ("plays invisibly", status still set) and
    [L:hide-method] (requests generated while hidden get a failure status).

**R-ANIM-10 Frame stepping lives in the server.** [OBS]
- agentsvr has its own frame stepper and window per character (class `AgentAnim`,
  agentsvr!0x0100c6a3).
- agentanm.dll is a separate player with the same timing. It is used through
  `CreateAgentAnimW`, probably for character previews (the gallery dialog 1102 in
  the intl DLLs). [INF]
- bentwire needs only one player.

---

## 2. States and when the server plays them

The documented state list is in [L:agent-states]:
- Showing, Hiding, Speaking;
- MovingUp/Down/Left/Right;
- GesturingUp/Down/Left/Right;
- Listening, Hearing;
- IdlingLevel1/2/3.

The server also uses a pseudo-state **"Speakable"** (agentsvr!0x0101a739). The data
provider recognises it in addition to the 16 states (strings in agentdp2 and
agentdpv).

**R-STATE-1 Random choice among a state's animations.** [OBS] [DOC]
- The server picks `rand() % n` among the n animations assigned to the state.
- agentdp2 does not retry: whatever index comes out is played (agentdp2!0x712067ec).
- agentdpv (HTTP) walks forward cyclically to the next animation that is
  downloaded (agentdpv!0x5a968089).
- [L:agent-states]: "the server picks one at random".

**R-STATE-2 A missing state is not fatal.** [OBS]
- If a state has no animations, the request for it completes. For idle the
  failure is handled as in R-IDLE-5.
- Evidence: the result codes are handled in agentsvr!0x0101a09e and its callers.
  Code 0x80042004 (by meaning [INF], string 10127 "No animation exists for this state") is treated
  as success for the Speakable/state-by-name requests (agentsvr!0x0101a739,
  0x0101b889).

### 2.1 Show and Hide (internal request type 0x0B, agentsvr!0x0101a46f) [OBS] [DOC]

**R-SHOW-1 Show.**
1. If the character is hidden, make the window visible.
2. Notify all clients with the Show event. The cause is:
   - 2 for user (menu or voice);
   - 4 for the client's own Show;
   - 6 for another client's Show.
3. Play the **Showing** state, unless `Fast`.
4. If the character is already visible, Show completes without an animation.

**R-SHOW-2 Hide.**
1. If the character is visible and not already hiding, play the **Hiding** state
   (unless `Fast`).
2. Then hide the window and the balloon.
3. Send the Hide event to all clients. The cause is:
   - 1 for user via the tray menu or voice;
   - 3 for own client;
   - 5 for another client;
   - 7 for user via the character menu.

Hide is queued ([L:hide-method]).

**R-SHOW-3 Fast with no prior frame.** [DOC] [L:show-method]
- `Show(Fast=True)` with no prior animation may show an empty frame.
- bentwire shows the first frame of the Showing animation, or frame 0 of RestPose,
  instead (design choice).

### 2.2 Moving (MoveTo) [OBS] [DOC]

[INF] MoveTo appears as **three internal queue entries**. The mask in
agentsvr!0x0101ad97 groups request types 5, 6 and 7 under "Move". The enqueue
path and the request constructors were not read, so the split is inferred from
the handlers (see U-13).

**R-MOVE-1 Phase 1 (type 5, agentsvr!0x0101a22f): play the Moving animation.**
- If the target equals the current top-left position, no animation is played and
  the phase completes.
- Otherwise the direction is computed from the target point (x, y) against the
  current frame rectangle, using R-DIR-1.
- If the target point is **outside** the current frame rectangle:
  - target to the screen-left gives **MovingRight**;
  - above gives **MovingUp**;
  - screen-right gives **MovingLeft**;
  - below gives **MovingDown**.
  - The names are relative to the character, which faces the user.
- **Quirk**: if the target top-left lies **inside** the current frame rectangle
  (a short move), the server picks **MovingDown** when the target is screen-left
  of it, and **MovingLeft** in all other cases.
  - The byte-level evidence is the branch at agentsvr!0x0101a2f7 to 0x0101a30f.
  - Reproduce this quirk.

**R-MOVE-2 Phase 2 (type 6, agentsvr!0x01017c47): slide the frame.**
- Message 0x602 carries (x, y, Speed). If the character is hidden, Speed is
  forced to 0.
- Speed 0 places the window immediately.
- The motion algorithm is agentsvr!0x0100bd63 and 0x0100af76 [OBS]:
  - `steps = 25`;
  - timer period = `Speed / 25` ms (integer division);
  - per tick, the major axis (the larger of |dx| and |dy|) advances by
    `max(major_distance / 25, 25)` pixels, without overshooting the target;
  - the minor axis follows the straight line from start to target
    (`y = m*x + b`, or the reverse for a vertical-major move), rounded toward zero;
  - when both coordinates equal the target, the timer stops and the phase completes.
- So with the default Speed of 1000:
  - moves of 625 px or less run at 25 px per 40 ms (625 px/s);
  - longer moves take 25 ticks, which is 1000 ms.
- When the position actually changes, the balloon is hidden at the start of a move.
- If the user starts dragging during a slide, the slide is cancelled and the
  request fails with 0x80042010 (by meaning [INF], string 10179 "A character cannot be moved while being dragged").
  Evidence: agentsvr!0x0100b180.
- No screen-edge clamping is applied (see R-WIN-4).

**R-MOVE-3 Phase 3 (type 7, agentsvr!0x0101a373).**
1. Play the **Speaking** state. This is the neutral pose after the move, using the
   standard set's RestPose.
2. Send the Move event to every client of the character. The cause is 2 for the
   requesting client and 3 for other clients.
- With Speed 0, phase 2 sends the Move event itself (agentsvr!0x01017c47).

**R-MOVE-4 MoveTo while hidden.** [DOC] [L:moveto-method] + [OBS]
- With a nonzero Speed, the Moving animation phase fails (R-ANIM-9). The frame is
  still moved, instantly.

**R-DIR-1 Direction of a point relative to a rectangle** (agentsvr!0x01015845). [OBS]
- Inputs: rectangle (l, t, r, b), point (px, py), slope `m = (b - t) / (r - l)`
  (floating point).
- Compute:
  - `A = b + m*(px - r)`, which is on the top-left to bottom-right diagonal;
  - `B = t + m*(r - px)`, which is on the bottom-left to top-right diagonal.
  - Both are truncated toward zero.
- Classify:
  - `py > A` and `py > B`: **Down (8)**;
  - `py > A` only: **ScreenLeft (1)**;
  - `py > B` only: **ScreenRight (3)**;
  - neither: **Up (2)**.
- Degenerate rectangles:
  - zero width: `px >= l` gives 3, otherwise 1;
  - zero height: `py >= t` gives 8, otherwise 2.
- The four triangles formed by the diagonals extend to infinity, so the rule also
  works for points inside the rectangle.

Test vectors for a frame at (100, 100) to (228, 228):

| Point | Expected |
|---|---|
| (0, 164) | ScreenLeft |
| (400, 164) | ScreenRight |
| (164, 0) | Up |
| (164, 400) | Down |

### 2.3 Gesturing (GestureAt, type 0x0A, agentsvr!0x0101a14a) [OBS] [DOC]

**R-GEST-1 Direction.**
- R-DIR-1 is applied to the point against the current window rectangle:
  - ScreenLeft gives **GesturingRight**;
  - Up gives **GesturingUp**;
  - ScreenRight gives **GesturingLeft**;
  - Down gives **GesturingDown**.
- [L:agent-states]: the directions are relative to the character.

### 2.4 Speaking (Speak, Think) [OBS] [DOC]

**R-SPEAK-1 Two queue phases.** [INF] for the split; the handlers are [OBS]
- Phase 1 (type 8, agentsvr!0x0101a739) plays the pseudo-state "Speakable".
  - [DOC] [L:speak-method]: the provider keeps the current animation if it has a
    speaking (overlay) frame. Otherwise it uses the Speaking state.
- Phase 2 (type 9, agentsvr!0x01017d5b) shows the balloon and runs the speech.
- Think uses the same phase-2 path with the thought-balloon flag (internal style
  0x400, see 4.1).
- [DOC] [L:think-method]: Think does not change the animation state.

**R-SPEAK-2 Requires visible.**
- Phase 2 on a hidden character fails with 0x8004200A (agentsvr!0x0100b63e).
- Empty text with no URL completes immediately with success.

**R-SPEAK-3 Random alternative.** [OBS] [DOC]
- Speak text containing `|` is split into alternatives. One is chosen with
  `rand() % count`.
- The RNG is seeded once, lazily, with `GetTickCount()`.
- Evidence: agentsvr!0x01019515, around 0x01019700. [L:speak-method]

**R-SPEAK-4 Completion status of Speak and Think.** [OBS] agentsvr!0x0100b63e, 0x01017d5b, 0x0101bc24 (case 0x703)
- In the text-only path (no TTS voice, user disabled spoken audio, or Think), the
  output phase stores a result code in the manager. When end-of-speech arrives
  (message 0x703), the request fails with that code if it is negative, and
  completes successfully otherwise.
- The result is:
  - **0 (success)** when "Play spoken audio" is on, but the character has no TTS
    voice or the request is a Think;
  - **0x80042016 (failure)** when the user turned off "Play spoken audio".
- In both cases the text is still shown word by word in the balloon (R-BAL-8). Only
  the final Status differs.
- [INF] 0x80042016 likely corresponds to string 10189 ("...spoken audio output for
  all characters has been disabled in the Advanced Character Options"). It was
  matched by meaning only.
- Think always takes the text-only pacer path, even when a TTS voice exists.

### 2.5 Idle [OBS] (all numbers below are from agentsvr; the docs give none)

Idle state lives in the per-character manager object (window procedure
agentsvr!0x0101bc24).

**R-IDLE-1 Arming.** The idle timer (id 100) is armed with **4000 ms** when all of
these hold:
- the character's queue becomes empty (the queue thread posts 0x610,
  agentsvr!0x0101682c);
- the active client has IdleOn = true;
- the character is visible;
- the timer is not already armed.

Evidence: agentsvr!0x0101a9f1.

**R-IDLE-2 Firing.** When the timer fires (agentsvr!0x0101aa4f):
1. Disarm it.
2. If this is the first idle animation since the last reset (counter = 0), send
   **IdleStart** to every client of the character.
3. Pick the level from the counter `n`:
   - `n < 10`: IdlingLevel1;
   - `10 <= n < 20`: IdlingLevel2;
   - `n >= 20`: IdlingLevel3.
4. Play that state (R-STATE-1). On success, increment `n` and mark "idle animation
   playing".

**R-IDLE-3 Re-arming.** When an idle animation completes (internal 0x701/0x702
while "idle playing" is set), the 4000 ms timer is armed again. So every idle
animation is followed by 4 s of the last frame and then the next pick.
- Level 1 lasts for 10 idle animations, level 2 for the next 10, and level 3 is
  used indefinitely.
- Evidence: agentsvr!0x0101bc24 (cases 0x701 to 0x702, then 0x0101a9f1).

**R-IDLE-4 Reset.** Any new request queued for the character (internal 0x60E,
agentsvr!0x0101bc24) immediately:
1. disarms the timer;
2. stops a playing idle animation (0x60B);
3. if IdleStart had been sent, sends **IdleComplete** to every client;
4. resets `n = 0`.

Evidence: agentsvr!0x01018117. Setting IdleOn = false runs the same reset
(agentsvr!0x0101ab3b). Setting it true re-arms when the queue is empty.
- Mouse clicks, drags and balloon activity do **not** reset idle by themselves.
  Only queued requests do. [OBS] (no call from the mouse paths).

**R-IDLE-5 Missing idle state.** If playing the idle state fails (for example the
character has no IdlingLevel2 animations), then:
- if `n` was 0, it is set to 1, so IdleStart is not sent again;
- the 4000 ms timer is re-armed;
- `n` is not otherwise advanced.

So a character without IdlingLevel1 animations stays at `n = 1` and retries every
4 s. Evidence: agentsvr!0x0101aa4f.

**R-IDLE-6 Memory trim.** At `n` = 0, 10 and 20 the server trims its working set.
This is irrelevant to bentwire. [OBS]

Test (fake clock, character with all three levels):
- Queue empty at t = 0. Idle animations start at t = 4000 and then 4000 ms after
  each one ends.
- Picks 1 to 10 are level 1, picks 11 to 20 are level 2, picks 21 and later are
  level 3.
- A Play at any point stops the idle animation, sends IdleComplete and restarts the
  count at level 1 after that Play completes plus 4 s.

### 2.6 Listening and Hearing (speech input; out of scope for bentwire v1)

[OBS] agentsvr!0x01019e45 and 0x01019e8a:
- When the listening hot key is pressed, the server stops the input-active
  character (cause "interrupted because the Listening key was pressed",
  0x8004210A) and plays **Listening**.
- When an utterance starts, it plays **Hearing**.
- When the utterance ends, if nothing is queued, it returns to **Speaking**.
- The timeouts are 5000 ms and 10000 ms (agentsvr!0x01011cb1, 0x01013cb1).
- [L:listen-method]: Listen(True) lasts 10 s.

---

## 3. Request queue

**R-Q-1 One FIFO per character.** [DOC] [OBS]
- Play, Speak, Think, MoveTo, GestureAt, Show, Hide, Wait, Interrupt and queued
  Get are appended to the character's queue. They run strictly in order, on a
  dedicated queue thread.
- The method returns at once with a Request object.
- Evidence: [L:animating-a-character], [L:the-request-object]; the queue thread is
  agentsvr!0x0101682c; the dispatcher is agentsvr!0x0101bc24 (case 0x60E).

**R-Q-2 Internal request types** (dispatch switch in agentsvr!0x0101bc24). [OBS]

| Type | Meaning | Handler |
|---|---|---|
| 0 | No-op or completion | |
| 2 | Get (Animation, State or WaveFile) | 0x01017b24 |
| 3 | Play | 0x01019f9a |
| 4 | Play state by name (internal) | 0x0101b889 |
| 5 | MoveTo animation phase | 0x0101a22f |
| 6 | MoveTo slide phase | 0x01017c47 |
| 7 | MoveTo end phase | 0x0101a373 |
| 8 | Speak/Think "Speakable" phase | 0x0101a739 |
| 9 | Speak/Think output phase | 0x01017d5b |
| 0x0A | GestureAt | 0x0101a14a |
| 0x0B | Show/Hide | 0x0101a46f |
| 0x0C | Wait | |
| 0x0D | Interrupt | |

The meaning of 0x0C and 0x0D is [INF] from the docs order and from the cross-window
messages they send.

**R-Q-3 Immediate (not queued) calls.** [DOC]
- Stop, StopAll, Activate, Listen, ShowPopupMenu, Unload, and property get/set
  (IdleOn, Balloon.Style, MoveCause and so on) act immediately.
- Evidence: [L:stop-method], [L:stopall-method], [L:activate-method],
  [L:listen-method].
- Balloon.Visible changes are deferred while the character is speaking, moving or
  being dragged ([L:visible-property-bal]).

**R-Q-4 Stop and StopAll masks.** [OBS] agentsvr!0x0101ad97 [DOC]

| Mask bit | Name | Covers |
|---|---|---|
| 0x01 | Play | Play (3), GestureAt (10) |
| 0x02 | Move | MoveTo phases (5, 6, 7) |
| 0x04 | Speak | Speak/Think phases (8, 9) |
| 0x20 | Show/Hide | Show/Hide (11) |

- Get and NonQueuedGet use further bits. [INF]
- `Stop()` with no argument removes everything except a Showing/Hiding that is in
  progress.
- `StopAll()` with no type also stops Showing/Hiding ([L:stop-method],
  [L:stopall-method]).
- Stopped requests complete with Status 3 (interrupted). The cause code is one of
  these. The pairing of codes and strings is [INF] (by meaning and order;
  0x8004210A and 0x8004210B appear in agentsvr!0x01019e45 and 0x01019e8a, the
  others were not seen in the code):
  - 0x8004210C "interrupted by the application";
  - 0x8004210D "because the character was hidden";
  - 0x8004210A "Listening key";
  - 0x8004210B "spoken input".
- These are the intl strings 10140 to 10143.
- `Stop(request)` for another character fails (string 10134). `Interrupt` of
  oneself fails (10135). `Wait` on one's own request fails (10136).

**R-Q-5 Request object.** [DOC] [L:the-request-object]
- Status values:
  - 0 complete;
  - 1 failed;
  - 2 pending;
  - 3 interrupted;
  - 4 in progress.
- RequestStart and RequestComplete go only to the client that holds the request.
- Wait(req) blocks this character's queue until `req` (normally another
  character's) completes.
- Interrupt(req) is queued in the calling character's queue. When it is reached,
  it stops the running looping animation of the other character's request without
  flushing that character's queue.

**R-Q-6 Request completion of state animations.** [OBS]
- A Play or state request completes when the animation ends (R-ANIM-3).
- A looping animation (branches with no exit to the end) never completes on its
  own and blocks the queue until it is stopped or interrupted
  ([L:creating-animations]).

**R-Q-7 Bookmarks.** [OBS] [DOC]
- `\Mrk=n\` raises a Bookmark(n) event when reached.
- The server reserves n = 2147483646 (0x7FFFFFFE, "next word") and 2147483647
  (0x7FFFFFFF, "end of speech"). Using them is an error (string 10137).
- Evidence: agentsvr!0x0100d79c, agentsvr!0x01009d37. [L:mrk-tag]

---

## 4. Speech balloon

### 4.1 Style and defaults

**R-BAL-1 Mapping from the API Style to the internal style.** [OBS] agentsvr!0x01016dd2 [DOC] [L:style-property]

API Style bits:
- bit0 BalloonOn;
- bit1 SizeToText;
- bit2 AutoHide;
- bit3 AutoPace;
- bits 16..23 CharsPerLine (8..255);
- bits 24..31 NumberOfLines (1..128).

Out-of-range values raise E_INVALIDARG (agentsvr!0x0101e6c8, 0x0101e747).

Internally (for test traceability):
- 0x200 means balloon on and 0x100 means off;
- 0x10000 means size-to-text;
- 0x20000 means **no** auto-hide;
- 0x40000 means **no** auto-pace;
- 0x400 means the thought balloon (Think).

**R-BAL-2 Fallback defaults** when the character data gives none. [OBS] agentsvr!0x010066bc (constructor), 0x01008962

| Property | Fallback |
|---|---|
| NumberOfLines | 2 |
| CharsPerLine | 28 |
| Text colour | system tooltip text colour (COLOR_INFOTEXT) |
| Background | tooltip background (COLOR_INFOBK) |
| Border | window frame colour (COLOR_WINDOWFRAME) |
| Font | "MS Sans Serif", height -13 (about 10 pt at 96 DPI), weight 400, not italic, charset 1 |

- bentwire maps the colours to black, #FFFFE1 and black.
- The character's own values come from the ACS balloon block (acs-format).
- The user may force one font for all characters (Advanced Character Options,
  Output page, "A single font for all characters").

### 4.2 Geometry (agentsvr!0x010067f0, 0x01006c30, 0x01007285, 0x010074ff)

**R-BAL-3 Body size.** [OBS] With TM = text metrics of the balloon font:
- body width `W = CharsPerLine * TM.aveCharWidth`, capped at the virtual-screen
  width;
- body height `H = lines * (TM.height + 2) + 18`;
- text area = body inset by **10 px** on every side, so the text width is `W - 20`;
- line pitch = `TM.height + 2`.

`lines` is NumberOfLines, or with SizeToText the wrapped line count of the whole
text (R-BAL-6), capped at 128. With SizeToText the line count is also reduced
until H fits the virtual-screen height.

**R-BAL-4 Shape.** [OBS] agentsvr!0x010074ff
- **Speak** balloon: a rounded rectangle with corner ellipse 16 x 20 px, plus a
  triangular tail.
  - The tail's tip is at the anchor point (R-BAL-5). Its base sits on the body edge.
  - The tail is `tailLen` long, with base half-width `tailHalf`.
  - Speak defaults: tailLen = 16 px, tailHalf = 4 px, margin 9.
  - The body and tail are filled together with the background colour.
  - A 1 px border is drawn around the outline in the border colour.
  - The window region is that outline.
- **Think** balloon (internal 0x400): tailLen = 37, tailHalf = 7, margin 15.
  - Instead of a triangle, three ellipses are drawn toward the anchor, getting
    smaller nearer the character.
  - For a balloon to the right of the character (tail on its left), the circles
    are 10 x 10, 11 x 12 and 12 x 14 px, starting at the tip.
  - Exact rectangles per side are in agentsvr!0x010074ff. Port them as constants.
  - Each circle has the background fill and a border pen.
- Colour-key transparency (palette index 253) is used for the non-layered path.
  The layered path uses per-pixel alpha (UpdateLayeredWindow).

**R-BAL-5 Placement.** [OBS] agentsvr!0x01007285

Inputs:
- the character window rectangle C;
- the **full monitor rectangle** M of the monitor containing C. This is
  rcMonitor, not the work area: the taskbar is not avoided.

Rules:
1. **Above (preferred).**
   - Window top = `C.top - tailLen - H`.
   - If that is `>= M.top`, the balloon goes above the character. The tail is at
     the bottom and points down.
   - Window height = `H + tailLen`.
   - Left = `C.left + (C.width - W)/2`, clamped so that `left >= M.left` and
     `left + W < M.right` (left = `M.right - W - 1` at most).
2. **Beside (no room above).**
   - Window top = `max(C.top, M.top)`.
   - If `C.left > (M.right - M.left)/2`, the balloon goes **left** of the
     character, with the tail on the balloon's right edge: window left =
     `C.left - tailLen - W`.
     - Note: this compares against half the monitor width, not the monitor centre.
       That matters on secondary monitors. Reproduce it.
   - Otherwise it goes **right** of the character: window left = `C.right`, and the
     body is offset by tailLen so the tail is on its left.
3. There is **no placement below** the character. A balloon placed beside the
   character may run past the bottom of the screen; no vertical clamp is applied
   in that case.
4. **Anchor (tail tip).**
   - If the current frame has a mouth overlay, the anchor is the mouth point of the
     first defined overlay type (agentsvr!0x0100aa4b calls the provider's mouth
     point getter agentdp2!0x71204ef0; see acs-format "mouth point").
   - Otherwise the anchor is the centre of the character frame.
   - It is scaled by the current/original size ratio and translated to screen
     coordinates, then clamped into M.
5. **Tail position along the edge.**
   - Side tails keep the tip's y between `top + 12 + tailHalf` and
     `bottom - 12 - tailHalf` of the window.
   - Bottom tails keep the tip's x between `left + 10 + tailHalf` and
     `right - 10 - tailHalf`.
6. The balloon window is a topmost tool window (WS_POPUP, WS_EX_TOOLWINDOW, layered
   when sprites are used), shown without activation.

Test vectors: with M = 1024 x 768 and a 128 x 128 character at (448, 400), W = 200
and H = 54:
- the balloon window is (412, 330) to (612, 400) with the tail at the bottom;
- with the character at (900, 10): left = 900 - 16 - 200 = 684, top = 10, tail on
  the right;
- with the character at (10, 10): left = 138, tail on the left.

**R-BAL-6 Word wrap.** [OBS] agentsvr!0x01006c30
- Words are the tokens between break characters. The break character is space,
  or U+200B for languages that need it (per LANGID, agentsvr!0x0102e1de).
- A word goes on the current line if the line width so far, plus one break-char
  width (except at line start), plus the word's width fits the text width.
  Otherwise a new line starts.
- A single word wider than a whole line is broken at the longest prefix that fits.
  The rest continues on the next line, and leading break chars are skipped.
- Right-to-left languages lay out from the right edge, with the same rule mirrored.

### 4.3 Show, pace, scroll and hide

**R-BAL-7 Showing.** [OBS] agentsvr!0x0100b63e, 0x01008047
- Speak or Think shows the balloon empty (sized per R-BAL-3) before the first word,
  unless the character is being dragged.
- BalloonShow and BalloonHide events go to the character's clients
  ([L:balloonshow-event]).

**R-BAL-8 Text-only pacing (no TTS, or spoken audio disabled).** [OBS] agentsvr!0x0100a871, 0x010099c8, 0x0100b3df
- Words are revealed one at a time:
  - the first word immediately;
  - each following word after an interval of `352 * defaultSpeed / effectiveSpeed`
    ms (integer, truncated).
- defaultSpeed is the character's TTS speed in words per minute. It is **170**
  when the character has no TTS voice.
- `effectiveSpeed = clamp(defaultSpeed * userFactor, 50, 250)`.
- userFactor comes from the "Speaking speed" slider (Output page):
  `slider * 0.1 + 0.5`. The default slider position gives 1.0.
- So by default a word appears every **352 ms**.
- Bookmarks in the text fire when the pacer reaches them.
- One tick after the last word, the pacer issues the internal end-of-speech
  bookmark. That ends the Speak output phase with the status from R-SPEAK-4 and
  starts auto-hide (R-BAL-10).
- So text-only Speak of N words lasts `N * interval` ms.
- With AutoPace off (internal 0x40000), the whole text is shown at once [DOC]
  [L:style-property]. The end timing still follows the pacer [INF].

**R-BAL-9 Line advance and scroll.** [OBS] agentsvr!0x01006efc
- When a word does not fit on the current line, it moves to the next line.
- When the balloon already shows NumberOfLines lines, the content scrolls up by
  one line pitch, with a smooth pixel scroll.
- The scroll aims to finish within one word time at the current speed
  (`60000 / effectiveSpeed` ms). The pixel step grows when it falls behind.
- The oldest line is discarded.

**R-BAL-10 Auto-hide.** [OBS] agentsvr!0x01009d37, 0x01006995, 0x01008194
- At end of speech, if AutoHide is on, a **1500 ms** timer starts. When it fires,
  the balloon is hidden.
- With AutoHide off, the balloon stays until any of:
  - the next Speak or Think;
  - Hide;
  - the user clicks or drags a character that is not speaking ([DOC]
    [L:style-property], [L:visible-property-bal]).
- The balloon is also hidden immediately:
  - when a MoveTo actually moves the frame (R-MOVE-2);
  - at drag start (R-UI-2);
  - when the character is hidden.

**R-BAL-11 Speak with TTS.** [OBS] [DOC]
- The parsed text is fed to the TTS engine.
- After each word the server inserts an internal bookmark 0x7FFFFFFE
  (agentsvr!0x0100e037). When the engine reports it, the next word is added to the
  balloon (agentsvr!0x01009d37). So the balloon follows the audio word by word.
- Tags (\Chr, \Ctx, \Emp, \Lst, \Map, \Mrk, \Pau, \Pit, \Rst, \Spd, \Vol) are
  parsed by agentsvr (strings at agentsvr!0x0100e128). Only \Map and \Mrk affect
  text-only output.
- `\Map="spoken"="balloon"\` shows the balloon text while speaking the spoken text
  ([L:map-tag]).

**R-LIP-1 Mouth choice.** [OBS] agentdp2!0x712037a2, 0x71204cbb (table in acs-format 5.4)
- The engine supplies one phoneme at a time, as an IPA code point or an ASCII
  letter. A fixed 116-entry table maps it to mouth type 0..6.
- The overlay of that type is drawn over the speaking frame.
- An unknown phoneme keeps the previous mouth.

**R-LIP-2 Engines that give only mouth geometry.** [OBS] mslwvtts!0x609c51f3 (case 0x46D)
- [INF] for the field name: the .LWV pseudo engine converts the first byte of its
  mouth record, read here as mouth height, to a representative
  phoneme for old (v1) sinks:
  - `< 0x10`: 'b';
  - `< 0x30`: 'f';
  - `< 0x50`: 'e';
  - `< 0x70`: 'ɑ';
  - otherwise: 'ɜ'.
- A label phoneme in the file overrides this. Silence ('_') becomes 'm'.
- bentwire can use the same quantisation to drive mouths from a Linux TTS that
  gives amplitude but not visemes. [INF] design suggestion.

**R-LIP-3 Text-only Speak and the mouth.** [INF] (see U-1)
- The text-only pacer (R-BAL-8) only adds words and fires bookmarks. No mouth
  shape is set on that path in agentsvr.
- So the character holds its speaking frame without moving overlays. Status:
  unverified.

---

## 5. User interaction

**R-UI-1 Click and double-click.** [OBS] agentsvr!0x0100c6a3 (window procedure), 0x0101a7cc
- Mouse-down on the character window starts tracking: capture plus a
  remembered grab point.
- A second button-down within the system double-click time turns into DblClick.
  The server times this itself, using GetTickCount and GetDoubleClickTime.
- Click and DblClick events go to the active client. Arguments:
  - button bits: 1 left, 2 right, 4 middle;
  - shift state;
  - screen x, y.
- If the click is a **right** click and the active client has AutoPopupMenu =
  true, the popup menu opens at the cursor (agentsvr!0x01026160).
- A left click while the client is in Help mode calls WinHelp instead (out of
  scope).
- [DOC] [L:click-event]: clicking does not stop the character's output.

**R-UI-2 Drag.** [OBS] agentsvr!0x0100b180, 0x0101a7cc
1. Dragging starts when the pointer moves **at least 2 px** on either axis from
   the grab point while the left or right button is held.
2. At drag start:
   - the balloon is hidden;
   - a running MoveTo slide is cancelled (R-MOVE-2);
   - **DragStart** is sent. Alt held adds bit 0x20 to the shift flags internally.
3. During the drag, the window top-left follows the pointer minus the grab offset.
   - The motion is 1:1.
   - There is no clamping to the screen.
   - **No animation is played and the current animation is not changed.**
4. On release:
   - **DragComplete** is sent;
   - every client gets the **Move** event with cause 1 (user);
   - MoveCause becomes 1.

**R-UI-3 Character popup menu** (agentsvr!0x01026160). [OBS] [DOC]

The menu is built fresh every time, in this order:
1. **Only if speech input is installed and enabled**: "Open &Voice Commands Window"
   (string 102), or "Close &Voice Commands Window" (string 103) when it is open.
2. "&Hide" (string 182) when the character is visible, otherwise "&Show" (183).
3. When visible, the commands of the input-active client. [INF] This part rests
   on agentsvr!0x0102812a, which was not read, and on [L:pop-up-menu-support]:
   - its Commands caption as the header, then a separator ([L:pop-up-menu-support]);
   - then a separator and the captions of the other clients of the character, to
     switch clients (agentsvr!0x0102542f).
   - Only visible commands are listed. Disabled ones are greyed.

Other behaviour:
- Only one menu exists at a time. A new one cancels the old one.
- The menu position is clamped to the virtual screen.
- The chosen item is executed after the menu closes.
- There is **no "Advanced Character Options" item** in the character menu. The
  string exists (111) but its use site was not located (U-5).

For bentwire v1 without speech input, the menu is "&Hide" plus the client
commands.

Menu strings (intl, string table):

| ID | en-US (agt0409.dll) | de-DE (agt0407.dll) |
|---|---|---|
| 102 | Open &Voice Commands Window | &Sprachbefehlsfenster öffnen |
| 103 | Close &Voice Commands Window | &Sprachbefehlsfenster schließen |
| 182 | &Hide | &Ausblenden |
| 183 | &Show | &Einblenden |
| 111 | Advanced Character Options | Erweiterte Figuroptionen |
| 115 | Hide (voice command name) | Ausblenden |
| 113 | Voice Commands | Sprachbefehle |
| 150 | Global Commands | Globale Befehle |
| 101 | Microsoft Agent | Microsoft Agent |

**R-UI-4 Tray icon.** [OBS] agentsvr!0x0101bc24 (messages 0x401 to 0x404), 0x0101b910, 0x0101b377, 0x0101b9b6 [DOC]
- **Each loaded character has its own notification-area icon.** The tray callback
  and "TaskbarCreated" are handled in the per-character manager window.
  - The icon image is the character's own icon, fetched from the data provider
    when the character is attached (agentsvr!0x0101b9b6).
  - The icon is registered with callback message 0x401 and icon id 0x401.
  - When the taskbar restarts (registered message "TaskbarCreated"), the icon is
    re-added.
- Tooltip (agentsvr!0x0101b377): on the first mouse-over, the tooltip is set to
  the character's name in the current UI language.
  - Names of 64 characters or more are cut to 60 and get the suffix "...".
  - [INF] The suffix is read from the data at agentsvr!0x01002ba8, which holds
    "..." followed by " + ".
- Left button **up** on the icon:
  - shows the character, with VisibilityCause 2 (user);
  - sends Click with button 1 and the taskbar flag.
- Right button up sends Click with button 2 and the taskbar flag. With
  AutoPopupMenu it shows the same menu:
  - character menu items when the character is visible;
  - only the server items when it is hidden ([L:pop-up-menu-support]).
- Double-click sends DblClick with the taskbar flag, and the following button-up
  is swallowed.
- **Conflict**: the binary ORs **0x1000** into the button flags for taskbar clicks.
  The docs ([L:click-event]) say bit 13 (**0x2000**). The control layer may
  translate it. bentwire exposes a separate boolean `from_tray` and does not use
  either constant.

**R-UI-5 Advanced Character Options.** [OBS] (dialog templates in agt0409/agt0407) [DOC] [L:the-advanced-character-options-window]

A property sheet titled "Advanced Character Options" / "Erweiterte Figuroptionen"
with three pages. The control texts are verbatim from the dialog resources.

**Output / Ausgabe:**
- [x] &Display spoken output in word balloon using / &Gesprochene Ausgabe in der
  Sprechblase anzeigen
  - ( ) Each &character's default font settings / mit der Standardschriftart jeder &Figur
  - ( ) A si&ngle font for all characters / mit e&iner Schriftart für alle Figuren
  - [&Font...] / [S&chriftart...]
- [x] &Play spoken audio / &Sprachausgabe abspielen
  - &Speaking speed: / S&prachgeschwindigkeit: Slower to Faster, with Default /
    Langsamer to Schneller, with Standard. The factor is `0.1*slider + 0.5`.
- [x] Play character sound &effects / Sound&effekte der Figur verwenden

**Speech Input / Spracheingabe:**
- Characters &listen for input / Figuren &achten auf Spracheingabe
- hot key
- "And &continue for N seconds or until you release the key" (verbatim English; the
  German reads "Und &diese N Sekunden drücken oder bis Sie sie loslassen")
- Display listening &tips / Zuhör&tipps anzeigen
- &Play tone when you can speak / Klang ab&spielen, wenn Spracheingabe bereit

**Copyright:** "Microsoft Agent", "Copyright (c) 1996-1998 Microsoft Corp.",
version.

The settings are persisted under `HKCU\Software\Microsoft\Microsoft Agent`, with
value names from the agentsvr strings:
- UseBalloon;
- UseCharacterFont;
- UserFont;
- UserFontForeColor;
- EnableSpeaking;
- SpeakingSpeed;
- UseSoundEffects;
- VoiceEnabled;
- and others.

bentwire v1 recommendation [INF]: a config file with the same knobs (balloon on,
single font, speech audio, speed factor, sound effects). It can be exposed over
D-Bus later.

**R-UI-6 Hide from the menu.** [DOC] [OBS]
- Choosing &Hide queues a server Hide for the character (R-SHOW-2) with
  VisibilityCause 7.
- Hide from the tray menu or voice uses cause 1.
- The Hide event goes to all clients ([L:hide-event], [L:visibilitycause-property]).

---

## 6. Window behaviour

**R-WIN-1 Window kind.** [OBS] agentsvr!0x0100cf8b (creates class `AgentAnim`, window procedure 0x0100c6a3)
- Each character is a borderless popup with extended style **topmost and tool
  window** (0x88): always on top, and not in the taskbar or Alt-Tab.
- In sprite mode (registry `UseSprites`) the window is also **layered**.
- Its size is the character frame size, scaled when the client sets Width/Height.
- A topmost-free mode exists (`NonTopmostAgentWindowParent`, agentsvr!0x0102e235)
  [INF]: used while a full-screen app or screen saver runs.
- Wayland mapping [INF]: use a layer-shell overlay surface (or xdg-toplevel with
  "keep above" where possible), with no decorations and no taskbar entry.

**R-WIN-2 Transparency.** [OBS] agentsvr!0x0100a90d, acs-format 5.1
- The visible shape is the union of the frame images' opaque pixels, applied as a
  window region, or alpha in layered mode.
- Pixels of the transparent colour index are click-through. The input region
  equals the shape.
- Wayland: set the input region from the same mask.

**R-WIN-3 MoveTo speed.** See R-MOVE-2. The default Speed is 1000 ms
([L:moveto-method]). With Speed 0 the move is instant and no Moving animation is
played.

**R-WIN-4 Screen edges.** [OBS]
- MoveTo and dragging do **not** clamp to the screen (agentsvr!0x0100bd63,
  0x0100b180). Characters may go partly or fully off screen.
- Only on a display configuration change (WM_DISPLAYCHANGE, agentsvr!0x010187f3)
  is the frame pulled back inside the monitor rectangle:
  - right and bottom edges first;
  - then left and top.
- Every client then gets a Move event with cause 4, and MoveCause becomes 4.
- The balloon clamps itself (R-BAL-5).

**R-WIN-5 Z-order.** [OBS] agentsvr!0x01007285, 0x01008047
- The balloon is positioned after the character window in z-order, or topmost when
  there is no reference window.
- Neither window takes focus.

---

## 7. API surface

### 7.1 Original objects (2.0 control; type library in agentctl.dll; 1.5 subset in agtctl15.tlb) [OBS] [DOC] [L:character-object-methods], [L:character-object-properties]

**Agent (control):**
- Characters
- AudioOutput (Enabled, SoundEffects)
- SpeechInput
- PropertySheet (Left, Top, Height, Width, Page, Visible)
- CommandsWindow
- Connected
- RaiseRequestErrors
- ShowDefaultCharacterProperties

**Characters:**
- `Load(CharacterID, Provider)` gives a Request
- `Unload(CharacterID)`
- `Character(CharacterID)`
- item enumeration

**Character methods:**
- `Activate([State])`
- `GestureAt(x, y)`
- `Get(Type, Name, [Queue])`
- `Hide([Fast])`
- `Interrupt(Request)`
- `Listen(State)`
- `MoveTo(x, y, [Speed=1000])`
- `Play(AnimationName)`
- `Show([Fast])`
- `ShowPopupMenu(x, y)`
- `Speak([Text], [Url])`
- `Stop([Request])`
- `StopAll([Types])`
- `Think(Text)`
- `Wait(Request)`

**Character properties:**
- Active
- AnimationNames (2.0 enumeration)
- AutoPopupMenu
- Balloon
- Commands
- Description
- ExtraData
- GUID
- HasOtherClients
- Height
- HelpContextID
- HelpFile
- HelpModeOn
- IdleOn
- LanguageID
- Left
- MoveCause
- Name
- OriginalHeight
- OriginalWidth
- Pitch
- SoundEffectsOn
- Speed
- SRModeID
- SRStatus
- Top
- TTSModeID
- Version
- VisibilityCause
- Visible
- Width

**Balloon:**
- Enabled
- Style
- NumberOfLines
- CharsPerLine
- FontName
- FontSize
- FontBold
- FontItalic
- FontStrikethru
- FontUnderline
- FontCharSet
- ForeColor
- BackColor
- BorderColor
- Visible

**Commands:**
- Add
- Insert
- Remove
- RemoveAll
- Command(Name)
- Caption
- Voice
- VoiceCaption
- Visible
- Enabled
- DefaultCommand
- FontName
- FontSize
- GlobalVoiceCommandsEnabled
- HelpContextID

**Command:**
- Caption
- Confidence
- ConfidenceText
- Enabled
- Name
- Visible
- Voice
- VoiceCaption
- HelpContextID

**Request:**
- ID
- Status
- Number
- Description

**Events:**
- ActivateInput
- ActiveClientChange
- AgentPropertyChange
- BalloonHide
- BalloonShow
- Bookmark
- Click
- Command
- DblClick
- DeactivateInput
- DefaultCharacterChange
- DragComplete
- DragStart
- HelpComplete
- Hide
- IdleComplete
- IdleStart
- ListenComplete
- ListenStart
- Move
- RequestComplete
- RequestStart
- Restart
- Show
- Shutdown
- Size

### 7.2 What matters on Linux

**Keep (v1):**
- Load, Unload;
- Show, Hide, Play, Speak (text-only, R-BAL-8), Think, MoveTo, GestureAt;
- Stop, StopAll;
- Wait (across characters);
- IdleOn;
- Balloon.Style, Visible;
- Left/Top/Width/Height;
- AnimationNames;
- Commands (popup menu items);
- AutoPopupMenu.

**Events (v1):**
- RequestStart, RequestComplete;
- Click, DblClick;
- DragStart, DragComplete;
- Move;
- Show, Hide;
- IdleStart, IdleComplete;
- BalloonShow, BalloonHide;
- Bookmark;
- Command.

**Later:**
- TTS (TTSModeID, Pitch, Speed; map to speech-dispatcher or piper);
- .LWV/.WAV Url in Speak;
- Interrupt;
- Activate and the multi-client input model;
- Get (HTTP/ACF).

**Drop:**
- speech recognition (Listen, SRModeID, SRStatus, Listening/Hearing, Voice
  Commands Window);
- HelpModeOn, HelpFile, HelpContextID;
- PropertySheet COM object;
- marshaling.

### 7.3 Recommended D-Bus v1 (bus name `org.bentwire.Agent`, sketch) [INF]

Object `/org/bentwire/Agent`, interface `org.bentwire.Agent1`:
- `Load(s path) -> o character`
- `Unload(o)`
- `ListCharacters() -> ao`

Object `/org/bentwire/Character/<id>`, interface `org.bentwire.Character1`:

| Method | Returns | CLI |
|---|---|---|
| `Show(b fast)` | request id `u` | `bentwire show [--fast]` |
| `Hide(b fast)` | `u` | `bentwire hide [--fast]` |
| `Play(s animation)` | `u` | `bentwire play <anim>` |
| `Speak(s text)` | `u` | `bentwire say <text>` |
| `Think(s text)` | `u` | `bentwire think <text>` |
| `MoveTo(i x, i y, u speed_ms)` | `u` | `bentwire moveto X Y [--speed MS]` |
| `GestureAt(i x, i y)` | `u` | `bentwire gestureat X Y` |
| `Stop(u request)` (0 = all except Show/Hide in progress) | | `bentwire stop` |
| `StopAll(as types)` | | `bentwire stop --all` |
| `Wait(o other_character, u request)` | `u` | |
| `AnimationNames() -> as` | | `bentwire animations` |

Properties: `Visible b`, `Left i`, `Top i`, `Width u`, `Height u`, `IdleOn b`,
`BalloonStyle u`, `Name s`, `MoveCause u`, `VisibilityCause u`.

Signals:
- `RequestStart(u)`
- `RequestComplete(u id, u status, u code)`
- `Click(u button, u shift, i x, i y, b from_tray)`
- `DblClick(...)` (same arguments)
- `DragStart(...)`, `DragComplete(...)`
- `Move(i x, i y, u cause)`
- `Shown(u cause)`, `Hidden(u cause)`
- `IdleStart`, `IdleComplete`
- `BalloonShown`, `BalloonHidden`
- `Bookmark(i id)`
- `Command(s name)`

Plain CLI calls are one-shot: they print the request id and, with `--wait`,
block until RequestComplete. Status values as in R-Q-5.

---

## 8. Unknowns and open questions

- **U-1** Mouth behaviour during text-only Speak, with no TTS.
  - No mouth-setting call was found on the pacer path (R-LIP-3).
  - Whether the server cycles mouths or shows the plain speaking frame is
    unverified. Nothing was run.
- **U-2** The exact path from the SAPI4 visual or phoneme notification inside
  agentsvr to the agentdp2 mouth selection was not traced. In particular: does
  the server use the engine's IPA phoneme or the TTSMOUTH geometry? The table
  itself is known (R-LIP-1).
- **U-3** Exact sequencing of exit branches after Stop and StopAll.
  - Known: stop cuts immediately (R-ANIM-7), and the provider switch logic plays
    exit or return on the next animation (acs-format 5.3).
  - Not verified: whether the server inserts the exit or return animation even
    when the next request is a Hide or a MoveTo slide. Also open: whether the
    "Speaking" return in R-MOVE-3 interacts with it.
- **U-4** `rand()` state is per thread in msvcrt.
  - The provider seeds on process attach but runs branching on the queue/UI
    thread.
  - An unseeded thread would start from seed 1, which would make branch sequences
    repeat across runs.
  - Which thread calls the provider was not determined. This only matters for
    bit-exact replay, which is not a goal.
- **U-5** Where string 111 "Advanced Character Options" is used.
  - Likely the property sheet caption, or a tray menu item loaded through another
    helper.
  - The tray menu's exact items when no character is loaded were not confirmed.
- **U-6** The click dispatch helper agentsvr!0x01009a92 (button-up to Click/DblClick
  translation and the 4-button mapping) was read only in outline.
- **U-7** The taskbar click flag: 0x1000 in the server versus 8192 (0x2000) in the
  docs (R-UI-4).
- **U-8** Idle gating flag at manager offset 0x78 ("Hearing in progress") and offset
  0x84 (input or suspend state). Idle is suppressed while they are set. Their
  exact meaning was not traced.
- **U-9** Get, NonQueuedGet and Interrupt (types 0x0C and 0x0D) masks and handlers
  were not fully read. Only the Play, Move, Speak and Show/Hide masks are
  confirmed.
- **U-10** Behaviour when AutoPace is off. The end-of-speech timing is assumed to
  stay word paced (R-BAL-8) but was not verified.
- **U-11** Balloon text drawing details: the colour of the text pen (assumed
  ForeColor), the exact first-line y offset (assumed 10 px inset), and
  underline/strike rendering through Uniscribe.
- **U-13** Which internal phase of a multi-phase request (MoveTo 5/6/7, Speak 8/9)
  carries the client's Request object. This decides when RequestStart and
  RequestComplete fire and which phase's status reaches the client.
  - Best reading [INF]: RequestStart on the first phase, RequestComplete with the
    status of the last phase.
  - Not verified; the enqueue path and the request constructors
    (agentsvr!0x0102ce71 to 0x0102d490) were not read.
- **U-14** The table that maps error codes 0x800420xx and 0x800421xx to strings
  10120 and up was not located. All such pairings in this file are by meaning.
- **U-12** Archived copies of the old docs could not be read. All [DOC] citations
  use the learn.microsoft.com copy, as collected by a subagent. Slugs not
  individually re-verified: restart-event and shutdown-event returned 404 there.
