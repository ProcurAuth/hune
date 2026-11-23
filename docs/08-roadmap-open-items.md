# Part 8: Roadmap, open items, glossary and references

Part 8 of the Hune specification (see `01-overview.md`).

This part has three sections. Section 18 gives the milestones, the effort, the cutover point and the rule to add a feature.
Section 19 lists the items that are still open. Section 20 gives the glossary and the references.

## 18. Roadmap, milestones and effort

### 18.1 Principles

- The work has the milestones M0 to M7. M3 has two parts, M3a and M3b. A parity spike of three weeks comes after M0 and
  before M1 freezes the format and the expander (18.3a). Each milestone has content, exit criteria and an effort.
- The risky parts come first. M0 measures the real use, and the spike tests the riskiest Grafana rules on real
  dashboards. M1 fixes the store contract, M2 the SQL expansion, and M3a the verified import.
- The reference deployment moves from Grafana to Hune after M4 (18.12). M5, M6 and M7 come after the cutover.
- Releases: the first release is 0.1.0 (decision ID-K335). It holds the work of the milestones that exists on its date.
  Version 1.0.0 comes after the exit criteria of M7 (17.10). The release of the cutover has no version number yet. The
  parity tools of M3a stay private (15.18).
- A milestone is complete only when all its exit criteria pass. Each milestone names only the cases that it can pass. The
  tests are in section 16, and 16.17 gives the jobs of the merge gate from each milestone.

### 18.2 Milestones

The effort is the bottom-up estimate of the written specification for one experienced engineer (18.11). One
person-month is 20 business days of one person.

| M | Name | Content (details in 18.3 to 18.10) | Exit criteria (summary) | Effort (person-months) | Range |
|---|---|---|---|---|---|
| M0 | Measure | Copy of `grafana.db` and exports; counts | Each measure has a count; each feature is mapped or out | 0.4 | 0.3 to 0.5 |
| Spike | Parity spike, three weeks | Two real dashboards: import, SQL against `executedQueryString`, one large table | A written result for each check of 18.3a | in M2, M3a and M3b | |
| M1 | Foundations and volume read | Workspace, configuration, listeners, format, volume read, CI, image, release dry run | Gate of M1 green; synthetic volume served | 2.0 | 1.6 to 2.5 |
| M2 | Query engine | ClickHouse client, frames, limits, cancellation, lexer, macros, variables, query routes | Random tests and ClickHouse cases green | 2.0 | 1.6 to 2.5 |
| M3a | Importer and SQL verification | `import-grafana`, parity tools, expansion vectors, V1 to V3 | Importer tests green; V1 to V3 signed | 1.25 | 1.0 to 1.6 |
| M3b | Viewer and visual verification | Front end of section 9, front-end units, V4 and V5 | Browser and front-end units green; V4 and V5 signed | 3.75 | 2.9 to 4.7 |
| M4 | Editor and volume writes | Volume writes, history, editor of section 10, conflicts, restore | Volume conformance and editor tests green | 2.75 | 2.1 to 3.5 |
| Cutover | Cutover of the reference deployment | The plan of 15.15 | T + 14 d without rollback; Grafana stopped | 0.5 | 0.4 to 0.6 |
| M5 | PostgreSQL backend and failover test | PostgreSQL store, `migrate-store`, `copy-store`, failover test, compose and Kubernetes examples | Conformance on PostgreSQL 15 and 18; failover F1 to F9 | 1.5 | 1.2 to 1.9 |
| M6 | ProcurAuth | Mode `authzen`, PEP, action document, options route, display filter, ceilings | Series U clean | 1.25 | 1.0 to 1.6 |
| M7 | Hardening and 1.0.0 | OTLP exporter, audit lines, fuzz, load test, CloudNativePG drill, documentation | Fuzz, load and drill pass; release 1.0.0 | 1.25 | 1.0 to 1.6 |
| | Total | | | 16.65 (about 16.6) | 13 to 21 |
| MPG | PostgreSQL data source | Dialect, client, role checks, block, importer rows and tests (18.10a) | `pgsource` job and the cases of 18.10a green | 1.1 (not in the total) | 0.9 to 1.5 |
| MMG | MongoDB data source | Query language, client behind a narrow interface, user checks, block, importer rows and tests (18.10b) | `mongosource` job and the cases of 18.10b green | 1.2 (not in the total) | 1.0 to 1.7 |

The cutover comes after M0 to M4 and the cutover work: about 12.6 person-months for one engineer.

### 18.3 M0: Measure

The measure of 1.3 gives the use of Grafana, but not the content of the queries. M0 measures it before the design of M2, M3a
and M3b becomes final.

Content:

1. Get the explicit approval of the owner for a new copy of `grafana.db` and for the export of 15.3 (19.3).
2. The operator makes the copy and the export, with the go of the owner for each run.
3. Measure on the copy and on the export, on the owner's machine:
   - the place and the format of each of the 306 variable uses (literal, code, `IN (...)`, `${v:format}`, `[[v]]`);
   - each query variable that the SQL uses as an identifier, which v1 refuses (19.11);
   - each other `$__` name in the SQL, and each use of `dictGet` (19.6);
   - the field defaults and the panel options of each type: `legend`, `tooltip`, `stacking`, axis side, threshold style,
     bar orientation, reduce calculations, colour modes (19.17);
   - the legend placement `right`, the legend mode of each panel (list or table), and the tooltip mode `none` that maps
     to `hidden` (19.20);
   - the `filterByValue` matchers outside the measured set, among them `substring` and `notSubstring` (15.6);
   - the 88 cell types of the overrides that the measure of 1.3 does not classify (19.15);
   - the units of the real panels (19.16), and the real use of `displayName` and `hidden` (19.19);
   - the use of table footers, pages and column filters (19.21);
   - panel time overrides, repeats, panel links, dashboard links, and the tokens of data links (19.21);
   - the regex of the variables with its flags, the sort with the natural modes, the hide and the chains (19.18);
   - the use of a UTC time axis, horizontal bars, threshold lines and a right y axis (19.20);
   - what the 138 saves changed, from a diff of each version with the version before;
   - the number of queries of each panel (the expected total is 142 ClickHouse, 2 MongoDB and 17 reuse queries);
   - the stored form of each dashboard (v1 or v2) and the v2 API version (15.3);
   - the Grafana version and the plugin version.
4. With the go of the owner, run two read-only queries on the production ClickHouse: `SELECT version()`, and the rows and
   bytes of each query of the Grafana account over two weeks from `system.query_log`.
5. Write the results to the private evidence directory (15.18). Update this specification with a new version.

Exit criteria:

- Each measure of item 3 and item 4 has a count.
- Each feature that M0 finds is a `mapped` row of 15.6 by the rule of 18.14, or an `unsupported` row.
- A new version of section 4 holds the classified cell types and the complete unit list. Its option sets hold no option
  that no real panel uses.
- The items 19.3, 19.4, 19.7, 19.15 to 19.19 and 19.21 are closed. The effort of 18.11 has a new estimate (19.5).

### 18.3a The parity spike

The spike comes after M0 and takes three weeks. M1 does not freeze the document format (section 4) or the expander
(section 7) before the spike ends. It tests early the Grafana rules that the reviews found most at risk.

Content:

1. Import two real dashboards with a first version of the importer: one in the v2 form and the dashboard in the v1 form.
2. Expand their SQL and compare it with `executedQueryString`, with a first version of the capture tool (15.14a). This
   run needs the go of the owner.
3. Render one large table panel of a real dashboard with its overrides, in a prototype page.

The checks:

- the pattern form of `byRegexp` and of `reduce.fields` (19.29);
- the rule of `max_data_points` (19.31);
- the v2 variable sort names and the transformation items of 15.6, and the matcher `between`;
- a sample of the 306 variable uses (19.11).

Exit criteria: each check has a written result, and the specification has a new version with the fixes before M1 freezes
the format. The work of the spike is a first part of M2, M3a and M3b, so its effort counts in these milestones.

### 18.4 M1: Foundations and volume read

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
- A dry run of the release gives the two archives and `SHA256SUMS`.

### 18.5 M2: Query engine

Content:

- The `DataSource` trait, the ClickHouse HTTP client with rustls, the frames, the limits, the queues and the cancellation
  (section 8).
- The lexer with the shared vectors, the macros, the interval rule, the variables, the substitution and the statement check
  (section 7).
- The routes of the panel query, of the variable options, of the sources and of the preview (section 6).
- The seeded random tests and the ClickHouse cases on two versions.

Exit criteria:

- The random tests pass on the seeds 1 to 64.
- The cases C01 to C13 pass on the production version and on the latest LTS release.
- The cancellation case C05 passes within 2 s (19.23).
- The cases A18 to A24 and K14a (the mode `open` part of K14) pass.
- The p95 of the server overhead is below 50 ms on the synthetic 45-panel dashboard. The load script of 16.16 measures it
  with 5 viewers.

### 18.6 M3a: Importer and SQL verification

Content:

- `hune import-grafana` with its report, its acceptance file and its nginx map (section 15).
- The capture tool and the test `parity_private` of `tools/parity/` (15.14a).
- The expansion vectors from the pinned Grafana (16.13).
- The verification V1 to V3 on the 10 real dashboards (15.14), with the go of the owner for each production run.

Exit criteria:

- The importer cases I01 to I19 pass. The expansion vectors equal the output of the pinned Grafana.
- V1 to V3 pass. The owner signs the acceptance file and the list of explained SQL differences.

### 18.6a M3b: Viewer and visual verification

Content:

- The front end of section 9:
  - shell, list, grid and sections, panels that load when visible;
  - time picker, refresh, URL state, variables bar;
  - the 7 panel types, the table features, field configuration, units, mappings, data links;
  - transformations, long-to-wide, panel reuse;
  - themes, accessibility and "View data".
- The changes of the chart engine that M0 finds in use (19.20).
- The front-end units and their vectors (16.10a).
- The harness `compare.mjs` (15.14b), and the verification V4 and V5.

Exit criteria:

- The browser cases B01 to B13, B14 on the list and dashboard pages, B15, and B24 to B27 pass.
- The front-end units of 16.10a pass.
- V4 and V5 pass. The owner signs the list of explained differences and the visual review.

### 18.7 M4: Editor and volume writes

Content:

- The writes of the volume: atomic write, history, writer lock, tombstone, restore and read-only mode (section 5).
- The editor of section 10: SQL tab, panel form, JSON tabs, layout by grid numbers, sections.
- The save with `If-Match` and `Idempotency-Key`, the draft in `localStorage`, the conflict screen.
- The history list, the read-only view, the diff and the restore. The export and the import of one document. The delete
  and `hune restore`.
- The write guard and `server.editing` (sections 12 and 13).

Exit criteria:

- On the volume, the common tests of 5.34 pass, except SC-44 and SC-47, which need PostgreSQL. The volume tests of 5.35
  and the case SX-01 of 16.7 pass.
- The browser cases B14 on the editor, history and conflict pages, and B16 to B22 pass. G07 passes for the volume example.
- The preconditions of 15.15 are met. The release of the cutover has no version number yet.

### 18.8 Cutover

Content: the plan of 15.15. The preprod environment goes first. Then come these steps:

- the parallel run of 14 days and the edit freeze;
- the final import and the nginx switch;
- Grafana read-only for 14 days, and the rollback window of 30 days.

Exit criteria:

- T + 14 d passes with no rollback trigger. Grafana is stopped.
- At T + 30 d, the owner decides the item 19.10.

### 18.9 M5: PostgreSQL backend and failover test

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

M6 content: the mode `authzen` of section 11. It holds the PEP on `procurauth-pep` and `procurauth-core`, and the check of
the identity token. It holds the rustls `PdpTransport` on a bounded pool of threads for synchronous calls. It also holds the
action document, the options route, the display filter, the ceilings, `docs/ACCESS.md` and `docs/CONFORMANCE.md`.

M6 exit criteria:

- The series U run of 16.12 passes, with the counts of 11.32 at the commit of 19.28. Its expected file is empty, or the
  owner accepts each line.
- The cases A39 to A45, K14b (the mode `authzen` part of K14) and B23 pass.

M7 content:

- the audit log lines (section 14);
- the fuzz targets, the load test and the CloudNativePG drill;
- the documentation in ASD-STE100: README, configuration, access, and the generated reference;
- the own OTLP exporter of the `telemetry` block (section 14). It is part of 1.0.0. A build before M7 has no exporter: it
  stops at start with `telemetry.enabled: this build has no OTLP exporter` when `telemetry.enabled = true`. The value
  `telemetry.enabled = false` is valid in every build.

M7 exit criteria:

- Each fuzz target runs 10 minutes on 7 nights in a row with no crash.
- K14c (the telemetry part of K14) and K15 on the `distributed` output pass, with the OTLP exporter.
- The load test of 16.16 and the drill of 16.19 pass.
- The language check of 16.18 gives no error on the documentation. Release 1.0.0.

### 18.10a MPG: PostgreSQL data source

MPG is outside the milestones M0 to M7 and their total effort. It needs the work of M1 to M6. It does not need M7. It follows the rule of 18.14 and the steps of 8.14.

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
- The owner has answered the items of 19.32, or has accepted their defaults.

### 18.10b MMG: MongoDB data source

MMG comes after MPG. It is outside the milestones M0 to M7 and their total effort. It needs the work of M1 to M6. It also needs the generic parts of MPG: the variant of the source type, the admission, the probe loop and the access rules. It does not need M7.

It follows the rule of 18.14 and the steps of 8.14. The query fits the member `sql` (4.15), so item 7 of 18.14 needs no new `format_version`.

The work has two parts, so that the code can start before the owner decides on the driver crates (`ID-MG2`):

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
  - Nothing of it starts before the owner answers `ID-MG2`.

Content:

- The query language, the frame types, the block and the importer rows of part 1.
- The client of `hune-source` (section 8b): the commands, the wire guard, the limits, the cancellation, the pool, the probe and the user checks.
- The tests of 16.9b, and the cases A52 to A56, K18, I23, B47 and B48.

Exit criteria:

- The unit tests and the golden vectors of the language pass, and the random tests R10 pass.
- The `mongosource` job passes: MS01 to MS18 on MongoDB 7.0 and on the newest 8.x release. The cases A52 to A56, K18, I23, B47 and B48 pass.
- The macro vectors of `tests/golden/expansion-mongo/` agree with the pinned plugin, or each difference is recorded.
- The owner has answered the items of 19.33, or has accepted their defaults.

### 18.11 Effort and assumptions

The total effort is about 16.6 person-months for one experienced engineer, with a range of 13 to 21 person-months. The
cutover comes after about 12.6 person-months. This is a bottom-up estimate of the written sections. The first design
estimates gave about 10 person-months (9.5, 10 and 9.8): they were made before the specification existed.

The written scope is larger. It holds about 230 named test cases and 17 vector sets of the front end. The cases are SC
52, SV 28, SP 19, SX 1, A 46, C 13, B 27, I 19, K 15 and G 9: 229 in total.

It also holds an own router, and an own ClickHouse client with `streaming` and cancellation. It also holds a synchronous TLS transport to the PDP and an importer of four input forms.
It also holds a parity harness against Grafana, and a Patroni test in two failure modes.

| M | Effort | Main reason |
|---|---|---|
| M0 | 0.4 | M0 also classifies options, units and cell types (18.3). |
| M1 | 2.0 | Closed parser with pointers and positions, about 150 rules, canonical writer; configuration with modes; router, logs, metrics. |
| M2 | 2.0 | Lexer, placeholders, escape rules, the property P, macros; own HTTP client with streaming, types, errors, admission, cancellation. |
| M3a | 1.25 | Importer of four input forms (0.75); capture tool, compare test and V1 to V3 (0.5). |
| M3b | 3.75 | Viewer (1.0), table (0.75), six other types and the engine changes (1.0), field rules and transformations (0.75), harness and V4, V5 (0.25). |
| M4 | 2.75 | Volume writes and recovery (0.75), editor (1.25), conflicts, history, diff, import and export (0.5), conformance (0.25). |
| Cutover | 0.5 | Preprod, a parallel run of 14 days with daily imports, fixes. |
| M5 | 1.5 | PostgreSQL store, roles, TLS, copy and export, Patroni harness in two modes, examples. |
| M6 | 1.25 | TLS transport, questions, one flight, action document, options route, display filter, series U. |
| M7 | 1.25 | OTLP exporter, fuzz, load, CloudNativePG drill, documentation in ASD-STE100. |

The coordinator of the specification made an earlier estimate for the same scope, and it was higher than the first design
estimates. So the owner SHOULD plan with the upper half of the range: about 17 to 21 person-months.

The estimate holds only with these assumptions:

1. One experienced engineer works full time on Hune. The engineer knows Rust (tokio, hyper), native JavaScript and
   ClickHouse.
2. The owner reviews each delivery within 2 business days. The owner signs the acceptance file of M3a and the visual
   review of M3b.
3. The work uses these parts as they are: `procurauth-pep` and `procurauth-core`, the lexer vectors of ClickHouse Dash
   and the conformance cases of ProcurAuth. `hclconf` has only the three rules of 13.1. The chart engine gets only the
   changes of 19.20 that M0 finds in use, and M3b counts them.
4. M0 finds no feature outside the measured set. Each feature that M0 adds follows 18.14 and adds its own effort.
5. The owner approves the copy of `grafana.db` and each production run in time for M0, the spike, M3a, M3b and the
   cutover.
6. An account of the ClickHouse HTTPS interface, with a CA file, exists for Hune at the start of M3a (19.6).
7. The scope of this specification does not change during the work.
8. The effort does not include the time of the owner's reviews. It does not include the calendar time of the parallel run
   and of the rollback window. It does not include the operation work on the hosts of the reference deployment.

Effort is not calendar time. The cutover adds about 3 weeks of calendar time before T: 7 days of preprod and 14 days of
parallel run. The work of M5 can start during the parallel run.

Calendar with one engineer: the cutover at about month 13, and 1.0.0 at about month 17 to 18.

Calendar with two engineers, the plan that the owner SHOULD use. After M1, one engineer takes the front end: M3b and the
editor of M4. The other takes the back end: M2, M3a, the volume writes of M4, then M5 and M6, which do not block the
cutover. The cutover then comes at about month 7, and 1.0.0 at about month 9 to 10.

The largest risks of the estimate are in M3b and M4. They are the table panel with its 332 overrides, the display-name
rules of Grafana, the gaps of the chart engine, and the editor. Their ranges are the widest.

These choices lower the cost where no requirement needs the work:

- The Kubernetes examples hold one Deployment, one Job and the CloudNativePG objects (17.8).
- M0 removes the panel options that no real panel uses before M1 freezes the format (19.17).

### 18.12 The cutover point

The reference deployment moves from Grafana to Hune after M4, with a release that has no version number yet. Hune then runs in volume storage and in mode `open`,
behind the nginx login of the reference deployment. The conditions:

- The verification V1 to V3 of M3a and V4 to V5 of M3b passed (15.14).
- The exit criteria of M4 passed.
- The preconditions of 15.15 are met.

Reason: people edit the dashboards (138 saves), so the editor must exist before the cutover. The reference deployment needs neither
PostgreSQL nor ProcurAuth. The cutover is not after M3b, because there is no editor then. It is not after M7, because M5 to
M7 give the reference deployment nothing that it uses.

### 18.13 Out of scope

These features are not in Hune 1.0. Each one needs the rule of 18.14 to enter:

- alerts, alert rules and contact points;
- data sources other than ClickHouse, PostgreSQL and MongoDB (milestones MPG and MMG): Kafka;
- the built-in annotations of Grafana (its stored events and alerts), and the annotations that a person writes with the mouse. Annotations from a saved query on a ClickHouse, a PostgreSQL or a MongoDB source are in scope (4.5b);
- library panels, playlists, snapshots, correlations, Explore, ad hoc filters;
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
- controls of the Grafana editor that have no member in the closed format of 4.27 to 4.33d (19.20m). A new member needs a new decision. It also needs a row in the table of 10.3h and a control that the audit B153 can prove.

### 18.14 The rule to add a feature later

A feature enters Hune only when all these conditions are true:

1. A measure shows its use at the reference deployment (`grafana.db`, access logs, `system.query_log`), or the owner writes a decision. A
   feature never enters because Grafana has it.
2. The specification gets a new written decision. It is a new row in the decisions table of section 2, and a new numbered
   section or paragraph with the status DECIDED.
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

Decided for v1.1 (section 10). Drag and resize of the panels on the grid is done in the work of 0.1 (ID-98, 10.5a). Wave 2 of milestone F did the visual editors (10.3a, 10.3b). They edit the overrides, the mappings, the data links, the transformations, the variables and the dashboard settings. They are no longer v1.1 items.

Candidates from the design papers. They are not decided, and each one needs the rule of 18.14:

- the upload of a Grafana JSON file in the user interface;
- a list of the deleted dashboards in the user interface;
- SQL autocomplete in the editor;
- ClickHouse query parameters (`{name:Type}`) next to the substitution of section 7;
- a first WebAssembly kernel, when a measure on the real dashboards shows a gain, with the rules of ClickHouse Dash
  `docs/wasm.md`.

## 19. Open items

### 19.1 Rules

Each item below is open. Each item has an owner and a milestone or a date by which it MUST be closed. A closed item becomes
a decision in a new version of the specification, and its number stays. The other parts of this specification hold no open
point.

| Item | Subject | Owner | Close by |
|---|---|---|---|
| 19.2 | Who uses the public dashboard and the short links | The owner, with the team of the reference deployment | T − 14 d |
| 19.3 | The content of the 142 queries | The owner (approval), the engineer (measure) | M0 exit |
| 19.4 | The ClickHouse version in production | The owner (go), the operator | M0 exit |
| 19.5 | The effort range | The owner | M0 exit, then M3b exit |
| 19.6 | The ClickHouse account, CA and `dictGet` grants for the HTTPS interface | The owner, with the team of the reference deployment | M3a start |
| 19.7 | The stored form and the v2 API version of the dashboards | The engineer | M0 exit |
| 19.8 | The product name | The owner | M1 exit (before 0.1.0) |
| 19.9 | The dashboards of the preprod Grafana | The owner, with the team of the reference deployment | M4 exit |
| 19.10 | The removal of Grafana and the archive of `grafana.db` | The owner | T + 30 d |
| 19.11 | Variable uses that put free text or a query variable in code | The owner (decision for each use), the engineer | M3a exit |
| 19.12 | The retention of the private evidence | The owner | T + 30 d |
| 19.13 | The production limits of the ClickHouse source at the reference deployment | The engineer, the owner | M4 exit |
| 19.14 | The memory allocator of the musl binary | The engineer | M2 exit |
| 19.15 | The 88 cell types that the measure of 1.3 does not classify | The engineer | M0 exit |
| 19.16 | The complete unit list | The engineer | M0 exit |
| 19.17 | The panel options that no real panel uses | The engineer | M0 exit |
| 19.18 | Regex flags and natural sort modes of the variables | The engineer (measure), the owner (decision) | M0 exit |
| 19.19 | The real use of `displayName` and `hidden` | The engineer | M0 exit |
| 19.20 | The gaps of the chart engine | The engineer (measure and change), the owner (review of the change in ClickHouse Dash) | M0 exit for the measure, M3b exit for the change |
| 19.21 | Data-link tokens, table footer, pages and the other panel features of M0 | The engineer | M0 exit |
| 19.22 | Unused: the checks moved to 15.14b | none | none |
| 19.23 | The detection of a client close by hyper on HTTP/1.1 | The engineer | M2 exit |
| 19.24 | `ceiling_exceeded` for `max_rows` after a byte overflow | The owner (decision), the engineer | M6 exit |
| 19.25 | No durable audit record for a folder change | The owner | M4 exit |
| 19.26 | The three rules of `hclconf` | Closed (ID-01) | M1 exit |
| 19.27 | The token rules of `procurauth-core`: whole-number `iat` and `exp`, `kid` with two keys or more | The owner (maintainer of ProcurAuth) | M6 start |
| 19.28 | The pinned commit of ProcurAuth and the series U figures | The engineer | M6 exit |
| 19.29 | The pattern form of `byRegexp` and of `reduce.fields` | The engineer | Spike exit (18.3a) |
| 19.30 | The common subset of the regular expressions | The engineer, with the writer of section 4 | Spike exit |
| 19.31 | The rule of `max_data_points` | The engineer, with the writer of section 9 | Spike exit |
| 19.32 | The security choices of the PostgreSQL source | The owner, the engineer | MPG exit |
| 19.33 | The security choices of the MongoDB source | The owner, the engineer | MMG exit |
| 19.34 | The query model of the MongoDB plugin, the two real queries, and the MongoDB version | The engineer | MMG start |

### 19.2 Who uses the public dashboard and the short links

Grafana holds one public dashboard and 7 short links. Section 18.13 removes both from v1. The nginx login guards all of
`/grafana/`, so the public dashboard is not public today. The owner asks the team of the reference deployment who uses them.

During the parallel run, the operator reads the nginx log for `/grafana/public-dashboards/` and `/grafana/goto/`. A use from
outside the login of the reference deployment needs a new decision by 18.14. Without such a use, the item closes as "not used".

### 19.3 The content of the 142 queries

The measure of 1.3 does not give what the 142 queries and the 9 variable queries do. M0 measures them (18.3). The measure needs a
new copy of `grafana.db`. The copy is an action on the production server. It needs the explicit approval of the owner
before it starts. Without the approval, M0 cannot end, and the verification of M3a cannot start.

### 19.4 The ClickHouse version in production

The ClickHouse cases of 16.9 run on the production version and on the latest LTS release. M0 reads the production version
with `SELECT version()`, with the go of the owner. The version goes into `tests/images.lock` with a digest.

### 19.5 The effort range

The estimate is about 16.6 person-months, with a range of 13 to 21 (18.11). The owner plans with the upper half and with
two engineers. The engineer makes a new estimate at the exit of M0 and of the spike, with the measures. The engineer
makes another at the exit of M3b, with the real cost of the front end.

### 19.6 The ClickHouse account and CA for the HTTPS interface

Hune uses the HTTP interface of ClickHouse with TLS (section 8). The operator of the ClickHouse server gives these items:

- a dedicated account for Hune, with the profile, the constraints and the grants of 8.12;
- one `dictGet` grant for each dictionary that the dashboards use, when M0 finds such a use (8.12);
- the CA file of the certificate of port 8443;
- the name in that certificate, for `tls.server_name`.

The case C09 checks the grants of the account. Without this item, Hune cannot query the production ClickHouse for V4.

### 19.7 The stored form and the v2 API version of the dashboards

The export of 15.3 needs the stored form of each dashboard (one is v1). It also needs the v2 API version that Grafana
serves. M0 reads both from the copy of `grafana.db` and from Grafana.

### 19.8 The product name

The owner chose the name Hune on 2026-11-12. The name is in the binary, the crates, the metric prefix and the log events.
It is also in the HTTP header, the `localStorage` keys and the tables of the PostgreSQL backend. A rename after 0.1.0 is a change
of the contract (17.10). The crate name `hune` is free on crates.io, and no notable GitHub repository has the name.
The script `tools/rename-hune.py` made the rename.

### 19.9 The dashboards of the preprod Grafana

A preprod Grafana exists. The plan of 15.15 moves preprod first. The
owner asks the team of the reference deployment if the preprod Grafana holds dashboards that must move, or only copies of production.

### 19.10 The removal of Grafana and the archive of `grafana.db`

At T + 30 d the rollback window ends (15.15). The owner decides when the Grafana service leaves the compose file of the
production host. The owner also decides where the archive of `grafana.db` stays, and for how long.

### 19.11 Variable uses that put free text or a query variable in code

Section 7 refuses free text outside a literal. Some of the 306 uses can put a text box value in code on purpose. In v1,
section 7 also refuses the value of a query variable as an identifier. M0 lists each such use. For each use, the owner
chooses one of two changes: a `custom` variable with fixed options, or a change of the SQL. The verification V3 shows that
no such use is left.

### 19.12 The retention of the private evidence

The exports, captures, screenshots and reports of 15.14 hold confidential data (15.18). The owner decides how long they stay
on the owner's machine after T + 30 d, and when they are deleted.

### 19.13 The production limits of the ClickHouse source at the reference deployment

The values of `max_result_rows`, `max_result_bytes` and `max_execution_time` for the reference deployment come from the M0 measure of rows
and bytes for each query. They go into the configuration of the cutover (17.6).

### 19.14 The memory allocator of the musl binary

The binary uses the allocator of musl. Under many threads, this allocator can be slow. The engineer measures the overhead
in the M2 criterion of 18.5. When the measure fails because of the allocator, another allocator is a new dependency, and it
follows 18.14.

### 19.15 The 88 cell types that the measure of 1.3 does not classify

The measure of 1.3 classifies 73 of the 161 cell-type overrides (auto 64, color-background 9). M0 classifies the other 88. Each
cell type in use becomes a value of the property set of 4.18, or an `unsupported` row of 15.6 with an accepted fallback.
Milestone F, wave 1, adds the cell types `gauge` and `json` (decision D4 of ID-87). M0 classifies the others.

### 19.16 The complete unit list

The unit list of 4.20 holds the units that the measure of 1.3 names. M0 reads the unit of each field default and of each override.
A new version of section 4 adds each unit in use. A unit that stays outside the list is `unsupported` in 15.6.
Milestone F, wave 1, adds 21 units (decision D5 of ID-87). M0 closes the list.

### 19.17 The panel options that no real panel uses

The option sets of 4.26 to 4.33 hold the options of the design papers. M0 reads the options of each real panel. A new
version of section 4 removes each option that no real panel uses, so the closed schema stays small.

### 19.18 Regex flags and natural sort modes of the variables

Grafana accepts a variable regex with flags (`/x/i`) and the natural sort modes. M0 counts their use in the 21 real
variables. The owner then decides, by 18.14, which flags and modes format 2 keeps (4.8, 4.24). A flag or a mode outside
format 2 is `unsupported` in 15.6.

### 19.19 The real use of `displayName` and `hidden`

Section 4 maps the field properties `displayName` and `custom.hidden`. M0 counts their real use. A property that no real
panel uses leaves the property set of 4.18 in a new version of section 4.

### 19.20 The gaps of the chart engine

The copied chart engine has these gaps (9.24):

- no UTC time axis: a dashboard with `timezone = "utc"` needs it;
- no horizontal bars in the engine: this is not a gap of Hune, because the module `barchart_h.js` draws them (9.19);
- milestone F, wave 1, closes the UTC axis and the threshold lines with marked edits (9.24, edits 7 and 8). Wave 2 of milestone F closes three gaps with the edits 13 to 15 of 9.24. They are the right y axis, the style of each series and the title of the right axis;
- no legend placement `right`: the legend is always under the plot;
- no legend mode for each panel: the list or table mode is a stored preference of the viewer;
- the tooltip modes `single`, multi and `none`: the `hidden` of format 2 maps to `none`.

### 19.20a Differences of milestone F, wave 2

Milestone F, wave 2, records these differences from Grafana. A chart has two y scales at most, so a third unit shares the right scale (9.19, decision D17). In a `timeseries`, a `barchart`, a `piechart` and an `xychart`, a continuous colour mode follows the rule of the mode `thresholds`. A time zone name that the browser does not know gives the zone `browser` and a notice. The time shift of a panel has no round to a unit, so `1d/d` is `unsupported`.

The checks on Grafana of 9.8b, 9.16 and 9.19 are done (paper, 10.8). They add these differences:

- A `left` series and an `auto` series of another unit share the left scale. Grafana draws two left axes.
- A `hidden` series and a series that sets its own `min`, `max` or soft ends share the scale of their unit. Grafana gives each set its own scale and its own axis.
- `now/w` starts on Monday (9.9). Grafana starts on Sunday by default, and follows the member `weekStart` of the dashboard.
- A `time_from` of `now/s`, `now/m` or `now/h` is valid. Grafana gives the note `invalid time override` and uses the range of the dashboard.
- The note of a shift writes the unit `M` as it is. Grafana writes `m`.
- A local time that occurs two times has the earlier instant (9.9). Grafana gives the later instant (decision ID-F2-V8).
- A dashboard link keeps 50 items at most, and a dropdown with no item shows no button. Grafana has no cap of 50 and shows the button.
- In the light theme, a named colour on a mark of a chart can differ from the colour of Grafana by a few levels (9.16). For example, `dark-orange` is `#DD5100` and Grafana gives `#E55400`.
- A dashboard link adds no `timezone` to the URL, puts the added names before a fragment, and breaks a tie of titles by uid. Grafana adds `timezone`, puts them after the fragment and breaks a tie by the order of creation.
- The series colours are a wanted difference (decision ID-K70, owner request of 2026-11-12). A series with no explicit colour draws the 18 colours of ClickHouse Dash (9.16), in the light and the dark theme. The mode `palette` does the same, and it is the one series palette (decision ID-K74, owner decision of 2026-11-13). Grafana draws its classic palette of 64 colours.
- The importer maps each Grafana palette mode to `palette`. A Grafana `palette-classic` gives `palette` with no item. The modes `palette-classic-by-name` and `palette-saturated` give a `changed` item. A named colour, a threshold step, a value mapping, an override with a fixed colour and a continuous mode keep their colour.
- A series has the slot of its place, as in ClickHouse Dash. Grafana can colour a series by its name (`palette-classic-by-name`): Hune cannot, and the importer lists it. After the 18th series, the colours repeat. Grafana has 64 colours.

The measure of the y ticks of a time series on 2026-11-16 (decision ID-K311) found four differences of the y range. The owner keeps the canvas of Hune for ID-K311a (decision ID-K315). For the other three, Hune follows Grafana since decision ID-K316 (9.19). The tick rule itself is the rule of Grafana (9.19).

- ID-K311a. A panel of height 3 has a plot of 19 px in Grafana, in a canvas of 48 px. Hune keeps a canvas of 60 px and a plot of 40 px at least, on purpose. The ticks then differ.
- ID-K311b. Hune follows Grafana. A time series with `draw = bars` uses the automatic range. For data from -50 to 80, both show -70 to 100. Bars do not start at 0 when the data stays far from 0: for data from 1,000 to 1,010, both show 999 to 1,011.
- ID-K311c. Hune follows Grafana. A stack (`stacking = normal`) uses the automatic range of the tops of the stack, with the tick rule of 9.19. For the same data, both show 0 to 160. The tops 1,000 to 2,017 give 800 to 2,200. A stack with `percent` shows 0 % to 100 %, with labels such as "20%".
- ID-K311d. Hune follows Grafana. With `axisSoftMax = 200` and data from 5 to 95, the axis ends at the soft end, 200. A soft end inside the data has no effect on its side, and then the room can cross 0. For example, `axisSoftMin = 10` gives -4 to 104 (9.19, y axes, rule 7).

The second measure of 2026-11-16 (100 panels, decision ID-K316) found two more facts:

- In Grafana, a hard end with no soft end on its side is also the data end of that side. Thus `min = 0` with data from 5 to 95 gives 0 to 105. Hune follows it (9.19, y axes, rule 6).
- Hune follows Grafana since decision ID-K321 (owner answer of 2026-11-21). A series with more values below 0 than above 0 stacks down from 0, in a group of its own (9.19, stacks). The measure of 2026-11-21 found the same ticks on 100 panels, and the same lines within 1 px.

These differences of the stacks stay:

- On a log scale, Hune keeps the rule of the engine (9.19, log scale), and it does not stack. Grafana stacks there, ignores the soft ends and writes the labels of a percent stack with `%`.
- Grafana also puts two lines with another `lineInterpolation` in two groups. Hune has the interpolation of the whole panel only (4.28), and the importer removes a `lineInterpolation` of an override. Thus these lines stack in one group in Hune. The measure of 2026-11-22 found 3 such panels of 24.

Hune follows Grafana since decision ID-K325 (owner answer of 2026-11-22, point 3) for these items. The measure of 2026-11-22 used temporary dashboards in the folder `hune-parity` of Grafana 12.4.2, and the worker deleted them.

- A series of another `draw` stacks in a group of its own (9.19, stacks, rule 7). In Grafana the key of a group holds the draw style, and the bar alignment or the line interpolation (`getStackingGroups`). Before, Hune stacked a line and bars of one chart together: up to 318 px apart on 21 of 24 panels.
- The vertical `barchart` stacks with the rules of the time series. A series goes in the group of the sign of most of its values, and each value adds with its sign. Before, a value below 0 stacked down from 0 alone: up to 153 px apart.
- The value axis of the `barchart` is the automatic axis of Grafana with the soft end 0 (9.19). Before, it had 10 % of the range as room, and a whole range of room with value labels: up to 186 px apart. Grafana gives no room for the value labels. A percent stack shows shares of 1 in the unit of the field.

These differences of the `barchart` stay. Each one is open for the owner (to re-read):

- A stack shows no value. Grafana 12.4.2 shows the value of each part over its top when it fits (`showValue` = `auto`). For example, it shows 14 values on a stack of 8 bars on a plot of 237 px.
- The horizontal `barchart` is Hune code (9.19), and it keeps its rules. A value below 0 stacks down from 0, the axis has 10 % of room, and a percent stack has the axis 0 % to 100 %. Grafana 12.4.2 draws the stack of the vertical chart there. For the series 10 to 59 and -25 to 40, it shows the axis -20 to 100 with the step 20. For a percent stack, it shows shares from 0 to 1.
- The band under the plot of a vertical bar chart is 21 px, and the labels are 9 px under the plot. In Grafana 12.4.2 the band is 31 px. It holds the gap of 15 px, a tick of 4 px that does not show, and the text of 12 px. Thus its plot is 10 px lower, and its labels are 15 px under the plot. On 3 of 45 panels, the other height gives another tick step. A change needs a measure of the turned labels too (E25).

The gradient modes and the cosmetic series options of wave H (items P10 and P14, decision ID-K326) follow Grafana 12.4.2 (9.19). The measure of 2026-11-22 found these differences, and they stay:

- The mode `scheme` with the colour mode `palette` spreads the 18 colours of the series palette (decision ID-K70). Grafana spreads its classic palette of 64 colours.
- A horizontal `barchart` (Hune code) paints the fill of a bar with the gradient, and the line of the bar with the plain colour. Grafana also paints the line with the gradient of `hue` and `scheme`.
- A log scale of base 2 with no value above 0 has the range 1 to 2. Grafana draws no axis there. A value of 0 or below has no point, where Grafana draws its line out of the plot.
- The symmetric log scale of Grafana (`symlog`) is not in wave H. The importer keeps it `unsupported` (15.6).

M0 finds which gaps the real panels use (18.3). Each gap in use is a change in ClickHouse Dash first, then a new copy with
a new lock (9.24). The effort of M3b counts the changes (18.11).

### 19.20b Differences of milestone F, wave 3: the repeat

Milestone F, wave 3, adds the repeat of a panel and of a section (4.13a, 9.8c, decisions ID-K80 to ID-K97). The measure is on Grafana 12.4.2. These differences are on purpose.

- The order of the copies is the order of the options of the variable. Grafana uses the order of the selection: the URL `var-n=3&var-n=1&var-n=2` gives 3, 1, 2 in Grafana. With "All", both use the order of the options.
- A selection with no value shows a message in Hune. Grafana shows the panel one time (the panel keeps its place and runs its query), and hides a repeated row with its panels.
- Hune has limits: by default 24 copies of a panel, 12 copies of a section and 120 copies in a dashboard. The three keys `server.repeat_max_per_panel`, `server.repeat_max_per_section` and `server.repeat_max_per_dashboard` set them (13.8), up to 200, 50 and 200. Above a limit nothing repeats, and the page shows a message. Grafana has no limit. A test on Grafana made 200 copies.
- Hune has one level of repeat. Grafana multiplies a repeated panel in a repeated row: 2 rows of 3 copies gave 6 panels, and a panel that repeats by the variable of its row gave one copy for each row. The importer removes the repeat of the inner panel.
- A panel with direction `h` needs its own rows in Hune. Grafana moves the other panels. The result in Grafana depends on the order of the panels. The importer removes the repeat when another panel shares a row unit, and lists it as `unsupported`.
- Hune needs a `multi` variable for a repeat. Grafana accepts a variable with one value: it makes one copy, and for direction `h` this copy has the full width. The importer refuses such a repeat and lists it as `unsupported`: it never changes the variable (decision ID-K94).
- Hune makes no copy from a value that is not an option. This is a value of the URL, or a typed value of an open `custom` variable (ID-K203). Grafana repeats on any value of the selection. The member `all_value` has no effect on a repeat in both. In Grafana, a copy gets the option value, not the value of "All".
- Hune writes `<id>` for the first copy and `<id>-r<n>` for the copy n. Hune reads `<id>`, `panel-<id>`, `<id>-r<n>`, `panel-<id>-r<n>` and the form `<x>$panel-<id>` of Grafana. Hune never reads `panel-<id>-clone-<n>`, which is a key of the page of Grafana and not a form of the URL.
- Grafana writes `<x>$panel-<id>` for the view of a copy. The text `<x>` is the value of the copy, not its index. The parity captures of 2026-11-15 on Grafana 12.4.2 measured these facts (ID-K92):
  - With the text values `a` to `e`, the item "View" writes `a$panel-1`, `b$panel-1` and so on to `e$panel-1`. The first copy has the prefix too. A repeated section gives `a$panel-2`, `b$panel-2` and so on.
  - With the values `10`, `20` and `30`, the item "View" writes `10$panel-1`, `20$panel-1` and `30$panel-1`.
  - A load of `b$panel-1` or of `20$panel-1` shows the correct copy. With the values `10`, `20` and `30`, `0$panel-1`, `1$panel-1` and `2$panel-1` show nothing.
  - The match is exact on the value. `Bravo$panel-1` (the option text), `B$panel-1` (another case) and `z$panel-1` show nothing. A value that is not selected shows nothing: `var-host=10` with `20$panel-1`.
- Hune reads `<x>$panel-<id>` in the same way (10.2a, rule 1). Where Grafana shows an empty view, Hune removes `viewPanel` with a notice and shows the dashboard (rule 4). While the values load, Hune keeps the URL and waits (rule 16). Hune writes `<id>` or `<id>-r<n>`, never the form of Grafana.
- Grafana does not read `panel-<id>-clone-<n>`. It does not read `panel-<id>` for a panel that repeats.
- A panel with a repeat and no direction has a layout of its own in Grafana: the copies share the width of the panel. The importer gives direction `h` and lists the change.
- The header of a copy of a repeated section (decision ID-K310). In Grafana 12.4.2 the copies 2 to n of a repeated row have a title of 14 px at weight 400. Their header is 27.25 px. The first copy and the other rows have 16 px at weight 500 and 30 px. Hune treats this as a fault of Grafana. Each section heading of Hune has one look, and each copy has the header of 30 px of 9.8.
  - Thus a copy of Hune is 2.75 px lower for each copy 2 to n above it, its own header included. A panel under the section is 2.75 px lower for each copy 2 to n.
  - The visual capture of 15.14c documents a box only when its offset is this sum, within 0.5 px (rule `copy-header`). Any other offset stays a fault.

### 19.20c Differences of milestone F, wave 3: the text panel

Milestone F, wave 3, adds the text panel (4.33a, 9.19b, decisions ID-K100 to ID-K118). The measure is on Grafana 12.4.2, on the local instance. The worker made four temporary dashboards in a folder `hune-f29` and deleted them. These differences are on purpose, except the last two.

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

### 19.20d Differences of milestone F, wave 3: the state timeline and the status history

Milestone F, wave 3, adds the two panels (4.33b, 9.19c, decisions ID-K120 to ID-K136). The measure is on Grafana 12.4.2, on the local instance, with the data source `clickhouse` and synthetic queries. The worker made 8 temporary dashboards in the folder `hune-f30` (the uids `hune-f30-a`, `-b`, `-c`, `-e`, `-f`, `-g`, `-h` and `-i`) and deleted them. The worker read the options and the draw rules in the plugin code that the instance serves, and checked each rule on a picture. These differences are on purpose.

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

- Hune merges by state (the text and the colour), not by the raw value. A range mapping on many numbers gives one segment for a run of one state (decision ID-K124).
- Hune applies a mapping before the step state of the colour mode `thresholds` (decision ID-K123).
- The ink has 4.5:1 with the fill (9.26). Grafana takes white at 3:1. Hune writes black on `green` where Grafana writes white.
- Hune has a cap of 10,000 segments, and it draws nothing above it. Grafana draws 100,000. The status history keeps the Grafana rule of half the width (decision ID-K126).
- Hune has no `perPage`, no `insertNulls` and no time threshold for `spanNulls`. It has no pinned tooltip, no data link on a box, no annotation, no sort or hide-zero of the tooltip and no table legend.
- A row name shows only when the pitch of the rows is 12 px or more. Grafana draws names that overlap.
- `show_labels` equal to `false` hides the names and keeps the time axis. Grafana hides the time axis too, because the setting is in the defaults.
- A bool with no mapping takes the first colour of a continuous mode. Grafana colours true as 1 and false as 0.
- A status history with one sample gives the error "A status history needs two samples at least." Grafana draws nothing.
- Two time columns: Hune uses the first. Grafana draws a broken picture.
- The text is cut at a whole character and never in a pair of surrogates.
- The Hune tooltip, the legend and the colours use the tokens and the shades of the theme (9.25). The sizes and the positions are those that this section lists.

### 19.20e Differences of milestone F, wave 3: the annotations

Milestone F, wave 3, adds the annotations from a query (4.5b, 6.21a, 7.37, 9.19d, decisions ID-K140 to ID-K157). The measure is on Grafana 12.4.2, on the local instance, with the plugin ClickHouse 4.5.1 (`annotations` true in its `plugin.json`) and synthetic queries. The worker made 11 temporary dashboards in the folder `hune-f25` (the uids `hune-f25-a` to `hune-f25-k`) and deleted the dashboards and the folder. The plugin has no annotation editor of its own, so the rules of the columns are those of Grafana.

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

- Hune runs the saved query on a ClickHouse, a PostgreSQL or a MongoDB source (decisions ID-K142 and ID-K333). Grafana runs it with the plugin of the data source. The query is in the document and not in a data source setting.
- Hune reads each number in a time column as epoch milliseconds, for each kind of source (7.37, rule 4). A query that gives Unix seconds gives marks in January 1970 in Hune. The result of Grafana for such a number is not measured for the PostgreSQL and MongoDB plugins.
- Hune reads the roles `time`, `timeend`, `title`, `text` and `tags` and ignores the others. It has no role `id`, no `mappings` and no `color` column. It has no fallback to the first time field when a column is named `time`. It has no fallback of `text` to the first text column.
- A `NULL` or 0 in `timeEnd` gives a point, also in a Nullable column. Grafana loses the region of such a column. An end below the time gives an error in Hune. Grafana draws a region.
- The text is plain text. Grafana renders HTML. A text with the form of an HTML tag gives the error `annotation_invalid` with the reason `bad_chars`, and no mark shows. A control character and a bidirectional control give the same error.
- The tags are trimmed, the empty tags are removed and the first of equal tags stays. An `Array(String)` is read from its JSON text.
- Hune has caps: 8 annotations, 1,000 rows for each query (an error above it) and 2,000 marks for each panel (no mark above it). Grafana has none.
- The marks show on the `timeseries` only. Grafana also draws them on the state timeline and on the status history (decision ID-K148).
- The marks are on a canvas of the page, over the series. The line and the triangle have the measured sizes. The hit zone is 24 px high and 8 px wide on each side, and Grafana has the triangle of 10 px by 5 px. The card appears for a point and for a region in the zone.
- The card has no "Edit" and no "Delete", because Hune has no stored annotation. The tags have one neutral colour. The text has a clamp of 12 lines. The card has no pin.
- The keyboard reads the marks. Grafana has no keyboard access. "View data" has a table of the marks.
- The SQL of an annotation cannot use the macros that need the interval. Grafana can. A query that has one is not imported.
- A failed annotation shows a note in the title bar of the panel, and the cap shows a note. Grafana shows nothing.
- The page has no switch for an annotation. `enabled` is a member of the document (decision ID-K157).
- The built-in item "Annotations & Alerts" is not imported. Hune has no alert and no stored annotation.
- The import of `filter` with `exclude` equal to `true` is a list of panel ids at the time of the import.
- The card, the lines and the colours use the tokens and the shades of the theme (9.25). The sizes of the marks are those that this section lists.

### 19.20f Differences of milestone F, wave 3: several queries

Milestone F, wave 3, adds several queries in one panel (4.15a, 9.8d, 9.22a, decisions ID-K160 to ID-K175). The measure is on Grafana 12.4.2 with the plugin ClickHouse. The worker made 4 temporary dashboards in the folder `hune-f24` and deleted them. These differences are on purpose.

Facts of Grafana that Hune follows:

- A panel sends one request `/api/ds/query` with all its visible targets, one request for each data source. The panel members `maxDataPoints`, `interval`, `timeFrom` and `timeShift` apply to all the targets. The request had one `from` and one `to`. It had the same `intervalMs` and `maxDataPoints` for each target.
- A hidden target is not sent. The panel with all targets hidden sends no request and shows "No data". The backend ignores the flag: a direct request for a hidden target runs it.
- The order of the series is the order of the targets. The refs `Z`, `A` and `M` gave `Z c`, `A a` and `M b`. The name of a series is the ref and the field name (`A a`) when two frames or more answer. It is the field name when one frame answers. A long frame with a label gives `A v h0`.
- A time series, a state timeline and a status history join the frames on time. A stat, a bar gauge, a gauge and a pie chart show one item for each numeric field of each frame, with no join. A table shows one frame and a select at the bottom. The headers show `A n`.
- A failed target gives the panel a red badge, whose tooltip holds the message of the plugin. The other frames draw. The matcher `byFrameRefID` matches the fields of one target.

Differences:

- Hune sends one request for each visible query. Grafana sends one for each panel and data source. A dashboard of 10 panels with 19 visible targets made 10 requests in Grafana and makes 19 in Hune. The reason is decision D43 and the bound of 8.8: a request runs one query.
- The server of Hune refuses a hidden query with `400`. The backend of Grafana runs it.
- Hune keeps the prefix `A a` when a query fails, because the rule counts the visible queries. Grafana drops the prefix of a time series when only one frame answers (`b`), and it keeps `A n` in a table. Hune never changes a name because of an outage, so an override by name stays valid.
- When all the queries of a panel fail, Hune shows the error state with the first error and "Retry". Grafana shows "No data" with the badge.
- A partial result has no "Retry" in Hune. The failed queries run at the next trigger. Grafana has the same behaviour with its badge.
- Hune lists each failed query in the badge, with its ref, its code and its message. Grafana shows one message.
- A bar chart shows the first frame only in Grafana (measured: the legend had `A a` and no `B b`). Hune refuses extra queries for a `barchart` and an `xychart`, and the importer keeps the first query. The `xychart` of Grafana did not load in the measure.
- The join needs a sorted time. Hune sorts each frame and drops the rows with a `null` time, as its long-to-wide does. Equal times in one frame give the last row. A frame with no time column is an error in Hune.
- A transform applies to each frame alone, as in Grafana for the transforms of 9.21. The transforms that read more than one frame (`merge`, `join`, `concatenate`, `series to columns`) stay `unsupported` until F28.
- A panel of Hune has 8 queries at most, and a document 400. Grafana has no limit.
- A query of Hune has its own source. A query of Grafana in a Mixed panel can use the data source `-- Dashboard --` or `-- Grafana --`: the importer removes it and lists it. A panel with `repeat` and several queries is refused in Hune. The importer removes the repeat and keeps the queries (decision ID-K165).
- A panel members group (`maxDataPoints`, `interval`, `timeFrom`, `timeShift`) lives in `data.query` in Hune. The first query can be hidden and the members still apply.
- The importer writes no `ref` for a panel with one query, whatever its refId. Thus a document with one query per panel keeps its bytes (4.42). A refId that breaks the grammar, or that two targets share, gets a new letter and the item is `changed`. A matcher `byFrameRefID` follows the new letter.
- A section with `repeat` that holds a panel with several queries loses the `repeat` of the section in the import. A `barchart` or an `xychart` that reuses a panel with several queries is removed.
- The capture of the parity harness pairs the requests by ref. The visual capture reads the legend, the headers of a table and the select of the frames. It compares them with the display names of Hune (15.14c).

### 19.20g Differences of milestone F, wave 3: the heatmap and the histogram

Milestone F, wave 3, adds the two panels (4.33c, 4.33d, 9.19e, 9.19f, decisions ID-K180 to ID-K195). The measure is on Grafana 12.4.2, on the local instance, with the data source `clickhouse` and synthetic queries. The worker made 8 temporary dashboards in the folder `hune-f31` (the uids `hune-f31-a` to `hune-f31-h`) and deleted them. The worker read the options and the bucket maths in the plugin code that the instance serves, and checked each rule on a picture. These differences are on purpose. Two of them are faults of Grafana that Hune does not copy: one bucket and a bucket offset (decision ID-K310).

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
- Facts of Grafana that Hune follows (9.19f, section 3, decision ID-K310):
  - The y axis starts at 0. Its end is the end of the range rule of a time series for the lowest count above 0 and the highest count. The room is 10 % of the range of the counts, rounded up to a tenth of its decade. One bucket gives an axis from 0 to 2 times the count.
  - The ticks are 30 px apart at least, or 15 px on a plot of 150 px or less. The code that the instance serves has this rule for the y axis (`calculateSpace`). The rule gave the ticks of Grafana on the 15 panels of the fixtures `histogram_f` and `histogram_h`. It also gave the ticks on 28 plots of 19 to 481 px (temporary dashboards of 2026-11-15) and on the fixture `parity_axes`.
- The card has the row `Bucket` with `low - high` and the rows of the series. A result of 100,000 rows draws in 1.1 s. A result of 500,000 rows shows "No histogram found in response", and a result of 200,000 rows shows no heatmap.

The differences:

- A cell with a count of 0 or less is not drawn, and `hide_le` has the default 0 (Grafana: 1e-9). A count between 0 and 1e-9 shows in Hune and not in Grafana (decision ID-K184).
- Hune skips a `null` and a `NaN` value. Grafana counts a `null` as 0 on a linear scale and a `NaN` breaks the grid. Hune ignores a value with a time outside the range.
- A value of 0 or less on a log scale is skipped in Hune, as in Grafana. Hune shows a note that gives the count of the skipped values, and the table "View data" shows it too. Grafana skips them with no message (decision ID-K183).
- Hune has caps (500,000 values, 250,000 cells, 4,000 buckets). Grafana has none, but it shows no data above about 100,000 rows with this data source (decision ID-K182).
- The frame that is already bucketed has the shape of a time field and number fields only. Grafana also reads cells with the names `xMin`, `yMin`, `count` and the layouts `le` and `ge`. The labels stay at the middle of the row. A `yBucketScale` and a unit of the names are not read.
- The value axis has the places `left` and `hidden`. Hune has no right axis, no `axisWidth` and no `axisLabel` for the heatmap.
- Hune has 10 schemes of the 38 of Grafana. The scheme `viridis` is a sample of 17 colours with a difference of 7 or less in one channel. A scheme that Hune has not is `changed` on import (decision ID-K181).
- The card of a heatmap has no histogram of the column, no colour bar and no exemplar. The plot has no selection by drag and no card that stays after a click (decision ID-K187).
- The labels of a log axis have the short form of the engine (`0.001`). Grafana writes `0.00100` and `0.100`.
- A cell of 3 px or less has no gap (E16). The sizes are not rounded for a gap of 4 px or more. A cell is 0.5 px at least, and Grafana has 1 device pixel.
- A bar of a histogram is its slot less 1 px (the engine), so two bars have a gap of 1 px. The bars of Grafana touch.
- A frame with one time row gives an error in Hune ("A heatmap needs two time rows at least."). Grafana draws nothing.
- The cells are colour data. The lowest colour of a scheme can have less than 3:1 with the panel in the light theme, as in Grafana (decision ID-K188).
- The histogram has no gradient mode. Hune has no `noValue` for the values.
- One bucket: all the values equal, one value, or `bucketCount` 1. Grafana draws no bar and no x label, because its canvas gets a rectangle with an x of `NaN`. This is a fault of Grafana. Hune draws the bar with its two edges and their labels (decision ID-K310). The y axis of both ends at 2 times the count. The visual capture keys this difference on the rectangle of Grafana with no x (rule `one-bucket`).
- A `bucketOffset` that is not a multiple of `bucketSize`. The x axis of Hune runs from the low edge of the first bucket. Its labels are the edges of the buckets (9.19f, section 3). Grafana cuts a part of the first bucket, and its labels mark no bar edge. This is a fault of Grafana (decision ID-K310). The visual capture pairs the bars by their x value, so their heights and their colours still compare (rule `bucket-offset`).
- The bars are drawn by the engine. The number of buckets is the number of the span, empty buckets included. Grafana draws only the buckets that have a count, and it has no cap.
- Hune follows Grafana for the y axis of a stack since decision ID-K325 (owner answer of 2026-11-22). With `percent`, the axis shows shares of 1, for example 0 to 1.1. In a stack, the axis uses the lowest drawn top. It is 0 when only a later series has a count in a bucket (9.19f, section 3). Before, the percent axis was 0 to 100, and a stack could end 10 counts lower. The marks were up to 14 px apart on 12 of 24 panels.
- The old heatmap of Grafana 7 to 9 is not imported (decision ID-K186).
- The Hune card, the legend and the colours use the tokens and the shades of the theme (9.25). The sizes and the positions are those that this section lists.

### 19.20h Differences of milestone F, wave 3: custom values of a variable

The figures are from Grafana 12.4.2, with 4 temporary dashboards (`hune-f41-a` to `hune-f41-d`, in the folder `hune-f41`). The worker deleted them. The code of the front end that the instance serves is the second source: the files `8060.*.js` and `4652.*.js` of the build.

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

The differences that Hune keeps (decision ID-K212):

- An absent member means closed in Hune and open in Grafana. The importer writes `true` for a `query` variable when Grafana says `true` or says nothing, so an imported dashboard behaves as in Grafana (15.6). For a `custom` variable it writes `true` only when Grafana says `true` and a free value is valid in each query that names the variable. An absent member of a `custom` variable stays closed, with a `changed` item, because a closed `custom` variable is a check of the server (ID-K206). The editor creates a new variable closed.
- A closed variable of Hune replaces a URL value at the load, with a notice. Grafana keeps it at the first load and shows no notice.
- An open variable of Hune keeps a typed value after each reload of the options. Grafana loses it. This is the value of the feature for a variable that reloads with the time range.
- A closed `custom` variable is a rule of the server in Hune (7.7). A closed `query` variable is a rule of the page, as in Grafana. Grafana has no rule on the server.
- An empty list gives "No options" and no value in Hune. Grafana gives an empty value in the URL and a blank control.
- The row of the typed value reads `Use "<text>"`, and a selected typed value is a checked row of the list. Grafana shows "Hit enter to add" and a chip in a `multi` variable.
- A typed value makes no copy of a repeat in Hune (4.13a, 9.8c). Grafana repeats on any value of the selection.
- The editor of Hune has a menu of SQL examples and a preview of the options. Grafana has neither.

### 19.20i Differences of F42: the minimal tree of the query editor

F42 adds a format on request, folds, a bracket pair and a current block to the SQL area (10.3c to 10.3g, decisions ID-K220 to ID-K233). Grafana 12.4.2 edits SQL in Monaco. Hune keeps its text area with two layers.

Facts of Grafana that Hune does not copy:

- Monaco is a code editor library with a full editor model. It has several cursors, a minimap, find and replace, and folds by indent.
- Monaco folds by the indent of the lines and by the brackets. It marks the bracket pair and has bracket colours.
- Monaco has code completion with names of tables and columns.

Differences:

- Hune has no multi-cursor, no minimap, no find and replace and no completion of names (10.16). The reason is the size and the dependency: Monaco is a large library, and the owner asked for a minimal tree that Hune writes (decision ID-K220).
- The folds of Hune follow the brackets of the tree of the dialect, not the indent. A fold hides a pair of brackets of two lines or more.
- The format of Hune changes only blanks and never the case of a keyword (decision ID-K224). A formatter library of Grafana can write the keywords in upper case.
- The format of Hune refuses a text with an error and says where. A formatter library often formats a partial text.
- The fold state is not saved. Monaco keeps it in a view state that Grafana does not save in the dashboard either.
- The tree is not a security check and has no effect on the save or the run. Grafana has no such check in the editor either, and its server is the authority, as the server of Hune is.
- The structure of a `postgres` source follows the lexer of PostgreSQL, but the colours still follow the ClickHouse lexer (10.3). This is the known limit of 10.3.
- The tabs "JSON" of the panel and of the dashboard have no tree (decision ID-K233).

### 19.20j Differences of milestone F, wave 3: more transforms

Milestone F, wave 3, adds eleven transforms (4.25a, 9.21a, decisions ID-K240 to ID-K254). The measure is on Grafana 12.4.2 with the plugin ClickHouse. The worker made 243 temporary dashboards in the folder `hune-f2827`, read each result in "Inspect, JSON, Panel data", and deleted the folder at the end. The prototype matches Grafana on 177 of 237 cases. These are the differences, and the cases are in `tests/frontend/vectors/measure-f28.json`.

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
- The import keeps the first 16 transformations of a panel and lists the others as `unsupported` (15.6). Before milestone F, the dashboard was not imported in this case.

### 19.20k Differences of milestone F, wave 3: interval and constant variables

Milestone F, wave 3, adds the variable kinds `interval` and `constant` (4.10a, 4.10b, 7.4a, decisions ID-K260 to ID-K270). The measure is on Grafana 12.4.2. The worker made 4 temporary dashboards in the folder `hune-f2827` and deleted them at the end.

Facts of Grafana that Hune follows:

- The option "Auto" has the value `$__auto`. Its text in the SQL is a table text of 7.4. The interval is the range divided by the step count, rounded by the table, and not below the minimum. The texts are `7d` for one week, `30d` and `1y`.
- A constant is not read from the URL. It goes into the SQL as it is.
- The interval texts reach the SQL as texts. ClickHouse reads them with `parseTimeDelta` for the units `ms`, `s`, `m`, `h`, `d`, `w` and `y`.

Differences:

- The server of Hune checks the value of an `interval` against the list of the document. Grafana accepts a URL value that is not an option, and it puts the text in the SQL. This difference is the security choice of decision ID-K263.
- Hune checks each item of the list. An item is digits and one unit of `ms`, `s`, `m`, `h`, `d`, `w` or `y`, at most 365 days. Grafana does not check the items (`abc`, `5x`, `0m`, `1M` are options). The importer removes the items that Hune refuses.
- Grafana splits the list at each character that is not a letter, a digit or `_` (`1.5h` gives `1` and `5h`, `-1m` gives `1m`). Hune splits the editor text at a comma and at a line end. The importer follows Grafana for a file that it imports.
- The minimum of "Auto" is one of the 30 texts of the table. Grafana takes any text, and `90s` gave `1m`. The importer maps such a text to the largest table text that is not above it. A step count of 0 and a bad minimum together made the page of Grafana fail. Hune refuses them at save.
- The text of "Auto" in a title is `Auto` in Hune. Grafana shows the computed interval. The reason is that the page holds no table of intervals (9.30).
- The SQL text of a constant gets the string escapes of 7.8 in a string. Grafana puts it in as it is, so a quote mark in the value breaks the SQL there.
- A constant is always hidden in Hune. Grafana shows a constant with `hide` 0. The importer lists this change.
- The member `$__auto_interval_<name>` in a SQL is not a variable in either product (Grafana leaves it as text, Hune refuses `$__name` in code, 7.10).

### 19.20m Differences of F43: the option forms of the panel editor

The worker measured the editor of Grafana 12.4.2 with a headless Chrome. It opened 13 temporary dashboards in the folder `hune-f43`, one for each type, with the data source `clickhouse`. It read the groups, the labels and the controls, and it changed each choice to find the controls that show and hide. The dashboards and the folder were deleted after the measure.

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

The xychart panel of Grafana did not load in the editor for a panel that had no `options.series` member. The migration code of the panel threw an exception. The worker added `options.series = []` to its temporary dashboard to measure it.

### 19.20n Differences of P01: the units of wave H

The worker measured the value formatter of Grafana 12.4.2 on 2026-11-22. A headless Chrome loaded `@grafana/data` from the served page. It called `getValueFormat` and `getDisplayProcessor` for the 30 new units of 4.20, and for 15 units that Hune had already. The measure made no dashboard. The texts are the vectors `FE-06-units-h.json` of FE-06. The page gives these texts, with the differences below.

| Case | Grafana 12.4.2 | Hune |
|---|---|---|
| `Infinity` with a count unit (`rowsps`, `rowspm`, `recps`, `cps`, `wps`, `opm`, `reqpm`, `rpm`) | `∞undefined rows/s`: the name of the step is `undefined`. | `∞` with no suffix, as each scale of 9.15. |
| `Infinity` with `dthms` or `dtdhms` | `Infinity:0NaN:0NaN` | `∞` |
| A text value with a count unit, for example `abc` with `rowsps` | The formatter gives `abcundefined rows/s`. The display shows `abc`. | `abc`, as for each unit. |
| A text that holds a number, for example `12` with `rowsps` or `0` with `bool` | The display reads the number: `12 rows/s`, `False`. | The text as it is: `12`, `0`. This applies to each unit (9.15). |
| A `bool` value with `bool_yes_no` | `true` or `false` | `Yes` or `No`, the rule of 9.15 since milestone F (to re-read by the owner). |
| The colour of a number with the unit `string` | The colour of the base step, because the display reads no number. | The colour of the number by the thresholds. |
| The y axis of a time series with `kbytes`, `mbytes`, `gbytes`, `tbytes`, `bits` or `binBps` | The steps are powers of 2: the time series of Grafana has a list of binary units. | The steps are 1, 2, 2.5 or 5 times a power of ten. Only `bytes` has steps that are powers of 2 (9.19). |

The measure also changed two rules of units that Hune had already:

- `decbytes` is the SI scale of Grafana (`SIPrefix`). Thus `0.5` gives `500 mB`. Before, the page gave `0.500 B`.
- The three bool units give `NaN` for `NaN`, as the display of Grafana does. Before, `bool_yes_no` gave `No`.

### 19.21 Data-link tokens, table footer, pages and the other panel features of M0

M0 measures the tokens of the data links (9.18), the table footer and pages (9.20), panel time overrides, repeats, and
panel and dashboard links. Each feature in use is a `mapped` row of 15.6 by 18.14, or an `unsupported` row with an accepted
fallback. Milestone F, wave 1, adds the table footer (9.20). Hune v1 has no pages. Milestone F, wave 2, adds the panel time override (9.8b) and the dashboard links (9.8a).

Milestone F, wave 3, adds the repeat of a panel and of a section (4.13a, 9.8c). M0 counts the real use, to confirm the limits of 4.35 (13.8).

### 19.22 Unused

The checks of this item are a test list, not an open point. They moved to 15.14b.

### 19.23 The detection of a client close by hyper on HTTP/1.1

The cancellation of 8.9 needs the server to see that the browser closed its connection. The case C05 of 16.9 MUST prove it:
the query leaves `system.processes` within 2 s after the close, through an HTTP/1.1 proxy. When hyper does not detect the
close in time, the engineer changes the design of 8.9 before the M2 exit.

### 19.24 `ceiling_exceeded` for `max_rows` after a byte overflow

ClickHouse gives the error 396 (`TOO_MANY_ROWS_OR_BYTES`) for both the row limit and the byte limit. When the ceiling
`max_rows` lowered `max_result_rows`, Hune answers `ceiling_exceeded`, also when the cause was the byte limit (8.5, 11.24).
The owner accepts this message, or asks for a check of the row count before the answer.

### 19.25 No durable audit record for a folder change

A change of a dashboard writes a history version before the effect: the durable audit (14.11). A change of a folder writes
only the log line `folder.changed`. The owner decides if a folder change needs a durable record. A durable record changes
the store of section 5, so the decision comes before the volume writes of M4.

### 19.26 The three rules of `hclconf`

Closed. The crate `hclconf` is written in Hune from section 13 (decision ID-01). It is not a copy of another HCL engine of
the maintainer, and its `PROVENANCE` file records the comparison. It has three rules (13.1): a duration with exactly one
unit, modes on blocks, and boolean mode keys. No other engine gets these rules from this work.

### 19.27 The token rules of `procurauth-core`

`procurauth-core` accepts only whole numbers for `iat` and `exp`. It needs a `kid` when the key set has two keys or more.
Hune follows the library (11.8, 11.9). Issue 1 of the repository `SCcagg5/procurauth` holds the open question of the
protocol. The owner answers it as the maintainer of ProcurAuth, before the engineer chooses the commit of 19.28.

### 19.28 The pinned commit of ProcurAuth and the series U figures

The engineer chooses the `rev` of the git dependencies `procurauth-pep` and `procurauth-core` at M6 (11.10, 17.9). The
profile and the counts are those of 11.32. At the commit `058563cfbc3536b9b850255d953568ef0a5cabbe`, the run gives 110
cases: 61 passed, 0 failed and 49 skipped. At the exit of M6, `docs/CONFORMANCE.md` records the counts of the chosen
commit.

### 19.29 The pattern form of `byRegexp` and of `reduce.fields`

A review shows that Grafana most likely reads the `byRegexp` matcher and the `reduceOptions.fields` of the stat, pie and
bar gauge panels with `stringToJsRegex`. In that form a text `/^Total$/` is a pattern with slashes, and a plain text `t`
means `^t$`. 15.6 copies the Grafana text as it is (the Grafana form of 4.24), and the fixtures of I01 and B04 use
`/^Total$/`. The spike (18.3a) checks the rule in the source and the behaviour of the pinned Grafana. Sections 4 and 9
then use the form that the check confirms.

Owner: the engineer. Close by: the exit of the spike, before M1 freezes the format.

### 19.30 The common subset of the regular expressions

Hune checks a pattern with the Rust crate `regex` (4.24) and runs it with the `RegExp` of the browser. The two engines
differ: `\d`, `\w`, `\s` and `\b` are Unicode in Rust and ASCII in JavaScript, and `[[:alpha:]]`, `\A`, `\z` and `\pL`
are valid in Rust only. Section 4 defines one subset that both engines read the same way, and `hune-core` refuses a
pattern outside it. The vectors of 16.10a check it.

Owner: the engineer, with the writer of section 4. Close by: the exit of the spike.

### 19.31 The rule of `max_data_points`

Section 9.8 sets `max_data_points` from the inner width of the panel frame, as Grafana does, and 7.4 agrees. Only the
check is open. The spike (18.3a) measures the `maxDataPoints` of the pinned Grafana on real panels. It compares the value
with the rule of 9.8. A difference near a bound of the interval table (7.4) gives other buckets of `$__timeInterval`.

The case B27 and the check of 15.14b test the rule after the spike.

Owner: the engineer, with the writer of section 9. Close by: the exit of the spike.

### 19.32 The security choices of the PostgreSQL source

The design of section 8a has choices that the owner has not validated. Each choice has a default, and the code follows the default. The table below gives each choice. Each one is a decision marked `PENDING OWNER` (`ID-PG1` to `ID-PG17`). The owner answers each choice, or accepts its default, before the exit of MPG.

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
| The other choices | The types of 7.16a, the `null` for a numeric NaN, the rule of 7.16b, no fill of gaps, the interval units of 7.23, PostgreSQL 15 to 18, the first release 0.1.0 (ID-K335), and the scanner of the page. | 7.16a to 7.23, 8a.14, 9.10 |

Owner: the owner, with the engineer. Close by: the exit of MPG.

### 19.33 The security choices of the MongoDB source

The design of section 8b and of 7.27 to 7.36 has choices that the owner has not validated. Each choice has a default, and the code follows the default. The table below gives each choice. Each one is a decision marked `PENDING OWNER` (`ID-MG1` to `ID-MG16`). The owner answers each choice, or accepts its default, before the exit of MMG.

| Choice | Default | Where |
|---|---|---|
| The place of the query in the format | The member `sql` holds one JSON text. No new member, no new `format_version`. | 4.15 |
| The driver crate | The crate `mongodb` and the crate `bson` from crates.io, with `rustls` and `ring`, behind the feature `mongo-driver`. The crate `mongodb` is a local copy with no environment read in `vendor/mongodb` (`ID-MG25`). | 8b.2, 17.9 |
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
| The other choices | The versions 7.0 and newer, the first release 0.1.0 (ID-K335), the importer rows and the frame with no column. | 8b.14, 15.6, 7.16c |

Owner: the owner, with the engineer. Close by: the exit of MMG.

### 19.34 The query model of the MongoDB plugin, the two real queries, and the MongoDB version

The rows of 15.6 for the plugin `haohanyang-mongodb-datasource` 0.5.0 follow the memory of the author. The author did not check these items against the real plugin:

- the members `queryText`, `queryType`, `database` and `collection`;
- the form of the time macros;
- the form of the text.

The reference deployment has 2 MongoDB queries. M0 or the start of MMG reads them from the export of 15.3. It records, for each query, the members of the target, the stages that it uses, its macros and its variables. It also records the version and the topology of the MongoDB server that the reference deployment reads. It records the privileges of the account that Grafana uses too.

The result can change the rows of 15.6, the macros of 7.34 and the supported versions of 8b.14. Each such change is a new decision with an id, and the vectors of 16.13 follow it.

Owner: the engineer. Close by: the start of MMG.

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
| cutover | The move of the reference deployment from Grafana to Hune (15.15). |
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
| person-month | 20 business days of one person. |
| readiness | The state of `/readyz`. It fails only until the catalog has loaded one time (section 14). |
| reuse | A panel that shows the result of another panel, the Grafana "-- Dashboard --" source. |
| `save_id` | The UUID of one save attempt, sent as `Idempotency-Key`. |
| section | A row of a dashboard: an optional title and its panels. |
| series U | The conformance cases of ProcurAuth for a PEP. |
| source | One data source of the configuration: a `clickhouse "<id>"` or a `postgres "<id>"` block. |
| SQLSTATE | The five-character error code of PostgreSQL. Hune maps it to a code of section 6 (8a.8). |
| store | The place of the documents, behind the `Store` trait (section 5). |
| subject | The person or key of a request. In mode `open`, the subject is `-`. |
| T | The day of the nginx switch of the cutover (15.15). |
| tombstone | The history version that a delete writes. |
| uid | The id of a dashboard. It is also its file stem or its row key. |
| variable | A dashboard control whose values go into the SQL (section 7). |
| volume | The folder of dashboard files on a mounted file system (section 5). |
| write guard | The check of `Sec-Fetch-Site`, `Origin` and `Content-Type` on each write (section 12). |

### 20.2 References

| Id | Document | Place |
|---|---|---|
| PROTOCOL | ProcurAuth Protocol, version 1.0 | `SCcagg5/procurauth`, `protocol/procurauth-protocol.md` |
| CONFORMANCE-DATA | The conformance cases and the runner `access-conformance` | `SCcagg5/procurauth`, `conformance/` and `crates/access-conformance/` |
| PEP | The crates `procurauth-pep` (with the trait `PdpTransport` in `transport.rs`) and `procurauth-core` | `SCcagg5/procurauth`, `crates/` |
| FAKE-PDP | The test PDP | `SCcagg5/procurauth`, `crates/fake-pdp/` |
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

### 20.3 Measured use of the Grafana of the reference deployment

These facts come from the measure of 2026-11-09. Sections 15 and 18 use them.

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
