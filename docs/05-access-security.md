# Part 5: Access and security

Part 5 of the Hune specification (see `01-overview.md`).

This part has two sections: 11 (access and ProcurAuth) and 12 (security). A reference such as P 9.4 is a paragraph of
the ProcurAuth protocol, version 1.0 (`procurauth/protocol/procurauth-protocol.md`). Section 13 gives the type, the
range and the forms of each configuration key that this part names.

## 11. Access and ProcurAuth

### 11.1 Principle

ProcurAuth is OPTIONAL. The operator selects it at run time with the key `access.mode`. One binary serves both modes.
No Cargo feature removes ProcurAuth, so the test matrix stays one build.

In the mode `authzen`, access has three parts:

1. Authentication. The edge authenticates the person and signs an identity token. The edge puts the token in a request
   header.
2. Check of the token. Hune checks the token on each request. The token gives the subject.
3. Decision. Hune asks a PDP for each operation with the AuthZEN Authorization API 1.0. Hune is the PEP.

In the mode `open`, Hune does none of these parts. A reverse proxy that authenticates the users decides who reaches
Hune (12.5, 12.6).

Hune applies each decision on the server. The page in the browser shows or hides controls (11.25). The page never
decides.

### 11.2 The access block and the two modes

The block `access` is necessary. The key `access.mode` is necessary and has no default value. A file without the block
or without the key stops the start. Thus nobody gets the mode `open` by mistake.

| `access.mode` | Behaviour |
|---|---|
| `open` | Hune reads no token, asks no PDP and trusts no identity header. Each request has the subject `-`. The blocks `identity` and `pdp` stop the start. The two ProcurAuth routes answer 404. |
| `authzen` | Hune checks the identity token of each request and asks the PDP for each operation. The blocks `identity` and `pdp` are necessary. |

The words `open` and `authzen` are the words of ClickHouse Dash. An operator of this product knows them.

### 11.3 Features in each mode

This table is the complete list of the features that `access.mode` changes. A feature that the table does not name is
the same in both modes.

| Feature | `open` | `authzen` | Rule |
|---|---|---|---|
| Check of the identity token | Off. Hune never reads the token header. | On, on each route of `server.listen` except the action document. | 11.8 |
| Questions to the PDP | Off. | On, one or more for each operation. | 11.21 |
| Subject in the history, the audit and the logs | `-` | The claim `sub` of the token. | 14.11 |
| `GET /.well-known/procurauth/actions` | 404 | 200, no token, no question. | 11.18 |
| `GET /.well-known/procurauth/resources` | 404 | 200 with a valid token. | 11.26 |
| Display filter of lists and controls | Off. Each item shows. The edit controls follow the primary store (13.10). | On. | 11.25 |
| Ceilings `max_rows` and `max_duration` | Off. The limits of the source apply alone. | On. | 11.24 |
| Check of `source.query` at a save and at a draft run | Off. | On. | 11.21 |
| Unknown dashboard uid or folder id | 404, no question. | 403 or 404 by the rule of 11.23. | 11.23 |
| Concurrency limit for each subject | Off. The key `max_concurrent_per_subject` is forbidden. | On. The key is necessary. | 13.12 |
| Edits (save, create, delete, move, restore, folder changes) | Permitted when a store is the primary store, and only in this store (11.35). | Permitted when a store is the primary store, only in this store, and the PDP answers `true`. | 11.21 |
| Answers 401 `unauthenticated` and 503 `pdp_unavailable` | Never. | Possible. | 11.30 |
| Log events `access.*` | Never. | Yes. | 14.7 |
| Metrics `hune_pdp_*` | Present, always 0. | Yes. | 14.8 |
| Start warning `config.warning` | Always (the mode itself). | When `access.pdp.plain_http = true` or `server.listen` has a wildcard host. | 13.15 |
| Write guard (CSRF) and security headers | On. | On. | 12.7, 12.8 |
| Symbolic links in the volume | Refused. | Refused. | section 5 |
| Conformance series U | Not run. | Run in CI. | 11.32 |

### 11.4 The block identity

The block `access.identity` gives the values of P 9.4. 13.11 gives the type, the range and the forms of each key.

| Key | Role | Protocol |
|---|---|---|
| `header` | The name of the request header that holds the token. | P 9.4 |
| `issuer` | The value that the claim `iss` must have. | P 9.3, P 9.4 |
| `audience` | The service id: the claim `aud`, the member `server` of the action document, and the root of each chain (11.14). | P 5.2, P 9.4 |
| `public_keys`, `public_keys_env`, `public_keys_file` | The public keys of the edge, as PEM or as a JWK Set (11.9). | P 9.2, P 9.4 |
| `max_age` | The longest life of a token: `exp` minus `iat`. A whole number of seconds. | P 9.4 |
| `clock_skew` | The skew that applies to `iat` and to `exp`. A whole number of seconds. | P 9.4 |

### 11.5 The block pdp

The block `access.pdp` gives the link to the PDP (P 10.0, step 4). 13.11 gives the type, the range and the forms of each
key.

| Key | Role | Protocol |
|---|---|---|
| `url`, `url_env`, `url_file` | The base URL of the PDP. The PEP finds the metadata and the endpoints of the PDP from this URL by the rule of 11.12. | P 10.0 |
| `token_env`, `token_file` | The credential of Hune at the PDP, sent in `Authorization: Bearer`. A secret. | P 10.0 |
| `ca_file` | The CA certificates that check the certificate of the PDP. Only with `plain_http = false`. | P 10.0 |
| `plain_http` | `true` accepts an `http` URL. It must agree with the scheme of `url`. | P 10.0 |
| `timeout` | The longest wait for the connection to the PDP, and the longest wait for one exchange. | P 10.6 |
| `cache_ttl` | The longest life of a kept answer. `0s` keeps no answer. The cache never keeps the questions of a write route and the question of 11.23 (11.27). | P 10.1 |
| `max_connections` | The most connections to the PDP, and the size of the pool of 11.13. | P 10.7 |

A complete example for the mode `authzen`:

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

The complete block for the mode `open`:

```hcl
access {
  mode = "open"
}
```

### 11.6 Start checks of the access block

These items stop the start, before Hune serves. 13.14 gives the exact error text of each item.

- No block `access`, no key `access.mode`, or a value other than `open` or `authzen` (lower case).
- `mode = "open"` with a block `identity` or a block `pdp`.
- `mode = "authzen"` without the block `identity` or the block `pdp`, or without one key of these blocks.
- An `http` URL in `pdp.url` with `plain_http = false`, or an `https` URL with `plain_http = true`.
- `pdp.ca_file` with `plain_http = true`, or no `pdp.ca_file` with `plain_http = false`.
- A `pdp.url` with a user name, a password, a query or a fragment.
- The PDP token in clear text. Give `token_env` or `token_file`.
- Two forms of one key, for example `public_keys` and `public_keys_file`.
- A text in `public_keys` that breaks a rule of 11.9.
- A forbidden name in `identity.header` (11.8).
- An `identity.audience` that is not an id of P 4.1: 1 to 256 bytes of UTF-8 with no control character.
- A `max_age` or a `clock_skew` that is not a whole number of seconds.
- A key `max_concurrent_per_subject` in a `clickhouse` block with `mode = "open"`, or no such key with
  `mode = "authzen"`.
- An error of the `PepConfig` or of the action document. Stage 2 of the checks builds both without network (13.13), so
  `check-config` also finds this error. The error names the key (13.14).

Hune does not contact the PDP at start. The PEP reads the PDP metadata at the first question (11.12). Thus a PDP
outage never stops the start and never causes a loop of restarts.

### 11.7 The facts that the operator and Hune exchange

Before the integration, the operator and Hune exchange the facts of P 10.0.

| Fact | Direction | Key or value |
|---|---|---|
| The id of the service | Hune to the operator | The value of `access.identity.audience`. |
| The origin of the action document | Hune to the operator | The origin of the edge in front of `server.listen`. The document is at `server.base_path` followed by `.well-known/procurauth/actions`. |
| The name of the token header | Operator to Hune | `access.identity.header` |
| The issuer | Operator to Hune | `access.identity.issuer` |
| The public keys of the edge | Operator to Hune | `access.identity.public_keys`, `public_keys_env` or `public_keys_file` |
| The maximum age of a token | Operator to Hune | `access.identity.max_age` |
| The clock skew | Operator to Hune | `access.identity.clock_skew` |
| The URL of the PDP | Operator to Hune | `access.pdp.url`, `url_env` or `url_file` |
| The credential of Hune for the PDP | Operator to Hune | `access.pdp.token_env` or `token_file` |

P 5.1 puts the action document at the root of the origin. With `server.base_path = "/"`, Hune serves it there. With
another base path, the edge maps the path `/.well-known/procurauth/` of the service origin to that base path.

### 11.8 The identity token

`procurauth-pep` checks the token with `Pep::authenticate` (P 9.4). Hune adds one rule (the subject rule below) and
removes no rule.

- The token is in the header that `access.identity.header` names. Hune never reads the token from `Authorization`,
  from a cookie or from a query parameter.
- Two copies of the token header are a refusal.
- The header name follows the token rule of RFC 9110. The names `Authorization`, `Proxy-Authorization`, `Cookie`,
  `Host`, `Content-Type` and `Content-Length` stop the start, in any case.
- The token has 16384 bytes at most.
- The JOSE header member `alg` is exactly `Ed25519` or `ES256`. Hune refuses `EdDSA`, `none`, `HS256` and each other
  name.
- The JOSE header member `typ` is exactly `JWT`. A token without `typ`, or with another value such as `at+jwt`, is
  refused.
- A JOSE header with the member `crit` is refused. A duplicate member in the header or in the claims is refused.
- When the key set holds two or more keys, the token must have a `kid` that the set holds. When the set holds one
  key, a token without `kid` uses that key.
- The claims `iss`, `sub`, `aud`, `iat` and `exp` are necessary. `iss` equals `access.identity.issuer`. `aud` equals
  `access.identity.audience`, or is an array that holds it.
- `iat` and `exp` are whole numbers of seconds. A fraction, an exponent or a sign is refused.
- Hune refuses a token when its clock is later than `exp` plus `clock_skew`. It refuses a token whose `iat` is later
  than its clock plus `clock_skew`. It refuses a token whose `exp` minus `iat` is more than `max_age`.
- The subject rule of Hune: `sub` has 1 to 256 bytes of UTF-8 with no control character. It is not one of the
  reserved texts of the history: `-`, `external` and `import-grafana` (section 5). Another `sub` is refused.
- Hune treats `sub` as opaque text. It sends `sub` to the PDP as the subject id with the type `user`. It sends no
  other claim (P 3.3, P 9.6).
- Hune ignores each unknown claim. It writes `owner` in the access log only. It never uses `owner` in a decision
  (P 9.5).

Two of these rules come from `procurauth-core`, and Hune follows the library for both. `iat` and `exp` must be whole
numbers: the library refuses a fraction, a sign and an exponent. A token must have a `kid` when the key set holds two
or more keys. ClickHouse Dash accepts a fraction, a sign and an exponent in `iat` and `exp`, and rounds the value
down. Issue 1 of the repository `ProcurAuth/procurauth` asks this question of the protocol, and section 19 holds it as an
open item. Its title is "Section 9.4: can iat and exp have a fraction, a sign or an exponent?".

Hune keeps no result of a token check. It checks the token again on each request. P 9.7 permits a cache, but a check
costs less than 0.1 ms.

A request with no valid token gets `401 unauthenticated`. The body gives no reason. When the request has a body, the
answer also has `Connection: close`, so Hune does not read the body. The log line `access.unauthenticated` (14.7) gives
the reason: `missing`, `duplicate_header`, `structure`, `header`, `alg`, `typ`, `kid`, `signature`, `claims`, `iss`,
`sub`, `aud`, `iat`, `exp`, `life` or `subject`.

### 11.9 The public keys

`access.identity.public_keys` holds PEM text or a JWK Set (RFC 7517). These are the rules of ClickHouse Dash. Hune
parses the text itself with these rules. It never gives the text to `KeySet::from_jwks_value` of
`procurauth-core`, which refuses some keys that these rules accept. For each key, Hune writes an SPKI PEM block and
calls `PublicKey::from_spki_pem(pem, Some(kid))`. It gives the list of the keys to `KeySet::from_keys`.

Hune reads the text as a JWK Set when its first character after spaces, tabs and line ends is `{`. Else it reads PEM.

- PEM: 1 to 16 blocks `-----BEGIN PUBLIC KEY-----`. Each key is Ed25519 (for `Ed25519`) or P-256 with an
  uncompressed point (for `ES256`). Outside a block, a line must be empty. Other text, a PEM header, a private key and
  another type of key stop the start. The base64 text of one block has 4096 bytes at most.
- JWK Set: a JSON object with a member `keys`, an array of 1 to 16 keys. A key of type `OKP` has `crv` `Ed25519` and
  `x` (RFC 8037). A key of type `EC` has `crv` `P-256`, `x` and `y` (RFC 7518). The point of a P-256 key must be on
  the curve.
- The member `alg` of a JWK is optional. When it is present, it is `Ed25519` or `EdDSA` for `OKP`, and `ES256` for
  `EC`. A token whose header has `alg` `EdDSA` stays refused (11.8).
- The members `use` and `key_ops`, and each other member that this list does not name, have no effect.
- A JWK with a private member stops the start: `d`, `p`, `q`, `dp`, `dq`, `qi`, `oth` or `k`. A JWK with another
  `kty` or `crv` stops the start. A duplicate member, or data after the JSON object, stops the start.
- The `kid` of a JWK is its member `kid`: 1 to 256 bytes with no control character. A PEM key, or a JWK without `kid`,
  gets the RFC 7638 thumbprint of the key as its `kid`. Two keys with the same `kid` stop the start.

Use a JWK Set with an explicit `kid` for each key. Make the edge put this `kid` in each token. With two or more
keys, Hune refuses a token without `kid` or with another `kid` (11.8). A PEM key has the RFC 7638 thumbprint as its
`kid`, so the edge must then send that thumbprint.

`hune print-config` shows each key as its type and its `kid`, for example
`["Ed25519 kid=kPrK_qmxVWaYVA9wwBF6Iuo3vVzz7TxHCTwXBygrS4k"]` (13.20). Hune reads the keys one time, at start. A
change of the keys needs a restart (14.21).

### 11.10 The PEP library and its dependency

Hune builds its PEP on the reference library of ProcurAuth. It does not copy the rules of P 10 into its own code.

| Item | Rule |
|---|---|
| Crates | `procurauth-pep` and `procurauth-core`, licence Apache-2.0, not on crates.io. |
| Source | The git repository `https://github.com/ProcurAuth/procurauth`. The repository is public, so CI needs no secret. |
| Version | `rev` is a full commit SHA of 40 hexadecimal characters. Both crates use the same `rev`. No `branch`, no `tag`. The commit is chosen at M6. |
| Features | `default-features = false`. The feature `mcp` stays off: Hune has no MCP endpoint. |
| Place | Only the crate `hune` (module `access`) depends on them. The pure crates never do. |
| Lock | `Cargo.lock` records the commit. Each build runs with `--locked`. |
| `cargo deny` | The section `[sources]` permits crates.io and this one git URL, and nothing else (12.17). |
| Conformance | The script `tests/conformance/run.sh` checks out the same commit of the ProcurAuth repository (11.32). It reads the commit from the `rev` of `Cargo.toml`, so no second place holds it. |

The lines in the file `crates/hune/Cargo.toml`:

```toml
procurauth-core = { git = "https://github.com/ProcurAuth/procurauth", rev = "<40 hex>", default-features = false }
procurauth-pep  = { git = "https://github.com/ProcurAuth/procurauth", rev = "<40 hex>", default-features = false }
```

The commit of `rev` is chosen at M6 (section 18). Until then, the figures of 11.32 come from the ProcurAuth commit
`f318e259013fdaa7e841936c2b8a172dc1b7699e` (the tag v1.1.1).

The script `tests/conformance/run.sh` stops when its checkout of ProcurAuth is not at the `rev` of `Cargo.toml`. The CI job
`policy` fails when a `rev` is not 40 hexadecimal characters, or when the two `rev` values differ.

To change the commit, do these steps in one change:

1. Write the new full SHA in both lines of `crates/hune/Cargo.toml`.
2. Run `cargo update -p procurauth-pep -p procurauth-core` in the builder container.
3. Update the committed file `deps.lock.txt` (12.17).
4. Run the series U (11.32). The script `tests/conformance/run.sh` takes the new SHA from `Cargo.toml`. Update the expected numbers.
5. Add one line to `CHANGELOG.md` with the old and the new SHA.

Hune builds the PEP one time at start with these values:

| Field of `procurauth-pep` | Value |
|---|---|
| `PepConfig.service_id` | `access.identity.audience` |
| `PepConfig.token_header` | `access.identity.header` |
| `PepConfig.issuer` | `access.identity.issuer` |
| `PepConfig.trusted_keys` | `KeySet::from_keys` of the keys of 11.9, each built with `PublicKey::from_spki_pem(pem, Some(kid))` |
| `PepConfig.max_token_age_seconds` | `access.identity.max_age`, in seconds |
| `PepConfig.clock_skew_seconds` | `access.identity.clock_skew`, in seconds |
| `PepConfig.cache_ttl` | `access.pdp.cache_ttl` |
| `PepConfig.cache_max_entries` | 16384 |
| `PepConfig.display_max_questions` | 200 |
| `PdpConfig.url` | `access.pdp.url` |
| `PdpConfig.credential` | the PDP token |
| `PdpConfig.allow_plain_http` | `access.pdp.plain_http` |
| `PdpConfig.request_timeout` | `access.pdp.timeout` |
| `PdpConfig.usage_wait_margin` | 1 s (no effect: Hune declares no budget) |
| `PdpConfig.idle_close_after` | 5 s |
| `PdpConfig.max_connections` | `access.pdp.max_connections` |
| `PdpConfig.evaluations_max_bytes` | 32768 (the metadata of the PDP can lower it, P 10.7a) |
| `PdpConfig.evaluations_max_items` | 50 (the metadata of the PDP can lower it, P 10.7a) |
| `PdpConfig.max_response_bytes` | 262144 |
| `PdpConfig.plain_retries` | 1 |
| `PdpConfig.metadata_max_age` | 3600 s |

### 11.11 Offline build

The release build and the image build make a vendor directory with `cargo vendor --locked`. This step has network
access. It writes the directory `vendor/` and a source replacement in `.cargo/config.toml` of the build container. The
next step runs `cargo build --release --locked --offline` in a container with no network. The repository does not
commit the directory `vendor/`.

Thus the ProcurAuth crates come from the pinned commit, and the compile step reads no network. Section 17 gives the
complete release workflow.

### 11.12 The PDP transport

The library is synchronous. It uses `std::net::TcpStream` and has no TLS code. Hune gives its own type that implements
the trait `procurauth_pep::transport::PdpTransport`. These are its rules:

- TCP: `TcpStream::connect_timeout` with TCP_NODELAY. One deadline of `access.pdp.timeout` covers the TCP connection and
  the TLS handshake.
- TLS: the crate `rustls` with the provider `ring`, TLS 1.2 and TLS 1.3 only. The trust roots are only the certificates
  of `access.pdp.ca_file`. The server name is the host of `access.pdp.url`. Hune sends no client certificate.
- HTTP/1.1 only. A `POST` has a `Content-Length`. Hune sends the headers that the library gives (`Authorization`,
  `Content-Type`, `Accept`). It adds `Host`, `X-Request-ID` (14.6) and, when telemetry is on, `traceparent` (14.10).
- The library gives the transport no context of the request. Before each call of the library, the task puts the
  request id and the trace context in a thread-local value of its thread. The transport reads this value for
  `X-Request-ID` and `traceparent`. The task clears the value after the call.
- The answer can use `Content-Length` or chunked encoding. An answer of more than 262144 bytes is the failure
  `TooLarge`.
- No redirect: a status 3xx is an answer with that status. No proxy. The transport reads no environment variable.
- `supports_scheme("https")` is true. `supports_scheme("http")` is true only when `access.pdp.plain_http = true`.
- The transport keeps idle connections, at most `access.pdp.max_connections`. It closes an idle connection after 5 s,
  or earlier when the PDP publishes `procurauth_idle_max_seconds`: after half of that time (P 10.7).
- The deadline of one exchange is the `timeout` that the library gives. It starts at the first byte written.

The transport gives the kind of each failure to the library. The library uses it for P 10.6.

| Event | Failure |
|---|---|
| The TCP connection or the TLS handshake fails, or no byte of the request is written | `NotSent` |
| The deadline ends after the first byte of the request | `Timeout` |
| The connection fails after the first byte of the request | `Lost` |
| The answer is not valid HTTP/1.1 | `Malformed` |
| The answer is larger than 262144 bytes | `TooLarge` |
| The scheme of the URL is not supported | `Unsupported` |

The library reads the metadata of the PDP at the first question, and again after 3600 s. The metadata URL is the
scheme and the host of `access.pdp.url`, then `/.well-known/authzen-configuration`, then the path of `access.pdp.url`. A
PDP that answers 404, 405, 410 or 501 there has no metadata. The library then uses the default endpoints: the path of
`access.pdp.url` followed by `/access/v1/evaluation` and `/access/v1/evaluations`.

| `access.pdp.url` | Metadata URL | Default endpoints |
|---|---|---|
| `https://pdp:4182` | `https://pdp:4182/.well-known/authzen-configuration` | `https://pdp:4182/access/v1/evaluation`, `https://pdp:4182/access/v1/evaluations` |
| `https://pdp:4182/pdp` | `https://pdp:4182/.well-known/authzen-configuration/pdp` | `https://pdp:4182/pdp/access/v1/evaluation`, `https://pdp:4182/pdp/access/v1/evaluations` |
 When a new read fails, the old metadata stays, and the next
read comes after 5 s. A first read that fails makes the question fail (11.28).

### 11.13 The bounded blocking pool

The calls of the library block the thread. Hune runs them out of the async workers:

- Each call that can send a request to the PDP runs in `tokio::task::spawn_blocking`. These calls are `Pep::decide`,
  `Pep::check`, `Pep::display` and the options walk of 11.26.
- Before the call, the task takes one permit of a semaphore with `access.pdp.max_connections` permits.
- The task waits for a permit for `access.pdp.timeout` at most. When no permit comes in this time, the operation gets
  `503 pdp_unavailable`. The log line `access.pdp_failed` has `error = "pool_busy"`.
- `Pep::authenticate` sends no request. It runs on the async worker.
- A call of the library cannot stop early. Each call runs in its own task. When the client closes the connection, the
  call continues to its end, and it gives back its permit. Its answer goes to the cache, except the answer of a question
  that is never kept (11.27).
- The metric `hune_pdp_pool_in_use` gives the number of permits in use (14.8).

The longest wait of one PDP request is thus three times `access.pdp.timeout`: the permit, the connection and the
exchange. An operation with other permissions sends at most two PDP requests in sequence (11.21). A restore sends at
most three: the use alone before the store read, then the others, then the use (11.21).

One call of the library can send more than one PDP request under one permit. A batch above the maxima of the PDP
becomes several requests (P 10.7a). The first question of each PEP of 11.27 also reads the metadata. Each request has its
own deadline. A list uses the two calls of 11.25. With the default maximum of 50 items in each request, these are the
bounds:

- One call of `Pep::display` with n questions sends at most n / 50 requests, rounded up, and one read of the metadata,
  under one permit.
- One call of `Pep::check` sends one request under its own permit. Several such calls run at the same time, within the
  permits of the pool.
- One page of `GET /api/dashboards` asks at most 200 questions (11.25), so it sends at most 200 requests.
- One request to `GET /api/folders` asks at most 2001 questions (11.25). It makes one call of `Pep::display` with at most
  1000 questions: 20 requests under one permit. It also makes at most 1001 calls of `Pep::check`: one request for each,
  each under its own permit.

### 11.14 The service id

The service id is `access.identity.audience`. It has no default value. It is the claim `aud` of each token and the member
`server` of the action document. It is also the id of the root node of type `service`. The reference value is `dashboards`.

### 11.15 The resource tree

Hune gives each operation a node of this tree:

```text
service        access.identity.audience
├── folder     the id of a folder
│   └── dashboard   the uid of a dashboard
└── source     the id of a clickhouse or postgres block
```

The type `folder` has the depth 1, the type `dashboard` has the depth 2, and the type `source` has the depth 1. P 4.1
permits the depth 4 below the service, so Hune uses 2 of 4.

A folder id and a dashboard uid follow the id rule of section 4: `^[a-z0-9][a-z0-9-]{0,39}$`. A source id follows the
same rule (13.12, 13.12a). These ids have only characters that P 5.2 does not encode. A grant on a node covers each node below
it (P 4.3). A grant on a folder thus covers each dashboard of the folder, now and later.

### 11.16 The chain of ancestors

Hune sends the node and the chain of its ancestors in `resource.properties.ancestors` (P 4.1). The chain starts at the
service and ends at the parent of the node. These are the four forms, for the service `dashboards`:

A dashboard `score` in the folder `transit`:

```json
{ "type": "dashboard", "id": "score",
  "properties": { "ancestors": [ { "type": "service", "id": "dashboards" },
                                 { "type": "folder", "id": "transit" } ] } }
```

The folder `transit`:

```json
{ "type": "folder", "id": "transit",
  "properties": { "ancestors": [ { "type": "service", "id": "dashboards" } ] } }
```

The source `prod`:

```json
{ "type": "source", "id": "prod",
  "properties": { "ancestors": [ { "type": "service", "id": "dashboards" } ] } }
```

The service:

```json
{ "type": "service", "id": "dashboards" }
```

The text form of a chain is `service:dashboards/folder:transit/dashboard:score`. The logs and the cache keys use it. The
options route uses the path form of P 5.2, without the service: `folder:transit/dashboard:score`.

The chain comes from the in-memory catalog of Hune, never from a value of the request (P 4.5). For a dashboard, the
folder comes from the catalog entry of its uid, not from the URL and not from the body.

### 11.17 The mapping of the two storage backends

Both backends give the same tree, because the tree comes from the catalog above the `Store` trait (section 5).

| Node | Volume backend | PostgreSQL backend |
|---|---|---|
| `folder:<f>` | The directory `<path of the volume>/<f>/` | The row of the table of folders with the id `<f>` |
| `dashboard:<u>` | The file `<path of the volume>/<f>/<u>.json`, whose member `uid` is `<u>` | The row of the table of dashboards with the uid `<u>` and the folder id `<f>` |
| `source:<s>` | The block `clickhouse "<s>"` or `postgres "<s>"` of the configuration | The block `clickhouse "<s>"` or `postgres "<s>"` of the configuration |

A move changes the chain of a dashboard. The grants on the old folder no longer apply to it, and the grants
on the new folder apply at once.

A change of the folder between the decision and the write is a race. Hune closes it: a write operation gives the store
the folder of the chain that the PDP saw. The store refuses the write with `409 conflict` when the current folder of the
uid is another folder (section 5). The etag does not show a move, because the document does not hold its folder.

### 11.18 The action document

`GET` and `HEAD /.well-known/procurauth/actions` give the action document (P 5.1). The route needs no token and asks the
PDP nothing, also when the request has a bad token. Hune builds the document one time at start with
`ActionDocumentBuilder` of the library. It serves the same bytes at each read, with `Content-Type: application/json`. In
the mode `open`, the route answers 404.

The document for `audience = "dashboards"`, `server.history` and `server.export` equal to `true`, and two sources, `prod` (label `Production`,
`max_result_rows = 100000`, `max_execution_time = "60s"`) and `preprod` (label `Pre-production`,
`max_result_rows = 50000`, `max_execution_time = "30s"`):

```json
{
  "server": "dashboards",
  "resource_types": [
    { "name": "folder", "parent": "service",
      "description": "One folder of dashboards, by its id." },
    { "name": "dashboard", "parent": "folder",
      "description": "One dashboard, by its uid." },
    { "name": "source", "parent": "service",
      "description": "One data source of the configuration, by the id of its block." }
  ],
  "actions": [
    { "name": "dashboard.read", "resource_types": ["service", "folder", "dashboard"],
      "description": "List and open dashboards, and run their saved queries and the queries of their variables.",
      "limits": ["max_rows", "max_duration"] },
    { "name": "dashboard.history", "resource_types": ["service", "folder", "dashboard"],
      "description": "List the versions of a dashboard of the primary store, read one version and compare two versions. With dashboard.write, restore a version." },
    { "name": "dashboard.export", "resource_types": ["service", "folder", "dashboard"],
      "description": "Download the file of the current document of a dashboard." },
    { "name": "dashboard.write", "resource_types": ["service", "folder", "dashboard"],
      "description": "Create, change, move, restore and delete dashboards. Create, rename and delete folders." },
    { "name": "source.query", "resource_types": ["service", "source"],
      "description": "Run SQL that is not saved on this source, and save dashboards whose SQL uses this source.",
      "limits": ["max_rows", "max_duration"] }
  ],
  "resources": [
    { "path": "source:preprod", "label": "Pre-production" },
    { "path": "source:prod", "label": "Production" }
  ],
  "limits": [
    { "name": "max_rows", "label": "Most rows of one query",
      "description": "The most rows that the result of one query can have.",
      "unit": "row", "kind": "ceiling", "min": 1, "max": 100000 },
    { "name": "max_duration", "label": "Longest query",
      "description": "The longest run of one query, in seconds.",
      "unit": "second", "kind": "ceiling", "min": 1, "max": 60 }
  ]
}
```

These rules make the document:

- `server` is `access.identity.audience`.
- The three types and the two limit declarations never change.
- `dashboard.read`, `dashboard.write` and `source.query` are always in `actions`, because a primary store always exists
  (13.10). `dashboard.history` is in `actions` only when `server.history` is `true`. `dashboard.export` is in `actions`
  only when `server.export` is `true`. The order is `dashboard.read`, `dashboard.history`, `dashboard.export`,
  `dashboard.write` and `source.query`. Hune sends no question for an action that it does not publish.
- The two keys are the ceiling of the two actions, also in the mode `open`. The PDP can only restrict: when the key is
  `false`, Hune refuses the route and asks nothing (6.17). A policy that grants `dashboard.history` or
  `dashboard.export` on a resource gives nothing more than the key gives. The older policies grant neither action, so
  their subjects lose the history and the export until the policy adds them: the answer fails closed.
- `resources` has one item for each source block of both kinds, sorted by the id in byte order. Its `path` is `source:<id>` and
  its `label` is the key `label` of the block. 
- The folders and the dashboards are not in `resources`, because they change while Hune runs. The options route gives
  them (11.26).
- `max` of `max_rows` is the highest `max_result_rows` of all source blocks. `max` of `max_duration` is the highest
  `max_execution_time` of all source blocks, in seconds.
- The document lists each source label. A source is thus never a secret.

### 11.19 Check of the action document against the protocol

| Protocol rule | The document of 11.18 |
|---|---|
| P 5.2: `server` equals the `aud` of the tokens. | `server` is `access.identity.audience`, and P 9.4 checks `aud` against the same key. |
| P 5.2: a type name has 1 to 32 characters, `a` to `z` first, then `a` to `z`, `0` to `9` or `_`, not `service`. | `folder`, `dashboard`, `source`. |
| P 5.2 and P 4.1: the parents have no cycle and a depth of 4 at most. | Depths 1, 2 and 1. |
| P 5.2: an action name has parts of `a` to `z`, `0` to `9` and `_`, with `.` between them. No action `access`. | `dashboard.read`, `dashboard.write`, `source.query`. |
| P 5.2: `resource_types` of an action holds `service` or declared types, each one time. | Yes. |
| P 5.2: each limit name of an action is declared, one time. | `max_rows` and `max_duration` are declared. |
| P 5.2: a resource path is `<type>:<id>` segments, each id percent-encoded. | `source:<id>`. The id rule needs no encoding. |
| P 5.2a: only the served actions. | `dashboard.write` and `source.query` need a primary store (13.10). |
| P 5.3c: 64 KiB, 64 actions, 16 types, 256 resources, 16 limits, a label of 60 characters, a description of 300 characters, a unit of 20 characters. | At most 64 sources in all (13.7), 3 actions, 3 types, 2 limits. Labels have 60 characters at most (13.12, 13.12a). The largest document is less than 64 KiB. |
| P 5.4: labels and descriptions in English. | Yes. |
| P 6.1: a ceiling has `min` and `max`, whole numbers, `min` not above `max`. | `min` is 1. `max` is 1 or more, because each source limit is 1 or more. |
| P 7.1: a ceiling name is `max_` and a noun. A lower value is stricter. A duration is in whole seconds. | `max_rows` (unit `row`) and `max_duration` (unit `second`). |
| P 6.1: a budget has no `min` or `max`. | Hune declares no budget. |

A unit test builds the document for 64 sources with labels of 60 characters of 4 bytes each, and checks each line of
this table.

### 11.20 Order of the checks

Each request on `server.listen` passes these steps in this order. A step that refuses stops the request.

1. Limits: the request line and the URL, the headers, the route, and the declared body size (`Content-Length`) against
   the limit of the route (12.16). Refusal: 400, 404, 405, 413, 414 or 431.
2. Write guard, on each `POST`, `PUT` and `DELETE` (12.7). Refusal: 403 `cross_site_request` or 415.
3. Token check, in the mode `authzen`, on each route except the action document (11.8). Refusal: 401.
4. Process ceilings: a history route and the restore, when `server.history` is `false`, get `403 history_disabled`. A
   read of a dashboard with `download=true`, when `server.export` is `false`, gets `403 export_disabled`. Neither answer asks a
   question, and neither depends on the uid (6.17, 6.14).
5. Body read, within the limit of the route (12.16). A chunked body above the limit gets 413. A body that does not
   arrive within 30 s gets 408.
6. Shape of the request and of its body (section 6). Refusal: 400. The document of a create, a save or a
   `validate_only` request stays bytes at this step. Hune checks the JSON syntax of the whole body and reads only the
   other members.
7. Catalog lookup. An unknown uid or folder id follows 11.23. An unknown source id gets 404 and no question. For a
   create and a save, the pre-pass of 11.21 reads the sources of the document before this lookup. Thus an item that
   exists and an item that does not exist get the same work before the decision (11.23).
8. Decision: the other permissions in one batch, then the use (11.21). Refusal: 403 or 503. A restore asks the use
   alone before it reads the version (11.21).
9. Validation of the document, of the draft of a preview or of the restored version (4.36, 7.18). It runs in
   `spawn_blocking`, within the limits of 12.16. Refusal: 422. For a create and a save, the check after the validation
   follows (11.21). This is the one validation of a restored version: the store does not validate it again (6.18).
10. Effect: the store write or the query, with the folder check of 11.17 and the ceilings of 11.24.

This is the order of 6.28.

The decision comes before the first call to ClickHouse and before the first write to the store. It also comes
before the validation of a document or of a draft. Thus a subject without the right never reaches the validator. The
work before the decision is the body read, the syntax check and the pre-pass. Each of these steps is linear in the
bytes.

### 11.21 The routes and their actions

This table gives the permissions of each route of section 6 in the mode `authzen`. Notation:

- N(u) is the node `dashboard:u` with the chain service, `folder:<folder of u>`.
- F(f) is the node `folder:f` with the chain service. S(s) is the node `source:s` with the chain service.
- "Use" is the permission of the operation (P 5.5). "Others" are the other permissions of the operation (P 5.5a).
- "Display" means questions for the display filter only (11.25). They never let an operation run.
- No action declares a budget. Thus no question has `context.usage` (P 6.4), and no question is a use with units.

| Route | Others | Use | Ceilings that apply |
|---|---|---|---|
| `GET /api/dashboards`, with the parameters `folder`, `q`, `limit` and `after` | none | none; display questions of 11.25 | none |
| `POST /api/dashboards` (create in the folder f) | `source.query` on S(s) for each source that the document names | `dashboard.write` on F(f) | none |
| `POST /api/dashboards?validate_only=true` | none | `dashboard.write` on F(f) | none |
| `GET /api/dashboards/{uid}` | none | `dashboard.read` on N(uid) | none (no query runs) |
| `GET /api/dashboards/{uid}?download=true` | `dashboard.read` on N(uid) | `dashboard.export` on N(uid) | none |
| `PUT /api/dashboards/{uid}` | `source.query` on S(s) for each source that the new document names | `dashboard.write` on N(uid) | none |
| `PUT /api/dashboards/{uid}?validate_only=true` | none | `dashboard.write` on N(uid) | none |
| `DELETE /api/dashboards/{uid}` | none | `dashboard.write` on N(uid) | none |
| `GET /api/dashboards/{uid}/history`, with the parameters `limit` and `after` | `dashboard.read` on N(uid) | `dashboard.history` on N(uid) | none |
| `GET /api/dashboards/{uid}/history/{n}`, also with `download=true` | `dashboard.read` on N(uid) | `dashboard.history` on N(uid) | none |
| `POST /api/dashboards/{uid}/restore` | `dashboard.history` on N(uid), and `source.query` on S(s) for each source that the restored version names | `dashboard.write` on N(uid) | none |
| `POST /api/dashboards/{uid}/move` (to the folder f2) | `dashboard.write` on N(uid) | `dashboard.write` on F(f2) | none |
| `POST /api/dashboards/{uid}/panels/{id}/query` | none | `dashboard.read` on N(uid) | `max_rows`, `max_duration` of the use |
| `POST /api/dashboards/{uid}/variables/{name}/options` | none | `dashboard.read` on N(uid) | `max_rows`, `max_duration` of the use |
| `POST /api/dashboards/{uid}/annotations/{index}/query` | none | `dashboard.read` on N(uid) | `max_rows`, `max_duration` of the use |
| `GET /api/folders` | none | none; display questions of 11.25 | none |
| `POST /api/folders` | none | `dashboard.write` on the service | none |
| `PUT /api/folders/{id}` | none | `dashboard.write` on F(id) | none |
| `DELETE /api/folders/{id}` | none | `dashboard.write` on F(id) | none |
| `GET /api/sources` | none | none; display questions of 11.25 | none |
| `POST /api/sources/{id}/preview`, with a body that holds `panel` or `variable` | `dashboard.write` on N(uid) when the body has `uid`; on F(f) when the body has `folder` (a dashboard that is not saved) | `source.query` on S(id) | `max_rows`, `max_duration` of the use |
| `GET /api/session` | none | none (token only, 11.29) | none |
| `GET`, `HEAD /.well-known/procurauth/actions` | none | none (no token, 11.29) | none |
| `GET`, `HEAD /.well-known/procurauth/resources` | none | display questions of P 5.6 (11.26) | none |
| Pages `/` and `/d/{uid}[/{slug}]`, files under `/static/{build-hash}/` | none | none (token only, 11.29) | none |

Rules for the table:

- "The sources that a document names" are the `source` of each query of each panel, each query variable and each annotation of the document. A panel query is `data.query` and each item of `data.extra_queries`. A hidden query counts. Hune removes the duplicates. A document that names no source needs no `source.query`.
- The save routes ask `source.query` because a save of SQL is the power to run that SQL with the credentials of Hune.
  The check applies to each named source, also when the SQL text did not change.
- `dashboard.read` gives the result of the stored SQL, whatever tables that SQL reads. `source.query` gives each read
  that the ClickHouse account permits (12.12). The documentation for operators states both facts.
- An annotation query asks the same question as a panel query: `dashboard.read` on N(uid). No new action exists, and the action document does not change (11.18). A viewer with `dashboard.read` gets the title, the text and the tags that the stored SQL reads, as a panel gets its values. The operator MUST NOT give a column of a secret to an annotation (12.18).
- Hune asks the others first, in one Access Evaluations request (P 10.1a). A `false` answer for one of them gives 403,
  and Hune does not ask the use. Then Hune asks the use.
- A create and a save ask their questions before the validation of the document (11.20). A pre-pass reads the
  sources from the bytes of the document. It reads the member `source` of each query variable, of each annotation,
  and of `data.query` and each item of `data.extra_queries` of each panel. A hidden query counts. The member `source`
  of an `adhoc` variable counts too (4.10c): a person who adds ad hoc filters on a source needs `source.query` on it.
- The pre-pass keeps only the ids that the configuration declares, without duplicates, in the order of the
  document: the variables, the annotations, then the panels. Thus an undeclared id never goes to the PDP. The
  pre-pass is linear in the bytes, and each list has its budget of 4.35.
- After the validation, each source of the valid document must be in the questions of the decision. Else Hune
  answers `500 internal_error`, writes the log line `access.prepass_mismatch` (14.7), and writes nothing. Thus a fault
  of the pre-pass can never skip a question.
- A document that the pre-pass cannot read (a wrong type, or a list above its budget) asks the use alone. The
  validation then gives its `422`. When such a document passes the validation, the answer is `500`, and Hune writes
  nothing.
- A restore asks the use alone first, before it reads the version. A `false` answer gives 403, and Hune reads
  nothing. Then Hune reads the version, validates it, and asks the others and the use of the table. The use thus goes
  to the PDP two times (11.13), because a write route uses no kept answer (11.27).
- The questions of each write route of 11.20, the preview included, never use a kept answer, and the library keeps no
  answer to them (11.27). Each request of a write route thus sends its questions to the PDP.
- `validate_only=true` and the preview ask their questions before the validation of the document or of the draft.
  Thus a `403` comes before a `422`.
- Hune removes each repeated question and each other permission that equals the use.
- `POST /api/sources/{id}/preview` runs SQL on the source `id` only. The body has exactly one of `uid` and `folder`,
  and exactly one of `panel` and `variable` (6.22). With `panel`, the query of the draft panel runs. With `variable`,
  the options query of the draft variable runs. With `uid`, the other permission is on N(uid). With `folder`, it is on
  F(f), with f the member `folder`.
- Inspect (panel query row): when `server.query_inspect` is `true`, Hune asks one more question in one operation. The question is `source.query` on S(s), with s the source of the query that the request runs (4.15a). A subject can see the SQL of a query of one source and not of another. The other permission is `dashboard.write` on N(uid). A `false` answer, or an error, removes `sql` and does not change the status (6.20).
- A panel with several queries needs no new question. Each request of 6.20 runs one query and asks `dashboard.read` on N(uid), as before. The questions of the requests of one panel are equal. The cache and the one-flight rule of 11.27 make one call of the library for them. With `cache_ttl = "0s"` each request asks its own question, so the PDP gets one question for each request that the page sends (9.8d). The PDP does not see the ref or the source of the query.
- The server refuses a hidden query after the question `dashboard.read` and before any byte to a source. It gives `400 bad_request` (6.20). A subject without `dashboard.read` gets `403`, so it learns nothing about the queries.
- `validate_only=true` (6.15) asks only the use question. It asks no `source.query`, because it runs no SQL and writes
  nothing.
- `download=true` (6.14, 6.17) asks the same question as the same route without the parameter.
- The parameters `folder`, `q`, `limit` and `after` choose the items that Hune examines. They never change the
  questions for one item. A `folder` that the catalog does not hold gives an empty list and no question (6.12). Thus a
  hidden folder and an unknown folder give the same answer.
- `HEAD` of a `GET` route asks the questions of that `GET` route (6.2).
- In the mode `authzen`, a route of section 6 that this table does not list answers `403 forbidden`. It asks no
  question and writes the log line `access.no_rule`. The CI job `rust` fails when a route of the router has no line in this
  table.

### 11.22 Questions in the mode open

The mode `open` sends no question. The process ceilings of step 4 of 11.20 still apply: `server.history` and
`server.export`. Each other operation runs within the limits of the process (12.16).

### 11.23 The unknown-uid rule

A uid or a folder id that does not exist must give no information to a subject without the right to see it. Hune
applies these rules in the mode `authzen`:

1. A route names a dashboard uid that the catalog does not hold. Hune asks `dashboard.read` on the service, with the
   match `node`. A `false` answer gives `403 forbidden`. A `true` answer gives `404 not_found`. A PDP failure gives
   `503 pdp_unavailable`.
   - Hune asks this question at each request. It never uses a kept answer, and the library keeps no answer to it
     (11.27).
   - On a write route, this question is the use of one operation. Its others are the other permissions of the route
     that name only items that exist (11.21). Hune asks them first, in one request, as for an item that exists. A
     `false` answer for one of them gives `403 forbidden`.
   - These others are `source.query` on each source of the pre-pass for a create and a save. For a move to a folder
     that does not exist, the other is `dashboard.write` on N(uid). Hune does not ask an other that names the unknown
     item. Thus it never asks a question about an item that does not exist.
2. Rule 1 also applies to a folder id that the catalog does not hold. Such an id can be in `PUT` and
   `DELETE /api/folders/{id}`, in the folder of `POST /api/dashboards`, in the target of a move, or in a preview.
3. `POST /api/dashboards` can name a uid that the store holds in any folder, also as a deleted dashboard (6.13). After
   a `true` answer for the use, Hune asks `dashboard.read` on N(uid) of the dashboard that has this uid. The catalog has
   no entry for a deleted dashboard: Hune reads its last version with `Store::history(uid, None, 1)` (section 5) and
   takes the folder of that version for the chain. A `false` answer gives `403 forbidden`. A
   `true` answer gives `409 already_exists`.
4. A panel id, a variable name or a history version that the dashboard does not hold gives `404 not_found`. Hune
   answers this after a `true` answer for the use of the route, so the subject already has access to the dashboard. A
   restore asks the use first, before it reads the version (11.21). Thus a version that does not exist gives `404`
   after the use only, because the others need the sources of the version.
5. A source id that the configuration does not hold gives `404 not_found` and no question. The action document already
   lists each source.
6. The `403` of rules 1 to 3 is the `403` of each other refusal of the route. It has the same status, the same headers
   and the same body. Its member `action` is the use of the route (11.30). Before the decision, Hune does the same work
   for an item that exists and for one that does not exist. This work is the body read, the syntax check and, for
   a create and a save, the pre-pass of 11.21. Hune reads no store and validates no document before the decision.
   - On a write route, the two items also send the same number of PDP requests. They ask the same others, and neither
     uses a kept answer.

Thus a subject with grants on one folder gets `403` for a dashboard of another folder. It gets the same `403`, byte
for byte, for a uid that does not exist. A subject with `dashboard.read` on the service can read each dashboard, so a
404 tells it nothing new.

On a write route, the request count and the use of the cache thus do not show whether an item exists (12.4, T98).
The last question of rule 1 is not the use of the route, so the PDP can still take another time to evaluate it. On a
read route, the use on N(uid) can be a kept answer, and the question of rule 1 cannot. Two requests for one uid in
one `cache_ttl` can thus take different times (12.18).

In the mode `open`, each unknown item gives `404 not_found` and no question.

### 11.24 The ceilings and the limits of a source

Each source block has its own limits (13.12, 13.12a). A `true` answer for `dashboard.read` or `source.query` can carry the
ceilings `max_rows` and `max_duration` in `context.limits` (P 7.3). Hune applies them to each query that the
permission lets run (P 7.4, P 5.5a):

- Effective `max_result_rows` = the lower of the key `max_result_rows` of the source and `max_rows`.
- Effective `max_execution_time` = the lower of the key `max_execution_time` of the source, in seconds, and
  `max_duration`.
- Without a ceiling in the answer, the key of the source applies alone.
- Hune sends the effective values to ClickHouse as the settings `max_result_rows` and `max_execution_time`, with
  `result_overflow_mode = 'throw'` (section 8). The client deadline of section 8 uses the effective
  `max_execution_time`.
- For a PostgreSQL source, Hune enforces the effective values in its own client (8a.6, 8a.7, 11.33).
- The ceilings change no other limit. `max_result_bytes`, `max_query_size` and the concurrency limits stay the values of
  the source.

The queries that a ceiling bounds:

| Route | Permission whose ceilings apply | Queries |
|---|---|---|
| `POST /api/dashboards/{uid}/panels/{id}/query` | `dashboard.read` on N(uid) | The query of the panel |
| `POST /api/dashboards/{uid}/variables/{name}/options` | `dashboard.read` on N(uid) | The query of the variable |
| `POST /api/dashboards/{uid}/annotations/{index}/query` | `dashboard.read` on N(uid) | The query of the annotation. The effective `max_result_rows` is also at most `MAX_ANNOTATION_ROWS` (7.37). |
| `POST /api/sources/{id}/preview` | `source.query` on S(id) | The query of the draft panel, or the options query of the draft variable |

The other routes run no query. A ceiling in an answer for such a route has nothing to bound, and Hune accepts the
answer. This includes `source.query` as another permission of a save.

Thus a save does not apply the ceilings of `source.query`. The saved SQL runs later with the ceilings of
`dashboard.read` of each viewer, and the author can be a viewer. Operators SHOULD give each author the same ceilings on
the `dashboard.read` grants as on the `source.query` grants (12.18).

Hune never cuts a result. A result above an effective limit is an error, and the browser gets no row of it:

- ClickHouse error 159 (`TIMEOUT_EXCEEDED`) when the effective `max_execution_time` comes from `max_duration` gives
  `403 ceiling_exceeded` with `ceiling = "max_duration"`.
- ClickHouse error 396 (`TOO_MANY_ROWS_OR_BYTES`) when the effective `max_result_rows` comes from `max_rows` gives
  `403 ceiling_exceeded` with `ceiling = "max_rows"`. ClickHouse uses one code for the rows and the bytes of a result.
  Thus the message names both limits that applied: the rows and `max_result_bytes`.
- When the value of the source is the effective value, the error is the limit error of the source (section 8).
- When the ceiling equals the value of the source, the ceiling is the cause.

The body of `ceiling_exceeded` has the members of section 6 and three more: `action`, `ceiling` and `limit`. The log line
is `access.ceiling_refused` (14.7).

Example. The source `prod` has `max_result_rows = 100000` and `max_execution_time = "60s"`. The PDP answers
`{"decision": true, "context": {"limits": {"max_rows": 1000, "max_duration": 30}}}` for `dashboard.read` on
`dashboard:score`. Hune sends `max_result_rows=1000` and `max_execution_time=30`. A result of 1001 rows gives this body:

```json
{"error": {"code": "ceiling_exceeded", "message": "The result is larger than a limit: 1000 rows or 33554432 bytes.",
           "retryable": false, "action": "dashboard.read", "ceiling": "max_rows", "limit": 1000}}
```

These answers of the PDP are refusals:

- A `true` answer with a ceiling that the action does not list gives `403 forbidden` (P 7.5). The log line is
  `access.ceiling_unenforceable`.
- A `true` answer with a budget name in `context.limits` is not valid. A ceiling value that is not a whole number of 0
  or more is not valid too. Both give `503 pdp_unavailable` (11.28).
- A ceiling value below the `min` of its declaration (1, 11.18) is outside the range of P 6.1. Hune treats the answer
  as not valid: `503 pdp_unavailable`, with the log line `access.pdp_invalid_answer`.
- Hune ignores `context.limits` in a `false` answer (P 7.6).

### 11.25 The display filter

The display filter shows each person the items and the controls that the person can use. It is for the display only.
The server checks each operation again (11.21). A `true` answer for the match `subtree` never lets an operation run
(P 4.4).

The library has two calls for these questions. `Pep::display` asks a batch of questions with the match `subtree` and
gives one answer for each. `Pep::check` asks one operation without usage, with the match `node`. Hune calls it with one
permission as the use. An outcome that lets the operation run is `true`, and a refusal 403 is `false`. A refusal 503
makes the list fail.

The list of dashboards (`GET /api/dashboards`) uses this procedure:

1. Hune asks `dashboard.read` on F(F) for each folder F of the catalog, in one call of `Pep::display`.
2. A folder with `false` is hidden, with each of its dashboards.
3. For each visible folder, Hune asks `dashboard.read`, `dashboard.write`, `dashboard.history` and `dashboard.export` on
   F(F), each with one call of `Pep::check`. It skips `dashboard.write` and `dashboard.history` for a folder of a read-only
   store, and each action whose key is `false` (`server.history`, `server.export`). These calls run at the same time, within the pool of 11.13.
4. A folder with `true` for `dashboard.read` shows with each of its dashboards. A grant on the folder covers each
   dashboard below it (P 4.3), so Hune asks no more question.
5. For the dashboards of the other visible folders, Hune asks `dashboard.read` on N(u) of each dashboard u, in one call
   of `Pep::display`. A dashboard has no node below it, so the match `subtree` gives the display of the match `node`. A
   dashboard with `true` shows.
6. Each visible folder carries the flag `access.write`: the `dashboard.write` answer of step 3. Each visible dashboard
   carries the flags `access.write`, `access.history` and `access.export` of its folder: the answers of step 3 (6.12).

The list of folders (`GET /api/folders`) uses steps 1 and 2. Then it asks `dashboard.write` on F(F) of each visible folder
with `Pep::check`, for its flag `access.write`. It also asks `dashboard.write` on the service with `Pep::check`, for the
flag `access.create` at its top level.

The other answers of the display filter:

| Answer | Member | Question |
|---|---|---|
| `GET /api/dashboards/{uid}` | `access.write` | `dashboard.write` on N(uid), one call of `Pep::check` after the `true` answer for the use |
| `GET /api/dashboards/{uid}` | `access.history` | `dashboard.history` on N(uid), one call of `Pep::check`; no question for a dashboard of a read-only store or with `server.history` equal to `false` |
| `GET /api/dashboards/{uid}` | `access.export` | `dashboard.export` on N(uid), one call of `Pep::check`; no question with `server.export` equal to `false` |
| `GET /api/sources` | the list itself | `source.query` on S(s) for each source s, in one call of `Pep::display`; a source has no node below it; a source with `true` is in the list |

The page uses these flags. It hides the edit, save, delete, move and restore controls of a dashboard without
`access.write`. It hides the History entry of a dashboard without `access.history`, and the Export and Download entries
of a dashboard without `access.export`. It hides "New dashboard" in a folder without `access.write`, and "New folder" without `access.create`.
The source picker and "Run draft" of the editor show only the sources of `GET /api/sources`. The move dialog offers
only the folders with `access.write`.

A folder or a dashboard of a read-only store has the flag `access.write` equal to `false`. Hune asks no question for
`dashboard.write` (11.35). The flag `access.history` is `false` too, with no question for `dashboard.history`, because
only the primary store has history (5.18). With `server.history` or `server.export` equal to `false`, the flag is `false`
and Hune asks no question.

In the mode `open`, each item shows. The flag `access.create` is `true`. The flag `access.write` is `true` for an item of the
primary store, and `false` for an item of a read-only store. The flag `access.history` is the value of `server.history` for an
item of the primary store, and `false` for an item of a read-only store. The flag `access.export` is the value of
`server.export`. `GET /api/sources` gives each source.

A PDP failure during a list gives `503 pdp_unavailable`. A list never shows a result that a failure made empty (fail
closed). Section 6 gives the place of each flag in each answer.

One page of `GET /api/dashboards` asks at most 200 questions. When a page reaches this bound, it holds the items that
Hune found and a `next` value (6.24). `GET /api/folders` has no pages. One request to it asks at most
2 x `MAX_FOLDERS` + 1 questions, two for each folder and one for the service. With the folder limit of section 5, this is
2001.
This bound does not grow with the requests.

The questions of one list are in Access Evaluations requests within the maxima of the PDP (P 10.7a). The kept answers
of 11.27 apply. For example, a deployment has 3 folders and a grant on each. There, one list of dashboards costs 9 questions,
and the list of folders costs 7. A list in the next
`cache_ttl` costs none.

### 11.26 The options route

`GET /.well-known/procurauth/resources` lists the children of a node, page by page (P 5.6 to P 5.8). It needs a valid
token. It has no action of its own. In the mode `open`, it answers 404.

| Parameter | Rule |
|---|---|
| `type` | Necessary. `folder` or `source` without `parent`. `dashboard` with a `parent` of the form `folder:<id>`. |
| `parent` | Absent for `folder` and `source`. For `dashboard`, the path `folder:<id>`. |
| `q` | A search text of 256 bytes at most: a substring of the id or of the label, without case (ASCII). |
| `limit` | A whole number of decimal digits, 1 or more. A value above 50 gives 50 (P 5.7 permits fewer items). |
| `after` | The value `next` of the previous answer. |

The rules of the query string:

- Hune reads the raw query. It splits it at each `&` and at the first `=` of each part. It decodes each `%XX` of each
  name and value one time. It does not read `+` as a space. A bad `%XX` sequence gives `400 invalid_options_request`.
- A parameter that occurs two times gives 400. A request without `type` gives 400. A `type` that is not the type of the
  children of `parent` gives 400. A `limit` that is not a whole number of 1 or more gives 400.
- A parameter that Hune does not know has no effect.
- The query string fits in the request line, which has 8 KiB at most (12.16). The cursor has 2048 bytes at most. A cursor that this route did not make for the
  same `type` and `parent` gives 400.
- `HEAD` gives the headers only. Another method gives 405.

The children:

| `type` | Children | `label` | `has_children` | Actions asked for each child, in order |
|---|---|---|---|---|
| `folder` | The folders of the catalog | The title of the folder, cut to 60 characters | `true` | `dashboard.read`, `dashboard.write` |
| `dashboard` | The dashboards of the folder of `parent` | The title of the dashboard, cut to 60 characters | `false` | `dashboard.read`, `dashboard.write` |
| `source` | The `clickhouse` blocks | The key `label` of the block | `false` | `source.query` |

For each child, Hune asks the published actions of the table with the match `subtree` and no usage. It stops at the
first `true` answer. A child with one `true` answer is in the list (P 5.6).

The items are sorted by the id in byte order. One answer has 50 items at most. One request asks at most 200 questions.
When this bound stops the walk, the answer holds the items found and a `next` value. The answer can have no item and a
`next` value. `next` is `null` on the last page.

The cursor is the base64url text, without padding, of `<type>` `\n` `<parent>` `\n` `<last visible id>` `\n` `<count>`.
The last visible id is the id of the last item that the subject can see, or empty before the first item. The count is
the number of children that the walk examined and hid after this item. It is a whole number in decimal digits.

The cursor never holds the id of a child that the subject cannot see. A page with no item keeps the position of its
request and adds its children to the count. A cursor of the older form, or with a bad count, gives 400.

A parent that does not exist gives an answer with no item, as P 5.7 says. A PDP that gives no valid answer gives
`503 pdp_unavailable`, never an empty list. The log line `access.options` gives the counts (14.7).

Example: the dashboards of the folder `transit` whose id or title holds `score`, two at a time.

```text
GET /.well-known/procurauth/resources?parent=folder%3Atransit&type=dashboard&q=score&limit=2
```

```json
{ "items": [ { "id": "score", "label": "Score", "has_children": false },
             { "id": "score-daily", "label": "Score daily", "has_children": false } ],
  "next": "ZGFzaGJvYXJkCmZvbGRlcjp0cmFuc2l0CnNjb3JlLWRhaWx5CjA" }
```

### 11.27 The answer cache

The library keeps the answers of the PDP for `access.pdp.cache_ttl` at most (P 10.1). These are its rules in Hune:

- The key of a kept answer is the subject, the action, the resource with its chain, and the match.
- A kept `true` answer lets the same operation run with no question. A kept `false` answer refuses it with no question.
- A failure is never kept.
- The cache holds 16384 entries at most. When it is full, the library keeps no new answer (P 10.1b).
- `cache_ttl = "0s"` keeps no answer.
- Each instance has its own cache. A revoked grant can stay active for `cache_ttl` at most on each instance.
- A cache hit writes no `access.decision` line. The metric `hune_pdp_cache_hits_total` counts it.

The cache does not keep these questions:

- The questions of each write route of 11.20, the preview included.
- The question of 11.23 on the service, on each route.

Hune asks them with a second PEP of the library that has no cache. Both PEPs use the same link and the same pool
(11.12, 11.13). The second PEP reads the metadata of the PDP at its first question. With `cache_ttl = "0s"`, Hune has
one PEP only. Thus a write route sees a revoked grant at once. On a write route, an item that exists and one that does
not exist also cost the same PDP requests (11.23).

Hune adds one rule above the library: one flight for each question. The rule applies when `cache_ttl` is more than
`0s`. Two equal questions that wait at the same time share one call of the library. Two questions are equal when they
have the same key in the cache. The second question waits for the answer of the first. The rule does not apply to a
question that the cache does not keep: each request asks its own.

This rule keeps a page with 45 panels at one PDP question for its first refresh. With
`cache_ttl = "0s"`, each operation asks its own question (P 10.1).

The load on the PDP is small. One subject on one dashboard with a refresh of 30 s and `cache_ttl = "30s"` sends about
one question each 30 s. Each write request and each request for an unknown item sends its questions to the PDP: three
PDP requests at most, in sequence. The pool of 11.13 bounds them.

### 11.28 The PDP down, slow or wrong

Hune fails closed (P 10.3). It never sends `context.reason` to the client.

| Case | Answer of Hune | Log line |
|---|---|---|
| `true`, with no ceiling or with ceilings of the action | The operation runs (11.24). | `access.decision` |
| `false` with no reason, or a reason other than `budget_exceeded` | `403 forbidden` | `access.decision` |
| `false` with `budget_exceeded` and a valid `retry_after` | `403 forbidden`. Hune declares no budget, so a correct PDP never gives it (11.29). | `access.decision` |
| `false` with `budget_exceeded` and a `retry_after` that is not a whole number of 1 or more | `503 pdp_unavailable` | `access.pdp_invalid_answer` |
| `true` with a ceiling that the action does not list | `403 forbidden` | `access.ceiling_unenforceable` |
| `true` with a budget in `context.limits`, or a ceiling value that is not a whole number, or that is below the `min` of its declaration | `503 pdp_unavailable` | `access.pdp_invalid_answer` |
| No connection, a connection error, or no answer within `access.pdp.timeout` | `503 pdp_unavailable` | `access.pdp_failed` |
| No permit of the pool within `access.pdp.timeout` (11.13) | `503 pdp_unavailable` | `access.pdp_failed` with `error = "pool_busy"` |
| An HTTP status other than 200 | `503 pdp_unavailable` | `access.pdp_failed`; also `access.pdp_credential_refused` for 401 and 403 |
| A status 200 with a body that is not a valid answer, or with a wrong number of items | `503 pdp_unavailable` | `access.pdp_invalid_answer` |
| The first read of the metadata gets no answer or a status that 11.12 does not accept | `503 pdp_unavailable` | `access.pdp_failed` |
| The first read of the metadata gets a body that is not valid metadata | `503 pdp_unavailable` | `access.pdp_invalid_answer` |

More rules:

- The body of `503 pdp_unavailable` has `retryable: true`. The page shows: "The access decision service does not answer.
  Try again later."
- A kept answer still applies during an outage, until its `cache_ttl` ends. A short outage is thus invisible to a page
  that refreshes. A write route uses no kept answer (11.27), so it gets `503 pdp_unavailable` during an outage.
- A slow PDP makes each operation wait, three times `access.pdp.timeout` at most for each PDP request (11.13). The
  timers of a query start after the decision.
- A list never shows a result that a failure made empty. It answers 503.
- The action document, the static files, the pages and `GET /api/session` need no PDP. They still answer during an
  outage. The listener `server.ops_listen` still answers too.
- A PDP outage never changes `/readyz` (14.4). A restart of Hune cannot repair the PDP.
- The library never sends a question again after a timeout (P 10.6). It repeats a plain question one time only after a
  connection failure on a reused connection.

### 11.29 Routes with no token or with no question

In the mode `authzen`, one route of `server.listen` needs no token:

| Route | Reason |
|---|---|
| `GET`, `HEAD /.well-known/procurauth/actions` | P 5.1: the reader of the document has no token. Hune asks the PDP nothing. |

These routes need the token but ask no question:

| Route | Reason |
|---|---|
| The pages `/` and `/d/{uid}[/{slug}]` | The shell is the same for each uid. It holds no data of a dashboard. |
| The files under `/static/{build-hash}/` | They hold the code of the page, not data. |
| `GET /api/session` | It gives the subject of the token, the mode, `editing` (always `true`) and the stores. |

The routes of `server.ops_listen` (`/livez`, `/readyz`, `/metrics`) need no token. They are on a second listener that
the edge does not reach (14.2). Thus no route through the edge is exempt from the token, except the action document,
and P 9.4 holds.

The deviations from ProcurAuth:

| Deviation | Reason |
|---|---|
| The pages, the static files and `GET /api/session` need the token but ask no question (P 10.1, P 15 item 6). | They hold no data of a dashboard or of a source. |
| `server.ops_listen` serves three routes without a token (P 10.0, step 2). | They are for the container platform and the monitoring. They give no data of a dashboard. The edge must not route to this listener (12.6). |
| `plain_http = true` accepts an `http` URL for the PDP (P 10.0, step 4). | A trusted network can be enough. Hune writes a warning at start. |
| A `false` answer with `budget_exceeded` gives 403, not 429 (P 10.3). | Hune declares no budget, so a correct PDP never gives this answer. The error table of section 6 has no code with the status 429. |
| The options route searches a substring of the id or of the label. | P 5.6 lets the service choose its search rules. |

### 11.30 Errors of the access layer

Section 6 owns the closed list of error codes and the body `{"error": {"code", "message", "retryable"}}`. The access
layer uses these codes:

| Status | Code | Cause | `retryable` |
|---|---|---|---|
| 401 | `unauthenticated` | No token, a token that is not valid, or two copies of the token header. | `false` |
| 403 | `forbidden` | A `false` answer (also with `budget_exceeded`), an answer that Hune treats as `false`, or a route with no rule. | `false` |
| 403 | `approval_cannot_wait` | A `false` answer to a question with a reason of 17.1 (`approval_required`, `approval_pending`, `approval_denied` or `approval_expired`). Hune has no approval flow. The body is `{"error": "approval_cannot_wait"}` and has no other member (P 17.7a). | `false` |
| 403 | `ceiling_exceeded` | A query above an effective limit that a ceiling gave (11.24). The body also has `action`, `ceiling` and `limit`. | `false` |
| 403 | `history_disabled` | A history route or the restore, with `server.history` equal to `false`. No question. | `false` |
| 403 | `export_disabled` | A read of a dashboard with `download=true`, with `server.export` equal to `false`. No question. | `false` |
| 404 | `history_unavailable` | A history route for a dashboard of a read-only store, after `dashboard.read` (6.17). | `false` |
| 403 | `read_only_store` | A write whose target is in a read-only store, after a `true` answer for `dashboard.read` (11.35). | `false` |
| 404 | `not_found` | An unknown item after the rule of 11.23, or a ProcurAuth route in the mode `open`. | `false` |
| 400 | `invalid_options_request` | A query string of the options route that is not valid. | `false` |
| 409 | `already_exists` | A create of a uid that exists and that the subject can read (11.23). | `false` |
| 409 | `conflict` | The folder of the dashboard changed after the decision (11.17). | `false` |
| 503 | `pdp_unavailable` | The PDP gave no valid answer in time. | `true` |

The body of `forbidden` has the member `action` (6.8). Its value is the action of the use of the route (11.21):
`dashboard.read` for a read route and a query route, `source.query` for a preview, and `dashboard.write` for each other
route. The question that the PDP refused does not change it, and the unknown-uid rule gives the same value (11.23).
Thus a `403` does not show whether a uid or a folder exists.

The value is always an action that the action document publishes for the route (11.18). For a route with no rule, it
is `unknown`. The log line `access.decision` gives each question and its answer (14.7).

The body of `approval_cannot_wait` is the fixed body of ProcurAuth for a call that cannot wait for an approval. It is the
only error of Hune whose `error` member is a text and not an object. It has no `action`, and no route changes it.

The message of each code is a fixed English text of section 6. It never holds `context.reason`, a value of the request,
or a detail of the PDP. The log events of the access layer are in 14.7.

### 11.31 Bounds that are not keys

| Bound | Value |
|---|---|
| Size of an identity token | 16384 bytes |
| Keys in `public_keys` | 16 |
| Base64 text of one PEM block | 4096 bytes |
| `sub`, `kid`, an id of a node | 256 bytes |
| Kept answers of the PDP | 16384 entries |
| Body of a PDP answer | 262144 bytes |
| Body of one Access Evaluations request | 32768 bytes, or less when the metadata of the PDP says so |
| Items of one Access Evaluations request | 50, or less when the metadata of the PDP says so |
| Idle connections to the PDP | `access.pdp.max_connections`, closed after 5 s or half of `procurauth_idle_max_seconds` |
| Age of the metadata of the PDP | 3600 s; 5 s before a new read after a failure |
| Questions of one options request | 200 |
| Items of one options page | 50 |
| Query string of the options route | within the request line of 8 KiB; `q` 256 bytes; cursor 2048 bytes |
| Sources in the action document | 64 (the bound of the `clickhouse` blocks) |

### 11.32 Conformance series U

The ProcurAuth repository has the conformance runner `access-conformance`. The CI job `procurauth` runs it with the role
`pep` and the series U against Hune in the mode `authzen`. The job runs on each change to `crates/hune/src/access/`,
to the configuration code, to the conformance files, and before each release.

The job checks out the ProcurAuth repository at the commit of 11.10. It runs the script `tests/conformance/run.sh`. The
script does these steps:

1. It builds the runner with the scripts of the ProcurAuth repository, in Docker.
2. It builds Hune as the static musl binary.
3. It makes the fixture volume: the folder `conf` with the dashboard `conf-dash`. The dashboard has one table panel
   (id 1) on the source `conf` with the SQL `SELECT n FROM analytics.conf_rows ORDER BY n`.
4. It starts Hune in a container with no network (`--network none`) and a read-only root file system.
5. It starts a ClickHouse container (the pinned image of section 16) in the network namespace of the Hune container.
   The container runs the init file `tests/fixtures/clickhouse/init.sql` at its first start.
6. It starts the runner in the same namespace. The runner is the client, the edge (it signs tokens with the test keys of
   ProcurAuth) and the PDP on `127.0.0.1:8181`.
7. It compares the failed cases with `tests/conformance/expected.txt`, then removes its containers.

The init file creates the settings profile `hune_readonly` and the user `hune` of 8.12. It uses the SQL of 8.12 with
the test database `analytics` in place of the production database, and a test password. It also creates the table
`analytics.conf_rows` with the 10 rows 1 to 10. Thus the source is `up`
(8.11), and the account follows the contract. The API suite of section 16 uses the same init file.

The service id of the run is `hune-conf`. The configuration of the run is `tests/conformance/config.hcl`. It has these
values:

- `access.mode = "authzen"`, `identity.audience = "hune-conf"`, and the test JWK Set of ProcurAuth in
  `public_keys_file`.
- `pdp.url = "http://127.0.0.1:8181"`, `plain_http = true`, a `token_file`, `timeout = "2s"` and `cache_ttl = "0s"`.
- A primary volume on the fixture.
- One block `clickhouse "conf"` with the URL `http://127.0.0.1:8123`, `user = "hune"`, `database = "analytics"` and
  `password_file = "/conformance/clickhouse_password"`. The file holds the test password of the init file.

The profile (`tests/conformance/profile.json`) gives `pep.service = "hune-conf"`, `answer_cache: false`,
`options_type: "folder"`, both test keys
(`Ed25519` and `ES256`), and these operations:

| Operation | Request | Action and node |
|---|---|---|
| `use_plain` | `GET /api/dashboards/conf-dash` | `dashboard.read` on `dashboard:conf-dash` below `folder:conf` |
| `use_ceiling` | `POST /api/dashboards/conf-dash/panels/1/query` with a fixed body (the etag of the fixture, a time range) | `dashboard.read`, ceiling `max_rows`, `ceiling_value` 1000 |
| `use_multi` | `PUT /api/dashboards/conf-dash` with the fixture document, its etag in `If-Match` and a fixed `Idempotency-Key` | `dashboard.write` on `dashboard:conf-dash`, other `source.query` on `source:conf` |

The profile has no `use_budget`, no `use_service`, no `mcp` and no `approvals`. The runner thus skips the cases with the
conditions `no_budget_action`, `no_mcp_endpoint`, `no_budget_session`, `no_approval_tool` and `no_approval_support`. It
also skips each step that needs `use_service`. U84 has only such a step, so the runner skips U84.
U02, U40 and U59 run their other steps.

With the cases U01 to U127 (ProcurAuth commit `f318e259013fdaa7e841936c2b8a172dc1b7699e`, the tag v1.1.1), the run MUST
give 127 cases: 65 passed, 0 failed and 62 skipped. The file `expected.txt` is empty.

| Skipped cases | Count | Reason |
|---|---|---|
| U18, U23, U25 to U31, U85 to U111, U114 to U117, U121 to U125 | 45 | The service has no MCP endpoint (`no_mcp_endpoint`). |
| U04, U19, U21, U22, U24, U32 to U35, U58, U62, U63, U67, U68, U126 | 15 | No action lists a budget (`no_budget_action`). |
| U84 | 1 | The profile has no operation `use_service`. |
| U127 | 1 | The profile has no operation `read_health`. |

The 65 cases that run are these: U01 to U03, U05 to U17, U20, U36 to U57, U59 to U61, U64 to U66, U69 to U83, U112, U113 and U118 to U120. They show
these properties of Hune:

| Property | Cases |
|---|---|
| The subject, the chain and the questions (P 3.3, P 4.1, P 4.5, P 9.6) | U01, U02, U07 to U11, U59 |
| A `subtree` answer never lets an operation run (P 4.4) | U12 |
| The action document: no token, no question, good, within the bounds (P 5.1 to P 5.3c) | U13 to U17, U112 |
| No usage for an action without a budget (P 6.4) | U20 |
| Ceilings: an unknown ceiling refuses, a known ceiling runs, a `false` answer ignores them, no opt-in member (P 7.3 to P 7.6) | U36 to U39 |
| No question for `access` (P 8.3) | U40 |
| Each rule of the identity token (P 9.3, P 9.4, P 9.7) | U03, U41 to U55, U118 to U120 |
| The PDP credential and the kept answers (P 10.0, P 10.1) | U56, U57 |
| The map of the refusals and of the failures (P 10.3, P 11, P 17.7a) | U06, U60, U61, U64 to U66, U113 |
| The connections to the PDP stay open (P 10.7, SHOULD) | U69 |
| The options route (P 5.6 to P 5.8) | U05, U70 to U83 |

A later commit of the runner can add cases. To change the commit, follow 11.10, then update the numbers of this
paragraph and `expected.txt`. A case that fails MUST have a line in `expected.txt` with its reason, and a reviewer
approves each line.

### 11.33 The PostgreSQL source in the access layer

A `postgres` block is a node `source:<id>` of the tree of 11.15, as a `clickhouse` block is. The access layer does not tell the two kinds apart. These rules apply:

- The questions of 11.21 are the same. `source.query` on S(s) covers a save, a draft run and a preview on a PostgreSQL source. `dashboard.read` covers a panel query and an options query.
- The PDP never gets the kind of a source. A rule of the PDP names the source id. The page gets the kind from `GET /api/sources` (6.22).
- The action document (11.18) holds one resource for each source block of both kinds. The description of the type `source` does not name a kind.
- The ceilings `max_rows` and `max_duration` apply as 11.24 says. Hune enforces them in its own client, with the checks of 8a.6 and 8a.7.
- A row check or a byte check that a ceiling lowered gives `403 ceiling_exceeded`, as 8a.8 says. A ceiling of 0 gives `403 ceiling_exceeded` before Hune sends the query.
- When the ceiling equals the key of the source, the ceiling is the cause (11.24).
- A grant of `source.query` gives each read that the role of the source permits (8a.13). It gives no write: the role cannot write, the session cannot, and the transaction cannot.
- The series U (11.32) needs no change. Its fixtures use ClickHouse.

### 11.34 The MongoDB source in the access layer

A `mongodb` block is a node `source:<id>` of the tree of 11.15, as a `clickhouse` block and a `postgres` block are. The access layer does not tell the kinds apart. The rules of 11.33 apply, with these changes:

- The questions of 11.21 are the same. `source.query` on S(s) covers a save, a draft run and a preview on a MongoDB source. `dashboard.read` covers a panel query and an options query.
- The PDP never gets the kind of a source, the database, the collection or the query. A rule of the PDP names the source id only. The page gets the kind from `GET /api/sources` (6.22).
- The action document (11.18) holds one resource for each source block of the three kinds. Its description of the type `source` does not name a kind.
- The ceilings `max_rows` and `max_duration` apply as 11.24 says. Hune enforces them in its own client, with the checks of 8b.6 and 8b.7. A ceiling of 0 gives `403 ceiling_exceeded` before Hune sends the query.
- A grant of `source.query` gives each read that the user of the source permits (8b.13), on the databases and the collections of the block. It gives no write: the user cannot write, and the connection sends only read commands.
- The query text, the collection and the database are not facts of a question. A rule of the PDP cannot permit one collection and refuse another. The operator uses one source block for each set of collections that needs another rule.
- The series U (11.32) needs no change. Its fixtures use ClickHouse.

### 11.35 A write into a read-only store

A configuration can have a primary store. Each other store is read-only (5.42). Hune never writes into a read-only store, whatever the PDP says.

1. The rule applies to a create, a save, a delete, a restore, a move and a change of a folder. It applies to a draft of a dashboard or of a folder (the preview and `validate_only`).
2. A move needs the primary store for the dashboard and for the target folder.
3. Hune finds the store of the target in the catalog, with no read of the store.
4. In the mode `open`, Hune answers `403 read_only_store` at once.
5. In the mode `authzen`, Hune asks one question first: `dashboard.read` on `N(uid)`, or on `F(folder)` for a folder. The question uses no kept answer (11.27).
6. When the PDP answers `false`, the answer is the `403 forbidden` of any other write of the route, with the member `action` equal to `dashboard.write` (11.30). The body does not show that the target is in a read-only store. The same holds for a uid or a folder that does not exist (11.23).
7. When the PDP answers `true`, the answer is `403 read_only_store`. Hune never asks `dashboard.write` for the target. A grant of `dashboard.write` on the service, on a folder or on a dashboard of a read-only store changes nothing.
8. The flag `access.write` of a folder and of a dashboard of a read-only store is `false`, with no question (11.25). The right to see the SQL needs `dashboard.write`. So nobody has it on a dashboard of a read-only store in the mode `authzen`.
9. The action document is the same as for any configuration with a primary store (11.18). It names the actions by type of resource, not by store. For a resource of a read-only store, the actions that matter are `dashboard.read` and `source.query` of the query routes. Hune never uses `dashboard.write` for such a resource.

The case K321 holds for a target of a read-only store. A subject with no right on it gets the same answer for a target that exists and for one that does not.

## 12. Security

### 12.1 Scope

This section gives the assets of Hune, its trust boundaries, its threat model and its controls. Each control has a
test. Section 16 gives the test layers. 12.18 lists what Hune does not protect.

The names of the tests in this section are the layers of section 16:

- "unit": the unit tests;
- "store suite": the conformance suite of the store, on both backends;
- "API suite", "ClickHouse suite", "browser suite", "failover test" and "importer suite";
- "series U": the ProcurAuth conformance run of 11.32;
- "policy job": the CI job `policy`;
- "environment test": the test of 13.24.

### 12.2 Assets

| Asset | Place | Why it matters |
|---|---|---|
| The data that the dashboards show | ClickHouse, PostgreSQL and MongoDB sources | The business data of the operator. A leak is the main risk. |
| The ClickHouse credentials | `password_env` or `password_file` of each `clickhouse` block | They give each read that the account permits. |
| The PostgreSQL source credentials | `password_env` or `password_file` of each `postgres` block | They give each read that the role permits. |
| The MongoDB source credentials | `password_env` or `password_file` of each `mongodb` block | They give each read that the user permits. |
| The dashboards and their history | The volume or PostgreSQL | The SQL and the layout. A bad change shows wrong data or runs heavy SQL. |
| The PostgreSQL credentials | `storage.postgres.password_env` or `password_file` | They give write access to each dashboard. |
| The PDP credential | `access.pdp.token_env` or `token_file` | A program with it can ask the PDP as Hune. |
| The identity of a person | The identity token, in the mode `authzen` | A forged identity gets the grants of another person. |
| The availability of the dashboards | The Hune process | People use the dashboards each day. |
| The capacity of ClickHouse | ClickHouse | A heavy query slows the ingestion and the other users. |
| The capacity of a PostgreSQL source | The PostgreSQL server | A heavy query slows the other users of the server. |
| The capacity of a MongoDB source | The MongoDB server | A heavy pipeline slows the other users of the server. |

### 12.3 Trust boundaries

```text
                 browser
                    |  HTTPS, with the session cookie of the edge
             +------v------+
             |    edge     |  reverse proxy and authentication (for example nginx with `auth_request`)
             +------+------+
                    |  HTTP on a private network; X-Identity-Token in the mode authzen
             +------v------+        server.ops_listen: /livez /readyz /metrics
             |    Hune    | <----- platform and Prometheus, on a private network
             +--+---+---+--+---+-------------+
                |   |   |      |             |
     HTTPS      |   |   | file |  HTTPS      |  HTTP or HTTPS
  ClickHouse <--+   |   +-> volume    PDP <--+       +--> OTLP collector
                    | TLS
               PostgreSQL
```

| Boundary | What crosses it | What Hune trusts | Controls |
|---|---|---|---|
| Edge to Hune (`server.listen`) | The requests of the browsers. | `open`: each request, because the edge decided. `authzen`: only a valid identity token. | Network rule (12.6), token check (11.8), write guard (12.7), limits (12.16). |
| Hune to ClickHouse | SQL, credentials, settings. ClickHouse returns the results. | ClickHouse applies the limits of the account. | TLS (12.14), read-only account (12.12), limits and ceilings (11.24). The page renders each value as text (12.8). |
| Hune to a PostgreSQL source | SQL, credentials, session parameters. The server returns the results. | The server applies the grants of the role and the limits of the session. | TLS (12.14), a read-only role, session and transaction (8a.3), closed grammar (7.25), limits and ceilings (11.24). The page renders each value as text (12.8). |
| Hune to a MongoDB source | A query tree, credentials, read preference. The server returns documents. | The server applies the roles of the user and the limits of the command. | TLS (12.14), a read-only user (8b.13), a closed list of commands (8b.5), a closed grammar (7.28 to 7.33), limits and ceilings (11.24). The page renders each value as text (12.8). |
| Hune to PostgreSQL | Documents and history. | PostgreSQL keeps the bytes. | TLS (12.14), a role with DML only (12.13), closed parse of each document at load (section 4). |
| Hune to the volume | Documents and history. | Nothing: people and git also write the files. | Closed parse, id rule, size limits, no symbolic link, one writer (section 5). |
| Hune to the PDP | Questions and the credential. The PDP returns the decisions. | The decisions of the PDP. | TLS (11.12), check of each answer, fail closed (11.28). |
| Hune to the OTLP collector | Spans with no SQL and no row data. | Nothing. | Best effort, bounded queue (14.9). |
| Monitoring to `server.ops_listen` | Probes and metrics. | Nothing. No token. | A private network only (12.6). No data of a dashboard in it (12.15). |

### 12.4 Threat model

| # | Asset | Threat | Control | Test |
|---|---|---|---|---|
| T1 | Data | A viewer reads a dashboard without a grant (`authzen`). | `dashboard.read` on N(uid) for each read and each panel query; the chain comes from the catalog (11.16, 11.21). | Series U; API suite: a panel query on a dashboard of another folder gives 403. |
| T2 | Data | A viewer learns that a uid exists. | Unknown-uid rule (11.23). The cursor of a list and the cursor of the options route hold the key of a visible item only (6.24, 11.26). | API suite: an unknown uid and a refused uid both give 403, with the same headers and the same body. A cursor holds no title and no uid of a hidden row. |
| T3 | Data | A viewer puts SQL in a variable value. The text has a form that ClickHouse and the lexer of Hune read in two ways, for example two `//` comments that each hold a `'`, so a value that Hune puts in a string is code for ClickHouse. | The server builds all SQL; context-aware escaping; free text becomes a literal only (section 7). The lexer of 7.5 agrees with ClickHouse 26.8.34.13: `//` starts a line comment, a line comment ends at LF only, and a heredoc tag can start with a digit. P, PQ and the second scan of 7.6a read the text with this lexer. The gap that the security review 3 found is closed (item 1). | Unit: seeded random tests "a value never leaves its literal"; the measured vectors of 7.5; the four texts of the review (a text value, a heredoc, a query parameter, an ad hoc value) give their errors. ClickHouse suite: the probe table of 7.5 against the server and the lexer of Hune, so an upgrade that changes the lexer fails; the four texts through the query route give 422 or keep the value out of the SQL, and no refused text reaches `system.query_log`. |
| T4 | Data | A viewer sends SQL of their own. | The query API takes no SQL; it names the dashboard and the panel (section 6, section 7). | API suite: a body with SQL text gives 400. |
| T5 | Data | An author runs any SQL with the account of Hune (`authzen`). | `source.query` for each save and each draft run (11.21); read-only account (12.12). | API suite: a save without `source.query` gives 403. |
| T6 | ClickHouse host | Authored SQL reads the network or the disk (`url()`, `s3()`, `file()`, `remote()`). | No `SOURCES` grant on the account (8.12, 12.12). | ClickHouse suite: each check of 8.12 gives its value. |
| T7 | Data | Authored SQL writes or changes data. | First keyword `SELECT` or `WITH`, one statement (section 7); account with `readonly = 2` and `SELECT` only (8.12). | ClickHouse suite: `INSERT` refused at save and at run; the checks of 8.12. |
| T8 | ClickHouse capacity | A heavy query. | Limits of each source, ceilings, profile constraints, admission (11.24, section 8). | ClickHouse suite: each limit gives its code. |
| T9 | ClickHouse capacity | A query runs on after the browser leaves. | Cancel on the close of the connection (section 8). | ClickHouse suite: the query leaves `system.processes` within 2 s. |
| T10 | ClickHouse capacity | Authored SQL lifts a limit, or cuts a result, with a `SETTINGS` clause. | Profile constraints `MAX` and `CONST` of 8.12. | ClickHouse suite: `SETTINGS max_execution_time = 3600`, `result_overflow_mode = 'break'`, `timeout_overflow_mode = 'break'` and `read_overflow_mode = 'break'` are refused. |
| T11 | Dashboards | A hostile page makes the browser of a person save or delete (CSRF). | Write guard (12.7). | API suite: a cross-site `POST` gives 403; a `text/plain` body gives 415. |
| T12 | Data, identity | Script injection through a title, a description, a value or a data link (XSS). | Text rendering only; CSP with no inline script; link schemes `http`, `https` and relative only (12.8, section 9). | Browser suite: payload fixtures; the count of CSP violations is 0. |
| T13 | Dashboards | Framing of the page (clickjacking). | `frame-ancestors 'none'` and `X-Frame-Options: DENY` (12.8). | API suite: header check on each route. |
| T14 | Identity | A forged, old or replayed token (`authzen`). | Signature, `alg`, `typ`, `iss`, `aud`, `exp`, `max_age`, skew (11.8). | Series U: U03, U41 to U55. |
| T15 | Identity | A client reaches Hune without the edge (`authzen`). | Network rule (12.6); warning for a wildcard `server.listen` (13.15). | Unit: the warning. Operator check: a NetworkPolicy of the cluster that admits only the edge (section 17 gives none). |
| T16 | Identity | A false identity header in the mode `open`. | The mode `open` reads no identity header (11.2). | API suite: a request with `X-Identity-Token` in the mode `open` has the subject `-`. |
| T17 | Data | A PDP outage lets operations run. | Fail closed: 503 (11.28). | Series U: U06, U64 to U66. |
| T18 | Data | A revoked grant stays active. | `cache_ttl` bounds the kept answers (11.27). | Unit: a kept answer ends after `cache_ttl`. |
| T19 | Credentials | A secret goes to a log, an error or `print-config`. | `_env` and `_file` only; `<hidden>`; no secret in a URL (12.10, 12.15). | API suite: the logs of the whole run hold no test secret. Unit: `print-config` shows `<hidden>`. |
| T20 | Credentials, data | An environment variable changes the behaviour (`OTEL_*`, proxy, CA store). | The environment is captured and cleared at start; no crate reads a proxy variable (13.23). | Environment test (13.24). |
| T21 | Dashboards | A path leaves the volume (`..`, symbolic link). | Ids only; `openat` with `O_NOFOLLOW`; links refused (section 5). | Store suite: link fixtures. |
| T22 | Dashboards | Two editors save at the same time (lost update). | `If-Match` etag; 409 (section 5). | Store suite: N parallel saves on one etag, exactly one wins. |
| T23 | Dashboards | A save repeats after a failover (two versions). | `save_id` as `Idempotency-Key` (section 5). | Failover test. |
| T24 | Dashboards | A bad file arrives by `git pull`. | Closed schema; the document is listed as not valid and not served (section 4, section 5). | Store suite. |
| T25 | Dashboards | A hostile Grafana export. | The importer is a pure function with limits; its output passes the closed reader (section 15). | Importer suite. |
| T26 | Availability | A large result fills the memory. | Memory budget for results; byte limit of each source (12.16). | ClickHouse suite: a result above the limit gives its code. |
| T27 | Availability | Many slow or idle connections. | `server.max_connections`, header and body timeouts, `server.idle_timeout` (12.16). | API suite. |
| T28 | Availability | Large or deep request bodies. | Body limits for each route; JSON depth 64 (12.16). | API suite: 413 and 400. |
| T28a | Availability | Many large request bodies at the same time fill the memory. | `MAX_INFLIGHT_DOC_BODIES` and `MAX_INFLIGHT_SMALL_BODIES` (12.16). | API suite: a create while 8 large bodies are in flight gets `503 server_busy` (section 16). |
| T29 | All | A malicious or unexpected crate. | `cargo deny`, `deps.lock.txt`, exact pins, one git source (12.17). | Policy job. |
| T30 | All | A changed release archive. | `SHA256SUMS`; the same sources give the same bytes (section 17). | Release workflow check. |
| T31 | Data, credentials | A person in the middle of a link to ClickHouse, PostgreSQL, the PDP or the collector. | `rustls` with the CA file of the link and a name check; no key skips the check (12.14). | ClickHouse suite and store suite: a wrong CA refuses the connection. |
| T32 | Data | A probe or `/metrics` shows data to the users. | Second listener; labels with no uid, subject or SQL (14.2, 14.8). | Unit: the label set of each metric. |
| T33 | Dashboards | The runtime role of PostgreSQL changes the schema. | DML grants only; `migrate-store` uses another role (12.13). | Store suite: DDL with the runtime role fails. |
| T34 | Data | Authored SQL writes or changes data on a PostgreSQL source. | Four layers: the role (R1 to R6 of 8a.12), `default_transaction_read_only` on the session, `BEGIN READ ONLY`, and the closed grammar (7.25). | PostgreSQL suite: PS07 and PS08; the unit tests of 7.25. |
| T35 | Data | A second statement in the text of a query on a PostgreSQL source. | One Parse message of the extended protocol (8a.5); the check of 7.25. | PostgreSQL suite: PS09. |
| T36 | PostgreSQL host | Authored SQL runs code or reads files of the server (`DO`, `COPY PROGRAM`, `pg_read_file`, `lo_import`). | The first word and the refused functions (7.25); R2 and R7 of 8a.12; the revoke of advice A1. | PostgreSQL suite: PS08. |
| T37 | Other servers | Authored SQL makes the server reach another server (`dblink`, a foreign data wrapper). | The refused functions; R7 and R8; Hune connects to one `host` and one `port` only. | PostgreSQL suite: PS08; the environment test (13.24). |
| T38 | PostgreSQL capacity | A heavy query, or `pg_sleep`. | The limits of time, rows and bytes; the client deadline with a cancel request; the defaults of the role (8a.7, 8a.13). | PostgreSQL suite: PS04. |
| T39 | PostgreSQL capacity | A query runs on after the browser leaves. | A cancel request and the close of the connection (8a.7). | PostgreSQL suite: PS05. |
| T40 | Data | Authored SQL changes the session (`set_config`, `SET`) to lift a limit or to leave the read-only mode. | The closed grammar; `DISCARD ALL` after each query; the setup check on each new connection; a read-only transaction cannot become read-write after its first query. | PostgreSQL suite: PS07. |
| T41 | Data | A value leaves its literal in PostgreSQL SQL: a backslash, an `E` string, a dollar quote, an operator. | The dialect of 7.20 to 7.24 and the property P. | Unit: the random tests of 16.4; PS03. |
| T42 | Credentials | The role of a source has more rights than a read. | The probe refuses it (R1 to R8 of 8a.12). | PostgreSQL suite: PS08. |
| T43 | Availability | A large result, or one large row, fills the memory. | The cap on rows and bytes and the memory budget (8a.6); the known limit of one row (8a.15). | PostgreSQL suite: PS04. |
| T44 | Data | Authored query text writes or changes data on a MongoDB source (`$out`, `$merge`, a write command). | Three layers: the user with the role `read` only (M2, M3 of 8b.12), the closed list of commands (8b.5), and the closed grammar (7.31, 7.32). | MongoDB suite: MS07 to MS09; the golden vectors of 16.3. |
| T45 | MongoDB host | Authored query text runs JavaScript on the server (`$where`, `$function`, `$accumulator`, `mapReduce`). | The grammar refuses each name and each `$expr` that holds one (7.32); no `$code` form (7.28). | Unit: the vectors of 7.32; MS07 and MS18. The setting `security.javascriptEnabled` is advice (8b.13). |
| T46 | Other collections | A `$lookup`, a `$unionWith` or a `$graphLookup` reads a collection or a database outside the lists. | The lists `databases` and `collections`; `from` never an object; `$graphLookup` refused (7.33). The role reads only the databases of the key (M2, M3). | Unit: 7.33; MS07 and MS08. |
| T47 | Data | A value of a variable becomes an operator, a stage, a key, a field path or a collection name. | A value is a typed leaf of the parsed tree and never text of the query. The property PM and the name places (7.34, 7.35). | Unit: the random tests R10 of 16.4; MS03. |
| T48 | Other servers | A client or a server steers Hune to a host that the operator did not list (SRV lookup, replica set discovery). | No `mongodb+srv`; one `host`, or the complete list of members; the probe check M4 (8b.4, 8b.12). | MongoDB suite: MS10 and MS12; the environment test (13.24). |
| T49 | Credentials | The user has more rights than a read (a cluster role, a write action, a role on another database). | The probe reads `connectionStatus` with `showPrivileges` and refuses the user (M1 to M3 of 8b.12). | MongoDB suite: MS11. |
| T50 | MongoDB capacity | A heavy pipeline, a regular expression or a large sort. | `maxTimeMS`; the client deadline with a kill of the cursor; `allowDiskUse: false`; the cap on rows and bytes (8b.6, 8b.7). | MongoDB suite: MS04, MS05. |
| T51 | Availability | A large result, or a large reply, fills the memory. | The stage `$limit` of `R + 1`; the cap on rows and bytes; the memory budget; the known limit of one reply (8b.6, 8b.15). | MongoDB suite: MS04. |
| T52 | Data | The query reads another field of a document through a variable or a field path that a viewer chooses. | The property PM: a free value never starts with `$` (7.35, PM2). | Unit: R10; MS03. |
| T53 | Data, identity | Script injection through the text of a text panel: raw HTML, a `javascript:` link, an image, an event attribute, a `style` attribute (XSS). | No HTML parser: the text is data and the page builds 24 element names from a constant table. The links pass the check of 4.23 and have `rel="noopener noreferrer"`. No image, no resource, no `style`, no attribute from the text. CSP with no inline script (9.19b). | Browser suite: B103 in three browsers with 40 payloads. Front units: FE-29 and the property tests of 16.10a. |
| T54 | Availability | A hostile text makes the browser work too long: deep nesting, many open emphasis marks, a long list of brackets, a large table (the same texts hang Grafana for more than 40 s). | The parser is linear with 6 steps for each character at most. Limits for the size, the depth, the nodes, the columns and the size after the replacement. The panel shows a message and no text (9.19b). | Front units: the step bound on 12 hostile families at 3 sizes. Browser suite: B105. |
| T55 | Data, identity | The value of a variable makes Markdown or HTML: a value such as `<img src=x onerror=alert(1)>`, `[x](javascript:alert(1))` or `**x**`. | The page parses the lines first and replaces the placeholders in the text leaves after. A value is text and never a construct. No replacement in a destination (9.19b). | Front units: FE-30 (the same shape with a hostile value). Browser suite: B104. |
| T56 | Data | The text makes a request to another host (a beacon by an image, a style sheet, a frame). | No element that loads a resource. CSP: `default-src 'none'` and `img-src 'self' data:` (9.4). | Browser suite: B103 records each request. |
| T57 | Availability | A result with a huge number of states makes the page slow: 100,000 states that change at each sample, or a status history of 100,000 points. | The cap `MAX_TIMELINE_SEGMENTS` of 10,000 segments or cells, and the rule of half the width for a status history. The panel draws nothing above the cap and shows a message (9.19c). | Front units: the cap and the time of the model. Browser suite: B114. |
| T58 | Data, identity | A state text, a mapping text or a row name carries markup, a script or a very long text. | The page draws the text on a canvas with `fillText` and builds the tooltip and the legend with `textContent`. The text is cut to the box. No HTML is built from data (9.1). | Browser suite: B112 and B115 with hostile texts. Front units: FE-33. |
| T59 | Data | A viewer sends SQL through the annotation route. | The route has the index and the etag only. It names a saved annotation. The body has no SQL member (6.21a, 7.37). | API suite: a body with an SQL member gives 400. The route has no way to run another text. |
| T60 | Data | The SQL of an annotation reads or changes more than a panel can. | The same pipeline: the property P, the statement check of the kind, the read-only account, the admission and the limits (7.37). `dashboard.read` and the ceilings (11.21, 11.24). A `postgres` source adds the statement check of 7.25 and the read-only role of 8a.13. A `mongodb` source adds the grammar of 7.30 to 7.32, the property PM (7.35) and the role of 8b.13. | API suite: the tests of the annotation route on each kind of source, with the cases of the options route (16.3). Container suites: PS17 and MS20 (16.17, 16.9b). |
| T61 | Data, identity | Script injection through the title, the text or a tag of a mark (XSS). Grafana renders HTML in the text. | The server refuses the form of an HTML tag and the control characters (7.37, rule 9). The page writes each text with `textContent`, never with `innerHTML`, and draws the marks on a canvas (9.19d). The CSP stays as it is (9.4). | Unit: 42 hostile texts give `bad_chars`. Front unit: the card is built with a fake document, and no element comes from data. Browser case B122. |
| T62 | Data | Text that looks like another text: a bidirectional control or a control character changes the order or hides a part of a text. | The server refuses U+202A to U+202E, U+2066 to U+2069, U+200B to U+200F, U+061C, U+2028, U+2029, U+FEFF and the controls (7.37). | Unit: the characters of the list. |
| T63 | Availability | An annotation query returns a huge number of marks, or the page runs a query for each panel or for each hover. | 8 annotations at most, 1,000 rows at most for each query (a fail-closed error), 2,000 marks at most for each panel (a fail-closed note), one query for each annotation and each trigger, queued after the panels (4.35, 7.37, 9.19d). | Server tests: 1,000 and 1,001 rows on a `clickhouse` and a `mongodb` source, and on real servers in PS17 and MS20. Browser cases B121 and B123: the count of requests and the cap. |
| T64 | Data | A viewer sends SQL, a source or a query that the document does not hold. | The request names the panel and a ref. The body has no SQL and no source member. The server reads the query from the catalog (6.20, 7.2). An unknown ref gives `404`. | API suite: a body with an SQL member or a source member gives 400. The route has no way to run another text. |
| T65 | Data | A viewer runs a query that its author hid, for example a draft or an expensive query. | The server refuses a hidden query with `400 bad_request`, after the question `dashboard.read` and before any byte to a source (6.20, 7.2). | API suite: case A71. The page never asks for a hidden query: browser case B131. |
| T66 | Data | The second query of a panel reads more than a panel query can, for example from a source of another trust level. | The same pipeline for each query: the property P, the statement check of the kind of its source, the read-only account of that source, the admission and the limits. A save asks `source.query` on the source of each query, hidden queries included (11.21). | API suite: A72 and A73. ClickHouse, PostgreSQL and MongoDB suites: one panel with two queries in each (C14, A74). |
| T67 | Availability | A panel with 8 queries, or a dashboard with 400 queries, floods a source. A repeat multiplies the queries. | 8 queries for each panel and 400 for each document (a fail-closed error at save). A panel with a repeat has one query. A trigger starts 519 requests at most, and the page sends 6 at the same time. The admission of 8.8 bounds the load of each source and each subject (9.8d). | Core tests: the limits at their value and one above. Browser case B134: the count of requests and the 6 at the same time. |
| T68 | Availability | A join of frames makes the page slow or fills its memory. After `partition_by_values`, a panel joins up to 100 frames, and each frame adds its fields. | `MAX_JOIN_ROWS` = 200,000 rows in all the frames of a panel, checked before the work. 8 queries at most. After the time axis and before it builds a field, the page checks `MAX_TRANSFORM_FIELDS` and `MAX_TRANSFORM_CELLS` (9.21a, rule 13). The panel shows an error above a limit and draws nothing (9.22a). | Front units: the cap and the time of the join, and the fields and the cells of the join (FE-39, 16.10a). Browser case B133. |
| T69 | Availability | A result with a huge number of values, cells or buckets makes the page slow or full of memory: 5 million values, a bucket size of 1 ms over a year, or a grid of 10,000 by 10,000 cells. | The caps `MAX_BUCKET_VALUES` (500,000), `MAX_HEAT_CELLS` (250,000) and `MAX_HIST_BUCKETS` (4,000), checked before any array is made. The panel draws nothing above a cap and shows a message. The page never cuts or samples the data (9.19e, 9.19f). | Front units: the caps and the time of the model (FE-46). Browser suite: B146. |
| T70 | Data, identity | A field name, a bucket label, a unit text or a series name carries markup, a script or a very long text. The names reach the axis labels, the card, the legend and the description. | The page gives the texts to the engine as data. The engine escapes each text with `escapeHtml` in the card and in the legend, and it draws the axis labels on a canvas. The hidden description and the colour bar labels use `textContent`. No HTML is built from data (9.1). | Browser suite: B141 and B145 with hostile names. Front units: FE-46 with a fake `document`. |
| T75 | Data | A viewer sends a value that is not an option for a closed `custom` variable, to put author-level text in the SQL. | The server refuses each value that is not an option (7.7). The check uses a set, with a time bound. The refusal runs before any source query. | API suite: A90 and A91. Unit: the set, 10,000 by 10,000 in 250 ms or less. |
| T76 | Data | An author or a viewer turns an open `custom` variable into code: a typed value in a quoted name, in code, or with `$` in MongoDB. | An open `custom` variable is free text for the server at save and at run (7.8). A quoted name gives `not_permitted` at save. A value in code is a number only. PM2 refuses a value that starts with `$`. The properties P, PG and PM do not change. | API suite: A92 to A95. ClickHouse suite: C20 and C21. |
| T77 | Data | A viewer sends a value that is not an option for a closed `query` variable and believes that the dashboard bounds it. | None on the server, and the text says so: the rule is a rule of the page (4.8, 12.18). The value is free text under P, PG and PM. The author bounds the value in the SQL of the panel. | API suite: A96 (the same expansion with the member `true` and `false`). Unit: the seeded random tests of 16.4 with both values. |
| T78 | Availability | A request with 10,000 values and a variable with 10,000 options makes the server compare 10^8 pairs on the request task. | The check uses a set (7.7). The options query keeps its caps (7.14). A request has 10,000 values at most and 256 KiB at most (6.27). | Unit: A97, 250 ms or less. Front unit: FE-48, 10,000 by 10,000 in 100 ms or less. |
| T79 | Data, identity | A typed value, or a value in the URL, makes markup or a script in the control, in the notice, in a title or in the preview of the editor (XSS). | The page writes each value with `textContent`, never with HTML. The server never sends a typed value back. No `innerHTML` in `vars.js` or in `var_examples.js`. CSP as in 9.4. A notice cuts a time or a zone of the URL to 64 characters (9.5). | Front unit: FE-49 (40 hostile texts, a fake `document`). Browser cases B163 and B212. |
| T80 | Data | The import maps the default `true` of Grafana to an open `custom` variable that the SQL uses in a quoted name or in code. | The importer keeps the member absent for such a variable, and the report has a row `changed` (15.6). The save check of 7.18 would refuse the quoted name in any case. | Importer test I01 with the fixtures of 16.13. Browser case B168. |
| T85 | Availability | A transform makes a very large result: `join_by_field` in the mode `inner` with equal keys multiplies the rows, `partition_by_values` makes thousands of frames, `merge` compares rows without end. A join of 100 frames, a long-to-wide of distinct label values or `grouping_to_matrix` of distinct keys makes millions of empty cells. | A step that makes new cells checks its result before it builds a cell: `MAX_TRANSFORM_FIELDS`, `MAX_TRANSFORM_ROWS` and `MAX_TRANSFORM_CELLS` (4.35, 9.21a, rule 13). `MAX_TRANSFORM_CELLS` is the key `server.transform_max_cells` (13.8), with a fixed bound of 100,000,000 cells (900 MB or more). The page reads it from the session (6.10), and without a valid value these steps fail closed. These steps are `join_by_field`, `merge`, `grouping_to_matrix`, the long-to-wide (9.22) and the join on time (9.22a). After each transform, the page checks `MAX_TRANSFORM_ROWS`, `MAX_TRANSFORM_FIELDS` and `MAX_TRANSFORM_FRAMES`. A compare of two rows of `merge` is one step for each field (`MAX_MERGE_STEPS`). A higher count is an error and never a cut. The query limits of 8.5 bound the input. | Front units: FE-75 (the caps, the time, a configured cap, the fail closed and no copy of the number in `web/js`), FE-13, FE-14 and FE-39 (the caps before the work). Rust units: the key and the session member. Browser case B185. |
| T86 | Data, identity | A new name (`organize`, `rename_by_regex`), a converted value or a name from a label makes markup or a script (XSS). | The page writes each name and each value with `textContent`, never with HTML. The rename text is plain text. No `innerHTML` in `transform_more.js` or in the forms. CSP as in 9.4. | Front unit: FE-70 (hostile names). Browser case B189. |
| T87 | Availability | An author writes a pattern that makes the regular expression engine of the browser slow (`filter_fields_by_name`, `rename_by_regex`, an override, a mapping, `filter_by_value`, the `regex` of a variable). A person who writes data puts a long name or a long cell that makes a pattern of the page slow. | The pattern follows 4.24: no look-around and no back reference. The names hold data (a label value of a long-to-wide, a MongoDB column name), and `filter_by_value` and a mapping read cells. Thus the page runs a pattern only on a text of 1,024 code units or fewer, the value of `MAX_VALUE_BYTES`. A longer text never matches (9.14). `filter_by_value` compiles its pattern one time. The author has the right to write the document, and a pattern with nested repetitions stays a risk of the author (12.18). The patterns of the page that read cells and data names match a run of digits in one way only: the number test of `convert_field_type`, of a heatmap row name and of a table filter (E28). The exact compare of decimal texts uses string steps (E29, 9.24). | Front unit: FE-70 (a slow pattern on 1,024 names, 50 ms). Front unit: `page_security_v3` (a cell of 40,001 characters in 50 ms, a sort of PostgreSQL `numeric` texts of 16,385 characters, and a static test that refuses the quadratic shapes in `web/js`). Front unit: `page_security_v3_low` (a label and a cell of 1,000,000 characters in each place of 9.14, 50 ms or less each, and the result for a text of 1,025 code units in each place). |
| T88 | Data | A viewer sends an `interval` value that is not an item, for example `1m'; DROP`, or a URL `var-iv=7m'`. Grafana accepts such a value and puts it in the SQL. | The server compares each value with the items of the document and with `$__auto` (7.7). Another value gives `422 variable_value_refused` before any source query. The importer and the validation check each item against the grammar of 4.10a. | API suite: A105. Unit: the hostile set against the set check. ClickHouse suite: C33. |
| T89 | Data | A viewer sends a value for a `constant`, or a URL `var-k=...`, to change author text in the SQL. | The server takes the `value` of the document and refuses a different member (7.7). The page never reads the URL value of a constant. | API suite: A106. Browser case B193. ClickHouse suite: C34. |
| T90 | Data | An author puts hostile text in a `constant` or in an interval item. | The text is author text, so its trust is the trust of the SQL. In a string, the escapes of 7.8 hold the text in its literal. The statement check (7.12) and the `readonly` account stay the boundary. An interval item has only letters and digits. | ClickHouse suite: C32 (a hostile value in a string, a quoted name and code). Unit: FE-68. |
| T91 | Availability | A very large or very small time range, or a step count of 1, gives an odd text for "Auto". | The text is a row of the table of 7.4a. The step count is 1 to 1,000 and the range is checked by 7.3. No free text and no cost grow with the input. | Unit: the 59 bound cases and the extremes of the range (16.3). |
| T92 | Data, identity | An interval item, a constant value or the text of "Auto" makes markup or a script in the control, in a title or in the URL (XSS). | The page writes each value with `textContent`, never with HTML. The URL writer encodes the value. No `innerHTML` in `vars.js`. CSP as in 9.4. | Front unit: FE-69 (40 hostile texts). Browser case B198. |
| T93 | Availability | A token holder with no grant sends large or hostile documents to the write routes, so that the validator runs before any decision. | The decision comes before the validation (11.20). The work before the decision is linear: the body read, the syntax check and the pre-pass of the sources (11.21). A subject holds 2 document bodies and 2 validations at most (12.16). | API suite: a subject with no grant gets 403 on the create, the save, `validate_only`, the preview and the restore, and the validator never runs. The cap of a subject gives 503. |
| T94 | Availability | A client sends heavy documents and closes the connections, so that the work goes on while new requests take the places. | The validation runs in `spawn_blocking` behind `MAX_INFLIGHT_VALIDATIONS`. The work keeps its permit and the place of its body until it ends (6.27, 12.16). | API suite: a ninth document while 8 dropped ones still run gets 503. |
| T95 | Availability | A heavy stored version blocks the async workers through a restore or a read of the history. | The validation of a version runs in `spawn_blocking`, with a permit that comes before the store read (6.17, 6.18, 6.27). The names of 6.14 are made when the catalog reads the document (5.8). | API suite: `/api/session` stays below 100 ms while 2 times the workers restore and read a heavy version. The first read of a dashboard does not make the names. |
| T96 | Availability | A document with a huge number of errors or warnings makes a huge answer. | The list of errors stops at `MAX_ERRORS` items and the list of warnings at `MAX_WARNINGS` items. The last item says that the list stops (4.35, 4.36, 6.13). | API suite: 214 errors and 2,048 warnings give 200 items each. |
| T97 | Data | A fault of the pre-pass skips a `source.query` question of a save. | The check after the validation: a source that the decision did not ask gives 500, and Hune writes nothing (11.21). | API suite: an injected pre-pass that misses a source gives 500 and no write. Unit: the pre-pass equals the sources of each valid document of the corpus. |
| T98 | Data | A subject without a grant compares the `403` of a uid or a folder that exists with the `403` of one that does not exist. The member `action` can show it: `dashboard.read` for an unknown uid, `dashboard.write` or `source.query` for a uid that exists. | The member `action` is the use of the route (11.30). Both paths do the same work before the decision: no store read, no validation, and the pre-pass for a create and a save (11.20, 11.23). On a write route, both paths send the same number of PDP requests with the same others, and neither uses a kept answer: the question of 11.23 and the questions of a write route are never kept (11.23, 11.27). This residual of the review of 2026-11-21 is closed. | API suite: on each route that names a uid or a folder, the two answers are equal byte for byte, and neither request reads the store or validates. On each write route, with `cache_ttl` of 0 s and of 30 s, a subject with no grant and a subject with `source.query` send three requests for each item: each request sends the same PDP requests and questions, and reads and writes no kept answer. On a read route, the question of 11.23 goes to the PDP at each request. |
| T99 | Availability | Many restores with a wrong `If-Match` hold the store lock while the store validates a heavy version again. One measure gave 11.6 s for the slowest of 30 restores at the same time. | A restore validates its version one time, in `spawn_blocking`, before the store. Under its lock, the store reads only the metadata of the version and the current document, then it compares `If-Match` and the folder (6.18, 12.16). | API suite: one restore gives one validation. A move ends within 200 ms while 16 clients restore a heavy version with a wrong `If-Match`. Store suite: SC-26 and SC-28. |
| T100 | Data, availability | A cell value of a column with `inspect` carries markup or a script (XSS), or it is very long or very deep, so that the drawer "Inspect value" blocks the page. | The page writes the value with `textContent` only, never with HTML. Only `JSON.parse` reads the value, and the indent keeps the tokens. The block has a limit of 1,048,576 UTF-16 code units, the indent stops at the same limit, and a text with a 65th level of objects and arrays gets no indent (9.20). CSP as in 9.4. | Front unit: `table_inspect.test.mjs` (hostile values with a fake `document`, the limits). Browser case B213. |
| T101 | Data | A viewer sends a value for a query parameter of ClickHouse (`{name:Type}`) to change the statement. An author writes a loose form, a type such as `Identifier`, or author text that makes a new form. | The value goes in the URL as `param_<name>` and never in the SQL text (property PQ). Only the strict form, with a closed list of 8 types and a shape for each kind of variable, at save and at run. A second scan of the final text refuses a form that the stored text has not. Each value passes the grammar of its type before any byte to the source. The caps are 32 names, 1,000 items and 4,096 bytes. A status 414 or 431 of a proxy changes no state of the source (7.6a, 8.5). | Unit: each loose form, the grammar of each type, PQ with seeded random values, a forged `{x:Identifier}`. ClickHouse suite: C40, the round trip of a hostile set for each type. API suite: A140 and A141. |
| T103 | Data | A viewer sends ad hoc filters (`$__adhocFilters`, 7.10a) to change the statement, to read a column that the author did not list, or to find hidden values by bisection. A git document or a change of the configuration names a column that the operator removed. | The key is only compared, byte for byte, with the keys of the variable and with `clickhouse "<id>".adhoc_columns`; the identifier text comes from the configured string, in backquotes. The operators are `=` and `!=` only, so no bisection with `<` or `>`. Each value is a string literal with the escapes of 7.8, and the property P checks it. The caps are 16 filters and 1,024 bytes for each value. The macro MUST be one term of the top-level `AND` of the `WHERE` or `PREWHERE` clause of the outer statement, so a filter never adds a row that the rest of the clause refuses. A `LIMIT` or an aggregate can still show other rows (7.10a, rule 4). A fault gives `422` before any source query, and a filter is never ignored. The log holds the count and the keys, never a value (4.10c, 7.7a, 7.10a, 13.12). The `sql_sha256` of `query.run` is a fingerprint of the values: treat the log as `system.query_log` (12.15). | Unit: seeded hostile values through the macro (P holds, each literal decodes to its value), a test that no byte of the request key reaches the SQL, the place rule, the save checks. API: an unknown key, an unknown operator and 17 filters give 422 and no source query; a change of the list fails closed. ClickHouse suite: the comparison of a string literal with each column type. Browser: B218, B219. |
| T104 | Data, availability | A log line of the logs panel (4.33f, 9.19h) holds markup, a script, a URL, a bidirectional control or a very long text. The text comes from the data source, so any person who writes logs can put such a text on the page of a viewer. A frame of many lines or of very long lines can also block the page. | The page writes each time, level, body, field name and field value with `textContent` only: no markup, no attribute and no link come from the text. Each text has `unicode-bidi: isolate`, so a control character changes the order inside its own element only. The level is a closed value of 7.16e, and its colour comes from a class of a closed list. The page draws 10,000 lines at most, 10,000 characters of a body or a value, and 2,000,000 characters of all the bodies (9.19h, section 4). The level scan is one linear pass. The caps of 8.3 bound the frame before it comes to the page. | Front: FE-81 and the hostile lines of `logs_panel.test.mjs` (no markup, no event attribute, the caps, the time of 10,000 lines). Browser: B221. |
| T105 | Data, privacy | A dashboard sends data of the page to a host outside Hune through the map: a tile URL in the document, a value in the URL of a tile, a view that follows the data, or a policy that permits more than the tile host. The tile host learns the address of each viewer and the area of the map. | The feature is off by default. The tile source comes only from the closed block `geomap` (13.12c): value form only, no environment variable, the strict grammar of 13.13. The document holds no tile URL (4.33g). The policy gets one source from the parsed parts only, in `img-src`, and `connect-src` does not change (12.8). The URL of a tile holds three whole numbers only. With tiles, the view `fit` does not apply. A tile is an image with `crossOrigin` "anonymous" (no cookie) and `referrerPolicy` "no-referrer". W9 and the privacy note of 12.8b tell the operator. | Unit: each refusal of the grammar (the shared list FE-85), the header from the parts, the header of 12.8 byte for byte with no block, the session member. Front unit FE-85. Browser cases B226 and B227 (no Referer, no cookie, z/x/y only, no request off the page origin with no block). |
| T106 | Availability, integrity | A hostile or broken tile host sends large images, images of another size, slow answers or redirects, or a map makes many requests to the host, which then refuses the instance. | The page asks only for the tiles of a map in the viewport, 200 ms after the last pan or zoom, with 6 requests in flight on a page, 256 tiles for one view, no prefetch and a cache of 256 tiles. It does not ask again for a failed tile in the same view, also after a refresh or a new result: the view and its failed tiles stay through a new draw of the map. It drops a tile that is not 256 x 256. The cache keeps decoded bitmaps, 64 MiB at most: the image and its downloaded bytes go after the decode. A host with no CORS header gives no tile. The policy permits only the path of the template, so a redirect to another host gives no tile. An image runs no script (9.19i). Residual risk: the page cannot cap the downloaded bytes of a tile, because it loads the tile as an image and `connect-src` stays `'self'`. A hostile tile host can use the bandwidth of each viewer (12.8b). | Front unit FE-85 (the queue, the cache of bitmaps and their close, the caps, the size check, and the real panel frame through the refreshes). Browser case B227 (6 in flight, the delay after a pan, CORS fail closed, a tile of 512 px, one request for each failed tile through refreshes of 10 s, and the memory of 24 tiles of 16 MiB). |

### 12.5 The open mode warning

At each start in the mode `open`, Hune writes one `config.warning` line with `key = "access.mode"` and this message:
"access.mode is open: Hune checks no identity and asks no PDP. A proxy that authenticates the users must guard each
route of server.listen."

In the mode `open`, each request that reaches `server.listen` can read each dashboard and run its queries. With
a primary store, it can also change each dashboard of this store. A change of the SQL runs that SQL with the ClickHouse
account of Hune. The mode `open` is thus safe only behind a proxy that authenticates each request.

The mode `open` fits a deployment where the proxy guards the whole prefix with `auth_request`. A proxy can guard the prefix
`/grafana/` of a Grafana instance in the same way.

### 12.6 Deployment rules

These rules apply to each deployment. The examples of section 17 follow them.

1. In the mode `open`, a proxy that authenticates the users MUST guard each route of `server.listen`.
2. In the mode `authzen`, Hune MUST receive requests only through the edge (P 9.4). Use a firewall, a network policy or
   a private network.
3. Do not publish `server.listen` or `server.ops_listen` to an untrusted network.
4. The proxy MUST NOT route a public path to `server.ops_listen`. Only the container platform and the monitoring reach it.
5. The proxy terminates TLS. The link from the proxy to Hune is on a private network or on the loopback interface.
6. The proxy passes the header `Host` unchanged, or sets `X-Forwarded-Host`. The write guard needs it (12.7).
7. The proxy adds `Strict-Transport-Security`. It does not replace the headers of 12.8.
8. The proxy does not cache the answers of `/api/`.
9. The keep-alive timeout of the proxy to Hune is less than `server.idle_timeout`.
10. In the mode `authzen`, the edge replaces the token header that a client sends (P 9.1).

An example with `server.base_path = "/dashboards/"`. Copy the `auth_request` lines of the location that guards your Grafana instance:

```nginx
location /dashboards/ {
    auth_request       /auth/verify;
    proxy_pass         http://hune:8080;
    proxy_http_version 1.1;
    proxy_set_header   Connection "";
    proxy_set_header   Host $host;
    proxy_set_header   X-Request-Id $request_id;
}
```

The port of `server.ops_listen` is not in a `location` of nginx. The compose file publishes it at most on the loopback
address of the host (`127.0.0.1`).

### 12.7 The write guard

A page of another site must not use the browser of a person to change a dashboard or to run a query. A proxy with `auth_request` and a ProcurAuth edge both authenticate the browser with a cookie. A hostile page can make the browser send
that cookie. Thus the guard applies in both modes.

Each `POST`, `PUT` and `DELETE` on `server.listen` passes the guard before Hune reads the body. This includes
the panel query, the variable options and the preview.

- When the request has `Sec-Fetch-Site`, its value must be `same-origin` or `none`. Else the answer is
  `403 cross_site_request`.
- When the request has no `Sec-Fetch-Site` but has `Origin`, the host and port of `Origin` must equal the header
  `Host`. When the request has `X-Forwarded-Host`, its first value replaces `Host` for this check. `Origin: null` fails.
  Else the answer is `403 cross_site_request`.
- A host without a port has the default port of the scheme of `Origin`: 80 for `http` and 443 for `https`. Thus
  `Origin: https://hune.example` equals `Host: hune.example:443`.
- A request with a body must have `Content-Type: application/json`, with an optional parameter `charset=utf-8`. Else the
  answer is `415 unsupported_media_type`. A page of another site cannot send this type without a CORS preflight.
- No answer has an `Access-Control-Allow-*` header. `OPTIONS` gets `405` with no CORS header, so no preflight ever
  succeeds.
- Hune sets no cookie.

A client that is not a browser sends neither `Sec-Fetch-Site` nor `Origin`. It passes the guard, for example `curl` or a
test. The token check (11.8) and the proxy still apply to it.

### 12.8 Security headers

Each answer of `server.listen` has these headers:

| Header | Value |
|---|---|
| `Content-Security-Policy` | `default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'` |
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `DENY` |
| `Referrer-Policy` | `same-origin` |
| `Cross-Origin-Resource-Policy` | `same-origin` |
| `Cross-Origin-Opener-Policy` | `same-origin` |
| `X-Request-Id` | The id of the request (14.6) |

The policy has no `'wasm-unsafe-eval'` and permits no inline script. `'wasm-unsafe-eval'` comes only with the first
WebAssembly kernel that a measure justifies (section 9). `'unsafe-inline'` in `style-src` is necessary because the
pages set style attributes. Section 9 holds the same value.

The block `geomap` (13.12c) changes the value of `img-src` and no other directive. The value is `'self' data:` and one source: `https://`, the host, the port when the template has one, and the path of the template up to the last `/` before the first placeholder.

Example: the template `https://tiles.example.org/osm/{z}/{x}/{y}.png` gives `img-src 'self' data: https://tiles.example.org/osm/`. Hune builds the header one time at start from the parsed parts of 13.13. Thus a `;`, a `,`, a blank or a quote mark cannot reach it.

Without the block, the header is the value of the table, byte for byte. The refusal of 6.27 for a full connection keeps the value of the table, because Hune writes it before it reads a request. A unit test checks the three cases.

The cache headers:

| Answers | `Cache-Control` |
|---|---|
| Each answer under `/api/`, the two ProcurAuth routes, and each error | `no-store` |
| The shell of the pages (`/`, `/d/{uid}[/{slug}]`) | `no-cache` |
| A file under `/static/{build-hash}/` | `private, max-age=31536000, immutable` (section 9). A shared cache in front of the edge cannot give the file to a client that has no token. |

The page renders each value from a dashboard or from a result with `textContent` or on a canvas, never with
`innerHTML`. A data link accepts only an `http` URL, an `https` URL or a relative URL. A relative URL
that starts with `//` or with `/\` is refused, because a browser reads both as another origin (section 4, 9.18). A link
with `target_blank: true` opens in a new tab with `rel="noopener noreferrer"`. Another link opens in the same tab (9.18).

The tokens of a link put values of variables and of cells in the URL (9.18). The page encodes each value for its place,
and it checks the final URL after it replaces all tokens. Thus a value cannot add a scheme, a start with `//`, a line
end or a parameter.

The text of a text panel is Markdown. The page reads it with its own parser and builds DOM nodes with `createElement`, `createTextNode` and `setAttribute` (9.19b). HTML in the text shows as text. The page makes no image, no `style` attribute and no element that loads a resource. A link has the destination check of 4.23 and `rel="noopener noreferrer"`. The policy above does not change.

The state timeline and the status history draw their text on a canvas. The card, the legend and the hidden description use `textContent`. The page builds no element from a state text, a row name or a mapping text. The policy above does not change.

The heatmap and the histogram draw on the canvas of the chart engine. The colour bar is a canvas of Hune. The card and the legend are the elements of the engine, which escapes each text. The hidden description and the labels of the colour bar use `textContent`. The page builds no element from a field name, a bucket label or a unit text. The policy above does not change.

### 12.8a Compression and BREACH

With `server.compress_static = true` (the default, 13.8), Hune sends a gzip copy of a static file of a text type (9.3). It compresses no other answer. This section gives the security reasons.

BREACH and the attacks of its type (TIME, HEIST) read a secret from the size of compressed answers. Such an attack needs all of these conditions:

1. The compressed body holds a secret, for example a token or a value of one user.
2. The same body holds a text that the attacker controls, such as a part of the URL.
3. The attacker sees the size of many answers to requests that the attacker chooses.

Hune makes sure that conditions 1 and 2 are never true for a compressed answer:

- A static file holds no secret and no value of a user. Its bytes are fixed at build time, and each user gets the same bytes.
- No part of a request goes into a static file. This includes the path, the query, each header and the identity token.
- The server makes each copy one time at start. The choice of the copy depends only on the file, on `Accept-Encoding` and on the key. The size of an answer thus tells only the size of a public file.
- The shell, each answer under `/api/`, each error and each answer of the ops listener are never compressed. The shell holds no secret, but it is not a static file (6.25).
- In the mode `authzen`, a static file needs a valid identity token (6.25). The token is in a request header and never in a body.

Two other points apply:

- CRIME used the compression of TLS. rustls has no TLS compression, so CRIME does not apply.
- A request never starts a compression. Thus a client cannot use the compression to load the CPU. The time of an answer does not show its content.

A later change can compress another answer only after a written proof. The proof MUST show that the answer holds no secret, or no text from a request. Without this proof, the answer MUST NOT be compressed.

### 12.8b The tile source of the map: privacy

The map of 9.19i can draw tiles of a host outside Hune. These rules apply:

- The feature is off by default. Without the block `geomap` (13.12c), no page contacts a host other than Hune, and the policy of 12.8 does not change.
- With the block, the browser of each viewer contacts the tile host for each tile of a view. The host gets the address of the viewer, the `User-Agent`, the `Accept-Language`, the time and the rate of the requests. It also gets the header `Origin` with the origin of Hune, and the tile coordinates z, x and y. The coordinates tell the area that the viewer looks at, and this is often the area of the data.
- Hune sends no cookie, no `Referer`, no identity token, no uid, no title, no variable and no value of the data to the tile host.
- An IP address is personal data. Read the terms of the provider. Make an agreement for the data with the provider where the law needs one.
- The public servers of OpenStreetMap forbid heavy use, and they can refuse a request with no `Referer`. Use a provider with a contract, or your own tile server.
- Your own tile server, or a tile cache on the same edge as Hune, removes the third party. Each request then stays inside your network.
- A compromised tile host can show wrong images or track the viewers. It cannot run a script, because the page loads only images from it, and the policy gives it no script source.
- The browser downloads all the bytes of a tile before the page can check its size. The page cannot cap these bytes, because it loads each tile as an image and `connect-src` stays `'self'`. Thus a hostile or compromised tile host can use the bandwidth of each viewer. The page keeps only the decoded tiles, 256 at most (64 MiB). It does not ask again for a failed tile in the same view (9.19i).
- An edge that sets its own policy must also permit the tile host in `img-src`.
- To turn the feature off, remove the block and restart Hune (13.26).

### 12.9 SQL safety

Section 7 gives the complete rules. In short:

- The server is the only place that builds SQL. A viewer sends no SQL.
- The server expands the macros and the variables. It escapes each value by its place in the SQL, and the ClickHouse
  lexer finds that place. A free text value can only become a literal. An identifier or a raw fragment comes only from a closed list.
- One statement, whose first keyword is `SELECT` or `WITH`. Hune refuses any other statement at save and at run.
- The SQL goes in the body of a `POST` to ClickHouse, never in a URL. The values of the query parameters of ClickHouse (7.6a) go in the URL as `param_<name>`, and never in the SQL text.
- These checks are not the security boundary. The ClickHouse account is the boundary (12.12).
- The ad hoc filters of an `adhoc` variable (4.10c, 7.7a, 7.10a) are values only. The key is only compared with the configured list, the operator is `=` or `!=`, and each value is a string literal. The exposure note below applies.
- The macro `$__adhocFilters` is one term of the top-level `AND` of the `WHERE` or `PREWHERE` clause of the outer statement (7.10a, rule 4). So a filter never adds a row that the rest of the clause refuses. The rule is about the clause and not about the result. A `LIMIT`, an aggregate or a window function can still show other rows, because the filter removes rows first.
- The syntax tree of the SQL area is never a security check. It never changes what the server accepts or refuses, and a fault of the tree never stops a save or a run (10.3c, rule 1).

The exposure of a listed column. Each column of `clickhouse "<id>".adhoc_columns` is exposed to each viewer of each dashboard on that source whose SQL has `$__adhocFilters`. With `=` and `!=`, a viewer can test a guess of a value and read the count or the sum that the panel shows. A viewer can thus learn which values a listed column holds in the rows of the panel. List only the columns that each viewer of the dashboards of the source may read. The list is empty by default: with the key absent, no filter applies on the source.

For a `postgres` source, the same rules hold with the dialect of 7.19 to 7.26. The SQL goes in one Parse message, never in a URL and never by the simple protocol. The first word is `SELECT`, `WITH`, `VALUES` or `TABLE`. The role of 8a.13 and the read-only transaction are the boundary (12.19).

For a `mongodb` source, the query is a JSON tree and not SQL. The parser reads it with the profile of 7.28.

- The grammar is a closed allowlist of stages and operators (7.31).
- Each value of a variable is a typed leaf of the tree (7.34, 7.35).
- The command goes to the server as BSON. It is never a text, and it never goes in a URL.
- The user of 8b.13 and the closed list of commands of 8b.5 are the boundary (12.20).

### 12.10 Secrets

The secrets are the passwords of the ClickHouse, `postgres` and `mongodb` sources, the PostgreSQL password of the store, and the PDP token.

- A secret accepts only `_env` and `_file` (13.5). A secret in clear text in the file stops the start.
- Hune reads each secret one time, at start. A change of a secret needs a restart (14.21).
- A secret never shows in a log line, an error, `print-config`, a metric, a span or an answer. `print-config` and the
  start log show `<hidden>`.
- The type that holds a secret has no `Debug` or `Display` form that shows the value.
- The ClickHouse credentials go in the headers `X-ClickHouse-User` and `X-ClickHouse-Key`, never in a URL (section 8).
- The password of a `postgres` source goes in the SCRAM-SHA-256 exchange of `tokio-postgres`. With `sslmode = "verify-full"`, Hune also requires channel binding (8a.4).
- The password of a `mongodb` source goes in the SCRAM-SHA-256 exchange of the driver. Hune offers no other mechanism, so a server cannot make it send the password in clear text (8b.4). Hune sets the options of the driver in code, never in a connection string.
- The PDP token goes in `Authorization: Bearer` on the link to the PDP (11.12).
- The PostgreSQL password goes in the SCRAM-SHA-256 exchange of `tokio-postgres`. With `sslmode = "verify-full"`,
  Hune also requires channel binding (12.13).
- A secret file is readable by the user 65532 (the user of the image) and by no other user of the host. Hune does not
  check the mode of the file. Hune writes no secret to a file.
- The identity token is not a secret of Hune, but Hune never logs it (12.15).

### 12.11 Path safety

- The volume: section 5 gives the rules. The root is absolute and canonical, and Hune opens it one time as a
  directory. Each later access uses `openat` with `O_NOFOLLOW`. Each name comes from a validated id. Hune follows and
  makes no symbolic link, in both modes.
- Each path of the configuration (`_file` values, `ca_file`, `path` of a volume) MUST be absolute. A relative path
  stops the start (13.6).
- A `_file` value and a CA file must be regular files. Hune does not follow a symbolic link in their last part.
- Hune never builds a path from a value of a request, except from an id that passed the id rule of section 4.

### 12.12 The ClickHouse account

The ClickHouse account is the real boundary of the SQL. 8.12 gives the contract of this account, and it is the only
contract. This paragraph is a summary, and 8.12 wins over it. The contract has these items:

- A dedicated user, not `default`.
- A settings profile with `readonly = 2`: the user can send settings, and ClickHouse refuses each write.
- `MAX` constraints on `max_execution_time`, `max_result_rows`, `max_result_bytes`, `max_rows_to_read`,
  `max_memory_usage` and `max_query_size`. Each `MAX` is at least the value of the key of each block of the account.
- `CONST` constraints with the value `throw` on `result_overflow_mode`, `timeout_overflow_mode` and
  `read_overflow_mode`. Thus authored SQL cannot cut a result and show it as complete.
- `SELECT` grants only, on the databases that the dashboards read.
- No grant of the kind `SOURCES` (`URL`, `S3`, `REMOTE`, `FILE`, `MONGO` and the others). Thus authored SQL cannot
  reach the network or the disk of the ClickHouse host.
- No `KILL QUERY` grant, no access management grant, and no other grant. Cancellation uses the close of the connection
  (section 8).

8.12 gives the SQL for the production database. The init file `tests/fixtures/clickhouse/init.sql` of 11.32 runs the
same SQL with the test database `analytics` and a test password:

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
    IDENTIFIED WITH sha256_password BY '<the test password>'
    DEFAULT DATABASE analytics
    SETTINGS PROFILE 'hune_readonly';

GRANT SELECT ON analytics.* TO hune;
```

The ClickHouse suite of section 16 creates this account with this init file. It logs in as the user `hune` and runs
each check of 8.12, with the database `analytics`.

### 12.13 The PostgreSQL account

- `hune serve` uses a role with `CONNECT` on the database and only `SELECT`, `INSERT`, `UPDATE` and `DELETE` on the
  tables of Hune. This role cannot change the schema.
- `hune migrate-store` uses the owner role through its own configuration file `migrate.hcl` (section 5).
- Neither role is a superuser.
- `hune serve` MUST stop at start when its role is a superuser. It MUST also stop when its role owns a table of
  Hune (5.25, step 6 of 5.29). A member of the role that owns a table counts as an owner. Hune then writes
  `server.start_failed` and exits with 1 (14.12).
- The message names the rule. It is the text of 5.29: `storage.postgres: the role <user> is a superuser or owns the
  Hune tables; serve needs a role with only SELECT, INSERT, UPDATE and DELETE (5.25, 12.13)`.
- The server authenticates the roles with `scram-sha-256` in `pg_hba.conf`.
- With `sslmode = "verify-full"`, Hune sets `channel_binding = require` in `tokio-postgres`. The server must then
  authenticate with SCRAM-SHA-256 with channel binding, and the methods `password` and `md5` fail.
- With `sslmode = "disable"`, Hune cannot refuse a weak method. `pg_hba.conf` is then the only control.
- `storage.postgres.sslmode = "verify-full"` is the rule. `sslmode = "disable"` is only for a private network or the
  loopback interface. Hune writes a `config.warning` for it when the host is not a loopback address (13.15).

### 12.14 TLS links

| Link | Library | Trust roots | Name that Hune checks | Plain link |
|---|---|---|---|---|
| ClickHouse | `rustls` in the own `hyper` client | `tls.ca_file` of the block | `tls.server_name` of the block | `http` URL, no `tls` block; warning when the host is not a loopback address |
| PostgreSQL source | `tokio-postgres-rustls` | `tls.ca_file` of the block | `tls.server_name` of the block | `sslmode = "disable"`; warning when the host is not a loopback address |
| MongoDB source | the driver `mongodb` with `rustls` (8b.2) | `tls.ca_file` of the block | The host name of each address that the driver connects to | `tls_mode = "disable"`; warning when a host is not a loopback address |
| PostgreSQL | `tokio-postgres-rustls` | `storage.postgres.tls.ca_file` | `storage.postgres.tls.server_name` | `sslmode = "disable"`; warning when the host is not a loopback address |
| PDP | the own `PdpTransport` with `rustls` (11.12) | `access.pdp.ca_file` | the host of `access.pdp.url` | `plain_http = true`; warning always |
| OTLP collector | `rustls` in the own `hyper` client | `telemetry.ca_file` | the host of `telemetry.otlp_endpoint` | `http` endpoint, no `ca_file` |

Rules for all links:

- One `rustls` version with the provider `ring`, TLS 1.2 and TLS 1.3 only.
- No key skips the check of the certificate or of the name. Grafana skips it today. Hune does not.
- Hune uses no system CA store. The scratch image has none, and `cargo deny` bans `rustls-native-certs`,
  `webpki-roots`, `openssl` and `native-tls` (12.17).
- A CA file holds 1 to 64 PEM certificates and has 1 MiB at most (13.13).
- Hune sends no client certificate.

### 12.15 Sensitive data in logs, metrics and traces

Hune never writes these values to a log line, a metric label or a span attribute:

- Row data of a result.
- SQL text, stored or expanded. A log line can hold the SHA-256 of the expanded SQL (hexadecimal). Read the note on this hash below.
- Variable values.
- A secret, the identity token, `Authorization`, `Cookie` and the value of the token header.
- The query string of a URL. The log holds the route template (for example `/api/dashboards/{uid}`) and the ids of the
  path.
- A request body or an answer body.
- The text of a ClickHouse exception. A log line holds the ClickHouse error code only.
- The text of a PostgreSQL error. A log line holds the SQLSTATE only.
- The text of a MongoDB error. A log line holds the server code only.
- The query tree of a MongoDB source, stored or expanded. A log line can hold the SHA-256 of the compact text of the expanded command (8b.11).
- The values of the query parameters of 7.6a. They go to ClickHouse in the request URI (8.3), and Hune never logs this URI.

These values are permitted in a log line:

- the subject (`sub`) and the claim `owner` (P 9.5);
- the client address and the request id;
- the uid, the folder id, the panel id, the source id and the `query_id`;
- the reason of a PDP answer (P 10.3 forbids it only in the answer to the client);
- the error codes. A metric label holds no id of a dashboard, a folder or a subject
(14.8).

The values of the query parameters of 7.6a travel in the request URI to ClickHouse, as `param_<name>` (8.3). ClickHouse writes this URI to its own log at the level `debug` or `trace`. A proxy in front of ClickHouse can write it to its access log. The operator keeps the ClickHouse log at the level `information` or higher, or treats that log as data of the same class as `system.query_log`.

The table `system.query_log` already holds the final SQL, with the values of the text form of 7.6. ClickHouse 26.8 also writes the statement there with the value of each query parameter in place of its form (case C41). Thus query parameters add no new class of data to that table.

The hash `sql_sha256` of the event `query.run` (14.7) is a fingerprint of the values. The expanded SQL holds each value of the text form of 7.6 and each value of an ad hoc filter (7.10a). A person with the log and the stored SQL can compute the hash for a guessed value. When the value has few possible texts, this person can thus find the value offline. The values of the query parameters of 7.6a are not in the expanded SQL, so the hash does not show them. Give the log of Hune the access control and the retention of `system.query_log`.

Hune does not use a keyed hash. The hash must equal the SHA-256 of the member `sql` of Inspect (6.20), and the tests compare the two.

### 12.16 Denial-of-service limits

| Limit | Value or key | Answer when Hune reaches it |
|---|---|---|
| Open connections on `server.listen` | `server.max_connections` | Hune accepts the connection, answers `503 server_busy` with `Retry-After: 1`, and closes it. |
| Time to read the request line and the headers | 10 s | Hune closes the connection. |
| Time to read a body | 30 s | `408 request_timeout`, then Hune closes the connection. |
| Idle time of a kept connection | `server.idle_timeout` | Hune closes the connection. |
| Request line and URL | 8 KiB | `414 uri_too_long` |
| Headers | 16 KiB and 100 headers | `431 headers_too_large` |
| Body of a dashboard create, save or import | `server.max_request_bytes` (at least the document limit of section 4 plus 65536 bytes, 13.8) | `413 payload_too_large` |
| Body of a panel query, of an options query or of an annotation query | 256 KiB (262144 bytes) | `413 payload_too_large` |
| Body of a preview | 512 KiB (524288 bytes) | `413 payload_too_large` |
| Body of a restore, a move or a folder route | 16 KiB (16384 bytes) | `413 payload_too_large` |
| Bodies of a dashboard create, save or import in flight at one time | `MAX_INFLIGHT_DOC_BODIES` = 8. Hune takes a place before it reads the body. | `503 server_busy` with `Retry-After: 1` and `Connection: close` |
| Other request bodies in flight at one time | `MAX_INFLIGHT_SMALL_BODIES` = 256. Hune takes a place before it reads the body. | `503 server_busy` with `Retry-After: 1` and `Connection: close` |
| Validations of a document that run or wait at one time: a create, a save, `validate_only`, a preview, a restore and the plain read of a version | `MAX_INFLIGHT_VALIDATIONS` = 4, in `spawn_blocking`; wait `VALIDATION_WAIT` = 5 s | `503 server_busy` with `Retry-After: 1` and `Connection: close` |
| Document bodies of one subject in flight at one time (`authzen`) | `MAX_INFLIGHT_PER_SUBJECT` = 2. Hune takes the place of the subject before the place of the body. | `503 server_busy` with `Retry-After: 1` and `Connection: close` |
| Validations of one subject that run or wait at one time (`authzen`) | `MAX_INFLIGHT_PER_SUBJECT` = 2, with no wait | `503 server_busy` with `Retry-After: 1` and `Connection: close` |
| JSON depth of a request body | 64 | `400 bad_request` |
| Queries of one panel, and of one document | 8 and 400 (constants of section 4). The page starts at most 519 requests for one trigger and sends 6 at the same time (9.8d). | A document above a limit is refused at save (4.35). |
| Queries at one time on one source | `max_concurrent`; wait `queue_wait` | `503 source_busy` with `Retry-After: 1` |
| Queries at one time for one subject on one source (`authzen`) | `max_concurrent_per_subject`; wait `queue_wait` | `503 source_busy` with `Retry-After: 1` |
| Memory of the results in flight | `server.max_result_memory_bytes`; each query reserves 2 times its `max_result_bytes`; wait `queue_wait` | `503 source_busy` with `Retry-After: 1` |
| Time of one query | effective `max_execution_time` plus 5 s (sections 8, 8a and 8b) | The code of section 8, or `ceiling_exceeded` (11.24) |
| Rows and bytes of one result | effective `max_result_rows`, `max_result_bytes` | The code of section 8, or `ceiling_exceeded` |
| Rows of one annotation query, and marks of one panel | `MAX_ANNOTATION_ROWS` (1,000) and `MAX_ANNOTATIONS_PER_PANEL` (2,000) | `422 query_limit` or `ceiling_exceeded`, and the note of the panel (9.19d) |
| Size of the expanded SQL, or of the expanded MongoDB command | `max_query_size` | The code of section 7 |
| Calls to the PDP at one time | `access.pdp.max_connections`; wait `access.pdp.timeout` | `503 pdp_unavailable` |
| Connections to PostgreSQL | `storage.postgres.max_connections`; wait `storage.postgres.statement_timeout` | `503 storage_unavailable` |
| Questions and items of one options request | 200 and 50 | A page with a `next` value (11.26) |
| Dashboards, folders, document size | the constants of section 4 and section 5 | The codes of section 4 and section 5 |
| Auto refresh of a page | the fixed list `refresh_choices` (6.10, section 9) | The page offers no other refresh. |

The two body constants bound the memory of the bodies in flight: 8 times `server.max_request_bytes`, plus 256 times
512 KiB.

The limits of the work before and after a decision:

- The work before the decision is linear in the bytes of the body. It is the body read, the check of the JSON syntax
  and the pre-pass of the sources (11.20, 11.21). Hune never validates a document before the decision.
- A validation runs in `spawn_blocking` and never on an async worker (3.7). It takes a permit of
  `MAX_INFLIGHT_VALIDATIONS` first. Thus the validations cannot take the threads of the PDP calls or of the store.
- A validation keeps its permit and the place of its body until it ends, also when the client closes the connection.
  The validation of a create or of a save also keeps its place while it waits for its permit. Thus the 8 places bound
  the work of the requests that the clients left, and a ninth document body gets `503 server_busy`.
- A restore and the plain read of a version (6.17) take the permit before the store read. Thus a request that waits
  holds no bytes of a version.
- A restore validates its version one time, before the store (6.18). Under its lock, the store reads only the
  metadata of the version and the current document. Thus a restore holds the store lock for no validation of a
  version.
- A subject can hold the 8 places with slow bodies before any decision. The caps of a subject stop this. In the mode
  `open`, all requests have the subject `-`, so these caps apply in the mode `authzen` only.
- The list of errors of a validation holds at most `MAX_ERRORS` items, and its list of warnings at most
  `MAX_WARNINGS` items (4.35, 4.36, 6.13).

The codes `server_busy`, `request_timeout`, `uri_too_long`, `headers_too_large`, `payload_too_large`, `source_busy` and
`storage_unavailable` are in the list of section 6 (6.8).

### 12.17 Supply chain

- `Cargo.lock` is in the repository. Each build runs with `--locked`.
- Each direct dependency has an exact version (`=x.y.z`), `default-features = false`, and a comment of one line that
  gives its reason.
- `rust-toolchain.toml` pins the toolchain. The builder image is pinned by its digest.
- `cargo deny check` runs in the CI job `policy`. It checks the advisories, the licences (a list of permitted licences),
  the bans and the sources.
- The bans include `openssl`, `native-tls`, `rustls-native-certs`, `webpki-roots`, `reqwest` and `aws-lc-rs`.
- The permitted sources are crates.io and the one git URL of 11.10. The policy `unknown-git` and `unknown-registry` is
  `deny`.
- `deps.lock.txt` is in the repository: the sorted list of `name version` of the resolved graph for the two musl
  targets. The job `policy` compares it with the output of `cargo metadata --locked`. Thus each new crate is a reviewed
  diff.
- `unsafe_code = "forbid"` applies to each crate of the workspace.
- The code has no `std::process::Command`. The job `policy` checks it with a search of the sources.
- The job `policy` searches the sources for the pattern `std::env::(var|var_os|vars|vars_os|set_var|remove_var)\b`.
  It permits a match only in `crates/hclconf/src/env.rs`, in the tests and in each `build.rs`.
- The copy of the chart engine of ClickHouse Dash has a lock file with the SHA-256 of each file (section 9). The job
  `policy` checks the copy against its lock file. The crate `hclconf` has a `PROVENANCE` file. It records that the crate is not a copy.
- The product has no npm package. npm runs only inside the image of the browser tests.
- The release has `SHA256SUMS`, and the same sources give the same bytes (section 17). Hune never pushes an image to a
  registry.

### 12.18 What Hune does not protect

- In the mode `open`, each request that reaches `server.listen` is trusted. The proxy is the only access control.
- A grant of `dashboard.read` gives each row that the stored SQL of the dashboard reads. Hune does not filter rows by
  subject.
- A grant of `source.query` gives each read that the ClickHouse account permits.
- A grant of `source.query` gives each read that the role of a PostgreSQL source permits (8a.13). The role limits the data. The closed grammar does not.
- A grant of `source.query` gives each read that the user of a MongoDB source permits (8b.13), on the databases of its block. The user limits the data. The closed grammar does not.
- The client of a MongoDB source has the limits of 8b.15. They are one large reply, a command that runs until `maxTimeMS` after the client leaves, and the setting `security.javascriptEnabled` that Hune cannot read.
- The client of a PostgreSQL source has the limits of 8a.15. They are one large row, the `statement_timeout` of each message, and the trust in the functions that an administrator wrote.
- A save does not apply the ceilings of `source.query`. The saved SQL runs with the ceilings of `dashboard.read` of the
  viewer, who can be the author (11.24). Operators SHOULD give each author the same ceilings on both grants.
- The action document is public. It shows the service id and the label of each source.
- A revoked grant stays active for `access.pdp.cache_ttl` at most on each instance.
- A person with write access to the volume or to the database can change any dashboard and its history. The history is
  not tamper-evident. On the volume, a change outside Hune shows as an external change (section 5).
- Several writers on one shared volume (NFS, FUSE) are not supported. `flock` on these file systems is best effort.
- A compromised edge can sign any identity. A compromised PDP can grant anything.
- The draft of the editor stays in `localStorage` of the browser until the save succeeds (section 10). Another program
  on that computer can read it. In the mode `authzen`, a shared browser keeps the draft of a person until a page of
  another subject removes it (10.8). Until then, the tools of the browser can show it.
- In the mode `open`, each history version and each `dashboard.changed` line has the subject `-` (14.11). The durable
  audit then holds no author.
- A person who can read a dashboard can copy its data, for example with a screenshot or a download.
- Hune does not encrypt the volume or the database at rest. The operator does it when it is necessary.
- The link from the proxy to Hune has no TLS. It must stay on a private network (12.6).
- A valid user can load ClickHouse up to the limits of 12.16. The limits bound the cost. They do not remove it.
- Differences of time can show that an operation reached the PDP or the store. Hune makes the body, the headers and
  its own work the same for an item that exists and one that does not exist (11.23, T98). On a write route, both
  items also send the same number of PDP requests. Neither uses a kept answer, so this residual is closed. Two differences stay:
  - The PDP can take another time to evaluate the question of 11.23 than the use of the route.
  - On a read route, a second request for a uid that exists can get a kept answer in one `cache_ttl`. The question of
    11.23 never gets one (11.27).
- A person who can write a dashboard can write a link whose text names another place than its destination. The page shows the link as the author wrote it, with an underline. The rules of 9.19b make sure that the destination is `http`, `https` or relative. They do not check that the text tells the truth. The title of a panel and the text of a note are as reliable as their author.
- The member `allow_custom_value` of a `query` variable is a rule of the page and not a control of security. A closed `query` variable shows the options, but a person who sends a request can send any value. The server treats it as free text under the properties P, PG and PM. An author who needs a bound writes it in the SQL of the panel (4.8). An open or a closed `custom` variable has the refusal or the free-text rule of 7.7.
- A pattern of the document is text that an author wrote: `filter_fields_by_name`, `rename_by_regex`, an override, the option `reduce`, a mapping, `filter_by_value` and the `regex` of a variable. The page runs it on names and cells that hold data. A long-to-wide name holds a label value, and a MongoDB column name has no limit. `filter_by_value` and a mapping read cells.
- The page runs a pattern only on a text of 1,024 code units or fewer, and a longer text never matches (9.14). This bounds a common pattern such as `(.*)-(.*)`. It does not bound a pattern with nested repetitions such as `(a+)+$`, which can be slow on a short text. The check of the document does not refuse such a pattern, and the same people can write the pattern and the SQL.
- The value of a `constant` and the items of an `interval` variable are text that an author wrote. The server puts them in the SQL as author text. A person who can write the document can also write the SQL, so the variables give no more power. The account of the source and its `readonly` profile are the boundary (7.12).
- The images of the tile host of 13.12c are outside the control of Hune. Hune checks their size and their CORS header (9.19i). It cannot check what they show. The privacy note of 12.8b tells what the host learns.

### 12.19 The PostgreSQL source account

The role of a `postgres` source is the real boundary of its SQL. 8a.13 gives the contract of this role, and it is the only contract. This paragraph is a summary, and 8a.13 wins over it. The contract has these items:

- A dedicated login role, which is not a superuser and not the role of the store.
- None of the attributes `SUPERUSER`, `CREATEROLE`, `CREATEDB`, `REPLICATION` and `BYPASSRLS`. No membership in a predefined role, or in a role with one of these attributes.
- `SELECT` grants only, on the schemas that the dashboards read. No `CREATE` on the database or on a schema. No write privilege on a table, a sequence or a column.
- No `EXECUTE` on the functions of `dblink` and on the functions that read or write files of the server. No `USAGE` on a foreign data wrapper or a foreign server.
- The settings and the revokes of 8a.13, as advice. They give the defaults of the role and `temp_file_limit`. They remove the `DO` block and the `EXECUTE` right of `PUBLIC` on the functions that the grammar refuses.

The probe of 8a.12 checks the mandatory items and gives the state `misconfigured` when one fails. The test of 16.9a creates the role with the SQL of 8a.13 and logs in as this role. It checks each write attempt of 8a.13 with a plain client.

### 12.20 The MongoDB source account

The user of a `mongodb` source is the real boundary of its queries. 8b.13 gives the contract of this user, and it is the only contract. This paragraph is a summary, and 8b.13 wins over it. The contract has these items:

- A dedicated login, which is not an administrator. The mechanism is SCRAM-SHA-256.
- The built-in role `read` on each database of the key `databases`, and no other role (M2).
- No privilege on `cluster` or `anyResource`, and no action that the `read` role does not hold (M3).
- The server settings of 8b.13, as advice: `security.authorization`, `security.javascriptEnabled: false` and TLS only. Hune cannot check the second one, because the user cannot read it.

The probe of 8b.12 checks the mandatory items with `connectionStatus` and `showPrivileges`, and gives the state `misconfigured` when one fails. The test of 16.9b creates the user with the commands of 8b.13 and logs in as this user. It checks each write attempt of 8b.13 with a plain client.
