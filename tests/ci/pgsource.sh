#!/bin/sh
# Job `pgsource` (16.9a, 16.17): the container tests of the PostgreSQL source on PostgreSQL 15 and 18.
# Use: tests/ci/pgsource.sh [--versions "15 18"] [--keep-going]
#   For each version, the script does these steps.
#   1. It makes the passwords (random, disposable) and the certificates under target/pgsource-<run>/.
#   2. It starts one disposable container from tests/compose/pgsource.yaml, on 127.0.0.1 only. The first
#      start runs tests/fixtures/postgres-source/init.sql.
#   3. It runs the self check of the fixture (tests/fixtures/postgres-source/selfcheck.sh): the roles
#      and the data. This check needs no code of Hune.
#   4. It runs the container tests of the PostgreSQL source with `cargo test -- --include-ignored`.
#      The connection comes to the tests in the environment variables HUNE_TEST_PGS_* (listed below).
#      Nothing else reads them: the program has no such variable.
#   5. It removes the container with `docker rm -f -v` (also in a trap, on exit or on a signal).
#
#   Where the tests are. Target `pgsource*` of the crate hune-source (crates/hune-source/tests/pgsource*.rs),
#   target `pgsource*` of the crate hune (crates/hune/tests/pgsource*.rs), and the ignored tests of the
#   library hune-source whose own name starts with `ps` (the filter is `::ps`). A test name starts with its
#   case id, for example
#   `ps04_rows_limit` is the case PS04 (16.17). The script reports each case PS01 to PS15 as PASS,
#   FAIL or NO TEST (no test of that name exists yet: the case is skipped).
#   The ignored tests `pg_real` of the binary hune (A47, A50 and A51 of 16.8, through the server) run with
#   `cargo test -p hune --locked --bin hune -- --ignored pg_real --test-threads=1`. The script reports them
#   in a second table, A47, A50 and A51, in the same way.
#   Exit code: 0 only when the fixture check passes, each existing case passes, and the merge gate
#   of tests/ci/gate-cases.txt is met. A skipped case fails the job only when the gate requires it
#   (milestone MPG).
#
#   Variables for the tests (set for each version):
#     HUNE_TEST_PGS_VERSION (15 or 18), _HOST (127.0.0.1), _PORT, _DATABASE (analytics), _SCHEMA (metrics),
#     _ADMIN_USER (postgres), _ADMIN_PASSWORD, _RO_USER (hune_ro), _RO_PASSWORD, _RW_USER (hune_rw),
#     _RW_PASSWORD, _BAD_ROLES (a list "role:check ..." of the deviant roles), _BAD_PASSWORD (the password of
#     each deviant role), _MD5_USER (hune_md5), _CLEAR_USER (hune_clear), _LEGACY_PASSWORD, _CA_FILE,
#     _WRONG_CA_FILE, _SERVER_NAME (localhost), _DOCKER (the docker command), _CONTAINER (the container name,
#     for the stop and start of PS13), _COMPOSE_PROJECT, _SERVICE.
#   Variables of the script: DOCKER (default `docker`), HUNE_PG_VERSIONS (default "15 18"),
#     HUNE_PGS_NO_PULL=1 (never pull an image; when the image of tests/images.lock is not in the local cache,
#     use the local tag `postgres:<version>` and print a warning. For a run with no network.)
#   The script reads no registry unless an image is missing from the cache. It never pushes (16.18).
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init

DOCKER=${DOCKER:-docker}
VERSIONS=${HUNE_PG_VERSIONS:-"15 18"}
while [ $# -gt 0 ]; do
    case $1 in
    --versions) VERSIONS=${2:?--versions needs a value}; shift 2 ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) ci_die "unknown option: $1" ;;
    esac
done
for v in $VERSIONS; do
    case $v in 15 | 18) ;; *) ci_die "unsupported version '$v' (use 15 or 18)" ;; esac
done

RUN=$$
BASE="$ROOT/target/pgsource-$RUN"
LOG="$ROOT/target/ci-pgsource.log"
COMPOSE_FILE="$ROOT/tests/compose/pgsource.yaml"
PROJECT="hune-pgsource-$RUN"
mkdir -p "$ROOT/target"
umask 077
mkdir -p "$BASE"
: >"$LOG"

cleanup() {
    "$DOCKER" compose -p "$PROJECT" -f "$COMPOSE_FILE" down -v --remove-orphans >/dev/null 2>&1 || true
    # The same by name, in case compose did not know the container.
    for c in $("$DOCKER" ps -aq --filter "label=com.docker.compose.project=$PROJECT" 2>/dev/null || true); do
        "$DOCKER" rm -f -v "$c" >/dev/null 2>&1 || true
    done
    rm -rf "$BASE"
}
trap cleanup EXIT INT TERM

# A free port on 127.0.0.1. The container keeps it across a stop and a start (PS13).
free_port() {
    python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'
}

secret() {
    openssl rand -hex 16
}

# One CA and one server certificate for `localhost` and 127.0.0.1, and a second CA that signed nothing.
make_certs() {
    tls=$1
    mkdir -p "$tls"
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$tls/ca.key" -out "$tls/ca.pem" -days 2 \
        -subj "/CN=hune-test-ca" >/dev/null 2>&1
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$tls/wrong-ca.key" -out "$tls/wrong-ca.pem" -days 2 \
        -subj "/CN=hune-test-wrong-ca" >/dev/null 2>&1
    openssl req -newkey rsa:2048 -nodes -keyout "$tls/server.key" -out "$tls/server.csr" \
        -subj "/CN=localhost" >/dev/null 2>&1
    printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' \
        >"$tls/server.ext"
    openssl x509 -req -in "$tls/server.csr" -CA "$tls/ca.pem" -CAkey "$tls/ca.key" -CAcreateserial \
        -out "$tls/server.crt" -days 2 -extfile "$tls/server.ext" >/dev/null 2>&1
    # The container reads the files as another user. They hold test keys only.
    chmod 755 "$tls"
    chmod 644 "$tls"/*
}

# image_for VERSION : print the image to run. The line postgres-<version> of tests/images.lock is the rule.
image_for() {
    ref=$(ci_lock_ref "postgres-$1" 2>/dev/null)
    if "$DOCKER" image inspect "$ref" >/dev/null 2>&1; then
        printf '%s\n' "$ref"
        return 0
    fi
    if [ "${HUNE_PGS_NO_PULL:-0}" != 1 ]; then
        printf '%s\n' "$ref"
        return 0
    fi
    if "$DOCKER" image inspect "postgres:$1" >/dev/null 2>&1; then
        ci_warn "image $ref is not in the local cache and HUNE_PGS_NO_PULL=1: using postgres:$1 (not the pinned tag)"
        printf 'postgres:%s\n' "$1"
        return 0
    fi
    ci_die "no image for PostgreSQL $1: $ref is not in the local cache and postgres:$1 is absent"
}

# integration_targets : print the crate and the target of each test file that is named pgsource*.
integration_targets() {
    # crate:target pairs.
    for f in "$ROOT"/crates/hune-source/tests/pgsource*.rs; do
        [ -e "$f" ] && printf 'hune-source:%s\n' "$(basename "$f" .rs)"
    done
    for f in "$ROOT"/crates/hune/tests/pgsource*.rs; do
        [ -e "$f" ] && printf 'hune:%s\n' "$(basename "$f" .rs)"
    done
    return 0
}

# run_tests LOGFILE : run every container test that exists. Returns 0 when each run ended well.
run_tests() {
    rt_log=$1
    rt_rc=0
    : >"$rt_log"
    for rt_pair in $(integration_targets); do
        rt_crate=${rt_pair%%:*}
        rt_target=${rt_pair#*:}
        echo "-- cargo test -p $rt_crate --test $rt_target" >>"$rt_log"
        cargo test -p "$rt_crate" --locked --test "$rt_target" -- --include-ignored --test-threads=4 \
            >>"$rt_log" 2>&1 || rt_rc=1
    done
    # The filter `::ps` selects the tests whose own name starts with `ps`. The other ignored tests of the module
    # (for example the tests of the probe, which have their own script) need another environment.
    echo "-- cargo test -p hune-source --lib (ignored tests named ps*)" >>"$rt_log"
    cargo test -p hune-source --locked --lib -- --ignored ::ps --test-threads=4 >>"$rt_log" 2>&1 || rt_rc=1
    # The cases A47, A50 and A51 through the server. The tests share one harness: one thread.
    echo "-- cargo test -p hune --bin hune (ignored tests pg_real: A47, A50, A51)" >>"$rt_log"
    cargo test -p hune --locked --bin hune -- --ignored pg_real --test-threads=1 >>"$rt_log" 2>&1 || rt_rc=1
    return "$rt_rc"
}

# case_report LOGFILE : print the state of PS01 to PS15 from a cargo log. Print "failed N" last.
case_report() {
    cr_log=$1
    cr_ok=$(ci_cases_in_log "$cr_log" ok | grep '^PS' || true)
    cr_fail=$(ci_cases_in_log "$cr_log" FAILED | grep '^PS' || true)
    cr_pass_n=0
    cr_fail_n=0
    cr_none_n=0
    cr_i=1
    while [ "$cr_i" -le 15 ]; do
        cr_id=$(printf 'PS%02d' "$cr_i")
        if printf '%s\n' "$cr_fail" | grep -qx "$cr_id"; then
            echo "  $cr_id  FAIL"
            cr_fail_n=$((cr_fail_n + 1))
        elif printf '%s\n' "$cr_ok" | grep -qx "$cr_id"; then
            echo "  $cr_id  PASS"
            cr_pass_n=$((cr_pass_n + 1))
        else
            echo "  $cr_id  NO TEST (skipped: no test with this case id exists yet)"
            cr_none_n=$((cr_none_n + 1))
        fi
        cr_i=$((cr_i + 1))
    done
    echo "  cases: $cr_pass_n passed, $cr_fail_n failed, $cr_none_n skipped (no test)"
    # The cases of the API table (16.8) that this job runs against a real PostgreSQL.
    cr_aok=$(ci_cases_in_log "$cr_log" ok | grep '^A' || true)
    cr_afail=$(ci_cases_in_log "$cr_log" FAILED | grep '^A' || true)
    cr_apass_n=0
    cr_afail_n=0
    for cr_id in A47 A50 A51; do
        if printf '%s\n' "$cr_afail" | grep -qx "$cr_id"; then
            echo "  $cr_id  FAIL"
            cr_afail_n=$((cr_afail_n + 1))
        elif printf '%s\n' "$cr_aok" | grep -qx "$cr_id"; then
            echo "  $cr_id  PASS"
            cr_apass_n=$((cr_apass_n + 1))
        else
            echo "  $cr_id  NO TEST (not run: the test pg_real of this id did not run)"
            cr_afail_n=$((cr_afail_n + 1))
        fi
    done
    echo "  cases A47, A50, A51: $cr_apass_n passed, $cr_afail_n failed or missing"
    CASE_FAILED=$((cr_fail_n + cr_afail_n))
    CASE_PASSED=$cr_pass_n
}

status=0
tls="$BASE/tls"
make_certs "$tls"
PGS_ADMIN_PASSWORD=$(secret)
PGS_RO_PASSWORD=$(secret)
PGS_RW_PASSWORD=$(secret)
PGS_BAD_PASSWORD=$(secret)
PGS_LEGACY_PASSWORD=$(secret)
PGS_PORT_15=$(free_port)
PGS_PORT_18=$(free_port)
[ "$PGS_PORT_15" != "$PGS_PORT_18" ] || PGS_PORT_18=$(free_port)
PGS_TLS_DIR=$tls
IMAGE_POSTGRES_15=$(image_for 15)
IMAGE_POSTGRES_18=$(image_for 18)
export PGS_ADMIN_PASSWORD PGS_RO_PASSWORD PGS_RW_PASSWORD PGS_BAD_PASSWORD PGS_LEGACY_PASSWORD
export PGS_PORT_15 PGS_PORT_18 PGS_TLS_DIR IMAGE_POSTGRES_15 IMAGE_POSTGRES_18
[ -x "$ROOT/tests/fixtures/postgres-source/apply.sh" ] || ci_die "tests/fixtures/postgres-source/apply.sh is not executable"

for v in $VERSIONS; do
    echo "== PostgreSQL $v"
    service="pg$v"
    eval "port=\$PGS_PORT_$v"
    if ! "$DOCKER" compose -p "$PROJECT" -f "$COMPOSE_FILE" up -d --wait --wait-timeout 240 "$service" \
        >"$BASE/up-$v.log" 2>&1; then
        echo "error: the service $service did not become healthy" >&2
        tail -n 40 "$BASE/up-$v.log" >&2
        "$DOCKER" compose -p "$PROJECT" -f "$COMPOSE_FILE" logs --tail 60 "$service" >&2 || true
        status=1
        "$DOCKER" compose -p "$PROJECT" -f "$COMPOSE_FILE" down -v --remove-orphans >/dev/null 2>&1 || true
        continue
    fi
    container=$("$DOCKER" compose -p "$PROJECT" -f "$COMPOSE_FILE" ps -q "$service")
    cname=$("$DOCKER" inspect --format '{{.Name}}' "$container" | sed 's|^/||')

    # The self check of the fixture.
    PGS_ADMIN_PASSWORD=$PGS_ADMIN_PASSWORD DOCKER=$DOCKER \
        "$ROOT/tests/fixtures/postgres-source/selfcheck.sh" "$cname" || status=1

    # The container tests.
    export HUNE_TEST_PGS_VERSION="$v"
    export HUNE_TEST_PGS_HOST=127.0.0.1
    export HUNE_TEST_PGS_PORT="$port"
    export HUNE_TEST_PGS_DATABASE=analytics
    export HUNE_TEST_PGS_SCHEMA=metrics
    export HUNE_TEST_PGS_ADMIN_USER=postgres
    export HUNE_TEST_PGS_ADMIN_PASSWORD="$PGS_ADMIN_PASSWORD"
    export HUNE_TEST_PGS_RO_USER=hune_ro
    export HUNE_TEST_PGS_RO_PASSWORD="$PGS_RO_PASSWORD"
    export HUNE_TEST_PGS_RW_USER=hune_rw
    export HUNE_TEST_PGS_RW_PASSWORD="$PGS_RW_PASSWORD"
    export HUNE_TEST_PGS_BAD_ROLES="hune_bad_createrole:R1 hune_bad_pgread:R2 hune_bad_create:R3 hune_bad_write:R4 hune_bad_seq:R5 hune_bad_col:R6 hune_bad_dblink:R7 hune_bad_fdw:R8 hune_bad_signal:R7 hune_bad_emit:R7 hune_bad_ts:R7 hune_bad_noinh:R4 hune_bad_owner:R9"
    export HUNE_TEST_PGS_BAD_PASSWORD="$PGS_BAD_PASSWORD"
    export HUNE_TEST_PGS_MD5_USER=hune_md5
    export HUNE_TEST_PGS_CLEAR_USER=hune_clear
    export HUNE_TEST_PGS_LEGACY_PASSWORD="$PGS_LEGACY_PASSWORD"
    export HUNE_TEST_PGS_CA_FILE="$tls/ca.pem"
    export HUNE_TEST_PGS_WRONG_CA_FILE="$tls/wrong-ca.pem"
    export HUNE_TEST_PGS_SERVER_NAME=localhost
    export HUNE_TEST_PGS_DOCKER="$DOCKER"
    export HUNE_TEST_PGS_CONTAINER="$cname"
    export HUNE_TEST_PGS_COMPOSE_PROJECT="$PROJECT"
    export HUNE_TEST_PGS_SERVICE="$service"

    run_rc=0
    run_tests "$BASE/run-$v.log" || run_rc=1
    cat "$BASE/run-$v.log" >>"$LOG"
    echo "-- cases of PostgreSQL $v"
    CASE_FAILED=0
    case_report "$BASE/run-$v.log"
    [ "$CASE_FAILED" -eq 0 ] || status=1
    # The merge gate: a required case that failed or did not run fails the job.
    ci_gate_evaluate pgsource "$BASE/run-$v.log" "$run_rc" || status=1
    if [ "$run_rc" -ne 0 ] && [ "$CASE_FAILED" -eq 0 ]; then
        echo "error: a test run failed with no failed case (build error or a test that is not a case)" >&2
        tail -n 30 "$BASE/run-$v.log" >&2
        status=1
    fi

    "$DOCKER" compose -p "$PROJECT" -f "$COMPOSE_FILE" rm -s -f -v "$service" >/dev/null 2>&1 || true
done

if [ "$status" -eq 0 ]; then
    echo "pgsource: ok"
else
    echo "pgsource: FAILED"
fi
exit "$status"
