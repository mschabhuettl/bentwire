#!/usr/bin/env python3
"""Rewrite the gallery table of a pack README between <!-- gallery:start --> and
<!-- gallery:end --> from the characters of the pack (Python 3.11+ stdlib only).

usage: gallery-table.py PACKDIR README

One cell per character: rest-pose image gallery/<id>.png, English name, id, family and
source product. The product comes from the manifest's [source] format (PRODUCT), with the
per-id exceptions that the format cannot tell (PRODUCT_BY_ID). Rows are ordered by product
(order of GROUPS), then id, five cells per row. Alt texts are "<name> (<product>)"; when
two characters would still share one, the id is added.
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
THIRD_PARTY_15 = ["al", "check", "gourdy", "max", "miku", "ozzar", "sharky", "spaceman", "totem"]
PRODUCT_BY_ID = {
    **{f"{i}-15": "Microsoft Agent 1.5 (third-party)" for i in THIRD_PARTY_15},
    "dolphin-97": "Office 97 (Japanese)",
    "offlady-97": "Office 97 (Japanese)",
    "rocky-greetings-act": "Greetings Workshop",
    "rocky-earlygreetings-act": "Greetings Workshop",
    "tutor-act": "My Personal Tutor",
}
GROUPS = [
    "Microsoft Agent 2.0",
    "Microsoft Agent 1.5",
    "Microsoft Agent 1.5 (third-party)",
    "Office 97",
    "Office 97 (Japanese)",
    "Microsoft Bob",
    "Greetings Workshop",
    "My Personal Tutor",
]
# English names the manifest does not carry (offlady-97 has only its Japanese name).
NAME_BY_ID = {"offlady-97": "Saeko-sensei", "tutor-act": "(unnamed)"}
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


def entry(cid, m):
    infos = m["character"]["info"]
    en = [i for i in infos if i["lang"] == "en" or i["lang"].startswith("en-")]
    name = NAME_BY_ID.get(cid) or (en or infos)[0]["name"]
    product = PRODUCT_BY_ID.get(cid) or PRODUCT[m["source"]["format"]]
    return {"id": cid, "name": name, "family": m["family"], "product": product}


def main():
    pack, readme = Path(sys.argv[1]), Path(sys.argv[2])
    chars = []
    for p in pack.iterdir():
        if p.suffix == ".bwc":
            chars.append(entry(p.stem, manifest(p)))
        elif (p / "character.toml").is_file():
            chars.append(entry(p.name, manifest(p)))
    chars.sort(key=lambda c: (GROUPS.index(c["product"]), c["id"]))
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
