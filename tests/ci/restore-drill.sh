#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Restore drill (16.14, 5.37 to 5.41): export, migrate-store, import and copy-store lose nothing.
# Use: tests/ci/restore-drill.sh [--versions "15 18"] [--binary PATH]
#   The script needs Docker, python3 and a Hune binary. It sends nothing to a registry.
#   Without --binary, it takes HUNE_BINARY, or it builds target/debug/hune with cargo.
#   Variables: DOCKER (the docker command, default `docker`), HUNE_PG_VERSIONS,
#   HUNE_BINARY, and IMAGE_POSTGRES_<version> (an image reference; the default is `postgres:<version>`).
#   tests/ci/store.sh exports IMAGE_POSTGRES_15 and IMAGE_POSTGRES_18 from tests/images.lock.
#
#   Step 1. The tool tests/ci/restore-drill-tool.py builds the seed volume through `hune serve`:
#           20 dashboards in 3 folders with saves, one move, one delete and one restore.
#           One dashboard has a version of kind `external` without bytes (the hook of 5.14).
#   For each PostgreSQL version, the script starts one container on 127.0.0.1 and does these steps:
#     2. `hune export` of the volume to out/a.
#     3. `hune migrate-store` as the owner role, the grants of 5.25, then `hune import` as the
#        application role.
#     4. `hune export` of PostgreSQL to out/b.
#     5. `diff -r out/a out/b` finds no difference.
#     6. `hune copy-store` from PostgreSQL to a new volume, and from the volume to a second
#        database. Each result equals out/a.
#     7. `hune restore` of the deleted dashboard on each store. Then `hune serve` on each store
#        gives the same list of dashboards.
#   Each export, import and copy keeps the version of kind `external` without bytes.
#   The script writes only under target/drill/. It removes its containers and its password files
#   at the end, also after an error. A container name starts with `hune-test-drill-`.
#   The exit code is 0 when each check passes, and 1 for any difference or error.
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd -P)
cd "$ROOT"
if [ -d "$HOME/.cargo/bin" ]; then
    PATH="$HOME/.cargo/bin:$PATH"
    export PATH
fi
DOCKER=${DOCKER:-docker}
VERSIONS=${HUNE_PG_VERSIONS:-"15 18"}
BINARY=${HUNE_BINARY:-}
while [ $# -gt 0 ]; do
    case $1 in
    --versions) VERSIONS=$2; shift 2 ;;
    --binary) BINARY=$2; shift 2 ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) echo "error: unknown option: $1" >&2; exit 1 ;;
    esac
done

TOOL="$ROOT/tests/ci/restore-drill-tool.py"
RUN=$$
OUT="$ROOT/target/drill"
LOG="$OUT/drill.log"
CONTAINERS=""

rm -rf "$OUT"
mkdir -p "$OUT"
chmod 700 "$OUT"
: >"$LOG"

cleanup() {
    for c in $CONTAINERS; do
        $DOCKER rm -f -v "$c" >/dev/null 2>&1 || true
    done
    # The passwords are disposable. Remove them with the other secrets of the run.
    rm -f "$OUT"/*.pw "$OUT"/*/*.pw "$OUT"/*/*/*.pw 2>/dev/null || true
}
trap cleanup EXIT INT TERM

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

pass() {
    echo "PASS: $*"
}

tool() {
    python3 -I "$TOOL" "$@"
}

# hune ARGS... : run the binary. The output goes to the log and to the variable LAST.
hune() {
    echo "+ hune $*" >>"$LOG"
    if LAST=$("$BINARY" "$@" 2>>"$LOG"); then
        printf '%s\n' "$LAST" >>"$LOG"
    else
        rc=$?
        printf '%s\n' "$LAST" >>"$LOG"
        tail -n 20 "$LOG" >&2
        fail "hune $1 exited with code $rc"
    fi
}

# random_secret : print 48 hex characters from the kernel generator.
random_secret() {
    head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n'
}

# same_tree LEFT RIGHT WHAT : the two trees have the same files with the same bytes.
same_tree() {
    if ! diff -r -x .hune "$1" "$2" >"$OUT/diff.txt" 2>&1; then
        head -n 20 "$OUT/diff.txt" >&2
        fail "$3: the trees differ"
    fi
    n=$(cd "$1" && find . -path ./.hune -prune -o -type f -print | wc -l)
    [ "$n" -gt 0 ] || fail "$3: the tree is empty"
    pass "$3: diff -r finds no difference ($n files)"
}

# counts_of TEXT : the three count lines of an export, an import or a copy, on one line.
counts_of() {
    printf '%s\n' "$1" | grep -E '^(folders|dashboards|versions): [0-9]+$' | tr '\n' ' '
}

# ---- Preparation ----

if [ -z "$BINARY" ]; then
    echo "== build target/debug/hune"
    cargo build -p hune --locked >>"$LOG" 2>&1 || { tail -n 30 "$LOG" >&2; fail "cargo build"; }
    BINARY="$ROOT/target/debug/hune"
fi
case $BINARY in /*) ;; *) BINARY="$ROOT/$BINARY" ;; esac
[ -x "$BINARY" ] || fail "the binary $BINARY is not executable"
command -v python3 >/dev/null || fail "python3 is absent"
$DOCKER version >/dev/null 2>&1 || fail "Docker does not answer"

# start_pg VERSION : start a container, wait for it, make the two roles of 5.25 and two databases.
# Sets CURRENT, PORT and the files OWNER_PW and APP_PW.
start_pg() {
    v=$1
    image=$(printenv "IMAGE_POSTGRES_$v" || true)
    [ -n "$image" ] || image="postgres:$v"
    name="hune-test-drill-$v-$RUN"
    CONTAINERS="$CONTAINERS $name"
    ADMIN_PW=$(random_secret)
    # The admin password is not used again: the setup runs on the local socket of the container.
    $DOCKER run -d --name "$name" -e "POSTGRES_PASSWORD=$ADMIN_PW" -p 127.0.0.1::5432 "$image" \
        postgres -c fsync=off -c full_page_writes=off >/dev/null
    ADMIN_PW=
    i=0
    until $DOCKER exec "$name" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; do
        i=$((i + 1))
        [ $i -lt 120 ] || { $DOCKER logs "$name" >&2; fail "$name does not start"; }
        sleep 1
    done
    # The first server of the image stops after its init: wait for the final one.
    sleep 2
    until $DOCKER exec "$name" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; do sleep 1; done
    (umask 077 && random_secret >"$OUT/owner-$v.pw" && random_secret >"$OUT/app-$v.pw")
    OWNER_PW="$OUT/owner-$v.pw"
    APP_PW="$OUT/app-$v.pw"
    # The SQL goes through the standard input, so no password is in a command line.
    {
        printf "CREATE ROLE hune_owner LOGIN PASSWORD '%s';\n" "$(cat "$OWNER_PW")"
        printf "CREATE ROLE hune_app LOGIN PASSWORD '%s';\n" "$(cat "$APP_PW")"
        echo "CREATE DATABASE hune_drill;"
        echo "CREATE DATABASE hune_drill2;"
    } | $DOCKER exec -i "$name" psql -U postgres -X -q -v ON_ERROR_STOP=1 >/dev/null
    PORT=$($DOCKER port "$name" 5432/tcp | head -n 1 | sed 's/.*://')
    CURRENT=$name
}

# grants DATABASE : the grants of 5.25 for the two roles, before the first migrate-store.
grants() {
    $DOCKER exec -i "$CURRENT" psql -U postgres -X -q -v ON_ERROR_STOP=1 -d "$1" >/dev/null <<EOF
GRANT CONNECT ON DATABASE $1 TO hune_owner;
GRANT CREATE, USAGE ON SCHEMA public TO hune_owner;
GRANT CONNECT ON DATABASE $1 TO hune_app;
GRANT USAGE ON SCHEMA public TO hune_app;
ALTER DEFAULT PRIVILEGES FOR ROLE hune_owner IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO hune_app;
EOF
}

# pg_config FILE DATABASE ROLE PASSWORD_FILE : write a configuration file for a PostgreSQL store.
pg_config() {
    tool config --backend postgres --out "$1" --host 127.0.0.1 --port "$PORT" --database "$2" \
        --user "$3" --password-file "$4"
}

# ---- Step 1: the seed volume (the same for each PostgreSQL version) ----

echo "== step 1: seed volume"
SEED="$OUT/seed"
mkdir -p "$SEED/volume"
tool config --backend volume --out "$SEED/volume.hcl" --path "$SEED/volume"
tool seed --binary "$BINARY" --config "$SEED/volume.hcl" --root "$SEED/volume" --summary "$SEED/summary.json" \
    >>"$LOG" 2>&1 || { tail -n 20 "$LOG" >&2; fail "the seed"; }
grep '^seed:' "$LOG" | tail -n 1
UID_DELETED=$(sed -n 's/.*"deleted_uid": "\(.*\)".*/\1/p' "$SEED/summary.json")
[ -n "$UID_DELETED" ] || fail "no deleted uid in the summary"
FOLDER_DELETED=$(ls "$SEED/volume/.history/$UID_DELETED" >/dev/null && sed -n 's/.*"folder": "\(.*\)".*/\1/p' \
    "$SEED/volume/.history/$UID_DELETED/1.meta.json")
pass "step 1: seed volume built"

# ---- Steps 2 to 7, for each PostgreSQL version ----

status=0
for v in $VERSIONS; do
    echo "== PostgreSQL $v"
    D="$OUT/pg$v"
    rm -rf "$D"
    mkdir -p "$D"
    cp -a "$SEED/volume" "$D/volume"
    # The configuration of the copy: same files, its own path and ports.
    tool config --backend volume --out "$D/volume.hcl" --path "$D/volume"
    start_pg "$v"
    grants hune_drill
    grants hune_drill2
    pg_config "$D/migrate.hcl" hune_drill hune_owner "$OWNER_PW"
    pg_config "$D/postgres.hcl" hune_drill hune_app "$APP_PW"
    pg_config "$D/migrate2.hcl" hune_drill2 hune_owner "$OWNER_PW"
    pg_config "$D/postgres2.hcl" hune_drill2 hune_app "$APP_PW"
    mkdir -p "$D/volume2"
    tool config --backend volume --out "$D/volume2.hcl" --path "$D/volume2"

    # Step 2.
    hune export -c "$D/volume.hcl" --to "$D/a"
    counts_a=$(counts_of "$LAST")
    tool check --what external --dir "$D/a" --summary "$SEED/summary.json" >>"$LOG" 2>&1 \
        || { tail -n 5 "$LOG" >&2; fail "PostgreSQL $v, step 2: the export of the volume lost the external version"; }
    pass "PostgreSQL $v, step 2: export of the volume ($counts_a)"

    # Step 3.
    hune migrate-store -c "$D/migrate.hcl"
    hune import -c "$D/postgres.hcl" --from "$D/a"
    [ "$(counts_of "$LAST")" = "$counts_a" ] || fail "PostgreSQL $v, step 3: the import counts differ from the export counts"
    pass "PostgreSQL $v, step 3: migrate-store and import ($(counts_of "$LAST"))"

    # Step 4 and step 5.
    hune export -c "$D/postgres.hcl" --to "$D/b"
    [ "$(counts_of "$LAST")" = "$counts_a" ] || fail "PostgreSQL $v, step 4: the export counts differ"
    same_tree "$D/a" "$D/b" "PostgreSQL $v, step 5: export of the volume and export of PostgreSQL"
    tool check --what external --dir "$D/b" --summary "$SEED/summary.json" >>"$LOG" 2>&1 \
        || { tail -n 5 "$LOG" >&2; fail "PostgreSQL $v, step 5: the import lost the external version"; }
    pass "PostgreSQL $v, step 5: the import keeps the external version without bytes"

    # Step 6: PostgreSQL to a new volume, and the volume to a second database.
    hune copy-store --from "$D/postgres.hcl" --to "$D/volume2.hcl"
    [ "$(counts_of "$LAST")" = "$counts_a" ] || fail "PostgreSQL $v, step 6: the copy counts differ"
    same_tree "$D/a" "$D/volume2" "PostgreSQL $v, step 6: copy-store from PostgreSQL to a volume"
    tool check --what external --dir "$D/volume2" --summary "$SEED/summary.json" >>"$LOG" 2>&1 \
        || { tail -n 5 "$LOG" >&2; fail "PostgreSQL $v, step 6: copy-store lost the external version"; }
    pass "PostgreSQL $v, step 6: copy-store keeps the external version without bytes"
    hune migrate-store -c "$D/migrate2.hcl"
    hune copy-store --from "$D/volume.hcl" --to "$D/postgres2.hcl"
    [ "$(counts_of "$LAST")" = "$counts_a" ] || fail "PostgreSQL $v, step 6: the second copy counts differ"
    hune export -c "$D/postgres2.hcl" --to "$D/b2"
    same_tree "$D/a" "$D/b2" "PostgreSQL $v, step 6: copy-store from the volume to PostgreSQL"

    # Step 7: restore the deleted dashboard on each store.
    [ ! -e "$D/volume/$FOLDER_DELETED/$UID_DELETED.json" ] || fail "$UID_DELETED is not deleted in the volume"
    hune restore -c "$D/volume.hcl" --uid "$UID_DELETED"
    restored_volume=$LAST
    hune restore -c "$D/postgres.hcl" --uid "$UID_DELETED"
    restored_pg=$LAST
    # Each answer holds the new version number and the etag. The etag must be equal on the two stores.
    etag_volume=$(printf '%s\n' "$restored_volume" | grep -Eo '[0-9a-f]{64}' | head -n 1)
    etag_pg=$(printf '%s\n' "$restored_pg" | grep -Eo '[0-9a-f]{64}' | head -n 1)
    [ -n "$etag_volume" ] || fail "PostgreSQL $v, step 7: no etag in the answer of restore: $restored_volume"
    [ "$etag_volume" = "$etag_pg" ] || fail "PostgreSQL $v, step 7: the etag differs ($etag_volume and $etag_pg)"
    hune export -c "$D/postgres.hcl" --to "$D/c"
    cmp "$D/volume/$FOLDER_DELETED/$UID_DELETED.json" "$D/c/$FOLDER_DELETED/$UID_DELETED.json" \
        || fail "PostgreSQL $v, step 7: the restored document differs"
    for tree in "$D/volume" "$D/c"; do
        [ "$(tool check --what head-kind --dir "$tree" --uid "$UID_DELETED")" = restore ] \
            || fail "PostgreSQL $v, step 7: the head version of $UID_DELETED is not a restore in $tree"
    done
    pass "PostgreSQL $v, step 7: restore of $UID_DELETED gives the same etag on both stores"
    tool list --binary "$BINARY" --config "$D/volume.hcl" >"$D/list-volume.txt" 2>>"$LOG" \
        || { tail -n 10 "$LOG" >&2; fail "PostgreSQL $v, step 7: serve on the volume"; }
    tool list --binary "$BINARY" --config "$D/postgres.hcl" >"$D/list-postgres.txt" 2>>"$LOG" \
        || { tail -n 10 "$LOG" >&2; fail "PostgreSQL $v, step 7: serve on PostgreSQL"; }
    cmp "$D/list-volume.txt" "$D/list-postgres.txt" \
        || { diff "$D/list-volume.txt" "$D/list-postgres.txt" | head -n 10 >&2; fail "PostgreSQL $v, step 7: the lists of dashboards differ"; }
    n=$(wc -l <"$D/list-volume.txt")
    [ "$n" -eq 21 ] || fail "PostgreSQL $v, step 7: the list has $n dashboards, expected 21 after the restore"
    pass "PostgreSQL $v, step 7: GET /api/dashboards gives the same list on both stores ($n dashboards)"

    $DOCKER rm -f -v "$CURRENT" >/dev/null 2>&1 || true
    echo "RESULT: PostgreSQL $v: restore drill passed"
done
exit $status
