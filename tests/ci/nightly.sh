#!/bin/sh
# Job `nightly` (16.17, 16.4, 16.16): 1,000 random seeds, the fuzz targets, the `source`, `pgsource` and `mongosource` jobs again.
# Use: tests/ci/nightly.sh [--local]
#   (default) runs cargo in the builder image. It runs no fuzz target (step 2).
#   --local   runs the same commands on the host.
# 1. 1,000 seeds of the random tests of the lexer and the substitution, 20,000 cases each. The seeds
#    come from the date: <YYYYMMDD> * 1000 + n, n from 1 to 1000. A failure prints the seed.
# 2. The fuzz targets (from M7): each runs 10 minutes. The step runs only when fuzz/Cargo.toml exists.
#    The default mode prints a note and runs no target: tests/images.lock has no image with a nightly
#    toolchain (the tag rust:nightly-trixie does not exist, 2026-11-22). --local runs the targets with the
#    nightly toolchain of the host.
# 3. The `source` job on its ClickHouse versions: tests/ci/source.sh (it needs Docker).
# 4. The `pgsource` job on PostgreSQL 15 and 18: tests/ci/pgsource.sh (it needs Docker).
# 5. The `mongosource` job on MongoDB 7.0 and 8.0: tests/ci/mongosource.sh (it needs Docker).
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init "$@"

base=$(date -u +%Y%m%d)
echo "nightly: seeds $((base * 1000 + 1)) to $((base * 1000 + 1000)), 20000 cases each"
in_builder sh -c '
    set -eu
    cargo test -p hune-core --locked --no-run
    n=1
    while [ "$n" -le 1000 ]; do
        seed=$(('"$base"' * 1000 + n))
        if ! HUNE_TEST_SEED="$seed" HUNE_TEST_CASES=20000 cargo test -p hune-core --locked random_ >target/ci-nightly-seed.log 2>&1; then
            echo "random tests failed: seed $seed" >&2
            tail -n 50 target/ci-nightly-seed.log >&2
            exit 1
        fi
        n=$((n + 1))
    done
'

if [ -f fuzz/Cargo.toml ]; then
    targets="lexer lexer_pg expand document_parse hcl_config grafana_import frame_decode"
    if [ "$LOCAL" -eq 1 ]; then
        for t in $targets; do
            cargo +nightly fuzz run "$t" -- -max_total_time=600
        done
    else
        echo "nightly: skipped the fuzz targets (tests/images.lock has no image with a nightly toolchain; use --local)"
    fi
else
    echo "nightly: fuzz/Cargo.toml is absent; the fuzz targets start at M7"
fi

if [ -x tests/ci/source.sh ]; then
    tests/ci/source.sh
else
    echo "nightly: tests/ci/source.sh is absent"
fi
if [ -x tests/ci/pgsource.sh ]; then
    tests/ci/pgsource.sh
else
    echo "nightly: tests/ci/pgsource.sh is absent"
fi
if [ -x tests/ci/mongosource.sh ]; then
    tests/ci/mongosource.sh
else
    echo "nightly: tests/ci/mongosource.sh is absent"
fi
echo "nightly: ok"
