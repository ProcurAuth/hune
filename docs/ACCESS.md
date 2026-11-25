# Access and ProcurAuth

This document uses ASD-STE100 Simplified Technical English.

This document is for the operator of Hune. It tells how to run Hune in the mode `authzen`. Section 11 of the specification is the reference. A reference such as P 9.4 is a paragraph of the ProcurAuth protocol 1.0.

## The two modes

The key `access.mode` is necessary. It has no default value. A file without it stops the start.

| Mode | What Hune does |
|---|---|
| `open` | Hune reads no token and asks no PDP. Each request has the subject `-`. A reverse proxy that authenticates the users MUST guard each route. |
| `authzen` | Hune checks an identity token on each request. It asks a PDP for each operation. It never trusts another header. |

The mode `authzen` has three parts. The edge authenticates the person and signs the token. Hune checks the token. The PDP decides. Hune is the PEP.

## What the operator gives

Before the first start, the operator and Hune exchange these facts.

| Fact | Direction | Key |
|---|---|---|
| The id of the service | Hune to the operator | `access.identity.audience` |
| The origin of the action document | Hune to the operator | The origin of the edge, with the path `/.well-known/procurauth/actions` |
| The name of the token header | Operator to Hune | `access.identity.header` |
| The issuer of the tokens | Operator to Hune | `access.identity.issuer` |
| The public keys of the edge | Operator to Hune | `access.identity.public_keys`, `_env` or `_file` |
| The longest life of a token, and the clock skew | Operator to Hune | `access.identity.max_age`, `access.identity.clock_skew` |
| The URL and the credential of the PDP | Operator to Hune | `access.pdp.url`, `access.pdp.token_env` or `token_file` |

With `server.base_path = "/"`, Hune serves the action document at the root. With another base path, the edge maps `/.well-known/procurauth/` to that path.

## An example

```hcl
access {
  mode = "authzen"

  identity {
    header           = "X-Identity-Token"
    issuer           = "https://edge.example"
    audience         = "dashboards"
    public_keys_file = "/run/secrets/edge/jwks.json"
    max_age          = "5m"
    clock_skew       = "60s"
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
```

Run `hune check-config` to check the file. This command builds the PEP and the action document. It sends nothing on the network. It finds each error that `serve` finds.

## The identity token

Hune reads the token from the header that `identity.header` names. It never reads `Authorization`, a cookie or a query parameter.

A token is valid when it passes each rule of P 9.4 and the subject rule of Hune.

- The header has one copy. The token has 16384 bytes at most.
- `alg` is `Ed25519` or `ES256`. `typ` is `JWT`. The header has no `crit` member.
- The claims `iss`, `sub`, `aud`, `iat` and `exp` are present. `iss` and `aud` match the configuration.
- `iat` and `exp` are whole numbers. `exp` minus `iat` is not more than `max_age`.
- The key set has the `kid` of the token. With one key, the `kid` is optional.
- `sub` has 1 to 256 bytes with no control character. It is not `-`, `external` or `import-grafana`.

A request without a valid token gets `401 unauthenticated`. The body gives no reason. When the request has a body, the answer has `Connection: close`. The log line `access.unauthenticated` gives the reason.

Hune reads the keys one time at start. A change of the keys needs a restart. Use a JWK Set with an explicit `kid` for each key.

## The public keys

`public_keys` holds PEM text or a JWK Set with 1 to 16 keys. Each key is Ed25519 or P-256. A private member, an RSA key or a duplicate `kid` stops the start. A PEM key gets the RFC 7638 thumbprint as its `kid`.

`hune print-config` shows each key as its type and its `kid`.

## The resource tree

Hune gives each operation a node of this tree. The service id is `access.identity.audience`.

```text
service
├── folder     the id of a folder
│   └── dashboard   the uid of a dashboard
└── source     the id of a clickhouse block, a postgres block or a mongodb block
```

A `clickhouse` block, a `postgres` block and a `mongodb` block are each a node `source`. The access layer does not tell the three kinds apart. The PDP never gets the kind of a source. A rule of the PDP names the source id. The page gets the kind from `GET /api/sources`.

A grant on a folder covers each dashboard of the folder, now and later. A grant on the service covers each node. A move changes the chain of a dashboard, so the grants of the new folder apply at once.

A dashboard without a folder has the chain `service`, `folder:_root`, `dashboard:<uid>`. The id `_root` is not a folder. The id rule of a folder (4.3) refuses a name that starts with an underscore, so no folder can have it. The PDP gets one more node of the type `folder`, and the chain keeps its two levels. The type and the actions do not change. The node `_root` is the same for the root of each store, because a uid is unique in all stores.

A grant on `folder:_root` covers each dashboard without a folder, now and later. A grant on a real folder does not cover them. A person who has rights only on real folders does not see a dashboard without a folder. Hune fails closed here. The author of a policy adds a grant on `folder:_root`, or a grant on the service, for each person who needs these dashboards.

Hune builds each chain from its own catalog. It never uses a value of the request.

## The actions

| Action | Resource types | Ceilings | What it permits |
|---|---|---|---|
| `dashboard.read` | service, folder, dashboard | `max_rows`, `max_duration` | List and open dashboards. Run their saved queries. Read their history. |
| `dashboard.write` | service, folder, dashboard | none | Create, change, move, restore and delete dashboards. Change folders. |
| `source.query` | service, source | `max_rows`, `max_duration` | Run a query that is not saved. Save a dashboard whose query uses the source. |

When no store is the primary store, Hune publishes `dashboard.read` only. It asks no question for the other actions.

A store that is not the primary store is read-only. Hune refuses each write into it with `403 read_only_store`, before it asks `dashboard.write`. It asks `dashboard.read` of the target first, so a subject that cannot read the target gets the refusal of any other write. A grant of `dashboard.write` on a resource of a read-only store changes nothing. For such a resource, the actions that matter are `dashboard.read` and `source.query`.

Two facts need care.

- `dashboard.read` gives the result of the stored SQL. This is true for each table that the SQL reads.
- `source.query` gives each read that the account of the source permits. For a `postgres` source, this is the role of the source (section 8a.13 of the specification). A save of SQL is the power to run that SQL with the credentials of Hune.
- A grant of `source.query` gives no write on a `postgres` source. The role cannot write, the session cannot write, and the transaction is read-only.
- For a `mongodb` source, `source.query` gives each read that the account of the source permits (section 8b.13 of the specification). The grammar of the query has no write stage.
- A grant of `source.query` gives no write on a `mongodb` source. The account has the role `read` only, and Hune sends only read commands.

Give each author the same ceilings on `dashboard.read` as on `source.query`. The saved SQL runs later with the ceilings of each viewer.

## The questions of each route

Section 11.21 of the specification has the complete table. The questions are the same for both kinds of source. These are the main rules.

- A read route asks `dashboard.read` on the dashboard.
- A save, a restore or a create asks `source.query` on each source of the document, then `dashboard.write`. The check applies also when the SQL did not change.
- A move asks `dashboard.write` on the dashboard, then `dashboard.write` on the target folder. The target of a move to the root is the node `_root`. A create without a folder asks `dashboard.write` on the node `_root`.
- A query route asks `dashboard.read` and applies the ceilings of the answer.
- A preview asks `dashboard.write` on the dashboard or the folder, then `source.query` on the source.
- `validate_only=true` asks the use question only.
- The pages, the static files and `GET /api/session` need a token and ask nothing.
- The action document needs no token and asks nothing.

Hune asks the other permissions first, in one request to the PDP. A `false` answer gives `403 forbidden`, and Hune does not ask the use.

## An unknown uid

An unknown uid must tell nothing to a subject without the right. Hune asks `dashboard.read` on the service. A `false` answer gives `403`. A `true` answer gives `404`. The same rule applies to an unknown folder id.

Hune asks this question at each request. It never uses a kept answer, and the PEP keeps no answer to it. On a write route, Hune first asks the other permissions of the route that name only items that exist, as for a uid that exists.

- A create and a save first ask `source.query` on each source of the document.
- A move to an unknown folder first asks `dashboard.write` on the dashboard.

The questions of a write route also never use a kept answer. Thus on a write route, a uid that exists and a uid that does not exist send the same number of requests to the PDP.

The `403` for an unknown uid equals the `403` for a uid that exists: the same status, headers and body. The member `action` of the body is the action of the use of the route. It does not name the question that the PDP refused. To find that question, read the log line `access.decision`.

A create of a uid that exists asks `dashboard.read` on that dashboard. A `false` answer gives `403`. A `true` answer gives `409 already_exists`.

## The ceilings

A `true` answer can carry `max_rows` and `max_duration`. Hune applies the lower of the ceiling and the key of the source. The ceilings are the same for the three kinds of source.

- For a `clickhouse` source, Hune sends the values to ClickHouse as `max_result_rows` and `max_execution_time`.
- For a `postgres` source, Hune enforces the values in its own client. It counts the rows and the bytes. It sets the time limit of the query (sections 8a.6 and 8a.7 of the specification).
- For a `mongodb` source, Hune enforces the values in its own client. It counts the rows and the bytes. It sends the time limit as `maxTimeMS` (sections 8b.6 and 8b.7 of the specification).

Hune never cuts a result. A result above a ceiling gives `403 ceiling_exceeded`. The body names the action, the ceiling and the limit. A ceiling of 0 gives the same answer before Hune sends the query. When a ceiling equals the key of the source, the ceiling is the cause.

These answers are refusals.

- A ceiling that the action does not list gives `403 forbidden`.
- A ceiling that is not a whole number, or that is below 1, gives `503 pdp_unavailable`.
- A `false` answer ignores its ceilings.

## The display filter

The page shows each person the items that the person can use. The server checks each operation again. A `subtree` answer never lets an operation run.

- The list of folders keeps the folders with `true` for `dashboard.read`. The member `root` of this list has `access.write`: the display answer for `dashboard.write` on the node `_root`. The node `_root` is not an item of the list.
- The list of dashboards keeps the dashboards of a visible folder when the folder grant covers them. Else Hune asks one question for each dashboard.
- Each folder and each dashboard carries the flag `access.write`. The list of folders carries `access.create`.
- `GET /api/sources` lists the sources with `true` for `source.query`. Each item has the member `kind`: `clickhouse`, `postgres` or `mongodb`. It holds no host, no user and no secret.

One page of dashboards asks 200 questions at most. A page can hold fewer items than `limit`. Only `next: null` marks the end.

## The options route

`GET /.well-known/procurauth/resources` lists the children of a node, page by page. It needs a valid token. It has no action of its own.

| Parameter | Rule |
|---|---|
| `type` | Necessary: `folder`, `source` or `dashboard`. |
| `parent` | Necessary for `dashboard`: `folder%3A<id>`, or `folder%3A_root` for the dashboards without a folder. Absent for the other types. |
| `q` | Optional search text of 256 bytes at most. It matches a substring of the id or of the label. |
| `limit` | Optional, from 1. A value above 50 gives 50. |
| `after` | The value `next` of the previous answer. |

The list of the type `folder` has one more item with the id `_root` and the label "Root (no folder)", as the first item. It is the node of the dashboards without a folder.

Hune asks the published actions for each child with the match `subtree`. It stops at the first `true` answer. A query that is not valid gives `400 invalid_options_request`.

## The answer cache

The PEP keeps the answers of the PDP for `cache_ttl` at most. The key of an answer is the subject, the action, the resource with its chain, and the match. A failure is never kept. The cache holds 16384 entries.

A revoked grant can stay active for `cache_ttl` on each instance. Use `0s` to keep no answer.

The PEP keeps no answer for the questions of a write route, the preview included, and for the question of an unknown uid. Hune asks them with a second PEP that has no cache, on the same link and the same pool. Thus a write route sees a revoked grant at once, and it gets `503` during an outage of the PDP.

Two equal questions that wait together share one call. A page with 45 panels thus asks one question for its first refresh.

## The PDP down, slow or wrong

Hune fails closed. It never sends `context.reason` to the client.

| Case | Answer |
|---|---|
| `true` | The operation runs. |
| `false` | `403 forbidden` |
| No connection, an error, or no answer in `timeout` | `503 pdp_unavailable`, retryable |
| No permit of the pool in `timeout` | `503 pdp_unavailable` |
| An HTTP status other than 200, or a body that is not valid | `503 pdp_unavailable` |

Hune sets `plain_retries = 1` in the PEP. No key of the file changes this value. No action declares a budget, so no question has `context.usage`, and the PEP can send each question again. When a request does not leave Hune, the PEP sends it one more time. This includes no free connection in `timeout`, and a timeout of the connection, of the TLS handshake or of the first write. After a lost connection, or after a second request that does not leave Hune, the PEP sends the question one more time.

Thus the PEP starts three requests at most for one question or one batch of questions. After a timeout that follows the first byte of the request, the PEP does not send the question again. An HTTP status other than 200 and a body that is not valid also get no new request. When the last request fails, the answer is `503 pdp_unavailable`. The PEP also reads the metadata of the PDP at its first question, and again at the first question after 3600 s. This read gets one more request after a lost connection or a request that does not leave Hune.

A kept answer still applies until its `cache_ttl` ends. The action document, the pages and `GET /api/session` still answer during an outage. `/readyz` does not change, because a restart cannot repair the PDP. Hune does not contact the PDP at start.

## The link to the PDP

The link uses `rustls` with the `ring` provider, TLS 1.2 and TLS 1.3. The trust roots are the certificates of `access.pdp.ca_file`. No other root applies. Hune sends no client certificate. It follows no redirect and uses no proxy.

The PEP is synchronous. Hune runs each call in `spawn_blocking`, behind a semaphore of `max_connections` permits. The runtime has `16 + max_connections` threads for blocked work. The longest wait for one PDP request is three times `timeout`: the permit, the connection and the exchange. A new request of the same call keeps the permit, and it has its own connection and exchange.

A call cannot stop early. When the client leaves, the call ends and gives back its permit. Its answer goes to the cache, except the answer of a question that is never kept.

`plain_http = true` accepts an `http` URL. Use it only on a trusted network. Hune writes a warning at start.

## The logs and the metrics

| Event | When |
|---|---|
| `access.unauthenticated` | A token is refused. The field `reason` gives the check. |
| `access.decision` | The PDP answers a question. A kept answer writes no line. |
| `access.pdp_failed` | No answer in time, a connection error, a bad status or no permit. |
| `access.pdp_credential_refused` | The PDP answers 401 or 403. |
| `access.pdp_invalid_answer` | A 200 answer is not valid. |
| `access.ceiling_unenforceable` | A `true` answer has a ceiling that the action does not list. |
| `access.ceiling_refused` | A query is above a ceiling. |
| `access.options` | A request to the options route. |
| `access.no_rule` | A route has no line in the access table. |
| `access.prepass_mismatch` | A create or a save names a source that the first read of the document did not find, so Hune asked no question for it. Hune gives 500 and writes nothing. |

The metrics `hune_pdp_requests_total`, `hune_pdp_duration_seconds`, `hune_pdp_questions_total`, `hune_pdp_cache_hits_total` and `hune_pdp_pool_in_use` describe the PDP traffic. No log line holds a token or the credential of the PDP.

## Checklist before the start

1. Give `server.listen` to the edge only. A wildcard host gives a warning.
2. Keep `server.ops_listen` away from the edge.
3. Put the PDP credential in `token_env` or `token_file`.
4. Give the CA of the PDP in `ca_file`. Keep `plain_http = false`.
5. Run `hune check-config`.
6. Read the action document at `/.well-known/procurauth/actions` through the edge.
7. Run the series U of `docs/CONFORMANCE.md` after each change of the access code.
