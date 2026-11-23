# Security model

This document tells what Hune protects and how. Section 12 of the [specification](SPECIFICATION.md) has the full threat model. To report a vulnerability, read the [security policy](../SECURITY.md).

## Trust boundaries

Hune sits between a browser, a data source and, in the mode `authzen`, an edge and a PDP.

| Party | Hune trusts it for | Hune does not trust it for |
| --- | --- | --- |
| Browser | Nothing. Each request is checked. | The SQL, the document and the values. |
| Proxy or edge | The identity of the person (the token or the login). | The link has no TLS. Keep it on a private network. |
| PDP | The decision and the ceilings. | A grant outside the action document. |
| Data source account | The boundary of the data. | A free choice of statement. |

## What Hune protects

- **SQL stays on the server.** The server is the only place that builds SQL. A viewer sends no SQL. The server escapes each variable by its place in the text. A free text value becomes a literal. An identifier or a raw fragment comes from a closed list (section 7).
- **One read statement.** Hune accepts one statement that starts with `SELECT` or `WITH`. PostgreSQL also accepts `VALUES` and `TABLE`. A MongoDB query is a JSON tree with a closed list of stages and operators. These checks are not the boundary. The account of the source is the boundary.
- **Read-only access.** A ClickHouse account has a dedicated user, the profile `readonly = 2`, `MAX` constraints and `SELECT` grants only (8.12). A PostgreSQL source runs in a read-only transaction with a role that cannot write. A MongoDB source has the role `read` only. A probe checks each account, and it refuses a PostgreSQL role with extra rights.
- **Cross-site writes.** Each `POST`, `PUT` and `DELETE` passes a guard before Hune reads the body. The guard checks `Sec-Fetch-Site` or `Origin` and the type `application/json`. No answer has a CORS header. Hune sets no cookie (12.7).
- **Browser isolation.** The page has a strict content security policy with no inline script and no external origin. It renders values with `textContent` or on a canvas. A text panel is Markdown with no HTML. A link accepts only `http`, `https` or a relative URL (12.8).
- **Secrets.** A secret accepts only `_env` and `_file`. A secret in clear text stops the start. Hune never writes a secret to a log, an error, `print-config`, a metric or a span.
- **Closed configuration.** An unknown block, an unknown key or a duplicate stops the start. Hune reads no environment variable that the file does not name.
- **Bounded work.** Each connection, body, queue, pool and cache has a bound. A bound that is reached gives an explicit code, never a silent cut (12.16).
- **Path safety.** The volume refuses a path that leaves its folder. It opens files with `O_NOFOLLOW` and writes a temporary file before a rename (5.12, 5.13).
- **Supply chain.** Each dependency has an exact version. `cargo deny` checks the licences, the sources and the advisories. The release is a static binary with a checksum (12.17).

## Access control

In the mode `authzen`, Hune asks the PDP before each operation. A refusal gives `403`. A PDP that does not answer gives `503`, never a grant. An unknown uid gives the same answer as a dashboard that the person cannot read, so a person learns nothing about names. The ceilings in an answer (`max_rows`, `max_duration`) lower the limits of the source. [Access and ProcurAuth](ACCESS.md) has the rules.

## What Hune does not protect

- In the mode `open`, each request that reaches the main port is trusted. The proxy is the only access control.
- A grant of `dashboard.read` gives each row that the stored SQL reads. Hune does not filter rows by person.
- A grant of `source.query` gives each read that the account of the source permits. The account limits the data. The grammar does not.
- A person with write access to the volume or to the database can change any dashboard and its history. The history is not tamper-evident.
- A compromised edge can sign any identity. A compromised PDP can grant anything.
- Hune does not encrypt the volume or the database at rest.
- The ad hoc filters expose the listed columns to each viewer of a dashboard that uses them (12.9).
- A valid person can load a data source up to the limits. The limits bound the cost. They do not remove it.

Section 12.18 of the specification has the complete list.

## Test values

The repository holds fixed test keys and certificates in `tests/` and in `crates/*/tests/`. They belong to isolated tests. They are public on purpose and they protect nothing.
