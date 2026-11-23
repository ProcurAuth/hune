#!/bin/sh
# Job `pgwire` (5.23 to 5.31, 14.12): the PostgreSQL tests of the crate `hune` on PostgreSQL 15 and 18.
# Use: tests/ci/pgwire.sh [--versions "15 18"]
#   For each version, the script starts one container on 127.0.0.1 with TLS, creates the two roles
#   of 5.25 (owner and application), and runs the ignored tests of the crate `hune` that start with
#   `pg_`: the tests of `serve` in the module `server::tests::pg_serve` and the tests of the binary in
#   `tests/pg_cli.rs`. The tests make their databases. The script removes the container at the end.
#   Names of the containers start with `hune-test-pgwire-`. The script reads the images from the
#   local cache and sends nothing to a registry.
#   Variables: DOCKER (the docker command, default `docker`), HUNE_PG_VERSIONS.
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
TLS="$ROOT/target/pgwire-tls-$RUN"
LOG="$ROOT/target/pgwire.log"
mkdir -p "$ROOT/target" "$TLS"
chmod 755 "$TLS"
: >"$LOG"
CONTAINERS=""

cleanup() {
    for c in $CONTAINERS; do
        $DOCKER rm -f -v "$c" >/dev/null 2>&1 || true
    done
    rm -rf "$TLS" "$LOG.run"
}
trap cleanup EXIT INT TERM

# One CA and one server certificate for `localhost` and 127.0.0.1, and a second CA that signed nothing.
make_certs() {
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TLS/ca.key" -out "$TLS/ca.pem" -days 2 \
        -subj "/CN=hune-test-ca" >/dev/null 2>&1
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TLS/wrong-ca.key" -out "$TLS/wrong-ca.pem" -days 2 \
        -subj "/CN=hune-test-wrong-ca" >/dev/null 2>&1
    openssl req -newkey rsa:2048 -nodes -keyout "$TLS/server.key" -out "$TLS/server.csr" \
        -subj "/CN=localhost" >/dev/null 2>&1
    printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' \
        >"$TLS/server.ext"
    openssl x509 -req -in "$TLS/server.csr" -CA "$TLS/ca.pem" -CAkey "$TLS/ca.key" -CAcreateserial \
        -out "$TLS/server.crt" -days 2 -extfile "$TLS/server.ext" >/dev/null 2>&1
    chmod 644 "$TLS"/*
}

# start_pg VERSION : start the container, wait until it takes TCP connections, and create the two roles.
start_pg() {
    v=$1
    name="hune-test-pgwire-$v-$RUN"
    CONTAINERS="$CONTAINERS $name"
    $DOCKER run -d --name "$name" -e POSTGRES_PASSWORD=adminpw -p 127.0.0.1::5432 -v "$TLS:/tls:ro" \
        --entrypoint sh "postgres:$v" -c '
        mkdir -p /var/tls && cp /tls/server.crt /tls/server.key /var/tls/ &&
        chown postgres:postgres /var/tls/* && chmod 600 /var/tls/server.key &&
        exec docker-entrypoint.sh postgres -c ssl=on -c ssl_cert_file=/var/tls/server.crt \
            -c ssl_key_file=/var/tls/server.key -c fsync=off -c full_page_writes=off -c max_connections=300' \
        >/dev/null
    i=0
    until $DOCKER exec "$name" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; do
        i=$((i + 1))
        [ $i -lt 90 ] || { echo "error: $name does not start" >&2; $DOCKER logs "$name" >&2; return 1; }
        sleep 1
    done
    # The first server of the image stops after its init: wait for the final one.
    sleep 2
    until $DOCKER exec "$name" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; do sleep 1; done
    # The two roles of 5.25. The tests make their databases and the grants of 5.25 in them.
    $DOCKER exec "$name" psql -U postgres -v ON_ERROR_STOP=1 \
        -c "CREATE ROLE hune_owner LOGIN PASSWORD 'ownerpw'" \
        -c "CREATE ROLE hune_app LOGIN PASSWORD 'apppw'" >/dev/null
    PORT=$($DOCKER port "$name" 5432/tcp | head -n 1 | sed 's/.*://')
    CURRENT=$name
}

status=0
make_certs
for v in $VERSIONS; do
    echo "== PostgreSQL $v"
    start_pg "$v"
    export HUNE_PGW_DOCKER="$DOCKER"
    export HUNE_PGW_CONTAINER="$CURRENT"
    export HUNE_PGW_HOST=127.0.0.1
    export HUNE_PGW_PORT="$PORT"
    export HUNE_PGW_OWNER_USER=hune_owner
    export HUNE_PGW_OWNER_PASSWORD=ownerpw
    export HUNE_PGW_APP_USER=hune_app
    export HUNE_PGW_APP_PASSWORD=apppw
    export HUNE_PGW_CA_FILE="$TLS/ca.pem"
    export HUNE_PGW_WRONG_CA_FILE="$TLS/wrong-ca.pem"
    export HUNE_PGW_SERVER_NAME=localhost
    # The unit tests of the module and the integration tests of the binary, with the ignored ones.
    {
        cargo test -p hune --locked --bin hune -- --include-ignored pg_ --test-threads=4 &&
            cargo test -p hune --locked --test pg_cli -- --include-ignored --test-threads=4
    } >"$LOG.run" 2>&1 || status=1
    tee -a "$LOG" <"$LOG.run"
    $DOCKER rm -f -v "$CURRENT" >/dev/null 2>&1 || true
done
exit $status
