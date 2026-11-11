"""Dump Agent 1.5 OLE .acs files as JSON with the research parsers cfb.py + acfparse.py.

Usage: PYTHONDONTWRITEBYTECODE=1 python3 -I agent15_dump.py --reference DIR --out OUTDIR FILE...

Copies agentlz.py, acfparse.py and cfb.py (from the directory below the reference directory
that holds all three) into a fresh temp directory and imports them from there. Writes OUTDIR/<basename>.json. An animation stream that the
research parser rejects (the damaged Ozzar streams) is written as "error". JSON layout as
acf_dump.py; mirrored by crates/agent-formats/tests/oracle/mod.rs (ole15_json).
"""
import argparse
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from acf_dump import animation_json, character_json, load_modules  # noqa: E402


def dump(cfbm, ap, lz, path):
    c = cfbm.CFB(open(path, "rb").read())
    byname = {e["name"].lower(): e for e in c.children(0) if e["type"] == 2}
    s = c.stream(byname["char.acf"])
    magic, ulen, clen = struct.unpack_from("<III", s)
    assert magic == 0xABCDABC1
    body, _ = lz.decompress(s[12:12 + clen], ulen)
    ch = ap.parse_acf_body(body)
    v = ch["version"]
    anims = []
    for name, fname, ret, ck in ch["anims"]:
        try:
            ver, ck2, flag, ab = ap.unwrap_aca(c.stream(byname[fname.lower()]), v, None)
            anims.append(animation_json((ver, ck2, flag), ap.parse_aca_body(ab, v), v))
        except Exception:
            anims.append("error")
    return {"root_clsid": c.hdr and c.entries[0]["clsid"], "character": character_json(ch), "animations": anims}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--reference", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("files", nargs="+")
    a = p.parse_args()
    lz, ap, cfbm = load_modules(a.reference, ("agentlz", "acfparse", "cfb"))
    for f in a.files:
        with open(os.path.join(a.out, os.path.basename(f) + ".json"), "w") as o:
            json.dump(dump(cfbm, ap, lz, f), o, ensure_ascii=False)
        print("dumped", f)


if __name__ == "__main__":
    main()
