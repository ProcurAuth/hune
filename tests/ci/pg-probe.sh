#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Job `pg-probe` (8a.12, 16.9a): the container test of the PostgreSQL probe, on PostgreSQL 15 and 18.
# Use: tests/ci/pg-probe.sh [--versions "15 18"]
#   For each version, the script starts one disposable container on 127.0.0.1, loads the roles and
#   databases of tests/fixtures/postgres-probe (a read-only role, a hardened role, and one role
#   for each forbidden right), runs the ignored tests `pg_probe_*` of the crate hune-source, and
#   removes the container with its volume.
#   The password of the roles is random and lives in target/pg-probe-<pid>/ only.
#   Names of the containers start with `hune-test-pgprobe-`. The script reads the images from the
#   local cache and sends nothing to a registry.
#   Variables: DOCKER (the docker command, default `docker`), HUNE_PG_VERSIONS,
#              IMAGE_POSTGRES_15 and IMAGE_POSTGRES_18 (default `postgres:15` and `postgres:18`).
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
if [ -d "$HOME/.cargo/bin" ]; then
    PATH="$HOME/.cargo/bin:$PATH"
    export PATH
fi
DOCKER=${DOCKER:-docker}
VERSIONS=${HUNE_PG_VERSIONS:-"15 18"}
while [ $# -gt 0 ]; do
    case $1 in
    --versions) VERSIONS=$2; shift 2 ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) echo "error: unknown option: $1" >&2; exit 1 ;;
    esac
done

RUN=$$
WORK="$ROOT/target/pg-probe-$RUN"
mkdir -p "$WORK"
chmod 700 "$WORK"
CONTAINERS=""

cleanup() {
    for c in $CONTAINERS; do
        $DOCKER rm -f -v "$c" >/dev/null 2>&1 || true
    done
    rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

random_password() {
    head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n'
}
ROLE_PW=$(random_password)
ADMIN_PW=$(random_password)

image_of() {
    case $1 in
    15) printf '%s\n' "${IMAGE_POSTGRES_15:-postgres:15}" ;;
    18) printf '%s\n' "${IMAGE_POSTGRES_18:-postgres:18}" ;;
    *) printf 'postgres:%s\n' "$1" ;;
    esac
}

status=0
for v in $VERSIONS; do
    echo "== PostgreSQL $v"
    name="hune-test-pgprobe-$v-$RUN"
    CONTAINERS="$CONTAINERS $name"
    $DOCKER run -d --name "$name" -e POSTGRES_PASSWORD="$ADMIN_PW" -p 127.0.0.1::5432 \
        -v "$ROOT/tests/fixtures/postgres-probe:/fixture:ro" \
        "$(image_of "$v")" -c fsync=off -c full_page_writes=off -c max_connections=200 >/dev/null
    i=0
    until $DOCKER exec "$name" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; do
        i=$((i + 1))
        [ $i -lt 90 ] || { echo "error: $name does not start" >&2; $DOCKER logs "$name" >&2; exit 1; }
        sleep 1
    done
    # The first server of the image stops after its init: wait for the final one.
    sleep 2
    until $DOCKER exec "$name" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; do sleep 1; done
    # The fixture runs through the socket of the container. The password goes in a psql variable.
    $DOCKER exec -w /fixture "$name" psql -U postgres -X -q -v ON_ERROR_STOP=1 -v pw="$ROLE_PW" -f roles.sql >/dev/null
    PORT=$($DOCKER port "$name" 5432/tcp | head -n 1 | sed 's/.*://')
    HUNE_TEST_PG_PORT="$PORT" HUNE_TEST_PG_PASSWORD="$ROLE_PW" HUNE_TEST_PG_ADMIN_PASSWORD="$ADMIN_PW" \
        HUNE_TEST_PG_VERSION="$v" \
        cargo test -p hune-source --lib --locked pg_probe -- --ignored --test-threads=4 || status=1
    $DOCKER rm -f -v "$name" >/dev/null 2>&1 || true
done
exit $status
