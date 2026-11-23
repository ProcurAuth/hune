# Implementation status

This document uses ASD-STE100 Simplified Technical English.

This document is not normative. It gives the state of the code against the specification 0.1. The rows up to F2 give the state of 2026-11-10. The rows of wave K and of the later work give the state of 2026-11-15. The rows of wave H and of the release 0.1.0 give the state of 2026-11-22.

The specification stays the reference. Where the code differs from a paragraph, the table "Differences from the specification" names the paragraph and the decision.

Each implementation decision has an id (ID-01 and the next ones). The maintainer keeps the decision record and the work log outside the repository. The decision ids in the documents and in the code are labels of that record.

## Scope of this work

The work started without the milestone M0 and without the parity spike of 18.3a. Both need a copy of the Grafana database of the reference deployment and runs on production. This work used synthetic data only. Thus the format and the expander are not frozen against the real dashboards.

These items are outside the scope of this work:

- M0, the spike, the verification V1 to V5 and the cutover. They need the real data and the go of the owner.
- The push of the repository and the publication of a release. The work makes local commits only. The coordinator pushes the repository and runs the release workflow for 0.1.0 (ID-K335).
- A deployment of the mode `authzen` with a real PDP. The code is in 0.1.0 (see the row M6). The tests use the ProcurAuth tools of a pinned commit, built offline (19.28).

## State of each milestone

| Milestone | State | What exists | What is not done |
|---|---|---|---|
| M1 | Done in code, in 0.1.0 | Workspace of six crates; `hclconf` and the key table; `check-config`, `print-config`, `print-reference`; the two listeners, logs, metrics, `/livez`, `/readyz`, `hune health`, the graceful stop; the format of section 4; the `Store` trait, the catalog and the volume; the read routes; the shell and the static files; CI scripts, workflows, Dockerfiles | The aarch64 build: the release dry run in Docker built x86_64 only, and two builds gave the same archive (ID-K344). `cargo deny` passes: `ctutils` is 0.4.4, and the owner accepted one advisory with a reason (ID-K358). |
| M2 | Done in code | Lexer, placeholders, escape rules, macros, interval rule, variables, expansion with the property P, statement check, checks at save; the ClickHouse client with the limits, the admission, the cancellation and the probe; the query, options, sources and preview routes; the queue time metric and `queue_ms` (ID-37) | The load test of 16.16 at full length (ID-49) |
| M3a | Library and command done | `hune-grafana` maps the four input forms; report, acceptance file, exit codes, nginx map; `hune import-grafana`; 300 expansion vectors from a local pinned Grafana 12.4.2 with the plugin 4.5.1, with their test (ID-84) | V1 to V3 (real data); the compose project `tests/compose/grafana.yaml` |
| M3b | Viewer done | Shell, list, dashboard page, time picker, variables bar, the 7 panel types, field configuration, transformations, reuse, themes; front-end vectors FE-01 to FE-15; the look of Grafana 12.4.2, measured side by side (ID-60 to ID-83); the entry redirect to the one visible dashboard (ID-83) | The engine gaps of 19.20 that stay (ID-33); axe-core (no network); V4 and V5 |
| M4 | Done in code | Volume writes, history, lock, tombstone, restore, read-only mode; the editor of section 10; conflicts; import and export of one document | Drag and resize (v1.1, 10.16) |
| M5 | Done in code, except the failover test | The PostgreSQL backend; `migrate-store`; `serve`, `copy-store`, `export`, `import`, `restore` and `import-grafana` on both backends. The common conformance tests, the PostgreSQL tests and the command cases pass on PostgreSQL 15 and 18. The restore drill of 16.14 passes on PostgreSQL 15 and 18 (ID-85). `migrate-documents` changes format 1 to format 2 (ID-75). | The failover test with Patroni (16.11); the compose test G07 for PostgreSQL; `tests/compose/store.yaml` |
| M6 | Done in code | Mode `authzen`: identity token, PEP and PDP transport, browser case B23, questions of each route, unknown-uid rule, ceilings, display filter, action document, options route; `docs/ACCESS.md`, `docs/CONFORMANCE.md`; series U gives 110 cases, 60 passed, 0 failed, 50 skipped (ID-56) | The black-box API suite of 16.8 with `fake-pdp` in compose; the Docker mode of the series U script |
| M7 | In progress | The OTLP exporter with the spans of 14.10; the audit lines of 14.11; the six fuzz targets; the load test with a Node driver (the criterion of M2 passes) | The fuzz campaigns (they wait for the owner; the nightly job runs no fuzz target, ID-K344); the 30-minute load run; the CloudNativePG drill; the release 1.0.0 |
| MPG | Done in code, pending the owner's decisions ID-PG1 to ID-PG17 | The PostgreSQL data source of section 8a: lexer, macros (87 vectors from the pinned Grafana), closed statement check, expansion with the property P, client with the extended protocol only, read-only transaction, exact row and byte limits, deadline and cancel, TLS verify-full with channel binding, probe R0 to R9, configuration block `postgres "<id>"`, Grafana importer, page scanner. An independent security review found one high, four medium and three low issues, and all are fixed with tests. The container suites pass on PostgreSQL 15 and 18. | The answers of the owner to ID-PG1 to ID-PG17 (19.32) |
| MMG | Done in code, pending the owner's decisions ID-MG1 to ID-MG24 | The MongoDB data source of section 8b: the strict JSON parser, the closed pipeline grammar with the refused names, typed expansion with the property PM, the frame, the wire guard, the probe M0 to M4, the block `mongodb "<id>"`, the importer and the page scanner. The driver is the crate `mongodb` 3.10.0 behind the feature `mongo-driver` (ID-MG2, granted), with trust only in the CA file of the block. A patched copy of the crate removes its reads of the environment (ID-MG25, option (a) of the owner). The cases MS01 to MS18 pass on MongoDB 7.0 and 8.0. | The answers of the owner to ID-MG1 to ID-MG24 (19.33) |
| UI | Done in code, pending the owner's decisions ID-UI-HDR, ID-UI-VER, ID-UI-THEME, ID-UI-SHARE, ID-UI-FLASH and ID-UF1 | Requests of the owner of 2026-11-11: the actions of the list and of a dashboard in a sticky header with buttons of fixed size, the theme selector of ClickHouse Dash, the line `version:checksum`, no Share, no visible "Unsaved changes", one stable render on a reload. Bug fixes: no scroll bar for a size error at a browser zoom (9.19a), no false mark on a macro (spell check off), the alignment of the selection in the SQL area. | The answers of the owner to these rows |
| F2 | Done in code, pending the owner's decisions ID-F2-1 to ID-F2-16 | Format 3 with a pure migration from format 2 and the chain from format 1. The right y axis and the axis options, the series style, the colour modes by name and the continuous modes, the panel time override, the dashboard links, the time zone names, and the forms of the editor for these members and for the overrides, mappings, links, transformations, settings and variables. | The answers of the owner to ID-F2-1 to ID-F2-16 |
| Wave K | Done in code. The owner accepted ID-K6 to ID-K66, ID-K71 and ID-K72 in bulk on 2026-11-13 (42 rows) | Requests of the owner of 2026-11-12. The product name Hune (ID-K40 to ID-K49). The placement of a variable in the grid (ID-K6 to ID-K10). The header of 44 px, the joined time control, the Edit switch and the split button Add (ID-K11 to ID-K17). The states of the page and their URL rules (ID-K21 to ID-K36). The colour theme of ClickHouse Dash (ID-K50 to ID-K55). Its time range picker (ID-K60 to ID-K66). One series palette of 18 colours with two themes (ID-K70, ID-K72, ID-K74). Browser cases B69 to B92. | ID-K1 to ID-K3 are replaced by ID-K6. ID-K13 and ID-K13a are superseded by ID-K177. ID-K71 is superseded by ID-K74. No row of this range waits for the owner. |
| F23 | Done in code. Three arbiters decided ID-K80, ID-K81, ID-K82, ID-K85, ID-K87 to ID-K89, ID-K91, ID-K92 and ID-K95 to ID-K97 on 2026-11-15, in the absence of the owner | The repeat of a panel or a section by a variable, in the directions h and v. Optional members only: no migration and no new format version. The caps are three configuration keys (`server.repeat_max_per_panel`, `server.repeat_max_per_section`, `server.repeat_max_per_dashboard`) with the defaults 24, 12 and 120, and the session gives them to the page. An empty or too large repeat fails closed. The URL forms, the editor group, the importer and the parity export. Browser cases B93 to B101. | One level of repeat only. A text panel and a panel with several queries do not repeat (ID-K116, ID-K165). The owner accepted ID-K83, ID-K84, ID-K86, ID-K90, ID-K93 and ID-K94 on 2026-11-13. The owner must re-read the twelve rows that the arbiters decided. |
| F29 | Done in code. The rows ID-K100 to ID-K119b are arbitrated in the absence of the owner, to re-read | The panel type `text` in Markdown (format 3). A closed list of constructs, no raw HTML, no image, and a closed list of link schemes (the same rule in the server and in the page). The page parser is linear. Variables in the content. The importer maps `markdown` and writes a note for the rest. Browser cases B102 to B109. | No mode `html` and no mode `code`. No repeat of a text panel (ID-K116). No save warning for an unknown placeholder (ID-K117). |
| F30 | Done in code. The rows ID-K120 to ID-K136 are arbitrated in the absence of the owner, to re-read | The panel types `state_timeline` and `status_history`, with closed options. A canvas that Hune code draws, with no edit of the chart engine. A cap of 10,000 segments that fails closed. The legend, the card of the tooltip, the keyboard, the editor groups and the importer rows. Browser cases B110 to B118 pass. | The look is measured on Grafana 12.4.2 and not compared to real dashboards (M0 is not done). |
| F25 | Done in code. The rows ID-K140 to ID-K158, ID-K155a and ID-K155b are arbitrated in the absence of the owner, to re-read | Annotations from a ClickHouse query: the document member `annotations` (0 to 8), the route `POST /api/dashboards/{uid}/annotations/{index}/query` with the same rights as a panel, and the caps (8 queries, 1,000 rows, 2,000 marks). The page draws the marks on a canvas over a time series. The editor tab, the importer rows and the fuzz target `annotation_frame`. Browser cases B120 to B127. | ClickHouse sources only (ID-K142). Marks on the `timeseries` only (ID-K148). No viewer switch (ID-K157). No marks in edit mode. The fuzz target ran as a short smoke test only. |
| F24 | Done in code. The rows ID-K160 to ID-K175c, ID-K166a, ID-K167a and ID-K170a are arbitrated in the absence of the owner, to re-read | Several queries in one panel: 1 to 7 extra queries (`extra_queries`), 8 for each panel and 400 for each document. One request for each query. The page joins the frames on time (200,000 rows at most) and shows a badge for a partial result. Hidden queries. The route 6.20 takes the optional member `ref`. The editor chips and dialogs. Browser cases B130 to B137 (B135 uses PostgreSQL). | A repeat panel, a `barchart` and an `xychart` refuse extra queries (ID-K165, ID-K169). No move of a query (ID-K170). |
| F41 | Done in code. The rows ID-K200 to ID-K213 are arbitrated in the absence of the owner, to re-read | The member `allow_custom_value` of a `query` or `custom` variable. A closed variable shows a notice once for a value that is not an option. An open variable is a combobox, and its value is free text. For a `query` variable this is a rule of the page and not a security promise (ID-K200). The editor switch, the three SQL examples and the preview "Show options". Browser cases B160 to B169. | A custom variable that has `multi` or `includeAll` in a quoted name is refused, open or closed (7.8). The data source variable is out of scope. |
| F27 | Done in code. The rows ID-K260 to ID-K270a are arbitrated in the absence of the owner, to re-read | The variable kinds `interval` (a closed item grammar, the item Auto computed by the server) and `constant` (the value of the document only, never in the URL). The four routes, the importer, the converter, the editor forms and the fixtures. Browser cases B190 to B199. | The data source variable and the ad hoc variable are out of scope. The editor cannot change the kind of a variable (10.3b). |
| F28 | Done in code. The rows ID-K240 to ID-K254b are arbitrated in the absence of the owner, to re-read | Eleven transform kinds: organize, calculate_field, sort_by, limit, merge, reduce, partition_by_values, join_by_field, filter_fields_by_name, rename_by_regex and convert_field_type. Closed members and caps that fail closed. The importer rows and the editor forms. Browser cases B180 to B189. Three differences from Grafana are in 19.20j. | No known gap. The full browser run of 2026-11-15 passes (198 of 198 cases). |
| F31 | Done in code. The rows ID-K180 to ID-K195c are arbitrated in the absence of the owner, to re-read; the owner decided ID-K195c (no prefix for the heatmap names) | The panel types `heatmap` and `histogram` with closed options, 10 colour schemes, and caps that fail closed (500,000 values, 250,000 cells, 4,000 buckets). The importer, the converter and the fixtures. The pure models `bucket_model.js` and `heat_model.js`. The page drawing with the engine edits E22 to E24, E26 and E27. Browser cases B140 to B148. The parity capture of B148 has no open fault (ID-K310). | Three differences from Grafana stay on purpose: one bucket, the bucket offset and the header of a repeated section (19.20b, 19.20g; ID-K310). |
| F42 | Done in code. The rows ID-K220 to ID-K233, ID-K230a and ID-K230b are arbitrated in the absence of the owner, to re-read | A syntax tree in the query editor for ClickHouse, PostgreSQL and MongoDB (the JSON text), written in Hune: format on request, folds, the bracket pair and the current block. Caps: 262,144 bytes, depth 128, 100,000 nodes. The tree is never a security check, and a fault never stops a save or a run (10.3c rule 1). Fuzz and property tests. Browser cases B170 to B177. | The colour layer draws only the rows near the visible rows (ID-K230a, fixed): a key costs about 8 ms of script on 246 KB, and one fold of 262 KB takes 33 ms with two frames. Firefox and WebKit are manual checks (16.19). |
| F43 | Done in code. The owner accepted the feature; the rows ID-K290 to ID-K298 and ID-K295b are arbitrated in the absence of the owner, to re-read; ID-K295a follows the answer of the owner | The option forms for each data type, with the renderer, the tables and the controls. The audit B153 with its closed lists (`lib/audit_*.mjs`). The bar chart member `x_tick_rotation` turns the x labels (engine edit E25), with the sign of Grafana (ID-K308). B153 and B200 to B206 pass in the full run of 2026-11-15. | `state_timeline`, `status_history` and `text` keep their forms. |
| K176 | Done in code. The owner accepted ID-K176 on 2026-11-14; ID-K176a is arbitrated in the absence of the owner, to re-read | The server gives the names of the variables that each panel, query variable and annotation uses: the closed member `names` of the answer 6.14 (64 names at most for each item, outside the etag). The scan is linear (ID-K280). A golden test with 4,750 texts and 42 documents loses no name. The page gain is 20,251 bytes (950,863 to 930,612). | An old server, a missing member or a malformed member makes the page treat each panel as dependent on each variable, with one notice. |
| Engine patch | Done in code (ID-K300). The owner accepted option b on 2026-11-14; the owner must read the patch again | A local patch of the chart engine. The rows E17 to E21 and E35 to E42 of `docs/engine-patches.md` remove the code that Hune never calls (ID-K300, ID-K350). The rows E22 to E34 add the edits of F31, F43 and the parity fixes, and E43 to E46 the edits of wave H. The security fix of 2026-11-22 adds E47. The file `vendor/chdash.patch` holds all marked edits, `tools/check-vendor-lock.sh --chdash` checks them, and the lock file has the new hashes. The next edit is E48. | Nothing goes upstream. |
| Security review | Done. Three independent reviewers worked on 2026-11-14, and three more on 2026-11-16. Each finding of 2026-11-16 was verified by a second reviewer who tried to refute it. All high and medium findings are fixed with tests (ID-K313, ID-K314). Three arbiters chose the new order of the decision on the write routes (ID-K312). The rows are to re-read | 2026-11-14: a quadratic name scan (ID-K280), a list cursor with hidden titles (ID-K281), an expansion with no byte budget (ID-K108a) and low issues. 2026-11-16, server: quadratic checks at save, lists with no cap, allocations before the caps and validations on the async threads, all before the decision. Page: quadratic number patterns on data, joins that allocated before the caps, and a quadratic sort of long decimals. The scope was F24, F25, F27, F28, F31, F41, F42, F43, the gzip encoder, the view form of Grafana and the engine edits E17 to E27. | Low and info findings stay open for the owner (ID-K313, ID-K314). After ID-K312, the conformance series U of ProcurAuth gives 60 passed, 0 failed and 50 skipped, as before. |
| Header order | Done in code. The owner accepted ID-K177 on 2026-11-14; ID-K178 is arbitrated, to re-read | No button of the view moves between the view mode and the edit mode. The order in edit mode is brand, crumbs, Add, Save, Revert, time control, Edit switch, theme and the menu. The four fixed controls have the same position at 1920, 1280, 900, 768 and 390 px, in both themes. The panel editor has a back link in the crumbs (ID-K178). Browser cases B150 and B151. | Nothing known. |
| Transparent fix | Done in code. ID-K179 is decided on the request of the owner of 2026-11-14 | The switch "Transparent background" of the panel editor changes the preview. The cause was a rule of `web/css/editor.css` that forced a transparent frame. The fix is in the CSS only. Browser cases B152 and B153 (the audit of 317 control changes on 10 panel types). | The audit B153 covers the options of the data types as F43 adds the forms. |
| Owner bugs of 2026-11-15 | Done in code. ID-K301 to ID-K303 are decided on the requests of the owner, to re-read | The charts no longer flash when the edit mode turns on or off (ID-K301, B207). The panel editor and its pane start under the header with the gap of the page (ID-K302, B208). The settings of a query take one row at 1280 px (ID-K303, B209). | Nothing known. |
| Compression | Done in code. ID-K304 is decided, to re-read | The binary compresses its static text files with gzip one time at start. The key `server.compress_static` (default `true`) turns this off. The encoder is Hune code (RFC 1951 and RFC 1952), with no new crate. The dashboard page goes from 1,035,927 to 326,225 bytes on the network. | No ETag for the static files. The shell is not compressed. |
| Parity captures | Done. ID-K306 to ID-K310 are decided or arbitrated in the absence of the owner, to re-read | The tool `tools/parity/visual.mjs` compares the canvas, the boxes, the titles and the queries with Grafana 12.4.2. Browser cases B101p, B137 and B148 run it. The list of open faults is empty. Fixes: the Grafana view form of a copy (ID-K307), the sign of E25 and the bar line (ID-K308), the top margin and the histogram y axis (ID-K310). | The comparison needs the disposable Grafana of 127.0.0.1:3010. |
| Wave A of 2026-11-22 | Done in code. ID-K327, ID-K332 and ID-K334 follow the owner answers of ID-K325, to re-read | The 11.23 question is asked again for each write route at each request, so a known uid and an unknown uid send the same number of PDP requests (ID-K327). The charts of a section draw in the task that expands it (ID-K332, B210). The stacks of a time series, a bar chart and a histogram follow Grafana 12.4.2, with the engine edit E33 (ID-K334). | A series with its own line interpolation keeps the interpolation of the panel (ID-K334). |
| Wave H, step 0 and step A | Done in code. ID-K331, ID-K336 to ID-K341 and ID-K343 are to re-read | The importer maps a data source variable with `--source-map`, moves the panels of a v1 row, and turns a Grafana default off only with an item (ID-K337, ID-K341). 30 more units (ID-K338). Six data link tokens of Grafana (ID-K339). The cell value inspect of a table (ID-K340). Three time forms of Grafana and `timezone` in the URL (ID-K336). The bar chart options `fill_opacity`, `line_width`, `axis_grid` and the orientation `auto`, with the engine edit E34 (ID-K343). In mode `authzen`, the entry redirect counts the filtered list (ID-K331). Browser cases B211 to B216. | No kiosk mode and no playlists (ID-K326). The URL time forms that give 1970 in Grafana stay invalid (ID-K336). |
| F39 | Done in code. ID-K330 is arbitrated, and ID-K345 is to re-read | The ClickHouse query parameters `{name:Type}`, in the strict form only, with a closed list of 8 types. The server checks each value by its type before it sends a byte to the source. It sends the pairs `param_<name>` with caps: 32 names, 1,000 items and 4,096 encoded bytes. The ClickHouse suite passes 34 of 34 cases. | ClickHouse sources only. |
| Annotations from PostgreSQL and MongoDB | Done in code. The owner said yes to ID-K142 (ID-K333); ID-K342 is to re-read | The member `annotations` takes PostgreSQL and MongoDB sources. The route, the caps, the PDP question and the page drawing are the same as for ClickHouse. The importer maps these items. Browser case B217. The container tests pass on PostgreSQL 15 and 18 and on MongoDB 7.0 and 8.0. | A number in the time column is epoch milliseconds for each kind of source (ID-K342). |
| Savings | Applied. ID-K350 is to re-read, as ID-K300 | The engine edits E35 to E42 remove engine code that no page code calls. The page goes from 1,023,019 to 937,315 bytes (-85,704). The test `engine_removals.test.mjs` keeps the removed names out. | Nothing goes upstream. |
| Wave H, step B | Done in code. ID-K328 and ID-K329 are arbitrated; ID-K346 to ID-K349, ID-K351 and ID-K352 are to re-read | The panel types `gauge` (ID-K348), `logs` (ID-K352) and `geomap` (ID-K349). The map is off by default: only the closed block `geomap` gives a tile source, and without the block the CSP does not change. The percent change and the text sizes of a stat (ID-K347). The gradient modes and the series options of a time series and a bar chart, with the engine edits E43 to E46 (ID-K346). Ad hoc filters with `=` and `!=` on the columns of the key `adhoc_columns` (ID-K351). Browser cases B218 to B227. | The ad hoc operators `<`, `>`, `IN`, `=~` and `!~` wait for the owner (ID-K329). |
| Page budget | Applied. The owner accepted ID-K211j on 2026-11-22 | The limit of 9.30 is 1,018 KB (1,042,432 bytes) for a measure of 1,037,796 bytes. It is above the ceiling of 1,000 KB and under the cap of 1,100 KB of the owner. | The owner can refuse the raise and give back the cost of the geomap, the series options or the gauge (ID-K211j). |
| Release 0.1.0 | Prepared. The owner decided ID-K335; ID-K344 is to re-read | The version 0.1.0 is in `Cargo.toml`, `Cargo.lock`, `hune version`, the page and the session. Each image of `tests/images.lock` has a digest. Two release builds of x86_64 give the same archive, byte for byte. The release workflow has the input `arm64` (17.3). `CHANGELOG.md` has the section of 0.1.0. | No aarch64 build was made. The coordinator pushes the repository and runs the release workflow. |
| Security review of 2026-11-22 | Done. All high and medium findings are fixed with tests (ID-K353 and ID-K356, to re-read) | Three independent reviewers examined the new surface of wave H: the server; the page and the engine; the importer, the format and the SQL properties P, PG, PM and PQ. A second reviewer verified each finding. One high finding: the ClickHouse lexer of Hune did not know the comments that start with `//`, the line ends of ClickHouse and the heredoc tags that start with a digit, so a value could leave its string. Medium findings: the place rule of `$__adhocFilters`, a second request for a failed map tile, and the tile bytes in the cache of the page. | Low and info findings that stay open for the owner: the display of hidden characters in the ad hoc chips, tiles that two large maps push out of the cache, and a `$__conditionalAll` condition with an open end beside `$__adhocFilters` (ID-K356). |
| Other browsers | Done for Firefox. ID-K309 is decided, to re-read | Firefox ESR 153 runs 14 cases through WebDriver BiDi (`HUNE_BROWSER=firefox`). | B170 and B171 are manual checks in Firefox. WebKit is a manual check: the host has no WebKit driver (16.19). |

## Decisions to read again

The owner is not yet on record for the rows below. The project arbitrated or decided them from 2026-11-13 to 2026-11-22, in the absence of the owner. The owner must read them again.

| Range | Subject |
|---|---|
| ID-K100 to ID-K119b | F29, the text panel |
| ID-K120 to ID-K136 | F30, the state timeline and the status history (ID-K132: the page size limit) |
| ID-K140 to ID-K158, ID-K155a, ID-K155b | F25, the annotations |
| ID-K160 to ID-K175c, ID-K166a, ID-K167a, ID-K170a | F24, several queries in one panel (ID-K172, ID-K172a: the page size limit) |
| ID-K176a, ID-K178 | The names of 6.14, and the back link of the panel editor |
| ID-K180 to ID-K195b | F31, the heatmap and the histogram (the plan ID-K211a replaces the budget row ID-K190) |
| ID-K200 to ID-K213 | F41, the closed or free value of a variable |
| ID-K211a to ID-K211g | The page size budget (ID-K211g: 986 KB, applied on 2026-11-16) |
| ID-K220 to ID-K233, ID-K230a to ID-K230d | F42, the syntax tree of the query editor, and the colour layer that draws the visible rows only |
| ID-K240 to ID-K254b | F28, the eleven transforms (the plan ID-K211a replaces the budget row ID-K252) |
| ID-K260 to ID-K270a | F27, the variables `interval` and `constant` |
| ID-K290 to ID-K298, ID-K295a, ID-K295b | F43, the option forms of the data types, and the rotation of the x labels |
| ID-K108a, ID-K280 to ID-K283 | The fixes of the security review |
| ID-K300 | The engine patch |
| ID-K97a | The fixes that the arbitration of F23 found |
| ID-K301 to ID-K305 | The bugs of the owner of 2026-11-15, the compression and the PostgreSQL container of the tests |
| ID-K306 to ID-K310 | The parity captures, the Grafana view form, the sign of E25, Firefox and WebKit, and the five parity items of the arbiters |
| ID-K311t, ID-K311a to ID-K311d, ID-K319 | The y ticks and the y range of a time series aligned on Grafana; the owner keeps the canvas of ID-K311a; a stack with negative values and the log scale stay open (19.20a) |
| ID-K315 to ID-K318 | The owner answers of 2026-11-16: static files `private`, the key `server.transform_max_cells`, and the fixes of the low findings |
| ID-K312 to ID-K314 | The security review of 2026-11-16: the order of the decision on the write routes, and the fixes of the server and of the page |
| ID-K211h, ID-K211i, ID-K320, ID-K322 to ID-K324 | The page budget of 2026-11-16 and 2026-11-21, the low findings of the importer, the stack below 0, the existence oracle of a 403 and the blank charts of B207 |
| ID-K327, ID-K331, ID-K332, ID-K334 | Wave A of 2026-11-22: the 11.23 question asked again, the entry redirect in mode `authzen`, the expand of a section and the stacks |
| ID-K328 to ID-K330 | The arbitrated rules of the geomap, the ad hoc filters and the ClickHouse query parameters |
| ID-K336 to ID-K343, ID-K345 to ID-K349, ID-K351, ID-K352 | The features of wave H, steps 0, A and B |
| ID-K350 | The engine savings of wave H |
| ID-K344 | The technical preparation of the release 0.1.0 |
| ID-K353 | The page fixes of the security review of 2026-11-22 |
| ID-K361 | The workflows stay as an option; `tests/ci/all.sh` and `tests/ci/release.sh` run the jobs and the release with no GitHub Actions |

Three arbiters decided these rows on 2026-11-15, in the absence of the owner: ID-K80, ID-K81, ID-K82, ID-K85, ID-K87, ID-K88, ID-K89, ID-K91, ID-K92, ID-K95, ID-K96 and ID-K97 (F23). The owner must re-read them. The rows ID-K83, ID-K84, ID-K86, ID-K90, ID-K93, ID-K94, ID-K176, ID-K177 and ID-K6 to ID-K66 have the owner's answer.

These items also wait for the owner:

- The decisions `ID-PG1` to `ID-PG17` (19.32) and `ID-MG1` to `ID-MG24` (19.33), marked `PENDING OWNER`.
- The low and info findings of the security reviews (ID-K313, ID-K314).
- The advisory RUSTSEC-2026-0387 of `crypto-bigint` 0.5.5 stays in the list `ignore` of `deny.toml` until `procurauth-core` moves to `p256` 0.14 (ID-K358).
- The origin of `web/icons.svg`, the commit pins of the workflow actions, and the toolchain of the fuzz job (ID-K344).
- WebAssembly in the page (ID-K333).

## How to build and test

Use these commands from the root of the repository. The toolchain file selects Rust 1.99.1.

1. Run `cargo build --workspace --locked`.
2. Run `cargo test --workspace --locked`.
3. Run `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
4. Run `cargo fmt --all -- --check`.
5. Run `node --test tests/frontend/*.test.mjs`.
6. Run `tests/ci/policy.sh --local`.
7. Run `python3 -I tools/ste_lint.py docs/*.md README.md CHANGELOG.md`.

The workflows in `.github/workflows` run the CI and the release on GitHub. They are optional. The owner can run every job and the release with `tests/ci/all.sh` and `tests/ci/release.sh`, with no GitHub Actions and no GitHub billing. The script `tests/ci/all.sh` runs every job of the merge gate in order and stops at the first failure (16.17).

These scripts need Docker. Each one starts local containers on `127.0.0.1` and removes them at the end.

| Script | Tests |
|---|---|
| `tests/ci/source.sh` | The ClickHouse cases of 16.9 for the source crate |
| `tests/ci/server.sh` | The panel query, options and preview routes against ClickHouse |
| `tests/ci/store-pg.sh` | The conformance suite and the PostgreSQL tests on PostgreSQL 15 and 18 |
| `tests/ci/pgwire.sh` | `serve`, `migrate-store` and the store commands on PostgreSQL 15 and 18 |
| `node tests/browser/run.mjs` | The browser cases of 16.10 with the local Chrome; `tests/browser/README.md` gives the range and the count (Firefox: `HUNE_BROWSER=firefox` for the cases of 16.10); set `PROCURAUTH_DIR` for B23, B35, B108, B124 and B214 |
| `tests/conformance/run.sh --local` | The series U of ProcurAuth; it needs a ProcurAuth checkout |
| `tests/load/run.sh` | The load test of 16.16 |
| `tests/ci/fuzz.sh` | The fuzz targets; it needs the nightly toolchain and cargo-fuzz |
| `tests/ci/nightly.sh` | The random seeds, and the jobs `source`, `pgsource` and `mongosource` again; `all.sh` does not run it |

## How to run Hune

1. Write a configuration file. Start from the output of `hune print-reference --format hcl --profile local`.
2. Run `hune check-config -c <file>`. Correct each error that it gives.
3. Run `hune serve -c <file>`.
4. Open the address of `server.listen` in a browser.

## Differences from the specification

| Paragraph | Difference | Decision |
|---|---|---|
| 4.24 | The regex subset refuses three more forms than the table. | ID-06 |
| 13.11, 13.17 | The key order of `access.pdp` follows 13.17. | ID-16 |
| 7.11 | An empty insertion also gets the token check. | ID-20 |
| 7.16, 8.3 | ClickHouse gives decimals without trailing zeros with the settings of 8.3. | ID-23 |
| 8.7 | The TCP keepalive idle time is the default of the system. | ID-25 |
| 9.3, 9.24 | The build accepts the file `web/js/vendor/chdash/LICENSE`. | ID-29 |
| 16.10 | The browser tests use Chrome through the DevTools protocol, not Playwright. | ID-40 |
| 16.10a, 16.17 | The front-end tests run with a file glob. | ID-32 |
| 19.20 | The gaps of the copied chart engine stay. | ID-33 |

The specification now states the look of the front end (ID-60 to ID-81). It also states the document format 3 (ID-75, ID-F2-1). The owner has not validated the formats 2 and 3 and their migrations yet. The text of 4.42 stays as the code has it until the owner decides.
