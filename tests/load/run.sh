#!/bin/sh
# Load test of 16.16 (job `load`): a local ClickHouse, the release binary of Hune, a synthetic 45-panel
# dashboard and the Node driver `dashboard.mjs`. The script stops the server and removes the container on
# every path.
#
# Use: tests/load/run.sh [LABEL]
# Environment (all optional):
#   VIEWERS=52  REFRESH=30  DURATION=1800  BASELINE=120   viewers, refresh period (s), load phase (s), baseline phase (s)
#   HUNE_BIN       the binary (default target/load-build/release/hune; the script builds it when it is absent)
#   DOCKER, NODE    the docker command and the node command
#   CH_PORT=18323  HUNE_PORT=18380  OPS_PORT=18390   host ports, all on 127.0.0.1
#   CH_TAG=26.8.34.13   tag of the image clickhouse/clickhouse-server
#   STAGGER=true    spread the first load of the viewers over one refresh period
# Output: target/load/LABEL/ (config, logs, LABEL.json with the numbers). Exit code 0 on pass, 1 on fail.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
label=${1:-run}
docker=${DOCKER:-docker}
node=${NODE:-node}
viewers=${VIEWERS:-52}
refresh=${REFRESH:-30}
duration=${DURATION:-1800}
baseline=${BASELINE:-120}
stagger=${STAGGER:-true}
ch_port=${CH_PORT:-18323}
hune_port=${HUNE_PORT:-18380}
ops_port=${OPS_PORT:-18390}
tag=${CH_TAG:-26.8.34.13}
bin=${HUNE_BIN:-$root/target/load-build/release/hune}
work=$root/target/load/$label
name=hune-test-load-$$
admin_user=hune_admin
admin_password=hune-test-admin-password
hune_pid=

cleanup() {
    trap - EXIT INT TERM
    if [ -n "$hune_pid" ] && kill -0 "$hune_pid" 2>/dev/null; then
        kill -TERM "$hune_pid" 2>/dev/null || true
        i=0
        while kill -0 "$hune_pid" 2>/dev/null && [ "$i" -lt 60 ]; do
            sleep 1
            i=$((i + 1))
        done
        if kill -0 "$hune_pid" 2>/dev/null; then
            echo "hune did not stop after SIGTERM: SIGKILL" >&2
            kill -KILL "$hune_pid" 2>/dev/null || true
        fi
        stop_status=0
        wait "$hune_pid" 2>/dev/null || stop_status=$?
        echo "hune stopped with status $stop_status" | tee -a "$work/run.log"
    fi
    "$docker" rm -f -v "$name" >/dev/null 2>&1 || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

rm -rf "$work"
mkdir -p "$work/volume"

if [ ! -x "$bin" ]; then
    echo "building the release binary"
    CARGO_TARGET_DIR="$root/target/load-build" cargo build --release --locked -p hune --manifest-path "$root/Cargo.toml"
fi

echo "== ClickHouse $tag ($name)"
"$docker" run -d --name "$name" \
    -p "127.0.0.1:$ch_port:8123" \
    -e CLICKHOUSE_USER="$admin_user" -e CLICKHOUSE_PASSWORD="$admin_password" \
    -e CLICKHOUSE_DEFAULT_ACCESS_MANAGEMENT=1 \
    -v "$root/tests/fixtures/clickhouse/init.sql:/docker-entrypoint-initdb.d/init.sql:ro" \
    -v "$root/tests/fixtures/clickhouse/config.d/keepalive.xml:/etc/clickhouse-server/config.d/keepalive.xml:ro" \
    -v "$root/tests/load/config.d/limits.xml:/etc/clickhouse-server/config.d/limits.xml:ro" \
    "clickhouse/clickhouse-server:$tag" >/dev/null

# The init script runs on a first temporary server: wait for the seed table on the final one.
ready=0
i=0
while [ "$i" -lt 90 ]; do
    if curl -fsS -u "$admin_user:$admin_password" "http://127.0.0.1:$ch_port/" \
        -d 'SELECT count() FROM analytics.types_extra' >/dev/null 2>&1; then
        ready=1
        break
    fi
    i=$((i + 1))
    sleep 1
done
if [ "$ready" -ne 1 ]; then
    echo "ClickHouse did not become ready" >&2
    "$docker" logs "$name" 2>&1 | tail -20 >&2
    exit 2
fi

# The synthetic dashboard goes in a volume. Hune validates it when it lists it (the driver checks).
"$node" "$root/tests/load/gen.mjs" "$work/volume" 45

printf '%s\n' 'hune-test-password' >"$work/ch.password"
chmod 600 "$work/ch.password"

# The configuration is the reference of the profile `local`, with the values of this run.
# `max_concurrent` covers the 6 queries in flight of each viewer. The `tls` block is removed.
max_concurrent=$((viewers * 6 + 16))
if [ "$max_concurrent" -gt 1024 ]; then max_concurrent=1024; fi
"$bin" print-reference --format hcl --profile local >"$work/reference.hcl"
"$node" -e '
const fs = require("fs");
const [ref, out, listen, ops, volume, chUrl, pwFile, maxConc] = process.argv.slice(1);
let t = fs.readFileSync(ref, "utf8");
const set = (re, to) => {
  if (!re.test(t)) { console.error("pattern not found: " + re); process.exit(2); }
  t = t.replace(re, to);
};
set(/^(\s*listen\s*=\s*)"[^"]*"/m, `$1"${listen}"`);
set(/^(\s*ops_listen\s*=\s*)"[^"]*"/m, `$1"${ops}"`);
set(/^(\s*path\s*=\s*)"[^"]*"/m, `$1"${volume}"`);
set(/^(\s*url\s*=\s*)"[^"]*"/m, `$1"${chUrl}"`);
set(/^(\s*password_file\s*=\s*)"[^"]*"/m, `$1"${pwFile}"`);
set(/^(\s*max_concurrent\s*=\s*)\d+/m, `$1${maxConc}`);
set(/^\s*tls\s*\{[^}]*\}\s*\n/m, "");
fs.writeFileSync(out, t);
' "$work/reference.hcl" "$work/hune.hcl" "127.0.0.1:$hune_port" "127.0.0.1:$ops_port" \
    "$work/volume" "http://127.0.0.1:$ch_port" "$work/ch.password" "$max_concurrent"

"$bin" print-config --config "$work/hune.hcl" >"$work/effective-config.txt" 2>&1 || {
    echo "the configuration is not valid:" >&2
    cat "$work/effective-config.txt" >&2
    exit 2
}

echo "== hune serve ($bin)"
"$bin" serve --config "$work/hune.hcl" >"$work/hune.log" 2>&1 &
hune_pid=$!
ready=0
i=0
while [ "$i" -lt 60 ]; do
    if ! kill -0 "$hune_pid" 2>/dev/null; then
        echo "hune stopped at start:" >&2
        tail -20 "$work/hune.log" >&2
        hune_pid=
        exit 2
    fi
    if curl -fsS "http://127.0.0.1:$ops_port/readyz" >/dev/null 2>&1; then
        ready=1
        break
    fi
    i=$((i + 1))
    sleep 1
done
if [ "$ready" -ne 1 ]; then
    echo "hune did not become ready" >&2
    tail -20 "$work/hune.log" >&2
    exit 2
fi

status=0
"$node" "$root/tests/load/dashboard.mjs" \
    --hune "http://127.0.0.1:$hune_port" --clickhouse "http://127.0.0.1:$ch_port" \
    --pid "$hune_pid" --out "$work" --label "$label" \
    --viewers "$viewers" --refresh "$refresh" --duration "$duration" --baseline-duration "$baseline" \
    --stagger "$stagger" --ch-password-file "$work/ch.password" 2>&1 | tee "$work/run.log" || true
# `tee` hides the status of the driver: read it from the result file.
if grep -q "^result $label: PASS" "$work/run.log"; then status=0; else status=1; fi
exit "$status"
