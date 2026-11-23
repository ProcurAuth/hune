#!/bin/sh
# Fuzz targets (16.16, 16.17). Called by the job `nightly` and by hand.
# Use: tests/ci/fuzz.sh [seconds] [target ...]
#   seconds  the run time of each target. The default is 600 (10 minutes).
#   target   one or more of: lexer lexer_pg expand document_parse hcl_config grafana_import frame_decode mongo_bson_decode annotation_frame.
#            The default is all seven targets, in this order.
# For each target the script does two steps:
# 1. It replays the regression inputs of fuzz/regression/<target>/ (each file one time).
# 2. It runs `cargo +nightly fuzz run <target> -- -max_total_time=<seconds>` on the corpus of
#    fuzz/corpus/<target>/.
# The script stops at the first failure with a non-zero code. A crash input is in
# fuzz/artifacts/<target>/. Copy it to fuzz/regression/<target>/ when you fix the bug.
# The script needs `cargo-fuzz` and the nightly toolchain: see fuzz/README.md.
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT/fuzz"
if [ -d "$HOME/.cargo/bin" ]; then
    PATH="$HOME/.cargo/bin:$PATH"
    export PATH
fi

case ${1:-} in
-h | --help)
    grep '^#' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
esac

seconds=600
if [ $# -gt 0 ]; then
    case $1 in
    '' | *[!0-9]*)
        # The first argument is a target name, not a number.
        ;;
    *)
        seconds=$1
        shift
        ;;
    esac
fi
if [ "$seconds" -lt 1 ]; then
    echo "error: seconds must be 1 or more, found $seconds" >&2
    exit 2
fi

all_targets="lexer lexer_pg expand document_parse hcl_config grafana_import frame_decode mongo_bson_decode annotation_frame"
if [ $# -gt 0 ]; then
    targets=$*
else
    targets=$all_targets
fi
for t in $targets; do
    case " $all_targets " in
    *" $t "*) ;;
    *)
        echo "error: unknown target $t (use one of: $all_targets)" >&2
        exit 2
        ;;
    esac
done

if ! command -v cargo-fuzz >/dev/null 2>&1; then
    echo "error: cargo-fuzz is not installed: run 'cargo +nightly install cargo-fuzz --locked'" >&2
    exit 2
fi

# The largest input of each target, in bytes (a document has up to 256 KiB, a configuration up to 1 MiB).
max_len_of() {
    case $1 in
    document_parse | grafana_import) echo 524288 ;;
    hcl_config) echo 1048576 ;;
    *) echo 65536 ;;
    esac
}

for t in $targets; do
    mkdir -p "corpus/$t"
    max_len=$(max_len_of "$t")
    if [ -d "regression/$t" ] && [ -n "$(ls -A "regression/$t")" ]; then
        echo "fuzz: replay of regression/$t"
        cargo +nightly fuzz run "$t" "regression/$t" -- -runs=0 -max_len="$max_len" -timeout=60
    fi
    echo "fuzz: $t for ${seconds}s"
    cargo +nightly fuzz run "$t" -- -max_total_time="$seconds" -max_len="$max_len" -timeout=60 -rss_limit_mb=4096
done
echo "fuzz: ok"
