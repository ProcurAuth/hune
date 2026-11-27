#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Split the font Inter by unicode range into the two files that the web page serves.

Use:
    python3 tools/split_font.py            write the two woff2 files in web/fonts/inter/
    python3 tools/split_font.py --check    make no change; exit 1 when a file or web/css/fonts.css differs

The input is tools/font/InterVariable-source.woff2, the unchanged variable font Inter 4.1 (git blob
5a8d3e72ad7ffb62af3b146e1b1f54ab5813a212). The output is InterVariable-main.woff2 (Latin, Cyrillic and Greek
text, and the usual symbols) and InterVariable-rare.woff2 (all other characters of the font). The two files
together have the cmap of the input, and no character is in both files.

Each file keeps all layout features, all variation axes, all names, the hinting and the outline of .notdef.
The tool needs fontTools and brotli. It uses no network. The output is the same on each run.
"""
import re
import sys
from io import BytesIO
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "tools/font/InterVariable-source.woff2"
OUT = ROOT / "web/fonts/inter"
CSS = ROOT / "web/css/fonts.css"

# The ranges of the first file: every code point of the font except the private use area and the planes above U+FFFF.
# The font keeps only the code points that it has: a range can name more.
# A string that mixes characters of the two files is shaped in two runs, and the kerning between the runs is lost.
# The page also shows no text of the second file until its file arrives. So the first file holds each character
# that a title, a series name or a text of a viewer can use. The second file holds only the private use area
# (the alternate glyphs of the font) and the emoji of the planes above U+FFFF.
FIRST_RANGES = "U+0000-DFFF, U+F900-FFFF"


def parse_ranges(text):
    """Return the set of code points of a list such as "U+0000-00FF, U+0131"."""
    points = set()
    for item in text.split(","):
        low, _, high = item.strip().removeprefix("U+").partition("-")
        points.update(range(int(low, 16), int(high or low, 16) + 1))
    return points


def to_ranges(points):
    """Return the sorted code points as a CSS value of unicode-range, with the shortest list of ranges."""
    items = []
    run = None
    for point in sorted(points):
        if run and point == run[1] + 1:
            run[1] = point
        else:
            run = [point, point]
            items.append(run)
    return ", ".join(
        "U+%04X" % low if low == high else "U+%04X-%04X" % (low, high) for low, high in items
    )


def build(font_bytes, points):
    """Return the woff2 bytes of the font with only the code points `points`."""
    options = subset.Options()
    options.layout_features = ["*"]
    options.name_IDs = ["*"]
    options.name_languages = ["*"]
    options.name_legacy = True
    options.notdef_outline = True
    options.hinting = True
    options.flavor = "woff2"
    font = subset.load_font(BytesIO(font_bytes), options)
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=sorted(points))
    subsetter.subset(font)
    out = BytesIO()
    subset.save_font(font, out, options)
    return out.getvalue()


def plan():
    """Return the source bytes, the code points of each file and the two files."""
    data = SOURCE.read_bytes()
    cmap = set(TTFont(BytesIO(data)).getBestCmap())
    main = parse_ranges(FIRST_RANGES) & cmap
    rare = cmap - main
    files = {
        "InterVariable-main.woff2": (main, build(data, main)),
        "InterVariable-rare.woff2": (rare, build(data, rare)),
    }
    for name, (points, blob) in files.items():
        got = set(TTFont(BytesIO(blob)).getBestCmap())
        if got != points:
            raise SystemExit("split_font: the cmap of %s is not the planned set" % name)
    if main & rare or main | rare != cmap:
        raise SystemExit("split_font: the two files do not cover the cmap of the source")
    return files


def css_ranges():
    """Return the two unicode-range values that web/css/fonts.css holds, in the order of the file."""
    text = CSS.read_text(encoding="utf-8")
    return [" ".join(value.split()) for value in re.findall(r"unicode-range:([^;]+);", text)]


def main(argv):
    check = argv[1:] == ["--check"]
    if argv[1:] and not check:
        raise SystemExit(__doc__)
    files = plan()
    wanted = [to_ranges(points) for points, _ in files.values()]
    bad = []
    for name, (points, blob) in files.items():
        path = OUT / name
        print("%s: %d characters, %d bytes" % (name, len(points), len(blob)))
        if check:
            if not path.is_file() or path.read_bytes() != blob:
                bad.append(name)
        else:
            path.write_bytes(blob)
    if check:
        if css_ranges() != wanted:
            bad.append("web/css/fonts.css")
        if bad:
            raise SystemExit("split_font: differs: " + ", ".join(bad))
        print("split_font: ok")
    else:
        for value in wanted:
            print("unicode-range: %s;" % value)


if __name__ == "__main__":
    main(sys.argv)
