# Configuration

Hune reads one HCL file. The file is the only source of the configuration. A mistake stops the program before it opens a port, and the message names the key, the expected type or range, and the value. The [configuration reference](configuration-reference.md) lists each key. The program generates it from its table of keys. Section 13 of the [specification](SPECIFICATION.md) has the full rules.

## The nine rules

| Rule | What Hune does |
| --- | --- |
| Closed schema | An unknown block, an unknown key, a duplicate key or a duplicate block stops the start. |
| Fail-fast | Each error stops the program before it serves. |
| No coercion | Hune never changes a value. The text `"5"` is not the integer `5`. A value out of range stops the start. |
| Typed once | The program parses the file one time into a typed structure. The rest of the code reads only this structure. |
| Single source | The file is the only source. Hune captures and clears the environment at start. It reads no `RUST_LOG`, no `OTEL_*` variable and no proxy variable. |
| `_env` and `_file` | A key can take its value from a variable or a file, but only when the file says so (see below). |
| Documented defaults | Seven keys have a default. Each other key is necessary. The reference lists the defaults. |
| Effective configuration | `hune print-config` prints the value and the source of each key. `serve` logs the same values at start. Secrets are hidden. |
| Contradictions | Two keys that contradict each other stop the start. |

## Values

A duration is a text with one whole number and one unit: `"500ms"`, `"30s"`, `"5m"` or `"1h"`. A size is a whole number of bytes. An integer has no sign, no fraction and no zero at the start. A boolean is `true` or `false`. A list is texts in brackets.

## Secrets and indirection

A key `<name>` that accepts the indirection has three forms. Give exactly one.

| Form | Meaning |
| --- | --- |
| `<name> = "..."` | The value is in the file. |
| `<name>_env = "VAR"` | The value is in the environment variable `VAR`. |
| `<name>_file = "/path"` | The value is the content of the file, without the last line end. |

A secret (a password or a token) accepts only `_env` and `_file`. A secret in clear text stops the start. A variable that is not set, an empty value and a file that Hune cannot read stop the start.

## The blocks

| Block | Role |
| --- | --- |
| `server` | The two listeners, the base path and the limits of the server. |
| `log` and `telemetry` | The log level and the OTLP exporter of traces. |
| `storage` | The stores: the blocks `volume "<id>"` and `postgres`. Exactly one store has `primary = true`, or the key `read_only = true` says that no store is the primary store. The primary store is the only store that accepts writes and keeps a history. |
| `access` | The mode `open` or `authzen`, with the blocks `identity` and `pdp` for `authzen`. |
| `clickhouse "<id>"`, `postgres "<id>"`, `mongodb "<id>"` | One block for each data source. At most 64 blocks for each kind. |
| `geomap` | An optional tile source for the map panel. The map is off without it. |

## A small example

This file runs Hune with a volume store, no access control and one ClickHouse source.
The keys `history` and `export` have no default. Each file MUST set both. The value `false` removes the right for each user.

```hcl
server {
  listen                  = "127.0.0.1:8080"
  ops_listen              = "127.0.0.1:9090"
  base_path               = "/"
  max_connections         = 1024
  idle_timeout            = "75s"
  shutdown_grace          = "30s"
  max_request_bytes       = 8388608
  max_result_memory_bytes = 536870912
  history                 = true
  export                  = true
}

log { level = "info" }

telemetry { enabled = false }

storage {
  volume "main" {
    path            = "/var/lib/hune/dashboards"
    primary         = true
    rescan_interval = "5s"
    history_keep    = 100
  }
}

access { mode = "open" }

clickhouse "main" {
  label              = "Main"
  url                = "http://127.0.0.1:8123"
  user               = "default"
  password_env       = "CLICKHOUSE_PASSWORD"
  database           = "default"
  connect_timeout    = "5s"
  max_execution_time = "30s"
  max_result_rows    = 100000
  max_result_bytes   = 33554432
  max_query_size     = 1048576
  max_concurrent     = 8
  queue_wait         = "5s"
}
```

Run `hune print-reference --format hcl --profile local` or `--profile distributed` to get a complete file with each key.

## The commands

| Command | Function |
| --- | --- |
| `hune check-config -c <file>` | Check the file, print each error and exit. It sends nothing on the network. |
| `hune print-config -c <file>` | Check the file and print the value and the source of each key. |
| `hune print-reference --format markdown` | Print the configuration reference from the table of keys. |
| `hune print-reference --format hcl --profile local` | Print a complete file for the topology local or distributed. |

After a change of the table of keys, run `hune print-reference --format markdown > docs/configuration-reference.md`. A test compares the file with the output of the command.
