# Load test

This directory holds the load test of 16.16 of the specification. It measures the overhead of the server on the
synthetic 45-panel dashboard. The M2 exit criterion of 18.5 uses it with 5 viewers.

## Files

| File | Use |
|---|---|
| `run.sh` | Start ClickHouse and Hune, run the driver, stop everything. |
| `dashboard.mjs` | The load driver. It is the Node equivalent of `tests/load/dashboard.js`. |
| `gen.mjs` | Make the synthetic dashboard as a volume folder. |
| `config.d/limits.xml` | Raise `max_concurrent_queries` of the test ClickHouse. |

k6 is not part of this tree, because k6 needs a download. The driver `dashboard.mjs` keeps the scenario of 16.16 and
needs only Node 22. A k6 script `dashboard.js` can replace it later, with the same scenario.

## Scenario

1. The script starts a ClickHouse container on `127.0.0.1`, with the init SQL of the `source` job.
2. The script writes a volume with the folder `load` and the dashboard `load45`. The dashboard has 45 panels.
   The panels are `table` and `timeseries` in turn. Each panel runs `SELECT number FROM numbers(100)`.
3. The script writes the configuration from `hune print-reference --format hcl --profile local`. It sets the
   addresses, the volume path, the ClickHouse URL and `max_concurrent`. It removes the `tls` block.
4. The script starts the release binary with `hune serve`.
5. The driver reads `GET /api/dashboards`. It stops when the dashboard is not `valid`.
6. Baseline phase: each viewer sends the SQL straight to ClickHouse over HTTP, with the settings of 8.3.
7. Load phase: each viewer calls the panel query route of Hune (6.20).
8. In both phases, a viewer loads the 45 panels with at most 6 queries in flight. Then it refreshes every R seconds.
9. The driver reads `VmRSS` of the Hune process in `/proc/<pid>/status` every 10 s during the load phase.

By default the first load of each viewer starts at a random time in the first refresh period. Browsers are not in
step. Set `STAGGER=false` to make all viewers load at the same time.

## Use

```sh
# The M2 parameters of 18.5: 5 viewers, refresh 30 s, 5 minutes.
VIEWERS=5 REFRESH=30 DURATION=300 tests/load/run.sh m2

# The parameters of 16.16: 52 viewers, refresh 30 s, 30 minutes.
VIEWERS=52 REFRESH=30 DURATION=1800 tests/load/run.sh full
```

| Variable | Default | Meaning |
|---|---|---|
| `VIEWERS` | `52` | The number of viewers. |
| `REFRESH` | `30` | The refresh period in seconds. |
| `DURATION` | `1800` | The length of the load phase in seconds. |
| `BASELINE` | `120` | The length of the baseline phase in seconds. |
| `STAGGER` | `true` | Spread the first load of the viewers over one refresh period. |
| `HUNE_BIN` | `target/load-build/release/hune` | The binary. The script builds it when it is absent. |
| `CH_TAG` | `26.8.34.13` | The tag of the ClickHouse image. |
| `CH_PORT`, `HUNE_PORT`, `OPS_PORT` | `18323`, `18380`, `18390` | The host ports on `127.0.0.1`. |
| `DOCKER`, `NODE` | `docker`, `node` | The commands. |

The script needs Docker, Node 22, `curl` and a built binary. It writes its files in `target/load/<label>/`.
The file `<label>.json` holds all numbers. The script stops the server with `SIGTERM` and removes the container on
each path, also after an error.

## Pass criteria

| Criterion | Limit |
|---|---|
| Errors | No answer is an error, in both phases. |
| Overhead | p95 of Hune minus p95 of the direct query is at most 50 ms. |
| Memory | The growth of `VmRSS` is below 5 percent in the last two thirds of the load phase. |

The time of a query is the time at the client. It starts at the send of the request and ends at the end of the body.

The memory growth is the change between two means. The first mean is of the first 3 samples of the last part.
The second mean is of the last 3 samples.
The exit code is 0 when all criteria pass and 1 when one fails. A set-up error gives 2.

## Limits of the measure

- The client, Hune and ClickHouse run on one machine. They share the CPU. The result is an upper bound of the overhead.
- Use the release binary. A debug binary gives no valid result.
- The baseline is shorter than the load phase by default. It has fewer samples, so its p95 has more noise.
- The ClickHouse container has no CPU limit.
