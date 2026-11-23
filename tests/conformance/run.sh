#!/bin/sh
# Job `procurauth` (16.12, 16.17, 11.32): the conformance series U of ProcurAuth against Hune in the mode authzen.
# Use: tests/conformance/run.sh [--local]
#   (default) builds the runner and the static musl binary of Hune in Docker. It starts Hune in a
#             container with no network and a read-only root file system, a ClickHouse container in the
#             same network namespace, and the runner in the same namespace too.
#   --local   runs the same checks on the host: the binary target/debug/hune (or target/release/hune), a
#             ClickHouse container with port 8123 published on 127.0.0.1, and the runner of the
#             ProcurAuth checkout. Use it for a first run. The result is the same.
# Input: PROCURAUTH_DIR, the checkout of https://github.com/SCcagg5/procurauth at the commit of the
#        dependency `procurauth-pep` (11.10). Without it, the script clones that commit into
#        target/procurauth. The script never changes this checkout, except for its `target/`.
# Output: target/conformance/report.txt. The script exits with 0 only when the set of failed cases equals
#        the set of ids in tests/conformance/expected.txt (the first word of each line).
# Pins: the ClickHouse image comes from tests/images.lock. No image is pushed.
set -eu
. "$(dirname "$0")/../ci/ci-lib.sh"
ci_init "$@"

OUT=$ROOT/target/conformance
WORK=$OUT/work
NAME=hune-conf-$RUN_ID
mkdir -p "$OUT"

# The commit of the dependency (11.10): the first `rev` of crates/hune/Cargo.toml or Cargo.toml.
REV=$(sed -n 's/.*procurauth-pep *= *{.*rev *= *"\([0-9a-f]\{40\}\)".*/\1/p' "$ROOT/Cargo.toml" | head -n 1)
[ -n "$REV" ] || ci_die "no procurauth-pep rev in Cargo.toml"

PA=${PROCURAUTH_DIR:-$ROOT/target/procurauth}
if [ ! -d "$PA/.git" ] && [ ! -f "$PA/Cargo.toml" ]; then
    git clone https://github.com/SCcagg5/procurauth "$PA"
    git -C "$PA" checkout --detach "$REV"
fi
if [ -d "$PA/.git" ]; then
    HEAD_REV=$(git -C "$PA" rev-parse HEAD)
    [ "$HEAD_REV" = "$REV" ] || ci_die "the ProcurAuth checkout is at $HEAD_REV, but the dependency is at $REV"
fi

CH_IMAGE=$(ci_lock_ref clickhouse-lts)
CH_NAME=$NAME-ch
HUNE_NAME=$NAME-hune
HUNE_PID=

cleanup() {
    [ -z "$HUNE_PID" ] || kill "$HUNE_PID" 2>/dev/null || true
    # Wait for the graceful stop (14.13), so no server stays after the script.
    [ -z "$HUNE_PID" ] || wait "$HUNE_PID" 2>/dev/null || true
    docker rm -f -v "$CH_NAME" "$HUNE_NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

# ---- The runner ----
RUNNER=$PA/target/release/access-conformance
if [ "$LOCAL" -eq 1 ]; then
    [ -x "$RUNNER" ] || (cd "$PA" && cargo build --release --locked -p access-conformance)
else
    ci_build_builder
    mkdir -p "$PA/target"
    docker run --rm --user "$(id -u):$(id -g)" -v "$PA:/pa" -w /pa -e HOME=/tmp -e CARGO_HOME=/pa/target/cargo-home \
        "$BUILDER_TAG" cargo build --release --locked -p access-conformance
fi

# ---- The binary of Hune ----
if [ "$LOCAL" -eq 1 ]; then
    if [ -x "$ROOT/target/release/hune" ]; then BIN=$ROOT/target/release/hune; else
        (cd "$ROOT" && cargo build --locked -p hune)
        BIN=$ROOT/target/debug/hune
    fi
else
    ci_build_binary "$(ci_host_triple)"
    BIN=$ROOT/target/$(ci_host_triple)/release/hune
fi

# ---- The fixture volume, the keys and the profile ----
rm -rf "$WORK"
mkdir -p "$WORK/data/dashboards"
cp -r "$ROOT/tests/conformance/fixture/conf" "$WORK/data/dashboards/conf"
printf '{\n  "format_version": 1,\n  "title": "Conformance"\n}\n' >"$WORK/data/dashboards/conf/_folder.json"
for f in jwks.json pdp-credential.txt clickhouse_password profile.json; do
    cp "$ROOT/tests/conformance/$f" "$WORK/$f"
done
cp "$PA/crates/access-conformance/examples/test-edge-ed25519.pem" "$PA/crates/access-conformance/examples/test-edge-es256.pem" "$WORK/"
if [ "$LOCAL" -eq 1 ]; then
    sed -e "s#/conformance/#$WORK/#g" -e "s#/data/dashboards#$WORK/data/dashboards#" \
        "$ROOT/tests/conformance/config.hcl" >"$WORK/config.hcl"
else
    cp "$ROOT/tests/conformance/config.hcl" "$WORK/config.hcl"
    chmod -R a+rwX "$WORK"
fi

wait_clickhouse() {
    ci_i=0
    while [ "$ci_i" -lt 90 ]; do
        if docker exec "$CH_NAME" clickhouse-client -q 'SELECT count() FROM analytics.conf_rows' >/dev/null 2>&1; then
            return 0
        fi
        ci_i=$((ci_i + 1))
        sleep 1
    done
    ci_die "ClickHouse did not start"
}

INIT="-v $ROOT/tests/fixtures/clickhouse/init.sql:/docker-entrypoint-initdb.d/01-init.sql:ro -v $ROOT/tests/conformance/init-conf.sql:/docker-entrypoint-initdb.d/02-conf.sql:ro"

if [ "$LOCAL" -eq 1 ]; then
    # shellcheck disable=SC2086
    docker run -d --name "$CH_NAME" -p 127.0.0.1:8123:8123 $INIT "$CH_IMAGE" >/dev/null
    wait_clickhouse
    "$BIN" serve --config "$WORK/config.hcl" >"$OUT/hune.log" 2>&1 &
    HUNE_PID=$!
else
    # Hune: no network, a read-only root file system. The volume and the files are the only mounts.
    docker run -d --name "$HUNE_NAME" --network none --read-only --user "$(id -u):$(id -g)" \
        -v "$BIN:/usr/local/bin/hune:ro" -v "$WORK:/conformance:ro" -v "$WORK/data:/data" \
        --entrypoint /usr/local/bin/hune "$BUILDER_TAG" serve --config /conformance/config.hcl >/dev/null
    # ClickHouse in the network namespace of Hune: 127.0.0.1:8123 for both.
    # shellcheck disable=SC2086
    docker run -d --name "$CH_NAME" --network "container:$HUNE_NAME" $INIT "$CH_IMAGE" >/dev/null
    wait_clickhouse
fi

# Wait for /readyz of Hune.
ci_i=0
while [ "$ci_i" -lt 60 ]; do
    if [ "$LOCAL" -eq 1 ]; then
        curl -fsS http://127.0.0.1:9090/readyz >/dev/null 2>&1 && break
    else
        docker exec "$CH_NAME" sh -c 'curl -fsS http://127.0.0.1:9090/readyz' >/dev/null 2>&1 && break
    fi
    ci_i=$((ci_i + 1))
    sleep 1
done
[ "$ci_i" -lt 60 ] || ci_die "Hune is not ready"

# ---- The run ----
status=0
if [ "$LOCAL" -eq 1 ]; then
    (cd "$WORK" && "$RUNNER" --role pep --profile profile.json --url http://127.0.0.1:8080 --series U) \
        >"$OUT/report.txt" 2>"$OUT/runner.err" || status=$?
else
    docker run --rm --network "container:$HUNE_NAME" --user "$(id -u):$(id -g)" -v "$WORK:/work" -w /work \
        -v "$PA/target/release/access-conformance:/usr/local/bin/access-conformance:ro" \
        "$BUILDER_TAG" access-conformance --role pep --profile profile.json --url http://127.0.0.1:8080 --series U \
        >"$OUT/report.txt" 2>"$OUT/runner.err" || status=$?
fi
cat "$OUT/report.txt"

# ---- The failed cases against expected.txt ----
awk '$2 == "fail" { print $1 }' "$OUT/report.txt" | sort >"$OUT/failed.txt"
sed -e 's/#.*//' "$ROOT/tests/conformance/expected.txt" | awk 'NF { print $1 }' | sort >"$OUT/expected.sorted"
if ! diff -u "$OUT/expected.sorted" "$OUT/failed.txt"; then
    ci_die "the failed cases differ from tests/conformance/expected.txt"
fi
SUMMARY=$(tail -n 1 "$OUT/report.txt")
echo "series U: $SUMMARY"
# The runner exits with 1 when a case fails. Here each failed case is listed, so the script exits with 0.
case $status in 0 | 1) exit 0 ;; *) ci_die "the runner exited with $status: see $OUT/runner.err" ;; esac
