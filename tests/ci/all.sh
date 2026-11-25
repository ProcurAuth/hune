#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# The merge gate of Hune (16.17): run the jobs of the gate in parallel, in one run.
# The workflow ci.yaml runs the same jobs on GitHub, as an option. A maintainer can run this script instead,
# with no GitHub Actions and no GitHub billing, before each release.
# Use: tests/ci/all.sh [--list] [--only JOB[,JOB]] [--from JOB] [--max-jobs N]
#   (no option) runs all jobs, each with its default mode (Docker only), at the same time.
#   --list        prints the jobs and exits.
#   --only JOBS   runs only the jobs in the list (comma separated), and the build tasks that they need.
#   --from JOB    runs JOB and the jobs after it in the order of --list. Use it to run again the end of a run.
#   --max-jobs N  starts at most N tasks at the same time (default: no limit). Use it on a small host.
#   --only and --from do not record the gate (see below).
# Jobs, in the order of --list: policy, rust, store, pgsource, mongosource, api, source, frontend, browser,
#   import, image (then the case I14 when both importer outputs exist), procurauth, failover.
#   The job `failover` is a stub until milestone M5: tests/failover/run.sh does not exist, so the job
#   prints a note and passes. Run the nightly job by hand: tests/ci/nightly.sh.
# Order of the run: the jobs start at the same time, except for these needs.
#   The task `setup` builds the builder image. It runs first.
#   The task `build-debug` builds the debug test artifacts of the workspace one time (`cargo test --workspace
#   --no-run`). The jobs policy and rust wait for it.
#   The task `build-host` builds the debug test artifacts that the jobs pgsource, mongosource and source use on the host
#   (these jobs run cargo on the host, and the host builds do not share artifacts with the builder builds).
#   The task `build-binary` fetches the sources one time and builds the release musl binary one time. The jobs
#   that use the binary (store, image, procurauth) wait for it. The jobs api and browser wait for it when their
#   compose files exist.
#   A job that waits for a task does not start if the task fails. The other jobs still run.
#   Each job and each task writes its output to target/ci-logs/<name>.log. The script prints one line when a
#   task starts and one line when it ends, a line each minute with the running tasks, and a table at the end
#   with the duration of each task and the total. Each job runs to its end, also after the failure of
#   another job: one run shows every failure. The script shows the tail of the log of each failed task.
#   The exit code is 1 when a task fails.
# Isolation: each job names its containers, networks, volumes, images and folders with its own process id,
#   and takes its host ports from ci_free_port (ci-lib.sh): no job uses a fixed host port.
# Gate record: when every job passes in a run with no --only and no --from (or --from policy), on a clean work tree,
#   the script writes the commit in target/ci-all.ok. The release script tests/ci/release.sh reads this file
#   and refuses another commit.
# Each job script uses Docker only. Each one removes its own containers. The script pushes nothing.
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init

JOBS="policy rust store pgsource mongosource api source frontend browser import image procurauth failover"
ONLY=
FROM=
MAXJOBS=0

while [ "$#" -gt 0 ]; do
    case $1 in
    --list)
        for j in $JOBS; do echo "$j"; done
        exit 0
        ;;
    --only)
        [ "$#" -ge 2 ] || ci_die "use: tests/ci/all.sh [--list] [--only JOB[,JOB]] [--from JOB] [--max-jobs N]"
        ONLY=$2
        shift 2
        ;;
    --from)
        [ "$#" -ge 2 ] || ci_die "use: tests/ci/all.sh [--list] [--only JOB[,JOB]] [--from JOB] [--max-jobs N]"
        FROM=$2
        shift 2
        ;;
    --max-jobs)
        [ "$#" -ge 2 ] || ci_die "use: tests/ci/all.sh [--list] [--only JOB[,JOB]] [--from JOB] [--max-jobs N]"
        MAXJOBS=$2
        shift 2
        ;;
    -h | --help)
        grep '^#' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    *) ci_die "unknown option: $1 (use --list, --only JOB[,JOB], --from JOB or --max-jobs N)" ;;
    esac
done

case $MAXJOBS in
'' | *[!0-9]*) ci_die "--max-jobs needs a number, found '$MAXJOBS'" ;;
esac
[ -z "$ONLY" ] || [ -z "$FROM" ] || ci_die "use --only or --from, not both"

# is_job NAME : succeed when NAME is a job.
is_job() {
    case " $JOBS " in
    *" $1 "*) return 0 ;;
    *) return 1 ;;
    esac
}

# The jobs of this run, in the order of $JOBS.
SELECTED=
if [ -n "$ONLY" ]; then
    for j in $(printf '%s' "$ONLY" | tr ',' ' '); do
        is_job "$j" || ci_die "unknown job '$j' (use --list)"
    done
    for j in $JOBS; do
        case ",$ONLY," in
        *",$j,"*) SELECTED="$SELECTED $j" ;;
        esac
    done
elif [ -n "$FROM" ]; then
    is_job "$FROM" || ci_die "unknown job '$FROM' (use --list)"
    seen=0
    for j in $JOBS; do
        [ "$j" != "$FROM" ] || seen=1
        [ "$seen" -eq 0 ] || SELECTED="$SELECTED $j"
    done
else
    SELECTED=$JOBS
fi

# needs_of TASK : print the tasks that TASK waits for.
needs_of() {
    case $1 in
    build-debug | build-binary | import) echo setup ;;
    policy | rust) echo build-debug ;;
    pgsource | mongosource | source) echo build-host ;;
    store | image | procurauth) echo build-binary ;;
    api)
        # api.sh builds the binary only when its compose file exists.
        if [ -f tests/compose/api.yaml ]; then echo build-binary; fi
        ;;
    browser)
        if [ -f tests/browser/Dockerfile ] && [ -f tests/compose/browser.yaml ]; then echo build-binary; fi
        ;;
    esac
}

# target_dir_of TASK : print the cargo target folder of a task (relative to the root), or nothing for the folder
# `target`. The tasks that build different artifacts get different folders, so they do not wait for the build lock
# of one another. The jobs that run cargo on the host (pgsource, mongosource, source) share one folder: the host
# builds do not share artifacts with the builder builds, because the paths of the registry differ.
target_dir_of() {
    case $1 in
    policy) echo target/policy ;;
    build-host | pgsource | mongosource | source) echo target/host ;;
    import) echo target/import ;;
    esac
}

# The tasks of this run: the build tasks that the jobs need (in dependency order), then the jobs.
TASKS=
add_task() {
    case " $TASKS " in
    *" $1 "*) return 0 ;;
    esac
    for n in $(needs_of "$1"); do
        add_task "$n"
    done
    TASKS="$TASKS $1"
}
for j in $SELECTED; do
    for n in $(needs_of "$j"); do
        add_task "$n"
    done
done
STAGES=$TASKS
for j in $SELECTED; do
    TASKS="$TASKS $j"
done

# The commit of the run. A run on a clean tree gives a gate record at the end.
commit=$(git rev-parse HEAD)
rm -f "$ROOT/target/ci-all.ok"
LOGS=$ROOT/target/ci-logs
rm -rf "$LOGS"
rm -rf "$ROOT/target/ci-ports"
mkdir -p "$LOGS"
echo "all: commit $commit"
echo "all: tasks:$TASKS"
echo "all: logs in target/ci-logs/<task>.log"

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
        tests/failover/run.sh kill && tests/failover/run.sh partition
    else
        echo "failover: tests/failover/run.sh does not exist yet (milestone M5); stub"
    fi
}

# run_task NAME : the command of a task. It runs in a subshell. Each command ends with the code of the task:
# join the commands with && (the option -e does not act in a context that has a test on its result).
run_task() {
    case $1 in
    setup) ci_build_builder ;;
    build-debug) in_builder cargo test --workspace --locked --no-run ;;
    build-host)
        # The three sets of features that the jobs pgsource, mongosource and source build on the host.
        cargo test -p hune-source --locked --no-run && cargo test -p hune --locked --no-run &&
            cargo test -p hune-source --locked --features mongo-driver --no-run
        ;;
    build-binary)
        # The sources of Cargo.lock come one time here. The jobs find them in target/cargo-vendor/.
        HUNE_CI_VENDORED=0 ci_vendor && HUNE_CI_VENDORED=1 ci_build_binary "$(ci_host_triple)"
        ;;
    policy) tests/ci/policy.sh ;;
    rust) tests/ci/rust.sh ;;
    store) tests/ci/store.sh ;;
    pgsource) tests/ci/pgsource.sh ;;
    mongosource) tests/ci/mongosource.sh ;;
    api) tests/ci/api.sh ;;
    source) tests/ci/source.sh ;;
    frontend) tests/ci/frontend.sh ;;
    browser) tests/ci/browser.sh ;;
    import) tests/ci/import.sh ;;
    image) tests/ci/image.sh && image_i14 ;;
    procurauth) tests/conformance/run.sh ;;
    failover) failover ;;
    *) ci_die "no command for the task '$1'" ;;
    esac
}

now() {
    date +%s
}

# fmt_dur SECONDS : print the duration as 3m07s.
fmt_dur() {
    printf '%dm%02ds' "$(($1 / 60))" "$(($1 % 60))"
}

state_of() {
    cat "$LOGS/$1.state"
}

set_state() {
    printf '%s\n' "$2" >"$LOGS/$1.state"
}

for t in $TASKS; do
    set_state "$t" pending
done

# start_task NAME : start the task in the background. The file NAME.rc tells that it ended.
start_task() {
    st_name=$1
    set_state "$st_name" running
    st_tdir=$(target_dir_of "$st_name")
    (
        if [ -n "$st_tdir" ]; then
            CARGO_TARGET_DIR=$st_tdir
            export CARGO_TARGET_DIR
        fi
        st_t0=$(now)
        st_rc=0
        # The test `||` below switches off the option -e inside run_task: the commands of a task use &&.
        (run_task "$st_name") </dev/null >"$LOGS/$st_name.log" 2>&1 || st_rc=$?
        st_t1=$(now)
        printf '%s\n' "$((st_t1 - st_t0))" >"$LOGS/$st_name.dur"
        printf '%s\n' "$st_rc" >"$LOGS/$st_name.rc.tmp"
        mv "$LOGS/$st_name.rc.tmp" "$LOGS/$st_name.rc"
    ) &
    printf '%s\n' "$!" >"$LOGS/$st_name.pid"
    echo "all: [$(date -u +%H:%M:%S)] start  $st_name"
}

# finish_task NAME : read the result of a task that ended.
finish_task() {
    ft_name=$1
    ft_rc=$(cat "$LOGS/$ft_name.rc")
    ft_dur=$(cat "$LOGS/$ft_name.dur")
    if [ "$ft_rc" -eq 0 ]; then
        set_state "$ft_name" ok
        echo "all: [$(date -u +%H:%M:%S)] ok     $ft_name ($(fmt_dur "$ft_dur"))"
    else
        set_state "$ft_name" failed
        echo "all: [$(date -u +%H:%M:%S)] FAILED $ft_name ($(fmt_dur "$ft_dur"), exit code $ft_rc)" >&2
    fi
}

# tree_of PID : print PID and the process ids of all its descendants.
tree_of() {
    ps -A -o pid= -o ppid= | awk -v root="$1" '
        { kids[$2] = kids[$2] " " $1 }
        END {
            n = 1; q[1] = root
            for (i = 1; i <= n; i++) {
                print q[i]
                c = split(kids[q[i]], a, " ")
                for (k = 1; k <= c; k++) q[++n] = a[k]
            }
        }'
}

# running_pids : print the pid of the wrapper of each task that runs.
running_pids() {
    for rp_t in $TASKS; do
        if [ "$(state_of "$rp_t")" = running ] && [ -f "$LOGS/$rp_t.pid" ] && [ ! -f "$LOGS/$rp_t.rc" ]; then
            cat "$LOGS/$rp_t.pid"
        fi
    done
}

# stop_running SIGNAL : send the signal to each running task and to its descendants.
stop_running() {
    for sr_p in $(running_pids); do
        # shellcheck disable=SC2046
        kill "-$1" $(tree_of "$sr_p") 2>/dev/null || true
    done
}

# On a signal, stop each running task, wait a little for its own cleanup, then kill what stays.
abort_run() {
    echo "all: signal $1: stop the running tasks" >&2
    trap '' INT TERM
    stop_running TERM
    ab_i=0
    while [ "$ab_i" -lt 30 ]; do
        ab_left=0
        for ab_p in $(running_pids); do
            if kill -0 "$ab_p" 2>/dev/null; then ab_left=1; fi
        done
        [ "$ab_left" -eq 1 ] || break
        ab_i=$((ab_i + 1))
        sleep 1
    done
    stop_running KILL
    exit 130
}
trap 'abort_run INT' INT
trap 'abort_run TERM' TERM

run_start=$(now)
last_beat=$run_start
while :; do
    n_running=0
    n_open=0
    for t in $TASKS; do
        case $(state_of "$t") in
        running)
            if [ -f "$LOGS/$t.rc" ]; then
                finish_task "$t"
            else
                n_running=$((n_running + 1))
                n_open=$((n_open + 1))
            fi
            ;;
        pending)
            n_open=$((n_open + 1))
            ready=1
            dead=
            for n in $(needs_of "$t"); do
                case $(state_of "$n") in
                ok) ;;
                failed) dead=$n ;;
                *) ready=0 ;;
                esac
            done
            if [ -n "$dead" ]; then
                set_state "$t" failed
                printf '%s\n' "0" >"$LOGS/$t.dur"
                printf 'The task %s did not start: the task %s failed.\n' "$t" "$dead" >"$LOGS/$t.log"
                printf '%s\n' "not started" >"$LOGS/$t.note"
                echo "all: [$(date -u +%H:%M:%S)] FAILED $t (not started: $dead failed)" >&2
            elif [ "$ready" -eq 1 ] && { [ "$MAXJOBS" -eq 0 ] || [ "$n_running" -lt "$MAXJOBS" ]; }; then
                start_task "$t"
                n_running=$((n_running + 1))
            fi
            ;;
        esac
    done
    [ "$n_open" -gt 0 ] || break
    now_t=$(now)
    if [ "$((now_t - last_beat))" -ge 60 ]; then
        last_beat=$now_t
        run_list=
        for t in $TASKS; do
            [ "$(state_of "$t")" != running ] || run_list="$run_list $t"
        done
        echo "all: [$(date -u +%H:%M:%S)] $(fmt_dur "$((now_t - run_start))") running:$run_list"
    fi
    sleep 1
done
run_end=$(now)

# ---- The summary ----
failed_tasks=
echo
echo "all: summary (the logs are in target/ci-logs/)"
printf 'all: %-14s %-8s %s\n' task result duration
for t in $TASKS; do
    r=$(state_of "$t")
    note=
    if [ -f "$LOGS/$t.note" ]; then
        note=" ($(cat "$LOGS/$t.note"))"
    fi
    printf 'all: %-14s %-8s %s%s\n' "$t" "$r" "$(fmt_dur "$(cat "$LOGS/$t.dur")")" "$note"
    if [ "$r" != ok ]; then
        failed_tasks="$failed_tasks $t"
    fi
done
printf 'all: %-14s %-8s %s\n' total "" "$(fmt_dur "$((run_end - run_start))")"

if [ -n "$failed_tasks" ]; then
    for t in $failed_tasks; do
        echo >&2
        echo "all: ---- tail of target/ci-logs/$t.log ----" >&2
        tail -n 40 "$LOGS/$t.log" >&2
    done
    echo >&2
    echo "all: FAILED:$failed_tasks" >&2
    echo "all: run a job again with: tests/ci/all.sh --only JOB" >&2
    exit 1
fi

if [ -n "$ONLY" ]; then
    echo "all: ok (partial run: --only $ONLY; no gate record)"
    exit 0
fi
if [ -n "$FROM" ] && [ "$FROM" != policy ]; then
    echo "all: ok (partial run from $FROM; no gate record)"
    exit 0
fi
if [ "$(git rev-parse HEAD)" != "$commit" ]; then
    echo "all: ok, but the commit changed during the run; no gate record" >&2
    exit 0
fi
if [ -n "$(git status --porcelain)" ]; then
    echo "all: ok, but the work tree is not clean; no gate record" >&2
    exit 0
fi
printf '%s\n' "$commit" >"$ROOT/target/ci-all.ok"
echo "all: ok. The gate is recorded in target/ci-all.ok for commit $commit"
