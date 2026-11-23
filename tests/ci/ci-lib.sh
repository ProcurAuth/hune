# Shared helpers of the CI scripts of tests/ci (16.17). Source this file. Do not run it.
#
# Use in a job script:
#   . "$(dirname "$0")/ci-lib.sh"
#   ci_init "$@"          # sets ROOT, LOCAL (0 or 1), RUN_ID and the PATH
#   in_builder cargo test --workspace --locked
#
# Modes:
#   Docker (default): cargo runs in the builder image `hune-builder` (build/Dockerfile.build).
#   --local         : the same commands run on the host, with no Docker.
#
# Images come from tests/images.lock only. Each line gives the variable IMAGE_<NAME>.
# No script of tests/ci pushes an image or logs in to a registry (16.18).

set -eu

BUILDER_TAG=hune-builder

ci_die() {
    echo "error: $*" >&2
    exit 1
}

ci_warn() {
    echo "warning: $*" >&2
}

# ci_init [--local] : find the repository root and read the common option.
ci_init() {
    ROOT=$(cd "$(dirname "$0")/../.." && pwd)
    cd "$ROOT"
    LOCAL=0
    for ci_arg in "$@"; do
        case $ci_arg in
        --local) LOCAL=1 ;;
        -h | --help)
            grep '^#' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) ci_die "unknown option: $ci_arg (use --local or no option)" ;;
        esac
    done
    RUN_ID=${GITHUB_RUN_ID:-local$$}
    if [ -d "$HOME/.cargo/bin" ]; then
        PATH="$HOME/.cargo/bin:$PATH"
        export PATH
    fi
    export ROOT LOCAL RUN_ID
}

# ci_lock_ref NAME : print the reference of the line NAME in tests/images.lock.
ci_lock_ref() {
    ci_line=$(grep -E "^$1 " "$ROOT/tests/images.lock" | head -n 1 || true)
    [ -n "$ci_line" ] || ci_die "no line '$1' in tests/images.lock"
    case $ci_line in
    *'digest: TO-PIN'*) ci_warn "tests/images.lock: '$1' has no digest (TO-PIN)" ;;
    esac
    printf '%s\n' "$ci_line" | awk '{print $2}'
}

# ci_export_images : export IMAGE_<NAME> for each image line of tests/images.lock.
ci_export_images() {
    while read -r ci_name ci_ref ci_rest; do
        case $ci_name in '' | '#'*) continue ;; esac
        ci_var=$(printf '%s' "$ci_name" | tr 'a-z.-' 'A-Z__')
        export "IMAGE_$ci_var=$ci_ref"
    done <"$ROOT/tests/images.lock"
}

# ci_toolchain : print the channel of rust-toolchain.toml.
ci_toolchain() {
    sed -n 's/^channel *= *"\(.*\)"/\1/p' "$ROOT/rust-toolchain.toml"
}

# ci_host_triple : the musl target of the architecture of this machine.
ci_host_triple() {
    case $(uname -m) in
    x86_64 | amd64) echo x86_64-unknown-linux-musl ;;
    aarch64 | arm64) echo aarch64-unknown-linux-musl ;;
    *) ci_die "unsupported architecture $(uname -m)" ;;
    esac
}

# ci_build_builder : build the builder image of 17.2 when it does not exist.
ci_build_builder() {
    if docker image inspect "$BUILDER_TAG" >/dev/null 2>&1; then
        return 0
    fi
    ci_rust_image=$(ci_lock_ref rust)
    ci_deny=$(ci_lock_ref cargo-deny-version)
    docker build -f "$ROOT/build/Dockerfile.build" \
        --build-arg "RUST_IMAGE=$ci_rust_image" --build-arg "CARGO_DENY_VERSION=$ci_deny" \
        -t "$BUILDER_TAG" "$ROOT/build"
}

# in_builder CMD... : run a command in the builder image (Docker mode) or on the host (--local).
# The container runs as the user of the caller, so target/ stays owned by the caller.
# CARGO_HOME is under target/, which .gitignore holds. Network access is on: cargo fetches crates.
in_builder() {
    if [ "$LOCAL" -eq 1 ]; then
        "$@"
        return $?
    fi
    ci_build_builder
    mkdir -p "$ROOT/target/ci-cargo-home"
    docker run --rm --user "$(id -u):$(id -g)" \
        -v "$ROOT:/src" -w /src \
        -e HOME=/tmp -e CARGO_HOME=/src/target/ci-cargo-home \
        -e "RUSTUP_TOOLCHAIN=$(ci_toolchain)" \
        "$BUILDER_TAG" "$@"
}

# in_builder_offline CMD... : like in_builder, with the options of the second stage of 17.2.
# Sources come from target/cargo-vendor/ (see ci_vendor). The container has no network.
in_builder_offline() {
    if [ "$LOCAL" -eq 1 ]; then
        "$@"
        return $?
    fi
    ci_build_builder
    docker run --rm --network none --user "$(id -u):$(id -g)" \
        -v "$ROOT:/src" -w /src \
        -e HOME=/tmp -e CARGO_HOME=/src/target/ci-cargo-home \
        -e "RUSTUP_TOOLCHAIN=$(ci_toolchain)" \
        -e "SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)" \
        -e "HUNE_BUILD_COMMIT=$(git rev-parse HEAD)" \
        -e CARGO_INCREMENTAL=0 \
        -e CC_x86_64_unknown_linux_musl=musl-gcc -e CC_aarch64_unknown_linux_musl=musl-gcc \
        -e "RUSTFLAGS=--remap-path-prefix=/src=/hune --remap-path-prefix=/usr/local/cargo=/cargo" \
        "$BUILDER_TAG" "$@"
}

# ci_vendor : stage 1 of 17.2, with network. Fetches the sources of Cargo.lock into target/cargo-vendor/.
# The directory vendor/ of the repository holds the local copy of the crate mongodb (ID-MG25): cargo vendor
# would delete it, so the fetched sources go to target/cargo-vendor/ (a path crate is not fetched).
# The config goes to target/vendor-config.toml (the file at the root would be untracked).
ci_vendor() {
    mkdir -p "$ROOT/target"
    if [ "$LOCAL" -eq 1 ]; then
        cargo vendor --locked --versioned-dirs "$ROOT/target/cargo-vendor" >"$ROOT/target/vendor-config.toml"
    else
        in_builder sh -c 'cargo vendor --locked --versioned-dirs /src/target/cargo-vendor > target/vendor-config.toml'
    fi
}

# ci_build_binary TRIPLE [TARGET_DIR] : stage 2 of 17.2 (no network).
# The binary is TARGET_DIR/TRIPLE/release/hune. TARGET_DIR is relative to the root; the default is target.
ci_build_binary() {
    ci_triple=$1
    ci_tdir=${2:-target}
    if [ "$LOCAL" -eq 1 ]; then
        # No vendor step on the host: cargo uses its own registry cache.
        SOURCE_DATE_EPOCH=$(git log -1 --format=%ct) HUNE_BUILD_COMMIT=$(git rev-parse HEAD) \
            CARGO_INCREMENTAL=0 CC_x86_64_unknown_linux_musl=musl-gcc CC_aarch64_unknown_linux_musl=musl-gcc \
            RUSTFLAGS="--remap-path-prefix=$ROOT=/hune" \
            cargo build --release --locked --target "$ci_triple" --target-dir "$ci_tdir" -p hune
    else
        ci_vendor
        in_builder_offline cargo build --release --locked --offline --config target/vendor-config.toml \
            --target "$ci_triple" --target-dir "$ci_tdir" -p hune
    fi
}

# ci_build_image TAG : build the image of 17.5 locally. It never pushes. The context is target/ci-image-ctx.
ci_build_image() {
    ci_tag=$1
    ci_triple=$(ci_host_triple)
    ci_ctx=$ROOT/target/ci-image-ctx
    rm -rf "$ci_ctx"
    mkdir -p "$ci_ctx"
    cp "$ROOT/target/$ci_triple/release/hune" "$ci_ctx/hune"
    cp "$ROOT/build/Dockerfile" "$ci_ctx/Dockerfile"
    docker build -t "$ci_tag" "$ci_ctx"
}

# ci_compose_project JOB : print the compose project name hune-<job>-<run id> (16.1).
ci_compose_project() {
    printf 'hune-%s-%s\n' "$1" "$RUN_ID"
}

# ci_compose_run FILE SERVICE JOB : run a compose project to its end, take the exit code of SERVICE,
# print the last 200 log lines of each container on failure, remove only this project.
# The log goes to target/ci-JOB.log. Returns the exit code of SERVICE.
ci_compose_run() {
    ci_file=$1
    ci_service=$2
    ci_job=$3
    ci_proj=$(ci_compose_project "$ci_job")
    ci_log=$ROOT/target/ci-$ci_job.log
    mkdir -p "$ROOT/target"
    ci_export_images
    ci_rc=0
    docker compose -p "$ci_proj" -f "$ci_file" up --abort-on-container-exit \
        --exit-code-from "$ci_service" >"$ci_log" 2>&1 || ci_rc=$?
    cat "$ci_log"
    if [ "$ci_rc" -ne 0 ]; then
        docker compose -p "$ci_proj" -f "$ci_file" logs --tail 200 || true
    fi
    docker compose -p "$ci_proj" -f "$ci_file" down -v --remove-orphans >/dev/null 2>&1 || true
    return "$ci_rc"
}

# ---- The merge gate (16.17) ----

CI_MILESTONES="M0 M1 M2 M3a M3b M4 M5 M6 M7 MPG MMG"

# ci_ms_index MILESTONE : print the position of the milestone in the order M0..M7, MPG, MMG (MPG is after M7, MMG after MPG).
ci_ms_index() {
    ci_i=0
    for ci_m in $CI_MILESTONES; do
        if [ "$ci_m" = "$1" ]; then
            echo "$ci_i"
            return 0
        fi
        ci_i=$((ci_i + 1))
    done
    ci_die "unknown milestone '$1' in tests/ci/milestone"
}

# ci_gate_cases JOB : print the case tokens of JOB that the gate requires, with the milestone of
# tests/ci/milestone, one token for each line. A token that starts with "!" removes cases.
ci_gate_cases() {
    ci_cur=$(ci_ms_index "$(tr -d ' \n' <"$ROOT/tests/ci/milestone")")
    while read -r ci_ms ci_job ci_ids; do
        case $ci_ms in '' | '#'*) continue ;; esac
        [ "$ci_job" = "$1" ] || continue
        [ "$(ci_ms_index "$ci_ms")" -le "$ci_cur" ] || continue
        printf '%s\n' "$ci_ids" | tr ',' '\n'
    done <"$ROOT/tests/ci/gate-cases.txt"
}

# ci_case_listed CASE : read tokens on stdin; print "yes" when the gate requires the case.
ci_case_listed() {
    awk -v c="$1" '
    function num(s) { sub(/^[^0-9]*/, "", s); sub(/[^0-9].*$/, "", s); return s + 0 }
    function pre(s) { sub(/[0-9].*$/, "", s); return s }
    function match_token(t, c,    a, lo, hi, p) {
        if (t == "-" || t == "") return 0
        if (t == "*") return 1
        if (index(t, "..") > 0) {
            split(t, a, /\.\./)
            if (pre(a[1]) != pre(c)) return 0
            if (c !~ /^[A-Za-z-]+[0-9]+$/) return 0
            return (num(c) >= num(a[1]) && num(c) <= num(a[2]))
        }
        if (t ~ /\*$/) { p = substr(t, 1, length(t) - 1); return index(c, p) == 1 }
        return t == c
    }
    { t = $0
      if (substr(t, 1, 1) == "!") { if (match_token(substr(t, 2), c)) excluded = 1 }
      else if (match_token(t, c)) listed = 1 }
    END { print (listed && !excluded) ? "yes" : "no" }'
}

# ci_failed_cases LOG : print the case ids of the tests that failed in a cargo test log.
# Convention: a test name starts with the case id, for example a01_list (A01), k14a_env (K14a),
# sc_44_x (SC-44), g05_health (G05). Other tests give no case id.
ci_cases_in_log() {
    # $1 = log, $2 = ok or FAILED. The sed removes the prefix "service-1  | " of compose.
    sed 's/^[^|]*| *//' "$1" | awk -v want="$2" '
    $1 == "test" && $NF == want {
        name = $2
        sub(/^.*::/, "", name)
        if (match(name, /^(sc|sv|sp|sx)_[0-9]+/)) {
            id = toupper(substr(name, 1, 2)) "-" substr(name, 4, RLENGTH - 3)
            print id
        } else if (match(name, /^[a-zA-Z]+[0-9]+[a-z]?/)) {
            s = substr(name, 1, RLENGTH)
            if (match(s, /^[a-zA-Z]+/)) {
                letters = toupper(substr(s, 1, RLENGTH)); rest = substr(s, RLENGTH + 1)
                print letters rest
            }
        }
    }' | sort -u
}

# ci_gate_evaluate JOB LOG RUN_STATUS : the verdict of a job with gate cases.
# Fail when a required case failed or did not run, or when the run failed with no failed case.
# Other results are for information only. Returns 0 or 1.
ci_gate_evaluate() {
    ci_job=$1
    ci_logf=$2
    ci_status=$3
    ci_tokens=$ROOT/target/ci-gate-$ci_job.tokens
    mkdir -p "$ROOT/target"
    ci_gate_cases "$ci_job" >"$ci_tokens"
    ci_failed=$(ci_cases_in_log "$ci_logf" FAILED)
    ci_passed=$(ci_cases_in_log "$ci_logf" ok)
    ci_bad=0
    for ci_c in $ci_failed; do
        if [ "$(ci_case_listed "$ci_c" <"$ci_tokens")" = yes ]; then
            echo "GATE FAIL: required case $ci_c failed"
            ci_bad=1
        else
            echo "info: case $ci_c failed (not in the gate list of $(cat "$ROOT/tests/ci/milestone"))"
        fi
    done
    # A required case that did not run: only exact ids and ranges of the log can be tested.
    while read -r ci_tok; do
        case $ci_tok in '' | '-' | '!'* | '*' | *'*') continue ;; esac
        case $ci_tok in
        *..*)
            ci_p=${ci_tok%%..*}
            ci_q=${ci_tok##*..}
            ci_pp=$(printf '%s' "$ci_p" | sed 's/[0-9]*$//')
            ci_lo=$(printf '%s' "$ci_p" | sed 's/^[^0-9]*//')
            ci_hi=$(printf '%s' "$ci_q" | sed 's/^[^0-9]*//')
            ci_n=$(printf '%s' "$ci_lo" | sed 's/^0*\([0-9]\)/\1/')
            ci_hi=$(printf '%s' "$ci_hi" | sed 's/^0*\([0-9]\)/\1/')
            ;;
        *) ci_n=0 ci_hi=0 ci_pp= ci_lo= ;;
        esac
        if [ -n "$ci_pp" ]; then
            ci_width=${#ci_lo}
            while [ "$ci_n" -le "$ci_hi" ]; do
                ci_id=$(printf '%s%0*d' "$ci_pp" "$ci_width" "$ci_n")
                if ! printf '%s\n%s\n' "$ci_passed" "$ci_failed" | grep -qx "$ci_id"; then
                    echo "GATE FAIL: required case $ci_id did not run"
                    ci_bad=1
                fi
                ci_n=$((ci_n + 1))
            done
        else
            if ! printf '%s\n%s\n' "$ci_passed" "$ci_failed" | grep -qx "$ci_tok"; then
                echo "GATE FAIL: required case $ci_tok did not run"
                ci_bad=1
            fi
        fi
    done <"$ci_tokens"
    if [ "$ci_status" -ne 0 ] && [ -z "$ci_failed" ]; then
        echo "GATE FAIL: the run failed with no failed case (build or infrastructure error)"
        ci_bad=1
    fi
    if [ "$ci_bad" -eq 0 ] && [ "$ci_status" -ne 0 ]; then
        echo "info: the run has failed cases, none of them required by the gate"
    fi
    return "$ci_bad"
}
