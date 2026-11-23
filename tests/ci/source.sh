#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Job `source` (16.9): start a local ClickHouse on 127.0.0.1, run the ignored tests of
# hune-source against it, and remove the container.
#
# Use: tests/ci/source.sh [TAG ...]
# Each TAG is a tag of the image `clickhouse/clickhouse-server`. The default is `26.8.34.13`.
# Environment: DOCKER (path of the docker command, default `docker`),
#              CH_PORT (host port of the HTTP interface, default 18123),
#              CH_TLS_PORT (host port of the HTTPS interface, default 18443).
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
docker=${DOCKER:-docker}
http_port=${CH_PORT:-18123}
tls_port=${CH_TLS_PORT:-18443}
name=hune-test-ch-$$
tags=${*:-26.8.34.13}
admin_user=hune_admin
admin_password=hune-test-admin-password

cleanup() {
    "$docker" rm -f -v "$name" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

status=0
for tag in $tags; do
    echo "== ClickHouse $tag =="
    cleanup
    "$docker" run -d --name "$name" \
        -p "127.0.0.1:$http_port:8123" -p "127.0.0.1:$tls_port:8443" \
        -e CLICKHOUSE_USER="$admin_user" -e CLICKHOUSE_PASSWORD="$admin_password" \
        -e CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1 \
        -v "$root/tests/fixtures/clickhouse/init.sql:/docker-entrypoint-initdb.d/init.sql:ro" \
        -v "$root/tests/fixtures/clickhouse/config.d/keepalive.xml:/etc/clickhouse-server/config.d/keepalive.xml:ro" \
        -v "$root/tests/fixtures/clickhouse/config.d/tls.xml:/etc/clickhouse-server/config.d/tls.xml:ro" \
        -v "$root/crates/hune-source/tests/fixtures/tls/server.pem:/etc/clickhouse-server/tls/server.pem:ro" \
        -v "$root/crates/hune-source/tests/fixtures/tls/server.key:/etc/clickhouse-server/tls/server.key:ro" \
        "clickhouse/clickhouse-server:$tag" >/dev/null

    # The init script runs on a first temporary server: wait for the seed table on the final one.
    ready=0
    i=0
    while [ "$i" -lt 90 ]; do
        if curl -fsS -u "$admin_user:$admin_password" "http://127.0.0.1:$http_port/" \
            -d 'SELECT count() FROM analytics.types_extra' >/dev/null 2>&1; then
            ready=1
            break
        fi
        i=$((i + 1))
        sleep 1
    done
    if [ "$ready" -ne 1 ]; then
        echo "ClickHouse $tag did not become ready" >&2
        "$docker" logs "$name" 2>&1 | tail -20 >&2
        status=1
        continue
    fi

    HUNE_TEST_CLICKHOUSE="127.0.0.1:$http_port" HUNE_TEST_CLICKHOUSE_TLS="127.0.0.1:$tls_port" \
        HUNE_TEST_CLICKHOUSE_VERSION="$tag" \
        cargo test -p hune-source --locked --test clickhouse --test adhoc_clickhouse -- --ignored --test-threads=4 || status=1
done
exit "$status"
