#!/bin/sh
# Job `browser` (16.17, 16.10): Playwright tests in the test image.
# Use: tests/ci/browser.sh [--local]
# The browser tests are milestone M3b. Until tests/browser/Dockerfile and tests/compose/browser.yaml
# exist, the script prints a message and exits 0. After that it builds the test image and runs the
# compose project tests/compose/browser.yaml. That project has the service `browser-tests`, which runs
# `npx playwright test`. The project keeps its screenshots and traces where its compose file says.
# --local runs `npx playwright test` in tests/browser on the host.
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init "$@"

if [ ! -f tests/browser/Dockerfile ] || [ ! -f tests/compose/browser.yaml ]; then
    echo "M3b browser tests: not built yet (tests/browser/Dockerfile or tests/compose/browser.yaml is absent)."
    exit 0
fi

if [ "$LOCAL" -eq 1 ]; then
    (cd tests/browser && npx playwright test)
    exit $?
fi

ci_export_images
tag=hune-browser-test:$RUN_ID
docker build -t "$tag" tests/browser
export HUNE_BROWSER_TEST_IMAGE=$tag
ci_build_binary "$(ci_host_triple)"
export HUNE_BINARY="$ROOT/target/$(ci_host_triple)/release/hune"
ci_compose_run tests/compose/browser.yaml browser-tests browser
echo "browser: ok"
