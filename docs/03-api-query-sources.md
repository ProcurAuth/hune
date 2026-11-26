# Part 3: HTTP API, query engine and data sources

Part 3 of the Hune specification (see `01-overview.md`).

This part holds section 6 (HTTP API and error model), section 7 (query engine), section 8 (data sources) and section 8a (the PostgreSQL data source). Section 11 gives the access question of each route. Section 13 gives the configuration keys. Section 4 gives
the document format and its constants. Section 5 gives the store.

## 6. HTTP API and error model

### 6.1 Listeners and base path

Hune has two listeners (14). The main listener (`server.listen`) serves the pages, the static files and the API. The
ops listener (`server.ops_listen`) serves `/livez`, `/readyz` and `/metrics` only (6.26). No route is on both listeners.

On the main listener, each path in this section is relative to `server.base_path`. Example: with
`server.base_path = "/dashboards/"`, the route `GET /api/session` has the full path `/dashboards/api/session`. The
proxy sends the full path to Hune and does not remove the prefix. A request whose path does not start with
`server.base_path` gets `404 not_found`.

The API has no version prefix. The user interface ships in the same binary, so the page and the API always match.

### 6.2 General rules

- A request body and a response body are JSON (RFC 8259, UTF-8), with the media type `application/json`.
- A request body has a closed schema. An unknown member, a duplicate member, a wrong type or an absent necessary member
  gives `400 bad_request`. The error lists each fault with its JSON pointer (RFC 6901).
- The JSON depth of a request body is 64 at most. A deeper body gives `400 bad_request`.
- The member `document` of a create, a save or a `validate_only` request is different. Hune checks it with 4.36, from
  its own bytes in the body. Its faults give `422 invalid_document`, with pointers relative to the document. Their
  `line` and `column` count in the text of the document. The rules above apply to the other members of the body.
- A preview body (6.22) is checked as a whole: its pointers are relative to the body, for example `/panel/title`.
- Member names use `snake_case`. A time of an event (`saved_at`, `updated_at`) is RFC 3339 text in UTC with
  milliseconds, for example `2026-11-09T10:15:03.120Z`. A time range is two integers of epoch milliseconds (7.3).
- An API route accepts only the query parameters that its paragraph names. Each parameter occurs one time at most. An
  unknown or repeated parameter gives `400 bad_request`. The page routes ignore unknown parameters (9.5).
- Each `GET` route also answers `HEAD`, with the same status and headers and no body.
- A method that a path does not list gives `405 method_not_allowed` with the header `Allow`. `OPTIONS` always gives 405,
  because Hune grants no CORS request.
- Hune never sets a cookie and never reads one.

### 6.3 Request headers

| Header | Routes | Rule |
|---|---|---|
| `Content-Type` | each route with a body | `application/json`, with an optional `charset=utf-8`. Another value gives `415 unsupported_media_type`. |
| `If-Match` | `PUT`, `DELETE /api/dashboards/{uid}`, `restore`, `move` | One strong entity tag (6.5). Absent: `428 precondition_required`. |
| `If-None-Match` | `GET /api/dashboards/{uid}` | Optional. One strong entity tag. Equal to the current etag: `304`, no body. |
| `Idempotency-Key` | `POST /api/dashboards`, `PUT`, `DELETE /api/dashboards/{uid}`, `restore`, `move` | The `save_id` (6.6). Absent: `428 precondition_required`. |
| identity header | each route in mode `authzen`, except the action document | The name is `access.identity.header` (11). |
| `X-Request-Id` | each route | Kept when it has 1 to 128 characters of `[A-Za-z0-9._:-]` (14.6). Else Hune makes a new id. |
| `Sec-Fetch-Site`, `Origin` | `POST`, `PUT`, `PATCH`, `DELETE` | The cross-site guard of 12.7 reads them (6.28). No route takes `PATCH`, so it gets `405` at step 3 of 6.28. |
| `Accept-Encoding` | `GET /static/...` only | With `server.compress_static = true`, a static file of a text type comes as its gzip copy when the header accepts gzip (9.3). Each other route ignores the header and sends no compressed answer. |

### 6.4 Response headers

| Header | Routes | Value |
|---|---|---|
| `Content-Type` | each answer with a body | `application/json` for the API. 9.3 gives the types of the static files. |
| `Cache-Control` | each API answer | `no-store`. 6.25 gives the value for pages and static files. |
| `ETag` | the routes that return a document or a new version | `"<etag>"`: the etag of section 4 in double quotes. |
| `X-Request-Id` | each answer | The request id of 6.3. |
| `X-Hune-Build` | each API answer | The build hash of 9.3. The page compares it with its own build hash (9.6). |
| `Retry-After` | `503 server_busy`, `503 source_busy`, `503 storage_unavailable` | Seconds: `1` for `server_busy` and `source_busy`, `5` for `storage_unavailable` (14). |
| `Idempotent-Replayed` | a replayed answer (6.6) | `true`. |
| `Location` | `201` of `POST /api/dashboards` | The relative path of the new dashboard. `POST /api/folders` sends no `Location`, because no route reads one folder. |
| security headers | each answer | Section 12 gives the list and the values. 9.4 gives the CSP. |

### 6.5 ETag and If-Match

The etag of a dashboard is the SHA-256 of its canonical bytes, as 64 lower-case hexadecimal characters (4.38, 4.39). In
a JSON body, the etag is this text with no quotes. In the headers `ETag`, `If-Match` and `If-None-Match`, the etag is
in double quotes: `"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"`.

Hune accepts one strong entity tag only. The forms `*`, `W/"..."`, a list of tags, or a text that is not 64
lower-case hexadecimal characters give `400 bad_request`. The comparison is a byte comparison of the 64 characters.

A move (6.19) does not change the document bytes, so it does not change the etag. Each other write that changes the
bytes gives a new etag.

### 6.6 Idempotency-Key

The header `Idempotency-Key` carries the `save_id` of a write. The value is a UUID in its canonical text form: 36
characters, lower-case hexadecimal, with the four `-`. Another form gives `400 bad_request`. The browser makes one `save_id` for
each write attempt and sends the same value when it repeats this attempt (10.9).

A `save_id` belongs to one uid. The store keeps it with the version that the write made (5.2). It keeps it as long as
the version stays in the history.

When a write arrives, Hune first finds its `save_id` in the history of the uid, if it is there. It does this under the
per-uid lock or in the transaction of the write (5.5, rules 1 and 3):

1. Not found: Hune does the normal checks (If-Match, then the write).
2. Found, and the request is the same request: Hune does not write. It answers the status and the body of the first
   answer, with the header `Idempotent-Replayed: true`.
3. Found, and the request is a different request: `422 idempotency_key_reused`.

Two requests are the same request when they have the same route and the same effect. For a create, a save and a
restore, the effect is the canonical etag of the document. For a move, it is the target folder. A delete has no other
value (5.5, rule 4). This rule makes a repeated write safe after a lost answer, for example during a PostgreSQL
failover (5.31).

A repeated delete is the exception. After the first delete, the uid is not in the catalog, so the repeated request
answers as for an unknown uid (6.9): `404 not_found`, or `403 forbidden` in mode `authzen`. The page counts a `404` on
the retry of a delete with the same `save_id` as a success (10.13).

### 6.7 Error body

Each error answer has this body:

```json
{
  "error": {
    "code": "conflict",
    "message": "The dashboard changed after you opened it.",
    "retryable": false,
    "current_etag": "9f2c5e0b2d1c4a7f8e6d3b2a1c0f9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f",
    "current_version": 8,
    "current_folder": "transit",
    "saved_at": "2026-11-09T10:15:03.120Z",
    "saved_by": "user:bob"
  }
}
```

- `code` is one code of the closed table of 6.8. The HTTP status is always the status of the code in that table.
- `message` is English text for a person, 1,000 bytes at most. It never holds a secret, a token, a password or a row of
  data. For the codes of a refusal of a source, it holds the first line of the ClickHouse message (8.5) or of the PostgreSQL message (8a.8).
- `retryable` is the value of the table. `true` tells that the same request can succeed later with no change.
- The other members are the extra members of the table, for this code only. No other member is present.

An error that `HEAD` gets has the same status and no body. A `304` has no body.

### 6.8 Error codes

This table is the closed list of the error codes of Hune. The other parts use only these codes. A new code needs a
change of this table. The code `editing_disabled` is removed. A file with `storage.read_only = true` refuses each write with `403 read_only_store` (13.10).

| Code | Status | Retryable | Cause | Extra members |
|---|---|---|---|---|
| `bad_request` | 400 | false | Bad JSON, a schema fault of the body, a bad query parameter or a bad header value. | `errors`: list of errors of 4.37 (optional) |
| `invalid_options_request` | 400 | false | A bad query of `GET /.well-known/procurauth/resources` (11). | none |
| `unauthenticated` | 401 | false | Mode `authzen`: no identity token, or a token that is not valid (11). | none |
| `forbidden` | 403 | false | The PDP answered `false`, the rule of 11.23 for an unknown uid, or a route with no rule (11.21). | `action` |
| `ceiling_exceeded` | 403 | false | A query stopped at a limit that a ProcurAuth ceiling lowered (8.5, 11.24). | `action`, `ceiling`, `limit` |
| `history_disabled` | 403 | false | A history route (6.17, 6.18) when `server.history` is `false` (13.8). No PDP question, and the answer does not depend on the uid. | none |
| `export_disabled` | 403 | false | A read with `download=true` (6.14) when `server.export` is `false` (13.8). No PDP question, and the answer does not depend on the uid. | none |
| `read_only_store` | 403 | false | A write whose target is in a read-only store (5.42, 11.35). Hune asks `dashboard.read` of the target first, and it never asks `dashboard.write`. | none |
| `cross_site_request` | 403 | false | The cross-site guard refused a `POST`, `PUT`, `PATCH` or `DELETE` (12.7, 6.28). | none |
| `not_found` | 404 | false | An unknown path, uid, folder, version, panel, variable or source. | none |
| `history_unavailable` | 404 | false | A history route for a dashboard of a read-only store. Only the primary store has history (5.18, 5.42). Hune asks `dashboard.read` first, so the answer tells nothing to a subject that cannot read the dashboard. | none |
| `method_not_allowed` | 405 | false | The path does not take this method. The answer has `Allow`. | none |
| `request_timeout` | 408 | false | The client did not send the whole body within 30 s (6.27). Hune then closes the connection. | none |
| `conflict` | 409 | false | `If-Match` differs from the current etag, or the folder of the dashboard is not the folder of the access decision (`expected_folder`, 5.5 rule 2, 11.17). | `current_etag`, `current_version`, `current_folder`, `saved_at`, `saved_by` |
| `already_exists` | 409 | false | A create names a uid that exists, also the uid of a deleted dashboard, or a folder id that exists (5.6, `Exists`). | none |
| `folder_not_empty` | 409 | false | A delete of a folder that holds a dashboard. | none |
| `dashboard_changed` | 409 | false | A query names an etag that is not the current etag (7.15). | `current_etag` |
| `payload_too_large` | 413 | false | The body is larger than the limit of the route (6.27). | `limit` |
| `uri_too_long` | 414 | false | The request target is longer than 8 KiB. | none |
| `unsupported_media_type` | 415 | false | A body without the type `application/json`. | none |
| `invalid_document` | 422 | false | The document breaks a rule of section 4 (4.36) or of 7.18. | `errors`: list of errors of 4.37 |
| `stored_document_invalid` | 422 | false | The stored document or version cannot be read with the rules of section 4 (5.6, `Invalid`). | `errors`: list of errors of 4.37 |
| `not_restorable` | 422 | false | A restore of a deletion version or of a version that cannot be read. | none |
| `idempotency_key_reused` | 422 | false | The `save_id` belongs to a different request (6.6). | none |
| `invalid_time_range` | 422 | false | A time range that breaks 7.3. | none |
| `variable_value_refused` | 422 | false | A variable value that the rules of 7.7 or 7.8 refuse. | `variable`, `reason` |
| `sql_invalid` | 422 | false | The SQL fails the lexer or the statement check (7.5, 7.12), or the query of a `mongodb` source fails the parser or the grammar (7.28 to 7.33, 7.35). | `line`, `column` (optional) |
| `sql_too_large` | 422 | false | The expanded SQL is larger than `max_query_size` (7.13), or the command of a `mongodb` source is (7.35, step 7). | `size`, `limit` |
| `query_limit` | 422 | false | The query stopped at a row, byte or read limit of the source (8.5, 8a.8). | `limit`, `value` |
| `annotation_invalid` | 422 | false | The result of an annotation query breaks a rule of 7.37: a column, a value, a character, a size or a form of text. The answer holds no value of the result. | `reason` (a text of the closed list of 7.37), `row` (the row, from 1, or `null`) |
| `query_memory_limit` | 422 | false | The source stopped the query at its memory limit: ClickHouse code 241, PostgreSQL SQLSTATE 53200 or 53400, or MongoDB code 146 or 292 (8.5, 8a.8, 8b.8). | none |
| `query_error` | 422 | false | The source refused the query for another reason (8.5, 8a.8, 8b.8). | `source`, `query_id`, and `clickhouse_code` for a ClickHouse source, `sqlstate` for a PostgreSQL source or `mongo_code` for a MongoDB source |
| `precondition_required` | 428 | false | A write without `If-Match` or without `Idempotency-Key`. | `header` |
| `headers_too_large` | 431 | false | The request has more than 100 headers, or its headers are larger than 16 KiB. | none |
| `internal_error` | 500 | false | A fault of Hune. The log line `http.request` has the details (14). | none |
| `source_access_denied` | 502 | false | ClickHouse code 497, PostgreSQL SQLSTATE 42501 or MongoDB code 13: the account has no grant for an object of the query. | `source`, `query_id` |
| `source_misconfigured` | 502 | false | The account breaks the contract of 8.12, 8a.13 or 8b.13, the login failed, or the certificate check failed. | `source` |
| `source_bad_response` | 502 | false | The answer of the source does not follow 8.4 or 7.16a. | `source`, `query_id` |
| `pdp_unavailable` | 503 | true | The PDP gave no valid answer (11). | none |
| `server_busy` | 503 | true | The listener has `server.max_connections` open connections, the places for bodies in flight are full, or no validation permit comes in time (6.27, 12.16). The answer has `Retry-After: 1`, and Hune closes the connection. | none |
| `server_stopping` | 503 | true | The process stops (14.13). The answer has `Connection: close`. | none |
| `storage_unavailable` | 503 | true | The store cannot do the operation now (5.6, `Unavailable`). The answer has `Retry-After: 5`. | none |
| `source_unavailable` | 503 | true | Hune cannot reach the source, or the source is overloaded (8.5, 8a.8). | `source` |
| `source_busy` | 503 | true | The admission of 8.8 refused the query. The answer has `Retry-After: 1`. | `source` |
| `query_timeout` | 504 | false | The query ran longer than its time limit (8.5, 8.9, 8a.7). | `source`, `query_id` |
| `storage_full` | 507 | false | The store has no free space (`ENOSPC`, `EDQUOT`), or a limit of 5.7 is reached (5.6, `Full`). | none |

Each item of the member `errors` has the form of 4.37: `pointer`, `code`, `message`, `line` and `column`. The list
holds at most `MAX_ERRORS` items (4.35). When the validation stops there, the last item has the code
`too_many_errors` (4.36). The base of `pointer` and of `line` and `column` depends on the case (6.2):

| Case | Base |
|---|---|
| `invalid_document` of a create, a save or `validate_only` | The member `document`: the pointer `/uid` names its member `uid`, and `line` and `column` count in its text. |
| `invalid_document` of a preview, and `bad_request` | The request body, for example `/panel/title`. |
| `stored_document_invalid` | The stored document or version. |

The members of `query_error` are `source`, `query_id` and one of `clickhouse_code`, `sqlstate` and `mongo_code`, by the kind of the source. The three members never appear together. The member `sqlstate` or `mongo_code` is `null` when the error comes from Hune and not from the server (8a.8, 8b.8). The member `clickhouse_code` is `null` when a proxy in front of ClickHouse refuses the request with the status 414 or 431 (8.5).

The member `reason` of `annotation_invalid` has one of seven values, a closed list: `no_time`, `bad_time`, `bad_end`, `bad_text`, `bad_chars`, `too_long` and `duplicate_column`. 7.37 gives each one. The member `row` is the row of the first fault, counted from 1. It is `null` for a fault of a column. The answer never holds the text of a cell.

The member `action` of `forbidden` is the action of the use of the route (11.21, 11.30): `dashboard.read`,
`dashboard.write` or `source.query`. It does not name the question that the PDP refused. The rule of 11.23 gives the
same value. Thus a `403` does not show whether a uid or a folder exists. For a route with no rule, it is `unknown`.

The member `current_folder` of `conflict` is the current folder of the dashboard. When it differs from the folder of
the page, the conflict comes from a move (10.10).

The extra members of `ceiling_exceeded` are those of 11.24:

| Member | Value |
|---|---|
| `action` | The action whose answer gave the ceiling: `dashboard.read` for a panel or an options query, `source.query` for a preview. |
| `ceiling` | `max_rows` or `max_duration`. |
| `limit` | The effective value that stopped the query: the rows, or the seconds. |

Example: `{"error": {"code": "ceiling_exceeded", "message": "The result is larger than a limit: 1000 rows or 33554432
bytes.", "retryable": false, "action": "dashboard.read", "ceiling": "max_rows", "limit": 1000}}`.

### 6.9 Route table

`N(uid)` is the node `service / folder:<folder of uid> / dashboard:<uid>` of section 11. "Use" is the permission of
the operation. "Others" are the other permissions (protocol 5.5). Section 11 gives the exact questions. In mode
`open`, no route asks a question.

| Method | Path | Purpose | Access (11) | Success |
|---|---|---|---|---|
| `GET` | `/api/session` | Mode, subject and limits for the page | none (token in `authzen`) | 200 |
| `GET` | `/api/folders` | List the folders | display questions, `dashboard.read` and `dashboard.write` | 200 |
| `POST` | `/api/folders` | Create a folder | use `dashboard.write` on the service | 201 |
| `PUT` | `/api/folders/{id}` | Rename a folder | use `dashboard.write` on `folder:{id}` | 200 |
| `DELETE` | `/api/folders/{id}` | Delete an empty folder | use `dashboard.write` on `folder:{id}` | 204 |
| `GET` | `/api/dashboards` | List the dashboards | display questions, `dashboard.read` and `dashboard.write` | 200 |
| `POST` | `/api/dashboards` | Create a dashboard | use `dashboard.write` on `folder:f`; others `source.query` on each source of the document | 201 |
| `GET` | `/api/dashboards/{uid}` | Read a dashboard | use `dashboard.read` on `N(uid)` | 200, 304 |
| `PUT` | `/api/dashboards/{uid}` | Save a new version | use `dashboard.write` on `N(uid)`; others `source.query` on each source of the document | 200 |
| `DELETE` | `/api/dashboards/{uid}` | Delete (deletion version) | use `dashboard.write` on `N(uid)` | 200 |
| `GET` | `/api/dashboards/{uid}/history` | List the versions | use `dashboard.read` on `N(uid)` | 200 |
| `GET` | `/api/dashboards/{uid}/history/{n}` | Read one version | use `dashboard.read` on `N(uid)` | 200 |
| `POST` | `/api/dashboards/{uid}/restore` | Restore a version as a new version | use `dashboard.write` on `N(uid)`; others `source.query` on each source of the restored document | 200 |
| `POST` | `/api/dashboards/{uid}/move` | Move to another folder | use `dashboard.write` on `folder:f2`; others `dashboard.write` on `N(uid)` | 200 |
| `POST` | `/api/dashboards/{uid}/panels/{id}/query` | Run the stored query of a panel | use `dashboard.read` on `N(uid)`, with its ceilings | 200 |
| `POST` | `/api/dashboards/{uid}/variables/{name}/options` | Run the options query of a variable | use `dashboard.read` on `N(uid)`, with its ceilings | 200 |
| `POST` | `/api/dashboards/{uid}/annotations/{index}/query` | Run the stored query of an annotation | use `dashboard.read` on `N(uid)`, with its ceilings | 200 |
| `GET` | `/api/sources` | List the sources for the editor | display questions, `source.query` | 200 |
| `POST` | `/api/sources/{id}/preview` | Run a draft panel or draft variable | use `source.query` on `source:{id}`, with its ceilings; others `dashboard.write` on `N(uid)`, or on `folder:f` for a new dashboard | 200 |
| `GET` | `/.well-known/procurauth/actions` | Action document | none, no token (protocol 5.1) | 200 |
| `GET` | `/.well-known/procurauth/resources` | Options route | the questions of protocol 5.6 | 200 |
| `GET` | `/` | List page (shell) | none (token in `authzen`) | 200 |
| `GET` | `/d/{uid}` and `/d/{uid}/{slug}` | Dashboard page (shell) | none (token in `authzen`) | 200 |
| `GET` | `/static/{build-hash}/{path}` | Embedded static file | none (token in `authzen`) | 200 |

In mode `open`, the two `/.well-known/procurauth/` routes answer `404 not_found` (11). Another path under `api/`,
`static/` or `.well-known/` answers the JSON error `404 not_found`. Each other path of the main listener answers the
shell with the status 404 (6.25).

An unknown uid follows the rule of section 11. In mode `authzen`, Hune asks `dashboard.read` on the service: `false`
gives `403 forbidden` and `true` gives `404 not_found`. This `403` has the status, the headers and the body of the
`403` for a uid that exists (11.23). In mode `open`, an unknown uid gives `404 not_found`.

In mode `authzen`, Hune asks this question at each request, with no kept answer (11.27). On a write route, the other
permissions of the route that name only items that exist come first, as for a uid that exists.

The write routes are: `POST`, `PUT` and `DELETE` of folders and dashboards, `restore`, `move` and `preview`. The query,
options and annotation routes use `POST`, but they are reads. The check for a primary store (6.28, rule 6) does not apply to them.

### 6.10 Session

`GET /api/session` gives the values that the page needs at start. It asks the PDP nothing. In mode `authzen` it needs a
valid identity token (11). The page calls it one time at load (9.6).

```json
{
  "mode": "authzen",
  "subject": "user:alice",
  "editing": true,
  "stores": [
    { "id": "main", "kind": "volume", "title": null, "primary": true, "read_only": false },
    { "id": "lab", "kind": "volume", "title": "Lab", "primary": false, "read_only": true }
  ],
  "query_inspect": false,
  "repeat_limits": { "panel": 24, "section": 12, "dashboard": 120 },
  "transform_max_cells": 10000000,
  "list_filters_min_dashboards": 6,
  "geomap": null,
  "version": "0.3.2",
  "build": "3f2a9c1e5b7d4a60",
  "refresh_choices": ["10s", "30s", "1m", "5m", "15m", "30m", "1h", "2h", "1d"],
  "limits": {
    "document_bytes": 4194304,
    "sql_bytes": 262144,
    "variable_values": 10000,
    "value_bytes": 1024,
    "max_data_points": 10000,
    "query_body_bytes": 262144
  }
}
```

| Member | Value |
|---|---|
| `mode` | `open` or `authzen`, the value of `access.mode`. |
| `subject` | The `sub` of the identity token in mode `authzen`. The text `-` in mode `open`. |
| `editing` | Always `true`: the start stops unless exactly one store has `primary = true` (13.10). The member stays for the page, and a later version removes it. It does not give the grants of the subject: the `access` objects do (6.11, 6.12, 6.14). |
| `stores` | The stores, the volumes in file order and then the PostgreSQL store. Each item has five members: `id` (the label of the volume, or `postgres`), `kind` (`volume` or `postgres`), `title` (the name of the root folder of a read-only store, or `null` for the primary store), `primary` and `read_only` (the opposite of `primary`). The values are public. The page shows the title of a read-only store as the name of a root folder, and it offers a new folder and a new dashboard only in the primary store (9.7). |
| `query_inspect` | Boolean. The value of `server.query_inspect`. The page shows the menu item Inspect only when it is `true` (9.11). |
| `repeat_limits` | An object with three integers: `panel`, `section` and `dashboard`. They are the values of `server.repeat_max_per_panel`, `server.repeat_max_per_section` and `server.repeat_max_per_dashboard` (13.8). The page checks the copies of a repeat with them (9.8c). The object has no other member. The server never trusts the page: it applies its own limits of 8.8 to each query. |
| `transform_max_cells` | Integer, 1 to 100000000. The value of `server.transform_max_cells` (13.8). The page checks the cells of a step of a transform with it (`MAX_TRANSFORM_CELLS` of 4.35, 9.21a, rule 13). The page keeps no copy of the number. A session with no such member, or with a value that is not a positive integer, makes each step that makes cells fail with an error (fail closed). |
| `list_filters_min_dashboards` | Integer, 0 to 1000. The value of `server.list_filters_min_dashboards` (13.8). The list page shows the sort control and the tag filter only when the user sees more dashboards than this number (9.7). The page keeps no copy of the number. A session with no such member, or with a value that is not a non-negative integer, makes the page show the filters (it never hides a control because of a bad value). |
| `geomap` | `null` when the configuration has no block `geomap` (13.12c). Else an object with three members: `tile_url` (the text of `geomap.tile_url`), `attribution` and `max_zoom` (an integer from 0 to 22). The object has no other member. The values are public. The page checks `tile_url` again with the grammar of 13.13, and a value that fails gives no tile source (9.19i). The answer is the same in the modes `open` and `authzen`, and it holds no token. |
| `version` | The version of the binary. |
| `build` | The build hash of 9.3. |
| `refresh_choices` | The refresh values that the page offers (9.9). |
| `limits` | Constants: the document and SQL limits of 4.35, the value limits of 7.7, the range of 7.4 and the body limit of 6.27. The example shows example values. The editor checks sizes before it sends a request. |

Errors: `401 unauthenticated`.

### 6.11 Folders

`GET /api/folders` lists the folders, in byte order of the id. It has no pagination: the folder limit `MAX_FOLDERS`
of 5.7 bounds the list.

```json
{ "access": { "create": true },
  "root": { "access": { "write": true }, "read_only": false, "store": "main" },
  "items": [ { "id": "general", "title": "General", "access": { "write": true }, "read_only": false, "store": "main" },
             { "id": "transit", "title": "Transit desk", "access": { "write": false }, "read_only": true, "store": "lab" } ] }
```

| Member | Value |
|---|---|
| `access.create` | At the top level. `true` when the subject can create a folder: the display answer for `dashboard.write` on the service (11.25). |
| `items[].access.write` | On each folder. `true` when the subject can write in this folder: the display answer for `dashboard.write` on the folder (11.25). It is `false` for a folder of a read-only store, with no question to the PDP. |
| `items[].read_only` | `true` when the store of the folder is read-only (5.42). The page shows a lock and no edit control. |
| `items[].store` | The id of the store of the folder: the label of a volume, or `postgres`. |
| `root` | The place for the dashboards without a folder. It is not a folder: it is not in `items`, and it has no id and no title. `root.access.write` is `true` when the subject can create a dashboard without a folder in the primary store: the display answer for `dashboard.write` on the node `_root` (11.25). `root.read_only` is `false`, and `root.store` is the id of the primary store. |

In mode `authzen`, the list holds only the folders that the display filter keeps (11.25). In mode `open`, the list
holds every folder. In mode `open`, `access.create` is `true`, `access.write` is `true` for each folder of the primary store, and `false` for each other folder.

`POST /api/folders` creates a folder in the primary store. The body is `{"id": "transit", "title": "Transit desk"}`. The id rule is in 4.3
and the title rule in 4.41. The answer is `201` with the body `{"id", "title"}`. An id that another store holds, or a title that
a read-only store has as its `title`, gives `409 already_exists` (5.42, rule 7).

`PUT /api/folders/{id}` renames a folder. The body is `{"title": "Transit desk"}`. The answer is `200` with the body
`{"id", "title"}`. A rename has no version check: the last rename wins. It changes no dashboard and no etag.

`DELETE /api/folders/{id}` deletes an empty folder. It has no body. The answer is `204`. A folder that holds a dashboard
gives `409 folder_not_empty`.

Errors of the folder routes: `400 bad_request`, `401 unauthenticated`, `403 forbidden`,
`403 read_only_store`, `403 cross_site_request`, `404 not_found`, `409 already_exists`, `409 folder_not_empty`, `413 payload_too_large`,
`415 unsupported_media_type`, `503 pdp_unavailable`, `503 storage_unavailable`, `507 storage_full`.

### 6.12 List of dashboards

`GET /api/dashboards` lists the dashboards. The member `folder` of an item is the folder id, or `null` for a dashboard without a folder. This holds for each route that gives the folder of a dashboard: 6.13 to 6.15, 6.17 and 6.19, and the member `current_folder` of 6.8. The query parameters are:

| Parameter | Rule |
|---|---|
| `folder` | Optional. A folder id. The list holds only the dashboards of this folder. An unknown folder gives an empty list. The value empty (`?folder=`) selects the dashboards without a folder. |
| `q` | Optional, 1 to 100 bytes. The list holds the dashboards whose title or uid holds this text. The comparison folds ASCII letters to lower case. |
| `tag` | Optional, 1 to 64 bytes. The list holds only the dashboards that have this tag. The comparison is exact, byte for byte. An invalid entry never matches. |
| `limit`, `after` | The pagination of 6.24. |

```json
{
  "items": [
    { "uid": "nodes", "folder": "transit", "title": "Nodes", "tags": ["chain"],
      "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f", "version": 12,
      "updated_at": "2026-11-09T10:15:03.120Z", "updated_by": "-", "state": "valid", "access": { "write": true, "history": true, "export": true },
      "read_only": false, "store": "main" },
    { "uid": "broken", "folder": "transit", "title": "broken", "tags": [],
      "etag": "7d1a9c3e5b2f4d6a8c0e1f3b5d7a9c2e4f6b8d0a1c3e5f7b9d2a4c6e8f0b1d3a", "version": 3,
      "updated_at": "2026-11-08T17:02:44.000Z", "updated_by": "external", "state": "invalid",
      "errors": [ { "pointer": "/sections/0/panels/2/type", "code": "bad_format",
                    "message": "type must be a panel type of 4.13; value heatmap", "line": null, "column": null } ],
      "access": { "write": false, "history": true, "export": true }, "read_only": false, "store": "postgres" }
  ],
  "next": null
}
```

- `read_only` is `true` when the store of the dashboard is read-only (5.42). `store` is the id of this store. For a read-only dashboard, `access.write` is `false` with no question to the PDP.
- `stale` is present when the dashboard shows its last valid state, because its file in a volume is not valid now (5.44). Its value is `file_invalid`. It never holds a path or a message of the parser. Each subject that can read the dashboard gets it. Only a row of the PostgreSQL store gives the state `invalid`.
- `state` is `valid`, or `invalid` when the stored bytes break section 4 (5.8, item 5). An `invalid` item has the uid
  as its `title` and an empty `tags` list. Its member `errors` has 20 items at most, in the form of 4.37. Hune does
  not serve this document: `GET` gives `422 stored_document_invalid`, and `GET` with `download=true` gives its stored
  bytes (6.14). A save with `If-Match` equal to its `etag` can replace it (4.39).
- `version` is the number of the head version (5.2). It is 0 when the uid has no version in the history, for example a
  file that came with `git pull` on a volume.
- `updated_by` is the subject of the last version, `-` in mode `open`, or `external` for a change from outside (5.2).
- `access` is on each dashboard, with three members: `write`, `history` and `export`.
  `access.write` is the `access.write` of its folder (11.25). In mode `open`, it is `true` for each dashboard of the
  primary store.
- `access.history` is `true` when `server.history` is `true` and the dashboard is in the primary store. In mode
  `authzen`, the display answer for `dashboard.history` MUST be `true` too (11.25). It is `false` for a dashboard of a
  read-only store, with no question to the PDP.
- `access.export` is `true` when `server.export` is `true`. In mode `authzen`, the display answer for
  `dashboard.export` MUST be `true` too. The page shows the History entry only when `access.history` is `true`. It shows
  the Export entry only when `access.export` is `true` (9.8, 9.11).
- `tag` is for the dashboard links of the kind `dashboards` (4.5a, 9.8a). It works with `folder` and `q`: a dashboard MUST pass all the parameters of the request. The route applies `tag` before the pagination of 6.24 and before the display filter of 11.25. The parameter occurs one time. A dashboard link with more than one tag sends the first tag. The page tests the other tags in the member `tags` of each item.

### 6.13 Create a dashboard

`POST /api/dashboards` creates a dashboard. The headers `Content-Type` and `Idempotency-Key` are necessary.

```json
{ "folder": "transit", "message": "First version", "document": { "format_version": 3, "uid": "q7k2m9x4b1c8dz" } }
```

The example cuts the document. The body members are:

| Member | Rule |
|---|---|
| `folder` | Necessary. A folder of the primary store that exists, or `null` for a dashboard without a folder. A body with no member `folder` gives `400 bad_request`: there is no default. A folder of a read-only store gives `403 read_only_store` (11.35). |
| `message` | Optional. 0 to 500 Unicode characters (`MAX_MESSAGE_CHARS`, 5.7), no control character. |
| `document` | Necessary. A document of section 4, checked by 4.36 from its own bytes (6.2). Its member `uid` is the uid of the new dashboard. |

A uid that a store holds, also as the uid of a deleted dashboard, gives `409 already_exists` (5.4). A uid of a read-only store gives it too (5.42, rule 7). The subcommand
`restore` brings a deleted dashboard back (5.40). The answer is `201`, with `ETag` and
`Location: api/dashboards/<uid>`:

```json
{ "uid": "q7k2m9x4b1c8dz", "folder": "transit", "etag": "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00",
  "version": 1, "saved_at": "2026-11-09T10:15:03.120Z", "saved_by": "-", "noop": false, "warnings": [] }
```

The member `warnings` lists the warnings of 7.18 as `{pointer, message}`. A warning never stops a write. The
list holds at most `MAX_WARNINGS` items (4.35). When the checks find more warnings, the last item is
`{"pointer": "", "message": "the list stops here; it has 200 warnings at most"}`. The answer of 6.15 and of
`validate_only` has the same cap. 7.18 gives the rules of the two lists.

Errors: `400 bad_request`, `401 unauthenticated`, `403 forbidden`, `403 read_only_store`, `403 cross_site_request`,
`404 not_found` (folder), `409 already_exists`, `413 payload_too_large`, `415 unsupported_media_type`,
`422 invalid_document`, `422 idempotency_key_reused`, `428 precondition_required`, `503 pdp_unavailable`,
`503 storage_unavailable`, `507 storage_full`.

### 6.14 Read a dashboard

`GET /api/dashboards/{uid}` gives the current document and its metadata, with the header `ETag`:

```json
{ "uid": "nodes", "folder": "transit", "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f",
  "version": 12, "updated_at": "2026-11-09T10:15:03.120Z", "updated_by": "user:bob", "access": { "write": true, "history": true, "export": true },
  "read_only": false, "store": "main",
  "document": { "format_version": 3, "uid": "nodes" },
  "names": { "panels": { "3": [["env", "host"], []] }, "variables": { "host": ["env"] }, "annotations": [["env"]] } }
```

- `access.write` is the display answer for `dashboard.write` on `N(uid)` (11.25). In mode `open`, it is `true` when the
  dashboard is in the primary store. It is `false` for a dashboard of a read-only store, with no question to the PDP.
  `access.history` and `access.export` are the members of 6.12, with the same rules.
- `download=true` needs the export right. With `server.export` equal to `false`, the route gives `403 export_disabled`.
  It does this before it reads anything and before any PDP question. Thus the answer is the same for a uid that exists
  and for one that does not.
- In mode `authzen`, a download asks `dashboard.read` first. Then it asks `dashboard.export` on `N(uid)`. A `false`
  answer gives `403 forbidden` with the member `action` equal to `dashboard.export`.
- The export rule holds for an `invalid` entry too. A request with no `download` is not concerned.
- `read_only` and `store` are the members of 6.12. The page opens the editor read-only when `read_only` is `true`.
- `stale` is the member of 6.12. It is present only when the dashboard shows its last valid state (5.44).
- The member `document` is the document as a JSON value. Its member order is the canonical order, but the bytes of
  this answer are not the canonical bytes.
- The member `names` gives the variable names that each saved query names. The page uses it to find the dependencies of
  the variables and the panels (9.10). The page does not scan a query text. The rules of `names` follow.
- The member `names` is an object with three members: `panels`, `variables` and `annotations`. The three members are
  always present, and the object has no other member.
- `names.panels` is an object. Each key is the id of a panel that has `data.query`, as a decimal text. A text panel
  and a panel with `data.reuse` have no key.
- Each value of `names.panels` is an array with one array of names for each query of the panel. The order is the order
  of 4.15a: `data.query` first, then each item of `data.extra_queries`. The array has 1 to 8 items. A query with
  `hide` equal to `true` has its array too.
- `names.variables` is an object. Each key is the name of a variable of the kind `query`. Each value is the array of
  names of its SQL. A variable of the kind `custom` or `text` has no key.
- `names.annotations` is an array with one array of names for each annotation, in the order of the document. An
  annotation with `enabled` equal to `false` has its array too.
- An array of names has 0 to 64 distinct texts, in the order of their first use. Each text is the name of a variable of
  the document. The array has no macro name and no name that no variable has. The array of a `query` variable does not
  have the name of this variable.
- A query can have `$__adhocFilters` (7.10a) on the source of an `adhoc` variable. Then its array has the name of this
  variable, at the place of the macro. The page then runs the query again after each change of the filters (9.10a). An
  `adhoc` variable has no key in `names.variables`.
- The server makes `names` with the dependency scan of 7.19. It reads the SQL text of the query (4.16) and keeps the
  names of the variables of the document. It applies no format check and no dialect, so a name that the run does not
  need can be in an array. An array never misses a name that a run of the query needs.
- The member `names` is a pure function of the document. The server runs no query, opens no source and keeps no
  cache for it. `names` is not in the document, not in the canonical bytes and not in the etag. The cap of 4.35 on
  the queries of a document (400) and on the variables (64) limits its size.
- The route, the rights and the errors of this section do not change. A page must treat an absent `names` and a `names` that breaks these rules as an
  error of the answer (9.10).
- The member `adhoc` is present only when the document has an `adhoc` variable (4.10c). It is an object with one member
  for each such variable. The value is the array of its permitted keys, in the order of the document. A permitted key is
  an item of `keys` that the list `clickhouse "<id>".adhoc_columns` of its source holds now. A source with no list gives
  an empty array.
- The page offers the permitted keys only (9.10a). The member `adhoc` comes after `names`. It is not in the etag,
  because the configuration can change.
- A request with `If-None-Match` equal to the current etag gets `304` with no body.
- The query parameter `download=true` gives the canonical bytes of the document, exactly (4.38). The answer has no `names`. The answer has
  `Content-Type: application/json`, `Content-Disposition: attachment; filename="<uid>.json"` and `ETag`. The export of
  10.12 uses it.
- For an `invalid` entry (6.12), `download=true` gives the stored bytes, with the `ETag` of these bytes (4.39). The
  plain `GET` gives `422 stored_document_invalid`. Thus a person can read the bytes and replace them (9.7, 10.12).

Errors: `400 bad_request`, `401 unauthenticated`, `403 forbidden`, `404 not_found`, `422 stored_document_invalid`,
`503 pdp_unavailable`.

### 6.15 Save a new version

`PUT /api/dashboards/{uid}` saves a new version of the document. The headers `Content-Type`, `If-Match` and
`Idempotency-Key` are necessary. The body is `{"message": "Fix the Score query", "document": {...}}`, with the rules of
6.13 for `message` and `document`. The member `uid` of the document MUST equal `{uid}`: else the save gives
`422 invalid_document` with the pointer `/uid`. A save does not change the folder (6.19).

Hune does these steps in this order:

1. It does the checks of 6.28.
2. It asks the access questions (11). In mode `authzen`, a pre-pass reads the sources from the bytes of the document
   first (11.21). In mode `open`, this step asks nothing.
3. It checks the document with 4.36 and 7.18, in `spawn_blocking` (6.27). It lists every error, not only the first,
   up to `MAX_ERRORS` (4.35). In mode `authzen`, each source of the valid document must be in the questions of step
   2 (11.21). Else the answer is `500`, and Hune writes nothing.
4. Under the per-uid lock, or in one transaction (5.5), it finds the `save_id` when the history holds it (6.6).
5. It compares `If-Match` with the current etag, and the folder of the access decision with the current folder (5.5,
   rule 2). A difference gives `409 conflict`. After a folder difference, the server reads the uid from the store and
   updates its catalog entry, as 7.15 does. Thus the next save can pass.
6. When the new canonical bytes equal the current bytes, it writes nothing. It answers `200` with `noop: true` and
   the current etag and version.
7. Else it writes the version and the document (5.14, 5.27), and answers `200`.

The answer has the shape of the answer of 6.13, with `ETag`, and the status `200`.

The query parameter `validate_only=true` on `PUT /api/dashboards/{uid}` and on `POST /api/dashboards` runs steps 1 to 3
only. It asks only the use question of 6.9, not the `source.query` questions. It needs no `If-Match` and no
`Idempotency-Key`, and it writes nothing. A valid document gives `200`:

```json
{ "valid": true, "etag": "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00",
  "canonical": "{\n  \"format_version\": 3,\n  \"uid\": \"nodes\"\n}\n", "warnings": [] }
```

The member `canonical` is the canonical text of the document (4.38), and `etag` is its SHA-256 (4.39). The cut text of
the example is not a valid document. The JSON tab (10.4) and the conflict screen (10.10) use this answer. A document
that is not valid gives `422 invalid_document`.

Errors: the errors of 6.13, and `404 not_found`, `409 conflict`.

### 6.16 Delete a dashboard

`DELETE /api/dashboards/{uid}` deletes a dashboard. The headers `If-Match` and `Idempotency-Key` are necessary. The
request has no body. Hune writes a deletion version (5.4) and removes the dashboard from the catalog. The answer is
`200`:

```json
{ "uid": "nodes", "version": 13, "deleted": true, "saved_at": "2026-11-09T11:00:00.000Z", "saved_by": "-" }
```

After a delete, the routes of the uid answer as for an unknown uid (6.9), also a repeated delete (6.6). The store keeps
the history. The subcommand `restore` brings the dashboard back (5.40, 10.13).

Errors: `400 bad_request`, `401 unauthenticated`, `403 forbidden`, `403 read_only_store`, `403 cross_site_request`,
`404 not_found`, `409 conflict`, `422 idempotency_key_reused`, `428 precondition_required`, `503 pdp_unavailable`,
`503 storage_unavailable`, `507 storage_full`.

### 6.17 History

`GET /api/dashboards/{uid}/history` lists the versions of a dashboard, the newest first, with the pagination of 6.24.

```json
{ "items": [ { "version": 12, "kind": "save", "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f",
               "saved_at": "2026-11-09T10:15:03.120Z", "saved_by": "user:bob", "message": "Fix the Score query",
               "folder": "transit", "size": 48213 } ],
  "next": "djEy" }
```

The example shows a page with one item (`limit=1`). Its `next` is `djEy`, the cursor of the version 12 (6.24).

Order of the checks of both history routes and of the restore (6.18):

1. `server.history` equal to `false` gives `403 history_disabled`. Hune asks no PDP question and reads nothing, so the
   answer is the same for a uid that exists and for one that does not.
2. In mode `authzen`, Hune asks `dashboard.read` on `N(uid)` (11.23). A `false` answer, and an unknown uid, give the same
   `403 forbidden`.
3. A dashboard of a read-only store gives `404 history_unavailable`: only the primary store has history (5.18).
4. In mode `authzen`, Hune asks `dashboard.history` on `N(uid)`. A `false` answer gives `403 forbidden` with the member
   `action` equal to `dashboard.history`.
5. `download=true` on a version needs the history right only. The export right is not for a version, because the
   history page reads the stored text of a version with it. The page shows the button
   "Export this version" only when `access.history` and `access.export` are both `true`.

- `kind` is one of the eight kinds of 5.2: `create`, `save`, `restore`, `move`, `delete`, `import`, `external` and
  `migrate`.
- A uid with a current document and no version in the history gives an empty list (5.4).
- `limit` is 1 to 100 (`MAX_HISTORY_PAGE`, 5.7), and 100 when it is absent (6.24).
- `size` is the size of the canonical bytes. `etag` and `size` are `null` for a deletion version.
- An `external` version without bytes (5.2) has its `etag`: the SHA-256 of the raw bytes of the file that was not
  valid. Its `size` is `null`.
- `message` is the text of the write, or the empty text.

`GET /api/dashboards/{uid}/history/{n}` gives one version. `{n}` is a whole number from 1.

```json
{ "version": 7, "kind": "save", "etag": "5e8d2c1b0a9f8e7d6c5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e9d8c7b6a5f4e3d",
  "saved_at": "2026-11-02T09:00:00.000Z", "saved_by": "-", "message": "", "folder": "transit",
  "document": { "format_version": 3, "uid": "nodes" } }
```

- A deletion version has `document: null`.
- An `external` version without bytes also has `document: null`, with its `etag` and `size: null`. The answer is `200`.
- `download=true` gives the stored bytes of the version, with `Content-Disposition: attachment;
  filename="<uid>-v<n>.json"`. For a deletion version and for an `external` version without bytes, it gives
  `404 not_found`: the store holds no bytes for them (5.4).
- A version that the current rules of section 4 cannot read gives `422 stored_document_invalid`. A version of
  format 2 is an example (4.42). `download=true` still gives its stored bytes, so that the history shows it
  as text (4.42, item 10).

Errors of both routes: `400 bad_request`, `401 unauthenticated`, `403 forbidden`, `403 history_disabled`, `404 not_found`, `404 history_unavailable`,
`422 stored_document_invalid`, `503 pdp_unavailable`, `503 storage_unavailable`. These routes read the store, not the
catalog (5.5, rule 10).

### 6.18 Restore a version

`POST /api/dashboards/{uid}/restore` makes a new version with the document of an earlier version. The headers
`If-Match` (the current etag) and `Idempotency-Key` are necessary. The body is
`{"version": 7, "message": "Back to the version of Monday"}`, with the rule of 6.13 for `message`.

- The checks of 1 to 3 of 6.17 come first. The restore needs the history right and the write right: Hune asks the use question
  `dashboard.write` with `dashboard.history` as another permission (11.21). A `false` answer gives `403`, and Hune
  reads nothing. Then Hune reads the version and validates it in `spawn_blocking` (6.27). Then it asks the others
  and the use, because the others name the sources of the version (11.21).
- This is the one validation of the version. The store gets the valid document (5.4, item 8). Under its lock, the
  store reads only the metadata of the version and the current document. Then it compares `If-Match` and the folder
  (5.5, rule 2): a difference gives `409 conflict`.
- A version that does not exist gives `404 not_found` after the use question (11.23, rule 4).
- The new version has the kind `restore` (5.2). The folder does not change. History is never rewritten.
- A deletion version, or a version whose bytes fail 4.36, gives `422 not_restorable`.
- A version that passes 4.36 and fails 7.18 gives `422 invalid_document`.
- When the restored bytes equal the current bytes, Hune writes nothing and answers `noop: true`.

The answer has the shape of the answer of 6.15. Errors: the errors of 6.15, and `422 not_restorable`.

### 6.19 Move a dashboard

`POST /api/dashboards/{uid}/move` moves a dashboard to another folder, or out of its folder. The headers `If-Match` and `Idempotency-Key`
are necessary. The body is `{"folder": "ops", "message": ""}`, and `message` is optional. The member `folder` is necessary. Its value `null` moves the dashboard to the root of the primary store: it has then no folder.

- The store writes a version of the kind `move` (5.2). The document bytes and the etag do not change.
- A move to the current folder writes nothing and answers `noop: true`. This holds for a move to `null` of a dashboard that has no folder.
- After a move, `N(uid)` names the new folder, so the grants of the new folder apply (11).

The answer is `200` with `{"uid", "folder", "etag", "version", "saved_at", "saved_by", "noop"}`. Errors: the errors of
6.16, and `404 not_found` for an unknown target folder. A target folder that already holds a file with this uid gives
`409 already_exists` (5.15, `move_to`).

### 6.20 Panel query

`POST /api/dashboards/{uid}/panels/{id}/query` runs the stored query of one panel. `{id}` is the panel id. The viewer
sends no SQL: the server reads the SQL from the catalog (7.2).

```json
{
  "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f",
  "from_ms": 1794211200000,
  "to_ms": 1794232800000,
  "max_data_points": 1200,
  "variables": {
    "node": { "all": false, "values": ["n7"] },
    "env": { "all": true, "values": [] },
    "min_weight": { "all": false, "values": ["1000"] }
  }
}
```

| Member | Rule |
|---|---|
| `etag` | Necessary. The etag of the document that the page shows (7.15). |
| `from_ms`, `to_ms` | Necessary. The absolute time range (7.3). |
| `max_data_points` | Necessary. A whole number from 1 to 10,000 (7.4). |
| `variables` | Necessary, possibly empty. The values of 7.7. The member of an `adhoc` variable holds its filters (7.7a). |
| `ref` | Optional. The ref of one query of the panel (4.15a). Absent: the first query, `data.query`. The grammar is the grammar of a ref (4.3). |

A panel whose data is `reuse` (section 4) has no query: the request gives `400 bad_request`. A panel of the type `text` has no query either (4.33a). The request gives `400 bad_request`. The message is "This panel has no query." The answer is `200` with the frame of 7.16 for a panel with a query.

The route runs one query for each request. A panel with several queries needs one request for each visible query (9.8d). The page sends `ref` for each of these requests. It sends no `ref` for a panel that has no `extra_queries`, so the request is the request of today.

- `ref` names `data.query` when it equals the `ref` of `data.query`, or `A` when that member is absent. It names an item of `extra_queries` when it equals the `ref` of that item.
- A `ref` that names no query gives `404 not_found` with the message "This panel has no such query.".
- A `ref` that names a hidden query gives `400 bad_request` with the message "This query is hidden.". The request with no `ref` for a panel whose first query is hidden gives the same answer. The server never runs a hidden query. This check comes with the other checks of step 4 of 7.2, so after the question `dashboard.read`.
- A `ref` that breaks the grammar of 4.3, and a `ref` that is not a text, give `400 bad_request` with the pointer `/ref`.
- The query that the `ref` names gives its own `source`, its `format` and its SQL to the steps of 7.2. The members `min_interval`, `time_from`, `time_shift` and `hide_time_override` of `data.query` apply to it (4.15a, rule 4). The body holds `max_data_points` as it did.
- The answer has the member `ref` (text) when the request has one. It has the value of the request. A request with no `ref` gets an answer with no `ref`, so the answer of today does not change. The member `ref` comes before the member `sql`.
- The permission is the same for each query: `dashboard.read` on N(uid) (11.21). The ceilings `max_rows` and `max_duration` apply to each query alone.
- The errors of the request and of the query do not change. A query of a panel that fails does not stop the other requests of the panel. The page shows the other frames (9.8d).

A panel of the type `state_timeline` or `status_history` has the ordinary panel query. The request, the answer and the limits do not change. The frame needs a column of the kind `time` and one value column or more (9.19c). The page draws what the frame holds and does not send a second query.

A panel of the type `heatmap` or `histogram` has the ordinary panel query. The request, the answer and the limits do not change. The server never buckets. A page that needs more rows than `max_result_rows` can bucket in the SQL, as 9.19e and 9.19f say. The page draws what the frame holds and does not send a second query.

The answer has one more optional last member, `sql` (text). The rules of this member:

- `sql` is present only when `server.query_inspect` is `true` and the subject can see the SQL.
- The value is the SQL text that Hune sent to the source after the expansion of 7.2. It holds nothing else: no setting, no header and no connection value.
- An error answer has no `sql`. The route of 6.21 and the preview of a variable have no `sql`.
- In the mode `authzen`, the subject needs `source.query` on S(id) and `dashboard.write` on N(uid) (11.21). S(id) is the source of the query that the request runs (4.15a).
- A subject with only `dashboard.read` gets the frame without `sql`, with the status `200`.
- For a source of the kind `mongodb`, the value is the compact Extended JSON text of the expanded tree (7.35, step 7). It holds no member that Hune adds to the command.
- Hune never writes `sql` to a log, a span or a metric.

Errors of the request: `400 bad_request`, `401 unauthenticated`, `403 forbidden`, `403 cross_site_request`,
`404 not_found`, `409 dashboard_changed`, `413 payload_too_large`, `415 unsupported_media_type`,
`422 invalid_time_range`, `422 variable_value_refused`, `422 stored_document_invalid`, `503 pdp_unavailable`,
`503 storage_unavailable` (the store read of 7.15).

Errors of the query: `403 ceiling_exceeded`, `422 sql_invalid`, `422 sql_too_large`, `422 query_limit`,
`422 query_memory_limit`, `422 query_error`, `502 source_access_denied`, `502 source_misconfigured`,
`502 source_bad_response`, `503 source_unavailable`, `503 source_busy`, `504 query_timeout`.

### 6.21 Variable options

`POST /api/dashboards/{uid}/variables/{name}/options` runs the options query of one variable of the kind `query`.

```json
{ "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f",
  "from_ms": 1794211200000, "to_ms": 1794232800000,
  "variables": { "env": { "all": false, "values": ["prod"] } } }
```

The members follow 6.20, without `max_data_points`. A variable of another kind gives `400 bad_request`. The answer is
`200`:

```json
{ "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f", "query_id": "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d",
  "options": [ { "value": "a1", "text": "Node A" }, { "value": "b2", "text": "b2" } ] }
```

7.14 gives the rules of the options. Errors: the errors of 6.20.

The member `allow_custom_value` of the variable (4.8) changes nothing in this route. The options, the caps and the errors are the same for an open and for a closed variable. The member of a parent variable can change the check of the values of the request (7.7).

### 6.21a Annotation query

`POST /api/dashboards/{uid}/annotations/{index}/query` runs the stored query of one annotation (4.5b). `{index}` is the position of the annotation in the array `annotations`, counted from 0. The viewer sends no SQL: the server reads it from the catalog (7.37).

```json
{
  "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f",
  "from_ms": 1794211200000,
  "to_ms": 1794232800000,
  "variables": { "env": { "all": false, "values": ["prod"] } }
}
```

The members follow 6.21: `etag`, `from_ms`, `to_ms` and `variables` are necessary. The body has no `max_data_points`, and a body that has it gives `400 bad_request`. The range is the range of the dashboard (7.3). The answer is `200`:

```json
{
  "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f",
  "query_id": "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d",
  "rows": 2,
  "annotations": [
    [1794211800000, null, "", "Deploy v1.2", ["prod", "deploy"]],
    [1794214800000, 1794216600000, "Maintenance", "Disk change", []]
  ]
}
```

| Member | Rule |
|---|---|
| `rows` | The number of marks. It equals the length of `annotations`. |
| `annotations` | One array for each row of the result, in the order of the time, then of the row. Each array has 5 items: the time and the end time in milliseconds (the end is `null` for a point), the title, the text and an array of tags. |

- An index that is not a whole number from 0 to 7 gives `404 not_found` with the message "No such annotation.". An index that no annotation has gives the same answer.
- An annotation with `enabled` equal to `false` gives `400 bad_request` with the message "This annotation is disabled.". The server runs no query for it.
- The source of an annotation can have the kind `clickhouse`, `postgres` or `mongodb`. The route, the frame shape, the caps and the access question are the same for the three kinds (7.37). Until the specification adds it, a kind that a later version adds to 8.2 gives `400 bad_request`. The message is "An annotation cannot run on a source of this kind.". The server runs no query for it.
- A source that the configuration does not declare gives `404 not_found`.
- The answer has no member `sql`. The route has no inspect view.
- Hune never writes the title, the text or a tag to a log, a span or a metric (14.7).
- Errors of the request and of the query: the errors of 6.21. A result with more rows than `MAX_ANNOTATION_ROWS` gives `422 query_limit` (7.37). A result that breaks a rule of 7.37 gives `422 annotation_invalid`.

### 6.22 Sources and preview

`GET /api/sources` lists the sources for the editor, in byte order of the id:

```json
{ "items": [ { "id": "prod", "label": "Production", "kind": "clickhouse", "state": "up" } ] }
```

`kind` is `clickhouse`, `postgres` or `mongodb`. `state` is `unknown`, `up`, `down` or `misconfigured` (8.11, 8a.12, 8b.12). In mode `authzen`, the list holds only the sources that
the display filter keeps for `source.query` (11.25). In mode `open`, it holds every source.

`POST /api/sources/{id}/preview` runs a draft panel or the options query of a draft variable. The editor uses it for
SQL that is not saved (10.3).

```json
{
  "uid": "nodes",
  "from_ms": 1794211200000,
  "to_ms": 1794232800000,
  "max_data_points": 1200,
  "dashboard_variables": [ { "name": "env", "kind": "custom" } ],
  "variables": { "env": { "all": false, "values": ["prod"] } },
  "panel": { "id": 4, "type": "table" }
}
```

The example cuts the variable and the panel: a real body holds each member of 4.7 to 4.13.

| Member | Rule |
|---|---|
| `uid` | The uid of a saved dashboard under edit. An unknown uid follows the rule of 6.9. |
| `folder` | The folder of a new dashboard that is not saved yet. An unknown folder follows the rule of 11.23. |
| `from_ms`, `to_ms`, `variables` | As in 6.20. |
| `dashboard_variables` | Necessary. The variables of the draft document (4.7 to 4.11) that the draft panel or variable names, with their parents (7.14). The page sends no other variable, so the body stays within its limit (6.27). The preview ignores the member `placement` (4.7a) of these variables. The items can have any kind of 4.7, `interval` and `constant` included. A `constant` is read from this member and not from the request. The objects keep the member `allow_custom_value` of 4.8 and 4.9, and the preview reads it as the save does (7.7). |
| `panel` | One draft panel of 4.13. Its query names the source `{id}`. |
| `variable` | The name of a variable of `dashboard_variables`, of the kind `query`, whose source is `{id}`. |
| `max_data_points` | Necessary with `panel`. Absent with `variable`. |

- The body holds exactly one of `uid` and `folder`, as 11.21 says. Two or none give `400 bad_request`.
- The body holds exactly one of `panel` and `variable`. Two or none give `400 bad_request`. Hune checks the draft
  with section 4 and 7.18, after the access questions, in `spawn_blocking` (6.27, 11.20). A fault gives `422 invalid_document`, with pointers relative to the body (6.2), for example
  `/panel/data/query/sql/3`. The panel editor removes `/panel` to show them (10.3).
- The access questions name `N(uid)` for a saved dashboard, and `folder:<folder>` for a new one (6.9, 11.21).
- The preview reads the catalog only to find the folder of `uid`. It writes nothing. It uses the engine, the limits and
  the admission of 6.20 (7.2, 8.8).
- A draft of a dashboard or of a folder of a read-only store gives `403 read_only_store`.
- A draft panel of the type `text` has no query. The request gives `400 bad_request`. The message is "This panel has no query." The editor never sends this request (10.3).
- A draft panel with the member `data.extra_queries` gives `400 bad_request` with the message "A preview runs one query.". The editor sends the selected query as `data.query` of the draft panel, with the members of the panel (10.3). The route does not change, and the body keeps its limit of 512 KiB (6.27).

With `panel`, the answer is the frame of 7.16 without `etag`. It can have the member `sql`, with the rules of 6.20. For a source of the kind `postgres`, the frame follows 7.16a, and the preview applies 7.16b to the format of the draft panel. For a source of the kind `mongodb`, the frame follows 7.16c and 7.16d. With `variable`, it is the answer of 6.21 without `etag`.
Errors: the errors of 6.20 without `409 dashboard_changed`, and `403 read_only_store`, `422 invalid_document`.

### 6.23 ProcurAuth routes

`GET /.well-known/procurauth/actions` gives the action document, and `GET /.well-known/procurauth/resources` is the
options route. Section 11 gives their content, their questions and their errors. In mode `open`, both answer
`404 not_found`.

### 6.24 Pagination and list rules

- `GET /api/dashboards` and `GET /api/dashboards/{uid}/history` have pages. The other lists give all their items in
  one answer.
- `limit` is a whole number: 1 to 500 for the dashboards, and 1 to 100 for the history (`MAX_HISTORY_PAGE`, 5.7).
  When it is absent, the value is 100. A larger value, or a value that is not a whole number, gives `400 bad_request`.
- `after` is the value of `next` of the previous answer: 1 to 512 characters of `[A-Za-z0-9_-]`. A value that Hune did
  not make gives `400 bad_request`.
- The answer has `items` and `next`. `next` is `null` on the last page.
- A page can hold fewer items than `limit`, also zero. This occurs when the display filter removes items, or when the
  request reaches the bound of PDP questions (11). Only `next: null` marks the end.
- The cursor of the dashboards holds the sort key of the last item that the subject can see. It holds also the count
  of the rows that Hune examined and hid after this item. It never holds the title or the uid of a row that the
  subject cannot read. A page with no visible item keeps the key of its request and adds its rows to the count.
- An item that does not change between two pages occurs exactly one time. An item that changes between two pages
  can occur on a later page or not at all. A row that becomes visible between two pages can be skipped.
- The history cursor is the text `v<version>`, with the number of the last version of the page in decimal, in base64url
  with no padding. Example: `v12` gives `djEy`. The next page holds the versions below this number.
- Order of the dashboards: first the folder id, then the title, then the uid. A dashboard without a folder (folder `null`) comes before each dashboard of a folder. Each comparison is in byte order, and
  the title comparison folds ASCII letters to lower case. Order of the history: version, the newest first.

### 6.25 Page routes and static files

- `GET /`, `GET /d/{uid}` and `GET /d/{uid}/{slug}` answer the shell (9.6) with `200`,
  `Content-Type: text/html; charset=utf-8` and `Cache-Control: no-cache`.
- The page route does not read the store: the page asks the API (9.8). Hune ignores `{slug}`: any text of 1 to 256
  bytes without `/` is accepted.
- Some paths are not under `api/`, `static/` or `.well-known/`, and are not a page route. A `GET` of such a path
  answers the shell with the status `404`. A `{uid}` that breaks the uid rule of 4.3 also gets it. The page then shows "This
  address does not exist." (9.5).
- `GET /static/{build-hash}/{path}` answers an embedded file of 9.2. `{build-hash}` MUST equal the build hash of the
  binary, and `{path}` MUST name an embedded file other than `shell.html`. Else the answer is `404 not_found`.
- A static file has the `Content-Type` of 9.3, `Cache-Control: private, max-age=31536000, immutable` and
  `X-Content-Type-Options: nosniff`. With the value `private`, a shared cache cannot give a file to a client without a token.
- A static file of a text type has `Vary: Accept-Encoding` while `server.compress_static` is `true`. A request that accepts gzip gets the gzip copy with `Content-Encoding: gzip` (9.3). A static answer has no `ETag`, and the server ignores `Range`.
- In mode `authzen`, the pages and the static files need a valid identity token and ask the PDP nothing (11).
- No other page route exists. The editor and the history are states of the dashboard page (9.5).

### 6.26 Ops listener routes

| Route | Answer |
|---|---|
| `GET /livez` | `200` with `{"status": "live"}` while the process serves. |
| `GET /readyz` | `200` or `503`, with the JSON body and the rule of 14. |
| `GET /metrics` | `200` with the Prometheus text format 0.0.4 and the metrics of 14. |

The ops listener takes `GET` and `HEAD`: another method gives `405`, and another path gives `404`. It has no base path,
no identity token, no PDP question and no cross-site guard. The subcommand `health` calls `/livez` (14).

### 6.27 Request size, time and concurrency limits

`DOC_MAX` is the document size limit `MAX_DOCUMENT_BYTES` of 4.35. Section 12.16 has the same values.

| Limit | Value | Result past the limit |
|---|---|---|
| Request line and target | 8 KiB | `414 uri_too_long` |
| Request headers | 100 headers, and 16 KiB for all of them | `431 headers_too_large` |
| Time to read the headers | 10 s | Hune closes the connection. |
| Body of a panel query, an options query or an annotation query | 262,144 bytes (256 KiB) | `413 payload_too_large` |
| Body of a preview | 524,288 bytes (512 KiB) | `413 payload_too_large` |
| Body of a create, a save or the import of one document (10.12) | `server.max_request_bytes`, which is at least `DOC_MAX` + 65,536 | `413 payload_too_large` |
| Body of a restore, a move or a folder route | 16,384 bytes (16 KiB) | `413 payload_too_large` |
| Body of a `GET` or a `DELETE` | no body | `400 bad_request` |
| Time to read the body | 30 s | `408 request_timeout`, then Hune closes the connection. |
| JSON depth of a body | 64 | `400 bad_request` |
| Open connections | `server.max_connections` | Hune accepts one more connection, answers `503 server_busy` with `Retry-After: 1`, and closes it (12.16). |
| Bodies in flight of a dashboard create, save or import | `MAX_INFLIGHT_DOC_BODIES` = 8 | `503 server_busy` with `Retry-After: 1` |
| Other bodies in flight | `MAX_INFLIGHT_SMALL_BODIES` = 256 | `503 server_busy` with `Retry-After: 1` |
| Validations of a document that run or wait | `MAX_INFLIGHT_VALIDATIONS` = 4; wait `VALIDATION_WAIT` = 5 s | `503 server_busy` with `Retry-After: 1` |
| Document bodies in flight, and validations, of one subject in mode `authzen` | `MAX_INFLIGHT_PER_SUBJECT` = 2 for each | `503 server_busy` with `Retry-After: 1` |
| Idle time of a kept connection | `server.idle_timeout` | Hune closes the connection. |

- Hune compares `Content-Length` with the limit before it reads the body. For a chunked body, it counts the bytes and
  stops at the first byte past the limit.
- Section 13 stops the start when `server.max_request_bytes` is below `DOC_MAX` + 65,536.
- The limit of a panel query bounds the values of the "All" list of 7.7. A list that does not fit gives `413`, and the
  page shows the error of 9.10.
- A body in flight takes a place from the moment Hune reads its first byte until the work on the body ends. The two
  constants bound the memory of the bodies: at most 8 × `server.max_request_bytes` plus 256 × 512 KiB. They are not
  configuration keys. Example: the ninth save that arrives while 8 saves read their bodies gets `503 server_busy` at once.
- Hune validates a document only in `spawn_blocking`, never on an async worker (3.7). A create, a save, a
  `validate_only` request, a preview, a restore and the plain read of a version (6.17) take a validation permit first.
  No permit within `VALIDATION_WAIT` gives `503 server_busy`.
- A validation keeps its permit and the place of its body until it ends, also when the client closes the connection.
  The validation of a create or of a save also keeps its place while it waits for its permit. Example: 8 heavy saves
  whose clients left still hold the 8 places, and a ninth save gets `503 server_busy` (12.16).
- A restore and the plain read of a version take the permit before the store read, so a request that waits holds no
  version.
- In mode `authzen`, a subject holds at most `MAX_INFLIGHT_PER_SUBJECT` document bodies and as many validations. A
  third gets `503 server_busy` at once (12.16).
- Each call that Hune sends has its own time limit: the store (5), the PDP (11) and the sources (8.9).
- The admission of 8.8 bounds the queries for each source and for each subject. The page sends 6 query requests at
  most at the same time (9.8).

### 6.28 Order of the steps of a request

Hune does these steps for each request of the main listener, in this order. This order is the reference: 11.20
follows it.

1. It accepts the connection. When `server.max_connections` connections are already open, it answers
   `503 server_busy` and closes the connection.
2. At a stop, `/readyz` answers 503 first. During `SHUTDOWN_DRAIN` = 5 s after that, the listener still accepts and
   serves requests, so that the proxy can remove the instance. Then a request gets `503 server_stopping` (14.13).
3. Limits. It reads the request line and the headers within the limits of 6.27 (`414`, `431`). It matches
   `server.base_path` and the route (`404`, `405`) and sets the request id (6.3). It compares `Content-Length` with
   the body limit of the route (`413`).
4. For `POST`, `PUT`, `PATCH` and `DELETE`, it applies the cross-site guard of 12.7 (`403 cross_site_request`). Then
   it checks `Content-Type` (`415`).
5. In mode `authzen`, it checks the identity token (11.8). A failure gives `401 unauthenticated`.
6. For a write route (6.9), a write whose target is in a read-only store gives `403 read_only_store` at step 8 (11.35).
7. It takes a place for a body in flight (6.27, `503 server_busy`). In mode `authzen`, a document body first takes
   a place of its subject (12.16). It reads the body within the limit of the route (`413`, `408`), and checks its
   JSON syntax and its shape (`400`). The document of a create, a save or a `validate_only` request stays bytes.
8. The handler finds the objects (uid, folder, panel, version) and asks the PDP questions (11.21). For a create and
   a save, the pre-pass of the sources comes before this lookup (11.23). Then it validates the document, the draft or
   the restored version in `spawn_blocking` (6.27, `422`). Then it does the effect.

The PDP decision comes before the first write to the store and before the first byte to a source. It also comes
before the validation of a document or of a draft (11.20).

## 7. Query engine

### 7.1 Place and scope

The query engine is in the crate `hune-core`, which does no I/O (3). It makes the final SQL text from the stored SQL,
the variable values, the time range and `max_data_points`. The server is the only place that makes SQL. The browser of a
viewer never sends SQL. The query routes name a dashboard, a panel or a variable, and the server reads the stored SQL.

The engine makes sure that a value never leaves its literal (7.11). The SQL of a ClickHouse source can also use the query parameters of ClickHouse (`{name:Type}`, 7.6a). Their values go in the URL of the request and never in the SQL text. The ClickHouse account of 8.12 is the real security boundary. The source client of section 8 sends the final SQL and makes the frame of 7.16. The same engine checks the SQL
at save (7.18), and the importer uses it (15).

A source of the kind `postgres` has its own dialect (7.19 to 7.26) and its own account (8a.13). It uses no query parameter of PostgreSQL: the SQL that it sends has no parameter (8a.5).

A source of the kind `mongodb` has no SQL. It has its own query language, a JSON tree with a closed grammar (7.27 to 7.36), and its own account (8b.13). The engine puts each value in the tree as a typed value, and it never writes a value in the text of the query (7.34).

### 7.2 Pipeline for one panel query

After the steps 1 to 7 of 6.28, the server does these steps for `POST /api/dashboards/{uid}/panels/{id}/query`:

1. It finds the uid in the catalog (5.8). An unknown uid follows the rule of 6.9.
2. It asks `dashboard.read` on `N(uid)` (11), and keeps the ceilings `max_rows` and `max_duration` of the answer.
3. It compares `etag` with the etag of the catalog (7.15). A difference gives `409 dashboard_changed`.
4. It finds the panel `{id}`. An unknown id gives `404 not_found`. A panel with the data `reuse` gives `400 bad_request`. Then it finds the query that `ref` names, or `data.query` when `ref` is absent. A query that no `ref` names gives `404 not_found`, and a hidden query gives `400 bad_request` (6.20).
5. It finds the source of the query of step 4. A source id that the configuration does not declare gives `404 not_found`. The kind of the source gives the dialect of the next steps (7.19).
6. It checks the time range (7.3) and `max_data_points` (7.4), and computes the interval (7.4).
7. It checks the variable values (7.7).
8. It joins the SQL lines of the query with one LF (U+000A) between two lines. The text has no LF at the end.
9. It reads the text with the lexer (7.5, or 7.20 for a `postgres` source). It checks the query parameters of the stored text (7.6a, run time, step 1).
10. It expands the placeholders and the macros (7.6 to 7.11, or 7.21 to 7.24 for a `postgres` source).
11. It reads the result again with the lexer and checks the property P (7.11).
12. It checks the statement (7.12, or 7.25) and the size of the final SQL (7.13). Then it checks the query parameters of the final text and their values (7.6a, run time, steps 3 to 5).
13. It computes the effective limits (8.3). `max_result_rows` is the lower of the source key and `max_rows`.
    `max_execution_time` is the lower of the source key and `max_duration`.
14. It passes the admission of 8.8.
15. It sends the query to the source and makes the frame (8.3, 8.4, 7.16, or 8a.5 and 7.16a for a `postgres` source). The values of the query parameters go in the URL of the request (7.6a, 8.3).
    15a. For a `postgres` source and the format `time_series`, it applies the rule of 7.16b to the frame.
16. It answers the frame, and writes the log line `query.run` and the metrics (14).

A failure at a step gives the error of that step, and the next steps do not occur. Nothing reaches the source before
step 15.

For a source of the kind `mongodb`, the steps 8 to 12 are the steps of 7.35. The step 15 follows 8b.5, and the frame follows 7.16c. The step 15a does not exist for this kind (7.16d). The errors of 7.35 are `422 sql_invalid`, `422 variable_value_refused` and `422 sql_too_large`.

### 7.3 Time model

- The browser changes the relative times of the time picker (9.9) into absolute epoch milliseconds. It does this one
  time for each refresh, for all the panels of the page. The server receives only absolute values.
- `from_ms` and `to_ms` are JSON integers with `0 <= from_ms < to_ms <= 4294967295999`. Another value gives
  `422 invalid_time_range`. The upper bound is the last millisecond of the ClickHouse type `DateTime`.
- The server never reads its clock to compute a time range. Thus two instances give the same SQL for the same request.
- The dashboard member `timezone` (`browser` or `utc`, 4.6) changes only the display and the date math of the
  browser. The macros use absolute Unix seconds, so the time zone of the browser never changes a filter.
- The macros `$__dateFilter`, `$__dateTimeFilter` and `$__dt` print calendar dates. They use the UTC date of `from_ms` and of
  `to_ms`, whatever the member `timezone` says. The ClickHouse plugin of Grafana 12.4.2 does the same. It gets epoch
  milliseconds only, and it prints the date in UTC. A check on Grafana 12.4.2 gave `toDate('2026-11-09')` for a range
  that starts at 2026-11-09T23:00:00Z.

Example: the range "Last 6 hours", computed at 2026-11-09T14:00:00.000Z, gives `from_ms = 1794211200000` and
`to_ms = 1794232800000`.

### 7.4 Interval rule

The interval rule restates the rule of Grafana `calculateInterval` and `roundInterval`:

- `D` is `max_data_points` of the request. The browser sends the panel member `max_data_points` when the document
  gives it. Else it sends the inner width of the panel frame in CSS pixels, rounded, as Grafana does (9.8). `D` is a
  whole number from 1 to 10,000. Another value gives `400 bad_request`.
- `L` is the panel member `min_interval` in milliseconds (4.15). For a query that is an item of `extra_queries`, `L` comes from `min_interval` of `data.query` (4.15a, rule 4). When it is `null`, `L` is 1. Grafana also takes the
  "Min interval" of its data source (`jsonData.timeInterval`) as a floor. M0 reads this setting from the export of the Grafana instance (19).
- `R` is `(to_ms - from_ms) / D`, a real number that the engine does not round.
- `I = max(L, round_interval(R))`, in milliseconds.
- `S = max(1, floor(I / 1000))`, in seconds.
- The frame gives `I` as `interval_ms` (7.16). The browser uses it for the width of bars.

`round_interval(R)` is the value of the first row whose bound is above `R`:

| `R` below (ms) | `round_interval(R)` (ms) | | `R` below (ms) | `round_interval(R)` (ms) |
|---|---|---|---|---|
| 10 | 1 | | 210,000 | 120,000 (2 min) |
| 15 | 10 | | 450,000 | 300,000 (5 min) |
| 35 | 20 | | 750,000 | 600,000 (10 min) |
| 75 | 50 | | 1,050,000 | 900,000 (15 min) |
| 150 | 100 | | 1,500,000 | 1,200,000 (20 min) |
| 350 | 200 | | 2,700,000 | 1,800,000 (30 min) |
| 750 | 500 | | 5,400,000 | 3,600,000 (1 h) |
| 1,500 | 1,000 (1 s) | | 9,000,000 | 7,200,000 (2 h) |
| 3,500 | 2,000 | | 16,200,000 | 10,800,000 (3 h) |
| 7,500 | 5,000 | | 32,400,000 | 21,600,000 (6 h) |
| 12,500 | 10,000 | | 86,400,000 | 43,200,000 (12 h) |
| 17,500 | 15,000 | | 604,800,000 | 86,400,000 (1 d) |
| 25,000 | 20,000 | | 1,814,400,000 | 604,800,000 (1 w) |
| 45,000 | 30,000 | | 3,628,800,000 | 2,592,000,000 (30 d) |
| 90,000 | 60,000 (1 min) | | no bound | 31,536,000,000 (365 d) |

The left column comes first, from the top, then the right column.

Worked examples:

| Range | `D` | `L` | `R` (ms) | `I` (ms) | `S` (s) |
|---|---|---|---|---|---|
| 6 h (21,600,000 ms) | 1,200 | 1 | 18,000 | 20,000 | 20 |
| 7 d (604,800,000 ms) | 800 | 1 | 756,000 | 900,000 | 900 |
| 1 h (3,600,000 ms) | 1,200 | 60,000 (`1m`) | 3,000 | 60,000 | 60 |
| 10 min (600,000 ms) | 1,920 | 1 | 312.5 | 200 | 1 |
| 30 d (2,592,000,000 ms) | 1,000 | 1 | 2,592,000 | 1,800,000 | 1,800 |

The unit tests hold this table and these examples (16).

### 7.4a The text of "Auto" of an `interval` variable

The server computes the text of the option "Auto" from the request. The page never sends it. The rule restates the rule of Grafana, with the table of 7.4.

1. `R = (to_ms - from_ms) / auto_count`, a real number that the engine does not round. `to_ms` and `from_ms` come from the request (7.3).
2. `I = max(M, round_interval(R))` in milliseconds. `M` is the value of `auto_min` in milliseconds.
3. The text is the text of the row of `I` in the table below. `I` is always a value of the table, because `round_interval` gives table values and `auto_min` is a table text.

| `I` (ms) | Text | `I` (ms) | Text | `I` (ms) | Text |
|---|---|---|---|---|---|
| 1 | `1ms` | 5,000 | `5s` | 1,800,000 | `30m` |
| 10 | `10ms` | 10,000 | `10s` | 3,600,000 | `1h` |
| 20 | `20ms` | 15,000 | `15s` | 7,200,000 | `2h` |
| 50 | `50ms` | 20,000 | `20s` | 10,800,000 | `3h` |
| 100 | `100ms` | 30,000 | `30s` | 21,600,000 | `6h` |
| 200 | `200ms` | 60,000 | `1m` | 43,200,000 | `12h` |
| 500 | `500ms` | 120,000 | `2m` | 86,400,000 | `1d` |
| 1,000 | `1s` | 300,000 | `5m` | 604,800,000 | `7d` |
| 2,000 | `2s` | 600,000 | `10m` | 2,592,000,000 | `30d` |
| | | 900,000 | `15m` | 31,536,000,000 | `1y` |
| | | 1,200,000 | `20m` | | |

The panel width and `max_data_points` play no part, so all the panels and all the variables of one request get the same text. The macro `$__interval` is not changed (7.4, 7.10). Worked example: a range of 6 hours, `auto_count` 30 and `auto_min` `10s` give `R` = 720,000 ms, `round_interval` 600,000 ms, and the text `10m`. A range of 5 minutes gives `R` = 10,000 ms and the text `10s`.

The unit tests hold the 59 bound cases of the measure, and the examples of this section (16.3).

### 7.5 ClickHouse lexer

The engine reads SQL with one lexer in `hune-core`. Its rules and its test vectors come from
`src/sql_lexer.hpp` of ClickHouse Dash, at the tag `v1.3.1` (commit `e970694e5e0b17b07cf187f3128af93fa1a82b76`).
Hune changes three rules of ClickHouse Dash, so that the lexer agrees with ClickHouse 26.8.34.13: `//` starts a line
comment, a line comment ends at LF only, and a heredoc tag can start with a digit. A position is a byte offset. The
editor converts it to UTF-16 units (10.3).

| Token | Rule |
|---|---|
| Blank | A run of space, TAB, LF, CR, FF, VT, and the Unicode spaces U+0085, U+00A0, U+180E, U+2000 to U+200D, U+2028, U+2029, U+202F, U+205F, U+2060, U+3000 and U+FEFF. |
| Line comment | Starts with `--`, with `//`, or with `#` followed by a space or `!`. Ends before the next LF. A CR, FF, VT or Unicode line separator does not end it. |
| Block comment | From `/*` to `*/`. It nests: `/* a /* b */ c */` is one comment. |
| String | `'...'`. A backslash escapes the next byte. A doubled `'` is one quote mark of the text. |
| Quoted name | `"..."` or `` `...` ``, with the escape rules of a string and its own quote mark. |
| Heredoc | `$tag$ ... $tag$`, or `$$ ... $$` with an empty tag. No escape. The tag has ASCII letters, digits and `_`. It can start with a digit. |
| Number | Starts with a digit, or with `.` and a digit. Continues with letters, digits, `_` and `.`. A `+` or `-` after the exponent mark (`e`, `E`, or `p`, `P` after `0x`) is part of the number. |
| Word | Starts with an ASCII letter, `_`, a byte from 0x80 that does not start a Unicode space, or a `$` where no heredoc starts. Continues with these bytes, digits and `$`. A Unicode space ends it. |
| Punctuation | Any other single byte. |
| Unknown | A `#` that starts no comment. |

An unknown token is a lexer error. A block comment, a string, a quoted name or a heredoc that the text ends before it
closes is also a lexer error. At run time a lexer error gives `422 sql_invalid`. At save it gives
`422 invalid_document` (7.18).

The measure of 2026-11-22 on ClickHouse 26.8.34.13 sent each form to the HTTP interface, as the source client does:

- A line comment starts with `--` and any next byte, with `#` and a space, with `#!`, with `//` and with `///`. ClickHouse
  refuses `#` and a TAB, `#` and a LF, and `/ /`: they start no comment.
- A line comment ends at LF only. A CR, FF, VT, NUL, U+0085, U+2028 or U+2029 stays in the comment. CR LF ends it at the LF.
- A block comment nests. `/*/ ... */` is one comment, and a `/*` inside a line comment starts nothing.
- A heredoc tag can start with a digit, for example `$1$` or `$1a$`. A tag with a blank or another byte is no tag.
- ClickHouse reads an open `$1$x` as a name. Hune reads an open heredoc, which is a lexer error, so Hune refuses more.
- The strings, the quoted names and the blanks agree with the table.

Before this change, the lexer of Hune read `//` as code, ended a comment at CR and refused a tag with a first digit.
A text with two `//` comments that each hold a `'` then put a free value in code, where Hune saw a string. P, PQ and the
second scan of 7.6a trust the lexer, so the value became SQL (a security fix). The ClickHouse
suite now sends the probe table to the server and compares each answer with the lexer of Hune (16.9). A ClickHouse that
reads a form in another way fails the suite, and the rules of this section must be measured again.

Hune differs from ClickHouse Dash on these three rules. The vector file of ClickHouse Dash stays an unchanged copy.
The test of the vectors lists the two vectors that the end of a line comment changes, with the reason.

Test-vector rule:

- The lexer MUST pass each vector of `test_lexer_rules` and `test_literals`. These functions are in
  `tests/native/sql_test.cpp` of ClickHouse Dash at the recorded commit. The Hune tests hold a copy of these vectors,
  with the commit and the file name in a comment.
- When ClickHouse Dash changes its lexer, Hune copies the new vectors and records the new commit in the same change.
- The browser copy `app_sql_lex.js` (9.24) follows the same rules for `//` and for the end of a line comment.
  The page uses it only to find placeholders (9.10)
  and to colour SQL (10.3). It never makes SQL.

The table below gives a part of the vectors. Each token is one letter:

| Letter | Token | | Letter | Token |
|---|---|---|---|---|
| `_` | blank | | `S`, `s` | closed or open string |
| `L` | line comment | | `H`, `h` | closed or open heredoc |
| `B`, `b` | closed or open block comment | | `N` | number |
| `W` | word | | `P` | punctuation |
| `Q`, `q` | closed or open quoted name | | `U` | unknown |

| Text | Tokens | | Text | Tokens |
|---|---|---|---|---|
| `SELECT 1` | `W_N` | | `'a''b'` | `S` |
| `a.b` | `WPW` | | `'abc` | `s` |
| `1--2` | `NL` | | `` `a\`b` `` | `Q` |
| `1 # x` LF `2` | `N_L_N` | | `$$a'b$$` | `H` |
| `1 #! x` | `N_L` | | `$tag$a$$b$tag$` | `H` |
| `1 #x` | `N_UW` | | `a$b` | `W` |
| `1 /* a /* b */ c */ 2` | `N_B_N` | | `$1` | `W` |
| `1 /* a /* b */` | `N_b` | | `1-2` | `NPN` |
| `1 // x` LF `2` | `N_L_N` | | `$1$a'b$1$` | `H` |
| `1 -- x` CR `2` | `N_L` | | `$1a$a'b$1a$` | `H` |
| `1 / / x` | `N_P_P_W` | | `$1$x` | `h` |
| `'a\'b'` | `S` | | `KILL` U+00A0 `QUERY` | `W_W` |

The text `0x1F 1e3 1_000 .5 1.e3 0b101 1e-5 0x1p-3` gives eight number tokens with a blank between two of them. For
each text, the tokens joined in order give the text again, with no gap.

### 7.6 Placeholders

A placeholder names a variable of the dashboard. The grammar restates the Grafana forms that Hune reads:

| Form | Grammar |
|---|---|
| `$name` | `$` and the longest run of `[A-Za-z0-9_]`. |
| `${name}` | `${`, a run of `[A-Za-z0-9_]`, and `}`. |
| `${name:format}` | `${`, the name, `:`, one format of 7.9, and `}`. |

A code token is a blank, a word, a number, a punctuation or an unknown token. The engine finds the placeholders in the
bytes of each run of code tokens. It also finds them in the content of each string and each quoted name. A placeholder
never spans a comment or a string.

| Place | Placeholders |
|---|---|
| Code | Expanded (7.8). A name that starts with `__` is a macro (7.10). |
| String, quoted name | Expanded (7.8). A name that starts with `__` stays text. |
| Line comment, block comment | Not read. The text stays. |
| Heredoc | A placeholder in a heredoc is a document error (7.18). |

- A name that is not a variable of the dashboard stays text, as in Grafana. The save gives a warning (7.18).
- The longest run decides the name: `$env_name` names the variable `env_name`, not `env`. Write `${env}_name` to name
  `env`.
- The form `[[name]]` is not a placeholder: it stays text. The importer rewrites it to `${name}` (15).
- In code, a `${` that does not match the grammar is a document error. In a string, it stays text.
- In a string, a `${name:format}` with a format that 7.9 does not list stays text, as in Grafana. The save gives a
  warning (7.18). Example: `'${env:date}'` stays as it is.

### 7.6a ClickHouse query parameters

A query parameter of ClickHouse is the form `{name:Type}` in code. ClickHouse puts the value of the URL parameter `param_<name>` in place of the form. The value never becomes SQL text. This form adds to the placeholders of 7.6 and does not replace them.

The strict form has these parts, with no byte between two parts:

| Part | Rule |
|---|---|
| `{` | A punctuation token in code. In the stored text, the byte before it is not `$`. |
| Name | `[A-Za-z_][A-Za-z0-9_]*`. It names a variable of the document. |
| `:` | One byte. |
| Type | One text of the table of types below, with the same spelling and the same case. |
| `}` | One byte. |

The candidate rule. A code `{` is a candidate when its next tokens are a word or a quoted name, then `:`. Blanks and comments can be between them. In the stored text, a `{` right after `$` is not a candidate. ClickHouse skips blanks and comments in the braces, so it reads such a candidate as a query parameter. A `param_x` of the request applies to each candidate with the name `x`.

ClickHouse 26.8 refuses a quoted name in the braces with a syntax error. Hune refuses it too, so a later version cannot read it in another way.

- Each candidate MUST be the strict form. Another candidate is a document error (7.18). Examples are `{ x:String}`, `{x : String}`, `{/* c */x:String}`, `` {`x`:String} `` and `{"x":String}`.
- A candidate with the strict syntax and a type outside the table is a document error too, for example `{x:Identifier}`.
- In a string, a quoted name, a comment or a heredoc, the form is text. ClickHouse does not read it there.
- A `${name:format}` is a placeholder of 7.6, not a query parameter. A `{` that a number follows, such as `{1:2}`, is not a candidate.
- The engine never expands the name in the braces as text. The name has no `$`, so the scan of 7.6 never reads it.

Places. A query parameter is permitted only on a ClickHouse source. The SQL of a panel, of an extra query, of a `query` variable and of an annotation can hold it. On a `postgres` source, a candidate is a document error (7.26). The query of a `mongodb` source is JSON, and its grammar has no such form (7.28).

Variables. The name MUST be a variable of the kind `text`, `custom` (closed or open), `query` or `constant`. These names are refused:

- an `interval` variable, which keeps the text form;
- a variable with an `all_value`, because "All" then gives author text for the SQL;
- a name that starts with `__`;
- a variable of another kind, for example a kind that a later version adds.

Types. The list is closed:

| Type | Shape | Grammar of one value |
|---|---|---|
| `String` | scalar | Any value of 7.7. |
| `Int64` | scalar | `^-?(0\|[1-9][0-9]*)$`, from -9223372036854775808 to 9223372036854775807. |
| `UInt64` | scalar | `^(0\|[1-9][0-9]*)$`, from 0 to 18446744073709551615. |
| `Float64` | scalar | The number grammar of 7.7, and a finite value as a 64-bit float. |
| `Date` | scalar | `YYYY-MM-DD`, a real calendar date from 1970-01-01 to 2149-06-06. |
| `Array(String)` | list | Each item as `String`. |
| `Array(Int64)` | list | Each item as `Int64`. |
| `Array(UInt64)` | list | Each item as `UInt64`. |

Hune refuses all other types. Examples are `Identifier`, `DateTime`, `DateTime64`, `Nullable`, `LowCardinality`, `Decimal`, `Enum`, `UUID`, `Bool`, `Map`, `Tuple` and an alias such as `string` or `Int`. `Identifier` would put a name in the SQL.

The grammars come from measures on ClickHouse 26.8.34.13. A zero before the first digit of an integer, such as `007`, gives the code 457, or 130 in an array. An integer out of range wraps with no error: `9223372036854775808` becomes the lowest `Int64`. A `Date` out of range becomes the nearest bound, and `2026-02-30` becomes `2026-03-02`. Thus Hune checks each value before the send, and the ClickHouse suite keeps these measures (C40).

The shape rule. A scalar type is for a variable that gives one value. These are a `text` variable, a `constant`, and a `custom` or `query` variable without `multi` and `include_all`. An Array type is for a `custom` or `query` variable with `multi` or with `include_all`. Another pair is a document error.

Other rules for one SQL text:

- One name with two types is a document error.
- One variable in a query parameter and in a placeholder of 7.6 is a document error. The second argument of `$__conditionalAll` is a placeholder for this rule.
- The text has `MAX_QUERY_PARAMS` (32) names of query parameters at most.

Run time. The steps of 7.2 get these additions:

1. Before the expansion, the engine finds the candidates of the stored text and applies the rules above. A fault gives `422 sql_invalid` with the line and the column.
2. The engine expands the text (7.11). It copies each form `{name:Type}` to the output as it is.
3. The engine finds the candidates of the final text again. Here a `{` right after `$` is a candidate too when its name has a pair in the stored text. Each candidate MUST be a strict form whose name and type are a pair of the stored text. Else the answer is `422 sql_invalid`.
4. For each name of the final text, the engine checks the value of the request with the grammar of its type.
5. The engine makes the text of each value and checks the caps below.

Step 3 is necessary because author text in code can make a new form, for example `{x:Identifier}`. ClickHouse 26.8 reads `a${t:Identifier}` as the name `a$` with the alias `{t:Identifier}`. Thus step 3 also reads a `{` right after `$`.

ClickHouse puts a value only where the request has a pair. A `${name:format}` whose name is not a variable stays text (7.6), and its name has no pair. Thus such a text gets no check. A name that only the stored text has gets no check of its value and goes to no request, because `$__conditionalAll` can remove its form.

Values. The checks of 7.7 come first. Then:

- A scalar type takes exactly one value. No value, or a list, gives `422 variable_value_refused`.
- An Array type takes 0 to `MAX_PARAM_ITEMS` (1,000) values. No value gives `[]`. The one value of a variable without `multi` gives a list of one item.
- Each value MUST match the grammar of its type.

A fault gives `422 variable_value_refused` with the variable and a reason that names the type. The reason never holds the value. Nothing goes to the source.

The text of a value is the text that ClickHouse reads in its escaped format:

| Type | Text |
|---|---|
| `String` | The value with `\` written `\\`. A TAB, a line feed, a carriage return and a zero byte get their escape sequence, but 7.7 refuses these characters first. |
| `Int64`, `UInt64`, `Float64`, `Date` | The value as the request gives it. |
| An Array type | `[`, the items joined by `,`, and `]`. A `String` item is in `'...'`, with the escapes of `String` and with `'` written `\'`. A number item is as it is. |

The request. The client adds one pair `&param_<name>=<text>` for each name of the final text, in the byte order of the names. The text is percent-encoded by the rule of 8.3. The pairs come after the parameters of the table of 8.3. The SQL body holds no value of a query parameter.

| Cap | Value |
|---|---|
| Items of one Array value | `MAX_PARAM_ITEMS`: 1,000 |
| Bytes of all the pairs after the percent encoding, with each `&` | `MAX_PARAM_BYTES`: 4,096 |

A value above a cap gives `422 variable_value_refused` with a reason that ends with "use the text form for long lists.". The cap of 32 names is a rule of the stored text (above), so the final text has 32 names at most too. The request line then stays below 8 KiB, the limit of many proxies in front of ClickHouse. The `http` crate refuses a request target above 65,534 bytes.

The property PQ. A variable of a query parameter has no placeholder in the same text. Thus, for these variables, the final SQL is the same, byte for byte, for all values. The values go only in the URL. A unit test checks PQ with seeded random values (16.9).

Other rules:

- The member `sql` of 6.20 (Inspect) shows the final SQL with the forms and with no value.
- The log line `query.run` holds the SHA-256 of the final SQL, so it is the same for all values of these variables.
- ClickHouse codes 456 `UNKNOWN_QUERY_PARAMETER` and 457 `BAD_QUERY_PARAMETER` give `422 query_error` (8.5). The checks of this section stop both causes before the send. The ClickHouse suite sends each type and each value of a hostile set, and no answer has one of these codes.
- The ClickHouse plugin of Grafana has no query parameters. The importer keeps the text, and the checks of 7.18 apply to it.

### 7.7 Variable values in a request

The member `variables` of a query request is an object. A member name is a variable name, and its value is
`{"all": <boolean>, "values": [<string>, ...]}`.

- Each member names a variable of the document. Another name gives `400 bad_request`.
- Each variable that the SQL to expand names MUST have a member. An absent member gives `400 bad_request`.
- Each value is a JSON string of 0 to 1,024 bytes, with no control character (U+0000 to U+001F, U+007F). `values`
  has 10,000 items at most. A value that breaks this rule gives `422 variable_value_refused`.

| Variable (4.7 to 4.10) | `all: false` | `all: true` |
|---|---|---|
| `custom`, not `multi`, closed | Exactly 1 value. It MUST equal the `value` of an option. | Permitted with `include_all` only. `values` MUST be empty. |
| `custom`, `multi`, closed | 0 to 10,000 values. Each MUST equal the `value` of an option. | As above. |
| `custom`, not `multi`, open | Exactly 1 value of 1 to 1,024 bytes. It need not be an option. | As above. |
| `custom`, `multi`, open | 0 to 10,000 values of 1 to 1,024 bytes each. They need not be options. | As above. |
| `query`, not `multi` | 0 or 1 value. 0 values is the empty selection of an options query with no row. | Permitted with `include_all` only. Without `all_value`, `values` holds the options that the page shows. |
| `query`, `multi` | 0 to 10,000 values. | As above. |
| `text` | Exactly 1 value. With `value_type = "number"`, it MUST match the number grammar. | Refused. |
| `interval` | Exactly 1 value. It MUST equal an item of `intervals`, or be `$__auto` when `auto` is `true`. | Refused. |
| `constant` | The member is optional. A request that has it MUST have `all: false` and exactly 1 value that equals the `value` of the document. | Refused. |

A fault of this table gives `422 variable_value_refused` with the variable name and the reason. With `all: true`, the
expansion uses `all_value` when the variable has one, and `values` MUST then be empty. A `custom` variable without
`all_value` uses all its option values, in document order.

A `query` variable without `multi` and with 0 values expands as the empty list of 7.8: `''` with the formats none,
`singlequote` and `sqlstring`, and the empty text with `raw` and `csv`. In a string, it gives the empty content.
Grafana also gives an empty value in this case.

The engine puts each value in one of two classes:

- Author text: an option value of a closed `custom` variable and an `all_value`. Each value of the "All" list of a `custom` variable is also author text, for a closed and for an open variable. The server builds this list from the options. Also an item of an `interval` variable, the text of "Auto" (7.4a) and the value of a `constant`. Author text comes from the document, so it has the trust of the SQL text.
- Free text: a value of a `text` variable, a value of a `query` variable, and a value of an open `custom` variable. An item of the "All" list of a `query` variable is also free text.

An `interval` variable that the SQL names MUST have a member in the request, as any variable does. A `constant` that the SQL names does not need one: the server takes the `value` of the document. A request member for a `constant` that differs from the document gives `422 variable_value_refused` with the reason "A constant takes its value from the document.". 

The server checks the values of an `interval` with a set, as it does for a `custom` variable. The check comes before any source query. The value `$__auto` with `auto` = `false` gives `422 variable_value_refused`. The text of "Auto" comes from 7.4a, with the time range of the request.

The number grammar is `^-?[0-9]+(\.[0-9]+)?$`.

The server keeps no value between two requests.

A repeated panel or section (4.13a) needs no rule in this section. The page sends one request for each copy. In this request, the repeat variable has `all` equal to `false` and one item in `values`. The item is an option of the variable.

A closed `custom` variable gets the check of the table above. A free value gets the property P (7.11) and the rules of 7.24 and 7.35. The server cannot see that the request is for a copy, and it reads no member `repeat`.

The member `allow_custom_value` (4.8, 4.9) has these effects on the server:

- A `query` variable: none. A request for an open and for a closed `query` variable gives the same expansion, byte for byte, or the same error.
- A closed `custom` variable: the table above. Each value that is not an option gives `422 variable_value_refused` and no source query. This holds for a value with another case, with a blank or with another Unicode form.
- An open `custom` variable: the table above, and the class "free text". The save and the run read the member, not the kind (7.8, 7.18). A request for an open `custom` variable with a value outside its options is not an error.
- The check of a closed `custom` variable uses a set of the options. The time is in proportion to the count of the values and the options, not to their product. A unit test runs 10,000 values against 10,000 options in 250 ms or less (16.3).
- The properties P, PG and PM do not change (7.11, 7.24, 7.35). The member adds no new way to put a value in code.

### 7.7a The filters of an `adhoc` variable

The member of an `adhoc` variable (4.10c) in `variables` has another form. It is the closed object `{"filters": [{"key": <string>, "op": <string>, "value": <string>},...]}`.

```json
"filters": { "filters": [ { "key": "host", "op": "=", "value": "web-1" }, { "key": "env", "op": "!=", "value": "dev" } ] }
```

- The member is optional. An absent member and an empty list give no filter.
- The object has the member `filters` and no other member. Each item has the members `key`, `op` and `value`, each a JSON string, and no other member. Another shape gives `400 bad_request`.
- The list has `MAX_ADHOC_FILTERS` (16) items at most. A longer list gives `422 variable_value_refused` before the server reads the items.
- `op` is `=` or `!=`. Another text gives `422 variable_value_refused`. The operators `<`, `>`, `<=`, `>=`, `IN`, `=~` and `!~` are not operators of Hune. With `<` and `>`, a viewer could find the hidden values of a column by bisection.
- `key` MUST be equal, byte for byte, to a key of the variable and to an item of `clickhouse "<id>".adhoc_columns` of its source. Else the answer is `422 variable_value_refused`. A change of the document or of the configuration thus fails closed.
- `value` has 0 to 1,024 bytes and no control character (U+0000 to U+001F, U+007F). Else the answer is `422 variable_value_refused`.

The reason of a refusal names the position of the filter, for example "Filter 2: the operator must be = or !=.". It never holds a value. A filter is never ignored: one fault refuses the request. The checks run before any source query, on each query route that receives the member (6.20, 6.21, 6.21a, 6.22). The filters apply only where a panel query has the macro.

The server keeps no filter between two requests. The log line `query.adhoc` (14.7) holds the count and the keys of the filters. It never holds a value or an operator.

### 7.8 Lexical contexts and escape rules

The context of a placeholder is the token that holds it (7.6). A selection is a single value, a list (a `multi`
variable, or "All" without `all_value`), or the `all_value`.

| Context | Selection | Format (7.9) | Result |
|---|---|---|---|
| String | single value or `all_value` | none, `raw`, `csv` | The value with the string escapes below. |
| String | list | `csv` | The values joined by `,`, then with the string escapes. |
| String | list | none, `raw` | Document error (7.18). |
| String | any | `singlequote`, `sqlstring` | Document error (7.18). |
| String | single value, `all_value` or list | `json`, `pipe`, `regex`, `glob`, `lucene`, `percentencode`, `queryparam`, `distributed`, `doublequote`, `html`, `text` | The text of 7.9, then the string escapes below on the whole text. |
| Quoted name | single value or `all_value` of a closed `custom` variable | none, `raw`, `csv` | The value with the name escapes below. |
| Quoted name | a `text` variable, a `query` variable, an open `custom` variable, or a list | any | Document error (7.18). |
| Quoted name | single value or `all_value` of a closed `custom` variable | the formats of the last group of 7.9 | Document error (7.18). |
| Comment | any | any | No change. |
| Heredoc | any | any | Document error (7.18). |
| Code | `all_value` | any | The `all_value` as it is. |
| Code | single value of a closed `custom` variable | none, `raw`, `csv` | The value as it is. |
| Code | single free value that matches the number grammar | none, `raw`, `csv` | The value as it is. |
| Code | single free value that does not match it | none, `raw`, `csv` | `422 variable_value_refused`. |
| Code | list | none, `singlequote` | Each value as a string literal (`'` and the string escapes), joined by `,`. |
| Code | single value or list | `singlequote` | Each value as a string literal, joined by `,`. |
| Code | single value or list | `sqlstring` | Each value in `'...'` with `'` written `''` and `\` written `\\`, joined by `,`. |
| Code | any | the formats of the last group of 7.9 | Document error (7.18). |
| Code | list of values of a closed `custom` variable, or the list "All" of a `custom` variable | `raw`, `csv` | The values as they are, joined by `,`. |
| Code | list with a free value | `raw`, `csv` | Each free value MUST match the number grammar, else `422 variable_value_refused`. The values are joined by `,`. |
| Code | empty list | none, `singlequote`, `sqlstring` | `''` (an empty string literal, as Grafana gives). |
| Code | empty list | `raw`, `csv` | The empty text. |

The kinds `interval` and `constant` use the rows of a closed `custom` variable that has one value. In a string, the text gets the string escapes. In a quoted name, it gets the name escapes. In code, it stays as it is.

The formats of 7.9 apply as for one author value. A value of an `interval` has only letters and digits, so no escape changes it. A `constant` can hold any text of 4.10b, so the escapes matter for it.

String escapes (from ClickHouse Dash `quote_string`): `\` becomes `\\` and `'` becomes `\'`. Name escapes: `\` becomes
`\\`, and the quote mark of the name (`` ` `` or `"`) gets a `\` before it. A value has no control character (7.7), so
no other escape applies.

The table gives the Grafana output for each value that has no `'` and no `\`. For a value with one of them, Hune
escapes where Grafana breaks the SQL.

The formats of the last group of 7.9 put free text in the SQL. After such a format, only the string
escape keeps the text inside its literal. So these formats are valid in a string only. The engine builds the text of the
format first. Then it applies the string escapes to the whole text.

Thus a quote mark or a backslash that the format adds is escaped too, and the property P (7.11) holds. In code and in a
quoted name, the escape of 7.8 does not make the text safe, so these formats are refused there.

An open `custom` variable has the rows of a `query` variable. A free value in code gives the value as it is only when it matches the number grammar. Else it gives `422 variable_value_refused`. In MongoDB, a free value never starts with `$` (7.35, PM2).

### 7.9 Formats

| Format | Grafana meaning | Hune |
|---|---|---|
| none | single value as it is, list as `'a','b'` | 7.8, with the escapes of 7.8 and the number rule for free text in code |
| `csv` | values joined by `,` | 7.8 |
| `singlequote` | each value in `'...'` with `'` escaped by `\` | 7.8, and `\` also escaped |
| `sqlstring` | each value in `'...'` with `'` doubled | 7.8, and `\` also escaped |
| `raw` | value as it is | 7.8: as it is only for author text and numbers |

In code, another format is a document error (7.18).

The last group of formats is valid in a string only (7.8). The engine builds the text of the format from
the values, in order. Then it applies the string escape of 7.8 to the whole text.

The table gives the text before this escape. `v` is one value, and `a` and `b` are values of a list. The name of the variable is `name`. A list has 0 or more
values.

| Format | One value | List |
|---|---|---|
| `json` | `v` | A JSON array of the values as JSON strings: `["a","b"]`. |
| `pipe` | `v` | The values joined by `\|`. |
| `regex` | `v` with a `\` before each of `\ ^ $ * + ? . ( ) \| [ ] { } /` | Each value as for one value. One value gives that text. Else `(a\|b)`. |
| `glob` | `v` | Two values or more give `{a,b}`. Fewer values give the values joined by `,`. |
| `lucene` | `v`, escaped (below) | No value gives `__empty__`. Else `("a" OR "b")`, each value escaped. |
| `percentencode` | `v` percent-encoded: only `A-Z a-z 0-9 - _ . ~` stay | The percent encoding of `{a,b}`. |
| `queryparam` | `var-name=v`, with `v` encoded as `toUrlParams` of Grafana (a space is `+`, and `@ : $ , ;` stay) | `var-name=a&var-name=b`. No value gives `var-name=`. |
| `distributed` | `v` | `a,name=b`: the first value as it is, each other value after `name=`, joined by `,`. |
| `doublequote` | `"v"`, with a `\` before each `"` of `v` | `"a","b"` |
| `html` | `v` with `&`, `<`, `>`, `'` and `"` as the entities `&amp;`, `&lt;`, `&gt;`, `&#39;` and `&quot;` | The values joined by `, `, then escaped as for one value. |
| `text` | `v` | The values joined by ` + `. |

- The text of an option is not known to the engine (7.7). So `text` gives the value, and not the text of its option.
- With "All" selected, `text` gives `All`. With "All" and an `all_value`, `queryparam` gives `var-name=$__all`,
  `percentencode` encodes the `all_value`, and the other formats give the `all_value` as it is.
- The format `lucene` leaves a value as it is when the JavaScript call `Number(v)` gives a number. Examples are the empty
  text, `12` and `0x1f`. Else it puts a `\` before each of `! * + - = < > & | ( ) [ ] { } ^ ~ ? : \ / "` and before each
  white space character.
- A format that this specification does not list stays text in a string, and a document error in code. This includes
  `date`, which gives `NaN` in Grafana, `join`, `uriencode` and `customqueryparam`.

Worked example. The stored SQL is `SELECT 1 FROM t WHERE match(host, '^${host:regex}$')`. The `multi` variable `host`
has the values `a.b` and `c'd`. The final SQL is `SELECT 1 FROM t WHERE match(host, '^(a\\.b|c\'d)$')`. The format gives
the text `(a\.b|c'd)`. The string escape then doubles the backslash and escapes the quote mark.

### 7.10 Macros

The macros restate the ClickHouse plugin 4.5.1 of Grafana. The binary of this plugin holds the same format texts.
`$__interval_ms`, `$__from` and `$__to` are built-in variables of Grafana. Grafana replaces them in the browser. Hune
expands them in code, as it expands a macro.

`F = floor(from_ms / 1000)` and `T = floor(to_ms / 1000)` are Unix seconds. `S` is the interval of 7.4 in seconds, and `I` is the same interval in milliseconds. `D1` and `D2` are the UTC dates
`YYYY-MM-DD` of `from_ms` and of `to_ms` (7.3).

| Macro | Arguments | Expansion |
|---|---|---|
| `$__timeFilter(expr)` | 1 | `expr >= toDateTime(F) AND expr <= toDateTime(T)` |
| `$__timeFilter_ms(expr)` | 1 | `expr >= fromUnixTimestamp64Milli(from_ms) AND expr <= fromUnixTimestamp64Milli(to_ms)` |
| `$__fromTime` | 0 | `toDateTime(F)` |
| `$__toTime` | 0 | `toDateTime(T)` |
| `$__fromTime_ms` | 0 | `fromUnixTimestamp64Milli(from_ms)` |
| `$__toTime_ms` | 0 | `fromUnixTimestamp64Milli(to_ms)` |
| `$__timeInterval(expr)` | 1 | `toStartOfInterval(toDateTime(expr), INTERVAL S second)` |
| `$__timeInterval_ms(expr)` | 1 | `toStartOfInterval(toDateTime64(expr, 3), INTERVAL I millisecond)` |
| `$__interval_s` | 0 | `S` as a decimal integer |
| `$__interval_ms` | 0 | `I` as a decimal integer |
| `$__from` | 0 | `from_ms` as a decimal integer |
| `$__to` | 0 | `to_ms` as a decimal integer |
| `$__dateFilter(expr)` | 1 | `expr >= toDate('D1') AND expr <= toDate('D2')` |
| `$__dateTimeFilter(date_expr, time_expr)` | 2 | `(date_expr >= toDate('D1') AND date_expr <= toDate('D2')) AND (time_expr >= toDateTime(F) AND time_expr <= toDateTime(T))` |
| `$__dt(date_expr, time_expr)` | 2 | The expansion of `$__dateTimeFilter(date_expr, time_expr)`, with the same checks and the same property P (7.11). The plugin has the two names. |
| `$__conditionalAll(condition, $variable)` | 2 | `1=1` or `condition` (below) |

- A macro expands only in code. In a string, a quoted name or a comment, its text stays.
- After a macro with an argument, the next byte MUST be `(`. The argument is the text between this `(` and the `)` that
  closes it. The engine counts only `(` and `)` punctuation tokens. Thus a parenthesis in a string or a comment does
  not count.
- A `,` punctuation token at depth 0 inside the parentheses separates two arguments. The macro MUST have the number of
  arguments of the table. Each argument MUST have at least one token that is not a blank.
- The engine removes the blank tokens at the start and at the end of each argument. A comment in an argument stays.
- The engine expands the placeholders of each argument first, with the rules of code (7.8). This is the order of
  Grafana: variables first, then macros. The exception is `$__conditionalAll` (below).
- A macro with no argument followed directly by `(` is a document error.
- A macro or a built-in variable with no argument can also be written `${__name}`. A format after the name, such as
  `${__from:csv}`, is a document error.
- Any other `$__name` in code is a document error with the text "$__name is not a macro of Hune". Examples are
  `$__range`, `$__range_s`, `$__range_ms`, `$__interval` and `$__timezone`. Grafana leaves `$__range`, `$__range_s` and
  `$__range_ms` as text, and it gives `$__interval` as a text such as `20s`. None of them is SQL.
- The built-in variables take no argument. A `(` directly after `$__interval_ms`, `$__from` or `$__to` is a document
  error.
- A macro name in a string, a quoted name or a comment stays text. The names `$__interval`, `$__range`, `$__range_s`
  and `$__range_ms` also stay text there. Grafana replaces some of these names in a string. The save gives a warning
  (7.18).
- A variable query (7.14) that names a macro that needs the interval is a document error. These macros are
  `$__timeInterval`, `$__timeInterval_ms`, `$__interval_s` and `$__interval_ms`. The other macros use the time range of
  the request.
- The macros with the suffix `_ms` serve the columns of type `DateTime64` and the intervals below 1 s.
  `$__dateFilter(expr)` lets ClickHouse prune the partitions of a table with a `Date` key.

`$__conditionalAll(condition, $variable)` makes a filter that "All" switches off:

1. The second argument MUST be exactly one placeholder (`$name` or `${name}`, with no format). It MUST name a variable
   of the kind `query` or `custom` of the document. Else the save refuses the document (7.18). A variable of the kind `interval` or `constant` gives the same error, because it has no "All".
2. The macro gives `1=1` when the viewer selected "All" (the request has `all: true`, with or without `all_value`).
   Grafana also gives `1=1` for a variable with no value or with one empty value. Hune does not: it gives the condition.
   Grafana also accepts a second argument that is not a variable, and then it keeps the condition. Hune refuses it.
3. Else the macro gives the first argument, with its placeholders expanded by the rules of code (7.8). The second
   argument never goes to the output.
4. The engine does not expand the first argument when the macro gives `1=1`. So a value that the rules of code would
   refuse causes no error then.
5. The property P (7.11) holds for each free value in the first argument, as for any text in code.
6. A request that has no member for the variable of the second argument gives `400 bad_request` (7.7).

Example: `WHERE $__conditionalAll(env IN ($env), $env)` with the `custom` `multi` variable `env` gives
`WHERE env IN ('prod','dev')` for the values `prod` and `dev`. It gives `WHERE 1=1` for "All".

Worked example. The stored SQL is:

```sql
SELECT $__timeInterval(event_time) AS t, node, sum(weight) AS weight
FROM analytics.weights
WHERE $__timeFilter(event_time) AND node IN ($node) AND env = '$env'
GROUP BY t, node
ORDER BY t
```

The request has the range of 7.3 and `max_data_points = 1200`. The `multi` query variable `node` has the values
`a1` and `b'2`. The `custom` variable `env` has the value `prod`. Then `I = 20,000`, `S = 20`, `F = 1794211200` and
`T = 1794232800`. The final SQL is:

```sql
SELECT toStartOfInterval(toDateTime(event_time), INTERVAL 20 second) AS t, node, sum(weight) AS weight
FROM analytics.weights
WHERE event_time >= toDateTime(1794211200) AND event_time <= toDateTime(1794232800) AND node IN ('a1','b\'2') AND env = 'prod'
GROUP BY t, node
ORDER BY t
```

### 7.10a The macro `$__adhocFilters`

The macro `$__adhocFilters` puts the filters of the `adhoc` variable of the source in the SQL of a panel. The SQL can also write it `${__adhocFilters}`. It takes no argument and no format.

1. It is permitted in the SQL of a panel and of an extra query (4.15a) of a `clickhouse` source. The document MUST have an `adhoc` variable whose `source` is the source of the query.
2. It is refused in the SQL of a variable and of an annotation, and inside the argument of another macro. It is refused on a `postgres` or a `mongodb` source.
3. It expands only in code. In a string, a quoted name or a comment, its text stays (7.10).
4. It MUST be one term of the top-level `AND` of the `WHERE` or `PREWHERE` clause of the outer statement. So a filter never adds a row that the rest of the clause refuses. These six conditions apply to the code tokens of the stored text. Blanks and comments can be between the tokens, and the words match in any case.
   1. The `$` is at depth 0: the count of `(` and `[` before it equals the count of `)` and `]`. Thus a subquery, a CTE, a function argument or a parenthesis cannot hold the macro.
   2. The nearest clause word at depth 0 before it is `WHERE` or `PREWHERE`. The search back stops at `SELECT`, `FROM`, `JOIN`, `ON`, `USING`, `WITH`, `ARRAY`, `SAMPLE` and `BY`, and at each end word of condition 5 except `WHERE`.
   3. No set operation `EXCEPT` at depth 0 comes before it. The column transformer `* EXCEPT (...)` is permitted.
   4. The code token before it is the clause word or a logical `AND`. The `AND` of a `BETWEEN` is not a logical `AND`. The macro is not inside `CASE ... END`.
   5. The code token after it is `AND`, the end of the clause, or no token. The end of the clause is a `)` with no match, a `;`, or an end word. The end words are `GROUP`, `ORDER`, `HAVING`, `LIMIT`, `OFFSET`, `SETTINGS`, `FORMAT`, `UNION`, `EXCEPT`, `INTERSECT`, `WINDOW`, `QUALIFY`, `WHERE` and `INTO`.
   6. The clause has no `OR`, no `?` and no `AS` at depth 0 outside `CASE ... END`.

   A token MUST start at the `$`, so `AND$__adhocFilters` is refused. The save and the run use the same rule. They read the tokens of a text one time. Thus the time grows with the length of the text, not with the count of macros. A macro in a CTE, in a subquery of `FROM` or in a subquery of `IN` is refused. Move it to the `WHERE` clause of the outer statement.
5. With no filter, the text is `1=1`.
6. Else the text is `(` and the filters joined by ` AND `, then `)`. A filter is the column in backquotes, a blank, the operator, a blank and the value as a string literal.
7. The text of the column comes from the item of the configured list, with the name escapes of 7.8. It never comes from the bytes of the request: the request key is only compared (7.7a). A configured item is a plain name, so no escape changes it.
8. The value is a string literal with the string escapes of 7.8, also for a number, a date or an `Enum` column. ClickHouse converts the literal to the type of the column for the comparison. The ClickHouse suite measures this conversion (16.9).
9. Hune does not use the setting `additional_table_filters`, and it never adds the filters to a query that has no macro.

The measure of 2026-11-22 on ClickHouse 26.8.34.13 is in the test `crates/hune-source/tests/adhoc_clickhouse.rs` (16.9). The literal compares with a number, a date, a `Decimal`, an `Enum` and a `LowCardinality` column as a value of the type of the column. A `Nullable` column never matches its NULL rows, with `=` and with `!=`. A text that does not convert gives the ClickHouse code 53 and `422 query_error`, for example `abc` or `07` for an `Int32` column. An invalid date such as `2026-13-45` converts with no error, and an unknown name of an `Enum` matches no row.

A fault of rules 1 to 4 is a document error at save (7.18, items 19 and 20). At run time, it gives `422 sql_invalid`.

Rule 4 is about the clause, not about the result. A `LIMIT`, an aggregate or a window function can still show a row that the query with no filter does not show. The filter removes other rows first. For example, `ORDER BY v DESC LIMIT 3` shows the next 3 rows after a filter.

The rule reads the stored text. Two forms of author text go into the SQL with no parentheses: the condition of `$__conditionalAll` (7.10) and an author value in code (7.8).

The rule sees the condition of `$__conditionalAll` inside the parentheses of the macro. So `$__conditionalAll(a = 1 OR b = 2, $v) AND $__adhocFilters` gives `a = 1 OR b = 2 AND (...)`, and the filters do not apply to the rows where `a = 1`. A condition with an open end, such as `x NOT BETWEEN 0`, joins the `AND` of the macro, and then a filter can add rows. Write a whole condition in parentheses: `$__conditionalAll((a = 1 OR b = 2), $v)`.

Example. The stored SQL is:

```sql
SELECT service, count() AS n FROM analytics.events
WHERE $__timeFilter(ts) AND $__adhocFilters
GROUP BY service
```

The request has the filters `host = web-1` and `env != it's`. The final SQL is:

```sql
SELECT service, count() AS n FROM analytics.events
WHERE ts >= toDateTime(1794211200) AND ts <= toDateTime(1794232800) AND (`host` = 'web-1' AND `env` != 'it\'s')
GROUP BY service
```

The expansion records the range of each literal of a value as a free value of the code context (7.11). The property P checks that each range is exactly one string token.

### 7.11 Expansion and the property P

The engine expands a text in one pass:

1. It reads the stored text with the lexer (7.5). A lexer error stops the pass.
2. It copies each token to the output in order. In a run of code tokens, it replaces each placeholder (7.8) and each
   macro (7.10). In a string or a quoted name, it replaces each placeholder of the content (7.8). It keeps the quote
   marks.
3. It never reads an inserted text again. A value that holds `$x` or `$__fromTime` stays text. Thus no expansion can
   loop.
4. It records, for each inserted free value, the byte range of its text in the output, and its context. A macro can
   copy its argument more than one time: `$__timeFilter(expr)` writes `expr` two times. The engine then records one
   range for each copy of each free value of the argument.
5. It reads the output with the lexer. A lexer error gives `422 sql_invalid`.
6. It checks the property P for each recorded range.

The property P: each free value stays inside its literal.

- String context: the range is inside one closed string token. This token starts and ends at the places of the
  original string token in the output.
- Code context, numbers: the range is exactly a sequence of numbers, with one `,` punctuation token between two
  numbers. A number is one number token, or one `-` punctuation token directly followed by one number token.
- Code context, string literals: the range is exactly a sequence of string tokens, with one `,` punctuation token
  between two strings. This case covers a list and the formats `singlequote` and `sqlstring`.
- In the two code cases, a token starts at the start of the range and a token ends at its end. An empty range has no
  check.

A failure of P gives `422 variable_value_refused` with the reason "The value changes the structure of the statement.".
P does not apply to author text, because the author also wrote the SQL.

A query parameter of 7.6a puts no value in the text, so P has no range to check for it. The property PQ of 7.6a holds for its variables.

The macro `$__adhocFilters` (7.10a) records one range for each value literal of its text, in the code context. P checks that each range is exactly one string token. The column names come from the configuration, so they are author text.

Example: the SQL `WHERE amount > 0-$min AND tenant = 'a'` with the free value `-5` gives `0--5 AND tenant = 'a'`. The
lexer reads `--5 AND tenant = 'a'` as a comment, so P fails and nothing runs. The author writes `0 - $min`.

The seeded random tests of 16 make values from the hostile pieces of the ClickHouse Dash generator `random_sql`. These
pieces are quote marks, backslashes, comment marks, `$` and Unicode spaces. For each SQL and each value, the expansion
either gives an error, or P holds.

### 7.12 Statement check

The engine checks the statement at save, on the stored text, and at run time, on the final text. The depth counts
`(`, `[` and `{` punctuation tokens, as the ClickHouse Dash scan does. Words compare with ASCII letters folded to
upper case.

1. The engine skips blanks, comments and `(` tokens at the start. The first word MUST be `SELECT` or `WITH`.
2. The text holds one statement. A `;` at depth 0 is permitted only as the last token that is not a blank or a comment.
3. No word `FORMAT` at depth 0 is followed by a word, because the server sets the format (8.3). The function
   `format(...)` stays permitted.
4. No word `INTO` at depth 0 is followed by the word `OUTFILE`.
5. No word `INSERT` at depth 0 is followed by the word `INTO`.

A failure gives `422 sql_invalid` at run time and `422 invalid_document` at save. This check is not the security
boundary: the account of 8.12 and its `readonly` profile are.

| SQL | Result |
|---|---|
| `SELECT 1` | Accepted. |
| `/* c */ (SELECT 1) UNION ALL (SELECT 2)` | Accepted. |
| `WITH x AS (SELECT 1) SELECT * FROM x;` | Accepted. |
| `SELECT format('{} {}', a, b) FROM t` | Accepted. |
| `SELECT 1; SELECT 2` | Refused: two statements. |
| `SELECT 1 FORMAT JSON` | Refused: `FORMAT`. |
| `SHOW TABLES` | Refused: the first word. |
| `WITH 1 AS x INSERT INTO t SELECT x` | Refused: `INSERT INTO`. |
| `SELECT 1 INTO OUTFILE 'x'` | Refused: `INTO OUTFILE`. |
| `-- only a comment` | Refused: no first word. |

### 7.13 Limits

| Limit | Value | Error |
|---|---|---|
| Stored SQL of one panel or one variable | `MAX_SQL_BYTES` of 4.35 (256 KiB) | `422 invalid_document` |
| Final SQL | `max_query_size` of the source, in bytes | `422 sql_too_large` |
| Values of one variable in a request | 10,000 | `422 variable_value_refused` |
| Bytes of one value | 1,024 | `422 variable_value_refused` |
| `max_data_points` | 1 to 10,000 | `400 bad_request` |
| Time range | 7.3 | `422 invalid_time_range` |
| Options of one variable | 10,000 rows | `422 query_limit` (7.14) |
| Names of query parameters in one stored text | 32 (7.6a) | `422 invalid_document` at save, `422 sql_invalid` at run time |
| Items of one Array value of a query parameter | 1,000 (7.6a) | `422 variable_value_refused` |
| Bytes of the `param_` pairs of one request | 4,096 (7.6a) | `422 variable_value_refused` |
| Filters of one `adhoc` variable in a request | 16 (7.7a) | `422 variable_value_refused` |
| Rows, bytes and time of a query | 8.3, 8a.6 | 8.5, 8a.8 |
| Lines and characters that a logs panel draws (7.16e) | 10,000 lines, 10,000 characters of one body, 2,000,000 characters of all the bodies (9.19h) | None: the page draws the first lines and shows a note |

Hune sends the same `max_query_size` to ClickHouse (8.3), so ClickHouse never refuses a text that Hune accepted for
its size.

### 7.14 Variable options queries

The route of 6.21, and the preview with `variable` (6.22), run the SQL of a variable of the kind `query`. The pipeline
is the pipeline of 7.2 with these changes:

- There is no panel and no `max_data_points`. The macros that need only the time range use the time range of the
  request. Examples are `$__timeFilter`, `$__fromTime`, `$__toTime`, `$__dateFilter`, `$__from` and `$__to`.
- The variables that the variable SQL names are its parents (4.34 refuses a cycle). The request holds their
  values.
- The effective `max_result_rows` is the lowest of the source key, the ceiling `max_rows` and 10,000. A result with
  more rows gives `422 query_limit`, because `result_overflow_mode` is `throw` (ClickHouse), or because the row check of 8a.6 stops it (PostgreSQL).

The server makes the options from the frame, in row order, with no other change:

- The first column gives `value`. The second column, when it exists, gives `text`. With one column, `text` equals
  `value`. Hune ignores the other columns. This is the rule of the ClickHouse plugin 4.5.1.
- A value of the kind `number` becomes the JSON number text. A `number_text` or `text` value stays as it is. A `bool`
  becomes `true` or `false`. A `time` value becomes the epoch milliseconds as decimal text, the form of Grafana.
- A row whose first value is `null` is left out.

For a source of the kind `mongodb`, the first column is the first field of the first document. The second column is the second field. The author orders the fields with `$project`. A row whose first value is `null` is left out, as above (7.16c). The effective `max_result_rows` is the lowest of the source key, the ceiling `max_rows` and 10,000. Hune adds the stage `$limit` of one more row (8b.6).

The page applies the variable `regex`, removes duplicate values and sorts the options (9.10).

The member `allow_custom_value` changes no rule of this section. The caps of the options query stay: 10,000 rows, the deadline, the read-only account, `dashboard.read` and the rights on the source. They are the same for ClickHouse, PostgreSQL and MongoDB.

### 7.15 Stale etag and 409

- Each panel query and each options query carries `etag`: the etag of the document that the page shows.
- The server compares it with the etag of its catalog. When they differ, the server reads the uid from the store one
  time (5.5, rule 10). When the store gives the etag of the request, the server updates this catalog entry and runs
  the query.
- Else the server answers `409 dashboard_changed` with `current_etag`, and runs nothing.
- When the store read fails, for example during a PostgreSQL failover, the server answers `503 storage_unavailable`
  (retryable) and runs nothing. The page tries the panel again at the next trigger (9.8).
- Thus a page that shows an old version never runs SQL that the store no longer holds. The extra read stops a loop
  between two instances whose catalogs have a different age (5.8).
- On `409 dashboard_changed`, the page reads the document again and runs its panels again (9.8). With a draft in edit
  mode, it shows a notice instead (10.8).

### 7.16 Frame format

The frame is the one answer format of a query, for each kind of source. The server makes it from the rows of the
source.

```json
{
  "etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f",
  "query_id": "019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d",
  "interval_ms": 20000,
  "columns": [
    { "name": "t", "type": "DateTime", "kind": "time" },
    { "name": "node", "type": "LowCardinality(String)", "kind": "text" },
    { "name": "weight", "type": "Decimal(38, 9)", "kind": "number_text" },
    { "name": "blocks", "type": "UInt32", "kind": "number" },
    { "name": "active", "type": "Bool", "kind": "bool" }
  ],
  "values": [
    [1794211200, 1794211220],
    ["a1", null],
    ["12.500000000", "3.000000000"],
    [42, 7],
    [true, false]
  ],
  "rows": 2,
  "stats": { "elapsed_ms": 81, "read_rows": 120000, "read_bytes": 9000000, "result_bytes": 512 }
}
```

| Member | Value |
|---|---|
| `etag` | The `etag` of the request. Absent in a preview. |
| `query_id` | The query id of 8.10. |
| `interval_ms` | `I` of 7.4. |
| `columns` | One item for each column, in the order of the SQL. `name` is the column name, exactly. Two columns can have the same name. `type` is the type text of the source: the ClickHouse type text of the answer, exactly, or the type name of 7.16a. `kind` is the kind of the table below. |
| `values` | One array for each column, in the order of `columns`. Each array has `rows` items. |
| `rows` | The number of rows. |
| `stats` | `elapsed_ms`: the time from the send of the request to the end of the answer, that Hune measures. `read_rows`, `read_bytes` and `result_bytes`: the values of `X-ClickHouse-Summary` (8.4), or `null` when the header does not give them. For a PostgreSQL source, `read_rows` and `read_bytes` are `null`, and `result_bytes` is the count of 8a.6. For a MongoDB source, they are `null` too, and `result_bytes` is the count of 8b.6. |

| Kind | JSON value | Value in the browser (9.12) |
|---|---|---|
| `time` | A number: Unix seconds, with a fraction for `DateTime64`. | A double of epoch milliseconds. |
| `number` | A number, or the text `"nan"`, `"inf"` or `"-inf"`. | A double. |
| `number_text` | A text with the exact integer or decimal. | The exact text, and a double. |
| `text` | A text. | A text. |
| `bool` | `true` or `false`. | A boolean. |
| `null` | Only `null`. | `null`. |

Each kind also takes `null` for a NULL of ClickHouse.

The server finds the kind from the type text. It first removes the wrappers `Nullable(...)` and `LowCardinality(...)`,
in any order and at any depth. The JSON values are the values that ClickHouse gives with the settings of 8.3.

| ClickHouse type | Kind | ClickHouse JSON value | Frame value |
|---|---|---|---|
| `UInt8`, `UInt16`, `UInt32`, `Int8`, `Int16`, `Int32` | `number` | number | The same number. |
| `Float32`, `Float64`, `BFloat16` | `number` | number, `"nan"`, `"inf"`, `"-inf"` | The same value. |
| `UInt64`, `Int64`, `UInt128`, `Int128`, `UInt256`, `Int256` | `number_text` | text of digits | The same text. |
| `Decimal(P, S)`, `Decimal32(S)`, `Decimal64(S)`, `Decimal128(S)`, `Decimal256(S)` | `number_text` | text of the decimal | The same text. |
| `Bool` | `bool` | `true`, `false` | The same value. |
| `Date`, `Date32` | `time` | text `YYYY-MM-DD` | The Unix seconds of 00:00:00 UTC of that day, an integer. |
| `DateTime`, `DateTime(tz)` | `time` | text or number of Unix seconds | A JSON number with the same digits. |
| `DateTime64(s)`, `DateTime64(s, tz)` | `time` | text or number of Unix seconds with `s` decimals | A JSON number with the same digits. |
| `String`, `FixedString(N)`, `UUID`, `IPv4`, `IPv6`, `Enum8(...)`, `Enum16(...)` | `text` | text | The same text. |
| `Nothing` | `null` | `null` | `null`. |
| `SimpleAggregateFunction(f, T)` | the kind of `T` | the value of `T` | The value of `T`. |
| Any other type: `Array`, `Tuple`, `Map`, `Nested`, `JSON`, `Object`, `Variant`, `Dynamic`, the geo types, the `Interval` types, `Time`, `Time64`, `AggregateFunction` | `text` | any JSON value | A text: the value itself when it is a JSON string, else its compact JSON text. |

Example: the `Date` value `"2026-11-09"` gives `1794182400`. The `DateTime64(3)` value `"1794211200.125"` gives the JSON
number `1794211200.125`.

The server checks each value. A value that does not fit its kind gives `502 source_bad_response`:

| Kind | Accepted values |
|---|---|
| `number` | A JSON number, `"nan"`, `"inf"`, `"-inf"`, `null`. |
| `number_text` | A text that matches `^-?[0-9]+$` for an integer type, or `^-?[0-9]+(\.[0-9]+)?$` for a decimal type. `null`. |
| `time` | For `DateTime` and `DateTime64`, a text or a number that matches `^-?[0-9]+(\.[0-9]{1,9})?$`. For `Date` and `Date32`, a text `YYYY-MM-DD` that is a real date. `null`. |
| `bool` | `true`, `false`, `null`. |
| `text` | Any JSON value. |
| `null` | `null`. |

The server changes a value only in three cases. A `Date` becomes seconds. A time text becomes a JSON number with the
same digits. A `text` value that is not a JSON string becomes its JSON text.

A table shows a `number_text` value exactly. Charts, stats, thresholds and transformations use its double, so a value
above 2^53 loses precision there (9.12). A time with more than 3 decimals loses the part below the millisecond in the
browser.

### 7.16a Frame types for PostgreSQL

The server finds the kind of a PostgreSQL column from its type. The client asks for the binary form of each value (8a.5) and decodes it by the type. A domain has the kind of its base type. The member `type` of the frame holds the name of the type in `pg_type.typname`. For an array it holds the name of the element type and `[]`.

| PostgreSQL type | Kind | Frame value |
|---|---|---|
| `bool` | `bool` | `true` or `false`. |
| `int2`, `int4`, `oid` | `number` | The JSON integer. |
| `int8` | `number_text` | The text of the digits, with `-` for a negative value. |
| `float4`, `float8` | `number` | The JSON number as `serde_json` writes it, or the text `"nan"`, `"inf"` or `"-inf"`. A `float4` is written as a `float4`: `0.1` stays `0.1`. |
| `numeric` | `number_text` | The exact decimal text, with the number of decimals of the value (its `dscale`): `12.50` stays `12.50`. The values `NaN`, `Infinity` and `-Infinity` give `null`. |
| `text`, `varchar`, `bpchar`, `name` | `text` | The text, as stored. `bpchar` keeps its blank padding. |
| `uuid` | `text` | The lower-case text with hyphens. |
| `json`, `jsonb` | `text` | The JSON text. `json` gives the text as stored. `jsonb` gives the text that PostgreSQL prints. |
| `inet`, `cidr` | `text` | The text that PostgreSQL prints. An `inet` host address leaves out `/32` and `/128`. |
| An enum type | `text` | The label. |
| `date` | `time` | The Unix seconds of 00:00:00 UTC of that day, an integer. The values `infinity` and `-infinity` give `null`. |
| `timestamptz` | `time` | Unix seconds with up to 6 decimals and no trailing zero. The values `infinity` and `-infinity` give `null`. |
| `timestamp` | `time` | As `timestamptz`, with the value read as UTC. Grafana does the same. |
| `time` | `text` | `HH:MM:SS`, and the decimals of the value without trailing zero. |
| An array of one of the types above, except `json` and `jsonb` | `text` | The compact JSON text of the array (rule below). |
| Any other type | none | `422 query_error` before the query runs (rule below). |

The array rule. The JSON text has one nested array for each dimension. The elements follow these rules:

- A `bool` element is a JSON boolean. A NULL element is `null`.
- An element of the kind `number` is a JSON number. The values `NaN`, `Infinity` and `-Infinity` are the JSON strings `"NaN"`, `"Infinity"` and `"-Infinity"`.
- An element of the kind `number_text` or `text` is a JSON string.
- A `numeric` element with the value `NaN`, `Infinity` or `-Infinity` is the JSON string `"NaN"`, `"Infinity"` or `"-Infinity"`.
- A `date` element is the string `YYYY-MM-DD`.
- A `timestamp` or `timestamptz` element is the string `YYYY-MM-DDTHH:MM:SS[.ffffff]Z`.
- A `date`, `timestamp` or `timestamptz` element with an infinite value is `null`.
- The array itself is `null` when the value is NULL.

The rule for another type. The client reads the types of the columns from the Describe answer, before it runs the query (8a.5, step 6). A column of another type gives `422 query_error` with `sqlstate` set to `null`. The message names the column and the type. The author casts the column in the SQL, for example `col::text`. Examples of such types are `bytea`, `interval`, `money`, `xml`, the range types, the geometric types and each composite type.

A value that does not decode, for example a `numeric` with a bad `dscale`, gives `502 source_bad_response`. A `time` value that does not match `^-?[0-9]+(\.[0-9]{1,9})?$` also gives it (7.16). The member `stats` of the frame has `read_rows` and `read_bytes` set to `null`, because PostgreSQL gives no summary. `result_bytes` holds the count of 8a.6.

Examples: the `timestamptz` value `2026-11-09 08:00:00.125+00` gives `1794211200.125`. The `int8` value 9223372036854775807 gives the text `"9223372036854775807"`. The `numeric(38,9)` value `12.5` gives the text `"12.500000000"`.

### 7.16b The time column of a PostgreSQL time series

Grafana changes the first column of a `time_series` result when the column is named `time` and holds Unix seconds. The macros `$__timeEpoch`, `$__timeGroup` and `$__unixEpochGroup` (7.22) give such a column. The server does the same change for a query of a `postgres` source:

1. The rule applies only when the format of the panel query is `time_series` (4.15) and the frame has no column of the kind `time`.
2. The first column is named `time`, in lower case. Its kind is `number` or `number_text`.
3. Each value is `null`, or a finite JSON number, or a text that matches `^-?[0-9]+(\.[0-9]{1,9})?$`.
4. When all values pass, the kind of the column becomes `time`. Each value is a JSON number with the same digits. The member `type` does not change.
5. Else the frame does not change, and the page shows the error of 9.22.

The server applies the rule after it makes the frame and before it sends the answer (step 15a of 7.2). The preview applies it to the format of the draft panel. The options query (7.14) does not apply it.

### 7.16c Frame types for MongoDB

The result of a query is a stream of documents. The server makes the frame when the stream ends, because it needs all the documents to know the types of the columns. The member `type` of a column holds a type name of the table below. The client of 8b gives each value as a Hune value (8b.16): the type of the value is its BSON type.

The columns are the top-level fields of the documents, in the order of their first appearance. A document that lacks a field gives `null` for it. A nested document is not split into columns: the author uses `$project` or `$replaceWith` to flatten it. A result with no document gives a frame with no column: `columns` is `[]`, `values` is `[]` and `rows` is 0. The page shows the state `empty` (9.8).

The type of one value gives the kind and the frame value:

| BSON type | Type name | Kind | Frame value |
|---|---|---|---|
| Int32 | `int` | `number` | The JSON integer. |
| Int64 | `long` | `number_text` | The text of the digits, with `-` for a negative value. |
| Double | `double` | `number` | The JSON number as `serde_json` writes it, or the text `"nan"`, `"inf"` or `"-inf"`. |
| Decimal128 | `decimal` | `number_text` | The exact decimal text that the server prints, with its scale: `12.50` stays `12.50`. The values `NaN`, `Infinity` and `-Infinity` give `null`. |
| String | `string` | `text` | The text, as stored. |
| Boolean | `bool` | `bool` | `true` or `false`. |
| Date | `date` | `time` | Unix seconds with 3 decimals at most and no trailing zero, computed from the integer milliseconds. The Date 1794211200125 gives `1794211200.125`. |
| ObjectId | `objectId` | `text` | 24 lower-case hexadecimal digits. |
| Binary of the subtype 4 | `uuid` | `text` | The lower-case UUID with hyphens. |
| Document | `object` | `text` | The compact Relaxed Extended JSON v2 text of the value, with the keys in document order. |
| Array | `array` | `text` | The same text. |
| Null, or no field | `null` | `null` | `null`. |

The type of a column is the type of its values, with these rules for a column that holds more than one BSON type. A column of `null` values only has the type `null`.

| Types in the column | Type of the column |
|---|---|
| Int32 and Int64 | `long`. Each Int32 value is a digit text. |
| Int32 and Double | `double`. |
| Int64 and Double | `double`, when no Int64 value has an absolute value above 2^53. Else the query fails (below). |
| Decimal128 with Int32 or Int64 | `decimal`. Each integer value is a digit text. |
| Document and Array | `json`, with the kind `text`. |
| Any other mix, for example Int32 and String, or Decimal128 and Double | The query fails (below). |

The rule for another type. A value of one of these BSON types gives `422 query_error` with `mongo_code` set to `null`. The types are Binary (another subtype), Regular expression, Timestamp, JavaScript code, Symbol, MinKey, MaxKey, DBPointer and Undefined. A mix that the table refuses gives the same error, and so does a column count above `MAX_FRAME_COLUMNS` (256). The message names the column and the types. The author casts the value in the pipeline, for example with `$toString`, `$toDouble` or `$toDate`.

A value that does not decode gives `502 source_bad_response`. A document with a repeated field name, a string that is not valid UTF-8, and a Decimal128 that is not valid are examples. The member `stats` of the frame has `read_rows` and `read_bytes` set to `null`, because the commands give no summary. `result_bytes` holds the count of 8b.6.

Examples: the Int64 value 9223372036854775807 gives the text `"9223372036854775807"`. The Decimal128 value `12.5` stored with the scale 9 gives `"12.500000000"`. The nested value `{"a": 1}` in a column gives the text `{"a":1}`. The choice to refuse nested values is an open question (19.33).

### 7.16d The time column of a MongoDB time series

The format `time_series` of a panel (4.15) needs a column of the kind `time`, as for the other kinds (9.22). A column of Date values has this kind. Hune does not change a number to a time: no rule like 7.16b exists for this kind. A time stored as epoch milliseconds or seconds is a number, and the server cannot know its unit. The author makes a Date with `$toDate` (the value in milliseconds) or with `$dateTrunc` or `$dateFromParts`.

A `$group` stage gives the key `_id`, whose value is often an object. The column `_id` is then a `json` text, and the page cannot draw it. The author adds a `$project` or `$replaceWith` stage. It gives the time column and the value columns at the top level (the example of 7.29). The server never flattens an `_id`.

### 7.16e Frame rules of the format `logs`

The format `logs` of a panel query (4.15, 4.33f) does not change the request, the expansion or the frame. The server runs it as the format `table`, for each kind of source. The caps of 7.13 and of 8.3 apply to the rows and to the bytes of the result, as for any panel query. The page reads the frame with the rules below. They are the rules of the logs panel of Grafana 12.4.2 for a frame with no `meta.type`. The frames of the ClickHouse plugin 4.5.1 have no `meta.type`.

1. The time. The time of a line is the first field of the kind `time`. A row with a time that is `null` shows no time, and it comes last in both orders (9.19h).
2. The body. The body of a line is the first field of the kind `text`. A `null` body is an empty line.
3. The level field. The level field is the field with the name `level`. When no field has this name, it is the field with the name `detected_level`. The match is exact and case-sensitive. Another name, such as `severity` or `SeverityText`, is not a level field. Write `SeverityText AS level` in the SQL to use it.
4. The details. The other fields of the frame, the level field included, are the fields of the details row (9.19h, section 3).
5. A frame with no field of the kind `time` or no field of the kind `text` shows no line. The panel shows a message (9.19h).

A field can be two of these. When the level field is the first field of the kind `text`, it is also the body, as in Grafana.

The level of a line is one of `critical`, `error`, `warning`, `info`, `debug`, `trace` and `unknown`. These steps give it:

1. A text that is not empty, or a number other than 0, in the level field gives the level of the value. The value `null`, the empty text, the number 0, a boolean and `NaN` give the level of the body.
2. The level of a value. The value is a level word of the table below, in any case: it gives the level of the word. Else, a number from 0 to 7 gives the syslog level of the table. A text of digits is such a number when it has no 0 before another digit, for example `3` and not `03`. Any other value gives `unknown`.
3. The level of the body. The page finds the first level word in the body, with a word boundary before and after it, in any case. The word gives the level. A body with no level word gives `unknown`. The page reads the part of the body that it shows (9.19h, section 4).

| Level | Words | Syslog numbers |
|---|---|---|
| `critical` | `emerg`, `fatal`, `alert`, `crit`, `critical` | 0, 1, 2 |
| `error` | `err`, `eror`, `error` | 3 |
| `warning` | `warn`, `warning` | 4 |
| `info` | `info`, `information`, `informational`, `notice` | 5, 6 |
| `debug` | `dbug`, `debug` | 7 |
| `trace` | `trace` | none |
| `unknown` | `unknown` | none |

Examples: the value `Error` gives `error`. The number 4 gives `warning`. The body `WARN: disk` with no level field gives `warning`. The body `all good` gives `unknown`.

A word boundary is the boundary of the regular expressions of the browser (ECMAScript `\b`): it is between a character of `[A-Za-z0-9_]` and another character. The page finds all the words in one pass of the body, so the work is linear in the length of the body. The vectors FE-81 hold the levels that the code of Grafana 12.4.2 gives (16.10a).

A text value of the level field that is the name of a member of a JavaScript object, such as `constructor`, gives `unknown`. Grafana reads such a name as a level by mistake.

### 7.17 Other users of the engine

- The preview (6.22) runs the pipeline of 7.2 on the draft SQL of its body. It has no etag check (no step 3). Section
  11 gives its access questions.
- The options route uses the pipeline of 7.14.
- The annotation route (6.21a) uses the pipeline of 7.37. It is the pipeline of 7.2 with no panel, no interval and no `max_data_points`.
- The importer (15) compares the expansion with the `executedQueryString` of Grafana. Two differences are known and
  explained. Hune does not expand a macro or a placeholder in a comment. Hune escapes a value with `'` or `\`.

### 7.18 Checks at save

Hune runs these checks at save, at restore, at import and at `validate_only` (4.16, item 5). They come after the
validation of 4.36, on the SQL of each panel, each `query` variable and each annotation. Each fault is one item of `invalid_document`, in the form of
4.37. Its pointer names the SQL line, for example `/sections/0/panels/2/data/query/sql/5`. Its `line` and `column`
are `null` (4.37), and its message gives the column in the line.

The checks use the dialect of the source of the panel or the variable. 7.26 gives the changes of the dialect `postgres`.

The checks run on the SQL of each query of a panel: `data.query` and each item of `data.extra_queries`, hidden queries included. The pointer of a fault in an extra query is `/sections/0/panels/2/data/extra_queries/1/sql/5`. Item 9 gives `bad_reference` at the `source` of the query. A query uses the dialect of its own source, so one panel can hold queries of two dialects.

| Error | Code of 4.37 |
|---|---|
| 1. A lexer error (7.5). | `bad_format` |
| 2. A failure of the statement check (7.12) on the stored text. | `bad_format` |
| 3. A `$__name` in code that is not a macro or a built-in variable of 7.10. A macro with a format, such as `${__from:csv}`, is also in this item. | `bad_format` |
| 4. A macro with a wrong number of arguments (0, 1 or 2), or a macro with no argument followed directly by `(`. The second argument of `$__conditionalAll` is also in this item (7.10). | `bad_format` |
| 5. A macro that needs the interval (`$__timeInterval`, `$__timeInterval_ms`, `$__interval_s`, `$__interval_ms`) in the SQL of a variable or of an annotation. | `not_permitted` |
| 6. A placeholder in a heredoc. | `not_permitted` |
| 7. In code, a `${` that does not match the grammar of 7.6, or a format that 7.9 does not list. | `bad_format` |
| 8. A placeholder in a context that the table of 7.8 marks as a document error. This includes a format of the last group of 7.9 in code or in a quoted name. A placeholder of an open `custom` variable in a quoted name is in this item, because 7.8 treats the variable as a `query` variable. The pointer names the SQL line, as for each item of this table. | `not_permitted` |
| 9. A source id that the configuration does not declare (4.3, item 5). | `bad_reference` |
| 10. A candidate of 7.6a that is not the strict form `{name:Type}`. | `bad_format` |
| 11. A query parameter with a type outside the table of 7.6a. | `not_permitted` |
| 12. A query parameter whose name is not a variable of the document. | `bad_reference` |
| 13. A query parameter of a refused variable (7.6a): an `interval` variable, a variable with an `all_value`, a name that starts with `__`, or another kind. | `not_permitted` |
| 14. A query parameter whose type does not agree with the shape of its variable (7.6a). | `not_permitted` |
| 15. In one text: one name with two types, one variable in a query parameter and in a placeholder, or more than 32 names of query parameters. | `not_permitted` |
| 16. A candidate of 7.6a on a `postgres` source (7.26). | `not_permitted` |
| 17. The `source` of an `adhoc` variable is not a `clickhouse` source, or the source has no key `adhoc_columns` (4.10c). An undeclared source is item 9, at the member `source` of the variable. | `not_permitted` |
| 18. A key of an `adhoc` variable that the list `clickhouse "<id>".adhoc_columns` of its source does not hold. The pointer names the key, for example `/variables/3/keys/1`. | `not_permitted` |
| 19. `$__adhocFilters` in the SQL of a variable or of an annotation, on a `postgres` source, or in a query whose source has no `adhoc` variable (7.10a). | `not_permitted` |
| 20. `$__adhocFilters` inside the argument of another macro, or in a place where it is not one term of the top-level `AND` of the `WHERE` or `PREWHERE` clause of the outer statement (7.10a, rule 4). In such a place, a filter could add a row that the rest of the clause refuses. A format or an argument of the macro is item 3 or 4. | `not_permitted` |
| 21. A placeholder of 7.6 that names an `adhoc` variable (4.10c, rule 6). | `not_permitted` |

The items 10 to 16 use the same function as the run time (7.6a), so the save and the run agree. Their pointer names the line of the `{`.

The items 19 and 20 use the rules of the run time (7.10a), so the save and the run agree. A load of a stored document does not run them: a fault gives `422 sql_invalid` when the panel runs.

For an annotation, the pointer names the line of the member `sql` of the annotation, for example `/annotations/0/sql/2`. Item 5 applies to the SQL of an annotation as it applies to the SQL of a variable: `$__timeInterval`, `$__timeInterval_ms`, `$__interval_s` and `$__interval_ms` give `not_permitted`. The text of an annotation gets the checks of the kind of its source: 7.18 for `clickhouse`, 7.26 for `postgres` and 7.36 for `mongodb`. A kind that a later version adds to 8.2 gives `not_permitted` at `/annotations/<i>/source`, and no other check runs on the text of that annotation.

The load of a document (5.8) does not run these checks, because the configuration can change and the format does not
hold them. A fault of a loaded document gives `422 sql_invalid` or `404 not_found` when its panel runs (7.2).

Warnings, in the member `warnings` of 6.13 and 6.15:

1. A `$name` or `${name}` whose name is not a variable of the document.
2. A `[[name]]` whose name is a variable of the document.
3. A word `FINAL` at any depth, or a word `SETTINGS` at depth 0.
4. A macro name, or `$__interval`, `$__range`, `$__range_s` or `$__range_ms`, in a string or a quoted name. The text stays as it is, but Grafana replaces some of these names.
5. A `${name:format}` in a string, with the name of a variable of the document and a format that 7.9 does not list. The text stays as it is (7.6). The format `date` is an example.

6. A placeholder of 7.6 (`$name`, `${name}` or `${name:format}`) whose name is not a variable of the document, in a display text.

The display texts of warning 6 are the title of a section, and the title and the description of a panel. They are also the title of a panel link, and the title of a data link in the defaults or in an override. The message is `<placeholder as written> is not a variable of the document`. The pointer is the member of the text: `/sections/0/panels/2/title`, `/sections/0/panels/2/links/1/title`, `/sections/0/panels/2/fields/defaults/links/0/title` or `/sections/0/panels/2/fields/overrides/3/set/links/0/title`.

A name that starts with `__` gives no warning. A `${` that does not match 7.6 gives no warning. A `[[name]]` gives no warning. The warnings 6 come after the warnings 1 to 5, in document order (9.13).

A warning never stops a save.

The caps of the two lists (4.35). These rules apply to the faults and the warnings of this section and of 7.36:

- The faults of all the texts of a document are one list, in the order of the texts. The list holds at most `MAX_ERRORS` items. When it has `MAX_ERRORS` minus 1 faults and one more fault occurs, the list stops (4.36, step 7). Its last item has the code `too_many_errors`, and the checks make no more faults.
- The warnings of all the texts are one list, with the warnings 6 last. The list holds at most `MAX_WARNINGS` items. When it has `MAX_WARNINGS` minus 1 warnings and one more warning occurs, the checks add one last item and make no more warnings. Its pointer is `""` and its message is `the list stops here; it has 200 warnings at most`.
- In one SQL text, the items that stay are the items of the lowest positions, in the order of their position. In one MongoDB text (7.36), they are the first items of the walk, in the order of their position.
- A message shows at most 200 characters of a placeholder, a name or a format of the text, as an error does (4.37). A longer value is cut and ends with `...`.

The checks of one text are linear in its length. A full list makes no more items, but the checks continue: a fault after a full list of warnings still stops the save.

### 7.19 SQL dialects

A source has a kind, and the kind gives the dialect of its SQL. The engine is one program with one dialect for each kind. The sections 7.5 to 7.18 describe the dialect `clickhouse`, and the sections 7.20 to 7.26 describe the dialect `postgres`. Both dialects share the steps of 7.2, the placeholders of 7.6 and the variable rules of 7.7. They also share the formats of 7.9 and the property P of 7.11. A dialect changes only the items of this table.

| Item | `clickhouse` | `postgres` |
|---|---|---|
| Lexer | 7.5 | 7.20 |
| Line comment | `--` and `#` | `--` only |
| Escape in a string | `\` and `'` get a backslash | `'` is doubled. A backslash is ordinary, except in an `E` string. |
| Quoted name | `` ` `` and `"`, with a backslash escape | `"`, doubled |
| Unicode spaces | Blank | Word bytes |
| Escape rules | 7.8 | 7.21 |
| Macros | 7.10 | 7.22, 7.23 |
| Statement check | 7.12 | 7.25 |
| Checks at save | 7.18 | 7.26 |
| Frame types | 7.16 | 7.16a, 7.16b |
| Client, limits, cancellation | 8.3 to 8.9 | 8a.4 to 8a.7 |
| Error map | 8.5 | 8a.8 |
| Account | 8.12 | 8a.13 |

The engine selects the dialect from the kind of the source of the query. At save, the kind comes from the configuration. The importer takes the kind from the type of the Grafana data source (15.9). An unknown source id has no dialect. The save then reports only item 9 of 7.18 and skips the other checks of that SQL.

The name scan. The format check of 4.34 and the answer of 6.14 find the variables that a query text names before they know a dialect. They use the name scan. A variable is named when the lexer of `clickhouse`, the lexer of `postgres` or the JSON reader finds its placeholder (7.6) in the text. The server is the only program that runs the name scan for a page of the viewer. The member `names` of 6.14 gives the result.

The name scan reads the text with the three readers and keeps the union of the answers. The first two readers read tokens. A `$name` in a string, or a `$tag$` inside a word, thus gives the same names as the run of a query. The JSON reader is for the text of a `mongodb` query (7.28). It reads each string of the text, keys included, and it decodes the escapes of the string before it looks for a placeholder.

The JSON reader accepts the formats of 7.34. A text that has no valid JSON string gives the same names as the two lexers give. The scan of the server does not check the format of a placeholder: it names `${x:unknown}` as `x`. The page scan of earlier versions did not read such a placeholder. This is an extra name, and an extra name is permitted.

The reader of `clickhouse` also reads the name of each `{name:` in code when the byte before the `{` is not `$`. This is the start of a query parameter of 7.6a. Thus a variable of a query parameter is a parent of a `query` variable (4.34), and the member `names` of 6.14 names it. The scan of the editor (`sqlscan.js`) reads the same form, so a preview sends its value.

The dependency scan. The member `names` of 6.14 uses a wider scan than the checks of 4.34. It keeps the union of the name scan and of the loose scan. The loose scan reads each placeholder form of 7.6 in the text with no lexer. Thus it reads a comment, a heredoc and a dollar quote too.

This makes one rule true by construction: the member `names` has each name that any reader of the text finds, whatever lexer the reader has. An extra name costs one run of a panel at most. The checks of 4.34, for example the check of cycles, use the name scan only. A stored document thus never becomes invalid because of the loose scan.

A golden test keeps the rule "never fewer". It holds the names that the page scan of earlier versions found for each document and each vector of the repository, in `tests/golden/names/`. The dependency scan MUST name each of them that can be a variable name (4.3). A name that only the server finds needs a reason in `tests/golden/names/extra.json`.

The union of the readers can find more names than one dialect needs. It never finds fewer. An extra name adds an edge to the graph of 4.34 and one more run of a panel. It changes no query. The check of 7.34 and 7.36 decides if a placeholder is valid for its place, for example a placeholder in a key.

The kind `mongodb` has no SQL dialect. Its query language is in 7.27 to 7.36, and it needs no column in the table above. A JSON escape that a SQL lexer reads in another way cannot hide a name, because the JSON reader decodes the escape.

### 7.20 PostgreSQL lexer

The lexer of the dialect `postgres` follows `scan.l` of PostgreSQL 15 to 18. A position is a byte offset. The session has `client_encoding = UTF8` (8a.4), so a multibyte character never hides a quote mark. The vectors of this lexer are in the repository, in the letter notation of 7.5 with three more letters: `E` and `e` for a closed and an open `E` string, `O` for an operator, and `R` for a parameter.

| Token | Rule |
|---|---|
| Blank | A run of space, TAB, LF, CR, FF and VT. No other byte is a blank. A Unicode space is a word byte. |
| Line comment | Starts with `--`. Ends before the next LF or CR. A `#` starts no comment. |
| Block comment | From `/*` to `*/`. It nests, as in 7.5. |
| String | `'...'`. A doubled `''` is one quote mark of the text. A backslash is an ordinary byte. |
| E string | `E'...'` or `e'...'`, with the letter directly before the quote mark. A backslash escapes the next byte. A doubled `''` is also one quote mark. |
| Prefixed string | `B'...'`, `X'...'`, `N'...'` and `U&'...'`, in any case, with the prefix directly before the quote mark. It follows the rule of a string. The prefix is part of the token. |
| Continued string | A closing quote of a string, an `E` string or a prefixed string, then a gap, then an opening quote, continue the string, as in `scan.l`. The gap holds blanks and line comments, and it has a line end. See the rule below. |
| Quoted name | `"..."`. A doubled `""` is one quote mark. A backslash is an ordinary byte. `U&"..."` has the same rule, and its prefix is part of the token. |
| Dollar quote | `$tag$ ... $tag$`, or `$$ ... $$` with an empty tag. No escape. The tag starts with an ASCII letter, `_` or a byte from 0x80. It goes on with these bytes and digits. It has no `$`. |
| Parameter | `$` and one or more digits. |
| Number | As in 7.5. |
| Word | Starts with an ASCII letter, `_`, a byte from 0x80, or a `$` where no dollar quote and no parameter starts. Continues with these bytes, digits and `$`. |
| Operator | A run of the bytes `+ - * / < > = ~ ! @ # % ^ & \|`, the backtick and `?`. The run stops before `--` and before `/*`. A run of two bytes or more that ends in `+` or `-`, and holds none of `~ ! @ # % ^ & \|`, the backtick and `?`, loses its trailing `+` and `-` bytes, but keeps one byte at least. |
| Punctuation | Any other single byte, for example `, ( ) [ ] . ; :`. |

The rule of a continued string. The gap starts with spaces, TABs, VTs, FFs and line comments, and then it has one line end. After this line end it can hold blanks and line comments that end in a line end. The part after the gap is a token of its own, with the kind and the escape rule of the first part. The blanks and the comments of the gap are tokens. A block comment in the gap, or a gap with no line end, ends the string.

PostgreSQL 16 and later accept a VT before the line end. PostgreSQL 15 does not accept it. The lexer follows the newer rule, so a text with a VT at this place has two readings on PostgreSQL 15. Only an `E` string gives a different token list, because its next part keeps the backslash escape.

A prefix is read only at the start of a token. The text `xE'a'` is the word `xE` and the string `'a'`. A string, a quoted name, a dollar quote or a block comment that the text ends before it closes is a lexer error. At run time it gives `422 sql_invalid`, and at save it gives `422 invalid_document` (7.26).

| Text | Tokens | | Text | Tokens |
|---|---|---|---|---|
| `SELECT 1` | `W_N` | | `'a\'b'` | `Ss` |
| `'a''b'` | `S` | | `E'a\'b'` | `E` |
| `1 # x` | `N_O_W` | | `1 --x` | `N_L` |
| `$1` | `R` | | `$$a'b$$` | `H` |
| `$tag$a$$b$tag$` | `H` | | `a$b` | `W` |
| `U&"a"` | `Q` | | `x@-1` | `WON` |
| `a<-b` | `WOOW` | | `'a' 'b'` | `S_S` |
| `1` U+00A0 `2` | `NW` | | `/* a /* b */ c */` | `B` |

The tokens joined in order give the text again, with no gap. The text `'a\'b'` gives `Ss`: the string `'a\'`, and then the prefixed string `b'` that does not close.

The code tokens of 7.6 are the Blank, Word, Number, Operator, Parameter and Punctuation tokens. The engine finds placeholders in a string, in an `E` string and in a quoted name. A placeholder in a prefixed string, in a `U&` name or in a dollar quote is a document error (7.21).

### 7.21 PostgreSQL escape rules

The table of 7.8 gives the result for each context, selection and format. It holds for the dialect `postgres`, with the escape functions below. The session has `standard_conforming_strings = on` (8a.4), so a backslash in a standard string is an ordinary byte.

| Place of the placeholder | Escape of the value |
|---|---|
| Standard string `'...'` | `'` becomes `''`. A backslash stays. |
| E string `E'...'` | `\` becomes `\\`, and `'` becomes `''`. |
| Quoted name `"..."` | `"` becomes `""`. Only a `custom` variable (7.8). |
| Code, as a string literal | The value in `'...'`, with the escape of a standard string. |
| Prefixed string, `U&"..."`, dollar quote | A placeholder is a document error (7.26). |
| Comment | No change. |

- The formats `singlequote` and `sqlstring` give the same text in code: each value in `'...'` with `'` written `''`. A backslash stays as it is. Grafana writes `\'` for `singlequote`, which breaks a PostgreSQL literal.
- In a string, the formats of the last group of 7.9 build their text first. Then the escape of the kind of the string applies to the whole text.
- A value has no control character (7.7), so no other escape applies, and no value holds a NUL byte.
- The rows of 7.8 that name "the string escapes" use the escape of this table. The rows that name "the name escapes" use the escape of the quoted name.

The format `regex` writes a `\` before each of `\ ^ $ * + ? . ( ) | [ ] { } /`. PostgreSQL reads this text correctly as an advanced regular expression, so the format needs no change.

### 7.22 PostgreSQL macros

The macros of the dialect `postgres` restate the macros of the core data source `grafana-postgresql-datasource` of Grafana 12. `F`, `T`, `S` and `I` are the values of 7.10. `N1` is `from_ms` times 1,000,000, and `N2` is `to_ms` times 1,000,000. `G` is the interval of 7.23 in seconds.

`TS(t)` is the UTC time of the epoch milliseconds `t`, in the form RFC 3339 with the suffix `Z`. Its form is `YYYY-MM-DDTHH:MM:SS`, and then a `.` and the milliseconds when they are not 0. The milliseconds have no zero at their end.

| Macro | Arguments | Expansion |
|---|---|---|
| `$__time(expr)` | 1 | `expr AS "time"` |
| `$__timeEpoch(expr)` | 1 | `extract(epoch from expr) as "time"` |
| `$__timeFilter(expr)` | 1 | `expr BETWEEN 'TS(from_ms)' AND 'TS(to_ms)'` |
| `$__timeFrom` | 0 | `'TS(from_ms)'` |
| `$__timeTo` | 0 | `'TS(to_ms)'` |
| `$__timeGroup(expr, interval)` | 2 | `floor(extract(epoch from expr)/G)*G` |
| `$__timeGroupAlias(expr, interval)` | 2 | `floor(extract(epoch from expr)/G)*G AS "time"` |
| `$__unixEpochFilter(expr)` | 1 | `expr >= F AND expr <= T` |
| `$__unixEpochFrom` | 0 | `F` |
| `$__unixEpochTo` | 0 | `T` |
| `$__unixEpochNanoFilter(expr)` | 1 | `expr >= N1 AND expr <= N2` |
| `$__unixEpochNanoFrom` | 0 | `N1` |
| `$__unixEpochNanoTo` | 0 | `N2` |
| `$__unixEpochGroup(expr, interval)` | 2 | `floor((expr)/G)*G` |
| `$__unixEpochGroupAlias(expr, interval)` | 2 | `floor((expr)/G)*G AS "time"` |
| `$__interval_s`, `$__interval_ms`, `$__from`, `$__to` | 0 | As in 7.10. |
| `$__conditionalAll(condition, $variable)` | 2 | As in 7.10. |

The rules of 7.10 apply to the dialect `postgres`, with these changes:

- A macro expands only in code. A macro name in a string, a quoted name or a comment stays text. The save gives warning 4 (7.18).
- A macro of the table that has 0 arguments can also be written with empty parentheses, with blanks inside: `$__timeFrom()`. Grafana writes it so. The macros `$__interval_s`, `$__interval_ms`, `$__from` and `$__to` take no parentheses.
- The macros `$__timeGroup`, `$__timeGroupAlias`, `$__unixEpochGroup` and `$__unixEpochGroupAlias` take exactly 2 arguments. A third argument, the fill value of Grafana, gives the error "wrong number of arguments" (7.26, item 4). Hune does not fill the gaps of a series.
- A macro name of the dialect `clickhouse` that this table does not list is a document error: "$__name is not a macro of Hune". Examples: `$__timeFilter_ms`, `$__timeInterval`, `$__fromTime`, `$__dateFilter`.
- A variable query (7.14) or an annotation query (7.37) that names `$__interval_s`, `$__interval_ms`, or a group macro with the interval `$__interval`, is a document error. The other macros use the time range of the request.

The texts of the table are the texts of Grafana, so the parity tool of 15.14a compares them as they are. The expansion vectors of 16.13 hold each macro from the pinned Grafana. A difference between a vector and this table changes this table, in the same change as the vector. The known differences from Grafana are these:

- A macro in a string, a quoted name or a comment stays text. Grafana also removes a comment.
- The third argument of a group macro is refused.
- `$__interval` has a place only in 7.23, and it stands for `I` exactly. Grafana rounds it down to one unit.
- The text `G` has no exponent. Grafana writes `2.592e+06`.
- The macros `$__interval_s` and `$__conditionalAll` are not in Grafana.
- A macro with no argument can be written without parentheses.
- An argument can hold parentheses and commas.
- A free value is escaped by 7.21.

Worked example. The stored SQL is:

```sql
SELECT $__timeGroupAlias(created_at, $__interval), host, avg(load) AS load
FROM metrics.node_load
WHERE $__timeFilter(created_at) AND host IN ($host) AND env = '$env'
GROUP BY 1, host
ORDER BY 1
```

The request has the range of 7.3 and `max_data_points = 1200`, so `I = 20,000` and `G = 20`. The `multi` query variable `host` has the values `a1` and `b'2`. The `custom` variable `env` has the value `prod`. The final SQL is:

```sql
SELECT floor(extract(epoch from created_at)/20)*20 AS "time", host, avg(load) AS load
FROM metrics.node_load
WHERE created_at BETWEEN '2026-11-09T08:00:00Z' AND '2026-11-09T14:00:00Z' AND host IN ('a1','b''2') AND env = 'prod'
GROUP BY 1, host
ORDER BY 1
```

### 7.23 The interval argument of a group macro

The second argument of `$__timeGroup`, `$__timeGroupAlias`, `$__unixEpochGroup` and `$__unixEpochGroupAlias` is the interval. The engine expands the placeholders of the argument first (7.10). The result has exactly one token, in one of three forms:

1. A string of the form `'<n><unit>'`. The number `<n>` is a whole number of 1 or more, with no sign and no zero at its start. The unit is `ms`, `s`, `m`, `h`, `d`, `w`, `M` (30 days) or `y` (365 days), as in `gtime.ParseInterval` of Grafana. A text with two units (`1h30m`), a decimal (`1.5s`), or any other form is a document error.
2. The built-in `$__interval`, or `${__interval}`, with no format. It stands for `I` of 7.4.
3. The string `'$__interval'`, with no other text. It also stands for `I`. Grafana replaces the built-in inside the quote marks.

The second argument is one string token, or the text `$__interval` or `${__interval}`. A macro in the second argument is a document error.

The interval MUST be from 1 ms to 365 days. Another value is a document error. The engine does not read `$__interval` as a macro of 7.10. `$__interval` is valid in this place only. In any other place of code, it is a document error.

At save, the engine checks the shape of the stored argument: one string token, or `$__interval`. It checks the grammar of a string only when the string has no placeholder. At run time, it checks the grammar of the expanded text. A fault in a text that a variable gave is `422 variable_value_refused`, with the reason "The value is not an interval of 7.23". Another fault is `422 sql_invalid`.

The engine never copies the argument to the output. It writes `G`, from the numbers that it parsed. Thus a free value cannot change the structure of the statement.

`G` is the interval in seconds, in decimal. A whole number of seconds gives the integer: 300 for `'5m'`. A fraction gives the shortest decimal text with 3 decimals at most, for example `0.5` for `'500ms'` and `0.001` for `'1ms'`. The text never has an exponent.

| Interval | `G` |
|---|---|
| `'5m'` | `300` |
| `'1h'` | `3600` |
| `'1d'` | `86400` |
| `'500ms'` | `0.5` |
| `'1M'` | `2592000` |
| `$__interval`, with `I = 20,000` | `20` |
| `$__interval`, with `I = 200` | `0.2` |
| `'$step'`, with the `custom` variable `step` = `5m` | `300` |
| `'0s'`, `'1.5s'`, `'1h30m'`, `'400d'`, `x` | document error |

### 7.24 Expansion and the property P for PostgreSQL

The expansion of 7.11 runs with the lexer of 7.20 and the escape rules of 7.21. The property P changes in three points:

- String context: the range is inside one closed string token. This token has the kind of the original token: a standard string stays a standard string, and an `E` string stays an `E` string.
- Code context, numbers: the range is exactly a sequence of numbers with one `,` between two numbers. A number is one Number token, or one Operator token whose text is exactly `-`, directly followed by one Number token.
- Code context, string literals: the range is exactly a sequence of standard string tokens with no prefix, with one `,` between two strings.

In the two code cases, a token starts at the start of the range, and a token ends at its end. A range that starts inside a longer token fails. Thus a `-` that joins an operator, for example `@-`, fails. An `E`, `B`, `X`, `N` or `U&` prefix directly before the range also fails, because it joins the first string token.

Take the case of a final text that does not lex. The engine refuses the first free value that ends at or after the start of the token that does not close. The error is `422 variable_value_refused`. If no free value is there, the error is `422 sql_invalid`.

Example: the SQL `WHERE n = E$name` with the free value `\` gives `WHERE n = E'\'`. The lexer reads an `E` string that does not close at that place. P fails and nothing runs. The SQL `WHERE x @$v` with the free value `-5` gives `@-5`. The operator `@-` starts before the range, so P fails.

The seeded random tests of 16.4 include the hostile pieces of this dialect: `E'` strings, dollar quotes, `U&` forms, nested comments, `#`, a backslash before a quote mark, Unicode spaces, and the operator runs of 7.20. The test C03 has its twin PS03 (16.9a).

### 7.25 PostgreSQL statement check

The engine checks the statement at save, on the stored text, and at run time, on the final text. The depth counts `(`, `[` and `{` punctuation tokens. Words compare with ASCII letters folded to upper case. A quoted name is not a word, so `"update"` is a name and not a refused word.

1. The engine skips blanks, comments and `(` tokens at the start. The first word MUST be `SELECT`, `WITH`, `VALUES` or `TABLE`.
2. The text holds one statement. A `;` at depth 0 is permitted only as the last token that is not a blank or a comment. A `;` inside a `(`, `[` or `{` is refused. The server also refuses a second statement, because the client sends the text in a Parse message only (8a.5).
3. These words are refused at any depth: `INTO`, `INSERT`, `UPDATE`, `DELETE` and `MERGE`. They close `SELECT INTO`, a CTE that changes data, and the clause `FOR UPDATE` that locks rows.
4. A word `FOR` followed by `UPDATE`, `SHARE`, `NO` or `KEY` is refused at any depth. This closes `FOR SHARE`, `FOR NO KEY UPDATE` and `FOR KEY SHARE`.
5. A Parameter token (`$1`) is refused. Hune sends no parameter.
6. A prefix `U&` before a quote mark or a double quote is refused. The escape of this form can hide a name.
7. A call of a refused function is refused. A call is a word, or a quoted name, followed by `(` after blanks and comments. A quoted name matches a refused function in all letter cases. The list is below.
8. A token that does not close is refused.

The list of refused functions is closed. It is a defence in depth: the role of 8a.13 is the boundary. A refused name stops the query before it runs, and the message names the function.

| Hazard | Refused names |
|---|---|
| Time | `pg_sleep`, `pg_sleep_for`, `pg_sleep_until` |
| Session state | `set_config` |
| Signals to other sessions | `pg_cancel_backend`, `pg_terminate_backend` |
| Notifications | `pg_notify` |
| Advisory locks | each name that starts with `pg_advisory_` or `pg_try_advisory_` |
| Files of the server | `pg_read_file`, `pg_read_binary_file`, `pg_ls_dir`, `pg_stat_file`, `lo_import`, `lo_export` |
| Other servers | each name that starts with `dblink` |
| SQL text from a string | `query_to_xml`, `query_to_xml_and_xmlschema`, `query_to_xmlschema`, `cursor_to_xml`, `cursor_to_xmlschema`, `ts_stat`, `ts_rewrite`, `connectby`, `xpath_table`, and each name that starts with `crosstab` |
| Server administration | `pg_reload_conf`, `pg_rotate_logfile`, `pg_switch_wal`, `pg_promote`, `pg_create_restore_point`, `pg_logical_emit_message` |

The functions of the row "SQL text from a string" run the text of a string argument as SQL, with the rights of the role. The check cannot read this text. `ts_stat` and `ts_rewrite` are in each PostgreSQL install, and `PUBLIC` can run them by default. They can call `pg_sleep`, `pg_terminate_backend` and `pg_logical_emit_message` without one of these names in the outer text. The functions `crosstab*`, `connectby` and `xpath_table` come from the contrib modules `tablefunc` and `xml2`. The role of 8a.13 MUST have no `EXECUTE` right on all of them (check R7).

A failure gives `422 sql_invalid` at run time and `422 invalid_document` at save. The check has limits that no client can remove. A function can run without its name in the text. Three ways exist: a cast (`'pg_sleep'::regproc`), a function that an administrator wrote, and a name that the list does not hold. Heavy SQL, such as a large join or a recursive query, uses CPU without a refused name. The limits of 8a.6 and 8a.7 and the role of 8a.13 close these cases.

| SQL | Result |
|---|---|
| `SELECT 1` | Accepted. |
| `(SELECT 1) UNION ALL (SELECT 2)` | Accepted. |
| `WITH x AS (SELECT 1) SELECT * FROM x;` | Accepted. |
| `VALUES (1), (2)` | Accepted. |
| `TABLE metrics.node_load` | Accepted. |
| `SELECT 1 AS "update"` | Accepted: a quoted name. |
| `SELECT 1; SELECT 2` | Refused: two statements. |
| `SHOW server_version` | Refused: the first word. |
| `EXPLAIN SELECT 1` | Refused: the first word. |
| `DO $$ BEGIN END $$` | Refused: the first word. |
| `CALL p()` | Refused: the first word. |
| `SET statement_timeout = 0` | Refused: the first word. |
| `LISTEN ch`, `NOTIFY ch`, `LOCK TABLE t`, `PREPARE p AS SELECT 1` | Refused: the first word. |
| `COPY t TO PROGRAM 'id'` | Refused: the first word. |
| `SELECT * INTO t2 FROM t` | Refused: `INTO`. |
| `WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d` | Refused: `DELETE`. |
| `SELECT * FROM t FOR UPDATE` | Refused: `UPDATE`. |
| `SELECT * FROM t FOR SHARE` | Refused: `FOR SHARE`. |
| `SELECT pg_sleep(10)` | Refused: `pg_sleep`. |
| `SELECT * FROM ts_stat('SELECT pg_sleep(60)::text::tsvector')` | Refused: `ts_stat`. |
| `SELECT pg_catalog."pg_sleep" (10)` | Refused: `pg_sleep`. |
| `SELECT set_config('statement_timeout', '0', false)` | Refused: `set_config`. |
| `SELECT $1` | Refused: a parameter. |
| `SELECT U&"d\0061t\0061"` | Refused: `U&`. |
| `-- only a comment` | Refused: no first word. |

### 7.26 Checks at save for PostgreSQL

The checks of 7.18 run with the dialect of the source of the panel, of the variable or of the annotation. The table gives the items with a change. The other items, and the warnings 1, 2, 4, 5 and 6, are the same.

| Item of 7.18 | Change for `postgres` |
|---|---|
| 1 | A lexer error of 7.20. |
| 2 | A failure of the statement check of 7.25 on the stored text. This includes a refused word, a refused function, a parameter and a `U&` form. |
| 3 | A `$__name` that the table of 7.22 does not list. The text "$__name is not a macro of Hune" is the message. |
| 4 | A macro with a wrong number of arguments. A group macro with a third argument is in this item. An interval argument with a wrong shape or a wrong string (7.23) is in this item. |
| 5 | In the SQL of a variable or of an annotation: `$__interval_s`, `$__interval_ms`, or a group macro with the interval `$__interval`. |
| 6 | A placeholder in a dollar quote. |
| 8 | Also a placeholder in a prefixed string (`B`, `X`, `N`, `U&`) or in a `U&"..."` name. |
| 10 to 15 | Do not apply. Item 16 applies: each candidate of 7.6a gives `not_permitted`, because query parameters are a form of ClickHouse. At run time, a candidate gives `422 sql_invalid`. |
| 19 | `$__adhocFilters` gives `not_permitted`, because the ad hoc filters are a form of ClickHouse (7.10a). At run time, it gives `422 sql_invalid`. |

The items 6 and 8 for these places apply only when the name is a variable of the document. A placeholder with another name gives warning 1 of 7.18.

Warning 3 of 7.18 (`FINAL`, `SETTINGS`) does not apply to the dialect `postgres`. This dialect has no other warning of its own.

### 7.27 The MongoDB query language: scope and status

The kind `mongodb` is the third kind of data source. It follows the steps of 8.14. This section and 7.28 to 7.36 give its query language. Section 8b gives its client, its limits, its probe and its account. Section 13 gives its keys (13.12b).

- The query is data, not code. It is one JSON object with a closed grammar. It has no JavaScript, no shell syntax and no text that the server reads as code.
- The document format does not change (4.15). The member `sql` of a panel query or of a variable holds the lines of the JSON text. There is no new `format_version` and no new optional member.
- The engine selects this language from the kind of the source (7.19). The steps of 7.2 apply, with the changes of 7.35.
- The grammar is an allowlist. A stage, an operator or a form that 7.31 does not list is refused. 7.32 lists the refused names that matter most, so that each message is exact.
- The checks of this language are a defence in depth. The boundary is the user of 8b.13 and the read-only connection of 8b.5.
- This section is the design for the milestone MMG (18.10b). The security choices of this section are open questions (19.33). Each open question has the default that this section states.
- The macros and the formats of 7.34 restate the Grafana plugin `haohanyang-mongodb-datasource` 0.5.0. They are NOT checked against a real plugin yet. The vectors of 16.13 decide.

### 7.28 The query text and its JSON profile

The query text is the SQL lines of the panel, joined with one LF between two lines (7.2, step 8). It is one JSON text with one object. The parser is in `hune-core`. It needs no MongoDB crate. It reads the text with the profile J of the table. A value of the tree is one of: null, boolean, Int32, Int64, Double, Decimal128, string, Date, ObjectId, array, object.

| Rule | Text |
|---|---|
| J1 | The text is UTF-8 with no byte order mark. It follows RFC 8259. The blank bytes are space, TAB, LF and CR. A comment, a trailing comma and a value after the first one are errors. |
| J2 | A duplicate member name in one object is an error. |
| J3 | A string holds no U+0000. A key holds no U+0000. |
| J4 | An integer literal with no fraction and no exponent is an Int32 when it is in the range of Int32, else an Int64 when it is in the range of Int64. A larger literal is an error. `-0` is the Int32 0. |
| J5 | A literal with a fraction or an exponent is a Double. It MUST be finite. |
| J6 | A typed object (the table below) is one value. Another object whose member names start with `$` is a stage or an operator object (7.30). |

The typed objects are a closed subset of Extended JSON v2. They are the only way to write a Date, an ObjectId, an Int64 above 2^31 and a Decimal128 in the text. The four names `$date`, `$oid`, `$numberLong` and `$numberDecimal` are never an operator.

| Form | Type | Rule |
|---|---|---|
| `{"$date": "<text>"}` | Date | `YYYY-MM-DDTHH:MM:SS` with 0 to 3 decimals and the suffix `Z`. A real UTC date from 0001-01-01 to 9999-12-31. |
| `{"$date": {"$numberLong": "<digits>"}}` | Date | Epoch milliseconds. The text matches `^-?[0-9]{1,16}$`. |
| `{"$oid": "<hex>"}` | ObjectId | 24 hexadecimal digits. The value keeps no case: it is stored as 12 bytes. |
| `{"$numberLong": "<digits>"}` | Int64 | The text matches `^-?[0-9]{1,19}$` and fits in an Int64. |
| `{"$numberDecimal": "<text>"}` | Decimal128 | The text matches `^-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?$` and has at most 34 digits. No `NaN` and no `Infinity`. |

Another form of Extended JSON is refused: `$binary`, `$uuid`, `$regularExpression`, `$timestamp`, `$code`, `$codeWScope`, `$symbol`, `$minKey`, `$maxKey`, `$undefined`, `$dbPointer`, `$numberInt` and `$numberDouble`. A regular expression is written with the operator `$regex` (7.31).

The limits of the parser are constants in `crates/hune-core/src/mongo/limits.rs`. They are not keys of the configuration, so that a text is valid on every instance. A breach gives `422 sql_invalid` at run time and `422 invalid_document` at save, with the code `too_deep` or `too_large` of 4.37.

| Constant | Value | Applies to |
|---|---|---|
| `MAX_QUERY_DEPTH` | 32 | The depth of objects and arrays. The top object has depth 1. |
| `MAX_QUERY_NODES` | 100,000 | The values of the tree. |
| `MAX_QUERY_STAGES` | 64 | The stages at all depths. A stage of a nested pipeline counts. |
| `MAX_PIPELINE_DEPTH` | 3 | The levels of nested pipelines (`$facet`, `$lookup`, `$unionWith`). |
| `MAX_QUERY_KEY_BYTES` | 256 | One key. |
| `MAX_QUERY_STRING_BYTES` | 16,384 | One string value. |
| `MAX_QUERY_ARRAY_ITEMS` | 10,000 | One array. |
| `MAX_REGEX_BYTES` | 1,024 | The pattern of one `$regex`. |

The stored text has at most `MAX_SQL_BYTES` (4.35). Each error of the parser or of the grammar has a pointer in the form of 4.37. It names the SQL line that holds the fault, for example `/sections/0/panels/2/data/query/sql/5`. Its `line` and `column` are `null`. The message gives the JSON pointer inside the query text, for example `/aggregate/2/$out`, and the column in the line.

### 7.29 The top object and the commands

The text is one object. It has exactly these members. An unknown member is an error (item L3 of 7.36).

| Member | Type | Rule |
|---|---|---|
| `collection` | text | Necessary. A collection of the key `collections` of the source (7.33). |
| `database` | text | Optional. A database of the key `databases` of the source. When it is absent, the query uses the first name of `databases`. |
| `aggregate` | array | A pipeline: 1 to 64 stages (7.30, 7.31). |
| `find` | object | The members `filter`, `projection`, `sort`, `skip` and `limit`, all optional (below). |

The object MUST have exactly one of `aggregate` and `find`.

| Member of `find` | Rule |
|---|---|
| `filter` | A filter object of the context Q (7.30). The default is the empty object. |
| `projection` | An object. Each value is `0`, `1`, `true` or `false`. A mix of inclusion and exclusion is an error, except for `_id`. |
| `sort` | An object with 1 to 32 keys. Each value is `1` or `-1`. |
| `skip` | An integer from 0 to 1,000,000,000. |
| `limit` | An integer from 1 to 1,000,000,000. |

The members `skip` and `limit` of `find` are integer literals. They hold no placeholder and no macro. A query on `find` has the same limits and the same frame as a query on `aggregate`. A count is a `$count` stage of `aggregate`. Hune sends no `count` command, no `distinct` command and no `mapReduce` command.

The command that Hune sends is made from this object and from the values of Hune. An author cannot set any of them. Section 8b.5 gives the whole command.

| Option of the command | Value |
|---|---|
| `allowDiskUse` | `false`. A stage that needs more memory than the server permits fails (8b.8). |
| `maxTimeMS` | The effective `max_execution_time` in milliseconds (8b.7). |
| `cursor.batchSize` | `min(1000, R + 1)`. `R` is the effective `max_result_rows` (8b.6). |
| `readConcern` | `{"level": "local"}`. |
| `comment` | The attribution document of 8b.11. |
| The last stage of `aggregate`, or `limit` of `find` | A `$limit` of `R + 1` rows (8b.6). For `find`, it is `min(limit, R + 1)`. |
| `hint`, `collation`, `let`, `explain`, `writeConcern`, `bypassDocumentValidation` | Never sent. |

Example. A time series of the mean load of each host, for the range of 7.3:

```json
{
  "collection": "node_load",
  "aggregate": [
    { "$match": { "ts": { "$gte": "$__timeFrom", "$lte": "$__timeTo" }, "host": { "$in": "${host:json}" } } },
    { "$group": {
        "_id": { "t": { "$dateTrunc": { "date": "$ts", "unit": "second", "binSize": "$__interval_s" } }, "host": "$host" },
        "load": { "$avg": "$load" } } },
    { "$project": { "_id": 0, "time": "$_id.t", "host": "$_id.host", "load": 1 } },
    { "$sort": { "time": 1 } }
  ]
}
```

### 7.30 Grammar of the tree

The grammar has four contexts. Q is a filter. E is an expression. A is an accumulator. W is a window operator. The rules apply to the tree of the text before the expansion and to the tree after it (7.35).

1. G1. A key that starts with `$` is a stage name or an operator name. It MUST be in the table of 7.31 for its context. Else it is refused (7.32).
2. G2. A key that does not start with `$` is a field name. It has 1 to 256 bytes. It holds no `$`, no U+0000 and no control character. A `.` separates the steps of a path. No step is empty and no step starts with `$`.
3. G3. In the context E, an object with exactly one key that starts with `$` is an operator expression. An object with no key that starts with `$` is a literal object, and each value is an expression. An object with both kinds of key is an error.
4. G4. In the context Q, the keys of an object are field names, `$and`, `$or`, `$nor` and `$expr`. The value of a field name is a literal, or an object whose keys all start with `$` and are operators of Q. A literal object has no key that starts with `$` at any depth.
5. G5. `$and`, `$or` and `$nor` take an array of 1 to 32 objects of Q. `$expr` takes an expression of E.
6. G6. A stage is an object with exactly one key. Each stage has the argument of its row in 7.31. A member that the row does not name is an error.
7. G7. A string value that starts with `$$` is a variable reference. The names `ROOT`, `CURRENT` and `REMOVE` are permitted. A name that starts with a lower-case letter is permitted: it names a variable that `$let`, `$map`, `$filter`, `$reduce` or `$lookup` defines. Another name is refused, for example `$$NOW` and `$$CLUSTER_TIME`.
8. G8. A string value that starts with `$` and not `$$` is a field path of the document. It follows G2 for each step. A string that starts with `$__` is a macro (7.34), or it is an error.
9. G9. A nested pipeline (`$facet`, `$lookup`, `$unionWith`) has the same stages as the top pipeline. The stage `$facet` is not permitted in a nested pipeline. A `$lookup` is not permitted at the third level of nested pipelines.
10. G10. The arity and the types of the arguments of an operator are the checks of the server. Hune checks the names, the contexts, the limits of 7.28 and the shapes of 7.31 only. A fault that only the server sees gives `422 query_error` (8b.8).

An operator that exists in a newer server but not in the oldest supported release (8b.14) is in the table with its first release. A server that does not know it refuses the query with the code 168 or 40324, and Hune maps it as `query_error`.

### 7.31 Allowed stages and operators

The names below are the whole allowlist. Each other name is refused.

Stages of `aggregate`:

| Stage | Argument |
|---|---|
| `$match` | A filter object (Q). |
| `$project` | An object. Each value is `0`, `1`, `true`, `false` or an expression (E). |
| `$addFields`, `$set` | An object. Each value is an expression (E). |
| `$unset` | A field name, or an array of 1 to 64 field names. |
| `$group` | An object with the member `_id` (an expression) and 0 to 64 other members. Each other member is an accumulator (A): an object with one operator. |
| `$sort` | An object with 1 to 32 keys. Each value is `1` or `-1`. |
| `$limit` | An integer from 1 to 1,000,000,000. |
| `$skip` | An integer from 0 to 1,000,000,000. |
| `$count` | A field name with no `.` and no `$`. |
| `$unwind` | A field path, or an object with the members `path`, `includeArrayIndex` and `preserveNullAndEmptyArrays`. |
| `$replaceRoot` | An object with the member `newRoot` (E). |
| `$replaceWith` | An expression (E). |
| `$sortByCount` | An expression (E). |
| `$bucket` | An object with `groupBy` (E), `boundaries` (2 to 1000 literals), `default` (a literal) and `output` (A members). |
| `$bucketAuto` | An object with `groupBy` (E), `buckets` (an integer from 1 to 1000) and `output` (A members). |
| `$facet` | An object with 1 to 16 members. Each value is a pipeline. Not permitted in a nested pipeline. |
| `$setWindowFields` | An object with `partitionBy` (E), `sortBy` and `output`. Each `output` member is an object with one operator of W and an optional `window`. |
| `$densify` | An object with `field`, `partitionByFields` and `range`. |
| `$fill` | An object with `partitionBy`, `partitionByFields`, `sortBy` and `output`. |
| `$lookup` | Form 1: `from`, `localField`, `foreignField`, `as`. Form 2: `from`, `let`, `pipeline`, `as`. The value `from` is a text of the key `collections` (7.33). The member `from` is never an object. |
| `$unionWith` | A text of the key `collections`, or an object with `coll` (that text) and `pipeline`. |

Operators of the context Q (the filter):

| Family | Names |
|---|---|
| Comparison | `$eq`, `$ne`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin` |
| Logic | `$and`, `$or`, `$nor`, `$not`, `$expr` |
| Element and array | `$exists`, `$type`, `$size`, `$all`, `$elemMatch`, `$mod` |
| Text | `$regex` with the optional `$options` (the letters `i`, `m`, `s` and `x` only) |

Operators of the context E (the expression):

| Family | Names |
|---|---|
| Comparison and logic | `$eq`, `$ne`, `$gt`, `$gte`, `$lt`, `$lte`, `$cmp`, `$and`, `$or`, `$not`, `$in`, `$cond`, `$ifNull`, `$switch` |
| Arithmetic | `$add`, `$subtract`, `$multiply`, `$divide`, `$mod`, `$abs`, `$ceil`, `$floor`, `$round`, `$trunc`, `$sqrt`, `$pow`, `$exp`, `$ln`, `$log`, `$log10` |
| String | `$concat`, `$substrBytes`, `$substrCP`, `$toLower`, `$toUpper`, `$trim`, `$ltrim`, `$rtrim`, `$split`, `$strLenBytes`, `$strLenCP`, `$strcasecmp`, `$indexOfBytes`, `$indexOfCP`, `$replaceOne`, `$replaceAll`, `$regexMatch`, `$regexFind`, `$regexFindAll` |
| Array | `$arrayElemAt`, `$first`, `$last`, `$size`, `$slice`, `$concatArrays`, `$filter`, `$map`, `$reduce`, `$isArray`, `$reverseArray`, `$range`, `$zip`, `$arrayToObject`, `$objectToArray`, `$sortArray`, `$firstN`, `$lastN`, `$maxN`, `$minN`, `$indexOfArray` |
| Set | `$setUnion`, `$setIntersection`, `$setDifference`, `$setEquals`, `$setIsSubset`, `$anyElementTrue`, `$allElementsTrue` |
| Date | `$dateTrunc`, `$dateAdd`, `$dateSubtract`, `$dateDiff`, `$dateFromParts`, `$dateFromString`, `$dateToString`, `$dateToParts`, `$year`, `$month`, `$dayOfMonth`, `$dayOfWeek`, `$dayOfYear`, `$hour`, `$minute`, `$second`, `$millisecond`, `$week`, `$isoWeek`, `$isoWeekYear`, `$isoDayOfWeek`, `$toDate` |
| Type | `$convert`, `$toInt`, `$toLong`, `$toDouble`, `$toDecimal`, `$toString`, `$toBool`, `$toObjectId`, `$isNumber`, `$type` |
| Object and variable | `$mergeObjects`, `$getField`, `$setField`, `$unsetField`, `$literal`, `$let` |

Operators of the context A (the accumulator): `$sum`, `$avg`, `$min`, `$max`, `$count`, `$first`, `$last`, `$push`, `$addToSet`, `$stdDevPop`, `$stdDevSamp`, `$top`, `$bottom`, `$topN`, `$bottomN`, `$firstN`, `$lastN`, `$maxN`, `$minN`, `$median`, `$percentile`, `$mergeObjects`.

Operators of the context W: the names of A, and `$rank`, `$denseRank`, `$documentNumber`, `$shift`, `$derivative`, `$integral`, `$expMovingAvg`, `$covariancePop`, `$covarianceSamp`, `$locf`, `$linearFill`.

The operators `$median`, `$percentile`, `$sortArray`, `$top`, `$bottom` and the others of the table exist from the release 7.0 (8b.14).

### 7.32 Refused stages, operators and forms

Each name below gives the message `<name> is not permitted in a MongoDB query of Hune`, and the reason. A name that is in no table is refused with the same message.

| Names | Reason |
|---|---|
| `$out`, `$merge` | They write to a collection. |
| `$function`, `$accumulator`, `$where` | They run JavaScript on the server. `$expr` is permitted only with the operators of 7.31, so `$expr` with `$function` fails at the name. |
| `mapReduce`, `$code`, `$codeWScope` | Server-side JavaScript in any other form. |
| `$currentOp`, `$listSessions`, `$listLocalSessions`, `$listSampledQueries`, `$queryStats`, `$planCacheStats` | They show the work of other users, or of the server. |
| `$collStats`, `$indexStats`, `$listCatalog`, `$listSearchIndexes` | They show the size, the use and the structure of the server. |
| `$changeStream`, `$changeStreamSplitLargeEvent` | They open a stream that does not end. |
| `$graphLookup` | It is a recursive join. |
| `$lookup` or `$unionWith` with a collection that the key `collections` does not list, with `from` as an object (another database), or with no `from` | The query leaves the allowlist (7.33). |
| `$search`, `$searchMeta`, `$vectorSearch`, `$documents`, `$geoNear`, `$redact`, `$text`, `$jsonSchema` | Not needed, or they need a server feature that the role cannot check. |
| `$sample`, `$sampleRate`, `$rand` | The result is not the same at each run. |
| `$$NOW`, `$$CLUSTER_TIME`, `$$USER_ROLES`, `$$SEARCH_META` | The time is a macro of 7.34. The others show the server or the role. |
| `$meta`, `$comment`, `$natural`, each name that starts with `$_` | Internal or not needed. |
| Each member of the top object that 7.29 does not list, for example `allowDiskUse`, `hint`, `collation`, `let`, `readConcern` and `maxTimeMS` | An author cannot change the command of Hune. |
| Each Extended JSON form that 7.28 does not list | The value type is not needed. |

### 7.33 Collections and databases

- The key `databases` of the block (13.12b) is the list of the databases that a query can read. The key `collections` is the list of the collections that a query can name. A name of `collections` is valid in each database of `databases`.
- Each of these names MUST be on the list: the member `collection`, the member `from` of `$lookup`, and the name of `$unionWith`. The comparison is exact and it is case-sensitive.
- A `$lookup` and a `$unionWith` read the database of the query. They never read another database.
- The checks run at save, with the configuration of that moment, and again at run time. A name that is not on the list gives `422 invalid_document` with the code `bad_reference` at save, and `422 sql_invalid` at run time.
- The lists are a defence in depth. The role of 8b.13 has the `read` role on the databases, so it can read each collection of them. The list is stricter than the role.
- No wildcard exists in the lists. A new collection needs a change of the configuration.

### 7.34 Variables and macros for MongoDB

The variables of the document (4.7 to 4.11) and the values of a request (7.7) keep their rules. The engine puts a value in the parsed tree as a typed value. It never writes a value in the text of the query. Thus a value can become a leaf of the tree, and nothing else. It can never become a key, a stage name, an operator name, a field path or a collection name.

The macros are built-in names. A string value that is exactly one macro, as `$__timeFrom` or `${__timeFrom}`, becomes the typed value of the table. The time model of 7.3 and the interval of 7.4 give `from_ms`, `to_ms`, `I` and `S`.

| Macro | Typed value |
|---|---|
| `$__timeFrom` | The Date of `from_ms`. |
| `$__timeTo` | The Date of `to_ms`. |
| `$__from` | The Int64 `from_ms`. |
| `$__to` | The Int64 `to_ms`. |
| `$__interval_ms` | The Int64 `I`. |
| `$__interval_s` | The Int64 `S`. |

A macro name inside a longer string stays text. Another name that starts with `$__` is an error. A macro in a key is an error (G2). A variable query (7.14) or an annotation query (7.37) that names `$__interval_ms` or `$__interval_s` is an error, as in 7.10.

Each macro has a type, and a place accepts it only when the type fits:

- `$__timeFrom` and `$__timeTo` are Date values. A place that needs an integer, a boolean, a sort direction or a projection flag refuses them.
- `$__from` and `$__to` are Int64 values. A place that needs an integer refuses them.
- Of the places that need an integer, only `$limit`, `$skip` and `$size` accept `$__interval_ms` and `$__interval_s`.
- A place that needs a text accepts no macro. A pattern of `$regex` is such a place.

A placeholder is a form of 7.6 in a string value. Its name MUST be a variable of the document, else the text stays. A string such as `"$host"` is thus a placeholder when `host` is a variable of the document, and a field path when it is not. A placeholder in a key is an error, because a key holds no `$` (G2).

A placeholder is also an error in a place that names an object of the server. These name places are `collection`, `database`, `from`, `as`, `localField`, `foreignField` and `coll`. They are also these places: the path and `includeArrayIndex` of `$unwind`, the name of `$count`, the names of `$unset` and `newRoot`. The `field` and `partitionByFields` of `$densify` are name places too. So are the `partitionByFields` of `$fill`, the `as` of `$map` and `$filter`, and the `field` of `$getField`, `$setField` and `$unsetField`. A plain string that is the whole argument of `$getField`, `$setField` or `$unsetField` is also a name place.

A string value has one of three shapes:

1. The string is exactly one placeholder (`$name`, `${name}` or `${name:format}`). It is a whole-leaf placeholder. The format of the table gives the typed value. This string takes only the formats of the first table below. The format `csv` needs a longer string.
2. The string holds a placeholder in a longer text. It is an embedded placeholder. The result is a string. The formats of the second table apply.
3. The string holds no placeholder. It is a literal. A literal that starts with `$` is a field path or a variable reference (G7, G8).

A stored string that starts with `$` and that is not a whole-leaf placeholder MUST NOT hold a placeholder. This closes a field path that a value completes, such as `"$a_${x}"`. To write the field path of a field that has the name of a variable, write `"$$CURRENT.name"`.

Formats of a whole-leaf placeholder:

| Format | Selection | Typed value | Value rule |
|---|---|---|---|
| none or `text` | One value or `all_value` | A string. | Any value of 7.7. |
| `number` | One value or `all_value` | An Int32, an Int64 or a Double. | The number grammar of 7.7. An integer in the range of Int64 gives an integer. Else a Double. |
| `date` | One value or `all_value` | A Date. | `YYYY-MM-DDTHH:MM:SS[.fff]Z`, or 1 to 16 digits that give epoch milliseconds. |
| `oid` | One value or `all_value` | An ObjectId. | 24 hexadecimal digits. |
| `bool` | One value or `all_value` | A boolean. | The text `true` or `false`. |
| `json` | A list, or one value | An array of strings. | Each value of 7.7. |
| `json_number` | A list, or one value | An array of numbers. | Each value follows the format `number`. |
| `json_oid` | A list, or one value | An array of ObjectIds. | Each value follows the format `oid`. |

A list with no format in a whole-leaf placeholder is an error. A value that breaks its rule gives `422 variable_value_refused` with the reason. A list with 0 values gives the empty array. An array that comes from a placeholder is one element of the array that holds it: the format `json` in `"$in": "${host:json}"` gives `"$in": ["a","b"]`.

Formats of an embedded placeholder:

- One value takes no format, `raw` or `csv`. A list takes `csv`.
- Both take `json`, `pipe`, `regex`, `glob`, `lucene`, `percentencode`, `queryparam`, `distributed`, `doublequote`, `html` and `text`, as 7.9 gives them.
- The engine builds the text of the format, and it inserts the text in the string. No other escape applies, because the tree holds the string as a value.
- A list with no format, or with `raw`, is an error. The formats `singlequote` and `sqlstring` are errors.
- The format `regex` gives a text that a regular expression of the server reads as a literal.

The author of a regular expression writes `{"$regex": "^${host:regex}$", "$options": "i"}`. A pattern above `MAX_REGEX_BYTES` after the expansion gives `422 variable_value_refused`.

Free text follows the property PM of 7.35. Author text has the trust of the query text, as in 7.7. It is an option of a `custom` variable, or an `all_value`.

The macro `$__conditionalAll` has no equivalent in this language. The importer marks a query that uses it `unsupported` (15.6).

### 7.35 The expansion and the property PM

For a source of the kind `mongodb`, the steps 8 to 12 of 7.2 are these steps:

1. The server joins the lines of the panel (the text of 7.28).
2. It parses the text with the profile J. A fault gives `422 sql_invalid`.
3. It checks the grammar of 7.29 to 7.33 on the tree T0 (the stored tree). Each placeholder is a value of an unknown type, and the check of its place is the check of a value. A fault gives `422 sql_invalid`.
4. It expands the macros and the placeholders of T0 (7.34) into the tree T1. It never reads an inserted value again. A value that holds `$x` or `$__timeFrom` stays text.
5. It checks the property PM for each inserted free value.
6. It checks the grammar again on T1, with the types of the values now known. A fault gives `422 sql_invalid`, except a fault at an inserted leaf (7.34). This check is a guard: T1 must pass when T0 passed and PM holds.
7. It builds the command (7.29). It checks that the BSON size of the command is at most `max_query_size` of the source. A larger command gives `422 sql_too_large`. The text of the inspect member (6.20) is the compact Extended JSON text of T1.

The property PM: each free value stays inside its leaf.

- PM1. T1 and T0 have the same keys, the same stage names, the same operator names and the same structure of objects. The only change is a leaf that becomes a typed value. The array of a format `json`, `json_number` or `json_oid` replaces one leaf and can have another length.
- PM2. A string leaf that holds a free value MUST NOT start with `$` in T1. This covers a field path (`$secret`) and a variable (`$$ROOT`). A leaf that holds author text only has no such check.
- PM3. A free value is never in a name place of 7.34. The save refuses the place, and the run refuses it again.
- PM4. A free value of a format that gives a number, a date, an ObjectId or a boolean follows the grammar of its format. Else the engine refuses it.

A failure of PM gives `422 variable_value_refused` with the reason "The value changes the structure of the query.". A fault of step 6 of 7.35 at a leaf that holds an inserted value is a fault of the value. It gives the same code with the reason "The value does not fit its place: <message>.". In this reason, `<message>` is the message of the grammar fault.

Example. The leaf `"$host"` with the free value `$secret` for the `text` variable `host` gives the reason above. The leaf `"^${host:regex}"` with the value `a.b` gives `"^a\\.b"` as the string `^a\.b`. The leaf `"${t:number}"` with the value `12abc` gives `422 variable_value_refused`.

The seeded random tests of 16.4 include the generator of 16.4 for this language. The property R10 states PM. The case MS03 (16.9b) runs the expanded queries on a real server.

### 7.36 Checks at save for MongoDB

The checks of 7.18 run for a source of the kind `mongodb` with the items of this table. They come after the validation of 4.36. Each fault is one item of `invalid_document`, with the pointer of the SQL line (7.28). Item 9 of 7.18 stays as it is. The other items of 7.18 do not apply.

| Item | Fault | Code of 4.37 |
|---|---|---|
| L1 | A fault of the profile J (7.28), or a limit of 7.28. | `bad_format`, `too_deep` or `too_large` |
| L2 | A top object that breaks 7.29, except an unknown member (L3). | `bad_format` |
| L3 | A stage, an operator or a form that is in no table of 7.31, or that is in 7.32. An unknown member of the top object, or of the object `find`. | `not_permitted` |
| L4 | A shape that breaks 7.30 (G2 to G9) or the row of 7.31. | `bad_format` |
| L5 | A `$__name` that is not a macro of 7.34. | `bad_format` |
| L6 | A placeholder in a key, in a name place, or in a stored string that starts with `$` (7.34). | `not_permitted` |
| L7 | A format that 7.34 does not list for the place of the placeholder, or a list with no format in a whole-leaf placeholder. | `bad_format` |
| L8 | A collection or a database that the source does not list (7.33). | `bad_reference` |
| L9 | In the query of a variable or of an annotation: `$__interval_ms` or `$__interval_s`. | `not_permitted` |

Warnings, in the member `warnings` of 6.13 and 6.15:

1. A `${name}` or a `${name:format}` whose name is not a variable of the document. A `$name` gives no warning, because it is a field path in most cases.
2. A `[[name]]` whose name is a variable of the document.
3. A macro name in a longer string. The text stays as it is.
4. A placeholder of 7.6 in a display text, as in warning 6 of 7.18.

A warning never stops a save. The checks use the kind of the source of the panel. An unknown source id gives only item 9 of 7.18.

### 7.37 Annotation queries

The route of 6.21a runs the SQL of an annotation (4.5b). The pipeline is the pipeline of 7.2 with these changes.

- There is no panel and no `max_data_points`. The macros that need only the time range use the range of the request (7.14). Placeholders work as in the SQL of a panel (7.6 to 7.11), and the property P applies.
- Step 4 of 7.2 finds the annotation by its index. The source of the annotation can have the kind `clickhouse`, `postgres` or `mongodb`. The expansion follows the kind: 7.5 to 7.13 and the property P for `clickhouse`, 7.21 to 7.24 and the property P for `postgres`, 7.35 and the property PM for `mongodb`. The statement check of the kind applies (7.12, 7.25, 7.30 to 7.32). The route refuses a kind that a later version adds to 8.2 with `400 bad_request`.
- The effective `max_result_rows` is the lowest of the source key, the ceiling `max_rows` and `MAX_ANNOTATION_ROWS` (1,000). A result with more rows gives `422 query_limit`, or `403 ceiling_exceeded` when the ceiling lowered the limit (8.5). The page never gets a cut result.
- The query takes its permits, its reservation and its place in the queue like a panel query (8.8). The log line has `kind` = `annotation` (14.7).

The server makes the marks from the frame of 7.16 with the function `annotations_from_frame` of `hune-core`. The function is pure. It does no I/O, and it reads no clock. The rules are:

1. The columns have a role by their name. The name is compared with the ASCII letters folded to lower case. The roles are `time`, `timeend`, `title`, `text` and `tags`. Hune ignores the other columns, as the plugin of Grafana does. A column named `time_end` or `end` has no role.
2. Two columns with the same role give `annotation_invalid` with the reason `duplicate_column`.
3. The role `time` is the column named `time`. When the frame has no such column, it is the first column of the kind `time`. A frame with neither gives the reason `no_time`.
4. A `time` value of the kind `time` is Unix seconds with a fraction (7.16). The mark gets the milliseconds, and the digits below the millisecond are cut. A `time` value of the kind `number` or `number_text` is epoch milliseconds: it MUST be a whole number. A value of another kind, a `null`, a value below 0 and a value above 4,294,967,295,999 give the reason `bad_time`.
5. The role `timeend` has the same rules. A `null`, the value 0 and a value equal to `time` mean "no end": the mark is a point. An end below the `time` of its row gives the reason `bad_end`. A column of another kind gives the same reason.
6. The roles `title` and `text` need the kind `text` or a `null`. A `null` is the empty text. Another kind gives the reason `bad_text`. There is no fallback to another text column. The Grafana plugin uses the first text column for `text` (19.20e).
7. The role `tags` needs the kind `text` or a `null`. When the text is a JSON array of texts, its items are the tags. This is the form that ClickHouse gives for an `Array(String)` (7.16). Else the text is split at each `,`. The server removes the blanks at both ends of each tag, drops the empty tags and keeps the first of equal tags. A column of another kind gives the reason `bad_text`.
8. The limits are 256 characters for a title, 2,048 bytes for a text, 8 tags and 64 characters for a tag. The server counts after the split and the trim. A longer value gives the reason `too_long`. The server never cuts a value.
9. A title, a text and a tag MUST NOT hold a control character other than U+0009 and U+000A. They MUST NOT hold U+007F or a character from U+0080 to U+009F. They MUST NOT hold a bidirectional control: U+202A to U+202E and U+2066 to U+2069.
   - They MUST NOT hold a zero-width character or a mark: U+200B to U+200F and U+061C. They MUST NOT hold U+2028, U+2029 or U+FEFF.
   - They MUST NOT hold the form of an HTML tag: a `<` followed by a letter, `/`, `!` or `?`. A fault gives the reason `bad_chars`.
   - The server checks each item of a tag list before the trim, so a control character at the edge of an item is a fault.
10. The marks come in the order of `time`, then of the row. The answer holds each row of the result and no other mark. The server drops nothing and merges nothing.
11. Any other fault of the first 10 rules gives `422 annotation_invalid` and no mark. The answer is one error and never a partial list (fail closed). The member `row` is the first row that breaks a rule. The message is one fixed English sentence for each reason, with no cell value.
12. A column of the kind `null` (for example `NULL AS timeEnd`) holds `null` in each row. It is valid for the roles `timeend`, `title`, `text` and `tags`, and it gives `bad_time` for the role `time`.
13. A frame with no column and no row gives no mark and no fault. A `mongodb` source gives this frame for a result with no document (7.16c). A frame with columns and no row also gives no mark.

The server checks the kind of each column before the first row, and it checks the rows in this order: `time`, `timeend`, `title`, `text`, `tags`. For one value, the length check comes before the check of the characters.

| Reason | Cause |
|---|---|
| `no_time` | No column named `time`, and no column of the kind `time`. |
| `bad_time` | The time is `null`, is not a whole number of milliseconds, is out of range, or has a wrong kind. |
| `bad_end` | The end has a wrong kind, is out of range, or is below the time. |
| `bad_text` | The title, the text or the tags have a kind other than `text`. |
| `bad_chars` | A control character, a bidirectional control, or the form of an HTML tag. |
| `too_long` | A title, a text, a tag or the count of tags is above its limit. |
| `duplicate_column` | Two columns have the same role. |

The roles for each kind of source. The rules read the kinds of 7.16 and not the types of the source. Thus the rules are the same for the three kinds. The table gives the types that each role accepts, from 7.16, 7.16a and 7.16c.

| Role | `clickhouse` | `postgres` | `mongodb` |
|---|---|---|---|
| `time`, `timeend`: a time (rule 4) | `Date`, `Date32`, `DateTime`, `DateTime64` | `timestamptz`, `timestamp` (read as UTC), `date` | Date |
| `time`, `timeend`: a whole number of epoch milliseconds (rule 4) | The integer types, `Float32`, `Float64`, `Decimal` | `int2`, `int4`, `int8`, `oid`, `float4`, `float8`, `numeric` | Int32, Int64, Double, Decimal128 |
| `title`, `text` (rule 6) | Each type of the kind `text` | `text`, `varchar`, `bpchar`, `name`, `uuid`, `json`, `jsonb`, `inet`, `cidr`, an enum, `time`, an array | String, ObjectId, UUID, Document, Array |
| `tags` (rule 7) | As `title`. An `Array(String)` gives its items. | As `title`. A `text[]` or a `varchar[]` gives its items. | As `title`. An Array of String gives its items. |
| Refused: `bad_time`, `bad_end` or `bad_text` | `Bool`. A type of the kind `text` for a time. A number for a text. | `bool`. A type of the kind `text` for a time. A number for a text. | Boolean. A String, a Document or an Array for a time. A number for a text. |

- A `mongodb` document that lacks a field gives `null` in that column (7.16c). Rules 4 to 7 apply to the `null`.
- An infinite `date`, `timestamp` or `timestamptz` value is `null` in the frame (7.16a). It gives `bad_time` for `time` and a point for `timeend`.
- An array with an item that is not a text (a number, a `null`) is not an array of texts. Rule 7 splits its JSON text at each `,`. Remove the `null` items (`array_remove`, `$filter`) or convert the items to text in the query.
- The rule of 7.16b does not apply, because the frame has the format `table` (4.5b, rule 2). A number is epoch milliseconds for each kind. `$__timeEpoch` and `extract(epoch from t)` give Unix seconds, and these seconds give marks in January 1970. Give the time value itself, for example with `$__time(t)`, or multiply the seconds by 1000.
- A column with two BSON types that 7.16c refuses stops the query with `422 query_error` before these rules.

The server makes the marks, not the page. The rules are in Rust, in one pure function that has vectors, and the page stays small (9.30). The page reads the answer as data and checks its shape again (9.19d).

A row that holds the text `<b>x</b>` gives `bad_chars`. The page would show it as text, but the refusal is a rule of the product. The text of Grafana is HTML, and the text of Hune never is. The author sees the error and removes the markup. The importer lists the difference (15.6, 19.20e).

## 8. Data sources

### 8.1 The DataSource trait

The crate `hune-source` holds the trait `DataSource`, the enum `AnySource` and the ClickHouse client. The names below
are normative. The code can change a signature when the behaviour stays the same.

```rust
/// One data source of the configuration. Three kinds: ClickHouse (8), PostgreSQL (8a) and MongoDB (8b).
pub trait DataSource: Send + Sync {
    /// The id of the block, for example "prod" for `clickhouse "prod" { ... }`.
    fn id(&self) -> &SourceId;
    /// The kind of the source: SourceKind::ClickHouse, SourceKind::Postgres or SourceKind::MongoDb.
    fn kind(&self) -> SourceKind;
    /// The state of 8.11.
    fn state(&self) -> SourceState;
    /// Runs one prepared query within the limits, and gives one frame (7.16).
    fn query(&self, query: PreparedQuery, limits: EffectiveLimits, ctx: QueryCtx)
        -> impl Future<Output = Result<Frame, SourceError>> + Send;
    /// Runs the probe of 8.11.
    fn probe(&self) -> impl Future<Output = Probe> + Send;
}

/// Enum dispatch: no `dyn`. A new kind adds a variant (8.14).
pub enum AnySource { ClickHouse(ClickHouseSource), Postgres(PostgresSource), Mongo(MongoSource) }

/// The final text from the query engine (7.11). A new kind adds a variant. `params` holds the
/// values of the query parameters of 7.6a: the names in byte order and the escaped texts.
pub enum PreparedQuery { ClickHouseSql { sql: String, params: QueryParams }, PostgresSql { sql: String }, MongoQuery { command: MongoCommand } }

pub struct EffectiveLimits {
    pub max_execution_time: Duration, // whole seconds
    pub max_result_rows: u64,
    pub max_result_bytes: u64,
    pub max_query_size: u64,
    pub rows_from_ceiling: bool,      // true when the ceiling max_rows is the effective max_result_rows (11.24)
    pub time_from_ceiling: bool,      // true when the ceiling max_duration is the effective max_execution_time
}

pub struct QueryCtx {
    pub kind: QueryKind,                  // Panel, Options, Preview (8.10)
    pub query_id: Uuid,                   // UUID version 7 (8.10)
    pub log_comment: LogComment,          // 8.10
    pub subject: SubjectId,               // the admission key of 8.8
    pub traceparent: Option<TraceParent>, // when telemetry is on (14)
}
```

- The types `Frame` and `Kind` are in `hune-core` (7.16). The ClickHouse lexer, the escape rules and the macros are
  in `hune-core` as the dialect of ClickHouse (7). The dialect of PostgreSQL is in `hune-core` too (7.19 to 7.26).
- `SourceError` has one variant for each Hune code of 8.5, 8a.8 and 8b.8.
- A ceiling is the effective value when it is at or below each other bound of its limit. These bounds are the source
  key and the cap of 10,000 rows of an options query (7.14). Example: an options query with the ceiling `max_rows = 50000` and the cap of
  10,000 has `rows_from_ceiling = false`, so a large result gives `422 query_limit`.
- The trait has no method that writes data.

### 8.2 Declaration of the sources

Each ClickHouse source is a labelled block `clickhouse "<id>" { ... }` of the configuration (13). A PostgreSQL source
is a block `postgres "<id>"` (8a.2). A MongoDB source is a block `mongodb "<id>"` (8b.2). The file has 1 to 64 source blocks in all. An id is unique across all kinds of
source, now and later. The id is the value of the member `source` in a
document (4.3, 4.15) and the ProcurAuth node `source:<id>` (11).

This section uses these keys of the block: `url`, `user` (or `user_env`, `user_file`), `password_env` or
`password_file`, `database`, `label`,
`tls { ca_file, server_name }`, `connect_timeout`, `max_execution_time`, `max_result_rows`, `max_result_bytes`,
`max_query_size`, `max_concurrent`, `queue_wait` and `max_concurrent_per_subject`. Section 13 gives their types, ranges
and contradictions.

### 8.3 ClickHouse client: the request

Hune has its own small client on the ClickHouse HTTP interface. It uses the client of `hyper` 1.x and `rustls`. It
does not use `reqwest`, because `reqwest` reads proxy variables of the environment. It uses HTTP/1.1 and never uses a
proxy.

One query is one HTTP request: `POST` to the `url` of the block, with the path `/`. The settings are query parameters,
and the SQL is the body. The SQL never goes in the URL, so a query of 71 KB has no URL limit.

The values of the query parameters of 7.6a go in the URL, as `param_<name>` after `log_comment`, in the byte order of the names. Only the names of the final text get a pair. All the pairs take 4,096 bytes at most (7.6a).

Thus the request URI now holds values of variables. ClickHouse can write the URI to its own log at the level `debug`. A proxy in front of it can log the URI too (12.15). The table `system.query_log` holds the final SQL, with the values of the text form of 7.6, as before. ClickHouse 26.8 writes the values of the query parameters there too, in place of the forms (C41). Example:

```text
POST /?database=analytics&default_format=JSONCompactEachRowWithNamesAndTypes&query_id=019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d&max_execution_time=60&timeout_overflow_mode=throw&max_result_rows=100000&max_result_bytes=33554432&result_overflow_mode=throw&max_query_size=1048576&wait_end_of_query=1&cancel_http_readonly_queries_on_client_close=1&output_format_json_quote_64bit_integers=1&output_format_json_quote_decimals=1&output_format_json_quote_denormals=1&output_format_json_validate_utf8=1&date_time_output_format=unix_timestamp&log_comment=%7B%22app%22%3A%22hune%22%2C%22kind%22%3A%22panel%22%2C%22uid%22%3A%22nodes%22%2C%22panel%22%3A4%2C%22request_id%22%3A%22r-5f2c%22%2C%22subject%22%3A%22-%22%7D HTTP/1.1
Host: clickhouse:8443
X-ClickHouse-User: hune
X-ClickHouse-Key: <the password>
Content-Type: text/plain; charset=utf-8
Content-Length: 412
User-Agent: hune/0.3.2

SELECT toStartOfInterval(toDateTime(event_time), INTERVAL 20 second) AS t, ...
```

Each parameter value is percent-encoded: each byte outside `A-Z`, `a-z`, `0-9`, `-`, `.`, `_` and `~` becomes `%XX`.

| Header | Value |
|---|---|
| `Host` | The host and the port of `url`. |
| `X-ClickHouse-User` | The value of `user`, `user_env` or `user_file` (13). |
| `X-ClickHouse-Key` | The password from `password_env` or `password_file`. It never goes in a URL, a log line or a span. |
| `Content-Type` | `text/plain; charset=utf-8`. |
| `Content-Length` | The size of the SQL in bytes. |
| `User-Agent` | `hune/<version>`. |
| `traceparent` | Only when `telemetry.enabled = true` and the request has a span (14). |

Hune sends no `Authorization`, no `Accept-Encoding` and no cookie.

| Parameter | Value | Reason |
|---|---|---|
| `database` | The key `database`. | The default database of the SQL. |
| `default_format` | `JSONCompactEachRowWithNamesAndTypes` | The format of 8.4. The statement check refuses `FORMAT` (7.12). |
| `query_id` | 8.10 | Attribution. |
| `max_execution_time` | The effective value (7.2), in whole seconds. | The time limit. |
| `timeout_overflow_mode` | `throw` | A query that ClickHouse stops is never shown as complete. |
| `max_result_rows` | The effective value (7.2, 7.14). | The row limit of the result. |
| `max_result_bytes` | The key `max_result_bytes`. | The byte limit of the result. |
| `result_overflow_mode` | `throw` | A cut result is never shown as complete. |
| `max_query_size` | The key `max_query_size`. | Equal to the SQL limit of 7.13. |
| `wait_end_of_query` | `1` | ClickHouse keeps the answer until the end, so an error gets a real HTTP status. |
| `cancel_http_readonly_queries_on_client_close` | `1` | The cancellation of 8.9. |
| `output_format_json_quote_64bit_integers` | `1` | Exact 64-bit and larger integers. |
| `output_format_json_quote_decimals` | `1` | Exact decimals. |
| `output_format_json_quote_denormals` | `1` | NaN and the infinities as text. |
| `output_format_json_validate_utf8` | `1` | The answer is always valid UTF-8. |
| `date_time_output_format` | `unix_timestamp` | Time as Unix seconds (7.16). |
| `log_comment` | 8.10 | Attribution in `system.query_log`. |
| `param_<name>` | The escaped text of the value of the query parameter `name` (7.6a). One pair for each name of the final text, in the byte order of the names. | The value of a query parameter, which never goes in the SQL text. |

Hune sends no `readonly`, no `session_id`, no `max_rows_to_read` and no `max_memory_usage`. The settings profile of the
account holds them (8.12). The constraints of this profile bound each value. An author can change a setting with a
`SETTINGS` clause only within these constraints.

### 8.4 ClickHouse client: the answer

An answer with the status `200` and no header `X-ClickHouse-Exception-Code` is a success. Its body has the format
`JSONCompactEachRowWithNamesAndTypes`:

```text
["t","node","weight"]
["DateTime","LowCardinality(String)","Decimal(38, 9)"]
["1794211200","a1","12.500000000"]
["1794211220",null,"3.000000000"]
```

- Line 1 is a JSON array of `N` texts: the column names. `N` is 1 or more.
- Line 2 is a JSON array of `N` texts: the column types.
- Each next line is a JSON array of `N` values: one row.
- Each line ends with LF. An empty line ends the rows. Hune ignores the lines after it: they hold totals or extremes.

The parser reads the body as it arrives, one line at a time. It checks each value with the table of 7.16 and adds it
to its column. It also does these checks while it reads:

| Check | Result |
|---|---|
| The body has more bytes than `max_result_bytes`. | Hune closes the connection: `422 query_limit` with `limit` `max_result_bytes`. |
| The rows are more than the effective `max_result_rows`. | Hune closes the connection: 8.5 gives the code. |
| Fewer than two lines, `N = 0`, a row with another number of values, a line that is not a JSON array, a value that does not fit its kind. | `502 source_bad_response`. |
| The header `X-ClickHouse-Query-Id` differs from the `query_id` that Hune sent. | `502 source_bad_response`. |

The header `X-ClickHouse-Summary` is a JSON object of texts. Hune reads `read_rows`, `read_bytes` and `result_bytes`
from it for the member `stats` of 7.16.

An answer with a status other than `200`, or with the header `X-ClickHouse-Exception-Code`, is an error. The code is the
integer of this header. The message is the first line of the body, cut to 1,000 bytes. A status other than `200`
without this header, for example from a proxy in front of ClickHouse, gives `503 source_unavailable`. The exceptions are the statuses 414 and 431 (8.5).

One query keeps at most `max_result_bytes` of answer and its frame in memory. Thus the results in flight of one source
take at most `max_concurrent × 2 × max_result_bytes`. Two bounds apply to the memory of all results: the sum of this
value over the sources, and the key `server.max_result_memory_bytes` (13, 12.16). The admission of 8.8 applies the key.

### 8.5 Error mapping

Hune maps an error by the ClickHouse code, never by the message.

| ClickHouse code or event | Hune code | Members |
|---|---|---|
| 159 `TIMEOUT_EXCEEDED`, 160 `TOO_SLOW`, the client deadline (8.9) | `504 query_timeout` | `source`, `query_id` |
| The same, when `time_from_ceiling` is true | `403 ceiling_exceeded` | `ceiling` = `max_duration`, `limit` = the effective seconds |
| 396 `TOO_MANY_ROWS_OR_BYTES` | `422 query_limit` | `limit` = `max_result` (rows or bytes of the result), `value` = `null` |
| The row check of 8.4 | `422 query_limit` | `limit` = `max_result_rows`, `value` = the effective rows |
| 396 or the row check, when `rows_from_ceiling` is true | `403 ceiling_exceeded` | `ceiling` = `max_rows`, `limit` = the effective rows |
| 158 `TOO_MANY_ROWS` | `422 query_limit` | `limit` = `max_rows_to_read`, `value` = `null` |
| 307 `TOO_MANY_BYTES` | `422 query_limit` | `limit` = `max_bytes_to_read`, `value` = `null` |
| The byte check of 8.4 | `422 query_limit` | `limit` = `max_result_bytes`, `value` = the key |
| 241 `MEMORY_LIMIT_EXCEEDED` | `422 query_memory_limit` | none |
| 497 `ACCESS_DENIED` | `502 source_access_denied` | `source`, `query_id` |
| 192 `UNKNOWN_USER`, 193 `WRONG_PASSWORD`, 194 `REQUIRED_PASSWORD`, 516 `AUTHENTICATION_FAILED` | `502 source_misconfigured` | `source` |
| A failed certificate check (8.6) | `502 source_misconfigured` | `source` |
| 202, 203, 209, 210, 279; a refused, reset or timed-out connection; a TLS fault other than the certificate check | `503 source_unavailable` | `source` |
| 394 `QUERY_WAS_CANCELLED`, 735 `QUERY_WAS_CANCELLED_BY_CLIENT` | no answer: the client left (8.9) | none |
| An answer that breaks 8.4 | `502 source_bad_response` | `source`, `query_id` |
| 456 `UNKNOWN_QUERY_PARAMETER`, 457 `BAD_QUERY_PARAMETER` (7.6a) | `422 query_error` | `source`, `clickhouse_code`, `query_id` |
| The HTTP status 414 or 431 with no header `X-ClickHouse-Exception-Code`: a proxy in front of ClickHouse refuses a long request line or long headers | `422 query_error` | `source`, `clickhouse_code` = `null`, `query_id` |
| Any other code, for example 62 `SYNTAX_ERROR`, 60 `UNKNOWN_TABLE`, 164 `READONLY`, 452 `SETTING_CONSTRAINT_VIOLATION` | `422 query_error` | `source`, `clickhouse_code`, `query_id` |

- The member `limit` of `query_limit` has one of five values, a closed list: `max_result`, `max_result_rows`,
  `max_result_bytes`, `max_rows_to_read` and `max_bytes_to_read`.
- The members of `ceiling_exceeded` are `action`, `ceiling` and `limit` (6.8, 11.24).
- When the ceiling equals the key of the source, the ceiling is the cause (11.24).
- When the ceiling lowered `max_result_rows`, a code 396 counts as the ceiling, also when the cause was the byte limit.
  The message then names both limits: the rows and `max_result_bytes` (11.24).
- A ceiling of 0 gives `403 ceiling_exceeded` before Hune sends the query, because ClickHouse reads 0 as "no limit"
  (11.24).
- `source_misconfigured` and `source_unavailable` change the state of the source (8.11).
- The status 414 or 431 changes no state. An earlier version gave `503 source_unavailable`, so a viewer with long values could mark a source down. The values of 7.6a stay below the caps, so these statuses come only from a proxy with a smaller limit.

### 8.6 TLS

- A `url` with the scheme `https` uses TLS 1.2 or TLS 1.3, by `rustls` with the provider `ring`.
- The trust roots are only the certificates of `tls.ca_file` (PEM). Hune reads no system store and no
  `SSL_CERT_FILE` (13.2, rule 5).
- Hune checks the certificate against `tls.server_name`, a necessary key of an `https` source (13.12). It sends the
  same name in SNI.
- v1 has no client certificate. No key turns the certificate check off: the `tlsSkipVerify` of Grafana has no
  equivalent (12).
- A `url` with the scheme `http` uses no TLS. Section 13 gives the contradictions and the warning for a host that is
  not a loopback address.

A Grafana data source can skip the certificate check. Hune cannot: it uses the HTTP interface with the CA file of the
server certificate (19.6).

### 8.7 Connection pool

- Each source has one pool of HTTP/1.1 connections with keep-alive.
- The pool keeps at most `max_concurrent` idle connections. It closes an idle connection after 5 s. The ClickHouse
  setting `keep_alive_timeout` MUST be above 5 s, so that Hune closes a connection before ClickHouse does.
- A new connection does the TCP connect and the TLS handshake within `connect_timeout`. Past it, the query gives
  `503 source_unavailable`.
- TCP keepalive is on, with an idle time of 30 s.
- Hune sends a request again one time only, when a reused idle connection fails before Hune wrote the request. It
  never sends a query again after it wrote it.
- A connection whose request Hune dropped (8.9) closes. It never goes back to the pool.

### 8.8 Admission control and queues

Each panel query, options query and preview passes the admission before Hune sends it:

1. In mode `authzen`, the query takes a permit of the semaphore of its source and its subject. This semaphore has
   `max_concurrent_per_subject` permits. In mode `open` this step does not exist, and section 13 forbids the key.
2. The query takes a permit of the semaphore of its source. This semaphore has `max_concurrent` permits.
3. The query reserves `2 × max_result_bytes` of its source in the memory budget `server.max_result_memory_bytes`
   (13, 12.16).
4. The three waits share one deadline: `queue_wait` after the arrival at the admission. The waits are in arrival order.
   Past the deadline, the query gives `503 source_busy` with `Retry-After: 1`.
5. At most 256 queries wait for one source. One more query gives `503 source_busy` at once.
6. The query keeps its permits and its reservation until the frame is complete, or until it fails or the client
   leaves.

The probe of 8.11 takes no permit. Hune removes the semaphore of a subject when it has no permit in use and no waiter.
Section 14 gives the metrics of the admission. The page also sends 6 query requests at most at the same time (9.8).

A copy of a repeated panel (4.13a, 9.8c) is a panel query. It takes its permits, its reservation and its place in the queue like any other query. A repeat adds no rule here. The page bounds the number of copies, so it bounds the number of queries that one trigger starts (9.8c).

A query of a panel that has several queries is a panel query. It takes its permits, its reservation and its place in the queue like any other query. A request runs one query, so a request holds one place.

The page starts one request for each visible query. It sends 6 requests at most at the same time (9.8d). Thus the number of queries of one page that run at one time does not change, and the memory budget is the same. One trigger starts 519 requests at most with the default limits (9.8d).

Each request has its own wait of `queue_wait`. A query that waits longer gives `503 source_busy`, and only that query shows the error.

An annotation query (6.21a) is a query like a panel query. It takes its permits, its reservation and its place in the queue. The page bounds the number of annotation queries. It sends one for each annotation at each trigger, and `MAX_ANNOTATIONS` (8) at most, whatever the number of panels (9.19d). The page puts them after the panel queries in its queue, so no panel waits for an annotation.

### 8.9 Cancellation and deadlines

- The browser aborts a query when the time range, a variable or the page changes (9.8). The proxy then closes its
  connection to Hune.
- Hune MUST find the close of the client connection while it waits for ClickHouse. It MUST then drop the ClickHouse
  request at once. The ClickHouse connection closes, and ClickHouse stops the query, because of
  `cancel_http_readonly_queries_on_client_close = 1`.
- The query MUST leave `system.processes` within 2 s after the client closes its connection. The test of 16 proves it,
  also on a kept HTTP/1.1 connection.
- After it sends the request, Hune waits for the end of the answer at most the effective `max_execution_time` plus
  5 s. Past this deadline, it closes the connection and answers as 8.5 says.
- Hune never sends `KILL QUERY`: the account has no such grant (8.12).
- At a stop, the queries in flight continue for the time of 14.13. Then Hune closes their connections, and their
  requests get `503 server_stopping`.
- The log line of a query that the client left has the outcome `cancelled` and the status 499 (14).

### 8.10 Attribution

- `query_id` is a new UUID version 7 for each query, in lower-case canonical text. Hune sends it as the parameter
  `query_id`. The frame (7.16), the error members and the log line `query.run` (14) give it. It joins the log of
  Hune, the answer and `system.query_log`.
- `log_comment` is compact JSON of 1,024 bytes at most, with its members in this order:

```json
{"app":"hune","kind":"panel","uid":"nodes","panel":4,"request_id":"r-5f2c","subject":"user:alice"}
```

| Member | Value |
|---|---|
| `app` | `hune`. |
| `kind` | `panel`, `options`, `annotation`, `preview` or `probe`. |
| `uid` | The uid of the dashboard. Absent for `probe`, and for the preview of a new dashboard. |
| `folder` | Only for the preview of a new dashboard: the `folder` of the body (6.22). It comes in place of `uid`. |
| `panel` | The panel id, for `panel` and for a preview of a panel. |
| `ref` | The ref of the query, for `panel`, when the request has a `ref` (6.20). Else the member is absent. It comes after `panel`. |
| `variable` | The variable name, for `options` and for a preview of a variable. It comes in place of `panel`. |
| `annotation` | The index of the annotation, for `annotation`. It comes in place of `panel`. |
| `request_id` | The request id of 6.3. Absent for `probe`. |
| `subject` | The subject (11), or `-` in mode `open`. Hune cuts it to 256 bytes. Absent for `probe`. |

The log line `query.run` (14.7) has the field `kind` with the value `panel`, `options`, `annotation` or `preview`, as in
`log_comment`. A probe writes no `query.run` line. The log line has the field `ref`, which is `null` when the request has no `ref` (14.6). It never holds the SQL.

Example of a search in ClickHouse:

```sql
SELECT query_id, JSONExtractString(log_comment, 'uid') AS uid, query_duration_ms, read_rows
FROM system.query_log
WHERE event_date = today() AND JSONExtractString(log_comment, 'app') = 'hune' AND type = 'QueryFinish'
```

The log of Hune never holds the SQL text, only its SHA-256 (14). The SQL text is in `system.query_log`.

### 8.11 Probe and state of a source

At start, and then every 30 s, Hune runs a probe on each source. The start does not wait for it, and the readiness
does not depend on it (14). The probe sends `SELECT version(), getSetting('readonly')` with all the parameters of 8.3.
Each limit has the value of its key, and the deadline is `connect_timeout` plus 5 s. The `kind` of `log_comment` is
`probe`. Then the probe sends `SHOW GRANTS` in a second request; the own user needs no privilege for it.

Each line of the answer of `SHOW GRANTS` MUST be a `GRANT` to the user itself. It MUST hold only the privileges
`SELECT` or `dictGet`, with or without a column list. A line with another privilege, a grant of a role, or `WITH GRANT OPTION`
breaks the contract of 8.12. Example: the line `GRANT SELECT ON analytics.* TO hune` passes, and the line
`GRANT SELECT, URL ON *.* TO hune` fails.

| State | When |
|---|---|
| `unknown` | Before the first result of the probe. |
| `up` | The probe succeeded, `readonly` is `2`, and each line of `SHOW GRANTS` passes. |
| `misconfigured` | `readonly` is not `2`; a line of `SHOW GRANTS` fails; the login failed; the certificate check failed; or the probe got code 452, because a key is above a constraint of the profile. |
| `down` | The probe failed on the link or on the server (a transport fault, a timeout, the class 08, 57P01 to 57P03 or 53300), or a query got `source_unavailable` after a good probe. |

- A query on an `unknown` source waits for the first probe, within `queue_wait`. One probe runs at a time.
- A query on a `misconfigured` source fails at once with `502 source_misconfigured`. Hune sends nothing to ClickHouse
  until a probe succeeds.
- A query on a `down` source still goes to ClickHouse. A success sets the state `up`.
- A change of state writes the log line `source.up`, `source.down` or `source.misconfigured` and sets the metric
  `hune_source_up` (14). The state of a source never changes the readiness.
- A version below the oldest supported release (8.13) writes the log line `source.version_unsupported` one time for
  each change. The queries still run.

### 8.12 Account contract

The ClickHouse account of Hune is the real security boundary. This paragraph is the only contract of the account:
section 12.12 refers to it. The account MUST have:

- a dedicated user, not `default`;
- a settings profile with `readonly = 2`;
- in this profile, a `MAX` constraint on `max_execution_time`, `max_result_rows`, `max_result_bytes`,
  `max_rows_to_read`, `max_memory_usage` and `max_query_size`;
- in this profile, a `CONST` constraint with the value `throw` on `result_overflow_mode`, `timeout_overflow_mode` and
  `read_overflow_mode`. Thus a `SETTINGS` clause cannot cut a result and show it as complete;
- `SELECT` grants only, on the databases that the dashboards read;
- no `SOURCES` grant (`URL`, `S3`, `REMOTE`, `FILE`, `MONGO` and the others). Thus a query cannot reach the network or
  the disk of ClickHouse;
- no `KILL QUERY` grant, no access management grant, and no other grant.

`readonly = 2` refuses each write and each change of `readonly`, and lets Hune send its limits. `readonly = 1` would
refuse the settings of 8.3. ClickHouse does not check a setting whose new value equals its current value. Thus Hune
can send `throw` for a `CONST` mode. The SQL below is an example:

```sql
CREATE SETTINGS PROFILE IF NOT EXISTS hune_readonly SETTINGS
    readonly = 2,
    max_execution_time = 60 MAX 60,
    max_result_rows = 100000 MAX 100000,
    max_result_bytes = 33554432 MAX 33554432,
    max_rows_to_read = 20000000000 MAX 20000000000,
    max_memory_usage = 4294967296 MAX 4294967296,
    max_query_size = 1048576 MAX 1048576,
    result_overflow_mode = 'throw' CONST,
    timeout_overflow_mode = 'throw' CONST,
    read_overflow_mode = 'throw' CONST;

CREATE USER IF NOT EXISTS hune
    IDENTIFIED WITH sha256_password BY '<the password of the secret store>'
    DEFAULT DATABASE analytics
    SETTINGS PROFILE 'hune_readonly';

GRANT SELECT ON analytics.* TO hune;
```

The block of the configuration names this account (13). The user name comes from exactly one of `user`, `user_env`
and `user_file`. The password comes from exactly one of `password_env` and `password_file`, never from the file:

```hcl
clickhouse "prod" {
  user          = "hune"                               # or user_env = "CLICKHOUSE_USER"
                                                        # or user_file = "/run/secrets/clickhouse/user"
  password_file = "/run/secrets/clickhouse/password"    # or password_env = "CLICKHOUSE_PASSWORD"
  database      = "analytics"
}
```

The example shows only the keys of the account. Section 13 gives the complete block.

- Each `MAX` value of the profile MUST be at or above the same key of each `clickhouse` block of this account. Else
  the probe gives `misconfigured` (8.11).
- The operator chooses `max_rows_to_read` and `max_memory_usage`. Hune does not send them.
- When the dashboards use dictionaries, add `GRANT dictGet ON analytics.<dictionary> TO hune` for each one. M0 tells
  if this is necessary (18).
- The operator MAY add `HOST IP '<network of Hune>'` to the user.

The test of 16 logs in as the user `hune` and runs these statements:

```sql
SELECT getSetting('readonly');            -- 2
CHECK GRANT SELECT ON analytics.*;          -- 1
CHECK GRANT INSERT ON analytics.*;          -- 0
CHECK GRANT ALTER ON analytics.*;           -- 0
CHECK GRANT CREATE ON *.*;                 -- 0
CHECK GRANT DROP ON *.*;                   -- 0
CHECK GRANT SOURCES ON *.*;                -- 0
CHECK GRANT URL ON *.*;                    -- 0
CHECK GRANT S3 ON *.*;                     -- 0
CHECK GRANT REMOTE ON *.*;                 -- 0
CHECK GRANT FILE ON *.*;                   -- 0
CHECK GRANT MONGO ON *.*;                  -- 0
CHECK GRANT KILL QUERY ON *.*;             -- 0
CHECK GRANT ACCESS MANAGEMENT ON *.*;      -- 0
CHECK GRANT SYSTEM ON *.*;                 -- 0
SHOW GRANTS;                               -- exactly the GRANT SELECT lines of the contract
```

When a ClickHouse release gives another name to a source privilege, the test uses the name of that release.

### 8.13 Supported ClickHouse versions

- Hune supports each LTS release that ClickHouse still maintains at the date of a Hune release. It also supports
  each newer stable release. At the date of this document, these are 26.3 LTS, 26.8 LTS and the newer releases.
- The CI tests the latest LTS release (16). The ClickHouse version of the Grafana instance in 1.3 is not measured yet (19).
- Each setting of 8.3 and the statement `CHECK GRANT` exist in each supported release.
- The probe writes a warning for an older release (8.11). An older version needs an upgrade of
  ClickHouse, or a test of Hune on that version (19).

### 8.14 How a future source is added

A new kind of source (for example PostgreSQL or MongoDB) needs these changes, in the same release:

1. A new section of this specification and a new line in the table of 2.
2. A configuration block kind with its keys (13). Its ids share the id space of 8.2.
3. A dialect in `hune-core`, with its lexer, its escape rules for each context, its macros and its statement check.
   Each one has test vectors. Sections 7.5 to 7.12 are the model for a SQL dialect.
4. A variant of `PreparedQuery` and of `AnySource`, and its client in `hune-source`. The client has admission (8.8),
   cancellation (8.9), attribution (8.10), a probe (8.11) and an account contract (8.12).
5. A mapping table from its types to the six kinds of 7.16. The frame does not change.
6. A new data kind in the document when the query is not SQL text. This is a new `format_version` (4.42).
7. The tests of a source: integration, limits, cancellation and frame mapping (16).

The kind `postgres` is the first kind that follows this list. Section 8a, 7.19 to 7.26 and 13.12a give its items. The kind `mongodb` is the second one. Section 8b, 7.16c, 7.27 to 7.36 and 13.12b give its items. Its query is not SQL text, but it fits the member `sql` of 4.15, so item 6 needs no new `format_version`.

The ProcurAuth type `source` already covers each kind (11). A source of streams, for example Kafka, needs a panel model
for streams, which v1 does not define.

## 8a. PostgreSQL data source

### 8a.1 Scope and status

The kind `postgres` is the second kind of data source. It follows the steps of 8.14. This section gives the client, the limits, the probe and the account of the kind. Section 7 gives its dialect (7.19 to 7.26) and its frame types (7.16a, 7.16b). Section 13 gives its keys (13.12a).

- The document format does not change. A panel or a variable of a PostgreSQL source has the same member `data.query` or `query` as a ClickHouse source (4.15). The member `source` holds the id of a `postgres` block. There is no new `format_version` (4.42).
- A PostgreSQL source reads data. It never writes data, never runs code on the server, and never opens a connection other than the one to its `host` and `port`.
- This kind is not the PostgreSQL store of section 5. The store has its own role, its own keys and its own pool. A source never shares a connection with the store. On the same server, it never uses the role of the store (13.14, C20).
- This section is the design for the milestone MPG (18.10a). The security choices of this section are open questions (19.32). Each open question has the default that this section states.

### 8a.2 Declaration and code

Each source is a labelled block `postgres "<id>" { ... }` of the configuration (13.12a). The file has 1 to 64 source blocks in all, of the kinds `clickhouse` and `postgres`. An id is unique across both kinds (13.14, C15). The id is the value of the member `source` in a document and the ProcurAuth node `source:<id>` (11.15).

The code follows the rules of 8.1 and 8.14:

- The crate `hune-source` gets the variant `AnySource::Postgres(PostgresSource)`, the variant `PreparedQuery::PostgresSql { sql }` and the value `SourceKind::Postgres`. The enum stays an enum, with no `dyn` (R8).
- The client uses `tokio-postgres` and `tokio-postgres-rustls`, which the workspace already holds for the store. The source adds no crate. It does not use `libpq`, `sqlx`, `deadpool-postgres` or an ORM. The pool is the small pool of 8a.9.
- The crate `hune-core` gets the dialect `postgres` (7.19 to 7.26) and the frame types of 7.16a and 7.16b. It does no I/O.
- The trait `DataSource` has no method that writes data. The client has no function that sends a text of a caller by the simple protocol (8a.5).

### 8a.3 The security model

The read-only rule holds on four sides. The role cannot write. The connection cannot write, and each query runs in a read-only transaction. The text of the query can only be a read. The table gives the ten rules. Each rule has a test in 16.9a.

| # | Rule | Where |
|---|---|---|
| 1 | The configuration is closed and strict. A secret comes only from `_env` or `_file`. No environment variable has an effect. | 13.2, 13.5, 13.12a |
| 2 | The role is read-only by construction: `SELECT` grants only, no write attribute, no dangerous membership. The probe checks it and refuses a role that can write. | 8a.12, 8a.13 |
| 3 | Each session starts with `default_transaction_read_only = on`. Hune reads the value back at each new connection. | 8a.4 |
| 4 | Each query runs in `BEGIN READ ONLY` and ends with `ROLLBACK`. | 8a.5 |
| 5 | The SQL goes in one Parse message of the extended protocol, so a second statement is impossible. The simple protocol never carries SQL of a caller. | 8a.5 |
| 6 | The grammar is closed: the first word, the refused words, the refused forms and the refused functions. | 7.25 |
| 7 | The result has a hard cap on rows and on bytes, and the query has a hard deadline with a cancel request. | 8a.6, 8a.7 |
| 8 | No code runs on the server and no other server is reached, as far as a client can make sure. The role requirements close the rest. | 7.25, 8a.13 |
| 9 | The link uses TLS with the configured CA file, a name check and channel binding. No key skips the check. | 8a.4 |
| 10 | No SQL text, value, row or secret goes to a log, a metric or a span. A log line holds the SQLSTATE and the SHA-256 of the SQL only. | 12.15 |

### 8a.4 Connection and session

- Hune reads no `PG*` variable, no password file and no service file. It builds the connection from the keys of the block only (13.23).
- The connection goes to the `host` and the `port` of the block. A DNS name resolves at each new connection. Hune tries each address in order. It never uses a second host, a Unix socket or a proxy.
- With `sslmode = "verify-full"`, the connection uses TLS 1.2 or TLS 1.3 by `rustls` with the provider `ring`. The trust roots are only the certificates of `tls.ca_file`. Hune checks the certificate against `tls.server_name` and sends the same name in SNI. It sets `channel_binding = require`, so the server MUST use SCRAM-SHA-256 with channel binding. No key skips a check, and Hune sends no client certificate.
- With `sslmode = "disable"`, the connection has no TLS. Hune cannot refuse a weak method then, and `pg_hba.conf` is the only control (12.13). Section 13 gives the warning for a host that is not a loopback address.
- The connection setup, TLS and authentication must end within `connect_timeout`. Past it, the query gives `503 source_unavailable`.
- TCP keepalive is on. With `sslmode = "verify-full"` the idle time is 30 s. With `sslmode = "disable"` the idle time is the value of the system.

The startup packet holds these parameters. The `options` parameter carries the `-c` items. Each value is a constant, a key, or a value that the grammar of 13.13 checks. Thus no value can hold a space or a quote mark.

| Parameter | Value | Reason |
|---|---|---|
| `client_encoding` | `UTF8` | A multibyte character never hides a quote mark (7.20). |
| `application_name` | `hune` | Attribution (8a.11). Each query sets a longer name. |
| `default_transaction_read_only` | `on` | The connection side of the read-only rule. |
| `standard_conforming_strings` | `on` | A backslash in a string is an ordinary byte (7.21). |
| `TimeZone` | `UTC` | The same SQL gives the same result on each instance (7.3). |
| `DateStyle` | `ISO,YMD` | The same text for a cast to `text`. |
| `search_path` | The key `search_path`. | The same names on each instance. |
| `statement_timeout` | The key `max_execution_time`, in milliseconds. | A default. Each query sets its own value (8a.5). |
| `lock_timeout` | `5000` | A schema change on the server does not hold a panel. |
| `idle_in_transaction_session_timeout` | `15000` | The server ends a session that Hune leaves open. |
| `client_connection_check_interval` | `1000` | The server stops a query when the client closes the connection (8a.7). |

The connection setup check (R0 of 8a.12) runs on each new connection, before any query. It reads the values of `server_version_num`, `default_transaction_read_only`, `standard_conforming_strings`, `client_encoding`, `TimeZone` and `search_path`. A value that differs from the table closes the connection. The state of the source becomes `misconfigured` (8a.12). A server that refuses a startup parameter of the table gives the same result.

### 8a.5 The query

The SQL of a caller goes in one Parse message of the extended protocol, and nowhere else. The text of the simple protocol is a constant of Hune. These are all the texts that Hune sends by itself:

| Text | Use |
|---|---|
| `BEGIN READ ONLY` | The start of each query and of each probe. |
| `SELECT set_config('statement_timeout', $1, true), set_config('lock_timeout', $2, true), set_config('application_name', $3, true)` | The values of one query. The three values are text parameters. |
| `ROLLBACK` | The end of each query and of each probe. |
| `DISCARD ALL` | The reset of a connection that goes back to the pool. |
| The statements of 8a.12 | The setup check, the probe and the advice. |

One query does these steps:

1. It passes the admission of 8.8 (8a.10).
2. It takes a connection from the pool, or opens one (8a.9).
3. It starts the deadline of 8a.7.
4. It sends `BEGIN READ ONLY`. A transport failure here, on a reused connection, makes Hune open a new connection and start again one time. After this step, Hune never sends a query again.
5. It sends the `set_config` statement. `statement_timeout` is the effective `max_execution_time` in milliseconds. `lock_timeout` is the lower of 5000 and that value. `application_name` is `hune/<kind>/<query_id>` (8a.11).
6. It sends Parse and Describe for the final SQL (7.2, step 12), with the attribution comment of 8a.11 before it. The answer to Describe gives the type of each column. A type that 7.16a does not list stops the query here with `422 query_error`. A syntax error, or an object that does not exist, also stops it here (8a.8).
7. It sends Bind with no parameter, and it makes one portal.
8. It sends Execute for the portal, again and again, until the portal ends. Each Execute asks for at most `min(1000, R + 1)` rows. `R` is the number of rows that the effective `max_result_rows` still permits.
9. Hune reads each row as it arrives. It decodes the row by 7.16a, adds the row to the columns, and updates the counts of 8a.6. A row above a cap stops the query (8a.6).
10. At the end of the portal, it makes the frame (7.16), sends `ROLLBACK` and `DISCARD ALL`, and puts the connection back in the pool. `DISCARD ALL` removes the prepared statements, so the next query prepares its statements again.
11. It releases the permits.

A text that is longer than `max_query_size` gives `422 query_error` with `sqlstate` set to `null`, before any connection. The server checks `sql_too_large` before (7.13), so this second check is a guard.

After an error, Hune reuses the connection only in one case. The error is an ErrorResponse of the server, or a stop of Hune before the portal runs. Examples of such a stop are a column of a type that 7.16a does not list, and a statement with a parameter. In both cases `ROLLBACK` and `DISCARD ALL` MUST both succeed within 2 s. In each other case, Hune closes the connection. This includes a cap, a deadline, a client that left, a transport failure and a protocol fault.

The text that Hune sends in step 6 is the attribution comment (8a.11), a line end, and the final SQL. The SQL is the text that the checks of 7.25 accepted. The comment is not part of the size limit of `max_query_size`.

### 8a.6 The answer and the limits

| Limit | Rule | Result |
|---|---|---|
| Rows | The effective `max_result_rows` (7.2, step 13). Hune asks for one row more than the limit. The extra row is the overflow. | `422 query_limit` with `limit` = `max_result_rows` and `value` = the effective rows. With `rows_from_ceiling`, `403 ceiling_exceeded` (8a.8). |
| Bytes | The key `max_result_bytes`. Hune counts for each value the length in bytes of its compact JSON text (7.16), plus 1. It checks the count after each row. | `422 query_limit` with `limit` = `max_result_bytes` and `value` = the key. |
| Time | 8a.7. | 8a.8. |
| SQL size | The key `max_query_size`, on the final SQL without the comment (7.13). | `422 sql_too_large` |

Hune never cuts a result. A result above a cap is an error, and the browser gets no row of it. When a cap stops the query, Hune closes the connection. It sends a cancel request first, so that the server stops the work (8a.7).

One query keeps at most `max_result_bytes` of frame and one row in memory. The results in flight of one source take at most `max_concurrent × 2 × max_result_bytes`. The admission reserves this memory in the budget `server.max_result_memory_bytes` (8.8). 8a.15 gives the limit of one large row.

### 8a.7 Time limits and cancellation

PostgreSQL starts `statement_timeout` again at each Parse, Bind and Execute message of the extended protocol. It does not apply it to the whole query. A query with many Execute messages can thus run longer than the value. The client deadline is the bound of the whole query.

- The deadline starts at step 3 of 8a.5. It is the effective `max_execution_time` plus 5 s. At the deadline, Hune sends a cancel request, closes the connection, and answers as 8a.8 says.
- The server value `statement_timeout` is the effective `max_execution_time`. For a query of one Execute message, the server stops first, with the SQLSTATE 57014.
- The cancel request goes to the same `host` and `port` of the block. It uses the same TLS rules and the same `connect_timeout`. It needs no privilege and no SQL.
- One cancel request runs at a time for each source. The wait for the turn and the request share the time limit `connect_timeout`. A lost request is not an error, because Hune closes the connection anyway.
- Hune never calls `pg_cancel_backend` or `pg_terminate_backend`. A role can signal each session of the same role without a special right, and `PUBLIC` can run both functions by default. The probe refuses a role that can run them (R7, 8a.12), so the role of 8a.13 has no `EXECUTE` right on them. The membership in `pg_signal_backend` is also refused (R2).
- The browser aborts a query when the time range, a variable or the page changes (9.8). Hune MUST find the close of the client connection while it waits for the server. It MUST then send a cancel request at once and close the connection.
- The query MUST leave `pg_stat_activity` within 2 s after the client closes its connection. The test PS05 proves it on a kept HTTP/1.1 connection.
- The startup parameter `client_connection_check_interval = 1000` makes the server stop a query that runs when the TCP connection closes. This works also when the cancel request is lost.
- A connection that carried a cancelled or dropped query never goes back to the pool.
- At a stop, the queries in flight continue for the time of 14.13. Then Hune sends the cancel requests, closes the connections, and the requests get `503 server_stopping`.
- The log line of a query that the client left has the outcome `cancelled` and the status 499 (14.7).

### 8a.8 Error mapping

Hune maps an error by the SQLSTATE, never by the message. The member `message` of `query_error` is the first line of the primary message of the server error, cut to 1,000 bytes (6.7).

| SQLSTATE or event | Hune code | Members |
|---|---|---|
| 57014 `query_canceled`, when Hune did not send the cancel request for a client that left; the client deadline (8a.7) | `504 query_timeout` | `source`, `query_id` |
| The same, when `time_from_ceiling` is true | `403 ceiling_exceeded` | `ceiling` = `max_duration`, `limit` = the effective seconds |
| 55P03 `lock_not_available` | `504 query_timeout` | `source`, `query_id` |
| The row check of 8a.6 | `422 query_limit` | `limit` = `max_result_rows`, `value` = the effective rows |
| The row check, when `rows_from_ceiling` is true | `403 ceiling_exceeded` | `ceiling` = `max_rows`, `limit` = the effective rows |
| The byte check of 8a.6; a message of the server that is longer than `max_result_bytes` plus 1 MiB (8a.15) | `422 query_limit` | `limit` = `max_result_bytes`, `value` = the key |
| 53200 `out_of_memory`, 53400 `configuration_limit_exceeded` | `422 query_memory_limit` | none |
| 42501 `insufficient_privilege` | `502 source_access_denied` | `source`, `query_id` |
| The class 28 (28000, 28P01) or 3D000 `invalid_catalog_name` in the setup of a connection, a refused startup parameter, a failed certificate check (8a.4) | `502 source_misconfigured` | `source` |
| The class 28 or 3D000, raised by a statement of the query on an open session, for example `SELECT pg_database_size('no_such_db')` | `422 query_error` | `source`, `sqlstate`, `query_id` |
| A failed setup check or probe check of 8a.12 | `502 source_misconfigured` | `source` |
| The class 08, 53000, 53100, 53300, 57P01, 57P02, 57P03, 25P03, 40001, 40P01; a refused, reset or timed-out connection; a TLS fault other than the certificate check | `503 source_unavailable` | `source` |
| A value that does not decode (7.16a) | `502 source_bad_response` | `source`, `query_id` |
| A column of a type that 7.16a does not list | `422 query_error` | `source`, `sqlstate` = `null`, `query_id` |
| 25006 `read_only_sql_transaction` | `422 query_error` | `source`, `sqlstate`, `query_id` |
| Any other SQLSTATE, for example 42601 `syntax_error`, 42P01 `undefined_table`, 22012 `division_by_zero` | `422 query_error` | `source`, `sqlstate`, `query_id` |

- A query that the client left gives no answer (8a.7). Hune knows this case only from its own cancel request for a client that left. A 57014 from the cancel of another session, or from the server `statement_timeout`, gives `504 query_timeout`.
- A fault of the login or of TLS that comes before any ErrorResponse gives `503 source_unavailable`. A failed certificate check and a refused channel binding are exceptions: they give `502 source_misconfigured`.
- The message of `source_unavailable` is a fixed English text, with the SQLSTATE when there is one. The message of `source_misconfigured` is one fixed English text for the viewer: it holds no SQLSTATE and no name of the catalog. The reason, for example `R4` and the names, goes to the log line `source.misconfigured` only. No message of these two codes holds text of the server, because the log lines `source.down` and `source.misconfigured` carry them (12.15). The messages of `query_error`, `source_access_denied` and `query_memory_limit` hold the first line of the server message, cut to 1,000 bytes.
- The member `limit` of `query_limit` has two values for this kind: `max_result_rows` and `max_result_bytes`. PostgreSQL has no limit on the rows or bytes that a query reads.
- The members of `ceiling_exceeded` are `action`, `ceiling` and `limit` (6.8, 11.33). A ceiling of 0 gives `403 ceiling_exceeded` before Hune sends the query, as in 8.5.
- A `query_error` of a source of this kind has the member `sqlstate` and no member `clickhouse_code`.
- The code 25006 shows that a text tried to write. It means that the checks of 7.25 let it pass or did not run. The log line `query.run` has the outcome `sql_error` and the field `sqlstate`.
- `source_misconfigured` and `source_unavailable` change the state of the source (8a.12). A statement of a query that raises the class 28 or 3D000 says something about the text of the query, not about the account. It never changes the state.

### 8a.9 Connection pool

- Each source has one pool of at most `max_concurrent` connections. The probe has its own connection, and a cancel request uses one more short connection. A source thus holds `max_concurrent + 2` sessions at most. The operator sets the `CONNECTION LIMIT` of the role to this number or more (8a.13).
- The pool keeps at most `max_concurrent` idle connections. It closes an idle connection after 60 s. It closes a connection that is older than 1 hour when the connection goes back.
- A new connection does the TCP connect, the TLS handshake, the startup, the authentication and the setup check (8a.4) within `connect_timeout`.
- A connection never goes back to the pool in the cases of 8a.5 and 8a.7 that close it.
- Hune supports a direct connection to PostgreSQL, or a connection pooler in session mode. A pooler in transaction mode is not supported, because the session parameters and `DISCARD ALL` need one session for each connection.

### 8a.10 Admission control

The admission of 8.8 applies to each panel query, options query, annotation query and preview of a PostgreSQL source. It uses the keys `max_concurrent`, `queue_wait` and `max_concurrent_per_subject` of the block. The pool of 8a.9 has the size of the semaphore of the source. A permit thus gives a connection, or the right to open one. The probe takes no permit. The metrics of 14.8 apply with the label `source`.

### 8a.11 Attribution

- `query_id` is a UUID version 7 for each query, as in 8.10. The frame, the error members and the log line `query.run` give it.
- The server value `application_name` of a query is `hune/<kind>/<query_id>`, with `<kind>` one of `panel`, `options`, `annotation`, `preview` and `probe`. It has at most 63 bytes. The probe uses `hune/probe`.
- The attribution comment is the first line of the text of step 6 of 8a.5. It is a block comment with compact JSON:

```text
/*hune {"app":"hune","kind":"panel","uid":"nodes","panel":4,"query_id":"019a6b3c-6f1e-7d2a-9b4c-1e2f3a4b5c6d","request_id":"r-5f2c","subject":"user%3Aalice"}*/
```

The members follow the order of 8.10, with `ref` after `panel`, and with `query_id` after `panel`, `ref`, `variable` or `annotation`. An annotation query has `annotation` (its index) in place of `panel`. The member `ref` is the ref of the query, when the request has one (6.20). The comment has 1,024 bytes at most. Each text member (`uid`, `folder`, `variable`, `request_id` and `subject`) is percent-encoded: each byte outside `A-Z`, `a-z`, `0-9`, `-`, `.`, `_` and `~` becomes `%XX`. A valid id and a valid request id have no such byte, so their text does not change.

A block comment nests in PostgreSQL, so no `/*` or `*/` can occur in the comment. Thus the comment can never close early and never hide the SQL. A unit test checks this with random input.

If the comment is longer than 1,024 bytes, Hune shortens `subject`. Then it drops `subject`, `request_id`, `variable`, `folder` and `uid`, in this order, until the comment fits.

An operator finds the queries of Hune in the server:

```sql
SELECT pid, application_name, state, now() - query_start AS running
FROM pg_stat_activity
WHERE application_name LIKE 'hune/%';
```

The statement log of PostgreSQL (`log_min_duration_statement`) shows the comment with the SQL. The log of Hune never holds the SQL text, only its SHA-256 (12.15). The hash covers the final SQL without the comment.

### 8a.12 Probe and state of a source

At start, and then every 30 s, Hune runs a probe on each source. The start does not wait for it, and the readiness does not depend on it (14.4). The probe opens its own connection, with the rules of 8a.4. It runs the statements below in one `BEGIN READ ONLY` transaction, in the extended protocol, and ends with `ROLLBACK`. The deadline of the whole probe is `connect_timeout` plus 10 s. A probe writes no `query.run` line.

The mandatory checks give the state `misconfigured` when one of them fails. The `$1` and the `$2` of a check are text arrays that the code holds as constants. R4, R5 and R6 give each name as schema, dot, name, with the function `format` and the pattern `%I`. R6 adds a dot and the name of the column. A `regclass` leaves out the schema that is in the `search_path`.

The checks R3 to R8 look at the login role and at each role that it is a member of. The text `pg_has_role(current_user, m.oid, 'MEMBER')` selects these roles. It is true for the login role and for each direct or indirect membership, with or without `INHERIT`. A role without `INHERIT` can still run `SET ROLE` to the group and use its rights. The check R9 refuses each object that one of these roles owns. An owner can give itself each right again.

| Check | Statement | It passes when |
|---|---|---|
| R0 | `SELECT current_setting('server_version_num')::int, current_setting('default_transaction_read_only'), current_setting('standard_conforming_strings'), current_setting('client_encoding'), current_setting('TimeZone'), current_setting('search_path')` | The values are an integer, `on`, `on`, `UTF8`, `UTC`, and the key `search_path` with the blanks removed. |
| R1 | `SELECT rolsuper, rolcreaterole, rolcreatedb, rolreplication, rolbypassrls FROM pg_roles WHERE rolname = current_user` | One row with five `false`. |
| R2 | `SELECT r.rolname FROM pg_roles r WHERE r.rolname <> current_user AND pg_has_role(current_user, r.oid, 'MEMBER') AND (r.rolname LIKE 'pg\_%' OR r.rolsuper OR r.rolcreaterole OR r.rolcreatedb OR r.rolreplication OR r.rolbypassrls) LIMIT 5` | No row. |
| R3 | `SELECT n.nspname FROM pg_namespace n WHERE n.nspname NOT LIKE 'pg\_toast%' AND n.nspname NOT LIKE 'pg\_temp\_%' AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_schema_privilege(m.oid, n.oid, 'CREATE')) LIMIT 5`, and `SELECT 'database' WHERE EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_database_privilege(m.oid, current_database(), 'CREATE'))` | No row from both. |
| R4 | `SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f') AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND n.nspname NOT LIKE 'pg\_toast%' AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND (has_table_privilege(m.oid, c.oid, 'INSERT') OR has_table_privilege(m.oid, c.oid, 'UPDATE') OR has_table_privilege(m.oid, c.oid, 'DELETE') OR has_table_privilege(m.oid, c.oid, 'TRUNCATE') OR has_table_privilege(m.oid, c.oid, 'REFERENCES') OR has_table_privilege(m.oid, c.oid, 'TRIGGER'))) LIMIT 5` | No row. |
| R5 | `SELECT format('%I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind = 'S' AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_sequence_privilege(m.oid, c.oid, 'UPDATE')) LIMIT 5` | No row. |
| R6 | `SELECT format('%I.%I.%I', n.nspname, c.relname, a.attname) FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE a.attacl IS NOT NULL AND a.attnum > 0 AND NOT a.attisdropped AND n.nspname NOT IN ('pg_catalog', 'information_schema') AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND (has_column_privilege(m.oid, a.attrelid, a.attnum, 'INSERT') OR has_column_privilege(m.oid, a.attrelid, a.attnum, 'UPDATE') OR has_column_privilege(m.oid, a.attrelid, a.attnum, 'REFERENCES'))) LIMIT 5` | No row. |
| R7 | `SELECT p.oid::regprocedure::text FROM pg_proc p WHERE (p.proname LIKE 'dblink%' OR (p.pronamespace = 'pg_catalog'::regnamespace AND p.proname::text = ANY ($1::text[])) OR p.proname::text = ANY ($2::text[])) AND EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_function_privilege(m.oid, p.oid, 'EXECUTE')) LIMIT 5`, with `$1` = `pg_read_file`, `pg_read_binary_file`, `pg_ls_dir`, `pg_stat_file`, `lo_import`, `lo_export`, `pg_reload_conf`, `pg_rotate_logfile`, `pg_switch_wal`, `pg_promote`, `pg_create_restore_point`, `pg_logical_emit_message`, `pg_cancel_backend`, `pg_terminate_backend`, `ts_stat`, `ts_rewrite`, and `$2` = `crosstab`, `crosstab2`, `crosstab3`, `crosstab4`, `connectby`, `xpath_table` | No row. |
| R8 | `SELECT w.fdwname FROM pg_foreign_data_wrapper w WHERE EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_foreign_data_wrapper_privilege(m.oid, w.oid, 'USAGE')) UNION ALL SELECT s.srvname FROM pg_foreign_server s WHERE EXISTS (SELECT 1 FROM pg_roles m WHERE pg_has_role(current_user, m.oid, 'MEMBER') AND has_server_privilege(m.oid, s.oid, 'USAGE'))` | No row. |
| R9 | `SELECT format('table %I.%I', n.nspname, c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relkind NOT IN ('i', 'I', 't') AND n.nspname NOT LIKE 'pg\_toast%' AND pg_has_role(current_user, c.relowner, 'MEMBER') UNION ALL SELECT format('schema %I', n.nspname) FROM pg_namespace n WHERE pg_has_role(current_user, n.nspowner, 'MEMBER') UNION ALL SELECT format('function %s', p.oid::regprocedure::text) FROM pg_proc p WHERE pg_has_role(current_user, p.proowner, 'MEMBER') UNION ALL SELECT format('type %I.%I', n.nspname, t.typname) FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace WHERE t.typrelid = 0 AND t.typcategory <> 'A' AND pg_has_role(current_user, t.typowner, 'MEMBER') UNION ALL SELECT format('database %I', d.datname) FROM pg_database d WHERE pg_has_role(current_user, d.datdba, 'MEMBER') UNION ALL SELECT format('large object %s', l.oid) FROM pg_largeobject_metadata l WHERE pg_has_role(current_user, l.lomowner, 'MEMBER') LIMIT 5` | No row. |

The advice checks never change the state. Each one that fails writes the log line `source.advice` one time for each change (14.7). The line holds the check and the first names. The operator closes them with the SQL of 8a.13.

| Check | Statement | The advice |
|---|---|---|
| A1 | `SELECT l.lanname FROM pg_language l WHERE l.laninline <> 0 AND has_language_privilege(current_user, l.oid, 'USAGE')` | The role can run `DO` blocks. Revoke `USAGE` on the language. |
| A2 | `SELECT 'temp' WHERE has_database_privilege(current_user, current_database(), 'TEMPORARY')` | The role can make temporary tables, if a session is read-write. |
| A3 | `SELECT p.oid::regprocedure::text FROM pg_proc p WHERE p.pronamespace = 'pg_catalog'::regnamespace AND (p.proname IN ('pg_sleep', 'pg_sleep_for', 'pg_sleep_until', 'pg_notify') OR p.proname LIKE 'pg\_advisory\_%' OR p.proname LIKE 'pg\_try\_advisory\_%' OR p.proname LIKE 'query\_to\_xml%' OR p.proname LIKE 'cursor\_to\_xml%') AND has_function_privilege(current_user, p.oid, 'EXECUTE') LIMIT 5` | The role can call a function that the grammar refuses. Revoke `EXECUTE` from `PUBLIC`. |
| A4 | `SELECT current_setting('temp_file_limit')` | The value is `-1`: a query can fill the disk with temporary files. Set `temp_file_limit` for the role. |

| State | When |
|---|---|
| `unknown` | Before the first result of the probe. |
| `up` | The probe succeeded and each mandatory check passed. |
| `misconfigured` | A mandatory check failed; the server is below version 15 (8a.14); the login failed; the certificate check failed; the server refused a startup parameter; a statement of the probe failed with another error than the class 08, 57P01 to 57P03 and 53300. A probe fault never leaves this state. |
| `down` | The probe failed for another reason, or a query got `source_unavailable` after the last good probe. |

- A query on an `unknown` source waits for the first probe, within `queue_wait`. One probe runs at a time.
- A query on a `misconfigured` source fails at once with `502 source_misconfigured`. Hune sends nothing to the server until a probe passes. A probe that has a fault, for example a timeout, does not pass: the state stays `misconfigured`. The pool closes its idle connections at the change. A connection that is in use at the change is closed when its query ends.
- A query on a source that has passed no probe since the start, or since its last `misconfigured` state, sends nothing to the server. It gets `503 source_unavailable`. This is the case of a source whose first probe met a fault.
- A query on a `down` source that has passed a probe still goes to the server. A success sets the state `up`. A query never changes a `misconfigured` state: only a probe that passes does.
- A change of state writes the log line `source.up`, `source.down` or `source.misconfigured` and sets the metric `hune_source_up` (14.7, 14.8). The line `source.misconfigured` has the check, for example `R4`, and the first names in the field `reason`. The message of the error to the viewer has none of them (8a.8). The state of a source never changes the readiness.
- A version below the oldest supported release (8a.14) fails the check R0: the state is `misconfigured`, and the reason names the version.

### 8a.13 Account contract

The role of the source is the real security boundary of this kind. This paragraph is the only contract of the role, and 12.19 refers to it. The role MUST have:

- a dedicated login role, which is not the role of the store (13.14, C20) and not a superuser;
- none of the attributes `SUPERUSER`, `CREATEROLE`, `CREATEDB`, `REPLICATION` and `BYPASSRLS` (R1);
- no membership, direct or indirect, in a role whose name starts with `pg_`, and none in a role with one of these attributes (R2). This excludes `pg_read_server_files`, `pg_write_server_files`, `pg_execute_server_program`, `pg_signal_backend` and the other predefined roles. A membership in a plain role is possible. The checks R3 to R8 then apply to this role too, with or without `INHERIT`;
- no `CREATE` privilege on the database or on a schema (R3);
- no `INSERT`, `UPDATE`, `DELETE`, `TRUNCATE`, `REFERENCES` or `TRIGGER` privilege on a table, a view or a foreign table (R4);
- no `UPDATE` privilege on a sequence (R5), and no column privilege `INSERT`, `UPDATE` or `REFERENCES` (R6);
- `SELECT` grants only, on the schemas that the dashboards read;
- no `EXECUTE` privilege on a function of the extension `dblink`;
- no `EXECUTE` privilege on a function that R7 names. These are the file functions, the administration functions, `pg_cancel_backend`, `pg_terminate_backend`, `pg_logical_emit_message`, `ts_stat`, `ts_rewrite`, `crosstab*`, `connectby` and `xpath_table`. A grant to `PUBLIC` is a grant to the role. The best way is not to install `dblink` and the contrib modules in this database;
- no `USAGE` privilege on a foreign data wrapper or a foreign server (R8);
- no object that the role owns, and none that a role of its memberships owns: no table, schema, function, type, database or large object (R9).

The role SHOULD also have the settings below. They are the closure of the advice A1 to A4. The `CONNECTION LIMIT` is `max_concurrent` plus 2 or more (8a.9). The SQL below is an example:

```sql
CREATE ROLE hune_ro LOGIN
    PASSWORD '<the password of the secret store>'
    NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS
    CONNECTION LIMIT 18;

ALTER ROLE hune_ro SET default_transaction_read_only = on;
ALTER ROLE hune_ro SET statement_timeout = '60s';
ALTER ROLE hune_ro SET idle_in_transaction_session_timeout = '15s';
ALTER ROLE hune_ro SET temp_file_limit = '1GB';
ALTER ROLE hune_ro SET jit = off;

GRANT CONNECT ON DATABASE analytics TO hune_ro;
GRANT USAGE ON SCHEMA metrics TO hune_ro;
GRANT SELECT ON ALL TABLES IN SCHEMA metrics TO hune_ro;
ALTER DEFAULT PRIVILEGES FOR ROLE metrics_owner IN SCHEMA metrics GRANT SELECT ON TABLES TO hune_ro;

REVOKE CREATE ON SCHEMA public FROM PUBLIC;
REVOKE USAGE ON LANGUAGE plpgsql FROM PUBLIC;
```

Only a superuser can set `temp_file_limit`, so the role cannot lift it. The other `ALTER ROLE` lines give defaults that the role can change. The last line makes `DO` blocks impossible for a role that has no direct `USAGE` grant. It does not change the functions that already use the language.

The role MUST NOT run five functions: `pg_terminate_backend`, `pg_cancel_backend`, `pg_logical_emit_message`, `ts_stat` and `ts_rewrite`. The probe refuses a role that can run one of them (R7). `PUBLIC` can run all five by default.

The statement below writes the commands that close them, with the other refused functions. The operator MUST run its output as the owner in each database that Hune reads. The function `set_config` is not in the list, because `pg_dump` needs it:

```sql
SELECT format('REVOKE EXECUTE ON FUNCTION %s FROM PUBLIC;', p.oid::regprocedure)
FROM pg_proc p
WHERE p.pronamespace = 'pg_catalog'::regnamespace
  AND (p.proname IN ('pg_sleep', 'pg_sleep_for', 'pg_sleep_until', 'pg_cancel_backend',
                     'pg_terminate_backend', 'pg_notify', 'pg_logical_emit_message', 'ts_stat', 'ts_rewrite')
       OR p.proname LIKE 'pg\_advisory\_%' OR p.proname LIKE 'pg\_try\_advisory\_%'
       OR p.proname LIKE 'query\_to\_xml%' OR p.proname LIKE 'cursor\_to\_xml%');
```

A new function with the same name in another schema, for example `crosstab` of `tablefunc`, is not in this list. The probe finds it (R7). The operator revokes it by name.

The operator SHOULD point the source at a read replica when the data permits it. A replica refuses each write, whatever the role holds.

The block of the configuration names this role (13.12a). The user name comes from exactly one of `user`, `user_env` and `user_file`. The password comes from exactly one of `password_env` and `password_file`, never from the file:

```hcl
postgres "metrics" {
  user          = "hune_ro"                           # or user_env, or user_file
  password_file = "/run/secrets/postgres-metrics/password"   # or password_env
  database      = "analytics"
  search_path   = "metrics"
}
```

The example shows only the keys of the account. Section 13 gives the complete block (13.12a).

The table gives each hazard, what the client does, and what closes it on the server. The client column is a defence in depth. The last column is the boundary.

| Hazard | The client | The server |
|---|---|---|
| Write of data (`INSERT`, `UPDATE`, `DELETE`, `MERGE`, `TRUNCATE`, `COPY FROM`) | First word, refused words (7.25); `default_transaction_read_only`; `BEGIN READ ONLY` | `SELECT` grants only (R4 to R6) |
| Change of the schema (`CREATE`, `ALTER`, `DROP`) | First word; read-only transaction | No `CREATE` privilege (R3); the role owns no object (R9) |
| Code on the server (`DO`, `CALL`, `CREATE FUNCTION`, `COPY PROGRAM`) | First word | No attribute and no membership of R1, R2, no `USAGE` on a language with `DO` (A1) |
| Files of the server (`COPY` to a file, `pg_read_file`, `lo_import`, `pg_ls_dir`) | First word; refused functions | No membership of `pg_read_server_files` and `pg_write_server_files` (R2); no `EXECUTE` (R7) |
| Another server (`dblink`, a foreign table, `COPY PROGRAM`) | Refused functions | No `dblink` function (R7); no `USAGE` on a wrapper or a server (R8). A foreign table that the role can read is the choice of the administrator. |
| Time (`pg_sleep`, a heavy query) | Refused functions; deadline; cancel request | `statement_timeout` (A3 for `pg_sleep`). The client cannot make heavy SQL impossible. |
| SQL text in a string (`ts_stat`, `ts_rewrite`, `crosstab`, `connectby`, `xpath_table`) | Refused functions | `EXECUTE` revoked (R7). The functions run the text with the rights of the role, so a refused function can hide inside. |
| Writes to the log of the server, also in a read-only transaction (`pg_logical_emit_message`) | Refused function | `EXECUTE` revoked (R7) |
| Memory and disk of the server | Cap on rows and bytes | `temp_file_limit`, `work_mem` (A4) |
| Session state (`SET`, `set_config`, `RESET`) | First word; refused function; `DISCARD ALL` after each query; setup check | The parameters that only a superuser can set are not changeable by the role |
| Signals to other sessions (`pg_cancel_backend`, `pg_terminate_backend`) | Refused functions | No membership of `pg_signal_backend` (R2). The sessions of the same role need no right, and `PUBLIC` can run both functions: `EXECUTE` revoked (R7). |
| Locks and notifications (`LOCK`, `FOR UPDATE`, `pg_advisory_lock`, `NOTIFY`, `pg_notify`) | First word; refused words and functions; read-only transaction | `EXECUTE` revoked (A3) |
| Capacity of connections | `max_concurrent`, admission | `CONNECTION LIMIT` of the role |

The test of 16.9a logs in as `hune_ro` with a plain client, without the startup parameters of 8a.4, and checks the role alone. In a transaction opened with `BEGIN READ WRITE`, each statement below MUST fail with the SQLSTATE that the comment gives:

```sql
INSERT INTO metrics.node_load DEFAULT VALUES;     -- 42501
UPDATE metrics.node_load SET load = 0;            -- 42501
DELETE FROM metrics.node_load;                    -- 42501
TRUNCATE metrics.node_load;                       -- 42501
CREATE TABLE metrics.x (a int);                   -- 42501
CREATE EXTENSION dblink;                          -- 42501
COPY metrics.node_load TO PROGRAM 'id';           -- 42501
SELECT pg_read_file('/etc/passwd');               -- 42501
SELECT lo_import('/etc/passwd');                  -- 42501
DO $$ BEGIN NULL; END $$;                         -- 42501 after the revoke of A1
SELECT pg_terminate_backend(:other_pid);          -- 42501 after the revoke of R7, also for a session of the same role
SELECT pg_logical_emit_message(false, 'p', 'x');  -- 42501 after the revoke of R7
SELECT * FROM ts_stat('SELECT 1::text::tsvector'); -- 42501 after the revoke of R7
```

The same test runs the probe against this role and expects the state `up`. It then runs the probe against the deviant roles, one for each check, and expects `misconfigured`. Three of the deviant roles are special. One role can run a function of R7 only through `PUBLIC`. One role is a member without `INHERIT` of a plain role that can write. One role owns a table.

### 8a.14 Supported PostgreSQL versions

- Hune supports PostgreSQL 15 to 18, the versions of the store (5.31). The CI tests the oldest and the newest of them (16.9a).
- The probe refuses a server below version 15. The check R0 fails and the source is `misconfigured`. The check runs before the other checks, at the setup of each connection. A server before 14 also refuses the startup parameter `client_connection_check_interval`.
- The client uses no feature of one version only. The `transaction_timeout` setting of PostgreSQL 17 is not used, because the client deadline gives the same bound on each version.
- A server on a platform that does not support `client_connection_check_interval` refuses the startup value. Its source is `misconfigured`.

### 8a.15 Known limits

- Hune reads the messages of the server through a size limit. A message that is longer than `max_result_bytes` plus 1 MiB fails the stream at its header, before Hune reads the body. The query gets `422 query_limit`, and Hune closes the connection. PostgreSQL permits a field of 1 GiB, so without this limit one row costs 1 GiB.
- A row that is smaller than this limit but larger than `max_result_bytes` is read in full before Hune checks the count. The memory budget holds `2 x max_result_bytes` for each query, and this row costs 1 MiB more at most. The author limits the columns, for example with `left(col, 1000)`. The cap on rows and bytes stops the second row.
- `statement_timeout` starts again at each message of the protocol. The client deadline limits the whole query (8a.7).
- A function that the role can execute and that an administrator wrote is trusted. A `SECURITY DEFINER` function of a superuser is outside this model.
- A foreign table that the role can read can make the database server connect to another server. Hune cannot see it.
- Hune sets one role for all viewers. It filters no row by subject (12.18). Row-level security cannot tell the viewers apart.
- A PostgreSQL error message can hold a value of the data, for example a message about a bad input text. The page shows the first line of the message (6.7). The log never holds it (12.15).
- A standby can stop a long query with the SQLSTATE 40001 (a conflict with recovery). The next refresh runs the query again.

## 8b. MongoDB data source

### 8b.1 Scope and status

The kind `mongodb` is the third kind of data source. It follows the steps of 8.14. This section gives the client, the limits, the probe and the account of the kind. Section 7 gives its query language (7.27 to 7.36) and its frame types (7.16c, 7.16d). Section 13 gives its keys (13.12b).

- The document format does not change. A panel or a variable of a MongoDB source has the same member `data.query` or `query` as the other kinds (4.15). The member `source` holds the id of a `mongodb` block. There is no new `format_version` (4.42).
- A MongoDB source reads data. It never writes data, never runs code on the server, and never opens a connection to a host that the block does not list.
- This section is the design for the milestone MMG (18.10b). The security choices of this section are open questions (19.33). Each open question has the default that this section states.
- The code uses the official driver `mongodb` only behind a narrow interface (8b.16). The crates are `mongodb` 3.10.0 and `bson` 3.1.0.

### 8b.2 Declaration and code

Each source is a labelled block `mongodb "<id>" { ... }` of the configuration (13.12b). The file has 1 to 64 source blocks in all, of the kinds `clickhouse`, `postgres` and `mongodb`. An id is unique across the three kinds (13.14, C15). The id is the value of the member `source` in a document and the ProcurAuth node `source:<id>` (11.15).

The code follows the rules of 8.1 and 8.14:

- The crate `hune-source` gets the variant `AnySource::Mongo(MongoSource)`, the variant `PreparedQuery::MongoQuery { command }` and the value `SourceKind::MongoDb`. The enum stays an enum, with no `dyn` (R8).
- The crate `hune-core` gets the query language (7.27 to 7.36), the value model of 8b.16 and the frame types of 7.16c and 7.16d. It does no I/O and it uses no MongoDB crate.
- The crate `hune-source` holds the interface `MongoClient` (8b.16). It also holds the logic of the source: the steps of 8b.5, the limits, the error map, the probe and the pool. This logic is generic over the interface. It uses no MongoDB crate.
- The driver is in one module, `crates/hune-source/src/mongo/driver.rs`, behind the cargo feature `mongo-driver`. This module is the only file that names the crates `mongodb` and `bson`. A build with `--no-default-features` has no driver. It refuses a `mongodb` block at start with the error `mongodb "<id>": this build has no MongoDB driver`. The unit tests use a fake client.
- The crate `mongodb` is a local copy of the release 3.10.0 in `vendor/mongodb`. The root `Cargo.toml` and `fuzz/Cargo.toml` select it with `[patch.crates-io]`. The copy reads no environment variable. It also does not read the file `/.dockerenv`. The original reads them to fill the metadata of `hello` (8b.15). Each edit holds a comment `// hune:`.
- The file `vendor/mongodb.patch` holds the edits as a readable diff against the original. The file `vendor/mongodb.lock.json` holds the checksum of the original crate and the SHA-256 of each file. The command `tools/check-vendor-lock.sh --mongodb` finds a changed file, a file that is not in the lock and a patch that does not match (16.18).
- The crate `hune` has the cargo feature `mongo-driver`, and it is on by default. It turns on the feature `mongo-driver` of `hune-source`. The release build has the driver.
- The trait `DataSource` has no method that writes data. The interface has no function that sends a command document of a caller (8b.5).

### 8b.3 The security model

The read-only rule holds on three sides. The user cannot write. The connection sends only read commands. The text of the query can only be a read. The table gives the ten rules. Each rule has a test in 16.9b.

| # | Rule | Where |
|---|---|---|
| 1 | The configuration is closed and strict. A secret comes only from `_env` or `_file`. No environment variable has an effect. | 13.2, 13.5, 13.12b |
| 2 | The user is read-only by construction: the built-in role `read` on the named databases and no other role. The probe reads the roles and the privileges with `connectionStatus` and refuses a user that can do more. | 8b.12, 8b.13 |
| 3 | The connection sends only the commands of the closed list of 8b.5. The interface has no method that writes, and no method that takes a command from a caller. A wire guard refuses each write key before the encode. | 8b.5, 8b.16 |
| 4 | The read preference of the block applies to each command. The read concern is `local`. No write concern exists. | 8b.4, 8b.5 |
| 5 | The query is a validated JSON tree: a closed allowlist of stages and operators, and a refused list that includes each write, each JavaScript form and each other server. | 7.28 to 7.33 |
| 6 | A value of a variable becomes a typed leaf of the tree. It never becomes a key, an operator, a field path or a collection name. | 7.34, 7.35 |
| 7 | The result has a hard cap on rows and on bytes, and the query has a time limit on the server and a deadline of the client with a kill of the cursor. | 8b.6, 8b.7 |
| 8 | No code runs on the server and no other server is reached, as far as a client can make sure. The role and the server settings close the rest. | 7.32, 8b.13, 8b.15 |
| 9 | The link uses TLS with the configured CA file and a name check, and the login uses SCRAM-SHA-256. No key skips a check. No SRV lookup exists. | 8b.4 |
| 10 | No query text, value, row or secret goes to a log, a metric or a span. A log line holds the server error code and the SHA-256 of the expanded query only. | 12.15 |

### 8b.4 Connection and session

- Hune reads no environment variable, no `mongorc` file and no connection string of a file. It builds the options of the driver in code, from the keys of the block only (13.23). It never builds a connection string, so a value of a key cannot add an option.
- The scheme `mongodb+srv` does not exist. Hune makes no SRV lookup and no TXT lookup.
- With `topology = "direct"`, the connection goes to the `host` and the `port` of the block, with the option `directConnection`. The driver makes no discovery and it follows no other host. A DNS name resolves at each new connection.
- With `topology = "replica_set"`, the connection goes to the hosts of `hosts`, with the replica set name of `replica_set`. These hosts MUST be all the members of the set, because the driver connects to each member that the set announces. The probe refuses a set that announces another host (8b.12, check M4).
- TLS with `tls_mode = "verify-full"` uses TLS 1.2 or TLS 1.3 by `rustls`. The trust roots are only the certificates of `tls.ca_file`. The driver checks the certificate against the host name of the address that it connects to. No key sets another name, and no key skips a check. Hune sends no client certificate.
- With `tls_mode = "disable"`, the connection has no TLS. Hune cannot refuse a weak method then. Section 13 gives the warning for a host that is not a loopback address.
- The login uses the mechanism SCRAM-SHA-256 and the database `auth_database`. Hune never offers another mechanism, so a server cannot make it send a password in clear text.
- The driver uses no compression and no proxy. It sets `appName` to `hune`, `retryReads` to `false` and `retryWrites` to `false`. The features of the crate for SRV, compression, a proxy and a system store of CA certificates are off. The code for them is not in the build.
- With `tls_mode = "verify-full"`, Hune gives the driver the path of the CA file and nothing else. The driver then loads no other root. A file that no longer holds the bytes that the start read is refused. The module of the driver reads no other root: the trust of the link has no other source.
- With `topology = "replica_set"`, Hune does not give the set name to the driver. The probe reads the name from `hello` and compares it with the key (check M4). A member of another set thus gives the reason M4, and not a timeout of the server selection.
- The connection setup, TLS and login must end within `connect_timeout`. This value is also the server selection timeout. Past it, the query gives `503 source_unavailable`.
- TCP keepalive is on.

### 8b.5 The commands

The driver sends commands of two kinds. The driver sends the first kind to do its work of connection, and Hune never builds them. The second kind is built by Hune from the query (7.29). No other command exists in the code. Each command has an entry in this closed table. A test with a proxy that logs each command checks it (MS09).

| Command | Sent by | Use |
|---|---|---|
| `hello`, `isMaster` | The driver | The handshake and the monitor of each member. |
| `saslStart`, `saslContinue` | The driver | The login with SCRAM-SHA-256. |
| `buildInfo` | Hune | The version, in the probe. |
| `connectionStatus` with `showPrivileges: true` | Hune | The roles and the privileges of the user, in the probe. |
| `listCollections` with `nameOnly: true` and `authorizedCollections: true` | Hune | The advice of 8b.12. |
| `aggregate` | Hune | A query of the member `aggregate`. |
| `find` | Hune | A query of the member `find`. |
| `getMore` | The driver, for Hune | The next batch of a cursor. It has `collection`, `batchSize` and `comment`, and no `maxTimeMS`: a server refuses this member for a cursor that is not tailable. |
| `killCursors` | Hune | The stop of a cursor (8b.7). |
| `endSessions` | The driver | The end of the session of a query. |

The command `aggregate` has these members, in this order. The `<..>` parts come from the query and from the limits.

```json
{ "aggregate": "<collection>", "pipeline": [ <stages>, { "$limit": <R + 1> } ],
  "cursor": { "batchSize": <min(1000, R + 1)> }, "allowDiskUse": false, "maxTimeMS": <ms>,
  "comment": { <attribution> }, "readConcern": { "level": "local" }, "$db": "<database>",
  "$readPreference": { "mode": "<read_preference>" } }
```

The command `find` has `filter`, `projection`, `sort`, `skip`, `limit` (the lower of the query limit and `R + 1`), `batchSize`, `allowDiskUse: false`, `maxTimeMS`, `comment`, `readConcern`, `$db` and `$readPreference`. The driver adds the session and the cluster time members.

- Hune sends no `insert`, `update`, `delete`, `findAndModify`, `create`, `drop`, `createIndexes`, `createUser`, `eval`, `mapReduce`, `distinct`, `count`, `explain`, `currentOp`, `killOp`, `serverStatus` or `getParameter`. The interface of 8b.16 has no method that can build one.
- The driver function `run_command` takes a command from a document. One module uses it, and it takes a value of the closed enum of 8b.16. A policy check searches the sources for it (16.18).
- The wire guard is a function of the driver module. It runs on each `ReadRequest` before the encode, and it walks the whole value tree.
  - It refuses a request that holds one of the keys `$out`, `$merge`, `$function`, `$accumulator`, `$where` and `$code` at any depth.
  - It also refuses a database or a collection that the lists of the block do not hold. The lists are arguments of the function.
  - It does not depend on the grammar of 7.31, so a fault of the grammar cannot send a write.
  - A refusal is an internal error: the server logs it and answers `500 internal_error`.
- A query never gets a retry after Hune sent it. The driver option `retryReads` is `false`.
- A reused connection can fail before the first byte of the command goes out. Hune then opens a new connection and starts again one time. After that, it never sends a command again. The driver module does this retry. The source logic never sends a command again.
- The text of an author is never a member name of the command. It is a value in the pipeline, after the validation of 7.35.

The steps of one query:

1. It passes the admission of 8.8 (8b.10).
2. It takes a connection from the pool (8b.9).
3. It starts the deadline of 8b.7.
4. It sends `aggregate` or `find`. The first reply holds the first batch and a cursor id.
5. It sends `getMore` until the cursor id is 0, or until a cap stops the query (8b.6).
6. It converts each document to the Hune value model as the batch arrives (8b.16), and it updates the counts of 8b.6.
7. At the end of the cursor, it makes the frame (7.16, 7.16c) and ends the session.
8. It releases the permits.

### 8b.6 The answer and the limits

| Limit | Rule | Result |
|---|---|---|
| Rows | The effective `max_result_rows` (7.2, step 13), named `R`. Hune adds the stage `$limit` of `R + 1`, so the server stops after one extra row. The extra row is the overflow. | `422 query_limit` with `limit` = `max_result_rows` and `value` = the effective rows. With `rows_from_ceiling`, `403 ceiling_exceeded` (8b.8). |
| Bytes | The key `max_result_bytes`. Hune counts for each value the length in bytes of its compact JSON text (7.16), plus 1. It checks the count after each document. | `422 query_limit` with `limit` = `max_result_bytes` and `value` = the key. |
| Time | 8b.7. | 8b.8. |
| Command size | The key `max_query_size`, on the BSON size of the command (7.35, step 7). | `422 sql_too_large` |
| Columns | `MAX_FRAME_COLUMNS` (256) (7.16c). | `422 query_error` |
| One document | A document is at most 16 MiB (the server limit). A reply holds at most 16 MiB of documents. | 8b.15 |

Hune never cuts a result. A result above a cap is an error, and the browser gets no row of it. When a cap stops the query, Hune kills the cursor (8b.7) and closes the connection.

One query keeps at most `max_result_bytes` of frame in memory, plus one reply of the server. The results in flight of one source take at most `max_concurrent × 2 × max_result_bytes`. The admission reserves this memory in the budget `server.max_result_memory_bytes` (8.8). 8b.15 gives the limit of one reply.

### 8b.7 Time limits and cancellation

MongoDB can apply `maxTimeMS` to the whole cursor or to each reply, by the release. Hune does not rely on one of the two. The client deadline is the bound of the whole query.

- The deadline starts at step 3 of 8b.5. It is the effective `max_execution_time` plus 5 s. At the deadline, Hune kills the cursor, drops the connection, and answers as 8b.8 says.
- The server value `maxTimeMS` is the effective `max_execution_time` in milliseconds. For a query that the first reply ends, the server stops first, with the code 50.
- A kill is the command `killCursors` with the cursor id that the first reply gave. Hune sends it on a new short connection. It needs no privilege beyond `find`, because the user owns the cursor. A lost kill is not an error, because Hune closes the connection anyway.
- One kill runs at a time for each source. The wait for the turn and the kill share the time limit `connect_timeout`.
- Hune never sends `killOp`. The user has no right for it (8b.13).
- The browser aborts a query when the time range, a variable or the page changes (9.8). Hune MUST find the close of the client connection while it waits for the server. It MUST then kill the cursor at once and close the connection.
- MongoDB does not stop a command when the client connection closes. Hune cannot kill a command whose first reply did not come. Such a command goes on until `maxTimeMS`. The query MUST leave `currentOp` within `maxTimeMS` plus 2 s after the client closes its connection. When a cursor exists, it MUST leave within 2 s. The test MS05 proves both.
- A connection that carried a cancelled or dropped query never goes back to the pool.
- At a stop, the queries in flight continue for the time of 14.13. Then Hune kills the cursors, closes the connections, and the requests get `503 server_stopping`.
- The log line of a query that the client left has the outcome `cancelled` and the status 499 (14.7).

### 8b.8 Error mapping

Hune maps an error by the code of the server, never by the message. A code is the integer member `code` of the error answer. The member `message` of `query_error` is the first line of `errmsg`, cut to 1,000 bytes (6.7).

| Code or event | Hune code | Members |
|---|---|---|
| 50 `MaxTimeMSExpired`, 262 `ExceededTimeLimit`; the client deadline (8b.7) | `504 query_timeout` | `source`, `query_id` |
| The same, when `time_from_ceiling` is true | `403 ceiling_exceeded` | `ceiling` = `max_duration`, `limit` = the effective seconds |
| The row check of 8b.6 | `422 query_limit` | `limit` = `max_result_rows`, `value` = the effective rows |
| The row check, when `rows_from_ceiling` is true | `403 ceiling_exceeded` | `ceiling` = `max_rows`, `limit` = the effective rows |
| The byte check of 8b.6 | `422 query_limit` | `limit` = `max_result_bytes`, `value` = the key |
| 292 `QueryExceededMemoryLimitNoDiskUseAllowed`, 146 `ExceededMemoryLimit` | `422 query_memory_limit` | none |
| 13 `Unauthorized` | `502 source_access_denied` | `source`, `query_id` |
| 18 `AuthenticationFailed`, 11 `UserNotFound`, 334 `MechanismUnavailable`, 31 `RoleNotFound`; a failed certificate check (8b.4); a failed probe check of 8b.12 | `502 source_misconfigured` | `source` |
| 6 `HostUnreachable`, 7 `HostNotFound`, 89 `NetworkTimeout`, 91 `ShutdownInProgress`, 133 `FailedToSatisfyReadPreference`, 189 `PrimarySteppedDown`, 202, 9001 `SocketException`, 10107 `NotWritablePrimary`, 11600, 11602, 13435, 13436; a refused, reset or timed-out connection; a server selection timeout; a TLS fault other than the certificate check | `503 source_unavailable` | `source` |
| 11601 `Interrupted`, 237 `CursorKilled`, when Hune sent the kill for a client that left | no answer: the client left (8b.7) | none |
| A value that does not decode (7.16c) | `502 source_bad_response` | `source`, `query_id` |
| A column of a type that 7.16c does not allow, a mix of types, too many columns | `422 query_error` | `source`, `mongo_code` = `null`, `query_id` |
| Any other code, for example 2 `BadValue`, 9 `FailedToParse`, 40324 `Location40324`, 168 `InvalidPipelineOperator`, 26 `NamespaceNotFound`, 43 `CursorNotFound` | `422 query_error` | `source`, `mongo_code`, `query_id` |

- A query that the client left gives no answer (8b.7). Hune knows this case only from its own kill for a client that left. A code 11601 from another cause gives `422 query_error`.
- A fault of the login or of TLS that comes before any error answer gives `503 source_unavailable`. A failed certificate check is an exception: it gives `502 source_misconfigured`.
- The messages of `source_misconfigured` and `source_unavailable` are fixed English texts, with the code when there is one. They never hold text of the server, because the log lines `source.down` and `source.misconfigured` carry them (12.15). The messages of `query_error`, `source_access_denied` and `query_memory_limit` hold the first line of `errmsg`, cut to 1,000 bytes.
- The member `limit` of `query_limit` has two values for this kind: `max_result_rows` and `max_result_bytes`.
- The members of `ceiling_exceeded` are `action`, `ceiling` and `limit` (6.8, 11.33). A ceiling of 0 gives `403 ceiling_exceeded` before Hune sends the query, as in 8.5.
- A `query_error` of a source of this kind has the member `mongo_code` and no member `clickhouse_code` and no member `sqlstate`.
- The code 168 or 40324 shows that the server does not know an operator of 7.31. It means that the server is older than the release that the operator needs (8b.14).
- `source_misconfigured` and `source_unavailable` change the state of the source (8b.12).

### 8b.9 Connection pool

- Each source has one driver client with a pool of at most `max_concurrent + 1` connections. The extra connection is for the probe. The driver also keeps one monitor connection for each member that it knows. A source thus holds up to `max_concurrent + 1 + n` connections, with `n` the number of members. The operator sets the connection limit of the server with this number in mind.
- The pool closes an idle connection after 60 s. The minimum pool size is 0.
- A new connection does the TCP connect, the TLS handshake, the handshake of the protocol and the login within `connect_timeout`.
- A connection never goes back to the pool in the cases of 8b.5 and 8b.7 that close it.
- Hune supports a direct connection, and a replica set with a list of all its members. A load balancer that hides the members is supported in the mode `direct` only, with one `host`. A proxy that changes the wire protocol is not supported.

### 8b.10 Admission control

The admission of 8.8 applies to each panel query, options query, annotation query and preview of a MongoDB source. It uses the keys `max_concurrent`, `queue_wait` and `max_concurrent_per_subject` of the block. A permit gives a connection, or the right to open one. The probe takes no permit. The metrics of 14.8 apply with the label `source`.

### 8b.11 Attribution

- `query_id` is a UUID version 7 for each query, as in 8.10. The frame, the error members and the log line `query.run` give it.
- The command has the member `comment`. It is a document. MongoDB shows it in the log, in `currentOp` and in the profiler. Its members have the order of 8.10 and 8a.11: `app` (`hune`), `kind` (`panel`, `options`, `annotation`, `preview` or `probe`), `uid` or `folder`, `panel`, `annotation` or `variable`, `query_id`, `request_id` and `subject`. A value is a text, or an integer for `panel` and `annotation`.
- The member `ref` comes after `panel` when the request has a `ref` (6.20).
- The document has 1,024 bytes at most. When it is longer, Hune shortens `subject`. Then it drops `subject`, `request_id`, `variable`, `folder` and `uid`, in this order, until it fits.
- The driver option `appName` is `hune`.
- An operator finds the queries of Hune with `db.currentOp({"command.comment.app": "hune"})`, with a user that has the right.
- The log of Hune never holds the query text, only its SHA-256 (12.15). The hash covers the compact Relaxed Extended JSON text of the command. It leaves out the members that Hune adds: `cursor`, `allowDiskUse`, `maxTimeMS`, `comment`, `readConcern`, `$db`, `$readPreference` and the final `$limit`.

### 8b.12 Probe and state of a source

At start, and then every 30 s, Hune runs a probe on each source. The start does not wait for it, and the readiness does not depend on it (14.4). The probe uses the extra connection of the pool (8b.9) and the rules of 8b.4. The deadline of the whole probe is `connect_timeout` plus 10 s. A probe writes no `query.run` line.

The mandatory checks give the state `misconfigured` when one of them fails. `D` is the list `databases` of the block. The answer of `connectionStatus` with `showPrivileges: true` has the members `authInfo.authenticatedUsers`, `authInfo.authenticatedUserRoles` and `authInfo.authenticatedUserPrivileges`.

| Check | Source | It passes when |
|---|---|---|
| M0 | `buildInfo` and `hello` | The version is a version number. `hello` has `maxWireVersion` and `maxBsonObjectSize` of at least 16,777,216. |
| M1 | `authenticatedUsers` | One user, with the `user` and the `db` equal to the keys `user` and `auth_database`. |
| M2 | `authenticatedUserRoles` | Each entry is the role `read` on a database of `D`. Each database of `D` has this role. |
| M3 | `authenticatedUserPrivileges` | Each `resource` has a `db` of `D`, and has no member `cluster` and no member `anyResource`. Each `actions` list holds only these actions: `changeStream`, `collStats`, `dbHash`, `dbStats`, `find`, `killCursors`, `listCollections`, `listIndexes`, `listSearchIndexes`, `planCacheRead`. |
| M4 | `hello` | In the mode `replica_set`: `setName` equals `replica_set`, and each address of `hosts`, `passives` and `arbiters` is in the key `hosts`. In the mode `direct`: no check. |

Check M2 refuses each other role: `readWrite`, `dbAdmin`, `userAdmin`, `root`, `readAnyDatabase`, `clusterAdmin`, `backup`, `restore` and a role of the administrator. Check M3 refuses each privilege that can write, and each cluster privilege, whatever role gives it. The `read` role of MongoDB 7.0 and 8.0 gives only the actions of the list. When a release adds an action to `read`, the action is added to this list in the same change as the CI image (8b.14).

The probe runs the checks in the order M0, M1, M2, M3, M4. The first failure gives the state and the reason, and the later checks do not run. A user that holds the role `read` and a custom role fails M2, before M3 runs.

The advice checks never change the state. Each one that fails writes the log line `source.advice` one time for each change (14.7). The line holds the check and the first names.

| Check | Source | The advice |
|---|---|---|
| A1 | `listCollections` for each database of `D` | A collection of the key `collections` does not exist in a database. A new collection can come later, so this is advice only. |
| A2 | `listCollections` for each database of `D` | The user can read a collection that the key `collections` does not list. The list is stricter than the role. The operator can keep it so. |

The advice that no check can give is in 8b.13: the setting `security.javascriptEnabled`. The user cannot read it.

| State | When |
|---|---|
| `unknown` | Before the first result of the probe. |
| `up` | The probe succeeded and each mandatory check passed. |
| `misconfigured` | A mandatory check failed; the login failed; the certificate check failed. |
| `down` | The probe failed for another reason, or a query got `source_unavailable` after the last good probe. |

- A query on an `unknown` source waits for the first probe, within `queue_wait`. One probe runs at a time.
- A query on a `misconfigured` source fails at once with `502 source_misconfigured`. Hune sends nothing to the server until a probe succeeds. The pool closes its idle connections at the change.
- A query on a `down` source still goes to the server. A success sets the state `up`.
- A change of state writes the log line `source.up`, `source.down` or `source.misconfigured` and sets the metric `hune_source_up` (14.7, 14.8). The line `source.misconfigured` has the check, for example `M3`, and the first names in the field `reason`.
- A version below the oldest supported release (8b.14) writes the log line `source.version_unsupported` one time for each change. The queries still run.

### 8b.13 Account contract

The user of the source is the real security boundary of this kind. This paragraph is the only contract of the user, and 12.20 refers to it. The user MUST have:

- a dedicated login, which is not an administrator and not a user of another application;
- the built-in role `read` on each database of the key `databases`, and no other role (M2);
- the mechanism SCRAM-SHA-256 only;
- no privilege on the resource `cluster`, and none on `anyResource` (M3);
- no write action. This excludes `insert`, `update`, `remove`, `createCollection`, `dropCollection`, `createIndex`, `createUser`, `grantRole`, `applyOps`, `internal` and `anyAction`. It also excludes each action that the list of M3 does not hold.

The server SHOULD also have these settings. Hune cannot check them, because the user has no right to read them:

- `security.authorization: enabled`. Without it, the server asks for no login, and the roles mean nothing.
- `security.javascriptEnabled: false` (the option `--noscripting`). This stops `$where`, `$function`, `$accumulator` and `mapReduce` on the server. Hune refuses them in the grammar (7.32), and this setting is the second line.
- `net.tls.mode: requireTLS`, and the protocols TLS 1.0 and TLS 1.1 disabled.
- A read replica for the reads, with `read_preference = "secondaryPreferred"` or `"secondary"`. A secondary refuses an `insert`, an `update` and a `delete`. Since MongoDB 5.0 it sends the write of `$out` and `$merge` to the primary. The role of the user is then the only defence against them.

Example commands, in `mongosh`, as the owner of the database:

```js
use admin
db.createUser({
  user: "hune_ro",
  pwd: passwordPrompt(),
  mechanisms: ["SCRAM-SHA-256"],
  roles: [ { role: "read", db: "analytics" } ]
})
```

The block of the configuration names this user (13.12b). The user name comes from exactly one of `user`, `user_env` and `user_file`. The password comes from exactly one of `password_env` and `password_file`, never from the file:

```hcl
mongodb "events" {
  topology      = "direct"
  host          = "hune-mongo-ro"
  port          = 27017
  user_file     = "/run/secrets/mongodb-events/username"
  password_file = "/run/secrets/mongodb-events/password"
  auth_database = "admin"
  databases     = ["analytics"]
  collections   = ["node_load", "node_events"]
}
```

The example shows only the keys of the account. Section 13 gives the complete block (13.12b).

The table gives each hazard, what the client does, and what closes it on the server. The client column is a defence in depth. The last column is the boundary.

| Hazard | The client | The server |
|---|---|---|
| Write of data (`insert`, `update`, `delete`, `$out`, `$merge`) | The closed command list (8b.5); the allowlist of stages (7.31); the refused list (7.32) | The role `read` has no write action (M2, M3) |
| Change of the schema (`create`, `drop`, `createIndexes`) | The closed command list | No such action (M3) |
| Code on the server (`$where`, `$function`, `$accumulator`, `mapReduce`) | The grammar refuses each name (7.32) | `security.javascriptEnabled: false` (advice, not checked) |
| Another database or collection (`$lookup`, `$unionWith`, `$graphLookup`) | The lists `databases` and `collections` (7.33); `$graphLookup` refused | The role reads only the databases of `D` (M2, M3) |
| Another server | One `host`, or the listed members (8b.4); no SRV lookup | The members come from the `hello` answer of a server that the operator controls (8b.15) |
| Time (a heavy pipeline, a regular expression) | `maxTimeMS`; the deadline; the kill of the cursor | The server limit of the user, if any |
| Memory of the server | `allowDiskUse: false`; the cap on rows and bytes | The server memory limit of a stage (code 292) |
| Work of other users (`$currentOp`, `$listSessions`) | Refused stages | No `inprog` action (M3) |
| Statistics of the server (`$collStats`, `$indexStats`) | Refused stages | The role `read` allows `collStats` and `dbStats`: they stay readable by a plain client. The data is the size and not the content. |
| Capacity of connections | `max_concurrent`; the admission | The connection limit of the server |

The test of 16.9b logs in as `hune_ro` with a plain client, without the options of 8b.4, and checks the role alone. Each statement below MUST fail with the code that the comment gives:

```js
db.getSiblingDB("analytics").node_load.insertOne({ x: 1 })          // 13
db.getSiblingDB("analytics").node_load.updateOne({}, { $set: { x: 1 } })   // 13
db.getSiblingDB("analytics").node_load.deleteMany({})                // 13
db.getSiblingDB("analytics").node_load.drop()                        // 13
db.getSiblingDB("analytics").createCollection("x")                   // 13
db.getSiblingDB("analytics").node_load.createIndex({ x: 1 })         // 13
db.getSiblingDB("admin").createUser({ user: "x", pwd: "y", roles: [] })   // 13
db.getSiblingDB("analytics").node_load.aggregate([{ $out: "x" }])    // 13
db.getSiblingDB("analytics").node_load.aggregate([{ $merge: { into: "x" } }])   // 13
db.getSiblingDB("admin").aggregate([{ $currentOp: { allUsers: true } }])   // 13
db.getSiblingDB("admin").runCommand({ serverStatus: 1 })             // 13
db.getSiblingDB("admin").runCommand({ getParameter: 1, javascriptEnabled: 1 })   // 13
db.getSiblingDB("other").stuff.find({})                              // 13
```

The same test runs the probe against this user and expects the state `up`. It then runs the probe against deviant users and expects `misconfigured`, with the name of the check in the reason. A user that lives in another database than `auth_database` has no account in `auth_database`, so the login fails before the check M1 runs. The check M1 stays as a second line of defence.

A user with a role other than `read` fails M2. A user that holds `read` and a custom role also fails M2, because M2 runs before M3. The check M3 has its own cases on the plain structures (MS11).

### 8b.14 Supported MongoDB versions

- Hune supports MongoDB 7.0, 8.0 and newer releases, for a standalone server and for a replica set. The CI tests the oldest and the newest of them (16.9b).
- The probe writes `source.version_unsupported` for a version below 7.0. The queries still run, and an operator that the server does not know gives `422 query_error` (8b.8).
- The client uses no feature of one release only. It sends no `serverApi` version.
- The MongoDB version of the target of the migration is not measured yet (19.34).

### 8b.15 Known limits

- The `hello` of the driver holds the metadata of the client. These are the name and version of the driver, the system, the architecture, the compiler and `appName`. It holds no value of the process environment and no name of a platform or a container. The local copy of the crate reads no variable and no file for it (8b.2). The copy never sees a FaaS platform, so the monitoring mode `auto` of the driver always selects the stream protocol. The test of 16.9b makes sure of this.
- A reply of the server can hold 16 MiB of documents. The memory budget does not count this one reply. The author limits the fields, for example with `$project`. The cap on bytes stops the query after the batch.
- MongoDB does not stop a command when the client leaves. A command that has no cursor yet goes on until `maxTimeMS` (8b.7).
- The setting `security.javascriptEnabled` cannot be read by the user. The grammar refuses each JavaScript form. A server that runs with the setting on is still safe from Hune, but a plain client of the same user can run `$where`.
- A regular expression of `$regex` or `$regexMatch` can use much CPU on the server. `maxTimeMS` and the limit of 1,024 bytes bound the cost.
- A `$lookup` and a `$group` can use much memory and time. `allowDiskUse: false` makes the server fail the stage with the code 292. `maxTimeMS` stops the time.
- The role `read` gives the actions `collStats`, `dbStats`, `listIndexes` and `listCollections`. A plain client of the same user can read these values. They give no document.
- In the mode `replica_set`, the driver can open a monitor connection to a member that the primary announces before the probe sees it. The CA file and the name check still apply. The operator lists all members.
- The read concern is `local`. On a secondary, a read can show data that a later rollback removes.
- An error message of the server can hold a value of the data. The page shows the first line of the message (6.7). The log never holds it (12.15).
- Hune sets one user for all viewers. It filters no document by subject (12.18).
- A sharded cluster through one `mongos` host in the mode `direct` works with the same rules. The CI does not test it.
- The schema of a collection can change in time. A column that holds two types that 7.16c does not mix gives `422 query_error` (7.16c).

### 8b.16 The client interface

The interface is the only way from the logic of the source to a MongoDB server. It has a small set of operations, each one a read of 8b.5. It has no function that writes and no function that takes a command document. The code of the logic is generic over the interface. The driver implements it (8b.2). The unit tests implement it with a fake client that returns scripted batches, errors and delays.

```rust
/// A value of the Hune model. The driver module converts a BSON value to this type, one document
/// at a time. The crate `hune-core` has this type and needs no MongoDB crate.
pub enum MongoValue {
    Null, Bool(bool), Int32(i32), Int64(i64), Double(f64), Decimal128(Decimal128Text),
    String(String), DateMs(i64), ObjectId([u8; 12]), Uuid([u8; 16]),
    Array(Vec<MongoValue>), Document(Vec<(String, MongoValue)>),
    Other(OtherBsonType),   // Binary (not subtype 4), Regex, Timestamp, Code, Symbol, MinKey, MaxKey, DbPointer, Undefined
}

/// One read that Hune can send. The enum is closed: no other command exists.
pub enum ReadOp {
    Aggregate { collection: String, pipeline: Vec<MongoValue> },
    Find { collection: String, filter: MongoValue, projection: Option<MongoValue>,
           sort: Option<MongoValue>, skip: u64, limit: u64 },
}

pub struct ReadRequest {
    pub database: String, pub op: ReadOp, pub batch_size: u32, pub max_time_ms: u64,
    pub read_preference: ReadPreference, pub comment: MongoValue,
}

pub struct Batch { pub docs: Vec<Vec<(String, MongoValue)>>, pub cursor: Option<CursorHandle>, pub reply_bytes: u64 }

/// The narrow client of a MongoDB source (8b.5). No method writes data.
pub trait MongoClient: Send + Sync {
    /// Connects, checks TLS and logs in with SCRAM-SHA-256 (8b.4).
    fn connect(options: ConnectOptions) -> impl Future<Output = Result<Self, MongoError>> + Send where Self: Sized;
    /// `hello` and `buildInfo` (8b.12, M0 and M4).
    fn server_info(&self) -> impl Future<Output = Result<ServerInfo, MongoError>> + Send;
    /// `connectionStatus` with `showPrivileges: true` (8b.12, M1 to M3).
    fn connection_status(&self) -> impl Future<Output = Result<AuthInfo, MongoError>> + Send;
    /// `listCollections`, names only (8b.12, A1 and A2).
    fn list_collections(&self, database: &str) -> impl Future<Output = Result<Vec<String>, MongoError>> + Send;
    /// `aggregate` or `find`, with `maxTimeMS`, after the wire guard (8b.5). Gives the first batch.
    fn read(&self, request: ReadRequest) -> impl Future<Output = Result<Batch, MongoError>> + Send;
    /// `getMore` for a cursor that `read` gave.
    fn get_more(&self, cursor: &mut CursorHandle) -> impl Future<Output = Result<Batch, MongoError>> + Send;
    /// `killCursors` for a cursor (8b.7). It never fails the query.
    fn kill_cursor(&self, cursor: CursorHandle) -> impl Future<Output = ()> + Send;
    /// Ends the session of a query (`endSessions`) and closes the link of the query.
    fn finish(&self, cursor: Option<CursorHandle>) -> impl Future<Output = ()> + Send;
}
```

- `MongoError` carries the server code, the code name, the first line of the message and a class: `Server`, `Network`, `Tls`, `Auth`, `Timeout`, `Decode` or `Guard`. The error map of 8b.8 reads the code and the class.
- The interface is the whole surface that the driver module has to give. The code of 8b.5 to 8b.12 has no other call to the network.
- The value model has a constructor from Extended JSON v2 text, for the golden vectors of 16.9b and the unit tests. The fake client reads its batches from such text.
- The probe of 8b.12 reads `ServerInfo` and `AuthInfo`, two plain structures. The role checks M0 to M4 are pure functions of these structures, so a unit test covers them with no server.
- The driver module converts the options of `ConnectOptions` to the options of the driver. It sets nothing that 8b.4 does not name.
