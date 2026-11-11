# Microsoft Agent 1.5 character formats (.ACS docfile, .ACF + .AAF)

Research for bentwire. This note describes the character files of Microsoft Agent 1.5
(1997/98) and how the Agent 2.0 runtime (v2.00.0.3427) reads them. It is written in our
own words from static analysis of the shipped binaries and from parsing real files. No
Microsoft code is reproduced and no extracted images or sounds are included.

It builds on two verified notes and references them for identical structures:
- `acs-format.md`: the 2.0 single-file ACS (magic 0xABCDABC3, agentdp2.dll), and the
  compression algorithm (section 4 there).
- `acf-aaf-format.md`: the 2.0 web format (ACF magic 0xABCDABC4 + ACA files,
  agentdpv.dll), including the version-gated parser. That note had no 1.5 samples; this
  one supplies them and closes most of its "binary only" rows.

## 1. Summary

- **Agent 1.5 has two containers for one data model.**
  - **1.5 ".ACS" = an OLE compound file ("DocFile")** with one stream `char.acf`
    (magic **0xABCDABC1**, compressed character definition) and one stream per animation
    named `anim1.aaf`, `anim2.aaf`, ... The root storage CLSID is
    {BB64DF2F-88E4-11D0-9E87-00C04FD7081F} ("Microsoft Agent DocFile Provider 1.5").
  - **1.5 web format = flat `.ACF` file (magic 0xABCDABC2) + one `.AAF` file per
    animation.** `.AAF` is the 1.5 name of what 2.0 calls `.ACA`.
  - In both, the character body is the ACF body at **version 0x1001F** (or 0x1001E,
    0x1001C) and each animation is an AAF stream with the same layout as a 2.0 ACA at the
    same version. So: the 1.5 ACS stream contents are byte-identical to the 1.5 web files
    (proven for Merlin, section 8.3).
- **The 2.0 ACS (0xABCDABC3) is a different, newer format.** Nothing of it is shared with
  1.5 except the compression algorithm, the pixel/palette/region conventions and the
  frame semantics.
- **Readers:** in Agent 2.0 all 1.5 files go to **agentdpv.dll** (DocFile provider for the
  OLE file, File provider for ACF/AAF). The 1.5 runtime's own data provider (AgentDPv.dll
  1.5.0.225 and 1.5.2.416) parses the same structures with the same version gates.
- **Compared with 2.0 ACS, a 1.5 character has:** pre-composited full-frame images per
  animation (no shared image pool), no per-frame exit branch and no transition byte
  (the runtime substitutes exit -1 and transition 0 "use return animation"), separate
  `*Return` animations, one English localized entry, voice without the extra block, no
  tray icon, and (for 0x1001E and older) no trailing balloon byte. Animation set version is
  reported as 1.50.
- **Samples found and fully parsed:**
  - 10 OLE .ACS files: Microsoft's Merlin (shipped with the Agent "1.7" beta build 1027,
    Oct 1997, content identical to 1.5 Merlin) and 9 third-party 1997/98 characters
    (ACF versions 0x1001F and 0x1001E). 469 animations (467 readable, 3,807 frames,
    3,237 images).
  - Microsoft's 1.5 web characters (public archive copies): 4 ACF (Genie, Merlin,
    Merlin with sound effects, Robby) and 12 distinct AAF files.
  - Every sector of every OLE file is accounted for (FAT, directory, mini FAT, mini
    stream or a referenced stream), every stream is referenced, and every decompressed
    body is consumed exactly, except two animation streams in one third-party file
    (Ozzar) whose compressed data has a single-bit error each (section 8.2).
  - All 3,318 images have a region equal to their non-transparent pixels.
  - 725 of the 755 Merlin 1.5 frame images are byte-identical to a composited frame of
    the 2.0 `merlin.acs`; the 30 others belong to animations Microsoft redrew for 2.0.
- **Confidence:** high for the OLE container, `char.acf`, ACF 0x1001F/0x1001E, AAF
  0x1001F/0x1001E, images, regions, overlays, sounds, compression and the 2.0 defaults for
  missing fields (sample- and binary-verified). Medium for ACF 0x1001C and headerless
  AAF, uncompressed AAF, empty images: binary only, no samples.

## 2. Terminology and versions

| Name | What it is | Magic / version | Evidence |
|---|---|---|---|
| 1.5 `.acs` | OLE compound file, streams `char.acf` + `animN.aaf` | stream magic 0xABCDABC1; ACF 0x1001F / 0x1001E | 10 samples; agentdpv 2.0 0x5a964d61; AgentDPv 1.5.2.416 0x67f91fe8 |
| 1.5 `.acf` | flat character file for web delivery | file magic 0xABCDABC2; ACF 0x1001F | 4 samples; 2.0 0x5a966d0d; 1.5 0x67f935e3 |
| `.aaf` | one animation (web file or OLE stream) | header version 0x1001F / 0x1001E | 12 web + 469 OLE samples |
| 2.0 `.acf` / `.aca` | 2.0 web format | 0xABCDABC4, version 0x20001 | `acf-aaf-format.md` |
| 2.0 `.acs` | 2.0 single file | 0xABCDABC3 | `acs-format.md` |
| Beta 1 files (`Genie.ani`, `genie.mth`, `Genie00.seq`, `genie.bmp`) | pre-release character files of Agent "Beta1/Beta1A" (per the INF comment) | none | named only in `DelGenie.inf` of the 1.5.2.416 installer, which deletes them. No string `.ani`/`.mth`/`.seq` exists in any 1.5 or 2.0 provider or server binary. No runtime here reads them. |

ACF/AAF versions are a u32 with the minor number in the low 16 bits: 0x1001F = "1.31",
0x20001 = "2.1". Accepted versions are exactly **0x1001C, 0x1001E, 0x1001F** (and 0x20001
in 2.0). The 1.5 runtime rejects anything above 0x1001F with 0x80042203 and anything else
outside the set with 0x80042202 (AgentDPv 1.5.2.416 0x67f9693c); 2.0 is the same plus
0x20001 (`acf-aaf-format.md` section 6).

Which Microsoft product version wrote which version is not proven. The samples: all
Microsoft 1.5 characters and 8 third-party ones are 0x1001F; Sharky (Dec 1997) is 0x1001E.
No 0x1001C file was found.

## 3. Sources and samples

Public archive copies (throwaway, not in this repository). Nothing was executed; installers
were unpacked with `cabextract` / `7z`.

### 3.1 OLE .ACS samples

| File | Product | Size | SHA-256 |
|---|---|---|---|
| Merlin `Characters/merlin.acs` (dated 1997-10-29) | Microsoft Agent build 1.7.0.1027 (beta) | 2,920,960 | `c7e8e80dd8fab1f2315f7b4b60dca12e081e35eb5dcc0b97f659819cfccc21a8` |
| Al.acs | third-party Agent 1.5 character | 2,217,472 | `437ccc7943cbc8d9364f07387c4a55c5e27b24a5ca604878c2dddb07da301cf9` |
| Check.acs | same | 170,496 | `069f6a62765e43fff3bfb7092f16023544f69d571b5b2f45f2377e5946687d6e` |
| Gourdy.acs | same | 426,496 | `f19dae253ad71cc64e6d152e0439f6c30945f3de5b0699ed47a9d873d0ea219c` |
| Max.acs | same | 581,120 | `72b9189584b3c4925de510a3cdece910cad7eab413724517162a3be66257097a` |
| Miku.acs | same | 13,824 | `3d792565767898d085323aced25cce9a37d9327fa28de2ab47720e679e11e0fc` |
| Ozzar.acs | same | 4,286,464 | `0e47edee88f3cae7d0cf1c80e2869cb47c1217c8dd7c833a8ba6830ceba75165` |
| Sharky.acs | same | 933,888 | `3119ae86024fb7a7c660121723ad3f23f27fdde5c70e1a139aee293eb7308260` |
| Spaceman.acs | same | 442,368 | `490a5b57bf4139206525bbe7643af87ba9888bcdd7e429a234af606ec32bc580` |
| Totem.acs | same | 1,033,216 | `ea2970bdb80a86e552ec9c67b08c5282ce41d43d5cbde8a26857adae1159cc40` |

The nine third-party files were picked from a public collection of Agent characters because
their sizes are multiples of 512 (OLE files are sector-aligned) and their dates are 1997/98.
Each file's CRC-32 and size were checked against the CRC-32 and size recorded for the
collection: all nine match.

### 3.2 1.5 web samples

Microsoft's Agent 1.5 web characters, `<char>/<File>` as on the original server. Several
files exist in two copies; where they do, the copies are identical.

| File | Copies | Size | SHA-256 |
|---|---|---|---|
| genie/genie.acf | 2 (identical) | 3,425 | `39a0171111538acff830beb68f0020d09a2fe4dc579ccca23cdc9982c21a8ff0` |
| Genie/GestureLeft.aaf | 1 | 33,744 | `fb9e36ac2168abd54a78c5cbd709028245003d97a3ae833fde5b0a16efa88406` |
| genie/Greet.aaf | 2 (identical) | 45,079 | `a3bb033bb3dcad3482b9cdc0d402e7e0a220564f0eef378dbc37544b39106655` |
| genie/GreetReturn.aaf | 2 (identical) | 22,084 | `b1896e1793ed341e890bc9aedd9ae3a4e38272c980572085679ab93d15903ddc` |
| Genie/Hide.aaf | 1 | 19,453 | `190be70341d74ab42d3ce8aaa4d50c4ebb6ab446587f68fd5fe4729f0b6a0382` |
| Genie/RestPose.aaf | 1 | 6,711 | `df4933a7bf10a69804a06bbf5abfd14f42ca713f99d30b8dd96207fe0beeb2c0` |
| merlin/merlin.acf | 2 (identical) | 3,473 | `3bbe638b4d6257270c5a9ebce13a97149b62af0c6f704acc30a24358551df2cb` |
| merlin/Blink.aaf | 1 | 4,937 | `731e513cb23a2ad5ad10a676bf2852be6f8ac36de604ba434a68d107a141975a` |
| merlin/Show.aaf | 1 | 26,881 | `8340f41767ff071b7ade8fd5cfe1eab2d066d425fd275d663ad8f8a93a46bd8c` |
| merlinsfx/merlinsfx.acf | 2 (identical) | 3,477 | `8d196dce3f7f7081fe9b24c6c3a1e20a44c21f2df3b9eb7ca63123c14778fbb3` |
| merlinsfx/Show.aaf | 2 (identical) | 31,673 | `68e3bcfc1e60e7506b6911b1233fd68490d7e22923fd2229eee7c7eeaabe91af` |
| robby/robby.acf | 2 (identical) | 3,249 | `06a70f43738ea712c549421e63f4e87dfc12c3f6578afc21d298c0142d0c3469` |
| robby/Acknowledge.aaf | 2 (identical) | 38,440 | `8725f55f1f203ff13dc2258d19d7e933a6cb400ec5e21cc81cb070984c68d900` |
| robby/Explain.aaf | 1 | 22,130 | `4a0de52596a221f101e55a123aed6fca7c50883e3e2220fc8f85c630960d1d34` |
| robby/Wave.aaf | 2 (identical) | 40,397 | `cc845baf72c33c0d25e4a35e8e6ffa6efb426c5725f8d9d39d4cd042de9aeb6e` |
| robby/WaveReturn.aaf | 2 (identical) | 27,938 | `446bb94fc69b67c2b70cb58bb49edb44c2e91abd010513fb631fac229c31fb93` |

The same server also had 2.0-format ACF (0xABCDABC4, 0x20001) and a few ACA files for genie,
merlin, peedy and robby; they parse with the 2.0 rules and are not discussed further.

### 3.3 Binaries

| Binary | Source | SHA-256 |
|---|---|---|
| AgentDPv.dll 1.5.2.416 (Feb 1998) | `MSagent.exe` (340,112 bytes, SHA-256 `5e00fe1b303dc56e3cf556adaf23a956ccf2ab2bb42b1a50736ad5bd4f3d5208`), the Agent 1.5 runtime installer, a self-extracting CAB | `4a5de7cc6327db9e8a610451b91343aa24f270e9f7050808087d0d9760a7b6d1` |
| agentdpv.dll 1.5.0.225 (Aug 1997) | Microsoft Agent build 1.5.0.225 | `039e856fa12f9d029e5faa2de7c7bd0ad00807ed3c74ae09d429c26f489da506` |
| AgentDpv.dll 1.7.0.1027 (Agent 2.0 beta) | Microsoft Agent build 1.7.0.1027 | `9bac229652b515dfd8bba22baade5c1daa967f4a3bf7463c9ca1ba1388682358` (not analysed) |
| agentdpv.dll 2.00.0.3427 | the Agent 2.0 runtime | `66a6765e2693b05c7b1bb72973b32264656ffbba7c176f8a86ce0edc8358875c` |
| agentsvr.exe 2.00.0.3427 | same | `b2aaa8618528037537fef6280d05ff3f88b2b50fc250ca04e7d4bf3fa970e7dd` |

### 3.4 Negative results

- Microsoft's own 1.5 `.acs` downloads of Genie, Merlin, Merlin with sound effects and
  Robby survive in no archive copy found for this note (only error pages), and the 1998
  download pages are redirect stubs that already point to the 2.0 files.
- So no Microsoft-released 1.5 Genie or Robby `.acs` was found, and no 1.5-format Peedy
  file of any kind (Peedy appears only in the 2.0 `v2/` folder). Merlin from build
  1027 stands in: its streams are byte-identical to the 1.5 web AAF files that exist
  (section 8.3).

## 4. Format specification

All integers are little-endian. Types and the ACF string encoding (u32 count of UTF-16
code units, **no terminator**) are as in `acf-aaf-format.md` section 4. The 1.5 runtime's
string reader has no length limit; 2.0 rejects counts above 0x1000000.

### 4.1 Detecting a 1.5 file

| First bytes | Meaning |
|---|---|
| `D0 CF 11 E0 A1 B1 1A E1` | OLE compound file: 1.5 `.acs` if it has a root-level stream `char.acf`. All 10 samples also carry the root CLSID {BB64DF2F-88E4-11D0-9E87-00C04FD7081F}. |
| `C2 AB CD AB` | 1.5 web `.acf` (0xABCDABC2) |
| `C4 AB CD AB` | 2.0 web `.acf` |
| `C3 AB CD AB` | 2.0 `.acs` |
| `1F 00 01 00` / `1E 00 01 00` | a headered `.aaf` (version 0x1001F / 0x1001E). A headerless `.aaf` (ACF 0x1001C) starts with the compression flag byte 0x01 or 0x00. |

How Agent 2.0 picks the provider (agentsvr.exe 0x0101F08C..0x0101F2DF, imports resolved
from the PE import table): for a local file it first reads 4 bytes and, if they are
0xABCDABC3, uses agentdp2's CLSID {D45FD301-...}. Otherwise it calls ole32
`GetClassFile` on the path:
- result {BB64DF2F-...} (the CLSID stored in the 1.5 docfile root) maps to
  {D45FD2FF-5C6E-11D1-9EC1-00C04FD7081F}, agentdpv's "DocFile Provider 2.0";
- result {1EAF7042-9F92-11D0-89AA-00A0C9054129} ("File Provider 1.5") maps to
  {D45FD300-...}, agentdpv's "File Provider 2.0";
- any other successful result is used as is (agentdpv registers `.acf`/`.aca` and file
  patterns C4/C2 for {D45FD300}; agentdp2 registers `.acs` and pattern C3 for {D45FD301});
- `MK_E_INVALIDEXTENSION` (0x800401E6) is retried up to 10 times with a 50 ms `Sleep`,
  then {D45FD301} is used; any other error selects {D45FD300} (presumably the URL case; not traced).
agentdpv's registry script also marks {BB64DF2F} as `TreatAs` {D45FD2FF}. The 1.5
AgentDPv registers {BB64DF2F} as `Agent.MSDocFileProvider` and {1EAF7042} as
`Agent.MSFileProvider`.

For bentwire: sniff the content, not the extension. A 1.5 `.acs` is recognised by the OLE
signature plus a `char.acf` stream whose first 4 bytes are 0xABCDABC1.

### 4.2 OLE container

Standard Microsoft compound file (MS-CFB); bentwire needs a reader for it (the throwaway
one in `scripts/cfb.py` is about 130 lines and has no DIFAT-chain test data). What the runtime does (2.0 0x5a964d61 and
0x5a965373; 1.5.2.416 0x67f91fe8 and 0x67f929f4):
- Opens the file with `StgOpenStorage`, mode 0x20 (read, deny write), retrying up to 10
  times with 100 ms sleeps on sharing/lock violations.
- Opens the root-level stream **`char.acf`** (mode 0x10, exclusive) and parses it
  (4.3) when the character is loaded.
- Opens an animation stream only when the animation is first used, by the **file-name
  field of the ACF animation table** (4.4.1). OLE stream names are case-insensitive.
- Nothing else in the storage is read: no other streams, no substorages, no property
  sets, no CLSID check.

Observed in all 10 samples (none of this is needed by a reader, it describes what
writers produced):
- CFB version 3 (major 3, minor 0x3E), 512-byte sectors, 64-byte mini sectors, mini
  stream cutoff 4096. Directory starts at sector 1, mini FAT at sector 2, the mini stream
  at sector 3. No DIFAT sectors (the largest file has 66 FAT sectors).
- The root storage contains only streams: `char.acf` plus `anim1.aaf` ... `animN.aaf`,
  where N = number of animations and the n-th entry of the ACF animation table names
  `anim<n>.aaf` (469 of 469). No unreferenced streams.
- Root CLSID {BB64DF2F-88E4-11D0-9E87-00C04FD7081F}; root modification time set, creation
  time 0; stream entries have null CLSIDs and zero times.
- Every sector of every file is FAT, directory, mini FAT, mini stream, or part of a
  referenced stream; no free or orphan sectors, no trailing bytes. (Slack bytes inside
  the last sector of a stream were not inspected.)

### 4.3 `char.acf` stream

| Off | Type | Field | Notes |
|---|---|---|---|
| 0 | u32 | magic | **0xABCDABC1**. Any other value: 0x80070002 (2.0) / load failure (1.5). This entry point accepts only C1, the flat-file entry point only C2/C4. |
| 4 | u32 | uncompressed size | |
| 8 | u32 | compressed size | |
| 12 | bytes | compressed ACF body | Agent compression (`acs-format.md` section 4). The decoded length must equal the uncompressed size, else 0x80070570 (both runtimes). |

In all 10 samples `12 + compressed size == stream size`, and exactly 3 bytes follow the
byte holding the end marker's last bit (same as 2.0 data). After this block 2.0 runs the
same IE security zone check as for web characters (`acf-aaf-format.md` 10.1) on the file
path, then parses the body.

### 4.4 ACF body, versions 0x1001C, 0x1001E, 0x1001F

Same parser as the 2.0 ACF body (`acf-aaf-format.md` section 6); this table spells out the
branches taken for 1.5 versions. "v" is the first field.

| # | Type | Field | 1.5 rule |
|---|---|---|---|
| 1 | u32 | version | 0x1001C, 0x1001E or 0x1001F |
| 2 | u16 | animation count N | |
| 3 | N x entry | animation table | 4.4.1 |
| 4 | GUID | character GUID | Merlin: same GUID as 2.0 merlin.acs |
| 5 | 3 x string | name, description, extra data | **one** localized entry, no count and no LANGID; the runtime files it as LANGID 0x0009. Microsoft's characters store `{74515580-09AB-11D1-89AB-00C04FC2DAFD}Copyright(c)1997 Microsoft Corporation` as extra data; third-party samples store an empty string. |
| 6 | u16 | width | 128 in 8 samples, Miku 128x256, Sharky 250x250, Totem 200x200 |
| 7 | u16 | height | |
| 8 | u8 | transparent index | |
| 9 | u32 | flags | same meanings as 2.0 (bit 0x20 voice block present, bit 0x100 no balloon block). See 4.4.2. |
| 10 | | animation set version | **absent**; 2.0 reports major 1, minor 0x32 (1.50) |
| 11 | voice block | if flags & 0x20 | GUID engine, GUID mode, u32 speed, u16 pitch. **No** "extra" byte or block (that needs v > 0x20000). Present in all 14 1.5 character files (10 OLE, 4 web); the engine is {1B6BF831-9299-101B-8A19-265D428C60FF} in all 14, the 2.0 Merlin uses {CA141FD0-...}. Sharky and Robby store speed 0xFFFFFFFF, Sharky pitch 0xFFFF (probably "default"; meaning not traced). |
| 12 | balloon block | if !(flags & 0x100) | u8 lines, u8 chars per line, u32 fg, u32 bg, u32 border (COLORREF), string font, i32 height, u32 weight, u8 italic; then **u8 (2.0 calls it underline) only if v > 0x1001E**. Sharky (0x1001E) has no such byte; all 0x1001F samples have it (value 0). |
| 13 | u32 + RGBQUAD[n] | palette | n = 256 in 9 files, 128 in Totem (highest pixel index used: 121). Reserved byte 0 everywhere. Merlin 1.5 palette is identical to 2.0 merlin.acs. |
| 14 | | tray icon | **absent** (needs v > 0x20000) |
| 15 | u16 + state[] | states | string name, u16 n, n x string animation name. 1.5 names are mixed case (`Showing`, `IdlingLevel1`, `GesturingLeft`); lookups are case-insensitive. Characters have 3 to 16 states. |

All 14 ACF bodies (10 OLE, 4 web) are consumed exactly by this layout.

#### 4.4.1 Animation table entry

| Type | Field | Notes |
|---|---|---|
| string | animation name | case-insensitive key |
| string | file name | OLE: `anim<n>.aaf`; web: `<Name>.aaf` (e.g. `RestPose.aaf`) |
| string | return animation | name of the animation played after this one has ended, before the next requested animation (section 5). Used heavily: 129 of the 467 readable OLE animations and e.g. Merlin's `Greet` -> `GreetReturn`. |
| u32 | checksum | only if v > 0x1001D. Repeated in the AAF header (4.5). The web loader compares the two when it checks a cached copy (2.0 0x5a9678c0, 1.5 0x67f94cf8); the OLE loader reads but ignores the AAF copy. Equal in all 469 OLE and 12 web AAFs. |

#### 4.4.2 Flags seen in 1.5 files

| Value | Files |
|---|---|
| 0x00000220 | Merlin, Genie, Robby, Al, Gourdy, Max, Miku, Ozzar, Totem |
| 0x00010220 | Check, Spaceman |
| 0x00030220 | merlinsfx (web) |
| 0x00001220 | Sharky |

- Bit 0x10 is never set in 1.5 files. In 2.0 files it is set exactly when voice is
  absent; 1.5 files all have voice, so this neither confirms nor refutes the "voice
  disabled" reading in `acs-format.md`.
- 0x10000 and 0x20000 lie in the 2.0 balloon-style mask 0xF0F00 (Lebeau: size-to-text,
  no auto-hide). 2.0 merlin.acs has 0x110220.
- **0x1000 (Sharky) is unknown.** It is outside the 0xF0F00 mask and outside bits 5 and
  20. A text search of the 2.0 agentdpv, 2.0 agentdp2 and 1.5.2.416 decompiles found no
  test of 0x1000 outside the decompressor; the 1.5 parser tests only 0x20 and 0x100. A
  getter that hands the whole flags word to agentsvr was not ruled out. A reader can
  ignore the bit.

### 4.5 AAF header (web file and OLE stream)

Identical to the 2.0 ACA header (`acf-aaf-format.md` 5.2); gated by the **ACF** version:

| Off | Type | Field | Notes |
|---|---|---|---|
| 0 | u32 | version | only if ACF v >= 0x1001D, i.e. for 0x1001E/0x1001F. Samples: equal to the ACF version (45 x 0x1001E in Sharky, the rest 0x1001F). Unrecognised values: the web path seeks back 4 bytes and parses headerless (and 2.0 re-downloads); the OLE path does not check the value at all. |
| 4 | u32 | checksum | only with the version field. See 4.4.1. |
| 8 (or 0) | u8 | compression flag | 1 = compressed block follows, 0 = body follows raw. All 481 samples (469 OLE, 12 web): 1. |
| +1 | u32 | uncompressed size | flag 1 only |
| +5 | u32 | compressed size | flag 1 only |
| +9 | bytes | body | Agent compression; decoded length must equal the uncompressed size, else the animation fails to load (0x80070570, 2.0 0x5a962ba2; 1.5 0x67f9b532). |

All samples: `17 + compressed size == stream size`.

ACF version 0x1001C: no version/checksum fields; the stream starts with the flag byte
(binary only).

### 4.6 AAF body for ACF versions below 0x10020

The 1.5 AgentDPv parser (0x67f95822, frames 0x67f95bcd, overlays 0x67f95d46) has no
version gates at all and reads exactly the layout below. 2.0 agentdpv (0x5a9695d7,
0x5a968ae5, 0x5a968618) produces the same layout for every ACF version up to 0x1001F
because all its extra fields need v >= 0x10020 or v > 0x1001F.

| # | Type | Field | Notes |
|---|---|---|---|
| 1 | u16 | sound count S | |
| 2 | S x (u32 size, bytes) | sounds | complete RIFF WAVE files |
| 3 | u16 | image count I | |
| 4 | I x image | images | 4.7 |
| 5 | | transition type | **absent**. 2.0 initialises it to 0 = "play the return animation" (constructor 0x5a968a49). |
| 6 | u16 | frame count F | 0 is rejected (0x8004200F) |
| 7 | F x frame | frames | 4.8 |

Every one of the 467 readable OLE animation bodies and 12 web bodies ends exactly after
the last frame.

### 4.7 Image and sound records

Image (same as 2.0 ACA 7.1):

| Type | Field | Notes |
|---|---|---|
| u32 | size | pixel byte count; 0 = empty image, nothing follows (binary only, no sample) |
| u8 | unknown | only if size != 0; 0 in all 3,318 1.5 images |
| bytes[size] | pixels | 8-bit palette indices, bottom-up rows, stride `(width + 3) & ~3` of the **character** size. Every 1.5 image is a full pre-composited frame: size == stride x height in 3,318 of 3,318. |
| u32 | region size R | only if size != 0 |
| bytes[R] | region | raw Win32 RGNDATA (`dwSize` 32, `iType` 1, nCount rects of i32 left/top/right/bottom in top-down coordinates). Not compressed. Equal to the set of non-transparent pixels in 3,318 of 3,318. |

Sounds: 60 RIFF WAVE files in the OLE samples, `RIFF length + 8 == size` for all.
Formats as (tag, channels, rate, bits): PCM (1,1,22050,8) x30, (1,1,11025,8) x13,
(1,1,11127,8) x7, (1,1,22050,16) x3, (1,1,44100,8), (1,1,44100,16), (1,2,22050,16);
MS ADPCM (2,1,22050,4) x4. The web merlinsfx/Show.aaf has one MS ADPCM 11025 Hz sound.
Microsoft's 1.5 Merlin, Genie and Robby animations carry **no sound** in the samples
(merlinsfx is the variant with sound effects).

### 4.8 Frame and overlay records

Frame (v < 0x10020):

| Type | Field | Notes |
|---|---|---|
| u16 | image index | into this animation's images; 0xFFFF = no image (1 sample frame, in Al) |
| u16 | sound index | into this animation's sounds; 0xFFFF = none |
| u16 | duration | same unit as 2.0 (1/100 s per the 2.0 notes). Samples: mostly 10, then 4, 5, 6, 100; never 0. |
| i16 | offset X | 0 in all 3,807 OLE + 89 web frames; not used by agentdpv |
| i16 | offset Y | same |
| | exit branch | **absent**; 2.0 stores -1 (frame record 0x5a968ae5): "end the animation when exiting" |
| u8 | branch count B | |
| B x (u16 target, u16 percent) | branches | same semantics as 2.0. 28 OLE frames have branches (no web frame); their probability sums are 100 (19 frames), 70 (3), 75 (2), 50 (2), 40 (2). |
| u8 | overlay count O | |
| | base image block | **absent** (needs v > 0x1001F) |
| O x overlay | mouth overlays | below |

Overlay (v <= 0x1001F):

| Type | Field | Notes |
|---|---|---|
| u8 | mouth type | 0..6, stored in a slot per type |
| u32 | size | 0 = empty overlay, nothing else follows (Check.acs: one frame with all 7 overlays empty) |
| u8 | reserved | only if size != 0; 0 in all 671 non-empty overlays (636 OLE, 35 web) |
| | replace-base-frame byte | **absent** (needs v > 0x1001F); treated as 0 |
| i16, i16 | x, y | top-left of the mouth rectangle in frame coordinates (top-down) |
| u16, u16 | w, h | real rectangle size |
| bytes[size] | pixels | 8 bpp, bottom-up rows, stride `(w + 3) & ~3`; size == stride x h in 671 of 671 |

In the 9 third-party files, overlay frames always carry types 1..6 and carry type 0
("closed") in 39 of 68 frames. Drawing follows `acf-aaf-format.md` 8.1 with "replace base frame" = 0: copy the
overlay onto the frame image with colour keying; the window shape stays the frame's
region.

### 4.9 Flat 1.5 `.acf` file

Exactly the 2.0 ACF wrapper with magic 0xABCDABC2: `u32 magic, u32 uncompressed size,
u32 compressed size, compressed ACF body` (`acf-aaf-format.md` 5.1); body as 4.4 with
v = 0x1001F in all 4 samples. 2.0 treats C2 and C4 identically. The 1.5 runtime accepts
only C2 (0x67f935e3; anything else gives 0x80042204). Animations are separate `.aaf`
files named by the ACF table and located relative to the ACF, as in 2.0
(`acf-aaf-format.md` section 10). For a local flat file, the 1.5 runtime opens it through
`StgCreateDocfile` with STGM_CONVERT | STGM_TRANSACTED (mode 0x30012) and reads the
stream `CONTENTS`; per the Win32 documentation that is how OLE exposes a non-OLE file's
bytes as a stream, and nothing is committed. It is an OS-level read path, not another
container format.

## 5. Mapping to 2.0 concepts

| 2.0 ACS concept | 1.5 equivalent |
|---|---|
| Header with 4 locators | OLE storage; `char.acf` stream; animation streams by name |
| Character info (version 0x20001) | ACF body 0x1001F/0x1001E/0x1001C, compressed as a whole |
| Localized info list | one entry (name, description, extra), LANGID 9 implied |
| Animation set version | absent, reported as 1.50 |
| Voice with extra (language, gender, age, style) | voice without extra |
| Balloon | same fields; trailing byte only for v > 0x1001E |
| Tray icon | absent |
| States | same, mixed-case names |
| Animation list (name -> locator) | ACF table (name, stream/file name, return animation, checksum) |
| Animation record: transition type | absent, behaves as 0 (use return animation) |
| Animation record: return animation | in the ACF table |
| Shared image pool, per-image size, frames layering several images with offsets | per-animation image list, every image a full composited frame, one image per frame |
| Image compression per image, region optionally compressed | whole AAF body compressed once; images and regions raw inside |
| Shared audio pool | per-animation sound list |
| Frame exit branch -1/-2/index | absent, behaves as -1 |
| Branches | identical |
| Mouth overlays referencing pool images, "mouth point" | inline cropped pixels with a real rectangle |
| Phoneme to mouth table | same table in the provider (`acf-aaf-format.md` 8.1) |

**Playback consequence.** agentdpv's switch routine (0x5a96834a, with the exit test
0x5a968d98) runs when a new animation is requested while one is current. It first tries
"exit mode", which needs the current frame's exit value to be other than -1. Because the
runtime stores -1 for every 1.5 frame, a 1.5 animation **never enters exit mode**. The
routine then looks at two flags handed in by the caller. If the first flag is set, the
current animation has a return animation in the ACF table, that return animation is not
the one requested, and it loads, then the return animation is played and the requested
one is remembered to follow. Otherwise the requested animation replaces the current one
at once. Reading the two caller flags as "current animation is at its end" and "request
was queued" follows the ACS note's reading of the equivalent agentdp2 routine
(`acs-format.md` 5.2/5.3); in agentdpv itself their meaning is **inferred**, since the
caller in agentsvr was not traced. Under that reading: a 1.5 animation interrupted
mid-play is cut immediately, and one that has finished plays its return animation
before the next request. For 1.5 data the transition value cannot change the outcome,
because the exit test fails anyway. 1.5 characters
rely on return animations: in Merlin 1.5, 31 animations name one and 38 animations are
called `*Return`.

## 6. Differences from 2.0 (short list)

1. Container: OLE compound file or flat ACF + AAF files, instead of one flat file with
   locators.
2. Magics: 0xABCDABC1 (OLE `char.acf`), 0xABCDABC2 (flat ACF).
3. Versions 0x1001C/0x1001E/0x1001F with the gates in 4.4 to 4.8.
4. Strings without terminator (ACF rule), unlike 2.0 ACS.
5. Images pre-composited per animation; no layering, no per-image compression.
6. No transition byte, no exit branches, no base-image block, no replace-base-frame byte.
7. Return animations as ordinary separate animations, named in the ACF table.
8. One localized entry, no tray icon, no voice extra, no animation set version.
9. Overlays carry their own pixels and a real rectangle (as in 2.0 ACA, not as in 2.0 ACS).
10. Checksum per animation (ACF vs AAF header); ignored in the OLE container.

## 7. Version matrix: what is verified

| Case | Samples | Status |
|---|---|---|
| OLE container, `char.acf` magic C1 | 10 | verified |
| Flat ACF magic C2 | 4 | verified |
| ACF 0x1001F (balloon trailing byte present) | 13 | verified |
| ACF 0x1001E (no balloon trailing byte, headered AAF) | 1 (Sharky, 45 AAF) | verified |
| ACF 0x1001C (no checksum, headerless AAF) | 0 | binary only |
| AAF compression flag 0 (raw body) | 0 | binary only |
| Image size 0 | 0 | binary only |
| Image index 0xFFFF | 1 frame | verified |
| Overlay size 0 | 7 overlays | verified |
| Sounds PCM and MS ADPCM inside AAF | 61 | verified |
| Branches | 28 frames | verified |
| Frame offsets X/Y nonzero | 0 | not seen; ignored by agentdpv |
| Damaged compressed data | 2 streams (Ozzar) | see 8.2 |

## 8. Verification

Throwaway reference scripts (not public), all run with `python3 -I`:
- `cfb.py`: own MS-CFB reader with a sector classifier.
- `ole15.py`: full parse of OLE files (asserts no unclassified sector, no extra stream,
  every body consumed). Output: `out/ole15_all.txt`.
- `run15.py`: full parse of web ACF + AAF. Output: `out/run15_web.txt`.
- `rgn15.py`: region vs opaque pixels; renders a few frames to `out/png/` (scratch only).
- `xcheck15.py`, `align15.py`: Merlin 1.5 vs 2.0.
- `lztrace.py`, `flipsearch.py`: token-level decoder and single-bit-flip search for the
  damaged Ozzar streams (outputs `out/flip60.txt`, `out/flip71_*.txt`).
- `acfparse.py`, `agentlz.py`: copied unchanged from the ACF researcher's scratch; their
  existing version gates parse every 1.5 sample without modification.

### 8.1 Results

| Check | Command | Result |
|---|---|---|
| OLE parse | `ole15.py <the 10 files of 3.1>` | 10 files; every sector classified (FAT, directory, mini FAT, mini stream, stream), no extra streams; 10 ACF bodies consumed exactly; 467 of 469 AAF bodies decode and are consumed exactly (2 damaged, 8.2); all 469 AAF headers: version = ACF version, checksum = ACF entry, flag 1, `17 + compressed size` = stream size; 3,807 frames, 3,237 images, 643 overlays, 60 sounds; all indices in range |
| Web parse | `run15.py <genie, merlin, merlinsfx, robby dirs>` | 4 ACF + 18 AAF copies (12 distinct): headers 0x1001F, checksums equal to the ACF, bodies consumed exactly; in the 12 distinct AAF: 89 frames, 81 images, 35 non-empty overlays, 1 sound |
| Regions | `rgn15.py` over all of the above | 3,318 of 3,318 regions equal the non-transparent pixels |
| Image size | both parsers | 3,318 of 3,318 images are stride x height of the character |
| Overlay size | both parsers | 671 of 671 non-empty overlays (636 OLE, 35 web) are stride x h |
| Visual | `PNG=out/png rgn15.py ...` | Merlin Greet and other frames render correctly (orientation, colours, transparency) |

### 8.2 The two damaged streams (Ozzar.acs)

- `anim60.aaf` (DontRecognizeReturn) decodes cleanly to the end marker but yields 126,119
  bytes instead of the declared 126,121. The first image is 2 bytes short, so its region
  size reads as 0x200000.
- `anim71.aaf` (StopListening) hits a back-reference before the start of the output after
  45,547 of 107,812 bytes, inside its third image.
- The OLE structure around them is intact (no shared or orphan sectors, chains have the
  expected length), and the file matches the CRC-32 recorded for the collection. The damage is therefore in
  the published file itself.
- Both runtimes reject these streams: the block readers (2.0 0x5a962ba2, 1.5 0x67f9b532)
  fail on a decoder error or on any length mismatch. In the OLE path only that one
  animation fails to load (0x80070570 returned from 0x5a965373); the character and its
  other animations are unaffected because animations are opened lazily.
- A single-bit-flip search (`flipsearch.py`) was run to test whether one flipped bit
  explains the damage. The criterion was: full decode to the declared size, a parse, and
  every region equal to its opaque pixels.
  - `anim60.aaf`: flipping bit 4 of byte 330 or bit 0 of byte 331 of the compressed
    block (stream offsets 347/348; both bits belong to the same code token, which starts
    at output byte 1,350) makes the stream decode to exactly 126,121 bytes, parse, and
    give 7 of 7 regions equal to their opaque pixels. The search covered compressed bits
    71 to about 8,300 of 46,690 before it was stopped. Dozens of other flips also restore
    the length and parse, but leave all 7 regions wrong. So anim60 is explained by a
    single bit error; which of the two bits cannot be decided.
  - `anim71.aaf`: the search covered every bit of the compressed data that produces
    output bytes 36,294 to 45,547 (the third image up to the failure point; compressed
    bits 87,013 to 101,127, all of them). Exactly one flip qualifies: bit 1 of byte
    12,564 of the compressed block (stream offset 12,581, in the token that starts at
    output byte 45,206). With it the stream decodes to exactly 107,812 bytes, parses, and
    all regions equal their opaque pixels.
  - Both damaged streams are therefore explained by one flipped bit each. The two headers
    are intact (version 0x1001F, checksum equal to the ACF entry, flag 1, sizes
    consistent, leading 0x00 and six trailing 0xFF).
- The other 104 Ozzar streams and all other samples decode exactly with the same
  algorithm. The 1.5 decoder (1.5.2.416 0x67f9a495) has the same validation (input at
  least 8 bytes, leading 0x00, six trailing 0xFF) and the same distance classes (6, 9, 12,
  20 bits with bases 1, 65, 577, 4673; end marker 0xFFFFF; length bonus +1 for the 20-bit
  class). It is compiled differently but encodes the same algorithm. So this is damage
  (bit errors in the published file), not a format variant.
- **For bentwire:** treat a decode error or length mismatch as "animation unavailable",
  not as "character corrupt", to match the runtime.

### 8.3 Same character in 1.5 and 2.0: Merlin

- **1.5 web vs 1.5 OLE:** the web `merlin/Blink.aaf` and `merlin/Show.aaf` have the
  same checksums as two streams of the build-1027 `merlin.acs`, and they are
  **byte-identical** to them. The web `merlin.acf` body and the OLE `char.acf` body are
  field-for-field equal (names, return animations, checksums, GUID, localized strings,
  size, flags, voice, balloon, palette, states) except the file-name column
  (`Blink.aaf` vs `anim2.aaf`). So the docfile is a container around the web files.
- **1.5 vs 2.0 (Agent 2.0 `merlin.acs`):**
  - Same GUID {4E574F44-B521-11D0-9E9A-00C04FD7081F}, size 128x128, transparent index
    10, identical 256-entry palette, same balloon except 28 vs 32 chars per line, same
    16 state names (1.5 mixed case, 2.0 upper case).
  - 1.5 has 104 animations, 2.0 has 73; 65 names are shared. 1.5 has 39 animations 2.0
    lacks: the `*Return` animations, `GlanceDown/Left/Right/Up`, and `Hear_1..4`
    (renamed `Hearing_1..4` in 2.0).
  - Image content: **725 of 755** 1.5 frame images are byte-identical to some composited
    2.0 frame (2.0 frames composited from its layered images, last listed drawn first).
    The 30 others belong to animations that were redrawn: GreetReturn 5,
    GestureLeftReturn 3, GestureRightReturn 3, Idle3_1Return 7, Idle2_2 2, Write 3,
    DoMagic2 2, Read 2, ReadContinued 2, WriteContinued 1.
  - Mouth overlays: **184 of 196** 1.5 overlays match a 2.0 overlay exactly in type,
    position and opaque pixels.
  - Sequences (aligned by image hash): 11 animations have identical image sequences (9 of
    them with retimed durations, 1.5 often 5 where 2.0 has 10). In 41 more, every image of
    the 1.5 animation (and of its return animation, if it has one) occurs somewhere in
    the 2.0 animation. For the animations that have a return animation, this fits 2.0
    folding the return into the animation via exit branches (transition type 1), but
    the plain concatenation "animation + return" never matched exactly, and some 2.0
    versions are shorter (Decline, GetAttention, Wave). 12 overlap partly, 1 (`Idle3_1`)
    has no image in common.
  - Conclusion: Microsoft re-authored the timing and structure of Merlin for 2.0 but
    reused the 1.5 artwork. Pixel format, palette, orientation, transparency, regions
    and overlay geometry decode identically. A positional frame-by-frame comparison is
    not meaningful beyond the 11 identical animations.

## 9. Evidence

agentdpv.dll 2.00.0.3427 (image base 0x5a960000; decompiled by the ACF researcher with
REA `decompile-all`, Ghidra 12.1.4; reused read-only):

| Address | Role for 1.5 data |
|---|---|
| 0x5a964d61 | OLE path: `StgOpenStorage` with retries, stream `char.acf`, magic 0xABCDABC1, block reader, zone check, ACF parser |
| 0x5a965373 | OLE path: animation stream by ACF file name; header read when v >= 0x1001D, checksum not compared; compression flag; animation object gets the ACF version |
| 0x5a966d0d | flat ACF: magic C2 or C4 |
| 0x5a962ba2 | compressed block reader: rejects decoder failure and any length mismatch (0x80070570) |
| 0x5a96a5ee | ACF body parser with the version gates of 4.4 |
| 0x5a968a49 | animation object constructor: transition byte initialised to 0 |
| 0x5a9695d7 | AAF body: transition byte only if v >= 0x10020 |
| 0x5a968ae5 | frame: exit set to 0xFFFF (-1) if v < 0x10020; base image block only if v > 0x1001F |
| 0x5a968618 | overlay: replace-base-frame byte only if v > 0x1001F |
| registry script (strings) | {D45FD2FF} "DocFile Provider 2.0"; {BB64DF2F} "DocFile Provider 1.5" TreatAs {D45FD2FF}; `.acf`/`.aca` -> {D45FD300}; FileType patterns C4 and C2 |

agentsvr.exe 2.00.0.3427: 0x0101F0FC..0x0101F12C reads 4 bytes and tests 0xABCDABC3;
0x0101F16D calls `GetClassFile` (IAT 0x0100150C), compares with {BB64DF2F} and {1EAF7042},
maps to the CLSIDs stored at 0x0100181C {D45FD2FF}, 0x0100182C {D45FD300} and 0x0100184C
{D45FD301}; 0x0101F2B6 `CoCreateInstance` (IAT 0x01001538).

AgentDPv.dll 1.5.2.416 (image base 0x67f90000; decompiled for this note with
`rea decompile-all`, Ghidra, 470 functions, output kept in a scratch directory):

| Address | Role |
|---|---|
| 0x67f91fe8 | OLE path: `StgOpenStorage`, `char.acf`, magic 0xABCDABC1 |
| 0x67f929f4 | OLE animation stream: header if v >= 0x1001D, flag, body (no version passed to the animation) |
| 0x67f935e3 | flat ACF: URL cache or local file via `StgCreateDocfile` STGM_CONVERT and stream `CONTENTS`; magic must be 0xABCDABC2 (else 0x80042204) |
| 0x67f94cf8 | web AAF header: accepts 0x1001C/0x1001E/0x1001F, else seeks back 4 bytes; in its cache-check mode compares the checksum with the ACF entry |
| 0x67f9693c | ACF body parser: version check (0x1001C/0x1001E/0x1001F), table with checksum if v > 0x1001D, GUID, 3 localized strings, size, transparent index, flags, voice if bit 0x20, balloon unless bit 0x100, palette, states |
| 0x67f96d05 | balloon block; trailing byte if v > 0x1001E |
| 0x67f96dba | states |
| 0x67f9a278, 0x67f9a2ed | string readers (u32 count, no terminator, no limit); the second converts to the ANSI code page in memory (used for localized strings and the font name); the bytes on disk are the same |
| 0x67f95822 | AAF body: sounds, images, frame count (0 rejected with 0x8004200F), frames |
| 0x67f95bcd | frame: image, sound, duration, offset X, offset Y, branches, overlays |
| 0x67f95d46 | overlay: type, size, then reserved byte, x, y, w, h, pixels if size != 0 |
| 0x67f9b532, 0x67f9a495 | block reader (exact length) and decompressor |

agentdpv.dll 1.5.0.225 (base 0x7f5c0000, output `rea/dpv225/`): the same version gates at
0x7f5c6744 (ACF), 0x7f5c4acc (AAF header), 0x7f5c28b1 (OLE animation), 0x7f5c1f6a (OLE
`char.acf`, magic C1), 0x7f5c3549 (flat ACF, magic C2), 0x7f5c6b0d (balloon byte).

Hand-checkable bytes:
- `char.acf` of Merlin (1027): `c1 ab cd ab 45 24 00 00 29 0d 00 00 00 3e 00 08 ...`
  (magic C1, uncompressed 0x2445 = 9285, compressed 0x0d29 = 3369, then the leading 0x00
  byte of the compressed data).
- `anim2.aaf` of Merlin (1027) = web `merlin/Blink.aaf`: `1f 00 01 00 | 5e e6 ee 82 | 01 |
  c1 ce 00 00 | 38 13 00 00` (version 0x1001F, checksum 0x82EEE65E, compressed,
  uncompressed size 52,929, compressed size 4,920).
- Sharky `anim1.aaf` starts `1e 00 01 00` (version 0x1001E).

## 10. Unknowns and open points

| Item | Status | Impact |
|---|---|---|
| ACF 0x1001C, headerless AAF, raw (flag 0) AAF, empty images | binary only, consistent in 1.5.0.225, 1.5.2.416 and 2.0 | low; implement per 4.5 to 4.7 |
| Which tool versions wrote 0x1001C / 0x1001E / 0x1001F | not determined; 0x1001E sample dated Dec 1997 | none for reading |
| Flag bit 0x1000 (Sharky) | no reader found in any provider | none |
| Voice speed 0xFFFFFFFF / pitch 0xFFFF | probably "use engine default"; consumer in agentsvr not traced | low |
| Extra data `{74515580-...}Copyright(c)1997 Microsoft Corporation` | meaning not traced (looks like an authenticity marker); 2.0 uses the field for greeting phrases | low |
| Image byte after the size, overlay reserved byte | always 0, not used by the 2.0 provider | none |
| Microsoft 1.5 Genie, Robby as OLE `.acs`; any 1.5 Peedy | not found (section 3.4); Genie's and Robby's 1.5 web ACF and a few AAF exist | coverage only; the format is the same |
| How the 1.5 agentsvr chose a provider | not analysed; irrelevant for bentwire, which follows 2.0 | none |
| Agent beta files (.ani/.mth/.seq/.bmp) | not read by any runtime examined | none |
| Slack bytes in partially used OLE sectors | not inspected | none |
