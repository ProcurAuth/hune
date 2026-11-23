#!/bin/sh
# Job `mongosource` (16.9b, 16.17): the container tests of the MongoDB source on MongoDB 7.0 and 8.0.
# Use: tests/ci/mongosource.sh [--versions "7.0 8.0"]
#   For each version, the script does these steps.
#   1. It makes the passwords (random, disposable), the certificates and the key file under target/mongosource-<run>/.
#   2. It starts five disposable containers from tests/compose/mongosource.yaml, on 127.0.0.1 only: the main
#      server (authorization on, TLS only, --noscripting), a server with scripting on (MS18), a replica set of
#      two members in one container (MS10, MS12; the decision ID-MG18 is open, so the set has two members, as the
#      key `hosts` needs), a server that offers SCRAM-SHA-1 only and one that offers PLAIN only (MS10). The first start
#      runs tests/fixtures/mongo-source/init.js. The ports come from the range 27101 to 27120 of 127.0.0.1.
#   3. It runs the self check of the fixture (tests/fixtures/mongo-source/selfcheck.sh) on each server: the data,
#      the role contract of 8b.13 (MS08, role side: an insert as hune_ro fails, connectionStatus shows read
#      actions only) and the users that break M1 to M3. This check needs no code of Hune.
#   4. It runs the tests of the MongoDB source. The server tests run with `cargo test -- --include-ignored`.
#      The connection comes to the tests in the variables HUNE_TEST_MGS_* (listed below). Nothing else reads
#      them: the program has no such variable.
#   5. It removes the containers with `docker rm -f -v` (also in a trap, on exit or on a signal).
#
#   Where the tests are. Target `mongosource*` of the crate hune-source (crates/hune-source/tests/mongosource*.rs),
#   target `mongosource*` of the crate hune (crates/hune/tests/mongosource*.rs), the tests of the library
#   hune-source whose own name starts with `ms` (the filter is `::ms`), and the ignored tests of the binary crate
#   hune whose name starts with `ms` (crates/hune/src/server/tests/mongo_real.rs: MS13, MS16, MS17, with the
#   real driver in the server of the tests). A test name starts with its case id,
#   for example `ms04_row_limit_is_exact` is the case MS04 (16.9b). The library tests run twice: the ignored
#   tests need a server, the others need none (the fake client of 8b.16).
#   The script reports each case MS01 to MS18 as PASS, FAIL or SKIP, and ends with a summary of each version:
#     PASS                 a test with the id ran against a server and passed.
#     PASS (no server)     only the tests with no server ran and passed (MS02, MS03, MS06, MS07, MS11 have a
#                          part of this kind). The part that needs a server is not covered.
#     SKIP                 no test of that name exists: the case is skipped. A build with no `mongo-driver`
#                          feature has no client for a server, so these cases stay skipped (8b.2).
#   Option --dev DIR: start the servers of the first version in the list, write DIR/env.sh (the variables for the
#   tests, to source with `.`), run no test and leave the servers up. Option --down DIR removes them. A developer
#   uses these two to run one test at a time. The folder DIR holds the passwords: keep it under target/.
#   The crate hune-source gets `--features mongo-driver` as soon as its Cargo.toml declares the feature.
#   Exit code: 0 only when each fixture check passes, each existing case passes, and the merge gate of
#   tests/ci/gate-cases.txt is met. A skipped case fails the job only when the gate requires it (milestone MMG).
#
#   Variables for the tests (set for each version): HUNE_TEST_MGS_VERSION (7.0 or 8.0), _HOST (localhost, the
#     name of the certificate), _IP (127.0.0.1), _PORT, _JS_PORT (scripting on, MS18), _RS_PORT, _RS_NAME (rs0),
#     _RS_PORT2 (the second member), _RS_HOSTS (localhost:<port>,localhost:<port2>), _SHA1_PORT (offers SCRAM-SHA-1 only), _PLAIN_PORT (offers PLAIN only), _DATABASE (analytics), _OTHER_DATABASE
#     (other), _AUTH_DATABASE (admin), _ADMIN_USER (root), _ADMIN_PASSWORD, _RO_USER (hune_ro), _RO_PASSWORD,
#     _RW_USER (hune_rw), _RW_PASSWORD, _BAD_USERS (a list "user:check:authdb ..." of the deviant users), _BAD_PASSWORD
#     (the password of each deviant user), _SHA1_USER (hune_sha1), _PLAIN_USER (hune_plain), _LEGACY_PASSWORD,
#     _CA_FILE, _WRONG_CA_FILE, _NODE_LOAD_DOCS (1000000), _BIG_BYTES (10485760), _DOCKER (the docker command),
#     _CONTAINER (the name of the main container, for the stop and start of MS13 and the admin session of MS14),
#     _COMPOSE_PROJECT, _SERVICE (the service name of the main server).
#     The certificate has the name `localhost` only: a connection to _IP fails the name check (MS10).
#   Variables of the script: DOCKER (default `docker`), HUNE_MG_VERSIONS (default "7.0 8.0"),
#     HUNE_MGS_NO_PULL=1 (never pull an image; when the image of tests/images.lock is not in the local cache,
#     use the local tag `mongo:<version>` and print a warning. For a run with no network.)
#   The script reads no registry unless an image is missing from the cache. It never pushes (16.18).
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init

DOCKER=${DOCKER:-docker}
VERSIONS=${HUNE_MG_VERSIONS:-"7.0 8.0"}
DEV_DIR=""
DOWN_DIR=""
while [ $# -gt 0 ]; do
    case $1 in
    --versions) VERSIONS=${2:?--versions needs a value}; shift 2 ;;
    --dev) DEV_DIR=${2:?--dev needs a folder}; shift 2 ;;
    --down) DOWN_DIR=${2:?--down needs a folder}; shift 2 ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) ci_die "unknown option: $1" ;;
    esac
done
for v in $VERSIONS; do
    case $v in 7.0 | 8.0) ;; *) ci_die "unsupported version '$v' (use 7.0 or 8.0)" ;; esac
done

RUN=$$
COMPOSE_FILE="$ROOT/tests/compose/mongosource.yaml"
FIXTURE="$ROOT/tests/fixtures/mongo-source"

# remove_project PROJECT : remove the containers of a compose project, with their volumes.
remove_project() {
    "$DOCKER" compose -p "$1" -f "$COMPOSE_FILE" down -v --remove-orphans >/dev/null 2>&1 || true
    # The same by label, in case compose did not know a container.
    for c in $("$DOCKER" ps -aq --filter "label=com.docker.compose.project=$1" 2>/dev/null || true); do
        "$DOCKER" rm -f -v "$c" >/dev/null 2>&1 || true
    done
}

if [ -n "$DOWN_DIR" ]; then
    # The ports and the passwords of the project are in the file that --dev wrote.
    [ -r "$DOWN_DIR/project" ] || ci_die "$DOWN_DIR/project is not readable"
    remove_project "$(cat "$DOWN_DIR/project")"
    rm -rf "$DOWN_DIR"
    echo "mongosource: servers of $DOWN_DIR removed"
    exit 0
fi

if [ -n "$DEV_DIR" ]; then
    mkdir -p "$DEV_DIR"
    BASE=$(cd "$DEV_DIR" && pwd)
    PROJECT="hune-mongodev-$RUN"
    printf '%s\n' "$PROJECT" >"$BASE/project.tmp"
    mv "$BASE/project.tmp" "$BASE/project"
else
    BASE="$ROOT/target/mongosource-$RUN"
    PROJECT="hune-mongosource-$RUN"
fi
LOG="$ROOT/target/ci-mongosource.log"
mkdir -p "$ROOT/target"
umask 077
mkdir -p "$BASE"
: >"$LOG"

cleanup() {
    remove_project "$PROJECT"
    if [ -z "$DEV_DIR" ]; then
        rm -rf "$BASE"
    fi
}
trap cleanup EXIT INT TERM

# A free port of the range 27101 to 27120 on 127.0.0.1. Each call sets FP to a port that no earlier call gave.
# The function does not run in a subshell: the list of the used ports must stay.
USED_PORTS=" "
free_port() {
    fp=27101
    while [ "$fp" -le 27120 ]; do
        case $USED_PORTS in *" $fp "*) fp=$((fp + 1)); continue ;; esac
        if python3 -I -c 'import socket, sys; s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); s.bind(("127.0.0.1", int(sys.argv[1])))' "$fp" 2>/dev/null; then
            USED_PORTS="$USED_PORTS$fp "
            FP=$fp
            return 0
        fi
        fp=$((fp + 1))
    done
    ci_die "no free port in the range 27101 to 27120"
}

secret() {
    openssl rand -hex 16
}

# One CA, one server certificate for the name `localhost` only (no IP address, so the name check fails for
# 127.0.0.1), a second CA that signed nothing, and the key file of the replica set.
make_certs() {
    tls=$1
    mkdir -p "$tls"
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$tls/ca.key" -out "$tls/ca.pem" -days 2 \
        -subj "/CN=hune-test-ca" >/dev/null 2>&1
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$tls/wrong-ca.key" -out "$tls/wrong-ca.pem" -days 2 \
        -subj "/CN=hune-test-wrong-ca" >/dev/null 2>&1
    openssl req -newkey rsa:2048 -nodes -keyout "$tls/server.key" -out "$tls/server.csr" \
        -subj "/CN=localhost" >/dev/null 2>&1
    printf 'subjectAltName=DNS:localhost\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth,clientAuth\n' \
        >"$tls/server.ext"
    openssl x509 -req -in "$tls/server.csr" -CA "$tls/ca.pem" -CAkey "$tls/ca.key" -CAcreateserial \
        -out "$tls/server.crt" -days 2 -extfile "$tls/server.ext" >/dev/null 2>&1
    cat "$tls/server.crt" "$tls/server.key" >"$tls/server.pem"
    openssl rand -base64 756 | tr -d '\n' >"$tls/keyfile"
    # The container reads the files as another user. They hold test keys only.
    chmod 755 "$tls"
    chmod 644 "$tls"/*
}

# image_for VERSION : print the image to run. The line mongo-<version> of tests/images.lock is the rule.
image_for() {
    ref=$(ci_lock_ref "mongo-$1" 2>/dev/null)
    if "$DOCKER" image inspect "$ref" >/dev/null 2>&1; then
        printf '%s\n' "$ref"
        return 0
    fi
    if [ "${HUNE_MGS_NO_PULL:-0}" != 1 ]; then
        printf '%s\n' "$ref"
        return 0
    fi
    if "$DOCKER" image inspect "mongo:$1" >/dev/null 2>&1; then
        ci_warn "image $ref is not in the local cache and HUNE_MGS_NO_PULL=1: using mongo:$1 (not the pinned tag)"
        printf 'mongo:%s\n' "$1"
        return 0
    fi
    ci_die "no image for MongoDB $1: $ref is not in the local cache and mongo:$1 is absent"
}

# The feature of the client. The crate has it as soon as its Cargo.toml declares it.
FEATURES=""
if grep -qE '^mongo-driver[[:space:]]*=' "$ROOT/crates/hune-source/Cargo.toml" 2>/dev/null; then
    FEATURES="--features mongo-driver"
fi

# integration_targets : print the crate and the target of each test file that is named mongosource*.
integration_targets() {
    for f in "$ROOT"/crates/hune-source/tests/mongosource*.rs; do
        [ -e "$f" ] && printf 'hune-source:%s\n' "$(basename "$f" .rs)"
    done
    for f in "$ROOT"/crates/hune/tests/mongosource*.rs; do
        [ -e "$f" ] && printf 'hune:%s\n' "$(basename "$f" .rs)"
    done
    return 0
}

# run_tests SERVERLOG NOSERVERLOG : run every test that exists. The tests that need a server write to the
# first log, the library tests with no server write to the second one. Returns 0 when each run ended well.
run_tests() {
    rt_log=$1
    rt_nolog=$2
    rt_rc=0
    : >"$rt_log"
    : >"$rt_nolog"
    for rt_pair in $(integration_targets); do
        rt_crate=${rt_pair%%:*}
        rt_target=${rt_pair#*:}
        echo "-- cargo test -p $rt_crate --test $rt_target" >>"$rt_log"
        feat=""
        [ "$rt_crate" != hune-source ] || feat=$FEATURES
        # shellcheck disable=SC2086
        cargo test -p "$rt_crate" --locked $feat --test "$rt_target" -- --include-ignored --test-threads=4 \
            >>"$rt_log" 2>&1 || rt_rc=1
    done
    # The filter `::ms` selects the tests whose own name starts with `ms`. The ignored ones need a server.
    echo "-- cargo test -p hune-source --lib (ignored tests named ms*)" >>"$rt_log"
    # shellcheck disable=SC2086
    cargo test -p hune-source --locked $FEATURES --lib -- --ignored ::ms --test-threads=4 >>"$rt_log" 2>&1 || rt_rc=1
    # The tests of the server with the real driver (MS13, MS16, MS17) are in the binary crate `hune`.
    echo "-- cargo test -p hune --bin hune (ignored tests named ms*)" >>"$rt_log"
    cargo test -p hune --locked --bin hune -- --ignored ::ms --test-threads=4 >>"$rt_log" 2>&1 || rt_rc=1
    echo "-- cargo test -p hune-source --lib (tests named ms* with no server)" >>"$rt_nolog"
    # shellcheck disable=SC2086
    cargo test -p hune-source --locked $FEATURES --lib -- ::ms --test-threads=4 >>"$rt_nolog" 2>&1 || rt_rc=1
    return "$rt_rc"
}

# case_report SERVERLOG NOSERVERLOG : print the state of MS01 to MS18. Sets CASE_FAILED and CASE_PASSED.
case_report() {
    cr_log=$1
    cr_nolog=$2
    cr_ok=$(ci_cases_in_log "$cr_log" ok | grep '^MS' || true)
    cr_fail=$( (ci_cases_in_log "$cr_log" FAILED; ci_cases_in_log "$cr_nolog" FAILED) | grep '^MS' || true)
    cr_nook=$(ci_cases_in_log "$cr_nolog" ok | grep '^MS' || true)
    cr_pass_n=0
    cr_fail_n=0
    cr_none_n=0
    cr_part_n=0
    if [ -n "$FEATURES" ]; then
        cr_why="no test with this case id exists"
    else
        cr_why="no test with this case id exists; this build has no mongo-driver feature"
    fi
    cr_i=1
    while [ "$cr_i" -le 18 ]; do
        cr_id=$(printf 'MS%02d' "$cr_i")
        if printf '%s\n' "$cr_fail" | grep -qx "$cr_id"; then
            echo "  $cr_id  FAIL"
            cr_fail_n=$((cr_fail_n + 1))
        elif printf '%s\n' "$cr_ok" | grep -qx "$cr_id"; then
            echo "  $cr_id  PASS"
            cr_pass_n=$((cr_pass_n + 1))
        elif printf '%s\n' "$cr_nook" | grep -qx "$cr_id"; then
            echo "  $cr_id  PASS (no server: only the part with no server ran)"
            cr_part_n=$((cr_part_n + 1))
        else
            echo "  $cr_id  SKIP ($cr_why)"
            cr_none_n=$((cr_none_n + 1))
        fi
        cr_i=$((cr_i + 1))
    done
    echo "  summary: $cr_pass_n PASS, $cr_part_n PASS (no server), $cr_fail_n FAIL, $cr_none_n SKIP of 18 cases"
    CASE_FAILED=$cr_fail_n
    CASE_PASSED=$cr_pass_n
}

status=0
tls="$BASE/tls"
make_certs "$tls"
MGS_ADMIN_PASSWORD=$(secret)
MGS_RO_PASSWORD=$(secret)
MGS_RW_PASSWORD=$(secret)
MGS_BAD_PASSWORD=$(secret)
MGS_LEGACY_PASSWORD=$(secret)
free_port
MGS_PORT_70=$FP
free_port
MGS_PORT_70_JS=$FP
free_port
MGS_PORT_70_RS=$FP
free_port
MGS_PORT_70_RS2=$FP
free_port
MGS_PORT_70_SHA1=$FP
free_port
MGS_PORT_70_PLAIN=$FP
free_port
MGS_PORT_80=$FP
free_port
MGS_PORT_80_JS=$FP
free_port
MGS_PORT_80_RS=$FP
free_port
MGS_PORT_80_RS2=$FP
free_port
MGS_PORT_80_SHA1=$FP
free_port
MGS_PORT_80_PLAIN=$FP
MGS_TLS_DIR=$tls
IMAGE_MONGO_7_0=$(image_for 7.0)
IMAGE_MONGO_8_0=$(image_for 8.0)
export MGS_ADMIN_PASSWORD MGS_RO_PASSWORD MGS_RW_PASSWORD MGS_BAD_PASSWORD MGS_LEGACY_PASSWORD
export MGS_PORT_70 MGS_PORT_70_JS MGS_PORT_70_RS MGS_PORT_70_RS2 MGS_PORT_70_SHA1 MGS_PORT_70_PLAIN
export MGS_PORT_80 MGS_PORT_80_JS MGS_PORT_80_RS MGS_PORT_80_RS2 MGS_PORT_80_SHA1 MGS_PORT_80_PLAIN
export MGS_TLS_DIR IMAGE_MONGO_7_0 IMAGE_MONGO_8_0
[ -x "$FIXTURE/apply.sh" ] || ci_die "tests/fixtures/mongo-source/apply.sh is not executable"
[ -x "$FIXTURE/start.sh" ] || ci_die "tests/fixtures/mongo-source/start.sh is not executable"
[ -x "$FIXTURE/selfcheck.sh" ] || ci_die "tests/fixtures/mongo-source/selfcheck.sh is not executable"

compose() {
    "$DOCKER" compose -p "$PROJECT" -f "$COMPOSE_FILE" "$@"
}

for v in $VERSIONS; do
    echo "== MongoDB $v"
    n=$(printf '%s' "$v" | tr -d .)
    services="mg$n mg$n-js mg$n-rs mg$n-sha1 mg$n-plain"
    # shellcheck disable=SC2086
    if ! compose up -d --wait --wait-timeout 420 $services >"$BASE/up-$v.log" 2>&1; then
        echo "error: the services of MongoDB $v did not become healthy" >&2
        tail -n 40 "$BASE/up-$v.log" >&2
        # shellcheck disable=SC2086
        compose logs --tail 60 $services >&2 || true
        status=1
        compose down -v --remove-orphans >/dev/null 2>&1 || true
        continue
    fi
    eval "port=\$MGS_PORT_$n; js_port=\$MGS_PORT_${n}_JS; rs_port=\$MGS_PORT_${n}_RS; rs_port2=\$MGS_PORT_${n}_RS2; sha1_port=\$MGS_PORT_${n}_SHA1; plain_port=\$MGS_PORT_${n}_PLAIN"
    cname_of() {
        cid=$(compose ps -q "$1")
        "$DOCKER" inspect --format '{{.Name}}' "$cid" | sed 's|^/||'
    }
    cname=$(cname_of "mg$n")

    # The self check of the fixture on each server.
    export MGS_ADMIN_PASSWORD MGS_RO_PASSWORD MGS_RW_PASSWORD MGS_BAD_PASSWORD MGS_LEGACY_PASSWORD DOCKER
    export MGS_EXPECT_VERSION="$v"
    echo "-- fixture check of MongoDB $v"
    "$FIXTURE/selfcheck.sh" "$cname" "$port" 1000000 10485760 >"$BASE/self-$v-main.log" 2>&1 || status=1
    echo "   server main: $(grep -c '^ok' "$BASE/self-$v-main.log") ok, $(tail -n 1 "$BASE/self-$v-main.log")"
    grep -E '^FAIL' "$BASE/self-$v-main.log" || true
    for pair in "js:$js_port" "rs:$rs_port" "sha1:$sha1_port" "plain:$plain_port"; do
        kind=${pair%%:*}
        p=${pair#*:}
        extra=""
        [ "$kind" != sha1 ] || extra=sha1
        [ "$kind" != plain ] || extra=plain
        noscripting=1
        [ "$kind" != js ] || noscripting=0
        replset=""
        [ "$kind" != rs ] || replset=rs0
        MGS_EXPECT_NOSCRIPTING=$noscripting MGS_EXPECT_REPLSET=$replset "$FIXTURE/selfcheck.sh" "$(cname_of "mg$n-$kind")" "$p" 2000 1048576 $extra >"$BASE/self-$v-$kind.log" 2>&1 ||
            status=1
        echo "   server $kind: $(grep -c '^ok' "$BASE/self-$v-$kind.log") ok, $(tail -n 1 "$BASE/self-$v-$kind.log")"
        grep -E '^FAIL' "$BASE/self-$v-$kind.log" || true
    done

    # The tests.
    export HUNE_TEST_MGS_VERSION="$v"
    export HUNE_TEST_MGS_HOST=localhost
    export HUNE_TEST_MGS_IP=127.0.0.1
    export HUNE_TEST_MGS_PORT="$port"
    export HUNE_TEST_MGS_JS_PORT="$js_port"
    export HUNE_TEST_MGS_RS_PORT="$rs_port"
    export HUNE_TEST_MGS_RS_NAME=rs0
    export HUNE_TEST_MGS_RS_PORT2="$rs_port2"
    export HUNE_TEST_MGS_RS_HOSTS="localhost:$rs_port,localhost:$rs_port2"
    export HUNE_TEST_MGS_SHA1_PORT="$sha1_port"
    export HUNE_TEST_MGS_PLAIN_PORT="$plain_port"
    export HUNE_TEST_MGS_DATABASE=analytics
    export HUNE_TEST_MGS_OTHER_DATABASE=other
    export HUNE_TEST_MGS_AUTH_DATABASE=admin
    export HUNE_TEST_MGS_ADMIN_USER=root
    export HUNE_TEST_MGS_ADMIN_PASSWORD="$MGS_ADMIN_PASSWORD"
    export HUNE_TEST_MGS_RO_USER=hune_ro
    export HUNE_TEST_MGS_RO_PASSWORD="$MGS_RO_PASSWORD"
    export HUNE_TEST_MGS_RW_USER=hune_rw
    export HUNE_TEST_MGS_RW_PASSWORD="$MGS_RW_PASSWORD"
    export HUNE_TEST_MGS_BAD_USERS="hune_bad_readwrite:M2:admin hune_bad_readany:M2:admin hune_bad_monitor:M2:admin hune_bad_extradb:M2:admin hune_bad_insert:M3:admin hune_bad_anyres:M3:admin hune_bad_cluster:M3:admin hune_bad_authdb:M1:other"
    export HUNE_TEST_MGS_BAD_PASSWORD="$MGS_BAD_PASSWORD"
    export HUNE_TEST_MGS_SHA1_USER=hune_sha1
    export HUNE_TEST_MGS_PLAIN_USER=hune_plain
    export HUNE_TEST_MGS_LEGACY_PASSWORD="$MGS_LEGACY_PASSWORD"
    export HUNE_TEST_MGS_CA_FILE="$tls/ca.pem"
    export HUNE_TEST_MGS_WRONG_CA_FILE="$tls/wrong-ca.pem"
    export HUNE_TEST_MGS_NODE_LOAD_DOCS=1000000
    export HUNE_TEST_MGS_BIG_BYTES=10485760
    export HUNE_TEST_MGS_DOCKER="$DOCKER"
    export HUNE_TEST_MGS_CONTAINER="$cname"
    export HUNE_TEST_MGS_COMPOSE_PROJECT="$PROJECT"
    export HUNE_TEST_MGS_SERVICE="mg$n"

    if [ -n "$DEV_DIR" ]; then
        # The tests are the work of the developer: write the variables and leave the servers up.
        {
            echo "# Variables of the tests of MongoDB $v. Source this file with the shell command ."
            env | grep '^HUNE_TEST_MGS_' | awk '{
                k = $0; sub(/=.*/, "", k); v = substr($0, length(k) + 2); gsub("\047", "\047\\\047\047", v)
                print "export " k "=\047" v "\047"
            }'
        } >"$BASE/env.sh"
        echo "mongosource: servers of MongoDB $v are up; the variables are in $BASE/env.sh; remove them with --down $BASE"
        trap - EXIT INT TERM
        exit "$status"
    fi

    run_rc=0
    run_tests "$BASE/run-$v.log" "$BASE/run-$v-noserver.log" || run_rc=1
    cat "$BASE/run-$v.log" "$BASE/run-$v-noserver.log" >>"$LOG"
    echo "-- cases of MongoDB $v"
    CASE_FAILED=0
    case_report "$BASE/run-$v.log" "$BASE/run-$v-noserver.log"
    [ "$CASE_FAILED" -eq 0 ] || status=1
    # The merge gate: a required case that failed or did not run fails the job.
    cat "$BASE/run-$v.log" "$BASE/run-$v-noserver.log" >"$BASE/run-$v-all.log"
    ci_gate_evaluate mongosource "$BASE/run-$v-all.log" "$run_rc" || status=1
    if [ "$run_rc" -ne 0 ] && [ "$CASE_FAILED" -eq 0 ]; then
        echo "error: a test run failed with no failed case (build error or a test that is not a case)" >&2
        tail -n 30 "$BASE/run-$v.log" >&2
        status=1
    fi

    # shellcheck disable=SC2086
    compose rm -s -f -v $services >/dev/null 2>&1 || true
done

if [ "$status" -eq 0 ]; then
    echo "mongosource: ok"
else
    echo "mongosource: FAILED"
fi
exit "$status"
