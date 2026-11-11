# Office 97 Office Assistant format (.ACT / .ACP) and its runtime

Research note for bentwire. Author: research subagent, 2026-11-11. Status: complete for the
file format, partial for Office-level behavior (see "Unknowns").

## 1. Summary

- An `.ACT` file (and the smaller `.ACP` gallery preview) is a tiny resource pack with magic
  `LP` holding exactly one "actor" resource. Office 97 uses pack version 2.2; Microsoft Bob
  actors use version 1.0. Same code path in MSO97.DLL reads both.
- The resource is a 70-byte header, an optional color table plus pose-transition table
  (v2 only), and four parallel sections: **payloads** (images and composite poses),
  **sounds** (complete RIFF WAVE files, embedded), **animation groups**, **string groups**.
  All offsets are relative to the start of the resource. Every byte of all 55 sample files
  is accounted for (0 unexplained bytes).
- Images are either **Windows metafiles** (vector, with an Aldus placeable header; Clippit,
  Dot, Hoverbot, Logo, Power Pup, Scribble, Will, Kairu) or **8-bit bitmaps** (Genius,
  Mother Nature, Earl, Rocky, F1, Saeko-sensei). Bitmaps are packed 1, 2 or 4 per blob
  (`DCIK` / `MNAK`), compressed with the same bit-level LZ77 scheme as Agent ACS images,
  then run-length coded. Bitmap pixels index a fixed 256-color "Office palette" that is
  **not stored in the .ACT** (it lives in MSO97.DLL, and identically in all 7 Office ACS
  files checked).
  Palette index 10 is the transparent color.
- A **pose** is a composite: a list of (image, rectangle) pairs in a logical coordinate
  space (usually 2880 x 2160 twips = 192 x 144 px at 96 dpi). Animations reference poses.
- An **animation** is a flat script of 6-byte commands: show pose for N ms, play sound,
  random branch (probability out of 32768), time-of-day branch, host-condition branch.
  A branch with probability 0 is an **exit branch**: it is only followed while the runtime
  fast-forwards an interrupted animation. This is the ACT form of Agent's exit branching.
- Animations are addressed by numeric group ID with random variants. In Office 97 files the
  IDs are exactly the public `MsoAnimationType` values (1 Idle ... 116 EmptyTrash), plus a
  few private IDs (119 to 123) and 10000+ transition clips. A complete ID to Agent
  animation-name table, taken from Office XP's MSO.DLL, is in section 6.
- Runtime rules recovered from MSO97.DLL: frames are scheduled on an absolute due time
  (lateness is dropped, frames that fit in the next 14 ms are coalesced, the timer is at
  least 14 ms); zero-duration frames are never drawn; interrupted animations play out
  with frames capped at 100 ms, forward branches taken and backward branches skipped; after
  any animation ends while the character is visible the engine replays animation group
  `header+0x06` (= 1, Idle) with a fresh random variant; between animations it inserts a
  transition clip when the last pose and the next first pose differ.
- Confidence: **high** for the byte layout, decompression, compositing, timing and branch
  semantics (decoded every file, rendered frames, matched against disassembly). **Medium**
  for Office-level triggers. Open: meaning of a few header words, private IDs 119 to 123.

## 2. Sources

Nothing was executed. The files are not in this repository.

### 2.1 Office 97 Professional CD

Files from the Microsoft Office 97 Professional CD (English), identified by their path on
the CD:

| ISO path | Bytes | SHA-256 |
|---|---|---|
| OFFICE/MSO97.DLL (FileVersion 8.0.3507, image base 0x306C0000, PE timestamp 847346555 = 1996-11-07 UTC) | 3,774,224 | `4d382905037551ef6b830a28b5b70b55043332c064665da30bae32095da1722d` |
| OFFICE/MSO7ENU.DLL (resources, not analysed further) | 487,184 | `54a06d45ee45e7479e4cd7c3724f79f6393ac467e039ee003661ded72320e4e9` |
| OFFICE/ACTORS/CLIPPIT.ACT | 468,266 | `47026de14f910ac2c51e6e509a043ac8e43fd24e0de8266e8feeeadbf65cac4b` |
| OFFICE/ACTORS/DOT.ACT | 441,828 | `d9e450149b2c6ea37be33da70d165aff624f2721239b3c605a073585c465f15c` |
| OFFICE/ACTORS/GENIUS.ACT | 1,899,513 | `fff92968d040d0f1b5567624bf6faa0eeb09e2e11d48ea801e3b9c73d5e8ae4f` |
| OFFICE/ACTORS/HOVERBOT.ACT | 455,427 | `4f8916e24e5213d092c5c845f0056d31d7a0cc112f326ef7f2e6c28d9595ff24` |
| OFFICE/ACTORS/LOGO.ACT | 356,778 | `37b09750ba6163b167aa273227902b2f6c27a721ca367a77d01620cfa79d8a4e` |
| OFFICE/ACTORS/MNATURE.ACT | 1,916,989 | `131b407d77e5116b5a72155a80aa3b7a53779a5871beb3e2d65a8ef5271bb2db` |
| OFFICE/ACTORS/POWERPUP.ACT | 479,272 | `26701407f7599c3e0c600a138fadc8551a0de33bc76d42c53cef7f5aca74030d` |
| OFFICE/ACTORS/SCRIBBLE.ACT | 377,183 | `d1d3e47ef76eeaac8c481b0a8fac89410a851908cbfc4b9f7719ac888c378b44` |
| OFFICE/ACTORS/WILL.ACT | 399,965 | `00e7169818797384dd1bfc67773396bcf5559eab93c5af27072fae56a94af22d` |
| OFFICE/ACTORS/PREVIEW/CLIPPIT.ACP | 36,540 | `8a77f2d758447f479c289e49f5f12a0e29e0c28ea533b4afe8d178bbd9116369` |
| OFFICE/ACTORS/PREVIEW/DOT.ACP | 10,932 | `e7db59b47059c7f3a7b8d40a1bef0af437927dd4b95606ac4ebb770d95035ae3` |
| OFFICE/ACTORS/PREVIEW/GENIUS.ACP | 81,930 | `d7e424c49c0fba129d4124884067ebda6605506cdefffbc781def314b6f762d0` |
| OFFICE/ACTORS/PREVIEW/HOVERBOT.ACP | 9,387 | `7ec150df35fd9b8f2ba1c7c30db3c4f7bbd5b5dd4e23fd39804172f76467b77d` |
| OFFICE/ACTORS/PREVIEW/LOGO.ACP | 3,200 | `2041881e60c57e9e8d7c37c10c51c42ad74bd446daf044288472e93558784e7f` |
| OFFICE/ACTORS/PREVIEW/MNATURE.ACP | 87,942 | `d6efecb630c406d6d2190128f9024c18a5970c5567319300c0df3b4f09f4e37e` |
| OFFICE/ACTORS/PREVIEW/POWERPUP.ACP | 23,514 | `76140e487372e23ad84a92adb9dc3cb4fe4a09b3d91f645c9ed265d477882d65` |
| OFFICE/ACTORS/PREVIEW/SCRIBBLE.ACP | 38,225 | `4f66523d35f55e02f76d6ae79fec10e5c6461a6aa0381a3c508286263c7b31b0` |
| OFFICE/ACTORS/PREVIEW/WILL.ACP | 41,133 | `b28aa4f2f2c984d6c24ce17d55c473bd73ba2471018b024e6036e39613044aed` |

The CD ships 9 actors (Clippit, The Dot, The Genius, Hoverbot, Office Logo, Mother Nature,
Power Pup, Scribble, Will). There is no Value Pack actor folder on this CD.

### 2.2 Actor collection

A public archive copy of a collection of 37 Office 97 and Microsoft Bob actor files (.ACT).
The 9 Office 97 actors are byte-identical to the CD files above. Additional files used for
verification:

Origins (inferred from content): DOLPHIN.act = Kairu (Office 97 Japanese), OFFLADY.act =
Saeko-sensei (Office 97 Japanese, item name in cp932), EARL.act (Office 97 web download),
ROBOT.act = F1, ROCKY(Office).act, TOFFLOGO.act (Office 97 add-ons or variants);
ROCKY(Greetings).ACT, ROCKY(EarlyGreetings).ACT (Greetings Workshop), TUTOR.ACT (My
Personal Tutor); 17 Microsoft Bob actors plus ZSPEAKER and ZVISIBLE (format v1.0).

| File | Bytes | SHA-256 |
|---|---|---|
| BAUD.ACT | 484,100 | `08a36be2f88dab26bb6b20d78e114f38fab0ce2777b3c136128a81bc59e0f9cb` |
| BLYTHE.ACT | 356,425 | `e881d4d1c7ae1113a85cb018788012e8e63294f4e7d8c996b359cf3686508a82` |
| CHAOS.ACT | 291,970 | `8836f8bbb7f36cee9e8ce1bba24b653e8efddbb1d0f06515c2cd3d87705a0d2b` |
| CHAZ.ACT | 541,158 | `91814700d2d619ceda221e6a101f99439b01a27cbaf477e263efa7c6da14d0c7` |
| CLIPPIT.ACT | 468,266 | `47026de14f910ac2c51e6e509a043ac8e43fd24e0de8266e8feeeadbf65cac4b` |
| DOLPHIN.act | 890,749 | `3c0594c1fcd2b74ffd5f5d50cdec1b7271fcf6a63e46423916d84cad2b3c1a15` |
| DOT(Bob).ACT | 531,015 | `4f647cecee8ebccd000429b825612f2436e2f42000661427b80c5cd8b281fa47` |
| DOT(Office).act | 441,828 | `d9e450149b2c6ea37be33da70d165aff624f2721239b3c605a073585c465f15c` |
| EARL.act | 2,409,895 | `f3109a7e78f7c3af68b3320b8aced3c58daaaa5465af2779b9e83919baebf19c` |
| GENIUS.act | 1,899,513 | `fff92968d040d0f1b5567624bf6faa0eeb09e2e11d48ea801e3b9c73d5e8ae4f` |
| HANK.ACT | 304,572 | `254fc752165496eeeadf3555333aeb94e4892026aad40bcdaa963e11430a109c` |
| HOPPER.ACT | 243,047 | `66d23ef84e1dc6e26044b53e6e7c40961ed26f565d1504c26e2e92c44cab858c` |
| HOVERBOT.act | 455,427 | `4f8916e24e5213d092c5c845f0056d31d7a0cc112f326ef7f2e6c28d9595ff24` |
| JAVA.ACT | 370,865 | `291dbe23f6f3f1af01c0658546d60423125bc2351a0c82edb6f8e5548ab72974` |
| LEXX.ACT | 369,561 | `3d1343b3e82c6b93ca8f005e9d1df09fa0ffafda9834750453a06b51ef7fa4a6` |
| LOGO.act | 356,778 | `37b09750ba6163b167aa273227902b2f6c27a721ca367a77d01620cfa79d8a4e` |
| LUCY.ACT | 631,877 | `fd5e70e4d4549de23aa47eb4b5c411e2dd430501c3d0dbe9507cb48b11df7ae2` |
| MNATURE.act | 1,916,989 | `131b407d77e5116b5a72155a80aa3b7a53779a5871beb3e2d65a8ef5271bb2db` |
| OFFLADY.act | 3,576,355 | `ab97f06cc0c42bfcdca09dae9f5b80c27654ad8e4928217b9fbb523ec6dcf3c8` |
| ORBY.ACT | 314,350 | `8bf1eebb778d67d22e0b500630075126f3920f0c4cdbde4d85292df06be9b46e` |
| POWERPUP.act | 479,272 | `26701407f7599c3e0c600a138fadc8551a0de33bc76d42c53cef7f5aca74030d` |
| ROBOT.act | 1,997,624 | `fcb2001c7746d17e8cbf1424edf2a03fbf890641f447637b37f92868cd1caf9b` |
| ROCKY(EarlyGreetings).ACT | 3,431,910 | `c94ed42b47bb4c2106113eecd8001d72088fe3725580cd2ef67be526c0eadc66` |
| ROCKY(Greetings).ACT | 2,820,438 | `02376b7ac73be74fcfb3265ae9d8193ed732918b14c915c984cc00f42ff8cc53` |
| ROCKY(Office).act | 2,300,295 | `8757ff434aa9d5fab6f60e6690f621b408c3c5d749b3fac7ef1e7a398a883c35` |
| ROVER.ACT | 429,869 | `ef4f9b9d2c69d87b8f8cb42b6af53f8516d211670a5e027d5c778ce439c31520` |
| RUBY.ACT | 417,198 | `b207d1878da00f033a43f53f557c05261aa7b2eefc26f99512ea0b310195a309` |
| SCRIBBLE.act | 377,183 | `d1d3e47ef76eeaac8c481b0a8fac89410a851908cbfc4b9f7719ac888c378b44` |
| SCUZZ.ACT | 413,786 | `4e6d88c1f5d5659b1150a78373e948eb933bb1b11d0ed5eea0278b151ca0741e` |
| SHELLY.ACT | 489,230 | `9a66defb85d254c1def41f2eb34116f013d26be5a697fcb643ad93287ecb573d` |
| TOFFLOGO.act | 4,120 | `e7040d39c4b9963ee380e4ca232377df71513d5fd606403d9f34d229591a4838` |
| TUTOR.ACT | 4,601,421 | `b216ece9a23a02497fb07381effa9652ebcefb316391580374fd4d18f60e9909` |
| WILL(Bob).ACT | 418,085 | `bfcc81933a17a314fe68d5d624a2a4290034807b3a40a93b805814bfc257743d` |
| WILL(Office).act | 399,965 | `00e7169818797384dd1bfc67773396bcf5559eab93c5af27072fae56a94af22d` |
| WORM.ACT | 359,018 | `6a187196b1064276147632c30183f68a373da5e5307ec2bd78bcd2cd34ad6901` |
| ZSPEAKER.ACT | 5,723 | `16205230b9f7d069cf371990aeac2b4e358415aae224658d81b7c9246377909b` |
| ZVISIBLE.ACT | 1,847 | `99f15fa45b0c1fb69616ec6d6b419c582a52798b505ac1cc732a61fd430d3512` |

### 2.3 Read-only comparison material

- `MSO.DLL` (Office XP, FileVersion 10.0.2625, SHA-256
  `14b7a79ad2cd96524671f8a63d63ed665253efcf02da74a814fec83f33225134`): used only for the
  MsoAnimationType to Agent animation name table (section 6).
- the Agent 2.0 Office character files (`*.ACS`, section 1.1 of `docs/sources.md`): used only to show that the Office palette is embedded there.

### 2.4 Prior public work (lead only, everything below re-verified)

- Treeki, robert.js (MIT), https://github.com/Treeki/robert.js : JavaScript reader for Bob
  and Office 97 actors. Correct on the pack and section layout and the three common command
  types. It does not handle bitmap actors, the color/transition tables, command types 3 and
  4, sound 0xFFFF, or the group-index indirection (it assumes variants are stored in group
  order, which is false in many files; see 4.11).
- agentpedia.tmafe.com has an "Actor Character Table" page (HTTP 403 for automated fetch,
  not used).

## 3. Method

- Static analysis only. MSO97.DLL was decompiled with Ghidra 12.1.4 headless in a private
  project (the shared REA MCP session was switched to another researcher's target in the
  middle of this run, so REA's single-target tools were unusable; REA `search_strings` was
  used once before that). Targeted disassembly with capstone. No Windows binary was run.
- A throwaway clean-room Python parser
  (`actparse.py`, `lz.py`, `coverage.py`, `render.py`) parses every
  file completely, decodes every bitmap, walks every WMF record stream, validates every
  cross reference and computes a byte coverage map. Proof renders (Clippit rest pose from
  WMF, Genius rest pose from bitmaps) are not part of this repo.
- No decompiled Microsoft code is reproduced here. Algorithms are described in prose.

## 4. File format

All integers little-endian. `u16`/`i16`/`u32` as usual. "R" denotes the start offset of the
resource inside the file; all section offsets are relative to R.

### 4.1 Pack container

| Offset | Type | Field | Notes |
|---|---|---|---|
| 0x00 | 2 bytes | magic | ASCII `LP` (bytes 4C 50). MSO97 compares the u16 against 0x504C. |
| 0x02 | u16 | major | 1 = Microsoft Bob, 2 = Office 97. MSO97 rejects anything else. |
| 0x04 | u16 | minor | 0 for Bob, 2 for every Office 97 file. Gates optional blocks (4.3). |
| 0x06 | u16 | item_count | 1 in every sample. |
| 0x08 | u16 | name_table_size | Bytes, including NUL terminators. |
| 0x0A | u32[item_count+1] | item_offsets | Absolute. Last entry must equal the file size (MSO97 checks this). |
| ... | char[name_table_size] | names | item_count NUL-terminated ANSI strings (build codepage; OFFLADY is cp932). First item starts right after the name table. |

The item name is the character's display name (`Clippit`, `The Genius`, `Kairu`, ...).
MSO97 looks items up by name (case-folded); with one item per file the first item is used.

### 4.2 Resource header (0x46 bytes at R)

| Off | Type | Field | Values seen | Meaning |
|---|---|---|---|---|
| 0x00 | u16 | header_size | 0x46 | Constant. |
| 0x02 | u16 | kind | 7 all Office actors; 1 Bob, Greetings Workshop and Tutor (the latter two are v2.2); 2 Hank (Bob) | MSO97 treats 1, 2, 7 as "actor"; string groups are attached only for kind < 3 or kind == 7. |
| 0x04 | u16 | rest_pose | e.g. Clippit 250, Genius 4, MNature 0 | Payload id of the neutral pose. Equals the u16 in the transition block (4.3). In the 9 CD actors 337 of 360 animation variants start with a 0 ms frame of this pose and 298 both start and end with one; the others start or end on another pose. |
| 0x06 | u16 | idle_anim | 1 in all files | Animation group auto-played when an animation ends while visible (5.6). 0xFFFF would mean "stop". |
| 0x08 | i16 | logical_w | 2880, 2490, 2145, 1860 | Logical width of the pose space (twips). |
| 0x0A | i16 | logical_h | 2160, 1875, 1950, 1395 | Logical height. |
| 0x0C | i16 | default_px_w | 192, 240, 166, 143, 124 | Default window width in pixels. |
| 0x0E | i16 | default_px_h | 144, 180, 125, 130, 93 | Default window height. |
| 0x10 | u16 | unk10 | 2083 always | Unknown. |
| 0x12 | u16 | unk12 | 2083 always | Unknown. |
| 0x14 | u16 | unk14 | 3 (Office), 1..6 (Bob) | Unknown. |
| 0x16 | u16 | unk16 | 1..3 | Unknown. |
| 0x18 | i16 | rect_a | e.g. Clippit 442 | Rectangle in logical units, see below. |
| 0x1A | i16 | rect_b | 452 | |
| 0x1C | i16 | rect_c | 2417 | |
| 0x1E | i16 | rect_d | 1825 | |
| 0x20 | u16 | nohit_image | 0xFFFF (Office), 0 (Bob) | Payload id that never counts as a mouse hit. |
| 0x22 | u16 | stretch_x0 | 0xFFFF | Bob feature: horizontally stretchable band start; 0xFFFF = not stretchable. |
| 0x24 | u16 | stretch_x1 | 0xFFFF | Band end. |
| 0x26 | u32 | off_payload_data | | Section offsets, all relative to R, non-decreasing in this order. |
| 0x2A | u32 | off_payload_index | | |
| 0x2E | u32 | off_sound_data | | |
| 0x32 | u32 | off_sound_index | | |
| 0x36 | u32 | off_anim_data | | |
| 0x3A | u32 | off_anim_index | | |
| 0x3E | u32 | off_string_data | | |
| 0x42 | u32 | off_string_index | | String index runs to the end of the resource. |

MSO97 offers two size modes (0x306D80E9): option 1 returns default_px; option 2 returns
`logical / k`, where k is a runtime global (inferred to be twips per pixel, 15 at 96 dpi).
logical / 15 equals default_px for all Office actors except The Dot, Scribble and Will
(default 240 x 180, natural 192 x 144). Which mode the Office 97 assistant window uses was
not established (see Unknowns). Bitmap actors store 124 x 93 images that are stretched to
166 x 125 (4.7). The 0x18..0x1E rectangle: MSO97's
accessor at 0x309AF67E passes +0x18 as left, +0x1A as top, +0x1E as right, +0x1C as bottom
(push order verified in the disassembly), scaled from logical units to the window. With
that reading the bottom edge lies below the frame in every file (Clippit: bottom 2417 >
2160), while reading it as left, top, right, bottom puts it inside the frame. The consumer
of this rectangle was not identified; treat as unknown (probably balloon or speech anchor).

### 4.3 Optional block between header and payload data (starts at R+0x46)

Present only in version 2 files. Its end must equal `off_payload_data`.

1. If `major >= 3` or (`major == 2` and `minor != 0`): **color table**
   - u32 `n`
   - `n` x 4 bytes: R, G, B, 0. In every WMF actor these are exactly the distinct colors
     used by its metafiles (Clippit 9, Will 26, Kairu 62). Bitmap actors have n = 0.
   - `n` x u16: all 0xFFFF in files; a per-color slot filled at load time. MSO97 enumerates
     metafile records through a palette-aware callback on display DCs (0x306E7EF1); the
     table exists for 256-color palette realization. bentwire can ignore it.
2. If `major >= 3` or (`major == 2` and `minor >= 2`): **pose transition table**
   - u32 `m`
   - u16 `rest_pose` (same value as header +0x04)
   - `m` x { u16 from_pose, u16 to_pose, u16 anim_id }. Example Genius: (0, 4, 10000),
     (4, 0, 10001); anim 10000 really runs pose 0 to pose 4. Usage in 5.5.

Bob (v1.0) files have no optional block (`off_payload_data == 0x46`).

### 4.4 Payload index

`off_payload_index .. off_sound_data` holds `N + 1` u32 entries (N = payload count). The
last entry is an end sentinel: `off_payload_data + sentinel == off_payload_index`.

Entry bits: bits 0..29 = blob offset relative to `off_payload_data`; bits 30..31 =
sub-image index inside a multi-image blob (only for `MNAK`). Several consecutive entries
share one blob (e.g. Genius: 0x00000000, 0x40000000, 0x80000000, 0xC0000000 are the four
images of the first pack). Blob size rule, as implemented by the runtime (0x30713FBA):
scan forward from entry i to the first entry whose masked offset differs; size = that
offset minus this offset. Masked offsets are non-decreasing in all files and blobs are
contiguous.

Payload ids are the index into this table. 0xFFFF is used in animations as "no image"
(blank frame).

### 4.5 Payload kinds

The first bytes decide the kind (0x30731562 reads 14 bytes, then branches). Counts are over
all 55 sample files (duplicates included):

| First bytes | Kind | Count in samples |
|---|---|---|
| u16 0x0014 | Composite pose | 26,215 poses with 251,036 child references |
| D7 CD C6 9A | Placeable WMF image | 5,995 |
| `DCIK` | Bitmap blob, 1 image | 4 |
| `MNAK` | Bitmap blob, n images | 2,528 blobs (n = 2 or 4) |

**Composite pose** (size exactly 4 + 10 x count):

| Off | Type | Field |
|---|---|---|
| 0 | u16 | 0x0014 tag |
| 2 | u16 | count |
| 4 + 10k | u16 | child payload id (image or, in principle, another composite) |
| 6 + 10k | i16 | left (logical units) |
| 8 + 10k | i16 | top |
| 10 + 10k | i16 | right |
| 12 + 10k | i16 | bottom |

Children are drawn in list order (first is backmost). Child rectangles are in the
resource's logical space relative to the parent rectangle: pixel = parent.left +
coord x parent.width / logical_w (MulDiv, truncating), same for y. Rectangles may extend
outside the frame (Power Pup: -180, -2235, 4980, 2685) and are then clipped. In the samples
every child is an image, never a nested composite, but the runtime recurses.

In WMF actors, payloads 0 .. rest_pose-1 are the WMF parts and rest_pose .. N-1 are poses.
In bitmap actors, most poses are the images themselves (an animation may reference an
image payload directly; it is drawn stretched to the full frame), with a handful of
composites (e.g. Genius 63) that place one or two images over the full frame.

### 4.6 WMF images

- 22-byte Aldus placeable header (key, hmf, bbox left/top/right/bottom as i16, inch = 72 in
  all files, reserved, checksum) followed by a standard Windows metafile.
- The payload size, not the metafile header, defines the length: in 179 of 5,995 metafiles
  `mtSize` disagrees with the payload (corrupt-looking header words). The runtime passes
  payload size minus 22 to SetMetaFileBitsEx and ignores `mtSize`. The placeable checksum
  is wrong in 23 Bob metafiles and is not checked.
- Playback (0x306E7EF1, 0x306E7F66): MM_ANISOTROPIC, viewport = destination rectangle,
  window = placeable bbox, then the record stream. All but one Office metafile contain their
  own SetWindowOrg/SetWindowExt, which then take over. In the 9 CD actors 956 metafiles use
  a window within 1 unit of the bbox, 328 use a different logical window, 1 has none. A
  renderer must honour in-stream window records.
- Because the target is the 8-bit back buffer (a display-type DC), MSO97 does not call
  PlayMetaFile directly but replays record by record through a callback (0x306E8018) that
  rewrites colors and passes every other record (including window records) unchanged:
  SetTextColor, SetBkColor, pen and solid-brush colors equal to RGB(245,250,245)
  (COLORREF 0xF5FAF5) are replaced by RGB(4,4,4), i.e. palette index 10, so that color
  means "transparent"; pen colors are snapped to the nearest Office palette entry; solid
  brushes are created through a palette mapping helper (0x306E7E4D, not analysed in
  detail; a special brush color 0xFDFF02 sets a flag). Neither magic color occurs in any
  sample file, but a faithful renderer should implement the 0xF5FAF5 rule. Office 97
  therefore showed WMF colors quantized to the Office palette; true-color rendering with
  the stored RGB values is a small, visible deviation that bentwire may accept.
- Record inventory, 9 Office 97 CD actors (each counted once): SelectObject 9343, Polygon
  8171, CreateBrushIndirect 4045, SetPolyFillMode 2003 (both ALTERNATE and WINDING),
  CreatePenIndirect 1926, Polyline 1617, SetBkMode 1412, SetTextColor 1410, SetROP2 1293,
  EOF 1285, SetMapMode 1284 (always 8), SetWindowOrg 1284, SetWindowExt 1284, Ellipse 567,
  DeleteObject 195, SaveDC 194, RestoreDC 194, SetBkColor 148, Escape 146, PolyPolygon 134,
  TextOut 68, SetTextAlign 27, CreateFontIndirect 16, RoundRect 1. Escape is always function
  15 (MFCOMMENT, ignorable): Dot 10, Hoverbot 24, Logo 100, Power Pup 5, Will 7. SetROP2 is
  13 (R2_COPYPEN) except 4 records in Hoverbot with 10 (R2_NOTXORPEN), which a renderer must
  implement as "destination XOR NOT pen color" or approximate. Bob metafiles use no record
  types beyond these.
- Pens: PS_SOLID, PS_NULL, PS_INSIDEFRAME with widths 0..48 logical units. Brushes: solid
  or hollow. Colors are plain RGB COLORREFs (high byte 0).
- Text: Clippit (TextOut of single characters, font "Albertus Medium") and Scribble
  ("Courier New") draw glyphs; Office Logo creates "Albertus Medium" fonts. bentwire needs
  a font fallback for these few frames.

### 4.7 Bitmap blobs (`DCIK`, `MNAK`) and image records

| Off | Type | Field |
|---|---|---|
| 0 | 4 bytes | `DCIK` (one image) or `MNAK` (pack) |
| 4 | u32 | total decompressed size |
| 8 | u32 | `MNAK` only: image count n (2 or 4 seen) |
| 12 | u32[n-1] | `MNAK` only: start offset of images 1..n-1 inside the decompressed buffer (image 0 starts at 0; image k ends where k+1 starts, the last at total) |
| 8 (`DCIK`) or 12 + 4(n-1) | bytes | compressed stream (4.8), always followed by 3 bytes FF FF FF padding in the samples; the runtime simply reads all remaining blob bytes |

The decompressed size must equal the declared total (the runtime fails otherwise).

Each image inside the decompressed buffer:

| Off | Type | Field |
|---|---|---|
| 0 | u32 | width (124 in Office bitmap actors; 166 x 125 for 3 Rocky images; 143 Tutor) |
| 4 | u32 | height |
| 8 | u32 | flags: bit 0 = RLE coded, bit 1 = raw; neither = error. All 10,106 samples use RLE. |
| 12 | bytes | pixel data |

Pixel data describes an 8-bit DIB: rows **bottom-up**, each row padded to
`stride = (width + 3) & ~3` bytes. RLE coding (decoder 0x3081EAB5): read a control byte c;
c == 0 ends the image; c < 0x80 means "repeat the next byte c times"; c >= 0x80 means
"copy the next (c & 0x7F) bytes literally". Output is `stride x height` bytes, sometimes
plus 2 trailing bytes (3,219 images); the runtime's output buffer has 2 spare bytes, so
truncate to `stride x height`.

**Palette:** indices refer to the fixed 256-entry "Office palette" built by
`MsoHpalCreate` (0x306E7190) from a LOGPALETTE at VA 0x309D9560 (file offset 0x317760 of
MSO97.DLL). On 256-color displays entries 1..9 and 246..255 may be replaced by system
colors; on true-color displays the table is used as is. The same 256 colors are embedded
byte-for-byte (as B,G,R,0) in all Office 2000 ACS files on hand (CLIPPIT, DOT, F1, LOGO,
MNATURE, OFFCAT, ROCKY.ACS), including index 10 = (4,4,4). SHA-256 of the 768-byte R,G,B
sequence: `a6c55c98cf2a3cec90166168985afd4ef17ae9fd3d29be9c94585f78d21c1b70`.

**Transparency:** palette index 10 is the color key. The runtime draws the character into
an 8-bit back buffer pre-filled with index 10 and copies only pixels != 10 (0x3072B6D0
called with key 10 from 0x30731A96); pixel hit testing also treats 10 as "miss"
(0x309AFD3D). For WMF actors, everything not painted stays transparent.

**Scaling:** bitmaps are stretched to the destination rectangle (the runtime caches one
converted copy per destination size). Genius-type actors therefore show 124 x 93 images
at 166 x 125 by default.

Decision for the lead: the bitmap actors cannot be colored from the .ACT alone. Options:
(a) ship the 256 RGB triples below in bentwire as a constant (they are a color table, also
identical to every Office ACS palette), (b) read them at runtime from the user's MSO97.DLL
or any Office ACS file. The table (R,G,B hex, index of first entry per row):

```
  0: 000000 800000 008000 808000 000080 800080 008080 c0c0c0
  8: c0dcc0 a6caf0 040404 080808 0c0c0c 111111 161616 1c1c1c
 16: 222222 292929 555555 4d4d4d 424242 393939 ff7c80 ff5050
 24: d60093 ccecff efd6c6 e7e7d6 ada990 330000 660000 990000
 32: cc0000 003300 333300 663300 993300 cc3300 ff3300 006600
 40: 336600 666600 996600 cc6600 ff6600 009900 339900 669900
 48: 999900 cc9900 ff9900 00cc00 33cc00 66cc00 99cc00 cccc00
 56: ffcc00 66ff00 99ff00 ccff00 000033 330033 660033 990033
 64: cc0033 ff0033 003333 333333 663333 993333 cc3333 ff3333
 72: 006633 336633 666633 996633 cc6633 ff6633 009933 339933
 80: 669933 999933 cc9933 ff9933 00cc33 33cc33 66cc33 99cc33
 88: cccc33 ffcc33 33ff33 66ff33 99ff33 ccff33 ffff33 000066
 96: 330066 660066 990066 cc0066 ff0066 003366 333366 663366
104: 993366 cc3366 ff3366 006666 336666 666666 996666 cc6666
112: 009966 339966 669966 999966 cc9966 ff9966 00cc66 33cc66
120: 99cc66 cccc66 ffcc66 00ff66 33ff66 99ff66 ccff66 ff00cc
128: cc00ff 009999 993399 990099 cc0099 000099 333399 660099
136: cc3399 ff0099 006699 336699 663399 996699 cc6699 ff3399
144: 339999 669999 999999 cc9999 ff9999 00cc99 33cc99 66cc66
152: 99cc99 cccc99 ffcc99 00ff99 33ff99 66cc99 99ff99 ccff99
160: ffff99 0000cc 330099 6600cc 9900cc cc00cc 003399 3333cc
168: 6633cc 9933cc cc33cc ff33cc 0066cc 3366cc 666699 9966cc
176: cc66cc ff6699 0099cc 3399cc 6699cc 9999cc cc99cc ff99cc
184: 00cccc 33cccc 66cccc 99cccc cccccc ffcccc 00ffcc 33ffcc
192: 66ff99 99ffcc ccffcc ffffcc 3300cc 6600ff 9900ff 0033cc
200: 3333ff 6633ff 9933ff cc33ff ff33ff 0066ff 3366ff 6666cc
208: 9966ff cc66ff ff66cc 0099ff 3399ff 6699ff 9999ff cc99ff
216: ff99ff 00ccff 33ccff 66ccff 99ccff ccccff ffccff 33ffff
224: 66ffcc 99ffff ccffff ff6666 66ff66 ffff66 6666ff ff66ff
232: 66ffff a50021 5f5f5f 777777 868686 969696 cbcbcb b2b2b2
240: d7d7d7 dddddd e3e3e3 eaeaea f1f1f1 f8f8f8 fffbf0 a0a0a4
248: 808080 ff0000 00ff00 ffff00 0000ff ff00ff 00ffff ffffff
```

### 4.8 Compression (own description)

The stream is the same bit-level LZ77 variant that Microsoft Agent uses for ACS images
(see `acs-format.md`); MSO97 contains its own copy (0x3081D66D). Description:

- Byte 0 must be 0x00; the bit stream starts at byte 1. Bits are consumed least
  significant bit first within each byte, bytes in order. Multi-bit values are assembled
  LSB first (the first bit read is bit 0 of the value).
- Repeat:
  - Read 1 bit. If 0: read 8 bits, output them as one literal byte.
  - If 1: a back reference. Read up to 3 further bits, stopping at the first 0, and count
    the 1s (k = 0..3). The distance is:
    k = 0: 6-bit value + 1; k = 1: 9-bit value + 65; k = 2: 12-bit value + 577;
    k = 3: 20-bit value + 4673, except that the raw 20-bit value 0xFFFFF marks the end of
    the stream.
  - Length starts at 2 (3 if k = 3). Then count consecutive 1 bits until a 0 bit (the 0 is
    consumed); call the count j (0..11; 12 ones is an error). Read a j-bit value v and add
    `v + (1 << j) - 1` to the length.
  - Copy `length` bytes from `distance` bytes back in the output, byte by byte (overlap
    allowed).
- The output must not exceed the declared size and a distance must not reach before the
  start; the runtime treats either as failure and also fails if the final size differs from
  the declared size. Minimum input length 8 bytes.

Pitfall found during verification: when j reaches 11 the terminating 0 bit still follows
and must be consumed (Earl and others contain such lengths).

### 4.9 Sounds

`off_sound_index .. off_anim_data` holds `S + 1` u32 offsets relative to
`off_sound_data` (last = end sentinel, `off_sound_data + sentinel == off_sound_index`).
Each sound blob is a complete RIFF WAVE file, embedded in the .ACT (no external files).
6 blobs are one byte longer than `RIFF size + 8` (odd chunk padding). Formats seen
(format tag, channels, rate, bits): PCM mono 8/16-bit at 11025, 16000, 22050, 44100 Hz;
**MS ADPCM (tag 2) mono 4-bit at 11025/22050/44100 Hz**, the dominant format in Office
actors. Some WAVs carry a LIST/INFO chunk. bentwire needs an MS-ADPCM decoder.

### 4.10 Grouped tables (animations and strings)

Animation groups: data `off_anim_data`, index `off_anim_index .. off_string_data`.
String groups: data `off_string_data`, index `off_string_index .. end of resource`
(absent if equal).

Index layout:

| Off | Type | Field |
|---|---|---|
| 0 | u32 | G = group count |
| 4 + 6g | u16 id, u16 count, u16 first | group headers, **sorted by id ascending** (binary search) |
| 4 + 6G + 6e | u32 offset, u16 size | entry e (offset relative to the section's data start) |

Entry count = (index length - 4) / 6 - G. Group g's variants are entries
`first .. first + count - 1`. `first` is an index into the entry list and is **not** in
group order (Clippit: group 1 uses entry 2). Two Greetings Workshop files have 3 entries
not referenced by any group; Office files reference every entry exactly once. Blobs are at
most 65,535 bytes (u16 size).

Variant choice (0x30714234): a caller may force a variant; otherwise
`rand() % count`, with MSVC's `rand` (state = state x 214013 + 2531011, result =
(state >> 16) & 0x7FFF).

### 4.11 Animation script

| Off | Type | Field |
|---|---|---|
| 0 | u16 | 0x0100 in all 1,686 scripts (not read by MSO97) |
| 2 | u16 | n = command count; blob size is exactly 4 + 6n |
| 4 + 6i | u16 type, u16 a, u16 b | command i |

| type | Name | a | b | Semantics |
|---|---|---|---|---|
| 0 | Frame | payload id, 0xFFFF = blank | duration ms | Show pose a for b ms. b = 0: not drawn (5.2). |
| 1 | Random branch | target command index | probability p | Jump to a if `rand() < p` (rand 0..32767). p = 0: never in normal play (exit branch). p >= 32768: always. |
| 2 | Sound | sound index, 0xFFFF = none | v2: low byte volume (15..128, mostly 128), high byte flag; v1: 0x0100 | Start sound a, asynchronously. |
| 3 | Host-condition branch | target | expected value | Jump if a host state value equals b. Only 1 occurrence (Tutor). |
| 4 | Time-of-day branch | target | period 0..3 | Jump if the current period equals b. Periods by local hour: 0 = 05:00 to 09:59, 1 = 10:00 to 17:59, 2 = 18:00 to 21:59, 3 = 22:00 to 04:59. Used by Genius, Saeko-sensei, Rocky. |

Counts over all files: type 0: 43,317; 1: 7,781; 2: 3,196; 3: 1; 4: 15. All targets are in
range; all frame and sound ids valid.

Conventions in Office 97 files: commands start with `Frame(rest_pose, 0)` and end with
`Frame(rest_pose or other neutral pose, 0)`; these zero-length frames mark entry and exit
poses for the transition logic. Looping animations (e.g. Clippit Idle) end in an
`always` backward branch; their exits are probability-0 forward branches.

### 4.12 String groups

Version 2: UTF-16LE, no terminator (the runtime appends two zero bytes). Version 1 (Bob):
ANSI. IDs present in every Office 97 actor (meanings inferred from content; no consumer
identified in MSO97):

| ID | Content |
|---|---|
| 1 | Display name |
| 8 | Description shown in the Assistant gallery |
| 40 | Greeting lines (2 to 3 variants) |
| 250 | Reminder headings |
| 433, 437, 439, 442 | Tip headings (feature tip, shortcut tip, suggestion, tip of the day) |

`.ACP` previews carry only 1, 8 and 40. Bob actors have 48 to 171 groups (dialog text).

### 4.13 .ACP preview files

Same format (version 2.2, kind 7) with a reduced payload set and two animation groups:
1 (Idle) and 12 (GetAttentionMinor), plus string groups 1, 8, 40. Used by the Office
Assistant gallery.

## 5. Playback behavior in MSO97.DLL

### 5.1 Drawing

- The character is drawn into an off-screen 8-bit DIB in the Office palette, pre-filled
  with index 10, then blitted skipping index 10; this applies to WMF and bitmap actors.
- Pose drawing (0x30731562) recurses through composites, scaling child rectangles from
  logical units (header +0x08/+0x0A) to the destination rectangle.
- Mouse hit testing walks the same tree and asks each image whether the pixel is opaque;
  `nohit_image` (header +0x20) is excluded.

### 5.2 Frame timing

- The engine keeps an absolute due time (0x30730F31). On each tick: if the tick is late,
  the due time is reset to now (lateness is dropped, not caught up); the step limit is
  due time + 14 ms. It then consumes commands, adding each frame's duration to the due
  time, until the due time passes the limit; so frames that fall within the next 14 ms are
  coalesced and only the last consumed frame with non-zero duration is drawn. Finally a
  timer is set for `due - now`, at least 14 ms (0x30731507).
- Zero-duration frames are not drawn; they only update the "logical last pose" used for
  transitions, and not even that while nothing is shown.
- Frame `0xFFFF` blanks the character (used in Appear and Disappear).

### 5.3 Branching

Evaluated at 0x30831C70: type 1 random (MSVC rand), type 3 host value, type 4 time of day
(4.11). A caller flag can override the time-of-day period (Play flags bits 28..31).

### 5.4 Interrupting an animation (exit mode)

When a new animation is requested while one is playing (0x307140EC), the new one is queued
(the queue holds one entry: a still pending request is discarded and replaced), the current
one switches to exit mode, and the next step is scheduled 100 ms later. In exit
mode: frame durations above 99 ms are cut to 100 ms; every branch command (types 1, 3, 4) is
not evaluated, instead execution continues at `max(next command, target)`, i.e. forward
branches are always taken and backward branches are skipped. Probability-0 forward branches
thus lead straight to the animation's exit sequence. This matches Agent's exit branches.

### 5.5 Transitions between animations

When the queued animation starts (0x307144C8): determine its start pose (the first frame's
pose; if the first frame has duration 0 and the following frame is blank, there is no start
pose). If the last logical pose of the finished animation differs from it, search the
transition table for an entry with `from_pose == last pose` and
`to_pose == start pose` or `to_pose == rest_pose` (first match in table order). If found,
the transition clip (group anim_id, typically 10000+) plays first, and the queued animation
remains queued; the check repeats after the clip, so pose A to B can chain through the rest
pose. Only bitmap actors use transitions (Genius, Mother Nature, Saeko-sensei, Rocky
Greetings, Tutor).

### 5.6 Automatic idle

On each timer tick (0x3073148A) after an animation has finished and the character is
visible (last drawn pose not blank), the engine starts animation group `idle_anim`
(header +0x06, = 1 everywhere) with a new random variant (0x309AF854). This happens also
when the finished animation was Idle itself; when the request comes from the start path it
is skipped if Idle is already current. Clippit's Idle loops internally; Genius's ten Idle
variants are finite and get re-rolled each time. After Disappear the last drawn frame is
blank, so no idle follows.

### 5.7 Sounds

Sound blobs are cached per index. With the sounds option enabled, a sound whose high flag
byte is 0 is played with PlaySound(memory, SND_MEMORY | SND_ASYNC) (0x3071363F); the
volume byte is not used by this build. If the high byte is non-zero the call goes to a stub
that does nothing (0x309AFCCD). In the Office actors the only such commands are 3 with
argument 0x0100 and sound 0xFFFF (Hoverbot, Power Pup, Scribble), i.e. no-sound markers;
only the Greetings Workshop Rocky uses a real sound with the flag (0x0150). Bob files (v1)
always use the PlaySound path; their argument is always 0x0100. Sound 0xFFFF plays nothing.

### 5.8 Office-level triggers (call sites of the play request 0x30714031 / 0x30714064)

| Situation | Animation | Evidence |
|---|---|---|
| Assistant shown | 2 Greeting on the first show (a global flag set at that moment; inferred: first show of the session), otherwise 32 Appear; a caller may pass another id | 0x306E6751 |
| Assistant hidden | 31 Disappear (caller may pass another id) | 0x306E6751 |
| Balloon shown while the current animation is Idle or none | 4 BeginSpeaking (when the caller passes no id) | 0x307E5270 |
| A caller-supplied point while Idle is current (some call sites pass constants such as (30, 100) or (80, 100); one passes coordinates through) | Look*: left of the window (screen) gives LookUpRight 111 / LookRight 108 / LookDownRight 106 by vertical band; right of it gives LookUpLeft 110 / LookLeft 107 / LookDownLeft 105; above or below gives LookUp 109 / LookDown 104 | 0x3071EF6B |
| Context menu "Animate!" (inferred from the 5-item assistant context menu) | one of 23 ids chosen by GetTickCount % 23 from the byte table at 0x309C0A30: 2, 4, 6, 11, 12, 13, 18, 19, 22, 23, 24, 25, 26, 32, 100, 101, 102, 103, 112 to 116; Idle again after 5 s | 0x308DA1AB, called from the menu handler 0x30757774 (item 0x3EB) |
| Other direct calls | 6 CharacterSuccessMajor (two sites), 23 WorkingAtSomething, 24 Thinking, 102 GetWizardy, 103 CheckingSomething, 19 GestureRight or 114 GestureLeft (by layout), Idle on certain dismissals | 0x30759A0B, 0x308D5F89, 0x309838ED, 0x3076D4B3, 0x3076DB7A, 0x3081CF99 |
| Switching actor | stop (-1), load new file, then Idle (or caller id) | 0x30738FB2 |

The host applications (Word, Excel, ...) and VBA (`Assistant.Animation`) request further
ids; those call paths are outside MSO97 and were not traced.

## 6. Mapping to ACS / Agent concepts

| ACT concept | ACS / Agent equivalent |
|---|---|
| Pack item name | Character name |
| default_px_w/h | Character width/height |
| Pose (composite of image + rect) | Frame with frame images (offset = rect.left/top; ACT scales the image into the rect instead of using native size) |
| WMF or bitmap payload | Image (ACS: 8-bit bitmap only) |
| Office palette, index 10 key | ACS palette + transparent color index (same 256 colors) |
| Frame command (pose, ms) | Frame duration (ACS in 1/100 s) |
| Random branch list | Frame branches (ACS: up to 3 per frame, % probability) |
| Probability-0 forward branch | Exit branch |
| Sound command | Frame sound |
| Animation group id with random variants | Named animation (+ states such as IDLINGLEVEL1 for variety) |
| Transition table + 10000+ clips | Return animations / transition types |
| idle_anim auto replay | Idle state handling of the Agent server |
| rest_pose | RestPose animation |
| String groups 1, 8 | Character name and description |

Animation ids found in Office 97 actors and the Agent animation name Office XP uses for the
same `MsoAnimationType` value (Office XP MSO.DLL, function 0x3108E414 maps the type to a
1-based index into a name table at 0x30C749AC; inverse at 0x3108E492; also documented in
`office-assistant.md`). Names in column 2 come from MSO97's own type library.

| ACT id | MsoAnimationType (Office 97 typelib) | Agent name (Office XP mapping) |
|---|---|---|
| 1 | msoAnimationIdle | Idle1_1 |
| 2 | msoAnimationGreeting | Greeting |
| 3 | msoAnimationGoodbye | Goodbye |
| 4 | msoAnimationBeginSpeaking | Explain |
| 5 | (not in Office 97; Office XP RestPose) | RestPose; in ACT use header rest_pose |
| 6 | msoAnimationCharacterSuccessMajor | Congratulate |
| 11 | msoAnimationGetAttentionMajor | GetAttention |
| 12 | msoAnimationGetAttentionMinor | Wave |
| 13 | msoAnimationSearching | Searching |
| 18 | msoAnimationPrinting | Print |
| 19 | msoAnimationGestureRight | GestureRight |
| 22 | msoAnimationWritingNotingSomething | Writing |
| 23 | msoAnimationWorkingAtSomething | Processing |
| 24 | msoAnimationThinking | Thinking |
| 25 | msoAnimationSendingMail | SendMail |
| 26 | msoAnimationListensToComputer | Hearing_1 |
| 31 | msoAnimationDisappear | Hide |
| 32 | msoAnimationAppear | Show |
| 100 | msoAnimationGetArtsy | GetArtsy |
| 101 | msoAnimationGetTechy | GetTechy |
| 102 | msoAnimationGetWizardy | GetWizardy |
| 103 | msoAnimationCheckingSomething | CheckingSomething |
| 104 | msoAnimationLookDown | LookDown |
| 105 | msoAnimationLookDownLeft | LookDownLeft |
| 106 | msoAnimationLookDownRight | LookDownRight |
| 107 | msoAnimationLookLeft | LookLeft |
| 108 | msoAnimationLookRight | LookRight |
| 109 | msoAnimationLookUp | LookUp |
| 110 | msoAnimationLookUpLeft | LookUpLeft |
| 111 | msoAnimationLookUpRight | LookUpRight |
| 112 | msoAnimationSaving | Save |
| 113 | msoAnimationGestureDown | GestureDown |
| 114 | msoAnimationGestureLeft | GestureLeft |
| 115 | msoAnimationGestureUp | GestureUp |
| 116 | msoAnimationEmptyTrash | EmptyTrash |
| 119, 120 | none (private) | none; The Dot only |
| 121 | none (private) | none; The Genius only |
| 122 | none (private) | none; Rocky (Office) only |
| 123 | none (private) | none; The Dot, Will, Earl, Rocky (Office) |
| 10000+ | none | transition clips, referenced only from the transition table |

All 9 CD actors define all 34 public ids from 1 to 116 above, except Mother Nature, which
lacks 19, 104 to 111 and 113 to 115. Beyond those: The Dot adds 119, 120, 123; The Genius
adds 121 and transitions 10000, 10001; Will adds 123; Mother Nature adds transitions 10000
to 10003. In MSO97 a request for a missing id fails in the group lookup and nothing is
played; bentwire should do the same or fall back to Idle. Non-Office files use other id ranges (Bob 1 to 32; Greetings 200
to 313; Tutor 401 to 428); their meanings are not covered here.

Agent states. ACT files have no state table; Office 97 used animation ids directly. The
table relates the Agent states of the Office 2000 ACS versions of the same characters to
ACT ids. State contents were read from CLIPPIT.ACS, ROCKY.ACS and MNATURE.ACS (read-only,
see `acs-format.md` for the state block format); the link to ACT ids goes through the
animation names of the table above, so it is derived, not taken from Office 97 code.

| Agent state | Animations in the Office ACS files | ACT id(s) |
|---|---|---|
| SHOWING | Show | 32 Appear |
| HIDING | Hide | 31 Disappear |
| IDLINGLEVEL1, 2, 3 | several Idle* animations (Clippit: IdleSideToSide, IdleEyeBrowRaise, IdleHeadScratch, IdleFingerTap, IdleRopePile, IdleSnooze, IdleAtom; Rocky: Idle(1..9), DeepIdle1) | 1 Idle (all idle behavior is inside the Idle variants and their branches; ACT has no idle levels) |
| GESTURINGLEFT / RIGHT / UP / DOWN | GestureLeft / GestureRight / GestureUp / GestureDown | 114 / 19 / 115 / 113 |
| LISTENING | Alert (Rocky also Explain) | none directly ("Alert" has no MsoAnimationType) |
| HEARING | Hearing_1 | 26 ListensToComputer |
| SPEAKING | not present in Office characters | 4 BeginSpeaking is the closest (Explain) |
| MOVING* | not present in Office characters | none |

## 7. Verification results

Commands (reference scripts, not public):

- `python3 -I scripts/actparse.py scripts <the 37 collection files> <the 9 CD .ACT> <the 9 CD .ACP>`
  : 55 of 55 files parse (37 collection files, 9 CD .ACT, 9 CD .ACP). Report in
  `out/parse_report.txt`.
- `python3 -I scripts/coverage.py scripts <same files>` : every file 0 unexplained bytes,
  0 overlapping bytes, payload offsets monotonic. `TOTAL unexplained bytes 0`.
- Checks performed per file: container offsets and size; name table; header size; optional
  block length exactly reaching payload data; payload/sound sentinels; every payload
  classified; 2,532 bitmap blobs decompressed to exactly their declared size (10,106
  images, all RLE, all >= stride x height); 5,995 WMF record streams walked to an EOF
  record that ends exactly at the payload end; 251,036 composite children in range with
  non-inverted rectangles; 1,017 sounds are RIFF WAVE with fmt chunk; group ids sorted;
  1,686 animation scripts with size 4 + 6n; every frame, branch target, sound and
  transition reference valid.
- Rendering proof: `scripts/render.py` produced `out/clippit_rest.png` (Clippit pose 250,
  composite of WMF parts) and `out/genius_rest.png` (Genius pose 4, bitmap with Office
  palette, key 10 transparent, rows upright when read bottom-up). Both look correct.
- Stride check: Rocky (Office) payload 391 (166 x 125, width not a multiple of 4) renders
  without shear when rows are read with stride 168 (`out/rocky_391.png`, part of the
  Disappear effect).
- Palette cross-check: the MSO97 table equals the palette inside 7 Office ACS files on all
  256 entries.

## 8. Evidence (MSO97.DLL 8.0.3507 virtual addresses)

| Address | Role |
|---|---|
| 0x306CFEC7 | Open pack: magic, major 1 or 2, offset table, last offset == file size |
| 0x306D00F8 | Find item by name |
| 0x306D01D8 | Load resource header (0x46), optional color and transition blocks, set up tables |
| 0x306D0478 | Group table reader setup (strings 0x3E/0x42, animations 0x36/0x3A) |
| 0x30714234 | Group lookup: binary search, variant by rand % count, blob read |
| 0x30713FBA | Payload entry to (offset, size, sub-index) |
| 0x30713A8E | DCIK/MNAK loader, sub-image selection, flags dispatch |
| 0x3081D66D | LZ decompressor |
| 0x3081E8C2, 0x3081EAB5 | RLE decode into padded bottom-up 8-bit DIB |
| 0x306E7CED | WMF payload: placeable check, SetMetaFileBitsEx(size - 22) |
| 0x306E7EF1, 0x306E7F66 | WMF playback: MM_ANISOTROPIC, viewport = rect, window = bbox |
| 0x30731562 | Draw / hit-test a payload (composite recursion with MulDiv) |
| 0x30731A96, 0x3072B6D0 | Back buffer blit with color key 10 |
| 0x309AFD3D | Pixel hit test (index != 10) |
| 0x306E7190, data 0x309D9560 | Office palette (LOGPALETTE, 256 entries) |
| vtable 0x306CFBC8 slot 3 = 0x3071435C | Play(id, flags) |
| 0x307140EC | Start or queue animation, exit mode |
| 0x30730F31 | Script step: frames, sounds, branches, 100 ms cap |
| 0x30831C70, 0x30730E91 | Branch conditions; MSVC rand |
| 0x307144C8 | Transition selection |
| 0x3073148A, 0x30731280, 0x309AF854 | Timer tick, auto-idle |
| 0x30731507 | Timer scheduling, 14 ms minimum |
| 0x30731180, 0x3071363F, 0x309AFCCD | Sound cache, PlaySound path, stub path |
| 0x306D80E9 | Size options (default px, natural size, Bob stretch band) |
| 0x309AF67E | Header rectangle accessor |
| 0x30782210 | String group fetch (ANSI conversion for v1) |
| 0x30714031, 0x30714064, 0x306E6751, 0x3071EF6B, 0x308DA1AB, 0x30738FB2 | Office-level play requests (5.8) |
| TYPELIB resource | `MsoAnimationType` values (section 6) |

Office XP MSO.DLL 10.0.2625: 0x3108E414 / 0x3108E492 (type to name index), name pointer
table 0x30C749AC.

## 9. Unknowns and open questions

- Which size mode (default_px or natural) the Office 97 assistant window uses, and the
  value of the twips-per-pixel global.
- Stretch filter for bitmaps shown at a different size (GDI StretchBlt-like nearest
  neighbour is likely; not verified).
- Header +0x10/+0x12 (always 2083), +0x14/+0x16 (small integers): no consumer found in a
  bounded search of the MSO97 decompilation.
- Header rectangle +0x18..+0x1E: field order per code contradicts the data range; purpose
  unknown (balloon anchor is a guess).
- Command type 3: compares with a host value read through an interface (property 0x61) and
  adjusted by Play flags; meaning unknown. Occurs once, not in Office 97 actors.
- Private animation ids 119 to 123: no caller with these constants in MSO97 (direct call
  sites listed in 5.8; vtable-dispatched and host-application callers were not traced).
- String group ids 40, 250, 433, 437, 439, 442: meaning inferred from text only.
- Color table slot array (u16 per color): runtime use only inferred.
- Sound volume byte and flag byte: stored but unused (volume) or routed to a stub (flag) in
  this build; may be used by later runtimes.
- Exact brush color mapping in 0x306E7E4D (nearest color or dither table) and the meaning
  of the special brush color 0xFDFF02 (4.6).
- Bob-only details (stretch band, v1 sound argument 0x0100, Bob metafiles without window
  records) are out of scope for bentwire's Office 97 support.
