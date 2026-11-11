"""Check a binary PPM (grim -t ppm) for bentwire's test character and balloons.

usage: bbox.py FILE.ppm --expect x0,y0,x1,y1|none
       bbox.py FILE.ppm --balloon agent|office
       bbox.py FILE.ppm --menu x
The test character (crates/bentwired/tests/fixtures) draws opaque squares in pure red,
green, blue or yellow; balloons are filled with #FFFFE1 (Agent default) or #FFFFCC
(Office).
"""
import sys

CHAR = {b"\xff\x00\x00", b"\x00\xc8\x00", b"\x00\x00\xff", b"\xff\xff\x00"}
BALLOON = {"agent": b"\xff\xff\xe1", "office": b"\xff\xff\xcc"}


def read_ppm(path):
    data = open(path, "rb").read()
    fields, pos = [], 0
    while len(fields) < 4:
        while data[pos:pos + 1].isspace():
            pos += 1
        if data[pos:pos + 1] == b"#":
            pos = data.index(b"\n", pos)
            continue
        end = pos
        while not data[end:end + 1].isspace():
            end += 1
        fields.append(data[pos:end])
        pos = end
    assert fields[0] == b"P6", "not a binary PPM"
    w, h = int(fields[1]), int(fields[2])
    return w, h, data[pos + 1:]


def main():
    path, mode, arg = sys.argv[1], sys.argv[2], sys.argv[3]
    w, h, px = read_ppm(path)
    if mode == "--expect":
        xs, ys = [], []
        for y in range(h):
            row = px[y * w * 3:(y + 1) * w * 3]
            for x in range(w):
                if row[x * 3:x * 3 + 3] in CHAR:
                    xs.append(x)
                    ys.append(y)
        got = (min(xs), min(ys), max(xs) + 1, max(ys) + 1) if xs else None
        want = None if arg == "none" else tuple(int(v) for v in arg.split(","))
        print(f"    character bbox {got}, expected {want}")
        return got == want
    if mode == "--menu":
        # The XP menu: white background inside a #ACA899 border (render menu.rs).
        n = px.count(b"\xac\xa8\x99")
        print(f"    {n} menu border pixels")
        return n > 100
    if mode == "--balloon":
        n = px.count(BALLOON[arg])
        print(f"    {n} balloon pixels")
        return n > 200
    return False


sys.exit(0 if main() else 1)
