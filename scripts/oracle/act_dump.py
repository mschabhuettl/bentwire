"""Structural dump of ACT/ACP files by the research parser (actparse.py).

Usage: PYTHONDONTWRITEBYTECODE=1 python3 -I act_dump.py <BENTWIRE_REFERENCE> <out_dir> [file...]
Without files, enumerates the sample set: every file in source-files
(sources.py SAMPLES "act" + "actg") plus the .ACT/.ACP files in source-files.
Writes <out_dir>/<basename>.json and <out_dir>/index.json (list of [basename, path]).
The Rust test crates/agent-formats/tests/act_real.rs builds the same JSON and compares.
"""
import collections
import hashlib
import json
import os
import struct
import sys

research, out_dir = sys.argv[1], sys.argv[2]
sys.argv = [sys.argv[0], os.path.join(research, "act/scripts")] + sys.argv[3:]
sys.path.insert(0, os.path.join(research, "act/scripts"))
import actparse  # noqa: E402


def sha(b):
    return hashlib.sha256(b).hexdigest()


def dump(path):
    d = open(path, "rb").read()
    major, minor, names, offs = actparse.parse_container(d)
    R = d[offs[0]:offs[1]]
    st = collections.defaultdict(collections.Counter)
    r = actparse.parse_resource(R, major, minor, st)
    S = r["S"]
    pays = []
    for i, (off, sub, size) in enumerate(r["payloads"]):
        if i in r["composites"]:
            pays.append({"kind": "composite", "children": [list(c) for c in r["composites"][i]]})
            continue
        kind, data = r["images"][i]
        if kind == "wmf":
            b = R[S["pay_data"] + off:S["pay_data"] + off + size]
            q = 40
            fns = []
            while True:
                sz, fn = struct.unpack_from("<IH", b, q)
                fns.append(fn)
                if fn == 0:
                    break
                q += 2 * sz
            pays.append({"kind": "wmf", "sha256": sha(b), "records": len(fns),
                         "functions_sha256": sha(struct.pack("<%dH" % len(fns), *fns))})
        else:
            w, h, stride, px = data
            top = b"".join(px[(h - 1 - y) * stride:(h - 1 - y) * stride + w] for y in range(h))
            pays.append({"kind": "bitmap", "width": w, "height": h, "pixels_sha256": sha(top)})
    n = (S["anim_data"] - S["snd_index"]) // 4 - 1
    sent = [struct.unpack_from("<I", R, S["snd_index"] + 4 * i)[0] for i in range(n + 1)]
    sounds = []
    for i in range(max(n, 0)):
        w = R[S["snd_data"] + sent[i]:S["snd_data"] + sent[i + 1]]
        f = r["sounds"][i]
        sounds.append({"sha256": sha(w), "format_tag": f[0], "channels": f[1], "rate": f[2], "bits": f[5]})
    anims = []
    for gid in sorted(r["anims"]):
        vs = []
        for blob in r["anims"][gid]:
            hdr, cnt = struct.unpack_from("<HH", blob, 0)
            vs.append({"header": hdr, "commands": [list(struct.unpack_from("<HHH", blob, 4 + 6 * k)) for k in range(cnt)]})
        anims.append([gid, vs])
    # strings: raw bytes are not kept by actparse; re-encode its decoded texts
    strs = []
    for gid in sorted(r["texts"]):
        enc = "utf-16le" if major >= 2 else "cp1252"
        strs.append([gid, [t.encode(enc).hex() for t in r["texts"][gid]]])
    h = r["h"]
    return {
        "major": major, "minor": minor, "item_name": names[0].hex(),
        "header": [h[k] for k in actparse.HDR[1:]],
        "color_table": [list(c[:3]) for c in r["pal"]],
        "transitions": [list(t) for t in r["trans"]], "transition_rest_pose": r["rest2"],
        "payloads": pays, "sounds": sounds, "animations": anims, "strings": strs,
    }


def samples():
    base = os.path.join(research, "act/downloads")
    ma = os.path.join(base, "source-files")
    iso = os.path.join(base, "source-files")
    out = [os.path.join(ma, f) for f in sorted(os.listdir(ma))]
    out += [os.path.join(iso, f) for f in sorted(os.listdir(iso)) if f.upper().endswith((".ACT", ".ACP"))]
    return out


files = sys.argv[2:] or samples()
os.makedirs(out_dir, exist_ok=True)
index = []
for p in files:
    key = os.path.basename(os.path.dirname(p)) + "__" + os.path.basename(p)
    with open(os.path.join(out_dir, key + ".json"), "w") as f:
        json.dump(dump(p), f)
    index.append([key, p])
    print("dumped", key, flush=True)
with open(os.path.join(out_dir, "index.json"), "w") as f:
    json.dump(index, f)
