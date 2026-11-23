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
| We acknowledge your report. | 5 working days |
| We tell you if we accept it as a vulnerability. | 15 working days |
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

## Test values

The directories `tests/` and `crates/*/tests/` hold fixed keys, certificates and passwords. They belong to the isolated test services. They are public on purpose and they protect nothing. A report about one of them is not a vulnerability.
