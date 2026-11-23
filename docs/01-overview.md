# Hune specification

Version 0.1 (draft for the owner), 2026-11-09. Status: all items are DECIDED except section 19.

This document uses ASD-STE100 Simplified Technical English.

The key words MUST, MUST NOT, SHOULD and MAY are as RFC 2119 and RFC 8174 define them. This applies when they are in capital letters.

The specification has eight files in `docs/`:

| File | Part | Sections |
|---|---|---|
| `01-overview.md` | Part 1: Overview | 1 Purpose and scope; 2 Decisions; 3 Architecture |
| `02-format-and-storage.md` | Part 2: Document format and storage | 4 Document format; 5 Storage |
| `03-api-query-sources.md` | Part 3: API, query engine and data sources | 6 HTTP API and error model; 7 Query engine; 8 Data sources |
| `04-frontend-editor.md` | Part 4: Front end and editor | 9 Front end; 10 Editor, history and conflicts |
| `05-access-security.md` | Part 5: Access and security | 11 Access and ProcurAuth; 12 Security |
| `06-configuration-operation.md` | Part 6: Configuration and operation | 13 Configuration; 14 Observability and operation |
| `07-migration-tests-delivery.md` | Part 7: Migration, tests and delivery | 15 Import and migration from Grafana; 16 Tests; 17 Delivery |
| `08-roadmap-open-items.md` | Part 8: Roadmap and open items | 18 Roadmap, milestones and effort; 19 Open items; 20 Glossary and references |

Rules of this document:

- A numbered paragraph `N.M` keeps its number in all later versions. A paragraph that leaves the document keeps its number with the word "Unused".
- A cross-reference gives a section number (for example "section 11") or a paragraph number (for example "5.14").
- Each item is DECIDED. The open items are in section 19 only.
- A code span holds an exact name: a key, a member, a route, a type or a command.

## 1. Purpose and scope

### 1.1 What Hune is

Hune is a dashboard product in Rust. It replaces the part of Grafana that the reference deployment uses: dashboards of ClickHouse SQL panels.

Hune is one static binary, `hune`. The binary serves a web page and an HTTP API. The web page shows dashboards and lets authors edit them. The binary also holds the command line tools: configuration checks, the Grafana importer, storage migration, export, import and restore.

The first data source is ClickHouse. PostgreSQL is the second data source (section 8a, milestone MPG). MongoDB is the third data source (section 8b, milestone MMG). The design lets Kafka come later as a data source, but Hune builds it not. ProcurAuth is optional: with `access.mode = "authzen"`, Hune is a ProcurAuth PEP.

### 1.2 The two use cases

Hune MUST support the two use cases below. One binary does both. The configuration block `storage` selects the use case.

| Use case | Storage | Instances | Typical place |
|---|---|---|---|
| LOCAL | `storage { backend = "volume" }`: one JSON file for each dashboard in a mounted folder | One writer process | A laptop, one server, a git checkout, the compose host of the reference deployment |
| DISTRIBUTED | `storage { backend = "postgres" }`: one row for each dashboard in PostgreSQL in high availability | Two or more instances | A Kubernetes cluster (for example K3s) with CloudNativePG |

Both backends store the same canonical bytes (4.38). The same `Store` trait and the same conformance suite apply to both (section 5). `hune copy-store` copies a store to the other kind in both directions (5.37).

### 1.3 What the reference deployment uses

A measure of the production Grafana on 2026-11-09, from its `grafana.db`, gives these numbers. They set the scope of v1.

| Item | Measure |
|---|---|
| Dashboards | 10 dashboards in 2 folders, plus 3 at the root; 161 panels; 7 rows. The two largest dashboards have 40 and 45 panels. |
| Panel types | table 73, timeseries 44 (41 lines, 3 bars), stat 30, barchart 9, piechart 2, xychart 2, bargauge 1 |
| Queries | About 142 raw ClickHouse SQL texts: median 1,171 characters, longest 71,119 characters. 96 table queries, 46 time series. |
| Panel reuse | 17 panels reuse the result of another panel ("-- Dashboard --") |
| Other sources | 2 MongoDB queries (1 percent) |
| Macros | `$__timeFilter` 95, `$__timeInterval` 42, `$__toTime` 29, `$__fromTime` 23, `$__interval_s` 19 |
| Variables | 21: 9 query variables (7 multi-value with "All" and a refresh on a time change), 10 text boxes, 2 custom lists. 306 uses in the SQL. |
| Overrides | 332 per-column overrides in 75 table panels: matcher byName 310, byRegexp 21 |
| Override properties | `custom.width` 239, cell type 161 (auto 64, color-background 9), align 132, decimals 113, unit 92, thresholds 70, min 56, color 39, mappings 36, max 26, data links 20 |
| Thresholds and units | Thresholds in 143 panels. Units: mostly custom suffixes, percent and ms. |
| Transformations | 56: filterByValue 36, groupingToMatrix 18, prepareTimeSeries 1, labelsToFields 1 |
| Editing | 138 saved versions for 10 dashboards |
| Identity | One shared identity behind a proxy that authenticates the users. 52 distinct browsers in about 2 months. |
| Not used | Alert rules (0), library panels, playlists, snapshots, teams, API keys, correlations, plugin settings, custom annotations (9 default definitions only) |
| Sharing | 1 public dashboard and 7 short links, all behind the nginx login |

The content of the 142 queries is not measured yet. Milestone M0 measures it (section 18). Section 19 keeps it as an open item.

### 1.4 Scope

The rule is: "minimise the work to what the reference deployment uses". A feature that the reference deployment does not use is out of scope. The table gives the reason for each line.

| Feature | Scope | Reason (measured use) | Section |
|---|---|---|---|
| Dashboards of ClickHouse SQL panels | In | 10 dashboards, 142 SQL texts | 4, 7, 8 |
| The 7 panel types, each with the options of the real panels | In | 161 panels of these 7 types | 4.27 to 4.33, 9 |
| An optional `placement` of a variable: its control in the grid of a section, with the top bar as the default | In | Owner request of 2026-11-12 | 4.7a, 9.10 |
| Variables: query, custom, text box | In | 21 variables, 306 uses | 4.7 to 4.11, 7 |
| An optional choice, for a `query` or a `custom` variable, between a closed value (an option) and a free value (a value that the person types), with a switch, SQL examples and a preview of the options in the editor | In | Owner request of 2026-11-14 | 4.8, 4.9, 7.7, 9.10, 10.3b |
| Two more variable kinds: interval (a closed list, with "Auto") and constant (a text of the document, not shown). The data source variable and the ad hoc variable stay out. | In | Owner work order of 2026-11-13 | 4.10a, 4.10b, 7.7, 7.4a |
| The 5 ClickHouse macros | In | 208 uses | 7 |
| Per-column overrides byName and byRegexp | In | 332 overrides | 4.17, 4.18, 9 |
| Thresholds, units, mappings, data links | In | 143 panels with thresholds; 36 mappings; 20 data links | 4.18 to 4.23 |
| Transformations filterByValue and groupingToMatrix | In | 54 uses | 4.25, 9 |
| Transformations prepareTimeSeries and labelsToFields | Mapped by the importer to built-in behaviour | 1 use each | 4.25, 15 |
| Eleven more transforms: organize, calculate field, sort by, limit, merge, reduce, partition by values, join by field, filter fields by name, rename by regex, convert field type | In | Owner work order of 2026-11-13 | 4.25, 9.21a, 15.6 |
| Reuse of the result of another panel | In | 17 panels | 4.15, 9 |
| Folders with one level | In | 2 folders and the root | 4.3, 5 |
| Editor, history, conflicts, restore | In | 138 saves | 10, 5 |
| Volume storage and PostgreSQL storage in HA | In | Owner requirement | 5 |
| ProcurAuth, optional | In | Owner requirement | 11 |
| Importer of Grafana dashboards | In | The 10 dashboards move to Hune | 15 |
| Alerting and contact points | Out | 0 alert rules; Alertmanager configuration empty | none |
| MongoDB as a data source | In (milestone MMG; in the release 0.1.0) | 2 queries (1 percent); the importer maps them from MMG (15.6) | 8b, 15 |
| Kafka as a data source | Out | Not used; the `DataSource` trait lets it come later | 8 |
| PostgreSQL as a data source | In (milestone MPG; in the release 0.1.0) | Owner requirement (milestone MPG) | 8a |
| Annotations from a query on a time series (saved query on a ClickHouse, PostgreSQL or MongoDB source, read only, with caps). The built-in annotations and the alerts of Grafana stay out. | In | Owner request of 2026-11-13: "annotations from a ClickHouse query". The reference deployment has 9 default definitions only. | 4.5b, 9.19d, 15 |
| Library panels, playlists, snapshots | Out | 0 uses | none |
| Organisations, teams, users and roles in the product | Out | One identity; ProcurAuth gives the subjects | 11 |
| API keys, service accounts, correlations | Out | 0 uses | none |
| Plugins | Out | Only the ClickHouse data source plugin is used | none |
| Public dashboards and short links | Out | 1 and 7, all behind the nginx login; the URL holds the full state | 15 |
| Explore and an ad hoc query page | Out | ClickHouse Dash covers this need | none |
| Logs panel: time, level, body, order, wrap and a details row (the ClickHouse logs format) | In | Owner decision ID-K326 of 2026-11-22 | 4.33f, 9.19h |
| Traces panel | Out | Owner decision ID-K333 of 2026-11-22 | none |
| Text panel in Markdown, with no HTML, no image and no query | In | Owner request of 2026-11-13 | 4.33a, 9.19b |
| State timeline and status history panels, with value mappings, a legend, a tooltip and a cap on the segments | In | Owner request of 2026-11-13 | 4.33b, 9.19c |
| Option forms in the panel editor for each type of panel with data, with a live preview and an audit of the effect of each control | In | Owner request of 2026-11-14 | 10.3h |
| Heatmap and histogram panels, with bucket maths in the page, a colour scale, a legend, a tooltip and caps on the buckets | In | Owner work order of 2026-11-13 | 4.33c, 4.33d, 9.19e, 9.19f |
| Panel and row repeat, by a variable, with limits of 24, 12 and 120 copies (default values of `server.repeat_max_per_*`) | In | Owner request of 2026-11-13 | 4.13a, 9.8c |
| Nested folders | Out | 2 folders, 3 dashboards at the root | none |
| A result cache shared between users | Out | Small load (3.17) | none |
| Internationalisation | Out | English only | 9 |
| WebAssembly kernels | Out until a measure shows a gain | ClickHouse Dash measures: the gain is in compute, not in canvas drawing | 9 |

### 1.5 Users

| Role | Who | What the role does |
|---|---|---|
| Viewer | Staff of the reference deployment behind the nginx login (52 browsers in 2 months) | Opens dashboards, selects the time range and the variable values, shares the URL |
| Author | Staff who change dashboards (138 saves) | Edits SQL, panels, layout and variables; reads the history; restores a version |
| Operator | The person who runs Hune | Writes the configuration, runs the server, the migrations, the backups and the restores |
| Migration owner | The owner of the cutover | Runs `hune import-grafana`, checks the report and the side-by-side comparison, does the cutover |

In mode `open`, every request has the same subject `-`, and `server.editing` decides if writes are possible. In mode `authzen`, the ProcurAuth subject decides each operation (section 11).

### 1.6 Goal: replace Grafana at the reference deployment

Hune replaces Grafana at the reference deployment after milestone M4, in volume storage and mode `open`, behind the current nginx. The cutover keeps Grafana read-only for 2 weeks, runs one final import, and switches nginx. Rollback is a switch back to Grafana. Grafana and its volume stay untouched for 30 days. Section 15 gives the procedure, and section 18 gives the milestones.

Version 1.0.0 comes after M7. It adds the PostgreSQL backend (M5), ProcurAuth (M6) and the final security and operation work (M7).

### 1.7 Rule to add a feature

A feature that this specification does not include needs these items:

1. A measured use at the reference deployment, or a written decision of the owner.
2. A new decision line in section 2 and new numbered paragraphs in the section that holds the rule.
3. Its configuration keys in the key table of section 13, with a closed schema.
4. Its tests and its ASD-STE100 documentation in the same change.
5. For a Grafana feature: a change of its importer class from "unsupported" to "mapped", with a fixture.
6. For a change of the document schema: a new `format_version` (4.42).
7. For a new data source: a new variant of the source enum and a new query kind in the format. It also needs a lexer dialect and a frame mapping table (section 8).

## 2. Decisions

### 2.1 How to read the table

Each line gives one decision. The column "From" names the question of the design record (Q1 to Q16) or the item `MISSING` of that record. The maintainer keeps the design record outside this repository. The value `Coordination` marks a decision of the coordinator that reconciled the parts. The column "Section" names the section or the paragraph that holds the full rule. A decision line does not replace its rule.

| # | Decision | From | Section |
|---|---|---|---|
| D1 | Name Hune: binary `hune`, metric prefix `hune_`, repository `SCcagg5/hune`. The owner chose the name on 2026-11-12. | Q1 | 1.1, 19 |
| D2 | Licence Apache-2.0. First version 0.1.0. Version 1.0.0 after M7. | Q1 | 17, 18 |
| D3 | One static musl binary with subcommands. | Q1 | 3.1, 17 |
| D4 | Cargo workspace of six crates: `hune-core`, `hclconf`, `hune-store`, `hune-source`, `hune-grafana`, `hune`. | Q1 | 3.2, 3.3 |
| D5 | `hune-core` holds the format, limits, ids, canonical JSON, etag, ClickHouse lexer, macros, variables, escaping, interval rule and frame types. It does no I/O. | Q1 | 3.2, 3.4 |
| D6 | HTTP server: `hyper` 1.x with an own small router (about 30 routes). No axum. | Q1 | 3.8, 6 |
| D7 | Own versioned and closed JSON format, member `format_version` (3 since milestone F, wave 2, 4.42), with the concepts of Grafana v2. | Q2 | 4.1 |
| D8 | One dashboard is one document: one file or one row. The member `uid` equals the location. | Q2 | 4.5, 5.9, 5.24 |
| D9 | Folders have one level. Id and uid rule `^[a-z0-9][a-z0-9-]{0,39}$`. Each dashboard is in one folder. Grafana root dashboards go to the folder `general`. | Q2 | 4.3, 4.41, 15 |
| D10 | SQL is an array of lines. | Q2 | 4.16 |
| D11 | Canonical bytes: fixed member order, two-space indent, final line end. The etag is the SHA-256 (hex) of the canonical bytes. | Q2 | 4.38, 4.39 |
| D12 | The limits are constants of the format. | Q2 | 4.35 |
| D13 | The server reads only the current `format_version`. A change of format: stop, `hune migrate-documents`, start. A stored document that is invalid or of another version is never served: 422 `stored_document_invalid`. | Q2 | 4.42, 5.6, 5.8 |
| D14 | Volume layout: `<root>/<folder>/<uid>.json`, `_folder.json`, `.history/<uid>/<n>.json` and `<n>.meta.json`. | Q3 | 5.9, 5.11 |
| D15 | Volume write order: history first, temporary file with `O_EXCL`, write, fsync, etag check under the lock of the uid, rename, directory fsync. | Q3 | 5.13, 5.14 |
| D16 | Volume: polling every `storage.volume.rescan_interval`. No inotify. | Q3 | 5.17 |
| D17 | Volume: one writer process with `flock` on `<root>/.hune/lock`. Several writers on one shared volume are not supported. | Q3 | 5.16 |
| D18 | Volume: no symbolic link is followed or created. The root is absolute and canonical. Read-only mode `storage.volume.read_only`. | Q3 | 5.12, 5.19, 5.21 |
| D19 | PostgreSQL: the document is `text`, not JSONB. Four tables. Client `tokio-postgres`, `deadpool-postgres`, `tokio-postgres-rustls`. | Q4 | 5.23, 5.24 |
| D20 | Schema migrations only by `hune migrate-store` under an advisory lock, with its own file and the owner role. `serve` uses a role with DML rights only, and stops at start when the schema version differs. | Q4 | 5.25, 5.28, 5.29 |
| D21 | Optimistic lock by etag and expected folder in one transaction that also inserts the version row (409 `conflict`). Idempotent saves by `save_id`; a reused `save_id` gives `idempotency_key_reused`. | Q4 | 5.5, 5.27 |
| D22 | No LISTEN/NOTIFY. Each instance polls the catalog every `storage.postgres.poll_interval`. Each panel query carries the etag of its dashboard. | Q4 | 5.30, 3.13, 7 |
| D23 | Reads come from the in-memory catalog while the primary is away. Writes need the primary. PostgreSQL 15 to 18. | Q4 | 5.31, 5.32 |
| D24 | Reference HA: CloudNativePG. Failover test: a three-node Patroni cluster in Docker. | Q4 | 5.32, 16 |
| D25 | One `Store` trait, one conformance suite, `copy-store` in both directions. | Q4 | 5.3, 5.33, 5.37 |
| D26 | The block `access { mode = "open" \| "authzen" }` MUST be in the file. Run-time choice only, no Cargo feature. | Q5 | 11 |
| D27 | Mode `open`: no authentication, no token, no PDP, subject `-`, ProcurAuth routes answer 404, `config.warning` at start, writes follow `server.editing`. | Q5 | 11 |
| D28 | Mode `authzen`: PEP on `procurauth-pep` and `procurauth-core` (git, full commit, `default-features = false`, vendored) with an own rustls `PdpTransport` on a bounded blocking pool. | Q5 | 3.7, 11 |
| D29 | Tree `service/folder/dashboard` and `service/source` (depth 2). Service id = `access.identity.audience`. | Q5 | 11 |
| D30 | Actions `dashboard.read`, `dashboard.write`, `source.query`. Ceilings `max_rows` and `max_duration`. No budget in v1. | Q5 | 11 |
| D31 | PDP down: 503 `pdp_unavailable`, fail closed; kept answers serve until `cache_ttl`. An unknown uid gives no existence leak. | Q5 | 11 |
| D32 | `DataSource` trait. ClickHouse over HTTP(S) with an own thin client (`hyper`, `rustls`), format `JSONCompactEachRowWithNamesAndTypes`. | Q6 | 8 |
| D33 | Labelled blocks `clickhouse "<id>"` (1 to 64). Credentials from `password_env` or `password_file`. Real certificate checks. | Q6 | 8, 13 |
| D34 | Per-source limits, semaphores per source and per subject, bounded wait queue, `query_id` and `log_comment`. Cancellation by connection close; no `KILL QUERY`. | Q6 | 8 |
| D35 | Only the server builds SQL. A viewer sends no SQL. Context-aware escaping driven by the ClickHouse lexer. A ClickHouse SQL text can also use the query parameters of ClickHouse (`{name:Type}`), with a closed list of types: the value goes in the URL as `param_<name>`, never in the SQL text (7.6a, decision ID-K330, which changes the first text "no query parameters in v1"). | Q7, ID-K326 | 7, 8 |
| D36 | One statement whose first keyword is `SELECT` or `WITH`. The ClickHouse account is the real boundary. | Q7 | 7, 8 |
| D37 | The 5 macros and the interval rule. | Q7 | 7 |
| D38 | Front end: native ES modules in `web/`, no bundler, no npm, no build step; `build.rs` embeds each file with its SHA-256; hashed asset URLs. | Q8 | 9 |
| D39 | CSP without `'wasm-unsafe-eval'` and without inline script in v1. | Q8 | 12 |
| D40 | The ClickHouse Dash chart engine is copied at a recorded commit, with a lock file and the MIT notice. Hune owns the copy. | Q8 | 9 |
| D41 | All 7 panel types in v1, each limited to the options of the real panels. Light and dark themes. English only. axe-core in the browser tests. | Q8 | 4.27 to 4.33, 9 |
| D42 | One documented columnar JSON frame. 64-bit integers, large integers and decimals travel as exact text. Time as Unix seconds. | Q9 | 7 |
| D43 | One HTTP request for each panel query. The binary compresses only its static files, with gzip, one time at start (ID-K304). Decimation in SQL and in the chart engine. | Q9 | 7, 9 |
| D44 | `filterByValue`, `groupingToMatrix`, long-to-wide and panel reuse run in the browser as pure functions. | Q9 | 4.25, 9 |
| D45 | Viewer first, editor second; both in 1.0. Editor v1: source text first. Visual editors for overrides, mappings, links and transformations in v1.1. | Q10 | 10 |
| D46 | Save with `If-Match` and `Idempotency-Key` (`save_id`). The draft stays in `localStorage` until the save succeeds. Conflict screen with three choices. | Q10 | 10, 6 |
| D47 | History list, read-only view, diff and restore (a new version). A delete writes a tombstone version; only `hune restore` restores the dashboard, and a create of its uid gives `already_exists`. | Q10 | 5.4, 5.15, 5.40, 10 |
| D48 | Public dashboards and short links are removed. Hune has no "Share" button: the URL of the browser holds the view (ID-UI-SHARE). | Q11 | 1.4, 15 |
| D49 | `hune import-grafana` reads exported files only. Dry run by default. Four classes. Exit code 1 for a loss that is not accepted. | Q12 | 15 |
| D50 | Verification on the 10 real dashboards: expanded SQL equal to Grafana `executedQueryString`, then a side-by-side visual check. | Q12 | 15, 16 |
| D51 | HCL read by `hclconf`: closed schema, no default value, `_env` and `_file`, literals only, generated reference. | Q13 | 13 |
| D52 | `hune check-config`, `print-config` and `print-reference`. No environment variable is read unless an `_env` key names it. | Q13 | 3.4, 13 |
| D53 | JSON logs on stdout with an `event` field; metrics with the prefix `hune_`; the `telemetry` block with an own OTLP/HTTP exporter. The exporter is part of 1.0, is not optional, and comes in M7. A build before M7 stops at start with the error `telemetry.enabled: this build has no OTLP exporter` when `telemetry.enabled = true`; `telemetry.enabled = false` is valid in every build. | Q14, Coordination | 13, 14, 18 |
| D54 | Durable audit in the store: the history version comes before the effect. Log line `dashboard.changed`. | Q14 | 5.14, 14 |
| D55 | Two listeners: `server.listen` for the page and the API, `server.ops_listen` for `/livez`, `/readyz` and `/metrics`. | Q14 | 3.8, 14 |
| D56 | `/readyz` fails only until the catalog loads one time. `hune health` is the container health check. | Q14 | 14 |
| D57 | Graceful shutdown with a bounded grace. Request size and concurrency limits. Security headers and the CSRF guard. | Q14 | 12, 14 |
| D58 | All tests run in Docker, in the layers of Q15. | Q15 | 16 |
| D59 | GitHub release with amd64 and arm64 tarballs and `SHA256SUMS`. Image `FROM scratch`, built locally, never pushed. Compose examples; K3s example. | Q15 | 17 |
| D60 | Milestones M0 to M7. Cutover of the reference deployment after M4 in volume storage and mode `open`. | Q16 | 1.6, 15, 18 |
| D61 | Effort: an open item until the re-estimate of M0. The baseline is 16.6 person-months (range 13 to 21) for one experienced engineer, with the cutover after about 12.6 person-months (about month 13 for one engineer). | Q16, Coordination | 18, 19 |
| D62 | Out of scope: alerting, MongoDB (until the milestone MMG, 18.10b), Kafka, annotations other than the query annotations of 4.5b, library panels, organisations, teams, plugins, public dashboards, short links. | Q16 | 1.4, 1.7 |
| D63 | URL contract: `/d/<uid>/<slug>`; query names `from`, `to`, `var-<name>`, `refresh`, `viewPanel`; `server.base_path`; relative URLs; an nginx `map` for 6 months. | MISSING 1 | 6, 9, 15 |
| D64 | Time model: the browser resolves relative times to epoch milliseconds; the server gets absolute values and checks `from < to`; `timezone` is `browser` or `utc`. | MISSING 2 | 4.4, 4.6, 7, 9 |
| D65 | API: the specification lists every route; errors are `{"error": {"code", "message", "retryable"}}` with a closed code table; the API has no version. | MISSING 3 | 6 |
| D66 | Backup and restore: `hune export` and `hune import`; volume also by file copy or git; PostgreSQL also by CloudNativePG backups; CI restore drill. | MISSING 4 | 5.38, 5.39, 5.41, 16 |
| D67 | ClickHouse account contract (read-only profile, constraints, `SELECT` only, no `SOURCES` grants, no `KILL QUERY`) and supported versions. | MISSING 5 | 8 |
| D68 | Display names rule for `byName` and `byRegexp`. `byRegexp` uses the Grafana form `stringToJsRegex`: `/body/flags` adds no anchor, and a plain text is an exact match. The parity harness checks the headers. | MISSING 6, Coordination | 4.17, 4.24, 9 |
| D69 | Value encoding: Unix seconds; exact text for 64-bit integers and decimals; charts lose precision above 2^53. | MISSING 7 | 7, 9 |
| D70 | Confidentiality: real exports, captured Grafana answers and screenshots never enter the repository. | MISSING 8 | 15, 16 |
| D71 | Open items: the name, the users of the public dashboard, the content of the 142 queries, the ClickHouse version in production, the effort range. | MISSING 9 | 19 |
| D72 | Field default `no_value` (Grafana `noValue`, 12 real panels), in `fields.defaults` only. | Coordination | 4.18 |
| D73 | Eight history version kinds: `create`, `save`, `restore`, `move`, `delete`, `import`, `external`, `migrate`. `saved_by` is a subject, `-`, `external` or `import-grafana`; these three texts are reserved. | Coordination | 5.2, 5.11 |
| D74 | A validation error has the members `pointer`, `code`, `message`, `line` and `column`. | Coordination | 4.37 |
| D75 | The client always gives the uid in the document that it creates. The server never makes a uid; a uid that exists gives `already_exists`. | Coordination | 4.3, 5.4 |
| D76 | `serve` stops at start when its PostgreSQL role is a superuser or owns the Hune tables. `migrate-store` uses the owner role through `migrate.hcl`. | Coordination | 5.25, 5.28, 5.29 |
| D77 | The document member `refresh` is `null` or one value of the constant `REFRESH_CHOICES`: `10s`, `30s`, `1m`, `5m`, `15m`, `30m`, `1h`, `2h`, `1d` (the `refresh_choices` of section 6). The importer maps another Grafana value to the next larger choice, class `changed`. | Coordination | 4.6, 4.35, 6, 15 |
| D78 | A regular expression of a document uses one common subset that `hune-core` checks and the browser uses. | Coordination | 4.24 |
| D79 | PostgreSQL is a data source from the milestone MPG: the block `postgres "<id>"`, the dialect of 7.19 to 7.26 and the client of section 8a. It reads only: the role, the session, the transaction and the grammar all refuse a write. The security choices are PENDING OWNER with the defaults of section 8a (19.32). | Owner request | 7.19 to 7.26, 8a, 13.12a, 19.32 |

## 3. Architecture

### 3.1 One binary

The product is one static binary, `hune`, linked with musl for `x86_64` and `aarch64`. The binary has the subcommands of the table. A subcommand reads the configuration file that `--config <path>` (short form `-c <path>`) names (section 13), with these exceptions:

- `version` and `print-reference` read no configuration file.
- `copy-store` reads two files, with `--from <path>` and `--to <path>` (5.37).
- `import-grafana` needs `--config` only with `--write` (section 15).

| Subcommand | Function | Section |
|---|---|---|
| `serve` | Run the server: page, API and ops listener | 3.5, 6, 14 |
| `check-config` | Check the configuration file, print every error, exit | 13 |
| `print-config` | Check the file, print the value and the source of each key, exit | 13 |
| `print-reference` | Print a complete configuration file from the key table | 13 |
| `health` | Call `/livez` on the ops listener (container health check) | 14 |
| `import-grafana` | Read exported Grafana files, report, write through the `Store` trait | 15 |
| `migrate-store` | Apply the PostgreSQL schema migrations | 5.28 |
| `migrate-documents` | Migrate stored documents to the current `format_version` | 4.42 |
| `copy-store` | Copy a store into an empty store of either kind | 5.37 |
| `export` | Write a store as a volume layout in a directory | 5.38 |
| `import` | Read a volume layout into an empty store | 5.39 |
| `restore` | Restore a deleted dashboard | 5.40 |
| `version` | Print the version and the commit | 17 |

### 3.2 Workspace

The repository is one Cargo workspace with six crates. The ProcurAuth crates are git dependencies, vendored for offline builds (section 11, section 17).

| Crate | Kind | Role | Main direct dependencies |
|---|---|---|---|
| `hune-core` | library | Document format 3: types, closed parse, validation, canonical JSON, etag, limits, id rules. ClickHouse lexer, macros, variables, escaping, interval rule, frame types. No I/O. | `serde`, `serde_json`, `sha2`, `ryu`, `regex` |
| `hclconf` | library | Strict HCL engine with a key table, written in Hune from section 13 (decision ID-01), with a `PROVENANCE` file. It is not a copy of another HCL engine of the maintainer. The crate is pinned to `edition = "2021"`, and `PROVENANCE` records it: in edition 2024, `std::env::remove_var` is an `unsafe` function, and R5 forbids `unsafe`. | none |
| `hune-store` | library | The `Store` trait, the volume backend, the PostgreSQL backend, the catalog, `copy_store`, the conformance suite (feature `conformance`) | `hune-core`, `tokio`, `tokio-postgres`, `deadpool-postgres`, `tokio-postgres-rustls`, `rustls`, `rustix`, `getrandom` |
| `hune-source` | library | The `DataSource` trait, the ClickHouse HTTP client, the PostgreSQL client, the frame builder, limits, admission, cancellation | `hune-core`, `tokio`, `hyper`, `hyper-util`, `rustls`, `tokio-rustls`, `tokio-postgres`, `tokio-postgres-rustls` |
| `hune-grafana` | library | Grafana v1 and v2 JSON to format 3 and the loss report, as pure functions | `hune-core`, `serde`, `serde_json` |
| `hune` | binary | Configuration types on `hclconf`, the HTTP server and its router, the access glue (PEP and `PdpTransport`), the embedded `web/` files, telemetry, the command line | the five crates above, `tokio`, `hyper`, `hyper-util`, `rustls`, `procurauth-pep`, `procurauth-core` |

Section 17 holds the complete list of permitted crates (`deps.lock.txt`) and the `cargo deny` rules. A new direct dependency needs a line in that list with its reason.

### 3.3 Dependency rules

The workspace dependency graph is fixed:

```text
                 hune (binary)
   +---------+------+------+------------+-------------+
   |         |             |            |             |
hclconf  hune-store  hune-source  hune-grafana  procurauth-pep -> procurauth-core
             |             |            |
             +-------------+------------+
                           |
                       hune-core
```

1. `hune-core` and `hclconf` depend on no crate of the workspace.
2. `hune-store`, `hune-source` and `hune-grafana` depend on `hune-core` only. They do not depend on each other.
3. Only `hune` depends on `hclconf`, `procurauth-pep` and `procurauth-core`.
4. The graph has no cycle. A test in the `policy` job of section 17 compares the graph with this list.

### 3.4 Code rules

These rules apply to all code of the workspace. The `policy` job of section 17 checks each rule on the source and on the dependency graph.

| Rule | Statement |
|---|---|
| R1 No I/O in the core | `hune-core` MUST NOT use `std::fs`, `std::net`, `std::env`, `std::process`, `tokio`, a clock or a random source. Time values and random bytes are arguments of its functions. |
| R2 One reader of the environment | Only `crates/hclconf/src/env.rs` reads the environment. `main` calls `CapturedEnvironment::capture_and_clear()` first, before any other thread starts. After this call, no library can read a variable such as `RUST_LOG`, `TOKIO_WORKER_THREADS` or `OTEL_*`. The check finds the pattern `std::env::(var|var_os|vars|vars_os|set_var|remove_var)\b` in each `.rs` file and exempts `crates/hclconf/src/env.rs` only. Tests (`tests/` directories) and `build.rs` files are not product code and are exempt. No file `crates/hune/src/config/env.rs` exists. |
| R3 No process spawn | No code uses `std::process::Command` or an `exec` function. |
| R4 One writer of files | Only `hune-store/src/volume/` creates, writes, renames or removes files and directories. The one exception is the report file of `import-grafana --report`, written by one function in `hune/src/cli/`. |
| R5 No unsafe code | Each crate of the workspace sets `unsafe_code = "forbid"`. |
| R6 One TLS stack | All TLS links (ClickHouse, PostgreSQL, PDP, OTLP) use `rustls` with the `ring` provider. No OpenSSL, no `native-tls`, no `aws-lc`. No TLS link loads the system root store: each link takes its CA certificates from a file of the configuration. |
| R7 No crate that reads variables | No `reqwest` (it reads proxy variables) and no OpenTelemetry SDK (it reads `OTEL_*`). |
| R8 Enum dispatch | The store is the enum `AnyStore { Volume, Postgres }`. The data source is an enum with the variants `ClickHouse` and `Postgres` (8a.2). No `dyn` trait object for these two. |

### 3.5 Start of `serve`

Section 14 owns the exact start sequence and the exit codes. In summary, the process does these steps in this order:

1. Call `CapturedEnvironment::capture_and_clear()`.
2. Read the command line. A command line that is not valid stops the process with exit code 2.
3. Read the configuration file and check it with `hclconf` (section 13). Any error stops the process with exit code 1 before it serves. The message names the key, the expected type or range, and the value.
4. Write the effective configuration to the log, with the secrets hidden (section 13).
5. Build the `tokio` runtime with the values of 3.6. Build the TLS configurations, the source clients and the PEP. This step sends nothing on the network.
6. Bind `server.ops_listen`. A bind error stops the process with exit code 1. The ops listener serves at once, and `/readyz` answers 503.
7. Open the store. The volume backend does its start checks (5.21). The PostgreSQL backend connects and checks the schema (5.29).
8. Load the catalog one time (5.8). Then start the poller of the catalog and the other tasks of 3.6.
9. Bind `server.listen` and accept connections on it. A bind error stops the process with exit code 1. From this point, `/readyz` answers 200. Before this step, a connection to `server.listen` is refused.

### 3.6 Runtime and tasks

The server runs on the multi-thread `tokio` runtime. The runtime builder sets `worker_threads` to the value of `std::thread::available_parallelism()`. It sets `max_blocking_threads` to 16 plus P, where P is `access.pdp.max_connections` in mode `authzen` and 0 in mode `open`.

| Task | Count | Function |
|---|---|---|
| Accept loop of `server.listen` | 1 | Accepts connections up to `server.max_connections`; one task for each connection |
| Accept loop of `server.ops_listen` | 1 | Serves `/livez`, `/readyz` and `/metrics` |
| Connection task | one for each connection | Reads HTTP/1.1 requests with `hyper` and runs the router |
| Catalog poller | 1 | Calls `Store::poll` every `storage.volume.rescan_interval` or `storage.postgres.poll_interval` (5.8) |
| Telemetry exporter | 0 or 1 | Sends OTLP data when `telemetry.enabled = true` (section 14) |
| Signal task | 1 | Starts the graceful shutdown on SIGTERM or SIGINT (section 14) |
| Source tasks | as section 8 | The tasks of the ClickHouse clients |

A request runs on its connection task. It never holds a store lock or a semaphore permit across an `await` that waits for the browser.

### 3.7 Blocking work

Four kinds of work block a thread. Each kind runs in `tokio::task::spawn_blocking` and takes a permit of its own semaphore first. Thus one kind cannot use all the threads that `max_blocking_threads` permits.

| Work | Semaphore permits | Reason |
|---|---|---|
| Calls to the ProcurAuth PEP (`Pep::decide` and the other entry points) | P = `access.pdp.max_connections` | `procurauth-pep` is synchronous. Its `PdpTransport` uses `std::net::TcpStream`. Hune gives a blocking rustls transport. |
| File operations of the volume backend, or the checks of stored documents of the PostgreSQL backend | 8 | System calls on files block. A check reads a whole document. |
| Validations of a document (4.36, 7.18) | `MAX_INFLIGHT_VALIDATIONS` = 4 | A validation reads a whole document (decision ID-K313) |
| DNS lookups of the HTTP clients | the other blocking threads | `getaddrinfo` blocks |

A task that waits for a permit counts this wait against the deadline of its request. The PEP transport bounds each exchange with `access.pdp.timeout` (section 11).

### 3.8 Listeners

| Listener | Key | Routes | Token in mode `authzen` |
|---|---|---|---|
| Main | `server.listen` | The page, the static files, the API, `/.well-known/procurauth/actions`, `/.well-known/procurauth/resources` | Required on every route except the action document (section 11) |
| Ops | `server.ops_listen` | `/livez`, `/readyz`, `/metrics` only | No token. The ops port MUST NOT be reachable through the edge. |

Both listeners speak HTTP/1.1 through `hyper` 1.x. An own router maps about 30 routes (section 6). All paths of the main listener are relative to `server.base_path`.

### 3.9 Caches

The table gives every cache of the server. No cache holds query results.

| Cache | Key | Value | Bound | Change |
|---|---|---|---|---|
| Catalog | uid; folder id | The current document: canonical bytes, parsed document, lexed SQL of each panel and variable, etag, version, folder. The title of each folder. The errors of each invalid document. | 10,000 dashboards and 1,000 folders (5.7) | Own writes update it at once. The poller applies outside changes (5.8). |
| PDP answers (inside `procurauth-pep`) | subject, action, resource with its chain, match mode, usage side | The decision | `cache_max_entries` of the PEP configuration (section 11) | An answer expires after `access.pdp.cache_ttl` |
| Static files | path under `/static/{build-hash}/` | The bytes, the media type, the SHA-256 | Fixed at build | Never: a new build gives a new hash |
| Connection pools | one pool for each source (ClickHouse or PostgreSQL); one pool for the PostgreSQL store | Idle connections | Section 8; `storage.postgres.max_connections` | Idle timeout; a broken connection leaves the pool |

### 3.10 Flow: open a dashboard

1. The browser asks for `/d/{uid}/{slug}`. The server sends the shell page with `Cache-Control: no-cache`.
2. The browser loads the static files from `/static/{build-hash}/`. These files have `immutable` caching.
3. The browser calls `GET /api/session`. The answer gives the access mode, the value of `server.editing` and the subject.
4. The browser calls `GET /api/dashboards/{uid}`.
5. In mode `authzen`, the server asks `dashboard.read` on `service/folder:<f>/dashboard:<uid>` (section 11).
6. The server reads the entry from the catalog. It sends the canonical document, the etag, the folder and the version (section 6).
7. The browser reads `from`, `to`, `var-<name>` and `refresh` from the URL. It resolves the time range to epoch milliseconds (section 9).
8. The browser asks for the options of each query variable in dependency order with `POST /api/dashboards/{uid}/variables/{name}/options`.
9. The browser runs the panel queries of the visible panels, at most 6 at the same time (3.11).

### 3.11 Flow: run a panel query

1. The browser sends `POST /api/dashboards/{uid}/panels/{id}/query`. The body holds the etag of the shown document and the variable values. It also holds `from` and `to` in epoch milliseconds and `max_data_points` (section 6). It holds no SQL.
2. The server does the steps of 6.28: limits, the write guard of section 12, the identity token, and the body checks.
3. The server finds the catalog entry. In mode `authzen`, it asks `dashboard.read` on the dashboard. The ceilings of the answer lower the limits of the source.
4. The server compares the etag of the request with the etag of the catalog. When they differ, it reads the uid from the store one time (7.15). When the etags still differ, it answers 409 `dashboard_changed`, and the browser loads the dashboard again (3.13).
5. The server finds the panel. A panel with `data.reuse` never reaches the server: the browser computes it from the frame of its source panel.
6. The server checks the time range, `max_data_points` and the variable values (section 7).
7. `hune-core` expands the stored SQL with the macros and the variable values. It escapes each value as section 7 specifies. The server checks the statement and the size of the final SQL.
8. The server takes a permit of the source and of the subject, or waits at most `queue_wait` (section 8).
9. `hune-source` sends the SQL to ClickHouse in the body of an HTTP POST. The request has the settings, `query_id` and `log_comment` of section 8. For a PostgreSQL source it sends the SQL in one Parse message, in a read-only transaction (section 8a).
10. The server reads the rows as a stream, builds the columnar frame of section 7 and sends it.
11. The browser draws the panel. It then computes each panel that reuses this frame, with its transforms (section 9).

Section 7 (7.2) gives the exact order of these steps. Nothing reaches ClickHouse before step 9.

When the browser aborts the request, the server drops it. The ClickHouse connection closes and ClickHouse cancels the query (section 8).

### 3.12 Flow: save a dashboard

1. The editor sends `PUT /api/dashboards/{uid}`. The request has `If-Match` with the etag of the draft base and `Idempotency-Key` with a new `save_id`. The body holds the document and an optional message (section 6).
2. The server does the steps of 6.28: limits, the write guard of section 12, the identity token, and the body checks.
3. `hune-core` validates the document and lists every error with its JSON pointer (4.36, 4.37). The server checks the SQL statements (section 7).
4. In mode `authzen`, the server asks `dashboard.write` on the dashboard. It also asks `source.query` on each source that the document names (section 11).
5. The server calls `Store::save` (5.4). The store writes the history version first, then the document (5.14, 5.27).
6. The server updates the catalog and writes the log line `dashboard.changed`.
7. The server answers 200 with the new etag and version. The browser removes the draft from `localStorage`.

When the store is not available, the server answers 503 and the browser keeps the draft. The browser sends the save again with the same `save_id`. The store then gives the result of the first save if it was written (5.5).

### 3.13 Flow: a stale etag

Two authors, A and B, open the same dashboard with the etag E1.

1. A saves. The store writes the version with the etag E2.
2. B saves with `If-Match: E1`. The store finds the current etag E2 and returns `Conflict` with E2.
3. The server answers 409 with the code `conflict` and the current etag E2 (section 6).
4. The editor of B gets the current document and shows the conflict screen with a line diff (section 10).
5. B selects one of the three choices: load theirs and keep the draft, overwrite with `If-Match: E2`, or cancel.

A viewer of the dashboard also sees the change. The next panel query of that page carries E1, and the server answers 409 `dashboard_changed`. The page then loads the document with E2. Other instances see E2 after one poll of the catalog (5.30), and they apply the same rule.

### 3.14 Flow: import

Grafana import (section 15):

1. The operator exports the Grafana dashboards and folders to files.
2. `hune import-grafana` reads the files and maps each dashboard to format 3. The default is a dry run: it prints the report and writes nothing.
3. With `--write`, the importer calls `Store::create` with the version kind `import`. It refuses a uid that exists, except with `--replace`, which calls `Store::save`.
4. A `serve` process on a PostgreSQL store sees the new dashboards after one poll. On a volume, `--write` needs the lock (5.16). Thus a `serve` that writes MUST be stopped first. A read-only `serve` (5.19) takes no lock and can run.

Import of one document in the editor (section 10): the browser reads a file of format 3. It sends the file with `POST /api/dashboards`. The server validates it and calls `Store::create`.

### 3.15 Topology LOCAL

```text
 browser
    |  HTTPS
    v
 nginx (auth_request, prefix /dashboards/)        reference compose host
    |  HTTP, full path (prefix kept)
    v
 +------------------------------------------+
 | hune serve (one process, uid 65532)     |
 |   server.listen      0.0.0.0:8080        | <-- page and API
 |   server.ops_listen  0.0.0.0:9090        | <-- hune health, Prometheus
 |   server.base_path = "/dashboards/"      |
 |   access.mode = "open"                   |
 +--------------+----------------+----------+
                |                |
                | files          | HTTPS 8443, dedicated read-only user
                v                v
   volume /var/lib/hune    ClickHouse
   (bind mount; git optional)
```

One process holds the lock of the volume. The ops port is on the compose network only. The proxy nginx keeps its current login for the whole prefix. It sends the full path, and Hune serves it below `server.base_path` (section 6).

### 3.16 Topology DISTRIBUTED

```text
 browser
    |  HTTPS
    v
 edge (ingress + ProcurAuth edge: session, identity token)
    |  HTTP + identity token header
    v
 Service hune :8080
    |                         |
    v                         v
 +----------------+    +----------------+    Deployment, 2 or more replicas
 | hune pod A    |    | hune pod B    |    access.mode = "authzen"
 | ops :9090      |    | ops :9090      |    <-- kubelet probes, Prometheus
 +--+----+----+---+    +--+----+----+---+
    |    |    |           |    |    |
    |    |    +-----------+----|----|------> PDP (HTTPS)
    |    +---------------------+----|------> ClickHouse (HTTPS)
    +-------------------------------+------> Service postgresql-rw (CloudNativePG)
                                               primary + 2 standbys, synchronous ANY 1

 Job "hune migrate-store": runs before each rollout that changes the schema
```

The instances do not talk to each other. Each instance polls the catalog every `storage.postgres.poll_interval`. A failover of PostgreSQL stops the saves for 1 to 2 minutes, and the reads continue from the catalog (5.31).

### 3.17 Sizing assumptions

The design uses these numbers. Section 16 gives the load test that checks them.

| Item | Assumption | Source |
|---|---|---|
| Browsers | About 50 at the same time | 52 distinct browsers in 2 months |
| Panels at each refresh | 45 (the largest dashboard) | Measure of 1.3 |
| Panel queries in flight from one tab | 6 at most | Section 9 |
| Panel queries at the peak | 50 browsers x 45 panels = 2,250 queries for each refresh period; about 75 queries each second with a 30 s refresh | Computed |
| Result size | Tables of hundreds to a few thousand rows; time series grouped by `$__timeInterval` | M1 measures it |
| Dashboards and documents | 10 dashboards, 161 panels; the largest SQL text is 71,119 characters | Measure of 1.3 |
| Catalog memory | A few MB for the 10 dashboards | Computed |
| Saves | 138 saves for 10 dashboards in the life of the Grafana data: a few saves each day at most | Measure of 1.3 |
| PDP questions (mode `authzen`) | One question for each subject, dashboard and `access.pdp.cache_ttl` | Section 11 |

At this load, the own time of a panel query MUST stay below 50 ms at the 95th percentile. The own time is the full time minus the ClickHouse time. Section 16 gives the test.
