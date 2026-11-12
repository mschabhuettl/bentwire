# bentwire decisions

Where the research left a gap, the build chose a behaviour and recorded it, with its reason,
in `docs/spec.md` section 3.14 (spec T-7). This table is an index of those entries. The decisions taken while
writing the spec itself are marked **bentwire decision** in its sections 3.1 to 3.13.

| ID | Summary |
|---|---|
| A-PACK-1 | Packs (`pack.toml`) and search roots (`character_paths`, XDG dirs, `--characters`) |
| A-ID-1 | Unique character ids in `classics` (`clippit`; suffixes `-web`, `-15`, `-97`, `-bob`, `-act`) |
| A-PACK-2 | `classics` holds every source that converts with no missing animations |
| A-CHAR-1 | With only `builtin` installed, `bentwire-placeholder` is the default |
| A-TEST-1 | Real-data tests read `BENTWIRE_SOURCES`, `BENTWIRE_REFERENCE` and `BENTWIRE_PACKS` (instead of `BENTWIRE_SAMPLES`) and fail with "NOT RUN" when unset |
| A-THEME-1 | The Office balloon theme is pack data with a generic loader; the look-alike is the fallback |
| A-TEST-3 | `--test-interface` exports a reduced `Test1` on the Wayland backend |
| B-REN-1 | Text: advance-only, whole-pixel advances, OS/2 win metrics, cell versus em heights, fontconfig fallback |
| B-REN-2 | Look of the own Office balloon art: button states, check boxes, hot labels, alert icon |
| B-REN-3 | XP-style menu metrics and colours |
| B-REN-4 | HiDPI input shape: a logical pixel counts if any covered buffer pixel is opaque |
| B-REN-5 | Headless frame dumps: one transparent PNG per output, fixed z-order |
| B-REN-6 | Balloon outlines as GDI-style regions; the region is also the input region |
| B-REN-7 | Balloon theme file format (`theme.toml`, button strips, bullets, icons) |
| B-REN-8 | Office balloon font size is the Tahoma cell height (13 / 16 px) |
| A-FMT-1 | Parser memory and size limits against hostile input |
| A-FMT-2 | Strict record consumption and size checks in ACS, ACF, ACA/AAF |
| A-FMT-3 | Format edge cases without samples (image kind 0, raw images, mouth types, UTF-16) |
| A-FMT-4 | Agent 1.5 and web characters fail per animation, not per file |
| A-CORE-1 | Office balloon sizing constants (area terms, minimum size, shape choice) |
| A-CORE-2 | Office balloon controls: check-box rows, mnemonics, where markup applies, argument errors |
| A-CORE-3 | Small engine choices (interrupts, double-click, gallery, drags during slides, Animate! shuffle) |
| A-CORE-4 | office97 transition checks and per-callback frame caps |
| A-CORE-5 | Agent balloon tail position and pixel-step scrolling |
| A-CORE-6 | Exit runs end when they revisit a frame |
| A-CHAR-2 | Loader strictness: errors versus warnings, accepted PNG variants |
| A-CHAR-3 | HiDPI sheet choice per layer from the destination rectangle |
| A-CHAR-4 | Character sanity limits; oversized output composes to an empty picture |
| A-CONV-1 | Vector actors rasterised at 1x and 2x with their exact metafile colours |
| A-CONV-2 | Sounds deduplicated by PCM, ids `sNN` |
| A-CONV-3 | ACT language, codepage and Office palette are converter options |
| A-CONV-4 | Agent 1.5 files record no animation set version |
| A-THEME-2 | How the OBALLOON.DLL bitmaps become the theme's strips and icons |
| A-CONV-5 | Ids for file names with a parenthesised qualifier; identical sources converted once |
| A-CONV-6 | Out-of-range indices in source data mean no layer, no sound or transparent |
| A-CONV-7 | Bitmap actors without a known palette take their colours from a sibling actor with the same pictures |
| A-PACK-3 | Fully transparent helper actors are excluded from `classics` |
| A-LABEL-1 | Display names are labels "<name> (<product>)", product from the format or `pack.toml`, never empty |
| W-RAST-1 | WMF geometry: mapping mode, pixel-centre coverage, pen widths |
| W-RAST-2 | Ellipse and RoundRect pens |
| W-COL-1 | WMF colours map to the nearest palette entry |
| W-ROP-1 | ROP2 codes evaluated per channel on palette colours |
| W-TEXT-1 | WMF TextOut drawn with the bundled Tahoma |
| A-DESK-1 | `Ask` rejects unknown keys |
| A-DESK-2 | CLI details: `characters` marker column, `ask` exit codes, `convert` options |
| A-DESK-3 | `Test1` arguments and hit-testing |
| A-DESK-4 | Headless trace format and frame dump names |
| A-DESK-5 | Wayland details: applied-position tracking, fractional scale, balloon subsurfaces |
| A-DESK-6 | Daemon flags, config error handling, signals, tray mnemonics |
| A-ACT-1 | ACT parser strictness and the runtime leniencies it keeps |
