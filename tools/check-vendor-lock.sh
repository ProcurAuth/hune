#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Check the copied files against their locks (16.18, "Copied chart engine" and "Lexer vectors").
# Use: tools/check-vendor-lock.sh [--chdash | --lexer | --mongodb]      (default: all three)
#   --chdash: each file of web/js/vendor/chdash/ and web/css/vendor/chdash/ is in
#             web/js/vendor/chdash/chdash.lock.json with its SHA-256 (9.24). The lock file itself is exempt.
#             A file whose sha256 differs from upstream_sha256 holds a `// hune:` comment. Each relative
#             import of a copied JavaScript file names a file of the lock. The reverse of vendor/chdash.patch,
#             applied to the copies, gives the upstream hashes of the lock.
#   --chdash-update DIR: write vendor/chdash.patch and the sha256 values of the chart engine lock again. DIR is a
#             ClickHouse Dash checkout. The tool reads each upstream file at the commit of the lock with
#             `git show`, and it changes no file of DIR. A changed upstream hash is printed. The file list of the
#             lock stays a manual edit. It is not part of the default checks. Run it after an edit of a copy,
#             then read the patch and run --chdash.
#   --lexer:  the SHA-256 of tests/fixtures/sql-lex-vectors.json equals its line in tests/images.lock.
#   --mongodb: the local copy of the crate mongodb (8b.15). The lock vendor/mongodb.lock.json holds the
#             SHA-256 of each file of vendor/mongodb/. A file that is not in the lock, a file that the lock lists
#             and the copy lacks, and a file with a changed hash are errors. Each changed Rust file holds a
#             `// hune:` comment. The reverse of vendor/mongodb.patch, applied to the copy, gives the upstream
#             hashes of the lock. The root Cargo.toml and fuzz/Cargo.toml patch the crate to the copy, and the
#             Cargo.lock files of the repository have the crate with no source.
#   --mongodb-update [DIR]: write vendor/mongodb.patch and the file list of the lock again. DIR is the original
#             crate source (default: the cargo registry source of mongodb 3.10.0, read only). It is not part of
#             the default checks. Run it after an edit of the copy, then read the patch.
# A file that does not exist is an error with a clear message. Exit code: 0, 1 (check failed), 2 (bad call).
set -eu
cd "$(dirname "$0")/.."
mode=${1:-all}
case $mode in
all | --chdash | --lexer | --mongodb | --mongodb-update) ;;
--chdash-update) [ -n "${2:-}" ] && [ -d "$2" ] || { echo "use: tools/check-vendor-lock.sh --chdash-update UPSTREAM_CHECKOUT" >&2; exit 2; } ;;
*) echo "use: tools/check-vendor-lock.sh [--chdash | --chdash-update DIR | --lexer | --mongodb | --mongodb-update [DIR]]" >&2; exit 2 ;;
esac

status=0

if [ "$mode" = all ] || [ "$mode" = --chdash ] || [ "$mode" = --chdash-update ]; then
    python3 -I - "$mode" "${2:-}" <<'PY' || status=1
import difflib, hashlib, json, os, re, subprocess, sys

MODE = sys.argv[1]
UPSTREAM = sys.argv[2]
LOCK = "web/js/vendor/chdash/chdash.lock.json"
# The readable diff of the edits of the copies (9.24). It is outside web/: the build refuses a file of web/ with an
# extension that 9.3 does not list.
PATCH = "vendor/chdash.patch"
DIRS = ["web/js/vendor/chdash", "web/css/vendor/chdash"]
errors = []


def sha(data):
    return hashlib.sha256(data).hexdigest()


def make_patch(old, new):
    # One unified diff for each copy that differs from upstream, with the paths of the lock (relative to web/).
    parts = []
    for path in sorted(old):
        a, b = old[path], new[path]
        if a == b:
            continue
        for side in (a, b):
            if not side.endswith(b"\n"):
                raise SystemExit("error: %s does not end with a line end" % path)
        parts.extend(difflib.unified_diff(a.decode().splitlines(keepends=True), b.decode().splitlines(keepends=True),
                                          "a/" + path, "b/" + path, n=3))
    return "".join(parts)


def parse_patch(text):
    files = {}
    cur = None
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.startswith("--- "):
            if i + 1 >= len(lines) or not lines[i + 1].startswith("+++ b/") or not line.startswith("--- a/"):
                raise ValueError("bad file header: %r" % line[:60])
            path = lines[i + 1][6:]
            if path in files:
                raise ValueError("%s is twice in the patch" % path)
            cur = files[path] = []
            i += 2
        elif line.startswith("@@ "):
            m = re.match(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", line)
            if not m or cur is None:
                raise ValueError("bad hunk header: %r" % line[:60])
            h = {"new_start": int(m.group(3)), "new_count": 1 if m.group(4) is None else int(m.group(4)), "lines": []}
            i += 1
            while i < len(lines) and lines[i][:1] in (" ", "+", "-") and not lines[i].startswith(("--- a/", "+++ b/")):
                h["lines"].append(lines[i])
                i += 1
            cur.append(h)
        else:
            raise ValueError("unexpected line: %r" % line[:60])
    return files


def reverse_apply(path, hunks, new_text):
    """Return the upstream text of a copy from its text and its hunks."""
    cur = new_text.splitlines(keepends=True)
    out = []
    pos = 0
    for h in hunks:
        start = h["new_start"] - 1 if h["new_count"] else h["new_start"]
        if start < pos or start > len(cur):
            raise ValueError("%s: a hunk is out of order or out of range" % path)
        out.extend(cur[pos:start])
        pos = start
        for l in h["lines"]:
            body = l[1:] + "\n"
            if l[0] in (" ", "+"):
                if pos >= len(cur) or cur[pos] != body:
                    raise ValueError("%s: the patch does not match the copy at line %d" % (path, pos + 1))
                pos += 1
            if l[0] in (" ", "-"):
                out.append(body)
    out.extend(cur[pos:])
    return "".join(out)


if not os.path.isfile(LOCK):
    print("error: %s does not exist" % LOCK)
    sys.exit(1)
lock = json.load(open(LOCK))
entries = {e["path"]: e for e in lock["files"]}
on_disk = set()
for d in DIRS:
    if not os.path.isdir(d):
        errors.append("directory %s does not exist" % d)
        continue
    for name in sorted(os.listdir(d)):
        full = os.path.join(d, name)
        if full == LOCK or not os.path.isfile(full):
            continue
        key = full[len("web/"):]
        on_disk.add(key)
        if key not in entries:
            errors.append("%s is not in the lock" % full)
for path in sorted(set(entries) - on_disk):
    errors.append("web/%s is in the lock but the file does not exist" % path)

if MODE == "--chdash-update":
    # Write the patch and the hashes of the lock again. The file list of the lock stays a manual edit (closed set).
    if errors:
        for e in errors:
            print("chdash lock: " + e)
        print("chdash lock: edit the file list of the lock first, then run --chdash-update again")
        sys.exit(1)
    upstream, current = {}, {}
    for path, e in sorted(entries.items()):
        data = open(os.path.join("web", path), "rb").read()
        e["sha256"] = sha(data)
        if e.get("origin") != "copy":
            continue
        r = subprocess.run(["git", "-C", UPSTREAM, "show", "%s:%s" % (lock["commit"], e["upstream_path"])], capture_output=True)
        if r.returncode != 0:
            print("chdash lock: git show %s:%s in %s failed: %s" % (lock["commit"], e["upstream_path"], UPSTREAM, r.stderr.decode().strip()))
            sys.exit(1)
        if sha(r.stdout) != e["upstream_sha256"]:
            print("chdash lock: %s: upstream_sha256 changes from %s to %s (commit %s)" % (path, e["upstream_sha256"], sha(r.stdout), lock["commit"]))
            e["upstream_sha256"] = sha(r.stdout)
        upstream[path] = r.stdout
        current[path] = data
    with open(PATCH, "w") as f:
        f.write(make_patch(upstream, current))
    out = {k: lock[k] for k in ("upstream", "tag", "commit", "licence")}
    out["patch"] = PATCH
    out["files"] = [entries[p] for p in sorted(entries)]
    with open(LOCK, "w") as f:
        f.write(json.dumps(out, indent=2) + "\n")
    changed = sum(1 for e in entries.values() if e.get("origin") == "copy" and e["sha256"] != e["upstream_sha256"])
    print("wrote %s (%d files) and %s (%d changed copies)" % (LOCK, len(entries), PATCH, changed))
    sys.exit(0)

for path, e in sorted(entries.items()):
    full = os.path.join("web", path)
    if not os.path.isfile(full):
        continue
    data = open(full, "rb").read()
    digest = sha(data)
    if digest != e["sha256"]:
        errors.append("%s: sha256 is %s, the lock says %s" % (full, digest, e["sha256"]))
    if e.get("origin") == "copy" and e.get("upstream_sha256") != e["sha256"]:
        if b"// hune:" not in data:
            errors.append("%s differs from upstream but holds no '// hune:' comment" % full)
    if path.endswith(".js"):
        base = os.path.dirname(path)
        for m in re.finditer(rb"""(?:from|import)\s*\(?\s*['"](\.[^'"]+)['"]""", data):
            target = os.path.normpath(os.path.join(base, m.group(1).decode()))
            if target not in entries:
                errors.append("%s imports %s, which is not in the lock" % (full, target))

# The reverse of the patch, applied to each copy that differs, gives its upstream hash (as for --mongodb).
if lock.get("patch") != PATCH:
    errors.append("the lock has no member \"patch\": \"%s\"" % PATCH)
spec = {}
if not os.path.isfile(PATCH):
    errors.append("%s does not exist" % PATCH)
else:
    try:
        spec = parse_patch(open(PATCH).read())
    except ValueError as ex:
        errors.append("%s: %s" % (PATCH, ex))
changed = {p for p, e in entries.items() if e.get("origin") == "copy" and e.get("upstream_sha256") != e["sha256"]}
for p in sorted(set(spec) ^ changed):
    errors.append("%s: the patch and the lock disagree on whether the file differs from upstream" % p)
for p in sorted(set(spec) & changed):
    full = os.path.join("web", p)
    if not os.path.isfile(full):
        continue
    try:
        old = reverse_apply(p, spec[p], open(full, "rb").read().decode())
    except ValueError as ex:
        errors.append("patch: %s" % ex)
        continue
    if sha(old.encode()) != entries[p]["upstream_sha256"]:
        errors.append("%s: the reverse of the patch gives sha256 %s, the lock says upstream %s" % (p, sha(old.encode()), entries[p]["upstream_sha256"]))
for e in errors:
    print("chdash lock: " + e)
sys.exit(1 if errors else 0)
PY
fi

if [ "$mode" = all ] || [ "$mode" = --lexer ]; then
    f=tests/fixtures/sql-lex-vectors.json
    if [ ! -f "$f" ]; then
        echo "lexer vectors: $f does not exist"
        status=1
    else
        line=$(grep -E '^sql-lex-vectors ' tests/images.lock || true)
        want=$(printf '%s' "$line" | sed -n 's/.*@sha256:\([0-9a-f]\{64\}\).*/\1/p')
        got=$(sha256sum "$f" | awk '{print $1}')
        if [ -z "$line" ]; then
            echo "lexer vectors: tests/images.lock has no line 'sql-lex-vectors'"
            status=1
        elif [ -z "$want" ]; then
            echo "lexer vectors: the line of tests/images.lock has no SHA-256 (digest: TO-PIN?)"
            status=1
        elif [ "$want" != "$got" ]; then
            echo "lexer vectors: $f has sha256 $got, tests/images.lock says $want"
            status=1
        fi
    fi
fi

if [ "$mode" = all ] || [ "$mode" = --mongodb ] || [ "$mode" = --mongodb-update ]; then
    python3 -I - "$mode" "${2:-}" <<'PY' || status=1
import difflib, glob, hashlib, json, os, re, sys

MODE = sys.argv[1]
ORIGINAL_ARG = sys.argv[2]
LOCK = "vendor/mongodb.lock.json"
PATCH = "vendor/mongodb.patch"
DIR = "vendor/mongodb"
# Files that the copy adds to the original: the standard Apache-2.0 text (the package of
# the crate holds no licence file). A new file holds no `// hune:` comment, and the patch does not list it.
ADDED_ALLOWED = {"LICENSE"}
errors = []


def sha(data):
    return hashlib.sha256(data).hexdigest()


def tree(root):
    out = {}
    for base, dirs, names in os.walk(root):
        dirs.sort()
        for n in sorted(names):
            full = os.path.join(base, n)
            rel = os.path.relpath(full, root).replace(os.sep, "/")
            if rel == ".cargo-ok":
                continue
            with open(full, "rb") as f:
                out[rel] = f.read()
    return out


def make_patch(old, new):
    # The patch holds the edits of the original files. A file that the copy adds is in ADDED_ALLOWED.
    parts = []
    for path in sorted(old):
        a = old.get(path)
        b = new.get(path)
        if a == b:
            continue
        for side in (a, b):
            if side is not None and not side.endswith(b"\n"):
                raise SystemExit("error: %s does not end with a line end" % path)
        la = a.decode().splitlines(keepends=True) if a is not None else []
        lb = b.decode().splitlines(keepends=True) if b is not None else []
        names = ("a/" + path if a is not None else "/dev/null", "b/" + path if b is not None else "/dev/null")
        parts.extend(difflib.unified_diff(la, lb, names[0], names[1], n=3))
    return "".join(parts)


def parse_patch(text):
    files = {}
    cur = None
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.startswith("--- "):
            old = line[4:]
            new = lines[i + 1][4:]
            path = new[2:] if new != "/dev/null" else old[2:]
            cur = {"old_none": old == "/dev/null", "new_none": new == "/dev/null", "hunks": []}
            files[path] = cur
            i += 2
        elif line.startswith("@@ "):
            m = re.match(r"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", line)
            if not m or cur is None:
                raise ValueError("bad hunk header: %r" % line[:60])
            h = {"new_start": int(m.group(3)), "new_count": 1 if m.group(4) is None else int(m.group(4)), "lines": []}
            i += 1
            while i < len(lines) and lines[i][:1] in (" ", "+", "-") and not lines[i].startswith(("--- ", "+++ ")):
                h["lines"].append(lines[i])
                i += 1
            cur["hunks"].append(h)
        else:
            raise ValueError("unexpected line: %r" % line[:60])
    return files


def reverse_apply(path, spec, new_text):
    """Return the original text of a file from its current text and its hunks."""
    cur = new_text.splitlines(keepends=True)
    out = []
    pos = 0
    for h in spec["hunks"]:
        start = h["new_start"] - 1 if h["new_count"] else h["new_start"]
        if start < pos or start > len(cur):
            raise ValueError("%s: a hunk is out of order or out of range" % path)
        out.extend(cur[pos:start])
        pos = start
        for l in h["lines"]:
            body = l[1:] + "\n"
            if l[0] in (" ", "+"):
                if pos >= len(cur) or cur[pos] != body:
                    raise ValueError("%s: the patch does not match the copy at line %d" % (path, pos + 1))
                pos += 1
            if l[0] in (" ", "-"):
                out.append(body)
    out.extend(cur[pos:])
    return "".join(out)


def registry_dir():
    if ORIGINAL_ARG:
        return ORIGINAL_ARG
    home = os.environ.get("CARGO_HOME") or os.path.expanduser("~/.cargo")
    found = sorted(glob.glob(os.path.join(home, "registry", "src", "*", "mongodb-3.10.0")))
    if not found:
        raise SystemExit("error: the source of mongodb 3.10.0 is not in the cargo registry; give its directory")
    return found[0]


if not os.path.isfile(LOCK):
    print("mongodb lock: %s does not exist" % LOCK)
    sys.exit(1)
if not os.path.isdir(DIR):
    print("mongodb lock: directory %s does not exist" % DIR)
    sys.exit(1)
with open(LOCK) as f:
    lock = json.load(f)
current = tree(DIR)

if MODE == "--mongodb-update":
    original = tree(registry_dir())
    bad = [e["path"] for e in lock["files"] if e.get("upstream_sha256") and e["path"] in original and sha(original[e["path"]]) != e["upstream_sha256"]]
    if bad:
        print("mongodb lock: the original source differs from the upstream hashes of the lock: %s" % ", ".join(bad))
        sys.exit(1)
    with open(PATCH, "w") as f:
        f.write(make_patch(original, current))
    lock["files"] = [
        {"path": p, "sha256": sha(current[p]), "upstream_sha256": sha(original[p]) if p in original else None}
        for p in sorted(current)
    ]
    with open(LOCK, "w") as f:
        f.write('{\n "crate": %s,\n "version": %s,\n "registry_checksum": %s,\n "patch": %s,\n "files": [\n'
                % tuple(json.dumps(lock[k]) for k in ("crate", "version", "registry_checksum", "patch")))
        f.write(",\n".join("  " + json.dumps(e) for e in lock["files"]))
        f.write("\n ]\n}\n")
    print("wrote %s (%d files) and %s" % (LOCK, len(lock["files"]), PATCH))
    sys.exit(0)

# 1 The files of the copy and the files of the lock are the same set, with the same hashes.
entries = {e["path"]: e for e in lock["files"]}
for p in sorted(current):
    if p not in entries:
        errors.append("%s is not in the lock" % p)
for p, e in sorted(entries.items()):
    if p not in current:
        errors.append("%s is in the lock but the file does not exist" % p)
        continue
    digest = sha(current[p])
    if digest != e["sha256"]:
        errors.append("%s: sha256 is %s, the lock says %s" % (p, digest, e["sha256"]))
    up = e.get("upstream_sha256")
    if up is None and p not in ADDED_ALLOWED:
        errors.append("%s is a new file that is not in the list of the added files" % p)
    if up is not None and up != e["sha256"]:
        if not p.endswith(".rs"):
            errors.append("%s differs from upstream and is not a Rust file" % p)
        elif b"// hune:" not in current[p]:
            errors.append("%s differs from upstream but holds no '// hune:' comment" % p)

# 2 The reverse of the patch, applied to the copy, gives the upstream files of the lock.
spec = {}
if not os.path.isfile(PATCH):
    errors.append("%s does not exist" % PATCH)
else:
    try:
        with open(PATCH) as f:
            spec = parse_patch(f.read())
    except ValueError as ex:
        errors.append("%s: %s" % (PATCH, ex))
changed = {p for p, e in entries.items() if e.get("upstream_sha256") not in (None, e["sha256"])}
for p in sorted(set(spec) ^ changed):
    errors.append("%s: the patch and the lock disagree on whether the file differs from upstream" % p)
for p in sorted(set(spec) & set(entries) & set(current)):
    up = entries[p].get("upstream_sha256")
    try:
        old = reverse_apply(p, spec[p], current[p].decode())
    except ValueError as ex:
        errors.append("patch: %s" % ex)
        continue
    if sha(old.encode()) != up:
        errors.append("%s: the reverse of the patch gives sha256 %s, the lock says upstream %s" % (p, sha(old.encode()), up))

if os.path.isfile("LICENSE-APACHE") and "LICENSE" in current:
    # LICENSE-APACHE has the notice of the project on line 190. The standard text has the template line.
    repo_lines = open("LICENSE-APACHE", "rb").read().split(b"\n")
    repo_lines[189] = b"   Copyright [yyyy] [name of copyright owner]"
    if sha(current["LICENSE"]) != sha(b"\n".join(repo_lines)):
        errors.append("LICENSE of the copy is not the standard Apache-2.0 text of LICENSE-APACHE")

# 3 The wiring: the patch section of the two manifests and the two Cargo.lock files.
for path, want in (("Cargo.toml", 'mongodb = { path = "vendor/mongodb" }'), ("fuzz/Cargo.toml", 'mongodb = { path = "../vendor/mongodb" }')):
    text = open(path).read() if os.path.isfile(path) else ""
    m = re.search(r"^\[patch\.crates-io\]\n((?:[^\[\n][^\n]*\n|\n)*)", text, re.M)
    if not m or want not in m.group(1):
        errors.append("%s has no [patch.crates-io] line '%s'" % (path, want))
for path in ("Cargo.lock", "fuzz/Cargo.lock"):
    text = open(path).read() if os.path.isfile(path) else ""
    m = re.search(r'\[\[package\]\]\nname = "mongodb"\nversion = "([^"]+)"\n((?:source|checksum)[^\n]*\n)?', text)
    if not m:
        errors.append("%s has no package mongodb" % path)
    elif m.group(1) != lock["version"] or m.group(2):
        errors.append("%s: the package mongodb is not the local copy of version %s (it has version %s, or a source)" % (path, lock["version"], m.group(1)))
if 'version = "%s"' % lock["version"] not in current.get("Cargo.toml", b"").decode():
    errors.append("%s/Cargo.toml has not the version %s of the lock" % (DIR, lock["version"]))

for e in errors:
    print("mongodb lock: " + e)
sys.exit(1 if errors else 0)
PY
fi
exit "$status"
