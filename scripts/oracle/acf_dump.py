"""Dump ACF + ACA/AAF characters as JSON with the research parser acfparse.py (the oracle).

Usage: PYTHONDONTWRITEBYTECODE=1 python3 -I acf_dump.py --research DIR --out OUTDIR CHARDIR...

Each CHARDIR holds one *.acf file and the animation files it references (matched
case-insensitively). Copies $BENTWIRE_REFERENCE/{acfparse,agentlz}.py into a fresh temp
directory and imports them from there. Writes OUTDIR/<basename of CHARDIR>.json.

Fields a version does not store are written as null (the research parser substitutes the
runtime defaults; the Rust reader keeps them absent). The layout is mirrored by
crates/agent-formats/tests/oracle/mod.rs (acf_json).
"""
import argparse
import hashlib
import json
import os
import shutil
import sys
import tempfile


def sha(b):
    return hashlib.sha256(bytes(b)).hexdigest()


def load_modules(research, subdir="acf/scripts", names=("agentlz", "acfparse")):
    d = tempfile.mkdtemp(prefix="bentwire-acf-oracle-")
    for n in names:
        shutil.copy(os.path.join(research, subdir, n + ".py"), d)
    sys.path.insert(0, d)
    mods = [__import__(n) for n in names]
    sys.path.remove(d)
    shutil.rmtree(d)
    return mods


def character_json(c):
    v = c["version"]
    out = {
        "version": v,
        "entries": [[n, f, r, ck if v > 0x1001D else None] for n, f, r, ck in c["anims"]],
        "guid": c["guid"],
        "localized": [list(l) for l in c["loc"]],
        "width": c["width"], "height": c["height"], "transparent": c["transparent"], "flags": c["flags"],
        "animation_set": list(c["animset"]) if v >= 0x20001 else None,
        "voice": None, "balloon": None,
        "palette": [len(c["palette"]), sha(b"".join(c["palette"]))],
        "tray_icon": [sha(c["icon_mask"]), sha(c["icon_color"])] if c.get("has_icon") else None,
        "states": [[s, a] for s, a in c["states"]],
    }
    if "tts" in c:
        t = c["tts"]
        out["voice"] = {
            "engine": t["engine"], "mode": t["mode"], "speed": t["speed"], "pitch": t["pitch"],
            "extra": [t["lang"], t["dialect"], t["gender"], t["age"], t["style"]] if t.get("has_extra") else None,
        }
    if "balloon" in c:
        b = c["balloon"]
        out["balloon"] = [b["lines"], b["chars"], b["fg"], b["bg"], b["border"], b["font"], b["height"],
                          b["weight"], b["italic"], b.get("unknown")]
    return out


def picture(data, rgn):
    return [sha(data), sha(rgn) if rgn else None]


def animation_json(hdr, a, v):
    ver, ck, flag = hdr
    return {
        "header": [ver, ck] if ver is not None else None,
        "compressed": flag == 1,
        "sounds": [sha(s) for s in a["sounds"]],
        "images": [None if not im["size"] else [im["flag"]] + picture(im["data"], im["rgn"]) for im in a["images"]],
        "transition": a["transition"] if v >= 0x10020 else None,
        "frames": [{
            "image": f["image"], "sound": f["sound"], "duration": f["duration"], "offset": [f["f6"], f["f8"]],
            "exit": f["exit"] if v >= 0x10020 else None,
            "branches": [list(b) for b in f["branches"]],
            "base": None if f["base"] is None else (
                "absent" if not f["base"]["present"] else
                "empty" if not f["base"]["size"] else picture(f["base"]["data"], f["base"]["rgn"])),
            "overlays": [[o["type"]] + ([] if not o["size"] else [
                o["reserved"], o["use_base"] if v > 0x1001F else None, o["x"], o["y"], o["w"], o["h"],
                sha(o["data"]), sha(o["rgn"]) if o.get("rgn") else None]) for o in f["overlays"]],
        } for f in a["frames"]],
    }


def dump_dir(ap, chardir):
    files = {f.lower(): os.path.join(chardir, f) for f in os.listdir(chardir)}
    acf = [p for f, p in files.items() if f.endswith(".acf")]
    assert len(acf) == 1, chardir
    magic, body = ap.unwrap_acf(open(acf[0], "rb").read())
    c = ap.parse_acf_body(body)
    out = {"magic": magic, "character": character_json(c), "animations": []}
    for name, fname, ret, ck in c["anims"]:
        p = files.get(fname.lower())
        if p is None:
            out["animations"].append(None)
            continue
        ver, ck2, flag, ab = ap.unwrap_aca(open(p, "rb").read(), c["version"], ck)
        out["animations"].append(animation_json((ver, ck2, flag), ap.parse_aca_body(ab, c["version"]), c["version"]))
    return out


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--research", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("dirs", nargs="+")
    a = p.parse_args()
    _, ap = load_modules(a.research)
    for d in a.dirs:
        res = dump_dir(ap, d)
        with open(os.path.join(a.out, os.path.basename(d.rstrip("/")) + ".json"), "w") as f:
            json.dump(res, f, ensure_ascii=False)
        print("dumped", d)


if __name__ == "__main__":
    main()
