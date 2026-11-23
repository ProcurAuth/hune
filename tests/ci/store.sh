#!/bin/sh
# Job `store` (16.17, 16.6, 16.14): the store conformance suite and the restore drill.
# Use: tests/ci/store.sh [--local]
#   (default) runs the restore drill (tests/ci/restore-drill.sh) on PostgreSQL 15 and 18 with the images
#             of tests/images.lock and the release binary. Then it runs the compose project
#             tests/compose/store.yaml: service `store-tests` runs
#             `cargo test -p hune-store --features conformance --locked` (volume on tmpfs, tmpfs of
#             1 MiB, PostgreSQL 15 and 18 from M5).
#   --local   runs `cargo test -p hune-store --features conformance --locked` on the host. The tests
#             that need PostgreSQL or a tmpfs report what they skip. Then it runs the restore drill
#             with the debug binary, when Docker answers.
# The restore drill writes only under target/drill/. Its log is target/ci-store-drill.log. A failed
# drill fails the job.
# Until tests/compose/store.yaml exists, the default mode runs the restore drill, prints a note for the
# conformance suite and runs no case of it, as the job `browser` does. The gate still fails when it requires
# a case at the current milestone.
# Every case runs on each push. The job fails only for a case of the gate list of tests/ci/gate-cases.txt
# at the milestone of tests/ci/milestone, or of an earlier one (16.17). Test names start with the case id.
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init "$@"

mkdir -p target
log=target/ci-store.log
drill_log=target/ci-store-drill.log
status=0
if [ "$LOCAL" -eq 1 ]; then
    cargo test -p hune-store --features conformance --locked >"$log" 2>&1 || status=$?
    cat "$log"
    if docker version >/dev/null 2>&1; then
        tests/ci/restore-drill.sh >"$drill_log" 2>&1 || status=$?
        cat "$drill_log"
    else
        echo "store: Docker does not answer; the restore drill is not run"
    fi
else
    ci_build_binary "$(ci_host_triple)"
    HUNE_BINARY="$ROOT/target/$(ci_host_triple)/release/hune"
    export HUNE_BINARY
    ci_export_images
    tests/ci/restore-drill.sh >"$drill_log" 2>&1 || status=$?
    cat "$drill_log"
    if [ ! -f tests/compose/store.yaml ]; then
        echo "store: the conformance suite is not built yet (tests/compose/store.yaml is absent); no case runs" | tee "$log"
    else
        ci_compose_run tests/compose/store.yaml store-tests store || status=$?
        cp target/ci-store.log "$log" 2>/dev/null || true
    fi
fi

ci_gate_evaluate store "$log" "$status"
