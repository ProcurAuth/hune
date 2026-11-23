# Hune

This document uses ASD-STE100 Simplified Technical English.

Hune is a dashboard product in Rust. It replaces the part of Grafana that the reference deployment uses: dashboards that query ClickHouse.
This folder holds the specification and the code.

## Status

Hune 0.1.0 of 2026-11-22 is the first release. It is a private release. Version 1.0.0 comes after the exit criteria of the milestone M7.

The specification is version 0.1 of 2026-11-09, with later changes. All items are DECIDED, except the open items of section 19.
The owner chose the name "Hune" on 2026-11-12. The script `tools/rename-hune.py` made the rename. See `CHANGELOG.md`.

### Implementation status

The code of the milestones M1 to M6 exists, but some of their tests are not done, for example the failover test of M5. The code also has the PostgreSQL and MongoDB data sources and the features that `CHANGELOG.md` lists for 0.1.0. M7 is in progress.

The milestone M0, the parity spike and the verification V1 to V5 are not done. They need the real data of the reference deployment. All tests use synthetic data, so the format and the expander are not frozen against the real dashboards.
Some decisions wait for the owner.

Read `docs/implementation-status.md` for the state of each milestone, the open decisions, the commands to build and test, and the differences from the specification.

## What Hune does

- It shows dashboards of tables, time series, stat panels and charts. The data comes from ClickHouse, PostgreSQL or MongoDB.
- It stores each dashboard as a JSON document. The store is a folder (a volume) or a PostgreSQL database in high availability.
- It has an editor, a history of versions and a tool that imports the dashboards of Grafana.
- ProcurAuth is optional. Mode `open` needs no access policy. Mode `authzen` makes Hune a PEP of ProcurAuth.
- The front end is native JavaScript, HTML and CSS. It has no framework and no build step.

## How to read the specification

Read `docs/01-overview.md` first. It holds the purpose, the decisions table and the architecture. The other files hold the rules.

| File | Sections |
|---|---|
| `docs/01-overview.md` | 1 Purpose and scope, 2 Decisions, 3 Architecture |
| `docs/02-format-and-storage.md` | 4 Document format, 5 Storage |
| `docs/03-api-query-sources.md` | 6 HTTP API, 7 Query engine, 8 Data sources |
| `docs/04-frontend-editor.md` | 9 Front end, 10 Editor |
| `docs/05-access-security.md` | 11 Access and ProcurAuth, 12 Security |
| `docs/06-configuration-operation.md` | 13 Configuration, 14 Observability and operation |
| `docs/07-migration-tests-delivery.md` | 15 Migration from Grafana, 16 Tests, 17 Delivery |
| `docs/08-roadmap-open-items.md` | 18 Roadmap, 19 Open items, 20 Glossary |

## Design records

The papers that led to the specification, the record of the implementation decisions and the work log are not in this repository. The maintainer keeps them.
The decision ids in the documents and in the code (for example `ID-K335`) are labels of that record.

## Tools

Run `python3 -I tools/ste_lint.py docs/*.md` to check the machine-checkable rules of ASD-STE100. The final result MUST be zero errors.

## Decisions that the owner keeps

- Approve a new copy of the Grafana database of the reference deployment. The milestone M0 needs it to measure the 142 queries and the options of the panels.
- Plan the effort at 16.6 person-months (range 13 to 21) for one engineer. Section 18 gives the plan and the plan for two engineers.
