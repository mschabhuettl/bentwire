#!/usr/bin/env python3
"""Rewrite the gallery table of a pack README between <!-- gallery:start --> and
<!-- gallery:end --> from the characters of the pack (Python 3.11+ stdlib only).

usage: gallery PACKDIR OUTDIR | gallery-table.py PACKDIR README

Reads the lines of bentwired's gallery example (id, name, product, label; tab-separated),
so names and labels are the daemon's (A-LABEL-1). One cell per character: rest-pose image
gallery/<id>.png, name, id, family and product; the alt text is the label. Rows are ordered
by product as listed in PACKDIR/pack.toml `products` (products not listed come last), then
id, five cells per row.
"""
import sys
import tomllib
import zipfile
from pathlib import Path

START, END = "<!-- gallery:start -->", "<!-- gallery:end -->"
CELL = (
    '<td align="center" valign="bottom"><img src="gallery/{id}.png" height="100" '
    'alt="{label}"><br><b>{name}</b><br><code>{id}</code><br><sub>{family}, {product}</sub></td>'
)


def family(pack, cid):
    path = pack / f"{cid}.bwc"
    if path.is_file():
        with zipfile.ZipFile(path) as z:
            text = z.read("character.toml").decode()
    else:
        text = (pack / cid / "character.toml").read_text()
    return tomllib.loads(text)["family"]


def main():
    pack, readme = Path(sys.argv[1]), Path(sys.argv[2])
    order = tomllib.loads((pack / "pack.toml").read_text()).get("products", [])
    chars = []
    for line in sys.stdin.read().splitlines():
        cid, name, product, label = line.split("\t")
        chars.append(
            {"id": cid, "name": name, "product": product, "label": label, "family": family(pack, cid)}
        )
    rank = {p: i for i, p in enumerate(order)}
    chars.sort(key=lambda c: (rank.get(c["product"], len(order)), c["product"], c["id"]))
    assert len({c["label"] for c in chars}) == len(chars), "labels are not unique"
    lines = ["<table>"]
    for i in range(0, len(chars), 5):
        lines += ["<tr>", *(CELL.format(**c) for c in chars[i : i + 5]), "</tr>"]
    lines.append("</table>")
    text = readme.read_text()
    head, rest = text.split(START + "\n", 1)
    _, tail = rest.split(END, 1)
    readme.write_text(head + START + "\n" + "\n".join(lines) + "\n" + END + tail)
    print(f"{readme}: gallery table, {len(chars)} characters")


main()
