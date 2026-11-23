#!/bin/sh
# Print the differences between the copy of the ClickHouse Dash chart engine and an upstream checkout (9.24).
# Use: tools/update-chdash.sh UPSTREAM_CHECKOUT
# This tool is read-only: it changes no file of the repository and no file of the checkout.
# It reads web/js/vendor/chdash/chdash.lock.json. For each file with origin "copy" it prints the unified
# diff between the copy in web/ and the upstream file at the path upstream_path. It also says when the
# commit of the checkout differs from the commit of the lock. Update the copy by hand, then the lock.
# The copies hold the marked edits E1 to E21 (docs/engine-patches.md), with the removals of the code
# that Hune never calls (E17 to E21, ID-K300). vendor/chdash.patch holds them as one diff. An update applies it again:
#   1. Copy each upstream file of the lock at the new commit over its copy. Write the new commit and tag in the lock.
#   2. Run: patch -p1 -d web < vendor/chdash.patch. Do each rejected hunk by hand with docs/engine-patches.md.
#   3. Look for new upstream calls of the removed code. A new call changes the removal or cancels it.
#   4. Run: tools/check-vendor-lock.sh --chdash-update UPSTREAM_CHECKOUT, then read vendor/chdash.patch.
#   5. Run: tools/check-vendor-lock.sh --chdash and the front tests.
set -eu
[ "$#" -eq 1 ] || { echo "use: tools/update-chdash.sh UPSTREAM_CHECKOUT" >&2; exit 2; }
cd "$(dirname "$0")/.."
up=$1
[ -d "$up" ] || { echo "error: $up is not a directory" >&2; exit 2; }
lock=web/js/vendor/chdash/chdash.lock.json
[ -f "$lock" ] || { echo "error: $lock does not exist" >&2; exit 2; }

python3 -I - "$lock" "$up" <<'PY'
import json, os, subprocess, sys

lock = json.load(open(sys.argv[1]))
up = sys.argv[2]
try:
    head = subprocess.run(["git", "-C", up, "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
except Exception:
    head = "unknown"
print("lock commit:     %s (%s)" % (lock["commit"], lock["tag"]))
print("checkout commit: %s" % head)
if head != lock["commit"]:
    print("The commits differ.")
changed = 0
for e in lock["files"]:
    if e.get("origin") != "copy":
        continue
    mine = os.path.join("web", e["path"])
    theirs = os.path.join(up, e["upstream_path"])
    if not os.path.isfile(theirs):
        print("\n== %s: the upstream file %s does not exist" % (e["path"], e["upstream_path"]))
        changed += 1
        continue
    r = subprocess.run(["diff", "-u", "--label", "hune/" + e["path"], "--label", "upstream/" + e["upstream_path"], mine, theirs],
                       capture_output=True, text=True)
    if r.returncode != 0:
        changed += 1
        print("\n== %s" % e["path"])
        print(r.stdout, end="")
print("\n%d copied file(s) differ from the checkout." % changed)
PY
