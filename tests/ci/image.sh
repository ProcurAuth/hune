#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Job `image` (16.17, 16.15, 16.13 case I14): the static binary, the image and the deployment examples.
# Use: tests/ci/image.sh [--local]
#      tests/ci/image.sh --compare-i14 DIR_AMD64 DIR_ARM64
#   (default) builds the musl binary in the builder image (17.2, stage 1 with network, stage 2 without),
#             then runs the cases below with Docker. The runner builds for its own architecture
#             (ubuntu-24.04 for amd64, ubuntu-24.04-arm for arm64): no build crosses an architecture.
#             The same job has a local script: tests/ci/all.sh runs this script on your machine.
#   --local   runs the cases that need no Docker, on the host: the build (when musl-gcc exists, else the
#             host binary is used and G01 is skipped), G01, G02, G08 (when kubeconform exists) and G09
#             with the host binary and the paths rewritten under target/. Cases that need Docker are skipped.
#   --compare-i14 compares the two output directories of case I14 (one from each architecture).
#
# Cases (16.15): G01 no program interpreter; G02 `hune version`; G03 the image builds, no push;
# G04 check-config in the image; G05 serve and the health command; G06 SIGTERM and exit 0;
# G07-volume and G07-postgres: each compose example `up --wait` and GET /api/dashboards;
# G08 kubeconform -strict on deploy/kubernetes; G09-volume, G09-postgres, G09-kubernetes: check-config on each
# configuration file of deploy/ (the test secret files at the paths that the file names), and a file with
# an unknown key must fail. I14: the importer output of this architecture, kept in dist/i14-<arch>/.
#
# The test secrets come from tests/fixtures/secrets/ when it exists. Else the script makes them under
# target/ci-secrets/: text files with fixed content, the CA of tests/fixtures/config/ca-a.pem and the key set
# tests/fixtures/config/jwks.json.
#
# Verdict: a failed case fails the job only when the gate list of tests/ci/gate-cases.txt requires it at the
# milestone of tests/ci/milestone (16.17). Every other result is for information only.
set -eu
. "$(dirname "$0")/ci-lib.sh"

# ---- I14 comparison mode ----
if [ "${1:-}" = "--compare-i14" ]; then
    [ "$#" -eq 3 ] || ci_die "use: tests/ci/image.sh --compare-i14 DIR_AMD64 DIR_ARM64"
    a=$2
    b=$3
    [ -d "$a" ] && [ -d "$b" ] || ci_die "both directories must exist"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    for d in a b; do
        eval "src=\$$d"
        mkdir -p "$tmp/$d"
        (cd "$src" && find . -type f | sort) | while read -r f; do
            mkdir -p "$tmp/$d/$(dirname "$f")"
            # The member created_at is the one allowed difference (I14).
            sed 's/"created_at": *"[^"]*"/"created_at": ""/' "$src/$f" >"$tmp/$d/$f"
        done
    done
    if diff -r "$tmp/a" "$tmp/b"; then
        echo "I14: the two architectures give equal output"
        exit 0
    fi
    echo "I14: the output of the two architectures differs" >&2
    exit 1
fi

ci_init "$@"
ci_export_images

results=$ROOT/target/ci-image.results
mkdir -p "$ROOT/target"
: >"$results"

# result ID PASS|FAIL|SKIP [text]
result() {
    printf '%s %s %s\n' "$1" "$2" "${3:-}" >>"$results"
    printf '%-14s %s %s\n' "$1" "$2" "${3:-}"
}

have_docker() {
    [ "$LOCAL" -eq 0 ]
}

triple=$(ci_host_triple)
arch=amd64
case $triple in aarch64*) arch=arm64 ;; esac
tag=hune:ci-$RUN_ID
work=$ROOT/target/ci-image
rm -rf "$work"
mkdir -p "$work"
cleanup_list=
cleanup() {
    for c in $cleanup_list; do
        case $c in
        container:*) docker rm -f -v "${c#container:}" >/dev/null 2>&1 || true ;;
        esac
    done
}
trap cleanup EXIT INT TERM

# ---- Build ----
bin=$ROOT/target/$triple/release/hune
static_build=1
if [ "$LOCAL" -eq 1 ] && ! command -v musl-gcc >/dev/null 2>&1; then
    echo "image: musl-gcc is absent: --local builds the host binary; G01 is skipped"
    static_build=0
    cargo build --locked -p hune
    bin=$ROOT/target/debug/hune
else
    ci_build_binary "$triple"
fi
[ -x "$bin" ] || ci_die "the binary $bin does not exist"

# ---- Test secrets ----
secrets=$work/secrets
mkdir -p "$secrets"
# secret_source BASENAME : print the path of the source file for a secret with this base name.
secret_source() {
    case $1 in
    jwks.json | edge_jwks.json) echo "$ROOT/tests/fixtures/config/jwks.json" ;;
    *.pem | ca.crt) echo "$ROOT/tests/fixtures/config/ca-a.pem" ;;
    *) echo "" ;;
    esac
}
# make_secret DEST : write the test secret file for the base name of DEST.
make_secret() {
    base=$(basename "$1")
    mkdir -p "$(dirname "$1")"
    if [ -f "$ROOT/tests/fixtures/secrets/$base" ]; then
        cp "$ROOT/tests/fixtures/secrets/$base" "$1"
    else
        src=$(secret_source "$base")
        if [ -n "$src" ]; then
            cp "$src" "$1"
        else
            printf 'test-secret-%s' "$base" >"$1"
        fi
    fi
    chmod 0444 "$1"
}
for n in clickhouse_password clickhouse_ca.pem pg_super_password pg_owner_password pg_app_password; do
    make_secret "$secrets/$n"
done
chmod 0755 "$secrets"

# ---- G01 ----
if [ "$static_build" -eq 0 ]; then
    result G01 SKIP "not a musl build"
elif in_builder readelf -lW "target/$triple/release/hune" >"$work/readelf.txt" 2>&1; then
    if grep -qE 'INTERP|Requesting program interpreter' "$work/readelf.txt"; then
        result G01 FAIL "the binary has a program interpreter"
    else
        result G01 PASS
    fi
else
    result G01 FAIL "readelf failed"
fi

# ---- G02 ----
if ver=$("$bin" version 2>&1) && printf '%s' "$ver" | grep -q '^hune '; then
    result G02 PASS "$ver"
else
    result G02 FAIL "hune version: $ver"
fi

# ---- G03 ----
image_ok=0
if have_docker; then
    if ci_build_image "$tag" >"$work/build-image.log" 2>&1; then
        image_ok=1
        result G03 PASS "$tag (local, no push)"
    else
        tail -n 30 "$work/build-image.log" >&2
        result G03 FAIL "docker build failed"
    fi
else
    result G03 SKIP "needs Docker"
fi

volume_dir=$ROOT/deploy/compose/volume

# ---- G04 ----
if [ "$image_ok" -eq 1 ]; then
    if docker run --rm --read-only --user 65532:65532 --network none \
        -v "$volume_dir/config.hcl:/etc/hune/config.hcl:ro" -v "$secrets:/run/secrets:ro" \
        "$tag" check-config -c /etc/hune/config.hcl; then
        result G04 PASS
    else
        result G04 FAIL "check-config exits with a code other than 0"
    fi
else
    result G04 SKIP "needs the image"
fi

# ---- G05 and G06 ----
if [ "$image_ok" -eq 1 ]; then
    cname=hune-image-$RUN_ID
    cleanup_list="$cleanup_list container:$cname"
    dash=$work/dashboards
    mkdir -p "$dash"
    chmod 0777 "$dash"
    docker run -d --name "$cname" --read-only --user 65532:65532 \
        -v "$volume_dir/config.hcl:/etc/hune/config.hcl:ro" -v "$secrets:/run/secrets:ro" \
        -v "$dash:/var/lib/hune/dashboards" "$tag" >/dev/null
    healthy=0
    i=0
    while [ "$i" -lt 60 ]; do
        st=$(docker inspect -f '{{.State.Health.Status}}' "$cname" 2>/dev/null || echo none)
        if [ "$st" = healthy ]; then
            healthy=1
            break
        fi
        i=$((i + 1))
        sleep 1
    done
    if [ "$healthy" -eq 1 ] && docker exec "$cname" /hune health -c /etc/hune/config.hcl; then
        result G05 PASS "healthy after $i s"
    else
        docker logs --tail 200 "$cname" >&2 || true
        result G05 FAIL "the container is not healthy, or the health command failed"
    fi
    # G06: SIGTERM, exit 0 within shutdown_grace (20 s in the example) plus SHUTDOWN_DRAIN (5 s).
    docker kill --signal=TERM "$cname" >/dev/null 2>&1 || true
    i=0
    while [ "$i" -lt 26 ]; do
        running=$(docker inspect -f '{{.State.Running}}' "$cname" 2>/dev/null || echo false)
        [ "$running" = true ] || break
        i=$((i + 1))
        sleep 1
    done
    code=$(docker inspect -f '{{.State.ExitCode}}' "$cname" 2>/dev/null || echo unknown)
    if [ "$running" != true ] && [ "$code" = 0 ]; then
        result G06 PASS "exit 0 after $i s"
    else
        docker logs --tail 200 "$cname" >&2 || true
        result G06 FAIL "running=$running exit code=$code after $i s"
    fi
    docker rm -f -v "$cname" >/dev/null 2>&1 || true
else
    result G05 SKIP "needs the image"
    result G06 SKIP "needs the image"
fi

# ---- G07 ----
# compose_example NAME DIR EXTRA_SECRETS... : up --wait, GET /api/dashboards, down -v.
compose_example() {
    cname=$1
    srcdir=$2
    shift 2
    dir=$work/compose-$cname
    rm -rf "$dir"
    cp -R "$srcdir" "$dir"
    # The example names the tag of the release (hune:0.1.1). The test uses its own tag and never touches a real one.
    sed -i -E "s#hune:[0-9]+\.[0-9]+\.[0-9]+#$tag#g" "$dir/compose.yaml"
    mkdir -p "$dir/dashboards" "$dir/secrets"
    chmod 0777 "$dir/dashboards"
    chmod 0755 "$dir/secrets"
    for s in clickhouse_password clickhouse_ca.pem "$@"; do
        make_secret "$dir/secrets/$s"
    done
    proj=hune-image-$cname-$RUN_ID
    ok=0
    if docker compose -p "$proj" -f "$dir/compose.yaml" up --wait -d; then
        j=0
        while [ "$j" -lt 30 ]; do
            if curl -fsS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8080/api/dashboards | grep -qx 200; then
                ok=1
                break
            fi
            j=$((j + 1))
            sleep 1
        done
    fi
    if [ "$ok" -ne 1 ]; then
        docker compose -p "$proj" -f "$dir/compose.yaml" logs --tail 200 >&2 || true
    fi
    docker compose -p "$proj" -f "$dir/compose.yaml" down -v --remove-orphans >/dev/null 2>&1 || true
    [ "$ok" -eq 1 ]
}
if [ "$image_ok" -eq 1 ]; then
    if compose_example volume "$ROOT/deploy/compose/volume"; then
        result G07-volume PASS
    else
        result G07-volume FAIL "up --wait or GET /api/dashboards failed"
    fi
    if compose_example postgres "$ROOT/deploy/compose/postgres" pg_super_password pg_owner_password pg_app_password; then
        result G07-postgres PASS
    else
        result G07-postgres FAIL "up --wait or GET /api/dashboards failed"
    fi
else
    result G07-volume SKIP "needs the image"
    result G07-postgres SKIP "needs the image"
fi

# ---- G08 ----
kube_args="-strict -summary -kubernetes-version 1.32.0 -schema-location default"
# TO-PIN: pin the catalog of the CloudNativePG schemas to a commit when a maintainer sets it.
crd_loc='https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/{{.Group}}/{{.ResourceKind}}_{{.ResourceAPIVersion}}.json'
if have_docker; then
    # shellcheck disable=SC2086
    if docker run --rm -v "$ROOT/deploy/kubernetes:/k:ro" "$IMAGE_KUBECONFORM" $kube_args \
        -schema-location "$crd_loc" -ignore-filename-pattern '^$' /k; then
        result G08 PASS
    else
        result G08 FAIL "kubeconform refused a manifest"
    fi
elif command -v kubeconform >/dev/null 2>&1; then
    # shellcheck disable=SC2086
    if kubeconform $kube_args -schema-location "$crd_loc" deploy/kubernetes; then
        result G08 PASS
    else
        result G08 FAIL "kubeconform refused a manifest"
    fi
else
    result G08 SKIP "needs Docker or kubeconform"
fi

# ---- G09 ----
# extract_key FILE KEY : print the block scalar of a ConfigMap key (the lines indented by 4 spaces).
extract_key() {
    awk -v key="$2" '
        $0 ~ "^  " key ": \\|" { on = 1; next }
        on && /^  [^ ]/ { on = 0 }
        on && /^[^ ]/ { on = 0 }
        on { sub(/^    /, ""); print }' "$1"
}

# check_config_file HCL_FILE NAME : build a root tree with the file at its example path and the test secret
# files at the paths that the file names; run check-config. Returns the exit code of check-config.
check_config_file() {
    hcl=$1
    nm=$2
    tree=$work/g09-$nm
    rm -rf "$tree"
    mkdir -p "$tree/etc/hune" "$tree/run/secrets" "$tree/var/lib/hune"
    chmod -R 0755 "$tree"
    cp "$hcl" "$tree/etc/hune/config.hcl"
    # Each absolute path of a `_file` or `ca_file` attribute gets a test file.
    grep -E '(_file|ca_file)[[:space:]]*=' "$hcl" | grep -oE '"/[^"]+"' | tr -d '"' | while read -r p; do
        make_secret "$tree$p"
    done
    chmod -R a+rX "$tree"
    if have_docker; then
        docker run --rm --read-only --user 65532:65532 --network none \
            -v "$tree/etc/hune:/etc/hune:ro" -v "$tree/run/secrets:/run/secrets:ro" \
            "$tag" check-config -c /etc/hune/config.hcl
    else
        # No container: the paths move under the tree.
        sed -i "s#/run/secrets#$tree/run/secrets#g; s#/etc/hune#$tree/etc/hune#g; s#/var/lib/hune#$tree/var/lib/hune#g" \
            "$tree/etc/hune/config.hcl"
        "$bin" check-config -c "$tree/etc/hune/config.hcl"
    fi
}

# g09_case ID HCL NAME : the file must pass; the same file with an unknown key must fail.
g09_case() {
    id=$1
    hcl=$2
    nm=$3
    if [ ! -f "$hcl" ]; then
        result "$id" FAIL "$hcl is absent"
        return
    fi
    if [ "$LOCAL" -eq 0 ] && [ "$image_ok" -ne 1 ]; then
        result "$id" SKIP "needs the image"
        return
    fi
    if ! check_config_file "$hcl" "$nm" >"$work/g09-$nm.log" 2>&1; then
        cat "$work/g09-$nm.log" >&2
        result "$id" FAIL "check-config refused $hcl"
        return
    fi
    # Negative control: an unknown key in the first block must stop check-config.
    bad=$work/g09-$nm-bad.hcl
    awk 'done == 0 && /^[a-z_]+ *\{/ { print; print "  zz_unknown_key = 1"; done = 1; next } { print }' "$hcl" >"$bad"
    if check_config_file "$bad" "$nm-bad" >"$work/g09-$nm-bad.log" 2>&1; then
        result "$id" FAIL "check-config accepted a file with an unknown key"
        return
    fi
    result "$id" PASS
}

g09_case G09-volume "$ROOT/deploy/compose/volume/config.hcl" volume
g09_case G09-postgres-config "$ROOT/deploy/compose/postgres/config.hcl" pg-config
g09_case G09-postgres-migrate "$ROOT/deploy/compose/postgres/migrate.hcl" pg-migrate
extract_key "$ROOT/deploy/kubernetes/10-config.yaml" config.hcl >"$work/k8s-config.hcl"
extract_key "$ROOT/deploy/kubernetes/10-config.yaml" migrate.hcl >"$work/k8s-migrate.hcl"
g09_case G09-kube-config "$work/k8s-config.hcl" kube-config
g09_case G09-kube-migrate "$work/k8s-migrate.hcl" kube-migrate
# Group the sub-cases in the two cases of the gate list.
grp() {
    # grp ID SUB... : PASS when all PASS; FAIL when one FAIL; else SKIP.
    id=$1
    shift
    st=PASS
    for s in "$@"; do
        r=$(awk -v s="$s" '$1 == s { print $2 }' "$results" | tail -n 1)
        case $r in
        FAIL) st=FAIL ;;
        SKIP) [ "$st" = FAIL ] || st=SKIP ;;
        PASS) ;;
        *) st=FAIL ;;
        esac
    done
    result "$id" "$st" "from $*"
}
grp G09-postgres G09-postgres-config G09-postgres-migrate
grp G09-kubernetes G09-kube-config G09-kube-migrate

# ---- I14 ----
args_file=$ROOT/tests/fixtures/grafana/i14.args
if [ ! -f "$args_file" ]; then
    result I14 SKIP "tests/fixtures/grafana/i14.args is absent (importer cases start at M3a)"
elif ! "$bin" import-grafana --help >/dev/null 2>&1; then
    result I14 SKIP "this build has no import-grafana"
else
    # The args file holds one argument for each line. The text @OUT@ stands for the output directory.
    # The output goes outside the git checkout: the importer refuses a path inside it (I13).
    out=$(mktemp -d)
    set --
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        set -- "$@" "$(printf '%s' "$line" | sed "s#@OUT@#$out#g")"
    done <"$args_file"
    if "$bin" import-grafana --in "$ROOT/tests/fixtures/grafana/export" "$@" --report "$out/report" \
        >"$out/summary.txt" 2>&1 || [ "$?" -eq 1 ]; then
        rm -rf "$ROOT/dist/i14-$arch"
        mkdir -p "$ROOT/dist/i14-$arch"
        cp -R "$out"/. "$ROOT/dist/i14-$arch"/
        result I14 PASS "output of $arch in dist/i14-$arch (compare with --compare-i14)"
    else
        result I14 FAIL "import-grafana failed"
    fi
    rm -rf "$out"
fi

# ---- Verdict ----
ci_gate_cases image >"$work/gate.tokens"
bad=0
while read -r id st text; do
    listed=$(ci_case_listed "$id" <"$work/gate.tokens")
    if [ "$listed" = yes ]; then
        if [ "$st" = FAIL ]; then
            echo "GATE FAIL: required case $id failed"
            bad=1
        elif [ "$st" = SKIP ] && [ "$LOCAL" -eq 0 ]; then
            echo "GATE FAIL: required case $id was skipped: $text"
            bad=1
        fi
    elif [ "$st" = FAIL ]; then
        echo "info: case $id failed (not in the gate list of $(cat "$ROOT/tests/ci/milestone"))"
    fi
done <"$results"
[ "$bad" -eq 0 ] && echo "image: ok (milestone $(cat "$ROOT/tests/ci/milestone"))"
exit "$bad"
