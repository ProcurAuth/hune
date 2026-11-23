# Conformance

This document uses ASD-STE100 Simplified Technical English.

This document tells how Hune shows that it follows the ProcurAuth protocol. The ProcurAuth repository has a runner, `access-conformance`. The runner sends the cases of the series U to Hune in the mode `authzen`. Section 11.32 and section 16.12 of the specification own the method.

## The pinned commit

The git dependencies `procurauth-pep` and `procurauth-core` use one full commit. The conformance run uses the same commit.

| Item | Value |
|---|---|
| Repository | `https://github.com/SCcagg5/procurauth` |
| Commit | `058563cfbc3536b9b850255d953568ef0a5cabbe` |
| Runner | `access-conformance`, role `pep`, series U |
| Cases | U01 to U110 |

To change the commit, follow section 11.10 of the specification. Then run the series U again and update the counts of this file.

## The result

The run on the commit above gave these counts. The last run was on 2026-11-11, with documents of format 2. The fixture `tests/conformance/fixture/conf/conf-dash.json` and the etag of the profile follow format 2. The run gave the same counts.

| Cases | Count |
|---|---|
| Run | 110 |
| Passed | 60 |
| Failed | 0 |
| Skipped | 50 |

The file `tests/conformance/expected.txt` is empty. The script exits with 0 only when the set of failed cases equals the ids in that file.

| Skipped cases | Count | Reason |
|---|---|---|
| U18, U23, U25 to U31, U85 to U110 | 35 | The service has no MCP endpoint. |
| U04, U19, U21, U22, U24, U32 to U35, U58, U62, U63, U67, U68 | 14 | No action declares a budget. |
| U84 | 1 | The profile has no operation `use_service`. |

The specification expects 61 passed cases and 49 skipped cases. The difference is the case U67. Its only step names the operation `use_budget`, and the runner skips it when no action declares a budget. The specification counts U67 as a case that runs. Section 11.32, section 16.12 and section 19.28 need a correction. The owner decides.

## The profile

The files of the run are in `tests/conformance/`.

| File | Content |
|---|---|
| `run.sh` | The script of the run. |
| `config.hcl` | The configuration of Hune: audience `hune-conf`, the PDP at `127.0.0.1:8181`, `cache_ttl = "0s"`, one source `conf`. |
| `profile.json` | The profile of the runner: the service id, the token rules, the three operations. |
| `jwks.json` | The public keys of the test edge of ProcurAuth. |
| `fixture/conf/conf-dash.json` | The fixture dashboard. It has one table panel (id 1) on the source `conf`. |
| `init-conf.sql` | The table `analytics.conf_rows` with the rows 1 to 10. |
| `expected.txt` | The failed cases that the owner accepts. It is empty. |

The profile has these operations.

| Operation | Request | Action and node |
|---|---|---|
| `use_plain` | `GET /api/dashboards/conf-dash?download=true` | `dashboard.read` on `dashboard:conf-dash` below `folder:conf` |
| `use_ceiling` | `POST /api/dashboards/conf-dash/panels/1/query` | `dashboard.read`, ceiling `max_rows`, value 1000 |
| `use_multi` | `PUT /api/dashboards/conf-dash` | `dashboard.write` on `dashboard:conf-dash`, other `source.query` on `source:conf` |

The profile has no `use_budget`, no `use_service`, no `mcp` and no `approvals`. It sets `answer_cache` to `false` and `options_type` to `folder`.

### A deviation from section 11.32

Section 11.32 gives the request `GET /api/dashboards/conf-dash` for `use_plain`. This request asks two questions: `dashboard.read` for the use, and `dashboard.write` for the flag `access.write` (section 11.25). Cases such as U01 count the questions of the subject and expect one. With this request, 10 cases fail.

The run uses `GET /api/dashboards/conf-dash?download=true`. Section 11.21 says that this request asks the same use question. It gives no flag, so it asks one question. The owner decides if section 11.32 follows the run or the run follows section 11.32.

## How to run

The script needs Docker for ClickHouse.

1. Run `tests/conformance/run.sh --local` on a host. The script uses the binary `target/debug/hune` or `target/release/hune`. It starts a ClickHouse container on `127.0.0.1:8123`.
2. Set `PROCURAUTH_DIR` to a checkout of the pinned commit. Without it, the script clones the commit into `target/procurauth`.
3. Run `tests/conformance/run.sh` for the full method. The script builds the runner and the static musl binary in Docker. It starts Hune with `--network none` and a read-only root. ClickHouse and the runner join the network namespace of Hune.

The script writes `target/conformance/report.txt`. The last line of the report gives the counts.

The CI job `procurauth` runs the script. It runs on each change to `crates/hune/src/access/`, to the configuration code, to the conformance files, and before each release.
The workflow `conformance.yml` calls it on GitHub. The owner can run it with `tests/ci/all.sh`, with no GitHub Actions. The script reads the pinned commit of the runner from the `rev` of `procurauth-pep` in `Cargo.toml`.

## What the cases show

The 60 cases that pass show these properties of Hune.

| Property | Cases |
|---|---|
| The subject, the chain and the questions | U01, U02, U07 to U11, U59 |
| A `subtree` answer never lets an operation run | U12 |
| The action document needs no token and asks no question | U13 to U17 |
| No usage for an action without a budget | U20 |
| The ceilings | U36 to U39 |
| No question for the action `access` | U40 |
| Each rule of the identity token | U03, U41 to U55 |
| The PDP credential and the kept answers | U56, U57 |
| The refusals and the failures | U06, U60, U61, U64 to U66 |
| The connections to the PDP stay open | U69 |
| The options route | U05, U70 to U83 |

## A failed case

A later commit of the runner can add cases. A case that fails MUST have a line in `expected.txt` with its reason. The owner approves each line. At the exit of M6, the list is empty.
