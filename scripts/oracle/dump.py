"""Oracle driver for the Rust round-trip check (architecture A-TEST-2).

Runs the Python prototype's oracle (canonical dump + interpreter A from the source file) or its
reader (from a converted character) and writes JSON: {"dump": ..., "traces": {name: [trace...]}}.
Traces follow the shared plan, mirrored in bentwire_character::dump::{trace_params, all_traces}:
for every animation, seeds 0..12 x modes 0..3; splitmix64 parameters and draws, draw(s) = next() % s.

Must run against a SCRATCH COPY of the prototype (it writes pickles into its cache/):
  PYTHONDONTWRITEBYTECODE=1 python3 -I scripts/oracle/dump.py --proto COPY oracle KIND SOURCE OUT.json
  PYTHONDONTWRITEBYTECODE=1 python3 -I scripts/oracle/dump.py --proto COPY reader CONVERTED OUT.json
"""
import hashlib
import json
import os
import sys

MASK = (1 << 64) - 1
SEEDS = 12
LIMIT = 300


class SplitMix64:
    def __init__(self, seed):
        self.s = seed & MASK

    def next(self):
        self.s = (self.s + 0x9E3779B97F4A7C15) & MASK
        z = self.s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def draw(self, s):
        return self.next() % s


def trace_params(seed, mode, frames):
    p = SplitMix64(seed * 7919 + mode)
    exit_at = None if mode == 0 else 1 + p.draw(1 + 2 * frames)
    dp = p.draw(4)
    host = p.draw(7)
    return exit_at, dp, host, seed


def traces(obj, animations):
    out = {}
    for a in animations:
        name, nframes = a[0], len(a[3])
        lst = []
        for seed in range(SEEDS):
            for mode in range(3):
                exit_at, dp, host, s = trace_params(seed, mode, nframes)
                g = SplitMix64(s)
                lst.append(obj.trace(name, g.draw, exit_at, dp, host, LIMIT))
        out[name] = lst
    return out


def content_tag(path):
    """SHA-256 prefix of a source file, or of the files of a source folder (name and bytes)."""
    h = hashlib.sha256()
    if os.path.isdir(path):
        for n in sorted(os.listdir(path)):
            p = os.path.join(path, n)
            if os.path.isfile(p):
                h.update(n.encode() + b"\0")
                with open(p, "rb") as f:
                    h.update(f.read())
    else:
        with open(path, "rb") as f:
            h.update(f.read())
    return h.hexdigest()[:16]


def main(argv):
    if len(argv) < 3 or argv[0] != "--proto":
        sys.exit(__doc__)
    proto = os.path.abspath(argv[1])
    ref = os.environ.get("BENTWIRE_REFERENCE")
    if ref and os.path.commonpath([os.path.realpath(proto), os.path.realpath(ref)]) == os.path.realpath(ref):
        sys.exit("refusing to run inside BENTWIRE_REFERENCE: pass a scratch copy of the prototype")
    sys.path.insert(0, proto)
    cmd, rest = argv[2], argv[3:]
    if cmd == "oracle":
        kind, src, out = rest
        import oracle
        import sources
        # The prototype caches by file name; add the content hash to the key so a reused
        # scratch copy never serves a pickle of another file with the same name.
        tag = content_tag(src)
        by_name = sources.cached
        sources.cached = lambda key, fn: by_name(key + "_" + tag, fn)
        raw = sources.load(kind, src)
        o = oracle.Oracle(raw)
        d = json.loads(json.dumps(o.dump()))
        res = dict(dump=d, traces=traces(o, d["animations"]), is_wmf=bool(getattr(o, "is_wmf", False)),
                   missing=list(raw.get("missing", [])), broken=[b[0] for b in raw.get("broken", [])])
    elif cmd == "reader":
        src, out = rest
        import reader
        c = reader.Character(src)
        d = json.loads(json.dumps(c.dump()))
        res = dict(dump=d, traces=traces(c, d["animations"]))
    else:
        sys.exit(__doc__)
    with open(out, "w", encoding="utf-8") as f:
        json.dump(res, f, ensure_ascii=False)


if __name__ == "__main__":
    main(sys.argv[1:])
