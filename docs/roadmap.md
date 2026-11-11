# Roadmap

Planned for 0.2. Nothing here is promised for a date; the list says what is known to be
missing or unproven after 0.1.1.

## Performance

- **Faster first frame.** Today the first frame of a character takes 0.7 to 4 ms, against
  0.05 to 0.2 ms for an ACS file. Planned: cache the decoded sheets in `~/.cache/bentwire/`
  and load them with mmap, preload the default character at daemon start, and put the frames
  of the Show animation first in the sheet.

## Characters

- **`bentwire characters install classics`**: download the classics pack from its release
  instead of cloning the repository by hand.
- **Optional smoothing for vector characters** (character format decision D2). Vector actors
  are rasterised aliased today.
- **The size of the classics pack** (about 74 MB, mostly the 2x sheets of the vector
  characters).
- **An own CC0 character for `builtin`**, so that screenshots without Microsoft art are
  possible.

## Fidelity

- **Office theme:** the order of the button states and the Office balloon size constants
  (A-CORE-1 in `docs/spec.md` 3.14) were chosen, not proven. Prove them from `OBALLOON.DLL`.
- **A test for the time-of-day query of Office 97 animations.**

## Tests

- **Watch the shell integration test that failed once** (`clients/shell/test.sh`).
- **Findings from testing on a real Plasma desktop** by hand: the points listed under
  "Known gaps" in the [README](../README.md#known-gaps) that the automated runs cannot
  prove.
