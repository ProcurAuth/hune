#!/bin/sh
# Steps 3 to 5 of the release workflow (17.3, 17.4): build one target two times, check, make the archive.
# The workflow release.yaml calls this script on a runner of each architecture. The owner can run it by hand
# on a machine of each architecture, with no GitHub Actions: then tests/ci/release.sh makes the release.
# Use: tests/ci/release-build.sh VERSION [--local]
#   VERSION is x.y.z. The script builds for the architecture of this machine (a release runner builds
#   its own architecture). The output is dist/hune_<VERSION>_linux_<arch>.tar.gz. In a local release, copy
#   the archive of the other machine into dist/ too.
# Steps:
#   1. Build the musl binary two times, in two new containers and two target directories (17.2).
#      The two binaries must have the same SHA-256.
#   2. Check the binary: no program interpreter (readelf -l); `hune version` prints the version and the commit.
#   3. Make the directory hune_<VERSION>_linux_<arch>/ with the files of 17.4 and the deterministic archive.
#   LICENSE-THIRD-PARTY comes from tools/third-party.sh. config.reference.hcl is the output of
#   `hune print-reference --format hcl --profile local`.
# The script never pushes anything. SHA256SUMS is made by the release job of release.yaml, or by tests/ci/release.sh, which see both archives.
set -eu
. "$(dirname "$0")/ci-lib.sh"
[ "$#" -ge 1 ] || { echo "use: tests/ci/release-build.sh VERSION [--local]" >&2; exit 2; }
V=$1
shift
ci_init "$@"
printf '%s\n' "$V" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$' || ci_die "VERSION must be x.y.z, found '$V'"

triple=$(ci_host_triple)
arch=amd64
case $triple in aarch64*) arch=arm64 ;; esac
commit=$(git rev-parse HEAD)
SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)
export SOURCE_DATE_EPOCH

rm -rf target/rel1 target/rel2
ci_build_binary "$triple" target/rel1
ci_build_binary "$triple" target/rel2
b1=target/rel1/$triple/release/hune
b2=target/rel2/$triple/release/hune
h1=$(sha256sum "$b1" | cut -d' ' -f1)
h2=$(sha256sum "$b2" | cut -d' ' -f1)
echo "build 1: $h1"
echo "build 2: $h2"
[ "$h1" = "$h2" ] || ci_die "the two builds differ: the build is not reproducible"

if in_builder readelf -lW "$b1" | grep -qE 'INTERP|Requesting program interpreter'; then
    ci_die "the binary has a program interpreter"
fi
ver=$("$b1" version)
echo "$ver"
case $ver in
"hune $V $(printf '%s' "$commit" | cut -c1-12) "*) ;;
*) ci_die "hune version must print 'hune $V <12 characters of $commit> <target>', found '$ver'" ;;
esac

name=hune_${V}_linux_${arch}
rm -rf "dist/$name" "dist/$name.tar.gz"
mkdir -p "dist/$name"
cp "$b1" "dist/$name/hune"
cp build/Dockerfile LICENSE README.md CHANGELOG.md "dist/$name/"
"$b1" print-reference --format hcl --profile local >"dist/$name/config.reference.hcl"
cp -R deploy "dist/$name/deploy"
in_builder tools/third-party.sh "dist/$name/LICENSE-THIRD-PARTY"
find "dist/$name" -type d -exec chmod 0755 {} +
find "dist/$name" -type f -exec chmod 0644 {} +
chmod 0755 "dist/$name/hune"
(
    cd dist
    tar --sort=name --mtime="@${SOURCE_DATE_EPOCH}" --owner=0 --group=0 --numeric-owner --format=gnu \
        -cf - "$name" | gzip -9n >"$name.tar.gz"
)
sha256sum "dist/$name.tar.gz"
echo "release-build: ok"
