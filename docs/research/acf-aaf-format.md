# Microsoft Agent 2.0 web delivery format: .ACF + .ACA (a.k.a. ".AAF")

Research note for bentwire. Clean-room description: everything below is written in our own
words from static analysis of the shipped binaries and from parsing real files. No Microsoft
code is reproduced and no extracted images or sounds are included.

## 1. Summary

- A web-delivered character is one small **.ACF** file (character definition, animation
  table, states, palette, balloon, voice, icon, localized names) plus **one file per
  animation**. In every real file and in the 2.0 runtime the per-animation extension is
  **.ACA**, not .AAF. The 2.0 SDK documentation still says ".AAF"; community sources call
  .AAF the Agent 1.5 animation format. The runtime never looks at the extension: it takes
  the per-animation file name verbatim from the ACF.
- Both files are wrapped in the same LZ-style compression that ACS uses for images
  (the "Agent compression"), but here it is applied to the **whole** body of the file.
  There is no other outer packaging (no CAB, no gzip): the files were served as plain
  `application/octet-stream`.
- An ACA is self-contained: its own sounds (RIFF WAVE), its own images, its frames. Images
  are **pre-composited full-size frames** (character width x height, 8 bpp, bottom-up DIB
  rows padded to 4 bytes) with a raw Win32 `RGNDATA` region each. Mouth overlays carry their
  own cropped pixel rectangle inline.
- The ACF lists, per animation: name, file name, return animation name, and a 32-bit
  "checksum". The ACA header repeats that checksum; the runtime compares the two for
  equality to detect stale cached copies. The editor computes it as a CRC-32 over its own
  textual animation definition, so a reader can only compare it, never recompute it.
- Loading on demand: `Load(url-or-path of .acf)` fetches only the ACF. Animations are
  fetched explicitly with `Get`/`Prepare` (type Animation, State or WaveFile); a state
  fetch also fetches each member's return animation. The ACA URL is the ACF URL up to and
  including its last `/` (or `\` for local paths) plus the file name from the ACF.
  The data provider only reads animations from the cache; per the SDK, playing an
  animation that was not fetched fails (agentsvr.exe not traced for this).
- Verified: a throwaway parser decodes all 4 ACF and 86 ACA files that survive in the
  Wayback Machine with zero unexplained bytes; all 796 image regions equal the opaque pixels
  of their image; all 444 Merlin frames that exist as ACA are pixel-identical to the
  composited frames of the user's `merlin.acs`, and sounds, branching, durations and
  overlays agree as well.
- Confidence: high for the ACF 0x20001 / ACA 0x20001 compressed path (sample-verified and
  binary-verified). Medium for older-version branches, uncompressed ACAs, headerless ACAs,
  "replace base frame" overlays and the OLE compound-file variant: these are read from the
  binary only, no samples exist.

## 2. Terminology: ACF, ACA, AAF

| Name | Meaning | Evidence |
|---|---|---|
| .ACF | "Agent Character File (HTTP format)", the character definition | agentdpv.dll registry script: `.acf` and `.aca` map to ProgID `Agent.Character.2`, described as "Microsoft Agent Character File (HTTP format)" |
| .ACA | per-animation file of Agent 2.0 | same registry script; every animation file on Microsoft's server (`*.aca`); ACE.EXE strings `.aca`, `.acf` |
| .AAF | name used by the 2.0 SDK docs ("separate character data (.ACF) and animation (.AAF) files"); community wiki says .AAF is the Agent 1.5 animation format | techshelps mirror of the SDK docs; agentpedia "List of file formats" (lead only) |
| .ACS | single-file format (other researcher's topic) | agentdp2.dll handles `.acs` (magic 0xABCDABC3) |

No `.aaf` string (ASCII or UTF-16, case-insensitive) exists in agentdpv.dll, agentctl.dll,
agentsvr.exe or ACE.EXE. If an Agent
1.5 ACF references `.aaf` files, the 2.0 runtime would still open them by that name and
parse them with the version-gated rules in section 9. No .AAF or 1.5-era ACF sample was
found (see section 3).

## 3. Samples and provenance

All downloads are Microsoft's own freely distributed web characters, fetched from the
Wayback Machine with the `id_` form (raw bytes): `(public archive copy)<ts>id_/<url>`.

ACF files (identical bytes at both locations, so the files did not change 1999 to 2006):

| Character | Original URL | Wayback timestamp | Size | SHA-256 |
|---|---|---|---|---|
| Genie | `http://agent.microsoft.com/agent2/chars/genie/genie.acf` | 20000407195309 | 8325 | `3d94ea569c8d8bbb99d7cdf500d32db44b70e686c912edb39617eaabab5bbccb` |
| Merlin | `http://agent.microsoft.com/agent2/chars/merlin/merlin.acf` | 19990508214516 | 9051 | `ae62f4c6ed914d2d9c2caea6ff39f8cde68eaca821c80116f7372e4377acbd7f` |
| Peedy | `http://agent.microsoft.com/agent2/chars/peedy/peedy.acf` | 19990508222229 | 7386 | `42ad3b2ad3328020f1380e7f0696fc39834a34c6b52b67977db9a39715cb61e5` |
| Robby | `http://agent.microsoft.com/agent2/chars/robby/robby.acf` | 20000511173803 | 8672 | `a4d3e0a4ffe4f65311b8ca8ae5ceace6e28d6c7cfacdc3fdac855653cf66bbe0` |

Second copies, byte-identical: `http://www.microsoft.com/msagent/chars/{genie,merlin,peedy,robby}/<name>.acf`
(timestamps 20060621202418, 20060621202614, 20060221035407, 20060708024144).

ACA files: `http://agent.microsoft.com/agent2/chars/<char>/<File>.aca`, captured between
2000-02-29 and 2002-05-26 (one CDX timestamp per URL, from
`(public archive copy)`).
86 files, 3,419,499 bytes. Only part of each character survives:

- genie (12 of 76): Announce, Blink, Explain, GestureRight, Greet, Hide, Idle1_1, Pleased, RestPose, Show, Think, Uncertain
- merlin (45 of 73): Acknowledge, Alert, Announce, Blink, Confused, Congratulate, Congratulate_2, Decline, DontRecognize, Explain, GestureDown, GestureLeft, GestureRight, GestureUp, GetAttention, GetAttentionReturn, Greet, Hearing_1, Hearing_3, Hearing_4, Hide, Idle1_1..Idle1_4, Idle2_2, LookDownBlink, LookDownReturn, LookLeft, LookLeftReturn, LookRightBlink, LookUpBlink, MoveDown, MoveLeft, MoveRight, MoveUp, Pleased, RestPose, Sad, Show, StartListening, Think, Uncertain, Wave, WriteReturn
- peedy (13 of 85): Announce, Blink, Explain, GestureLeft, GestureRight, Idle1_1, Pleased, RestPose, Search, Show, Think, Uncertain, Wave
- robby (16 of 68): DontRecognize, Greet, Hearing_2, Hearing_3, MoveDown, MoveRight, MoveUp, Pleased, Process, Processing, RestPose, Sad, Show, Think, Uncertain, Wave

The server URLs use mixed case (`Genie/Blink.aca`, `genie/Show.aca`) while the ACF holds
`Blink.aca`; IIS was case-insensitive. A Linux reader of a local directory must look file
names up case-insensitively.

Character Editor (for the writer side and the checksum): `http://agent.microsoft.com/agent2/sdk/ace.exe`,
Wayback 20010608083800 (self-extracting CAB, unpacked with cabextract, never run). Contains
`ACE.EXE` (Sept 1998, SHA-256 `1f2856e79f764eb8a6dfb259cc47240e929db412e10ba512790d69367cef20a2`).

Negative results: the full CDX listing of `agent.microsoft.com/agent2/chars/*`,
`www.microsoft.com/msagent/chars/*` and `msdn.microsoft.com/msagent/*` (status 200) contains
only the extensions .aca, .acf, .acs, .asp and .asx; a filtered CDX query for `.acf`/`.aaf`
under `www.microsoft.com/msagent/` returned only the .acf URLs above. Queries for the
Agent 1.5 era paths (`agent.microsoft.com/` as a whole, `www.microsoft.com/workshop/prog/agent/`,
`/intdev/agent/`, `/workshop/imedia/agent/`) returned 504 or "temporarily offline" on every
attempt in this session, so they are inconclusive, not negative. Result: no 1.5 (.AAF or ACF
version < 0x20001) sample was obtained.

Local scratch copies (throwaway): downloads in
`$BENTWIRE_REFERENCE/`,
scripts in `.../scratchpad/`.

## 4. Primitive types

All integers little-endian. `GUID` is 16 bytes in Windows layout (u32, u16, u16, 8 bytes).

**ACF string (differs from ACS):** `u32 n` = number of UTF-16LE code units, then `2*n` bytes.
**No NUL terminator**, also not when `n > 0` (ACS strings carry a terminator). The runtime
rejects `n > 0x1000000`. `n = 0` is the empty string.

## 5. Container layer

### 5.1 ACF file

| Offset | Type | Field | Notes |
|---|---|---|---|
| 0 | u32 | magic | 0xABCDABC4 in all samples. Runtime accepts 0xABCDABC2 too, with identical handling (likely the 1.5 ACF magic, unverified). Anything else: error 0x80070002. |
| 4 | u32 | uncompressed size | size of the ACF body |
| 8 | u32 | compressed size | number of compressed bytes that follow |
| 12 | bytes | compressed body | Agent compression (5.3). Decompressed length must equal the uncompressed size, otherwise error 0x80070570. |

In the samples `12 + compressed size == file size` exactly. There is no "stored" variant:
the ACF body is always compressed.

### 5.2 ACA file

Header presence depends on the **ACF** version (the runtime checks the character's version,
not anything in the ACA):

| Offset | Type | Field | Notes |
|---|---|---|---|
| 0 | u32 | version | only if ACF version >= 0x1001D. Must be one of 0x1001C, 0x1001E, 0x1001F, 0x20001 (all samples: 0x20001). If it is not, the runtime seeks back 4 bytes and treats the file as headerless. |
| 4 | u32 | checksum | only with a recognized version. Must equal the checksum of this animation in the ACF table, else error 0x8004200F (stale cache, triggers re-download in the prepare path). |
| 8 (or 0) | u8 | compression flag | 1 = compressed block follows, 0 = the body follows directly, other = error 0x80070570 |
| +1 | u32 | uncompressed size | only when flag = 1 |
| +5 | u32 | compressed size | only when flag = 1 |
| +9 | bytes | body | compressed or raw ACA body (section 7) |

All 86 samples: version 0x20001, flag 1, `17 + compressed size == file size`.

Headerless ACAs: when the ACF version is below 0x1001D the headerless ACA is accepted as is
(no verification, no re-download). When a header is expected but its version is not
recognized, the runtime parses it headerless but reports it as "unverified", and the
prepare path then downloads it again (section 10.3).

### 5.3 Agent compression (same algorithm as ACS, applied to the whole body)

Own description, implemented in `scripts/agentlz.py` and cross-checked against agentdpv
0x5a962a17 and the ACS researcher's independent decoder:

- Input must be at least 8 bytes, start with a 0x00 byte and end with at least six 0xFF
  bytes; otherwise decoding fails.
- After the first byte the input is a bit stream read least-significant bit first within
  each byte, bytes in order.
- Loop: read 1 bit.
  - 0: literal. The next 8 bits are one output byte.
  - 1: back-reference. Read the distance class from up to 3 more bits:
    `0` -> 6 bits, distance = value + 1;
    `10` -> 9 bits, distance = value + 65;
    `110` -> 12 bits, distance = value + 577;
    `111` -> 20 bits; if the value is 0xFFFFF this is the **end marker**, otherwise
    distance = value + 4673 and the length bias becomes 2 instead of 1.
    Then the length: count consecutive 1 bits `k` (stop at the first 0 bit, which is
    consumed; more than 11 ones is an error), read `k` more bits `v`;
    length = bias + 2^k + v (minimum 2, or 3 for the 20-bit class).
    Copy `length` bytes from `distance` bytes back in the output, byte by byte (overlap allowed).
- Errors: distance beyond produced output, output beyond the declared size.
- In the samples the end marker is followed by a few more 0xFF padding bytes (ACF: 3 bytes
  unused after the marker); readers must ignore the remainder after the end marker.

### 5.4 OLE compound-file variant (binary only, no sample)

agentdpv.dll has a third entry point (0x5a964d61) for a character stored as an OLE
structured storage file (opened with `StgOpenStorage`, retried up to 10 times on sharing
violations):

- stream `char.acf` inside the storage: u32 magic **0xABCDABC1**, then the same
  "u32 uncompressed size, u32 compressed size, compressed ACF body" block as 5.1;
- each animation is a stream in the same storage, named by the ACF table's file-name field
  (0x5a965373). Stream layout: if ACF version >= 0x1001D, u32 version + u32 checksum
  (read but **not** compared), then the u8 compression flag and body exactly as 5.2.

This is most likely the Agent 1.5 ".ACS" container (ACE.EXE also checks 0xABCDABC1 at
0x4248e0). It is neither the 2.0 ACS (0xABCDABC3, agentdp2.dll) nor ACF/ACA. **Nobody owns
this format yet**; whoever writes the spec should decide whether bentwire supports it. It
would need an OLE compound-file reader plus the parsers in this note.

## 6. ACF body (after decompression)

Fields in file order. "v" is the ACF version (first field).

| # | Type | Field | Condition / notes |
|---|---|---|---|
| 1 | u32 | version | accepted: 0x1001C, 0x1001E, 0x1001F, 0x20001. `<= 0x1001B` or other values in range: error 0x80042202; `> 0x20001`: error 0x80042203. All samples 0x20001 (bytes `01 00 02 00`, i.e. minor 1, major 2). |
| 2 | u16 | animation count N | |
| 3 | N x entry | animation table | see 6.1 |
| 4 | GUID | character GUID | same GUID as the matching ACS (Merlin checked) |
| 5 | localized info | | v >= 0x20001: `u16 count`, then per entry `u16 LANGID, string name, string description, string extra`. v < 0x20001: exactly one entry without count or LANGID: `string name, string description, string extra`, LANGID implied 0x0009. Samples: 30 languages, LANGID 0x0009 first. |
| 6 | u16 | width | frame width in pixels (Genie 128, Peedy 160) |
| 7 | u16 | height | frame height in pixels |
| 8 | u8 | transparent color index | palette index treated as transparent (10, Peedy 254) |
| 9 | u32 | flags | same bit meanings as ACS. Bit 0x20 gates the voice block, bit 0x100 suppresses the balloon block. Samples: 0x00110220. |
| 10 | u16, u16 | animation set version major, minor | only v >= 0x20001 (samples 2, 0). For older v the runtime substitutes major 1, minor 0x32 (50). |
| 11 | voice block | | only if flags & 0x20: `GUID engine, GUID mode, u32 speed, u16 pitch`; then, only if v > 0x20000: `u8 has_extra`; if nonzero: `u16 LANGID, string dialect, u16 gender, u16 age, string style` |
| 12 | balloon block | | only if !(flags & 0x100): `u8 lines, u8 chars per line, u32 fg COLORREF, u32 bg COLORREF, u32 border COLORREF, string font name, i32 font height, u32 font weight, u8 italic`; then `u8` (unknown, 0 in samples; ACS calls it underline) only if v > 0x1001E |
| 13 | u32 + RGBQUAD[] | palette | `u32 count`, then `count` x 4 bytes (B, G, R, reserved). Samples: 256 entries, identical to the ACS palette. |
| 14 | tray icon | | only if v > 0x20000: `u8 present`; if nonzero: `u32 size, mask DIB[size], u32 size, color DIB[size]`. Each DIB is a packed BITMAPINFOHEADER + palette + bits (samples: 16x16 1 bpp mask 112 bytes, 16x16 4 bpp color 232 bytes). |
| 15 | states | | `u16 count`, then per state `string state name, u16 n, n x string animation name`. Names are upper case in samples (`SHOWING`, `IDLINGLEVEL1`, ...); lookups are case-insensitive. |

The body must end exactly after the states; all 4 samples do.

Field order differs from ACS: ACS puts the localized info at the end behind a locator and
has no animation table here. Field contents (voice, balloon, palette, icon, states) are the
same as ACS apart from the string encoding.

### 6.1 Animation table entry

| Type | Field | Notes |
|---|---|---|
| string | animation name | lookup key, compared case-insensitively |
| string | file name | relative file name of the ACA (`Blink.aca`); in samples always `<name>.aca` |
| string | return animation | name of the animation to play when this one is exited (transition type 0). Empty in all samples (they use transition types 1 and 2), and for Merlin it matches the ACS return names exactly. |
| u32 | checksum | only if v > 0x1001D. Opaque; compared with the ACA header (5.2). |

### 6.2 About the checksum

ACE.EXE builds a textual definition of the animation (`DefineAnimation ... EndAnimation`,
including frame durations, offsets, branches, sound and bitmap file names and their file
timestamps) and stores the standard CRC-32 (reflected polynomial 0xEDB88320, init and
final XOR 0xFFFFFFFF; table found at VA 0x44e93c) of that text as the checksum (ACE
0x408c70, 0x4084d0, 0x429bd0). The text is not stored anywhere, so readers can only test
equality. Our checks: none of CRC-32 / Adler-32 / byte sum over the stored ACA body,
compressed block or file matches, which is consistent with this. Inferred, not proven by
recomputation.

## 7. ACA body (after decompression)

The layout is gated by the **ACF** version `v` (the runtime passes the character's
version into the animation object at 0x5a968a49). The ACA body has no name: the name comes
from the ACF entry.

| # | Type | Field | Notes |
|---|---|---|---|
| 1 | u16 | sound count S | |
| 2 | S x (u32 size, bytes[size]) | sounds | complete RIFF WAVE files. Samples: PCM 8/16 bit and MS-ADPCM (format tag 2), mono, 11025/16000/22050 Hz; RIFF length + 8 == size in all 86 files. |
| 3 | u16 | image count I | |
| 4 | I x image | images | see 7.1 |
| 5 | u8 | transition type | only if v >= 0x10020 (that is, only 0x20001 among accepted versions); otherwise 0. Same meaning as ACS: 0 = use return animation, 1 = use exit branches, 2 = none. |
| 6 | u16 | frame count F | must be nonzero (error 0x8004200F) |
| 7 | F x frame | frames | see 7.2 |

The body must end exactly after the last frame; all 86 samples do.

### 7.1 Image

| Type | Field | Notes |
|---|---|---|
| u32 | size | number of pixel bytes. 0 means "empty image" and **nothing else follows** for this image. |
| u8 | unknown | only if size != 0. 0 in all samples, never read back by agentdpv. ACE writes it from an uninitialized-looking local (unresolved). |
| bytes[size] | pixels | 8 bpp palette indices, bottom-up rows, row stride `(width + 3) & ~3` using the **ACF** width and height. In all 796 sample images `size == stride * height`. |
| u32 | region size R | only if size != 0 |
| bytes[R] | region | raw Win32 `RGNDATA`: header `u32 dwSize=32, u32 iType=1 (RDH_RECTANGLES), u32 nCount, u32 nRgnSize, RECT rcBound`, then nCount `RECT {i32 left, top, right, bottom}`, coordinates top-down in frame space. Not compressed (the ACS region may be). R may be 0. |

Each image is the complete, already composited frame picture. ACE composites the frame's
bitmap layers and deduplicates identical layer stacks within one animation (ACE 0x408010),
so several frames may share an image index. The region is the window shape for that frame:
in all 796 images the region's rectangles cover exactly the pixels whose index is not the
transparent index.

### 7.2 Frame

| Type | Field | Notes |
|---|---|---|
| u16 | image index | into this ACA's image list; 0xFFFF = no image (62 such frames in samples) |
| u16 | sound index | into this ACA's sound list; 0xFFFF = none |
| u16 | duration | same unit as the ACS frame duration (1/100 s per SDK and ACS research; not traced here). Values 0..30 typical, identical to merlin.acs |
| i16 | offset X | ACE writes the frame's `OffsetX` here (ACE 0x410a90 vs 0x411230). Not used by agentdpv; 0 in all samples. |
| i16 | offset Y | ACE `OffsetY`, same notes |
| i16 | exit branch | only if v >= 0x10020, else -1. -1 = none, -2 = "continue with normal branching while exiting", >= 0 = target frame |
| u8 | branch count B | |
| B x (u16, u16) | branches | target frame index, probability in percent |
| u8 | overlay count O | |
| base image block | | only if v > 0x1001F and O != 0: `u8 present`; if nonzero: `u32 size`; if size != 0: `bytes[size]` full-frame pixels (layout as 7.1, but **no** unknown byte), `u32 R`, `bytes[R]` RGNDATA. present = 0 in all 47 sample frames that have overlays. |
| O x overlay | overlays | see 7.3 |

### 7.3 Overlay (mouth shape)

| Type | Field | Notes |
|---|---|---|
| u8 | overlay type | mouth shape 0..6 (ACE names them starting with "Closed"; ACS researcher has the full list). The runtime stores overlays in a slot indexed by this value. |
| u32 | size | pixel byte count; 0 = empty overlay, nothing else follows |
| u8 | reserved | only if size != 0. ACE always writes 0; runtime ignores it. |
| u8 | replace base frame | only if size != 0 and v > 0x1001F. ACE writes its `ReplaceBaseFrame` flag here. |
| i16, i16 | x, y | top-left of the overlay rectangle in frame coordinates (top-down) |
| u16, u16 | w, h | rectangle size. **A real size**, unlike the ACS overlay record. |
| bytes[size] | pixels | 8 bpp, bottom-up rows, stride `(w + 3) & ~3`; size == stride * h in all 329 sample overlays |
| u32 R, bytes[R] | region | only if "replace base frame" != 0 |

Samples: each overlay frame has all 7 mouth types, all with replace base frame = 0. The
rectangle is a small crop around the mouth, while ACS stores the overlay as a full-frame
image; the opaque pixels are the same (Merlin: 147 of 147 overlays have exactly the same
opaque pixels at the same positions as the ACS overlays).

## 8. Runtime semantics needed for playback

### 8.1 Drawing a frame (agentdpv 0x5a96910f, 0x5a969432, 0x5a962ceb)

- No mouth requested: copy the frame's image pixels; window shape = image region.
- Mouth shape m requested and the frame has a non-empty overlay of type m whose rectangle
  intersects the frame:
  - if the overlay's "replace base frame" flag is 1: start from the frame's base image
    block (or, when that block is absent or has size 0, from a canvas filled with the
    transparent index); otherwise start from the frame's normal image;
  - copy the overlay rectangle onto it, skipping overlay pixels equal to the transparent
    index, clipped to the frame;
  - window shape: only when the flag is 1, the overlay region united with the base block's
    region; otherwise the normal image region.
- The mouth shape comes from the current speech phoneme: the caller passes a 16-bit
  phoneme code (negative = not speaking, draw the plain image). A built-in table
  (agentdpv 0x5a96b779, lookup 0x5a96b470) maps Unicode IPA code points to mouth types 0..6
  (table below). For a phoneme not in the table, the mouth used last in this animation is
  reused; if there is none, the plain image is drawn.

| Mouth type | Phoneme code points |
|---|---|
| 0 | U+0000, U+0020, `_`, b, m, p, U+0253, U+0278, U+0298, U+03B2 |
| 1 | f, i, n, v, U+0268, U+0269, U+026A, U+026F, U+0271, U+0287, U+028B |
| 2 | c, d, e, k, l, q, s, t, z, U+00F0, U+014B, U+0256, U+0257, U+0258, U+025F, U+0261, U+0262, U+026C, U+026D, U+026E, U+0272, U+0273, U+027A, U+027D, U+027E, U+027F, U+0282, U+0288, U+0290, U+0294, U+0297, U+02A3, U+02A5, U+02A6, U+02A8, U+03B8 |
| 3 | j, x, U+00E7, U+0251, U+0259, U+025B, U+0260, U+0263, U+0265, U+0267, U+0270, U+0274, U+0280, U+0281, U+0286, U+028C, U+028E, U+0293, U+03C7 |
| 4 | a, h, U+00E6, U+0127, U+025C, U+0266, U+0295, U+0296 |
| 5 | U+0153, U+0252, U+0254, U+0255, U+025A, U+025D, U+025E, U+0264, U+026B, U+0276, U+0279, U+027B, U+0283, U+0284, U+0285, U+0291, U+0292, U+02A4, U+02A7 |
| 6 | o, r, u, w, y, U+00F8, U+0275, U+0277, U+027C, U+0289, U+028A, U+028D, U+028F |

  This table lives in the ACF/ACA data provider; whether agentdp2.dll (ACS) uses the same
  one is for the ACS note to confirm.

### 8.2 Frame advance (agentdpv 0x5a968dcb, 0x5a968d98)

- Normal play: if the frame has branches, draw r uniformly in 1..100 and walk the branch
  list, taking the first branch whose probability is >= r (subtracting each probability from
  r as you pass it), provided the target index is valid; if none is taken, go to the next
  frame; past the last frame the animation ends.
- Exiting (a stop or a new request arrives): if the current frame's exit branch is not -1
  the animation switches to exiting mode; in that mode a frame with exit -1 ends the
  animation, -2 uses normal branching, any other value jumps to that frame.
- Frames with no image (0xFFFF) and duration 0 are skipped automatically, at most 32 in a
  row.
- When an animation is left and it is not using exit branches, the runtime plays its return
  animation (ACF entry field 3) unless that is the animation being requested next
  (agentdpv 0x5a96834a).

### 8.3 Pixels and colors

Palette from the ACF (RGBQUAD order B, G, R, x). Transparent index from the ACF. Frames are
`width x height`. Same conventions as ACS images, but without per-image width/height and
without per-image compression.

## 9. Version-gating matrix and what is verified

| Condition (v = ACF version) | Effect | Sample-verified? |
|---|---|---|
| ACF magic 0xABCDABC4 | normal | yes (4 files) |
| ACF magic 0xABCDABC2 | identical handling | binary only |
| OLE storage + `char.acf` magic 0xABCDABC1 | section 5.4 | binary only |
| v in {0x1001C, 0x1001E, 0x1001F} | all older-version branches below | binary only |
| v > 0x1001D | ACF entry has checksum; ACA has version + checksum header | yes (0x20001) |
| v < 0x1001D | no checksum, headerless ACA | binary only |
| ACA header version unknown although v >= 0x1001D | seek back, parse headerless, always re-download | binary only |
| v >= 0x20001 | localized list with LANGIDs; animation set version present | yes |
| v > 0x20000 | voice "extra" block; tray icon block | yes (voice extra = 1, icon present) |
| voice extra = 0, flags without 0x20, flags with 0x100, icon absent | skip those blocks | binary only |
| v > 0x1001E | balloon trailing byte | yes |
| v >= 0x10020 | ACA transition byte, frame exit branch | yes |
| v > 0x1001F | frame base image block, overlay "replace base frame" byte | yes for the bytes being present; present/replace flags = 1 never seen |
| ACA compression flag 0 | raw body | binary only |
| image size 0, overlay size 0 | short records | binary only |
| frame offsets X/Y nonzero | ignored by agentdpv | binary only (always 0) |

## 10. Loading on demand

### 10.1 Opening a character (agentdpv 0x5a966aff, 0x5a966fb6, 0x5a966d0d)

- The path given to `Load` is parsed with `InternetCrackUrlW`.
  - Not a URL (parse fails or unknown scheme): **local file mode**, path at most 259
    characters. Base directory = everything up to and including the last `\`.
  - Scheme FTP (1), Gopher (2) or HTTP (3): **URL mode**. Backslashes in the URL are
    rewritten to `/`. Base = everything up to and including the last `/`. Max 2047
    characters.
  - Scheme FILE (5): local file mode using the URL's path part.
  - **HTTPS (4) and every other scheme are rejected** (error 0x80042209, the localized
    message is "The specified URL protocol is not supported by Microsoft Agent").
- URL mode: the ACF is downloaded asynchronously with a URL moniker
  (`CreateURLMoniker`, `BindToStorage` for an IStream, bind-status callback; agentdpv
  0x5a96ad8c); the call returns "pending" and completion is reported later. The parser
  then reads the ACF **from the WinINet (IE) cache** (`RetrieveUrlCacheEntryStreamW`,
  0x5a9666c1), not from the transfer stream.
- File mode: the ACF is opened and parsed synchronously.
- Before parsing, the ACF location is checked against the IE security zones
  (agentdpv 0x5a964115): Local Machine and Trusted Sites pass; Intranet passes only if
  `HKLM\SOFTWARE\Microsoft\MSAgent\TrustIntranetZone` is nonzero; Internet, Restricted and
  untrusted Intranet show a Yes/No warning (strings in `xpsp3res.dll`) and continue only on
  Yes. This is a late security update in build 2.00.0.3427, not part of the format.

### 10.2 URL of an animation (agentdpv 0x5a9675dc, 0x5a967804)

`aca_location = base + file_name_from_ACF_entry`, plain string concatenation (no URL
normalization, no `..` handling; the combine API is not used here). Animation names are
looked up in the ACF table case-insensitively; an unknown name is error 0x80042003. In URL
mode the ACA is read from the IE cache; in file mode it is opened from disk.

### 10.3 Get / Prepare (agentctl 0x71225370, agentdpv 0x5a967e74, 0x5a967f6c, 0x5a967664)

The control's `Get(Type, Name, Queue)` maps Type case-insensitively: `ANIMATION` -> 0,
`STATE` -> 1, `WAVEFILE` -> 2, anything else error 0x8004200E; it calls the server's
Prepare with the type, the name string and the queue flag. The SDK documents that Name may
be a comma-separated list and that state group names like `Idling` or `Moving` exist; the
splitting and group expansion happen in the server, not in the data provider (not traced).

In the data provider:
- **Animation**: first a "check only" load from the cache (parse completely, then discard).
  If it succeeds (verified header, or a legitimately headerless ACA of an old ACF), nothing
  is downloaded. If it fails (not cached, checksum mismatch, corrupt) or the header version
  was unrecognized, an asynchronous URL-moniker download of `base + file name` starts; the
  result is "pending".
- **State**: for every animation listed under the state, prepare the animation **and its
  return animation**.
- **WaveFile**: an absolute URL is accepted only with the HTTP scheme; a relative name is
  appended to the ACF base (`base + name`), apparently only for characters opened in URL
  mode. Downloaded the same way, stored in the IE cache.
- The data provider's load path only reads from the cache and never starts a download by
  itself. The SDK says `Play` of an animation that is not loaded fails with Request status
  "failed". Whether agentsvr.exe triggers a prepare on its own was not traced (section 14).

### 10.4 Using what is cached (agentdpv 0x5a967c0a, 0x5a968089)

There is an "is this animation in the cache" probe. When the server asks for a random
animation of a state (idling), the provider picks a random member and, if that one is not
cached, walks the list cyclically to the next member that is cached; if none is, it returns
nothing. So partially downloaded characters still idle with what they have.

### 10.5 Caching and staleness

All downloaded files live in the IE cache. The ACF checksum per animation is what detects
a stale cached ACA: mismatch makes the check fail and triggers a new download.

## 11. Differences from ACS (summary)

| Aspect | ACS (2.0, magic 0xABCDABC3) | ACF + ACA |
|---|---|---|
| Files | one file with locator tables | one ACF plus one file per animation |
| Compression | per image (optional) and per region (optional); header and tables uncompressed | whole ACF body always compressed; whole ACA body compressed or raw (flag); nothing compressed inside |
| Strings | u32 length + chars + NUL terminator | u32 length + chars, **no terminator** |
| Character info order | version, localized-info locator, GUID, size, ..., states, localized info at the end | version, animation table, GUID, localized info, size, flags, ..., states |
| Animation list | name + locator into the same file | name, file name, return animation, checksum |
| Transition type and return animation | inside the animation record | transition in the ACA, return animation in the ACF table |
| Images | shared pool for the whole character, each with own width/height, frames composite several images with x/y offsets | pool per animation, every image is a full pre-composited frame (character width/height), one image per frame, identical images deduplicated within the animation |
| Image record | flag, width, height, compression flag, data, region (maybe compressed) | size, unknown byte, raw pixels, raw region |
| Sounds | shared pool with locators | pool per animation, inline RIFF WAVE |
| Frame record | image list, sound, duration, exit, branches, overlays | image index, sound, duration, offset X/Y (unused), exit, branches, optional base image, overlays |
| Overlays | reference an image index; ACS w/h is a point (per ACS researcher) | carry inline pixels; x, y, w, h is a real rectangle; optional region |
| Integrity | CRC-32 per image and sound | opaque per-animation checksum, ACF vs ACA |
| Data provider | agentdp2.dll | agentdpv.dll |

The same character content is in both: Merlin's GUID, palette, states, transition types,
return animations, frame counts, durations, exits, branches, sounds and composited pixels
match `merlin.acs` exactly (section 12).

## 12. Verification results

Throwaway parser: `scripts/agentlz.py` (decompressor), `scripts/acfparse.py` (ACF and ACA
parser, asserts every byte is consumed), drivers `run.py`, `rgncheck.py`, `xcheck.py`,
`xcheck2.py`, `ovl.py` in
`$BENTWIRE_REFERENCE/`.
All run with `python3 -I`.

| Check | Command | Result |
|---|---|---|
| ACF full parse | `run.py <char>` for genie, merlin, peedy, robby | 4/4 ACF: decompressed size matches header, body consumed exactly |
| ACA full parse | same | 86/86 ACA (12 + 45 + 13 + 16): header checksum == ACF checksum, decompression exact, body consumed exactly, all indices in range |
| Image size | same | 796/796 images are `stride * height` of the ACF size |
| Regions | `rgncheck.py` (all 4 chars) | 796/796 regions == set of non-transparent pixels (top-down) |
| Overlay sizes | `run.py` | 329/329 overlays: size == `((w+3)&~3) * h` |
| ACS cross-check, frames | `xcheck.py $BENTWIRE_SOURCES/merlin.acs merlin <acs tools dir>` | GUID, palette, states equal; 444/444 frames with an image are pixel-identical to the ACS frame composited with the **last listed image drawn first**; 0 metadata mismatches (transition, return, frame count, duration, exit, branches) |
| ACS cross-check, sounds and overlays | `xcheck2.py` (same args) | 30/30 sound uses byte-identical; 147/147 overlays have identical opaque pixels and positions |
| Visual | rendered a few frames and an overlay composite to PNG (kept out of the repo) | correct orientation and colors, mouth placed on the face |

Notes: the ACS side of the cross-check used the ACS researcher's parser read-only. The two
frame orders tested were "list order" and "reversed"; only reversed matches.

## 13. Evidence

Binaries (static analysis only, never executed). Decompiled with the REA CLI
(`rea decompile-all`, Ghidra 12.1.4) into
`.../scratchpad/acf/rea/{agentdpv,agentctl,ace}/functions/`. The ACS researcher's earlier
REA export of agentdpv.dll (same SHA-256) was read too; all 382 function bodies are identical
to this export (pseudocode compared by hash). Two instruction-level facts (the zone dialog's
Yes/No return value at 0x5a964271..0x5a964281 and the seek of -4 bytes from the current
position at 0x5a967974..0x5a96798e) come from an objdump listing of the same binary. The shared
REA MCP session was used only for orientation.

| Binary | SHA-256 |
|---|---|
| agentdpv.dll 2.00.0.3427 | `66a6765e2693b05c7b1bb72973b32264656ffbba7c176f8a86ce0edc8358875c` |
| agentctl.dll 2.00.0.3427 | `b50ca2fdd6c41b9c7637c486a49f044286f19f83b7d9fc97cce3eb2e21ae315c` |
| ACE.EXE (Character Editor, 1998) | `1f2856e79f764eb8a6dfb259cc47240e929db412e10ba512790d69367cef20a2` |

agentdpv.dll (image base 0x5a960000):

| Address | Role |
|---|---|
| 0x5a966aff | open: URL/file classification, scheme whitelist, base directory |
| 0x5a966fb6 | load: start async ACF download (URL) or parse now (file) |
| 0x5a966d0d | ACF: read magic (C2/C4), decompress, zone check, parse |
| 0x5a964d61 | OLE storage variant: `char.acf` stream, magic C1 |
| 0x5a962ba2 | "u32 uncompressed, u32 compressed, data" block -> memory stream |
| 0x5a962a17 | Agent decompressor |
| 0x5a96a5ee | ACF body parser (version gates) |
| 0x5a969a18 | balloon block |
| 0x5a96a472 | states |
| 0x5a963056 | string reader (no terminator) |
| 0x5a969bd8 / 0x5a969c22 / 0x5a969d18 | ACF entry lookup: file name / return animation / checksum |
| 0x5a9675dc, 0x5a967804 | ACA location = base + file name; open from cache or disk |
| 0x5a9678c0 | ACA header, checksum compare, compression flag (URL/file path) |
| 0x5a965373 | ACA stream in OLE storage (checksum not compared) |
| 0x5a968a49 | animation object, takes the ACF version, width, height, transparent index |
| 0x5a9695d7 | ACA body: sounds, images, transition, frames |
| 0x5a968ae5 | frame record |
| 0x5a968618 | overlay record |
| 0x5a96910f, 0x5a969432, 0x5a962ceb | frame and overlay compositing, transparent blit, regions |
| 0x5a968dcb, 0x5a968d98 | next-frame selection, exit mode |
| 0x5a96834a | leaving an animation: exit branches or return animation |
| 0x5a967e74, 0x5a967f6c, 0x5a967664 | prepare animation / state / wave file |
| 0x5a96ad8c | URL moniker async download |
| 0x5a9666c1 | read a URL from the WinINet cache |
| 0x5a967c0a, 0x5a968089 | cache probe; random cached state member |
| 0x5a964115 | security zone check |
| 0x5a96685c | tray icon from the two DIBs |
| 0x5a96b779, 0x5a96b470 | phoneme to mouth-type table and lookup |

agentctl.dll: 0x71225370 (`Get`: ANIMATION / STATE / WAVEFILE -> Prepare type 0 / 1 / 2).

ACE.EXE: 0x40ac90 (build ACF, version 0x20001, magic 0xABCDABC4), 0x407750 (write ACA:
version, checksum, compression flag, sounds, images, transition, frames), 0x408010 (images,
compositing and deduplication), 0x410a90 (frame writer: image, sound, duration, offset X,
offset Y, exit, branches), 0x411230 (frame text with `Duration`, `OffsetX`, `OffsetY`,
`ExitBranch`, `DefineMouth`, `ReplaceBaseFrame`), 0x416160 (overlay writer, constant 0
reserved byte), 0x408c70 / 0x4084d0 / 0x429bd0 (checksum = CRC-32 of the animation text).

Sample offsets worth checking by hand: `genie/blink.aca` header bytes
`01 00 02 00 | 0a c8 38 25 | 01 | 3a d1 00 00 | 4e 13 00 00`; merlin.acf body starts
`01 00 02 00 49 00 08 00 00 00 52 00 ...` (version 0x20001, 73 animations, first name
"RestPose" with 8 code units and no terminator).

## 14. Unknowns and open points

- The ACA image record's single byte after the size: always 0, never read by the runtime;
  what ACE means by it is unresolved. Safe to ignore.
- ACF magic 0xABCDABC2 and ACF versions 0x1001C to 0x1001F (probably Agent 1.5, the
  ".AAF" era): parsed from the binary only, no samples. The exact 1.5 semantics of headerless
  ACAs are therefore unverified.
- OLE compound-file variant (5.4): binary only; nobody owns it yet.
- Return animation and transition type 0 never occur in the web samples (they do occur in
  ACS files, per the ACS researcher).
- "Replace base frame" overlays with a base image block and overlay regions: never seen in
  any sample; the compositing rule is from the binary.
- Frame offset X/Y: written by ACE, ignored by agentdpv; whether agentsvr uses them was not
  traced. Always 0, so ignoring them is safe for known files.
- Whether agentsvr.exe ever prepares an animation implicitly (for example on `Play`, `Show`
  or idle) was not traced; the SDK says such plays fail.
- Comma-separated `Get` names and state group names (`Idling`, `Moving`, `Gesturing`) are
  handled in agentsvr.exe, not traced here.
- 52 to 72 animations per character (all but Merlin) are not archived, so those animations
  of the web characters cannot be tested; the user's ACS files cover the same characters.
- HRESULT values above are taken from the code; their symbolic names are not in the binary.
