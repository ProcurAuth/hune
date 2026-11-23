# Image of the capture tools of 15.14: Node and Chromium. The tools drive Chromium through the DevTools protocol
# (tests/browser/lib/cdp.mjs), not through Playwright: this repository has no Playwright and its tests run
# without a network. The compose project tests/compose/grafana.yaml builds this image.
#
# Use: docker build --build-arg NODE_IMAGE=<ref of the line "node" of tests/images.lock> -f tools/parity/Dockerfile .
ARG NODE_IMAGE
FROM ${NODE_IMAGE}
USER root
RUN apt-get update \
 && apt-get install -y --no-install-recommends chromium unzip \
 && rm -rf /var/lib/apt/lists/*
# The compose project mounts a volume here. The user node owns it, so that the capture can write its dashboards.
RUN mkdir /dashboards && chown node:node /dashboards
ENV CHROME=/usr/bin/chromium
USER node
WORKDIR /src
