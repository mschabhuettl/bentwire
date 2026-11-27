# Office XP Office Assistant on top of Microsoft Agent (MSO.DLL 10.0.2625)

First assessment for bentwire. Scope: how MSO.DLL drives Microsoft Agent, what is
Office-specific (balloon, tips, options, gallery, right-click menu) and what a Linux
reproduction would take.

Evidence tags used below:

- **[obs]** observed in the shipped bytes (address given, image base 0x30B00000).
- **[inf]** inferred from observed code, not proven.
- **[pub]** public documentation (URL given).
- **[unk]** not established.

All descriptions are written in my own words. No decompiled Microsoft code is
reproduced. No Microsoft tip texts are quoted (none are stored in MSO.DLL anyway).

## Method

- Input: `MSO.DLL` (Office XP, 10.0.2625), SHA-256
  `14b7a79ad2cd96524671f8a63d63ed665253efcf02da74a814fec83f33225134`, x86 PE, image
  base 0x30B00000. Never executed.
- The REA MCP server holds one active target per server, and during this work it was
  bound to `agentsvr.exe` by a parallel session. To avoid disrupting that session I did
  not re-open MSO.DLL in REA. MSO.DLL was analysed with a linear-sweep disassembly
  (capstone) of the whole `.text` section, `pefile` for imports, exports and resources,
  and a small hand-written MSFT typelib parser for the Office typelib embedded in
  MSO.DLL and the Agent typelib in `agentsvr.exe`.
- Caveats of that method: caller lists only see direct `E8`/`E9` calls, and MSO.DLL
  splits functions into hot and cold chunks, so every caller count below is a lower
  bound. Virtual calls were resolved by matching vtable offsets against the Agent
  typelib.
- Work files (not public): scripts `pe.py`, `lst.py`, `calls.py`, `tlb.py`, `tlb2.py`
  and the full listing `full.lst`.
- Second pass, same method, on further Office XP files (German), all read-only and never
  executed:
  - `OBALLOON.DLL` 10.0.2607 (image base 0x324A0000): full listing `ob.lst`; bitmaps
    rendered to a scratch directory.
  - `MSOINTL.DLL` 10.0.2609, German 1031: string groups (type 217) parsed with a small
    script; the SDM dialog template (type 221) was decoded by hand.
  - `OFTIP10.HLP`, `OLTIP10.HLP`, `MSTIP10.HLP`: my own WinHelp reader with LZ77 and
    Hall decompression.

  The OBALLOON and MSOINTL files are German builds, so captions below are German.
- Third pass: `BLNMGR.DLL` 10.0.2607 (SHA-256
  `aa43a71da074aa1a0fe249a699d23e35c724f84ae2925a9e4291812591e0701f`, image base
  0x324E0000), listed in `bm.lst`. Same method, never executed.
- Fifth pass: `OBALLOON.DLL` 10.0.2607 again (SHA-256
  `686dfb1a491d73454691cbb5a1e272bb24348aa07c5647aa23ee26e985608f96`), same method, never
  executed. It covers the owner-draw and strip code (R-OB-8..11, R-OB-36), the size method
  and the placement choice (R-OB-31, R-OB-34). Every address cited in that pass was
  checked afterwards by decoding the single instruction at that address from the file
  bytes and comparing it with the claim.

## Architecture in one paragraph

Office host apps (Word, Excel, ...) do not talk to Agent. They talk to MSO.DLL through
a C++ interface MSO calls "TFC" (exports `MsoFCreateITFC`, `MsoFCreateITFCHwnd`,
`MsoFCreateITFCNoAssistant`, `MsoDestroyITFC`; vtable at 0x30BE5CA0) **[obs]**. MSO owns
one worker thread that is the only thread touching Agent COM objects; the UI thread
posts numbered requests to it with `PostThreadMessage` (message 0xD00) and waits on an
event (poster at 0x30C76F88, dispatcher with 22 commands at 0x30C77049) **[obs]**. The
Assistant balloon is not Agent's normal Speak balloon. Its content is drawn by a separate
Office DLL, `oballoon.dll`, which MSO loads and calls through an interface from the
export `FCreateIOBalloon` (0x30C73FC7) **[obs]**. Its outer frame (the yellow
window with rounded corners, black outline and tail) is drawn by `blnmgr.dll`. The Agent
server loads that COM component in-process on oballoon's request, through a private
Agent interface (section 2, R-OB-20..30) **[obs]**. VBA's `Assistant`/`Balloon` objects (typelib
in MSO.DLL) are thin wrappers over the same TFC paths **[obs]**.

## 1. How MSO.DLL talks to Microsoft Agent

### COM objects and interfaces [obs]

GUIDs found in MSO.DLL and named via the `agentsvr.exe` typelib:

| GUID | Name | Use in MSO |
|---|---|---|
| D45FD2FC-5C6E-11D1-9EC1-00C04FD7081F | CLSID AgentServer | `CoCreateInstance` at 0x30C75290 |
| 48D12BA0-5B77-11D1-9EC1-00C04FD7081F | IAgentEx | server pointer, marshalled to the worker thread (0x30C752AF, 0x30C73FAE) |
| 6BA90C00-3910-11D1-ACB3-00C04FD97575 | IAgentCommandsEx | queried from the character at 0x30C7673D |
| B0913412-3B44-11D1-ACBA-00C04FD97575 | IAgentCommandEx | per menu command (0x30C767F7) |
| 08C75162-3C9C-11D1-91FE-00C04FD701A5 | IAgentNotifySinkEx | MSO's own sink, QI at 0x30C775C6, vtable 0x30C73F24 |
| A7B93CA0-7B81-11D0-AC5F-00C04FD97575 | IAgentAudioOutputPropertiesEx | referenced at 0x30C746F8 |

Not present anywhere in MSO.DLL: the IIDs of IAgentBalloon / IAgentBalloonEx **[obs]**.

### Agent calls actually made [obs]

Character load (0x30C75168): `IAgentEx::Load` with the .acs path, `GetCharacterEx`,
then `SetAutoPopupMenu(FALSE)`. MSO then finds the Agent window class `AgentAnim` to
learn the server's UI thread.

Worker commands (dispatcher jump table at 0x30C77072), mapped to IAgentCharacterEx
vtable slots:

| Cmd | Agent call |
|---|---|
| 1 | load character (above) |
| 3 | `Show` |
| 4 | `Hide` |
| 5 / 6 / 7 | `SetPosition` / `GetPosition` / `GetSize` |
| 8 | `GetActive` |
| 9 | `StopAll(play)` then `Play(name)` with the name taken from the animation table |
| 10 | `StopAll(play)` |
| 11 | `GetActive`, `Activate` |
| 12 / 13 / 14 | `HasOtherClients` / `GetVisible` / `GetVisibilityCause` |
| 15 | enable menu commands, then `ShowPopupMenu(x, y)` |
| 16 / 17 / 18 | `GetName` / `GetDescription` / `GetExtraData` |
| 19 | `SetHelpFileName`, `SetHelpContextID(40001)`, help mode |
| 20 | `SetSoundEffectsOn` |
| 21 | `SetLanguageID` (Office UI language) |
| 22 | release everything (shutdown) |

Also used elsewhere: `SetIdleOn` (6 call sites), `SetHelpModeOn`, `SetSize` is not used.
Never called through the character pointer: `Speak`, `Think`, `MoveTo`, `GestureAt`,
`Listen`, `Interrupt`, `Wait` **[obs, lower bound per Method]**. Consequence: Office
moves the character with `SetPosition` itself and never uses Agent's text output.

Notify sink handlers that do real work **[obs]**: `Command` (menu, see section 4),
`Click` (posts an internal "open the Assistant balloon" request; 0x30C76E68),
`DblClick`, `DragStart`/`DragComplete`, `VisibleState` (logs "Agent: Visible State"),
`RequestComplete`, `Idle`, `Move`, `Size`, `HelpComplete`.

### Animation table: enum, internal index, Agent animation name [obs]

Three layers, all observed:

1. Public enum `MsoAnimationType` (typelib in MSO.DLL).
2. Internal index 1..35, converted by the function at 0x3108E414 (reverse at 0x3108E492).
3. Agent animation name, from the UTF-16 pointer table at 0x30C749AC (index 1) to
   0x30C74A34 (index 35), played by worker command 9.

| msoAnimation... (value) | idx | Agent animation name |
|---|---|---|
| Idle (1) | 1 | Idle1_1 |
| Greeting (2) | 2 | Greeting |
| BeginSpeaking (4) | 3 | Explain |
| CharacterSuccessMajor (6) | 4 | Congratulate |
| GetAttentionMajor (11) | 5 | GetAttention |
| GetAttentionMinor (12) | 6 | Wave |
| Searching (13) | 7 | Searching |
| Printing (18) | 8 | Print |
| GestureRight (19) | 9 | GestureRight |
| WritingNotingSomething (22) | 10 | Writing |
| WorkingAtSomething (23) | 11 | Processing |
| Thinking (24) | 12 | Thinking |
| SendingMail (25) | 13 | SendMail |
| ListensToComputer (26) | 14 | Hearing_1 |
| Appear (32) | 15 | Show |
| GetArtsy (100) | 16 | GetArtsy |
| GetTechy (101) | 17 | GetTechy |
| GetWizardy (102) | 18 | GetWizardy |
| CheckingSomething (103) | 19 | CheckingSomething |
| LookDown (104) .. LookUpRight (111) | 20..27 | LookDown, LookDownLeft, LookDownRight, LookLeft, LookRight, LookUp, LookUpLeft, LookUpRight |
| Saving (112) | 28 | Save |
| GestureDown / Left / Up (113..115) | 29..31 | GestureDown, GestureLeft, GestureUp |
| EmptyTrash (116) | 32 | EmptyTrash |
| RestPose (5) | 33 | RestPose |
| Goodbye (3) | 34 | Goodbye |
| Disappear (31) | 35 | Hide |

Special case **[obs]** (0x30FB07AF): when the UI language is Japanese and the character
file is Rocky.acs or Offcat.acs, index 4 (Congratulate) is replaced by index 18
(GetWizardy). Reason unknown **[unk]**.

### Which event plays which animation

Host apps request animations by internal index through TFC slot 16 (0x30C7795C, guarded
by "assistant on and visible", then into the play routine at 0x30CF7606) **[obs]**. The
mapping for app events (print, save, send mail, empty trash, spelling, ...) is therefore
in the app executables, which were not provided.

| Event | Animation | Evidence |
|---|---|---|
| Assistant shown | two variants, chosen by a caller flag and an "animations allowed" check: either an animated Agent `Show` with no extra animation, or a fast (non-animated) Agent `Show` followed by `Greeting` (or `RestPose` when greeting is not allowed) | [obs] 0x30C747BC..0x30C7483A, 0x312801B0..0x312801F7 |
| Assistant hidden by user/code | either `Goodbye` queued, then a fast Agent `Hide` (falls back to an animated `Hide` if Goodbye could not be queued), or directly an animated Agent `Hide` | [obs] 0x31280206..0x31280251 |
| New tip available (Assistant visible [inf]) | lightbulb placed next to the character, `Wave` (GetAttentionMinor) | [obs] 0x30F1D86E..0x30F1D88D |
| Look toward a screen point | one of the eight `Look*` animations, chosen by which side of the character's rectangle the point lies on. Names are from the character's point of view, so a point to the screen-left plays `LookRight` | [obs] 0x30F15357..0x30F153E1; trigger [unk] |
| "Animate!" menu item | next entry of a shuffled list of animations, 5 s timer | [obs] 0x31284C42 |
| Balloon display path | `Explain` (BeginSpeaking) | [inf] 0x31282BC5, in balloon code |
| Help search setup (near Answer Wizard language keys) | `Thinking` | [inf] 0x312837A8 |
| Other MSO-internal sites | `Congratulate` (0x30FF0554), `Writing` (0x3108A191), `Processing` (0x31178DE3), `GetWizardy` (0x30FBDBA5) | trigger not identified |
| Printing, saving, sending mail, emptying trash, searching, wizard start | `Print`, `Save`, `SendMail`, `EmptyTrash`, `Searching`, `GetWizardy` | [inf] from enum names only; triggers live in host apps |
| Idle | MSO calls `SetIdleOn`; Agent itself plays the character's Idling states | [obs] SetIdleOn calls; [pub] Agent behavior |

### Animation-name coverage in the provided characters [obs, string scan]

Case-insensitive scan of UTF-16 names in each .acs. Names are case-insensitive in
Agent; Clippit stores `GoodBye`.

- All 35 names present: CLIPPIT, DOT, F1, LOGO, MNATURE, OFFCAT, ROCKY (the Office set).
- merlin.acs lacks 14 (for example Greeting, Print, Save, SendMail, Goodbye, GetArtsy).
- courtney, earl and rover lack about 20 each; qmark showed no names (possibly stored
  differently, so not conclusive).

bentwire needs case-insensitive lookup plus a fallback map (for example Greeting to Wave
or Show, Goodbye to Hide, Save or Print to Processing or Writing).

## 2. The Office Assistant balloon

### Who draws it

First pass (MSO.DLL only):

- MSO.DLL loads Gimme token 0x70, which its file table resolves to **`oballoon.dll`**
  (table at 0x30BA34A8), through the localized-library loader. It then calls the export
  `FCreateIOBalloon` to get an interface stored at 0x3135F72C **[obs]**.
- MSO initialises that interface with the IAgentEx pointer and the character ID
  (0x30C75762) **[obs]**.
- MSO.DLL never registers the window class `MSOBALLOON`. It only looks it up with
  `FindWindow` / `GetClassName` (0x30B7519D, 0x30F8CFD9) **[obs]**.
- No Agent speech-balloon interface is used and `Speak` is never called **[obs]**.

Second pass, with OBALLOON.DLL 10.0.2607 (German build, image base 0x324A0000, sole
export `FCreateIOBalloon` at 0x324AD5CC):

- The balloon is split in two layers **[obs]**:
  - **Content layer, drawn by oballoon.dll.** It is a child window of class
    `MsoBalloonChild` (style WS_CHILD | WS_VISIBLE; registered at 0x324A2FFE, created at
    0x324A37FA). oballoon paints heading, icon, text, label rows and owner-drawn buttons
    straight into that window's DC. It also creates native child controls (see R-OB-14).
  - **Frame layer (shape, outline, tail), not drawn by oballoon.dll.** oballoon imports no
    region or shape API at all: no CreateRoundRectRgn, CreatePolygonRgn, SetWindowRgn,
    RoundRect or layered-window calls. It only asks a "frame site" object to make a frame
    of a given size (virtual call at 0x324A358E with width, height and two layout flags).
- **Where the frame site comes from [obs]:** at init (0x324AD7C4, 0x324AD9A0) oballoon
  takes the IAgentEx pointer MSO passed in. It queries it for the undocumented private
  Agent interface `{D6589121-FC70-11D0-AC94-00C04FD97575}`. That IID is also present in
  `agentsvr.exe` (0x0100179C) and in the proxy `agentmpx.dll`. It then registers an
  extension identified by `{3F8A6C33-E0FD-11D0-8A8C-00A0C90C2BC5}` and gets back an
  object queried for `{3F8A6C31-E0FD-11D0-8A8C-00A0C90C2BC5}`, which becomes the frame
  site (global 0x324B3BC8).
- **What the Agent server does with that extension [obs, agentsvr.exe]:**
  - A server routine (0x01027A3B) copies a CLSID received by value from its caller and
    calls `CoCreateInstance(CLSID, NULL, CLSCTX_INPROC_SERVER, IID_IDispatch)`.
  - So the Agent server loads a COM class named by the client into its own process.
  - Matching this routine to the D6589121 method oballoon calls (slot 0x1C) is [inf].
- **The frame component is BLNMGR.DLL [obs, third pass].**
  - BLNMGR.DLL 10.0.2607 (79 KB, image base 0x324E0000, PDB name `BLNMGRO.pdb`) is an
    ATL in-proc COM server: it exports DllGetClassObject and DllRegisterServer.
  - Its registry script (REGISTRY resource 101) registers `BlnMgr.BlnMgr.1` with
    CLSID {3F8A6C33-E0FD-11D0-8A8C-00A0C90C2BC5}, threading model Apartment. That is
    exactly the CLSID oballoon hands to the Agent server.
  - It also registers proxy classes (BlnMgr, BlnUser, BlnSet proxies).
  - It registers the window classes **`MSOBALLOON`** (frame window, 0x324E14C1) and
    **`MattDillon`** (a hidden helper window that MSO looks for with FindWindow).
- **Division of labour [obs]:**

  | Component | Process | Job |
  |---|---|---|
  | MSO.DLL | Office app | decides what to show |
  | OBALLOON.DLL | Office app | lays out and paints the content in `MsoBalloonChild` |
  | Agent server (agentsvr.exe) | Agent | hosts BLNMGR.DLL in-process |
  | BLNMGR.DLL | inside agentsvr | owns the top-level `MSOBALLOON` window, its region (shape and tail), outline, background and position next to the character |

  The content window is parented into the frame window across the process boundary.
  BLNMGR reads the character rectangle itself through IAgentCharacter
  GetPosition/GetSize (0x324E5226) **[obs]**.

### What the balloon can contain (contract) [obs from typelib, plus pub]

From the Office typelib in MSO.DLL:

- **Balloon properties:** BalloonType, Icon, Heading, Text, Mode, Animation, Button,
  Callback, Private, Labels(1..5), Checkboxes(1..5).
  Methods: Show, Close, SetAvoidRectangle.
- **MsoBalloonType:** Buttons 0, Bullets 1, Numbers 2. Labels render as clickable
  option rows, as bullets or as a numbered list.
- **MsoIconType:** None 0, Alert 2, Tip 3 (light bulb), AlertInfo 4,
  AlertWarning 5, AlertQuery 6, AlertCritical 7.
- **MsoModeType:** Modal 0, AutoDown 1, Modeless 2. My reading (from memory, unverified):
  AutoDown closes on the next click elsewhere, and Modeless needs a Callback, which the
  error codes ButtonlessModal and ButtonModeless are consistent with.
- **MsoButtonSetType:** None, OK, Cancel, OkCancel, YesNo, YesNoCancel, BackClose,
  NextClose, BackNextClose, RetryCancel, AbortRetryIgnore, SearchClose, BackNextSnooze,
  TipsOptionsClose, YesAllNoCancel (0..14).
- **MsoBalloonButtonType** return values: OK -1, Cancel -2, Yes -3, No -4, Back -5,
  Next -6, Retry -7, Abort -8, Ignore -9, Search -10, Snooze -11, Close -12, Tips -13,
  Options -14, YesToAll -15. A clicked label returns its 1-based index.
- **MsoBalloonErrorType** includes TooBig, BadPictureRef, ButtonlessModal,
  ButtonModeless, CharNotTopmostForModal and TooManyControls. These describe the
  validation rules.

Public documentation:

- Up to 5 labels and 5 checkboxes. A label click dismisses the balloon; checkboxes need
  a button. [pub] https://learn.microsoft.com/en-us/previous-versions/office/developer/office-xp/aa141368(v=office.10)
- Inline text markup: `{cf n}` sets a colour from the 16 system palette colours,
  `{ul 1}`/`{ul 0}` toggles underline, `{bmp path}` and `{wmf path}` embed pictures.
  The icon sits at the upper left next to the heading.
  [pub] https://techshelps.github.io/VBA/ofun/ch12e.htm ,
  https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2010/ms248014(v=office.14)
- MSO itself builds tip text that starts with a custom-bitmap code `{CBMP 1 ...}`
  (UTF-16 string at 0x30EE6D38, used at 0x311D7A96) **[obs]**. So the markup parser
  (in oballoon.dll) also knows CBMP **[inf]**.

### Look and layout: rules from OBALLOON.DLL

Each rule cites its evidence. "Content" means the area inside the frame, in the
`MsoBalloonChild` window. Colours are written as #RRGGBB. COLORREF constants from the
binary were converted, so 0x00800000 becomes #000080.

**Rule R-OB-1 (background colour).** Content background and the brush used for child
controls is pale yellow #FFFFCC. It is created at init as a palette-relative COLORREF
0x02CCFFFF (0x324A2CF0) **[obs]**. The host can override it through the colour setter
(0x324A33EA, which sets the five colours below) **[obs]**.

**Rule R-OB-2 (text colours).** Defaults, overridable by the host **[obs, values at
0x324B30B0..B8 and 0x324B3874]**:

| Use | Colour |
|---|---|
| Body text | black (#000000) |
| Label text | navy #000080 |
| Second label colour | purple #800080 |
| Disabled / focus colour | grey #808080 |

That the purple is the hot or visited state is **[inf]** (selected by a flag at
0x324A3D78).

**Rule R-OB-3 (content inset and tail reservation).**

- oballoon's own layout produces a balloon size W x H (struct passed from 0x324A6161 to
  0x324A617F).
- The content area is (W - 12) x (H - 12), i.e. 6 px on every side (0x324A62F0).
- The content origin moves 16 px right when the tail is on the left, and 0 otherwise.
- oballoon then passes the content size and two layout flags to the frame site
  (0x324A358E).

All of this is **[obs]**. How the layout chooses W, including any maximum width, is
**[unk]**. A bounded pass over the sizing code (0x324A6354..0x324A6560) found only the
work-area-based free-space computations, no fixed width constant.

**Rule R-OB-4 (fonts).** All fonts are Tahoma, from RT_STRING resources (102 to 105) and
also from MSOINTL strings, with positive `lfHeight` (cell height in pixels):

| Screen width | Heading | Body, labels, buttons |
|---|---|---|
| under 1024 px | Tahoma, height 13, bold | Tahoma, height 13, regular |
| 1024 px or more | Tahoma, height 16, bold | Tahoma, height 16, regular |

- Evidence: string table plus selection at 0x324A873A by the flag set at 0x324A2B79
  from GetSystemMetrics(SM_CXSCREEN) < 0x400 **[obs]**.
- Every font also exists in an underlined variant (`lfUnderline` = 1), used for
  `{ul 1}` and for hot labels **[obs creation, inf use]**.
- The host can rewrite the LOGFONT through a callback before creation (0x324A8777), for
  example to apply Office's UI font **[obs call, inf purpose]**.
- The spec parser also accepts a `p` suffix for point sizes, scaled by max(LOGPIXELSY,
  96) / 72 (0x324AEA05) **[obs]**.

**Rule R-OB-5 (heading and icon).**

- A balloon icon is drawn at the content's top left, 32 x 32 (DrawIcon at 0x324AA93E).
  The heading text then starts 37 px from the left: 32 for the icon plus 5 px gap
  (0x324AA8EA).
- The heading uses the bold font. When the heading is shorter than the icon, the
  following content starts below the icon **[obs, inf exact rounding]**.
- Icon sources **[obs ids, inf mapping]**:
  - `msoIconTip` uses oballoon's own icon group 100, a 32 x 32 yellow light bulb
    (4-bit and 8-bit images).
  - The alert icons are the Windows stock icons IDI_HAND, IDI_QUESTION, IDI_EXCLAMATION
    and IDI_ASTERISK (0x7F01..0x7F04, loaded at 0x324A9A23).

**Rule R-OB-6 (block spacing).** Heading, body text, label list, check boxes and buttons
are stacked vertically. Between blocks the layout adds half the body font's tmHeight
(global 0x324B37F0, set at 0x324A2DFB). The exact placement is in R-OB-35 **[obs]**.

**Rule R-OB-7 (label rows).** Labels are owner-drawn rows (BUTTON with BS_OWNERDRAW).

- Each row has a prefix column, then the wrapped label text. The text starts after the
  prefix column plus 5 px (0x324A67E5).
- The prefix column width is the largest of these, with a floor of 17 px:
  - the bullet bitmap width (15);
  - the width of "1.";
  - the width of the bullet character;
  - the 17 px arrow glyph.

  Evidence: 0x324A2D12..0x324A2DA7 **[obs]**.
- Prefix by `BalloonType`:

  | BalloonType | Prefix |
  |---|---|
  | Buttons | blue round "button" bullet, bitmap 101 |
  | Bullets | the character from string 106, `0x2022` (the bullet character) |
  | Numbers | "n." |

  Evidence: dispatch at 0x324A66D4 **[obs]**.
- The row height is the larger of the prefix height and the wrapped text height. The
  prefix is vertically centred on the first line (0x324A67C9..0x324A6817) **[obs]**.
- RTL languages (Arabic 0x401, Hebrew 0x40D) mirror the row: prefix on the right,
  right-aligned text (0x324A2E26, 0x324A6827) **[obs]**.

**Rule R-OB-8 (label bullets and the tip bulb bullet).** Both are 15 x 15 strips of 5
frames, loaded as vertical strips of 5 (0x324A2B84..0x324A2BB6). Listed top to bottom,
frame 0 is the mask and frames 1 to 4 are images. The state order is proven by the
drawing code (R-OB-36) **[obs]**:

| State | Strip index | Frame, top to bottom |
|---|---|---|
| mask | 4 | 0 |
| normal | 0 | 4 |
| hot (pointer over the row) | 1 | 3 |
| pressing (first frame of the press) | 2 | 2 |
| pressed (last frame of the press) | 3 | 1 |

- There is no disabled frame.
- **101:** cyan-to-navy glass ball, the label bullet. Normal is the darkest frame; hot
  and pressed get brighter.
- **102:** small light bulb, from pale to saturated yellow-green. It replaces 101 when a
  flag byte of the host's balloon description (offset 0x51) is set **[obs]**.
  - Only the search balloon copies that flag (0x324A99D4); the generic balloon clears it
    (0x324AB02E, where ebx is still the 0 set at 0x324AAF7A) **[obs]**.
  - The two setters are the setup methods in slot 11 of the two balloon vtables:
    - generic balloon, vtable 0x324A17BC: 0x324AAF70, size method 0x324AB4BA in slot 21;
    - search balloon, vtable 0x324A1728: 0x324A9918, size method 0x324A9D66 in slot 21.
  - Which host requests set the flag is **[unk]**.

**Rule R-OB-9 ("See previous..." / "See also..." rows).**

- Long label lists (search results) are paged with two special rows captioned by
  strings 128 "Siehe vorherige..." and 129 "Siehe auch..." (German) **[obs]**.
- Their prefixes are the arrow bitmaps 103 (up) and 104 (down). These are 17 x 45 strips
  of 5 frames of 17 x 9 (count 5, frame size 17 x 9 at 0x324A974E..0x324A977F). The item
  measure reports 17 x 9 for them (0x324A4FAE) **[obs]**.
- They are drawn by the bullet routine, so they have the same layout and state order as
  the bullets **[obs]**.

**Rule R-OB-10 (keyboard focus).**

- **Labels:** the focused label gets a closed one-pixel dotted outline (CreatePen PS_DOT,
  grey #808080). Removing focus redraws the same polyline in the background colour
  (0x324A6BA2) **[obs]**.
- **Push buttons:** DrawFocusRect is drawn around the caption, not inset from the frame
  (0x324A59FE..0x324A5A23, drawn at 0x324A5BA9) **[obs]**.
  - The rectangle is the caption's text extent, centred in the button before any pressed
    shift, widened by 2 px on the left and right and by 1 px at the bottom.
  - It is drawn when the draw request carries ODS_FOCUS, and when a hover redraw finds
    that the button has the keyboard focus.
- **Other owner-drawn items** with ids from 1000 up also use DrawFocusRect (0x324A50BB),
  inset by 1 px from their item rectangle **[obs]**. What these items are is **[unk]**.

**Rule R-OB-11 (push buttons).** Buttons are owner-drawn. The bitmaps and the state order
are **[obs]**; the state order is proven by the drawing code (R-OB-36):

- Each button is assembled from three bitmap strips: left cap (11 px), stretchable
  middle (5 px), right cap (11 px).
- Two sizes:

  | Screen width | Bitmaps | Button height |
  |---|---|---|
  | under 1024 px | 105 / 106 / 107 | 22 px |
  | 1024 px or more | 108 / 109 / 110 | 24 px |

  Selection is by the same small-screen flag, at 0x324A2BC3..0x324A2C37.
- Each strip holds 10 frames. Listed top to bottom, frames 0 to 4 are masks and 5 to 9
  are images. Each image has its own mask. Black in an image frame is the transparent
  background of the OR blit, not an outline colour. The balloon background shows through,
  so buttons are outlines on yellow, not grey bevels.
- State order and look:

  | State | Strip index (image, mask) | Image frame, top to bottom | Look |
  |---|---|---|---|
  | normal | 0, 5 | 9 | 1 px light grey #C0C0C0 outline with 2 px corner cuts |
  | hot | 1, 6 | 8 | raised, 2 px: cream #E7E7D6 outside; inside it white along the top and left, #868686 along the bottom and right |
  | pressing | 2, 7 | 7 | sunken: the same with white and #868686 swapped |
  | pressed | 3, 8 | 6 | identical to pressing |
  | disabled | 4, 9 | 5 | identical to normal |

  - The pixels of image frames 5 and 9 are byte-identical, and so are those of 6 and 7.
    The difference between normal and disabled is only the caption colour.
  - There is no "default" state. ODS_DEFAULT and ODS_DISABLED are never tested (R-OB-36).
- **Captions:**
  - They use the body font, black, or #B2B2B2 when the button is disabled (0x324A58E8..
    0x324A58FC) **[obs]**.
  - They are drawn single-line and centred (DrawText flags 0x125).
  - In the pressing and pressed states the caption moves 1 px right and down
    (0x324A5A3C..0x324A5A4D, 0x324A5B57..0x324A5B65) **[obs]**.
  - When keyboard cues are off (flag 0x324B3694), the "&" markers are removed before
    drawing **[obs code, inf flag meaning]**.
- **Assembly:** the left cap is drawn at x 0, then (button width - 22) / 5 middle tiles
  at x 11, 16, ..., then the right cap after them (0x324A57EE..0x324A5B52) **[obs]**.
- German captions (RT_STRING 112 to 127): OK, Abbrechen, &Ja, &Nein, &Zurück,
  &Weiter, &Wiederholen, &Abbrechen, &Ignorieren, &Suchen, &Erneut erinnern (Snooze),
  Schließen, &Tipps, &Optionen, Ja, &alle, &Hilfe **[obs]**.

**Rule R-OB-12 (check boxes).** Check boxes are native Windows check boxes
(BS_AUTOCHECKBOX, control table entry 4 at 0x324A14AC). The glyph reserves 13 x 13 px
(globals 0x324B37F8/FC) **[obs]**. The control background is painted with the yellow
brush in the control-colour handler (0x324A51F7..0x324A5205) **[obs]**.

**Rule R-OB-13 (bullet character).** The bullet for `msoBalloonTypeBullets` comes from a
resource string (106 = "0x2022", U+2022), so it is localizable **[obs]**.

**Rule R-OB-14 (search field and drop-down).** The text input of the search balloon is a
`RichEdit20W` control with ES_MULTILINE | ES_AUTOVSCROLL [obs]. The colour handler
returns a white stock brush for one control kind (0x324A51FA); that this is the edit
field is [inf]. A `COMBOBOX` (CBS_DROPDOWNLIST) also exists for some balloons
**[obs control table, inf use]**.

**Rule R-OB-15 (text wrapping).**

- Wrapping uses the host's text-measure service (0x324B3BB8), with an optional word
  breaker `FindWordBreak` from `saext.dll` (0x324A4492).
- Line-start and line-end prohibition tables (kinsoku) apply for Japanese, Chinese
  (Simplified and Traditional) and Korean, chosen by UI language at 0x324A2E46..
  0x324A2ED6.

All of this is **[obs]**. There is no fixed maximum width. The width comes from the
area-based sizing in R-OB-31.

**Rule R-OB-16 (inline markup).** The text parser recognises the keywords `cf`, `ul`,
`bmp`, `wmf`, `pict`, `dlx` and `cbmp`, in a UTF-16 keyword table at 0x324B3034
**[obs]**. `pict`, `dlx` and `cbmp` are undocumented. `cbmp` is the keyword of MSO's tip string
`{CBMP 1 n}` **[obs]**. The meaning of `pict` and `dlx` is [unk].

`{cf n}` accepts only these values (0x324A3E80..0x324A3EAC) **[obs]**:

| n | Colour | n | Colour |
|---|---|---|---|
| 0 | #000000 | 248 | #808080 |
| 1 | #800000 | 249 | #FF0000 |
| 2 | #008000 | 250 | #00FF00 |
| 3 | #808000 | 251 | #FFFF00 |
| 4 | #000080 | 252 | #0000FF |
| 5 | #800080 | 253 | #FF00FF |
| 6 | #008080 | 254 | #00FFFF |
| 7 | #C0C0C0 | 255 | #FFFFFF |

These are the 16 static colours of the Windows system palette. Other values leave the
colour unchanged. A run without `cf` uses the default colour.

Underline is drawn with a separate pen as a polyline under the run. The thickness comes
from the font's outline metrics (0x324A92D2..0x324A9347) **[obs]**.

**Rule R-OB-17 (placement next to the character).**

- At creation oballoon reads the character's position and size through
  IAgentCharacterEx (GetPosition and GetSize at 0x324A34E4 and 0x324A34FB) **[obs]**.
- Anchor rectangle C (0x324A61B4) **[obs]**:
  - normally the character rectangle clipped to the monitor work area;
  - with no character, a 10 x 10 square at the bottom-right corner of the work area.
- Candidate balloon rectangles for a body of W x H (0x324A8550) **[obs]**:

  | Candidate | top | bottom | left | right |
  |---|---|---|---|---|
  | Above, left-aligned | C.top - H - 4 | C.top + 12 | C.left | left + W |
  | Above, right-aligned | C.top - H - 4 | C.top + 12 | C.right - W | left + W |
  | Beside, to the right | C.top | C.top + H | C.right - 12 | C.right + W + 4 |
  | Beside, to the left | C.top | C.top + H | C.left - W - 4 | C.left + 12 |

  So the window is 16 px larger than the body on the tail side. There is a 4 px gap
  between body and character, and the tail strip reaches 12 px into the character
  rectangle. No below-the-character candidate exists.
- Choice of candidate (0x324A85D6) **[obs]**: the free space of each candidate is
  computed against the work area (globals 0x324B3820..382C, from the multi-monitor
  wrapper) and against `SetAvoidRectangle` (IntersectRect at 0x324A86C1). The order and
  choice rules are in R-OB-34.
- If the balloon still does not fit, the character is moved by a delta through a
  SetPosition-slot call (0x324A614B) and the balloon re-anchors **[inf]**.

**Rule R-OB-18 (animation).** No fade or slide code: no SetLayeredWindowAttributes,
AnimateWindow or alpha imports. Timers exist (SetTimer at 0x324AD54A) **[obs]**. Their
use, probably auto-down or hover, is **[unk]**. So within oballoon the balloon appears
and disappears instantly **[inf]**.

**Rule R-OB-19 (modality).** Modal balloons create a helper window `MsoModalBWnd`
(WS_POPUP | WS_VISIBLE, 1 x 1 at (-10000, -10000), so off-screen; 0x324A3703). They
then enable it with EnableWindow. They run a
private message loop. Keyboard layout and keyboard state are saved and restored around it
(0x324A36A8) **[obs]**.

### Frame rules from BLNMGR.DLL (third pass)

Coordinates in R-OB-20..30 are frame-window coordinates.

- **B** is the body rectangle inside the window.
- **W x H** is the body size.
- **C** is the character rectangle (Agent GetPosition/GetSize) clipped to the monitor
  rectangle. It is computed with MonitorFromRect and IntersectRect into a global at
  0x324E5218 **[obs]**.

**Rule R-OB-20 (frame settings and defaults).** The BlnMgr object keeps a 20-byte
settings block (0x324E2BA8) **[obs]**:

| Setting | Default |
|---|---|
| Background | palette-relative #FFFFCC (0x02CCFFFF) |
| Horizontal margin | 6 px |
| Vertical margin | 6 px |
| Gap between balloon and character | 4 px |

The background brush is created from that colour at 0x324E2C94 **[obs]**. Whether a
client ever changes these defaults is **[unk]**. MSO and oballoon use the same values.

**Rule R-OB-21 (body size).** From the inner content size w x h it receives from
oballoon (method at 0x324E5454): W = w + 2 x 6 and H = h + 2 x 6 **[obs]**. This matches
oballoon's own inset (R-OB-3).

**Rule R-OB-22 (window).**

| Property | Value |
|---|---|
| Class | `MSOBALLOON` |
| Class styles | CS_OWNDC, CS_SAVEBITS (0x820) |
| Cursor | arrow |
| Window style | WS_POPUP, WS_BORDER |
| Extended style | WS_EX_TOPMOST, WS_EX_TOOLWINDOW |
| Created at | 0x324E536F |

The window is made non-rectangular with SetWindowRgn (0x324E5398, and again on every
resize at 0x324E7ACD) **[obs]**. There is **no drop shadow**: no CS_DROPSHADOW, no
layered window, no second window **[obs]**.

**Rule R-OB-23 (body shape and corner radius).** The body region is a rounded rectangle
from (B.left + 1, B.top + 1) to (B.right, B.bottom) with a 16 x 16 corner ellipse, so an
8 px corner radius (CreateRoundRectRgn at 0x324E559C). The tail triangle is merged into
it with CombineRgn(RGN_OR) (0x324E5620) **[obs]**.

**Rule R-OB-24 (outline and fill).**

- **Outline:** 1 px black. The window's own region is fetched, shifted by (-1, -1) into
  client coordinates (because of WS_BORDER) and framed with the stock black brush using
  a 1 x 1 frame (FrameRgn at 0x324E504A). So the outline follows the rounded body and
  the tail as one continuous line.
- **Fill:** the whole client area with the #FFFFCC brush (FillRect at 0x324E50BC).
- **Palette:** on 8-bit displays a halftone palette is selected first.

All of this is **[obs]**.

**Rule R-OB-25 (tail triangle).** The tail is a right triangle whose base of 10 px
lies on the body edge and whose length is 16 px (region code 0x324E55D7..0x324E5613,
cold variants 0x324E7F78..0x324E8072) **[obs]**. Six variants exist; 0 means no tail.
With t = tail offset along the edge and L = tail length:

| Variant | Side | Corners of the triangle |
|---|---|---|
| 5 | bottom | (t, B.bottom-1), (t+10, B.bottom-1+L), (t+10, B.bottom-1). The vertical edge is on the right. |
| 6 | bottom | (t, B.bottom-1), (t, B.bottom-1+L), (t+10, B.bottom-1). The vertical edge is on the left. |
| 4 | right | (B.right-1, t), (B.right-1+L, t), (B.right-1, t+10). The horizontal edge is on top, the tip points right. |
| 3 | right | same side as 4, with the horizontal edge at the bottom |
| 2 | left | (B.left+1, t), (B.left+1-L, t), (B.left+1, t+10). The horizontal edge is on top, the tip points left. |
| 1 | left | same side as 2, with the horizontal edge at the bottom |

Variants 1 and 3 are not selected by the placement code found (R-OB-27) **[obs]**.

**Rule R-OB-26 (tail length clamp).** L is 16 px but is shortened so the tip never goes
past the far edge of C. For a bottom tail that is C.bottom; for a side tail it is
C.right or C.left (0x324E55D7, 0x324E7F7F, 0x324E7FB5, 0x324E800E) **[obs]**.

**Rule R-OB-27 (which side, and tail position).** oballoon picks the quadrant and passes
two flags, "above" and "toward the left" (R-OB-17, 0x324A3572). BLNMGR places the
window and tail from them (0x324E54EC, 0x324E5634, 0x324E8099) **[obs]**:

| Placement | Window position | Tail |
|---|---|---|
| Above, left-aligned | x = C.left, y = C.top - 4 - H. Window W x (H + 16). | at the bottom |
| Above, right-aligned | x = C.right - W, same y. | at the bottom |
| Beside, balloon to the right | x = C.right + 4 - 16, y = C.top. Window (W + 16) x H. Body starts 16 px in. | on the left, variant 2 |
| Beside, balloon to the left | x = C.left - 4 - W, y = C.top. Window (W + 16) x H. | on the right, variant 4 |

Tail position t for a bottom tail:

1. t = 70 % of min(W, C.width), measured from the body's left edge. For a right-aligned
   balloon it is measured from the right edge instead.
2. If the tail would lie right of the character's horizontal centre, use variant 6 at t.
   Otherwise use variant 5 at t - 10. Either way the vertical edge sits at the 70 % point
   and the slanted edge leans toward the centre of the character.
3. Clamp t to [8, W - 18], so the 10 px base keeps at least 8 px from both rounded
   corners.

Tail position t for a side tail:

1. t = min(70 % of H, 30 % of C.height) from the body top.
2. Clamp t to [8, H - 18].
3. The tip then points horizontally at the character, 30 % down its height.

**Rule R-OB-28 (screen edges).** BLNMGR does not flip by itself. Flipping (above or
beside, left or right) is oballoon's candidate choice against the work area and the
avoid rectangle (R-OB-17), so the flags already arrive flipped.

- BLNMGR only clips C to the monitor (R-OB-20 note) and clamps the tail length (R-OB-26).
- The candidate order itself is not in BLNMGR either. It is in oballoon (R-OB-34).

**Rule R-OB-29 (resize and follow).**

- On a size change the frame does three things (0x324E7A96..0x324E7B06):
  - moves and resizes itself with SetWindowPos (SWP_NOZORDER | SWP_NOACTIVATE);
  - rebuilds the region;
  - moves its first child (the oballoon content window) to body origin plus margins.
- A 250 ms timer (0x324E5B98 and others; callback 0x324E4153) polls state, presumably
  to follow a moving character **[inf]**.
- A 20 s timer (0x324E5B3D; callback 0x324E729E) manages the OfficeAssistantMutex
  **[obs call, inf purpose]**.

**Rule R-OB-30 (visibility housekeeping, inside the Agent process).**

- BLNMGR checks the foreground window's class against `WindowsScreenSaverClass`
  (0x324E4FF0). It reads `AsstState` / `CurrAsstState` under
  `Software\Microsoft\Office\Common\Assistant` and its policy key.
- It can drive the character directly: StopAll, Show, then Play("RestPose")
  (0x324E688E..0x324E68C3, 0x324E7360..0x324E7395).
- So it restores the Assistant after obscuring situations such as the screen saver
  [inf].
- It has no text drawing APIs at all. Wrapping, maximum width and button layout are
  not in BLNMGR **[obs, import table]**.

### Remaining layout rules from OBALLOON.DLL (fourth pass)

Notation: **TM** is the text metrics of the regular body font (R-OB-4); **G** =
floor(TM.tmHeight / 2) (global 0x324B37F0).

**Rule R-OB-31 (balloon size: area based, golden ratio, no fixed maximum).**

1. **Text blocks.** Every text block is measured unwrapped (0x324A7E2E):
   - The text is split into runs at control characters. Each run adds its width x height
     (the host's text-measure service) to the block's area.
   - An inline picture adds its own width x height. The widest picture is the block's
     minimum width, so plain text has none.
   - A line break ends a line. Its height is max(tallest item, TM.tmHeight) (0x324A7EC8).
     The last line counts max(tallest item, last run height).
   - The block height is the sum of its line heights.
2. **Required area A and minimum size.** The generic balloon's size method (0x324AB4BA,
   virtual slot 0x54, called at 0x324A600D) adds up the terms in the table below. The
   search balloon has its own method (0x324A9D66), not re-read in this pass.
3. **Two candidate shapes (0x324A6046..0x324A605E).** Shape 0 (landscape) and shape 1
   (portrait) both start at the minimum. Each grows until width x height >= A, tested
   before every step (0x324A7FCD..0x324A7FDA). The proportion test (0x324A8028) works in
   integers:
   - landscape: in proportion once 618 x w >= 1000 x h;
   - portrait: once 1000 x w >= 618 x h.

   The growth (0x324A7FAC) runs in three phases:
   - Phase 0, if the start is not in proportion: the width grows by 10 px until the test
     passes.
   - Phase 1, if the start is already in proportion: the height grows by 10 px until the
     test fails.
   - Phase 2, after either: the longer side grows by 10 px and the other is set to
     longer x 618 / 1000, truncated (0x324A8064). Phase 2 assigns that side, so it can
     come out a few px smaller than before.
4. **Fit.** If the minimum is wider or taller than the work area, the balloon is
   rejected (0x324A6021..0x324A603A). Which shape is used follows from the placement
   choice (R-OB-34).
5. **Wrapping.** Text is then wrapped at the chosen content width:
   - with the word-break rules of R-OB-15;
   - label and check-box text wraps at content width minus prefix column minus 5 px
     (R-OB-7).

Terms of the generic balloon's size method. PC is the prefix column (global 0x324B37F4,
R-OB-7) and BH the button height (global 0x324B3808, 22 or 24):

| Constant | What it is | Address | Tag |
|---|---|---|---|
| A = 1024, min w = 32, min h = 32 | an icon starts the sums with a 32 x 32 block | 0x324AB4EA..0x324AB4F7 | [obs] |
| heading | heading text area added to A; min h = max(min h, heading height); min w = max(min w, its widest picture) | 0x324AB502..0x324AB534 | [obs] |
| 12 x heading block | A += 12 x (min h so far: max(32, heading height) with an icon), then min h += 6 | 0x324AB537..0x324AB555 | [obs] |
| 12 x block height | body text, and the block measured after the check boxes (presumably the R-OB-35 footer [inf]): A += area + 12 x height, min h += height | 0x324AB589..0x324AB5A5, 0x324AB719..0x324AB735 | [obs] |
| PC + 5 | minimum width when there are labels, and again when there are check boxes | 0x324AB5D6, 0x324AB662 | [obs] |
| 15 | label row height floor for BalloonType Buttons (bullet frame height, global 0x324B380C) | 0x324AB5CD | [obs] |
| TM.tmHeight | label row height floor for Bullets and Numbers (global 0x324B36BC) | 0x324AB5C5 | [obs] |
| 13 | check-box row height floor (glyph height, global 0x324B37F8) | 0x324AB66D | [obs] |
| G + row | each label or check-box row adds G + max(floor, text height) to min h | 0x324AB62A..0x324AB63E, 0x324AB6C5..0x324AB6DD | [obs] |
| PC + 17 | each such row adds its text area + (PC + 17) x (G + row) to A | 0x324AB641..0x324AB647, 0x324AB6D4..0x324AB6E6 | [obs] |
| (bw + 10) x n - 10, + 30 | button row width (sets 15 and 17 add 30); min w = max(min w, row) | 0x324AB742..0x324AB767 | [obs] |
| row + 12 | A += (row + 12) x BH, min h += BH | 0x324AB76A..0x324AB779 | [obs] |
| + 12, + 6 | min w = widest + 12, min h = sum + 6 (12 with the 6 added after the heading) | 0x324AB785..0x324AB793 | [obs] |
| 12 x min w | A += 12 x (min w), the top and bottom insets | 0x324AB790..0x324AB79A | [obs] |
| 10 | growth step of both shapes | 0x324A7FE3, 0x324A7FFA, 0x324A806F, 0x324A8086 | [obs] |
| 618 / 1000 | the golden-ratio test and the dependent side | 0x324A8034..0x324A8054, 0x324A8074..0x324A808B | [obs] |

Read as geometry, A is the area of the whole body: each block's area plus 6 px insets on
its left and right, plus the top and bottom insets across the minimum width **[inf]**.
The 37 px heading indent and the 5 px icon gap (R-OB-5) are not part of A or of the
minimum width **[obs]**. The indent only enters through the fit's "too narrow" answer
(R-OB-34).

Values an earlier bentwire reading assumed, which do not occur in the size method
(0x324AB4BA..0x324AB79F) **[obs]**:
- a heading term of (37 + heading width) x max(32, heading height);
- a minimum width of 47 with an icon;
- a minimum width of prefix column + 15;
- a minimum height of 12 + 32 + button height;
- a label term of (text width + PC + 17) x max(TM.tmHeight, 15);
- a growth test on (w - 12) x (h - 12).

**Rule R-OB-32 (button row).** **[obs]** throughout.

- **Uniform width:** every button in a row gets the same width (0x324A6C64):
  1. take the widest caption, measured single-line in the body font (DrawText flags
     0x520), with a floor of 5 px;
  2. round up to a multiple of 5;
  3. add 22 px.
- **Height:** 22 or 24 px (R-OB-11).
- **Spacing:** buttons are 10 px apart (0x324A6F51). The two-button sets 15 and 17 add
  30 px more after each button, so their single gap is 40 px (0x324A6F58).
- **Alignment:** the row is centred: x0 = (content width - row width) / 2, where row
  width = n × (button width + 10) - 10 (+30 for sets 15 and 17) (0x324ABFCA..0x324ABFFB).
  RTL layouts walk the list backwards (0x324A6E74).
- **Captions and return values:** button ids 0..14 map in order to RT_STRING 112..126
  and to the return values -1..-15 (0x324A2F7E..0x324A2F9D):

  | Id | Caption (German) | Return value |
  |---|---|---|
  | 0 | OK | -1 |
  | 1 | Abbrechen | -2 |
  | 2 | &Ja | -3 |
  | 3 | &Nein | -4 |
  | 4 | &Zurück | -5 |
  | 5 | &Weiter | -6 |
  | 6 | &Wiederholen | -7 |
  | 7 | &Abbrechen (Abort) | -8 |
  | 8 | &Ignorieren | -9 |
  | 9 | &Suchen | -10 |
  | 10 | Er&neut erinnern (Snooze) | -11 |
  | 11 | Schließen | -12 |
  | 12 | &Tipps | -13 |
  | 13 | &Optionen | -14 |
  | 14 | Ja, &alle | -15 |

  - The mnemonic is the character after "&". OK has none.
  - Esc is bound to Cancel (id 1) and Close (id 11) (0x324A2FAC, 0x324A2FB5).
- **Button sets:** table at 0x324A1340, button order left to right. Indices 0..14 are
  exactly `MsoButtonSetType`:

  | Set | Buttons |
  |---|---|
  | 0 None | (none) |
  | 1 OK | OK |
  | 2 Cancel | Cancel |
  | 3 OkCancel | OK, Cancel |
  | 4 YesNo | Yes, No |
  | 5 YesNoCancel | Yes, No, Cancel |
  | 6 BackClose | Back, Close |
  | 7 NextClose | Next, Close |
  | 8 BackNextClose | Back, Next, Close |
  | 9 RetryCancel | Retry, Cancel |
  | 10 AbortRetryIgnore | Abort, Retry, Ignore |
  | 11 SearchClose | Search, Close |
  | 12 BackNextSnooze | Back, Next, Snooze |
  | 13 TipsOptionsClose | Tips, Options, Close |
  | 14 YesAllNoCancel | Yes, Yes to all, No, Cancel |
  | 15 (internal) | Options, Search |
  | 16 (internal) | Search |
  | 17 (internal) | Options, Close |

  Values from 101 upward mean caller-supplied custom buttons. Their count is value -
  100, and they return -25, -26, ... (0x324A6C7A, 0x324A6D7D).

**Rule R-OB-33 (search field).** **[obs]** throughout.

- **Requested size (0x324A9B71..0x324A9BB3, 0x324A3C0B..0x324A3C35):** 20 characters ×
  2 lines. Width = (20 + 2) × TM.tmAveCharWidth. Height = 2 × TM.tmHeight + 8.
- **Minimum width (0x324A97E9..0x324A982E):** the caption width of the "&Suchen" (Search)
  button plus 6 × tmAveCharWidth, rounded up to a multiple of 5, plus 22.
- **Placement:** when painted, the field spans the full content width at x = 0, with
  the requested height (0x324AABD6).
- **Label:** an optional bold label sits directly above it, followed by 3 px when present
  (0x324AABBD..0x324AABCA).
- **Control:** a RichEdit20W (R-OB-14).

**Rule R-OB-34 (placement order and screen edges).** **[obs]** throughout.

- **Preferred side:** "balloon to the left of the character" when the character's
  horizontal centre is in the right half of the work area, or when there is no
  character. Otherwise "to the right" (0x324A6070..0x324A6091).
- **Candidate order (0x324A6696):**
  1. above, preferred side;
  2. above, other side;
  3. beside, preferred side;
  4. beside, other side.

  For "above", the side flag means right-aligned versus left-aligned (R-OB-17). There
  is never a candidate below the character.
- **Choice (0x324A6640):**
  1. Each candidate is scored by 0x324A809E. If that finds no usable shape, the
     candidate is skipped.
  2. The first candidate with score 0 wins at once (0x324A6673). Otherwise the lowest
     score wins. The comparison is strict, so ties go to the earlier candidate
     (0x324A6677).
- **Free space of a candidate (0x324A85D6).** This is a rectangle between the work-area
  edges and the anchor rectangle (the character), 4 px away from the anchor:
  - "above": from the work-area top down to 4 px above the anchor. Horizontally it runs
    from the anchor's left edge to the work-area right, or from the work-area left to the
    anchor's right edge.
  - "beside": from the anchor top down to the work-area bottom. Horizontally it runs to
    the right of the anchor plus 4 px, or to the left of it minus 4 px.
  - If the host gave an avoid rectangle (balloon field 0x1EC), its intersection with the
    free space is recorded. So is the distance from the free space's edge to that
    intersection. Without one, both are 0.
- **Shapes inside a candidate (0x324A809E):** the loop runs over the candidate first and
  the two R-OB-31 shapes second.
  - A shape wider or taller than the free space is skipped (0x324A8101..0x324A8113).
  - The others are fitted by 0x324A81EA and scored.
  - The candidate is evaluated once, or twice when the avoid rectangle intersects its
    free space (0x324A80C1..0x324A80D2).
- **Score:** the score of a fitted shape (0x324A8481..0x324A84B6) is the area by which
  the balloon, anchored at the character, overlaps the avoid rectangle. It is 0 when
  there is no avoid rectangle, or when the balloon does not reach it. Overflow off the
  screen is never scored: such a shape is skipped.
- **Tie between the shapes (0x324A8147..0x324A8199).** A lower score always wins. On
  equal scores:
  - The squarer body wins when the balloon's preference is 2. The ratio is the integer
    (long side / short side) (0x324A851A), and a further tie goes to the smaller area.
    The preference (balloon field 0x7F8) is set by the setup methods (R-OB-8). It is 2
    for the search balloon (0x324A9BEA), and for a generic balloon with more than one
    label and check-box row together (0x324AB23A..0x324AB267). It is 0 otherwise.
  - The same comparison runs when the same shape index comes round again in the second
    pass.
  - Otherwise the new shape wins only if its index equals the preference. With
    preference 0 the landscape shape, tried first, stays.
- **Fitting a shape (0x324A81EA).** This is only partly decoded.
  - It lays the balloon out at the shape's size through the layout method (vtable slot
    23, offset 0x5C; generic 0x324AB7A2) **[obs]**.
  - The layout method returns one of four statuses **[obs]**:

    | Status | Meaning | Where |
    |---|---|---|
    | 0 | fits; reports the height + 6 | 0x324ABB57..0x324ABB63 |
    | 1 | too narrow | see the list below |
    | 2 or 3 | too short | 0x324ABB48..0x324ABB53 |

    The layout method answers "too narrow" when:
    - the icon and the 37 px heading indent leave no room (0x324AB7ED..0x324AB7FD);
    - an unbreakable piece (word or picture) of a text block is wider than its line box
      (0x324A8BB7, failing through 0x324A8CE1 and 0x324AB84A);
    - a label or check-box row fails the same way (0x324AB99C, 0x324ABA25);
    - the button row + 12 is wider than the balloon (0x324ABB13..0x324ABB36).
  - On "too narrow" the fit widens by 10 px, clamped to the free space, and fails once
    the free space is used up (0x324A8414..0x324A843E) **[obs]**.
  - On "too short" it grows the width or the height in 10 px steps within the free space
    **[inf]**: the branch order and the ratio limits 6 and 2 (constants at 0x324A15F8
    and 0x324A15FC) are not decoded.
  - Once the layout fits, it narrows the width by binary search, keeping the layout
    fitting (0x324A846A..0x324A847C, 0x324A84CC). It returns that width and the laid-out
    height **[obs]**.
- **Fallback:** if no candidate fits, a fallback (0x324A6354) measures the free space on
  all four sides of the character. The character is then moved so the balloon fits
  (R-OB-17), and the chosen flags are stored (0x324A60F2..0x324A60FD).

**Rule R-OB-35 (vertical spacing, generic balloon, y from the content top).**
Function 0x324ABC0A **[obs]**:

1. **Heading block.** The icon (32 px) and the heading share a block of height
   max(32, heading height). They are bottom-aligned: the shorter of the two is pushed
   down (0x324ABD06..0x324ABD4C).
2. **Body text.** G, then the body text.
3. **Labels.** G before every label row, including the first.
4. **Check boxes.** G before every check-box row, including the first.
5. **Before the buttons** (if there are buttons or a footer):
   - G;
   - an optional 1 px separator line, colour #B2B2B2, across the full content width;
   - G again.
6. **Footer.** Optional footer text, then one TM.tmHeight.
7. **Buttons.** The button row (R-OB-32).

Notes:

- G is only added when something precedes the block (flag in local -8).
- When a label list follows nothing, 1 px is added instead (0x324ABE14).
- The search balloon (0x324AA816) uses the same G spacing, inserting its "see previous"
  row, result labels, "see also" row and the edit field (R-OB-33) before the buttons.

**Rule R-OB-36 (strip drawing and control states, fifth pass).** **[obs]** unless
tagged otherwise.

- **Strip objects.** The loader (0x324AC64E, 0x324AC6D2) is given:
  - a resource id and a frame count;
  - flags 0x300, where 0x200 means "frames stacked vertically";
  - optionally a fixed frame size.

  It takes RT_BITMAP data unchanged (0x324AC358..0x324AC3FC); the bitmaps are bottom-up
  DIBs. Without a fixed size the frame height is the bitmap height / count (0x324AC627..
  0x324AC643).
- **Frame index to rows.** The draw helper (0x324AC89A) sets the source y to frame
  height x index for vertical strips (0x324AC8C2..0x324AC8CE). It passes that y to
  StretchDIBits (0x324AC9A1) or SetDIBitsToDevice. For a bottom-up DIB that y counts from
  the bottom row, so strip index i is frame n - 1 - i counted from the top.
  - This direction is confirmed by the raster operations. The AND blit (0x8800C6) uses
    button indices 5..9 and bullet index 4. Those are exactly the black-and-white mask
    frames at the top of the bitmaps. The other direction would AND with colour images.
- **Push button routine (0x324A57CC).** It is called with a start state, an end state and
  a focus flag.
  - Per state s, it ANDs mask s + 5 and ORs image s for the left cap, the middle tiles
    and the right cap (0x324A5A7D..0x324A5B52).
  - If the start state differs from the end state, it steps to the end state, redrawing
    every 25 ms (0x324A5BD2..0x324A5BE8).
  - A disabled window (IsWindowEnabled, 0x324A59E4) forces state 4 for both.
- **Bullet routine (0x324A558E).** It is used for label bullets and the arrows. It ANDs
  mask index 4 and ORs image s, stepping every 50 ms (0x324A5635..0x324A574C).
- **Who asks for which state:**

  | Event | State | Where |
  |---|---|---|
  | WM_DRAWITEM with ODA_SELECT and ODS_SELECTED | 2, then 3 | 0x324A4FE8..0x324A5007 |
  | any other WM_DRAWITEM | 0 | 0x324A5009 |
  | WM_MOUSEMOVE inside the control, no mouse button held | 1 (pointer captured) | 0x324A770B, 0x324A77AB..0x324A77E0 |
  | WM_MOUSEMOVE outside | 0 (capture released) | 0x324A77E2..0x324A7801 |
  | disabled button | 4 | 0x324A59EE |

  - Mouse-button state is tested with MK_LBUTTON, MK_RBUTTON and MK_MBUTTON (mask 0x13).
  - Labels (control ids 1..999) take the bullet routine with strip 101 or 102 (R-OB-8).
    Ids -17 and -16 take the arrows. Ids of 0 and below are push buttons.
  - The focus flag comes from ODS_FOCUS (0x324A515C), or from GetFocus during hover.
  - itemState is tested only for ODS_SELECTED and ODS_FOCUS. ODS_DEFAULT and
    ODS_DISABLED are never tested.

The earlier first-pass notes still apply. The Choose Assistant preview in MSO
(0x3108A9D9) is the only observed frame-like geometry **[obs]**:

- #FFFFCC fill and a 1 px black frame;
- 10 x 10 corner ellipses;
- a triangular tail 16 px deep and 10 px tall, starting 40 px below the top;
- a 10 px text inset.

It is a preview, not proof of the real frame. MSO.DLL's 12 x 12 check box
bitmaps (RT_BITMAP 1141/1142) are not used by the balloon, since it uses native check
boxes **[inf]**. The search box sends its query to the Answer Wizard engine (`aw.dll`,
Gimme token 0x63, `.aw` index files) and opens HTML Help topics (`ofmain10.chm`)
**[obs names, inf flow]**.

## 3. Tips (light bulb) and the letter prompt

### Tip machinery in MSO.DLL

- Window class `MSOTIPBULB`, caption "Tip Bulb" (registered at 0x311D8168, created at
  0x311D78ED). It is a borderless popup with a shaped region.
  - The outline is a 44-point polygon built from 22 stored half points (0x313717E8),
    mirrored as x' = 18 - x (axis at x = 9). Overall about 18 x 27 px.
  - The image is MSO resource type 998, id 520. Its header says 18 x 27, in MSO's own
    compressed bitmap format, so I did not render it.
- When a tip becomes available (0x30F1D7C5..0x30F1D88D), one branch only shows the bulb
  window, the other places the bulb next to the character and plays `Wave` [obs]. That
  the split is "Assistant hidden" versus "Assistant visible" is [inf].
- Bulb-module help routine (0x311D7F24): it resolves Gimme token 0x34 = **`oftip10.hlp`**
  and asks the host to show a WinHelp popup at the bulb position, with commands 0x0D
  (HELP_SETPOPUP_POS) and 0x08 (HELP_CONTEXTPOPUP) [obs]. Its only direct caller is the
  bulb window procedure, on a mouse message, with the constant context ID 0x122
  (0x311D8047) [obs]. So clicking the bulb shows one fixed popup topic, not the tip
  itself [inf].
  - Context number 290 (0x122) is **not** in OFTIP10.HLP's context map [obs, second
    pass]. Either the host redirects the call or that topic is missing [unk].
  - OFTIP10.HLP is confirmed to be the "What's This?" popup file for Office shared
    dialogs (see "The *TIP10.HLP files" below) [obs].
- The bulb module builds a balloon string of the form `{CBMP 1 <number>}` followed by
  the localized string with the constant ID 0x50019, loaded through MSO's string loader
  (0x30B2BF99, the same loader as the menu captions) [obs, 0x311D7A47 and 0x311D7ABB].
  - In MSOINTL that ID is "Klicken Sie hier, um es auszuprobieren." ("Click here to try
    it") [obs, second pass].
  - So the bulb's own balloon line is a fixed "try it" link with the bulb bitmap in
    front, not a tip text.
  - Where the actual tip sentences come from is **[unk]**: host apps pass them in, from
    their own resources [inf].
- `Assistant.ResetTips` (0x3108D1C6 to 0x311D7EC9):
  - optionally shows a confirmation (string 0x50035, German "Sie haben gerade Ihre Tipps
    zurückgesetzt. ...");
  - asks every registered host app to reset its own tip state (linked list at
    0x3135B7F8);
  - clears several MSO preference IDs.

  Tips are therefore owned per app, and MSO only coordinates them **[obs]**.
- Options that gate tips (registry `HKCU\Software\Microsoft\Office\Common\Assistant`),
  with the defaults from MSO's preference table at 0x30BDE248 **[obs]**:

  | Value | Default |
  |---|---|
  | AsstFeatureTips | 1 |
  | AsstMouseTips | 1 |
  | AsstKeyboardShortcutTips | 0 |
  | AsstOnlyHighPriorityTips | 0 |
  | AsstShowTipOfDay | 0 |
  | AsstTipTimeout | 20 (unit [unk]) |

### Where tip detection lives

- MSO has no text heuristics for tips. The "you could do X faster" decisions are made
  by each host app, which then calls TFC **[inf]** from the per-app reset design and
  the absence of trigger code in MSO.
- The letter prompt: a bounded negative search of MSO.DLL's ASCII and UTF-16 strings
  for "Dear", "letter" and "To whom" found only command-bar names, no salutation
  patterns **[obs]**. The detection is in Word (WINWORD.EXE plus its intl DLL).
  Microsoft documents it as an AutoFormat-as-you-type option that starts the Letter
  Wizard when the user types a salutation or closing (`AutoFormatAsYouTypeAutoLetterWizard`,
  `Document.RunLetterWizard`) **[pub]**:
  https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2003/aa212594(v=office.11) ,
  https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2010/ff821600(v=office.14)

### Where tip texts are stored

- **Not in MSO.DLL.** It has no RT_STRING, RT_DIALOG or RT_MENU resources at all
  **[obs]**.
- **Not in the *TIP10.HLP files either** (second pass; see the next subsection). Despite
  their names, those are "What's This?" control-help files for dialogs.
- **Not in MSOINTL.DLL as a tip list.** Its Assistant string group has only framework
  messages, for example "Leider habe ich gerade keine Tipps für Sie." ("Sorry, I have no
  tips for you right now") **[obs]**.
- Therefore the actual light-bulb tip sentences live in the host applications
  **[inf]**: `WINWORD.EXE` / `WWINTL.DLL`, `EXCEL.EXE` / `XLINTL32.DLL`,
  `POWERPNT.EXE` / `PPINTL.DLL`, `OUTLOOK.EXE` / `OUTLLIB.DLL` and its intl DLL. Their
  trigger logic also lives there.

### The *TIP10.HLP files (OFTIP10, OLTIP10, MSTIP10) [obs]

Structure, parsed with my own reader:

- **Format:** WinHelp 4 files (magic 0x00035F3F, `|SYSTEM` minor version 33, LCID
  0x0407 German), with LZ77-compressed 4 KB topic blocks and Hall phrase compression
  (`|PhrIndex` plus `|PhrImage`).
- **Internal files, all three:** `|CONTEXT`, `|CTXOMAP`, `|FONT`, `|PhrImage`,
  `|PhrIndex`, `|SYSTEM`, `|TOPIC` and `|TTLBTREE`. There is no keyword index
  (`|KWBTREE`). Every topic title in `|TTLBTREE` is empty.
- Sizes:

  | File | Topics | Context-map entries | Distinct target topics | Owner |
  |---|---|---|---|---|
  | OFTIP10.HLP | 3,732 | 6,374 | 3,278 | Office shared dialogs |
  | OLTIP10.HLP | 4,508 | 6,383 | 2,617 | Outlook dialogs |
  | MSTIP10.HLP | 106 | 31 | 31 | Clip Organizer (MStore) dialogs |

- **Content:** each topic is one short paragraph describing one dialog control. Topic
  text averages about 100 characters. Two identification examples:
  - MSTIP10: "Geben Sie einen Namen für die Sammlung ein."
  - OFTIP10: "Zeigt Tipps zur schnellen Erledigung Ihrer Arbeit mithilfe der Maus an.",
    By its wording this is the What's-This text of the "Maus effektiver nutzen" check
    box in the Assistant options page [inf].
- **Keying:** topics are selected only by numeric context number through `|CTXOMAP`
  (HELP_CONTEXTPOPUP).
  - Most numbers are large composite values. Their 16-bit halves look like a dialog or
    owner id and a control id, for example 0x138F8406.
  - About 100 small numbers exist (1, 2, 100..300, 4000..4004). OFTIP10 also contains
    40001, which is exactly the help context ID MSO gives the Agent character
    (`SetHelpContextID(40001)`, section 1). So the character's own Help-mode click
    probably lands in OFTIP10.HLP [inf]; the `SetHelpFileName` argument was not
    traced.
- **Conclusion:** there is no tip-of-the-day or light-bulb tip collection in these files.
  They are not needed for bentwire beyond, optionally, What's-This texts for an options
  dialog.

## 4. Options dialog, right-click menu, gallery

### Right-click menu [obs]

- Office turns off Agent's automatic popup menu and adds three commands of its own with
  `IAgentCommandsEx::AddEx` (0x30C76726):
  - captions are MSOINTL string IDs 0x60015, 0x60016 and 0x60017;
  - MSO action codes 1001, 1002 and 1003 (also packed into each command's help context
    ID; the Agent command IDs are whatever `AddEx` returns, stored at 0x31360380);
  - the menu font is set to Office's UI font.
- On right click MSO enables the commands and calls `ShowPopupMenu`. The visible menu
  also has a Hide item (KB 290115:
  https://ftp.zx.net.nz/pub/archive/ftp.microsoft.com/MISC/KB/en-us/290/115.HTM). MSO
  does not add it, so Agent supplies it [inf].
- The `Command` sink maps the Agent ID back to the action code and forwards it to MSO's
  window. The handler at 0x31284C00 does:

  | ID | Action |
  |---|---|
  | 1001 | opens the Assistant dialog with argument 1 |
  | 1002 | opens the Assistant dialog with argument 0 |
  | 1003 | "Animate!" (plays the next shuffled animation). Only added when a condition holds (0x30FB092F) |

- **Confirmed with MSOINTL.DLL (German) [obs]:** MSO's string IDs map to MSOINTL
  resource type 217, group = (ID >> 16) + 1, index = ID & 0xFFFF. With that mapping:

  | Item | String ID | German caption |
  |---|---|---|
  | 1001 | 0x60015 | "&Optionen..." |
  | 1002 | 0x60016 | "Assistenten &wählen..." |
  | 1003 | 0x60017 | "A&nimation!" |

  The dialog's two tabs are 0x60000 "&Katalog" (page 0) and 0x60001 "&Optionen"
  (page 1). So 1001 opens the Options tab (argument 1) and 1002 the Gallery tab
  (argument 0), which confirms the earlier inference.
- Other menu-related strings in the same group: "Office-Assistenten &aktivieren" /
  "&deaktivieren", "Office-&Assistenten anzeigen" / "ausblenden", and the dialog title
  "Office-Assistent" **[obs]**. Public guides list the same menu items for the English
  UI **[pub]** (https://bettersolutions.com/word/documents/office-assistant.htm).

### Options page (and gallery page), from MSOINTL.DLL [obs]

The Assistant dialog is one MSO SDM dialog template, MSOINTL resource type 221, id 12,
1,708 bytes. It contains both pages. Item records are 8 bytes: type, text offset
relative to the record, then x, y, width and height. The units are probably dialog units
[inf].

Gallery page ("Katalog"):

| Item | Position (x, y, w, h) |
|---|---|
| Explanatory text "Sie können sich die verschiedenen Assistenten ansehen, ..." | (11, 25) |
| Preview area | not decoded |
| "Name:" label | (145, 97) |
| Name value | (187, 97, 105, 9) |
| Button "<&Zurück" | (23, 142, 50, 14) |
| Button "&Weiter>" | (76, 142, 50, 14) |

Options page ("Optionen"). Check boxes are 10 du tall, rows 13 du apart, in two columns
at x = 31 and x = 187:

| German caption (English meaning) | Position | Registry value |
|---|---|---|
| "&Den Office-Assistenten verwenden" (use the Office Assistant) | (12, 26) | `On` / AsstState |
| "Auf &F1-TASTE reagieren" (respond to F1) | (31, 39) | AsstAssistWithHelp |
| "&Hilfe zu Assistenten" (help with wizards) | (31, 52) | AsstAssistWithWizards |
| "Warn&meldungen anzeigen" (display alerts) | (31, 65) | AsstAssistWithAlerts |
| "Sowohl &Produkt- als auch Programmierhilfe während der Programmierung durchsuchen" | (31, 78) | AsstSearchInProgram |
| "&Verschieben, wenn im Weg" (move when in the way) | (187, 39) | AsstMoveWhenInTheWay |
| "Hilfethemen er&raten" (guess help topics) | (187, 52) | AsstGuessHelp |
| "&Sounds aktivieren" (make sounds) | (187, 65) | AsstSounds |
| Group label "Tipps anzeigen" (show tips about) | (23, 96) | n/a |
| "&Leistungsmerkmale effektiver nutzen" (using features more effectively) | (31, 108) | AsstFeatureTips |
| "M&aus effektiver nutzen" (using the mouse more effectively) | (31, 121) | AsstMouseTips |
| "&Tastenkombinationen" (keyboard shortcuts) | (31, 134) | AsstKeyboardShortcutTips |
| "N&ur Tipps mit hoher Priorität anzeigen" (only show high priority tips) | (187, 108) | AsstOnlyHighPriorityTips |
| "T&ipps und Tricks beim Start anzeigen" (show the tip of the day at startup) | (187, 121) | AsstShowTipOfDay |
| Button "Meine Tipps &zurücksetzen" (reset my tips) | (187, 134, 94, 14) | calls ResetTips |

The control-to-registry mapping is **[inf]** from captions and the preference table.
The default values from MSO's preference table (first pass) still apply:

| Value | Default |
|---|---|
| AsstAssistWithHelp | 1 |
| AsstAssistWithWizards | default [unk] |
| AsstAssistWithAlerts | default [unk] |
| AsstMoveWhenInTheWay | 1 |
| AsstGuessHelp | 1 |
| AsstSounds | 1 |
| AsstSearchInProgram | 0 |
| AsstOfficeWizards | 1 |
| AsstFeatureTips | 1 |
| AsstMouseTips | 1 |
| AsstKeyboardShortcutTips | 0 |
| AsstOnlyHighPriorityTips | 0 |
| AsstShowTipOfDay | 0 |
| AsstTipTimeout | 20 (unit [unk]) |
| AsstWizardTime | default [unk] |
| AsstFile | current .acs |
| AsstLeft / AsstTop | -12345 = not set |
| AsstHiddenCount | 3 |

AsstHiddenCount = 3 matches the MSOINTL message "Sie haben mich mehrere Male
ausgeblendet. Möchten Sie mich endgültig ausschalten oder lediglich erneut
ausblenden?" (0x5000D in MSO numbering), with the answers "Nein, nur ausblenden",
"Ja, ausschalten" and "Weitere Optionen ändern" **[obs strings, inf link]**.

Other Assistant UI strings in the same MSOINTL group, useful for a bentwire German
locale **[obs]**:

- the first-run greeting "Hallo, ich bin Ihr Office-Assistent.";
- the search-result footer rows "Vollständige Liste der Hilfethemen anzeigen" and
  "Keine der oben stehenden Auswahlmöglichkeiten. Nach weiterer Hilfe im Web suchen";
- the balloon button captions, a duplicate of oballoon's (R-OB-11);
- the alert prompt "Möchten Sie Hilfe zu diesem Feature?".

### Choose Assistant gallery [obs]

The gallery is implemented in MSO (code around 0x3108A000..0x3108C200).

- It lists `*.acs` from `<Windows>\msagent\chars\`, special-casing `genius.acs` and a
  legacy `wizard.acs` name. Default characters are `clippit.acs` (with `rocky.acs` as
  the other named default in the install table).
- Back/Next buttons step through the list (control IDs 0x14 and 0x15 are
  enabled/disabled at the ends).
- The preview does not use the Agent server. MSO loads `msagent\AgentAnm.dll` directly
  and calls its export `CreateAgentAnimW` to render the character inside the dialog,
  playing `Wave` and then the state `IdlingLevel1`.
- Name and description are shown in a balloon-like box (see the colour notes in
  section 2).
- `Saeko.acs` / `Saeko.acg` are special-cased (a Japanese character; `.acg` purpose
  [unk]).

## 5. Effort estimate for a Linux reproduction (bentwire)

| Behavior | Needs | Feasibility as a bentwire client | Effort |
|---|---|---|---|
| Animation vocabulary (enum to name) | the table in section 1, fallback map, case-insensitive lookup | easy, pure data | S |
| Show (animated, or fast plus Greeting/RestPose), Hide (Goodbye plus fast hide, or animated hide), Look* toward a point, Animate! shuffle, Wave on tip | small state machine | easy | S |
| Office-style balloon for `ask` (heading, text, icon, up to 5 labels as buttons/bullets/numbers, up to 5 checkboxes, button sets, modal/autodown/modeless, return codes) | own renderer on Wayland (layer-shell surface next to the character), markup subset `{cf}`, `{ul}`, `{bmp}` | feasible. The return-code contract is fully known from the typelib | M |
| Balloon content close to Office (R-OB-1..19: Tahoma 13/16 px, #FFFFCC, 6 px inset, 16 px tail strip, 37 px heading indent, outline buttons 22/24 px, glass-ball bullets, native-style check boxes, placement candidates) | own artwork with the same sizes and states (do not ship Microsoft bitmaps); text layout with Pango | feasible now; sizing, button row, search field, placement order and spacing are in R-OB-31..35, control states in R-OB-36. Only the shape fit inside a candidate is inferred | M |
| Balloon frame (R-OB-20..30: 8 px corner radius, 1 px black outline following body and tail, #FFFFCC, 10 x 16 px right-triangle tail at 70 % / 30 %, no shadow, topmost tool window) | a shaped layer-shell surface (or a client-side shape mask) with the same geometry | feasible now | S |
| Search box ("What would you like to do?") | own search backend (desktop help, man pages, web), not Answer Wizard | feasible, but different content | M |
| Tip bulb + tip of the day | tip source (own text), timeout, per-app "already shown" state | feasible. Microsoft tip texts are not ours to ship | S to M |
| Tip triggers (feature, mouse, keyboard-shortcut tips) | per-application hooks. On Linux this means app plugins or accessibility events (AT-SPI) | only per integrating app, no general solution | L |
| "It looks like you're writing a letter" | a text-watching client in an editor (LibreOffice extension, editor plugin) matching salutations | feasible as a demo client, not generic | M |
| Options dialog | settings UI (GTK) writing bentwire config, same switches | easy | S |
| Right-click menu (Hide, Options, Choose Assistant, Animate!) | bentwire's own menu | easy | S |
| Choose Assistant gallery with live preview | enumerate .acs, render preview with bentwire's own decoder, Back/Next, name and description | easy once the ACS renderer exists | S to M |
| Japanese Congratulate swap, RTL balloon mirroring | flags | trivial | S |

Overall: everything that is presentation and state (animations, balloon contract, menu,
options, gallery, bulb) is reproducible as a bentwire client without Microsoft code.
What cannot be reproduced generically is the trigger intelligence, which lived inside
each Office app. That has to be rebuilt per integration. The balloon frame is now fully
specified (R-OB-20..30), and most content metrics are known (R-OB-1..19).  The layout rules
R-OB-31..36 close the remaining layout items, apart from the shape fit inside a candidate.

## Unknowns

- Closed by BLNMGR.DLL: frame window class (`MSOBALLOON`), outline (1 px black),
  corner radius (8 px), tail geometry and side selection, shadow (none), and how
  frame, content and Agent cooperate.
- Closed by the fourth OBALLOON pass:
  - balloon size and wrapping (R-OB-31, area based, no fixed maximum width);
  - button row (R-OB-32);
  - search field (R-OB-33);
  - placement order (R-OB-34);
  - vertical spacing (R-OB-35).
- Closed by the fifth OBALLOON pass:
  - the state order of the button, bullet and arrow strips (R-OB-8, R-OB-9, R-OB-11,
    R-OB-36);
  - the area terms, minimum size and growth constants (R-OB-31);
  - the candidate score and the tie rule between the two shapes (R-OB-34).
- Still open in OBALLOON.DLL:
  - how 0x324A81EA fits a shape into a candidate's free space: the growth order, the
    ratio limits 6 and 2, and the binary search that narrows the width (R-OB-34);
  - the search balloon's own size method (0x324A9D66), not re-read in the fifth pass;
  - where the avoid rectangle (balloon field 0x1EC) comes from;
  - the exact amount the character is moved when nothing fits;
  - the footer-text and separator flags: which balloons set them, and whether the
    block measured after the check boxes in the size method is that footer;
  - which host requests set the flag that selects the bulb bullet 102 (R-OB-8);
  - what the owner-drawn items with ids from 1000 up are (R-OB-10);
  - the meaning of the flag 0x324B3694 that keeps "&" markers in button captions
    (R-OB-11).
- Whether any client changes BLNMGR's default margins, gap or colour (R-OB-20).
- The exact purpose of BLNMGR's 250 ms and 20 s timers.
- The meaning of the undocumented markup keywords `pict` and `dlx`.
- Which host-app events request which animation index (print, save, mail, trash,
  spelling, wizards). These are in the app executables, still missing.
- The triggers of the MSO-internal `Congratulate`, `Writing`, `Processing`,
  `GetWizardy`, `Explain` and `Thinking` sites, and of the `Look*` helper.
- The units of AsstTipTimeout and AsstWizardTime.
- Where the light-bulb tip sentences live. They are not in MSO.DLL, MSOINTL.DLL or the
  *TIP10.HLP files, so presumably in the host apps. Also unknown: why bulb context
  0x122 is missing from OFTIP10.HLP.
- Whether the tip bulb image (MSO resource 998/520) and the `{CBMP}` bitmaps share one
  source.
- The qmark.acs name scan result (no names found) needs confirmation with a real ACS
  parser.

## Other Office XP files needed

- Provided and analysed: `OBALLOON.DLL`, `BLNMGR.DLL`, `MSOINTL.DLL` (German),
  `OFTIP10.HLP`, `OLTIP10.HLP`, `MSTIP10.HLP`.
- Still useful:
  - One or two reference screenshots of an Office XP Assistant balloon (search balloon
    and a tip balloon), to confirm the frame.
  - `WINWORD.EXE` + `WWINTL.DLL`: letter detection, Word's event-to-animation calls and
    Word's tip texts. The same applies to `EXCEL.EXE`/`XLINTL32.DLL`,
    `POWERPNT.EXE`/`PPINTL.DLL` and `OUTLOOK.EXE`/`OUTLLIB.DLL` if their behavior
    matters.
  - An English (1033) `MSOINTL.DLL`, only if English captions are wanted verbatim.
  - `aw.dll` and the `.aw` index files, only if the search box behavior is to be studied.

## Sources

- Office XP: Using Balloon Controls,
  https://learn.microsoft.com/en-us/previous-versions/office/developer/office-xp/aa141368(v=office.10)
- Balloon.Text property,
  https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2010/ms248014(v=office.14)
- Adding Icons and Bitmaps to Balloons, https://techshelps.github.io/VBA/ofun/ch12e.htm
- AutoFormatAsYouTypeAutoLetterWizard,
  https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2003/aa212594(v=office.11)
- Document.RunLetterWizard,
  https://learn.microsoft.com/en-us/previous-versions/office/developer/office-2010/ff821600(v=office.14)
- KB 290115, Control How and When the Office Assistant Appears (Office XP),
  https://ftp.zx.net.nz/pub/archive/ftp.microsoft.com/MISC/KB/en-us/290/115.HTM
- Better Solutions, Office Assistant menu,
  https://bettersolutions.com/word/documents/office-assistant.htm
