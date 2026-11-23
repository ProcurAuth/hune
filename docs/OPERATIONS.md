# Operations

This document is for the operator of Hune. Part 6 of the [specification](SPECIFICATION.md) has the full rules (sections 13 and 14).

## Run it

Hune runs as one process: `hune serve -c <file>`. The release archive has a `Dockerfile` for a local image and the folder `deploy/`. The folder holds Docker Compose files for the two topologies and Kubernetes manifests. The project publishes no container image.

| Topology | Store | Instances | Folder |
| --- | --- | --- | --- |
| Local | A volume (a folder) | One writer process | `deploy/compose/volume/` |
| Distributed | PostgreSQL | Two or more | `deploy/compose/postgres/` and `deploy/kubernetes/` |

The `deploy/compose/postgres/` example is not highly available: its one PostgreSQL is a single point of failure. The Kubernetes example uses CloudNativePG.

Before the first start, run `hune check-config -c <file>`. It finds each error that `serve` finds. Then run `hune print-config -c <file>`. It shows the value and the source of each key, and it hides secrets.

## Network

Hune has two listeners. `server.listen` serves the page and the API. `server.ops_listen` serves `/livez`, `/readyz` and `/metrics` and needs no token. Keep the ops port on a private network. Both listeners speak plain HTTP, so a proxy ends TLS.

In the mode `open`, a reverse proxy must authenticate each request. Put it in front of the whole base path. The link from the proxy to Hune must stay on a private network. `server.base_path` sets the prefix. The proxy must keep the full path.

## Health

| Route | Answer |
| --- | --- |
| `/livez` | `200` while the process runs. The command `hune health -c <file>` calls it. |
| `/readyz` | `503` before the first load of the catalog and after the stop signal, else `200`. The body is JSON. A source that is down never makes it fail. |
| `/metrics` | The Prometheus text format. |

The metrics have the prefix `hune_`. They count requests, query time and rows for each source, the queue wait, the store state and the PDP calls.

## Logs and telemetry

`serve` writes one JSON object for each line on the standard output. Each line has `ts`, `level` and `event`. A line of a request has `request_id`. The key `log.level` sets the lowest level. Hune writes an audit line `dashboard.changed` for each change of a dashboard. In the mode `authzen`, the line has the subject.

With `telemetry.enabled = true`, Hune exports traces with OTLP. Set each telemetry value in the file. Hune reads no `OTEL_*` variable.

## Limits

Each source has keys for the time, the rows, the bytes, the size of a query and the number of queries at the same time. The server has bounds for the connections, the bodies, the validations and the memory of the results in flight. A bound that is reached gives a code and, when a retry can work, `Retry-After`. Section 12.16 of the specification lists the bounds. In the mode `authzen`, the PDP can lower the limits of a source for a person.

## Stop

SIGTERM and SIGINT start a graceful stop. `/readyz` answers `503` first. Hune waits 5 seconds. It stops the accept, lets the requests in flight end for `server.shutdown_grace` and cancels the queries that remain. Give the container more time than the grace plus 5 seconds.

## Backup and restore

The command `hune export --to <dir>` writes a store as a volume layout. The command `hune import --from <dir>` reads it into an empty store. The command `hune restore --uid <uid>` brings back a deleted dashboard. The history of the page restores an old version with no stop. A volume works well with `git`. Section 14.22 of the specification has the procedures.

## Upgrade

1. Read the [changelog](../CHANGELOG.md). It names each change of a key, of the format and of the schema.
2. Run `hune check-config` of the new binary on your file.
3. Make a backup with `hune export`.
4. Replace the binary. With a volume, stop the old process first, because the lock lets one writer run.
5. When the release changes the PostgreSQL schema, stop each instance and run `hune migrate-store`. When it changes the document format, run `hune migrate-documents`.
6. Find the log line `server.ready` and read each `config.warning`.

## Find a problem

| Symptom | Check |
| --- | --- |
| The process stops at start | The message names the key, the expected type or range, and the value. Run `check-config`. |
| `409 dashboard_changed` | The document changed under the page. Reload the dashboard. |
| `503 source_busy` | The source reached `max_concurrent` or the memory bound. Look at `hune_source_in_flight`. |
| `403 ceiling_exceeded` | A result went above a ceiling of the PDP or a key of the source. The body names the limit. |
| `503 pdp_unavailable` | The PDP does not answer or the answer is not valid. Hune never grants in this case. |
| The volume does not start | Another process holds the lock. The message names the path and the holder. |
| A change on the volume does not show | The catalog polls each `storage.volume.rescan_interval`. |
