#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# The merge gate of Hune (16.17): run each job of the gate in order. Stop at the first failure.
# The workflow ci.yaml runs the same jobs on GitHub, as an option. A maintainer can run this script instead,
# with no GitHub Actions and no GitHub billing, before each release.
# Use: tests/ci/all.sh [--list] [--from JOB]
#   (no option) runs the jobs in the order below, each with its default mode (Docker only).
#   --list      prints the jobs and exits.
#   --from JOB  starts at JOB and skips the jobs before it. Use it to continue after a fix.
#               A run that starts after the first job does not record the gate (see below).
# Jobs, in order: policy, rust, store, pgsource, mongosource, api, source, frontend, browser, import,
#   image (then the case I14 when both importer outputs exist), procurauth, failover.
#   The job `failover` is a stub until milestone M5: tests/failover/run.sh does not exist, so the job
#   prints a note and passes. Run the nightly job by hand: tests/ci/nightly.sh.
# Gate record: when every job passes on a clean work tree, the script writes the commit in
#   target/ci-all.ok. The release script tests/ci/release.sh reads this file and refuses another commit.
# Each job script uses Docker only. Each one removes its own containers. The script pushes nothing.
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init

JOBS="policy rust store pgsource mongosource api source frontend browser import image procurauth failover"
FROM=

while [ "$#" -gt 0 ]; do
    case $1 in
    --list)
        for j in $JOBS; do echo "$j"; done
        exit 0
        ;;
    --from)
        [ "$#" -ge 2 ] || ci_die "use: tests/ci/all.sh [--list] [--from JOB]"
        FROM=$2
        shift 2
        ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) ci_die "unknown option: $1 (use --list or --from JOB)" ;;
    esac
done

if [ -n "$FROM" ]; then
    case " $JOBS " in
    *" $FROM "*) ;;
    *) ci_die "unknown job '$FROM' (use --list)" ;;
    esac
fi

rm -f "$ROOT/target/ci-all.ok"
started=0
[ -n "$FROM" ] || started=1
commit=$(git rev-parse HEAD)
echo "all: commit $commit"

run_job() {
    rj_name=$1
    shift
    if [ "$started" -eq 0 ]; then
        if [ "$rj_name" != "$FROM" ]; then
            echo "all: skip $rj_name"
            return 0
        fi
        started=1
    fi
    echo "all: ==== $rj_name: start $(date -u +%H:%M:%S)"
    if "$@"; then
        echo "all: ==== $rj_name: ok"
    else
        echo "all: ==== $rj_name: FAILED" >&2
        echo "all: stop at the first failure. Fix it, then run: tests/ci/all.sh --from $rj_name" >&2
        exit 1
    fi
}

# The case I14: the two architectures give equal importer output. The outputs come from tests/ci/image.sh
# on an amd64 machine and on an arm64 machine. A machine of one architecture has one output only.
image_i14() {
    if [ -d dist/i14-amd64 ] && [ -d dist/i14-arm64 ]; then
        tests/ci/image.sh --compare-i14 dist/i14-amd64 dist/i14-arm64
    else
        echo "I14: no output of the second architecture in dist/; nothing to compare"
    fi
}

failover() {
    if [ -x tests/failover/run.sh ]; then
        tests/failover/run.sh kill
        tests/failover/run.sh partition
    else
        echo "failover: tests/failover/run.sh does not exist yet (milestone M5); stub"
    fi
}

run_job policy tests/ci/policy.sh
run_job rust tests/ci/rust.sh
run_job store tests/ci/store.sh
run_job pgsource tests/ci/pgsource.sh
run_job mongosource tests/ci/mongosource.sh
run_job api tests/ci/api.sh
run_job source tests/ci/source.sh
run_job frontend tests/ci/frontend.sh
run_job browser tests/ci/browser.sh
run_job import tests/ci/import.sh
run_job image tests/ci/image.sh
if [ "$started" -eq 1 ] && ! image_i14; then
    echo "all: ==== image (case I14): FAILED" >&2
    exit 1
fi
run_job procurauth tests/conformance/run.sh
run_job failover failover

if [ -n "$FROM" ] && [ "$FROM" != policy ]; then
    echo "all: ok (partial run from $FROM; no gate record)"
    exit 0
fi
if [ -n "$(git status --porcelain)" ]; then
    echo "all: ok, but the work tree is not clean; no gate record" >&2
    exit 0
fi
printf '%s\n' "$commit" >"$ROOT/target/ci-all.ok"
echo "all: ok. The gate is recorded in target/ci-all.ok for commit $commit"
