#!/usr/bin/env python3
"""Extracts SmartArt "starter kits" (the five parts of one diagram per layout) from decks.

Usage: extract_smartart_starters.py OUT_DIR DECK:DATA_PART:NAME ...
  e.g. samples/vr/test7.pptx:ppt/diagrams/data5.xml:vList2

Writes OUT_DIR/NAME/{data,layout,quickStyle,colors,drawing}.xml and frame.txt (the
graphic frame's size in EMU, which is the drawing's coordinate space). All
text runs are replaced with a neutral word so no document content is kept.
"""
import posixpath
import re
import sys
import zipfile

REL = {"dm": "diagramData", "lo": "diagramLayout", "qs": "diagramQuickStyle", "cs": "diagramColors"}


def neutral(xml: str) -> str:
    return re.sub(r"(<a:t>)[^<]*(</a:t>)", r"\1Text\2", xml)


def extract(deck: str, data_part: str, name: str, out: str) -> None:
    z = zipfile.ZipFile(deck)
    for rels_name in z.namelist():
        if not re.match(r"ppt/slides/_rels/slide\d+\.xml\.rels$", rels_name):
            continue
        slide = "ppt/slides/" + posixpath.basename(rels_name)[:-5]
        rels = {}
        for m in re.finditer(r"<Relationship [^>]*/>", z.read(rels_name).decode()):
            tag = m.group(0)
            rels[re.search(r'Id="([^"]*)"', tag).group(1)] = (
                re.search(r'Type="([^"]*)"', tag).group(1),
                posixpath.normpath(posixpath.join("ppt/slides", re.search(r'Target="([^"]*)"', tag).group(1))),
            )
        for frame in re.finditer(r"<p:graphicFrame>.*?</p:graphicFrame>", z.read(slide).decode(), re.S):
            f = frame.group(0)
            ids = dict(re.findall(r'r:(dm|lo|qs|cs)="(\w+)"', f))
            if len(ids) != 4 or rels[ids["dm"]][1] != data_part:
                continue
            ext = re.search(r'<p:xfrm>.*?<a:ext cx="(\d+)" cy="(\d+)"', f, re.S).groups()
            data = z.read(data_part).decode()
            drawing_rel = re.search(r'dataModelExt[^>]*relId="(\w+)"', data).group(1)
            parts = {
                "data": data,
                "layout": z.read(rels[ids["lo"]][1]).decode(),
                "quickStyle": z.read(rels[ids["qs"]][1]).decode(),
                "colors": z.read(rels[ids["cs"]][1]).decode(),
                "drawing": z.read(rels[drawing_rel][1]).decode(),
            }
            import os

            d = posixpath.join(out, name)
            os.makedirs(d, exist_ok=True)
            for key, xml in parts.items():
                open(posixpath.join(d, key + ".xml"), "w", encoding="utf-8").write(neutral(xml) if key in ("data", "drawing") else xml)
            open(posixpath.join(d, "frame.txt"), "w").write(f"{ext[0]} {ext[1]}\n")
            print(name, ext)
            return
    sys.exit(f"{data_part} not found in {deck}")


if __name__ == "__main__":
    out = sys.argv[1]
    for spec in sys.argv[2:]:
        deck, part, name = spec.rsplit(":", 2)
        extract(deck, part, name, out)
