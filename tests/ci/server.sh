#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Job `server` (not in 16.17): the ignored tests of the crate hune against a real ClickHouse.
# Use: tests/ci/server.sh [--local] [TAG ...]
# Each TAG is a tag of the image `clickhouse/clickhouse-server`. The default is `26.8.34.13`.
# The script starts the container as tests/ci/source.sh does, waits for the seed table, runs
#   cargo test -p hune --locked -- --ignored real_ch --test-threads=1
# and removes the container. Docker runs the ClickHouse container in both modes.
# Default mode runs cargo in the builder image with the host network; --local runs cargo on the host.
# Variables: DOCKER (docker command), CH_PORT and CH_TLS_PORT (default: a free port of 127.0.0.1, taken at start).
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init_args=
tags=
for a in "$@"; do
    case $a in
    --local) ci_init_args="--local" ;;
    *) tags="$tags $a" ;;
    esac
done
# shellcheck disable=SC2086
ci_init $ci_init_args

docker=${DOCKER:-docker}
http_port=${CH_PORT:-$(ci_free_port)}
tls_port=${CH_TLS_PORT:-$(ci_free_port)}
name=hune-test-ch-server-$$
tags=${tags:-26.8.34.13}
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
        -v "$ROOT/tests/fixtures/clickhouse/init.sql:/docker-entrypoint-initdb.d/init.sql:ro" \
        -v "$ROOT/tests/fixtures/clickhouse/config.d/keepalive.xml:/etc/clickhouse-server/config.d/keepalive.xml:ro" \
        -v "$ROOT/tests/fixtures/clickhouse/config.d/tls.xml:/etc/clickhouse-server/config.d/tls.xml:ro" \
        -v "$ROOT/crates/hune-source/tests/fixtures/tls/server.pem:/etc/clickhouse-server/tls/server.pem:ro" \
        -v "$ROOT/crates/hune-source/tests/fixtures/tls/server.key:/etc/clickhouse-server/tls/server.key:ro" \
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

    if [ "$LOCAL" -eq 1 ]; then
        HUNE_TEST_CLICKHOUSE="127.0.0.1:$http_port" HUNE_TEST_CLICKHOUSE_TLS="127.0.0.1:$tls_port" \
            HUNE_TEST_CLICKHOUSE_VERSION="$tag" \
            cargo test -p hune --locked -- --ignored real_ch --test-threads=1 || status=1
    else
        ci_build_builder
        mkdir -p "$ROOT/target/ci-cargo-home"
        "$docker" run --rm --network host --user "$(id -u):$(id -g)" \
            -v "$ROOT:/src" -w /src \
            -e HOME=/tmp -e CARGO_HOME=/src/target/ci-cargo-home \
            -e "RUSTUP_TOOLCHAIN=$(ci_toolchain)" \
            -e "HUNE_TEST_CLICKHOUSE=127.0.0.1:$http_port" \
            -e "HUNE_TEST_CLICKHOUSE_TLS=127.0.0.1:$tls_port" \
            -e "HUNE_TEST_CLICKHOUSE_VERSION=$tag" \
            "$BUILDER_TAG" \
            cargo test -p hune --locked -- --ignored real_ch --test-threads=1 || status=1
    fi
done
exit "$status"
