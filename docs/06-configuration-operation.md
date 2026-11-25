# Part 6: Configuration and operation

Part 6 of the Hune specification (see `01-overview.md`).

This part has two sections: 13 (configuration) and 14 (observability and operation). Section 13 owns the table of the
configuration keys. The other parts name keys of this table only.

## 13. Configuration

### 13.1 One file, one source

Hune reads its configuration from one HCL file. Each subcommand that needs a configuration takes the option
`--config <path>` (short form `-c <path>`). There is no default path: a subcommand without the option stops with the
exit code 2. These subcommands are the exceptions:

- `version` and `print-reference` read no configuration (13.21).
- `copy-store` takes `--from <path>` and `--to <path>` in place of `--config` (section 5).
- `import-grafana` without `--write` takes `--config` as an option (section 15).

The file is the only source of the configuration. Hune reads an environment variable only when an attribute of the
file names it with `_env` (13.5). Hune reads a file other than the configuration file only when a key names it.

Hune reads the file one time, at start, into one typed structure. The rest of the code reads only this structure. A
change of the file has an effect only after a restart (13.26).

The engine is `hclconf`, a crate of this workspace with a `PROVENANCE` file. It is written for Hune from this section. It has these three rules:

1. A duration has exactly one integer and one unit (13.4). The texts `1m 30s`, `us` and `ns` are not valid.
2. A block can depend on a mode key, as an attribute can.
3. A boolean key can be a mode key.

The annotations of a dashboard (4.5b) add no key. Their limits are constants of the code: 8 annotations, 1,000 rows for each query, 2,000 marks for each panel and the limits of a text (4.35). The keys `max_result_rows`, `max_result_bytes`, `max_execution_time` and `max_concurrent` of a source apply to an annotation query as they apply to a panel query. A closed configuration has nothing new to check. The configuration reference does not change.

### 13.2 The nine configuration rules

| Rule | What Hune does | Test |
|---|---|---|
| 1. Closed schema | An unknown block, an unknown attribute, a duplicate attribute, a duplicate block and a duplicate label stop the start. | Unit: one case for each kind. |
| 2. Fail-fast | Each configuration error stops the program before it binds a port. The message names the key, the expected type or range, and the value (13.14). | Unit: the text of each error. |
| 3. No coercion | Hune never changes a value. The text `"5"` is not the integer `5`. `1.0`, `1e3`, `-1` and `007` are not integers. A value out of range stops the start. | Unit: each refused form. |
| 4. Typed once | `hclconf` checks the file against the key table, and Hune builds one typed structure from it. | Code review; the unit tests of the extraction. |
| 5. Single source | The environment is captured and cleared at start. No crate reads `RUST_LOG`, `OTEL_*`, a proxy variable or a CA store variable (13.23). The dependencies obey too: the crate `mongodb` is a local copy with no read, and the other reads are inert (13.23). | The environment test (13.24). The job `policy` and the test `mongosource_hello` (16.9b, 16.18). |
| 6. `_env` and `_file` | Exactly one form of each key. A secret accepts only `_env` and `_file`. The tables of 13.8 to 13.12c give the forms of each key (13.5). | Unit: two forms, no form, secret in clear text, absent variable, empty file. |
| 7. Documented defaults | Seven keys have a default value: `server.query_inspect` (default `false`), `server.repeat_max_per_panel` (default 24), `server.repeat_max_per_section` (default 12), `server.repeat_max_per_dashboard` (default 120), `server.compress_static` (default `true`), `server.transform_max_cells` (default 10000000) and `server.list_filters_min_dashboards` (default 6). The file can omit these keys. No other key has a default value. The key `clickhouse "<id>".adhoc_columns` is optional with no default: absent, no ad hoc filter applies (4.10c). The file gives each other key that applies. A key that does not apply must not be in the file. `hune print-reference` writes complete files and the reference document from the key table (13.21). | The job `policy` compares `docs/configuration-reference.md` with the output. |
| 8. Effective configuration | `hune print-config` prints the value and the source of each key, with the secrets hidden. `serve` writes the same values in its start log (13.20, 13.22). | Unit: the output for the two example files. |
| 9. Contradictions | Two keys that contradict each other stop the start (13.14). | Unit: one case for each check. |

### 13.3 Syntax of the file

The file is a subset of HCL:

- Blocks, labelled blocks (`clickhouse "prod" { }`) and attributes (`name = value`).
- Values are literals only: a text in double quotes, a whole number, `true`, `false`, or a list in brackets.
- A text has the escapes `\"`, `\\`, `\n`, `\r`, `\t` and `\uXXXX`. A text ends on its line.
- Comments start with `#` or `//`, or sit between `/*` and `*/`.
- The engine refuses expressions, operators, functions, variables, templates (`${...}` and `%{...}`), heredocs, objects
  and `null`. The error names the key and says "write a literal value".
- A text with a character that a literal never needs outside quotes (`? : + - * / % ! < > & |`) is a syntax error.

A syntax error and a duplicate attribute stop the check at once: the list then has one error. Each other error is in
the list, in the order of the file (13.25).

### 13.4 Durations, sizes, integers, booleans, texts and lists

| Kind | Syntax | Examples | Refused |
|---|---|---|---|
| Duration | A text with one whole number and one unit: `ms`, `s`, `m` or `h`. No sign, no fraction, no space, no leading zero (except `0`). | `"500ms"`, `"30s"`, `"5m"`, `"1h"`, `"0s"` | `"1m30s"`, `"1m 30s"`, `"1.5s"`, `"-1s"`, `"05s"`, `"30"`, `30`, `"30 s"`, `"1d"` |
| Size | A whole number of bytes. No unit. | `33554432` | `"32MiB"`, `32e6`, `"33554432"` |
| Integer | A whole number in decimal, with no sign, no leading zero, no fraction and no exponent. | `16`, `0` | `16.0`, `"16"`, `+16`, `016` |
| Boolean | `true` or `false`. | `true` | `"true"`, `1`, `yes` |
| Text | A text in double quotes. | `"Production"` | `Production` |
| List | Values in brackets with commas. | `["a", "b"]` | |

Some durations must be a whole number of seconds: `access.identity.max_age`, `access.identity.clock_skew` and the
`max_execution_time` of each `clickhouse`, `postgres` and `mongodb` block. For these keys, `"1500ms"` stops the start and `"2000ms"` is valid.

### 13.5 `_env` and `_file`

An attribute `<name>` that accepts the indirection has three forms:

- `<name> = "..."`: the value is in the file (form V).
- `<name>_env = "VAR"`: the value is in the environment variable `VAR` (form E).
- `<name>_file = "/path"`: the value is the content of the file at `/path`, without one last line end (form F). The last
  line end is `\n` or `\r\n`.

These rules apply:

- Give exactly one form. Two forms stop the start, and Hune then reads neither the variable nor the file.
- No form of a necessary key stops the start.
- A secret accepts only E and F. A secret in clear text stops the start, also together with another form. The error
  never shows the value.
- A key that does not accept the indirection refuses `<name>_env` and `<name>_file`.
- A variable that is not set, or whose value is empty, stops the start. A value that starts or ends with a space or a
  line end stops the start. A value that is not UTF-8 stops the start.
- A file that Hune cannot read stops the start. A file content that is empty or not UTF-8 also stops the start. So
  does a content of more than 65536 bytes, or with a byte order mark.
- The name of the variable has `A-Z`, `a-z`, `0-9` and `_`, and does not start with a digit. The names `SSL_CERT_FILE`
  and `SSL_CERT_DIR` stop the start (13.23).
- The error gives the attribute and the variable or the path, never the value.
- `print-config` shows the source of each value: `file`, `env:VAR` or `file:/path` (13.20).

These keys accept the indirection:

| Key | Forms | Secret |
|---|---|---|
| `storage.postgres.user` | V, E, F | no |
| `storage.postgres.password` | E, F | yes |
| `access.identity.public_keys` | V, E, F | no |
| `access.pdp.url` | V, E, F | no |
| `access.pdp.token` | E, F | yes |
| `telemetry.otlp_endpoint` | V, E, F | no |
| `clickhouse "<id>".user` | V, E, F | no |
| `clickhouse "<id>".password` | E, F | yes |
| `postgres "<id>".user` | V, E, F | no |
| `postgres "<id>".password` | E, F | yes |
| `mongodb "<id>".user` | V, E, F | no |
| `mongodb "<id>".password` | E, F | yes |

Each other key accepts only the form V.

### 13.6 Paths

- Each path is absolute: the value of a `_file` attribute, each key whose name ends in `ca_file`, and
  `path` of a block `storage.volume`. A relative path stops the start.
- A key whose name ends in `ca_file` is a path key, not an indirection. Hune reads the file as a PEM file (13.13).
- A path has 1 to 4096 bytes and no NUL byte.
- `_file` values and CA files are regular files. Hune opens their last part with `O_NOFOLLOW`.
- Section 5 gives the rules of `path` of a volume (absolute, canonical, no symbolic link).

### 13.7 Blocks

| Block | Count | Necessary when | Forbidden when |
|---|---|---|---|
| `server` | 1 | always | never |
| `log` | 1 | always | never |
| `telemetry` | 1 | always | never |
| `storage` | 1 | always | never |
| `storage.volume "<id>"` | 0 to 16 | always | never |
| `storage.postgres` | 0 or 1 | no `storage.volume` block exists | never |
| `storage.postgres.tls` | 0 or 1 | `storage.postgres.sslmode = "verify-full"` | `storage.postgres.sslmode = "disable"` |
| `access` | 1 | always | never |
| `access.identity` | 0 or 1 | `access.mode = "authzen"` | `access.mode = "open"` |
| `access.pdp` | 0 or 1 | `access.mode = "authzen"` | `access.mode = "open"` |
| `clickhouse "<id>"` | 0 to 64 (1 to 64 source blocks in all, C14) | no condition | never |
| `postgres "<id>"` | 0 to 64 (1 to 64 source blocks in all, C14) | no condition | never |
| `mongodb "<id>"` | 0 to 64 (1 to 64 source blocks in all, C14) | no condition | never |
| `clickhouse "<id>".tls` | 0 or 1 | the `url` of the block has the scheme `https` | the `url` of the block has the scheme `http` |
| `postgres "<id>".tls` | 0 or 1 | `sslmode = "verify-full"` of the block | `sslmode = "disable"` of the block |
| `mongodb "<id>".tls` | 0 or 1 | `tls_mode = "verify-full"` of the block | `tls_mode = "disable"` of the block |
| `geomap` | 0 or 1 | never (the block is optional) | never |

The tables of 13.8 to 13.12c are the complete key table. These rules apply to each row:

- Seven keys have a default value: `server.query_inspect`, the three keys `server.repeat_max_per_*`, `server.compress_static`, `server.transform_max_cells` and `server.list_filters_min_dashboards`. The key `clickhouse "<id>".adhoc_columns` is optional with no default (13.12). Each other key that applies is necessary. A key that does not apply is forbidden.
- "Forms" uses V, E and F of 13.5.
- "Necessary when" and "Forbidden when" give the mode. A row with "always" has no mode. The keys of a block with a mode
  follow the mode of the block.
- "Reference value" is the value that `hune print-reference` writes (13.21). It is not a default.

### 13.8 Key table: server

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value |
|---|---|---|---|---|---|---|
| `server.listen` | IP address and port, canonical form, such as `"0.0.0.0:8080"`. The main listener (14.2). | no | V | always | never | `"0.0.0.0:8080"` |
| `server.ops_listen` | IP address and port, canonical form. The listener of the probes and the metrics (14.2). | no | V | always | never | `"0.0.0.0:9090"` |
| `server.base_path` | Text of 1 to 128 bytes: `/`, or a path that starts and ends with `/`, with segments of 1 to 64 characters from `A-Z a-z 0-9 . _ ~ -`, not `.` or `..`. The prefix of the routes of `server.listen` (section 6). | no | V | always | never | `"/"` |
| `server.query_inspect` | Boolean. The value `true` adds the expanded SQL to the answer of a panel query. The page shows the menu item Inspect. In the mode `authzen`, only a subject with `source.query` and `dashboard.write` gets the SQL (11.21). Make sure that you accept this exposure: the SQL holds the values of the variables. | no | V | optional, default `false` | never | `false` |
| `server.repeat_max_per_panel` | Integer, 1 to 200. The most copies of one panel that repeats by a variable (4.13a, 9.8c). Above it, the page shows a message and no copy. The upper bound is `MAX_PANELS` (4.35). | no | V | optional, default `24` | never | `24` |
| `server.repeat_max_per_section` | Integer, 1 to 50. The most copies of one section that repeats by a variable. The upper bound is `MAX_SECTIONS` (4.35). | no | V | optional, default `12` | never | `12` |
| `server.repeat_max_per_dashboard` | Integer, 1 to 200. The most copies of panels that repeat in one dashboard. The upper bound is `MAX_PANELS` (4.35). Contradictions C28 and C29 compare it with the two keys above. | no | V | optional, default `120` | never | `120` |
| `server.compress_static` | Boolean. The value `true` sends the gzip copy of a static file of a text type to a request that accepts gzip (9.3). Hune makes the copies one time at start (14.12). An API answer, the shell and an error are never compressed (12.8a). | no | V | optional, default `true` | never | `true` |
| `server.transform_max_cells` | Integer, 1 to 100000000. The most cells (fields times rows) of the frame that one step of a transform makes in the page (4.35, 9.21a, rule 13). Above it, the panel shows an error and no frame. The session gives the value to the page (6.10), and the page keeps no copy of it. The upper bound is fixed. One cell takes 9 bytes or more: a double and a null flag. Thus 100000000 cells take 900 MB or more, near the memory of one browser tab. | no | V | optional, default `10000000` | never | `10000000` |
| `server.list_filters_min_dashboards` | Integer, 0 to 1000. The list page shows the sort control and the tag filter only when the user sees more dashboards than this number (9.7). With this number or fewer, the page hides them. The value 0 shows them for any list that is not empty. The session gives the value to the page (6.10), and the page keeps no copy of it. The upper bound is fixed at 1000. | no | V | optional, default `6` | never | `6` |
| `server.max_connections` | Integer, 16 to 65536. The most open connections on `server.listen`. | no | V | always | never | `1024` |
| `server.idle_timeout` | Duration, 1s to 10m. Hune closes a kept connection that is idle for this time. | no | V | always | never | `"75s"` |
| `server.shutdown_grace` | Duration, 5s to 10m. The longest graceful stop (14.13). | no | V | always | never | `"30s"` |
| `server.max_request_bytes` | Size, from the document limit of section 4 plus 65536 to 67108864. The body limit of a dashboard create, save or import (12.16). | no | V | always | never | `8388608` |
| `server.max_result_memory_bytes` | Size, 16777216 to 68719476736. The memory budget of the results in flight (12.16). | no | V | always | never | `536870912` |

With the document limit of 4 MiB of section 4, the lowest `server.max_request_bytes` is 4259840.

### 13.9 Key table: log and telemetry

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value |
|---|---|---|---|---|---|---|
| `log.level` | `"error"`, `"warn"`, `"info"` or `"debug"`. The lowest level that Hune writes (14.6). | no | V | always | never | `"info"` |
| `telemetry.enabled` | Boolean. `true` exports the spans with OTLP (14.9). | no | V | always | never | `false` |
| `telemetry.otlp_endpoint` | URL with the scheme `http` or `https`, a host and an optional port. No path, query, fragment or user info. Hune sends to `<endpoint>/v1/traces`. | no | V, E, F | `telemetry.enabled = true` | `telemetry.enabled = false` | `"http://otel-collector:4318"` |
| `telemetry.ca_file` | Absolute path of a PEM file (13.13). | no | V | `telemetry.enabled = true` and an `https` endpoint | `telemetry.enabled = false`, or an `http` endpoint | absent (the reference endpoint is `http`) |
| `telemetry.service_name` | Text of 1 to 64 bytes of printable ASCII. The resource attribute `service.name`. | no | V | `telemetry.enabled = true` | `telemetry.enabled = false` | `"hune"` |
| `telemetry.export_interval` | Duration, 1s to 5m. | no | V | `telemetry.enabled = true` | `telemetry.enabled = false` | `"15s"` |

The OTLP exporter is part of Hune (14.9, M7). With `telemetry.enabled = true`, the three other keys of the block are
necessary. `telemetry.enabled = false` is valid in each build.

Both reference files (13.16, 13.17) have `enabled = false`. The reference values of the three other keys are the
values of the example of 14.9.

### 13.10 Key table: storage

The block `storage` holds the stores. A volume store is a block `storage.volume "<id>"`: the label is the id of the store. The PostgreSQL store is the block `storage.postgres`, and its id is `postgres`. The file has one store at least (C2).

The primary store is the store with `primary = true`. At most one store has it (C10). It is the only store that accepts a write (section 5). A file with no primary store has no edit. Each write route answers `403 editing_disabled`, the page shows no edit control, and Hune publishes no `dashboard.write` and no `source.query` (11.18).

Each other store is read-only, and its `title` is the name of its root folder in the page. The primary store has no `title` (C30) and it is the only store with `history_keep` (C31).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value |
|---|---|---|---|---|---|---|
| `storage.volume "<id>".path` | Absolute and canonical path of a directory, 1 to 4096 bytes (section 5). The paths of two volumes are different, and no path holds another one (C32). | no | V | always | never | `"/var/lib/hune/dashboards"` (local), absent (distributed) |
| `storage.volume "<id>".title` | Text of 1 to 128 bytes, no control character. The name of the root folder of the store in the page (C30, C34). | no | V | the store is not the primary store | the store is the primary store | `"Lab"` (shown in the reference document only) |
| `storage.volume "<id>".primary` | Boolean. `true` makes this store the primary store (C10). | no | V | never (the key is optional) | another store has `primary = true` | `true` (local), absent (distributed) |
| `storage.volume "<id>".rescan_interval` | Duration, 1s to 1h. The period of the scan for changes made outside Hune. | no | V | always | never | `"5s"` (local), absent (distributed) |
| `storage.volume "<id>".history_keep` | Integer, 1 to 10000. The versions kept for each dashboard (C31). | no | V | the store is the primary store | the store is not the primary store | `100` (local), absent (distributed) |
| `storage.postgres.host` | A DNS name of 1 to 253 bytes, or an IP address. | no | V | always | never | `"hune-pg-rw"` |
| `storage.postgres.port` | Integer, 1 to 65535. | no | V | always | never | `5432` |
| `storage.postgres.database` | Text of 1 to 63 bytes, no control character. | no | V | always | never | `"hune"` |
| `storage.postgres.user` | Text of 1 to 63 bytes, no control character. | no | V, E, F | always | never | `user_file = "/run/secrets/pg/username"` |
| `storage.postgres.password` | Text of 1 to 1024 bytes. | yes | E, F | always | never | `password_file = "/run/secrets/pg/password"` |
| `storage.postgres.sslmode` | `"verify-full"` or `"disable"`. `verify-full` checks the certificate and the name (12.14). | no | V | always | never | `"verify-full"` |
| `storage.postgres.tls.ca_file` | Absolute path of a PEM file (13.13). | no | V | `sslmode = "verify-full"` | `sslmode = "disable"` | `"/run/secrets/pg-ca/ca.crt"` |
| `storage.postgres.tls.server_name` | A DNS name of 1 to 253 bytes. The name that the certificate must hold. | no | V | `sslmode = "verify-full"` | `sslmode = "disable"` | `"hune-pg-rw"` |
| `storage.postgres.max_connections` | Integer, 2 to 64. The size of the connection pool. | no | V | always | never | `8` |
| `storage.postgres.connect_timeout` | Duration, 1s to 60s. | no | V | always | never | `"5s"` |
| `storage.postgres.statement_timeout` | Duration, 100ms to 60s. Also `idle_in_transaction_session_timeout` (section 5). | no | V | always | never | `"10s"` |
| `storage.postgres.poll_interval` | Duration, 1s to 5m. The period of the poll of the catalog (section 5). | no | V | always | never | `"5s"` |
| `storage.postgres.title` | Text of 1 to 128 bytes, no control character. The name of the root folder of the store in the page (C30, C34). | no | V | the store is not the primary store | the store is the primary store | `"Archive"` (shown in the reference document only) |
| `storage.postgres.primary` | Boolean. `true` makes this store the primary store (C10). | no | V | never (the key is optional) | another store has `primary = true` | `true` (distributed), absent (local) |
| `storage.postgres.history_keep` | Integer, 1 to 10000. The versions kept for each dashboard (C31). | no | V | the store is the primary store | the store is not the primary store | `100` (distributed), absent (local) |

The label of a volume matches `^[a-z0-9][a-z0-9-]{0,39}$`, and it is not `postgres` (C33). A file has 16 volumes at most. Each value of a store that is not a key of 13.2, rule 7 has no default.

### 13.11 Key table: access

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value |
|---|---|---|---|---|---|---|
| `access.mode` | `"open"` or `"authzen"` (11.2). | no | V | always | never | `"open"` (local), `"authzen"` (distributed) |
| `access.identity.header` | HTTP header name of 1 to 64 token characters (RFC 9110). Not `Authorization`, `Proxy-Authorization`, `Cookie`, `Host`, `Content-Type` or `Content-Length`, in any case. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `"X-Identity-Token"` |
| `access.identity.issuer` | Text of 1 to 1024 bytes, no control character. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `"https://edge.example"` |
| `access.identity.audience` | An id of P 4.1: 1 to 256 bytes of UTF-8, no control character. The service id (11.14). | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `"dashboards"` |
| `access.identity.public_keys` | PEM or JWK Set with 1 to 16 keys (11.9). | no | V, E, F | `access.mode = "authzen"` | `access.mode = "open"` | `public_keys_file = "/run/secrets/edge/jwks.json"` |
| `access.identity.max_age` | Duration, 1s to 24h, whole seconds. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `"5m"` |
| `access.identity.clock_skew` | Duration, 0s to 5m, whole seconds. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `"1m"` |
| `access.pdp.url` | URL with the scheme `http` or `https`, a host, an optional port and an optional path. No user info, query or fragment. | no | V, E, F | `access.mode = "authzen"` | `access.mode = "open"` | `"https://pdp.example:4182"` |
| `access.pdp.token` | Text of 1 to 4096 bytes, no space and no control character. | yes | E, F | `access.mode = "authzen"` | `access.mode = "open"` | `token_file = "/run/secrets/pdp/token"` |
| `access.pdp.plain_http` | Boolean. Must agree with the scheme of `access.pdp.url`. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `false` |
| `access.pdp.ca_file` | Absolute path of a PEM file (13.13). | no | V | `access.pdp.plain_http = false` | `access.pdp.plain_http = true`, or `access.mode = "open"` | `"/run/secrets/pdp/ca.pem"` |
| `access.pdp.timeout` | Duration, 10ms to 60s. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `"500ms"` |
| `access.pdp.cache_ttl` | Duration, 0s to 1h. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `"30s"` |
| `access.pdp.max_connections` | Integer, 1 to 32. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `4` |

### 13.12 Key table: clickhouse

The block `clickhouse "<id>"` declares one ClickHouse server (section 8). The file has 0 to 64 such blocks, and 1 to 64 source blocks in all (13.12a, 13.14). The label
`<id>` matches `^[a-z0-9][a-z0-9-]{0,39}$`. It is unique, it is the source id of the documents, and it is the ProcurAuth
node `source:<id>` (11.15). In this table, `<id>` stands for the label.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value |
|---|---|---|---|---|---|---|
| `clickhouse "<id>".label` | Text of 1 to 60 characters, no control character. The label in the page and in the action document. | no | V | always | never | `"Production"` |
| `clickhouse "<id>".url` | URL with the scheme `http` or `https`, a host and an optional port. No path, query, fragment or user info. The HTTP interface of ClickHouse. | no | V | always | never | `"https://clickhouse:8443"` |
| `clickhouse "<id>".user` | Text of 1 to 256 bytes, no control character. | no | V, E, F | always | never | `"hune"` |
| `clickhouse "<id>".password` | Text of 1 to 1024 bytes. | yes | E, F | always | never | `password_file = "/run/secrets/clickhouse/password"` |
| `clickhouse "<id>".database` | A ClickHouse identifier: `^[A-Za-z_][A-Za-z0-9_]{0,63}$`. The default database of the queries. | no | V | always | never | `"analytics"` |
| `clickhouse "<id>".tls.ca_file` | Absolute path of a PEM file (13.13). | no | V | an `https` URL | an `http` URL | `"/run/secrets/clickhouse/ca.pem"` |
| `clickhouse "<id>".tls.server_name` | A DNS name of 1 to 253 bytes. The name that the certificate must hold. | no | V | an `https` URL | an `http` URL | `"clickhouse"` |
| `clickhouse "<id>".connect_timeout` | Duration, 100ms to 60s. | no | V | always | never | `"5s"` |
| `clickhouse "<id>".max_execution_time` | Duration, 1s to 1h, whole seconds (11.24, section 8). | no | V | always | never | `"1m"` |
| `clickhouse "<id>".max_result_rows` | Integer, 1 to 1000000000. | no | V | always | never | `100000` |
| `clickhouse "<id>".max_result_bytes` | Size, 1048576 to 1073741824. | no | V | always | never | `33554432` |
| `clickhouse "<id>".max_query_size` | Size, 262144 to 16777216. The largest expanded SQL, also sent as the setting `max_query_size` (section 7). | no | V | always | never | `1048576` |
| `clickhouse "<id>".max_concurrent` | Integer, 1 to 1024. The most queries at one time on this source. | no | V | always | never | `16` |
| `clickhouse "<id>".queue_wait` | Duration, 0s to 60s. The longest wait for a place (12.16). | no | V | always | never | `"5s"` |
| `clickhouse "<id>".max_concurrent_per_subject` | Integer, 1 to 1024, at most `max_concurrent`. The most queries at one time for one subject on this source. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `6` |
| `clickhouse "<id>".adhoc_columns` | List of 1 to 256 texts. Each item matches `^[A-Za-z_][A-Za-z0-9_]{0,63}$`, and no item is repeated. The columns that an ad hoc filter of this source can name (4.10c, 7.7a, 7.10a). The key is optional and has no default: absent, no ad hoc filter applies on this source. Read the exposure note of 12.9 before you list a column. | no | V | never (the key is optional) | never | `["service", "host", "env"]` |

### 13.12a Key table: postgres

The block `postgres "<id>"` declares one PostgreSQL server for panel queries (section 8a). It is not the storage backend of 13.10. The file has 0 to 64 such blocks. The label `<id>` follows the rule of 13.12, and it is unique across both kinds of source (C15). In this table, `<id>` stands for the label.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value |
|---|---|---|---|---|---|---|
| `postgres "<id>".label` | Text of 1 to 60 characters, no control character. The label in the page and in the action document. | no | V | always | never | `"Metrics"` |
| `postgres "<id>".host` | A DNS name of 1 to 253 bytes, or an IP address. No Unix socket path. The one host that Hune connects to (8a.4). | no | V | always | never | `"hune-pg-ro"` |
| `postgres "<id>".port` | Integer, 1 to 65535. | no | V | always | never | `5432` |
| `postgres "<id>".database` | Text of 1 to 63 bytes, no control character. The database of the connection. | no | V | always | never | `"analytics"` |
| `postgres "<id>".user` | Text of 1 to 63 bytes, no control character. The read-only role of 8a.13. | no | V, E, F | always | never | `user_file = "/run/secrets/postgres-metrics/username"` |
| `postgres "<id>".password` | Text of 1 to 1024 bytes. | yes | E, F | always | never | `password_file = "/run/secrets/postgres-metrics/password"` |
| `postgres "<id>".search_path` | Text of 1 to 256 bytes: 1 to 8 names separated by `,`, with no blank. A name matches `^[a-z_][a-z0-9_]{0,62}$` and does not start with `pg_temp`. Hune sends it as the session value (8a.4). | no | V | always | never | `"metrics"` |
| `postgres "<id>".sslmode` | `"verify-full"` or `"disable"`. `verify-full` checks the certificate and the name (12.14). | no | V | always | never | `"verify-full"` |
| `postgres "<id>".tls.ca_file` | Absolute path of a PEM file (13.13). | no | V | `sslmode = "verify-full"` | `sslmode = "disable"` | `"/run/secrets/postgres-metrics-ca/ca.crt"` |
| `postgres "<id>".tls.server_name` | A DNS name of 1 to 253 bytes. The name that the certificate must hold. | no | V | `sslmode = "verify-full"` | `sslmode = "disable"` | `"hune-pg-ro"` |
| `postgres "<id>".connect_timeout` | Duration, 100ms to 60s. The connection, TLS, login and setup check (8a.4). | no | V | always | never | `"5s"` |
| `postgres "<id>".max_execution_time` | Duration, 1s to 1h, whole seconds (11.24, section 8a). | no | V | always | never | `"1m"` |
| `postgres "<id>".max_result_rows` | Integer, 1 to 1000000000. | no | V | always | never | `100000` |
| `postgres "<id>".max_result_bytes` | Size, 1048576 to 1073741824. | no | V | always | never | `33554432` |
| `postgres "<id>".max_query_size` | Size, 262144 to 16777216. The largest expanded SQL (section 7). Hune checks it itself. | no | V | always | never | `1048576` |
| `postgres "<id>".max_concurrent` | Integer, 1 to 256. The most queries at one time on this source, and the size of its pool (8a.9). | no | V | always | never | `16` |
| `postgres "<id>".queue_wait` | Duration, 0s to 60s. The longest wait for a place (12.16). | no | V | always | never | `"5s"` |
| `postgres "<id>".max_concurrent_per_subject` | Integer, 1 to 256, at most `max_concurrent`. The most queries at one time for one subject on this source. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `6` |

Only the seven keys of 13.2, rule 7, have a default value. The reference values appear in `docs/configuration-reference.md` only. The reference files of 13.16 and 13.17 hold no `postgres` block, so `print-reference` writes none. The keys `user` and `password` name a read-only role (8a.13). Contradiction C20 stops a source that uses the role of the store on the same server.

An example block, with the keys in the order of the table:

```hcl
postgres "metrics" {
  label                      = "Metrics"
  host                       = "hune-pg-ro"
  port                       = 5432
  database                   = "analytics"
  user_file                  = "/run/secrets/postgres-metrics/username"
  password_file              = "/run/secrets/postgres-metrics/password"
  search_path                = "metrics"
  sslmode                    = "verify-full"
  connect_timeout            = "5s"
  max_execution_time         = "1m"
  max_result_rows            = 100000
  max_result_bytes           = 33554432
  max_query_size             = 1048576
  max_concurrent             = 16
  queue_wait                 = "5s"
  max_concurrent_per_subject = 6

  tls {
    ca_file     = "/run/secrets/postgres-metrics-ca/ca.crt"
    server_name = "hune-pg-ro"
  }
}
```

This block gives no warning. It needs `access.mode = "authzen"` for the key `max_concurrent_per_subject`.

### 13.12b Key table: mongodb

The block `mongodb "<id>"` declares one MongoDB server or one replica set for panel queries (section 8b). The file has 0 to 64 such blocks. The label `<id>` follows the rule of 13.12, and it is unique across the three kinds of source (C15). In this table, `<id>` stands for the label. The keys `host` and `port` apply to the topology `direct`, and the keys `hosts` and `replica_set` apply to the topology `replica_set`. No key sets a connection string, a mechanism, a read concern or a write concern, and no key permits the scheme `mongodb+srv`.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value |
|---|---|---|---|---|---|---|
| `mongodb "<id>".label` | Text of 1 to 60 characters, no control character. The label in the page and in the action document. | no | V | always | never | `"Events"` |
| `mongodb "<id>".topology` | `"direct"` or `"replica_set"`. `direct`: one host, no discovery. `replica_set`: the complete list of the members (8b.4). | no | V | always | never | `"direct"` |
| `mongodb "<id>".host` | A DNS name of 1 to 253 bytes, or an IP address. No Unix socket path. The one host that Hune connects to (8b.4). | no | V | `topology = "direct"` | `topology = "replica_set"` | `"hune-mongo-ro"` |
| `mongodb "<id>".port` | Integer, 1 to 65535. | no | V | `topology = "direct"` | `topology = "replica_set"` | `27017` |
| `mongodb "<id>".hosts` | List of 2 to 7 texts. Each text has 1 to 262 bytes, with no blank and no control character, and has the form `host:port`. The members of the replica set, all of them (8b.4). | no | V | `topology = "replica_set"` | `topology = "direct"` | `["mongo-0:27017", "mongo-1:27017", "mongo-2:27017"]` |
| `mongodb "<id>".replica_set` | Text that matches `^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$`. The name of the replica set. | no | V | `topology = "replica_set"` | `topology = "direct"` | `"rs0"` |
| `mongodb "<id>".user` | Text of 1 to 128 bytes, no control character. The read-only user of 8b.13. | no | V, E, F | always | never | `user_file = "/run/secrets/mongodb-events/username"` |
| `mongodb "<id>".password` | Text of 1 to 1024 bytes. | yes | E, F | always | never | `password_file = "/run/secrets/mongodb-events/password"` |
| `mongodb "<id>".auth_database` | Text that matches `^[A-Za-z0-9_-]{1,63}$`. The database that holds the user. | no | V | always | never | `"admin"` |
| `mongodb "<id>".databases` | List of 1 to 8 texts. Each text matches `^[A-Za-z0-9_-]{1,63}$`, and is not `admin`, `local` or `config`. The databases that a query can read (7.33). | no | V | always | never | `["analytics"]` |
| `mongodb "<id>".collections` | List of 1 to 64 texts. Each text matches `^[A-Za-z0-9_][A-Za-z0-9_.-]{0,99}$` and does not start with `system.`. The collections that a query can name (7.33). | no | V | always | never | `["node_load", "node_events"]` |
| `mongodb "<id>".read_preference` | `"primary"`, `"primaryPreferred"`, `"secondary"`, `"secondaryPreferred"` or `"nearest"`. The read preference of each command (8b.5). | no | V | always | never | `"secondaryPreferred"` |
| `mongodb "<id>".tls_mode` | `"verify-full"` or `"disable"`. `verify-full` checks the certificate and the host name (12.14). | no | V | always | never | `"verify-full"` |
| `mongodb "<id>".tls.ca_file` | Absolute path of a PEM file (13.13). | no | V | `tls_mode = "verify-full"` | `tls_mode = "disable"` | `"/run/secrets/mongodb-events-ca/ca.crt"` |
| `mongodb "<id>".connect_timeout` | Duration, 100ms to 60s. The connection, TLS, login and server selection (8b.4). | no | V | always | never | `"5s"` |
| `mongodb "<id>".max_execution_time` | Duration, 1s to 1h, whole seconds (11.24, section 8b). | no | V | always | never | `"1m"` |
| `mongodb "<id>".max_result_rows` | Integer, 1 to 1000000000. | no | V | always | never | `100000` |
| `mongodb "<id>".max_result_bytes` | Size, 1048576 to 1073741824. | no | V | always | never | `33554432` |
| `mongodb "<id>".max_query_size` | Size, 262144 to 8388608. The largest BSON size of the expanded command (7.35). Hune checks it itself. | no | V | always | never | `1048576` |
| `mongodb "<id>".max_concurrent` | Integer, 1 to 256. The most queries at one time on this source (8b.9). | no | V | always | never | `16` |
| `mongodb "<id>".queue_wait` | Duration, 0s to 60s. The longest wait for a place (12.16). | no | V | always | never | `"5s"` |
| `mongodb "<id>".max_concurrent_per_subject` | Integer, 1 to 256, at most `max_concurrent`. The most queries at one time for one subject on this source. | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `6` |

Only the seven keys of 13.2, rule 7, have a default value. The keys `read_preference`, `collections` and `topology` have no default. The reference values appear in `docs/configuration-reference.md` only. The reference files of 13.16 and 13.17 hold no `mongodb` block, so `print-reference` writes none. The generated reference gets its rows for this block from the key table in the code, as for the other blocks.

An example block, with the keys in the order of the table:

```hcl
mongodb "events" {
  label                      = "Events"
  topology                   = "direct"
  host                       = "hune-mongo-ro"
  port                       = 27017
  user_file                  = "/run/secrets/mongodb-events/username"
  password_file              = "/run/secrets/mongodb-events/password"
  auth_database              = "admin"
  databases                  = ["analytics"]
  collections                = ["node_load", "node_events"]
  read_preference            = "secondaryPreferred"
  tls_mode                   = "verify-full"
  connect_timeout            = "5s"
  max_execution_time         = "1m"
  max_result_rows            = 100000
  max_result_bytes           = 33554432
  max_query_size             = 1048576
  max_concurrent             = 16
  queue_wait                 = "5s"
  max_concurrent_per_subject = 6

  tls {
    ca_file = "/run/secrets/mongodb-events-ca/ca.crt"
  }
}
```

This block gives no warning. It needs `access.mode = "authzen"` for the key `max_concurrent_per_subject`. The certificate of the server MUST hold the name `hune-mongo-ro` (8b.4).

### 13.12c Key table: geomap

The block `geomap` gives the tile source of the panel type `geomap` (4.33g, 9.19i). The file has 0 or 1 such block, with no label. Without the block, the feature is off. The page contacts no tile host, and the map draws its markers on a plain background. The policy of 12.8 is then the value of its table, byte for byte. With the block, each key is necessary, and each key accepts the form V only.

The values are public: the session gives them to each page (6.10).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value |
|---|---|---|---|---|---|---|
| `geomap.tile_url` | Text of 16 to 512 bytes of printable ASCII, with the grammar below. The template of the URL of a tile. | no | V | the block exists | never | `"https://tiles.example.org/osm/{z}/{x}/{y}.png"` |
| `geomap.attribution` | Text of 1 to 200 characters, with no control character, no bidirectional control, no zero-width character and no line or paragraph separator (the list of 7.37). The page shows it with textContent, with no link. | no | V | the block exists | never | `"Map tiles: example.org"` |
| `geomap.max_zoom` | Integer, 0 to 22. The highest zoom of the map. The lowest zoom is 0. | no | V | the block exists | never | `19` |

The grammar of `tile_url` (13.13):

1. The text starts with `https://`. Another scheme is refused.
2. The host is a DNS name in lower case with one dot or more, of 253 bytes at most. A label has 1 to 63 bytes of `a-z 0-9 -`, with no `-` at the start or at the end. A host whose last label has digits only is refused, so an IPv4 address is refused. An IPv6 address in brackets, user info (`@`) and a placeholder in the host are refused.
3. An optional port follows the host: `:` and a whole number from 1 to 65535, with no zero at the start.
4. The path starts with `/`. It holds `{z}`, `{x}` and `{y}` exactly once each. Its other characters are `A-Z a-z 0-9 . _ ~ -` and `/`. A `%`, another brace, an empty segment and a segment `.` or `..` are refused. Thus `{s}`, `{r}`, `{-y}` and a list of hosts are refused.
5. An optional query follows the path: `?` and 1 or more characters of `A-Z a-z 0-9 . _ ~ = & -`, with no placeholder. A fragment (`#`) is refused.

A key of a provider in the query is public: each viewer can read it in the session and in the requests of the tiles. Use a key that the provider limits to the origin of Hune, or your own tile server.

An example block:

```hcl
geomap {
  tile_url    = "https://tiles.example.org/osm/{z}/{x}/{y}.png"
  attribution = "Map tiles: example.org"
  max_zoom    = 19
}
```

This block gives the warning W9 (13.15). The policy of 12.8 gets the source `https://tiles.example.org/osm/` in `img-src` only. `print-config` and the start log show it on the line `geomap.img_src` (13.20, 13.22).

Privacy note. With the block, the browser of each viewer contacts the tile host. The host gets the address of the viewer, the `User-Agent`, the `Accept-Language`, the time and the rate of the requests. It also gets the origin of Hune and the coordinates of the tiles, which tell the area of the map. Hune sends no cookie, no `Referer`, no token and no value of the data.

An IP address is personal data: read the terms of the provider. Your own tile server removes the third party. To turn the feature off, remove the block and restart. 12.8b gives the complete note.

The reference files of 13.16 and 13.17 hold no block `geomap`, so `print-reference` writes none. The reference values appear in `docs/configuration-reference.md` only.

### 13.13 Checks of values after the key table

`hclconf` checks the type, the range and the form of each key. Hune then checks these values:

| Value | Check |
|---|---|
| A CA file (`access.pdp.ca_file`, `storage.postgres.tls.ca_file`, `clickhouse "<id>".tls.ca_file`, `postgres "<id>".tls.ca_file`, `mongodb "<id>".tls.ca_file`, `telemetry.ca_file`) | A regular file of 1 MiB at most, with 1 to 64 PEM blocks `CERTIFICATE`. Each block parses as an X.509 certificate. Other text outside the blocks stops the start. |
| `access.identity.public_keys` | The rules of 11.9. |
| `access.identity.audience` | 1 to 256 bytes of UTF-8, no control character. |
| The label of a `clickhouse`, `postgres` or `mongodb` block | `^[a-z0-9][a-z0-9-]{0,39}$`. |
| `postgres "<id>".search_path` | 1 to 8 names, each one matches `^[a-z_][a-z0-9_]{0,62}$`, none starts with `pg_temp`, and no name is repeated. The names are separated by `,` with no blank. |
| `postgres "<id>".host` | A DNS name or an IP address. A text that starts with `/` is refused, so no key can name a Unix socket. |
| `mongodb "<id>".host`, and each item of `mongodb "<id>".hosts` | A DNS name or an IP address. A text that starts with `/`, or that holds `://`, `@`, `?` or a blank, is refused, so no key can name a Unix socket, a connection string or the scheme `mongodb+srv`. An item of `hosts` is `host:port`, with an IPv6 address in brackets and a port from 1 to 65535. A port with a leading zero is refused. |
| `mongodb "<id>".hosts`, `databases`, `collections` | A list holds no duplicate item. A comparison of names is exact and case-sensitive. A host name is compared without regard to case. Two items of `hosts` are equal when their hosts are equal and their ports are equal. Two IP addresses are equal when they have the same value. |
| `clickhouse "<id>".adhoc_columns` | Each item matches `^[A-Za-z_][A-Za-z0-9_]{0,63}$`: a plain column name, with no dot and no map access. No item is repeated, and the comparison is exact and case-sensitive. A bad item gives `expected an item that matches ^[A-Za-z_][A-Za-z0-9_]{0,63}$, found "x"`. A repeated item gives `expected a list with no repeated item, found "x" more than once`. |
| A build without the cargo feature `mongo-driver` | Stage 2 refuses each `mongodb` block with the error `mongodb "<id>": this build has no MongoDB driver`. The error has the line of the block. The modes of `migrate-store` and `import-grafana` do not make this check (13.18). |
| `server.base_path` | The rule of 13.8. |
| A URL | `url`, `access.pdp.url` and `telemetry.otlp_endpoint` parse as absolute URLs in canonical form, with the parts that their row permits. |
| A whole number of seconds | `access.identity.max_age`, `access.identity.clock_skew` and `max_execution_time` of each source block (13.4). |
| The PEP configuration and the action document | In the mode `authzen`, Hune builds the `PepConfig` of 11.10 and calls `PepConfig::check`. It builds the action document of 11.18 with `ActionDocumentBuilder::build`. Neither call uses the network. |
| `geomap.tile_url` | The grammar of 13.12c. The error has the form `expected <rule> (<reason>), found <value>`. The reason names the first fault, for example `the host is an IP address`. Stage 2 parses the text one time, and the policy of 12.8 and the session use the parts. |
| `geomap.attribution` | 1 to 200 characters, and none of the characters of the list of 7.37, item 9. |

Stage 2 gives these messages for a list of a `mongodb` block. A bad item gives `expected an item that matches <rule>, found "x"`. A repeated item gives `expected a list with no repeated item, found "x" more than once`. The error of stage 1 for `hosts` is `expected a list of 2 to 7 texts, each of 1 to 262 bytes with no space and no control character, found <value>`.

The values that come from E and F get the same checks after Hune reads them. These checks are stage 2 of 13.19, so
`check-config`, `print-config` and `serve` find the same errors.

### 13.14 Errors and contradiction checks

Each error is one line in this form:

```text
<key path>: <message> (line <n>)
```

The key path has the form of `print-config`, for example `clickhouse "prod".max_result_rows`. The part `(line <n>)` is
absent when the error has no line, for example an error of the whole file. A contradiction gives the paths of its keys,
with a comma between them. The message never holds the value of a secret: it shows `(value hidden)`.

The errors of the engine:

| Error | Message |
|---|---|
| The file is not valid HCL | `the configuration file is not valid HCL at line <l>, column <c>: <detail>` |
| The file is too large | `the configuration file has <n> bytes; the maximum is 1048576 bytes` |
| Unknown attribute or block | `unknown attribute`, `unknown block` |
| Duplicate attribute, block or label | `duplicate attribute; the first one is at line <n>`, `duplicate block; the first one is at line <n>`, `duplicate label; the first block with this label is at line <n>` |
| Necessary attribute absent | `missing attribute; expected <type and range>` |
| Necessary key with forms absent | ``missing attribute; give `<name>`, `<name>_env` or `<name>_file` (<type and range>)`` |
| Necessary secret absent | ``missing secret; give `<name>_env` or `<name>_file` `` |
| Necessary block absent | `missing block` |
| Count of labelled blocks | ``expected 0 to 64 blocks `<kind> "<label>"`, found <n>`` |
| Wrong type or out of range | `expected <type and range>, found <value>` |
| Not a literal | `expected <type>, found <what>: <text>; write a literal value` |
| Two forms | ``the file gives `<form 1>` at line <n> and `<form 2>`; give exactly one form`` |
| Secret in clear text | ``a secret accepts only `<name>_env` or `<name>_file`, but the file gives it in clear text (value hidden)`` |
| Indirection on a key that refuses it | ``<name>` accepts only a value in the file, not `_env` or `_file` `` |
| Variable absent or bad | `the environment variable <VAR> is not set`, `the value of the environment variable <VAR> is empty` |
| File of `_file` | `cannot read the file <path>: <reason>`, `the file <path> is empty` |
| Key or block forbidden by the mode | ``not permitted when `<mode key>` is <value>; the key is only for `<mode key>` = <values>`` |
| Block absent for the mode | ``missing block; expected when `<mode key>` is <value>`` |

The contradiction checks of Hune. In the message, `` `key` (value) `` stands for each key with the value of the file.

| # | Keys | Rule | Message |
|---|---|---|---|
| C1 | `access.mode`, `access.identity`, `access.pdp` | The two blocks exist only in the mode `authzen`. | The mode errors of the engine (two last rows of the table above). |
| C2 | `storage.volume`, `storage.postgres` | The file has at least one store. This check is in stage 3 of 13.19. | ``storage.volume, storage.postgres: the file has no store; give at least one block `storage.volume "<id>"` or the block `storage.postgres` `` |
| C3 | `telemetry.enabled` and its three other keys | The keys exist only with `enabled = true`. | The mode errors of the engine. |
| C4 | `storage.postgres.sslmode`, `storage.postgres.tls` | The block exists only with `verify-full`. | The mode errors of the engine. |
| C5 | `access.pdp.plain_http`, `access.pdp.url` | `false` needs an `https` URL, and `true` needs an `http` URL. | ``access.pdp.plain_http, access.pdp.url: `access.pdp.plain_http` (false) needs the scheme https, but `access.pdp.url` has the scheme "http"``; and ``access.pdp.plain_http, access.pdp.url: `access.pdp.plain_http` (true) needs the scheme http, but `access.pdp.url` has the scheme "https"`` |
| C6 | `access.pdp.plain_http`, `access.pdp.ca_file` | `ca_file` exists only with `plain_http = false`. | The mode errors of the engine, with the mode key `access.pdp.plain_http`. |
| C7 | `clickhouse "<id>".url`, `clickhouse "<id>".tls` | An `https` URL needs the block `tls`, and an `http` URL forbids it. | ``clickhouse "prod".url, clickhouse "prod".tls: `clickhouse "prod".url` ("https://clickhouse:8443") needs the block tls`` |
| C8 | `telemetry.otlp_endpoint`, `telemetry.ca_file` | An `https` endpoint needs `ca_file`, and an `http` endpoint forbids it. | ``telemetry.otlp_endpoint, telemetry.ca_file: `telemetry.otlp_endpoint` ("http://otel-collector:4318") has the scheme http and forbids `telemetry.ca_file` ("/run/ca.pem")`` |
| C9 | `server.listen`, `server.ops_listen` | The two addresses have different ports, or two different hosts that are not wildcards. | ``server.listen, server.ops_listen: `server.listen` ("0.0.0.0:8080") and `server.ops_listen` ("127.0.0.1:8080") use the same port on overlapping hosts`` |
| C10 | `storage.volume "<id>".primary`, `storage.postgres.primary` | At most one store has `primary = true`. The error names the first store and each later store. | ``storage.volume "lab".primary, storage.volume "main".primary: `storage.volume "lab".primary` (true) and `storage.volume "main".primary` (true): at most one store has `primary = true` `` |
| C11 | `server.max_result_memory_bytes`, `clickhouse "<id>".max_result_bytes` | The budget is at least 2 times the `max_result_bytes` of each source. | ``server.max_result_memory_bytes, clickhouse "prod".max_result_bytes: `server.max_result_memory_bytes` (33554432) is less than 2 x `clickhouse "prod".max_result_bytes` (33554432)`` |
| C12 | `clickhouse "<id>".max_concurrent_per_subject`, `clickhouse "<id>".max_concurrent` | The value for each subject is at most `max_concurrent`. | ``clickhouse "prod".max_concurrent_per_subject, clickhouse "prod".max_concurrent: `clickhouse "prod".max_concurrent_per_subject` (20) is more than `clickhouse "prod".max_concurrent` (16)`` |
| C13 | `access.mode`, `clickhouse "<id>".max_concurrent_per_subject` | The key exists only in the mode `authzen`. | The mode errors of the engine. |
| C14 | `clickhouse`, `postgres`, `mongodb` | The file has 1 to 64 source blocks in all, of the three kinds. This check is in stage 3 of 13.19. | ``clickhouse, postgres, mongodb: expected 1 to 64 blocks `clickhouse "<label>"`, `postgres "<label>"` or `mongodb "<label>"` in all, found <n>`` |
| C15 | `postgres "<id>"`, `clickhouse "<id>"`, `mongodb "<id>"` | A label is unique across the three kinds. The first block in the file owns the label, and the error names the later block. This check is in stage 3 of 13.19. | ``postgres "prod": duplicate label; the first block with this label is `clickhouse "prod"` at line <n> (line <m>)`` |
| C16 | `postgres "<id>".sslmode`, `postgres "<id>".tls` | The block `tls` exists only with `verify-full`. | ``postgres "x".tls: missing block; expected when `postgres "x".sslmode` is "verify-full"`` and ``postgres "x".tls: not permitted when `postgres "x".sslmode` is "disable"; the key is only for `postgres "x".sslmode` = "verify-full"``. |
| C17 | `server.max_result_memory_bytes`, `postgres "<id>".max_result_bytes` | The budget is at least 2 times the `max_result_bytes` of each source. | ``server.max_result_memory_bytes, postgres "metrics".max_result_bytes: `server.max_result_memory_bytes` (33554432) is less than 2 x `postgres "metrics".max_result_bytes` (33554432)`` |
| C18 | `postgres "<id>".max_concurrent_per_subject`, `postgres "<id>".max_concurrent` | The value for each subject is at most `max_concurrent`. | ``postgres "metrics".max_concurrent_per_subject, postgres "metrics".max_concurrent: `postgres "metrics".max_concurrent_per_subject` (20) is more than `postgres "metrics".max_concurrent` (16)`` |
| C19 | `access.mode`, `postgres "<id>".max_concurrent_per_subject` | The key exists only in the mode `authzen`. | The mode errors of the engine. |
| C20 | `postgres "<id>".user`, `storage.postgres.user` | When the block `storage.postgres` exists and the host and the port of the source equal those of `storage.postgres`, the two users differ. | ``postgres "metrics".user, storage.postgres.user: `postgres "metrics".user` ("app") equals `storage.postgres.user` on the same host and port; a source needs its own read-only role`` |
| C21 | `server.query_inspect`, `access.mode`, `storage` | In the mode `authzen`, the value `true` needs a primary store, because the right to see the SQL is `source.query` with `dashboard.write`. | ``server.query_inspect, access.mode, storage: `server.query_inspect` (true) in the mode `authzen` needs a primary store (`primary = true` on one store), because the right to see the SQL is `source.query` with `dashboard.write`; found no primary store`` |
| C22 | `mongodb "<id>".tls_mode`, `mongodb "<id>".tls` | The block `tls` exists only with `verify-full`. | ``mongodb "x".tls: missing block; expected when `mongodb "x".tls_mode` is "verify-full"`` and ``mongodb "x".tls: not permitted when `mongodb "x".tls_mode` is "disable"; the key is only for `mongodb "x".tls_mode` = "verify-full"``. |
| C23 | `server.max_result_memory_bytes`, `mongodb "<id>".max_result_bytes` | The budget is at least 2 times the `max_result_bytes` of each source. | ``server.max_result_memory_bytes, mongodb "events".max_result_bytes: `server.max_result_memory_bytes` (33554432) is less than 2 x `mongodb "events".max_result_bytes` (33554432)`` |
| C24 | `mongodb "<id>".max_concurrent_per_subject`, `mongodb "<id>".max_concurrent` | The value for each subject is at most `max_concurrent`. | ``mongodb "events".max_concurrent_per_subject, mongodb "events".max_concurrent: `mongodb "events".max_concurrent_per_subject` (20) is more than `mongodb "events".max_concurrent` (16)`` |
| C25 | `access.mode`, `mongodb "<id>".max_concurrent_per_subject` | The key exists only in the mode `authzen`. | The mode errors of the engine. |
| C26 | `mongodb "<id>".topology`, `host`, `port`, `hosts`, `replica_set` | The keys `host` and `port` exist only with `direct`. The keys `hosts` and `replica_set` exist only with `replica_set`. | The mode errors of the engine, with the mode key `mongodb "<id>".topology`. |
| C27 | `mongodb "<id>".databases`, `mongodb "<id>".auth_database` | The list `databases` holds none of `admin`, `local` and `config`. The key `auth_database` can be `admin`. | ``mongodb "events".databases: expected a name that is not "admin", "local" or "config", found "admin" (line <n>)`` |
| C28 | `server.repeat_max_per_panel`, `server.repeat_max_per_dashboard` | The limit of a panel is at most the limit of the dashboard. An absent key counts with its default. | ``server.repeat_max_per_panel, server.repeat_max_per_dashboard: `server.repeat_max_per_panel` (30) is more than `server.repeat_max_per_dashboard` (20): a repeat of one panel or one section cannot have more copies than the dashboard`` |
| C29 | `server.repeat_max_per_section`, `server.repeat_max_per_dashboard` | The limit of a section is at most the limit of the dashboard. An absent key counts with its default. | The message of C28 with `server.repeat_max_per_section`. |
| C30 | `storage.volume "<id>".title`, `storage.postgres.title`, `primary` | A store that is not the primary store has a `title`. The primary store has none. | ``storage.volume "main".title, storage.volume "main".primary: `storage.volume "main".title` is necessary: the store is read-only and the title is the name of its root folder`` and ``... `storage.volume "main".title` ("Main") is not permitted on the primary store; the primary store shows at the root of the page`` |
| C31 | `history_keep`, `primary` | The primary store has `history_keep`. A store that is not the primary store has none. | ``storage.volume "lab".history_keep, storage.volume "lab".primary: `storage.volume "lab".history_keep` (7) is not permitted: only the primary store has history`` and ``... `storage.volume "main".history_keep` is necessary: the store is the primary store`` |
| C32 | `storage.volume "<id>".path` | The paths of two volumes are different, and no path holds another one. A path that only starts with the text of another path is a different path. | ``storage.volume "lab".path, storage.volume "main".path: `storage.volume "lab".path` ("/var/lib/hune/dashboards") and `storage.volume "main".path` ("/var/lib/hune/dashboards"): the second is the same path``; the other texts end with `the second holds the first path` and `the second is inside the first path` |
| C33 | `storage.volume "<id>"` | The label is not `postgres`: this id is the id of the PostgreSQL store. | ``storage.volume "postgres": the label "postgres" is the id of the PostgreSQL store; give another label`` |
| C34 | `storage.volume "<id>".title`, `storage.postgres.title` | The titles of two stores that are not the primary store are different. | ``storage.volume "a".title, storage.volume "b".title: `storage.volume "a".title` ("Lab") and `storage.volume "b".title` ("Lab") name the same root folder`` |

The checks C26 and C27 are in stage 3 of 13.19. The checks C2, C10, C21 and C30 to C34 are in stage 3 too. They use the values of the file only, and no read mode skips them. A key that the topology needs and that the block lacks gives `missing attribute; expected <type and range>`, with the line of the block. A key of the other topology gives the mode error, with the line of the key. C27 gives one error for each refused name, with the line of the key `databases`.

The checks C17, C20 and C23 need the values of `user`, `password` and the memory budget. C20 skips a read mode that does not read the values of `user` (13.18).

In C15, `<n>` is the line of the first block, and `<m>` is the line of the later block that has the error. In C20, two hosts are the same when they are equal without regard to case, or when both are IP addresses with the same value. The check never resolves a name.

An error of `PepConfig::check` (13.13) has the form `<key path>: expected <expected>, found <value>`. Its key path is
the key of 13.11 that gives the field of 11.10, for example `access.identity.header` for `token_header`. An error of
the action document has the key path `access.identity.audience` or `clickhouse "<id>".label`.

A wildcard host is `0.0.0.0` or `::`. Two hosts overlap when they are equal, or when one of them is a wildcard of the
same address family. The other value errors of 13.13 use the message form `expected <rule>, found <value>`. Example:
`clickhouse "Prod": expected a label that matches ^[a-z0-9][a-z0-9-]{0,39}$, found "Prod" (line 40)`.

### 13.15 Warnings

A warning never stops the start. `serve` writes each warning as a log line `config.warning` with the fields `key` and
`message` (14.7). `check-config` and `print-config` write each warning on the standard error as
`warning: <key>: <message>`.

| # | Condition | Key | Message |
|---|---|---|---|
| W1 | `access.mode = "open"` | `access.mode` | `access.mode is open: Hune checks no identity and asks no PDP. A proxy that authenticates the users must guard each route of server.listen.` |
| W2 | `access.mode = "authzen"` and the host of `server.listen` is a wildcard | `server.listen` | `server.listen has a wildcard host: make sure that only the edge reaches this port (ProcurAuth 9.4).` |
| W3 | `access.pdp.plain_http = true` | `access.pdp.plain_http` | `access.pdp.plain_http is true: the PDP token and the questions go without TLS.` |
| W4 | A `clickhouse` URL with the scheme `http` and a host that is not a loopback address | `clickhouse "<id>".url` | `clickhouse "<id>".url has no TLS to a host that is not a loopback address.` |
| W5 | `storage.postgres.sslmode = "disable"` and a host that is not a loopback address | `storage.postgres.sslmode` | `storage.postgres.sslmode is disable for a host that is not a loopback address.` |
| W6 | `telemetry.otlp_endpoint` with the scheme `http` and a host that is not a loopback address | `telemetry.otlp_endpoint` | `telemetry.otlp_endpoint has no TLS to a host that is not a loopback address.` |
| W7 | `postgres "<id>".sslmode = "disable"` and a host that is not a loopback address | `postgres "<id>".sslmode` | `postgres "<id>".sslmode is disable for a host that is not a loopback address.` |
| W8 | `mongodb "<id>".tls_mode = "disable"` and a host that is not a loopback address. In the topology `replica_set`, one such host in `hosts` is enough. | `mongodb "<id>".tls_mode` | `mongodb "<id>".tls_mode is disable for a host that is not a loopback address.` |
| W9 | The block `geomap` exists | `geomap.tile_url` | `geomap.tile_url is set: the browser of each viewer contacts <origin>. That host sees the address of the viewer and the area of the map.` The origin is `https://`, the host and the port. |

A loopback address is `localhost`, an address of `127.0.0.0/8`, or `::1`.

### 13.16 Example file LOCAL

This is the output of `hune print-reference --format hcl --profile local`: the volume backend, the mode `open` and no
telemetry. The file of 17.6 starts from this file and sets `server.base_path = "/dashboards/"`.

```hcl
# Hune configuration, profile local: volume storage, access mode open, no telemetry.
# Each key that applies is in the file, except the seven keys that have a default: `server.query_inspect`, `server.repeat_max_per_panel`, `server.repeat_max_per_section`, `server.repeat_max_per_dashboard`, `server.compress_static`, `server.transform_max_cells` and `server.list_filters_min_dashboards`.

server {
  listen                  = "0.0.0.0:8080"
  ops_listen              = "0.0.0.0:9090"
  base_path               = "/"
  max_connections         = 1024
  idle_timeout            = "75s"
  shutdown_grace          = "30s"
  max_request_bytes       = 8388608
  max_result_memory_bytes = 536870912
}

log {
  level = "info"
}

telemetry {
  enabled = false
}

storage {
  volume "main" {
    path            = "/var/lib/hune/dashboards"
    primary         = true
    rescan_interval = "5s"
    history_keep    = 100
  }
}

access {
  mode = "open"
}

clickhouse "prod" {
  label              = "Production"
  url                = "https://clickhouse:8443"
  user               = "hune"
  password_file      = "/run/secrets/clickhouse/password"
  database           = "analytics"
  connect_timeout    = "5s"
  max_execution_time = "1m"
  max_result_rows    = 100000
  max_result_bytes   = 33554432
  max_query_size     = 1048576
  max_concurrent     = 16
  queue_wait         = "5s"

  tls {
    ca_file     = "/run/secrets/clickhouse/ca.pem"
    server_name = "clickhouse"
  }
}
```

This file gives one warning: W1.

### 13.17 Example file DISTRIBUTED

This is the output of `hune print-reference --format hcl --profile distributed`: the PostgreSQL backend, the mode
`authzen` and no telemetry. The block `telemetry` of 14.9 can replace the block of this file. The PostgreSQL names are those of a CloudNativePG cluster `hune-pg` (section 5).

```hcl
# Hune configuration, profile distributed: PostgreSQL storage, access mode authzen, no telemetry.
# Each key that applies is in the file, except the seven keys that have a default: `server.query_inspect`, `server.repeat_max_per_panel`, `server.repeat_max_per_section`, `server.repeat_max_per_dashboard`, `server.compress_static`, `server.transform_max_cells` and `server.list_filters_min_dashboards`.

server {
  listen                  = "0.0.0.0:8080"
  ops_listen              = "0.0.0.0:9090"
  base_path               = "/"
  max_connections         = 1024
  idle_timeout            = "75s"
  shutdown_grace          = "30s"
  max_request_bytes       = 8388608
  max_result_memory_bytes = 536870912
}

log {
  level = "info"
}

telemetry {
  enabled = false
}

storage {
  postgres {
    host              = "hune-pg-rw"
    port              = 5432
    database          = "hune"
    user_file         = "/run/secrets/pg/username"
    password_file     = "/run/secrets/pg/password"
    sslmode           = "verify-full"
    max_connections   = 8
    connect_timeout   = "5s"
    statement_timeout = "10s"
    poll_interval     = "5s"
    primary           = true
    history_keep      = 100

    tls {
      ca_file     = "/run/secrets/pg-ca/ca.crt"
      server_name = "hune-pg-rw"
    }
  }
}

access {
  mode = "authzen"

  identity {
    header           = "X-Identity-Token"
    issuer           = "https://edge.example"
    audience         = "dashboards"
    public_keys_file = "/run/secrets/edge/jwks.json"
    max_age          = "5m"
    clock_skew       = "1m"
  }

  pdp {
    url             = "https://pdp.example:4182"
    token_file      = "/run/secrets/pdp/token"
    ca_file         = "/run/secrets/pdp/ca.pem"
    plain_http      = false
    timeout         = "500ms"
    cache_ttl       = "30s"
    max_connections = 4
  }
}

clickhouse "prod" {
  label                      = "Production"
  url                        = "https://clickhouse:8443"
  user                       = "hune"
  password_file              = "/run/secrets/clickhouse/password"
  database                   = "analytics"
  connect_timeout            = "5s"
  max_execution_time         = "1m"
  max_result_rows            = 100000
  max_result_bytes           = 33554432
  max_query_size             = 1048576
  max_concurrent             = 16
  queue_wait                 = "5s"
  max_concurrent_per_subject = 6

  tls {
    ca_file     = "/run/secrets/clickhouse/ca.pem"
    server_name = "clickhouse"
  }
}
```

This file gives one warning: W2, because `server.listen` has a wildcard host. In a Kubernetes pod, the wildcard host is
necessary. The operator applies the network rule with a NetworkPolicy of the cluster (section 17 gives none).

### 13.18 The subcommands that read a configuration

| Subcommand | Reads `_env` and `_file` | Checks of section 13 | Other work |
|---|---|---|---|
| `serve` | yes | all | Start checks of the store and of the listeners, and the role check of 12.13 (14.12). |
| `check-config` | yes | all | None (13.19). |
| `print-config` | yes | all | Prints the effective configuration (13.20). |
| `health` | no; each such value is `<not read>` | all, except the checks of the values that it does not read | `GET /livez` on `server.ops_listen` (14.5). |
| `migrate-store`, `migrate-documents`, `copy-store`, `export`, `import`, `restore` | only the values of the block `storage`; each other such value is `<not read>` | the key table and 13.14, except the checks of the values that it does not read; the checks of 13.13 for the block `storage` only | Section 4 and section 5. `copy-store` reads two files. |
| `import-grafana` with `--write` | only the values of the block `storage`; each other such value is `<not read>` | the key table and 13.14, except the checks of the values that it does not read; the checks of 13.13 for the block `storage` and for the labels of the `clickhouse`, `postgres` and `mongodb` blocks only | Section 15. It uses the labels and the kinds of the source blocks. A label and a kind are in the file, so it reads no `_env` or `_file` value of these blocks. |
| `print-reference`, `version` | no file | none | 13.21; `version` prints the version and the commit. |

These subcommands skip each check of 13.13 for the blocks that they do not use, for example the CA file of a
`clickhouse` block. Thus the Job of `migrate-store` needs only the files of the block `storage` (17.8). No subcommand of this table
contacts a network before its checks of section 13 pass. `check-config` and `print-config`
never contact a network, never bind a port and never open the volume.

### 13.19 check-config

```text
hune check-config --config <path>
```

- It does each check of section 13. These are the syntax, the key table, the forms, the reads of E and F, the checks
  of 13.13 and the contradictions.
- It writes nothing on the standard output.
- It writes each error on the standard error, one for each line, in the form of 13.14. It writes all the errors of a
  stage, not only the first.
- It writes each warning on the standard error, in the form of 13.15.

The checks run in three stages: the file against the key table, the checks of 13.13, then the contradictions. A stage
runs only when the stage before it has no error.

| Exit code | Meaning |
|---|---|
| 0 | The file is valid. Warnings can be on the standard error. |
| 1 | The file has one or more errors. |
| 2 | The command line is not valid, for example no `--config`. |

### 13.20 print-config

```text
hune print-config --config <path>
```

`print-config` does the checks of `check-config`. With an error, it does what `check-config` does and exits with 1. Else
it writes the effective configuration on the standard output and exits with 0.

The output has one line for each key that has a value, in the order of the key table. The labelled blocks come in the
order of the key table. The `clickhouse` blocks come first, then the `postgres` blocks, then the `mongodb` blocks. Each kind keeps the order of the file. Each line has this form:

```text
<key path> = <value>  # <source>
```

- A text is in double quotes, with the escapes of JSON.
- An integer is in decimal. A boolean is `true` or `false`. A list is in brackets.
- A duration is in its canonical form: one integer and the largest unit that gives a whole number. Thus `"60s"` shows as
  `"1m"`, and `"1500ms"` shows as `"1500ms"`.
- A secret shows as `<hidden>`.
- `access.identity.public_keys` shows as a list with the type and the `kid` of each key (11.9).
- A key with the forms E or F shows its own name, without `_env` or `_file`.
- The source is `file` (the value is in the configuration file), `env:<VAR>`, `file:<path>` or `default`. A key that the file omits and that has a default shows with the source `default`.
- The key `clickhouse "<id>".adhoc_columns` is optional and has no default. When the file omits it, no value applies, and the output and the start log have no line for it. No ad hoc filter then applies on that source (4.10c).
- The block `geomap` adds one line after `geomap.max_zoom`: `geomap.img_src = "<value>"  # derived`. The value is the value of the directive `img-src` of 12.8 with the tile source. The line is not a key: a file that sets `img_src` stops the start with `unknown attribute`.

Example:

```text
geomap.tile_url = "https://tiles.example.org/osm/{z}/{x}/{y}.png"  # file
geomap.attribution = "Map tiles: example.org"  # file
geomap.max_zoom = 19  # file
geomap.img_src = "'self' data: https://tiles.example.org/osm/"  # derived
```

The complete output for the file of 13.16:

```text
server.listen = "0.0.0.0:8080"  # file
server.ops_listen = "0.0.0.0:9090"  # file
server.base_path = "/"  # file
server.query_inspect = false  # default
server.repeat_max_per_panel = 24  # default
server.repeat_max_per_section = 12  # default
server.repeat_max_per_dashboard = 120  # default
server.compress_static = true  # default
server.transform_max_cells = 10000000  # default
server.list_filters_min_dashboards = 6  # default
server.max_connections = 1024  # file
server.idle_timeout = "75s"  # file
server.shutdown_grace = "30s"  # file
server.max_request_bytes = 8388608  # file
server.max_result_memory_bytes = 536870912  # file
log.level = "info"  # file
telemetry.enabled = false  # file
storage.volume "main".path = "/var/lib/hune/dashboards"  # file
storage.volume "main".primary = true  # file
storage.volume "main".rescan_interval = "5s"  # file
storage.volume "main".history_keep = 100  # file
access.mode = "open"  # file
clickhouse "prod".label = "Production"  # file
clickhouse "prod".url = "https://clickhouse:8443"  # file
clickhouse "prod".user = "hune"  # file
clickhouse "prod".password = <hidden>  # file:/run/secrets/clickhouse/password
clickhouse "prod".database = "analytics"  # file
clickhouse "prod".tls.ca_file = "/run/secrets/clickhouse/ca.pem"  # file
clickhouse "prod".tls.server_name = "clickhouse"  # file
clickhouse "prod".connect_timeout = "5s"  # file
clickhouse "prod".max_execution_time = "1m"  # file
clickhouse "prod".max_result_rows = 100000  # file
clickhouse "prod".max_result_bytes = 33554432  # file
clickhouse "prod".max_query_size = 1048576  # file
clickhouse "prod".max_concurrent = 16  # file
clickhouse "prod".queue_wait = "5s"  # file
```

Lines of the output for a `postgres` block, for the example of 13.12a:

```text
postgres "metrics".label = "Metrics"  # file
postgres "metrics".host = "hune-pg-ro"  # file
postgres "metrics".port = 5432  # file
postgres "metrics".database = "analytics"  # file
postgres "metrics".user = "hune_ro"  # file:/run/secrets/postgres-metrics/username
postgres "metrics".password = <hidden>  # file:/run/secrets/postgres-metrics/password
postgres "metrics".search_path = "metrics"  # file
postgres "metrics".sslmode = "verify-full"  # file
postgres "metrics".tls.ca_file = "/run/secrets/postgres-metrics-ca/ca.crt"  # file
postgres "metrics".tls.server_name = "hune-pg-ro"  # file
postgres "metrics".connect_timeout = "5s"  # file
postgres "metrics".max_execution_time = "1m"  # file
postgres "metrics".max_result_rows = 100000  # file
postgres "metrics".max_result_bytes = 33554432  # file
postgres "metrics".max_query_size = 1048576  # file
postgres "metrics".max_concurrent = 16  # file
postgres "metrics".queue_wait = "5s"  # file
postgres "metrics".max_concurrent_per_subject = 6  # file
```

Lines of the output for a `mongodb` block, for the example of 13.12b. A list shows in brackets, as in 13.20:

```text
mongodb "events".label = "Events"  # file
mongodb "events".topology = "direct"  # file
mongodb "events".host = "hune-mongo-ro"  # file
mongodb "events".port = 27017  # file
mongodb "events".user = "hune_ro"  # file:/run/secrets/mongodb-events/username
mongodb "events".password = <hidden>  # file:/run/secrets/mongodb-events/password
mongodb "events".auth_database = "admin"  # file
mongodb "events".databases = ["analytics"]  # file
mongodb "events".collections = ["node_load", "node_events"]  # file
mongodb "events".read_preference = "secondaryPreferred"  # file
mongodb "events".tls_mode = "verify-full"  # file
mongodb "events".tls.ca_file = "/run/secrets/mongodb-events-ca/ca.crt"  # file
mongodb "events".connect_timeout = "5s"  # file
mongodb "events".max_execution_time = "1m"  # file
mongodb "events".max_result_rows = 100000  # file
mongodb "events".max_result_bytes = 33554432  # file
mongodb "events".max_query_size = 1048576  # file
mongodb "events".max_concurrent = 16  # file
mongodb "events".queue_wait = "5s"  # file
mongodb "events".max_concurrent_per_subject = 6  # file
```

Lines of the output for the file of 13.17 that show the other sources:

```text
storage.postgres.user = "app"  # file:/run/secrets/pg/username
storage.postgres.password = <hidden>  # file:/run/secrets/pg/password
access.identity.public_keys = ["Ed25519 kid=edge-2026-11", "ES256 kid=edge-2026-05"]  # file:/run/secrets/edge/jwks.json
access.pdp.token = <hidden>  # file:/run/secrets/pdp/token
```

### 13.21 print-reference

```text
hune print-reference --format hcl --profile local
hune print-reference --format hcl --profile distributed
hune print-reference --format markdown
```

- `--format hcl --profile local` writes the file of 13.16, byte for byte.
- `--format hcl --profile distributed` writes the file of 13.17, byte for byte.
- `--format markdown` writes the reference document `docs/configuration-reference.md`. It has one table for each block,
  with the columns of 13.8 to 13.12a and the description of each key.
- Both options are necessary where they apply. An absent or unknown value stops the command with the exit code 2.
- The command reads no configuration file and no environment variable.

All three outputs come from the key table in the code. The CI job `policy` fails when the committed
`docs/configuration-reference.md` differs from the output. Both HCL outputs have `telemetry.enabled = false`, so they pass the checks in each build
(13.9). A unit test checks each HCL output with the checks of
`check-config`. Its test environment gives each file that the output names. The compose files of section 17 start
from these two files.

### 13.22 The start log

After the checks pass, `serve` writes these lines, in this order, before any other line (14.6):

1. `server.config`: the field `config` is a JSON object with one member for each line of `print-config`. The member
   name is the key path. A text, a duration and `<hidden>` are JSON strings. An integer is a JSON number. A boolean is a
   JSON boolean, and a list is a JSON array.
2. `server.config.sources`: the field `sources` is a JSON object with one member for each key whose source is not
   `file`. The value is the source, for example `"file:/run/secrets/clickhouse/password"`. A key that the file omits and that has a default shows with the source `default`. The derived line `geomap.img_src` of 13.20 has the source `derived`.
3. One `config.warning` line for each warning of 13.15.

An example of the second line, for the file of 13.16:

```json
{"ts":"2026-11-09T10:15:30.123Z","level":"info","event":"server.config.sources","sources":{"server.query_inspect":"default","server.repeat_max_per_panel":"default","server.repeat_max_per_section":"default","server.repeat_max_per_dashboard":"default","server.compress_static":"default","server.transform_max_cells":"default","server.list_filters_min_dashboards":"default","clickhouse \"prod\".password":"file:/run/secrets/clickhouse/password"}}
```

Before the checks pass, the log configuration is not known. `serve` then writes each error on the standard error in the
form of 13.14, not in JSON, and exits with 1.

### 13.23 The environment rule

Hune reads no environment variable that the file does not name. These are the rules of the code:

- The first statement of `main` calls `CapturedEnvironment::capture_and_clear()` of `hclconf`, before any other thread
  starts. It copies the environment, then removes each variable from the process.
- The engine reads the variables that `_env` attributes name from this copy, at start, one time.
- The process keeps only `SSL_CERT_FILE` and `SSL_CERT_DIR`, because `hclconf` keeps them for a TLS
  library. No crate of Hune reads them: Hune uses no system CA store (12.14). An `_env` attribute cannot name them.
- No crate reads a proxy variable. Hune has no `reqwest`, and its HTTP clients use no proxy.
- No crate reads `RUST_LOG`: the log filter comes from `log.level` only. No crate reads an `OTEL_*` variable: Hune has
  its own OTLP exporter (14.9).
- The job `policy` searches the sources for the pattern `env::(var|var_os|vars|vars_os|set_var|remove_var)\b`. It
  permits a match only in `crates/hclconf/src/env.rs`, in the tests and in each `build.rs`. The tests and `build.rs`
  are not code of the binary.
- The crate `mongodb` is a local copy with no environment read (8b.2). The original reads the variables of
  the platforms (for example `AWS_REGION`, `K_SERVICE` and `VERCEL`), `KUBERNETES_SERVICE_HOST` and the file
  `/.dockerenv` to fill the metadata of `hello`. The job `policy` searches the copy and `crates/` for each read. The list
  of the reads that this check accepts is empty (16.18).

The audit of the dependency tree of the binary found reads in other crates. Hune does not change these crates. Each
read sees an empty environment, because `main` clears the environment first. The result is short:

- `tokio` reads `TOKIO_WORKER_THREADS` only when the builder has no worker count. Hune sets the count. `tokio` also
  reads `TOKIO_UNSTABLE_SHARDED_BLOCKING_QUEUE` in each runtime builder. The variable selects the queue type of the pool of `spawn_blocking` calls.
- `rustls` reads `SSLKEYLOGFILE` in the type `KeyLogFile` only. No crate of the tree builds this type.
- `deadpool-postgres` (function `get_pg_config`), `hyper-util` (feature `client-proxy`, off) and `whoami` (locale and
  desktop functions) read variables in code that Hune does not call.
- A build script or a procedural macro reads the variables of `cargo` and `rustc` at build time. It does not run in the binary.
- The standard library reads `RUST_BACKTRACE` and `RUST_MIN_STACK`. The system resolver reads the files `/etc/hosts` and
  `/etc/resolv.conf` when a `host` is a name. These files are not variables.

These variables thus have no effect: `RUST_LOG`, `RUST_BACKTRACE`, each `OTEL_*` variable, `HTTP_PROXY`, `HTTPS_PROXY`,
`ALL_PROXY`, `NO_PROXY` (in each case), `SSL_CERT_FILE`, `SSL_CERT_DIR`, each `PG*` variable of libpq, `TZ`, and each
`HUNE_*` variable. A file can still name any of them with `_env`, except `SSL_CERT_FILE` and `SSL_CERT_DIR`. Example:
`otlp_endpoint_env = "OTEL_EXPORTER_OTLP_ENDPOINT"`.

### 13.24 The test of the environment rule

The CI job `api` runs the test `environment_has_no_effect` with a compose file of the test (section 16). The test has
three parts. Each part runs from the milestone that gives the features that it needs (section 18):

| Part | From | Configuration of `serve` | Rows of the table below |
|---|---|---|---|
| 1 | M2 | Mode `open`, `telemetry.enabled = false`, `log.level = "info"` | `RUST_LOG`, the proxy variables, `SSL_CERT_*` (link to ClickHouse), `PG*`, `TZ`, `HUNE_LOG_LEVEL` |
| 2 | M6 | Mode `authzen` with the `fake-pdp` of ProcurAuth, `telemetry.enabled = false` | The proxy variables and `SSL_CERT_*` (link to the PDP) |
| 3 | M7 | Mode `open`, `telemetry.enabled = true` with collector A | The `OTEL_*` variables |

Each part has these steps:

1. Start a fake proxy at `127.0.0.3:3128` and a ClickHouse container. Part 2 also starts the `fake-pdp`. Part 3 also
   starts a fake OTLP collector A at the address of the test file, and a fake collector B at `127.0.0.2:4318`.
2. Run `hune print-config` with an empty environment. Keep the output.
3. Run `hune print-config` again with the bad variables of the table below. The output MUST be byte-equal to the output
   of step 2.
4. Run `hune serve` with the bad variables and the configuration of the part.
5. Send one panel query, then stop Hune.
6. Check the rows of the part in the table below.

| Bad variable | Value | Check | Part |
|---|---|---|---|
| `RUST_LOG` | `trace` | No log line has the level `debug`. | 1 |
| `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | `http://127.0.0.2:4318` | Collector B gets no connection. Collector A gets the spans. | 3 |
| `OTEL_SERVICE_NAME`, `OTEL_RESOURCE_ATTRIBUTES` | `wrong`, `service.name=wrong` | Each span has `service.name = "hune"`. | 3 |
| `OTEL_SDK_DISABLED`, `OTEL_TRACES_SAMPLER` | `true`, `always_off` | Collector A gets the spans. | 3 |
| `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY` and their lower case forms | `http://127.0.0.3:3128`, `*` | The proxy gets no connection. | 1, 2 |
| `SSL_CERT_FILE`, `SSL_CERT_DIR` | `/nonexistent` | The TLS links to ClickHouse (part 1) and to the PDP (part 2) succeed with the CA files of the configuration. | 1, 2 |
| `PGHOST`, `PGPASSWORD`, `PGSSLMODE`, `PGOPTIONS`, `PGSERVICE`, `PGPASSFILE`, `PGSSLROOTCERT` | `127.0.0.4`, `wrong`, `disable`, `-c default_transaction_read_only=off`, `x`, `/nonexistent`, `/nonexistent` | No effect on a PostgreSQL run of the same test. No effect on a `postgres` source from the milestone PG: it connects to its own `host`, and its session keeps `default_transaction_read_only = on` (8a.4). | 1 |
| `MONGODB_URI`, `MONGODB_HOST`, `MONGODB_USER`, `MONGODB_PASSWORD`, `MONGODB_TLS`, `MONGODB_AUTH_MECHANISM`, `MONGODB_READ_PREFERENCE`, `MONGOSH_HOME` | `mongodb://127.0.0.4:27017/?tls=false&authMechanism=PLAIN`, `127.0.0.4`, `wrong`, `wrong`, `false`, `PLAIN`, `primary`, `/nonexistent` | No effect on a `mongodb` source from the milestone MMG: it connects to its own `host`, with TLS and SCRAM-SHA-256 (8b.4). | 1 |
| `TZ` | `Pacific/Kiritimati` | Each `ts` ends with `Z` and is UTC. | 1 |
| `HUNE_LOG_LEVEL` | `debug` | No log line has the level `debug`. | 1 |

### 13.25 Limits of the files and of the error list

| Item | Limit |
|---|---|
| The configuration file | 1048576 bytes; a regular file; UTF-8; no byte order mark |
| Nested brackets, braces and parentheses | 32 |
| An integer in the file | at most 18446744073709551615 (then its range applies) |
| The content of a `_file` value | 65536 bytes; UTF-8; no byte order mark; one last line end removed |
| The value of an `_env` variable | UTF-8; no space or line end at its start or end |
| A CA file | 1048576 bytes; 1 to 64 certificates |
| `access.identity.public_keys` | 16 keys; 4096 bytes of base64 text for each PEM block |
| Errors for one key | 10, then one line `and <n> more errors` |
| Errors in one list | 100, then one line `and <n> more errors` |

### 13.26 A change of the configuration

Hune has no reload: no signal and no route reads the file again. To change a value, do these steps:

1. Edit the file.
2. Run `hune check-config --config <path>` with the binary of the instance. Correct each error.
3. Restart the instance. In the distributed case, restart one instance at a time and wait for `/readyz`.

A new version of Hune can add, remove or rename keys. The closed schema then stops the start of an old file. The
`CHANGELOG.md` of each release lists each change of a key. Before an upgrade, run `check-config` of the new binary on the
current file (14.18).

## 14. Observability and operation

### 14.1 Principles

- Two listeners: `server.listen` serves the pages and the API. `server.ops_listen` serves the probes and the metrics.
- Logs are JSON lines on the standard output, each with the field `event`.
- Metrics use the Prometheus text format and the prefix `hune_`. They are only on `server.ops_listen`.
- Traces go to an OTLP collector when the block `telemetry` enables them (14.9). Hune has its own small
  exporter.
- The durable audit is the history of the store. Hune writes it before the effect (14.11).
- Each queue and each pool is bounded. Each refusal is an explicit answer (14.14).

### 14.2 The two listeners

| Property | `server.listen` | `server.ops_listen` |
|---|---|---|
| Routes | The pages, the static files, the API and the ProcurAuth routes of section 6, below `server.base_path`. | `GET` and `HEAD` of `/livez`, `/readyz` and `/metrics`. No base path. |
| Token in the mode `authzen` | On each route except the action document (11.29). | None. |
| Clients | The proxy or the edge. | The container platform and the monitoring. |
| Limits | 12.16. | 64 connections, 10 s to read the headers, no body. |
| Log line `http.request`, metrics `hune_http_*`, spans | Yes. | No. |
| Headers | 12.8. | `Cache-Control: no-store` and `X-Content-Type-Options: nosniff`. |
| During a stop | Closes at step 3 of 14.13. | Stays open until the exit. |

On `server.ops_listen`, another method gets 405 and another path gets 404. A request with a body gets 400. Both
listeners use plain HTTP: the proxy terminates TLS (12.6).

The two addresses must not overlap (13.14, C9). A bind error stops the start with the exit code 1 and the log line
`server.start_failed`, which names the address.

### 14.3 The probe /livez

- `GET /livez` answers 200 with the body `{"status":"live"}` and `Content-Type: application/json` (6.26).
- It answers from the bind of `server.ops_listen` to the exit of the process.
- It checks no dependency. A failure means a process that does not serve. A restart can help.

### 14.4 The probe /readyz

`GET /readyz` follows these rules:

1. It answers 503 before the first complete load of the catalog.
2. It answers 200 after the first complete load, while the process does not stop.
3. It answers 503 from the signal of a stop (14.13).
4. After the first load, a down PostgreSQL, volume, ClickHouse source or PDP never makes it answer 503.

Rule 4 prevents a full outage. A readiness that fails on each instance at the same time removes all of them. But the
catalog in memory can still serve the dashboards.

The body is JSON with `Content-Type: application/json`. Example:

```json
{"ready": true, "stopping": false, "catalog_loaded": true,
 "store": {"backend": "postgres", "state": "up", "catalog_age_seconds": 3},
 "sources": {"prod": "up", "preprod": "down"},
 "pdp": "up"}
```

| Member | Values |
|---|---|
| `ready` | `true` when the status is 200. |
| `stopping` | `true` from the signal of a stop. |
| `catalog_loaded` | `true` after the first complete load. |
| `store.backend` | `volume` or `postgres`. |
| `store.state` | `up` when the last poll or scan succeeded, else `down`. |
| `store.catalog_age_seconds` | The whole seconds since the last poll or scan that succeeded. |
| `sources.<id>` | `up` or `down` from the last ping of section 8 or 8a, or `unknown` before the first ping. |
| `pdp` | `none` in the mode `open`. `unknown` before the first request to the PDP. `up` or `down` from the last request. |

The members after `catalog_loaded` are for people. They never change the status after the first load.

An example for Kubernetes, with `server.shutdown_grace = "30s"` and the drain of 14.13:

```yaml
livenessProbe:  { httpGet: { path: /livez,  port: 9090 }, periodSeconds: 10, timeoutSeconds: 3, failureThreshold: 3 }
readinessProbe: { httpGet: { path: /readyz, port: 9090 }, periodSeconds: 5,  timeoutSeconds: 3, failureThreshold: 1 }
startupProbe:   { httpGet: { path: /livez,  port: 9090 }, periodSeconds: 2,  failureThreshold: 30 }
terminationGracePeriodSeconds: 40
```

### 14.5 The subcommand health

```text
hune health --config <path>
```

- It checks the file with the rules of section 13, but it reads no `_env` and no `_file` value (13.18).
- It sends `GET /livez` to `server.ops_listen`. A wildcard host becomes the loopback address of the same family
  (`127.0.0.1` or `::1`).
- One deadline of 2 s covers the connection and the answer.
- It exits with 0 when the status is 200. It exits with 1 for another status, no answer or a configuration error. It
  exits with 2 for a bad command line.
- It writes nothing when it succeeds, and one line on the standard error when it fails.

The image has no `curl`. Its health check is `["/hune", "health", "--config", "/etc/hune/config.hcl"]` with an interval
of 10 s, a timeout of 3 s and 3 retries (section 17).

### 14.6 The log format

- `serve` writes one JSON object for each line, in UTF-8, on the standard output. The other subcommands follow 13.19 to
  13.21 and section 4, 5 and 15.
- Each line has `ts`, `level` and `event` first. `ts` is the time in UTC, in RFC 3339 form with milliseconds and `Z`,
  for example `2026-11-09T10:15:30.123Z`. `level` is `error`, `warn`, `info` or `debug`.
- A line that belongs to a request also has `request_id`. When telemetry is on, it also has `trace_id` and `span_id`.
- The other fields of each event are in 14.7. A field without a value is `null`.
- `log.level` sets the lowest level that Hune writes. The order is `debug`, `info`, `warn`, `error`.
- A line has 16 KiB at most. Hune cuts a longer text field, adds `...` to it, and adds the field `truncated` with the
  value `true`.
- The writes to the standard output block the writer. The container runtime reads the output.

The request id:

- When the request has `X-Request-Id` with 1 to 128 characters of `A-Z a-z 0-9 . _ : -`, Hune keeps it.
- Else Hune makes one: 32 lower-case hexadecimal characters from the random source of the system.
- Hune sends it in the answer header `X-Request-Id`, to ClickHouse in `log_comment` (section 8), and to the PDP in
  `X-Request-ID` (11.12).

### 14.7 The log events

This is the complete list of the log events of Hune. `serve` writes each event, except `store.migrated`, which
`hune migrate-store` writes (section 5). Each event has the fields of 14.6 and the fields of this table.

| Event | Level | When | Fields |
|---|---|---|---|
| `server.config` | info | At start, after the checks (13.22). | `config` |
| `server.config.sources` | info | At start (13.22). | `sources` |
| `config.warning` | warn | At start, one line for each warning (13.15). | `key`, `message` |
| `server.listening` | info | `server.listen` is bound (step 10 of 14.12). | `listen`, `ops_listen`, `version`, `commit`, `access_mode`, `storage_backend` |
| `server.start_failed` | error | A start check fails after the log configuration is known (14.12). | `step`, `error` |
| `store.loaded` | info | The first complete load of the catalog, and each later complete load. | `backend`, `folders`, `dashboards`, `invalid`, `ignored`, `elapsed_ms` |
| `server.ready` | info | `/readyz` answers 200 for the first time. | `elapsed_ms` (from the start of the process) |
| `server.stopping` | info | A signal of stop. | `signal`, `in_flight` |
| `server.stopped` | info | Just before the exit with 0. | `elapsed_ms` (from the signal) |
| `server.shutdown_timeout` | error | The grace ends with requests in flight (14.13). | `in_flight` |
| `server.signal_again` | error | A second signal during a stop. | `signal` |
| `http.request` | info | Each answer of `server.listen`. | `request_id`, `method`, `route`, `status`, `code`, `duration_ms`, `request_bytes`, `response_bytes`, `subject`, `client` |
| `query.run` | info | The end of each query to a source. | `request_id`, `kind` (`panel`, `options`, `annotation`, `preview`), `subject`, `uid`, `panel`, `ref`, `variable`, `annotation`, `source`, `source_kind`, `query_id`, `sql_sha256`, `queue_ms`, `duration_ms`, `rows`, `bytes`, `status`, `outcome`, `code`, `clickhouse_code`, `sqlstate`, `mongo_code`, `max_result_rows`, `max_execution_time_s`, `ceiling` |
| `query.adhoc` | info | A panel query with the filters of an `adhoc` variable (7.7a), before the query runs. | `request_id`, `uid`, `panel`, `ref`, `source`, `variable`, `count`, `keys` |
| `source.down` | warn | The ping of a source changes to a failure (section 8, 8a, 8b). | `source`, `source_kind`, `error` |
| `source.up` | info | The ping of a source changes to a success. | `source`, `source_kind`, `down_seconds` |
| `source.misconfigured` | error | The state of a source changes to `misconfigured` (8.11): `readonly` is not 2, the login failed, the certificate check failed, or a key is above a constraint of the profile. For a PostgreSQL source (8a.12): a mandatory check fails, the login failed, the certificate check failed, or the server refused a startup parameter. For a MongoDB source (8b.12): a mandatory check fails, the login failed or the certificate check failed. | `source`, `source_kind`, `reason`, `clickhouse_code`, `sqlstate`, `mongo_code` |
| `source.advice` | warn | An advice check of 8a.12 fails on a PostgreSQL source, or an advice check of 8b.12 fails on a MongoDB source. One line for each change. | `source`, `check`, `names` (at most 5) |
| `source.version_unsupported` | warn | The version of a source is below the oldest supported release (8.13, 8a.14, 8b.14). One line for each change. | `source`, `source_kind`, `version` |
| `store.document_invalid` | warn | A document fails the closed parse (section 4). | `uid`, `folder`, `errors` (at most 20 objects with `pointer` and `message`) |
| `store.entry_ignored` | warn | The scan of the volume ignores an entry (section 5). | `path` (relative to the root), `reason` |
| `store.sync_failed` | warn | The poll or the scan changes to a failure. | `backend`, `error` |
| `store.sync_recovered` | info | The poll or the scan changes back to a success. | `backend`, `down_seconds` |
| `store.lock_unsupported` | warn | The file system of the volume has no `flock`, and the writer continues (section 5). | `path`, `errno` |
| `store.query_failed` | warn | PostgreSQL gives a SQLSTATE that section 5 does not map. | `request_id`, `op`, `sqlstate` |
| `store.migrated` | info | `hune migrate-store` applied one step of the schema (section 5). | `step` |
| `dashboard.changed` | info | After each change of a dashboard (14.11). | `request_id`, `action`, `uid`, `folder`, `from_folder`, `version`, `etag_before`, `etag_after`, `subject`, `save_id`, `bytes` |
| `folder.changed` | info | After each change of a folder. | `request_id`, `action` (`create`, `rename`, `delete`), `folder`, `subject` |
| `access.unauthenticated` | info | Hune refuses a token (11.8). | `request_id`, `method`, `route`, `reason` |
| `access.no_rule` | error | A route has no line in 11.21. | `request_id`, `method`, `route` |
| `access.prepass_mismatch` | error | A source of a valid document is not in the questions of its create or save, or the pre-pass could not read a document that passed the validation (11.21). Hune answers 500 and writes nothing. | `request_id`, `route`, `uid`, `readable`, `missing` (the source ids that the decision did not ask) |
| `access.decision` | info | Each question that the PDP answers. | `request_id`, `subject`, `owner`, `action`, `resource`, `match`, `decision`, `reason`, `ceilings`, `duration_ms` |
| `access.pdp_failed` | warn | A request to the PDP gets no answer in time, a connection error, a status that 11.12 does not accept, or no permit of the pool (11.28). | `request_id`, `questions`, `error` |
| `access.pdp_credential_refused` | error | The PDP answers 401 or 403. | `request_id`, `status` |
| `access.pdp_invalid_answer` | error | An answer with the status 200 is not a valid answer or not valid metadata (11.24, 11.28). | `request_id`, `detail` |
| `access.ceiling_unenforceable` | warn | A `true` answer has a ceiling that the action does not list. | `request_id`, `action`, `ceiling` |
| `access.ceiling_refused` | info | A query is above a ceiling (11.24). | `request_id`, `subject`, `action`, `ceiling`, `limit`, `uid`, `source` |
| `access.options` | info | A request to the options route. | `request_id`, `subject`, `type`, `parent`, `items`, `questions` |
| `telemetry.export_failed` | warn | The OTLP export changes to a failure. | `error`, `dropped` |
| `telemetry.export_recovered` | info | The OTLP export changes back to a success. | `dropped` |

The field `annotation` of `query.run` is the index of the annotation. It is `null` for another kind of query. The log line never holds the title, the text or a tag of a mark.

The field `keys` of `query.adhoc` is the list of the configured columns of the filters, in the order of the request. The field `count` is their number. The line never holds a value or an operator of a filter (7.7a).

The field `sql_sha256` of `query.run` is the SHA-256 of the expanded SQL. This SQL holds the values of the text form of 7.6 and the values of the ad hoc filters. Thus the hash is a fingerprint of these values. A person with the log and the stored SQL can test a guessed value offline. Keep the log of Hune with the access control and the retention of `system.query_log` (12.15).

- `ref` in `query.run` is the ref of the query, when the request of a panel has a `ref` (6.20). It is `null` for another request, as 14.6 says. The line never holds a value of a variable or the SQL.

Rules for the fields:

- `route` is the route template with the method, for example `PUT /api/dashboards/{uid}`. A request that matches no
  route has `unmatched`.
- `code` is the error code of section 6, or `null` for a status below 400.
- `subject` is the claim `sub` in the mode `authzen`, and `-` in the mode `open` or before the token check.
- `client` is the address of the TCP peer. Hune does not read `X-Forwarded-For`.
- `resource` is the text form of a chain (11.16), for example `service:dashboards/folder:transit/dashboard:score`.
- `ceiling` in `query.run` is `true` when a ceiling lowered a limit of the query.
- `source_kind` is `clickhouse`, `postgres` or `mongodb`. Of the three fields `clickhouse_code`, `sqlstate` and `mongo_code`, only the one of the kind of the source is not `null`. For a PostgreSQL source, `reason` of `source.misconfigured` is the check of 8a.12 and its first names, for example `R4: metrics.node_load`.
- `status` in `query.run` is the HTTP status of the answer, or 499 when the client closed the connection before the
  answer (8.9). `outcome` is `ok`, `timeout`, `limit`, `ceiling`, `sql_error`, `unavailable` or `cancelled`: the
  values of the label `status` of `hune_query_duration_seconds` (14.8).
- A query that never reaches ClickHouse writes no `query.run` line: a refusal of the admission or a `misconfigured`
  source (section 8). The line `http.request` and the metrics count it.
- The `action` of `dashboard.changed` is a version kind of 5.2: `create`, `save`, `restore`, `move`, `delete`,
  `import`, `external` or `migrate`. An external change (section 5) has `subject = "-"` and `request_id = null`.
- Only the process that writes a change writes its `dashboard.changed` line. A poll of PostgreSQL that finds the change
  of another instance writes no line. On the volume, a scan that finds a change made outside Hune writes the line with
  `action = "external"`.
- An event "changes to" is written one time for each change of state, not at each try.

### 14.8 The metrics

`GET /metrics` on `server.ops_listen` gives the metrics in the Prometheus text format 0.0.4, with
`Content-Type: text/plain; version=0.0.4; charset=utf-8`.

| Metric | Type | Labels | Meaning |
|---|---|---|---|
| `hune_build_info` | gauge, always 1 | `version`, `commit` | The build. |
| `hune_start_time_seconds` | gauge | none | The Unix time of the start. |
| `hune_ready` | gauge | none | 1 when `/readyz` answers 200. |
| `hune_stopping` | gauge | none | 1 from the signal of a stop. |
| `hune_http_requests_total` | counter | `route`, `status` | The answers of `server.listen`. |
| `hune_http_request_duration_seconds` | histogram | `route` | The time from the first byte of the request to the last byte of the answer. |
| `hune_http_in_flight` | gauge | none | The requests that Hune works on now. |
| `hune_http_refused_total` | counter | `reason` | The refusals before the route (12.16). |
| `hune_query_duration_seconds` | histogram | `source`, `status` | The time of a query, from its admission to the end of its frame. |
| `hune_query_rows_total` | counter | `source` | The rows of the results. |
| `hune_query_response_bytes_total` | counter | `source` | The bytes that the source sent. For a PostgreSQL source, the count of 8a.6. |
| `hune_source_in_flight` | gauge | `source` | The queries that run now on the source. |
| `hune_source_queue_wait_seconds` | histogram | `source` | The wait for a place (12.16). |
| `hune_source_refused_total` | counter | `source`, `reason` | The queries that got `source_busy`. |
| `hune_source_up` | gauge | `source` | 1 when the last ping succeeded. |
| `hune_result_memory_reserved_bytes` | gauge | none | The bytes that the queries in flight reserve. |
| `hune_store_operation_duration_seconds` | histogram | `backend`, `op` | The time of an operation of the store. |
| `hune_store_operations_total` | counter | `backend`, `op`, `status` | The operations of the store. |
| `hune_store_conflicts_total` | counter | none | The saves that got `409 conflict`. |
| `hune_store_poll_changes_total` | counter | none | The documents that a poll or a scan found changed. |
| `hune_store_up` | gauge | none | 1 when the last poll or scan succeeded. |
| `hune_store_catalog_age_seconds` | gauge | none | The seconds since the last poll or scan that succeeded. |
| `hune_documents` | gauge | `state` | The documents of the catalog. |
| `hune_pdp_requests_total` | counter | `result` | The requests to the PDP. Always 0 in the mode `open`. |
| `hune_pdp_duration_seconds` | histogram | none | The time of one request to the PDP. |
| `hune_pdp_questions_total` | counter | `decision` | The questions that the PDP answered. |
| `hune_pdp_cache_hits_total` | counter | none | The questions that a kept answer served (11.27). |
| `hune_pdp_pool_in_use` | gauge | none | The permits of the pool in use (11.13). |
| `hune_telemetry_dropped_spans_total` | counter | none | The spans that the exporter dropped (14.9). |

The values of the labels:

| Label | Values |
|---|---|
| `route` | The route templates of section 6 with the method, `GET /static/*`, `GET /d/*`, `GET /`, and `unmatched`. |
| `status` of `hune_http_requests_total` | The HTTP status code, three digits. |
| `reason` of `hune_http_refused_total` | `max_connections`, `header_timeout`, `body_timeout`, `uri_too_long`, `headers_too_large`, `payload_too_large`, `cross_site_request`, `unsupported_media_type` |
| `source` | The labels of the `clickhouse`, `postgres` and `mongodb` blocks (64 at most in all). |
| `status` of `hune_query_duration_seconds` | `ok`, `timeout`, `limit`, `ceiling`, `sql_error`, `unavailable`, `cancelled` |
| `reason` of `hune_source_refused_total` | `queue_wait`, `per_subject`, `memory` |
| `backend` | `volume`, `postgres` |
| `op` | The operations of the `Store` trait (5.3): `load`, `poll`, `get`, `create`, `save`, `move_to`, `delete`, `restore`, `history`, `version`, `create_folder`, `rename_folder`, `delete_folder`, `export`, `import` |
| `status` of `hune_store_operations_total` | `ok`, `conflict`, `unavailable`, `full`, `invalid` |
| `state` | `valid`, `invalid`, `ignored` |
| `result` | `ok`, `timeout`, `transport`, `status`, `invalid`, `pool_busy` |
| `decision` | `true`, `false` |

The buckets of the histograms, in seconds:

| Histograms | Buckets |
|---|---|
| `hune_http_request_duration_seconds`, `hune_query_duration_seconds`, `hune_store_operation_duration_seconds` | 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120 |
| `hune_source_queue_wait_seconds`, `hune_pdp_duration_seconds` | 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10, 60 |

No label holds a uid, a folder id, a subject, SQL text, an error message or a value of a request. A unit test reads the
output of `/metrics` and checks the label set of each metric against these tables.

### 14.9 The telemetry block and the OTLP exporter

The block `telemetry` has the keys of 13.9. With `enabled = false`, Hune makes no span and runs
no exporter. With `enabled = true`, the exporter follows these rules:

- Protocol: OTLP/HTTP with the JSON encoding. Each export is a `POST` to `<otlp_endpoint>/v1/traces` with
  `Content-Type: application/json` and `User-Agent: hune/<version>`.
- Client: the own `hyper` client with `rustls` (12.14). It uses no proxy and reads no environment variable. No
  OpenTelemetry SDK is in the build.
- Resource attributes: `service.name` = `telemetry.service_name`, `service.version` = the version of Hune, and
  `service.instance.id` = a random UUID made at start. The scope name is `hune`.
- Queue: 2048 spans. When the queue is full, Hune drops the new span and adds 1 to
  `hune_telemetry_dropped_spans_total`.
- Export: each `export_interval`, or at once when 512 spans wait. One export at a time. One export has 512 spans at most.
- Deadline of one export: the lower of 10 s and `export_interval`.
- Failure: no connection, the deadline, or a status other than 2xx. Hune drops the spans of that export and adds their
  number to the counter. It never sends them again. The line `telemetry.export_failed` comes one time for each change of
  state.
- At a stop, Hune exports the queue one last time, for 2 s at most (14.13).
- An export never delays a request. The telemetry is best effort and is never the audit.

An example of the block:

```hcl
telemetry {
  enabled         = true
  otlp_endpoint   = "http://otel-collector:4318"
  service_name    = "hune"
  export_interval = "15s"
}
```

### 14.10 The spans

| Span | Kind | Name | Attributes |
|---|---|---|---|
| A request of `server.listen` | SERVER | `<METHOD> <route template>` | `http.request.method`, `http.route`, `http.response.status_code`, `client.address`, `hune.request_id`, `hune.dashboard.uid` and `hune.panel.id` when the route has them, `error.type` (the error code) for a status of 400 or more |
| A query to ClickHouse | CLIENT | `clickhouse.query` | `db.system.name = "clickhouse"`, `server.address`, `server.port`, `hune.source`, `hune.query_id`, `hune.rows`, `error.type` (the ClickHouse error code) |
| A query to PostgreSQL | CLIENT | `postgres.query` | `db.system.name = "postgresql"`, `server.address`, `server.port`, `hune.source`, `hune.query_id`, `hune.rows`, `error.type` (the SQLSTATE) |
| A query to MongoDB | CLIENT | `mongodb.query` | `db.system.name = "mongodb"`, `server.address`, `server.port`, `hune.source`, `hune.query_id`, `hune.rows`, `error.type` (the server code) |
| An operation of the store | CLIENT | `store.<op>` | `hune.store.backend`, and `db.system.name = "postgresql"` for PostgreSQL |
| A request to the PDP | CLIENT | `pdp.evaluate` | `server.address`, `hune.questions` |

Rules:

- A request with a valid W3C `traceparent` header is a child of that trace. Else the SERVER span starts a new trace.
- Hune sends `traceparent` to ClickHouse and to the PDP. It sends nothing to PostgreSQL, neither for the store nor for a source. It sends nothing to MongoDB either.
- The span status is error for a status of 500 or more, and for a CLIENT span that failed.
- No attribute holds SQL text, a variable value, row data, a subject or a secret (12.15).
- The requests of `server.ops_listen` make no span.

### 14.11 The audit trail

- The durable audit is the history of each dashboard in the store (section 5). Hune writes the history record before
  the effect is visible. On the volume, it writes it before the rename. On PostgreSQL, it writes it in the same
  transaction.
- A change whose history record cannot be written does not happen. The client gets the error of section 5.
- The record holds the version, the kind of change, the time and the subject (`sub`, or `-` in the mode `open`). It
  also holds the message, the `save_id`, the folder, the etag and the document (section 5).
- After the effect, Hune writes the log line `dashboard.changed` (14.7). The log line is not the audit: a lost log line
  loses no record.
- A change of a folder writes the log line `folder.changed`.
- OTLP is never the audit.

### 14.12 The start sequence

`hune serve --config <path>` does these steps in this order:

1. Capture and clear the environment (13.23).
2. Read and check the configuration (section 13), with the PEP configuration and the action document (13.13). An
   error: the errors on the standard error, exit 1.
3. Write `server.config`, `server.config.sources` and each `config.warning` (13.22).
4. Build the TLS configurations, the clients of the sources and the PEP. With `server.compress_static = true`, make the
   gzip copies of the static files (9.3). This step sends nothing on the network.
5. Bind `server.ops_listen`. A bind error: `server.start_failed`, exit 1. The ops listener serves at once: `/livez`
   answers 200, and `/readyz` answers 503.
6. Open the store (section 5). For the volume, do its checks and take its lock. For PostgreSQL, do the checks of 5.29:
   the connection, the encoding, the schema version and the role (12.13). Each check that fails writes
   `server.start_failed` with the message of 5.29, and exits with 1.
7. A failed connection to PostgreSQL gets a retry only for four causes. They are a network failure, a timeout, the
   SQLSTATE `57P03`, and a server that is a standby. Hune then retries each `storage.postgres.connect_timeout`, with no limit.
   Each such failure writes `store.sync_failed` one time for each change of state. `/readyz` stays 503.
8. Load the catalog: `store.loaded`.
9. Start the tasks: the poll or the scan of the store, the pings of the sources and the OTLP exporter.
10. Bind `server.listen` and accept on it. A bind error: `server.start_failed`, exit 1. `/readyz` answers 200. Write
    `server.listening` and `server.ready`.

Three connection failures stop the start at step 7 with `server.start_failed` and the exit code 1. They are an
SQLSTATE of the class `28` (the authentication failed) and the SQLSTATE `3D000` (the database does not exist). The third
is a failed check of the certificate or of its name. They are errors of the configuration or of the account, and a retry cannot repair them.

The messages of 5.29 for the schema version and the role:

| Case | Message |
|---|---|
| The database has an older schema | `storage.postgres: database schema version V is older than this binary (M); run: hune migrate-store -c <file>` |
| The database has a newer schema | `storage.postgres: database schema version V is newer than this binary (M); use a binary of schema version V` |
| The role is a superuser or owns a table | `storage.postgres: the role <user> is a superuser or owns the Hune tables; serve needs a role with only SELECT, INSERT, UPDATE and DELETE (5.25, 12.13)` |

Before step 10, a connection to `server.listen` is refused, so the proxy answers 502 at once and does not wait.

The exit codes of `serve`:

| Code | Meaning |
|---|---|
| 0 | A graceful stop that ended in time. |
| 1 | A configuration error, a start check that failed, the end of the grace with requests in flight, or a second signal. |
| 2 | A command line that is not valid. |

### 14.13 The graceful stop

SIGTERM and SIGINT start the stop. The binary is the process 1 of the container, and it handles both signals itself. G
is `server.shutdown_grace`. `SHUTDOWN_DRAIN` is a constant of 5 s.

1. Write `server.stopping`. `/readyz` answers 503. `hune_stopping` is 1.
2. Wait for `SHUTDOWN_DRAIN`. During this time, Hune serves each request as usual. The container platform and the proxy
   remove the instance from their lists, which they do some time after the signal.
3. Stop the accept on `server.listen`. Close the idle connections. A request that comes on an open connection gets
   `503 server_stopping` with `Connection: close`.
4. Let the requests in flight continue, for G minus 3 s at most.
5. Close the connections of the queries still in flight. ClickHouse cancels them (section 8). A PostgreSQL source gets a cancel request first (8a.7). Their requests
   get `503 server_stopping`. A write to the store continues: a PostgreSQL transaction commits or rolls back, and a
   write to the volume ends. This step waits 1 s at most.
6. Stop the tasks of step 9 of 14.12. Close the pools. Export the OTLP queue, for 2 s at most.
7. Close `server.ops_listen`. Write `server.stopped` and exit with 0.

When requests are still in flight at the end of step 5, Hune writes `server.shutdown_timeout`, does step 6 and exits
with 1. A second signal during the stop writes `server.signal_again` and exits with 1 at once.

A call of the PEP that blocks cannot stop early (11.13). The exit does not wait for it.

The budget of a stop is G plus `SHUTDOWN_DRAIN`. Give the container more time than this budget. Set
`stop_grace_period` in compose and `terminationGracePeriodSeconds` in Kubernetes to G plus `SHUTDOWN_DRAIN` plus 5 s at
least. For G = 30 s, use 40 s (section 17).

### 14.14 Limits and back-pressure

These rules apply to each resource:

- Each queue, pool and cache has a bound. 12.16 gives the bounds and the answers.
- When a bound is reached, Hune answers at once with an explicit code. It never keeps a request in an unbounded queue.
- A refusal that can succeed later has `retryable: true`. The header `Retry-After` gives the wait in seconds: 1 for
  `server_busy` and `source_busy`, 5 for `storage_unavailable`.
- A wait for a place is bounded by a key: `queue_wait` for a source, `access.pdp.timeout` for the PDP pool and
  `storage.postgres.statement_timeout` for the PostgreSQL pool.
- Hune never repeats a write by itself. The browser repeats a save with the same `Idempotency-Key` (section 10).
- The browser runs at most 6 panel queries at one time for each tab (section 9).

The bounded items that are not in 12.16:

| Item | Bound |
|---|---|
| The OTLP queue | 2048 spans (14.9) |
| The kept answers of the PDP | 16384 entries (11.27) |
| Connections on `server.ops_listen` | 64 |
| The kernel backlog of each listener | 1024 |

### 14.15 A dependency that is down

| Dependency | Effect | What the operator sees | Recovery |
|---|---|---|---|
| One source (ClickHouse or PostgreSQL) | The panels of that source show the error of section 8 or 8a, with `retryable: true`. The other panels work. | `hune_source_up{source}` is 0; `source.down`; `query.run` with the status `unavailable`. | Automatic: the next refresh tries again. No circuit breaker. |
| PostgreSQL | The catalog serves the lists, the dashboards and the panel SQL. The saves get `503 storage_unavailable` with `Retry-After: 5`. | `hune_store_up` is 0; `hune_store_catalog_age_seconds` grows; `store.sync_failed`. | Automatic: the next poll; `store.sync_recovered` (14.23). |
| The volume (read error) | The catalog keeps the last good state. The saves fail with the error of section 5. | `hune_store_up` is 0; `store.sync_failed`. | Repair the mount. The next scan recovers. |
| The PDP | Each operation that needs a question gets `503 pdp_unavailable`, after the kept answers end (11.28). | `hune_pdp_requests_total{result!="ok"}`; `access.pdp_failed`; `/readyz` member `pdp` is `down`. | Automatic: the next question. |
| The OTLP collector | Hune drops the spans. Requests do not change. | `hune_telemetry_dropped_spans_total` grows; `telemetry.export_failed`. | Automatic: the next export. |

In each case `/readyz` stays 200 after the first load (14.4).

### 14.16 Runbook: start of the LOCAL case

1. Copy the directory `deploy/compose/volume/` of the release to the host. It holds `compose.yaml` and `config.hcl`
   (17.6).
2. Edit `config.hcl`: the source, the user, the database and `server.base_path`. Keep its secret paths.
3. Create the ClickHouse account of 8.12.
4. Make the directories `dashboards` and `secrets` and the two secret files with the commands of 17.6.
5. Make sure that only the user 65532 of the host can read each secret file (12.10).
6. Run `docker compose run --rm hune check-config -c /etc/hune/config.hcl`. Correct each error.
7. Start the container: `docker compose up -d`.
8. Find the log line `server.ready`. The container health check must show "healthy".
9. Add the nginx location of 12.6, then reload nginx.
10. Open the base path in a browser.

To bring the Grafana dashboards, follow section 15.

### 14.17 Runbook: start of the DISTRIBUTED case

1. Copy the directory `deploy/kubernetes/` of the release (17.8).
2. Edit the two files of the ConfigMap of `10-config.yaml`. `config.hcl` names the application role, and `migrate.hcl`
   names the owner role (5.25).
3. Create the database and the two roles of 5.25 and 12.13, as 17.8 gives.
4. Make the Kubernetes Secrets with the commands of 17.8. They mount under `/etc/hune/secrets/`.
5. Make sure that only the user 65532 of the node can read each secret file (12.10).
6. Give the operator of the edge and of the PDP the facts of 11.7.
7. Run `hune check-config` on `config.hcl` in a container with the mounts of the Deployment. Run it on `migrate.hcl`
   in a container that has each file that `migrate.hcl` names.
8. Apply `20-migrate-job.yaml`. Wait for the exit code 0 of the Job.
9. Give the grants of 5.25 to the application role, with the `psql` command of 17.8.
10. Apply `30-deployment.yaml` and the other files of 17.8, with 2 replicas or more.
11. Find the log line `server.ready` in each pod. Make sure that `hune_store_up` is 1.
12. Fill the store with `hune import` of an export, or with `hune import-grafana --write` (section 15).

### 14.18 Runbook: upgrade with no change of the schema or of the format

The `CHANGELOG.md` of each release says when a release changes the PostgreSQL schema or the document format. For such
a release, use 14.19 or 14.20.

1. Read the `CHANGELOG.md` of the new release. Note each change of a key.
2. Run `hune check-config --config <path>` with the new binary on the current file. Correct the file.
3. Make a backup with `hune export` (14.22).
4. LOCAL: stop the old container, then start the new one. The lock of the volume goes to the new process.
5. DISTRIBUTED: update the Deployment one pod at a time. A new pod gets traffic after `/readyz` answers 200.
6. Find `server.ready` and read each `config.warning`.

In the LOCAL case, the proxy answers 502 for some seconds during step 4. For a rollback, start the old binary with the old
file.

In the DISTRIBUTED case, the ConfigMap carries the version in its name (17.8), and each Deployment names its own
ConfigMap. For a rollback, apply the old Deployment. It names the old ConfigMap, so the old binary reads the old file.

### 14.19 Runbook: upgrade with migrate-store

`serve` stops at start when the schema version of the database differs from the version of the binary (section 5).
Thus a change of the schema stops each instance for some minutes.

1. Run `hune check-config` of the new binary on `config.hcl` and on `migrate.hcl`.
2. Make a backup with `hune export` of the old binary. Also make a CloudNativePG backup.
3. Scale the Deployment to 0. Wait until no pod runs.
4. Run `hune migrate-store --config /etc/hune/migrate.hcl` of the new binary as a Job. Wait for the exit code 0.
5. Apply the grants that the `CHANGELOG.md` of the release names (5.25).
6. Scale the Deployment back with the new binary.
7. Find `server.ready` in each pod.

From step 3 to step 6, the dashboards are not available. The old binary refuses the new schema. For a rollback, restore
the backup of step 2 into a new database (14.22). Then start the old binary on it. The saves made after step 6 are lost in
this case.

### 14.20 Runbook: upgrade with migrate-documents

The server reads only the current `format_version` (section 4). A change of the format thus needs a stop.

1. Run `hune check-config` of the new binary on the current file.
2. Make a backup with `hune export` of the old binary.
3. Stop each instance.
4. Run `hune migrate-documents` of the new binary as section 4 gives. Run the dry run first and read its report. A warning names a document that the command does not migrate (4.42, item 8).
5. Run the command again in the mode that writes.
6. In the LOCAL case with git, commit the changed files in one commit.
7. Start the new binary.
8. Check that the log has no `store.document_invalid` line.

For a rollback, restore the backup of step 2 (14.22) and start the old binary.

### 14.21 Runbook: rotation of a key or a secret

Hune reads each secret, each key and each CA file one time, at start (12.10). Each rotation thus needs a restart.

The public key of the edge (mode `authzen`):

0. Make sure that each token of the edge has a `kid` equal to the configured `kid` of its key. The log of the last
   `max_age` must hold no line `access.unauthenticated` with `reason = "kid"`. Without this step, the second key of
   step 1 makes Hune refuse each token without `kid` (11.8).
1. Add the new key to the text of `access.identity.public_keys`: a new PEM block, or a new JWK with its `kid`.
2. Keep the old key. Restart each instance.
3. Make the edge sign with the new key.
4. Wait for `max_age` plus `clock_skew`. Then each token of the old key has expired.
5. Remove the old key from the text. Restart each instance.

The PDP token:

1. Make the PDP accept the old token and the new token.
2. Write the new token to the file of `access.pdp.token_file`. Restart each instance.
3. Make the PDP refuse the old token.

A ClickHouse password:

1. Write the new password to the file of `password_file` of the block.
2. Change the password of the user in ClickHouse.
3. Restart each instance at once.

Between step 2 and step 3, the queries of an instance fail with the authentication error of section 8.

The PostgreSQL password:

1. Write the new password to the file of `storage.postgres.password_file`.
2. Change the password of the role in PostgreSQL. With CloudNativePG, change the Secret of the role.
3. Restart each instance, one at a time.

Between step 2 and step 3, the saves fail and the reads continue from the catalog.

A CA certificate of ClickHouse, PostgreSQL, the PDP or the collector:

1. Add the new CA certificate to the CA file, next to the old one. Restart each instance.
2. Change the certificate of the server.
3. Remove the old CA certificate from the file. Restart each instance.

### 14.22 Runbook: restore

Section 5 gives the subcommands `export`, `import` and `restore` and their options. The restore drill of section 16
tests the procedure A.

Procedure A, a whole store from an export:

1. Stop each instance.
2. Make an empty store: an empty directory for the volume, or a new database with `hune migrate-store`.
3. Run `hune import --config <path>` with the directory of the export.
4. Start the instances.
5. Compare the counts of folders and dashboards of `store.loaded` with the export.

Procedure B, one deleted dashboard, with `hune restore` and its uid (section 5):

- Volume: stop `serve`, because `hune restore` takes the lock of the volume. Run `hune restore`, then start `serve`.
- PostgreSQL: run `hune restore` while `serve` runs. The next poll shows the dashboard.

Procedure C, an old version of a dashboard: use the history of the page, or `POST /api/dashboards/{uid}/restore`. This
makes a new version and needs no stop.

Procedure D, PostgreSQL to a point in time:

1. Recover the CloudNativePG cluster into a new cluster.
2. Change `storage.postgres.host`, `storage.postgres.tls.server_name` and `storage.postgres.tls.ca_file` to the
   service and the CA of the new cluster.
3. Run `hune check-config` on the changed file.
4. Restart each instance.

Procedure E, the volume from git: run `git checkout` of the commit in the volume. The scan finds the changes and records each one as
an external change (section 5).

### 14.23 Runbook: PostgreSQL failover

With CloudNativePG, a lost primary gives 1 to 2 minutes with no primary. Hune does this during the failover:

- The catalog serves the lists, the dashboards and the panel SQL. The panel queries run.
- A save gets `503 storage_unavailable` with `Retry-After: 5`. The page keeps the draft (section 10).
- `hune_store_up` is 0, `hune_store_catalog_age_seconds` grows, and the log has `store.sync_failed`.
- `/readyz` stays 200.
- Each connection has `target_session_attrs=read-write`. A former primary that returns as a replica gets no write.
- After the promotion, the next poll succeeds. The log has `store.sync_recovered`.
- A save that the browser repeats with the same `Idempotency-Key` makes one version only.

The operator does these steps:

1. Do nothing in Hune during the failover. Watch `hune_store_up`.
2. When `hune_store_up` stays 0 for more than 5 minutes, check the state of the PostgreSQL cluster.
3. When the cluster has a primary and `hune_store_up` stays 0, check the network policy and the Secret of the role.
4. Restart one instance and read its log. A wrong password, an absent database or a failed certificate check stops it
   with `server.start_failed` (14.12). A network failure gives `store.sync_failed` while it retries.

### 14.24 Runbook: a stuck volume lock

The symptom: `serve` stops at start, and its error says that another process holds the lock `<root>/.hune/lock`
(section 5).

The kernel releases a `flock` when the process that holds it ends. A held lock thus means one of two things. A process
holds it now, or an NFS server keeps the lock of a client that crashed.

1. On the host of the volume, find the process that holds the lock: `fuser -v <root>/.hune/lock`.
2. When it is another Hune or a `hune import`, stop it. Two writers on one volume are not supported.
3. When no process holds it and the volume is NFS, wait for the lease time of the NFS server. The default lease of
   NFSv4 is 90 s. Then start Hune again.
4. Do not delete the lock file to start a second writer. A delete does not release the lock of a process that runs, and
   two writers then race.
5. When a volume must have more readers, give each reader the volume as a read-only store. A read-only store takes no lock. Several instances that write one primary volume need a shared volume and the lock above.
