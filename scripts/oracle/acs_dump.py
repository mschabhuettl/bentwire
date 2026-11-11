"""Dump ACS 2.0 files as JSON with the research parser acs.py (the independent oracle).

Usage: PYTHONDONTWRITEBYTECODE=1 python3 -I acs_dump.py --research DIR --out OUTDIR FILE...

Copies $BENTWIRE_REFERENCE/acs.py into a fresh temp directory and imports it from there, so
nothing is written next to the research files. Writes OUTDIR/<basename>.json per input.
The JSON layout is mirrored by crates/agent-formats/tests/oracle/mod.rs (acs_json).
"""
import argparse
import hashlib
import json
import multiprocessing
import os
import shutil
import struct
import sys
import tempfile


def sha(b):
    return hashlib.sha256(bytes(b)).hexdigest()


def load_module(research):
    d = tempfile.mkdtemp(prefix="bentwire-acs-oracle-")
    shutil.copy(os.path.join(research, "acs.py"), d)
    sys.path.insert(0, d)
    import acs  # noqa: E402
    sys.path.remove(d)
    shutil.rmtree(d)
    return acs


class All:
    def __contains__(self, x):
        return True


def dump(acs, path):
    st = acs.parse(path, verbose=False, want_png=All())
    d = open(path, "rb").read()
    hdr = struct.unpack_from("<9I", d, 0)
    r = acs.R(d, hdr[3], hdr[3] + hdr[4])
    names = [(r.string(), r.u32(), r.u32())[0] for _ in range(r.u32())]
    r = acs.R(d, hdr[5], hdr[5] + hdr[6])
    ients = [(r.u32(), r.u32(), r.u32()) for _ in range(r.u32())]
    r = acs.R(d, hdr[7], hdr[7] + hdr[8])
    aents = [(r.u32(), r.u32(), r.u32()) for _ in range(r.u32())]
    ci = st["ci"]
    out = {
        "version": ci["version"],
        "guid": ci["guid"],
        "width": ci["width"],
        "height": ci["height"],
        "transparent": ci["transparent"],
        "flags": ci["flags"],
        "animation_set": [ci["anim_major"], ci["anim_minor"]],
        "palette": [ci["npal"], sha(ci["palette"])],
        "tray_icon": [sha(ci["icon_mask"]), sha(ci["icon_color"])] if ci["has_icon"] else None,
        "states": [[s, a] for s, a in ci["states"]],
        "localized": [list(l) for l in ci["localized"]],
        "voice": None,
        "balloon": None,
    }
    if "voice" in ci:
        v = ci["voice"]
        out["voice"] = {
            "engine": v["engine"], "mode": v["mode"], "speed": v["speed"], "pitch": v["pitch"],
            "extra": [v["langid"], v["dialect"], v["gender"], v["age"], v["style"]] if v["has_extra"] else None,
        }
    if "balloon" in ci:
        b = ci["balloon"]
        out["balloon"] = [b["lines"], b["cpl"], b["fg"], b["bg"], b["border"], b["font"], b["font_height"],
                          b["font_weight"], b["italic"], b["underline"]]
    anims = []
    for name in names:
        a = st["anim_objs"][name.lower()]
        anims.append({
            "name": name, "record_name": a["name"], "transition": a["transition"], "return": a["return"],
            "frames": [{
                "images": [list(i) for i in f["images"]], "sound": f["audio"], "duration": f["dur"],
                "exit": f["exit"], "branches": [list(b) for b in f["branches"]],
                "overlays": [list(o) for o in f["overlays"]],
            } for f in a["frames"]],
        })
    out["animations"] = anims
    images = []
    for i, (off, size, chk) in enumerate(ients):
        w, h, pix = st["decoded"][i]
        comp = d[off + 5]
        dsz, = struct.unpack_from("<I", d, off + 6)
        p = off + 10 + (dsz if comp else ((w + 3) & ~3) * h)
        csz, usz = struct.unpack_from("<II", d, p)
        if csz:
            rgn, ok, _ = acs.decompress(d[p + 8:p + 8 + csz], usz)
            assert ok
        else:
            rgn = d[p + 8:p + 8 + usz]
        images.append({"checksum": chk, "width": w, "height": h, "pixels": sha(pix),
                       "region": sha(rgn) if usz else None})
    out["images"] = images
    out["sounds"] = [[chk, sha(d[o:o + s])] for o, s, chk in aents]
    return out


def work(args):
    research, outdir, path = args
    acs = load_module(research)
    res = dump(acs, path)
    with open(os.path.join(outdir, os.path.basename(path) + ".json"), "w") as f:
        json.dump(res, f, ensure_ascii=False)
    return path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--research", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("files", nargs="+")
    a = ap.parse_args()
    jobs = [(a.research, a.out, p) for p in a.files]
    with multiprocessing.get_context("fork").Pool(min(len(jobs), os.cpu_count() or 1)) as pool:
        for p in pool.imap_unordered(work, jobs):
            print("dumped", p)


if __name__ == "__main__":
    main()
