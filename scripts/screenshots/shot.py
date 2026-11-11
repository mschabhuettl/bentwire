"""Helpers for scripts/screenshots.sh: Snapshot geometry, deterministic crops and the
side-by-side composition. Every PNG is written by PIL from grim's PPM output.

usage: shot.py has SNAPSHOT.json KIND...           exit 0 if every surface kind is mapped
       shot.py rect SNAPSHOT.json MARGIN KIND...   union of those surfaces plus MARGIN,
                                                   clamped to the first output: x y w h
       shot.py crop IN.ppm OUT.png SCALE x y w h   crop a logical rect (times SCALE)
       shot.py pixel IN.ppm RRGGBB                 exit 0 if pixel (0, 0) has the colour
       shot.py opaque IN.ppm RRGGBB x y w h       the drawn pixel nearest the rect centre: x y
       shot.py compose OUT.png RRGGBB GAP IN.png...  side by side, bottom-aligned
"""
import json
import sys

from PIL import Image


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
    if cmd == "crop":
        src, out, scale = args[0], args[1], int(args[2])
        x, y, w, h = (int(v) * scale for v in args[3:7])
        Image.open(src).convert("RGB").crop((x, y, x + w, y + h)).save(out, optimize=False)
        return True
    if cmd == "pixel":
        return Image.open(args[0]).convert("RGB").getpixel((0, 0)) == tuple(bytes.fromhex(args[1]))
    if cmd == "opaque":
        im = Image.open(args[0]).convert("RGB")
        bg = tuple(bytes.fromhex(args[1]))
        x, y, w, h = (int(v) for v in args[2:6])
        cx, cy = x + w // 2, y + h // 2
        pts = sorted(((px - cx) ** 2 + (py - cy) ** 2, py, px)
                     for py in range(y, y + h) for px in range(x, x + w))
        for _, py, px in pts:
            if im.getpixel((px, py)) != bg:
                print(px, py)
                return True
        return False
    if cmd == "compose":
        out, bg, gap = args[0], tuple(bytes.fromhex(args[1])), int(args[2])
        ims = [Image.open(p).convert("RGB") for p in args[3:]]
        w = sum(i.width for i in ims) + gap * (len(ims) - 1)
        h = max(i.height for i in ims)
        canvas = Image.new("RGB", (w, h), bg)
        x = 0
        for i in ims:
            canvas.paste(i, (x, h - i.height))
            x += i.width + gap
        canvas.save(out, optimize=False)
        return True
    return False


sys.exit(0 if main() else 1)
