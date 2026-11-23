#!/usr/bin/env python3
"""Rename the product from its old name to Hune in the whole repository (owner request of 2026-11-12).

Use: python3 -I tools/rename-hune.py [--dry-run | --apply | --check | --regenerate | --migrate-volume DIR]

  --dry-run          list the files and the paths that --apply changes, with the number of changes. No write.
  --apply            change the text files and the paths. Keep a copy of each changed file in the backup folder.
  --check            list every remaining occurrence of the old name. Exit code 1 if one remains.
  --regenerate       after --apply: rewrite Cargo.lock and fuzz/Cargo.lock (cargo, offline), the lock of the
                     copied chart engine, and the lock and the patch of vendor/mongodb.
  --migrate-volume D move the folder D/.<old> to D/.hune (local volume). A copy goes to the backup folder first.
  --backup DIR       the backup folder (default: target/orch/rename-backup/files).

Rules of the rename:
  - The three spellings change: old name in lower case, with a capital letter and in upper case.
  - A line that holds the text "rename-keep" does not change. The rename note of CHANGELOG.md uses
    this mark.
  - The absolute path of the repository folder (the parent folder of tools/) does not change.
  - These paths do not change: .git, target, __pycache__, and this script.
  - A binary file does not change. A file name that holds the old name changes.
The script is idempotent: after --apply, a second --apply finds nothing to change.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

OLD = "vi" + "gie"  # the old name is spelled in two parts so that --check does not find the script itself
NEW = "hune"
SKIP_DIRS = {".git", "target", "__pycache__"}
SKIP_FILES = {"tools/rename-hune.py"}
KEEP_MARK = "rename-keep"

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
PATTERN = re.compile(r"((?-i:" + re.escape(ROOT) + r"))|(" + OLD + r")", re.IGNORECASE)


def map_case(word):
    if word.islower():
        return NEW
    if word.isupper():
        return NEW.upper()
    if word == OLD.capitalize():
        return NEW.capitalize()
    raise ValueError("mixed-case spelling of the old name: %r" % word)


def rewrite_line(line):
    """Return the line and the number of changes."""
    if KEEP_MARK in line:
        return line, 0
    count = 0

    def sub(m):
        nonlocal count
        if m.group(1):
            return m.group(1)
        count += 1
        return map_case(m.group(2))

    return PATTERN.sub(sub, line), count


def rewrite_text(text):
    out = []
    total = 0
    for line in text.splitlines(keepends=True):
        new, n = rewrite_line(line)
        out.append(new)
        total += n
    return "".join(out), total


def walk(root):
    """Yield (relative path, absolute path) of each file to examine, in a stable order."""
    for base, dirs, names in os.walk(root):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
        for n in sorted(names):
            full = os.path.join(base, n)
            rel = os.path.relpath(full, root).replace(os.sep, "/")
            if rel in SKIP_FILES or os.path.islink(full):
                continue
            yield rel, full


def read_text(full):
    with open(full, "rb") as f:
        data = f.read()
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def plan(root):
    """Return the list of text changes and the list of path renames."""
    changes = []
    for rel, full in walk(root):
        text = read_text(full)
        if text is None:
            continue
        new, n = rewrite_text(text)
        if n:
            changes.append((rel, full, new, n))
    renames = []
    for base, dirs, names in os.walk(root, topdown=False):
        rel_base = os.path.relpath(base, root)
        if rel_base.split(os.sep)[0] in SKIP_DIRS:
            continue
        for n in dirs + names:
            if OLD in n.lower():
                renames.append((os.path.join(base, n), os.path.join(base, rewrite_line(n)[0])))
    return changes, renames


def backup_file(backup, rel, full):
    dest = os.path.join(backup, rel)
    if os.path.exists(dest):
        return
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    shutil.copy2(full, dest)


def cmd_dry_run(root):
    changes, renames = plan(root)
    for rel, _, _, n in changes:
        print("%5d  %s" % (n, rel))
    for a, b in renames:
        print("rename  %s -> %s" % (os.path.relpath(a, root), os.path.relpath(b, root)))
    print("%d file(s) to change, %d change(s), %d path(s) to rename." % (len(changes), sum(c[3] for c in changes), len(renames)))
    return 0


def cmd_apply(root, backup):
    changes, renames = plan(root)
    for rel, full, new, _ in changes:
        backup_file(backup, rel, full)
        with open(full, "w", encoding="utf-8", newline="") as f:
            f.write(new)
    for a, b in renames:
        os.rename(a, b)
    print("%d file(s) changed, %d change(s), %d path(s) renamed." % (len(changes), sum(c[3] for c in changes), len(renames)))
    return 0


def cmd_check(root):
    found = 0
    for rel, full in walk(root):
        if OLD in rel.lower():
            print("%s: the path holds the old name" % rel)
            found += 1
        text = read_text(full)
        if text is None:
            continue
        for i, line in enumerate(text.splitlines(), 1):
            if KEEP_MARK in line:
                continue
            stripped = PATTERN.sub("", line)
            if OLD in stripped.lower():
                print("%s:%d: %s" % (rel, i, line.strip()[:120]))
                found += 1
    print("%d occurrence(s) of the old name." % found)
    return 1 if found else 0


def sha256_file(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def run(cmd, root, env_extra=None):
    env = dict(os.environ)
    env["PATH"] = os.path.expanduser("~/.cargo/bin") + os.pathsep + env.get("PATH", "")
    print("+ " + " ".join(cmd))
    subprocess.run(cmd, cwd=root, check=True, env=env)


def cmd_regenerate(root):
    # Cargo writes the lock files again with the new package names. No download: --offline.
    # `cargo update --workspace` changes only the entries of the workspace members. No external crate changes.
    run(["cargo", "update", "--workspace", "--offline"], root)
    run(["cargo", "update", "--workspace", "--offline", "--manifest-path", "fuzz/Cargo.toml"], root)
    # The lock of the copied chart engine: new SHA-256 of each copied file. The upstream hashes stay.
    lock_path = os.path.join(root, "web/js/vendor/chdash/chdash.lock.json")
    with open(lock_path, encoding="utf-8") as f:
        lock = json.load(f)
    for e in lock["files"]:
        e["sha256"] = sha256_file(os.path.join(root, "web", e["path"]))
    with open(lock_path, "w", encoding="utf-8") as f:
        json.dump(lock, f, indent=2, ensure_ascii=False)
        f.write("\n")
    # The lock and the patch of the local copy of the crate mongodb.
    run(["sh", "tools/check-vendor-lock.sh", "--mongodb-update"], root)
    return 0


def cmd_migrate_volume(root, backup, volume):
    old = os.path.join(volume, "." + OLD)
    new = os.path.join(volume, "." + NEW)
    if not os.path.isdir(old):
        print("nothing to do: %s does not exist." % old)
        return 0
    if os.path.exists(new):
        print("error: %s exists already." % new, file=sys.stderr)
        return 1
    dest = os.path.join(backup, "volume-dot-folder")
    shutil.copytree(old, dest, dirs_exist_ok=True)
    os.rename(old, new)
    print("moved %s to %s (copy in %s)." % (old, new, dest))
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--dry-run", action="store_true")
    g.add_argument("--apply", action="store_true")
    g.add_argument("--check", action="store_true")
    g.add_argument("--regenerate", action="store_true")
    g.add_argument("--migrate-volume", metavar="DIR")
    ap.add_argument("--backup", default=os.path.join(ROOT, "target/orch/rename-backup/files"))
    a = ap.parse_args()
    if a.dry_run:
        return cmd_dry_run(ROOT)
    if a.apply:
        return cmd_apply(ROOT, a.backup)
    if a.check:
        return cmd_check(ROOT)
    if a.regenerate:
        return cmd_regenerate(ROOT)
    return cmd_migrate_volume(ROOT, a.backup, a.migrate_volume)


if __name__ == "__main__":
    sys.exit(main())
