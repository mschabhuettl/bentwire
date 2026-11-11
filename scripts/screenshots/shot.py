"""Helpers for scripts/screenshots.sh: Snapshot geometry, deterministic crops, the
side-by-side strip and the GIF frame list. Every PNG is written by PIL from grim's PPM
output or from lossless recorder frames.

usage: shot.py has SNAPSHOT.json KIND...           exit 0 if every surface kind is mapped
       shot.py rect SNAPSHOT.json MARGIN KIND...   union of those surfaces plus MARGIN,
                                                   clamped to the first output: x y w h
       shot.py inside SNAPSHOT.json x y w h KIND...  exit 0 if those surfaces lie in the rect
       shot.py crop IN.ppm OUT.png SCALE x y w h   crop a logical rect (times SCALE)
       shot.py pixel IN.ppm RRGGBB                 exit 0 if pixel (0, 0) has the colour
       shot.py opaque IN.ppm REF.ppm x y w h       the pixel nearest the rect centre that
                                                   differs from REF (the bare desktop): x y
       shot.py button SNAPSHOT.json CAPTION        centre of that Office balloon button: x y
       shot.py strip OUT.png x y w h IN.ppm...     the rect, column band i taken from IN i
       shot.py frames DIR PTS OUT.txt FIRST LAST GRID
                                                   ffconcat list of the distinct frames in
                                                   DIR (00001.png..., times in PTS, seconds),
                                                   durations rounded to GRID ms, the first
                                                   and last frame held FIRST and LAST ms
"""
import json
import os
import sys

from PIL import Image, ImageChops


def surfaces(path, kinds):
    s = json.load(open(path))
    return s, [q for q in s["surfaces"] if q["kind"] in kinds and q["mapped"]]


def main():
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "has":
        _, got = surfaces(args[0], args[1:])
        return {q["kind"] for q in got} == set(args[1:])
    if cmd == "rect":
        s, got = surfaces(args[0], args[2:])
        m = int(args[1])
        o = s["outputs"][0]
        x0 = max(min(q["x"] for q in got) - m, o["x"])
        y0 = max(min(q["y"] for q in got) - m, o["y"])
        x1 = min(max(q["x"] + q["w"] for q in got) + m, o["x"] + o["w"])
        y1 = min(max(q["y"] + q["h"] for q in got) + m, o["y"] + o["h"])
        print(x0 - o["x"], y0 - o["y"], x1 - x0, y1 - y0)
        return True
    if cmd == "inside":
        _, got = surfaces(args[0], args[5:])
        x, y, w, h = (int(v) for v in args[1:5])
        return all(q["x"] >= x and q["y"] >= y and q["x"] + q["w"] <= x + w
                   and q["y"] + q["h"] <= y + h for q in got)
    if cmd == "crop":
        src, out, scale = args[0], args[1], int(args[2])
        x, y, w, h = (int(v) * scale for v in args[3:7])
        Image.open(src).convert("RGB").crop((x, y, x + w, y + h)).save(out, optimize=False)
        return True
    if cmd == "pixel":
        return Image.open(args[0]).convert("RGB").getpixel((0, 0)) == tuple(bytes.fromhex(args[1]))
    if cmd == "opaque":
        im = Image.open(args[0]).convert("RGB")
        ref = Image.open(args[1]).convert("RGB")
        x, y, w, h = (int(v) for v in args[2:6])
        cx, cy = x + w // 2, y + h // 2
        pts = sorted(((px - cx) ** 2 + (py - cy) ** 2, py, px)
                     for py in range(y, y + h) for px in range(x, x + w))
        for _, py, px in pts:
            if im.getpixel((px, py)) != ref.getpixel((px, py)):
                print(px, py)
                return True
        return False
    if cmd == "button":
        s = json.load(open(args[0]))
        for k in s["office_balloon"]["buttons"]:
            if k["caption"] == args[1]:
                r = k["rect"]
                print(r["x"] + r["w"] // 2, r["y"] + r["h"] // 2)
                return True
        return False
    if cmd == "strip":
        out = args[0]
        x, y, w, h = (int(v) for v in args[1:5])
        ims = [Image.open(p).convert("RGB") for p in args[5:]]
        canvas = ims[0].crop((x, y, x + w, y + h))
        band = w // len(ims)
        for i, im in enumerate(ims):
            x0 = x + i * band
            x1 = x + w if i == len(ims) - 1 else x0 + band
            canvas.paste(im.crop((x0, y, x1, y + h)), (x0 - x, 0))
        canvas.save(out, optimize=False)
        return True
    if cmd == "frames":
        d, pts, out = args[0], args[1], args[2]
        first, last, grid = (int(v) for v in args[3:6])
        times = [float(t.strip(",")) for t in open(pts).read().split()]
        names = sorted(f for f in os.listdir(d) if f.endswith(".png"))
        assert len(names) == len(times), (len(names), len(times))
        keep = []  # (name, start ms) of each frame that differs from the one before
        prev = None
        for n, t in zip(names, times):
            im = Image.open(os.path.join(d, n)).convert("RGB")
            if prev is None or ImageChops.difference(im, prev).getbbox() is not None:
                keep.append((n, t * 1000))
            prev = im
        lines = ["ffconcat version 1.0"]
        for i, (n, t) in enumerate(keep):
            if i == len(keep) - 1:
                ms = last
            elif i == 0:
                ms = first
            else:
                ms = max(grid, round((keep[i + 1][1] - t) / grid) * grid)
            lines += [f"file {n}", f"duration {ms / 1000:.3f}"]
        # the concat demuxer drops the last duration unless the file is repeated
        lines += [f"file {keep[-1][0]}"]
        open(out, "w").write("\n".join(lines) + "\n")
        print(len(keep))
        return True
    return False


sys.exit(0 if main() else 1)
