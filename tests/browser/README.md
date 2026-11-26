# Browser suite

This suite runs the browser cases B01 to B404 of section 16.10. It uses the real `hune` binary and a real ClickHouse. The range has 273 cases.

Three cases have a letter: B101p, B145a and B145b. These numbers are reserved and have no case: B119, B128 and B129, B138 and B139, B149, B154 to B159, and B178 and B179. Some cases also use a real PostgreSQL, a real MongoDB or the ProcurAuth tools (see "What you need").

## What you need

- Node 22. The suite has no package file and no npm dependency.
- Google Chrome at `/usr/bin/google-chrome`. Set `CHROME` to use another path.
- For a run in Firefox (optional): Firefox at `/usr/bin/firefox-esr` (set `FIREFOX` to use another path). Firefox 153 ESR gives WebDriver BiDi, and no other file is needed. See "Run in Firefox".
- Docker with the image `clickhouse/clickhouse-server:26.8.34.13` in the local cache. Set `DOCKER` to use another command.
- For the cases B44, B45 and B135: the local image `postgres:18`. B135 mixes a PostgreSQL source and a ClickHouse source in one panel.
- For the cases B47 and B48: the local image `mongo:8.0`, `openssl` and a build of `hune` that has the MongoDB driver. Without the driver (a build with `--no-default-features`), the runner reports B47 and B48 as `skip`.
- For the case B217: the local images `postgres:18` and `mongo:8.0`, `openssl` and the MongoDB driver. Without the driver, the runner reports B217 as `skip`.
- The debug binary. Build it with `cargo build -p hune --locked`.
- For the cases B23, B35, B108, B124 and B214: `openssl`, and the tools `fake-pdp` and `access-token` of ProcurAuth. B23, B35 and B214 test the mode `authzen`. B108 and B124 test the access to the text panel and to the annotations in this mode. Without the tools, the runner reports these five cases as `skip`.
- The default folder of the tools is `target/procurauth`. Set `PROCURAUTH_DIR` to use another checkout. The tools must be in `target/release/` of this folder.
- Build the tools from the committed revision that Hune pins in `Cargo.toml` (`rev = "f318e259..."`). Do not change the work tree of ProcurAuth. Run `mkdir -p target/procurauth`. Then run `git archive <revision> | tar -x -C <Hune>/target/procurauth` in the repository of ProcurAuth. Then run `cargo build --release --locked --offline -p fake-pdp -p access-token` in `target/procurauth`. The build needs no network when the dependencies are in the local cache.

Each case has a time limit. The default is 120 seconds. A case that needs more time sets `timeout` (in milliseconds) in its definition. The runner fails the case with "timeout of <n> ms" when the limit passes.

B153 sets 900000, B174 sets 400000, B176 sets 300000, and B175, B177, B207 and B210 set 200000. Set a higher `timeout` for a new long case. Do not raise the default.

Build the binary again after each change in `web/`. The binary embeds the files of `web/`.

## How to run

1. Run `node tests/browser/run.mjs` from the root of the repository.
2. To run some cases only, add `--only B01,B05`. The ids are separated by commas, with no space. The runner starts the container of PostgreSQL or MongoDB only when a selected case needs it.
3. Read the output. It has one line for each case: `ok <id> <name>`, `not ok <id> <name>: <reason>` or `skip <id> <name>: <reason>`.
4. The last line is a summary. The exit code is 1 when one case fails.

## Run in Firefox

1. Set `HUNE_BROWSER=firefox` and run `node tests/browser/run.mjs --only B01,B02,B03` from the root of the repository.
2. The runner then loads `lib/bidi.mjs` in place of `lib/cdp.mjs`. Without the variable, Chrome runs as before. Another value stops the runner.
3. The client starts `firefox-esr` with `--remote-debugging-port 0` and a disposable profile. It speaks WebDriver BiDi on the printed endpoint.
4. The cases that are known to pass: B01, B02, B03, B07, B08, B15, B16, B18, B103, B112, B122, B141, B163 and B172.
5. A CDP call that BiDi cannot give throws an error that starts with `unsupported on Firefox:`. B171 stops at `Input.imeSetComposition` for this reason.
6. Firefox has no media emulation in BiDi. `emulateColorScheme` sets the key `hune.theme` of localStorage and sends `hune-theme` (9.25). Print media is not supported.
7. Firefox shows only the selected tab. The client selects a page before each call on it, so that `document.hidden` is false and the timers run.
8. `Input.insertText` runs `document.execCommand("insertText")` on the focused element. WebDriver has no command for it.
9. The client pauses the requests of a page only after a rule (`failWhen`, `fulfillWhen`, `setHeaders`). Firefox 153 stopped to answer when it held many paused requests.
10. Set `HUNE_BIDI_TRACE=<file>` to write each BiDi message to a file.

## Run in an isolated copy

The runner takes its paths from the place of its own file. The binary is `target/debug/hune`. The work folder is `target/browser/`. Two runs in one tree use the same binary and the same work folder, and they disturb each other. A person who edits `web/` or the Rust code while another run is active uses an isolated copy.

1. Make a copy of the repository in `target/ws/<name>/`. Include the files that are not in git yet. Do not copy `target/` and `.git/`.
2. Run `cargo build -p hune --locked` in the copy. Add `--offline` when the host has no network. The copy has its own `target/debug/hune`. A build in a copy takes a full compile, because the copy has no cache.
3. Run `node tests/browser/run.mjs --only <ids>` in the copy. The copy has its own work folder `target/ws/<name>/target/browser/`.
4. The ports are free ports that the runner asks from the system for each server. The ClickHouse container and its port are new for each run, and the name has the pid of the runner. Two copies do not meet on a port or on a name.
5. For B23, B35, B108, B124 and B214, set `PROCURAUTH_DIR` to the tools of the main tree. The copy has no `target/procurauth`, and these cases are `skip` without it.
6. Stop only the processes that you started. Use the pid that you recorded. Do not stop a process by its name.

Build the binary of the copy again after each change in `web/` or in the Rust code. The runner removes the container, the servers and Chrome at exit, also after a failure.

## What the runner does

1. It starts a ClickHouse container with the name `hune-test-browser-ch-<pid>`. The port is bound to 127.0.0.1. The seed data come from `tests/fixtures/clickhouse/init.sql`.
2. It makes one volume for each server under `target/browser/`. The folder `transit` holds the dashboard `nodes`. A volume holds no file that is not valid, because the start stops for such a file.
3. It writes a configuration from `hune print-reference --format hcl --profile local`. It sets the ports, the volume, the ClickHouse URL and the password file. It removes the `tls` block. It adds the source `zdown`, which is always down.
4. It starts three servers: one with a primary volume, one with no primary store, and one with `server.base_path = "/dashboards/"`. Each of the three sets `server.list_filters_min_dashboards = 0`, so the sort control and the tag filter show on a volume of two items. B228 and B34 start servers of their own that use the default or another value.
5. It starts one Chrome and drives it with the DevTools protocol. Each case has its own browser context.
6. It runs the cases. A case fails on a console error, an uncaught exception or a CSP violation, unless the case provokes it on purpose.
7. It stops the servers with SIGTERM, removes the container and kills Chrome. This also happens when a case fails or when you press Ctrl+C.

The cases B28 to B35 test the entry redirect of the list page (9.7). Each case starts a server with its own volume. The volumes hold 0, 1 (in a folder or not) or 2 or more dashboards. Other cases use one item that is not valid, a lost access (an injected `403`), or a search text `q`. B35 tests the mode `authzen` with the subjects `solo`, `nobody` and `reader`.

The cases B36 to B42 test the first set of Grafana-parity features. They use synthetic SQL only (`numbers(...)` or the seed table).

- B36: the option `justify` of a `stat`. The test reads `data-align` of each item, the computed style and the position of the texts.
- B37: hidden series. The matchers `names` (`include` and `exclude`) with `hide_from`. The test counts the pixels of the series colour on the canvas, and it reads the legend, the tooltip and the y axis. A legend click must change the page only: the document and its etag stay the same.
- B38: `thresholds_style` in a `timeseries`, a `barchart` and a horizontal `barchart`, with the modes `absolute` and `percentage`. For the canvas charts, the test reads the attribute `data-thresholds` of the plot element (the hook of 9.29). For the horizontal chart, it reads the elements `hbar__thr` and `hbar__band`.
- B39: a dashboard with `timezone = "utc"` in Chrome with the zones `Asia/Tokyo` and `America/New_York`. The runner sets the zone with `Emulation.setTimezoneOverride`. The test records the axis labels with a wrapper of `fillText` (`recordCanvasText` in `lib/cdp.mjs`).
- B40: the table footer with one reducer, with two reducers and with a filter. Also `wrap`, the `gauge` cells in three modes, and the `json` cell with an injection value.
- B41: the CSV file of "Download CSV". The test reads the file in the download folder of the case, and it listens for `securitypolicyviolation`.
- B42: titles with variables, in a panel and in a section.

The case B43 tests drag and resize of the panels in edit mode (10.5a).

The cases B44 and B45 test the PostgreSQL source in the page. They use synthetic data only (`tests/fixtures/postgres-source/init.sql`).

- B44: a table with `int8`, `numeric` and `timestamptz` columns, and a time series with `$__timeGroupAlias`. The case also tests a refused function in the editor and the kind of each source in the source picker.
- B45: the scanner of 9.10 reads a variable after `#`, and a change of the variable runs the panel again.

The cases B44 and B45 need the local image `postgres:18`. The module `cases/pgsource.mjs` starts a disposable container with `startPostgres` and removes it at exit. It starts the container only when the run selects B44 or B45. The container has the name `hune-test-browser-pg-<pid>`, and its port is bound to 127.0.0.1.

Each call of `startPostgres` takes a tag, and the tag is part of the container name and of the folder. B135 uses the tag `mix135`, so its container is `hune-test-browser-pg-<pid>-mix135`. A second call with the same tag throws. Without a tag, a second call would remove the container of the first call. Before B44 and B45 run, the case makes sure that its container still runs.

The case B46 tests the item "Inspect" of the panel menu (9.11). It starts two servers of its own with the option `queryInspect` of `lib/env.mjs`. With `server.query_inspect = true`, it reads `query_inspect: true` in the session, opens the dialog `inspect-<id>` and reads the expanded SQL in `inspect-<id>-sql`. It also checks the button `inspect-copy`. With `false`, it checks that the item `panel-<id>-inspect` is absent and that the answer of a query has no member `sql`.

The cases B47 and B48 test the MongoDB source in the page. They use synthetic data only (`tests/fixtures/mongo-source/init.js`).

- B47: a table with Int64, Decimal128 and Date values, and a time series with `$dateTrunc` and `$__interval_s`. The case also tests a refused stage (`$out`) in the editor and the kind of each source in the source picker.
- B48: an empty result, a nested document, a time series with no Date column, and the scanner of 9.10 on the string `"${n:number}"`.

The module `cases/mongo.mjs` asks the binary first. It runs `hune check-config` on a configuration with one `mongodb` block. A build with no MongoDB driver refuses the block (8b.2): the exit status is not 0 and the message holds "no MongoDB driver". The runner then reports B47 and B48 as `skip`, with the reason "no MongoDB driver in this build". Another failure of this probe fails the case.

A build with the driver gets a disposable container from `startMongo`. The container has the name `hune-test-browser-mongo-<pid>`, with TLS made by `openssl`, and its port is bound to 127.0.0.1. The module starts it only when the run selects B47 or B48, and it removes the container at exit.

The cases B69 to B71 and B90 test the header (9.6, 9.9, 10.2). They use synthetic dashboards, and B70 and B71 also start a server with no primary store.

- B69: the joined time control. The groups, the buttons that join, no second refresh button, and the keyboard.
- B70: the Edit switch (`role="switch"`, `aria-checked`, Space and Enter, the right to write). "Add", "Save" and "Revert" in edit mode. The confirmation of "Revert". "Save" that is not active with no change.
- B71: one header row of 44 px at 1920, 1280, 768 and 390 px, in light and dark. The case checks the view, edit mode, the panel editor and the list. It also checks the items of "More actions" by rights and the confirmation of "Delete".
- B90: the split button "Add". One click adds a panel. The menu has "Add section" and "Place a variable in the grid". This item opens a menu of choice, places the control at once, or is not active. The icon only at 640 px, and the keyboard.

The cases B80 to B86 test the placement of a variable in the grid (4.7a, 9.10, 9.26, 10.3b, 10.6). They use synthetic dashboards and the seed table.

- B80: the controls in their frames, the bar with the other variables, the effects of a change, the number grammar and a reload.
- B81: the row under the header with no height when no variable is left, and no move at the first frame.
- B82: the keyboard, the accessible names and the scripted checks in both themes. The case does not run axe-core, which needs the network.
- B83: `viewPanel`, the panel editor, a collapsed section and print.
- B84: the editor. The switch "Place in the grid", the fields, the menu of the frame, the keys, the sections, and a save and a reload.
- B85: the layouts at four widths and at the zoom levels of B55.
- B86: the format and the server. `bad_reference` and `not_permitted` give `422`, and a document with no placement keeps its form.

The case B91 tests the colour theme of the chrome (9.25, 9.26). It reads the file `00-tokens.css` of ClickHouse Dash from `web/css/vendor/chdash/` and compares each computed colour with it. It also checks the contrast of WCAG 2.2 AA.

The case B92 tests the time range picker of ClickHouse Dash (9.9). It checks the popup, the quick ranges, the absolute fields, the calendar and the keys. It also checks the motion of 160 ms, the reduced motion and the narrow layouts.

The case B23 starts its own server in the mode `authzen` and its own `fake-pdp`. It makes a key pair with `openssl` and the identity tokens with `access-token`. It sends the token with `Network.setExtraHTTPHeaders`, because the edge is absent. It changes the PDP state during the case: it stops the PDP and it starts it again.

## Files

| Path | Content |
|---|---|
| `run.mjs` | The runner and the case context. |
| `lib/cdp.mjs` | The DevTools client: pages, clicks, input, waits, request log. |
| `lib/bidi.mjs` | The WebDriver BiDi client for Firefox, with the same classes and methods (see "Run in Firefox"). |
| `lib/env.mjs` | The ClickHouse container, `startPostgres`, `startMongo`, `probeMongoDriver`, the volumes, the configuration, the servers and the API helper. Two callers of `startPostgres` or of `startMongo` in one run give two different tags. |
| `lib/authzen.mjs` | The mode `authzen` of B23: keys, tokens, the policy of `fake-pdp`, and its start and stop. |
| `lib/docs.mjs` | Builders of synthetic dashboard documents. |
| `lib/preview_fixtures.mjs` | A fixture dashboard with one panel of each of the 14 queried types, on the seed table. |
| `lib/preview_measure.mjs` | The measurement harness of the previews: probes, comparison by tolerance class, table. See "Measure the previews". |
| `preview_measure.mjs` | The command of the harness. It measures a dashboard of a running server. |
| `lib/parity.mjs` | The visual parity capture of 15.14c (`tools/parity/visual.mjs`) for B101p, B137 and B148. It runs only when the Grafana of 127.0.0.1:3010 answers. It reads `tests/fixtures/parity/visual-open-faults.json`. |
| `lib/ed.mjs`, `lib/h.mjs` | Helpers of the editor cases and of all cases. `h.mjs` also reads the canvas texts, the tooltip and the data of the threshold hook. |
| `cases/*.mjs` | The cases. Each case has an id, a name and one function. |
| `cases/home.mjs` | The cases B28 to B35: the entry redirect of the list page (9.7). B34 tests `q` as a tag filter. |
| `cases/entry_one.mjs` | The case B214: the entry redirect in mode `authzen`, with the list of the display filter (9.7, 11.25). |
| `cases/features_f1.mjs` | The cases B36 to B42: the first set of Grafana-parity features. |
| `cases/editor_drag.mjs` | The cases B43 and B229: drag and resize in edit mode (10.5a), and what the edit mode leaves on the frames (10.16). |
| `cases/pgsource.mjs` | The cases B44 and B45: the PostgreSQL source in the page. |
| `cases/states.mjs` | The cases B72 to B79 and B87 to B89: the states of the page and their transitions (10.2a). |
| `cases/inspect.mjs` | The case B46: the item "Inspect" and the key `server.query_inspect` (9.11). |
| `cases/inspect_value.mjs` | The case B213: the drawer "Inspect value" of a table cell, its keys and its targets (9.20, 4.18). |
| `cases/mongo.mjs` | The cases B47 and B48: the MongoDB source in the page. |
| `cases/header_k.mjs` | The cases B69 to B71, B90, B150 and B151: the header (9.6, 9.9, 10.2). |
| `cases/var_panel.mjs` | The cases B80 to B86: the placement of a variable in the grid (4.7a, 9.10). |
| `cases/theme_k.mjs` | The case B91: the colour theme of the chrome (9.25, 9.26). |
| `cases/picker_k.mjs` | The case B92: the time range picker of ClickHouse Dash (9.9). |
| `cases/viewer_a.mjs`, `cases/viewer_b.mjs` | The cases B01 to B13, B24 to B27 and B233: the viewer, the page of one panel without a scrollbar, the list (cards, tags, no search field) and the base path. |
| `cases/list_cards.mjs` | The case B228: the cards of the list page, the threshold of the filters, the folder titles, the grid, the three dots and the menu of a card, and the read-only card (9.7). |
| `cases/stores.mjs` | The cases B241 to B244: a read-only store (`makeLabVolume`), the lock, the warning of a file that is not valid after the start (`breakFile`), the header selector and the refusal of a write. |
| `cases/loaded_pie.mjs` | The case B400: the circle of the loaded pie chart equals the real box with its fraction of a pixel, at five widths and zoom levels (9.19a, 9.29). |
| `cases/panel_preview.mjs` | The cases B320 and B321: the loading preview of a panel (9.11). The queries are held, so the preview stays. B320 measures the preview of each of the 14 queried types with the harness, then measures the loaded panels. B321 tests the light, the reduced motion and the reload. They need the hold of a request, so they are `skip` with Firefox. |
| `cases/preview_rings.mjs` | The cases B370 to B377: the loading preview of the pie chart and of the map (9.11). The fixtures are in `lib/preview_rings_fixtures.mjs`. The cases measure 14 pies and 6 maps against the loaded panels, check that the preview draws no data, follow a resize, test the warning line of a draft, and save captures in the light and the dark theme (`HUNE_PREVIEW_SHOTS` sets the folder). They need the hold of a request, so they are `skip` with Firefox. |
| `cases/charts_spec.mjs` | The cases B380 to B384: the chart panels against 4.26 and 9.19. The table legend with no value column, the hidden axes, a right legend that every entry leaves, the cap of the narrow fallback, the x axis of bars, and the legend of horizontal bars. |
| `cases/loaded_panels.mjs` | The cases B390 to B392: the facts of the loaded `logs` and `table` panels that the loading preview copies. B390 and B391 measure the lines of the logs panel with `wrap` and with empty cells. B392 measures the header of a table on a fine and on a coarse pointer. |
| `cases/var_states.mjs` | The cases B290 to B296: the states of the control of a query variable (loading, error, wait), a failure that blocks nothing, the cache of the options, the cancel of a request, and the chips of the selected values (9.10). They need the hold of a request, so they are `skip` with Firefox. |
| `cases/root_dashboards.mjs` | The cases B300 to B305: the dashboards with no folder. The list with a root group, the dialogs New dashboard, Import and Move with the entry "Root (no folder)", the header selector, and a server with no primary store. `addRootDashboard` of `lib/env.mjs` makes a file at the root of a volume; the options `prepare` and `labPrepare` of `startServer` call it. |
| `cases/json_view.mjs` | The cases B240, B270 and B271: the JSON view of the edit mode, its colours and their contrast, the marks of the gutter, the revert, the full height at four sizes and the caps of the colour layer; the look of VS Code (the palettes, the selection, the scroll bar); the band of the caret line, the boxes of the brackets, the guides and the wavy line of an error |
| `cases/editor_a.mjs`, `cases/editor_b.mjs` | The cases B16 to B22 and B230: the editor, conflict, history, import and export, delete, no editing, the id of a new folder. |
| `cases/misc.mjs` | The cases B14 and B15: the scripted accessibility audit. |
| `cases/authzen.mjs` | The case B23: the mode `authzen` with `fake-pdp`. |
| `cases/shell.mjs` | The cases B49 to B54: the header bar, the theme button and the version line. |
| `cases/zoom.mjs`, `cases/flash.mjs` | The case B55 (browser zoom), and the cases B58 and B59 (one stable render, fixed button sizes). |
| `cases/editor_fix.mjs` | The cases B56 and B57: fixes of the query editor. |
| `cases/charts_axes.mjs`, `cases/charts_colors.mjs`, `cases/charts_zone.mjs` | The cases B60 to B63: the y axes, the series style, the colour modes and the zone names. |
| `cases/time_override.mjs`, `cases/time_links.mjs` | The cases B64 and B65: the time override of a panel and the dashboard links. |
| `cases/editor_forms.mjs` | The cases B66 to B68: the forms of the panel editor and of the settings view. |
| `cases/repeat.mjs` | The cases B93 to B101 and B101p: the repeat of panels and sections by a variable (F23). B101p is the parity capture of the copies. |
| `cases/text.mjs` | The cases B102 to B109: the text panel in Markdown (F29). B108 uses `fake-pdp`. |
| `cases/timeline.mjs` | The cases B110 to B118 and B395: the state timeline and the status history (F30). |
| `cases/annotations.mjs` | The cases B120 to B127: the annotations from a ClickHouse query (F25). B124 uses `fake-pdp`. |
| `cases/queries.mjs` | The cases B130 to B137: several queries in one panel (F24). B135 uses PostgreSQL. |
| `cases/editor_audit.mjs` | The cases B152 and B153: the switch "Transparent background" and the audit of the options of the panel editor. |
| `cases/open_vars.mjs` | The cases B160 to B169: the closed or free value of a variable (F41). |
| `cases/ast.mjs` | The cases B170 to B177: the syntax tree of the query editor (F42). |
| `cases/transforms_more.mjs` | The cases B180 to B189: the eleven more transforms (F28). |
| `cases/interval_vars.mjs` | The cases B190 to B199: the variables of the kinds `interval` and `constant` (F27). |
| `cases/editor_options_a.mjs`, `cases/editor_options_b.mjs`, `cases/editor_options_c.mjs` | The cases B200 to B206: the option forms of the data types (F43). |
| `cases/edit_flash.mjs` | The case B207: the charts stay painted across the switch "Edit", the panel editor, "Back" and "Forward" (10.2a, rule 17). |
| `cases/section_expand.mjs` | The case B210: the charts of a section draw in the task that expands it (9.8, 10.2a, rule 17). |
| `cases/time_defaults.mjs` | The cases B231 and B232: the range and the refresh that a person sets in edit mode stay after it, and the Save dialog offers them as the defaults (10.9). |
| `cases/url_time.mjs` | The case B212: a link of Grafana 12 with an ISO range and `timezone` opens with that range and that zone (9.5). |
| `cases/adhoc.mjs` | The cases B218 and B219: the ad hoc filters of an `adhoc` variable, the bar control, the address of Grafana and the refusals (9.10a). A server of the case has `adhoc_columns` (option `chLines` of `startServer`). |
| `cases/units_h.mjs` | The case B215: the added units in a stat and in a table, with the texts of Grafana 12.4.2 (4.20, 9.15). |
| `cases/barchart_p03.mjs` | The case B216: the fill, the line, the grid lines and the orientation `auto` of a bar chart, with the values of Grafana 12.4.2 (4.18, 4.30, 9.19). |
| `cases/annotation_kinds.mjs` | The case B217: the annotations from a PostgreSQL source and a MongoDB source (4.5b, 9.19d, 10.3c). It starts its own containers with the tag `an217`. |
| `cases/gauge.mjs` | The case B225: the gauge, with the rules of the classic gauge of Grafana 12.4.2, at three sizes and in both themes (4.33e, 9.19g). |
| `cases/stat_percent.mjs` | The case B222: the percent change and the text sizes of a stat in both themes, with the values of Grafana 12.4.2 (4.29, 9.19). |
| `cases/series_p10_p14.mjs` | The cases B223 and B224: the gradient modes of a time series and a bar chart in both themes, and the cosmetic series options (the dash array, log base 2, the null threshold, the bar alignment, the centred zero, the width, the border and the colour of an axis), with the values of Grafana 12.4.2 (4.18, 4.28, 4.30, 9.19). |
| `cases/logs.mjs` | The cases B220 and B221: the logs panel in both themes (the time, the level colours of Grafana 12.4.2, the order, wrap, the details row and its keys), and hostile lines, the caps of lines and characters, "No data" and the save that refuses the format `logs` in a table (4.33f, 7.16e, 9.19h, T104). |
| `cases/geomap.mjs` | The cases B226 and B227: the map with no tile source (the plain background, the markers, a hostile name as text in the card, the zoom, no request off the page origin, the header of 12.8 byte for byte) and the map with a local tile server on 127.0.0.1 and a Chrome of the case alone (the header with the tile source in `img-src` only, the session, no `Referer`, no cookie, z/x/y only, 6 requests in flight, the delay after a pan, a tile of 512 px dropped, CORS fail closed, one request for each failed tile through refreshes of 10 s, a pan that survives a refresh, and the memory of 24 tiles of 16 MiB). It needs `openssl` and `/proc` (Linux). |
| `lib/audit_a.mjs`, `lib/audit_b.mjs`, `lib/audit_c.mjs`, `lib/audit_rules.mjs` | The closed lists and the verdict rule of the audit B153. |

Chrome profiles, server logs, downloads and failure screenshots are in `target/browser/`. A failed case keeps `artifacts/<id>-<n>.png` and its console log.

## Measure the previews

The harness measures the preview of each panel against the loaded panel (9.11). It does not start a server. It needs a Hune server that is on, a Chrome and a dashboard.

1. Run `node tests/browser/preview_measure.mjs --url http://127.0.0.1:8080 --uid <uid>`.
2. The command holds the queries of the panels. Each panel keeps its preview.
3. The command reads the parts of each preview. They are the elements with `data-skel-part`. Each part has a class in `data-skel-class`: `exact`, `least`, `most` or `data`.
4. The command releases the queries. It waits for the panels, and then it reads each loaded panel with the extractor of its type.
5. The command prints one row for each part. A row has the preview rectangle, the loaded rectangle, the delta and the verdict.

The verdicts:

- `ok`: the part agrees with the loaded panel in its class. An `exact` part differs by 0.5 px at most in x, y, width and height. A `least` part is not larger than the loaded part. A `most` part is not smaller.
- `fail`: the part differs more than the tolerance of its class.
- `no-loaded`: the preview claims a part that the loaded panel does not have.
- `unclaimed`: the loaded panel has a part that the preview does not claim. These rows are the targets of a family.
- `data`: the class `data` gives no shape, and the harness does not compare it.

All rectangles are `[x, y, w, h]` in px from the top left corner of the content box of the panel body. The options are `--ids 1,2`, `--all` (print the labels), `--json <file>`, `--width`, `--height`, `--query` and `--strict` (exit code 1 for a `fail` or a `no-loaded`). The default viewport is 1440 by 4000 px, because the page loads only the panels near the viewport.

The harness reads the text of the axis labels with a recorder of `fillText`. The recorder is a script that runs at the start of each document.

## Limits of this suite

- The case B14 runs scripted checks only. It does not run axe-core, because axe-core needs the network to install. Run the axe part in the CI image.
- The case B23 injects the move conflict (10.10) with a `409` answer. A save after a plain move does not give this conflict, because the server decides on the folder that it reads at the request.
- The suite runs in Chromium. A subset runs in Firefox (see "Run in Firefox"). WebKit has no driver on the test host, so WebKit is a manual check (docs/07, 16.19).
