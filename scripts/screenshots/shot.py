"""Helpers for scripts/screenshots.sh: Snapshot geometry, deterministic crops, the
side-by-side strip and the GIF frame list. Every PNG is written by PIL from grim's PPM
output or from lossless recorder frames.

usage: shot.py has SNAPSHOT.json KIND...           exit 0 if every surface kind is mapped
       shot.py rect SNAPSHOT.json MARGIN KIND...   union of those surfaces plus MARGIN,
                                                   clamped to the first output: x y w h
       shot.py only SNAPSHOT.json KIND...          exit 0 if exactly these kinds are mapped
       shot.py inside SNAPSHOT.json x y w h KIND...  exit 0 if those surfaces lie in the rect
       shot.py crop IN.ppm OUT.png SCALE x y w h   crop a logical rect (times SCALE)
       shot.py pixel IN.ppm RRGGBB                 exit 0 if pixel (0, 0) has the colour
       shot.py opaque IN.ppm REF.ppm SCALE x y w h the pixel nearest the logical rect's
                                                   centre that differs from REF (the bare
                                                   desktop), in logical coordinates: x y
       shot.py button SNAPSHOT.json CAPTION        centre of that Office balloon button: x y
       shot.py strip OUT.png x y w h IN.ppm...     the rect, column band i taken from IN i
       shot.py frames DIR PTS OUTDIR FIRST LAST GRID
                                                   the distinct frames of DIR (00001.png...,
                                                   times in PTS, seconds) into OUTDIR, with
                                                   OUTDIR/durations: ms rounded to GRID, the
                                                   first and last frame held FIRST and LAST;
                                                   states under 35 ms are dropped, other
                                                   frames capped at 1000 ms
       shot.py gif DIR OUT.gif DURATIONS           the frames (00001.png...) into a looping
                                                   GIF, see gif()
"""
import json
import os
import sys

from PIL import Image, ImageChops


HOLD = 1000  # ms, longest frame inside a GIF


BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]


def gif(d, out, ms):
    """One palette for all frames: every colour that changes during the GIF (character,
    balloon, menu, pointer) exactly, up to 128 of them by frequency, the other 128
    from the first frame (the desktop). A pixel whose colour is in the palette maps to
    it as is, so pixel art stays exact; other pixels (wallpaper gradients) get an ordered
    dither, which keeps unchanged pixels unchanged from frame to frame."""
    import numpy as np

    names = sorted(f for f in os.listdir(d) if f.endswith(".png"))
    assert len(names) == len(ms)
    load = lambda n: np.asarray(Image.open(os.path.join(d, n)).convert("RGB"), dtype=np.int32)
    pack = lambda a: (a[..., 0] << 16) | (a[..., 1] << 8) | a[..., 2]
    first = load(names[0])
    counts = {}
    for n in names[1:]:
        f = load(n)
        changed = pack(f)[(f != first).any(axis=2)]
        for c, k in zip(*np.unique(changed, return_counts=True)):
            counts[int(c)] = counts.get(int(c), 0) + int(k)
    exact = sorted(counts, key=lambda c: (-counts[c], c))[:128]
    rest = Image.fromarray(first.astype(np.uint8)).quantize(
        colors=256 - len(exact), method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
    pal = rest.getpalette()[: 3 * (256 - len(exact))]
    colors = [((c >> 16) & 255, (c >> 8) & 255, c & 255) for c in exact]
    colors += [tuple(pal[i:i + 3]) for i in range(0, len(pal), 3)]
    while len(colors) < 256:
        colors.append((0, 0, 0))
    lut = {}
    for i, c in enumerate(colors):
        lut.setdefault((c[0] << 16) | (c[1] << 8) | c[2], i)
    keys = np.array(sorted(lut), dtype=np.int64)
    vals = np.array([lut[int(k)] for k in keys], dtype=np.int32)
    P = np.array(colors, dtype=np.int32)
    h, w, _ = first.shape
    bay = (np.array(BAYER, dtype=np.int32) - 8) * 2  # about +-16 levels
    off = np.tile(bay, (h // 4 + 1, w // 4 + 1))[:h, :w]

    def index(f, mask):
        px = f[mask]
        packed = pack(px)
        pos = np.minimum(np.searchsorted(keys, packed), len(keys) - 1)
        idx = np.where(keys[pos] == packed, vals[pos], -1).astype(np.int32)
        miss = idx < 0
        if miss.any():
            v = px[miss] + off[mask][miss][:, None]
            best = np.empty(len(v), dtype=np.int32)
            for s in range(0, len(v), 20000):
                dd = ((v[s:s + 20000, None, :] - P[None, :, :]) ** 2).sum(axis=2)
                best[s:s + 20000] = dd.argmin(axis=1)
            idx[miss] = best
        return idx

    flat = [c for col in colors for c in col]
    ims = []
    prev = None
    cur = np.zeros((h, w), dtype=np.uint8)
    for n in names:
        f = load(n)
        mask = np.ones((h, w), bool) if prev is None else (f != prev).any(axis=2)
        if mask.any():
            cur = cur.copy()
            cur[mask] = index(f, mask)
        prev = f
        im = Image.fromarray(cur, "P")
        im.putpalette(flat)
        ims.append(im)
    ims[0].save(out, save_all=True, append_images=ims[1:], duration=ms, loop=0, optimize=False)
    return True


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
    if cmd == "only":
        s = json.load(open(args[0]))
        return {q["kind"] for q in s["surfaces"] if q["mapped"]} == set(args[1:])
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
        k = int(args[2])
        x, y, w, h = (int(v) for v in args[3:7])
        cx, cy = x + w // 2, y + h // 2
        pts = sorted(((px - cx) ** 2 + (py - cy) ** 2, py, px)
                     for py in range(y, y + h) for px in range(x, x + w))
        for _, py, px in pts:
            q = (px * k + k // 2, py * k + k // 2)
            if im.getpixel(q) != ref.getpixel(q):
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
        keep = []  # (image, start ms) of each frame that differs from the one before
        prev = None
        for n, t in zip(names, times):
            im = Image.open(os.path.join(d, n)).convert("RGB")
            if prev is None or ImageChops.difference(im, prev).getbbox() is not None:
                # A state shown for at most two recorder frames is a transient (the
                # pointer already moved, the hover not yet drawn): whether it is caught
                # depends on latency, so it is dropped.
                if len(keep) > 1 and t * 1000 - keep[-1][1] < 35:
                    keep.pop()
                keep.append((im, t * 1000))
            prev = im
        os.makedirs(out)
        ms = []
        for i, (im, t) in enumerate(keep):
            im.save(os.path.join(out, f"{i + 1:05d}.png"), optimize=False)
            if i == len(keep) - 1:
                ms.append(last)
            elif i == 0:
                ms.append(first)
            else:
                # Holds longer than HOLD come from the script waiting in real time
                # (daemon start, settling): capped, so they do not vary between runs.
                ms.append(min(HOLD, max(grid, round((keep[i + 1][1] - t) / grid) * grid)))
        open(os.path.join(out, "durations"), "w").write(" ".join(map(str, ms)) + "\n")
        print(len(keep))
        return True
    if cmd == "gif":
        return gif(args[0], args[1], [int(v) for v in open(args[2]).read().split()])
    return False


sys.exit(0 if main() else 1)
