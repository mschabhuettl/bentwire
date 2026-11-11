# ACS character file format (Microsoft Agent 2.0)

Research for bentwire. This describes the single-file `.acs` format (signature
`0xABCDABC3`) closely enough to write a parser from scratch. Every field below is
backed by the shipped reader code, by checks against the 12 sample files, or by
both. Fields that rest on public documentation only are marked **unverified**.

## 1. Summary

- **Who reads ACS:** the reader is **agentdp2.dll** ("AgentDataProvider2",
  "Microsoft Character Animation Data Provider"), not agentdpv.dll. agentdpv.dll
  only checks the magics `0xABCDABC1` (a `char.acf` stream in an OLE structured
  storage), `0xABCDABC2` and `0xABCDABC4` (ACF). agentsvr.exe and agentanm.dll
  each read the first 4 bytes of a file. If they are `0xABCDABC3` they pick the
  provider CLSID `{D45FD301-5C6E-11D1-9EC1-00C04FD7081F}`, which agentdp2.dll
  registers (evidence in section 7). The image decompressor
  decompiles to identical code in both DLLs, so the compression section holds
  for ACF data too.
- **Overall layout:**
  - All integers are little-endian.
  - The file starts with a 36-byte header: the magic plus four (offset, size)
    locators. They point to the character info, the animation list, the image
    list and the audio list.
  - Images and sounds are separate records that animations reference by index.
  - Strings are length-prefixed UTF-16LE.
  - Pixels are 8-bit palette indices: bottom-up DIB rows, each row padded to
    4 bytes.
  - Each image carries a Win32 RGNDATA region that equals its non-transparent
    pixel mask.
  - Images and regions are compressed with a bit-level LZ77 variant. The
    algorithm is given as pseudocode in section 4.
  - Audio is a complete RIFF/WAVE file per entry, either PCM or MS ADPCM.
- **Verification:** the throwaway parser walks every byte of all 12 files.
  - No gaps, no overlaps and no leftover bytes in any record.
  - All 7,656 images decompress to exactly `stride*height` bytes.
  - All 7,656 regions decompress to their declared size and have valid RGNDATA
    headers.
  - All 495 animations (13,361 frames) and all 211 RIFF sounds parse.
- **Corrections to the best public spec** (Lebeau v1.3, 2009). Details are in
  section 9.
  - Wrong voice flag bit.
  - Wrong RGBQUAD byte order.
  - The overlay image index is a 32-bit field.
  - The overlay "width/height" is really the mouth centre point.
  - Exit index -1 vs -2 semantics.
  - Balloon colours are most likely COLORREFs, not RGBQUADs.
  - The decoder has validation rules the spec does not mention.
- **Confidence:** high for layout, decompression, compositing order, branching
  and exit semantics, and lip-sync mouth selection. Medium for the meaning of a
  few flag bits. The per-image "checksum" dword is unknown, but it is never read
  by the runtime, so it is safe to ignore.

Parser and tools (throwaway, scratchpad):
`$BENTWIRE_REFERENCE/acs.py`
(run as `python3 -I acs.py FILE...`), plus `checks.py` (semantic cross-checks) and
`render.py` (frame to PNG) in the same directory.

## 2. Conventions

| Type | Size | Notes |
|---|---|---|
| u8 / u16 / u32 | 1 / 2 / 4 | little-endian, unsigned |
| i16 | 2 | signed (frame and overlay offsets) |
| GUID | 16 | Win32 layout: u32, u16, u16, u8[8] |
| STRING | 4 + 2(n+1), or 4 | see below |
| COLORREF | 4 | 0x00BBGGRR (bytes R, G, B, 0) |
| RGBQUAD | 4 | bytes B, G, R, 0 |
| LOCATOR | 8 | u32 absolute file offset, u32 byte size |

**STRING** (agentdp2 `0x7120489d`):
- `u32 n` is a count of UTF-16 code units, not including the terminator.
- If `n == 0`, the string is just these 4 bytes: no terminator and no data. The
  runtime stores a NULL pointer, so an empty string and a missing string are the
  same thing.
- If `n > 0`, `n+1` UTF-16LE code units follow, and the last unit must be
  `0x0000`. Otherwise the file is rejected.
- Values of `n > 0x1000000` are rejected.

**Bounds rule:**
- Every primitive read inside the character-info block or an animation record
  checks `cursor + size <= block_end`. Failure gives `0x8007006F`, which the
  caller turns into `0x80070570` (ERROR_FILE_CORRUPT).
- A block given by a locator is accepted only if `offset + size` does not
  overflow 32 bits and is `<= file size` (`0x71204930`/`0x71204905`).
- A parser should apply the same checks and treat a failure as a corrupt file.

## 3. Format specification

### 3.1 Header (offset 0, 36 bytes)

agentdp2 `0x71207a0b` reads exactly 36 bytes. A short read or a wrong magic gives
`0x80070002`.

| Off | Type | Field |
|---|---|---|
| 0x00 | u32 | magic `0xABCDABC3` |
| 0x04 | LOCATOR | character info block |
| 0x0C | LOCATOR | animation list |
| 0x14 | LOCATOR | image list |
| 0x1C | LOCATOR | audio list |

The whole file is memory-mapped and the four blocks are parsed in this order:
char info, animation list, image list, audio list. Animations, images and sounds
are decoded lazily on use.

**Physical order:** all 12 files share the same order on disk, but a reader must
not rely on it.
- header
- animation records
- image records
- audio records
- animation list
- image list
- audio list
- character info (last)

### 3.2 Character info block (`0x71206c43`)

The block is copied to the heap and read sequentially from its start. Optional
parts depend on the flags.

| # | Type | Field | Notes |
|---|---|---|---|
| 1 | u32 | version | Low u16 = minor, high u16 = major. Accepted values are only `0x0001001F` and `0x00020001`. Below `0x1001F` returns `0x80042202`, above `0x20001` returns `0x80042203`, and 0 is corrupt. The extra comparisons with 0x1001C and 0x1001E in the binary are unreachable. All samples are `0x20001` (2.1). |
| 2 | LOCATOR | localized info | See 3.2.6. The runtime does **not** seek here. |
| 3 | GUID | character GUID | The runtime records its file offset (block offset + 8). |
| 4 | u16 | width | Character frame width in pixels. |
| 5 | u16 | height | Character frame height. |
| 6 | u8 | transparent index | Palette index treated as transparent. |
| 7 | u32 | flags | See 3.2.1. |
| 8 | u16 | anim set major | Always 2 in the samples. Meaning **unverified**. |
| 9 | u16 | anim set minor | Always 0 in the samples. |
| 10 | VOICE | only if `flags & 0x20` | 3.2.2 |
| 11 | BALLOON | only if `(flags & 0x100) == 0` | 3.2.3 |
| 12 | u32 + RGBQUAD[n] | palette | n = 256 in 11 files, 255 in qmark. Reserved byte 0 in all files. |
| 13 | u8 | tray icon present | 0 or nonzero |
| 14 | TRAYICON | only if 13 != 0 | 3.2.4 |
| 15 | u16 + STATE[n] | states | 3.2.5 |
| 16 | u16 + LOCALIZED[n] | localized info | 3.2.6 |

The block must be consumed exactly. In all 12 files the localized locator points
at item 16 and ends at the block end.

#### 3.2.1 Flags (u32)

| Bit mask | Evidence | Meaning |
|---|---|---|
| 0x00000020 | Gates voice parsing; getter `0x71207d33` returns bit 5 | Voice output (TTS) info present / enabled. |
| 0x00000100 | Gates balloon parsing; all balloon getters return `E_FAIL` when set | Word balloon disabled (no balloon info stored). |
| 0x00000200 | Set in every file that has a balloon | Balloon enabled (public spec). **Unverified** in code. |
| 0x000F0F00 | Getter `0x71207ec4` returns `flags & 0xF0F00` | Balloon style group, interpreted by the server. |
| 0x00010000 | rover, merlin | "Size to text" balloon style (public spec, **unverified**). Lebeau also lists 0x20000 auto-hide off and 0x40000 auto-pace off; neither appears in the samples. |
| 0x00100000 | Getter `0x71208194` returns bit 20; merlin only | "Standard animation set supported" (public spec). |
| 0x00000010 | Set exactly when 0x20 is clear, in all 12 files (complementary pair, like 0x100/0x200); no consumer found in agentdp2 | Probably "voice disabled" (inferred). |

Observed values:
- 0x210 in 9 files
- 0x120 in earl (voice, no balloon)
- 0x10210 in rover
- 0x110220 in merlin

#### 3.2.2 Voice info (present if flags & 0x20)

| Type | Field |
|---|---|
| GUID | TTS engine ID |
| GUID | TTS mode ID |
| u32 | speed |
| u16 | pitch |
| u8 | extra present (nonzero = yes) |
| u16 | (extra) language ID |
| STRING | (extra) dialect |
| u16 | (extra) gender (earl 1, merlin 2) |
| u16 | (extra) age |
| STRING | (extra) style |

If voice is absent, the getters return GUID_NULL for both GUIDs and 0 for speed
and pitch (`0x71207d59`, `0x71207d8c`, `0x71207dbf`, `0x71207def`).

#### 3.2.3 Balloon info (present if (flags & 0x100) == 0)

| Type | Field | Notes |
|---|---|---|
| u8 | lines | The getter (`0x71207ee9`) maps 0 to **2**. |
| u8 | chars per line | The getter (`0x71207f23`) maps 0 to **28**. |
| u32 | foreground colour | COLORREF |
| u32 | background colour | COLORREF |
| u32 | border colour | COLORREF |
| STRING | font face | |
| u32 | font height | Treat as i32 (LOGFONT lfHeight; all samples -13 or -15). |
| u32 | font weight | 400 in all samples. |
| u8 | italic | getter `0x7120805c` |
| u8 | second font flag | getter `0x7120808f`; 0 in all samples. Name not proven: underline or strikeout. |

The getters return the colours unchanged. They are most likely COLORREF, not
RGBQUAD (inferred from the values; the consumer in agentsvr was not traced). The
common background `0x00E1FFFF` decodes to R=FF G=FF B=E1, the familiar pale
yellow balloon; read as RGBQUAD it would be cyan.

#### 3.2.4 Tray icon (if flag byte != 0)

| Type | Field |
|---|---|
| u32 | size of the mask DIB |
| bytes | mask DIB: BITMAPINFOHEADER + colour table + bits (1 bpp) |
| u32 | size of the colour DIB |
| bytes | colour DIB: BITMAPINFOHEADER + colour table + bits |

`0x71206926` builds the icon this way:
- The colour table length is taken as `1 << biBitCount`; biClrUsed is ignored.
- The bits start right after the colour table.
- `biHeight` is the real height, not doubled as in `.ico` files.

merlin is the only sample with an icon: a 16x16 1 bpp mask of 112 bytes and a
16x16 4 bpp colour image of 232 bytes.

#### 3.2.5 States (u16 count, then each)

| Type | Field |
|---|---|
| STRING | state name |
| u16 | animation count |
| STRING[count] | animation names |

Lookup (`0x712063cf`) is case-insensitive (`_wcsicmp`). When a state is played,
`0x712067ec` picks one of its animations uniformly with `rand() % count`.

The special state name `"Speakable"` (`0x71208106`) works like this:
- If the current animation's current frame has mouth overlays, it keeps playing
  that animation.
- Otherwise it plays a random animation of state `"Speaking"`.

State names seen across the 12 files, with the number of files that have each:
- HIDING 12, GESTURINGLEFT 12, GESTURINGUP 12
- SHOWING 11, IDLINGLEVEL1 11
- IDLINGLEVEL2 10, IDLINGLEVEL3 10
- GESTURINGRIGHT 9, GESTURINGDOWN 9, LISTENING 9, HEARING 9
- MOVINGLEFT/RIGHT/UP/DOWN 2, SPEAKING 2

All are stored in uppercase. agentdpv.dll also contains mixed-case versions of
these names as string literals.

#### 3.2.6 Localized info (u16 count, then each)

| Type | Field |
|---|---|
| u16 | LANGID |
| STRING | character name |
| STRING | description |
| STRING | extra data |

In 6 of the 12 files (the Office characters other than LOGO), the English
"extra data" is several phrases separated by `~~`. Their
use was not traced; it is probably a list of greeting phrases.

LANGIDs vary between files:
- Some files store full IDs such as 0x0407.
- Others store primary-only IDs such as 0x0007.
- All files store English as 0x0009.

**Language selection** (`0x71207669`), for requested LANGID L and field k
(name, description or extra):
1. Return the first entry whose LANGID equals L exactly and whose field k is
   non-empty.
2. Otherwise remember the **last** entry whose primary language
   (`LANGID & 0x3FF`) equals that of L. Also remember the last entry with
   primary language 9 (English).
3. Return the primary-language match if its field is non-empty. Otherwise return
   the English one. Otherwise return an empty string.

### 3.3 Animation list (`0x7120789e`)

| Type | Field |
|---|---|
| u32 | count |
| per entry: STRING | animation name (author's case, e.g. "GestureLeft") |
| per entry: LOCATOR | animation record |

- Names go into a sorted tree compared with `_wcsicmp`, so animation lookup is
  case-insensitive.
- `_wcsicmp` folds only ASCII in the C locale. That is enough for all sample
  names.
- The name inside the record is uppercase in all 495 sample records. The list name is the
  one used for lookup.

### 3.4 Animation record (`0x712059ae`)

All reads are bounded by the record locator's size.

| Type | Field | Notes |
|---|---|---|
| STRING | name | uppercase copy |
| u8 | transition type | 0 = use return animation, 1 = use exit branches, 2 = none. Sample check: all 14 type-0 animations have a return animation, and all 45 type-1 animations have exit indices. |
| STRING | return animation | Empty means none. Played after this animation ends if the next request cannot exit gracefully (see 5.3). |
| u16 | frame count | **0 is rejected** with `0x8004200F`. |
| FRAME[count] | frames | |

#### FRAME

| Type | Field | Notes |
|---|---|---|
| u16 | image count | May be 0. |
| per image: u32 | image index | Into the image list. An index `>= image count` is silently skipped (`0x71204a20` returns "no image"). |
| per image: i16 | x offset | Relative to the top-left of the character frame; may be negative. |
| per image: i16 | y offset | |
| u16 | audio index | Into the audio list. Any value `>= audio count` (0xFFFF in practice) means no sound. |
| u16 | duration | 1/100 s per public docs; consistent with the data (mostly 10). The time base was **not traced** in the player. 0 occurs (208 frames). |
| i16 | exit index | `-1`, `-2` or a frame index. See 5.2. |
| u8 | branch count | |
| per branch: u16 | target frame | |
| per branch: u16 | probability | Percent. The sum is at most 100 in all frames (252 frames with sum = 100, 687 with less). |
| u8 | overlay count | |
| OVERLAY[n] | mouth overlays | 14 bytes each, below. |

#### OVERLAY (14 bytes)

| Off | Type | Field | Notes |
|---|---|---|---|
| 0 | u8 | mouth type | 0..6. A value above 6 rejects the animation. It is stored into a 7-slot array by type, so a later duplicate overwrites an earlier one. |
| 1 | u8 | replace top image | If nonzero, frame image #0 (the topmost) is not drawn while this overlay is shown. 0 in all samples. |
| 2 | u32 | image index | Read as **u32** by the runtime. Lebeau splits it into u16 index + u8 + "region flag" u8 and adds an optional region block; agentdp2 never reads such a block. The high 16 bits are 0 in all samples. |
| 6 | i16 | x offset | |
| 8 | i16 | y offset | |
| 10 | u16 | mouth point x | **Not a width.** `0x71204ef0` returns (x + this) as the mouth position. |
| 12 | u16 | mouth point y | Returns (y + this). |

Evidence that bytes 10..13 are a point and not a size: in 275 of 291 overlay
records the value pair equals the centre of the overlay image's opaque bounding
box within 1 px. Merlin, for example, has (65,55) in a 128x128 image whose
opaque box is 50..79 x 43..66. If a frame has no overlays, the same getter
returns (width/2, height/2) of the character. The consumer (which uses this
point) is in agentsvr and was not traced; it is probably balloon or mouth
anchoring.

Sample notes: overlays exist only in earl (7 frames), merlin (34) and qmark (5).
Most frames carry all 7 types; 6 frames carry fewer.

### 3.5 Image list (address from header)

| Type | Field |
|---|---|
| u32 | count |
| per entry: LOCATOR | image record |
| per entry: u32 | "checksum": unknown; no read of it found in agentdp2 (section 8) |

Entry i starts at `list + 4 + 12*i`. `0x71204a20` and `0x71204d8b` only touch
entry bytes +0 and +4.

### 3.6 Image record (`0x71204a20`)

| Off | Type | Field | Notes |
|---|---|---|---|
| 0 | u8 | kind | 0 = no image: the frame layer is skipped and contributes no region. 1 = image. Other nonzero values reach a path that sets the size but never fills pixels (reserved; treat as an error). 1 in all 7,656 sample images. |
| 1 | u16 | width | |
| 3 | u16 | height | |
| 5 | u8 | compressed | 1 = compressed, 0 = raw. Other values: pixels are never filled and the region is not parsed (treat as an error). |
| 6 | u32 | data size | Byte count of the pixel data that follows. |
| 10 | bytes | pixel data | |
| ... | u32 | region compressed size | 0 means the region is stored raw. |
| ... | u32 | region uncompressed size | |
| ... | bytes | region data | `compressed size` bytes, or `uncompressed size` bytes if raw. |

**Pixel buffer:**
- 8 bpp palette indices.
- Row stride = `(width + 3) & ~3`.
- Rows are stored **bottom-up**, as in a Win32 DIB. Confirmed by rendering
  Clippit and Dot frames.
- Decoded size is exactly `stride * height`.
- The runtime refuses an image buffer above 10,000,000 bytes.

**Raw images:** the runtime **ignores the data size** and copies
`stride*height` bytes from +10. It then expects the region block right after
those bytes, at `+10 + stride*height`, not at `+10 + data size`. No sample image
is raw. A writer should make data size equal `stride*height`.

**Region data:**
- A Win32 RGNDATA: header `dwSize=32, iType=1 (RDH_RECTANGLES), nCount,
  nRgnSize, rcBound`, followed by `nCount` RECTs (i32 left, top, right, bottom).
- RECTs are in **top-down** image coordinates.
- In all 480 checked images (the first 40 of each file), the union of the
  rectangles equals exactly the set of pixels whose index is not the
  transparent index.
- So the region is redundant for rendering. On Wayland the input/opaque region
  can come from either source.
- All 7,656 regions in the samples are compressed. All have `32 + 16*nCount ==
  uncompressed size`.
- When the region is compressed, the runtime decompresses into a reused buffer,
  passing the buffer **capacity** and not the declared size. It then hands
  `uncompressed size` bytes to ExtCreateRegion. A short stream therefore leaves
  stale bytes.

Image sizes may differ from the character size. 954 of 961 DOT images are
partial sprites, and the largest is 137x121 on a 124x93 character.

### 3.7 Audio list and records

The audio list has the same layout as the image list: u32 count, then
12-byte entries (LOCATOR, u32 unknown "checksum").

The record is a complete RIFF/WAVE file. The runtime copies the bytes as they
are (`0x71204d8b`) and loads winmm.dll and msacm32.dll for playback
(`0x7120589c`).
- In all 211 sample sounds, `RIFF length + 8 == record size`.
- Formats, as (format tag, channels, rate, bits) with counts:
  - MS ADPCM (tag 2): (2,1,22050,4) x90, (2,1,11025,4) x32, (2,2,44100,4) x6,
    (2,1,44100,4) x6, (2,1,8000,4) x6. 140 in total.
  - PCM (tag 1): 71 in total, 8 or 16 bit, 8000 to 44100 Hz. Includes one odd
    rate of 22254 Hz.
- **bentwire needs an MS ADPCM decoder.**

## 4. Compression algorithm

The same routine is used for image pixels and regions. It is in agentdp2
`0x712023e0` and agentdpv `0x5a962a17`; the two decompile to identical code.
It is an LZ77 variant with a bit-level code. The following is a description in
our own words, not decompiled code.

**Bit reader:** bits are consumed least-significant-bit first. Stream bit `k` is
bit `(k mod 8)` of byte `k div 8`. A multi-bit field of width `w` read at bit
position `p` has value `sum(bit(p+i) << i)`, so the first bit read is the least
significant. The stream starts at **byte 1**, i.e. bit position 8.

**Container validation** (all done before decoding; any failure means
"decompression failed"):
- input length must be at least 8 bytes;
- the last 6 bytes must all be `0xFF`;
- byte 0 must be `0x00`.

**Decoding loop:**

```
out = empty; pos = 8
loop:
    if read(1) == 0:                       # literal
        if len(out) == capacity: FAIL
        append read(8); continue
    # back-reference: distance class
    if   read(1) == 0: dist = read(6)  + 1;     bonus = 0
    elif read(1) == 0: dist = read(9)  + 65;    bonus = 0
    elif read(1) == 0: dist = read(12) + 577;   bonus = 0
    else:
        v = read(20)
        if v == 0xFFFFF: SUCCESS (result length = len(out))
        dist = v + 4673;                        bonus = 1
    # length: unary prefix n (count of 1-bits, terminated by a 0-bit), then n bits
    n = 0
    while read(1) == 1:
        n += 1
        if n == 12: FAIL                        # at most 11 ones allowed
    length = 2 + bonus + (2^n - 1) + read(n)    # read(0) = 0; range 2..4097
    if length > capacity - len(out) or dist > len(out): FAIL
    repeat length times: append out[len(out) - dist]   # byte-wise, overlap allowed
```

Code sizes: a literal is 9 bits. A back-reference distance code is 8, 12, 16 or
24 bits, followed by a length code of `2n+1` bits.

**Edge cases and behaviour to reproduce or decide:**

- **Return value ignored:** the image caller (`0x71204a20`) ignores the
  decompressor's result. A failed or short stream leaves the rest of the output
  buffer with whatever it held before. That buffer is reused between images, so
  stale pixels can appear.
  - Recommendation: zero-fill or reject. Every sample stream decodes cleanly,
    so this never happens with valid files.
- **Capacity overflow is a failure, not truncation.** A literal when the buffer
  is full fails, and so does a copy longer than the remaining space.
- **Reading past the end:** the reader fetches a 32-bit little-endian window at
  the current byte, so it can read up to 3 bytes past the logical position.
  Nothing during decoding checks the input length. The 6-byte `0xFF` trailer is
  what normally stops it, because an all-ones bit run always decodes as the end
  marker.
  - A safe reimplementation must bounds-check input reads, for example by
    treating missing bytes as `0xFF`.
- **End marker position:** the leading `0x00` byte is not part of the bit
  stream. In all 15,312 sample streams (7,656 image and 7,656 region streams),
  exactly 3 bytes follow the byte that holds the end marker's last bit. The same
  holds for Lebeau's 19-byte example (marker ends in byte 16). The decoder does
  not check this; it only requires the 6-byte `0xFF` tail.
- **Successful length:** the decoder reports the number of bytes written. The
  image caller does not compare it with `stride*height`; the bentwire parser
  should.
- **Public example:** Lebeau's 19-byte region example decodes to the expected
  32-byte RGNDATA with this algorithm. Lebeau's prose matches it; only the
  validation and failure rules above were missing.

## 5. Runtime semantics needed for faithful playback

These are behaviours of the data provider (agentdp2), which the player drives
frame by frame. The server-side state machine in agentsvr.exe is a separate
topic.

### 5.1 Frame compositing (`0x712053fc`, blits `0x71205342`, `0x712022ee`, `0x71202338`)

1. Fill a `stride*height` 8-bit canvas, in character size and bottom-up, with
   the transparent index.
2. Choose the mouth overlay, if any (5.4). If it has the replace flag set,
   exclude frame image #0.
3. Draw frame images **from the last listed to the first**, so image #0 ends up
   on top.
   - Each image is placed with its top-left corner at (x, y) in top-down
     coordinates and clipped to the character rectangle.
   - The first image drawn is copied opaquely. Every later one is a colour-keyed
     copy that skips pixels equal to the transparent index.
   - Fast path, which gives the same result: if the frame has exactly one image
     at (0,0) and it is character-sized, decode it straight into the canvas.
4. Draw the overlay image last, colour-keyed, at the overlay's (x, y).
5. Window region:
   - The frame region is the union of every drawn image's region, each offset
     by its (x, y). It is not clipped to the character rectangle.
   - The overlay's region is fetched only when its replace flag is nonzero. It
     then goes to a separate output: overlay region (offset) OR frame region,
     AND the character rectangle.
   - With replace = 0, which covers every sample, the window region is just the
     union of the image regions. Which output the server uses when replace is
     nonzero was not traced.

Images whose kind byte is 0 are skipped entirely.

### 5.2 Next frame, branching and exit (`0x71204fee`, `0x712052cf`, `0x71205261`)

**Choosing the next frame after frame `cur`:**

1. **Exit mode, if set:**
   - `exit == -1`: end the animation now.
   - `exit >= 0`: next frame = exit.
   - `exit == -2`: no exit jump; continue with normal sequencing below.
2. **Normal sequencing**, when step 1 did not pick a frame:
   - If the frame has branches: draw `r = rand() % 100 + 1` (1..100, held in a
     byte). Walk the branches in order. Take the first one where
     `r <= probability` and `target < frame count`. Otherwise do
     `r -= probability` and continue; the subtraction wraps as an 8-bit
     unsigned value.
   - If no branch is taken: `next = cur + 1`. If `next >= frame count`, the
     animation ends.
   - Any chosen index (exit or branch result) that is negative or
     `>= frame count` also ends the animation.
3. **Skip empty frames:** if `next` has no images **and** a duration of 0, repeat
   the procedure from `next`. After 32 iterations, return the last index anyway.

**Start frame:** frame 0, unless it has no images and a duration of 0, in which
case apply the procedure above from 0.

**Entering exit mode** (when a new animation is requested) is allowed if all of
these hold:
- the current frame is valid;
- its exit index is not -1;
- at least one of:
  - the animation is not at its end;
  - the transition type is 1;
  - an internal "queued" flag is set.

**At end:** the animation is "at end" if exit mode is on and the current frame is
the last one, or if the precomputed next frame is "end".

Sample counts:
- 1,704 frames have exit -1, 9,117 have -2, and 2,540 have a frame index.
- Every animation's last frame has -1 or -2, except one in merlin.

### 5.3 Switching animations (`0x71206595`)

When a new animation is requested while one is playing:
- If the current one can enter exit mode, the new one is queued and the current
  one plays out through its exit indices.
- Otherwise, if the current one is at its end and has a return animation, the
  return animation is played next and the new one is queued. If the return
  animation is the requested one, it simply plays.
- Otherwise the current one is replaced immediately.

### 5.4 Lip sync: mouth selection (`0x71204cbb`, table built in `0x712037a2`)

**Input:** the speech engine supplies one UTF-16 phoneme code unit at a time. It
is either an IPA code point or an ASCII letter.

**Mapping:** a fixed table maps it to mouth type 0..6.
- If the code is not in the table, the previous mouth type is kept.
- Initially the previous mouth is -1, meaning no overlay.
- If the current frame has no overlays, or none of the chosen type, no overlay
  is drawn.

The table below is reproduced in full because playback depends on it. It
consists of constants taken from agentdp2.dll, not character data; strip it if
the project treats such constants as Microsoft data.

| Mouth type | Phoneme code points (char) |
|---|---|
| 0 (closed) | U+0000, U+0020, `_`, b, m, p, ɓ, ɸ, ʘ, β |
| 1 | f, i, n, v, ɨ, ɩ, ɪ, ɯ, ɱ, ʇ, ʋ |
| 2 | c, d, e, k, l, q, s, t, z, ð, ŋ, ɖ, ɗ, ɘ, ɟ, ɡ, ɢ, ɬ, ɭ, ɮ, ɲ, ɳ, ɺ, ɽ, ɾ, ɿ, ʂ, ʈ, ʐ, ʔ, ʗ, ʣ, ʥ, ʦ, ʨ, θ |
| 3 | j, x, ç, ɑ, ə, ɛ, ɠ, ɣ, ɥ, ɧ, ɰ, ɴ, ʀ, ʁ, ʆ, ʌ, ʎ, ʓ, χ |
| 4 (wide) | a, h, æ, ħ, ɜ, ɦ, ʕ, ʖ |
| 5 | œ, ɒ, ɔ, ɕ, ɚ, ɝ, ɞ, ɤ, ɫ, ɶ, ɹ, ɻ, ʃ, ʄ, ʅ, ʑ, ʒ, ʤ, ʧ |
| 6 (rounded) | o, r, u, w, y, ø, ɵ, ɷ, ɼ, ʉ, ʊ, ʍ, ʏ |

The table has 116 entries in total. Lebeau's names are 0 closed, 1-4 "wide open
1-4", 5 medium, 6 narrow. They fit the table (bilabials map to 0, rounded vowels
to 6, "a" to 4), but they are names, not behaviour.

## 6. Per-file verification results

Command: `python3 -I acs.py $BENTWIRE_SOURCES/*.ACS $BENTWIRE_SOURCES/*.acs`
(output saved as `tools/all.txt`). Result: exit 0, 0 anomalies, and `gaps: []`
and `overlaps: []` for all 12 files. `checks.py` reports the cross-checks quoted
in this document (output in `tools/checks.txt`).

| File | Bytes | WxH | Transp | Flags | Pal | Icon | Voice | Balloon | Langs | States | Anims | Frames | Images | Sounds | Overlay frames | Exit -1/-2/idx | Trans 0/1/2 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| CLIPPIT.ACS | 2904417 | 124x93 | 253 | 0x210 | 256 | n | n | y | 30 | 11 | 43 | 1233 | 902 | 15 | 0 | 20/946/267 | 0/0/43 |
| DOT.ACS | 555163 | 124x93 | 254 | 0x210 | 256 | n | n | y | 30 | 11 | 36 | 887 | 961 | 18 | 0 | 38/711/138 | 0/0/36 |
| F1.ACS | 2554070 | 124x93 | 253 | 0x210 | 256 | n | n | y | 30 | 11 | 48 | 1560 | 897 | 31 | 0 | 11/1272/277 | 0/0/48 |
| LOGO.ACS | 1030546 | 124x93 | 253 | 0x210 | 256 | n | n | y | 30 | 11 | 38 | 934 | 259 | 10 | 0 | 9/859/66 | 0/0/38 |
| MNATURE.ACS | 1530968 | 124x93 | 253 | 0x210 | 256 | n | n | y | 30 | 11 | 38 | 1768 | 477 | 7 | 0 | 123/1546/99 | 0/0/38 |
| OFFCAT.ACS | 2071708 | 124x93 | 253 | 0x210 | 256 | n | n | y | 30 | 11 | 50 | 1219 | 727 | 21 | 0 | 9/1027/183 | 0/0/50 |
| ROCKY.ACS | 3006178 | 124x93 | 253 | 0x210 | 256 | n | n | y | 30 | 11 | 46 | 2170 | 984 | 34 | 0 | 246/1451/473 | 0/0/46 |
| courtney.acs | 816535 | 80x80 | 10 | 0x210 | 256 | n | n | y | 26 | 7 | 34 | 490 | 342 | 4 | 0 | 248/192/50 | 7/0/27 |
| earl.acs | 1472718 | 80x80 | 10 | 0x120 | 256 | n | y | n | 26 | 9 | 47 | 881 | 652 | 22 | 7 | 581/248/52 | 7/0/40 |
| merlin.acs | 2180663 | 128x128 | 10 | 0x110220 | 256 | y | y | y | 30 | 16 | 73 | 945 | 614 | 32 | 34 | 77/75/793 | 0/37/36 |
| qmark.acs | 1174050 | 200x157 | 249 | 0x210 | 255 | n | n | y | 1 | 10 | 13 | 168 | 138 | 7 | 5 | 68/45/55 | 0/8/5 |
| rover.acs | 1861820 | 80x80 | 10 | 0x10210 | 256 | n | n | y | 26 | 5 | 29 | 1106 | 703 | 10 | 0 | 274/745/87 | 0/0/29 |

All files have version 0x20001 and animation set 2.0.

**Totals:** 495 animations, 13,361 frames, 7,656 images (all compressed, all
regions compressed), 211 sounds.

**Other per-file observations:**
- **Every image and every sound is used** by at least one frame or overlay.
- **States:**
  - courtney has 7 states, earl 9 and rover 5. They lack some of the standard
    states: none of the three 80x80 characters has GESTURINGRIGHT or
    GESTURINGDOWN, and rover has only IDLINGLEVEL1.
  - merlin and qmark are the only files with MOVING* and SPEAKING states.
  - qmark has no SHOWING or IDLING states.
  - The player must handle missing states.
  - Per-file states, as name(animation count):
    - CLIPPIT: IDLINGLEVEL1(3) IDLINGLEVEL2(4) IDLINGLEVEL3(6) HIDING(1)
      GESTURINGLEFT/RIGHT/UP/DOWN(1 each) LISTENING(1) HEARING(1) SHOWING(1)
    - DOT: SHOWING HIDING IDLINGLEVEL1/2/3 GESTURINGLEFT/RIGHT/UP/DOWN
      LISTENING HEARING (1 each)
    - F1: SHOWING(1) HIDING(1) IDLINGLEVEL1(8) IDLINGLEVEL2(10)
      IDLINGLEVEL3(6) GESTURING*(1 each) LISTENING(1) HEARING(1)
    - LOGO: SHOWING(1) HIDING(1) IDLINGLEVEL1(2) IDLINGLEVEL2(3)
      IDLINGLEVEL3(3) GESTURING*(1 each) LISTENING(1) HEARING(1)
    - MNATURE: SHOWING(1) IDLINGLEVEL1/2/3(2 each) HIDING(1) LISTENING(1)
      HEARING(1) GESTURING*(1 each)
    - OFFCAT: IDLINGLEVEL1(7) IDLINGLEVEL2(11) IDLINGLEVEL3(12) SHOWING(1)
      HIDING(1) LISTENING(1) GESTURING*(1 each) HEARING(1)
    - ROCKY: SHOWING(1) HIDING(1) GESTURING*(1 each) LISTENING(2)
      IDLINGLEVEL2(8) IDLINGLEVEL3(2) IDLINGLEVEL1(6) HEARING(1)
    - courtney: SHOWING(1) HIDING(2) GESTURINGLEFT(2) GESTURINGUP(2)
      IDLINGLEVEL2(1) IDLINGLEVEL3(1) IDLINGLEVEL1(1)
    - earl: SHOWING(1) HEARING(1) HIDING(2) GESTURINGLEFT(2) GESTURINGUP(2)
      LISTENING(1) IDLINGLEVEL1(1) IDLINGLEVEL2(2) IDLINGLEVEL3(3)
    - merlin: SHOWING(1) HIDING(1) GESTURING*(1 each) LISTENING(1) HEARING(4)
      IDLINGLEVEL1(5) IDLINGLEVEL2(7) IDLINGLEVEL3(3) MOVINGLEFT/RIGHT/UP/DOWN(1
      each) SPEAKING(1)
    - qmark: MOVINGUP SPEAKING MOVINGDOWN HIDING GESTURINGLEFT GESTURINGRIGHT
      MOVINGLEFT MOVINGRIGHT GESTURINGUP GESTURINGDOWN (1 each)
    - rover: SHOWING(2) GESTURINGLEFT(1) GESTURINGUP(1) HIDING(2)
      IDLINGLEVEL1(1)
    - GESTURING* means all four of LEFT/RIGHT/UP/DOWN.
- **Frame contents:**
  - DOT uses up to 11 images per frame and negative offsets (33 placements).
  - ROCKY has 96 negative offsets, earl 41 and qmark 8.
- **Empty and zero-duration frames:** 107 frames have no images. 208 frames have
  duration 0; of those, 76 also have no images and are skipped by the next-frame
  logic.
- **Duplicate image records** (byte-identical records stored twice):
  - DOT 135, ROCKY 47, LOGO 26, F1 9, OFFCAT 4, earl 4, qmark 3, merlin 1.
- **qmark:**
  - 255-entry palette; the highest pixel index used is 249 (= transparent).
  - Only one language (0x0009).
  - Its balloon has custom colours and the Arial font.

**Proof render** (scratchpad only, not part of the deliverable):
- `.../scratchpad/clippit_restpose_0.png`: Clippit RestPose frame 0,
  one image.
- `.../scratchpad/dot_restpose_0.png`: Dot RestPose frame 0, 8 layered
  images.
- Both look correct, which confirms bottom-up rows, BGR palette order, the
  last-to-first draw order and colour keying.

## 7. Evidence

**Binaries:** all from the Microsoft Agent 2.0 runtime, v2.00.0.3427.
- agentdp2.dll: SHA-256 `77eb0d33243b92ba6e550df31f874f576a7e69c66687123c6514a641fe98aae1`,
  image base 0x71200000.
- agentdpv.dll: SHA-256 `66a6765e2693b05c7b1bb72973b32264656ffbba7c176f8a86ce0edc8358875c`.

**Analysis:** Ghidra 12.1.4 via REA (decompile_all), plus objdump for
instruction-level checks. Decompiled dumps are in the scratchpad under
`acs/dp2_decomp`, `acs/dpv_decomp`, `acs/dp2.asm` and `acs/svr.asm`.

| Address | Role |
|---|---|
| agentdp2 `0x71207a0b` | open, map file, read 36-byte header, check magic, dispatch block parsers |
| `0x71206c43` | character info parser (version check at `0x71206cd8`..`0x71206d0f`) |
| `0x7120489d` | STRING reader (empty = 4 bytes, terminator check, 0x1000000 limit) |
| `0x712047ed` / `0x71204827` / `0x71204862` / `0x712061a9` / `0x7120616a` | bounded u8 / u16 / u32 / GUID / n-byte readers |
| `0x71204930`, `0x71204905` | locator bounds check against file size |
| `0x7120789e` | animation list, case-insensitive name tree (`0x71206a9b`, compares `0x712061e4`/`0x7120620d`) |
| `0x7120642d` | image / audio list base pointers |
| `0x712059ae` | animation record parser (frames, branches, overlays) |
| `0x71204a20` | image record: kind, size, raw/compressed pixels, region, 10 MB cap |
| `0x712023e0` | decompressor (identical: agentdpv `0x5a962a17`) |
| `0x712053fc`, `0x71205342`, `0x712022ee`, `0x71202338` | frame compositing, clipping, colour-keyed and opaque blits |
| `0x71204fee` | next-frame selection: exit, branch probabilities, skip of empty frames |
| `0x712052cf`, `0x71205261`, `0x71206595` | exit mode, at-end test, animation switching and return animation |
| `0x71204cbb`, `0x71202c75`, `0x712037a2` | mouth type selection and phoneme table |
| `0x71204ef0` | mouth point getter (overlay x + field, else character centre) |
| `0x71204d8b`, `0x7120589c` | audio copy and winmm/msacm32 loading |
| `0x71207669` | localized string language fallback |
| `0x712063cf`, `0x712067ec`, `0x71208106` | state lookup, random animation per state, Speakable/Speaking |
| `0x71206926` | tray icon construction |
| `0x71207d33`, `0x71207ec4`, `0x71208194`, `0x71207ee9`, `0x71207f23`, `0x7120805c`, `0x7120808f`, `0x71207f5d`.. | flag and balloon getters (defaults 2 lines / 28 chars) |
| agentdpv `0x5a964d61` | `0xABCDABC1` + structured storage `char.acf` (not ACS) |
| agentdpv `0x5a966d0d` | `0xABCDABC2` / `0xABCDABC4` stream formats (not ACS) |
| agentsvr.exe `0x0101F122`, agentanm.dll `0x71253875` | sniff first 4 bytes for `0xABCDABC3` and select CLSID `{D45FD301-...}` (agentanm copies it from `0x7125130c`), the class agentdp2.dll registers |

**File offset examples (CLIPPIT.ACS):**
- character info: block `0x2BFEC5` (size `0x529C`).
- localized info: `0x2C06CF` (size 19090), ending exactly at the block end
  `0x2C5161`.
- animation list: `0x2BCE45`.
- image list: `0x2BD3C1`, size `0x2A4C` = 4 + 902*12.
- audio list: `0x2BFE0D`, size `0xB8` = 4 + 15*12.

## 8. Unknowns

| Item | What was tried | Impact |
|---|---|---|
| Image/audio list third dword ("checksum") | Searched the agentdp2 decompile for any read at entry+8: none found (only +0/+4). Compared against CRC-32 (zlib) of the record, the compressed data, decoded pixels with and without padding, the region and the record minus byte 0; Adler-32; 32-bit sum; 32-bit XOR. No match. No CRC table in any shipped binary. Byte-identical image records can carry **different** values (F1: 9 duplicates, none equal; LOGO: 26, none equal; DOT: 135, all equal), so it is not a function of the stored bytes. Probably a hash of the editor's source asset. | None for playback. A writer can store 0, but this is not tested against the runtime. |
| Flag bit 0x10 | Set exactly when 0x20 (voice) is clear in all 12 files. No consumer found in agentdp2 (getters mask 0xF0F00, bit 5, bit 20). Read as "voice disabled", inferred only. | Probably none for a player; use 0x20. |
| Meaning of balloon-style bits beyond 0x100 and 0x200 | Only the masked getter exists in agentdp2. The interpretation lives in agentsvr (not traced). | Medium; belongs to the balloon research. |
| Last balloon byte (+0xb0) | Getter exists; it is 0 in all samples. Underline or strikeout is unproven. | Low. |
| Frame duration time base | Raw u16 getter `0x71204e64`; the scaling is in the player. The data fits 1/100 s (Clippit mostly 10, LOGO mostly 5). | Confirm in the agentsvr/agentanm research. |
| Animation set version (2 x u16) | Parsed; no consumer found in agentdp2. | Low. |
| Use of the localized "extra data" (`~~`-separated phrases) | Not traced. | Low. |
| Consumer of the overlay mouth point | The getter is known; the caller in agentsvr was not traced. | Low to medium (balloon tail or mouth anchoring). |
| Overlay replace flag, raw images, region-less images, image kind 0 | Code paths described above; no sample exercises them. | Edge cases only. |

## 9. Corrections to public documentation (Lebeau, "MSAgent Character Data Specification" v1.3)

1. **GUID** is listed as ULONG, ULONG, USHORT, BYTE[8]. That is 18 bytes and
   wrong; it is the Win32 16-byte layout.
2. **RGBQUAD** is listed as R, G, B, reserved. The real order is B, G, R,
   reserved: renders are only correct this way.
3. **Voice flag** is listed as bit 4 (0x10). The bit that gates the voice
   data and the voice getter is **bit 5 (0x20)**. 0x10 is its complement in all
   12 files (set exactly when voice is absent), so it is probably a "voice
   disabled" bit, mirroring the 0x100/0x200 balloon pair.
4. **Balloon colours** are listed as RGBQUAD. They are most likely COLORREF
   (0x00BBGGRR), inferred from the values.
5. **Overlay:** the spec has a USHORT image index, an unknown BYTE, a region
   flag and an optional region DATABLOCK. The runtime reads a **u32 image
   index** and never reads an overlay region. The "width/height (divided by 2)"
   fields are a **mouth centre point** in overlay-image coordinates.
6. **Exit index** -2 is described as "unknown". It is now explained:
   - **-1:** stop when exiting.
   - **-2:** no exit jump; continue normal sequencing.
   - **index:** jump to that frame.
7. **Image "unknown" first byte:** 0 means no image, 1 means image.
8. **Missing details:**
   - accepted versions (0x1001F and 0x20001);
   - the empty-string encoding is confirmed;
   - the localized-info locator is redundant (data follows the states);
   - the decoder validation and failure rules;
   - the raw-image region offset rule;
   - branch walking and the 8-bit wrap;
   - skipping of empty frames;
   - the phoneme to mouth table.
9. **Compression:** the bit-level description and the worked example are
   correct.
