# Hune configuration reference

This document is the reference of the configuration keys of Hune. The command `hune print-reference --format markdown` generates it from the key table in the code. Do not edit it by hand.

Hune reads one HCL file. The file is the only source of the configuration. Seven keys have a default value and can be absent: `server.query_inspect` (default `false`), `server.repeat_max_per_panel` (default 24), `server.repeat_max_per_section` (default 12), `server.repeat_max_per_dashboard` (default 120), `server.compress_static` (default `true`), `server.transform_max_cells` (default 10000000) and `server.list_filters_min_dashboards` (default 6). The key `clickhouse "<id>".adhoc_columns` is optional and has no default: absent, no ad hoc filter applies on that source. For each other key, a key that applies is necessary and a key that does not apply is forbidden.

The column Forms uses three letters. V is a value in the file. E is `<name>_env`, the name of an environment variable. F is `<name>_file`, the absolute path of a file. Give exactly one form for each key. A secret accepts only E and F.

A duration is a text with one whole number and one unit: `ms`, `s`, `m` or `h`. A size is a whole number of bytes. An integer is a whole number with no sign and no zero at the start. A text is in double quotes. A list is texts in brackets with commas.

The reference value is the value that the reference files use. It is not a default, except for the seven keys that have a default value. Run `hune print-reference --format hcl --profile local` or `--profile distributed` to get a complete file. These two files hold no `postgres` block, no `mongodb` block and no `geomap` block. The reference value of a `postgres` key, a `mongodb` key or a `geomap` key is a value for the example block of section 13.

Section 13 of the specification gives the checks after the key table, the contradictions and the warnings.

## Blocks

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
| `clickhouse "<id>"` | 0 to 64 | always | never |
| `clickhouse "<id>".tls` | 0 or 1 | the `url` of the block has the scheme `https` | the `url` of the block has the scheme `http` |
| `postgres "<id>"` | 0 to 64 | always | never |
| `postgres "<id>".tls` | 0 or 1 | `sslmode = "verify-full"` of the block | `sslmode = "disable"` of the block |
| `mongodb "<id>"` | 0 to 64 | always | never |
| `mongodb "<id>".tls` | 0 or 1 | `tls_mode = "verify-full"` of the block | `tls_mode = "disable"` of the block |
| `geomap` | 0 or 1 | never (the block is optional) | never |

## Block `server`

The two listeners and the limits of the server (section 14).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `server.listen` | an IP address and a port in canonical form | no | V | always | never | `"0.0.0.0:8080"` | The main listener (14.2). Give the canonical form of an IP address and a port. |
| `server.ops_listen` | an IP address and a port in canonical form | no | V | always | never | `"0.0.0.0:9090"` | The listener of the probes and the metrics (14.2). Its port is not the port of `server.listen`. |
| `server.base_path` | a text of 1 to 128 bytes with no control character | no | V | always | never | `"/"` | The prefix of the routes of `server.listen`. Give `/`, or a path that starts and ends with `/`. A segment has 1 to 64 characters from `A-Z a-z 0-9 . _ ~ -`, and is not `.` or `..`. |
| `server.query_inspect` | a boolean | no | V | never (the key is optional) | never | `false` | The value `true` adds the expanded SQL to the answer of a panel query, for the menu item Inspect (6.20). The key is optional. The default is `false`. |
| `server.repeat_max_per_panel` | an integer from 1 to 200 | no | V | never (the key is optional) | never | `24` | The most copies of one panel that repeats by a variable (4.13a). Above it, the page shows a message and no copy. The key is optional. The default is 24. The upper bound is the limit of panels of one document (4.35). |
| `server.repeat_max_per_section` | an integer from 1 to 50 | no | V | never (the key is optional) | never | `12` | The most copies of one section that repeats by a variable (4.13a). The key is optional. The default is 12. The upper bound is the limit of sections of one document (4.35). |
| `server.repeat_max_per_dashboard` | an integer from 1 to 200 | no | V | never (the key is optional) | never | `120` | The most copies of panels that repeat in one dashboard (4.13a). The key is optional. The default is 120. The upper bound is the limit of panels of one document (4.35). Contradictions C28 and C29 compare it with the two keys above. |
| `server.compress_static` | a boolean | no | V | never (the key is optional) | never | `true` | The value `true` sends the gzip copy of a static file of a text type when the request accepts gzip (9.3). Hune makes the copies one time at start. An API answer and the shell are never compressed. The key is optional. The default is `true`. |
| `server.transform_max_cells` | an integer from 1 to 100000000 | no | V | never (the key is optional) | never | `10000000` | The most cells (fields times rows) of the frame that one step of a transform makes in the page (4.35, 9.21a, rule 13). Above it, the panel shows an error and no frame. The session gives the value to the page (6.10). The key is optional. The default is 10000000. The upper bound is fixed. One cell takes 9 bytes or more: a double and a null flag. Thus 100000000 cells take 900 MB or more, near the memory of one browser tab. |
| `server.list_filters_min_dashboards` | an integer from 0 to 1000 | no | V | never (the key is optional) | never | `6` | The list page shows the sort control and the tag filter only when the user sees more dashboards than this number (9.7). With this number or fewer, the page hides them. The value 0 shows them for any list that is not empty. The session gives the value to the page (6.10). The key is optional. The default is 6. The upper bound is fixed at 1000. |
| `server.max_connections` | an integer from 16 to 65536 | no | V | always | never | `1024` | The most open connections on `server.listen`. |
| `server.idle_timeout` | a duration from 1s to 10m | no | V | always | never | `"75s"` | Hune closes a kept connection that is idle for this time. |
| `server.shutdown_grace` | a duration from 5s to 10m | no | V | always | never | `"30s"` | The longest graceful stop (14.13). |
| `server.max_request_bytes` | a size in bytes from 4259840 to 67108864 | no | V | always | never | `8388608` | The body limit of a dashboard create, save or import (12.16). The lowest value is the document limit of section 4 plus 65536. |
| `server.max_result_memory_bytes` | a size in bytes from 16777216 to 68719476736 | no | V | always | never | `536870912` | The memory budget of the results in flight (12.16). |

## Block `log`

The log level (14.6).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `log.level` | "error", "warn", "info" or "debug" | no | V | always | never | `"info"` | The lowest level that Hune writes (14.6). |

## Block `telemetry`

The export of the spans with OTLP (14.9).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `telemetry.enabled` | a boolean | no | V | always | never | `false` | The value `true` exports the spans with OTLP (14.9). |
| `telemetry.otlp_endpoint` | a text of 1 to 2048 bytes with no control character | no | V, E, F | `telemetry.enabled = true` | `telemetry.enabled = false` | `"http://otel-collector:4318"` | A URL with the scheme `http` or `https`, a host and an optional port. It has no path, query, fragment or user info. Hune sends to `<endpoint>/v1/traces`. |
| `telemetry.ca_file` | an absolute path of 1 to 4096 bytes | no | V | `telemetry.enabled = true` and an `https` endpoint | `telemetry.enabled = false`, or an `http` endpoint | absent | A PEM file of CA certificates (13.13). |
| `telemetry.service_name` | a text of 1 to 64 bytes of printable ASCII | no | V | `telemetry.enabled = true` | `telemetry.enabled = false` | `"hune"` | The resource attribute `service.name`. |
| `telemetry.export_interval` | a duration from 1s to 5m | no | V | `telemetry.enabled = true` | `telemetry.enabled = false` | `"15s"` | The period of the export. |

## Block `storage`

The store of the dashboards (section 5).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|

## Block `storage.volume "<id>"`

One volume store: one file for each dashboard. The label is the store id. The store with `primary = true` is writable. Each other store is read-only.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `storage.volume "<id>".path` | an absolute path of 1 to 4096 bytes | no | V | always | never | `"/var/lib/hune/dashboards"` (local), absent (distributed) | The directory of the dashboards. The path is canonical: it has no `.` or `..` segment and no repeated or last `/` (section 5). |
| `storage.volume "<id>".title` | a text of 1 to 128 bytes with no control character | no | V | the store is not the primary | the store is the primary | `"Lab"` | The name of the root folder of the store in the page. A read-only store needs it, and the primary store has none (section 5). |
| `storage.volume "<id>".primary` | a boolean | no | V | never (the key is optional) | another store has `primary = true` | `true` (local), absent (distributed) | The value `true` makes this store the primary store: the only store that accepts writes. At most one store has it. The key is optional (section 5). |
| `storage.volume "<id>".rescan_interval` | a duration from 1s to 1h | no | V | always | never | `"5s"` (local), absent (distributed) | The period of the scan for changes made outside Hune. |
| `storage.volume "<id>".history_keep` | an integer from 1 to 10000 | no | V | the store is the primary | the store is not the primary | `100` (local), absent (distributed) | The versions kept for each dashboard. Only the primary store has history. |

## Block `storage.postgres`

The PostgreSQL store: one row for each dashboard. The store with `primary = true` is writable. Each other store is read-only.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `storage.postgres.host` | a DNS name of 1 to 253 bytes, or an IP address | no | V | always | never | absent (local), `"hune-pg-rw"` (distributed) | The host of the PostgreSQL server. |
| `storage.postgres.port` | an integer from 1 to 65535 | no | V | always | never | absent (local), `5432` (distributed) | The port of the PostgreSQL server. |
| `storage.postgres.database` | a text of 1 to 63 bytes with no control character | no | V | always | never | absent (local), `"hune"` (distributed) | The name of the database. |
| `storage.postgres.user` | a text of 1 to 63 bytes with no control character | no | V, E, F | always | never | absent (local), `user_file = "/run/secrets/pg/username"` (distributed) | The name of the role of `serve`. |
| `storage.postgres.password` | a text of 1 to 1024 bytes | yes | E, F | always | never | absent (local), `password_file = "/run/secrets/pg/password"` (distributed) | The password of the role of `serve`. |
| `storage.postgres.sslmode` | "verify-full" or "disable" | no | V | always | never | absent (local), `"verify-full"` (distributed) | The value `verify-full` checks the certificate and the name (12.14). |
| `storage.postgres.max_connections` | an integer from 2 to 64 | no | V | always | never | absent (local), `8` (distributed) | The size of the connection pool. |
| `storage.postgres.connect_timeout` | a duration from 1s to 1m | no | V | always | never | absent (local), `"5s"` (distributed) | The longest wait for a connection. |
| `storage.postgres.statement_timeout` | a duration from 100ms to 1m | no | V | always | never | absent (local), `"10s"` (distributed) | The longest statement. Hune also uses it as `idle_in_transaction_session_timeout` (section 5). |
| `storage.postgres.poll_interval` | a duration from 1s to 5m | no | V | always | never | absent (local), `"5s"` (distributed) | The period of the poll of the catalog (section 5). |
| `storage.postgres.title` | a text of 1 to 128 bytes with no control character | no | V | the store is not the primary | the store is the primary | `"Archive"` | The name of the root folder of the store in the page. A read-only store needs it, and the primary store has none (section 5). |
| `storage.postgres.primary` | a boolean | no | V | never (the key is optional) | another store has `primary = true` | absent (local), `true` (distributed) | The value `true` makes this store the primary store: the only store that accepts writes. At most one store has it. The key is optional (section 5). |
| `storage.postgres.history_keep` | an integer from 1 to 10000 | no | V | the store is the primary | the store is not the primary | absent (local), `100` (distributed) | The versions kept for each dashboard. Only the primary store has history. |

## Block `storage.postgres.tls`

The TLS link to PostgreSQL (12.14).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `storage.postgres.tls.ca_file` | an absolute path of 1 to 4096 bytes | no | V | always | never | absent (local), `"/run/secrets/pg-ca/ca.crt"` (distributed) | A PEM file of CA certificates (13.13). |
| `storage.postgres.tls.server_name` | a DNS name of 1 to 253 bytes | no | V | always | never | absent (local), `"hune-pg-rw"` (distributed) | The name that the certificate must hold. |

## Block `access`

The access mode (section 11).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `access.mode` | "open" or "authzen" | no | V | always | never | `"open"` (local), `"authzen"` (distributed) | The access mode (11.2). |

## Block `access.identity`

The identity token of the edge (11.4).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `access.identity.header` | a text of 1 to 64 bytes with no control character | no | V | always | never | absent (local), `"X-Identity-Token"` (distributed) | The name of the header that holds the token: 1 to 64 token characters (RFC 9110). It is not `Authorization`, `Proxy-Authorization`, `Cookie`, `Host`, `Content-Type` or `Content-Length`, in any case. |
| `access.identity.issuer` | a text of 1 to 1024 bytes with no control character | no | V | always | never | absent (local), `"https://edge.example"` (distributed) | The value that the claim `iss` must have. |
| `access.identity.audience` | a text of 1 to 256 bytes with no control character | no | V | always | never | absent (local), `"dashboards"` (distributed) | The service id (11.14). It is an id of P 4.1: 1 to 256 bytes of UTF-8. |
| `access.identity.public_keys` | a text of 1 to 65536 bytes | no | V, E, F | always | never | absent (local), `public_keys_file = "/run/secrets/edge/jwks.json"` (distributed) | The public keys of the edge: PEM text or a JWK Set with 1 to 16 keys (11.9). |
| `access.identity.max_age` | a duration from 1s to 24h | no | V | always | never | absent (local), `"5m"` (distributed) | The longest life of a token. A whole number of seconds. |
| `access.identity.clock_skew` | a duration from 0s to 5m | no | V | always | never | absent (local), `"1m"` (distributed) | The skew that applies to `iat` and `exp`. A whole number of seconds. |

## Block `access.pdp`

The link to the policy decision point (11.5).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `access.pdp.url` | a text of 1 to 2048 bytes with no control character | no | V, E, F | always | never | absent (local), `"https://pdp.example:4182"` (distributed) | A URL with the scheme `http` or `https`, a host, an optional port and an optional path. It has no user info, query or fragment. |
| `access.pdp.token` | a text of 1 to 4096 bytes with no space and no control character | yes | E, F | always | never | absent (local), `token_file = "/run/secrets/pdp/token"` (distributed) | The credential of Hune at the PDP. Hune sends it in `Authorization: Bearer`. |
| `access.pdp.ca_file` | an absolute path of 1 to 4096 bytes | no | V | `access.pdp.plain_http = false` | `access.pdp.plain_http = true`, or `access.mode = "open"` | absent (local), `"/run/secrets/pdp/ca.pem"` (distributed) | A PEM file of CA certificates (13.13). |
| `access.pdp.plain_http` | a boolean | no | V | always | never | absent (local), `false` (distributed) | The value `true` accepts an `http` URL. It must agree with the scheme of `access.pdp.url`. |
| `access.pdp.timeout` | a duration from 10ms to 1m | no | V | always | never | absent (local), `"500ms"` (distributed) | The longest wait for the PDP. |
| `access.pdp.cache_ttl` | a duration from 0s to 1h | no | V | always | never | absent (local), `"30s"` (distributed) | The longest life of a kept answer. |
| `access.pdp.max_connections` | an integer from 1 to 32 | no | V | always | never | absent (local), `4` (distributed) | The most connections to the PDP. |

## Block `clickhouse "<id>"`

One ClickHouse server (section 8). The label is the source id.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `clickhouse "<id>".label` | a text of 1 to 240 bytes with no control character | no | V | always | never | `"Production"` | The label in the page and in the action document: 1 to 60 characters. |
| `clickhouse "<id>".url` | a text of 1 to 2048 bytes with no control character | no | V | always | never | `"https://clickhouse:8443"` | The HTTP interface of ClickHouse: a URL with the scheme `http` or `https`, a host and an optional port. It has no path, query, fragment or user info. |
| `clickhouse "<id>".user` | a text of 1 to 256 bytes with no control character | no | V, E, F | always | never | `"hune"` | The name of the ClickHouse account. |
| `clickhouse "<id>".password` | a text of 1 to 1024 bytes | yes | E, F | always | never | `password_file = "/run/secrets/clickhouse/password"` | The password of the ClickHouse account. |
| `clickhouse "<id>".database` | a text of 1 to 64 bytes with no control character | no | V | always | never | `"analytics"` | The default database of the queries: a ClickHouse identifier that matches `^[A-Za-z_][A-Za-z0-9_]{0,63}$`. |
| `clickhouse "<id>".connect_timeout` | a duration from 100ms to 1m | no | V | always | never | `"5s"` | The longest wait for a connection. |
| `clickhouse "<id>".max_execution_time` | a duration from 1s to 1h | no | V | always | never | `"1m"` | The longest run of a query (11.24, section 8). A whole number of seconds. |
| `clickhouse "<id>".max_result_rows` | an integer from 1 to 1000000000 | no | V | always | never | `100000` | The most rows of a result. |
| `clickhouse "<id>".max_result_bytes` | a size in bytes from 1048576 to 1073741824 | no | V | always | never | `33554432` | The most bytes of a result. |
| `clickhouse "<id>".max_query_size` | a size in bytes from 262144 to 16777216 | no | V | always | never | `1048576` | The largest expanded SQL. Hune also sends it as the setting `max_query_size` (section 7). |
| `clickhouse "<id>".max_concurrent` | an integer from 1 to 1024 | no | V | always | never | `16` | The most queries at one time on this source. |
| `clickhouse "<id>".queue_wait` | a duration from 0s to 1m | no | V | always | never | `"5s"` | The longest wait for a place (12.16). |
| `clickhouse "<id>".max_concurrent_per_subject` | an integer from 1 to 1024 | no | V | `access.mode = "authzen"` | `access.mode = "open"` | absent (local), `6` (distributed) | The most queries at one time for one subject on this source. It is at most `max_concurrent`. |
| `clickhouse "<id>".adhoc_columns` | a list of 1 to 256 texts, each of 1 to 64 bytes with no space and no control character | no | V | never (the key is optional) | never | `["service", "host", "env"]` | The columns that an ad hoc filter of this source can name (4.10c, 7.7a). Each item matches `^[A-Za-z_][A-Za-z0-9_]{0,63}$`, and no item is repeated. The key is optional, with no default: absent, no ad hoc filter applies on this source. Each listed column is exposed to each viewer of a dashboard of the source whose SQL has `$__adhocFilters` (12.9). |

## Block `clickhouse "<id>".tls`

The TLS link to one ClickHouse server (12.14).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `clickhouse "<id>".tls.ca_file` | an absolute path of 1 to 4096 bytes | no | V | always | never | `"/run/secrets/clickhouse/ca.pem"` | A PEM file of CA certificates (13.13). |
| `clickhouse "<id>".tls.server_name` | a DNS name of 1 to 253 bytes | no | V | always | never | `"clickhouse"` | The name that the certificate must hold. |

## Block `postgres "<id>"`

One PostgreSQL server for panel queries (section 8a). It is not the storage backend. The label is the source id and it is unique across both kinds of source.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `postgres "<id>".label` | a text of 1 to 240 bytes with no control character | no | V | always | never | `"Metrics"` | The label in the page and in the action document: 1 to 60 characters. |
| `postgres "<id>".host` | a DNS name of 1 to 253 bytes, or an IP address | no | V | always | never | `"hune-pg-ro"` | The one host that Hune connects to (8a.4): a DNS name or an IP address. A Unix socket path is not valid. |
| `postgres "<id>".port` | an integer from 1 to 65535 | no | V | always | never | `5432` | The port of the host. |
| `postgres "<id>".database` | a text of 1 to 63 bytes with no control character | no | V | always | never | `"analytics"` | The database of the connection. |
| `postgres "<id>".user` | a text of 1 to 63 bytes with no control character | no | V, E, F | always | never | `user_file = "/run/secrets/postgres-metrics/username"` | The name of the read-only role (8a.13). The role must not be the role of the store on the same server. |
| `postgres "<id>".password` | a text of 1 to 1024 bytes | yes | E, F | always | never | `password_file = "/run/secrets/postgres-metrics/password"` | The password of the read-only role. |
| `postgres "<id>".search_path` | a text of 1 to 256 bytes with no control character | no | V | always | never | `"metrics"` | 1 to 8 names separated by `,` with no blank. A name matches `^[a-z_][a-z0-9_]{0,62}$`, does not start with `pg_temp` and is not repeated. Hune sends it as the session value (8a.4). |
| `postgres "<id>".sslmode` | "verify-full" or "disable" | no | V | always | never | `"verify-full"` | The value `verify-full` checks the certificate and the name (12.14). |
| `postgres "<id>".connect_timeout` | a duration from 100ms to 1m | no | V | always | never | `"5s"` | The longest time for the connection, the TLS handshake, the login and the setup check (8a.4). |
| `postgres "<id>".max_execution_time` | a duration from 1s to 1h | no | V | always | never | `"1m"` | The longest run of a query (11.24, section 8a). A whole number of seconds. |
| `postgres "<id>".max_result_rows` | an integer from 1 to 1000000000 | no | V | always | never | `100000` | The most rows of a result. |
| `postgres "<id>".max_result_bytes` | a size in bytes from 1048576 to 1073741824 | no | V | always | never | `33554432` | The most bytes of a result. |
| `postgres "<id>".max_query_size` | a size in bytes from 262144 to 16777216 | no | V | always | never | `1048576` | The largest expanded SQL (section 7). Hune checks it itself. |
| `postgres "<id>".max_concurrent` | an integer from 1 to 256 | no | V | always | never | `16` | The most queries at one time on this source, and the size of its pool (8a.9). |
| `postgres "<id>".queue_wait` | a duration from 0s to 1m | no | V | always | never | `"5s"` | The longest wait for a place (12.16). |
| `postgres "<id>".max_concurrent_per_subject` | an integer from 1 to 256 | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `6` | The most queries at one time for one subject on this source. It is at most `max_concurrent`. |

## Block `postgres "<id>".tls`

The TLS link to one PostgreSQL source (12.14).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `postgres "<id>".tls.ca_file` | an absolute path of 1 to 4096 bytes | no | V | `sslmode = "verify-full"` | `sslmode = "disable"` | `"/run/secrets/postgres-metrics-ca/ca.crt"` | A PEM file of CA certificates (13.13). |
| `postgres "<id>".tls.server_name` | a DNS name of 1 to 253 bytes | no | V | `sslmode = "verify-full"` | `sslmode = "disable"` | `"hune-pg-ro"` | The name that the certificate must hold. |

## Block `mongodb "<id>"`

One MongoDB server or one replica set for panel queries (section 8b). The label is the source id and it is unique across the three kinds of source.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `mongodb "<id>".label` | a text of 1 to 240 bytes with no control character | no | V | always | never | `"Events"` | The label in the page and in the action document: 1 to 60 characters. |
| `mongodb "<id>".topology` | "direct" or "replica_set" | no | V | always | never | `"direct"` | The value `direct` is one host, with no discovery. The value `replica_set` is the complete list of the members (8b.4). |
| `mongodb "<id>".host` | a DNS name of 1 to 253 bytes, or an IP address | no | V | `topology = "direct"` | `topology = "replica_set"` | `"hune-mongo-ro"` | The one host that Hune connects to (8b.4): a DNS name or an IP address. A Unix socket path, a connection string and the scheme `mongodb+srv` are not valid. |
| `mongodb "<id>".port` | an integer from 1 to 65535 | no | V | `topology = "direct"` | `topology = "replica_set"` | `27017` | The port of the host. |
| `mongodb "<id>".hosts` | a list of 2 to 7 texts, each of 1 to 262 bytes with no space and no control character | no | V | `topology = "replica_set"` | `topology = "direct"` | `["mongo-0:27017", "mongo-1:27017", "mongo-2:27017"]` | The members of the replica set, all of them (8b.4). Each item is `host:port`, with an IPv6 address in brackets and a port from 1 to 65535. No item is repeated. |
| `mongodb "<id>".replica_set` | a text of 1 to 64 bytes with no control character | no | V | `topology = "replica_set"` | `topology = "direct"` | `"rs0"` | The name of the replica set. It matches `^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$`. |
| `mongodb "<id>".user` | a text of 1 to 128 bytes with no control character | no | V, E, F | always | never | `user_file = "/run/secrets/mongodb-events/username"` | The name of the read-only user (8b.13). |
| `mongodb "<id>".password` | a text of 1 to 1024 bytes | yes | E, F | always | never | `password_file = "/run/secrets/mongodb-events/password"` | The password of the read-only user. |
| `mongodb "<id>".auth_database` | a text of 1 to 63 bytes with no control character | no | V | always | never | `"admin"` | The database that holds the user. It matches `^[A-Za-z0-9_-]{1,63}$`. It can be `admin`. |
| `mongodb "<id>".databases` | a list of 1 to 8 texts, each of 1 to 63 bytes with no control character | no | V | always | never | `["analytics"]` | The databases that a query can read (7.33). Each item matches `^[A-Za-z0-9_-]{1,63}$` and is not `admin`, `local` or `config`. No item is repeated. |
| `mongodb "<id>".collections` | a list of 1 to 64 texts, each of 1 to 100 bytes with no control character | no | V | always | never | `["node_load", "node_events"]` | The collections that a query can name (7.33). Each item matches `^[A-Za-z0-9_][A-Za-z0-9_.-]{0,99}$` and does not start with `system.`. No item is repeated. |
| `mongodb "<id>".read_preference` | "primary", "primaryPreferred", "secondary", "secondaryPreferred" or "nearest" | no | V | always | never | `"secondaryPreferred"` | The read preference of each command (8b.5). |
| `mongodb "<id>".tls_mode` | "verify-full" or "disable" | no | V | always | never | `"verify-full"` | The value `verify-full` checks the certificate and the host name (12.14). |
| `mongodb "<id>".connect_timeout` | a duration from 100ms to 1m | no | V | always | never | `"5s"` | The longest time for the connection, the TLS handshake, the login and the server selection (8b.4). |
| `mongodb "<id>".max_execution_time` | a duration from 1s to 1h | no | V | always | never | `"1m"` | The longest run of a query (11.24, section 8b). A whole number of seconds. |
| `mongodb "<id>".max_result_rows` | an integer from 1 to 1000000000 | no | V | always | never | `100000` | The most rows of a result. |
| `mongodb "<id>".max_result_bytes` | a size in bytes from 1048576 to 1073741824 | no | V | always | never | `33554432` | The most bytes of a result. |
| `mongodb "<id>".max_query_size` | a size in bytes from 262144 to 8388608 | no | V | always | never | `1048576` | The largest BSON size of the expanded command (7.35). Hune checks it itself. |
| `mongodb "<id>".max_concurrent` | an integer from 1 to 256 | no | V | always | never | `16` | The most queries at one time on this source (8b.9). |
| `mongodb "<id>".queue_wait` | a duration from 0s to 1m | no | V | always | never | `"5s"` | The longest wait for a place (12.16). |
| `mongodb "<id>".max_concurrent_per_subject` | an integer from 1 to 256 | no | V | `access.mode = "authzen"` | `access.mode = "open"` | `6` | The most queries at one time for one subject on this source. It is at most `max_concurrent`. |

## Block `mongodb "<id>".tls`

The TLS link to one MongoDB source (12.14).

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `mongodb "<id>".tls.ca_file` | an absolute path of 1 to 4096 bytes | no | V | `tls_mode = "verify-full"` | `tls_mode = "disable"` | `"/run/secrets/mongodb-events-ca/ca.crt"` | A PEM file of CA certificates (13.13). |

## Block `geomap`

The tile source of the panel type `geomap` (13.12c). The block is optional. Without it, the page contacts no tile host, and a map draws its markers on a plain background.

| Key | Type and range | Secret | Forms | Necessary when | Forbidden when | Reference value | Description |
|---|---|---|---|---|---|---|---|
| `geomap.tile_url` | a text of 16 to 512 bytes of printable ASCII | no | V | the block `geomap` exists | never | `"https://tiles.example.org/osm/{z}/{x}/{y}.png"` | The template of the URL of a tile (13.12c). It has `https://`, a lower-case DNS name with a dot, an optional port and a path. The path holds `{z}`, `{x}` and `{y}` once each. An optional fixed query can follow. The page loads each tile as an image. The directive `img-src` gets the host and the path up to the last `/` before the first placeholder (12.8). |
| `geomap.attribution` | a text of 1 to 800 bytes with no control character | no | V | the block `geomap` exists | never | `"Map tiles: example.org"` | The attribution of the tiles: 1 to 200 characters, with no bidirectional control, no zero-width character and no line or paragraph separator. The page shows it as text, with no link. |
| `geomap.max_zoom` | an integer from 0 to 22 | no | V | the block `geomap` exists | never | `19` | The highest zoom of a map. The lowest zoom is 0. |

