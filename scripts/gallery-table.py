#!/usr/bin/env python3
"""Rewrite the gallery table of a pack README between <!-- gallery:start --> and
<!-- gallery:end --> from the characters of the pack (Python 3.11+ stdlib only).

usage: gallery-table.py PACKDIR README

One cell per character: rest-pose image gallery/<id>.png, English name, id, family and
source product. The product comes from the manifest's [source] format (PRODUCT). The pack
may carry PACKDIR/gallery.tsv (columns id, product, name; '#' lines are comments) with
what the format cannot tell: per-id product and name overrides (empty: none). Products
are ordered as they first appear in gallery.tsv (rows with id '*' only set that order),
else as in PRODUCT. Rows are ordered by product, then gallery.tsv row, then id, five
cells per row. Alt texts are "<name> (<product>)"; when two characters would still share
one, the id is added.
"""
import sys
import tomllib
import zipfile
from pathlib import Path

PRODUCT = {
    "acs-2.0": "Microsoft Agent 2.0",
    "acs-1.5": "Microsoft Agent 1.5",
    "act-2.2": "Office 97",
    "act-1.0": "Microsoft Bob",
}
ORDER = list(PRODUCT.values())  # product order without a gallery.tsv
START, END = "<!-- gallery:start -->", "<!-- gallery:end -->"
CELL = (
    '<td align="center" valign="bottom"><img src="gallery/{id}.png" height="100" '
    'alt="{alt}"><br><b>{name}</b><br><code>{id}</code><br><sub>{family}, {product}</sub></td>'
)


def manifest(path):
    if path.suffix == ".bwc":
        with zipfile.ZipFile(path) as z:
            return tomllib.loads(z.read("character.toml").decode())
    return tomllib.loads((path / "character.toml").read_text())


def overrides(pack):
    path = pack / "gallery.tsv"
    if not path.is_file():
        return ORDER, {}
    order, rows = [], {}
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        cid, product, name = line.split("\t")
        if product and product not in order:
            order.append(product)
        if cid != "*":
            rows[cid] = (len(rows), product, name)
    return order, rows


def entry(cid, m, rows):
    infos = m["character"]["info"]
    en = [i for i in infos if i["lang"] == "en" or i["lang"].startswith("en-")]
    rank, product, name = rows.get(cid, (-1, "", ""))
    name = name or (en or infos)[0]["name"]
    product = product or PRODUCT[m["source"]["format"]]
    return {"id": cid, "name": name, "family": m["family"], "product": product, "rank": rank}


def main():
    pack, readme = Path(sys.argv[1]), Path(sys.argv[2])
    order, rows = overrides(pack)
    chars = []
    for p in pack.iterdir():
        if p.suffix == ".bwc":
            chars.append(entry(p.stem, manifest(p), rows))
        elif (p / "character.toml").is_file():
            chars.append(entry(p.name, manifest(p), rows))
    chars.sort(key=lambda c: (order.index(c["product"]), c["rank"], c["id"]))
    for c in chars:
        # "Microsoft Agent 1.5 (third-party)" -> "Microsoft Agent 1.5, third-party"
        name = "Unnamed character" if c["name"] == "(unnamed)" else c["name"]
        c["alt"] = f'{name} ({c["product"].replace(" (", ", ").rstrip(")")})'
    for c in chars:
        if sum(d["alt"] == c["alt"] for d in chars) > 1:
            c["alt2"] = f'{c["alt"][:-1]}, {c["id"]})'
    for c in chars:
        c["alt"] = c.pop("alt2", c["alt"])
    assert len({c["alt"] for c in chars}) == len(chars)
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
