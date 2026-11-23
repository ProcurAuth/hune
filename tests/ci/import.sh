#!/bin/sh
# Job `import` (16.17, 16.13): the importer.
# Use: tests/ci/import.sh [--local]
# Steps:
#   1. cargo test -p hune-grafana --locked
#   2. the CLI cases of 16.13 on the release binary: cargo test -p hune --release --test import --locked
#      (only when crates/hune/tests/import.rs exists)
#   3. the expansion vectors: the compose project tests/compose/grafana.yaml, service `import-tests`
#      (only when the file exists; the project needs Docker, so --local skips it)
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init "$@"

in_builder cargo test -p hune-grafana --locked

if [ -f crates/hune/tests/import.rs ]; then
    in_builder cargo test -p hune --release --test import --locked
else
    echo "import: crates/hune/tests/import.rs is absent; the CLI cases are not run"
fi

if [ ! -f tests/compose/grafana.yaml ]; then
    echo "import: tests/compose/grafana.yaml is absent; the expansion vectors are not run"
elif [ "$LOCAL" -eq 1 ]; then
    echo "import: skipped the expansion vectors (needs Docker; --local given)"
elif grep -q '^grafana-clickhouse-plugin .*digest: TO-PIN' tests/images.lock; then
    # The compose project downloads the plugin archive and checks its SHA-256 (16.13). Without a pinned
    # digest it refuses to run. The committed vectors still run in the unit tests of hune-core.
    echo "import: skipped the expansion vectors (the plugin archive has no pinned digest in tests/images.lock)"
else
    ci_compose_run tests/compose/grafana.yaml import-tests import
fi
echo "import: ok"
