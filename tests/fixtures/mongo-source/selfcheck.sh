#!/bin/sh
# Self check of the fixture of the job `mongosource` (16.9b): does init.js give the users and the data that the
# cases expect? It does not need the source code of Hune.
#
# Use: selfcheck.sh CONTAINER PORT EXPECT_DOCS EXPECT_BIG [sha1|plain]
#   The container runs the compose project tests/compose/mongosource.yaml. The script uses `docker exec` and the
#   mongosh of the container (file selfcheck.js), so it sends nothing outside the container.
#   PORT is the port of the server. EXPECT_DOCS is the size of node_load, and EXPECT_BIG is the size of the
#   text of the collection `big`. The last argument `sha1` is for the
#   server that offers SCRAM-SHA-1 only, and `plain` is for the server that offers PLAIN only.
# Environment (mandatory for the checks of the server): MGS_EXPECT_VERSION (for example 7.0). Optional:
#   MGS_EXPECT_NOSCRIPTING (1, the default, or 0) and MGS_EXPECT_REPLSET (a set name, default none).
# Environment (mandatory): MGS_ADMIN_PASSWORD, MGS_RO_PASSWORD, MGS_RW_PASSWORD, MGS_BAD_PASSWORD,
#   MGS_LEGACY_PASSWORD. Environment (optional): DOCKER (default docker).
#   The passwords go to the container as process variables. Not one of them is on a command line.
# Output: one line "ok" or "FAIL" for each check, named FXnn. Exit code: 0 when all checks pass, 1 else.
set -eu

CONTAINER=${1:?use: selfcheck.sh CONTAINER PORT EXPECT_DOCS EXPECT_BIG}
PORT=${2:?use: selfcheck.sh CONTAINER PORT EXPECT_DOCS EXPECT_BIG}
EXPECT_DOCS=${3:?use: selfcheck.sh CONTAINER PORT EXPECT_DOCS EXPECT_BIG}
EXPECT_BIG=${4:?use: selfcheck.sh CONTAINER PORT EXPECT_DOCS EXPECT_BIG}
SHA1_ONLY=0
PLAIN_ONLY=0
[ "${5:-}" != sha1 ] || SHA1_ONLY=1
[ "${5:-}" != plain ] || PLAIN_ONLY=1
DOCKER=${DOCKER:-docker}
: "${MGS_EXPECT_VERSION:?}" "${MGS_ADMIN_PASSWORD:?}" "${MGS_RO_PASSWORD:?}" "${MGS_RW_PASSWORD:?}" "${MGS_BAD_PASSWORD:?}" "${MGS_LEGACY_PASSWORD:?}"

out=$("$DOCKER" exec -i \
    -e "MGS_PORT=$PORT" -e MGS_ADMIN_PASSWORD -e MGS_RO_PASSWORD -e MGS_RW_PASSWORD -e MGS_BAD_PASSWORD \
    -e MGS_LEGACY_PASSWORD -e "MGS_SHA1_ONLY=$SHA1_ONLY" -e "MGS_PLAIN_ONLY=$PLAIN_ONLY" -e "MGS_EXPECT_VERSION=$MGS_EXPECT_VERSION" \
    -e "MGS_EXPECT_NOSCRIPTING=${MGS_EXPECT_NOSCRIPTING:-1}" -e "MGS_EXPECT_REPLSET=${MGS_EXPECT_REPLSET:-}" -e "MGS_EXPECT_DOCS=$EXPECT_DOCS" -e "MGS_EXPECT_BIG=$EXPECT_BIG" \
    "$CONTAINER" mongosh --quiet --norc --nodb --file /fixtures/selfcheck.js 2>&1) || {
    printf '%s\n' "$out"
    echo "FAIL    FX00 selfcheck.js did not finish"
    exit 1
}
printf '%s\n' "$out"
n=$(printf '%s\n' "$out" | sed -n 's/^failed \([0-9][0-9]*\)$/\1/p' | tail -n 1)
[ "$n" = 0 ]
