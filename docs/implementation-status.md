# Implementation status

This document uses ASD-STE100 Simplified Technical English.

This document is not normative. It gives the state of the code at version 0.1.1 against the specification 0.1.
The specification stays the reference. Where the code differs from a paragraph, the table "Differences from the specification" names the paragraph.
Section 18 of the specification (`08-roadmap-open-items.md`) gives the milestones and their exit criteria.

## Checks on real data

The milestone M0 and the parity spike of 18.3a are not done. The verification V1 to V5 of 15.14 is not done either.
These checks need a copy of a real Grafana installation and runs on its ClickHouse server.

All tests of this repository use synthetic data. Thus the document format and the expander are not frozen against real dashboards.
The measures of Grafana 12.4.2 in 19.20a to 19.20n come from temporary dashboards on a local instance.

## State of each milestone

| Milestone | State | What exists | What is not done |
|---|---|---|---|
| M1 | Done | Workspace of six crates; `hclconf` and the key table; `check-config`, `print-config`, `print-reference`; the two listeners, logs, metrics, `/livez`, `/readyz`, `hune health`, the graceful stop; the format of section 4; the `Store` trait, the catalog and the volume; the read routes; the shell and the static files; CI scripts, workflows, Dockerfiles | The `aarch64` build: the release dry run in Docker built `x86_64` only, and two builds gave the same archive. |
| M2 | Done | Lexer, placeholders, escape rules, macros, interval rule, variables, expansion with the property P, statement check, checks at save; the ClickHouse client with the limits, the admission, the cancellation and the probe; the query, options, sources and preview routes; the queue time metric and `queue_ms` | The load test of 16.16 at full length |
| M3a | Partial | `hune-grafana` maps the four input forms; report, acceptance file, exit codes, nginx map; `hune import-grafana`; 300 expansion vectors from a local pinned Grafana 12.4.2 with the plugin 4.5.1, with their test | V1 to V3 on real dashboards |
| M3b | Partial | Shell, list, dashboard page, time picker, variables bar, the panel types, field configuration, transformations, reuse, themes; front-end vectors FE-01 to FE-15; the look of Grafana 12.4.2, measured side by side; the entry redirect to the one visible dashboard | The engine gaps of 19.20 that stay; axe-core (no network in the test image); V4 and V5 on real dashboards |
| M4 | Done | Volume writes, history, lock, tombstone, restore, read-only mode; the editor of section 10, with drag and resize of the panels (10.5a); conflicts; import and export of one document | Nothing known |
| M5 | Partial | The PostgreSQL backend; `migrate-store`; `serve`, `copy-store`, `export`, `import`, `restore` and `import-grafana` on both backends. The common conformance tests, the PostgreSQL tests and the command cases pass on PostgreSQL 15 and 18. The restore drill of 16.14 passes on PostgreSQL 15 and 18. `migrate-documents` changes format 1 to format 2. The compose cases G07 pass. | The failover test with Patroni (16.11) |
| M6 | Done | Mode `authzen`: identity token, PEP and PDP transport, browser case B23, questions of each route, unknown-uid rule, ceilings, display filter, action document, options route; `docs/ACCESS.md`, `docs/CONFORMANCE.md`; series U gives 110 cases, 60 passed, 0 failed, 50 skipped | The black-box API suite of 16.8 with `fake-pdp` in compose; the Docker mode of the series U script |
| M7 | Partial | The OTLP exporter with the spans of 14.10; the audit lines of 14.11; nine fuzz targets; the load test with a Node driver (the criterion of M2 passes) | The fuzz campaigns (the nightly job runs no fuzz target); the 30-minute load run; the CloudNativePG drill; the release 1.0.0 |
| MPG | Done | The PostgreSQL data source of section 8a: lexer, macros (87 vectors from the pinned Grafana), closed statement check, expansion with the property P, client with the extended protocol only, read-only transaction, exact row and byte limits, deadline and cancel, TLS verify-full with channel binding, probe R0 to R9, configuration block `postgres "<id>"`, Grafana importer, page scanner. A security review found one high, four medium and three low issues, and all are fixed with tests. The container suites pass on PostgreSQL 15 and 18. | The security choices of 19.32 are open questions with defaults. |
| MMG | Done | The MongoDB data source of section 8b: the strict JSON parser, the closed pipeline grammar with the refused names, typed expansion with the property PM, the frame, the wire guard, the probe M0 to M4, the block `mongodb "<id>"`, the importer and the page scanner. The driver is the crate `mongodb` 3.10.0 behind the feature `mongo-driver`, with trust only in the CA file of the block. A patched copy of the crate removes its reads of the environment. The cases MS01 to MS18 pass on MongoDB 7.0 and 8.0. | The security choices of 19.33 are open questions with defaults. |

## Features since the first plan

The table lists the features that the code has in addition to the milestones, and the limits of each one.

| Feature | What exists | Limits |
|---|---|---|
| Header, theme and page states | The actions of the list and of a dashboard in a sticky header with buttons of fixed size, the theme selector of ClickHouse Dash, the line `version:checksum`, no Share, one stable render on a reload. The header of 44 px, the joined time control, the Edit switch and the split button Add. The placement of a variable in the grid. One series palette of 18 colours with two themes. No button of the view moves between the view mode and the edit mode. Browser cases B69 to B92, B150 and B151. | No kiosk mode and no playlists. |
| Format 3 (F2) | A pure migration from format 2 and the chain from format 1. The right y axis and the axis options, the series style, the colour modes by name and the continuous modes, the panel time override, the dashboard links, the time zone names, and the forms of the editor for these members and for the overrides, mappings, links, transformations, settings and variables. | Nothing known. |
| Repeat (F23) | The repeat of a panel or a section by a variable, in the directions h and v. Optional members only: no migration and no new format version. Three configuration keys (`server.repeat_max_per_panel`, `server.repeat_max_per_section`, `server.repeat_max_per_dashboard`) with the defaults 24, 12 and 120. An empty or too large repeat fails closed. Browser cases B93 to B101. | One level of repeat only. A text panel and a panel with several queries do not repeat. |
| Text panel (F29) | The panel type `text` in Markdown. A closed list of constructs, no raw HTML, no image, and a closed list of link schemes (the same rule in the server and in the page). The page parser is linear. Variables in the content. Browser cases B102 to B109. | No mode `html` and no mode `code`. No repeat of a text panel. No save warning for an unknown placeholder. |
| State timeline and status history (F30) | The panel types `state_timeline` and `status_history`, with closed options. A canvas that Hune code draws, with no edit of the chart engine. A cap of 10,000 segments that fails closed. The legend, the card of the tooltip, the keyboard, the editor groups and the importer rows. Browser cases B110 to B118. | The look is measured on Grafana 12.4.2 and not compared to real dashboards. |
| Annotations (F25) | The document member `annotations` (0 to 8), the route `POST /api/dashboards/{uid}/annotations/{index}/query` with the same rights as a panel, and the caps (8 queries, 1,000 rows, 2,000 marks). The page draws the marks on a canvas over a time series. The editor tab, the importer rows and the fuzz target `annotation_frame`. Sources: ClickHouse, PostgreSQL and MongoDB. Browser cases B120 to B127 and B217. | Marks on the `timeseries` only. No viewer switch. No marks in edit mode. A number in the time column is epoch milliseconds for each kind of source. |
| Several queries (F24) | 1 to 7 extra queries (`extra_queries`), 8 for each panel and 400 for each document. One request for each query. The page joins the frames on time (200,000 rows at most) and shows a badge for a partial result. Hidden queries. The route 6.20 takes the optional member `ref`. Browser cases B130 to B137. | A repeat panel, a `barchart` and an `xychart` refuse extra queries. No move of a query. |
| Custom value of a variable (F41) | The member `allow_custom_value` of a `query` or `custom` variable. A closed variable shows a notice once for a value that is not an option. An open variable is a combobox, and its value is free text. For a `query` variable this is a rule of the page and not a security promise. The editor switch, the three SQL examples and the preview "Show options". Browser cases B160 to B169. | A custom variable that has `multi` or `includeAll` in a quoted name is refused, open or closed (7.8). The data source variable is out of scope. |
| Interval and constant variables (F27) | The variable kinds `interval` (a closed item grammar, the item Auto computed by the server) and `constant` (the value of the document only, never in the URL). The importer, the converter, the editor forms and the fixtures. Browser cases B190 to B199. | The ad hoc variable is out of scope. The editor cannot change the kind of a variable (10.3b). |
| More transforms (F28) | Eleven transform kinds: organize, calculate_field, sort_by, limit, merge, reduce, partition_by_values, join_by_field, filter_fields_by_name, rename_by_regex and convert_field_type. Closed members and caps that fail closed. The importer rows and the editor forms. Browser cases B180 to B189. The differences from Grafana are in 19.20j. | Nothing known. |
| Heatmap and histogram (F31) | The panel types `heatmap` and `histogram` with closed options, 10 colour schemes, and caps that fail closed (500,000 values, 250,000 cells, 4,000 buckets). The importer, the converter and the fixtures. The pure models `bucket_model.js` and `heat_model.js`. The page drawing with the engine edits E22 to E24, E26 and E27. Browser cases B140 to B148. | Three differences from Grafana stay on purpose: one bucket, the bucket offset and the header of a repeated section (19.20b, 19.20g). |
| Syntax tree of the query editor (F42) | A syntax tree for ClickHouse, PostgreSQL and MongoDB (the JSON text), written in Hune: format on request, folds, the bracket pair and the current block. Caps: 262,144 bytes, depth 128, 100,000 nodes. The tree is never a security check, and a fault never stops a save or a run (10.3c rule 1). The colour layer draws only the rows near the visible rows. Fuzz and property tests. Browser cases B170 to B177. | Firefox and WebKit are manual checks (16.19). |
| Option forms (F43) | The option forms for each data type, with the renderer, the tables and the controls. The audit B153 with its closed lists (`lib/audit_*.mjs`). The bar chart member `x_tick_rotation` turns the x labels (engine edit E25). The switch "Transparent background" of the panel editor changes the preview. | `state_timeline`, `status_history` and `text` keep their forms. |
| Variable names of a panel | The server gives the names of the variables that each panel, query variable and annotation uses: the closed member `names` of the answer 6.14 (64 names at most for each item, outside the etag). The scan is linear. | An old server, an absent member or a malformed member makes the page treat each panel as dependent on each variable, with one notice. |
| Engine patch | A local patch of the chart engine. The rows of `docs/engine-patches.md` list each marked edit. Some edits remove the code that Hune never calls, and others add the edits of the features above. The file `vendor/chdash.patch` holds all marked edits, `tools/check-vendor-lock.sh --chdash` checks them, and the lock file has the hashes. | Nothing goes upstream. |
| Static text compression | The binary compresses its static text files with gzip one time at start. The key `server.compress_static` (default `true`) turns this off. The encoder is Hune code (RFC 1951 and RFC 1952), with no new crate. The dashboard page goes from 1,035,927 to 326,225 bytes on the network. | No ETag for the static files. The shell is not compressed. |
| Parity captures | The tool `tools/parity/visual.mjs` compares the canvas, the boxes, the titles and the queries with Grafana 12.4.2. Browser cases B101p, B137 and B148 run it. The list of open faults is empty. | The comparison needs a disposable local Grafana. |
| Write routes and charts of a section | The 11.23 question is asked again for each write route at each request, so a known uid and an unknown uid send the same number of PDP requests. The charts of a section draw in the task that expands it (B210). The stacks of a time series, a bar chart and a histogram follow Grafana 12.4.2, with the engine edit E33. | A series with its own line interpolation keeps the interpolation of the panel. |
| Importer and units | The importer maps a data source variable with `--source-map`, moves the panels of a v1 row, and turns a Grafana default off only with an item. 30 more units. Six data link tokens of Grafana. The cell value inspect of a table. Three time forms of Grafana and `timezone` in the URL. The bar chart options `fill_opacity`, `line_width`, `axis_grid` and the orientation `auto`, with the engine edit E34. In mode `authzen`, the entry redirect counts the filtered list. Browser cases B211 to B216. | The URL time forms that give 1970 in Grafana stay invalid. |
| ClickHouse query parameters | The ClickHouse query parameters `{name:Type}`, in the strict form only, with a closed list of 8 types. The server checks each value by its type before it sends a byte to the source. It sends the pairs `param_<name>` with caps: 32 names, 1,000 items and 4,096 encoded bytes. The ClickHouse suite passes 34 of 34 cases. | ClickHouse sources only. |
| Gauge, logs and geomap | The panel types `gauge`, `logs` and `geomap`. The map is off by default: only the closed block `geomap` gives a tile source, and without the block the CSP does not change. The percent change and the text sizes of a stat. The gradient modes and the series options of a time series and a bar chart, with the engine edits E43 to E46. Ad hoc filters with `=` and `!=` on the columns of the key `adhoc_columns`. Browser cases B218 to B227. | The ad hoc operators `<`, `>`, `IN`, `=~` and `!~` are not done. |
| Engine savings | The engine edits E35 to E42 remove engine code that no page code calls. The page goes from 1,023,019 to 937,315 bytes. The test `engine_removals.test.mjs` keeps the removed names out. | Nothing goes upstream. |
| Page budget | The limit of 9.30 is 1,018 KB (1,042,432 bytes) for a measure of 1,037,796 bytes. | Each new feature needs room in this budget. |
| Panel loader | A panel that waits shows three dots in the middle of the body, 300 ms after the request starts (9.11, 9.26). A refresh still dims the earlier content to 60 %. Reduced motion gives three still, dimmed dots. A print has no dots. | Nothing known. |
| Security reviews | Independent reviews of the server, the page, the importer, the format and the SQL properties P, PG, PM and PQ. Each finding was verified by a second reviewer. All high and medium findings are fixed with tests. Examples: a quadratic name scan, lists with no cap, allocations before the caps, and a ClickHouse lexer that did not know the comments that start with `//`, the line ends of ClickHouse and the heredoc tags that start with a digit. | Low and info findings stay open: the display of hidden characters in the ad hoc chips, tiles that two large maps push out of the cache, and a `$__conditionalAll` condition with an open end beside `$__adhocFilters`. |
| Other browsers | Firefox ESR 153 runs 14 cases through WebDriver BiDi (`HUNE_BROWSER=firefox`). | B170 and B171 are manual checks in Firefox. WebKit is a manual check (16.19). |
| Releases | Version 0.1.1 is in `Cargo.toml`, `Cargo.lock`, `fuzz/Cargo.lock`, `hune version`, the page, the session and the deployment examples. Each image of `tests/images.lock` has a digest. Two release builds of `x86_64` give the same archive, byte for byte. The release workflow has the input `arm64` (17.3). | No `aarch64` build was made. GitHub Actions did not verify version 0.1.1: the jobs of `tests/ci/all.sh` ran on a local machine. |

## Open points

- The security choices of the PostgreSQL source (19.32) and of the MongoDB source (19.33) are open questions. The code follows the default of each choice.
- The advisory RUSTSEC-2026-0387 of `crypto-bigint` 0.5.5 stays in the list `ignore` of `deny.toml` until `procurauth-core` moves to `p256` 0.14.
- The origin of `web/icons.svg`, the commit pins of the workflow actions, and the toolchain of the fuzz job need a check.
- WebAssembly in the page is not done.

## How to build and test

Use these commands from the root of the repository. The toolchain file selects Rust 1.99.1.

1. Run `cargo build --workspace --locked`.
2. Run `cargo test --workspace --locked`.
3. Run `cargo clippy --workspace --all-targets --locked -- -D warnings`.
4. Run `cargo fmt --all -- --check`.
5. Run `node --test tests/frontend/*.test.mjs`.
6. Run `tests/ci/policy.sh --local`.
7. Run `python3 -I tools/ste_lint.py docs/*.md README.md CHANGELOG.md`.

The workflows in `.github/workflows` run the CI and the release on GitHub. They are optional. An operator can run every job and the release with `tests/ci/all.sh` and `tests/ci/release.sh`, with no GitHub Actions. The script `tests/ci/all.sh` runs every job of the merge gate in order and stops at the first failure (16.17). The option `--list` prints the jobs.

These scripts need Docker. Each one starts local containers on `127.0.0.1` and removes them at the end.

| Script | Tests |
|---|---|
| `tests/ci/source.sh` | The ClickHouse cases of 16.9 for the source crate |
| `tests/ci/server.sh` | The panel query, options and preview routes against ClickHouse |
| `tests/ci/store-pg.sh` | The conformance suite and the PostgreSQL tests on PostgreSQL 15 and 18 |
| `tests/ci/pgwire.sh` | `serve`, `migrate-store` and the store commands on PostgreSQL 15 and 18 |
| `tests/ci/pgsource.sh` | The container tests of the PostgreSQL source (16.9a) |
| `tests/ci/mongosource.sh` | The container tests of the MongoDB source (16.9b) |
| `node tests/browser/run.mjs` | The browser cases of 16.10 with the local Chrome; `tests/browser/README.md` gives the range and the count (Firefox: `HUNE_BROWSER=firefox` for the cases of 16.10); set `PROCURAUTH_DIR` for B23, B35, B108, B124 and B214 |
| `tests/conformance/run.sh --local` | The series U of ProcurAuth; it needs a ProcurAuth checkout |
| `tests/load/run.sh` | The load test of 16.16 |
| `tests/ci/fuzz.sh` | The fuzz targets; it needs the nightly toolchain and cargo-fuzz |
| `tests/ci/nightly.sh` | The random seeds, and the jobs `source`, `pgsource` and `mongosource` again; `all.sh` does not run it |

The Docker mode of the jobs `api` and `browser` is not built, because `tests/compose/api.yaml` and `tests/browser/Dockerfile` do not exist. The job `failover` is a stub until the failover test exists. Run the browser cases with `node tests/browser/run.mjs`.

## How to run Hune

1. Write a configuration file. Start from the output of `hune print-reference --format hcl --profile local`.
2. Run `hune check-config -c <file>`. Correct each error that it gives.
3. Run `hune serve -c <file>`.
4. Open the address of `server.listen` in a browser.

## Differences from the specification

| Paragraph | Difference | Why |
|---|---|---|
| 4.24 | The regex subset refuses three more forms than the table. | The Rust engine and the browser engine must read a pattern in the same way. |
| 13.11, 13.17 | The key order of `access.pdp` follows 13.17. | The reference file of 13.17 sets the order. |
| 7.11 | An empty insertion also gets the token check. | The token check applies to each insertion, with no exception. |
| 7.16, 8.3 | ClickHouse gives decimals without trailing zeros with the settings of 8.3. | ClickHouse formats the decimal, and Hune keeps the text that it receives. |
| 8.7 | The TCP keepalive idle time is the default of the system. | The system sets the idle time. |
| 9.3, 9.24 | The build accepts the file `web/js/vendor/chdash/LICENSE`. | The licence file of the copied chart engine is one of the vendored files. |
| 16.10 | The browser tests use Chrome through the DevTools protocol, not Playwright. | The runner needs no download and has no npm dependency. |
| 16.10a, 16.17 | The front-end tests run with a file glob. | `node --test` on a directory fails on Node 22. |
| 19.20 | The gaps of the copied chart engine stay. | An edit of the engine is made only for a gap that a real panel uses (19.20). |

The specification states the look of the front end and the document format 3. The text of 4.42 stays as the code has it.
