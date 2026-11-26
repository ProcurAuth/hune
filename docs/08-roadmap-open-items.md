# Part 8: Roadmap, open questions, glossary and references

Part 8 of the Hune specification (see `01-overview.md`).

This part has three sections. Section 18 gives the milestones, the state of each one, the criteria to move from Grafana and the rule to add a feature.
Section 19 lists the open questions and the differences from Grafana. Section 20 gives the glossary and the references.
The file `implementation-status.md` gives the state of the code in more detail.

## 18. Roadmap and milestones

### 18.1 Principles

- The work has the milestones M0 to M7. M3 has two parts, M3a and M3b. A parity spike comes after M0 and
  before M1 freezes the format and the expander (18.3a). Each milestone has content and exit criteria.
- The risky parts come first. M0 measures the real use of Grafana, and the spike tests the riskiest Grafana rules on real
  dashboards. M1 fixes the store contract, M2 the SQL expansion, and M3a the verified import.
- An installation can move from Grafana to Hune after M4 (18.12). M5, M6 and M7 come after M4.
- Releases: the first release is 0.1.0. It holds the work of the milestones that exists on its date.
  Version 1.0.0 comes after the exit criteria of M7 (17.10). The parity tools of M3a are in `tools/parity/`.
  Their inputs and outputs stay outside the repository (15.18).
- A milestone is complete only when all its exit criteria pass. Each milestone names only the cases that it can pass. The
  tests are in section 16, and 16.17 gives the jobs of the merge gate from each milestone.
- The state of a milestone is `Done`, `Partial` or `Planned`. `Done` means that the code exists and its tests pass on
  synthetic data. `Partial` means that some exit criteria are not met. `Planned` means that nothing of it exists in the
  repository.

### 18.2 Milestones

The column "State" gives the state at version 0.1.1. The file `implementation-status.md` gives the details.

| M | Name | Content (details in 18.3 to 18.10) | Exit criteria (summary) | State |
|---|---|---|---|---|
| M0 | Measure | Copy of the Grafana database and exports; counts | Each measure has a count; each feature is mapped or out | Planned. The checks on real dashboards are not done. |
| Spike | Parity spike | Two real dashboards: import, SQL against `executedQueryString`, one large table | A written result for each check of 18.3a | Planned |
| M1 | Foundations and volume read | Workspace, configuration, listeners, format, volume read, CI, image, release dry run | Gate of M1 green; synthetic volume served | Done. The release dry run built `amd64` only. |
| M2 | Query engine | ClickHouse client, frames, limits, cancellation, lexer, macros, variables, query routes | Random tests and ClickHouse cases green | Done. The load run of 16.16 at full length is not done. |
| M3a | Importer and SQL verification | `import-grafana`, parity tools, expansion vectors, V1 to V3 | Importer tests green; V1 to V3 pass | Partial. The importer, the tools and 300 expansion vectors exist. V1 to V3 are not done. |
| M3b | Viewer and visual verification | Front end of section 9, front-end units, V4 and V5 | Browser and front-end units green; V4 and V5 pass | Partial. The viewer exists. V4 and V5 are not done. |
| M4 | Editor and volume writes | Volume writes, history, editor of section 10, conflicts, restore | Volume conformance and editor tests green | Done |
| Cutover | Cutover from Grafana | The criteria of 18.8 and the plan of 15.15 | T + 14 d without rollback; Grafana stopped | Planned. Each operator runs it. |
| M5 | PostgreSQL backend and failover test | PostgreSQL store, `migrate-store`, `copy-store`, failover test, compose and Kubernetes examples | Conformance on PostgreSQL 15 and 18; failover F1 to F9 | Partial. The failover test is not done. |
| M6 | ProcurAuth | Mode `authzen`, PEP, action document, options route, display filter, ceilings | Series U clean | Done |
| M7 | Hardening and 1.0.0 | OTLP exporter, audit lines, fuzz, load test, CloudNativePG drill, documentation | Fuzz, load and drill pass; release 1.0.0 | Partial. The fuzz campaigns, the 30-minute load run and the drill are not done. |
| MPG | PostgreSQL data source | Dialect, client, role checks, block, importer rows and tests (18.10a) | `pgsource` job and the cases of 18.10a green | Done |
| MMG | MongoDB data source | Query language, client behind a narrow interface, user checks, block, importer rows and tests (18.10b) | `mongosource` job and the cases of 18.10b green | Done |

### 18.3 M0: Measure

M0 and the parity spike (18.3a) are the verification on real dashboards. The measure of 20.3 gives the use of one Grafana
instance, but not the content of its queries. M0 is the plan to measure the use of a Grafana instance before the design of
M2, M3a and M3b becomes final. An operator can run M0 on a copy of the Grafana database and on an export of the dashboards (15.3).

State: Planned. No measure of this paragraph is done in the repository. All tests use synthetic data.

Content:

1. Make a copy of the Grafana database and the export of 15.3. Keep both in a private folder (15.18).
2. Measure on the copy and on the export:
   - the place and the format of each variable use in the SQL: literal, code, `IN (...)`, `${v:format}` or `[[v]]` (20.3 counts 306 uses);
   - each query variable that the SQL uses as an identifier, which v1 refuses (19.11);
   - each other `$__` name in the SQL, and each use of `dictGet` (19.6);
   - the field defaults and the panel options of each type: `legend`, `tooltip`, `stacking`, axis side, threshold style,
     bar orientation, reduce calculations, colour modes (19.17);
   - the legend placement `right`, the legend mode of each panel (list or table), and the tooltip mode `none` that maps
     to `hidden` (19.20);
   - the `filterByValue` matchers outside the measured set, among them `substring` and `notSubstring` (15.6);
   - the cell types of the overrides that the measure of 20.3 does not classify (19.15);
   - the units of the real panels (19.16), and the real use of `displayName` and `hidden` (19.19);
   - the use of table footers, pages and column filters (19.21);
   - panel time overrides, repeats, panel links, dashboard links, and the tokens of data links (19.21);
   - the regex of the variables with its flags, the sort with the natural modes, the hide and the chains (19.18);
   - the use of a UTC time axis, horizontal bars, threshold lines and a right y axis (19.20);
   - what each save changed, from a diff of each version with the version before;
   - the number of queries of each panel, and the number of panels that reuse another panel;
   - the stored form of each dashboard (v1 or v2) and the v2 API version (15.3);
   - the Grafana version and the plugin version.
3. Run two read-only queries on the ClickHouse server that Grafana reads: `SELECT version()`, and the rows and
   bytes of each query of the Grafana account over two weeks from `system.query_log`.
4. Write the results to the private evidence directory (15.18). Update this specification with a new version.

Exit criteria:

- Each measure of item 2 and item 3 has a count.
- Each feature that M0 finds is a `mapped` row of 15.6 by the rule of 18.14, or an `unsupported` row.
- A new version of section 4 holds the classified cell types and the complete unit list. Its option sets hold no option
  that no real panel uses.
- The questions 19.4, 19.7, 19.15 to 19.19 and 19.21 are closed for the measured instance.

### 18.3a The parity spike

The spike comes after M0. M1 should not freeze the document format (section 4) or the expander
(section 7) before the spike ends. It tests early the Grafana rules that are most at risk.

State: Planned. M1 and M2 were done without the spike. The format of section 4 and the expander of section 7 are thus
not verified against real dashboards.

Content:

1. Import two real dashboards with a first version of the importer. One is in the v2 form and one is in the v1 form.
2. Expand their SQL and compare it with `executedQueryString`, with a first version of the capture tool (15.14a). The
   capture tool reads the Grafana and the ClickHouse of the operator.
3. Render one large table panel of a real dashboard with its overrides, in a prototype page.

The checks:

- the pattern form of `byRegexp` and of `reduce.fields` (19.29);
- the rule of `max_data_points` (19.31);
- the v2 variable sort names and the transformation items of 15.6, and the matcher `between`;
- a sample of the variable uses (19.11).

Exit criteria: each check has a written result, and the specification has a new version with the fixes.

### 18.4 M1: Foundations and volume read

State: Done.

Content:

- The workspace with the six crates of section 3.
- The crate `hclconf` with its `PROVENANCE` file and `edition = "2021"` (17.1). It is not a copy (19.26). It has the
  three rules of 13.1: durations with one unit, modes on blocks, and boolean mode keys.
- The key table of section 13, `check-config`, `print-config`, `print-reference` and the start log.
- The two listeners, the logs, the metrics, `/livez`, `/readyz`, `hune health` and the shutdown of section 14.
- The document format of section 4: types, closed parse, validation, canonical bytes and etag.
- The `Store` trait, the catalog and the read path of the volume with its polling (section 5).
- The routes of the dashboard list, of one dashboard, of the folder list and of the session. The shell page and the static
  files.
- The builder image, the CI jobs of the merge gate of M1 (16.17), the release workflow as a dry run, and the image.

Exit criteria:

- The jobs of the merge gate of M1 pass (16.17).
- `hune serve` serves the synthetic volume in mode `open`: the cases A01, A02 and A33 pass.
- The cases K01 to K13, K15 on the `local` output, G01 to G05, and G09 on the volume example pass.
- A dry run of the release gives the two archives and `SHA256SUMS`. The dry run in Docker built the `amd64` archive only.

### 18.5 M2: Query engine

State: Done. The load test of 16.16 at full length is not done.

Content:

- The `DataSource` trait, the ClickHouse HTTP client with rustls, the frames, the limits, the queues and the cancellation
  (section 8).
- The lexer with the shared vectors, the macros, the interval rule, the variables, the substitution and the statement check
  (section 7).
- The routes of the panel query, of the variable options, of the sources and of the preview (section 6).
- The seeded random tests and the ClickHouse cases on two versions.

Exit criteria:

- The random tests pass on the seeds 1 to 64.
- The cases C01 to C13 pass on the ClickHouse versions that 16.9 names.
- The cancellation case C05 passes within 2 s (19.23).
- The cases A18 to A24 and K14a (the mode `open` part of K14) pass.
- The p95 of the server overhead is below 50 ms on the synthetic 45-panel dashboard. The load script of 16.16 measures it
  with 5 viewers.

### 18.6 M3a: Importer and SQL verification

State: Partial. The importer, the capture tool and 300 expansion vectors from a pinned Grafana 12.4.2 exist.
The verification V1 to V3 on real dashboards is not done. The compose project `tests/compose/grafana.yaml` exists.

Content:

- `hune import-grafana` with its report, its acceptance file and its nginx map (section 15).
- The capture tool and the test `parity_private` of `tools/parity/` (15.14a).
- The expansion vectors from the pinned Grafana (16.13).
- The verification V1 to V3 on real dashboards (15.14). Each run reads the Grafana and the ClickHouse of the operator.

Exit criteria:

- The importer cases I01 to I19 pass. The expansion vectors equal the output of the pinned Grafana.
- V1 to V3 pass. The operator signs the acceptance file and the list of explained SQL differences.

### 18.6a M3b: Viewer and visual verification

State: Partial. The viewer, the front-end units and the vectors exist. V4 and V5 on real dashboards are not done.

Content:

- The front end of section 9:
  - shell, list, grid and sections, panels that load when visible;
  - time picker, refresh, URL state, variables bar;
  - the panel types, the table features, field configuration, units, mappings, data links;
  - transformations, long-to-wide, panel reuse;
  - themes, accessibility and "View data".
- The changes of the chart engine that M0 finds in use (19.20).
- The front-end units and their vectors (16.10a).
- The harness `compare.mjs` (15.14b), and the verification V4 and V5.

Exit criteria:

- The browser cases B01 to B13, B14 on the list and dashboard pages, B15, and B24 to B27 pass.
- The front-end units of 16.10a pass.
- V4 and V5 pass. The operator signs the list of explained differences and the visual review.

### 18.7 M4: Editor and volume writes

State: Done.

Content:

- The writes of the volume: atomic write, history, writer lock, tombstone, restore and read-only mode (section 5).
- The editor of section 10: SQL tab, panel form, JSON tabs, layout by grid numbers, sections.
- The save with `If-Match` and `Idempotency-Key`, the draft in `localStorage`, the conflict screen.
- The history list, the read-only view, the diff and the restore. The export and the import of one document. The delete
  and `hune restore`.
- The write guard and the primary store (sections 12 and 13).

Exit criteria:

- On the volume, the common tests of 5.34 pass, except SC-44 and SC-47, which need PostgreSQL. The volume tests of 5.35
  and the case SX-01 of 16.7 pass.
- The browser cases B14 on the editor, history and conflict pages, and B16 to B22 pass. G07 passes for the volume example.
- The preconditions of 15.15 are met.

### 18.8 Cutover from Grafana

State: Planned. Each operator runs the cutover on their own installation. The repository has no cutover of its own.

Content: the plan of 15.15. A test environment goes first. Then come these steps:

- the parallel run of 14 days and the edit freeze;
- the final import and the proxy switch;
- Grafana read-only for 14 days, and the rollback window of 30 days.

Exit criteria:

- T + 14 d passes with no rollback trigger. Grafana is stopped.
- At T + 30 d, the operator decides what happens to the Grafana service and to the archive of its database.

### 18.9 M5: PostgreSQL backend and failover test

State: Partial. The PostgreSQL store, the commands, the conformance tests and the restore drill are done on PostgreSQL 15 and 18.
The failover test with Patroni (16.11) is not done. The compose cases G07 pass in `tests/ci/image.sh`.

Content:

- The PostgreSQL store of section 5: documents as text, the tables, the polling of the catalog, the `save_id`.
- `hune migrate-store` under an advisory lock, and the schema check at the start of `serve`.
- `copy-store`, `export` and `import` on both backends. The restore drill (16.14).
- The failover test with Patroni (16.11).
- The compose example with PostgreSQL (17.7) and the Kubernetes examples (17.8).

Exit criteria:

- All the common tests of 5.34, SC-44 and SC-47 included, pass on the volume and on PostgreSQL 15 and 18. The PostgreSQL
  tests of 5.36 pass on PostgreSQL 15 and 18.
- The cases A01 to A38 pass on PostgreSQL. The restore drill gives byte-equal results.
- The failover criteria F1 to F9 pass in the modes `kill` and `partition`.
- The cases G07 (both compose examples), G08 and G09 (every file of `deploy/`) pass.

### 18.10 M6: ProcurAuth and M7: hardening

M6 state: Done.

M6 content: the mode `authzen` of section 11. It holds the PEP on `procurauth-pep` and `procurauth-core`, and the check of
the identity token. It holds the rustls `PdpTransport` on a bounded pool of threads for synchronous calls. It also holds the
action document, the options route, the display filter, the ceilings, `docs/ACCESS.md` and `docs/CONFORMANCE.md`.

M6 exit criteria:

- The series U run of 16.12 passes, with the counts of 11.32 at the commit of 19.28. Its expected file is empty, or each
  line has an accepted reason. The run gives 110 cases: 60 passed, 0 failed and 50 skipped. `docs/CONFORMANCE.md` explains
  the difference from 11.32.
- The cases A39 to A45, K14b (the mode `authzen` part of K14) and B23 pass.

M7 state: Partial. The OTLP exporter, the audit lines, nine fuzz targets and the load driver exist.
The fuzz campaigns, the 30-minute load run, the CloudNativePG drill and the release 1.0.0 are not done.

M7 content:

- the audit log lines (section 14);
- the fuzz targets, the load test and the CloudNativePG drill;
- the documentation in ASD-STE100: README, configuration, access, and the generated reference;
- the own OTLP exporter of the `telemetry` block (section 14). It is part of 1.0.0. A build with no exporter stops at
  start with `telemetry.enabled: this build has no OTLP exporter` when `telemetry.enabled = true`. The value
  `telemetry.enabled = false` is valid in every build.

M7 exit criteria:

- Each fuzz target runs 10 minutes on 7 nights in a row with no crash.
- K14c (the telemetry part of K14) and K15 on the `distributed` output pass, with the OTLP exporter.
- The load test of 16.16 and the drill of 16.19 pass.
- The language check of 16.18 gives no error on the documentation. Release 1.0.0.

### 18.10a MPG: PostgreSQL data source

State: Done.

MPG is outside the milestones M0 to M7. It needs the work of M1 to M6. It does not need M7. It follows the rule of 18.14 and the steps of 8.14.

Content:

- The dialect `postgres` in `hune-core` (7.19 to 7.26). It has the lexer, the escape rules, the macros, the statement check and the checks at save. The frame types are in 7.16a and 7.16b.
- The PostgreSQL client in `hune-source` (section 8a). It has the session, the read-only transaction, the limits, the cancellation, the pool, the probe and the role checks.
- The block `postgres "<id>"`, its keys, its contradictions and the generated reference (13.12a, 13.14).
- The access rules (11.33), the logs, the metrics and the spans (section 14), and the rows of the importer (15.6). Also the scanner of the page (9.10).
- The tests of 16.9a and the cases A47 to A51, K16, I22, B44 and B45.

Exit criteria:

- The unit tests of the dialect pass, and the random tests R1 to R9 pass for the dialect `postgres`.
- The `pgsource` job passes: PS01 to PS15 on PostgreSQL 15 and 18. The cases A47 to A51, K16, I22, B44 and B45 pass.
- The vectors of `tests/golden/expansion-pg/` agree with the pinned Grafana, or each difference is recorded.
- The security choices of 19.32 are settled, or their defaults apply.

### 18.10b MMG: MongoDB data source

State: Done. The driver is the crate `mongodb` behind the feature `mongo-driver`.

MMG comes after MPG. It is outside the milestones M0 to M7. It needs the work of M1 to M6. It also needs the generic parts of MPG: the variant of the source type, the admission, the probe loop and the access rules. It does not need M7.

It follows the rule of 18.14 and the steps of 8.14. The query fits the member `sql` (4.15), so item 7 of 18.14 needs no new `format_version`.

The work has two parts, so that the code can start before the choice of the driver crates:

- Part 1 needs no MongoDB crate. Its parts are:
  - the query language in `hune-core` (7.27 to 7.36), the value model and the frame types (7.16c, 7.16d);
  - the block and its keys (13.12b, 13.14), and the importer rows (15.6);
  - the logic of the source in `hune-source`, behind the interface `MongoClient` (8b.16), with a fake client;
  - the access rules (11.34), the logs, the metrics and the spans (section 14), and the scanner of the page (7.19).
  - Its tests are the unit tests, the golden vectors, the random tests R10, and the cases of 16.9b that need no server.
- Part 2 needs the driver. Its parts are:
  - the module `crates/hune-source/src/mongo/driver.rs` behind the feature `mongo-driver`;
  - the change of `Cargo.toml`, `Cargo.lock`, `deps.lock.txt` and `deny.toml`;
  - the cases of 16.9b that need a server.

Content:

- The query language, the frame types, the block and the importer rows of part 1.
- The client of `hune-source` (section 8b): the commands, the wire guard, the limits, the cancellation, the pool, the probe and the user checks.
- The tests of 16.9b, and the cases A52 to A56, K18, I23, B47 and B48.

Exit criteria:

- The unit tests and the golden vectors of the language pass, and the random tests R10 pass.
- The `mongosource` job passes: MS01 to MS18 on MongoDB 7.0 and on the newest 8.x release. The cases A52 to A56, K18, I23, B47 and B48 pass.
- The macro vectors of `tests/golden/expansion-mongo/` agree with the pinned plugin, or each difference is recorded.
- The security choices of 19.33 are settled, or their defaults apply.

### 18.11 Assumptions of the roadmap

This paragraph gives the assumptions of the roadmap. It gives no effort and no schedule.

The scope holds an own router and an own ClickHouse client. The client streams rows and cancels queries. It also holds a synchronous TLS
transport to the PDP and an importer of four input forms. It holds a parity harness against Grafana, and a Patroni test in two failure modes.

The roadmap holds only with these assumptions:

1. The work uses these parts as they are: `procurauth-pep` and `procurauth-core`, the lexer vectors of ClickHouse Dash
   and the conformance cases of ProcurAuth. `hclconf` has only the three rules of 13.1. The chart engine gets only the
   changes of 19.20 that M0 finds in use.
2. M0 finds no feature outside the measured set. Each feature that M0 adds follows 18.14.
3. An account of the ClickHouse HTTPS interface, with a CA file, exists for Hune before the verification of M3a (19.6).
4. The scope of this specification does not change during the work.

The largest risks are in M3b and M4. They are the table panel with many overrides, the display-name
rules of Grafana, the gaps of the chart engine, and the editor.

These choices keep the scope small where no requirement needs more:

- The Kubernetes examples hold one Deployment, one Job and the CloudNativePG objects (17.8).
- M0 removes the panel options that no real panel uses before the format is frozen (19.17).

### 18.12 The cutover point

An installation can move from Grafana to Hune after M4, in volume storage and in mode `open`, behind a proxy that checks
the login. The conditions:

- The verification V1 to V3 of M3a and V4 to V5 of M3b passed (15.14).
- The exit criteria of M4 passed.
- The preconditions of 15.15 are met.

Reason: people edit the dashboards, so the editor must exist before the cutover. An installation that needs neither
PostgreSQL nor ProcurAuth needs nothing from M5 to M7. The cutover is not after M3b, because there is no editor then.
It is not after M7, because M5 to M7 give such an installation nothing that it uses.

### 18.13 Out of scope

These features are not in Hune 1.0. Each one needs the rule of 18.14 to enter:

- alerts, alert rules and contact points;
- data sources other than ClickHouse, PostgreSQL and MongoDB (milestones MPG and MMG): Kafka;
- the built-in annotations of Grafana (its stored events and alerts), and the annotations that a person writes with the mouse. Annotations from a saved query on a ClickHouse, a PostgreSQL or a MongoDB source are in scope (4.5b);
- library panels, playlists, snapshots, correlations and Explore;
- ad hoc filters with an operator other than `=` and `!=`;
- organisations, teams, users, roles and a login page inside Hune (identity comes from the edge or from ProcurAuth);
- plugins, and panel types other than the types of 9.19, 9.19b, 9.19c, 9.19e, 9.19f and 9.19h;
- public dashboards and short links;
- API keys and service accounts;
- traces panels (the logs panel is in scope: 4.33f, 9.19h);
- display inside an iframe of another site, images of panels made on the server, PDF reports;
- translation into other languages, and a dedicated mobile layout (9.8 gives only the stacked grid of a narrow screen);
- a live synchronisation with the Grafana API;
- ProcurAuth budgets;
- a result cache shared between users or instances;
- folders inside folders;
- WebAssembly kernels before a measure shows a gain (section 9).
- controls of the Grafana editor that have no member in the closed format of 4.27 to 4.33d (19.20m). A new member needs the rule of 18.14. It also needs a row in the table of 10.3h and a control that the audit B153 can prove.

### 18.14 The rule to add a feature later

A feature enters Hune only when all these conditions are true:

1. A measure shows its use on a real Grafana instance (the Grafana database, access logs, `system.query_log`), or a
   written rule gives another reason. A feature never enters because Grafana has it.
2. The specification gets a new written rule. It is a new row in the decisions table of section 2, and a new numbered
   section or paragraph.
3. Its configuration keys are in the key table of section 13, with a closed schema and a generated reference. An optional
   feature has a mode key.
4. Its tests in the layers of section 16 and its documentation in ASD-STE100 come in the same change.
5. For a Grafana feature: its rows of 15.6 move from `unsupported` to `mapped`, with a synthetic fixture.
6. For a new crate: a line in `deps.lock.txt` and a reason in `Cargo.toml` (17.9).
7. For a new data source: a new variant of the source type. A new query kind in the document format (so a new
   `format_version`) is necessary only when the query is not SQL text (8.14). It also needs its dialect (lexer, escape rules, macros), its table of frame types and its
   conformance tests.
8. It comes in a minor release (17.10).

### 18.15 The v1.1 list

Drag and resize of the panels on the grid are done (10.5a). The visual editors are done (10.3a, 10.3b). They edit the
overrides, the mappings, the data links, the transformations, the variables and the dashboard settings. They are no longer
v1.1 items. The ClickHouse query parameters (`{name:Type}`) are done in the strict form of section 7.

Candidates for v1.1. They are not decided, and each one needs the rule of 18.14:

- the upload of a Grafana JSON file in the user interface;
- a list of the deleted dashboards in the user interface;
- SQL autocomplete in the editor;
- a first WebAssembly kernel, when a measure on real dashboards shows a gain, with the rules of ClickHouse Dash
  `docs/wasm.md`.

## 19. Open questions

### 19.1 Rules

Each paragraph below is an open question, a difference from Grafana, a requirement, or a paragraph that is not used. An open
question gives its options and the default that the code applies today. A closed question becomes a rule in a new version of the
specification, and its number stays. The other parts of this specification hold no open point.

| Item | Subject | State |
|---|---|---|
| 19.2 | Unused | Unused |
| 19.3 | Unused | Unused |
| 19.4 | The ClickHouse versions of the tests | Open question |
| 19.5 | Unused | Unused |
| 19.6 | The ClickHouse account, CA and `dictGet` grants for the HTTPS interface | Requirement of the operator |
| 19.7 | The stored form and the v2 API version of the dashboards | Measure of M0 |
| 19.8 | Unused | Unused |
| 19.9 | Unused | Unused |
| 19.10 | Unused | Unused |
| 19.11 | Variable uses that put free text or a query variable in code | Open question |
| 19.12 | Unused | Unused |
| 19.13 | Unused | Unused |
| 19.14 | The memory allocator of the musl binary | Open question |
| 19.15 | The cell types that the measure of 20.3 does not classify | Measure of M0 |
| 19.16 | The complete unit list | Measure of M0 |
| 19.17 | The panel options that no real panel uses | Measure of M0 |
| 19.18 | Regex flags and natural sort modes of the variables | Open question |
| 19.19 | The real use of `displayName` and `hidden` | Measure of M0 |
| 19.20 | The gaps of the chart engine, and the differences from Grafana (19.20a to 19.20n) | Differences from Grafana |
| 19.21 | Data-link tokens, table footer, pages and the other panel features of M0 | Measure of M0 |
| 19.22 | Unused: the checks moved to 15.14b | Unused |
| 19.23 | The detection of a client close by hyper on HTTP/1.1 | Open question |
| 19.24 | `ceiling_exceeded` for `max_rows` after a byte overflow | Open question |
| 19.25 | No durable audit record for a folder change | Open question |
| 19.26 | The three rules of `hclconf` | Closed |
| 19.27 | The token rules of `procurauth-core`: whole-number `iat` and `exp`, `kid` with two keys or more | Open question |
| 19.28 | The pinned commit of ProcurAuth and the series U figures | Fact |
| 19.29 | The pattern form of `byRegexp` and of `reduce.fields` | Open question |
| 19.30 | The common subset of the regular expressions | Rule of 4.24 |
| 19.31 | The rule of `max_data_points` | Open question |
| 19.32 | The security choices of the PostgreSQL source | Open question |
| 19.33 | The security choices of the MongoDB source | Open question |
| 19.34 | The query model of the MongoDB plugin and the MongoDB version | Open question |
| 19.35 | The card view of the list page, the threshold of its filters and the new look | Decision to read again |

### 19.2 Unused

This paragraph held an item for one installation. It is not used. Section 18.13 removes public dashboards and short links from v1.

### 19.3 Unused

This paragraph held an item for one installation. It is not used. M0 (18.3) measures the content of the queries of a Grafana instance.

### 19.4 The ClickHouse versions of the tests

The ClickHouse cases of 16.9 run on the versions that 16.9 names. The image of `tests/images.lock` pins one version with a digest.
An operator can run `SELECT version()` to compare the version of their server with the tested versions.

Open question: which ClickHouse versions does Hune support? The options are the pinned version and the latest LTS release only,
or every version that passes the cases of 16.9. Default today: the tests cover the pinned version and the latest LTS release.

### 19.5 Unused

This paragraph held an item for one installation. It is not used.

### 19.6 The ClickHouse account and CA for the HTTPS interface

Hune uses the HTTP interface of ClickHouse with TLS (section 8). The operator of the ClickHouse server gives these items:

- a dedicated account for Hune, with the profile, the constraints and the grants of 8.12;
- one `dictGet` grant for each dictionary that the dashboards use, when M0 finds such a use (8.12);
- the CA file of the certificate of port 8443;
- the name in that certificate, for `tls.server_name`.

The case C09 checks the grants of the account. Without this item, Hune cannot query the ClickHouse server over HTTPS.

### 19.7 The stored form and the v2 API version of the dashboards

The export of 15.3 needs the stored form of each dashboard (v1 or v2). It also needs the v2 API version that Grafana
serves. M0 reads both from the copy of the Grafana database and from Grafana.

### 19.8 Unused

This paragraph held an item for one installation. It is not used. Decision D1 gives the name Hune. The name is in the binary, the crates, the metric prefix and the log events.
It is also in the HTTP header, the `localStorage` keys and the tables of the PostgreSQL backend. A rename after 0.1.0 is a change
of the contract (17.10).

### 19.9 Unused

This paragraph held an item for one installation. It is not used.

### 19.10 Unused

This paragraph held an item for one installation. It is not used. Section 15.15 gives the rollback window and the end of a cutover.

### 19.11 Variable uses that put free text or a query variable in code

Section 7 refuses free text outside a literal. Some variable uses of a real dashboard can put a text box value in code on purpose. In v1,
section 7 also refuses the value of a query variable as an identifier. M0 lists each such use. For each use, the operator
has two options: a `custom` variable with fixed options, or a change of the SQL. The verification V3 shows that
no such use is left.

Open question: does Hune need a third option, a typed identifier variable? Default today: no. Section 7 refuses these uses, and the importer lists each one.

### 19.12 Unused

This paragraph held an item for one installation. It is not used. Section 15.18 gives the rules for the private evidence.

### 19.13 Unused

This paragraph held an item for one installation. It is not used. The keys `max_result_rows`, `max_result_bytes` and `max_execution_time` of a source set its limits (13.12).
The M0 measure of rows and bytes for each query can help an operator to choose them.

### 19.14 The memory allocator of the musl binary

The binary uses the allocator of musl. Under many threads, this allocator can be slow. The load script measures the overhead
in the M2 criterion of 18.5. When the measure fails because of the allocator, another allocator is a new dependency, and it
follows 18.14.

Open question: does Hune need another allocator? Default today: the allocator of musl. The code sets no other allocator.

### 19.15 The cell types that the measure of 20.3 does not classify

The measure of 20.3 classifies 73 of the 161 cell-type overrides of one instance (auto 64, color-background 9). M0 classifies the other 88. Each
cell type in use becomes a value of the property set of 4.18, or an `unsupported` row of 15.6 with an accepted fallback.
Hune has the cell types `gauge` and `json`. M0 classifies the others.

### 19.16 The complete unit list

The unit list of 4.20 holds the units that the measure of 20.3 names, and more. M0 reads the unit of each field default and of each override.
A new version of section 4 adds each unit in use. A unit that stays outside the list is `unsupported` in 15.6.
M0 closes the list.

### 19.17 The panel options that no real panel uses

The option sets of 4.26 to 4.33 hold the options of the Grafana panel types. M0 reads the options of each real panel. A new
version of section 4 removes each option that no real panel uses, so the closed schema stays small.

### 19.18 Regex flags and natural sort modes of the variables

Grafana accepts a variable regex with flags (`/x/i`) and the natural sort modes. M0 counts their use in the 21 real
variables of one instance. The rule of 18.14 then decides which flags and modes the format keeps (4.8, 4.24). A flag or a mode outside
the format is `unsupported` in 15.6.

Open question: which flags and modes does the format keep? Default today: the flags and the modes that 4.8 and 4.24 list.

### 19.19 The real use of `displayName` and `hidden`

Section 4 maps the field properties `displayName` and `custom.hidden`. M0 counts their real use. A property that no real
panel uses leaves the property set of 4.18 in a new version of section 4.

### 19.20 The gaps of the chart engine

The copied chart engine had these gaps (9.24):

- no UTC time axis: a dashboard with `timezone = "utc"` needs it;
- no horizontal bars in the engine: this is not a gap of Hune, because the module `barchart_h.js` draws them (9.19);
- Hune closes the UTC axis and the threshold lines with marked edits (9.24, edits 7 and 8). It closes three more gaps with the edits 13 to 15 of 9.24. They are the right y axis, the style of each series and the title of the right axis.

These gaps stay:

- no legend placement `right`: the legend is always under the plot;
- no legend mode for each panel: the list or table mode is a stored preference of the viewer;
- the tooltip modes `single`, multi and `none`: the `hidden` of format 2 maps to `none`.

### 19.20a Differences from Grafana: scales, time and colours

Hune has these differences from Grafana. A chart has two y scales at most, so a third unit shares the right scale (9.19). In a `timeseries`, a `barchart`, a `piechart` and an `xychart`, a continuous colour mode follows the rule of the mode `thresholds`. A time zone name that the browser does not know gives the zone `browser` and a notice. The time shift of a panel has no round to a unit, so `1d/d` is `unsupported`.

Checks on Grafana of 9.8b, 9.16 and 9.19 give these differences:

- A `left` series and an `auto` series of another unit share the left scale. Grafana draws two left axes.
- A `hidden` series and a series that sets its own `min`, `max` or soft ends share the scale of their unit. Grafana gives each set its own scale and its own axis.
- `now/w` starts on Monday (9.9). Grafana starts on Sunday by default, and follows the member `weekStart` of the dashboard.
- A `time_from` of `now/s`, `now/m` or `now/h` is valid. Grafana gives the note `invalid time override` and uses the range of the dashboard.
- The note of a shift writes the unit `M` as it is. Grafana writes `m`.
- A local time that occurs two times has the earlier instant (9.9). Grafana gives the later instant.
- A dashboard link keeps 50 items at most, and a dropdown with no item shows no button. Grafana has no cap of 50 and shows the button.
- In the light theme, a named colour on a mark of a chart can differ from the colour of Grafana by a few levels (9.16). For example, `dark-orange` is `#DD5100` and Grafana gives `#E55400`.
- A dashboard link adds no `timezone` to the URL, puts the added names before a fragment, and breaks a tie of titles by uid. Grafana adds `timezone`, puts them after the fragment and breaks a tie by the order of creation.
- The series colours are a wanted difference. A series with no explicit colour draws the 18 colours of ClickHouse Dash (9.16), in the light and the dark theme. The mode `palette` does the same, and it is the one series palette. Grafana draws its classic palette of 64 colours.
- The importer maps each Grafana palette mode to `palette`. A Grafana `palette-classic` gives `palette` with no item. The modes `palette-classic-by-name` and `palette-saturated` give a `changed` item. A named colour, a threshold step, a value mapping, an override with a fixed colour and a continuous mode keep their colour.
- A series has the slot of its place, as in ClickHouse Dash. Grafana can colour a series by its name (`palette-classic-by-name`): Hune cannot, and the importer lists it. After the 18th series, the colours repeat. Grafana has 64 colours.

A measure of the y ticks of a time series found four differences of the y range. Hune keeps its own canvas for the first one. For the other three, Hune follows Grafana (9.19). The tick rule itself is the rule of Grafana (9.19).

- Small panel. A panel of height 3 has a plot of 19 px in Grafana, in a canvas of 48 px. Hune keeps a canvas of 60 px and a plot of 40 px at least, on purpose. The ticks then differ.
- Bars. Hune follows Grafana. A time series with `draw = bars` uses the automatic range. For data from -50 to 80, both show -70 to 100. Bars do not start at 0 when the data stays far from 0: for data from 1,000 to 1,010, both show 999 to 1,011.
- Stack. Hune follows Grafana. A stack (`stacking = normal`) uses the automatic range of the tops of the stack, with the tick rule of 9.19. For the same data, both show 0 to 160. The tops 1,000 to 2,017 give 800 to 2,200. A stack with `percent` shows 0 % to 100 %, with labels such as "20%".
- Soft ends. Hune follows Grafana. With `axisSoftMax = 200` and data from 5 to 95, the axis ends at the soft end, 200. A soft end inside the data has no effect on its side, and then the room can cross 0. For example, `axisSoftMin = 10` gives -4 to 104 (9.19, y axes, rule 7).

A second measure on 100 panels found two more facts:

- In Grafana, a hard end with no soft end on its side is also the data end of that side. Thus `min = 0` with data from 5 to 95 gives 0 to 105. Hune follows it (9.19, y axes, rule 6).
- Hune follows Grafana. A series with more values below 0 than above 0 stacks down from 0, in a group of its own (9.19, stacks). A later measure found the same ticks on 100 panels, and the same lines within 1 px.

These differences of the stacks stay:

- On a log scale, Hune keeps the rule of the engine (9.19, log scale), and it does not stack. Grafana stacks there, ignores the soft ends and writes the labels of a percent stack with `%`.
- Grafana also puts two lines with another `lineInterpolation` in two groups. Hune has the interpolation of the whole panel only (4.28), and the importer removes a `lineInterpolation` of an override. Thus these lines stack in one group in Hune. The measure of 2026-11-22 found 3 such panels of 24.

Hune follows Grafana for these items. The measure used temporary dashboards on Grafana 12.4.2. They were deleted after the measure.

- A series of another `draw` stacks in a group of its own (9.19, stacks, rule 7). In Grafana the key of a group holds the draw style, and the bar alignment or the line interpolation (`getStackingGroups`).
- The vertical `barchart` stacks with the rules of the time series. A series goes in the group of the sign of most of its values, and each value adds with its sign.
- The value axis of the `barchart` is the automatic axis of Grafana with the soft end 0 (9.19). Grafana gives no room for the value labels. A percent stack shows shares of 1 in the unit of the field.

These differences of the `barchart` stay. Each one is an open question. The default is the behaviour of Hune today:

- A stack shows no value. Grafana 12.4.2 shows the value of each part over its top when it fits (`showValue` = `auto`). For example, it shows 14 values on a stack of 8 bars on a plot of 237 px.
- The horizontal `barchart` is Hune code (9.19), and it keeps its rules. A value below 0 stacks down from 0, the axis has 10 % of room, and a percent stack has the axis 0 % to 100 %. Grafana 12.4.2 draws the stack of the vertical chart there. For the series 10 to 59 and -25 to 40, it shows the axis -20 to 100 with the step 20. For a percent stack, it shows shares from 0 to 1.
- The band under the plot of a vertical bar chart is 21 px, and the labels are 9 px under the plot. In Grafana 12.4.2 the band is 31 px. It holds the gap of 15 px, a tick of 4 px that does not show, and the text of 12 px. Thus its plot is 10 px lower, and its labels are 15 px under the plot. On 3 of 45 panels, the other height gives another tick step. A change needs a measure of the turned labels too (E25).

The gradient modes and the cosmetic series options follow Grafana 12.4.2 (9.19). A measure found these differences, and they stay:

- The mode `scheme` with the colour mode `palette` spreads the 18 colours of the series palette. Grafana spreads its classic palette of 64 colours.
- A horizontal `barchart` (Hune code) paints the fill of a bar with the gradient, and the line of the bar with the plain colour. Grafana also paints the line with the gradient of `hue` and `scheme`.
- A log scale of base 2 with no value above 0 has the range 1 to 2. Grafana draws no axis there. A value of 0 or below has no point, where Grafana draws its line out of the plot.
- The symmetric log scale of Grafana (`symlog`) is not in Hune. The importer keeps it `unsupported` (15.6).

M0 finds which gaps the real panels use (18.3). Each gap in use is a marked edit of the copied engine, with a new lock (9.24,
`docs/engine-patches.md`).

### 19.20b Differences from Grafana: the repeat

Hune has the repeat of a panel and of a section (4.13a, 9.8c). The measure is on Grafana 12.4.2. These differences are on purpose.

- The order of the copies is the order of the options of the variable. Grafana uses the order of the selection: the URL `var-n=3&var-n=1&var-n=2` gives 3, 1, 2 in Grafana. With "All", both use the order of the options.
- A selection with no value shows a message in Hune. Grafana shows the panel one time (the panel keeps its place and runs its query), and hides a repeated row with its panels.
- Hune has limits: by default 24 copies of a panel, 12 copies of a section and 120 copies in a dashboard. The three keys `server.repeat_max_per_panel`, `server.repeat_max_per_section` and `server.repeat_max_per_dashboard` set them (13.8), up to 200, 50 and 200. Above a limit nothing repeats, and the page shows a message. Grafana has no limit. A test on Grafana made 200 copies.
- Hune has one level of repeat. Grafana multiplies a repeated panel in a repeated row: 2 rows of 3 copies gave 6 panels, and a panel that repeats by the variable of its row gave one copy for each row. The importer removes the repeat of the inner panel.
- A panel with direction `h` needs its own rows in Hune. Grafana moves the other panels. The result in Grafana depends on the order of the panels. The importer removes the repeat when another panel shares a row unit, and lists it as `unsupported`.
- Hune needs a `multi` variable for a repeat. Grafana accepts a variable with one value: it makes one copy, and for direction `h` this copy has the full width. The importer refuses such a repeat and lists it as `unsupported`: it never changes the variable.
- Hune makes no copy from a value that is not an option. This is a value of the URL, or a typed value of an open `custom` variable. Grafana repeats on any value of the selection. The member `all_value` has no effect on a repeat in both. In Grafana, a copy gets the option value, not the value of "All".
- Hune writes `<id>` for the first copy and `<id>-r<n>` for the copy n. Hune reads `<id>`, `panel-<id>`, `<id>-r<n>`, `panel-<id>-r<n>` and the form `<x>$panel-<id>` of Grafana. Hune never reads `panel-<id>-clone-<n>`, which is a key of the page of Grafana and not a form of the URL.
- Grafana writes `<x>$panel-<id>` for the view of a copy. The text `<x>` is the value of the copy, not its index. Parity captures on Grafana 12.4.2 measured these facts:
  - With the text values `a` to `e`, the item "View" writes `a$panel-1`, `b$panel-1` and so on to `e$panel-1`. The first copy has the prefix too. A repeated section gives `a$panel-2`, `b$panel-2` and so on.
  - With the values `10`, `20` and `30`, the item "View" writes `10$panel-1`, `20$panel-1` and `30$panel-1`.
  - A load of `b$panel-1` or of `20$panel-1` shows the correct copy. With the values `10`, `20` and `30`, `0$panel-1`, `1$panel-1` and `2$panel-1` show nothing.
  - The match is exact on the value. `Bravo$panel-1` (the option text), `B$panel-1` (another case) and `z$panel-1` show nothing. A value that is not selected shows nothing: `var-host=10` with `20$panel-1`.
- Hune reads `<x>$panel-<id>` in the same way (10.2a, rule 1). Where Grafana shows an empty view, Hune removes `viewPanel` with a notice and shows the dashboard (rule 4). While the values load, Hune keeps the URL and waits (rule 16). Hune writes `<id>` or `<id>-r<n>`, never the form of Grafana.
- Grafana does not read `panel-<id>-clone-<n>`. It does not read `panel-<id>` for a panel that repeats.
- A panel with a repeat and no direction has a layout of its own in Grafana: the copies share the width of the panel. The importer gives direction `h` and lists the change.
- The header of a copy of a repeated section. In Grafana 12.4.2 the copies 2 to n of a repeated row have a title of 14 px at weight 400. Their header is 27.25 px. The first copy and the other rows have 16 px at weight 500 and 30 px. Hune treats this as a fault of Grafana. Each section heading of Hune has one look, and each copy has the header of 30 px of 9.8.
  - Thus a copy of Hune is 2.75 px lower for each copy 2 to n above it, its own header included. A panel under the section is 2.75 px lower for each copy 2 to n.
  - The visual capture of 15.14c documents a box only when its offset is this sum, within 0.5 px (rule `copy-header`). Any other offset stays a fault.

### 19.20c Differences from Grafana: the text panel

Hune has the text panel (4.33a, 9.19b). The measure is on Grafana 12.4.2, on a local instance, with four temporary dashboards. They were deleted after the measure. These differences are on purpose, except the last two.

The facts of Grafana:

- Grafana has three modes: `markdown`, `html` and `code`. An absent or unknown mode gives `markdown`. A panel without `options` shows the text "# Title" and a help line with a link.
- Markdown is CommonMark with extensions. It makes headings (also with an underline), strike, task lists, tables with alignment, footnotes (not read), autolinks for `www.` and e-mail addresses, images and entities. A list that starts at 3 shows no start number: Grafana drops the attribute.
- Grafana keeps raw HTML after a filter. In the test, it kept these element names, with the headings, the lists and the table elements: `a`, `b`, `i`, `img`, `iframe`, `div`, `span`, `p`, `br`, `hr`, `details`, `summary`, `font` and `video`.
- It showed `script`, `style`, `svg`, `form`, `input`, `button`, `object`, `meta`, `link`, `base`, `math` and `marquee` as text. It kept `href`, `target`, `class`, `width`, `color` and a `style` with `color`. It removed `onclick`, `onerror`, `id` and the `data-` attributes.
- An `iframe` gets `sandbox`. An `img` with a remote `src` made a request.
- Grafana keeps these link destinations: `http`, `https`, `mailto`, `ftp`, `//host`, `../up` and `/path`. It empties `javascript:`, `data:`, `vbscript:` and `?x=1`. A link has no `rel` and no `target`.
- Variables. Grafana replaces them in the source, before it parses. The value `*x*` of a variable shows as emphasis. The value `[x](javascript:alert(1))` makes a link with an empty `href`.
- The default form escapes `<` and `>`. The forms `raw` and `text` do not, so the filter then keeps `<i>`. A text panel shows the values, `a, c` for two values. Its title shows the option text, `Alpha + Gamma`. "All" with an `all_value` gives the `all_value`.
- The formats `json`, `lucene`, `regex`, `queryparam`, `distributed`, `singlequote`, `html` and `raw` work. An unknown format gives the `glob` form, `{a,c}`.
- Look: the font is Inter 14 px with a line height of 22 px. The headings are 28, 24, 22, 18, 16 and 14 px. The code is Roboto Mono 12 px. The link is rgb(110, 159, 255) in the dark theme and rgb(31, 98, 224) in the light theme, with no underline. The body has a padding of 8 px, and the text scrolls in a box with `tabindex="0"`.
- A text of spaces gives the error "html prop can't be null". An empty content gives an empty body.
- Each text panel sends one empty query to the default data source: 9 panels sent 9 requests.
- Nested quotes: 200 levels took 2 s to load the page, and 2,000 levels did not finish in 35 s. The text `*a ` repeated 3,000 times took 2.5 s, and 6,000 times took 5.6 s. This is about 5 times the cost for twice the length.
- The same text 30,000 times, and a mix of `_` and `__`, blocked the tab for more than 40 s. Brackets, backticks, long lines and `<` added less than 0.3 s at 20,000 to 1,000,000 characters.

The differences:

- Hune has no HTML. The text `<b>x</b>` shows as these characters. There is no mode `html`.
- Hune has no image. The text `![alt](url)` shows `!` and a link.
- A link goes to `http`, `https` or a URL that starts with `/`, `?` or `#`. Hune shows `mailto:`, `ftp:`, `//host` and `../up` as text. A link has `rel="noopener noreferrer"` and an underline. A destination has no parenthesis and has printable ASCII only.
- The closed list of constructs has no setext heading, no indented code block, no strike and no task list. It has no footnote, no link reference, no entity, no `www.` or e-mail autolink and no link title. A list shows no start number, as in Grafana.
- A variable is replaced after the parse, in the text only. The value is text. A placeholder in a link destination stays as written. The forms are `$name`, `${name}`, `text`, `csv`, `pipe` and `glob`. The names `__from`, `__to` and `__dashboard` stay as written.
- A text that breaks a limit shows a message with no text. Grafana has no limit and blocks.
- A text of spaces shows an empty body. A panel without options is not possible: the options are necessary.
- The mode `code` becomes a code block of Markdown, with no colour and no line numbers. The mode `html` becomes a code block at the import.
- The panel does not repeat in Hune. Grafana repeats a text panel.
- A text panel sends no request in Hune.
- Known difference: a loose list shows the paragraphs in `p` elements only when an item has more than one paragraph. Grafana shows `p` in each item of a loose list.
- Known difference: the headings are `h1` to `h6` as written, and Grafana does the same. A panel title is `h3` in Hune and `h2` in Grafana (9.26).

### 19.20d Differences from Grafana: the state timeline and the status history

Hune has the two panels (4.33b, 9.19c). The measure is on Grafana 12.4.2, on a local instance, with the data source `clickhouse` and synthetic queries. It used 8 temporary dashboards. They were deleted after the measure. The options and the draw rules come from the plugin code that the instance serves, and each rule was checked on a picture. These differences are on purpose.

The facts of Grafana:

- Defaults of the state timeline: `mergeValues` true, `showValue` auto, `alignValue` left, `rowHeight` 0.9, `lineWidth` 0, `fillOpacity` 70, `spanNulls` false, `insertNulls` false and the colour mode `continuous-GrYlRd`. Defaults of the status history: `showValue` auto, `rowHeight` 0.9, `colWidth` 0.9, `lineWidth` 1, `fillOpacity` 70 and the colour mode `thresholds`. `perPage` has no default, so the panel has no pages.
- The data needs a time field. Else the panel says "Data does not have a time field". A frame with a time field only says "No graphable fields". An empty result gives the first message. The fields of the types number, string, boolean and enum are rows.
- Grafana sorts the rows by time. Two time columns give a broken picture of boxes of 1 px.
- The plot has a margin of 8 px at the top and at the right, and a band of 21 px at the bottom. The left gutter is `round(w + 20.5)` for the widest row name of width `w`: 28 px for `b`, 48 px for `state`, 49 px for `st h0`. The names end 16 px before the plot and are centred on the row.
- One row fills the plot height, whatever `rowHeight` is. With 3 rows and a plot of 213 px, the bars are 64 px high at 0.9, with a pitch of 75 px. They are 71 px at 1, 36 px at 0.5, 14 px at 0.2 and 0 px at 0. With 15 rows each bar is 13 px high, and with 40 rows it is 5 px.
- The fill is the colour at the alpha 0.7. With `lineWidth` 2 and `fillOpacity` 30, the fill has the alpha 0.3. A line of 2 px at full opacity is inset by 1 px. A mapping colour `rgba(255,0,0,0.5)` or `#00ff0080` keeps its alpha, and `transparent` draws nothing.
- Each segment has a width of 1 px at least. 100,000 samples with a new state at each sample made 100,000 boxes in 2.5 s, with 92 MB of heap. 1,000 samples took 1.7 s and 52 MB, and 10,000 took 1.5 s.
- A constant state made one box. A result of 1,000,000 rows gave no data. The state timeline has no cap.
- The text is 12 px at weight 500. The characters that fit are `floor(w / 8.5)`: widths 8 to 16 give 1 character, 17 to 25 give 2. `auto` needs 2 characters. The text is cut with no ellipsis: `WARN` in 31 px shows `WAR`. The text starts at `left + lineWidth + 2`.
- The ink is white when the contrast of white with the fill over the theme background is 3:1 or more, else black. The background is `#111217` in the dark theme and `#ffffff` in the light theme. At the alpha 0.7 the ink is white on `green` and black on `yellow`, and at the alpha 0.3 it is white on both.
- Merge is by the raw value. A range mapping on 30 different numbers gave 30 segments with one text. The value 0.25 and 0.5 are different states, also when both show `LOW`.
- A sample with no row in the time makes no gap: the state goes on to the next sample. A null ends the segment and draws nothing. A NaN draws nothing. A mapped null draws a state. The last segment ends at the right edge of the range, and a single sample draws a bar from its time to the right edge.
- `spanNulls` true joins the segments over the nulls. A number joins the nulls shorter than it. `insertNulls` true made each sample a box of 1 px. A number cuts a state at that time after its last sample.
- With the colour mode `thresholds`, a number in a `state_timeline` becomes its step. The texts are `< 1`, `1+` and `3+`, with the unit and the decimals (`50.0%+`). The mappings of the field do not apply to it. Equal steps merge: the values 1 and 2 in the step `1+` made one segment.
- The default colour is the gradient from the first stop to the last. The values 0, 1, 2 and 3 are `rgb(115,191,105)`, `rgb(198,203,69)`, `rgb(240,163,64)` and `rgb(242,73,92)` in the dark theme. A text value with no mapping has the first colour, also in the mode `palette-classic`. A bool is red for true and green for false.
- The legend is absent for a continuous mode. With `thresholds` and 2 steps or more it lists the steps (`< 1`, `1+`, `3+`). Else it lists the different state texts in order of first appearance, with the colour of the state. The status history gave `< 80` and `80+` for mapped data.
- An item is 18.8 px high. One row of items is under the plot and makes the plot 32 px lower. 30 items took 5 rows and 85 px. At the right the items are 22.8 px apart and 71 px wide.
- The tooltip of a `state_timeline` has the head `<start> -` and `<end>` on two lines. It has the row name, the value with a dot of 12 px, and the row `Duration`. The card is 11 px to the right of the pointer and 10 px below it. It has the surface `rgb(34,37,43)`, a border of 1 px, the radius 6 px and the shadow `0 4px 8px rgba(1,4,9,.75)`.
- A row is 18.8 px high and the padding is 8 px. The `multi` card has one row for each row of the frame. It sorts the rows by value for `desc`.
- The hovered box gets a white layer of the alpha 0.2.
- The status history. The step is the difference of the first two times. The cell width is `step_px * colWidth - lineWidth`: 27 px at 0.9, 15 px at 0.5, 31 px at 1 and 1 px at 0. The cell has a line of 1 px inset by half a pixel.
- The x range is the data plus half a step when the data covers the range. Irregular steps keep the first step. One sample draws nothing. The text needs 2 characters, so a cell of 15 px has none.
- The status history shows "Too many points to visualize properly" when the frame has more rows than half of the width. 500 rows at a width of 982 px failed, and 400 passed.
- `perPage` makes pages of rows. The legend in the table mode shows names only. A hidden axis (`axisPlacement`) in the defaults hides the time axis too, and the plot then has no margin.
- A regex mapping needs the form `/body/` for a partial match, and the text of the mapping replaces the matched part. The time axis, the zone and the mapping rules follow the same rules as the time series and the stat panel.

The differences:

- Hune merges by state (the text and the colour), not by the raw value. A range mapping on many numbers gives one segment for a run of one state.
- Hune applies a mapping before the step state of the colour mode `thresholds`.
- The ink has 4.5:1 with the fill (9.26). Grafana takes white at 3:1. Hune writes black on `green` where Grafana writes white.
- Hune has a cap of 10,000 segments, and it draws nothing above it. Grafana draws 100,000. The status history keeps the Grafana rule of half the width.
- Hune has no `perPage`, no `insertNulls` and no time threshold for `spanNulls`. It has no pinned tooltip, no data link on a box, no annotation, no sort or hide-zero of the tooltip and no table legend.
- A row name shows only when the pitch of the rows is 12 px or more. Grafana draws names that overlap.
- `show_labels` equal to `false` hides the names and keeps the time axis. Grafana hides the time axis too, because the setting is in the defaults.
- A bool with no mapping takes the first colour of a continuous mode. Grafana colours true as 1 and false as 0.
- A status history with one sample gives the error "A status history needs two samples at least." Grafana draws nothing.
- Two time columns: Hune uses the first. Grafana draws a broken picture.
- The text is cut at a whole character and never in a pair of surrogates.
- The Hune tooltip, the legend and the colours use the tokens and the shades of the theme (9.25). The sizes and the positions are those that this section lists.

### 19.20e Differences from Grafana: the annotations

Hune has the annotations from a query (4.5b, 6.21a, 7.37, 9.19d). The measure is on Grafana 12.4.2, on a local instance, with the plugin ClickHouse 4.5.1 (`annotations` true in its `plugin.json`) and synthetic queries. It used 11 temporary dashboards. They were deleted after the measure. The plugin has no annotation editor of its own, so the rules of the columns are those of Grafana.

The facts of Grafana:

- An annotation item of a dashboard has `datasource`, `enable`, `hide`, `iconColor`, `name`, `target` (the plugin query: `datasource`, `editorType`, `format`, `queryType`, `rawSql`, `refId`), `filter` (`exclude` and `ids`), `mappings`, `builtIn` and `type`.
- One query runs for each item that is enabled: at the load, at each refresh and at each change of the time range. A dashboard with 4 time series and 1 item ran 1 annotation query. A dashboard with 6 items and 6 panels ran 6. The query goes through `/api/ds/query` with the ref id `Anno`, in a request of its own, before the panel requests. A switch off runs no query, and a switch on runs 1.
- The built-in item calls `GET /api/annotations?limit=100&matchAny=false&dashboardUID=<uid>` at the load, at a refresh and at a change of the range. It reads the events that Grafana keeps in its database. The limit 100 is for this item only.
- The marks show on the `timeseries` (with the draw style `bars` too), on the `state-timeline` and on the `status-history`. They do not show on a `barchart` (0 marks, also with a time x field), a `stat`, a `table` and a `piechart`.
- The columns have a role by their name, with the case folded: `time`, `timeEnd`, `title`, `text`, `tags` and `id`. The names `timeend` and `TIMEEND` work. `time_end` and `end` do not. `time` is the first time field when no column has the name, and a `UInt64` column of epoch milliseconds works. `text` is the first text field when no column has the name.
   - A query with a `title` column only showed the title two times. The other columns, for example `color`, are ignored. `mappings` with `{"source": "field", "value": "<column>"}` choose other columns, and they work.
- A region needs a `timeEnd` above `time`. A `timeEnd` that is `NULL`, 0 (the epoch) or equal to `time` gives a point. A Nullable column `timeEnd` with a `NULL` in some rows drew no region for the row that has a value (measured two times). A `timeEnd` below `time` drew a region.
- A point is a line of 2 px with the dash `[5, 5]`, from the top to the bottom of the plot. A triangle of 10 px by 5 px is under the plot (a border of 5 px, the tip up). A region is a fill of the colour at the opacity 0.1 over the plot, with two dashed lines. A bar of 5 px under the plot has the width of the region. The marks are DOM nodes with the cursor `pointer`, and they have no role, no label and no tab stop.
- The colour is `iconColor`. A token of Grafana gives its colour: `red` is `#f2495c`, `green` `#73bf69`, `blue` `#5794f2` and `semi-dark-blue` `#3274d9`. A hex colour and an `rgba` colour keep their value. An absent value gives `#808080`.
   - A name that Grafana does not know gave a green line and no fill. A `color` column of the result is ignored.
- The card shows when the pointer is on the triangle or on the bar, and not on the line. It has a minimum width of 300 px, the surface `#22252b` (dark), a border of 1 px, the radius 6 px and the shadow `0 8px 24px`. The head has the time as `2026-11-13 00:20:00` (a region: `<start> - <end>`) at 14 px. It has the buttons "Edit" and "Delete", also for a query annotation.
   - The body has 12 px in the muted colour: the title, a `<br>` and the text. The tags are chips of 12 px with a colour that comes from a hash of the tag. A click does not pin the card, and `Escape` does not close it.
- The text is HTML. `<b>bold</b>` and `<i>it</i>` showed as elements. `<script>` was removed, an `<img onerror>` lost its attribute and gave an empty text, and an `<a href>` stayed a link. A line end showed as a space.
- The tags: a text is split at each comma with no trim and no removal of equal tags (`'x,y , z,,x'` gave `x`, `y `, ` z`, `x`). An `Array(String)` column works.
- Grafana has no limit for an annotation query. 1,000, 10,000 and 50,000 rows drew 1,000, 10,000 and 50,000 marks, each one a DOM node.
- An annotation query that fails (HTTP 400) draws nothing. The panels stay, and the page showed no badge. A result with no `time` column draws nothing, with no message.
- The variables and the macros work in the SQL: `$env`, `$__fromTime`, `$__timeFilter(...)` and `$__interval_ms` (20000 for the range of 5 hours).
- `filter` with `exclude` equal to `false` draws on the listed panel ids only. With `exclude` equal to `true` it draws on all the other panels. `enable` equal to `false` draws nothing. Each item with `hide` equal to `false` has a switch with its name above the panels.

The differences:

- Hune runs the saved query on a ClickHouse, a PostgreSQL or a MongoDB source. Grafana runs it with the plugin of the data source. The query is in the document and not in a data source setting.
- Hune reads each number in a time column as epoch milliseconds, for each kind of source (7.37, rule 4). A query that gives Unix seconds gives marks in January 1970 in Hune. The result of Grafana for such a number is not measured for the PostgreSQL and MongoDB plugins.
- Hune reads the roles `time`, `timeend`, `title`, `text` and `tags` and ignores the others. It has no role `id`, no `mappings` and no `color` column. It has no fallback to the first time field when a column is named `time`. It has no fallback of `text` to the first text column.
- A `NULL` or 0 in `timeEnd` gives a point, also in a Nullable column. Grafana loses the region of such a column. An end below the time gives an error in Hune. Grafana draws a region.
- The text is plain text. Grafana renders HTML. A text with the form of an HTML tag gives the error `annotation_invalid` with the reason `bad_chars`, and no mark shows. A control character and a bidirectional control give the same error.
- The tags are trimmed, the empty tags are removed and the first of equal tags stays. An `Array(String)` is read from its JSON text.
- Hune has caps: 8 annotations, 1,000 rows for each query (an error above it) and 2,000 marks for each panel (no mark above it). Grafana has none.
- The marks show on the `timeseries` only. Grafana also draws them on the state timeline and on the status history.
- The marks are on a canvas of the page, over the series. The line and the triangle have the measured sizes. The hit zone is 24 px high and 8 px wide on each side, and Grafana has the triangle of 10 px by 5 px. The card appears for a point and for a region in the zone.
- The card has no "Edit" and no "Delete", because Hune has no stored annotation. The tags have one neutral colour. The text has a clamp of 12 lines. The card has no pin.
- The keyboard reads the marks. Grafana has no keyboard access. "View data" has a table of the marks.
- The SQL of an annotation cannot use the macros that need the interval. Grafana can. A query that has one is not imported.
- A failed annotation shows a note in the title bar of the panel, and the cap shows a note. Grafana shows nothing.
- The page has no switch for an annotation. `enabled` is a member of the document.
- The built-in item "Annotations & Alerts" is not imported. Hune has no alert and no stored annotation.
- The import of `filter` with `exclude` equal to `true` is a list of panel ids at the time of the import.
- The card, the lines and the colours use the tokens and the shades of the theme (9.25). The sizes of the marks are those that this section lists.

### 19.20f Differences from Grafana: several queries

Hune has several queries in one panel (4.15a, 9.8d, 9.22a). The measure is on Grafana 12.4.2 with the plugin ClickHouse. It used 4 temporary dashboards. They were deleted after the measure. These differences are on purpose.

Facts of Grafana that Hune follows:

- A panel sends one request `/api/ds/query` with all its visible targets, one request for each data source. The panel members `maxDataPoints`, `interval`, `timeFrom` and `timeShift` apply to all the targets. The request had one `from` and one `to`. It had the same `intervalMs` and `maxDataPoints` for each target.
- A hidden target is not sent. The panel with all targets hidden sends no request and shows "No data". The backend ignores the flag: a direct request for a hidden target runs it.
- The order of the series is the order of the targets. The refs `Z`, `A` and `M` gave `Z c`, `A a` and `M b`. The name of a series is the ref and the field name (`A a`) when two frames or more answer. It is the field name when one frame answers. A long frame with a label gives `A v h0`.
- A time series, a state timeline and a status history join the frames on time. A stat, a bar gauge, a gauge and a pie chart show one item for each numeric field of each frame, with no join. A table shows one frame and a select at the bottom. The headers show `A n`.
- A failed target gives the panel a red badge, whose tooltip holds the message of the plugin. The other frames draw. The matcher `byFrameRefID` matches the fields of one target.

Differences:

- Hune sends one request for each visible query. Grafana sends one for each panel and data source. A dashboard of 10 panels with 19 visible targets made 10 requests in Grafana and makes 19 in Hune. The reason is the bound of 8.8: a request runs one query.
- The server of Hune refuses a hidden query with `400`. The backend of Grafana runs it.
- Hune keeps the prefix `A a` when a query fails, because the rule counts the visible queries. Grafana drops the prefix of a time series when only one frame answers (`b`), and it keeps `A n` in a table. Hune never changes a name because of an outage, so an override by name stays valid.
- When all the queries of a panel fail, Hune shows the error state with the first error and "Retry". Grafana shows "No data" with the badge.
- A partial result has no "Retry" in Hune. The failed queries run at the next trigger. Grafana has the same behaviour with its badge.
- Hune lists each failed query in the badge, with its ref, its code and its message. Grafana shows one message.
- A bar chart shows the first frame only in Grafana (measured: the legend had `A a` and no `B b`). Hune refuses extra queries for a `barchart` and an `xychart`, and the importer keeps the first query. The `xychart` of Grafana did not load in the measure.
- The join needs a sorted time. Hune sorts each frame and drops the rows with a `null` time, as its long-to-wide does. Equal times in one frame give the last row. A frame with no time column is an error in Hune.
- A transform applies to each frame alone, as in Grafana for the transforms of 9.21. The transforms that read more than one frame follow 19.20j.
- A panel of Hune has 8 queries at most, and a document 400. Grafana has no limit.
- A query of Hune has its own source. A query of Grafana in a Mixed panel can use the data source `-- Dashboard --` or `-- Grafana --`: the importer removes it and lists it. A panel with `repeat` and several queries is refused in Hune. The importer removes the repeat and keeps the queries.
- A panel members group (`maxDataPoints`, `interval`, `timeFrom`, `timeShift`) lives in `data.query` in Hune. The first query can be hidden and the members still apply.
- The importer writes no `ref` for a panel with one query, whatever its refId. Thus a document with one query per panel keeps its bytes (4.42). A refId that breaks the grammar, or that two targets share, gets a new letter and the item is `changed`. A matcher `byFrameRefID` follows the new letter.
- A section with `repeat` that holds a panel with several queries loses the `repeat` of the section in the import. A `barchart` or an `xychart` that reuses a panel with several queries is removed.
- The capture of the parity harness pairs the requests by ref. The visual capture reads the legend, the headers of a table and the select of the frames. It compares them with the display names of Hune (15.14c).

### 19.20g Differences from Grafana: the heatmap and the histogram

Hune has the two panels (4.33c, 4.33d, 9.19e, 9.19f). The measure is on Grafana 12.4.2, on a local instance, with the data source `clickhouse` and synthetic queries. It used 8 temporary dashboards that were deleted after the measure. The options and the bucket maths come from the plugin code that the instance serves, and each rule was checked on a picture. These differences are on purpose. Two of them are faults of Grafana that Hune does not copy: one bucket and a bucket offset.

The facts of Grafana, heatmap:

- The defaults: `calculate` false, `cellGap` 1, the colour mode `scheme` with `Oranges`, 64 steps, `exponent` 0.5, `fill` `dark-orange`, the scale `exponential`. `filterValues.le` is 1e-9. The legend is on, the tooltip is `single`, `yHistogram` and `showColorScale` are off, the axis is on the left and the layout is `auto`.
- The panel reads a frame with a time field and number fields. A frame of three columns (time, a number for the bound, a count) gives an error in the panel. With `calculate` false the error is `Cannot read properties of undefined (reading 'config')`. With `calculate` true it is `no heatmap fields found`. The names of the number fields are the rows. A result with 200,000 rows or more shows no data, and 100,000 rows draw.
- A frame that is already bucketed has an ordinal axis. The names are the labels, and the rows have the same height. A name that is not a number keeps the field order. When all the names are numbers, the rows sort by them, and `+Inf` is last.
- The time of a row is the end of its bucket: with 30 rows of 600 s the first cell starts before the plot. The step is the difference of the first two times, and irregular times keep it. One time row draws nothing.
- Null cells and cells with a count of 1e-9 or less are not drawn: 122 of 240 positions draw in the test with nulls and zeros. A negative count is not drawn: 79 cells draw in the test with negative counts, and the legend runs from 1 to 60.
- With `calculate` true, Grafana joins all the number fields of all the frames. The count mode uses 20 time buckets and 10 value buckets when the value is empty.
- The time size is the largest of the list of 9.19e not above `(to - from) / (n - 1)`. It is 15 minutes for 5 hours and 20 buckets, and 30 minutes for 10 buckets. The value size is the largest number of the list `N` not above `(max - min) / (n - 1)`: 50 for the test data.
- The cells that have a value are 126 with the defaults. They are 37 for 10 by 5, and 383 for 10 minutes by 20. They are 272 for log 2, 101 for log 10, and 494 for log 2 with a split of 2.
- The time buckets follow the epoch: `floor(t / size) * size`. The cells outside the range of the time are not drawn. The code of Grafana counts a null value as 0 on a linear scale, and a `NaN` makes all the sizes `NaN`. It skips a value of 0 or less on a log scale with no message. These three rules are from the code and not from a picture.
- The log rows: row `j` spans `b^((r0 + j) / s)` to `b^((r0 + j + 1) / s)`. The axis has a tick at every `m`-th power. It is every 4th power for base 2 (labels `0.00781`, `0.125`, `2`, `32`, `512`). It is every 2nd power for base 10 (`0.00100`, `0.100`, `10`, `1000`). The first tick is the top.
- The colour of a cell is linear in its count. The index in the palette is `floor(steps * (v - min) / (max - min))`. The exponent applies to the mode `opacity` only. The range `min` to `max` is the range of the counts that are above `le` and below `ge`: 101 to 263 with `le` 100 and `ge` 400 on the test data.
- The palette has `steps` colours of the B-spline of the stops of the scheme (the function `interpolateRgbBasis` of d3). In the dark theme the single-hue schemes go from dark to light, and in the light theme they go from light to dark. `Oranges` gives `#fff5eb`, `#fff3e7`, `#fff1e4`, `#ffefe0` and `#7f2704` in the measure. The mode `opacity` gives the opacity 0 to the lowest count: 56 of 126 cells had the colour `rgba(250, 99, 0, 0)`.
- The cell width is the pitch of the columns less the gap. The cell height is the pitch of the rows less the gap. A gap of 4 px or more rounds the sizes. The minimum is 1 device pixel. With `cellGap` 0 the cells are 47.1 and 16.6 px, and with 1 they are 46.1 and 15.6 px.
- The legend is a gradient of 300 by 9 px. It is 25 px from the left edge and 10 px under the top of the legend area. The area is 46.3 px high. The labels are 11 px, with the text colour at 61 % of its alpha, under the ends of the bar.
- The tooltip is the card of the other panels. The head is the end of the time bucket (`YYYY-MM-DD HH:mm:ss`). The rows are `Count` with the count and a dot, and `Bucket` with `low-high`, or `Value` and `Name` for a frame that is already bucketed. The mode `multi` lists each non-empty bucket of the column. The hovered cell has a layer of white with 0.3 opacity, and a dashed cursor crosses the plot.
- `yAxis.min` and `yAxis.max` cut the axis, and the cells outside are drawn clipped. A right axis with `axisWidth` 80 puts the labels 9 px after the plot. A hidden axis leaves a margin of 8 px at the left. The plot then has no margin at the top. The measure of 2026-11-15 found a plot at y 0 with 170 px, where a shown axis gives y 8 and 162 px. Hune follows this rule (E27, 9.24).
- A grid of 1,000 by 100 cells, which is already bucketed, drew in 3.8 s with 93 MB of heap. The cells with a count of 0 (42,214) were not drawn. The panel has no cap.

The facts of Grafana, histogram:

- The defaults: `bucketCount` 30, `bucketOffset` 0, `fillOpacity` 80, `lineWidth` 1, `gradientMode` none, the colour mode palette-classic. The options `bucketSize` and `combine` have no default.
- The panel buckets the number fields of the frames. A frame with the fields `xMin` (or `BucketMin`), `xMax` (or `BucketMax`) and counts is taken as it is. A null value is dropped. A zero count has no bar. The empty buckets between two bars have no bar.
- The size is the first number `R` of the list `Z` with `(max - min) / bucketCount < R` and `R` at or above the smallest difference of two next values. 5,000 values of 0 to 360 gave the size 20 with 18 buckets, and 10 buckets gave 40. Equal values and one value gave no label on the x axis. The measure of 2026-11-15 found no bar for one bucket. The canvas gets a rectangle with an x of `NaN`, and the canvas ignores it. The fixture with `bucketCount` 1 also gave one bucket and an empty plot, with a y axis from 0 to 2 times the count.
- The bars touch each other. A bar is 1 px wide at least: 5,000 values with the size 0.01 gave 3,950 bars of 1 px. The ticks are the edges of the buckets, and every 2nd edge shows when a bar is less than 50 px wide.
- Two fields draw bars that overlap at the same place, with one colour for each. `combine` makes one series with the name `Count`. With the option `stacking` equal to `normal`, the bars stack. With `percent`, the axis goes from 0 to 1.
- The fill is the colour of the series with the opacity of `fillOpacity`: `rgba(115, 191, 105, 0.8)` for the first field and `rgba(242, 204, 12, 0.8)` for the second. The mode of the gradient `opacity` draws a canvas gradient.
- The measure of 2026-11-15 (the parity faults F3, F4 and F6 of the capture of 15.14c) added these facts:
  - The line of a bar is inside the bar at its top and at its bottom. The box loses half the line width at each end. A line of 3 px gave a fill 3 px lower than the count. The x and the width do not change. The clip of the bars is the plot with half the line width more on each side.
  - With a `bucketOffset` that is not a multiple of `bucketSize`, the x range starts after the low edge of the first bucket. Thus a part of the first bucket is cut, and the ticks mark no bar edge. The range ends at the high edge of the last bucket. The ticks are multiples of `bucketSize`, and Grafana also writes the label `-25` outside the canvas.
  - That measure used the size 25 with the offsets 5 and 10 on two sets of data. The range started at -25 for the values of -30 to 30, and at 0 for the values of 0 to 360. The first multiple of `bucketSize` at or above the low edge of the first bucket fits both, but the measure does not exclude other rules.
- Facts of Grafana that Hune follows (9.19f, section 3):
  - The y axis starts at 0. Its end is the end of the range rule of a time series for the lowest count above 0 and the highest count. The room is 10 % of the range of the counts, rounded up to a tenth of its decade. One bucket gives an axis from 0 to 2 times the count.
  - The ticks are 30 px apart at least, or 15 px on a plot of 150 px or less. The code that the instance serves has this rule for the y axis (`calculateSpace`). The rule gave the ticks of Grafana on the 15 panels of the fixtures `histogram_f` and `histogram_h`. It also gave the ticks on 28 plots of 19 to 481 px (temporary dashboards of 2026-11-15) and on the fixture `parity_axes`.
- The card has the row `Bucket` with `low - high` and the rows of the series. A result of 100,000 rows draws in 1.1 s. A result of 500,000 rows shows "No histogram found in response", and a result of 200,000 rows shows no heatmap.

The differences:

- A cell with a count of 0 or less is not drawn, and `hide_le` has the default 0 (Grafana: 1e-9). A count between 0 and 1e-9 shows in Hune and not in Grafana.
- Hune skips a `null` and a `NaN` value. Grafana counts a `null` as 0 on a linear scale and a `NaN` breaks the grid. Hune ignores a value with a time outside the range.
- A value of 0 or less on a log scale is skipped in Hune, as in Grafana. Hune shows a note that gives the count of the skipped values, and the table "View data" shows it too. Grafana skips them with no message.
- Hune has caps (500,000 values, 250,000 cells, 4,000 buckets). Grafana has none, but it shows no data above about 100,000 rows with this data source.
- The frame that is already bucketed has the shape of a time field and number fields only. Grafana also reads cells with the names `xMin`, `yMin`, `count` and the layouts `le` and `ge`. The labels stay at the middle of the row. A `yBucketScale` and a unit of the names are not read.
- The value axis has the places `left` and `hidden`. Hune has no right axis, no `axisWidth` and no `axisLabel` for the heatmap.
- Hune has 10 schemes of the 38 of Grafana. The scheme `viridis` is a sample of 17 colours with a difference of 7 or less in one channel. A scheme that Hune has not is `changed` on import.
- The card of a heatmap has no histogram of the column, no colour bar and no exemplar. The plot has no selection by drag and no card that stays after a click.
- The labels of a log axis have the short form of the engine (`0.001`). Grafana writes `0.00100` and `0.100`.
- A cell of 3 px or less has no gap (E16). The sizes are not rounded for a gap of 4 px or more. A cell is 0.5 px at least, and Grafana has 1 device pixel.
- A bar of a histogram is its slot less 1 px (the engine), so two bars have a gap of 1 px. The bars of Grafana touch.
- A frame with one time row gives an error in Hune ("A heatmap needs two time rows at least."). Grafana draws nothing.
- The cells are colour data. The lowest colour of a scheme can have less than 3:1 with the panel in the light theme, as in Grafana.
- The histogram has no gradient mode. Hune has no `noValue` for the values.
- One bucket: all the values equal, one value, or `bucketCount` 1. Grafana draws no bar and no x label, because its canvas gets a rectangle with an x of `NaN`. This is a fault of Grafana. Hune draws the bar with its two edges and their labels. The y axis of both ends at 2 times the count. The visual capture keys this difference on the rectangle of Grafana with no x (rule `one-bucket`).
- A `bucketOffset` that is not a multiple of `bucketSize`. The x axis of Hune runs from the low edge of the first bucket. Its labels are the edges of the buckets (9.19f, section 3). Grafana cuts a part of the first bucket, and its labels mark no bar edge. This is a fault of Grafana. The visual capture pairs the bars by their x value, so their heights and their colours still compare (rule `bucket-offset`).
- The bars are drawn by the engine. The number of buckets is the number of the span, empty buckets included. Grafana draws only the buckets that have a count, and it has no cap.
- Hune follows Grafana for the y axis of a stack. With `percent`, the axis shows shares of 1, for example 0 to 1.1. In a stack, the axis uses the lowest drawn top. It is 0 when only a later series has a count in a bucket (9.19f, section 3).
- The old heatmap of Grafana 7 to 9 is not imported.
- The Hune card, the legend and the colours use the tokens and the shades of the theme (9.25). The sizes and the positions are those that this section lists.

### 19.20h Differences from Grafana: custom values of a variable

The figures are from Grafana 12.4.2, with 4 temporary dashboards. They were deleted after the measure. The code of the front end that the instance serves is the second source: the files `8060.*.js` and `4652.*.js` of the build.

Facts about Grafana:

- The default is `true`. The scene code reads an absent member as `true` for a query variable and a custom variable. The v2 schema has `allowCustomValue: true` as the default of `QueryVariable`, `CustomVariable` and `DatasourceVariable`. `TextVariable` and `ConstantVariable` have no such member.
- The back end that converts a v1 dashboard to v2 gives `true` for an absent member of a query and a custom variable. It gives no member for a text variable.
- The member changes the control only. With `false`, the typed text of the field gives no row "Hit enter to add", and `Enter` changes nothing. With `true` or absent, the row shows. Grafana checks nothing else with it, so a closed variable is not a rule.
- A URL value that is not an option stays at the first load, with `true`, with `false` and with an absent member: `var-x=zzz` shows `zzz`. For a `multi` variable the values `2` and `zzz` both stay.
- A saved `current` value that is not an option is replaced at the load in the three modes. A single value becomes the first option. A `multi` variable keeps the valid values: `a` stays and `zzz` goes.
- A reload of the options replaces a value that is not an option, in the three modes. A URL value `zzz` and `3` both give the first option after a change of the range to a range that gives one option. A `multi` selection of `2` and `zzz` gives the first option, not a part.
- A value that the person typed with `true` is also lost at the next reload of the options. The measure typed `zzz` and then used "Refresh" with the same options. The variable had a refresh on the time range. The value became the first option. A typed value stays only for a variable that never reloads.
- Grafana shows no notice for any replacement.
- An empty list of options gives an empty value (`var-x=` in the URL) and an empty control, with `false`. A URL value stays at the first load with an empty list. With "All", the control shows "All".
- The synthetic exports of the repository: the exports `monitor` (v1), `monitor-wrapped` and `monitor-v2a` have no `allowCustomValue`. The export `monitor-v2b` has `allowCustomValue: true` on its 5 variables, the two text variables included. The 8 export fixtures for PostgreSQL and MongoDB, and `text_panels.json`, have none. The export "stream" (45 panels) has no variable: its list `variables` is empty in the fixture and in the generator output. The repository has no file of the v2 schema.

The differences that Hune keeps:

- An absent member means closed in Hune and open in Grafana. The importer writes `true` for a `query` variable when Grafana says `true` or says nothing, so an imported dashboard behaves as in Grafana (15.6). For a `custom` variable it writes `true` only when Grafana says `true` and a free value is valid in each query that names the variable. An absent member of a `custom` variable stays closed, with a `changed` item, because a closed `custom` variable is a check of the server. The editor creates a new variable closed.
- A closed variable of Hune replaces a URL value at the load, with a notice. Grafana keeps it at the first load and shows no notice.
- An open variable of Hune keeps a typed value after each reload of the options. Grafana loses it. This is the value of the feature for a variable that reloads with the time range.
- A closed `custom` variable is a rule of the server in Hune (7.7). A closed `query` variable is a rule of the page, as in Grafana. Grafana has no rule on the server.
- An empty list gives "No options" and no value in Hune. Grafana gives an empty value in the URL and a blank control.
- The row of the typed value reads `Use "<text>"`, and a selected typed value is a checked row of the list. Grafana shows "Hit enter to add" and a chip in a `multi` variable.
- A typed value makes no copy of a repeat in Hune (4.13a, 9.8c). Grafana repeats on any value of the selection.
- The editor of Hune has a menu of SQL examples and a preview of the options. Grafana has neither.

### 19.20i Differences from Grafana: the minimal tree of the query editor

Hune has a format on request, folds, a bracket pair and a current block in the SQL area (10.3c to 10.3g). Grafana 12.4.2 edits SQL in Monaco. Hune keeps its text area with two layers.

Facts of Grafana that Hune does not copy:

- Monaco is a code editor library with a full editor model. It has several cursors, a minimap, find and replace, and folds by indent.
- Monaco folds by the indent of the lines and by the brackets. It marks the bracket pair and has bracket colours.
- Monaco has code completion with names of tables and columns.

Differences:

- Hune has no multi-cursor, no minimap, no find and replace and no completion of names (10.16). The reason is the size and the dependency: Monaco is a large library, and the project chose a minimal tree that Hune writes.
- The folds of Hune follow the brackets of the tree of the dialect, not the indent. A fold hides a pair of brackets of two lines or more.
- The format of Hune changes only blanks and never the case of a keyword. A formatter library of Grafana can write the keywords in upper case.
- The format of Hune refuses a text with an error and says where. A formatter library often formats a partial text.
- The fold state is not saved. Monaco keeps it in a view state that Grafana does not save in the dashboard either.
- The tree is not a security check and has no effect on the save or the run. Grafana has no such check in the editor either, and its server is the authority, as the server of Hune is.
- The structure of a `postgres` source follows the lexer of PostgreSQL, but the colours still follow the ClickHouse lexer (10.3). This is the known limit of 10.3.
- The tabs "JSON" of the panel and of the dashboard have no tree.

### 19.20j Differences from Grafana: more transforms

Hune has eleven more transforms (4.25a, 9.21a). The measure is on Grafana 12.4.2 with the plugin ClickHouse. It used 243 temporary dashboards. Each result was read in "Inspect, JSON, Panel data", and the dashboards were deleted at the end. A prototype matched Grafana on 177 of 237 cases. These are the differences, and the cases are in `tests/frontend/vectors/measure-f28.json`.

Facts of Grafana that Hune follows:

- A transform on a frame finds a field by its display name, and a rename sets the display name and keeps the name. The order of the frame and the stable sort follow Grafana. The names of the new fields follow Grafana (`x + y`, `abs(v)`, `Row`, `Total`).
- A null cell counts as 0 in `calculate_field`, `binary` and `unary`. A division by 0 gives infinity.
- `limit` with a negative number keeps the last rows. Each frame is cut alone.
- `merge` joins the rows that have the same key values and no conflict. `join_by_field` in the mode `outer` sorts a number key and a time key, and the last row of a key wins. In the mode `inner`, equal keys multiply.
- `partition_by_values` makes one frame for each value in the order of first appearance, and a null value is the text `null`.

Differences:

- Hune finds the fields of `merge` and `join_by_field` by `name`. The other kinds use the current name. Grafana uses the display name for all of them, and its display name has the frame in it when there are two frames.
- A text operand of `calculate_field` is an error in Hune. Grafana makes text (`a1`, `bnull`) in a field that it calls a number. A field that is not there gives no new field in both.
- In `reduce_row`, Hune uses the calculations of 9.19. For a row with a null cell, `count`, `median` and `distinct_count` can differ from Grafana (measured: `[null, 20, 200]` gives 3, 20 and 3 in Grafana, and 3, 110 and 2 in Hune). `ignoreNulls` and `nullValueMode` have no field. The mode `null as zero` is `unsupported`.
- `sort_by` uses the table sort. A null comes last in both directions. Grafana puts a null first in a number sort from low to high, and last in the same text sort.
  - Hune compares text by the rule of the table (a capital letter before a small letter). Grafana uses the locale order.
  - A boolean sort puts `false` first in Hune and `true` first in Grafana. The option `reverse` has no field.
- `merge` gives an error for a field with two types. Grafana keeps the first type and loses the other values with no message.
- `sort_by` uses all the keys of `sort`, up to 8. Grafana 12.4.2 uses the first key only (measured: a second key does not order the rows that the first key leaves equal).
- Hune finds a field by its current name only, which is the new name after a rename. Grafana also finds a renamed field by its old name (measured: `excludeByName` with the old name removes a field that an earlier step renamed).
- `convert_field_type` keeps the first conversion of a field. The import drops a second conversion of the same field and lists it as `unsupported`. Grafana applies the conversions in order (measured: text, then number, gives a number).
- `reduce` works on each frame, and `partition_by_values` splits each frame. Grafana reduces all frames to one frame (the names are `A x`), and it splits the first frame only and drops the others. `count` in `reduce` includes the null cells in Hune and not in Grafana. The option `fields` of `reduce` gives a panel error in Grafana, and it has no field in Hune.
- `join_by_field` has no mode `outerTabular`. A frame with no key field is an error in Hune. Grafana leaves it out. With no key field in any frame, Hune gives an error and Grafana gives a frame with no field. The Grafana label `name` on the joined fields is the `ref` and the prefix of 9.22a in Hune.
- `convert_field_type` is stricter. A text that is not a number gives null. Grafana gives NaN, and `""`, `null`, `0x10` and `1,5` give 0, 0, 16 and 15.
  - A text to boolean gives `true` for `true` and `1`, and `false` for `false` and `0`. Grafana gives `true` for every non-empty text, also `false`.
  - A null stays null. Grafana gives `null` as text, `false` as boolean and 0 as number.
  - A text to time reads ISO 8601 with a zone, or a date alone. Grafana reads a date and time with no zone as local time, and it takes the option `dateFormat`.
  - A time to text is an ISO text in UTC with `Z`. Grafana uses the zone of the browser. A text to text does nothing. Grafana puts quote marks around the value.
- The kinds have fewer members than the Grafana options (15.6): no `includeByName`, no `%`, no cumulative and window modes, no variable in `limit`.
- The page gives an error when a result is above the caps (9.21a, rule 13). Grafana has no cap.
- The import keeps the first 16 transformations of a panel and lists the others as `unsupported` (15.6).

### 19.20k Differences from Grafana: interval and constant variables

Hune has the variable kinds `interval` and `constant` (4.10a, 4.10b, 7.4a). The measure is on Grafana 12.4.2. It used 4 temporary dashboards. They were deleted at the end.

Facts of Grafana that Hune follows:

- The option "Auto" has the value `$__auto`. Its text in the SQL is a table text of 7.4. The interval is the range divided by the step count, rounded by the table, and not below the minimum. The texts are `7d` for one week, `30d` and `1y`.
- A constant is not read from the URL. It goes into the SQL as it is.
- The interval texts reach the SQL as texts. ClickHouse reads them with `parseTimeDelta` for the units `ms`, `s`, `m`, `h`, `d`, `w` and `y`.

Differences:

- The server of Hune checks the value of an `interval` against the list of the document. Grafana accepts a URL value that is not an option, and it puts the text in the SQL. This difference is a security choice.
- Hune checks each item of the list. An item is digits and one unit of `ms`, `s`, `m`, `h`, `d`, `w` or `y`, at most 365 days. Grafana does not check the items (`abc`, `5x`, `0m`, `1M` are options). The importer removes the items that Hune refuses.
- Grafana splits the list at each character that is not a letter, a digit or `_` (`1.5h` gives `1` and `5h`, `-1m` gives `1m`). Hune splits the editor text at a comma and at a line end. The importer follows Grafana for a file that it imports.
- The minimum of "Auto" is one of the 30 texts of the table. Grafana takes any text, and `90s` gave `1m`. The importer maps such a text to the largest table text that is not above it. A step count of 0 and a bad minimum together made the page of Grafana fail. Hune refuses them at save.
- The text of "Auto" in a title is `Auto` in Hune. Grafana shows the computed interval. The reason is that the page holds no table of intervals (9.30).
- The SQL text of a constant gets the string escapes of 7.8 in a string. Grafana puts it in as it is, so a quote mark in the value breaks the SQL there.
- A constant is always hidden in Hune. Grafana shows a constant with `hide` 0. The importer lists this change.
- The member `$__auto_interval_<name>` in a SQL is not a variable in either product (Grafana leaves it as text, Hune refuses `$__name` in code, 7.10).

### 19.20m Differences from Grafana: the option forms of the panel editor

A headless Chrome measured the editor of Grafana 12.4.2. It opened 13 temporary dashboards, one for each type, with the data source `clickhouse`. It read the groups, the labels and the controls, and it changed each choice to find the controls that show and hide. The dashboards were deleted after the measure.

The order of the groups in Grafana is this. A group of Hune has the same name where it has a member.

| Type | Groups of Grafana, in order, after "Panel options", "Panel links" and "Repeat options" |
|---|---|
| `timeseries` | Tooltip, Legend, Axis, Graph styles, Standard options, Data links and actions, Value mappings, Thresholds |
| `stat` | Value options, Stat styles, Text size, Standard options, Data links and actions, Value mappings, Thresholds |
| `bargauge` | Value options, Bar gauge, Legend, Text size, Standard options, Data links and actions, Value mappings, Thresholds |
| `piechart` | Value options, Pie chart, Tooltip, Legend, Standard options, Data links and actions, Value mappings |
| `barchart` | Bar chart, Tooltip, Legend, Text size, Axis, Standard options, Data links and actions, Value mappings, Thresholds |
| `xychart` | XY Chart, Tooltip, Legend, Axis, Standard options, Data links and actions, Value mappings, Thresholds |
| `table` | Table, Table footer, Cell options, Standard options, Data links and actions, Value mappings, Thresholds |
| `heatmap` | Heatmap, Y Axis, Colors, Cell display, Tooltip, Legend, Data links and actions |
| `histogram` | Histogram, Tooltip, Legend, Standard options, Data links and actions, Value mappings, Thresholds |
| `state-timeline`, `status-history` | The type group, Legend, Tooltip, Axis, Standard options, Data links and actions, Value mappings, Thresholds |
| `text` | Text |

The sizes in the pane are these.

- The pane has items of 287 px, at 1500 px of window. A label is 12 px at weight 500. A control is 32 px high.
- A choice is 32 px high and has segments of 26 px with a radius of 3 px. A slider is 187 px wide, and its number field is 69 px by 32 px.
- A switch is 32 by 16 px. The head of a group is 40 px high.
- A choice in Grafana is a `radiogroup` of native radio inputs. The controls that Hune has already use the same sizes, so the forms add no new size.

The controls that show and hide in Grafana are these. A row in Hune can differ where the table of 10.3h says so.

| Type | Change in Grafana | Controls that show | Controls that hide |
|---|---|---|---|
| `timeseries` | Style = Bars | Bar alignment, Bar width factor | Line interpolation, Line style, Connect null values, Disconnect values |
| `timeseries` | Style = Points | none | Line interpolation, Line width, Fill opacity, Gradient mode, Line style, Connect null values, Disconnect values, Show points |
| `timeseries` | Show points = Never | none | Show values, Point size |
| `timeseries`, `barchart`, `piechart`, `histogram`, `state-timeline` | Tooltip mode = All | Values sort order, Hide zeros (not in a state timeline), Max height | none |
| `timeseries`, `barchart` | Scale = Logarithmic | Log base | Centered zero |
| `stat`, `bargauge`, `piechart` | Show = All values | Limit | Calculation (and Show percent change in a stat) |
| `bargauge` | Display mode = Retro LCD | none | Show unfilled area |
| `bargauge` | Bar size = Manual | Min width, Min height, Max height | none |
| `barchart` | Stacking = Normal | none | Group width, Highlight full area on hover |
| `xychart` | Show = Lines | Line style, Line width | Point size, Min point size, Max point size, Point shape, Point stroke width, Fill opacity |
| `heatmap` | Calculate from data = No | Tick alignment, Value name | X Bucket, Y Bucket, Y Bucket scale, Min value, Max value |
| `heatmap` | Colors Mode = Opacity | Color, Scale, Exponent | Scheme |
| `heatmap` | Y Bucket scale = Logarithmic | Log base, Split | none |
| `text` | Mode = Code | Language, Show line numbers, Show mini map | none |

The differences of Hune are these.

| Where | Grafana | Hune |
|---|---|---|
| Show unfilled area in the mode `lcd` | Hidden | Shown, because the bar gauge of Hune draws the unfilled track in this mode (9.19). |
| Show points and Point size with Bars | Shown | Hidden, because the engine draws marks on a line and on points only (`app_chart_core.js`). |
| Show values of a stacked bar chart | Shown | Hidden, because a stack has no value label (9.19). |
| Legend values in the mode List | Shown | Hidden, because the mode List has the names only (4.26). |
| Connect null values | A choice: Never, Always, Threshold | A switch (`span_nulls`). The value Threshold has no member. |
| Group width, Bar width, Row height, Column width | A fraction from 0 to 1 | A percent from 10 to 100 (4.30, 4.33b). |
| Color mode of a stat | Background Gradient and Background Solid | One value, Background (4.29). |
| Orientation of a bar chart, a stat and a bar gauge | Auto, Horizontal, Vertical | A bar chart and a bar gauge have no Auto (4.30, 4.33). |
| Tooltip mode of an XY chart | Single and Hidden | Single, All and Hidden, because the format has three values (4.26). |
| Display name, Sort by, Sort descending, Wrap text, Column filter | Some are in other places | They are controls of the member of the format (10.3h). |

The controls of Grafana with no member in the closed format have no control in Hune. The importer lists the settings (15.6).

| Type | Controls of Grafana with no member |
|---|---|
| All with a tooltip | Hover proximity, Max width, Max height |
| `timeseries`, `barchart`, `xychart` | Axis: Time zone, Width, Show grid lines, Color, Show border, Centered zero. Scale: Symlog and the log base. Graph styles: Gradient mode, Disconnect values, Show values. |
| `stat` | Text size (Title, Value, Percent change), Show percent change, Percent change color mode |
| `bargauge` | Value display, Name placement, Bar size and its three sizes, Legend, Text size, Limit |
| `piechart` | Slice sorting, Limit |
| `barchart` | X-axis labels minimum spacing, Bar radius, Highlight full area on hover, Color by field, Text size, Line width, Fill opacity, Gradient mode |
| `xychart` | Series mapping, Frame, Min and Max point size, Point shape, Point stroke width, Fill opacity, Line style |
| `table` | Frozen columns, Cell height, Max row height, Enable pagination, Minimum column width, Wrap header text, Cell value inspect, Tooltip from field, Styling from field, and the cell types Sparkline, Pill, Image, Markdown and HTML, Data links, Actions |
| `heatmap` | Y axis: Unit, Decimals, Axis width, Axis label. Tooltip: Max width, Max height. Tick alignment, Value name. |
| `histogram`, `state-timeline`, `status-history` | Gradient mode, Page size (enable pagination), the Axis group |
| `text` | The modes HTML and Code, and their controls |

The xychart panel of Grafana did not load in the editor for a panel that had no `options.series` member. The migration code of the panel threw an exception. The measure added `options.series = []` to its temporary dashboard.

### 19.20n Differences from Grafana: the units

The value formatter of Grafana 12.4.2 was measured. A headless Chrome loaded `@grafana/data` from the served page. It called `getValueFormat` and `getDisplayProcessor` for the 30 new units of 4.20, and for 15 units that Hune had already. The measure made no dashboard. The texts are the vectors `FE-06-units-h.json` of FE-06. The page gives these texts, with the differences below.

| Case | Grafana 12.4.2 | Hune |
|---|---|---|
| `Infinity` with a count unit (`rowsps`, `rowspm`, `recps`, `cps`, `wps`, `opm`, `reqpm`, `rpm`) | `∞undefined rows/s`: the name of the step is `undefined`. | `∞` with no suffix, as each scale of 9.15. |
| `Infinity` with `dthms` or `dtdhms` | `Infinity:0NaN:0NaN` | `∞` |
| A text value with a count unit, for example `abc` with `rowsps` | The formatter gives `abcundefined rows/s`. The display shows `abc`. | `abc`, as for each unit. |
| A text that holds a number, for example `12` with `rowsps` or `0` with `bool` | The display reads the number: `12 rows/s`, `False`. | The text as it is: `12`, `0`. This applies to each unit (9.15). |
| A `bool` value with `bool_yes_no` | `true` or `false` | `Yes` or `No`, the rule of 9.15. |
| The colour of a number with the unit `string` | The colour of the base step, because the display reads no number. | The colour of the number by the thresholds. |
| The y axis of a time series with `kbytes`, `mbytes`, `gbytes`, `tbytes`, `bits` or `binBps` | The steps are powers of 2: the time series of Grafana has a list of binary units. | The steps are 1, 2, 2.5 or 5 times a power of ten. Only `bytes` has steps that are powers of 2 (9.19). |

The measure also gives two rules of units that Hune follows:

- `decbytes` is the SI scale of Grafana (`SIPrefix`). Thus `0.5` gives `500 mB`.
- The three bool units give `NaN` for `NaN`, as the display of Grafana does.

### 19.21 Data-link tokens, table footer, pages and the other panel features of M0

M0 measures the tokens of the data links (9.18), the table footer and pages (9.20), panel time overrides, repeats, and
panel and dashboard links. Each feature in use is a `mapped` row of 15.6 by 18.14, or an `unsupported` row with an accepted
fallback.

Hune has the table footer (9.20), the panel time override (9.8b) and the dashboard links (9.8a). It also has the repeat of a panel and of a section (4.13a, 9.8c). Hune v1 has no pages.

M0 counts the real use, to confirm the limits of 4.35 (13.8).

### 19.22 Unused

The checks of this item are a test list, not an open point. They moved to 15.14b.

### 19.23 The detection of a client close by hyper on HTTP/1.1

The cancellation of 8.9 needs the server to see that the browser closed its connection. The case C05 of 16.9 MUST prove it:
the query leaves `system.processes` within 2 s after the close, through an HTTP/1.1 proxy. When hyper does not detect the
close in time, the design of 8.9 must change.

Open question: is the design of 8.9 enough? Default today: the design of 8.9, with the case C05 as its check.

### 19.24 `ceiling_exceeded` for `max_rows` after a byte overflow

ClickHouse gives the error 396 (`TOO_MANY_ROWS_OR_BYTES`) for both the row limit and the byte limit. When the ceiling
`max_rows` lowered `max_result_rows`, Hune answers `ceiling_exceeded`, also when the cause was the byte limit (8.5, 11.24).
Open question: keep this message, or check the row count before the answer? Default today: Hune answers `ceiling_exceeded`.

### 19.25 No durable audit record for a folder change

A change of a dashboard writes a history version before the effect: the durable audit (14.11). A change of a folder writes
only the log line `folder.changed`.

Open question: does a folder change need a durable record? A durable record changes the store of section 5. Default today: the log line only.

### 19.26 The three rules of `hclconf`

Closed. The crate `hclconf` is written in Hune from section 13. It is not a copy of another HCL engine, and its
`PROVENANCE` file records the comparison. It has three rules (13.1): a duration with exactly one
unit, modes on blocks, and boolean mode keys.

### 19.27 The token rules of `procurauth-core`

`procurauth-core` accepts only whole numbers for `iat` and `exp`. It needs a `kid` when the key set has two keys or more.
Hune follows the library (11.8, 11.9). A change of these rules belongs to the ProcurAuth protocol
(https://github.com/ProcurAuth/procurauth).

Open question: must the protocol accept fractional `iat` and `exp`, and an absent `kid` with two keys? Default today: no. The library refuses both.

### 19.28 The pinned commit of ProcurAuth and the series U figures

The `rev` of the git dependencies `procurauth-pep` and `procurauth-core` is chosen at M6 (11.10, 17.9). The
profile and the counts are those of 11.32. At the commit `f318e259013fdaa7e841936c2b8a172dc1b7699e` (the tag v1.1.1), the
profile gives 127 cases. The run gives 65 passed, 0 failed and 62 skipped, and the specification states the same counts.
The runner skips the case U67, because no action declares a budget. `docs/CONFORMANCE.md` records the counts of the chosen
commit.

The owner MUST read again the change from 110 cases (61 passed) to 127 cases (65 passed).

### 19.29 The pattern form of `byRegexp` and of `reduce.fields`

Grafana most likely reads the `byRegexp` matcher and the `reduceOptions.fields` of the stat, pie and
bar gauge panels with `stringToJsRegex`. In that form a text `/^Total$/` is a pattern with slashes, and a plain text `t`
means `^t$`. 15.6 copies the Grafana text as it is (the Grafana form of 4.24), and the fixtures of I01 and B04 use
`/^Total$/`. The spike (18.3a) checks the rule in the source and the behaviour of the pinned Grafana.

Open question: is this form right? The check is not done. Default today: the code reads the Grafana form of 4.24.

### 19.30 The common subset of the regular expressions

Hune checks a pattern with the Rust crate `regex` (4.24) and runs it with the `RegExp` of the browser. The two engines
differ: `\d`, `\w`, `\s` and `\b` are Unicode in Rust and ASCII in JavaScript, and `[[:alpha:]]`, `\A`, `\z` and `\pL`
are valid in Rust only. Section 4 defines one subset that both engines read the same way, and `hune-core` refuses a
pattern outside it. The vectors of 16.10a check it.

### 19.31 The rule of `max_data_points`

Section 9.8 sets `max_data_points` from the inner width of the panel frame, as Grafana does, and 7.4 agrees. Only the
check is open. The spike (18.3a) measures the `maxDataPoints` of the pinned Grafana on real panels. It compares the value
with the rule of 9.8. A difference near a bound of the interval table (7.4) gives other buckets of `$__timeInterval`.

The case B27 and the check of 15.14b test the rule after the spike.

Open question: does the rule match Grafana on real panels? Default today: the rule of 9.8.

### 19.32 The security choices of the PostgreSQL source

The design of section 8a has choices that no review of real use has validated. Each choice has a default, and the code follows the default. The table below gives each choice. Each one is an open question, and a review can change a default.

| Choice | Default | Where |
|---|---|---|
| The form of the block | A block `postgres "<id>"` for each kind, as 8.14 says. The block `source "<id>" { kind = ... }` is not used, because it breaks the blocks `clickhouse`. | 13.12a |
| The role checks of the probe | R1 to R8 are mandatory. A failure gives the state `misconfigured`. | 8a.12 |
| The predefined roles | A membership in a role named `pg_...` fails R2, also `pg_read_all_data` and `pg_monitor`. | 8a.12, 8a.13 |
| The hardening of the role | A1 to A4 are advice only: the `DO` language right, the `TEMPORARY` right, the functions that `PUBLIC` runs, and `temp_file_limit`. | 8a.12 |
| The grammar | The first word is `SELECT`, `WITH`, `VALUES` or `TABLE`. The words `INTO`, `INSERT`, `UPDATE`, `DELETE` and `MERGE` fail at any depth. | 7.25 |
| The refused functions | The closed list of 7.25. `set_config` is refused by the client and is not revoked on the server, because `pg_dump` needs it. | 7.25, 8a.13 |
| The session | The fixed parameters of 8a.4, read back at each new connection, with the key `search_path`. | 8a.4 |
| The protocol | One Parse message for the SQL of a caller. `DISCARD ALL` after each query. A connection lives 1 hour at most. | 8a.5, 8a.9 |
| The limits | A cap of rows plus one, a count of bytes, a deadline of 5 s above the time limit, a cancel request, and a check of the connection every second. | 8a.6, 8a.7 |
| The link | `verify-full` with channel binding, or `disable` with a warning. One host, one port, no Unix socket, no variable. | 8a.4 |
| The role of the store | A source on the server of the store MUST NOT use the user of the store (C20). | 13.14 |
| The errors | The page shows the first line of the server message. The logs hold the SQLSTATE only. | 8a.8, 12.15 |
| The attribution | A comment before the SQL and the `application_name` of each query. | 8a.11 |
| The accepted limits | One large row, `statement_timeout` for each message, the trust in the functions of the administrator, and the foreign tables. | 8a.15 |
| The other choices | The types of 7.16a, the `null` for a numeric NaN, the rule of 7.16b, no fill of gaps, the interval units of 7.23, PostgreSQL 15 to 18, the first release 0.1.0, and the scanner of the page. | 7.16a to 7.23, 8a.14, 9.10 |


### 19.33 The security choices of the MongoDB source

The design of section 8b and of 7.27 to 7.36 has choices that no review of real use has validated. Each choice has a default, and the code follows the default. The table below gives each choice. Each one is an open question, and a review can change a default.

| Choice | Default | Where |
|---|---|---|
| The place of the query in the format | The member `sql` holds one JSON text. No new member, no new `format_version`. | 4.15 |
| The driver crate | The crate `mongodb` and the crate `bson` from crates.io, with `rustls` and `ring`, behind the feature `mongo-driver`. The crate `mongodb` is a local copy with no environment read in `vendor/mongodb`. | 8b.2, 17.9 |
| The read-only rule | Three sides: the user, the closed commands with a wire guard, and the closed grammar. | 8b.3 |
| The user | The role `read` on the listed databases and no other role. The probe refuses a user with another role, a cluster privilege or a write action. | 8b.12, 8b.13 |
| The grammar | A closed allowlist of stages and operators, and the refused list of 7.32. `$expr` is checked with the operators of the allowlist. | 7.31, 7.32 |
| The joins | `$lookup` and `$unionWith` only to the collections of the key `collections`, in the same database. `$graphLookup` is refused. | 7.33 |
| The form of the block | A block `mongodb "<id>"` for each source, with the keys `databases` and `collections` as necessary lists and no default. | 13.12b |
| SRV | No `mongodb+srv`, no SRV lookup and no key for it. | 8b.4 |
| The topology | `direct` with one host, or `replica_set` with the complete list of members and the check M4. | 8b.4, 8b.12 |
| The variables | A value is a typed leaf of the tree, never text of the query. A free value never starts with `$`. A placeholder is refused in a key and in a name place. | 7.34, 7.35 |
| The frame | A nested document or an array is a JSON text. A mix of types is refused, except the number widening of 7.16c. No rule like 7.16b. | 7.16c, 7.16d |
| The limits | The stage `$limit` of `R + 1`, `maxTimeMS`, a deadline of 5 s above it, `allowDiskUse: false`, a kill of the cursor, the read concern `local`. | 8b.5 to 8b.7 |
| The errors | Mapping by the server code. The page shows the first line of the message. The logs hold the code only. | 8b.8, 12.15 |
| The accepted limits | One large reply, a command that runs to `maxTimeMS` after the client leaves, the server setting for JavaScript that Hune cannot read, and the role actions `collStats` and `dbStats`. | 8b.15 |
| The other choices | The versions 7.0 and newer, the first release 0.1.0, the importer rows and the frame with no column. | 8b.14, 15.6, 7.16c |


### 19.34 The query model of the MongoDB plugin and the MongoDB version

The rows of 15.6 for the plugin `haohanyang-mongodb-datasource` 0.5.0 follow the documentation of the plugin. They were not checked against a live plugin:

- the members `queryText`, `queryType`, `database` and `collection`;
- the form of the time macros;
- the form of the text.

M0 or the verification of MMG reads the MongoDB queries of a Grafana export (15.3). It records, for each query, the members of the target, the stages that it uses, its macros and its variables. It also records the version and the topology of the MongoDB server that Grafana reads. It records the privileges of the account that Grafana uses too.

The result can change the rows of 15.6, the macros of 7.34 and the supported versions of 8b.14. Each such change is a new rule, and the vectors of 16.13 follow it.

Open question: do the rows of 15.6 match the real plugin? Default today: the rows of 15.6 as written.

### 19.35 The card view of the list page, the threshold of its filters and the new look

The list page has a card view, and the whole page has a calmer look. The text of 9.7 and 9.25 follows the validated static mockups. These choices are new, and the owner MUST read them again:

- The key `server.list_filters_min_dashboards` (13.8). It is an integer from 0 to 1000, optional, and its default is 6. The upper bound is fixed. The list page shows the sort control and the tag filter only when the user sees more dashboards than this number. The session gives the number to the page (6.10). If the session has no number, or an invalid number, the page shows the filters.
- The threshold 6. The mockup hid the filters for "few" dashboards. The owner did not give the number. The default 6 is the value of the mockup. The count includes the items that are not valid.
- Three controls are removed. These are the search field, the view control (folder view and list view) and the collapse of a folder. The keys `hune.list.layout` and `hune.list.collapsed` of `localStorage` are not read any more. The URL parameter `q` stays and holds one tag name. It matches a tag exactly, with ASCII letters folded. It does not match a part of a title or of a uid any more.
- The sort control keeps "Alphabetically (A-Z)" and "Alphabetically (Z-A)". The mockup showed "Starred" and "Recent". Hune has no such state, and the owner did not ask for it.
- "Delete" in the menu of a card. It shows under the same condition as the three dots: `editing` is `true` and the item has `access.write`. The route `DELETE api/dashboards/<uid>` needs `dashboard.write`. The page does not read a separate right to delete. The server decides, and a `403` shows the usual refusal.
- The menu of a card. "Move" was in the menu of a row, and the dashboard page has no "Move". A small menu stays on a card that the user can edit, with "Export" and "Move". A card that the user cannot edit has no button, and its "Export" is in the menu "More actions" of the dashboard page.
- The folder title shows only when the user sees two folders or more. With one folder, the user has no menu to rename or to delete the folder from the list page.
- The entry redirect ignores `access.create`. A user who sees exactly one dashboard goes to it, also when the user can create a folder or a dashboard. The owner wanted the redirect for a user who also cannot create. The code does not check this condition.
- A card that is not valid shows its uid, because it has no title. The mockup had no such card.
- The look. The file `css/look.css` sets the new tokens of the surfaces, the accent, the buttons and the fields in the four contexts of the theme. The accent is `#2f5bd8` in the light theme and `#6c8cff` in the dark theme. The text on the primary button is dark in the dark theme. The edge of a field keeps the copied value, because of the 3:1 rule of 9.26.
- The tag chips use the 29 colours of Grafana as a tint. The mockup used colours that were chosen by hand. The chips of Hune are less bright.
- The logo. It is a square with round corners and one line that goes up. The green dot is gone. The site keeps its own accent for the mark of its header.

Open question: are these choices right? Default today: the text of 9.7 and 9.25.

### 19.36 The parallel run of the merge gate

`tests/ci/all.sh` runs the jobs of the gate at the same time on one host (16.17a). It runs each job to its end and does not stop at the first failure. The record `target/ci-all.ok` is the same as before. It names the commit after all jobs passed on a clean work tree.

The speed of the run depends on the host. The times of 16.17a come from a host with 22 cores, 93 GB of memory and Docker. A smaller host gives longer times, and the option `--max-jobs` limits the load. The run needs about 25 GB of disk for its target folders. A timing test can fail on a busy host with no fault of the product. Two tests of the front end got more runs for this reason, and the limits did not change.

The seeded random tests of `hune-core` run their seeds on all cores. The other choice was a higher `opt-level` for `hune-core` in the dev profile. It made the one-core test about 5 times faster (41 s against 205 s). It cost 16 s of compile time for each build, and it needs a change of the profile.

The threads made the test 12 times faster (17 s). They need no change of the profile and have no compile cost. The seeds, the cases and the checks did not change.

The owner MUST read again these choices. They are the parallel run on one host, the resources that it needs, and the threads for the random tests. The options of `all.sh` (`--only`, `--from`, `--max-jobs`) are the last choice. The jobs `pgsource` and `mongosource` also run their two versions at the same time now.

### 19.37 Several stores, the primary store and the read-only stores

A file can name several stores. The store with `primary = true` is the only store that accepts writes. Each other store is read-only. The text of 5.42 to 5.44, 11.35 and 13.10 gives the rules.

The owner set the shape of the blocks and the key `primary` on a block. The owner set the removal of three keys and the title of a read-only store. The owner set the failure at start for a volume and the last valid state after the start. The owner MUST read again these choices, which the owner did not set:

- The ids. The id of a volume is its label, and the id of the PostgreSQL store is `postgres`. A volume cannot have this label (C33). A file has 16 volumes at most. A `title` has 1 to 128 bytes.
- The `title`. A read-only store needs it and the primary store has none (C30). The titles of two read-only stores are different (C34). A read-only store shows as one root folder, and its folders are below it. The primary store shows at the root with no title.
- The `history_keep`. Only the primary store has it, and it needs it (C31). A read-only store has no history: the list is empty and the version is `404`.
- The clashes between stores. A folder id and a dashboard uid exist in one store only. A folder of the primary store has not the title of a read-only store. A break stops the start. A `create_folder` or a `rename_folder` with such a title gives `409 already_exists`. After the start, the catalog keeps the entry of the store that holds the id and ignores the other.
- The error `read_only_store`, status 403. Hune asks `dashboard.read` of the target first. A subject that cannot read it gets the refusal of any other write, with the same bytes (the case K321). A subject that can read it gets `read_only_store`. Hune never asks `dashboard.write` for a target of a read-only store. The mode `open` answers `read_only_store` at once.
- The routes that the rule covers. A move needs the primary store for the dashboard and for the target folder. The preview of a draft of a read-only dashboard or folder, and `validate_only` of a save or a create, are refused too. Nobody gets the SQL (`server.query_inspect`) of a read-only dashboard in the mode `authzen`, because this right needs `dashboard.write`.
- The flags. `access.write` of a read-only item is `false` with no question to the PDP. The item has `read_only` and `store`. The session keeps the name `editing`, and it adds `stores`.
- The empty store. The primary store can be empty, so the owner can create the first dashboard. A read-only store with no dashboard stops the start.
- The files that are not valid. A file of a volume that cannot be read, or that is not valid, stops the start. This holds for each volume, also the primary volume. The message has the label, the file and the reason. A foreign entry is still ignored, as before. A row of the PostgreSQL store that is not valid stays an invalid entry.
- The last valid state. After the start, a file that becomes not valid keeps its last valid state. The dashboard has `stale = "file_invalid"` for each subject that can read it. The member never holds the path or the message of the parser. A new file that is not valid is ignored. The log line is `store.rescan_rejected`, and the gauge is `hune_store_invalid_files{store}`.
- The period of the poll. Each store has its own period. The poller wakes at the shortest period.
- The commands. They work on the primary store only. `migrate-store` needs `storage.postgres` as the primary store.
- The replicas. A primary volume that more than one instance writes needs a shared volume and the lock. A read-only volume can be a copy for each instance. For the homelab, the PostgreSQL store is the primary store, and the volumes of the repository are read-only stores.
- The loss. A replica cannot be made view-only on a shared PostgreSQL primary with the same file any more. Each file needs exactly one primary store (19.39).
- The version. The change removes keys, so it is a minor version.

Open question: are these choices right? Default today: the text of the sections above.

### 19.38 Dashboards without a folder

A dashboard can have no folder. The API writes `null` for its folder. The text of 4.3, 5.9, 5.10, 5.24, 6.11 to 6.13, 6.19, 6.24 and of the file ACCESS.md gives the rules.

The owner set the form `null` in the API and the file at the root of a volume. The owner set the access node `folder:_root` and the root of Grafana without a folder. The owner set the stop of the start for a file at the root of a volume that is not a valid dashboard. The owner MUST read again each default below.

- The inside of the code. The store, the catalog and the handlers use one reserved value of the type `FolderId` for "no folder". Its text is `_root`. The check of 4.3 refuses it, so no client can send it as a folder id. The API never shows this text: it shows `null`. The access tree shows it as the node `folder:_root`.
- A body that has no member `folder` stays refused (6.13). The value `null` is a choice, not a default.
- The filter `GET /api/dashboards?folder=` with an empty value selects the dashboards without a folder. No other form exists.
- `GET /api/folders` has the member `root`, and it does not list the root as a folder. `root.access.write` follows `dashboard.write` on the node `_root`.
- The order. A dashboard without a folder comes before each dashboard of a folder, in all lists. In the display, the cards of the root come first, without a title.
- The rights. A person who has rights on real folders only does not see a dashboard without a folder (fail closed). The author of a policy adds a grant on `folder:_root`. The portal list of folders gets the item `_root`, "Root (no folder)", as the first item.
- The root of each store is the same node `_root`, because a uid is unique in all stores. A dashboard without a folder in a read-only store is read-only, and each write to it gives `read_only_store`.
- The break for a volume. A file `<name>.json` at the root of a volume, with a name that fits the uid rule, was ignored before. It is now a dashboard. A file that is not a valid dashboard, for example `package.json`, stops the start. The message names the volume, the file and the reason. A file with another kind of name is still ignored.
- The break for PostgreSQL. The schema version goes from 1 to 2. Run `hune migrate-store` before `serve`. The step is one way. A binary of schema version 1 refuses a database of schema version 2.
- The importer of Grafana puts the dashboards of the root of Grafana in no folder. The id `general` is no more reserved: a folder map can name a folder `general`.
- The version. This change moves the schema, so the release is a minor version.

### 19.39 The primary store, the history and the export as explicit rights

A file needs exactly one primary store. The history and the export are two explicit rights, with two mandatory keys and two actions of the PDP. The text of 5.42, 6.14, 6.17, 11.18, 11.21, 13.8 and 13.10 and of the file ACCESS.md gives the rules.

The owner set three rules. A file has exactly one primary store. A store with no history gives no history. The history and the export are explicit rights. The owner set the form of the right: two mandatory keys and two actions. The owner MUST read again the defaults below, which the owner did not set.

- The start. A file with no primary store, and a file with two or more, stop the start (C10). The mode "no primary store" is removed, and so are the contradiction C21 and the code `editing_disabled`. This is a break for each file that has no `primary`: add it to one store.
- The keys `server.history` and `server.export`. Both are Booleans with no default. A key that is not in the file stops the start. The value `false` is a ceiling in all modes, also in the mode `open`.
- The actions `dashboard.history` and `dashboard.export`. Hune publishes each one only when its key is `true`. A PDP can only restrict: it cannot give a right that the key refuses. A policy that grants neither action removes the history and the export for all subjects (fail closed).
- The history. It covers the list, one version, the diff and the restore. The restore needs `dashboard.history` and `dashboard.write`. A dashboard of a read-only store has no history: the routes give `404 history_unavailable` after `dashboard.read`, and the page has no "History".
- The export. It covers `download=true` on the read of a dashboard. The download of one version belongs to the history right, because the history page reads the stored text of a version this way. The page shows "Export this version" only with both rights. The commands of the command line are not concerned.
- The flags. Each dashboard has `access` with `write`, `history` and `export`. The page hides each entry that its flag refuses. A user who opens the history URL by hand gets the standard refusal page.
- The session. The member `editing` stays and is always `true`. Its removal is a follow-up.
- The version. The change adds two mandatory keys and removes a mode, so each file of an installation must change. The release is a minor version.

Open question: are these choices right? Default today: the text of the sections above.

### 19.40 A file with no primary store: `storage.read_only`

An installation can have only read-only stores, for example volumes inside an image. The key `storage.read_only = true` says this. The text of 5.42, 6.10, 11.18, 13.10 and 13.14 and of the file ACCESS.md gives the rules. This change reverses one choice of 19.39: a file can have no primary store, but it must say so.

The owner asked for the case with no primary store and for an explicit statement, not an omission. The syntax is a choice of the agent. The owner set neither a form nor a name for the key. The owner MUST read again the points below, which the owner did not set.

- The key. `storage.read_only` is a Boolean with no default. It is necessary when no store has `primary = true`. It is not permitted when a store has it. The value `false` stops the start. The value `null` is not a Boolean in the file format: it reads as an absent key.
- The contradictions. `server.history` is `false` (C35), because only the primary store has history. `server.query_inspect` is not `true` (C36), because the SQL needs the right to write.
- The actions of the PDP. Hune does not publish `dashboard.write` and `source.query`. It publishes `dashboard.read` and, when `server.export` is `true`, `dashboard.export`. A policy that names these two actions has no effect.
- The routes. Each write route and the preview of a draft answer `403 read_only_store` at once. Hune asks no question to the PDP. The session member `editing` is `false`, and the flag `access.create` is `false`.
- The history. `history_keep` is necessary only for a primary store when `server.history` is `true`. With `server.history = false`, the key is not permitted and the store keeps the newest version only. The save protocol needs this version. A file of 0.3.0 with `history = false` and `history_keep` must remove the key.
- The version. A file of 0.3.0 that sets `server.history = false` and `history_keep` stops the start. The release is 0.3.1, because the file of the installation that asked for this change is the only known one. The owner can ask for 0.4.0.

Open question: are these choices right? Default today: the text of the sections above.

## 20. Glossary and references

### 20.1 Glossary

| Term | Meaning in this specification |
|---|---|
| action document | The JSON document at `/.well-known/procurauth/actions` that lists the resource types, the actions and the limits of a service (protocol section 5). |
| AuthZEN | The authorization API of the OpenID Foundation. ProcurAuth is a profile of it. |
| `authzen` (mode) | The access mode in which Hune is a PEP of ProcurAuth (section 11). |
| backend | One implementation of the `Store` trait: the volume or PostgreSQL. |
| budget | A ProcurAuth limit that counts uses in a window. Hune has no budget. |
| canonical bytes | The one byte form of a document: fixed member order, indent of two spaces, one line end at the end (section 4). |
| catalog | The list of folders and documents that each instance keeps in memory. |
| ceiling | A ProcurAuth limit on one use. Hune has `max_rows` and `max_duration`. |
| class | The result of the importer for one Grafana member: `mapped`, `changed`, `dropped` or `unsupported` (15.5). |
| CloudNativePG | A Kubernetes operator for PostgreSQL. The reference for high availability (17.8). |
| conformance suite | The store tests that run on each backend with the same expected results (16.6). |
| cutover | The move of an installation from Grafana to Hune (15.15). |
| data link | A link in a cell or a point that a panel builds from the values. |
| dialect | The SQL rules of one kind of source: the lexer, the escape rules, the macros and the statement check (7.19). |
| display filter | The rule that hides items without a right. It never lets an operation run. |
| display name | The name of a field after the field configuration. `byName` and `byRegexp` match it. |
| document | One dashboard in the format of section 4: one file or one row. |
| edge | The proxy in front of Hune that checks the login. In mode `authzen`, it sends the identity token. |
| etag | The SHA-256, in hex, of the canonical bytes of a document. |
| `executedQueryString` | The final SQL that Grafana gives in the metadata of a frame. The verification compares it (15.14). |
| folder | A group of dashboards, one level deep: a directory or a row. |
| `format_version` | The member of a document that gives the version of the format. |
| frame | The columnar JSON result of a panel query (section 7). |
| history version | A stored version of a document. Each save adds one. |
| identity token | The signed JWT that the edge sends to Hune (protocol section 9). |
| interval | The time step of `$__timeInterval` and `$__interval_s` (section 7). |
| lexer | The code that reads SQL into tokens: literals, identifiers, comments and code. |
| literal | A string literal of SQL, between single quotes. |
| long-to-wide | The step that gives one series for each set of label values (section 9). |
| macro | A `$__name` form that the server expands (section 7). |
| metadata member | A dropped Grafana member with no behaviour in Hune. It needs no acceptance (15.5). |
| `open` (mode) | The access mode without authentication in Hune. A proxy guards the access. |
| ops listener | The second listener, `server.ops_listen`, for `/livez`, `/readyz` and `/metrics` only. |
| options route | `/.well-known/procurauth/resources`, which lists the child nodes of the tree (protocol section 5). |
| panel | One chart or table of a dashboard. |
| parity harness | The tool that compares the panels of Grafana and Hune (15.14b). |
| Patroni | A high-availability manager for PostgreSQL. The failover test uses it (16.11). |
| PDP | Policy decision point: the server that answers the questions of a PEP. |
| PEP | Policy enforcement point: the service that asks the PDP before each operation. |
| readiness | The state of `/readyz`. It fails only until the catalog has loaded one time (section 14). |
| reuse | A panel that shows the result of another panel, the Grafana "-- Dashboard --" source. |
| `save_id` | The UUID of one save attempt, sent as `Idempotency-Key`. |
| section | A row of a dashboard: an optional title and its panels. |
| series U | The conformance cases of ProcurAuth for a PEP. |
| source | One data source of the configuration: a `clickhouse "<id>"` or a `postgres "<id>"` block. |
| SQLSTATE | The five-character error code of PostgreSQL. Hune maps it to a code of section 6 (8a.8). |
| store | The place of the documents, behind the `Store` trait (section 5). |
| subject | The person or key of a request. In mode `open`, the subject is `-`. |
| T | The day of the proxy switch of the cutover (15.15). |
| tombstone | The history version that a delete writes. |
| uid | The id of a dashboard. It is also its file stem or its row key. |
| variable | A dashboard control whose values go into the SQL (section 7). |
| volume | The folder of dashboard files on a mounted file system (section 5). |
| write guard | The check of `Sec-Fetch-Site`, `Origin` and `Content-Type` on each write (section 12). |

### 20.2 References

| Id | Document | Place |
|---|---|---|
| PROTOCOL | ProcurAuth Protocol, version 1.0 | `ProcurAuth/procurauth`, `protocol/procurauth-protocol.md` |
| CONFORMANCE-DATA | The conformance cases and the runner `access-conformance` | `ProcurAuth/procurauth`, `conformance/` and `crates/access-conformance/` |
| PEP | The crates `procurauth-pep` (with the trait `PdpTransport` in `transport.rs`) and `procurauth-core` | `ProcurAuth/procurauth`, `crates/` |
| FAKE-PDP | The test PDP | `ProcurAuth/procurauth`, `crates/fake-pdp/` |
| AUTHZEN | AuthZEN Authorization API 1.0, OpenID Foundation | `openid/authzen` |
| RFC 2119, RFC 8174 | Key words for requirement levels | IETF |
| RFC 6901 | JSON Pointer | IETF |
| RFC 7519 | JSON Web Token | IETF |
| RFC 8259 | The JSON data interchange format | IETF |
| RFC 8615 | Well-known URIs | IETF |
| RFC 9110 | HTTP semantics (`ETag`, `If-Match`) | IETF |
| RFC 9562 | Universally Unique IDentifiers (UUID versions 7 and 8) | IETF |
| FIPS 180-4 | Secure Hash Standard (SHA-256) | NIST |
| SEMVER | Semantic Versioning 2.0.0 | semver.org |
| WCAG 2.2 | Web Content Accessibility Guidelines 2.2 | W3C |
| ASD-STE100 | Simplified Technical English | ASD |
| HCL | HCL native syntax specification | `hashicorp/hcl` |
| CHDASH | ClickHouse Dash 1.3.1, commit `e970694e`: the chart engine, the lexer, the CSP, the wasm rules | The ClickHouse Dash repository: `src/static/app_chart_core.js`, `src/sql_lexer.hpp`, `tests/frontend/model/sql-lex-vectors.json`, `docs/access.md`, `docs/configuration.md`, `docs/wasm.md` |
| CH-PLUGIN | Grafana ClickHouse data source plugin 4.5.1: the macros | `grafana/clickhouse-datasource`, `README.md` of the tag `v4.5.1` |
| CH-DOCS | ClickHouse documentation: HTTP interface, settings, `readonly`, settings constraints, `CHECK GRANT` | `clickhouse.com/docs` |
| CNPG-DOCS | CloudNativePG 1.30 documentation: `Cluster`, `Database`, managed roles | `cloudnative-pg.io` |
| PATRONI | Patroni documentation: REST API, synchronous mode | `patroni.readthedocs.io` |

### 20.3 Measured use of one Grafana instance

These facts come from the measure of one Grafana instance that queries ClickHouse. Sections 1, 15 and 18 use them.

| Fact | Value |
|---|---|
| Plugins | `grafana-clickhouse-datasource` 4.5.1, `haohanyang-mongodb-datasource` 0.5.0 |
| Dashboards | 10 in 2 folders plus 3 at the root; schema `dashboard.grafana.app/v2` in the unified storage (table `resource`); one dashboard is v1 |
| Panels | 161 in total, 7 rows; table 73, timeseries 44 (41 lines, 3 bars), stat 30, barchart 9, piechart 2, xychart 2, bargauge 1 |
| Queries | about 142 raw ClickHouse SQL texts (96 tables, 46 time series; median 1,171 characters, longest 71,119); 2 MongoDB queries; 17 panels reuse another panel |
| Macros | `$__timeFilter` 95, `$__timeInterval` 42, `$__toTime` 29, `$__fromTime` 23, `$__interval_s` 19 |
| Variables | 21: 9 query (7 multi-value with All and a refresh on a time change), 10 text boxes, 2 custom; 306 uses in the SQL |
| Overrides | 332 in 75 table panels: `byName` 310, `byRegexp` 21 |
| Transformations | 56: `filterByValue` 36, `groupingToMatrix` 18, `prepareTimeSeries` 1, `labelsToFields` 1 |
| Editing | 138 saved versions for the 10 dashboards |
| Sharing | 1 public dashboard, 7 short links |
| Users | one shared identity behind a proxy that authenticates the users; 52 distinct browsers in about 2 months |
| Not used | alert rules, library panels, playlists, snapshots, teams, API keys, correlations, plugin settings, custom annotations |
