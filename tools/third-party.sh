#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Generate LICENSE-THIRD-PARTY (17.4) from `cargo metadata --locked`, with no network.
# Use: tools/third-party.sh [OUTPUT_FILE]      (default: the standard output)
# The crates are those of deps.lock.txt: the resolved graph of the two musl targets, outside the workspace.
# The local copy of a crate under vendor/ counts as a third-party crate. Its LICENSE file is in the copy.
# For each crate the file gives the name, the version, the licence expression and the text of its licence
# files (LICENSE*, COPYING*, UNLICENSE*, NOTICE*). Crates with equal texts share one block.
# It adds the MIT notice of the copied chart engine and of the copied colour tokens, the notice of the font Inter
# (SIL Open Font License 1.1, embedded in the binary) and the notice of the crate hclconf.
# Check: each licence file under web/ (LICENSE*, LICENCE*, COPYING*, NOTICE*, OFL*) must be one of the notices
# that this script adds, else the script stops with an error and writes nothing. A new copied file in web/ thus
# needs its notice here. The font notice must hold the text of the SIL Open Font License, Version 1.1.
# The crate sources must be in the cargo cache (run `cargo fetch --locked` or a build first).
set -eu
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The licence files of web/ that this script adds below. The binary embeds web/ (9.3).
web_notices="web/js/vendor/chdash/LICENSE web/fonts/inter/LICENSE"
find web -type f \( -iname 'LICENSE*' -o -iname 'LICENCE*' -o -iname 'COPYING*' -o -iname 'NOTICE*' -o -iname 'OFL*' \) |
    sort >"$tmp/web-notices.txt"
missing=
while read -r f; do
    case " $web_notices " in
    *" $f "*) ;;
    *) missing="$missing $f" ;;
    esac
done <"$tmp/web-notices.txt"
[ -z "$missing" ] || { echo "third-party.sh: no notice block for:$missing (add it to this script)" >&2; exit 1; }
for f in $web_notices; do
    [ -f "$f" ] || { echo "third-party.sh: $f does not exist" >&2; exit 1; }
done
[ -f web/fonts/inter/InterVariable.woff2 ] || { echo "third-party.sh: web/fonts/inter/InterVariable.woff2 does not exist" >&2; exit 1; }
grep -q 'SIL Open Font License, Version 1.1' web/fonts/inter/LICENSE ||
    { echo "third-party.sh: web/fonts/inter/LICENSE does not hold the SIL Open Font License, Version 1.1" >&2; exit 1; }
for t in x86_64-unknown-linux-musl aarch64-unknown-linux-musl; do
    cargo metadata --locked --offline --format-version 1 --filter-platform "$t" >"$tmp/$t.json"
done
out=$(python3 -I - "$tmp" <<'PY'
import glob, hashlib, json, os, sys

pkgs = {}
for f in sorted(glob.glob(os.path.join(sys.argv[1], "*.json"))):
    d = json.load(open(f))
    ids = {n["id"] for n in d["resolve"]["nodes"]}
    vendor = d["workspace_root"] + "/vendor/"
    for p in d["packages"]:
        if p["id"] in ids and (p.get("source") is not None or p["manifest_path"].startswith(vendor)):
            pkgs[(p["name"], p["version"])] = p
blocks = {}
for (name, ver), p in sorted(pkgs.items()):
    root = os.path.dirname(p["manifest_path"])
    texts = []
    for fn in sorted(os.listdir(root)):
        up = fn.upper()
        if up.startswith(("LICENSE", "LICENCE", "COPYING", "UNLICENSE", "NOTICE")) and os.path.isfile(os.path.join(root, fn)):
            texts.append("--- %s ---\n%s" % (fn, open(os.path.join(root, fn), encoding="utf-8", errors="replace").read().rstrip()))
    text = "\n\n".join(texts) if texts else "(no licence file in the crate; the licence expression above applies)"
    if not texts and p.get("license") == "Apache-2.0":
        text += "\n(the text of the Apache License 2.0 is in the file LICENSE-APACHE of this release)"
    key = hashlib.sha256((str(p.get("license")) + text).encode()).hexdigest()
    blocks.setdefault(key, {"license": p.get("license") or p.get("license_file") or "unknown", "crates": [], "text": text})
    blocks[key]["crates"].append("%s %s" % (name, ver))
print("Licence notices of the third-party software in this release of Hune.\n")
for key in sorted(blocks, key=lambda k: blocks[k]["crates"][0]):
    b = blocks[key]
    print("=" * 78)
    print("Crates: " + ", ".join(b["crates"]))
    print("Licence: " + str(b["license"]))
    print("=" * 78)
    print(b["text"])
    print()
PY
)
{
    printf '%s\n' "$out"
    echo "=============================================================================="
    echo "Copied chart engine of ClickHouse Dash (web/js/vendor/chdash, web/css/vendor/chdash)"
    echo "Licence: MIT"
    echo "Colour theme of the chrome: the token values of web/css/vendor/chdash/00-tokens.css,"
    echo "copied from src/static/css/00-tokens.css of ClickHouse Dash (tag v1.3.1, commit e970694e5e0b17b07cf187f3128af93fa1a82b76, licence MIT)."
    echo "Time range picker: the layout, the texts, the keys and the values of web/css/picker.css follow"
    echo "src/static/app_timerange.js and its CSS in ClickHouse Dash (same tag and commit, licence MIT). The code of web/js/time_picker.js is new."
    echo "=============================================================================="
    cat web/js/vendor/chdash/LICENSE
    echo
    echo "=============================================================================="
    echo "Font Inter 4.1 (web/fonts/inter/InterVariable.woff2, embedded in the binary)"
    echo "Licence: OFL-1.1 (SIL Open Font License, Version 1.1)"
    echo "=============================================================================="
    cat web/fonts/inter/LICENSE
    echo
    echo "=============================================================================="
    echo "Crate hclconf (crates/hclconf)"
    echo "=============================================================================="
    if ls crates/hclconf/LICENSE* >/dev/null 2>&1; then
        cat crates/hclconf/LICENSE*
    else
        cat crates/hclconf/PROVENANCE
    fi
} >"$tmp/result.txt"
if [ "$#" -ge 1 ]; then
    cp "$tmp/result.txt" "$1"
else
    cat "$tmp/result.txt"
fi
