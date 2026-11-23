#!/bin/sh
# Job `rust` (16.17): format, lint and unit tests.
# Use: tests/ci/rust.sh [--local]
#   (default) runs cargo in the builder image of 17.2.
#   --local   runs the same commands on the host, with no Docker.
# Steps: cargo fmt --all --check; cargo clippy ... -D warnings; cargo test --workspace --locked.
# The tests run the seeded random tests of 16.4 (seeds 1 to 64) and the configuration tests of 16.5.
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init "$@"

in_builder cargo fmt --all --check
in_builder cargo clippy --workspace --all-targets --locked -- -D warnings
in_builder cargo test --workspace --locked
echo "rust: ok"
