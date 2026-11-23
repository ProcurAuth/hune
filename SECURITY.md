# Security policy

This document tells how to report a vulnerability in Hune. It also tells what the project covers. For the design of the protections, read the [security model](docs/SECURITY-MODEL.md).

## Report a vulnerability

Do not open a public issue for a vulnerability. Use the private reports of GitHub.

1. Open [the page to report a vulnerability](https://github.com/ProcurAuth/hune/security/advisories/new).
2. Describe the problem. Name the version, the configuration that you used, and the steps to repeat it.
3. Add a test or a proof of concept if you have one. Remove each real secret and each real host name from it.

Only the maintainers of the repository can read your report. We never ask for an e-mail address or for a secret. If the page is not available, open a public issue with the title "Security contact needed". Write no detail in it. A maintainer then starts a private channel.

## Response times

These times are targets for a project that has no paid staff.

| Step | Target |
| --- | --- |
| We acknowledge your report. | 5 business days |
| We tell you if we accept it as a vulnerability. | 15 business days |
| We publish a fix and an advisory. | 90 days after the report, or sooner |

We ask you to wait for the fix before you publish details. We give credit in the advisory, unless you ask us not to.

## Supported versions

| Version | Supported |
| --- | --- |
| The latest release (0.1.x) | Yes |
| The branch `main` | Yes |
| Older releases | No. Update to the latest release. |

## What the project covers

- **The access policy.** A bypass of the check of the identity token is a vulnerability. So is a route that skips the question to the PDP, or a decision that Hune applies in the wrong way.
- **The query engine.** A way to make a data source run a statement that is not one read statement is a vulnerability. So is a value of a variable that escapes its place in the SQL or in the MongoDB command.
- **The browser.** A script that runs from a dashboard, a result or a link is a vulnerability. So is a way to load a resource from another origin, or a write route that a page of another site can use.
- **The stores.** A read or a write outside the folder of a volume is a vulnerability. So is a path traversal.
- **The configuration.** A secret that appears in the output of `print-config`, in the log, in a metric or in an answer is a vulnerability.
- **The release.** A release archive that does not match its checksum, or a build that is not static, is a defect.

## What the project does not cover

- A deployment that does not follow the documents. Examples are the mode `open` with no proxy, an ops port on the edge, and a source account that can write.
- A weakness of a data source, of an edge or of a PDP. Report it to that project.
- A person who holds a grant of `dashboard.read` and reads the rows that the saved SQL reads. The data limits come from the account of the source.
- A denial of service that needs an authorized person to ask for work that the documented limits already bound.

## Known limits

The limits of the protections are in [What Hune does not protect](docs/SECURITY-MODEL.md#what-hune-does-not-protect) and in [section 12.18](docs/05-access-security.md#1218-what-hune-does-not-protect) of the specification. These limits are not vulnerabilities. Two more limits come from a dependency and from the PostgreSQL protocol.

**RUSTSEC-2026-0387.** The crate `procurauth-core` uses `p256` 0.13 on `crypto-bigint` 0.5. The advisory [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html) applies to `crypto-bigint` 0.5. Some of its operations on secret values do not run in constant time. Hune uses `p256` only to check ES256 signatures with public keys, and it never signs. The advisory does not apply to a check. `deny.toml` ignores the advisory for this reason, until `procurauth-core` moves to `p256` 0.14.

**One large PostgreSQL row.** A row can be larger than `max_result_bytes` and smaller than this value plus 1 MiB. Hune reads such a row in full before it checks the count. The query can then use 1 MiB more than its memory budget of `2 x max_result_bytes`. [Section 8a.15](docs/03-api-query-sources.md#8a15-known-limits) of the specification gives the details.

## Test values

The repository holds fixed keys, certificates and passwords for the tests. They belong to the isolated test services or to the unit tests. They are public on purpose and they protect nothing. A report about one of them is not a vulnerability.

| Path | Test value |
| --- | --- |
| `crates/hune-source/tests/fixtures/tls/` | A test root certificate, a server certificate that this root signs, and the private key of the server certificate. The folder also holds a second root that did not sign the server certificate. The TLS tests of the source clients use them. |
| `tests/api/tls/` | A test root certificate, a server certificate that this root signs, and the private key of the server certificate, for the API cases. |
| `tests/fixtures/config/` | Two test root certificates, an Ed25519 public key, a P-256 public key and a JWK Set, for the configuration tests. |
| `tests/conformance/` | The ClickHouse password, the PDP credential and the public keys of the test edge for the conformance run. |
| `tests/fixtures/clickhouse/init.sql`, `tests/browser/lib/env.mjs`, `crates/hune-source/tests/clickhouse.rs`, `crates/hune-source/tests/adhoc_clickhouse.rs`, `crates/hune/src/server/tests/real_ch.rs` | The passwords `hune-test-password` and `hune-test-admin-password` of the ClickHouse test users. |
| `tests/browser/lib/authzen.mjs` | The PDP credential of the browser cases. |
| `crates/hune/src/server/tests/authzen_pdp.rs` | An Ed25519 key from a fixed seed. It signs the identity tokens of the tests. |
| `crates/hune/src/config/ca.rs`, `crates/hune/src/config/pubkeys.rs`, `crates/hune/src/config/tests.rs` | PEM blocks with a short body that is not a real key, for the tests that refuse a private key. |

The table names the main test values. Other unit tests hold short placeholder values, for example `secret-password`, `p4ss-secret` or `pg-password-marker`. These values also protect nothing.

The scripts `tests/ci/pgsource.sh` and `tests/ci/mongosource.sh` make the passwords and the certificates of the PostgreSQL and MongoDB test services at each run. The conformance test (`tests/conformance/run.sh`) copies the test private keys of the ProcurAuth specification at run time. The repository does not hold them.
