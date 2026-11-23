#!/bin/sh
# Job `api` (16.17, 16.8): the black-box suite of the HTTP API, and the environment test K14.
# Use: tests/ci/api.sh [--local]
#   (default) builds the musl binary (17.2), then runs the compose project tests/compose/api.yaml.
#             Its service `api-tests` runs
#             `cargo test -p hune --test api --locked -- --test-threads=1` and then the test K14.
#   --local   runs the same cargo test on the host, with no compose project.
# The compose file reads these variables: HUNE_BINARY (the built binary), IMAGE_<NAME> (tests/images.lock).
# Until tests/compose/api.yaml exists, the default mode prints a note, runs no case and builds nothing, as the
# job `browser` does. The gate below still fails when it requires a case at the current milestone.
# Every case runs on each push. The job fails only for a case of the gate list (tests/ci/gate-cases.txt)
# at the milestone of tests/ci/milestone, or of an earlier one. Test names start with the case id,
# for example a01_list_dashboards or k14a_environment_has_no_effect.
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init "$@"

mkdir -p target
log=target/ci-api.log
status=0
if [ "$LOCAL" -eq 1 ]; then
    if [ -f crates/hune/tests/api.rs ] || [ -d crates/hune/tests/api ]; then
        cargo test -p hune --test api --locked -- --test-threads=1 >"$log" 2>&1 || status=$?
        cat "$log"
    else
        echo "api: crates/hune/tests/api.rs is absent; no case runs" | tee "$log"
        status=0
    fi
else
    if [ ! -f tests/compose/api.yaml ]; then
        echo "api: the black-box suite is not built yet (tests/compose/api.yaml is absent); no case runs" | tee "$log"
    else
        ci_build_binary "$(ci_host_triple)"
        HUNE_BINARY="$ROOT/target/$(ci_host_triple)/release/hune"
        export HUNE_BINARY
        ci_compose_run tests/compose/api.yaml api-tests api || status=$?
        cp target/ci-api.log "$log" 2>/dev/null || true
    fi
fi

ci_gate_evaluate api "$log" "$status"
