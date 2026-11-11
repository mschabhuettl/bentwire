# bentwire character format

Status: 2026-11-11. Design, prototype and measurements. The owner accepted the eight
recommendations in section 13 as written (D1 to D8, 2026-11-11); the open items left are in
section 14. Sources: `docs/research/*.md` (cited as
acs-format, acf-aaf-format, agent15-format, act-format, agent-runtime) and the prototype
(section 15, not public). All numbers were measured on the real
files; commands are given next to each table.

## 1. Summary

- A character is a folder (or the same folder as a ZIP with stored media, extension `.bwc`):
  one text manifest `character.toml`, PNG sprite sheets, sound files, an optional icon.
- **One animation model holds all four source formats.** An animation is an ordered list of
  frames; a frame has a duration in ms, a picture (layers or a named pose), an optional
  sound, an ordered branch list and, for Agent data, an exit target and mouth overlays.
  What the numbers do not carry is stated by one key, `family = "agent" | "office97"`,
  which selects how branch draws and interruption work (section 7). Probabilities are
  stored as the source's raw integers (percent for Agent, n/32768 for Office 97).
- The prototype converted **63 characters**: all 12 ACS files, 4 web characters (ACF+ACA),
  10 Agent 1.5 OLE files, all 37 distinct ACT files (11 with pictures, 26 vector actors
  graph only). 2,456 animations, 57,849 frames. The round-trip check passes for all of
  them (section 11), and 17 deliberate one-value mutations are all detected.
- **Pixels:** frames stay layered as in ACS (image pool plus per-frame layer list); sheets
  are indexed PNGs that keep the source palette order and the transparent index. RGBA is
  what loaders treat as normative, so new characters can use true alpha.
- **Size** (12 ACS files, 21.16 MB): 11.89 MB as folders with FLAC audio (0.56x), 11.23 MB
  as `.bwc` (0.53x). Every character is smaller than its ACS.
- **Load time** (compiled Rust, both sides): full load is in the same range as ACS, 90 ms
  against 110 ms summed over 12 characters; 11 characters are equal or faster, DOT is
  slower (6.2 ms against 1.2 ms). First frame is slower than ACS, which decodes one image
  lazily (0.05 to 0.18 ms): 0.7 to 4.0 ms with row streaming, 2 to 12 ms when the whole
  sheet is decoded first.
- **Audio** is stored as FLAC of the PCM that Microsoft's own codec and Agent server
  produce. That rule was taken from `msadp32.acm` and `agentsvr.exe` by static analysis
  (section 6); no common decoder reproduces it completely, sox comes closest.
- **Decided** (section 13): FLAC audio, vector actors rasterized at conversion time, TOML,
  one sheet per character by default, the `family` key.

## 2. Goals and what "lossless" means

Goals as set by the owner: lossless, open and hand-editable (text manifest with schema, PNG,
FLAC or WAV, folder or ZIP), efficient (fast to load, no larger than ACS), extensible (higher
resolution, alpha, HiDPI), testable (round-trip check).

**Lossless is defined as playback-equivalent, not byte-reversible.** A conversion is lossless
when the converted character and the source produce the same:

1. picture for every frame and every mouth variant (RGBA after compositing; palette colours
   and the transparent index included),
2. frame durations,
3. control flow: branch targets and probabilities, exit behaviour, return animations and
   transition types, states and their animation lists, Office 97 pose transitions and idle
   group, conditional branches,
4. sound per frame (as decoded PCM, section 6),
5. character data used at run time: names and descriptions in all languages, balloon
   settings, voice settings, tray icon, GUID.

It does not promise to regenerate the source file. Section 9 lists every field as kept,
derivable or dropped; section 9.5 lists what cannot be round-tripped.

Two limits are stated here because they follow from the sources, not from the format:

- **Vector (WMF) actors have no pixel-exact definition.** Office 97 rasterized them with GDI
  into an 8-bit buffer; there is no reference rasterizer (section 5.4).
- **Scaled bitmap placements in ACT** (124x93 bitmaps shown in a 166x125 window, zoom
  effects) are exact in data (native pixels and integer rectangles are kept verbatim), but
  the stretch filter of MSO97 is unverified (act-format 9). The sampling rule in 5.3 is a
  bentwire definition.

## 3. Package layout

```
pebble/                  folder form; this is what bentwire-characters would hold
  character.toml         manifest, UTF-8, required
  sheets/a.png           one or more sprite sheets
  sounds/s00.flac        sound files
  icon.png               optional tray icon
pebble.bwc               the same tree as a ZIP
```

ZIP rules: `character.toml` at the root; entry names UTF-8 with `/`, no absolute paths and no
`..`; PNG and audio entries **stored** (method 0), the manifest deflated; no encryption, no
ZIP64. A converter writes entries in a fixed order with a fixed timestamp so that output is
reproducible. Folder and ZIP are interchangeable; the prototype checks both.

## 4. Manifest

TOML 1.0. Machine-readable schema: the prototype's `character.schema.json` (JSON Schema
2020-12, applied to the parsed tree); `validate.py` adds the cross-reference rules a schema
cannot express. All 63 converted manifests validate.

| Key | Type | Meaning |
|---|---|---|
| `format` | int | Format version, 1. |
| `family` | string | `"agent"` or `"office97"`: playback rule family (section 7). |
| `character.id` | string | GUID `{...}`, optional. |
| `character.canvas` | [w, h] | Frame size in canvas units. For Agent data one unit is one pixel. |
| `character.display` | [w, h] | Default on-screen size in pixels; default = canvas. |
| `character.standard_animation_set` | bool | Agent flag 0x100000. |
| `[[character.info]]` | table array | `lang` (BCP 47), `name`, `description`, `extra`; source order kept. |
| `[balloon]` | table | `enabled`, `lines`, `chars_per_line`, `fg`, `bg`, `border` (`#RRGGBB`), `font`, `font_height`, `font_weight`, `italic`, `font_flag2`, `size_to_text`, `auto_hide`, `auto_pace`. |
| `[voice]` | table | `engine`, `mode` (GUIDs), `speed`, `pitch`, optional `language`, `dialect`, `gender`, `age`, `style`. |
| `[icon]` | table | `file`. |
| `[[sheets]]` | table array | `id`, `file`, `cell` = [w, h], `count`, optional `sizes`, `scale`, `hidpi` (section 5). |
| `[sounds]` | table | sound id = file path. |
| `[states]` | table | state name = list of animation names; one is picked uniformly at random. |
| `[poses]` | table | pose id = layer list (named pictures; required for `office97`). |
| `[office97]` | table | `rest_pose`, `idle`, `transitions`, `ids`, `strings` (section 7.3). Present exactly when `family = "office97"`. |
| `[[animations]]` | table array | `name`, `frames`, and for `agent`: `transition` (`"return"`, `"exit"`, `"none"`), `return`. |
| `[source]` | table | Provenance and source-only fields (file name, hash, format version, raw flags, unknown header words, `missing_animations`). Not used for playback. |

A frame is an inline table, one per line:

| Key | Type | Family | Meaning |
|---|---|---|---|
| `ms` | int | both | Duration in milliseconds (ACS value x 10; ACT value as is). |
| `layers` | list | both | Pictures drawn back to front. A layer is `"sheet:n"`, `["sheet:n", x, y]` or `["sheet:n", x, y, w, h]`. Absent = empty frame. |
| `pose` | int | both | Instead of `layers`: id into `[poses]`. Pose identity drives Office 97 transitions. |
| `sound` | string | both | Sound id started when the frame is entered. |
| `sound_arg` | int | office97 | Raw ACT sound argument when it is not the default (0x0080; 0x0100 for Bob files). |
| `branch` | list | both | Ordered branches: `[target, p]`, and for office97 also `[target, "daypart", v]`, `[target, "host", v]`. |
| `exit` | `"end"` or int | agent | Exit target; absent = continue normally (ACS -2). |
| `mouths` | list | agent | `{ type = 0..6, layer = ..., anchor = [x, y], base = [layers] }`; `anchor` and `base` optional. |

Example (a made-up character, cut down to three animations):

```toml
format = 1
family = "agent"

[character]
id = "{0B5E7C1A-4D2F-4E8B-9A6C-3F1D2E4B5A60}"
canvas = [96, 96]

[[character.info]]
lang = "en"
name = "Pebble"
description = "A small round stone that likes to help."

[[character.info]]
lang = "de"
name = "Kiesel"

[balloon]
enabled = true
lines = 2
chars_per_line = 28
fg = "#000000"
bg = "#FFFFE1"
border = "#000000"
font = "DejaVu Sans"
font_height = -13
font_weight = 400
italic = false
size_to_text = true
auto_hide = true
auto_pace = true

[[sheets]]
id = "a"
file = "sheets/a.png"
cell = [96, 96]
count = 24
[sheets.hidpi]
2 = "sheets/a@2x.png"

[sounds]
pop = "sounds/pop.flac"

[states]
Showing = ["Show"]
Speaking = ["Talk"]
IdlingLevel1 = ["Look"]

[[animations]]
name = "Show"
transition = "none"
frames = [
  { ms = 80, layers = ["a:0"], sound = "pop" },
  { ms = 80, layers = ["a:1"] },
  { ms = 120, layers = ["a:2"] },
]

[[animations]]
name = "Look"
transition = "exit"
frames = [
  { ms = 100, layers = ["a:2"], exit = "end" },
  { ms = 100, layers = ["a:3"], exit = 4, branch = [[1, 60], [3, 20]] },
  { ms = 400, layers = ["a:4", ["a:9", 30, 12]], exit = 4 },
  { ms = 200, layers = ["a:5"], exit = 4 },
  { ms = 100, layers = ["a:2"] },
]

[[animations]]
name = "Talk"
transition = "return"
return = "Look"
frames = [
  { ms = 100, layers = ["a:2"], exit = "end", mouths = [{ type = 0, layer = ["a:10", 40, 60], anchor = [48, 66] }, { type = 4, layer = ["a:11", 40, 60], anchor = [48, 66] }] },
]
```

Rules that apply to every manifest:

- Animation names are unique ignoring ASCII case; lookups ignore ASCII case (as agentdp2
  does). The converter writes state members with the animation's own spelling and the 16
  Agent state names in canonical mixed case.
- Frame indices (`branch`, `exit`) are 0-based positions in `frames`.
- References to animations that the source named but did not contain (web characters that
  were only partly archived, damaged 1.5 streams) are legal only when the name is listed in
  `source.missing_animations`.
- Unknown keys starting with `x-` are ignored. Other unknown keys are an error in the
  validator and a warning in the loader.

## 5. Images

### 5.1 Sheets

A sheet is a PNG holding `count` pictures in a uniform grid of `cell` size, row-major,
columns = PNG width / cell width. Picture `n` of sheet `id` is referenced as `"id:n"`.
If pictures are smaller than the cell, `sizes[n]` gives the true size and the picture sits in
the top-left corner of its cell, the rest is transparent (only DOT.ACS needs this: 954 of its
961 images are partial sprites). A character may have any number of sheets; new characters
can use one sheet per animation, which is what sprite editors export.

Converter layout (decided, section 13 D4): one sheet, as many columns as fit into 4096
pixels, pictures in order of first use with the animations of state `Showing` first. That
order lets a row-streaming PNG decoder reach the first frame after a few rows.

### 5.2 Pixel format

- **Normative is the decoded RGBA.** A loader accepts 8-bit indexed PNG (with `tRNS`) and
  8-bit RGBA PNG. Pixels with alpha 0 are equal whatever their colour.
- **Converted palette characters are written as indexed PNG:** `PLTE` is the source palette
  in source order (255 entries for qmark, 128 for Totem, as stored), `tRNS` makes exactly the
  source's transparent index transparent. Other indices with the same RGB stay opaque, as
  in the source. The round-trip check compares palette hash and transparent index, so "same
  palette and transparency" is tested, while hand-edited files that an editor re-palettized
  still load.
- Indexed sheets are also the memory format: the daemon keeps one byte per pixel plus a
  256-entry RGBA table and composites by table lookup (81.8 megapixels for all 12 ACS
  characters, i.e. 82 MB instead of 327 MB as RGBA).
- Identical pictures are stored once: 7,656 ACS image records are 7,122 distinct pictures
  after decoding.

### 5.3 Compositing (normative)

Output size `O` is `display` (or an integer multiple). Start from a fully transparent canvas.
For each layer, back to front:

1. Rectangle in canvas units: `x, y` from the layer; `w, h` from the layer or, if absent, the
   picture size divided by the sheet `scale`.
2. If `O` equals `canvas`, the layer has no explicit size and the picture is 1x: copy the
   pixels 1:1 at `(x, y)`.
3. Otherwise map the edges with `x0 = trunc(x * Ow / Cw)`, `x1 = trunc((x + w) * Ow / Cw)`
   (same for y, truncation toward zero, act-format 4.5) and fill destination pixel
   `(dx, dy)` of the `dw x dh` rectangle from source pixel
   `(floor((2*dx + 1) * sw / (2*dw)), floor((2*dy + 1) * sh / (2*dh)))` (nearest neighbour,
   pixel-centre sampling; bentwire definition).
4. Clip to the canvas. Blend source-over with straight alpha: alpha 255 replaces, alpha 0
   leaves the destination, otherwise `out_c = round((s_c*a*255 + d_c*da*(255-a)) /
   (a*255 + da*(255-a)))` and `out_a = round((a*255 + da*(255-a)) / 255)`. Converted palette
   characters only have alpha 0 and 255.

A mouth variant is the frame's layers (or the mouth's `base` list when present) followed by
the mouth layer. The window and input shape are the pixels with alpha above 0, which equals
the stored region for every ACS image (section 9.1).

### 5.4 Layers versus composed frames

The format stores **layers**, not pre-composited frames, because that is the superset:
ACS frames are layer lists (DOT uses up to 11 per frame, with negative offsets), ACT poses
are lists of placed pictures, and the pre-composited formats (ACA, 1.5) are the one-layer
case. Composed storage would not be smaller (9.73 MB against 9.47 MB at 8 columns, table in
12.2) and would lose the structure an editor wants. Mouth overlays stay separate pictures in
every source and here.

Vector actors are the exception: their poses are stored as one rasterized picture per pose
(Office 97 Clippit: 679 poses used, 496 distinct pictures), because the vector parts are
placed at 684 different sizes and cannot be reused as bitmaps (D2).

## 6. Audio (normative)

**Storage (D1).** Every converted sound is a FLAC file holding the PCM defined below, 8 or
16 bit as decoded, original rate and channel count. `[sounds]` maps an id to the file.
Loaders accept FLAC and RIFF WAVE with PCM (for hand-made characters); WAVE with MS ADPCM is
not allowed in a package. The identity of a sound, for the round-trip check and for
deduplication, is the hash of its PCM (channels, rate, bit depth, samples).

**PCM sounds** (71 of the 211 ACS sounds): the samples of the `data` chunk, unchanged.

**MS ADPCM sounds** (140 of 211, 66%) are decoded once, at conversion time, by Microsoft's
rule. It was recovered by static analysis (nothing was executed) of

- `msadp32.acm` 5.1.2600.5512 (Windows XP SP3, German), 14,848 bytes, SHA-256
  `81eac2425efa2c31edf22d4c8a87e9fdadeb380b2471779c508f00145c5dd243`, image base 0x72C60000;
- `agentsvr.exe` 2.00.0.3427, which decides how the codec is driven.

Codec, in our own words:

| Address | What it does |
|---|---|
| 0x72C61CE7 | One sample: prediction = (sample1 x coef1 + sample2 x coef2) **shifted right arithmetically by 8** (`sar`, 32-bit signed, so it rounds toward minus infinity); add the nibble, sign-extended from 4 bits, times the current delta; **clamp to -32768..32767**. |
| 0x72C61CBD | Delta update: delta = (table[nibble] x delta) shifted right arithmetically by 8; **if below 16 it becomes 16**; no upper limit. Table at 0x72C610C8: 230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230. |
| 0x72C633ED (mono), 0x72C63691 (stereo), 16-bit output | Block loop. A block is started while at least 7 bytes per channel remain; its length is the smaller of the remaining bytes and `nBlockAlign`, so **a short final block is decoded as far as it goes**. Header: per channel the predictor index (1 byte), then per channel the delta (signed 16 bit), then per channel sample 1, then per channel sample 2. Output: sample 2, then sample 1, then two samples per remaining byte, **high nibble first** (in stereo: high nibble left, low nibble right). Each sample is computed with the current delta, then the delta is updated. Coefficients come from the format header (signed 16-bit pairs). A predictor index not below `wNumCoef` makes the routine return 0 and stop. `wSamplesPerBlock` is not used here. |
| 0x72C632C7, 0x72C63505 | The same loops with 8-bit output (sample shifted right by 8, plus 0x80). Not used by Agent. |
| 0x72C61BFE | Convert request: for decoding the whole source length is passed on, unless the caller sets the block-align flag, in which case it is rounded down to whole blocks. |
| 0x72C61B04 | Size request (source to destination): **fails (0x200) when the source is shorter than one block**; otherwise (whole blocks, plus one if bytes remain) x `wSamplesPerBlock` x destination block size. |
| 0x72C6175B | Format suggestion for a PCM target: same channels and rate, 16 bit unless the caller fixes the depth. |

How Agent drives it (agentsvr.exe):

- 0x010098C2 starts a frame sound: it creates a sound object (vtable 0x01002B54), calls its
  load method 0x010151D7 and then its play method 0x010140FD; if loading fails there is no
  sound.
- 0x010151D7 opens the RIFF data in memory and reads the `fmt ` and `data` chunks only.
  **The `fact` chunk is never read.** For a non-PCM format it calls 0x01013F5A.
- 0x01013F5A: format suggestion (PCM), stream open, size request for the whole `data`
  chunk, a **zero-filled** buffer of that size, one convert call with flags 0x30 (start and
  end, **no block-align flag**). It then keeps the buffer **with the requested size as its
  length** (0x0101405D to 0x01014066), not the number of bytes the codec reports as written.
  0x010140FD plays that buffer through waveOut.

The resulting rule, which is normative for the converter (Python reference:
`common.msadpcm_decode_ms`):

1. Use the `data` chunk only; ignore `fact`. Output is 16-bit PCM with the source's channels
   and rate.
2. If `data` is shorter than `nBlockAlign`, Agent cannot load the sound and never plays it.
   The converter omits it, removes the references and lists it in
   `source.unplayable_sounds`.
3. The PCM has `ceil(data bytes / nBlockAlign) x wSamplesPerBlock` sample frames, initially
   zero.
4. Decode block after block with the arithmetic above while at least 7 bytes per channel
   remain; what a short final block does not fill stays zero.
5. A predictor index not below `wNumCoef` ends decoding; samples written before stay.

What this means for the 140 ADPCM sounds of the 12 ACS files (`measure_ms.py`,
`results/adpcm_ms.txt`):

- One sound is not playable: DOT.ACS sound 0 (134 data bytes, block size 512). 23 DOT
  animations reference it in their first frame; under Agent it was always silent.
- Against the PCM this draft used before the analysis (arithmetic shift, cut at the `fact`
  count): **all sample values are identical; 112 of the other 139 sounds become longer, 27
  are unchanged.** The added tails total 72,720 sample frames; 3,183 of those samples, in 6
  sounds, are not zero. 41 sounds grow because Agent ignores `fact` and plays the block
  padding, 71 because a short final block is padded with silence to a full block.
- No existing decoder reproduces the rule completely:

| Decoder | Sample values | Bit-exact including length | Otherwise |
|---|---|---|---|
| sox 14.7 | identical for all 139 | 68 | 71 lack the silent tail after a short final block |
| libsndfile | identical where decoded | 68 | 71 lack the short final block and the tail |
| ffmpeg 8.0 | differ in 115 (it divides and truncates toward zero) | 10 | 14 same samples but shorter |
| symphonia 0.5.5 | differ in 113 (same reason) | 10 | 16 same samples but shorter |

- rodio 0.23.0 with its default `wav` feature rejects all 140 ADPCM sounds; with
  `symphonia-adpcm` it behaves like symphonia. This no longer matters at run time: the
  daemon only decodes FLAC and PCM WAVE, and all 210 FLAC files written by the rule decode
  bit-exactly through symphonia.

Limits of this analysis: `msacm32.dll` itself was not available; it is assumed to pass the
size and convert requests to the codec unchanged. Office 97 plays ACT sounds through
`PlaySound`, a path that was not analysed. The converter applies the same arithmetic and
length rule there, but keeps sounds shorter than one block (4 in the 37 ACT files: Clippit 1,
Scribble 1, Will 2), decoded and padded to one block (section 14).

## 7. Animation model and playback semantics

### 7.1 Shared

- A request names an animation, or a state; a state picks one of its animations with a
  uniform draw (`rand() % n` in both runtimes).
- Frames play in list order unless a branch or exit changes it. Timing, queueing, idle and
  the switch between animations are runtime behaviour (agent-runtime, act-format 5) and not
  part of the file, except where a key below says so.
- All randomness is described as "a uniform integer in [0, scale)", so tests can inject
  draws.

### 7.2 Family `agent` (ACS, ACF+ACA, 1.5)

Next frame after frame `cur` (agentdp2 `0x71204fee`, acs-format 5.2):

```
next(cur, exiting):
  repeat at most 32 times:
    f = frames[cur]; t = none
    if exiting:
      if f.exit == "end": return END
      if f.exit is an index: t = f.exit
    if t is none:
      if f.branch is not empty:
        r = uniform integer in [0, 100)          # ONE draw per frame visit
        for [to, p] in f.branch, in order:
          if r < p: t = to; stop
          r = r - p                              # cumulative walk
      if t is none: t = cur + 1
    if t >= frame count: return END
    if frames[t] has no layers and ms == 0: cur = t; continue   # invisible branch point
    return t
  return t
```

- Start frame: 0, or `next(0, false)` if frame 0 is an invisible branch point.
- An invisible branch point is never shown and its sound is not started.
- A frame with layers and `ms = 0` is shown (132 such frames in the ACS files).
- `exit`: `"end"` ends the animation when exiting and also means "this frame cannot start an
  exit" (ACS -1); absent means no exit jump, normal sequencing continues (ACS -2).
- `transition` and `return` are the animation-level inputs of the switch rule in
  acs-format 5.3: exit mode may start when the current frame's `exit` is not `"end"` and
  (the animation is not at its end, or `transition = "exit"`, or a request is queued);
  otherwise a finished animation with a `return` plays that first.
- Constraints: `1 <= p <= 100`, the sum per frame is at most 100, targets are valid. Under
  these the rule equals agentdp2's `r = rand() % 100 + 1; take if r <= p` exactly. All 939
  branching frames of the 12 ACS files satisfy them (largest sum 100). agentdp2's behaviour
  for data outside them (8-bit wrap of `r`, skipping an invalid target) is not
  representable; the converter rejects such a file.
- `mouths`: type 0..6 selected by the phoneme table (acs-format 5.4). `anchor` is the mouth
  point (balloon tail target) in canvas coordinates.

### 7.3 Family `office97` (ACT and Bob actors)

```
i = 0
while i < frame count:
  f = frames[i]
  start f.sound if present (also when f.ms == 0)
  present f.pose for f.ms; a frame with ms == 0 is never drawn and only sets the logical pose
  when exiting, a frame longer than 99 ms lasts 100 ms
  t = i + 1
  for b in f.branch, in order:
    if exiting:            taken = b.target > i          # forward taken, backward skipped
    elif b is [to, p]:     taken = p >= 32768 or (p > 0 and uniform integer in [0, 32768) < p)
    elif b is "daypart":   taken = current period == v   # 0: 05-10h, 1: 10-18h, 2: 18-22h, 3: 22-05h
    elif b is "host":      taken = host value == v       # meaning unknown, one use (Tutor)
    if taken: t = b.target; stop
  i = t
```

- **Every random branch has its own draw.** `p = 0` is never taken in normal play; it is the
  exit branch. There is no `exit` key: exiting is the structural rule above (act-format
  5.4), and an animation can always be exited.
- `[office97]`: `idle` is the state the engine replays, with a new random variant, whenever
  an animation ends while the character is visible; `rest_pose` is the neutral pose;
  `transitions = [[from_pose, to_pose, state], ...]` are the clips played when the last
  logical pose and the next animation's first pose differ (act-format 5.5); `ids` maps state
  names to the numeric `MsoAnimationType` values; `strings` holds the remaining ACT string
  groups by id.
- An ACT script is a flat command list. Its mapping to frames is exact because **all 1,408
  distinct scripts in the 37 sample files consist of units `[sound] frame branch*`**, and
  all 7,217 branch targets point at the start of a unit (6,886 at a frame, 331 at a sound
  command; none into a branch run; no script starts with a branch; a sound command is always
  followed by a frame; at most 4 non-frame commands in a row). One unit is one frame. A
  script that does not fit is rejected by the converter with the offending command; the
  format would need an explicit no-pose marker frame for it, which is not specified because
  no file needs it.

### 7.4 What differs between the families, in one place

| Aspect | agent | office97 |
|---|---|---|
| Branch draw | one draw per frame, cumulative, percent | one draw per branch, n/32768, plus conditional branches |
| Exit | per-frame `exit`, `transition`, `return` | structural: forward branches taken, frames capped at 100 ms |
| `ms = 0` | shown if it has layers, skipped if empty | never drawn; sets the logical pose |
| Sound of a non-shown frame | not started | started |
| Between animations | return animation | pose transition table |
| Idle | states `IdlingLevel1..3`, timer in the server | replay state `idle` |
| Picture | layers at pixel offsets | poses with rectangles in logical units, scaled |

## 8. Extensibility

- **Resolution:** `canvas` is free; nothing assumes 8-bit or a size limit.
- **Alpha:** RGBA sheets; blending as in 5.3. Converted characters stay bit-exact because
  they only use alpha 0 and 255.
- **HiDPI:** a sheet may list `hidpi = { 2 = "sheets/a@2x.png" }`: the same grid with every
  dimension multiplied by the key. A sheet that only exists at a higher density sets
  `scale = k`. Layer coordinates stay in canvas units, so a character is authored once.
  Converted characters ship 1x only; vector actors can get real 2x sheets (D2).
- **Versioning:** `format` is the major version. Additions that old loaders may ignore keep
  the number; a loader refuses a higher number and an unknown `family`. `x-` keys are free
  for tools.
- **New characters** use `family = "agent"`: percent branches, exit targets, optional
  mouths. They need neither a palette nor a GUID.

## 9. Field inventory

Status: **kept** (stored, checked by the round trip), **derivable** (can be recomputed from
kept data), **dropped** (reason given), **source** (kept in `[source]` for the record, no
playback effect).

### 9.1 ACS 2.0 (acs-format 3)

| Field | Status | Where, or why not |
|---|---|---|
| Header magic, four locators, block order | dropped | container layout |
| Version (0x20001) | source | `source.version` |
| Localized-info locator | dropped | redundant (acs-format 3.2) |
| GUID | kept | `character.id` |
| Width, height | kept | `character.canvas` |
| Transparent index | kept | PNG `tRNS` |
| Flags 0x20 (voice present) | derivable | presence of `[voice]` |
| Flags 0x100 / 0x200 (balloon off / on) | kept | `balloon.enabled` |
| Flags 0x10000, 0x20000, 0x40000 | kept | `balloon.size_to_text`, `auto_hide`, `auto_pace` |
| Flags 0x100000 | kept | `character.standard_animation_set` |
| Flags 0x10 and all other bits | source | `source.flags` (raw u32); no consumer found |
| Animation set version | source | `source.animation_set`; no consumer found |
| Voice: engine, mode, speed, pitch, language, dialect, gender, age, style | kept | `[voice]` |
| Balloon: lines, chars per line, colours, font, height, weight, italic, second flag byte | kept | `[balloon]`; raw values (0 lines stays 0; the getter default is runtime) |
| Palette | kept | PNG `PLTE`, source order and count; reserved byte dropped (0 everywhere) |
| Tray icon (mask DIB, colour DIB) | kept, re-encoded | `icon.png`, mask becomes alpha. Inverting pixels (mask 1, colour not black) are not representable; Merlin's icon has none |
| States | kept | `[states]`; spelling of names normalized |
| Localized names, descriptions, extra data, LANGID | kept | `character.info`, order kept; LANGID as BCP 47 tag ([MS-LCID], primary-only ids become bare language tags; all 51 LANGIDs in the samples map reversibly) |
| Animation list name, order | kept | `animations[].name` |
| Uppercase name copy inside the record | dropped | derivable from the list name |
| Transition type, return animation | kept | `transition`, `return` |
| Frame images (index, x, y) | kept | `layers`, reversed to back-to-front; index replaced by sheet reference |
| Frame sound index, duration, exit index, branches | kept | `sound`, `ms`, `exit`, `branch` |
| Overlay type, image, x, y | kept | `mouths[].type`, `layer` |
| Overlay mouth point | kept | `mouths[].anchor` = (x + px, y + py) |
| Overlay "replace top image" | kept | `mouths[].base` = frame layers without the top one; no sample |
| Image list "checksum" dword | dropped | never read; not a function of the stored bytes (acs-format 8) |
| Image kind byte, compression flag, data size, LZ streams | dropped | re-encoded as PNG; kind 0 becomes "no layer" |
| Image width, height | kept | `cell` / `sizes` |
| Region (RGNDATA) per image | derivable | equals the non-transparent pixels for **7,656 of 7,656** images (`check.py`, every image) |
| Duplicate image records | dropped | merged (7,656 to 7,122) |
| Audio list checksum | dropped | as for images |
| Audio RIFF data | kept as PCM | FLAC of the PCM per section 6. Not kept: the ADPCM bitstream, the `fact` chunk (Agent never reads it), other RIFF chunks (LIST/INFO). A sound shorter than one ADPCM block is omitted because Agent cannot play it (`source.unplayable_sounds`) |

### 9.2 ACF + ACA 2.0 (acf-aaf-format 5 to 7); only what differs from 9.1

| Field | Status | Where, or why not |
|---|---|---|
| ACF magic, sizes, whole-body compression | dropped | container |
| Animation table file name | dropped | HTTP loading detail (`<name>.aca` in all samples) |
| Animation table checksum, ACA header version and checksum | dropped | cache-staleness check; opaque CRC of editor text |
| Return animation (in the ACF table) | kept | `return` |
| Per-animation image and sound pools | kept | merged into the character's pools by content |
| Image "unknown" byte, overlay reserved byte | dropped | 0 everywhere, never read |
| Image region | derivable | 796 of 796 equal the opaque pixels (acf-aaf-format 12) |
| Frame offset X/Y | dropped | 0 in all files, ignored by agentdpv; converter fails if nonzero |
| Overlay rectangle and inline pixels | kept | cropped picture plus `layer` position |
| Mouth point | absent | ACA has none; `anchor` omitted. What agentdpv reports instead was not traced (section 14) |
| Base image block, overlay "replace base frame", overlay region | kept | `mouths[].base` (base picture, or empty list); region derivable; no sample |
| Animations the ACF lists but that are not on disk | not converted | named in `source.missing_animations` |

### 9.3 Agent 1.5 (agent15-format 4)

| Field | Status | Where, or why not |
|---|---|---|
| OLE container (sectors, root CLSID, timestamps, `animN.aaf` names), magics C1/C2 | dropped | container |
| ACF version 0x1001F / 0x1001E / 0x1001C | source | `source.version` |
| Single localized entry | kept | one `character.info` with `lang = "en"` (the runtime files it as LANGID 9) |
| Extra data (`{74515580-...}Copyright...`) | kept | `info.extra` |
| Animation set version | derivable | absent; runtime reports 1.50; `source.animation_set` |
| Voice without the extra block | kept | `[voice]` without language fields; speed 0xFFFFFFFF kept as is |
| Balloon without trailing byte (0x1001E) | kept | `font_flag2` absent |
| Palette with 128 entries (Totem) | kept | as stored |
| Transition type | derivable | absent in the file; runtime uses 0, written as `"return"` |
| Frame exit | derivable | absent in the file; runtime uses -1, written as `exit = "end"` on every frame |
| Checksums | dropped | as 9.2 |
| Flag 0x1000 (Sharky) | source | `source.flags` |
| Damaged streams (Ozzar, 2 animations) | not converted | reported and listed in `source.missing_animations` |

### 9.4 ACT (act-format 4)

| Field | Status | Where, or why not |
|---|---|---|
| Pack magic, item count, offsets | dropped | container |
| Pack version | source | `source.format` (`act-2.2`, `act-1.0`) |
| Item name | source | `source.item_name`; decoding needs the build codepage (cp932 for Saeko-sensei), a converter option |
| Header: kind, unk10, unk12, unk14, unk16, rectangle 0x18..0x1E, nohit image, stretch band | source | `source.header`, raw; no consumer identified (Bob hit testing and stretch band are not modelled) |
| rest_pose | kept | `office97.rest_pose` |
| idle_anim | kept | `office97.idle` |
| Logical size | kept | `character.canvas` |
| Default pixel size | kept | `character.display` |
| Colour table and slot array | dropped | the distinct colours of the metafiles; used only for palette realization |
| Pose transition table | kept | `office97.transitions`; its copy of rest_pose is redundant |
| Payload index, DCIK/MNAK packing, RLE, LZ, 2 trailing bytes | dropped | re-encoded as PNG |
| Bitmap pixels | kept | native size, indexed PNG |
| Office palette (not in the file) | kept | written into `PLTE` from an Office ACS or MSO97.DLL at conversion time; index 10 transparent |
| Composite poses (child, rectangle) | kept | `[poses]`, rectangles verbatim in logical units |
| WMF payloads | rasterized | section 5.4, D2; vector data not carried |
| Payloads no frame, transition or rest pose uses | dropped | never shown |
| Sounds (RIFF) | kept | as 9.1; odd pad byte dropped |
| Animation group id, variants and their order | kept | `[states]` plus `office97.ids` |
| Group index order (`first`), unreferenced entries (3, in two Greetings Workshop files) | dropped | no playback effect |
| Script header word 0x0100 | dropped | constant, not read |
| Frame command (pose, ms) | kept | `pose`, `ms`; pose 0xFFFF = no `pose` |
| Random branch (target, p) | kept | `branch`; target as frame index, p raw |
| Host and time-of-day branches | kept | `[target, "host", v]`, `[target, "daypart", v]` |
| Sound command (index, argument) | kept | `sound`; argument in `sound_arg` when not the default, also for "no sound" commands |
| String groups 1 and 8 | kept | `character.info` name and description. ACT has no language field: the prototype writes `lang = "en"` for every actor, also the two Japanese ones; the real converter needs a language option |
| Other string groups | kept | `office97.strings` |
| `.ACP` preview files | not converted | subset of the `.ACT` |

### 9.5 Not round-trippable

The original bytes cannot be regenerated: container layout and block order; compressed
streams; image and audio checksums; image and sound index numbering (pools are deduplicated
and reordered); duplicate records; RGNDATA rectangle decomposition; string terminators and
empty-versus-missing strings; spelling case of state names, state members and the in-record
animation name; ACA/AAF file names and checksums; OLE sector layout; ACT blob packing, group
index order, unused payloads and colour table; WMF vector data; the ADPCM bitstream and all
RIFF chunks other than `fmt ` and `data`.

## 10. Conversion rules

Common: decode every picture to top-down pixels, merge identical pictures, write sheets as
in 5.1 and 5.2; decode sounds by section 6, write them as FLAC and identify them by their PCM
hash; write frames in source
order; fail loudly on anything outside the documented cases instead of guessing; record the
source file name and SHA-256 in `[source]`.

- **ACS:** layers = frame images in reverse order (the last listed is drawn first,
  acs-format 5.1), skipping indices beyond the image count. `ms = duration * 10`.
  Exit -1 becomes `"end"`, -2 is omitted. A later overlay of the same mouth type replaces
  an earlier one. Sound index beyond the list = no sound. Reject branch sums above 100 and
  invalid targets.
- **ACF + ACA:** parse with the ACF version gates; one layer per frame (the pre-composited
  picture), image 0xFFFF or size 0 = no layer; overlays become cropped pictures at their
  rectangle position; empty overlay records are omitted. Animations whose file is missing are
  listed, not invented.
- **Agent 1.5:** same parser at version 0x1001F/1E/1C; `transition = "return"`,
  `exit = "end"` on every frame (the runtime's defaults); damaged streams are reported.
- **ACT:** split each script into units (7.3) and reject scripts that do not fit; state
  name (D6): for Office actors (header kind 7) the Agent animation name that Office XP uses
  for the same id (act-format 6: 2 `Greeting`, 4 `Explain`, 32 `Show`, ...), `Anim<id>` for
  ids without a name and for every non-Office file (Bob, Greetings Workshop and Tutor use
  other id ranges); in addition the Agent state aliases `Showing` (32), `Hiding` (31),
  `GesturingLeft/Right/Up/Down` (114, 19, 115, 113) and `Hearing` (26) where the group
  exists. The prototype predates this decision and writes the Office 97 type-library names
  without aliases. Animation name = state name, plus `.1`, `.2` for several variants; branch targets
  become frame indices; poses keep their payload id; bitmap pictures get the Office palette;
  nested composites (none in the samples) are rejected; vector poses are rasterized per D2.

## 11. Round-trip check

Definition, to be carried into the build:

1. **Canonical dump.** A JSON document with: family, canvas, display, GUID, info list,
   balloon, voice, icon hash, palette (count, hash of the RGB triples, transparent index),
   states, and per animation its name, transition, return and per frame: `ms`, exit, branch
   list, sound hash (decoded PCM, section 6), picture hash (canonical RGBA of the composited
   frame at display size: straight alpha, all-zero where alpha is 0) and per mouth the type,
   the hash of the composited mouth variant and the anchor. For `office97`: pose id per
   frame, the `[office97]` table, and one picture hash per pose.
2. **Two producers.** The oracle builds the dump from the *source file* with the research
   Python parsers and the source runtime's compositing rules (`oracle.py`). The reader builds
   it from the *converted character only* (`reader.py`, later the Rust loader). The dumps
   must be equal, for the folder and for the `.bwc`.
3. **Traces.** Interpreter A runs the source structures (ACS frames with agentdp2's rules;
   ACT raw command lists with MSO97's rules), interpreter B runs the manifest. Both get the
   same scripted draws, time period and host value; each animation is run in normal mode and
   with exit mode entered at a random step. The sequences of (picture, ms, sound) must be
   equal. This checks the stated semantics, not just the stored numbers.
4. **Independence.** In the real project the Rust parser and converter are checked against
   the Python research parsers, which stay an independent oracle. In this prototype converter
   and oracle share those parsers, so the check proves that the format can hold everything
   and that writer and reader agree; what is independent already: the reader never imports
   the parsers, and Microsoft's own compositing is used as a pixel oracle (next table).

The check runs on the decided package form: FLAC audio written by the rule of section 6 and
read back with the `flac` tool. Oracle and reader share the ADPCM decoder, so the check does
not test that decoder; it is tested separately against sox (identical sample values for all
139 playable sounds) and the FLAC files against symphonia (210 of 210 bit-exact). Not
exercised: `hidpi` and `scale` are specified but no reader loaded such a sheet (section 14).

Results (`python3 -I check.py`, exit 0; `results/check.txt`):

| Source | Characters | Animations | Frames | Traces | Result |
|---|---|---|---|---|---|
| ACS 2.0 | 12 | 495 | 13,361 | 17,820 | dumps and traces equal; 7,656 of 7,656 regions equal the opaque pixels |
| ACF + ACA 2.0 (Merlin, Genie, Peedy, Robby) | 4 | 86 | 1,038 | 3,096 | equal; 216 listed animations are not archived |
| Agent 1.5 OLE (Merlin and 9 third-party) | 10 | 467 | 3,807 | 16,812 | equal; 2 damaged Ozzar animations reported |
| ACT bitmap actors (incl. Tutor, 3 Rocky variants) | 10 | 468 | 16,905 | 16,848 | equal, pose pixels included |
| ACT vector actor with pictures (Office 97 Clippit) | 1 | 37 | 756 | 1,332 | graph and traces equal; pixels not checkable |
| ACT vector actors, graph only (19 Bob, 7 Office) | 26 | 903 | 21,982 | 32,508 | graph and traces equal |
| **All** | **63** | **2,456** | **57,849** | **88,416** | 3,276,133 trace steps |

Cross-format, compared through the converted output only:

- Merlin from `merlin.acs` (layered images, my compositing) and Merlin from ACF+ACA
  (frames composited by Microsoft's editor): **45 of 45 shared animations identical** in
  transition, return, timing, exits, branches, sounds, frame pixels and mouth pixels (477
  frames); GUID, palette, states, names, icon and voice identical.
- Merlin 1.5 against Merlin 2.0: 424 of the 451 distinct 1.5 frame pictures also occur as
  a 2.0 frame (the rest are the animations Microsoft redrew, agent15-format 8.3).

Sensitivity (`python3 -I mutate.py`): 17 single mutations of a converted character (one pixel,
one palette entry, one duration, one probability, one exit target, one sound byte, layer
order, layer offset, mouth anchor, transition type, a balloon value, a state member, an
Office 97 probability, a daypart value, a pose rectangle, a transition pose, an exit-branch
target): **17 of 17 detected**.

## 12. Measurements

Machine: Intel Core i7-10700T, Ubuntu 26.04. Python 3.14 (zlib level 9 for PNG), Rust 1.99.1
release build with png 0.17.16, toml 0.9.12, zip 2.6.1, symphonia 0.5.5; flac 1.5.0,
ffmpeg 8.0.1, sox 14.7, oxipng 10.2.1.

### 12.1 Where the bytes are in ACS (`measure.py breakdown`)

Of 21,158,836 bytes in the 12 files: images 18,189,616 (86%), audio 2,392,967 (11%),
everything else 576,253 (3%).

### 12.2 Sheet layout, image bytes only (`measure.py layout`)

| Layout (all 12 ACS files) | Files | Bytes | vs ACS image bytes |
|---|---|---|---|
| ACS image records (LZ, with regions) | | 18,189,616 | 1.00 |
| One sheet, 1 column | 12 | 9,715,720 | 0.53 |
| One sheet, 8 columns | 12 | 9,470,242 | 0.52 |
| One sheet, 32 columns | 12 | 8,861,141 | 0.49 |
| **One sheet, width up to 4096, first-use order (chosen)** | 12 | 8,799,192 | 0.48 |
| The same after `oxipng -o 4 --nx` | 12 | 8,233,512 | 0.45 |
| One sheet per animation, 8 columns | 347 | 9,854,458 | 0.54 |
| One PNG per picture | 7,122 | 19,481,751 | 1.07 |
| Pre-composited frames instead of layers, 8 columns | 12 | 9,728,067 | 0.53 |

Wider sheets compress better because the same scanline of consecutive frames lies side by
side. One PNG per picture pays for a palette and headers 7,122 times.

### 12.3 Package size per audio option (`measure.py total`), in kB

A = original RIFF bytes (ADPCM kept), **B = FLAC of the PCM of section 6 (decided)**, C = FLAC
for PCM sounds and ADPCM kept, D = PCM WAVE. Folder = sum of file sizes; zip = `.bwc`.
B and D use Microsoft's decode rule and leave out the one unplayable sound.

| File | ACS | sheet | manifest | audio A | audio B | A folder | A zip | B folder | B zip | C folder | C zip | D folder | D zip |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| CLIPPIT.ACS | 2904 | 1420 | 69 | 40 | 69 | 1529 | 1475 | 1558 | 1505 | 1523 | 1469 | 1623 | 1569 |
| DOT.ACS | 555 | 116 | 114 | 99 | 211 | 330 | 235 | 441 | 347 | 330 | 235 | 631 | 537 |
| F1.ACS | 2554 | 1123 | 88 | 195 | 396 | 1406 | 1336 | 1607 | 1537 | 1397 | 1327 | 1917 | 1847 |
| LOGO.ACS | 1031 | 387 | 47 | 229 | 65 | 663 | 624 | 499 | 460 | 499 | 460 | 662 | 623 |
| MNATURE.ACS | 1531 | 805 | 84 | 295 | 206 | 1184 | 1112 | 1094 | 1022 | 1068 | 996 | 1344 | 1272 |
| OFFCAT.ACS | 2072 | 860 | 65 | 103 | 125 | 1028 | 976 | 1050 | 999 | 996 | 945 | 1174 | 1123 |
| ROCKY.ACS | 3006 | 1273 | 116 | 109 | 190 | 1498 | 1401 | 1580 | 1482 | 1490 | 1393 | 1784 | 1687 |
| courtney.acs | 817 | 278 | 31 | 16 | 19 | 325 | 300 | 328 | 303 | 320 | 294 | 342 | 317 |
| earl.acs | 1473 | 483 | 57 | 204 | 235 | 744 | 697 | 775 | 728 | 696 | 649 | 1025 | 978 |
| merlin.acs | 2181 | 1140 | 76 | 207 | 360 | 1423 | 1363 | 1576 | 1516 | 1398 | 1338 | 1861 | 1801 |
| qmark.acs | 1174 | 206 | 10 | 850 | 326 | 1066 | 1058 | 542 | 534 | 542 | 534 | 1066 | 1058 |
| rover.acs | 1862 | 708 | 55 | 46 | 75 | 809 | 763 | 838 | 792 | 809 | 763 | 943 | 896 |
| **Total** | 21159 | 8799 | 812 | 2393 | 2278 | 12005 | 11341 | 11889 | 11226 | 11068 | 10404 | 14372 | 13708 |
| **Ratio to ACS** | 1.00 | 0.42 | 0.04 | 0.11 | 0.11 | 0.57 | 0.54 | 0.56 | 0.53 | 0.52 | 0.49 | 0.68 | 0.65 |

- Audio alone: A 2,392,967, B 2,277,936, C 1,456,095, D 4,760,324 bytes. FLAC halves the PCM sounds (LOGO, qmark) and
  doubles the ADPCM ones (806,149 bytes of playable ADPCM become 1,628,308 as FLAC).
  Microsoft's longer tails cost 15,794 bytes of FLAC against the earlier reference decoder.
- Worst single character: A 0.91 (qmark), B 0.79 (DOT), D 1.14 (DOT, the only case above
  its ACS).
- With oxipng on the sheets, option B as `.bwc` is 10,660,089 bytes (0.50).
- The manifests total 812 kB as TOML (696 kB as compact JSON) and 124 kB deflated.
- Other sources, folder with FLAC audio: Merlin 1.5 OLE 2,920,960 to 1,284,430; Genius
  (ACT bitmap) 1,899,513 to 1,603,330; web Merlin (45 animations) 1,430,634 to 919,180.

### 12.4 Load time (`timing.py`, Rust harness), median of 21 runs in ms

Like with like: both sides are compiled Rust (release), read the file each run (warm page
cache) and produce the same result. "First frame" = frame 0 of the first animation of state
Showing, composited to RGBA (the two sides' frame hashes are asserted equal). "Full" = every
picture decoded and every animation parsed; for the new format this includes the TOML parse.
What limits the comparison: the ACS side is a from-spec LZ decompressor written for this
test (64-bit bit reader, one refill per token; a first version with a byte-wise reader was
1.6x slower, so the ratio depends on that code), it does not decode the regions (favours
ACS), and the PNG side is the tuned `png` crate.

| Character | ACS first | ACS full | TOML parse | new first, whole sheet | new first, row streaming | new full | `.bwc` full | per-animation sheets, first |
|---|---|---|---|---|---|---|---|---|
| clippit | 0.14 | 17.82 | 2.21 | 12.04 | 2.28 | 12.08 | 12.17 | 2.00 |
| dot | 0.05 | 1.24 | 3.56 | 6.25 | 3.79 | 6.23 | 6.40 | 3.69 |
| f1 | 0.13 | 14.85 | 2.34 | 10.98 | 2.74 | 10.42 | 11.64 | 3.77 |
| logo | 0.06 | 4.23 | 1.37 | 3.83 | 1.67 | 3.74 | 3.95 | 1.50 |
| mnature | 0.10 | 9.17 | 2.41 | 7.77 | 2.73 | 7.31 | 8.84 | 2.58 |
| offcat | 0.11 | 11.22 | 1.81 | 8.30 | 2.12 | 8.86 | 9.06 | 1.95 |
| rocky | 0.18 | 16.16 | 3.22 | 12.46 | 3.97 | 14.38 | 12.82 | 3.40 |
| courtney | 0.05 | 3.47 | 0.95 | 2.87 | 1.34 | 2.86 | 2.87 | 1.02 |
| earl | 0.07 | 5.27 | 1.65 | 5.01 | 2.05 | 5.25 | 5.80 | 2.16 |
| merlin | 0.12 | 13.68 | 2.07 | 10.08 | 2.88 | 10.57 | 10.23 | 2.49 |
| qmark | 0.09 | 4.33 | 0.33 | 2.28 | 0.73 | 2.20 | 2.32 | 0.97 |
| rover | 0.09 | 8.57 | 1.64 | 6.12 | 2.03 | 6.09 | 6.36 | 1.79 |
| **Sum** | 1.20 | 110.02 | 23.56 | 87.99 | 28.33 | 90.00 | 92.45 | 27.33 |

- Full load: 90 ms against 110 ms in sum (0.82x). Per character the new format is equal
  or faster for 11 of 12. DOT is slower (6.2 ms against 1.2 ms): its small sprites sit in
  padded cells and its manifest is the largest; 3.6 ms of it is TOML parsing.
- First frame: ACS wins by one to two orders of magnitude because it decodes a single
  image. In absolute terms the new format needs 0.7 to 4.0 ms with row streaming, most of it
  TOML parsing. The same data as JSON parses in 8.02 ms instead of 23.56 ms (sum of 12).
- Folder and `.bwc` load equally fast (stored entries).

### 12.5 Vector actors (`measure_wmf.py`, probes)

27 of the 37 distinct ACT files use WMF: 8 of the 15 Office 97 actors (Clippit, The Dot,
Hoverbot, Office Logo, Power Pup, Scribble, Will, Kairu; 7 of the 9 on the Office 97 CD) and
all 19 Bob files. For Office 97 Clippit (468,266 bytes, 250 WMF parts, 679 poses, 496
distinct pose pictures at 192x144):

| Raster option | Sheet bytes | vs the .ACT |
|---|---|---|
| 1x, not antialiased, indexed PNG (the 9 exact WMF colours) | 352,350 | 0.75 |
| 1x, antialiased RGBA (4x supersampled) | 3,384,198 | 7.23 |
| 2x, not antialiased, indexed PNG | 864,494 | 1.85 |
| 2x, antialiased RGBA | 8,089,882 | 17.28 |

The prototype rasterizer is the research one (polygons, polylines, ellipses, round
rectangles); it has no TextOut and no SetROP2, so these numbers are sizes, not final
pictures.

## 13. Decisions

Accepted by the owner on 2026-11-11, each as recommended. The options that were weighed stay
listed for the record.

**D1 Audio storage. Decided: B, FLAC of decoded PCM.**
- A: keep the original RIFF bytes, ADPCM included (2.39 MB; the daemon would need its own MS
  ADPCM decoder).
- **B: decode at conversion time by the rule in section 6, store everything as FLAC**
  (2.28 MB; one decoder in the daemon; the PCM is fixed once and is the same on every
  machine and in every editor).
- C: FLAC for PCM sounds, ADPCM kept (1.46 MB; two decoders).
- D: PCM WAVE (4.76 MB; DOT becomes larger than its ACS).
- The question that was open when this was decided, which rounding Microsoft's codec uses,
  is settled (section 6): the rule is now Microsoft's own, read from `msadp32.acm` and
  `agentsvr.exe`. Against the earlier reference decoder no sample value changed; 112 of 140
  sounds got a longer tail and one sound that Agent cannot play is left out.

**D2 Vector (WMF) actors. Decided: A, rasterize at conversion time.**
- **A: the package holds pictures only**: a 1x sheet without antialiasing, indexed, as the
  faithful rendering (0.75x of the `.ACT` for Clippit), plus an optional 2x sheet under
  `hidpi`; both rendered by a bentwire WMF rasterizer in the converter (24 record types,
  act-format 4.6). Antialiasing is a converter option, off by default. With the 2x sheet the
  pictures come to about 2.6x of the `.ACT` (0.75 + 1.85): vector actors with HiDPI sheets
  are the one case that is larger than its source. Tests for the rasterizer can only be
  golden images. Re-convert from the `.ACT` when the rasterizer improves.
- B: carry vector data and rasterize in the daemon. C: both.
- The prototype's converted Clippit still uses the antialiased RGBA sheet of the research
  rasterizer; the sizes of the decided variant are in 12.5.

**D3 Manifest syntax. Decided: TOML**, validated with the JSON Schema. (TOML parses in
23.6 ms for 12 characters, JSON in 8.0 ms and is 14% smaller, but has no comments.)

**D4 Sheet layout. Decided: the converter writes one sheet per character**; the format
allows several, so authors can work per animation. (One wide sheet: smallest, 12 files, first
frame 0.7 to 4.0 ms with row streaming. One sheet per animation: 0.54 against 0.48, 347
files. One PNG per picture: 1.07, 7,122 files.)

**D5 Rule families. Decided: keep the `family` key** as specified. Normalizing ACT into Agent
rules would drop Office 97 behaviour that the data depends on (independent draws, exit by
structure, 0 ms marker frames, pose transitions, idle replay). The daemon implements two
small interpreters (the prototype's are about 40 lines each).

**D6 Names for ACT animations. Decided: Office XP's Agent names plus state aliases** for
Office actors (kind 7), so that Office 97 and Office 2000 characters answer to the same
requests; numeric ids are always kept in `office97.ids`; Bob, Greetings Workshop and Tutor
files have no name table and stay `Anim<id>`. Exact rule in section 10. This is converter
policy and does not change the format.

**D7 PNG optimizer. Decided: `oxipng --nx` for `bentwire-characters` releases only**, not in
the default converter path. It saves a further 6.4% on the sheets and keeps palette order.

**D8 Incomplete sources. Decided: convert what exists and list the rest** in
`source.missing_animations` (web characters with missing animation files, the two damaged
Ozzar streams). `bentwire-characters` only ships complete characters.

## 14. Items to verify before or during the build

1. **MS ADPCM rule: closed 2026-11-11.** `msadp32.acm` 5.1.2600.5512 uses the arithmetic
   shift (sample step 0x72C61CE7, delta step 0x72C61CBD), decodes a short final block, and
   Agent pads the result to whole blocks and ignores `fact` (agentsvr 0x01013F5A). Details
   and the effect on the 140 sounds are in section 6. Two smaller points remain from it:
   `msacm32.dll` was not available and is assumed to forward the size and convert requests
   unchanged; and how Office 97's `PlaySound` path treats a short final block or a sound
   shorter than one block (4 ACT sounds) was not analysed.
2. **ACT rectangle scaling.** act-format 4.5 describes the MulDiv as truncating; Win32
   `MulDiv` rounds to nearest. It only matters for rectangles that are not multiples of 15
   logical units; the 48 zoomed placements in each Rocky file are the candidates. The
   stretch filter and the window size mode are open in act-format 9.
3. **Mouth overlays outside the frame shape.** 50 of the 291 ACS overlays have opaque pixels
   (1,080 in total) where the frame itself is transparent. In Agent's region mode the window
   region is the frame's region, so these pixels would be cut off (acs-format 5.1). The
   format stores what either reading needs; the behaviour spec has to pick one.
4. **Mouth point for ACA and 1.5 characters.** ACS stores it; agentdpv's substitute was not
   traced.
5. **ACS frames with empty picture, 0 duration and a sound** (2 frames, F1 and ROCKY): the
   rule "not started" follows from agentdp2's skip logic and was not observed at run time.
6. **Blend rule with partial alpha** (5.3) was only exercised with alpha 0 and 255 and with a
   single antialiased layer; it needs its own test vectors when the first RGBA character
   appears.
7. **`hidpi` and `scale`** are specified and pass the schema, but no converted character uses
   them and neither reader implements them. The first 2x sheet (D2) is the test.

## 15. Prototype

The prototype lives in the reference directory `$BENTWIRE_REFERENCE` (throwaway, outside
Git, not public; run with `python3 -I`):

| File | Role |
|---|---|
| `sources.py` | Loads the samples through the reference parsers (ACS, ACF, Agent 1.5, ACT), cached in `cache/`. |
| `convert.py` | Source structures to the neutral model, sheets, manifest, FLAC audio (D1), folder and `.bwc`. `python3 -I convert.py` writes `out/` (63 characters, plus `out/variants/`). |
| `reader.py` | Loads a converted character only; compositor, canonical dump, interpreter B. |
| `oracle.py` | Canonical dump and interpreter A from the research parsers' structures. |
| `check.py` | Round-trip check, section 11. `mutate.py`: sensitivity test. |
| `character.schema.json`, `validate.py` | Manifest schema and cross-reference rules. |
| `measure.py` (`breakdown`, `layout`, `audio`, `total`), `measure_wmf.py`, `timing.py` | Section 12; raw output in `results/`. |
| `common.py` (`msadpcm_decode_ms`), `measure_ms.py`, `measure_adpcm.py` | Microsoft's ADPCM rule, its comparison with the earlier reference and with sox, libsndfile, ffmpeg, symphonia (section 6). |
| `msadp32/` | Working copy of the codec and its disassembly (`text.asm`, objdump). The original `msadp32.acm` is untouched. |
| `harness/` | Rust: ACS loader with LZ decompressor, new-format loader, symphonia decode. `~/.cargo/bin/cargo build --release`. `harness-rodio/`: rodio decode test (feature `adpcm` adds `symphonia-adpcm`). |
| `probe_act*.py` | Script and pose statistics behind 7.3 and 2. |

Installed for this work: rustup toolchain 1.99.1 (user-local), oxipng (cargo), and the apt
packages flac, ffmpeg, zip, build-essential, sox, sndfile-programs.
