#!/bin/sh
# Job `frontend` (16.17, 16.10a): the pure functions of the browser, with golden vectors.
# Use: tests/ci/frontend.sh [--local]
#   (default) runs `node --test` in a container: the browser test image when tests/browser/Dockerfile
#             exists, else the Node image of tests/images.lock. The tests need no browser and no binary.
#   --local   runs the same command with the Node of the host.
set -eu
. "$(dirname "$0")/ci-lib.sh"
ci_init "$@"

# The glob form is on purpose: `node --test tests/frontend/` (a directory) fails on Node 22.
if [ "$LOCAL" -eq 1 ]; then
    node --test tests/frontend/*.test.mjs
elif [ -f tests/browser/Dockerfile ]; then
    tag=hune-browser-test:$RUN_ID
    docker build -t "$tag" tests/browser
    docker run --rm --user "$(id -u):$(id -g)" -v "$ROOT:/src:ro" -w /src "$tag" \
        sh -c 'node --test tests/frontend/*.test.mjs'
else
    node_image=$(ci_lock_ref node)
    docker run --rm --user "$(id -u):$(id -g)" -v "$ROOT:/src:ro" -w /src "$node_image" \
        sh -c 'node --test tests/frontend/*.test.mjs'
fi
echo "frontend: ok"
