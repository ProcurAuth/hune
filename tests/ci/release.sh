#!/bin/sh
# Release of Hune (17.3): check, make SHA256SUMS, make the draft release, check the draft, publish it.
# The workflow release.yaml makes the same release on GitHub, as an option. The owner can run this script
# instead, on a machine, after tests/ci/all.sh. It needs no GitHub Actions and no GitHub billing.
# Use: tests/ci/release.sh VERSION [--amd64-only] [--partial-gate TEXT]
#   VERSION is x.y.z. The archives dist/hune_<VERSION>_linux_<arch>.tar.gz MUST exist: make each one with
#   `tests/ci/release-build.sh VERSION` on a machine of its architecture, then copy it into dist/.
#   --amd64-only  expects and releases the amd64 archive only (a person with no arm64 machine).
#   --partial-gate TEXT  replaces the check of target/ci-all.ok. TEXT MUST NOT be empty. It says which jobs ran on the
#                 commit and which did not. The script writes it in the release notes under "Checks". Use it only when
#                 the owner decides that a partial gate is enough. Every other check of step 1 stays.
# Steps:
#   1. Check the input: the branch is main and the commit is on origin; the work tree is clean; VERSION is the
#      version of Cargo.toml; CHANGELOG.md has `## VERSION - <date>`; the tag v<VERSION> does not exist;
#      tests/images.lock has no image with `digest: TO-PIN`; target/ci-all.ok names this commit (not with --partial-gate).
#   2. Make dist/SHA256SUMS. The notes come from tools/changelog-section.sh.
#   3. Make the draft release with gh (a draft has no tag), check its files, download them and check the sums.
#   4. Publish the draft with one call. The publish creates the tag.
#   When a step before the publish call fails, the script deletes the draft. After the publish call starts,
#   it never deletes the draft or the release: check the release and the tag by hand.
# Needs: git, gh (logged in by the owner), sha256sum. The script pushes no image and logs in to no registry.
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init

V=
ARM64=true
PARTIAL=
HAS_PARTIAL=false
while [ "$#" -gt 0 ]; do
    case $1 in
    --amd64-only) ARM64=false ;;
    --partial-gate)
        [ "$#" -ge 2 ] || ci_die "--partial-gate needs a text"
        HAS_PARTIAL=true
        PARTIAL=$2
        shift
        ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    -*) ci_die "unknown option: $1" ;;
    *)
        [ -z "$V" ] || ci_die "use: tests/ci/release.sh VERSION [--amd64-only] [--partial-gate TEXT]"
        V=$1
        ;;
    esac
    shift
done
[ -n "$V" ] || ci_die "use: tests/ci/release.sh VERSION [--amd64-only] [--partial-gate TEXT]"
if [ "$HAS_PARTIAL" = true ]; then
    # A text of blanks only is empty too.
    [ -n "$(printf '%s' "$PARTIAL" | tr -d '[:space:]')" ] || ci_die "--partial-gate needs a text that is not empty"
fi
command -v gh >/dev/null 2>&1 || ci_die "gh is not installed"

# ---- Step 1: check the input ----
printf '%s\n' "$V" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$' || ci_die "bad version: $V"
[ "$(git rev-parse --abbrev-ref HEAD)" = main ] || ci_die "run this script on the branch main"
commit=$(git rev-parse HEAD)
remote_main=$(git ls-remote origin refs/heads/main | cut -f1)
[ "$remote_main" = "$commit" ] || ci_die "origin/main is at '$remote_main', not at $commit: push main first"
[ -z "$(git status --porcelain)" ] || ci_die "the work tree is not clean"
cargo_v=$(sed -n '/^\[workspace\.package\]/,/^\[/{s/^version *= *"\(.*\)"/\1/p}' Cargo.toml)
[ "$cargo_v" = "$V" ] || ci_die "Cargo.toml has version $cargo_v, not $V"
grep -qE "^## $V - [0-9]{4}-[0-9]{2}-[0-9]{2}\$" CHANGELOG.md || ci_die "CHANGELOG.md has no heading '## $V - <date>'"
if git ls-remote --exit-code --tags origin "refs/tags/v$V" >/dev/null 2>&1; then ci_die "tag v$V exists"; fi
if grep -n 'digest: TO-PIN' tests/images.lock; then ci_die "tests/images.lock has an image with no digest"; fi
if [ "$HAS_PARTIAL" = true ]; then
    echo "release: PARTIAL GATE. target/ci-all.ok is not checked. The notes will carry this text:" >&2
    printf '%s\n' "$PARTIAL" >&2
else
    [ -f target/ci-all.ok ] && [ "$(cat target/ci-all.ok)" = "$commit" ] ||
        ci_die "no gate record for $commit: run tests/ci/all.sh on this commit first"
fi

if [ "$ARM64" = true ]; then
    ARCHIVES="hune_${V}_linux_amd64.tar.gz hune_${V}_linux_arm64.tar.gz"
    ASSETS=3
else
    ARCHIVES="hune_${V}_linux_amd64.tar.gz"
    ASSETS=2
fi
# ARCHIVES holds one or two names: the split on blanks is intended.
for f in $ARCHIVES; do
    [ -f "dist/$f" ] || ci_die "dist/$f does not exist: run tests/ci/release-build.sh $V on a machine of this architecture"
done

# ---- Step 2: SHA256SUMS and the notes ----
(cd dist && sha256sum $ARCHIVES >SHA256SUMS && cat SHA256SUMS)
tools/changelog-section.sh "$V" >target/release-notes.md
if [ "$HAS_PARTIAL" = true ]; then
    {
        printf '\n### Checks\n\n'
        printf 'The full gate (tests/ci/all.sh) did not run on this commit. The owner chose a partial gate.\n\n'
        printf '%s\n' "$PARTIAL"
    } >>target/release-notes.md
fi

# ---- Step 3: the draft, and its check ----
PUBLISHING=0
cleanup() {
    rc=$?
    if [ "$rc" -ne 0 ] && [ "$PUBLISHING" -eq 0 ] && gh release view "v$V" >/dev/null 2>&1; then
        gh release delete "v$V" --yes || true
    fi
    exit "$rc"
}
trap cleanup EXIT INT TERM
(cd dist && gh release create "v$V" --draft --target "$commit" --title "Hune v$V" \
    --notes-file ../target/release-notes.md $ARCHIVES SHA256SUMS)
n=$(gh release view "v$V" --json assets --jq '.assets | length')
[ "$n" = "$ASSETS" ] || ci_die "the draft has $n files, not $ASSETS"
rm -rf target/release-check
mkdir -p target/release-check
gh release download "v$V" --dir target/release-check
(cd target/release-check && sha256sum -c SHA256SUMS)

# ---- Step 4: publish. After this call starts, the script never deletes the draft or the release. ----
PUBLISHING=1
gh release edit "v$V" --draft=false --latest
echo "release: v$V is published"
