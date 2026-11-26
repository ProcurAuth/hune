# Part 4: Front end and editor

Part 4 of the Hune specification (see `01-overview.md`).

This part holds section 9 (the front end) and section 10 (the editor, the history and the conflicts). Section 6 gives
the routes that the page calls. Section 4 gives the document members that the page shows and edits. Section 11 gives
the display filter.

## 9. Front end

### 9.1 Principles

- The front end is native ES modules (ES2022), HTML and CSS. It has no framework, no bundler, no npm and no build step.
  The files in `web/` are the files that the browser runs.
- The page calls only the API of section 6, with relative URLs.
- The page puts text from a dashboard or a result in the DOM only with `textContent`, attribute setters or a canvas. Hune code never uses `innerHTML`, `outerHTML` or `insertAdjacentHTML` with such text. The copied modules of
  9.24 escape text with `util.escapeHtml` where they make HTML.
- The page is in English only. It has no translation framework.
- The look of the dashboard view follows Grafana 12.4.2. The values in 9.5 to 9.25 come from measures on a local Grafana. It has the same dashboard and the same data. Where a Grafana value fails WCAG 2.2 AA (9.26), Hune uses the nearest value that passes. The colour theme of the chrome is the exception: it is the theme of ClickHouse Dash (9.25).
- The data functions of 9.9, 9.10, 9.12 to 9.23, 10.5 and 10.15 are pure functions in modules with no DOM access. A
  Node layer of tests runs them with golden vectors, in the test image only (16). The table below gives the vector id
  of each function.
- Each read and write of `localStorage` is in a `try` block. A failure never stops the page.
- Each regular expression of a document is in the common subset of 4.24: no POSIX class, no `\A`, `\z` or `\p`, and
  ASCII classes only. The page compiles a pattern with `new RegExp(body, flags)`, with the flags of the pattern only. It
  never adds the flag `u`.

| Vector id | Function | Paragraph |
|---|---|---|
| `FE-01` | Date math: a time text to epoch milliseconds, in each zone | 9.9 |
| `FE-02` | Options of a `query` variable: regex, duplicates, sort | 9.10 |
| `FE-03` | Frame to fields | 9.12 |
| `FE-04` | Display names | 9.13 |
| `FE-05` | Field configuration and matchers | 9.14 |
| `FE-06` | Units and decimals | 9.15 |
| `FE-07` | Colours, palette and thresholds | 9.16 |
| `FE-08` | Value mappings | 9.17 |
| `FE-09` | Data links: tokens, encoding, final URL check | 9.18 |
| `FE-10` | Reduce calculations | 9.19 |
| `FE-11` | Table sort and column filter | 9.20 |
| `FE-12` | `filter_by_value` | 9.21 |
| `FE-13` | `grouping_to_matrix` | 9.21 |
| `FE-14` | Long-to-wide | 9.22 |
| `FE-15` | Panel reuse | 9.23 |
| `FE-16` | Layout normalization | 10.5 |
| `FE-17` | Line diff | 10.15 |

### 9.2 File layout of `web/`

```text
web/
  shell.html                 the page of each page route (9.6)
  favicon.svg                the icon of the browser tab
  icons.svg                  the sprite of the icons (main.js puts its symbols in the page, 9.6)
  fonts/
    inter/                   InterVariable.woff2 and the LICENSE of Inter (9.25)
  css/
    fonts.css                the font face of Inter (9.25)
    tokens.css               tokens of Hune on top of the copied tokens: type, geometry, roles, series colours (9.25)
    look.css                 the tokens of the calm look (surfaces, accent, buttons, fields, tag chips) in the four theme contexts (9.25)
    base.css                 reset, layout of the shell, header, focus rules
    components.css           buttons, menus, dialogs, forms, theme button, header controls
    picker.css               the time picker and the refresh picker, the popup of the time range (9.9)
    var-select.css           the states, the error and the chips of the control of a variable (9.10)
    list.css                 list page: cards, folder titles, tag chips, header controls (9.7)
    dashboard.css            grid, sections, panel frames, colour tokens of 9.16
    panel-table.css          table panel (9.20)
    panel-chart.css          time series, bar chart and xy chart panels (9.19)
    panel-stat.css           stat, bar gauge, gauge and pie chart panels (9.19, 9.19g)
    panel-variable.css       the frame of a placed control (9.10, 9.11)
    panel-text.css           the text panel (9.19b)
    panel-timeline.css       the state timeline and the status history panels (9.19c)
    panel-bucket.css         the heatmap and histogram panels: the colour bar, the card, the message of a cap and the glyph of the histogram (9.19e, 9.19f)
    panel-logs.css           the logs panel: the lines, the level colours, the details row and the glyph of the type (9.19h)
    panel-annotations.css    the marks layer, the card and the keys of the annotations (9.19d)
    panel-loading.css        the dots and the preview of a panel that waits (9.11)
    editor.css               edit mode, panel editor, history, diff (loaded with the editor)
    vendor/chdash/           copied CSS of 9.24
  js/
    boot_theme.js            classic script: sets data-theme before the first paint (9.25)
    theme.js                 theme button of the header (9.25)
    header_layout.js         one row or two rows of the header (9.6)
    ui.js                    elements, menus, dialogs, notices
    pickers.js               the time control of the header and the refresh picker (9.9)
    time_picker.js           the range button and its popup, with the calendar (9.9)
    time_calendar.js         the days of a month, a typed duration and the recent ranges, pure (9.9)
    display.js               display text and colour of a value (9.14 to 9.17)
    main.js                  entry, session, router (9.5, 9.6)
    api.js                   each route of section 6, abort, error body (6.7)
    url_state.js             read and write of the URL contract (9.5)
    view_state.js            the state of the page from the URL, the URL of a state, and the table of transitions (10.2a)
    time.js                  date math, quick ranges, time zones (9.9)
    zone.js                  the text of a time in a zone: `formatTime(ms, zone, pattern)` (9.15, 9.19)
    runtime.js               time range, variable graph, query scheduler, results (9.8, 9.10)
    vars.js                  variables bar and the control of a variable (9.10)
    var_errors.js            the short sentence of a failed options request, by code of the API and by ClickHouse number (9.10)
    options_cache.js         the cache of the options: key, period, time of life, size (9.10)
    var_chips.js             the chips of a selection and the count of the chips that fit (9.10)
    adhoc.js                 the ad hoc filters: the URL form, the checks and the control of an adhoc variable (9.10a)
    var_layout.js            the placement of a variable: placed items of the grid, the home of each control (4.7a, 9.10)
    markdown.js              the Markdown parser: lines to a tree, with limits and no DOM access (9.19b)
    timeline_model.js        the states, the segments, the cells and the rules of the caps of 9.19c: pure functions with no DOM access
    bucket_model.js          the shared helpers, the bucket maths and the caps of the histogram (9.19f): pure functions with no DOM access
    heat_model.js            the grid, the cells, the colour scale and the axis of the heatmap (9.19e): pure functions with no DOM access
    geo_model.js             the tile URL grammar, Web Mercator, the tiles of a view and the caps of the map (9.19i): pure functions with no DOM access
    annotations.js           the annotations of a dashboard: which one applies to a panel, the check of an answer, the marks, the cap and the hit test: pure functions (4.5b, 9.19d)
    repeat.js                the values, the copies, the keys and the layout of a repeat: pure functions (4.13a, 9.8c)
    queries.js               the queries of a panel, the join of frames, the display prefix and the state of a panel: pure functions (4.15a, 9.8d, 9.22a)
    sqlscan.js               placeholders of a SQL text or a JSON text, with app_sql_lex.js, sql_lex_pg.js and a JSON reader: only the editor imports it (9.10, 7.19)
    sql_lex_pg.js            lexer of the dialect `postgres` (7.20), only for sqlscan.js
    grid.js                  sections and grid (9.8)
    panel_frame.js           panel header, menu, states, View data (9.11)
    panel_wait.js            the wait of a panel: first load and reload, the test of the same data: pure functions (9.11)
    preview/                 the loading previews (9.11)
      index.js               the registry: the family of each panel type, the build and the draw of a preview
      common.js              the shared pure helpers: nodes, rectangles, bounds, width of a text
      <family>.js            one module for each of the 12 families (timeseries, barchart, piechart, stat, gauge, bargauge, table, heatmap, histogram, timeline, logs, geomap)
    frame.js                 frame to fields (9.12)
    names.js                 display names (9.13)
    fields.js                field configuration and matchers (9.14)
    units.js                 units and decimals (9.15)
    colors.js                named colours, palette, thresholds (9.16)
    mappings.js              value mappings (9.17)
    links.js                 data links (9.18)
    panel_time.js            range of a panel with a time override, and its note (9.8b)
    dash_links.js            URLs and items of the dashboard links (9.8a)
    transform.js             filter_by_value, grouping_to_matrix, long-to-wide, reuse (9.21 to 9.23)
    transform_more.js        organize, calculate_field, sort_by, limit, merge, reduce, partition, join, filters, renames, conversions (9.21a)
    panels/                  table.js, timeseries.js, stat.js, barchart.js, barchart_h.js, piechart.js, xychart.js, bargauge.js, text.js, timeline.js, heatmap.js, histogram.js, gauge.js, geomap.js
                             and their helpers: common.js, series.js, items.js, annotate.js
                             variable.js: the frame of a placed control (9.11), not a type of panel
                             text.js: the DOM of the Markdown tree and the body of a text panel (9.19b)
                             timeline.js: the canvas, the legend, the tooltip and the text alternative of the types `state_timeline` and `status_history` (9.19c)
                             heatmap.js: the cells, the colour bar, the card and the text alternative of the type `heatmap` (9.19e)
                             histogram.js: the bars, the legend and the card of the type `histogram` (9.19f)
                             logs.js: the lines, the levels and the details row of the type `logs` (9.19h)
                             gauge.js: the cells, the canvases and the titles of the type `gauge` (9.19g)
                             geomap.js: the tiles, the plain background, the markers and the card of the type `geomap` (9.19i)
                             annotate.js: the canvas of the marks, the card, the keys and the note of the annotations of a time series (9.19d)
    list.js                  list page (9.7)
    editor/                  modules of section 10, loaded only on demand
      edit_mode.js  panel_editor.js  json_editor.js  layout.js  draft.js  save.js
      conflict.js  diff.js  history.js  import_export.js  folders.js  drag.js  text_check.js
      props.js  forms/       the property catalog and the forms of 10.3a and 10.3b
      place.js               the menu of a placed control in the draft grid (10.6)
      forms/var_examples.js  the SQL examples of a `query` variable and the preview of its options (10.3b), loaded with the forms
      forms/option_form.js   the renderer of the option forms: it reads one table of rows and builds the groups of a type (10.3h)
      forms/options/         one table of rows for each type: timeseries.js barchart.js xychart.js stat.js bargauge.js piechart.js
                             table.js heatmap.js histogram.js gauge.js logs.js geomap.js. rows.js (the pure helpers: the row model, the write rule, the rules of
                             display, the shared rows of the legend and the tooltip) and index.js (the registry). A table imports rows.js only.
      sql_editor.js          the SQL text area (10.3); sql_fold_view.js: the folded view, the markers, the band and the pair marks (10.3e, 10.3f)
      ast/                   the minimal syntax tree of the query text, pure modules with no DOM access (10.3c):
                             nodes.js  tokens_ch.js  tokens_pg.js  sql_tree.js  json_tree.js  format.js  structure.js
    vendor/chdash/           copied modules, shim modules, LICENSE, chdash.lock.json (9.24)
```

The editor loads the files `forms/option_form.js` and `forms/options/` with the other modules of `js/editor/`, on demand.

A paragraph of section 9 or 10 can need a new module. This module MAY go in `js/` or in one of its directories. Each
file of `web/` stays in one of the directories of this layout.

### 9.3 Embedding, build hash and hashed URLs

- The `build.rs` of the crate `hune` reads each file of `web/`, in byte order of the path. It embeds each file with
  `include_bytes!` and computes its SHA-256.
- A symbolic link, or an extension outside the table below, stops the build.
- The build list is a text with one line `<path> <sha256>` for each file, in path order. Each line ends with LF. The
  build hash is the first 16 hexadecimal characters of the SHA-256 of the build list.
- A static file has the URL `static/<build-hash>/<path>`, relative to the `<base href>` of the shell (9.6). A relative
  import in a module names a file under the same prefix, so no import is rewritten.
- `GET /static/...` answers with `Cache-Control: private, max-age=31536000, immutable` (6.25). The browser keeps each file, and a shared cache does not. The shell answers with
  `Cache-Control: no-cache`.
- The binary holds no compressed copy. The key `server.compress_static` (13.8) has the default `true`. With `true`,
  the server makes a gzip copy of each static file of a text type at start. These files are `.html`, `.js`, `.css`,
  `.svg`, `.json` and `LICENSE`. A font gets no copy. A copy that is not smaller than its file is not kept.
- The encoder is code of Hune (RFC 1951 and RFC 1952), with no crate. It is deterministic, and its memory and its time
  for each byte have a limit. Its output is 0.04 % larger than the output of `gzip -6` (measure of 2026-11-15).
- A request gets the copy only when its `Accept-Encoding` accepts gzip (RFC 9110, 12.5.3). The header names `gzip` or
  `x-gzip` with a weight above 0. Or it has no gzip item, and it has `*` with a weight above 0. An absent header or a
  header that does not parse gets the file.
- The answer with the copy has `Content-Encoding: gzip` and the `Content-Length` of the copy. While the key is `true`,
  each answer of a file of a text type has `Vary: Accept-Encoding`. This rule applies with or without the copy.
- A static answer has no `ETag`, so the copy and the file never share one. The server ignores `Range`, so each answer
  holds the whole file or the whole copy. A `HEAD` gets the headers of the `GET` with the same `Accept-Encoding`.
- The shell, the API answers and the errors are never compressed (12.8a). With `server.compress_static = false`, no
  answer is compressed, and the proxy can compress.
- At start, the server makes the shell from `shell.html`. It replaces the marker `{{base_path}}` with
  `server.base_path`, HTML-escaped, and the marker `{{build}}` with the build hash. It also replaces `{{version_text}}`
  and `{{version_title}}` with the version line of the header and its title (9.6). These two texts have only the
  characters `A-Z`, `a-z`, `0-9`, space and `.,:;+()_-`, so they are safe in the JSON string. The result does not change
  while the process runs.

| Extension | `Content-Type` |
|---|---|
| `.html` | `text/html; charset=utf-8` |
| `.js` | `text/javascript; charset=utf-8` |
| `.css` | `text/css; charset=utf-8` |
| `.svg` | `image/svg+xml` |
| `.json` | `application/json` |
| `.woff2` | `font/woff2` |
| `.png` | `image/png` |

The file `fonts/inter/InterVariable.woff2` and the file `fonts/inter/LICENSE` ship in `web/` and have the same URL form. The extension rule of `build.rs` accepts the file named exactly `LICENSE` (9.24).

After an upgrade, an open page asks files with the old build hash and gets `404`. The page then shows the notice of 9.6.

### 9.4 Content security policy and page rules

Each answer of the main listener has this header (section 12 gives the other headers):

```text
Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'
```

- The shell has no inline script. The boot data is a data block `<script type="application/json" id="hune-boot">`,
  which the browser does not run.
- `style-src` has `'unsafe-inline'` because the copied modules set style attributes. No script can come in by it.
- v1 has no `'wasm-unsafe-eval'` (9.27). The page never uses `eval`, `new Function` or a text argument of
  `setTimeout`.
- `base-uri 'self'` permits the `<base href>` of the shell.
- The browser tests fail on each CSP violation and on each console error (9.29, 16).
- The policy does not change for the text panel (9.19b). The text makes no `img`, no `iframe`, no `style` attribute and no script. Thus a browser that ignores the policy shows the same page, and the policy stays a second layer.
- The block `geomap` of the configuration (13.12c) adds one source to `img-src` (12.8). The source is the origin of the tile host and the path of the template up to the last `/` before the first placeholder. No other directive changes, and `connect-src` stays `'self'`, because the page loads a tile as an image. Without the block, the header is the header above, byte for byte.

### 9.5 URL contract and routing

The paths are relative to `server.base_path` (6.1):

| Path | Page |
|---|---|
| empty (the base path itself) | List page (9.7) |
| `d/<uid>` | Dashboard page (9.8) |
| `d/<uid>/<slug>` | Dashboard page (9.8) |

The slug comes from the title. The page folds ASCII letters to lower case and replaces each run of other characters
with one `-`. It removes a `-` at the start and at the end, and cuts the slug to 64 characters. An empty slug gives
the path `d/<uid>`. The page writes its own slug in
the URL with `history.replaceState`. The server ignores the slug (6.25).

| Query name | Page | Value | Meaning |
|---|---|---|---|
| `from`, `to` | dashboard | A time text of 4.4: epoch milliseconds, or date math (9.9). The page also reads three time forms of Grafana (see below). | The time range. Absent: the member `time` of the document. |
| `timezone` | dashboard | `browser`, `utc`, or a name that `isKnownZone` accepts (9.9) | The zone of the view. Absent: the member `time.timezone` of the document. |
| `var-<name>` | dashboard | A value. Repeated for each value of a `multi` variable. `$__all` for "All". | The selection of a variable (9.10). An open variable (4.8, 4.9) can hold a typed value of 1 to 1,024 bytes. A closed variable drops a value that is not an option, with a notice (9.10). |
| `refresh` | dashboard | A value of `refresh_choices` (6.10), or `off` | The refresh. Absent: the member `refresh` of the document. |
| `viewPanel` | dashboard | A panel id, or `panel-<id>` (the form of Grafana scenes). For a copy of a repeat: `<id>-r<n>` or `panel-<id>-r<n>`, with `n` from 2 (9.8c). The page also reads the form `<x>$panel-<id>` of Grafana, where `<x>` is the value of the copy (10.2a, rule 1). | One panel at full size (9.8). The page reads all forms and writes the number for the first copy and `<id>-r<n>` for the other copies. |
| `edit` | dashboard | `1` | Edit mode (10.2). |
| `editPanel` | dashboard | A panel id | The panel editor (10.3). |
| `editview` | dashboard | `json`, `history` or `settings` | The dashboard JSON tab (10.4), the history (10.11) or the dashboard settings (10.3b). |
| `version` | dashboard | A version number, or `current` | A version of the history (10.11). |
| `base` | dashboard | A version number, or `current` | The base of a diff (10.11). |
| `q` | list | A tag name | The tag filter of the list (9.7). |

An `interval` variable writes one `var-<name>` with the text of the option, or `$__auto` for "Auto" (4.10a). A `constant` writes no `var-<name>`. The page ignores a `var-<name>` of a `constant` in the URL (4.10b, rule 3). An `adhoc` variable writes one `var-<name>` for each filter, in the form `key|op|value` of Grafana, or one empty item with no filter (9.10a).

- At load, a URL value wins over the document value.
- The page reads the query with `URLSearchParams`, so `+` reads as a space. It writes each value with
  `encodeURIComponent`.
- After each change of the time range, the refresh or a variable, the page writes the URL with `history.replaceState`. When it opens another page or view, it uses `history.pushState`.
  The rules of 10.2a say which change uses `push`, `replace` or `pop`. The browser "Back" returns to the previous view.
- The page writes the names in this order: `from`, `to`, `timezone`, the `var-<name>` in document order, `refresh`, then the view
  names. It ignores an unknown name and drops it when it writes the URL. Grafana writes `orgId=1`: the page drops it.
- The page writes a name only when its value differs from the default of the dashboard. The default is the value that the page uses when the URL has no such name. When all values are at the default, the address has the view names only, for example `?edit=1`.
- These rules say when a value is at the default:
  - `from` and `to` are one unit. The page leaves both out when they equal `time.from` and `time.to` of the document. When one of them differs, the page writes both.
  - `timezone` is left out when it equals `time.timezone` of the document, or the zone that this member gives (9.9).
  - `refresh` is left out when it equals `time.refresh` of the document. A document with no refresh has the default `off`.
  - A `var-<name>` is left out when the selection equals the selection that the page picks with no URL value (9.10). The All flag and the values MUST be the same, in the same order.
  - A `constant` writes no `var-<name>`. The default of a `query` variable is known after its options load. Until then, the page keeps the name.
- A name that the URL does not have gives the default, also after "Back" and "Forward" of the browser. A link that names a value at the default still works. The page writes the short form at its next change of the address.
- A `pop` that the page makes is the one exception: it keeps the values that the person set (10.2a, rule 12).
- Hune has no "Share" button and no "Share" menu item. The URL of the browser holds the view: the time range, the variables, the refresh and,
  for one panel, `viewPanel`. A name at the default is not in it. A user copies the address from the address bar.
  Hune has no short link and no public dashboard.
- A copied address does not fix a value that equals the default. When the document changes a default, an old address shows the new default.
- During 6 months after the cutover, an nginx `map` sends `/grafana/d/<old uid>/...` to the new prefix (15). The query
  names `from`, `to`, `timezone`, `var-<name>`, `refresh` and `viewPanel` are the names of Grafana.

`main.js` maps the location to the list page, to the dashboard page, or to the page "This address does not exist.". The entry redirect of 9.7 can replace the list page with a dashboard page.
A link of the page is an `<a href>` with a relative URL. A click with no modifier key changes the view with
`history.pushState` and no reload. A click with a modifier key keeps the action of the browser.

The names `viewPanel`, `edit`, `editPanel` and `editview` can be in one URL. Then rule 1 of 10.2a gives one state, and the page removes the names that this state does not use.
A `viewPanel`, an `editPanel` or an `editview` with an unknown value is removed in the same way, with a notice.

The time forms of Grafana. Grafana 12.4.2 writes each absolute time of a URL in ISO 8601 in UTC, for example
`from=2026-11-09T08:00:00.000Z`. The nginx map of 15.17 keeps the query, so an old link can hold these forms. The page reads
these three forms of `from` and `to` as UTC:

| Form | Example | Length |
|---|---|---|
| `YYYY-MM-DDTHH:mm:ss.SSSZ` | `2026-11-09T08:00:00.123Z` | 24 |
| `YYYY-MM-DDTHH:mm:ss` | `2026-11-09T08:00:00` | 19 |
| `YYYYMMDDTHHmmss` | `20261109T080000` | 15 |

- The page changes each of these forms to epoch milliseconds when it reads the URL (`urlTimeText` of `time.js`). The milliseconds of the first form stay.
- The range of the page holds epoch milliseconds, and the page writes epoch milliseconds in the URL, as for each absolute time.
- The page compares the length of the text with the three lengths before it applies a fixed pattern. A text of another length never goes through the pattern.
- The month is 1 to 12, and the day is a day of that month. The hour is 0 to 23, and the minute and the second are 0 to 59.
- A time after 2106-02-07 gives the error of 4.4. A time before 1970 is not a URL form.
- Another text stays as it is, and the grammar of 4.4 applies to it. A text that this grammar refuses gives the notice "The time range of the address is not used: <error>" with the level `warning`. The page then uses the range of the document.
- An error text cuts a time text of more than 64 characters to 64 characters and `…`.

Known differences from Grafana 12.4.2 (39 URLs, measure of 2026-11-22):

- Grafana reads some texts as the number before the first `-`, and the range then starts at 1970-01-01T00:00:02.026Z. These texts are `2026-11-09T08:00:00Z`, `2026-11-09T08:00:00.000`, a bare date such as `2026-11-09`, an offset such as `+01:00`, and a text with a space before it. Hune refuses them with the notice.
- Grafana also reads an offset with no colon in a text of 24 characters, for example `2026-11-09T08:00:00+0100`. It reads a space in the place of `T`, a lower-case `t` in the compact form and the hour 24. Grafana never writes these forms. Hune refuses them with the notice.
- Grafana reads a text of 8 digits as a date `YYYYMMDD` at 00:00 UTC. Hune reads it as epoch milliseconds (4.4), because the page writes epoch milliseconds and reads its own URL again.
- Grafana accepts a time before 1970 and a time after 2106. Hune refuses them with the notice.

The name `timezone`:

- The page reads `timezone` when it opens the dashboard. When the URL has two `timezone` names, the first one applies, as in Grafana.
- A name that `isKnownZone` accepts sets the zone of the view. The range, the time picker, the panels and the axes use it, as for `time.timezone` (4.6, 9.9). The document does not change. The edit mode uses the zone of the draft.
- When the URL gives a known zone, the page does not show the notice of an unknown zone of the document (9.9).
- A name that `isKnownZone` refuses gives the notice "The time zone “<name>” of the address is not known to this browser. The page uses the zone of the dashboard." The notice has the level `warning` and the attribute `data-notice="zone-url"`. It cuts a name of more than 64 characters. An empty name is a name that `isKnownZone` refuses.
- With an unknown name the page uses the zone of the document. Grafana uses the zone of the browser and shows no notice.
- The page writes a known name after `to`. It drops an unknown name when it writes the URL again.

### 9.6 The shell

`shell.html` has this structure:

```html
<!doctype html>
<html lang="en" data-theme="light" data-hune-state="busy" data-hune-hold>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <base href="{{base_path}}">
  <title>Hune</title>
  <link rel="icon" href="static/{{build}}/favicon.svg" type="image/svg+xml">
  <link rel="preload" href="static/{{build}}/fonts/inter/InterVariable.woff2" as="font" type="font/woff2" crossorigin>
  <link rel="stylesheet" href="static/{{build}}/css/vendor/chdash/00-tokens.css">
  <link rel="stylesheet" href="static/{{build}}/css/fonts.css">
  <link rel="stylesheet" href="static/{{build}}/css/tokens.css">
  <link rel="stylesheet" href="static/{{build}}/css/look.css">
  <link rel="stylesheet" href="static/{{build}}/css/base.css">
  <link rel="stylesheet" href="static/{{build}}/css/components.css">
  <link rel="stylesheet" href="static/{{build}}/css/picker.css">
  <link rel="stylesheet" href="static/{{build}}/css/var-select.css">
  <link rel="stylesheet" href="static/{{build}}/css/dashboard.css">
  <link rel="stylesheet" href="static/{{build}}/css/list.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-table.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-chart.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-annotations.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-loading.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-stat.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-variable.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-text.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-timeline.css">
  <link rel="stylesheet" href="static/{{build}}/css/panel-bucket.css">
  <script src="static/{{build}}/js/boot_theme.js"></script>
  <script type="application/json" id="hune-boot">{"build":"{{build}}","version":"{{version_text}}","version_title":"{{version_title}}"}</script>
  <script type="module" src="static/{{build}}/js/main.js"></script>
</head>
<body>
  <header id="app-header" data-testid="app-header"></header>
  <main id="app-main"></main>
  <div id="app-notices" role="status" aria-live="polite"></div>
  <noscript>Hune needs JavaScript.</noscript>
</body>
</html>
```

The page starts in this order:

1. `boot_theme.js` sets `data-theme` (9.25). The attribute `data-hune-hold` holds the first paint until step 5 ends.
2. `main.js` reads the boot block and builds the header with the version line. It starts to load the font Inter (weights 400 and 500), the file `icons.svg` and the module of the route. It calls `GET api/session` (6.10).
3. `401` gives the full page "You are not signed in, or your session ended. Reload the page.". The edge does the
   login, not Hune.
4. Another failure gives the full page "Hune does not answer." with a "Retry" button.
5. The page waits for the font, for 1 s at most, and for the icons. Then the router shows the page of the URL (9.5). A font that does not load changes nothing else. When the first view is complete, the page releases the first paint.

The header is one bar. It is opaque, and it has a line of 1 px below it. The bar is one row at every width, also in edit mode. Its height is 44 px with the line. It has six parts, in this order:

1. The brand: a logo of 20 px, the product name "Hune" and the version line. The brand is a link to the list. The logo is a square with round corners and one line that goes up. The square has the colour `--accent-fill`, and the line is white.
2. The start controls. The list page puts its sort control and its tag filter here, after the brand (9.7). Other pages leave the slot empty.
3. The place of the page. On a dashboard it holds the selector of the dashboards (see below). In the view of one panel and in the panel editor, a way back comes after the selector. These views have no name in the header. The list page leaves the place empty.
4. The controls of the page. The list page and the dashboard page put their controls here (9.7, 9.9). A full page of an error has none.
5. The theme button (9.25).
6. The button "More actions" (9.9). Only the dashboard page has it. It is the last control of the header.

The controls have fixed sizes (9.9), and they do not depend on the state of the page. The header never has a second row.

When the window is narrow, the controls get smaller in steps. The last steps move controls into the menu "More actions". The css of the page and `header_layout.js` use the same widths. A unit test compares them. The steps are these:

| Width | What changes |
|---|---|
| Above 1280 px | The version line shows beside the name. |
| 1100 px or less | The labels of "Edit" and "Save" hide. The range button is 156 px wide. |
| 900 px or less | The buttons "Shift the time range back" and "Shift the time range forward" and "Zoom out" move into the menu "More actions". The button "Add" of the edit mode shows its icon only. |
| 768 px or less | The range button shows its clock and its chevron only. It is 52 px wide. |
| 640 px or less | The product name hides. The button "Revert" of the edit mode is not in the header. The menu "More actions" has it (10.2). |
| 560 px or less | The range button is 40 px wide and the switch "Edit" is 36 px wide. The menu part of "Add" is 20 px wide. The interval button shows its chevron only. The gaps of the time control and of the edit group are 4 px. |
| 480 px or less | The gaps are 4 px, and the icon buttons are 32 px. The name of a view hides, so the selector keeps its room. The way back stays. |

The edit mode adds a group of controls at the left of the time control (9.9, 10.2). The group takes its room from the selector only. The steps of 900 px, 640 px and 560 px keep this room. At each width of 390 px or more, the place of the page keeps a width of 24 px or more. The header has no horizontal scroll. The title in the selector ends with an ellipsis, and the tooltip of the button has the whole text.

The selector of the dashboards follows the host selector of ClickHouse Dash and the bucket selector of Stowr. These rules apply:

- The button shows the title of the dashboard, with a grid icon before it and a chevron after it. It is 32 px high. It is 22 rem wide, and narrower when the place is narrow. It is 12 rem wide at least when the place has 240 px or more.
- The menu has the width of the button exactly. When the button is narrower than 224 px (a phone), the menu is 224 px wide, and never wider than the window. A long title ends with an ellipsis in the button and in the menu. The full title is the tooltip.
- The way back is 28 px wide, after the button. Under 100 px the selector hides, and the way back may get narrower. The hook is `data-testid="header-switcher"`.
- A click on the button opens the menu, and a second click closes it. The key Down or Up on the button opens the menu too.
- The menu touches the button. The button loses its bottom edge and the menu has no top edge, so the two make one shape (see the joined menu in 9.7). The menu has the hook `data-testid="header-switcher-menu"`.
- The menu lists the dashboards that the user can see. It reads the same list as the list page (`GET api/dashboards`). A dashboard that is not valid is not in the menu.
- When the user sees two folders or more, each folder is a row with a chevron and the folder icon. A click on the row opens or closes the list of its dashboards under it. A folder with no dashboard has no row.
- The folder of the current dashboard is open at the first open of the menu. The page keeps the choice of the person until the page loads again.
- When the user sees one folder, the menu lists the dashboards with no folder row. This is the rule of the list page for the folder title (9.7).
- A dashboard with no folder (`folder` is `null`, 6.12) is a line of the menu on its own, with no row. These lines come first, in title order, before the folder rows. With a dashboard with no folder and one folder, the folder keeps its row, so the two are not mixed.
- A read-only store is a row with a lock icon and its title. A click opens or closes the list under it. The list holds the dashboards of the store with no folder first, then its folders as rows, or its dashboards for one folder.
- The rows at the root are the folders of the primary and the read-only stores. With one root group that is a store, the menu lists its folders, or its dashboards for one folder.
- The store of the current dashboard is open at the first open of the menu, as its folder is.
- The current dashboard has the selected fill, its edge and a check mark. A choice of another dashboard opens it as a link of the page does. The leave warning of 10.2a applies.
- The menu reads the folders and the dashboards at each open. It shows the last answer at once. At the first open it shows the disabled line `Loading…` until the answer comes. The new answer fills the menu again, and the focus stays on the same row.
- The page never waits for this read to draw the dashboard. A read that fails shows "The list did not load." in the menu.
- Down and Up move from row to row. Home and End go to the first and to the last row. A letter goes to the next row that starts with it.
- Right opens a folder row, and a second Right goes to its first dashboard. Left closes an open folder row. On a dashboard, Left goes to the folder row.
- Enter and Space choose the row. Escape closes the menu and gives the focus back to the button. Tab closes the menu.

The header is at the top of the window at every width, and it never scrolls away. A name or a line that hides for the eye stays for assistive technology.

The document does not scroll. The body fills the window and has no scrollbar. The element `main` (`#app-main`) is the one scroll container. It fills the window under the header. The scrollbar of the page starts under the header, and the header has the full width of the window. `main` is a positioned box, so a child with `position: absolute` scrolls with the content and never makes the document taller.

`main.js` sets `--header-h`, the height of the header, and `--scrollbar-w`, the width of the scrollbar of the container (0 when the content does not scroll). A part that is fixed to the right edge of the window leaves `--scrollbar-w` free. A new page starts at the top of the container. A Back or a Forward puts the container at the position that it had on that path.

The row of the variables sticks at the top of the container. The panels load when they come within one window height of the container. A drag of a panel in the edit mode scrolls the container near its top and its bottom edge.

This bar replaces the controls row below the header of Grafana. ClickHouse Dash also has its actions in the header. The deviation from the Grafana parity of 9.8 and 9.9 is on purpose.

The edit mode has no row of its own: its actions are in the header or in the menu "More actions" (10.2). The header of Grafana 12.4.2 is 48 px high, and the header of Hune is 44 px high. The variables bar (9.10) keeps its own row under the header.

Some views add a way back after the selector: the view of one panel (9.8) and the panel editor (10.3). These views have no name in the header. The way back is a link with an arrow only. Its name and its tooltip say the target.

The content of each view starts under the header with a gap of one token, `--page-pad`. The token is 16 px, and 12 px at 600 px or less. The view of the dashboard, the settings, the JSON, the history and the panel editor use the same token. In the panel editor, the preview and the top of the options pane start at this gap. The row of the variables starts there too. The browser case B208 measures the gap.

The version line is the text `<version>:<checksum>`, for example `0.1.0:1f2e3d4c5`. It is 12 px high text in the muted colour, with `data-testid="app-version"`. It is beside the name, on the same line, so the brand needs no second line. The rules are these:

- The version is `CARGO_PKG_VERSION`. It is the member `version` of `GET api/session` (6.10).
- The checksum is the first 9 hexadecimal characters of the commit of the release build. The release job gives the commit to the build in `HUNE_BUILD_COMMIT`, and `build.rs` fixes it. This is the SHA-1 that git computed. Hune never computes it at run time.
- A development build has no commit (`unknown`). The checksum is then the first 9 characters of the build hash of 9.3.
- The `title` of the line gives the full value and its source. For a commit it reads `Hune <version>, commit <40 characters>`. For a build hash it reads `Hune <version>, build <hash> (the hash of the web files; this build has no commit)`.
- The line holds no file path, no secret and no value of the environment. The server reads nothing at run time for it.
- The server puts the text and the title in the boot block of the shell, as `{{version_text}}` and `{{version_title}}` (9.3). The line shows on every page, also on the pages of an error and before `GET api/session` answers.
- The line is never cut. It sets the width of the brand. The link of the brand has the accessible name "Hune, the list of dashboards", and the line is its description (`aria-describedby`).
- The line shows above 1280 px. At 1280 px or less it hides for the eye and stays for assistive technology. The link of the brand has a `title` with the full version text. Thus the tooltip of the brand gives the version at each width.

The first paint is held. A reload then shows one stable render:

- `shell.html` sets `data-hune-hold` on `<html>`. While the attribute is present, `base.css` sets `visibility: hidden` on each child of `<body>`, except `<noscript>`. The layout stays, so `main.js` can measure the real sizes.
- The browser then has no text and no image to paint. On a reload, it keeps the old page on the screen until the new page is complete. On a first visit, it shows the page colour of the theme.
- `main.js` removes the attribute when the first view is complete. A complete view has the header with its controls, the page of the route, the icons and the font. The page measures the layout of the header at that time.
  For a URL that names an editor state (10.2a), the first view is complete only when the editor view is in the page. The page then shows no frame of the plain dashboard before it.
- A route that another route replaced (the entry redirect of 9.7) does not remove the attribute. The last route removes it. The pages "You are not signed in" and "Hune does not answer." remove it at once.
- `boot_theme.js` removes the attribute after 2 s in all cases. Thus a page whose script does not load, or does not end, is never hidden.
- `data-theme` is set before the first paint (9.25). Thus the first frame has the right colours and the right icon in the theme button.
- `main.js` reads `icons.svg` and puts its symbols in one hidden `<svg id="hune-sprite">` at the start of `<body>`. Each symbol has the id prefix `hune-icon-`. The function `icon()` writes `<use href="#hune-icon-<name>">`. An icon thus shows with its element. A reference to the file would draw the icon after its element.
- The page starts to load the module of the route (`dashboard.js` or `list.js`) while `GET api/session` runs.
- After the first frame, nothing of the page moves or goes away. The header, its buttons, the row of the variables and each panel frame keep their place and their size. The browser case B58 checks this.
- The same holds for a change of the state between the view and the edit mode (10.2a, rule 17). Each chart stays painted with its data, and the browser case B207 checks this.

A full page has no card. The page "not found" (9.8) shows an illustration, its text and the button "Go to the list". Another full page shows a round icon, its text and its buttons.

The notice area shows one line for each notice, with a close button. A notice has the level `info`, `warning` or
`error`.

- An API answer with an `X-Hune-Build` header that differs from the boot build gives a notice. A static file that
  gives `404` also gives it. The notice is "A new version of Hune is available. Reload the page.", with a "Reload"
  button. The page does not reload by itself.
- The code `pdp_unavailable` gives "The access decision service does not answer. Try again later."
- A `401` of any API call gives the full page of step 3.

### 9.7 The list page

The list page is a set of cards on the page background. It has no title, no search field and no table.

- The page reads all the pages of `GET api/dashboards` (`limit=500`, then each `next`) and `GET api/folders`.
- The page shows one card for each dashboard. A folder has no box around its cards. The folders are in the order of their titles. The comparison folds ASCII letters to lower case. In a folder, the cards are in title order.
- A card has two parts. The first is a row with the title at the left, then the badge of the last change and the three dots at the right. The second is the tags.
- The title is on one line. It ends with an ellipsis when it is too long. Its `title` attribute holds the whole name.
- A card shows no description, no uid, no number, no shape and no count of panels. It has no word about the access of the user, such as "Can edit" or "Read only". The page has no counter of dashboards.
- A card is an `article`. Its title is a link to `d/<uid>/<slug>`. The link covers the whole card with a pseudo-element. The buttons and the tag chips are siblings of the link, one layer above it. A card is never one anchor around buttons.
- A tag is a chip. Its colour comes from a hash of the tag text over the 29 tag colours of Grafana 12.4.2. The chip shows this colour as a tint. The fill is 32 % of the colour, and the edge is 65 %. The text is the colour mixed with the ink of the theme. The ink is black at 30 % in the light theme, and white at 60 % in the dark theme.
- Each chip has 4.5:1 or more on each surface that holds it, in both themes.
- A tag chip filters the list when the page shows the filters. A second click on the same chip clears the filter. When the page hides the filters, a chip is only a picture.

The badge of the last change:

- The badge is a small pill at the top right of the card. It has a clock icon and the text `Last change <age>`, for example `Last change 1d`. The badge is 20 px high. The row that holds it has one height, so a card with a badge and a card without one have the same rows.
- The age is the age of `updated_at` in a short form: `now` (less than one minute), `<n>m`, `<n>h`, `<n>d`, `<n>w`, `<n>mo` or `<n>y`. The unit is the largest that fits, and the number is rounded down. A week is 7 days, a month is 30 days and a year is 365 days. A time in the future gives `now`.
- The `title` attribute of the badge holds the absolute time in the browser zone. The hook is `data-testid="card-change-<uid>"`.
- Hune knows a change only when it records one. A system that cannot record changes has no last change. The date of a file is not a change.
- The badge shows only when all these conditions are true. `editing` is `true` (6.10). The item has a `version` of 1 or more (6.12). The member `updated_by` is a text, and it is not `external` and not empty. The time is a valid time.
- A dashboard that comes from a read-only volume or from git has `version` 0 or the author `external` (5.2). A dashboard that a person changed outside Hune has the author `external`. These cards have no badge.
- In mode `open`, the author of a save is `-`. This is a recorded change, so the card has the badge.
Folders:

- A folder title shows above its cards only when the user sees two groups or more. It is the folder icon and the name, in small muted capitals. It spans the whole row.
- With one group, or with no folder, the page shows no folder title. An empty folder shows the text "This folder is empty." under its title.
- The dashboards with no folder (`folder` is `null`) form the root group of their store. The root group never has a title. It counts as a group, so the folders below it keep their titles. The hook is `data-testid="root-<store>"`, and `root-primary` when the server gives no store.
- The root groups come first, then the folders in the order of this section. The root group of the primary is the top of the page. The sort "Z–A" reverses the folders and the cards, and the root groups stay first.
- With `editing` `true` and `access.write` on a folder, the title has a menu with "Rename" and "Delete" (10.7). A user who sees one folder only has no title, and so has no menu of the folder. "New folder" is in the header (below).

Stores:

- The server has one primary store and read-only stores (6.10). The member `stores` of the session lists them. The folders of the primary show at the root, as above.
- A read-only store is one root group. Its title is the `title` of the store, with a lock icon, in small capitals. The hook is `data-testid="store-<id>"`.
- The root group of a read-only store is the first group under its title, before its folders. The folders of a read-only store show under its title. They have an indent of 16 px, and their title is smaller. Their hooks are the same as for the folders of the primary.
- The groups at the root are the root group of the primary, the folders of the primary and the read-only stores. The title of a folder or of a store shows only when there are two groups or more.
- When the title of the only root group is a store, its folders show their titles for two groups or more. Its own root group counts as one group.
- The read-only stores come after the folders of the primary, in title order. The sort "Z–A" reverses the order of the stores.
- The page has no visible title. It has one `h1` with the text "Dashboards" for assistive technology, hidden with `visually-hidden`.

Marks on a card:

- A card whose item has `read_only` set to `true` shows a lock after the badge. Its name and its tooltip are "Read-only store". The hook is `data-testid="card-lock-<uid>"`.
- A card that is read-only has no three dots and no button. Its `access.write` is `false` (6.12).
- A card whose item has a `stale` code shows a warning mark. Its name and its tooltip are "The file is not valid. The last valid version is shown.". The hook is `data-testid="card-stale-<uid>"`.
- A stale card is a valid card: it keeps its title, its link and its tags. The mark never shows a path or a message of the parser.
- Each mark is a picture 20 px high, so a card has the same height with and without it. The mark of a file that is not valid uses the token `--warning`.
- "New dashboard", "Move" and "Import" list only the folders of the primary: they list the folders with `access.write` set to `true`.

The grid:

- The cards are in a grid of columns of 400 px. The folders share the columns, so the cards of two folders do not align by folder. At 440 px or less, there is one column of the full width.
- The whole block of cards is centred, across and down, in the window below the header. When the block is taller than the window, it starts at the top and the content scrolls.

The three dots of a card:

- A card that the user can edit has one button, the three dots. It is at the right end of the first row, after the badge. It is 24 px high, and it keeps the height of the row, so a card with and without it has the same rows.
- The button has no visible label. Its accessible name and its tooltip give the name.
- The user can edit a card when `editing` is `true` (6.10) and the item has `access.write` (6.12). The three dots have this one condition. "Delete" needs `dashboard.write`, as the route `DELETE api/dashboards/<uid>` does.
- The menu of a card has "Edit", "Export", "Move" and "Delete", in this order. "Export" shows only when `access.export` is `true`.
- "Delete" has the danger colour.
- "Edit" is a link to `d/<uid>/<slug>?edit=1`. It opens the dashboard in edit mode.
- The menu "More actions" of the dashboard page has "Export" but not "Move" (9.9). Thus the menu of the card is the way to "Move" (10.7).
- "Delete" runs the delete flow of 10.13 and names the dashboard in its dialog. The server decides. A `403` shows the usual refusal.
- A card that the user cannot edit has no button. The user finds "Export" in the menu "More actions" of the dashboard page.

An item that is not valid (6.12):

- Its card shows the uid in place of the title, and the badge "Not valid". It has no link to the dashboard and no "Edit" or "Delete".
- The button "Show errors" opens the errors in the card, each with its pointer and its message.
- The three dots of the card show only when the menu has an entry. The menu has "Download" only when `access.export` is `true`. "Download" reads `GET api/dashboards/<uid>?download=true` (6.14). With `access.write`, the menu also has "Replace": the import of 10.12 with this uid and the `etag` of the item.

The header (9.6):

- The sort control and the tag filter follow the brand, at the left. One primary button "New" is at the right, with a plus icon and a chevron. The theme button follows it. The page body has no control.
- The button "New" opens a menu with "New dashboard", "New folder" and "Import", in this order (`data-testid` `list-new-menu`, `list-new-menu-popup`, `new-dashboard`, `new-folder`, `import`). It is the same button at every width.
- "New dashboard" and "Import" show when `editing` is `true` and one folder has `access.write` (6.11). "New folder" shows when `editing` and `access.create` are `true`. The button is hidden when the menu has no item.
- The sort control has "Alphabetically (A–Z)" and "Alphabetically (Z–A)". Its button shows the order that applies in short, "A–Z" or "Z–A", and never a placeholder. Without a choice, the order is the order of the folders and cards above, which is A–Z. The button then shows "A–Z", and the menu marks "Alphabetically (A–Z)". The page keeps the choice in `localStorage` key `hune.list.sort`.
- The button "Filter by tag" opens a menu of the tags. Each line has the chip of the tag and the number of dashboards of this tag at the right. The most used tag comes first, then the tags in text order. A choice sets the filter and puts the tag in `q` of the URL. A card stays when one of its tags is this tag, with ASCII letters folded.
- The sort control, the tag filter and the selector of the dashboards (9.6) use one component: the joined menu. The next rules apply to each of them.

The joined menu follows the host selector of ClickHouse Dash and the bucket selector of Stowr:

- The button shows the chosen value and a chevron. When the menu is open, the button has a square bottom, no bottom edge and the surface of the menu. The chevron turns. The menu touches the button, and it has no top edge, so the two make one shape.
- The menu is as wide as the button at least, and 480 px at most. The menu of the selector of the header is as wide as its button exactly (9.6). It scrolls when it is higher than 70 % of the window, or 520 px. A line is 35 px high.
- The chosen line has the selected fill, its edge and a check mark at the right. A short fact, such as a count, is at the right of the line in the muted colour. A line that is too long ends with an ellipsis.
- A click on the button opens the menu, and a second click closes it. The keys Down and Up on the button open it. At the open, the focus goes to the chosen line, or to the first line.
- In the menu, Down and Up move from line to line. Home and End go to the first and to the last line. A letter goes to the next line that starts with it. Enter and Space choose the line. Escape closes the menu and gives the focus back to the button. Tab closes the menu.
- A group is a line with a chevron. It opens and closes the lines under it. Right opens it, and Left closes it.
- One menu is open at a time. A click outside the menu closes it.
- The page has no search field, no view control and no way to collapse a folder. The URL parameter `q` holds one tag name.

The threshold of the filters:

- The session gives the key `list_filters_min_dashboards` (6.10, 13.2). The default is 6.
- The page shows the sort control and the tag filter only when the user sees more dashboards than this number. With this number or fewer, it shows neither. The number counts the items of the list, also the items that are not valid.
- When the filters are hidden, the page uses the default order, and it ignores `q` and the saved sort. A hidden control never changes the page.
- A session with no member `list_filters_min_dashboards`, or with a member that is not a whole number of 0 or more, shows the filters. The page keeps no copy of the number: each paint reads the session.

States and widths:

- While the list loads, the body shows three empty cards and has `aria-busy`. A hidden live status names the tag filter when it is active. It gives no count.
- A tag filter with no result shows "No dashboard has this tag." with the button "Clear the filter".
- An empty list shows only its empty state, with no toolbar. The title is "No dashboard yet.". In mode `authzen` it is "No dashboard is visible to you.". The state also has a hint and the same button "New" (`data-testid` `new-menu-empty`), with the items that the subject can use.
- The header of the list page is one row at every width (9.6). The sort control and the tag filter are 150 px wide. Up to 1180 px they show their icon only, and they are 36 px wide. Their names and tooltips stay. At 480 px or less the icon controls are 32 px wide.
- The button "New" has no other form at a narrow width. Up to 480 px it has less padding and a smaller gap. The header has no 900 px rule for the actions.

The entry redirect:

- The list page can open a dashboard at once. This is the entry redirect. It runs only when these conditions are all
  true:
  - The list page is the first route of the page load, or of a full reload.
  - The URL has no tag filter `q`. An empty `q` counts as no text.
  - The complete list of all pages holds exactly one item, and this item has the state `valid`.
- The folder of the item does not matter. An `invalid` item never causes a redirect, also next to one valid item.
- The redirect does not read `access.create`. A user who can create a folder or a dashboard also goes to the dashboard. The redirect can later depend on this right, so that a user who can create stays on the list. This is an open item (19.35).
- In mode `authzen`, the list is the list that the display filter gives (11.25), so a subject who can see one dashboard goes to it.
- The redirect goes to `d/<uid>/<slug>` with `history.replaceState`. Thus the browser "Back" does not return to the list.
- A navigation in the page never causes a redirect. The brand link, the selector, "Go to the list", "Back" and "Forward" show
  the list with its cards.
- At each redirect, the page writes the uid and the time in `sessionStorage` key `hune.entry.redirect`. A second
  redirect to the same uid in the same tab does not run during 10 seconds. This stops a loop of full page loads. A
  failed read or write of `sessionStorage` counts as no guard. A guard with a time in the future counts as no guard.
- When the dashboard answers `403` or `404` after the redirect, its page shows the link to the list (9.8). The list then
  shows its card, or the empty state.

### 9.8 The dashboard page

The page loads in this order:

1. It calls `GET api/dashboards/<uid>`. `404` gives "This dashboard does not exist." with a link to the list. `403`
   gives "You have no access to this dashboard.". `422 stored_document_invalid` gives the list of errors.
2. It computes the time range, the refresh and the variable selections from the URL and the document (9.9, 9.10).
3. It shows the header with its controls (9.9), the row with the variables bar (9.10), and the grid, with no data.
4. It loads the variable options (9.10), then it plans the panel queries.

Stores:

- The answer of step 1 has `read_only` and, for a file that is not valid now, `stale` (6.14). These members come from the store of the dashboard.
- A dashboard with `read_only` set to `true` has a lock in the header, where the switch "Edit" would be. Its name and its tooltip are "Read-only store". The hook is `data-testid="read-only-lock"`.
- A read-only dashboard has no switch "Edit", because its `access.write` is `false`. It is the path of a user who has no right to write. A link with `edit=1` opens no editor.
- A dashboard with `stale` shows one warning notice after the load. Its text is "The file of this dashboard is not valid. The last valid version is shown.". The hook is `data-notice="stale"`.
- The notice shows once for each page load, and it moves no part of the page. The views JSON and Inspect stay readable.

Grid:

- Each section has a header (none when its `title` is `null`) and a CSS grid of 24 columns. The grid has
  `grid-auto-rows: 30px` and a gap of 8 px for rows and columns. This is the grid of Grafana.
- A panel has `grid-column: <x + 1> / span <w>` and `grid-row: <y + 1> / span <h>`. Its height is `30 × h + 8 × (h - 1)`
  pixels, as in Grafana.
- The grid has the panels of each section. It also has one placed item with a negative id for each variable that has a placement in that section (4.7a). The code `gridDoc` of `var_layout.js` makes these items. The document never holds such an item. The drag code of 10.5a treats it as a panel and writes its new place into `placement` (the code `splitLayout`).
- A panel or a section that has a member `repeat` (4.13a) gives more than one item. 9.8c gives the rules of the copies. The rest of this section applies to each copy as to a panel.
- A screen wider than 768 px has the grid above. At 768 px or less the grid becomes a flow. The panels follow the order of `y`, then `x`, then `id`. From 481 to 768 px the flow has two columns. A panel with `grid.w` of 12 or less takes one column, and a wider panel takes both. At 480 px or less the flow has one column.
- In the flow, the height of a panel is a minimum height. It comes from the formula above, with these floors: 160 px, 240 px for a `table` and 280 px for a `timeseries`, `barchart`, `piechart` or `xychart`. The frame of a placed control (9.10) has no floor: its height in the flow is the formula only.
- Print keeps the grid on an A4 landscape page with margins of 10 mm. It shows the title of the dashboard, and one line with the time range and the values of the variables. It hides the header, the variables bar, the frames of the placed controls, the menus and the buttons of the legend. The line of the variable values (above) shows the selection of each variable, also of a variable that has a placement. The print line leaves out a `constant`, because it is a part of the document and not a choice of the viewer. A panel stays on one page.
- The section header is 30 px high, with 8 px between it and its first panel. It has the title (16 px, weight 500), and a button with `aria-expanded` and a chevron of 16 px. The title of a collapsed section is muted. The title shows after the replacement of the variables (9.13).
- The number of panels of a collapsed section is hidden from the eye, and a screen reader keeps it. In view mode, a collapse changes only the page, not the document and not the URL.
- A collapsed section is out of the layout, so the chart engine releases the canvas of each chart in it. The charts of a section that expands draw in the task of the expand, not at the next resize callback.
  - For this, the grid sends the event `hune-shown` of 10.2a, rule 17, after the section shows. Thus no paint and no script after the expand finds a blank canvas.
  - The rule applies to each copy of a section that repeats (9.8c), and to the draft grid of 10.2. An expand sends no query when the data did not change.
  - A section that is collapsed at the load has no data, because it sends no query (see below). Its first expand shows the empty frames of the states `idle`, `queued` and `loading` (9.11).
  - It then sends one query for each panel near the viewport, and its charts draw when the data comes.
  - The browser case B210 checks these rules in both themes. Its probe reads the frames after each paint, at the end of the expand task and in the next task.
- Grafana 12.4 has a sidebar of 42 px at the right of its grid. Hune has none, so a panel is about 3 % wider at the same window width. Thus the bucket interval of 7.4 can differ from Grafana near a boundary.
- The panels of a section with a header start 24 px to the right of the grid edge. An expanded section has a vertical guide of 1 px, 16 px to the left of its panels.
- When the document has `cursor_sync = "crosshair"` (4.5), the time series panels share the `syncKey` of the
  chart engine.
- A drag on a time series sets an absolute time range and writes the URL with `history.pushState`.

When a panel queries:

- Its section is expanded, and its frame is within one viewport height of the viewport. The page finds this with an
  `IntersectionObserver` with `rootMargin: "100% 0px"`.
- Or it is the source of a reuse panel that queries (9.23). The source then queries also out of view or in a collapsed
  section.
- A panel keeps its result until the next trigger. A panel that leaves the viewport does not stop its query.

These rules apply to each copy of a repeat. A copy is a panel for the scheduler (9.8c).

A placed control (9.10) has no query. It is not a panel, it is not a trigger, and it is not in the queue of the scheduler. Its frame has `data-state="ready"` from the first frame. A change of its control is a change of the variable, and 9.10 gives its effects.

A panel with several queries is one frame for the grid and for the view. It has one state (9.8d) and one result (9.22a). A copy of a repeat never has extra queries (4.15a, rule 8), so 9.8c does not change.

A text panel (9.19b) has no query. It is not a trigger and it is not in the queue of the scheduler. Unlike a placed control, it is a panel: it has a title bar, a menu, the member `transparent`, panel links and a view (`viewPanel`). Its frame has `data-state="ready"` after the first draw. A time range and a refresh do not change it. A change of a variable that its content names draws it again.

The scheduler of `runtime.js`:

- These events are triggers: the load, a change of the time range, a refresh, and "Retry". A change of a variable that
  the panel SQL names (9.10) is a trigger. The expansion of a section, and a panel with no current result that comes
  near the viewport, are triggers.
- The queue is in document order: the sections in order, and the panels of a section by `y`, then `x`, then `id`.
- The queue holds queries, not panels. A panel with several queries puts one entry in the queue for each visible query (9.8d). The page sends 6 query requests at most at the same time, queries of all the panels and copies included.
- A new time range or a refresh aborts each request of the earlier state with its `AbortController`. It also empties
  the queue. A change of a variable aborts only the requests of the panels that it changes. It aborts the requests of the queries whose SQL names a changed variable. A query that does not name it stays (9.8d).
- `max_data_points` is the panel member when the document gives it. Else it is the inner width of the panel frame
  (7.4). Grafana also uses the width of the panel, not the width of the plot.
- The inner width is the width of the content box of the panel frame element `data-testid="panel-<id>"` (9.29). It is
  its `clientWidth` minus its left and right padding, in CSS pixels, rounded with `Math.round`. The page reads it at the
  time of the request and keeps the value from 1 to 10,000. A browser test compares it with the width of a panel of a
  fixture (16).
- Each request carries the etag of the document. On `409 dashboard_changed` (7.15), the page reads the document again.
  It keeps the time range, the refresh and the variable selections that stay valid (9.10). It shows the grid again,
  runs the panels and shows the notice "The dashboard changed: Hune loaded the new version.". In edit mode it does
  10.8 instead.
- An annotation (4.5b) has a query like a panel has one, but the scheduler runs it as a different kind of job (9.19d). It has no `max_data_points`, and it uses the range of the dashboard.
- The same triggers start the queries of the annotations. They are the load, a change of the time range and a refresh. A change of a variable that the SQL names is a trigger. So is the expansion of a section that holds a panel that the annotation needs.
- An annotation job runs only when two conditions are true. The annotation has `enabled` not equal to `false`. At least one `timeseries` panel that it applies to is in view, as for a panel query. This panel is in an expanded section and in the viewport, or it is the panel of `viewPanel`. The page sends one request for each job at each trigger. All the panels share the result.
- The jobs of the annotations go in the queue after all the panel jobs of the same trigger. They wait one microtask, so the panels that one batch of the observer shows start first. They count in the 6 requests at the same time. A new trigger aborts them as it aborts the panel requests.
- The page never starts an annotation query for a pointer move, a hover or a zoom of the plot. A legend click and a change of the theme start none.

`viewPanel=<id>` shows one panel in all the main area, below the row of the variables bar. `Escape` closes it. The other panels do not query.

- The view has no row of its own for the way back. The way back is the arrow after the selector in the header, a link without `viewPanel` (`data-testid="view-back"`).
- The content has no scrollbar in this view. The panel fills the window under the header and the row of the variables. The page padding is above it and under it.
- The padding is 16 px, and 12 px at 480 px or less. The browser case B233 checks this at four sizes, in both themes, with and without variables.
  The key Escape and the arrow are the event `close-view` of 10.2a. The panel menu of the view has the item "View", which does nothing for the panel that the view shows.
- When the view opens, the focus moves to the view (`role="region"`, `aria-label` "View panel"). A pointer user sees no focus ring there.
- The arrow stays at every width.

### 9.8a Dashboard links

The member `links` of the document (4.5a) gives the dashboard links. They show in the row under the header (9.9), in a group after the variables bar (9.10). The group has the hook `dash-links`. It is absent when `links` is empty. It wraps with the row.

- A link of the kind `link` is an `<a>`. It shows its title and an icon of 16 px.
- A link of the kind `dashboards` shows the dashboards that have all its tags.
- With `as_dropdown` = `false`, the group shows one `<a>` for each dashboard. The text is the title of the dashboard.
- With `as_dropdown` = `true`, the group shows one button with the title and a chevron. The button opens a menu with one item for each dashboard.
- A link that has no dashboard shows nothing. A dropdown that has no dashboard shows no button. Grafana shows the button with an empty menu (paper, V7).
- The page writes each title with `textContent`, after the replacement of the variables (9.13). It sets `href` with a property setter, never as HTML.
- `target_blank` has the rule of 9.18.

The list of a link of the kind `dashboards`:

1. When the dashboard loads, the page sends `GET api/dashboards?tag=<first tag>&limit=500` (6.12). With no tag, it sends no `tag`. It follows `next` for 3 pages at most.
2. It keeps the items with `state` = `valid` that have all the tags of the link. The comparison is exact.
3. It removes the current dashboard from the list.
4. It sorts by the title in lower case, compared by UTF-16 code unit, and then by uid. It shows 50 items at most. Grafana sorts the same way, with no fold of accents, and breaks a tie by the order of creation (paper, V7).
5. A failed request gives no item for this link and one warning in the console. No other part of the page changes.
6. The page does not read the list again before the next load of the dashboard.
7. The route applies the display filter (11.25). A viewer sees only the dashboards that the viewer can see.

The URL of a link:

- The URL of a dashboard item is `d/<uid>/<slug>` under the base path (9.5).
- `keep_time` = `true` adds `from=<from>&to=<to>`. The two values are those that the URL of the page holds now (9.5). Grafana also adds `timezone=<zone>`. The URL of Hune has no zone member, so the page does not add it (paper, V7).
- `include_vars` = `true` adds `var-<name>=<value>` for each variable, in document order, as 9.5 writes them. This is the text of the token `${__all_variables}` (9.18).
- The page puts the added names at the end of the query. It writes `?` before the first name when the URL has no query, and `&` when it has one. It puts them before a fragment that starts with `#`. Grafana puts them after the fragment: this is a known difference (paper, V7).
- For the kind `link`, the page replaces only seven tokens in `url`: `$<name>`, `${<name>}`, `${<name>:queryparam}`, `${__from}`, `${__to}`, `${__url_time_range}` and `${__all_variables}`. The values are those of 9.18. Another token stays as it is.
- Then the page checks the final URL with the rule of 4.23 (`FE-09`). A URL that fails gives no link and one warning in the console.
- A click with no modifier key on a relative URL changes the view with `history.pushState` (9.5).

In edit mode the links stay in the row. The draft is in `localStorage` (10.8), so a click does not lose it. Print hides the group, as it hides the variables bar (9.8).

The hooks are:

- `dash-links`: the group.
- `dash-link-<n>`: the `<a>` of a link of the kind `link`, or the group of a link of the kind `dashboards`. `n` is the index in `links`, from 0.
- `dash-link-<n>-button`: the dropdown button.
- `dash-link-<n>-item-<uid>`: one dashboard.

### 9.8b Panel time override

The members `time_from`, `time_shift` and `hide_time_override` of `data.query` (4.15) change the range of one panel. The function `panelRange(query, range, now, zone)` of `panel_time.js` computes it. `range` is the range of the dashboard (9.9). `now` is the time `t` of 9.9. `zone` is the zone of the dashboard. The steps are (`FE-22`):

1. `time_from` has an effect when at least one end of the dashboard range is a `now` text. Then `to` is `now`, also when the `to` of the dashboard is absolute. `from` is the time text `time_from`, resolved as the `from` of 9.9 with `now` as `t`. When both ends of the dashboard range are absolute, `time_from` does nothing and the panel shows no note for it. Without `time_from`, `from` and `to` are the values of the range of the dashboard (paper, V6).
2. With `time_shift`, both ends move back by `<count><unit>`, also when the dashboard range is absolute. The rule is the rule of `-<count><unit>` in 9.9: `d`, `w`, `M` and `y` are calendar units in the zone of the dashboard. The shift comes after `time_from`.
3. `from` MUST be below `to`. When `time_from` is `now/<unit>` at the first millisecond of a unit, `from` equals `to`, and the page uses `to` = `from + 1`.
4. Both ends MUST be from 0 to 4294967295999 (7.3). Else the panel has the state `error` and sends no query. The message is "The time override of this panel gives a range that Hune cannot use."
5. The request of the panel, `max_data_points` (9.8), the interval rule (7.4) and the macros (7.10) use this range. The interval follows the span of this range.
6. A refresh computes the range again with a new `now` (9.9). The range of a panel with only `time_shift` follows the range of the dashboard.
7. A drag on a chart, "Zoom out" and the shift buttons change the range of the dashboard, never a member. These actions give an absolute range, so `time_from` has no effect (step 1) and the panel follows the dashboard range. A panel with only `time_shift` moves.
8. The options of a `query` variable with `refresh` = `time_change` use the range of the dashboard (9.10). The tokens `${__from}` and `${__to}` of 9.18 use it too.
9. A panel with `reuse` has no member of its own. It shows the frame of its source, so it shows the range and the note of its source.

The note of the title bar. `timeNote(query, zone)` gives its text. It gives `null` when `hide_time_override` is `true` or when the panel has no override. The text is the first row of this table that fits.

| `time_from` | Text |
|---|---|
| `now-<n><unit>`, with or without a round `/<unit>` | `Last <n> <word>`. `<word>` is `second`, `minute`, `hour`, `day`, `week`, `month` or `year`, with `s` at the end when `<n>` is more than 1 (`Last 1 hour`, `Last 6 hours`, `Last 5 days`) |
| `now/d`, `now/w`, `now/M`, `now/y` | `Today so far`, `This week so far`, `This month so far`, `This year so far` |
| `now/s`, `now/m`, `now/h` | `<time_from> to now`. Grafana gives the note `invalid time override` and the range of the dashboard (known difference) |
| absent, with `time_shift` | `Timeshift -<time_shift>` |

With `time_from` and `time_shift`, the text is the text of the table, then ` + timeshift -<time_shift>` (for example `Last 6 hours + timeshift -1h`). When `time_from` has no effect (step 1), the text is the text for `time_shift` alone. Grafana writes the unit `M` of a shift as `m` (`Timeshift -1m`). Hune writes the member as it is (known difference).

The note is a text of 14 px at weight 400 in the link colour of the theme. It has a clock icon of 14 px and a gap of 8 px, after the title. Grafana uses `#6E9FFF` in the dark theme and `#1F62E0` in the light theme. Its tooltip gives the range of the panel as `YYYY-MM-DD HH:mm:ss to YYYY-MM-DD HH:mm:ss` in the zone of the dashboard (9.19). Grafana adds the name of the zone on a line after it.

`hide_time_override` hides the note, also for a shift alone. The hook is `panel-<id>-time-note`. The texts are those measured on Grafana 12.4.2 (paper, 10.8, V6).

### 9.8c Repeat of panels and sections

A panel or a section that has the member `repeat` (4.13a) shows one copy for each value of its variable. The page makes the copies. The document, the draft and the saved bytes never hold a copy. The pure module `repeat.js` (9.2) holds the rules of this section. It has no DOM access.

The limits are keys of the server configuration. The page reads them from the member `repeat_limits` of the session (6.10): `{ panel, section, dashboard }`. Each function of `repeat.js` takes the limits as an argument.

The constants `MAX_REPEAT_PER_PANEL`, `MAX_REPEAT_PER_SECTION` and `MAX_REPEAT_PER_DASHBOARD` (4.35) are the defaults. The page uses them only when the session has no such member, as with an older server. A member that is present fails closed when it is not a whole number from 1 to its hard bound. The bounds are 200 for a panel, 50 for a section and 200 for a dashboard. 

Then the limit is 0, nothing repeats, and the frame shows the state `repeat-limit` with a message. The page never changes the number to make it fit. A test compares the defaults with `limits.rs` (16.10a).

The values:

- The values of a repeat are the options of the variable that its selection holds (9.10). With "All", they are all the options. A `custom` variable has its own options. A `query` variable has the options after the regex, the removal of the duplicates and the sort of 9.10.
- The page puts the values in the order of that list. The order of the selection, and the order of the items `var-<name>` in the URL, do not count. A selection that is the same set gives the same copies in the same order.
- A value is always an option of the variable. The page never makes a copy from a URL value that is not an option.
- A repeat whose variable does not exist, or cannot repeat (4.13a, rule 1), fails closed. Its frame shows the state `repeat-empty` and sends no query. A valid document never has this case.
- While the options of a `query` variable load, the repeat has no value yet. The frame of the panel shows the state `idle` and sends no query. The page shows no message for an empty selection before the load ends.
- Let k be the number of values. The page then shows one of these results.

| k | Result |
|---|---|
| 0 | One frame with the state `repeat-empty`. It has the message below. The panel sends no query. |
| 1 to the limit | k copies. A repeat with k = 1 shows one copy. It is a normal panel, with the width rule of the direction. |
| Above the limit | One frame with the state `repeat-limit`. It has the message below. The panel sends no query. No copy shows. |

The limit of a panel is `repeat_limits.panel`, and the limit of a section is `repeat_limits.section`. The page never cuts the list to the limit. This rule is in one function of `repeat.js` (`classify`), so a change of this rule is small.

The budget of the dashboard:

1. The page walks the repeats in the order of the document: the sections in order. In a section, it takes the repeat of the section first, then each panel that has a repeat, by `y`, then `x`, then `id`.
2. A repeat that has k values in the limit of its own kind takes a cost from the budget `repeat_limits.dashboard`. The cost of a panel is k. The cost of a section is k times the number of panels of the section.
3. A repeat whose cost is more than what is left gets the state `repeat-limit` with the message of the dashboard. It takes nothing from the budget. The walk goes on with the next repeat.
4. The walk does not depend on the scroll, on a collapsed section or on the order of the answers. For the same document and the same selections, it gives the same result at each load.

The messages are text, never HTML. `<label>` is the label of the variable, or its name (9.10).

| State | Message |
|---|---|
| `repeat-empty` | `No value of <label> is selected. Nothing to repeat.` |
| `repeat-limit`, a panel | `<k> values of <label> are selected. A panel repeats <limit> times at most. Select fewer values.` |
| `repeat-limit`, a section | `<k> values of <label> are selected. A section repeats <limit> times at most. Select fewer values.` |
| `repeat-limit`, the dashboard | `The repeats of this dashboard have more than <limit> copies. Select fewer values of <label>.` |

The frame of a panel in one of these states has the title bar of the panel. The title shows after the replacement of the variables (9.13), with the selection of the page. The body shows the message in the muted colour. The frame has no menu item "View data", no "Inspect" and no "Download CSV".

A repeated section in one of these states shows its header once. It shows one line with the message in place of its panels.

A frame in the state `idle`, `repeat-empty` or `repeat-limit` has the stored `grid` of its panel. For direction `h`, it has the full width. Its extra height E is 0.

The copies and their keys:

- Each copy is an instance. It has a key. The key of the first copy is the id of the panel. The key of the copy n, with n from 2, is `<id>-r<n>`. The page does not use the value in a key, because a value can have 1,024 bytes of any text.
- Grafana names a copy in `viewPanel` by its value: `<x>$panel-<id>` (9.5). The page reads this form and finds the copy whose value is `<x>`. It writes the key of that copy (10.2a, rules 1 and 16).
- In a repeated section, the number n is the number of the copy of the section. A panel of the copy n has the key `<id>` for n = 1, and `<id>-r<n>` for n from 2.
- A panel that does not repeat, in a section that does not repeat, has one instance and its key is its id.
- The runtime (9.8) keeps one entry for each instance, with its key. The scheduler, the state of the frame and the result are the entries of the instances. A change of the list of values makes new instances. The old instances leave with their requests aborted. An instance that has the same key and the same value stays with its result.

The title of a copy:

- A copy shows the title, the description, the text of its panel links and the title of its section with the placeholders replaced (9.13). In this replacement, the selection of the repeat variable is the value of the copy. Thus `${host}` gives the option text of the copy. The other variables keep the selection of the page.
- A title with no placeholder of the repeat variable is the same in each copy. The editor warns about this case (10.3). The accessible name of a copy of a panel and of a copy of a section adds the option text (9.26).
- The text is plain text. The page writes it with `textContent`. An option text such as `<b>x</b>` shows as these characters.

The queries of the copies:

- A copy that queries sends `POST api/dashboards/<uid>/panels/<id>/query` (6.20). The `<id>` is the stored id of the panel, never a key. The member `variables` is the member of the page for all the variables. One entry changes: the repeat variable has `all` equal to `false` and `values` with the one value of the copy. Nothing else changes.
- The value goes through the same server code as each value of a panel with no repeat. This code is the check of 7.7, the property P (7.11), and the rules PG (7.24) and PM (7.35). The server cannot see that the request is for a copy. The log line has the id of the panel, and it never holds the value (14).
- In the mode `authzen`, each copy asks the PDP the question of a panel query: `dashboard.read` on N(uid) (11.21). The count of questions has the same bound as the count of queries.
- The values of a repeat cost no request. They come from the options that the variable loaded for its control (9.10). A repeat adds no request to read them.
- A trigger of 9.8 makes a panel with a repeat start one query for each copy that is near the viewport in an open section. It starts no other query. The number is k at most, so `repeat_limits.panel` at most.
- A repeat of a section with r copies and p panels that query starts r times p queries at most. The budget of the dashboard bounds the sum of all the repeats to `repeat_limits.dashboard` queries for one trigger. The panels with no repeat start the queries that they started before.
- The copies are in the queue of the scheduler (9.8). Their order is the order of the sections, then of the panels by displayed `y`, then `x`, then the number n. The page sends 6 query requests at most at the same time, copies included.
- Each copy takes its permits and its place in the admission of 8.8. A copy that waits more than `queue_wait` shows the error `503 source_busy`, as a panel does. "Retry" on a copy runs this copy only.
- Two copies never share a response. The page merges no request and keeps no result for a value that left the list.
- A copy outside the viewport sends no query. The page finds the copies near the viewport with the same observer as for a panel (9.8). A copy in a collapsed section copy sends no query.
- A change of the selection of the repeat variable makes the copies again. The page aborts the requests of the old copies, and each new copy queries when it is near the viewport. A change of another variable that the SQL names runs each copy of the panel again, as for a panel with no repeat. A new time range and a refresh run each copy again.

The layout of a panel with `repeat`. The unit is the row unit of 4.14. Let h be `grid.h`, and let m be `max_per_row`, or 4 when it is absent.

- Direction `h`. Let c be the smaller of k and m. The panel makes a band with the full width of the section.
  - The band has c columns of the same width, with the gap of 8 px between them. A copy has the height of h row units.
  - The copy n is in the column ((n - 1) modulo c) + 1 and in the row ((n - 1) divided by c, rounded down) + 1.
  - The last row is not stretched: its copies keep the width of a column.
  - The band has q rows, with q equal to k divided by c, rounded up. Its height is q times h row units. With c = 1, the copy has the full width.
- Direction `v`. The copy n has the `x` and the `w` of the panel, and its first row unit is `y` + (n - 1) times h. The region has k times h row units.
- The extra height E of a repeat is the height of its region minus h. It is (q - 1) times h for `h`, and (k - 1) times h for `v`.
- The other items of a section move down as these steps say. An item is a panel or a placed control. A section with no panel that repeats has no move.
  1. The columns of an item are `x` to `x + w - 1`. The columns of a panel with `repeat` and direction `h` are 0 to 23.
  2. An item P is above an item Q when `y + h` of P is at most `y` of Q. The columns of P and Q must have a common column.
  3. The move of Q is 0 when no item is above it. Else it is the largest value of (move of P + E of P) for the items P above Q. E is 0 for a panel with no repeat.
  4. The displayed first row unit of Q is `y` of Q plus its move. The page computes the moves in the order of `y`, `x` and `id`.
- The validation of 4.13a (rule 5) gives a band its own rows, so no other item is beside a band. A panel beside a region of direction `v` does not move. A panel below the region moves down by the extra height, and so do the panels below it, in the same columns.
- A section copy needs no rule of the grid. The copies of a section are one after the other, in the order of the values, with the gap of the sections. Each copy has the grid of the section as stored. A repeated section holds no panel that repeats, so no move applies in it.
- In the page the copies are CSS items. A copy of direction `v` is an item of the grid of the section, with `grid-column` and `grid-row` from the numbers above.
  - A band is one item of that grid, with `grid-column: 1 / span 24`.
  - It has an inner grid with `grid-template-columns: repeat(c, minmax(0, 1fr))`, `grid-auto-rows: 30px` and the gap of 8 px. Each copy has `grid-row: span h`.
  - The height of the band equals the formula of 9.8.
- A narrow screen (768 px or less, 9.8) has the flow. The copies are items of the flow in the order of the displayed `y`, then `x`, then the number n. A copy of direction `h` takes one column when c is 2 or more, and both columns when c is 1. A copy of direction `v` follows the rule of its `grid.w`. The floors of the heights apply to a copy as to a panel.
- Print keeps the copies as they show. A copy stays on one page.

A repeated section:

- The copy n of a section has its header and the grid of the section. The title has the placeholders replaced for the value of the copy (as above). The copies are in the order of the values, one after the other.
- Each copy opens and closes by itself. At the start, each copy has the `collapsed` of the section. In view mode, the state of a copy is not stored (9.8). A collapsed copy sends no query.
- The attribute `data-testid` of the copy 1 is `section-<index>`, and of the copy n it is `section-<index>-r<n>`. The `<index>` is the index of the section in the document.
- A section copy holds no placed control (4.13a, rule 7).

Print, focus and the line of the values:

- The print line of the variable values (9.8) does not change.
- A change of the selection of the repeat variable does not move the focus. The focus stays on the control of the variable.
- The panel menu of a copy has "View" for this copy (`viewPanel`, 10.2a), "View data" and "Inspect" for this copy. In edit mode, "Edit" opens the panel editor of the stored panel (`editPanel=<id>`). The other entries of 10.2 act on the stored panel from each copy.

### 9.8d Queries of a panel

A panel can have several queries (4.15a). The runtime gives each instance of a panel (9.8c) one entry for each visible query. The entry is a slot. A slot has the ref, the query, a frame, an error and its own controller. Its state is `idle`, `queued`, `loading`, `ready` or `error`. A hidden query has no slot.

The pure module `queries.js` (9.2) holds the rules of this section that need no DOM.

The requests:

1. A slot sends `POST api/dashboards/<uid>/panels/<id>/query` (6.20). The body is the body of the panel. It adds `ref` when the panel has `extra_queries`. A panel with no `extra_queries` sends no `ref`, so its request does not change.
2. The queue holds slots in this order: the order of the instances (9.8), then the order of the queries in the document. A slot is in the queue under the conditions of 9.8 for its instance.
3. The page sends 6 requests at most at the same time. A slot is one request. An annotation query (9.19d) is one request too. It stays after the slots of all panels in the queue.
4. The panel members apply to each slot. They are the time override, `max_data_points` and `min_interval` of `data.query` (4.15a, rule 4). All the slots of one panel use the same range.
5. The dirty state is a state of the slot. A time range, a refresh and "Retry" mark all the slots of the instance. A change of a variable marks the slots whose query names a variable that changed. The server gives these names (9.10). A change aborts only these slots. The other slots keep their frames, and the panel draws again when the aborted slots end.

The bound of one trigger:

- Before this feature, one trigger started 319 requests at most with the default limits of 13.8. The document has 200 panels at most. 199 of them do not repeat, and the copies of a repeat make 120 requests at most (9.8c).
- After this feature, one trigger starts 519 requests at most with the same limits. The document has 400 queries at most (4.15a, rule 6). A panel that repeats has one query (rule 8). The copies of a repeat make 120 requests at most, so the sum is 399 plus 120.
- A trigger starts a request for a visible query only. A panel that is out of view starts none (9.8). A panel in a collapsed section starts none, and neither does a panel in a repeat state with no query (9.8c). A hidden query starts none.
- The page never starts a request that the document cannot name, and never sends two requests for one slot at the same time.
- The 6 requests at the same time and the admission of 8.8 are the real bound on the load of a source. The bound of one trigger limits the work in the queue.

The state of the panel is a function of its slots. The page computes it after each change of a slot:

| Slots | State of the panel |
|---|---|
| No visible query | `empty`: the body shows "No data". The page sends no request. |
| A slot is `loading` | `loading` |
| No slot is `loading`, and a slot is `queued` | `queued` |
| No slot loads or waits, and a slot has not started | `idle` |
| All slots ended, and at least one has a frame | `ready` |
| All slots ended, and none has a frame | `error` |

- The panel draws when all its slots ended, not before. During a refresh, the earlier content stays until then (9.11). A slow query thus makes the panel wait for it, as in Grafana.
- In the state `error`, the body shows the error of the first slot in the order of the queries, with its code. The message starts with "Query <ref>: " when the panel has `extra_queries`. The message of a panel with one query does not change. "Retry" runs all the visible queries.
- A partial result is a `ready` panel with at least one slot in `error`. The panel draws the frames of the slots that have one. The title bar shows the red badge (9.11). Its tooltip and its accessible name are the text "Error:" and then one line for each failed slot: "Query <ref>: <code>: <message>". The attribute `data-failed` of the frame has the refs, separated by `,`. It is absent when no slot failed.
- A partial result is not an error state, and "Retry" does not show. The failed slots run again at the next trigger that marks them. This is the behaviour of Grafana, which draws the other frames and shows a badge (measured, 19.20f).
- A request error that has no relation to one query gives the same error to all the slots. The page aborts the other requests of the panel. These errors have the status `401`, `403`, `404`, `409` or `413`, or the code `pdp_unavailable`. A `409 dashboard_changed` has the effect of 9.8 for the whole page. The other errors are the errors of one query. A time override with an unusable range (9.8b) is an error of all the slots too.
- A frame is never partial. A slot has a whole frame (7.16) or an error. The server never cuts a frame (8.5).
- The names of the series do not change when a slot fails. The prefix rule counts the visible queries, not the answers (9.22a, rule 5). A matcher or an override by name stays valid in an outage of one query.

The view and the reuse:

- `viewPanel=<id>` runs the visible queries of that panel with the width of the view, as 9.8 says for one query.
- A `reuse` panel (9.23) shows the result of the slots of its source. The source runs its slots also out of view. A change of one slot of the source draws the reuse panel again when all the slots ended.
- "View data" shows the fields of the panel as drawn (9.22a). "Inspect" shows one block for each answered query (9.11).

### 9.8e Select and copy the text of a panel

A person can select the text of a panel with the pointer and copy it. This holds for the cells of a table, the headers, the legends, the links of the cells and the text that a chart draws.

1. The legend items, the sortable headers of a table and the links of the cells are controls. A browser does not select the text of a control by a drag. The module `copy_text.js` makes the selection from the pointer.
2. A drag over the text of such a control selects the text. The click that ends the drag does nothing: no series toggles, no column sorts and no link opens.
3. A double click selects a word. A triple click selects the text of the control. The second and the third click do not reach the control. The first click acts as a plain click, because the page cannot know that more clicks come.
4. A plain click keeps its action: it toggles a series, sorts a column or follows a link. The keyboard keeps its action too: Tab, Enter and Space.
5. The key Ctrl+C or Cmd+C on a control that has the focus, with no selection, copies the text of the control.
6. A link in a panel does not start the drag of the link when the pointer goes down on its text.
7. A chart draws its axes, its titles and its labels on a canvas, and a browser cannot select the text of a canvas. Each chart canvas that takes the pointer has a layer of text over it. The module `text_layer.js` makes the layer.
8. The layer holds the texts of the last draw of the canvas. They are transparent and have the same places, font, alignment and turn. The layer is hidden from assistive technology, because the panel gives its data in text already.
9. The layer takes the pointer over the box of each text only. The pointer over the rest of the plot goes to the chart. The tooltip, the cursor and the zoom by a drag work as before. A chart can draw a label inside its plot, for example the state of a segment. The layer then sends the move and the leave of the pointer to the canvas, so the tooltip stays. It sends no press and no click.
10. The layer changes only when the texts change. A draw that gives the same texts at the same places keeps the layer. A draw that changes a text, a place or a font replaces it, and a selection in the layer ends at this draw.
11. The texts are in order of use: the top row first, and from left to right in a row. A layer holds 400 texts at most. The texts after the 400th are not in the layer, and the chart still draws them. A draw stops to record after 800 texts.
12. A copy of a selection that is in the layers only gives the selected part of each text, with one text on each line. A copy of any other selection uses the copy of the browser.
13. The value at the end of a bar of the horizontal bar chart takes the pointer, so a person can select it. The text of a gauge cell of a table is selectable already.
14. The page does not change the chart engine of the copy of ClickHouse Dash. The two modules wrap `fillText` and `clearRect` of the 2D context one time.
15. The control of a variable is a select, and it does not offer its text for selection. The tooltip of a chart is a box that shows for a short time. The list page has its own rules (9.7).
16. A log line is a row of cells: the time, the level and the body. A copy of a selection over log lines gives one text line for each log line. The cells are joined by one space, and one line end separates the lines. The module `row_copy.js` builds this text. A selection that starts or ends inside a cell gives the selected part of that cell.
17. A selection inside one cell is the text of the cell. A selection that leaves the list of rows, and a selection that holds the details of a line, use the copy of the browser. A table keeps the copy of the browser, with a tab between two cells.
18. The rows of the horizontal bar chart and of the bar gauge follow item 16. The name and the value are on one text line. A cell that is hidden from assistive technology is not in the copy.
19. A triple click on a log line selects the whole line: the time, the level and the body. A double click selects a word. Neither leaves the details of the line changed. The second click gives back the toggle of the first click, and the third click does not toggle. A plain click toggles the details as before.
20. The key Ctrl+A or Cmd+A with the focus on a log line selects the lines of the panel and not the whole page. A copy then gives all the lines, one text line for each.
21. The cursor over the text of data is the text cursor. This holds for the cells of a log line, the label of a legend item and the cells of a table. It holds for the titles and the labels of an axis. It holds for the values of a stat, a gauge or a bar, and for the text of a text panel. A control keeps its own cursor: a button, a link, a select and a sortable header have the pointer. In the edit mode, the title bar of a panel keeps the cursor `move`.
22. The right button never acts in a panel. A click of the right button on a selection keeps the selection. It toggles no line, no series and no sort, and it opens the menu of the browser. The page never stops this menu. The item Copy of the menu gives the same text as Ctrl+C, because both send the event `copy`.

### 9.9 Time picker and refresh

The controls of the dashboard page are in the header (9.6), in one row. The variables bar (9.10) has its own row under the header.

The controls come in this order, from the left:

1. The group of the edit mode (10.2): the button "Add" with its menu, "Save" and "Revert". It is in the page in edit mode only. It takes its room from the selector, so no button of the view moves.
2. The time control, which is one joined control.
3. The switch "Edit" (10.2). It shows only with `editing` true and `access.write` (10.2).
4. The theme button (9.25).
5. The button "More actions" (`data-testid="dashboard-menu"`).

The first three are in the controls part of the header. The theme button and "More actions" follow them. Each control keeps its `data-testid` and its accessible name. The order of the keyboard focus is the same as this visual order.

The menu "More actions" has these items, in this order. An item shows only when the user has the right for it:

- Up to 900 px: "Shift the time range back", "Shift the time range forward" and "Zoom out", then a separator. The buttons of these actions hide at this width.
- In edit mode: the actions of 10.2, then a separator. They are "Revert all changes" (`data-testid="discard-menu"`, up to 640 px only), "Dashboard JSON" and "Settings". The menu of the button "Add" has "Add panel" and "Add section" (10.2).
- "History" (`data-testid="history-button"`). It shows only when `access.history` is `true`. A dashboard of a read-only store has no history, so it never shows there.
- "Export" (`data-testid="export-button"`). It shows only when `access.export` is `true`.
- A separator, then "Delete" (`data-testid="delete-button"`). It shows with `editing` true and `access.write`. It opens the dialog of 10.13, so the delete always asks first.

The time control has two groups, and its buttons touch. Only the outer corners are round. There is no second refresh button and no separate interval control.

- The time group has four buttons: "Shift the time range back", the range button, "Shift the time range forward" and "Zoom out".
- The refresh group has two buttons: the refresh button and the interval button.

A header button is 32 px high. It has a text of 14 px (weight 500), an icon of 18 px and a radius of 6 px. The header is 44 px high (9.6). The range button shows the quick range text, or the absolute range.

Each button of the header has one place and one size in each state. Above 1100 px, the sizes are these:

- The range button is 220 px wide. The text takes the room between the clock and the chevron. A longer text, such as an absolute range, ends with an ellipsis. The tooltip and the accessible name give the whole text.
- The refresh button is an icon button of 36 px. The spinner has the size of the icon. The accessible name and the tooltip stay "Refresh".
- The interval button is 58 px wide, with a value or with a dash.
- The switch "Edit" is 76 px wide. A user with no right to write has no switch and no group of the edit mode.
- The button "Add" has a main part of 68 px and a menu part of 28 px. The two parts touch. "Save" is 64 px wide and "Revert" is 36 px wide. The gap between them is 8 px.
- "More actions" and the theme button have the same size in each state. "Add", "Save" and "Revert" keep their size when they are not active.

The sizes change in steps on a narrow window (9.6):

- Up to 1100 px, the range button is 156 px wide, and the switch is 44 px wide. Its label hides. "Save" is an icon button of 36 px.
- Up to 768 px, the range button is 52 px wide, with the clock and the chevron.
- Up to 900 px, the time shift and the zoom out are in the menu "More actions".
- Up to 900 px, "Add" is an icon button of 36 px. Its name and its tooltip stay "Add panel". The menu part stays 28 px wide.
- Up to 640 px, "Revert" has no button in the header. The menu "More actions" has the item "Revert all changes".
- Up to 560 px, the range button is 40 px wide, the switch is 36 px wide and the interval button is 28 px wide. The interval value hides, and the chevron has the accent colour when an interval runs. The menu part of "Add" is 20 px wide.
- Up to 480 px, the icon buttons are 32 px wide.

Tooltips and accessible names stay at each width. The header sticks to the top at each width (9.6). The browser case B59 checks the places, the sizes and the sticky header. The case B71 checks the one row.

The height and the size of each button are fixed. No button of the view changes its place when the edit mode starts or ends. These rules apply:

- The time control, the switch "Edit", the theme button and "More actions" keep their x and y between the view and the edit mode. The browser case B150 measures this at 1920, 1280, 900, 768 and 390 px, in the light and the dark theme.
- The group of the edit mode is at the left of the time control, one gap from it. Its width pushes only the selector.
- The selector and the brand are the only parts that change width. The selector gets smaller by the width of the group plus one gap. Its title ends with an ellipsis, and the tooltip has the whole text.
- The gap is 8 px, and 4 px up to 560 px for the time control and the group. The height of the header does not change.
- The switch "Edit" does not move, so it stays under the pointer. The size of the selector is the only change that the eye sees.
- The switch makes two layouts, the layout of the view and the layout of the edit mode, and no layout between them. The page puts the group in the same step in which it shows the editor view (10.2a). A sampler of the frames in the browser case B59 checks this. The end of the edit mode is one step too.
- The other views of the editor keep the group in its place. These views are the panel editor, the JSON, the settings and the history. "Add" is not active there.

The row of the variables bar has 12 px above the variables and 8 px below them. A dashboard with no control in the bar and no dashboard link (9.8a) has no such row (9.10). The variables take the whole width of their row, and they wrap inside it. The gap is 8 px between the buttons of the header.

Every menu of the page has the same style. It has no border, a radius of 6 px, items of 32 px and icons of 16 px. It opens 8 px below its button.

The grammar of date math is a subset of the grammar of Grafana. It is the time text of 4.4:

```text
time-text = relative / absolute
relative  = "now" [ "-" count tunit ] [ "/" tunit ]
count     = %x31-39 *8DIGIT               ; 1 to 999999999
tunit     = "s" / "m" / "h" / "d" / "w" / "M" / "y"
absolute  = "0" / ( %x31-39 *14DIGIT )   ; epoch milliseconds, at most 253402300799999
```

`t` is the current time in epoch milliseconds. `Z` is the zone of the dashboard. It is the zone of the browser for `browser`, UTC for `utc`, and the named zone for an IANA name (4.6). The page computes the epoch milliseconds of a relative time in these steps (`FE-01`):

1. It starts from `t`.
2. `-<count><unit>` subtracts. `s`, `m` and `h` are fixed milliseconds. `d` and `w` are days and 7-day weeks of the
   calendar of `Z`, so that a change of daylight time keeps the clock time. `M` and `y` are calendar months and years.
   When the day does not exist in the target month, it becomes the last day of that month.
3. `/<unit>` rounds. For `from`, the result is the start of the unit in `Z`. For `to`, it is the start of the next unit
   minus 1 ms. A week starts on Monday at 00:00.
4. The result is epoch milliseconds. When `from` is not below `to`, the picker shows an error and keeps the earlier
   range.
5. A time above 4294967295999 (2106-02-07T06:28:15.999Z, the last millisecond of the ClickHouse type `DateTime`) is
   above the bound of the server (7.3). The picker shows the error "The time is after 2106-02-07." and keeps the
   earlier range.

Example in UTC at 2026-11-09T14:30:00.000Z: `now-6h` gives 2026-11-09T08:30:00.000Z, `now/d` as `from` gives
2026-11-09T00:00:00.000Z, and `now/d` as `to` gives 2026-11-09T23:59:59.999Z.

Quick ranges of the picker:

| Text | `from` | `to` |
|---|---|---|
| Last 5 minutes, 15 minutes, 30 minutes | `now-5m`, `now-15m`, `now-30m` | `now` |
| Last 1 hour, 3 hours, 6 hours, 12 hours, 24 hours | `now-1h`, `now-3h`, `now-6h`, `now-12h`, `now-24h` | `now` |
| Last 2 days, 7 days, 30 days, 90 days | `now-2d`, `now-7d`, `now-30d`, `now-90d` | `now` |
| Last 6 months, 1 year | `now-6M`, `now-1y` | `now` |
| Today, Today so far | `now/d` | `now/d`, `now` |
| Yesterday | `now-1d/d` | `now-1d/d` |
| This week, This week so far | `now/w` | `now/w`, `now` |
| This month, This month so far | `now/M` | `now/M`, `now` |
| This year, This year so far | `now/y` | `now/y`, `now` |

- The time group has four buttons. They are "Shift the time range back", the range button, "Shift the time range forward" and "Zoom out".
- The range button shows the text of the quick range that has the same `from` and `to`, else `<from> to <to>`. An absolute time shows as `YYYY-MM-DD HH:mm:ss` in `Z`. The button has no zone text. Its tooltip and its accessible name hold the label "Browser time" or "UTC".
- A shift moves `[from, to]` by half of the span `d = to - from`. The step is `floor(d / 2)`, and it is negative for "back". The result is absolute and keeps the span `d`. A shift that passes 0 or the last time of 4.4 stops at that limit and keeps the span.
- "Zoom out" changes `[from, to]` to `[from - d / 2, to + d / 2]` with `d = to - from`. The result is absolute.
- At each refresh, the page computes a relative range again with a new `t`. An absolute range stays the same.
- Zone names. `time.js` has the function `isKnownZone(name)`. It is true for `browser`, `utc` and each name that `Intl.DateTimeFormat` accepts.
- When the dashboard loads with a name that is not known, the page uses the zone `browser`. It shows the notice "The time zone <name> is not known to this browser. The page uses the browser zone." The notice has the level `warning` and the attribute `data-notice="zone"`. It shows one time for each name in one page mount. The document does not change.
- A known name in the URL name `timezone` replaces the zone of the document for the view, and `Z` is then that zone (9.5). An unknown name in the URL gives the notice of 9.5.
- The calendar rules of a named zone are those of `Intl.DateTimeFormat`. A local time that occurs two times has the earlier instant. A local time that does not exist uses the offset before the change (`wallToEpoch` of `time.js`).
- Grafana agrees for a local time that does not exist (Paris, 2026-03-29, 02:30 gives 01:30Z). For a local time that occurs two times, Grafana gives the later instant (Paris, 2026-10-25, 02:30 gives 01:30Z, and Havana, 2026-11-01, `now/d` gives 05:00Z). This is a known difference (paper V8).
- The same rules apply to `now/d` on a day of a clock change. The end of a day is the last millisecond of the local day, so a day has 23 or 25 hours. The units `h`, `m` and `s` are fixed, and `24h` is 24 hours. The units `d`, `w`, `M` and `y` are calendar units (paper, V8).
- For a named zone, the zone label of the popup is the name. The footer shows the name and the offset, for example `Europe/Paris UTC+01:00`. The tooltip and the accessible name of the range button hold the name.

The range button opens a popup. The popup is the range picker of ClickHouse Dash. It is 604 px wide, or the width of the window less 16 px when this is smaller. It has a border of 1 px (`--border-strong`) and a radius of 8 px. Its right edge is the right edge of the time group, 5 px below it. It has two columns and a footer:

- The left column is the absolute range. It has the title "Absolute time range" and the fields "From" and "To" side by side. It has a hint, a calendar, an error text and the button "Apply time range".
- A field accepts `YYYY-MM-DD HH:mm:ss` in `Z`, or date math. A field with an error shows the error under it, and the first field with an error gets the focus. A range that the page refuses, for example `From` after `To`, shows one error text above the button. The earlier range stays.
- The calendar shows six weeks of the month of the range. A week starts on Monday. The days are the days of `Z`. The buttons "Previous year", "Previous month", "Next month" and "Next year" move it. The first and the last day of the range have the accent fill, and the days between them have a tint.
- The hint reads "Pick the start date" or "Pick the end date". A first click on a day sets `From` to that day at 00:00:00 and moves the focus to `To`. A second click sets `To` to that day at 23:59:59 and moves the focus to the button. A click on a day before the start makes a new start. While the end is open, the days between the start and the pointer have a lighter tint. A change in a field moves the calendar to the day of the field.
- The right column is 216 px wide. It has the field "Search quick ranges", the list "Recently used" when it has items, and the list "Quick ranges" of the table above. An item is 27 px high. The item of the current range has the fill of a selection and `aria-current="true"`.
- The search keeps the items whose text holds the search text, or whose `from` is the search text. A search text such as `45m`, `last 2d` or `now-3h` adds the item "Last 45 minutes" (`now-45m` to `now`) at the top of the list. The grammar is the one of 4.4. When no item matches, the list shows "No quick range matches “<text>”."
- "Recently used" holds the last two ranges that the user applied with the form or from this list. A quick range is never in it. The browser keeps the list in `localStorage` under the key `hune.timeRanges`, and all dashboards share it. A browser with no store has no list.
- The footer shows the zone and its offset at the time of the popup, for example `Browser time · Europe/Paris (UTC+01:00)`, `UTC (UTC+00:00)` or `Europe/Paris (UTC+01:00)`.
- Up to 768 px the popup is one column: the absolute range, then the search and the lists. The lists are 280 px high and they scroll. On a touch screen, each button and each item is 40 px high at least.
- The popup opens and closes with a fade of 160 ms: the opacity, a move of 6 px and a vertical scale of 0.98. It leaves the page when the fade ends. With `prefers-reduced-motion: reduce` there is no fade, and the popup leaves at once. A press on the range button during the fade opens a new popup.

The keys of the popup:

- When the popup opens, the focus goes to the field "From" with its text selected. On a screen of 560 px or less, the focus goes to the popup, so that the keyboard of a phone does not hide it.
- `Enter` in a field applies the form. `Enter` in the search applies the first item. `ArrowDown` in the search moves the focus to the first item. `ArrowDown` and `ArrowUp` move in the list. `ArrowUp` on the first item goes back to the search. `Enter` on an item applies it.
- In the calendar, one day is the tab stop. The arrow keys move by one day or by one week. `Home` and `End` go to the Monday and to the Sunday of the week. `PageUp` and `PageDown` move by one month, and with `Shift` by one year. `Enter` on a day picks it.
- `Escape` closes the popup and puts the focus back on the range button. A Tab out of the popup, or a press outside it, also closes it.

Hune keeps these points, which ClickHouse Dash does not have:

- The quick ranges are the table above.
- The fields accept the grammar of 4.4 and the zone of the dashboard. A bare date is not a time (4.4).
- The back, forward and zoom out buttons are in the time group, not in the footer of the popup.
- A range has no maximum span, only the bounds of 4.4.
- The popup has no copy button, no paste button and no link to time settings. Hune does not have these features.

Refresh:

- The picker offers "Off" and the values of `refresh_choices` (6.10): `10s`, `30s`, `1m`, `5m`, `15m`, `30m`, `1h`,
  `2h` and `1d`. These are the values `REFRESH_CHOICES` that the format permits for `time.refresh` (4.6), so the page
  always runs a stored value.
- Only a URL value can be outside this list. It gives "Off" and the notice "The refresh value <value> is not offered.
  Refresh is off.".
- The refresh group has two buttons: "Refresh" and the interval button. They are part of the time control and they touch the time group.
- A click on "Refresh" always refreshes. The key Enter or Space on the focused button does the same. It aborts the queries in flight and runs them again. While queries run, a spinner replaces the icon of the button. The name stays "Refresh".
- The interval button is a small chevron. It shows the value, such as `30s`, beside the chevron when an interval runs. It shows a dash, "-", when there is no interval, so it is never empty, and its accessible name stays "Refresh interval: Off".
- The interval button opens a menu with the items "Off" and the values of `refresh_choices`. The key Enter or Space opens it on the current item. The arrow keys move, Enter chooses, and Escape closes it and gives the focus back to the button.
- The items are radio items (`role="menuitemradio"`). The current item has a neutral fill, no check mark and `aria-checked="true"`.
- The key Tab goes through the time control in the order of the picture. The order is: "Shift the time range back", the range button, "Shift the time range forward", "Zoom out", "Refresh" and the interval button.
- A hidden native list (`data-testid="refresh-select"`) holds the value and the choices. A `change` event on it is a choice of the menu.
- A tick comes at each interval. The page skips a tick while a request of the previous refresh is in flight or queued.
- While `document.hidden` is `true`, no tick runs. When the page becomes visible again after a skipped tick, the page
  refreshes one time at once.

### 9.10 Variables bar

- The bar has its own row under the header (9.9). It hides on the JSON tab, on the history, on a version and on a diff (10.4, 10.11). It has one control for each variable that has no `placement` (4.7a) and no `hide = "variable"` (4.7), in document order. The label is the `label`, else the `name`. With `hide = "label"`, the control has
  no visible label but keeps its `aria-label`. A hidden variable keeps its selection.
- A `custom` or `query` variable has a select, or a select of several values when it is `multi`. The option "All" is
  first when the variable has `include_all`.
- An `interval` variable has a select of one value. Its options are in 4.10a. "Auto" is the first option when `auto` is `true`, and its text is `Auto`. The control has no "All" and no `multi`. It uses the code of the `custom` select (`optionsOf`).
- The selection of an `interval` follows the rules of a closed `custom` variable (F41). The order is the URL value, the current value, `default`, then the first option. The page drops a value that is not an option, with the notice of this section. The page never computes the text of "Auto": the server does (7.4a).
- A `constant` has no control and no place in the bar. The selection of the page is its `value`. The page never reads the URL value of a `constant` and never writes one. The request body holds the `value` (6.20), and the server compares it with the document.
- A variable that is open (`allow_custom_value` = `true`, 4.8, 4.9) has a select with free entry: its popup has an editable combobox. A closed variable has the popup that this section describes first. The text below says what is different for the open variable.
- A `text` variable has a text field. A change applies on `Enter` or when the field loses the focus. With
  `value_type = "number"`, a value that breaks the number grammar of 7.7 shows an error and does not apply.
- The select of one value shows its text. The select of a `multi` variable shows chips (below). Its name for assistive technology has the selected texts joined by " + ", the first three only, then " + <n> more". Its popup has a list
  with `role="listbox"` and a search field. The search keeps the texts that hold it, with letters folded to lower case.
- The cost of a large selection. The select finds the texts of the three values that it shows only, in one pass over the options. The popup keeps the selected values in a set. Thus a paint of the select and a key in the search field take a time that is linear in the count of options. With 10,000 selected values of 1,005 characters and 10,000 options, each one takes 100 ms or less (`page_security_v3_low`).
- The popup of a `multi` variable that has options has a row "Selected (n)" before the list. The number n is the count of
  the selected values, and "All" counts as 1. A click on the row selects all options: this is "All" when the variable has
  `include_all`, else every option. When all options are already selected, a click keeps the first option only.
  The row is a button with `aria-pressed`. It is not an item of the listbox.
- The rows of the popup are 38 px high, with 8 px of padding and a checkbox of 16 px. The popup is at least 170 px wide.
- Keys of the popup: the arrows move, and `Space` toggles a value of a `multi` variable. `Enter` selects or applies,
  and `Escape` closes with no change. A change of a `multi` variable applies when the popup closes. A Tab out of the popup closes it, and a `multi` selection applies. These keys are the keys of a closed variable. An open variable has the keys of the combobox (below).
- The URL holds the selection (9.5): one `var-<name>` for each value, or `var-<name>=$__all` for "All". A typed value stays in the URL while the variable is open.

Placed controls (4.7a):

- A variable with a `placement` has its control in the grid, in the frame of the section that the `placement` names. The bar never shows it, with the exceptions below.
- The page computes the place of each control before the first frame, from the document. Thus the grid does not move when the options load (case B58).
- The control is the same code as the control of the bar (`vars.js`). It has the same popup, the same keys, the same search field and the same option rules. A `custom` or a `query` variable has the select of this section. A `text` variable has the text field.
- The control has one selection for each variable (`rt.selections`). A change in the grid has the same effects as a change in the bar. It writes the URL value `var-<name>` and reloads the options of the variables below it. It also runs the panels whose SQL names it. A change of a variable that a `repeat` names also makes the copies again (9.8c). The values of the copies come from the options that this variable loaded.
- The frame has the place and the size of the `placement` in the grid of its section (4.14). The label follows `hide`: `none` shows the label chip at the left of the control, as in the bar. `label` shows the control only. The chip has an ellipsis when the width is short, and the control takes the rest of the frame.
- The control keeps its hooks `variable-<name>`, `variable-<name>-button`, `variable-<name>-input` and `variable-<name>-search` (9.29). A variable has one control in the page, so each hook occurs one time. The states and the chips add the hooks `variable-<name>-chips`, `variable-<name>-remove`, `variable-<name>-fail`, `variable-<name>-retry` and `variable-<name>-detail`.
- The bar has zero height when it has no control: no margin, no border and no gap. The row under the header (9.9) has `display: none` when the bar and the group of the dashboard links (9.8a) are both empty. When the dashboard has links, the row stays for the links, and the bar part of the row has zero width.
- A variable with no `placement` stays in the bar. A new variable has no `placement`, so it shows in the bar until a person places it (10.3b, 10.6).
- A placed control in a collapsed section (4.12) is hidden with the section. The variable stays out of the bar, so the person opens the section to change the value. The value still applies and shows in the print line (9.8). The other choice, which Hune does not use, shows in the bar each variable whose frame is not visible.
- When one panel (`viewPanel`, 10.2a) or the panel editor takes the main area, the grid is not visible. The bar then shows each variable that has no `hide = "variable"`, also the variables that have a `placement`. Each variable has one control in the page at all times. The grid view shows the bar as above again.
- In edit mode (10.2), the draft grid draws the placements of the draft. The control of a variable that the saved document has works on the selection of the page. The control of a variable that only the draft has (new or renamed) works on a selection that lives in the editor only.

Options of a `query` variable. The page takes the answer of 7.14 and does these steps (`FE-02`):

1. Regex. When the variable has `regex` (4.8, 4.24), the page makes a JavaScript `RegExp`. The text `/re/flags` gives
   `new RegExp("re", "flags")`, and another text `t` gives `new RegExp("^" + t + "$")`. The page tests the option
   text, and removes an option with no match. An option text of more than 1,024 code units never matches (9.14).
2. Groups. The named groups `value` and `text` set the value and the text. Else the first group sets both. With the
   flag `g` and several matches, each match gives one option.
3. Duplicates. Of the options with the same value, only the first stays.
4. Sort, with the member `sort` of 4.8:

| `sort` | Order |
|---|---|
| `none` | The order of the rows. |
| `alpha_asc`, `alpha_desc` | Text in code unit order, up or down. |
| `alpha_ci_asc`, `alpha_ci_desc` | Text with letters folded to lower case, up or down. |
| `num_asc`, `num_desc` | The first number in the text (`-?[0-9]+(\.[0-9]+)?`), up or down. Options with no number come last. |
| `natural_asc`, `natural_desc` | `Intl.Collator("en", {numeric: true})`, up or down. |

Selection:

- At load, the requested values are the URL values, else the current selection, else the member `default` (4.11).
- A requested value stays when it is an option. A closed variable keeps no other value. An open variable also keeps a typed value: a value of 1 to 1,024 bytes, with no control character, that is not `$__all`. The value `$__all` stays only with `include_all`.
- When no value stays, the page takes the default items that stay under the same rules. Else it takes "All" when the variable has `include_all`, else the first option. Else the selection is empty.
- The page drops a value that is not an option, also when it comes from `default`. It shows no error and it does not block the dashboard. It shows one notice (below).
- After new options of a variable, the same rules apply to its current selection. New options come from a reload (`refresh`) or from a change of a parent. An open variable keeps its typed values after the reload.
- A `query` variable without `multi` and without options has the empty selection. The control shows "No options" (below), and the requests send `values: []` for it (7.7).
- The page never sends a typed value to the server before the person applies it. A typed value starts no options query and no panel query.
- The check uses a set of the option values. The time is in proportion to the count of the requested values and of the options, not to their product (T78, FE-48).

Notice of a replaced value:

- The page shows one notice for a variable when it drops at least one requested value and the variable has an option. An open variable drops only a text that is not a typed value. The level is `info`. The hook is `data-notice="variable"`. The key of the notice is `variable-` and the name of the variable, so a new notice of this variable replaces the old one.
- The text is: `This value is not an option of <label>: "<value>". The page used "<text>".` With more than one value, the first words are "These values are", and the list has 3 values and then "and <n> more". A value of more than 40 characters is cut, with `...`. The text after "used" is the text of the first 3 selected options, or "All", or "no value".
- The runtime tells the page with the option `onNotice` of its constructor. The call comes after the selection is set. The page writes the new value of this variable in the URL with `history.replaceState` (9.5) and keeps the rest of the query. Thus a reload of this URL shows no second notice.
- The page shows no notice when the options query failed, because the control shows the error and keeps its selection (below). It shows no notice for an empty list, because the control shows "No options".
- The notice uses the area of the notices (9.6, 9.26): `role="status"` with `aria-live="polite"`.

No options:

- A `query` variable whose options loaded with no row has no value. The button shows the muted text "No options". Its name for assistive technology is `<label>: no options`. The popup shows "No options" for a closed variable and "No options. Type a value." for an open variable.
- With `include_all`, the variable has "All" and the control shows "All".
- A panel that names the variable runs with the empty value of 7.7. The page adds no new error. The panel shows what the query gives, for example no data.

Free entry:

- The popup of an open variable has the parts of the popup of a closed variable. They are the field, the row "Selected (n)" of a `multi` variable, and the list. The field is a combobox with the placeholder "Search or type a value".
- The field has `role="combobox"`, `aria-expanded="true"`, `aria-controls` (the list), `aria-autocomplete="list"` and `aria-activedescendant`.
- The text in the field filters the options, as the search of a closed variable does. When the text is a typed value, the list ends with the row `Use "<text>"` (hook `variable-<name>-use`). The text of the row is cut at 40 characters. The row is absent when the text is the `value` or the `text` of a row, and when the text is not a typed value.
- A typed value is a text of 1 to 1,024 bytes with no control character, and it is not `$__all`. For another text, the field shows an error below it. The error has `role="alert"` and the field has `aria-describedby` to it. The text is "A value has 1 to 1024 bytes.", "A value has no control character." or "This value is reserved.". The row `Use` is absent. An empty field shows no error.
- A typed value that the selection holds is a row of the list, before the options. It has `aria-selected="true"`. A `multi` variable shows each typed value as a checked row, in the order of the selection. A click on a checked row removes the value, and the row leaves the list. The control shows the same value as a chip (below).
- The DOM focus stays in the field. The arrow keys move the active row (`aria-activedescendant`) through the visible rows, with the row `Use` last. `Enter` chooses the active row. With no active row, it adds the typed text, or chooses the row whose `value` or `text` equals the text.
- A variable that is not `multi` applies the choice and closes the popup. A `multi` variable keeps the popup open, empties the field and toggles the value. `Enter` in an empty field closes the popup and applies the selection. `Enter` on a text that is not a typed value does nothing.
- `Space` types a space. `Escape` closes the popup with no change. `Tab` and a click outside close the popup, and a `multi` selection applies, as for a closed variable. `Home` and `End` move the cursor in the field. The row "Selected (n)" is not in the Tab order of an open variable.
- The value is in the URL as `var-<name>`, one for each value, in the order of the selection. It is in the selection of the page and in the selection of the editor draft (10.2). It shows in the select as the text of the value.
- A repeat by an open variable makes one copy for each selected option. A typed value makes no copy, as a value that is not an option never does (4.13a).
- The request holds the typed value as a free value (7.7). For a `custom` variable, the server accepts it only when the variable is open.

Variable graph:

- The parents of a `query` variable are the variables that its SQL names (7.14). The page does not read a query text. The server gives the
  names in the member `names` of the answer of 6.14, which it makes with the dependency scan of 7.19. The page needs no lexer. It does not
  need the kind of the source, which a viewer cannot read.
- The page reads the member `names` in three places. `names.variables.<name>` gives the parents of a `query` variable. `names.panels.<id>` gives
  the variables that the queries of a panel name: the union of its lists. `names.annotations[i]` gives the variables of an annotation.
  A reuse panel and a text panel have no key, and the reuse panel follows its source (9.23).
- The page keeps two filters of its own. A copy of a repeat (9.8c) does not name the variable of its repeat. A variable never names itself.
- The page checks the shape of `names` before it builds the runtime, and it stops at the first fault (fail fast).
- The rules of the shape: `names` MUST be an object with the
  keys `panels`, `variables` and `annotations` only. The keys of `names.panels` MUST be the ids of the panels that have `data.query`. The keys of
  `names.variables` MUST be the names of the `query` variables. `names.annotations` MUST have one item for each annotation.
- Each list MUST be an
  array of distinct names of variables of the document. A panel MUST have one list for each of its queries (4.15a).
- When `names` is absent or breaks one of these rules, the page stays usable and never shows old data. The runtime then lets each panel and each
  annotation name every variable. It lets each `query` variable name every other variable.
- Thus each change of a variable runs each panel and each annotation again, and it reloads each other `query` variable.
  The page shows one notice with the hook `data-notice="names"`. The notice says
  that the server did not give the variable names. It gives the fault. It says that Hune runs every query when a variable changes. The notice shows
  once for the page, and a reload of the document after a `409` does not show it again.
- The cause is a page that is newer than the server, or a broken answer. In this case the page does not read a query text and does not guess.
  It chooses more queries over a missed run, because a missed run shows old data.
- The etag of the document does not cover `names` (6.14). The page thus reads `names` from a fresh answer of 6.14. It never keeps `names` of an
  earlier answer for the same etag. A new server can give other names for the same bytes.
- The editor keeps `sqlscan.js`, `sql_lex_pg.js` and `app_sql_lex.js` for its own needs. These are the hint of a repeat, the notice of a rename,
  the preview and the variable graph of the forms (10.3, 10.6). The editor loads them on demand, so they are not in the measure of 9.30.
  The runtime of the page gets its member from the server and never imports them. The tests of the runtime make the member with the scan of the editor.
- The editor does not give the draft to the runtime of the page. The draft grid uses the runtime of the saved document (10.2). Thus the member
  `names` of the saved document is the only one that the runtime reads.
- At load, the page loads the options in the order of the graph (document order between variables with no link). A variable that has a parent waits for it (below).
- A change of a variable reloads the options of each `query` variable below it in the graph, in graph order. Then the
  page runs each panel whose SQL names a changed variable. A reuse panel follows its source (9.23).
- A `query` variable with `refresh = "time_change"` also asks for its options at each change of the time range. It does
  the same at each refresh, before the panels run. The cache (below) can answer in place of a request. With `refresh = "load"`, it asks only at load and after a change of a parent.
- The query request holds each variable of the document, so that it never depends on the lexer of the page (7.7
  permits the extra members). For "All" without `all_value` on a `query` variable, `values` holds the options that
  the bar shows.
- A query body above its limit of 256 KiB (6.27) gives `413 payload_too_large`, for example with a long "All" list.
  The panel then shows the error "The selection of the variables is too large. Select fewer values."

State of the control:

- A `query` variable has one state: `loading`, `waiting`, `ready` or `error`. The other kinds of variable are always `ready`.
- At load, a variable with no `query` parent is `loading`. A variable with a `query` parent is `waiting` until its parent is `ready`.
- A control that is `loading` shows a spinner and the text `Loading…`. It is disabled and has `aria-busy="true"`. Its name for assistive technology is `<label>: Loading…`.
- A control that is `waiting` shows the muted text `waiting for <label>`, where `<label>` is the label of the parent (or its name). It is disabled. Its name is `<label>: waiting for <parent label>`.
- The control keeps its width and its height in each state. While the control loads, the ghost of the current selection holds the room. Thus the bar and the grid do not move when the options come (case B290).
- A selection that the page already has stays in the URL and in the requests of the panels while the control loads.

Error of the options:

- A request of options that fails sets the state `error`. The control keeps its selection and its text. It has a red edge (`is-failed`), and its name is `<label>: <text>. Options failed: <short sentence>`.
- The control shows no text of error in the bar. The error lies in the popup, over the page. Thus the bar and the grid do not move (case B291). The bar has no element `select__error`.
- The popup of a failed control has the hook `variable-<name>-fail`. It shows the short sentence, a button "Retry" (hook `variable-<name>-retry`) and a folded `<details>` (hook `variable-<name>-detail`) with the full text of the error. An open variable (4.8) also has its field, so a person can type a value.
- A click on "Retry" closes the popup and sends one new request for this variable and for the variables below it. It ignores the cache. The other variables send no request.
- The short sentence comes from the first rule that fits: the code of the API error, then the ClickHouse number, then a general sentence. The ClickHouse number is the member `clickhouse_code` of a `query_error`. Else it is the number in the text (`Code: <number>.`), else the name in the text, before `(version ...)`. The page never changes the text of the source: the folded part holds it in full.

| Code of the API | Short sentence |
|---|---|
| `query_memory_limit` | The query used too much memory. |
| `query_timeout` | The query took too long. |
| `query_limit` | The query stopped at a limit of the source. |
| `sql_invalid` | The SQL of the variable is not valid. |
| `sql_too_large` | The SQL of the variable is too large. |
| `source_access_denied` | The account of Hune has no right for this query. |
| `source_misconfigured` | The login or the certificate for the source failed. |
| `source_unavailable` | Hune cannot reach the source. |
| `source_busy` | The source is busy. |
| `server_busy` | Hune is busy. |
| `source_bad_response` | The source gave an answer that Hune cannot read. |
| `pdp_unavailable` | The access check did not answer. |
| `forbidden` | You may not read these options. |
| `not_found` | The variable or its source does not exist. |
| `network_error` | Hune does not answer. |
| `request_timeout` | Hune did not answer in time. |
| `internal_error` | Hune had a fault. |
| `query_error` | The sentence of the ClickHouse number, else "The source refused the query." |
| another code | The sentence of the ClickHouse number, else "The options did not load." |

| ClickHouse number | Short sentence |
|---|---|
| 47 | The query names a column that does not exist. |
| 60 | The query names a table that does not exist. |
| 62 | The query has a syntax error. |
| 159 | The query took too long. |
| 202 | ClickHouse has too many queries at this time. |
| 209 | ClickHouse did not answer in time. |
| 210 | Hune could not reach ClickHouse. |
| 241 | The query used too much memory. |
| 497 | The account of Hune has no right for this query. |
| 516 | ClickHouse refused the login of Hune. |

- The API gives the code `query_memory_limit` for the memory limit of ClickHouse, and its body has no `clickhouse_code`. Thus the code of the API gives the sentence of the number 241. The body of a `query_error` has `clickhouse_code`, and the page reads it.

A variable that waits:

- A variable below a variable (7.14) sends no request of options while its parent is `loading`, `waiting` or `error`. Its state is `waiting`, with the name of the parent.
- When the parent is `ready`, the variable loads. When the parent failed, the variable stays `waiting` until a "Retry" of the parent succeeds.
- Variables that name each other in a cycle load in document order. A failure blocks only the variables that name the failed variable.

A failure blocks nothing:

- A variable whose options failed keeps its selection: the URL value, else the current selection, else `default`. This holds for `$__all`, for several values and for a typed value.
- A variable that waits keeps its selection in the same way. The page does not write the default value in the URL (9.5).
- The panels run with these values. They wait for no option. A panel whose SQL names a failed variable runs with the value that the variable has (case B293).
- The popup of a failed open variable (4.8) takes a typed value. A change applies as for a variable with options.

The cache of the options:

- The runtime keeps the answers of the options requests of the page. The key is the name of the variable, the requested values of its parents and the period.
- The period is empty when `refresh` is not `time_change`. For a relative range, it is the two texts of the range and the zone, and an answer is valid for 60 s. For an absolute range, it is the two times, and an answer is valid for 5 minutes.
- The cache holds 64 answers at most. It drops the least recent answer first. It never keeps a failed answer.
- A refresh, a change of the period and a change of a parent look in the cache first. A request leaves only when no valid answer exists. A "Retry" ignores the cache.
- At load, each variable sends one request. A refresh inside the time of the cache sends none. A quick range that the page showed before sends none (case B294).
- A variable below a `time_change` variable reloads when its key changes: that is a new value of a parent. The same values of the parent, in the same period, give a cached answer.

Cancel of the requests:

- Each variable has at most one request of options in flight. A newer request for the variable cancels the older one (`AbortController`).
- When the person leaves the dashboard (the brand, the browser back, a link of the page), the runtime cancels each request of options in flight. The runtime of the next page does not receive an answer for them (case B295).

Chips of the selected values:

- The control of a `multi` variable (`custom` or `query`) shows its selection as chips. A control of one value keeps its text. The `interval` and `adhoc` variables have their own controls (9.10a).
- Each value is one chip. "All" is one chip with the text "All". The chip has the text of the option, with an ellipsis after 160 px, and a cross.
- The chip has a cross when the selection has two values or more. The chip "All" and the last chip have no cross, because a selection is never empty.
- A click on the text of a chip, or on the empty room of the control, opens the popup. A click on a cross removes the value and applies it at once. The same keys work: the cross is a `button` in the Tab order, with the name `Remove <value>` and the hook `variable-<name>-remove`.
- The key `Enter` or `Space` on the cross removes the value. The focus goes to the next cross, or to the control when no cross is left.
- `Backspace` in the empty field of the combobox of an open variable (4.8) removes the last value of the draft. It does nothing when only one value is left.
- The chips that do not fit in the row are hidden, and the chip "+<n>" shows the count. A click on it opens the popup. At least one chip shows, with its text cut when needed (case B296).
- The control keeps its height (32 px) when it has more chips. The chips use the tokens of the page and have the contrast of the light and the dark theme (9.25).
- The hidden element `select__value` of the control keeps the text of the selection (`a + b + c`). Thus the name of the control for assistive technology does not change.

### 9.10a The control of an `adhoc` variable

An `adhoc` variable (4.10c) has a control in the variables bar. The control shows the filters of the viewer and lets the viewer add and remove filters (measure of Grafana 12.4.2 on 2026-11-22). The code is in `web/js/adhoc.js`.

- The control is the label chip (9.10), then one chip for each filter, then a button with a plus icon. The chip shows the text `<key> <op> <value>`, as Grafana does, for example `host = web-1`. A long text has an ellipsis, and the attribute `title` holds the full text.
- Each chip has a remove button with the name `Remove filter with key <key>`, as in Grafana. A click removes the filter and gives the focus to the plus button.
- The plus button has the name `Add filter to <label>`. A click opens a small form. The form has a select of the keys, a select of the operators `=` and `!=`, a value field and the button "Add". `Enter` in the value field adds the filter, and `Escape` closes the form. The focus then goes back to the plus button.
- The select of the keys holds the permitted keys of the variable: the member `adhoc` of the answer of 6.14. A key that the configured list of the source does not hold is not offered. An absent or wrong member gives no key, so the page offers no filter (fail closed).
- A value of more than 1,024 bytes, or with a control character, shows an error under the field with `role="alert"`. The field gets `aria-invalid="true"`, and the page sends nothing.
- The plus button is disabled when the variable has 16 filters, or when it has no permitted key.
- Each change of the filters runs again each panel whose query has `$__adhocFilters` on the source of the variable. The member `names` of 6.14 holds the variable for each such query. A panel with no macro does not run.
- The page has no list of values and runs no query for the keys or the values. Grafana reads the keys and the values from the source; Hune reads the keys from the document and the configuration only.
- The control has the hooks `variable-<name>-add`, `variable-<name>-filter` (each chip), `variable-<name>-key`, `variable-<name>-op`, `variable-<name>-value` and `variable-<name>-apply` (9.29).
- The page writes each text with `textContent`. A value never becomes HTML.

The address (9.5). The page reads and writes the form of Grafana 12.4.2: one `var-<name>` item for each filter.

- The text of an item is `<key>|<op>|<value>`. In the key and in the value, `|` is written `__gfp__`, `,` is written `__gfc__` and `#` is written `__gfh__`. The page then encodes the item with `encodeURIComponent`. With no filter, the page writes one empty item `var-<name>=`, as Grafana does.
- The page reads an item as Grafana does. It keeps the text before the first `#`, cuts it at each `|`, and keeps the text of each part before a `,`. Then it changes the three escapes back. The first three parts are the key, the operator and the value. A fourth part is not read. An empty item gives no filter.
- The page drops an item with fewer than three parts, a key that is not permitted, or an operator other than `=` and `!=`. It also drops an item with a value that breaks 7.7a, and each item after the 16th. The page shows the notice of 9.10 (hook `data-notice="variable"`) with the text `The page does not use these filters of <label>: "<item>", ...`. It writes the address again without these items.
- The selection of an adhoc variable holds the item of the address of each filter. Thus a reload or a link gives the same filters.

Known differences from Grafana 12.4.2:

- Grafana writes no item for a filter with an empty value. Hune writes `<key>|<op>|` and reads it as the empty value.
- Grafana encodes `!` as `%21`. The page writes `!` as it is, and both forms read the same.
- A key or a value that holds the text `__gfp__`, `__gfc__` or `__gfh__` reads back with `|`, `,` or `#`, in Grafana too. The form of the page refuses such a value, with the error "A value cannot hold __gfp__, __gfc__ or __gfh__.".
- Grafana keeps the operators `<`, `>`, `=~` and `!~` of an address. Hune drops them with the notice.
- The vectors FE-80 hold the read and the write forms of the measure (16.10a).

The request (7.7a). The member of the variable in `variables` is `{"filters": [{"key", "op", "value"}, ...]}`, in the order of the chips.

### 9.11 Panel frame

- The frame is a box with a border of 1 px and a radius of 6 px. Its title bar is 40 px high. The bar has the title (14 px, weight 500), the description icon with its tooltip and the time note of 9.8b. It also has the menu of the panel links (9.18) and the panel menu. An empty title shows no text. A time series with annotations can also have the annotation note of 9.19d, after the time note.
- The title and the description show after the replacement of the variables (9.13). The text of a panel link shows in the same way.
- The icon of the description is 16 px, and it follows the title at 8 px. The title gets an ellipsis when it is too long. The menu button has 16 px of room before it, and this room comes from the title. In a title bar of 184 px, the title next to the icon of the description has 88 px.
- The button of the panel menu is 20 x 24 px. It shows on hover, on keyboard focus inside the panel and while its menu is open. On a touch screen it always shows.
- The body of the panel has a padding of 8 px.
- The panel menu has "View" (`viewPanel`), "View data", and "Inspect" (when `session.query_inspect` is `true`, see the rules below). The item "View" opens one panel (`viewPanel`). The menu has no "Share" item (9.5). In edit mode it also has the entries of 10.2. Each item has an icon of 16 px.
- The menu of a text panel has "View" only, and the entries of 10.2 in edit mode. It has no "View data" and no "Inspect", because the panel has no data (9.19b).
- A panel of the type `state_timeline` or `status_history` has the menu of a panel with data. The menu has "View", "View data" and "Inspect". "View data" shows the frame of the panel. It is the text alternative of the picture (9.26).
- A panel of the type `heatmap` or `histogram` has the menu of a panel with data. The menu has "View", "View data" and "Inspect". "View data" shows the frame of the panel. It is the full text alternative of the picture (9.26).
- A time series that has marks (9.19d) shows a second table in "View data", after the table of the fields. Its title is "Annotations". It has the columns "Time", "End", "Title", "Text" and "Tags", one row for each mark in the order of the time. It is the text alternative of the marks (9.26). The page writes each cell with `textContent`. The file of "Download CSV" holds the first table only.
- The frame of a placed control (9.10) is like a panel frame, with these differences.
  - It has no title bar when the `placement` has no `title`. The body then takes the whole frame, with the padding of 8 px. In edit mode a thin strip at the top is the handle of the drag. With a `title`, the title bar of 40 px shows as for a panel. The title is the text of the title bar, and the label of the variable is the chip of the control.
  - It has no query, no state of the scheduler, no "View", no "View data" and no "Inspect". In view mode it has no menu button. In edit mode its menu has "Edit the variable" and "Put back in the top bar" (10.6).
  - The body holds the control, centred on the vertical axis. A frame of 1 row (30 px) is too low for the control: its body scrolls, and the editor gives no error for it. A frame needs 2 rows (68 px) for the control without a title bar. With a title bar it needs 3 rows (98 px). From 481 to 768 px the grid has two columns, so a frame of 1 row can share its row with a higher frame.
  - The frame has `role="group"`. The attribute `data-type` is `variable` and `data-testid` is `placement-<name>`. This `data-type` marks a placed item of the grid. It is never a type of panel of a document (4.13).
- A panel with `transparent = true` (4.13) has no fill and no edge at rest. Its edge shows on hover and on keyboard focus inside the panel. The frame has the attribute `data-transparent`.
  - The rule is the same in the dashboard view, in the edit grid and in the preview of the panel editor (10.3). No other rule gives a frame a fill or an edge.
  - The fill of the title bar, of the body and of the overlay of a state is transparent, so the page colour shows through them. The states `error` and `loading` and the messages of a limit stay readable. Their text has a contrast ratio of 4.5 or more against the page colour.
  - The title has the text colour of the theme. It has a contrast ratio of 4.5 or more against the page colour. The menu of the panel has its own fill (`--surface-menu`), so it stays readable.

| `data-state` | The panel shows |
|---|---|
| `idle` | The empty frame: the panel is not near the viewport, or the variables load. |
| `queued` | The empty frame: the request waits in the queue. |
| `loading` | The empty frame, or the earlier content on a refresh. Three pulsing dots at the top right of the title bar, 300 ms after the request starts. The empty frame also shows the preview of its panel type. Nothing moves. |
| `ready` | The panel type with the data. |
| `empty` | "No data": zero rows after the transformations. The text is plain: 18 px, weight 400, muted colour, centred, with no icon. A `stat` with zero rows does not use this state: it draws its own "No data" (9.19). |
| `error` | A red badge at the left of the title, with the message as its tooltip. The body shows the code and the message of the error, and a "Retry" button. The frame stays neutral. A panel with several queries shows this state when no query gave a frame (9.8d). |
| `text-limit` | The message of 9.19b: the text breaks a limit of the page. The body shows the message in the muted colour and no text. The frame has no "Retry". |
| `timeline-limit` | The message of 9.19c: the data breaks the cap of the segments, or a status history has too many points for its width. The body shows the message in the muted colour and no picture. The frame has no "Retry", because a new query gives the same data. |
| `bucket-limit` | The message of 9.19e or 9.19f: the data breaks a cap of the values, the cells or the buckets. The body shows the message in the muted colour and no picture. The frame has no "Retry", because a new query gives the same data. |
| `geo-limit` | The message of 9.19i: the data has more rows than `MAX_GEO_POINTS`. The body shows the message in the muted colour and no picture. The frame has no "Retry", because a new query gives the same data. |
| `repeat-empty` | The message of 9.8c: the selection of the repeat variable has no value. The frame sends no query. |
| `repeat-limit` | The message of 9.8c: the count of values is above a limit. The frame sends no query. |

- The frame has its final size from the first frame (9.8). The size does not change when the state changes.
- A copy of a repeat (9.8c) is a frame like another panel. Its `data-testid` is `panel-<key>` (9.29), and it has the attributes `data-repeat-n` (the number n) and `data-repeat-count` (the number of copies). The first copy of a repeat that has one value has the same attributes with the value 1.
- A panel with several queries (9.8d) can have the state `ready` and a red badge. The badge lists each failed query. The frame has the attribute `data-failed` with the refs. A table with several queries has a select under the table (9.22a, rule 9). The select is part of the frame: it takes its height from the body, and it does not change the size of the frame.
- "Inspect" of a panel with several queries shows one block for each query that has an answer, in the order of the queries. Each block has the title "Query <ref>", the facts of the answer and the expanded SQL. Each block has its own button "Copy SQL". The footer has the button "Close" only. A panel with one query shows the dialog of today.
- The test hooks of a block are `inspect-<id>-meta-<ref>`, `inspect-<id>-sql-<ref>`, `inspect-<id>-none-<ref>` and `inspect-copy-<ref>`. The hooks of a panel with one query do not change.
- A panel that waits (`idle`, `queued` or `loading`) is in one of two cases. The attribute `data-wait` of the frame says which case.
- First load (`data-wait="first"`): nothing is on the screen. This is the first wait of the panel, or the wait after an error. The frame shows its empty frame. After 300 ms with no result, the body shows the preview of the panel type and the title bar shows the dots (class `panel--wait`). A result that comes sooner replaces the empty frame at once, with no preview between.
- Reload (`data-wait="reload"`): the panel has content, or the text "No data". They stay as they are. After 300 ms of `loading`, the title bar shows the dots (class `panel--progress`). The page does not dim the content. A faster answer changes nothing before the new data shows.
- A reload that gives the same data and the same range draws nothing again, so the panel does not blink. The data are the columns, the interval and the values of the answers. The members `query_id`, `stats`, `sql` and `etag` are not data. An answer of more than 500,000 cells counts as new data.
- A time series, a state timeline, a status history and a heatmap draw their x axis over the range of the panel. They draw again when the range changes, also when the data are the same.
- A new picture replaces the old one in one step. The page loads the module of the panel type before it removes the old picture. No frame has an empty body between them.
- A reload that fails gives the state `error`, as in the table above. The page does not keep the old data, because old data can look current.
- The dots are the only sign of a reload, and the dots with the preview are the only sign of a first load. The title bar has no spinner, no bar and no other mark. The title and the icon of the description do not move.
- The loader is an element `panel__loader` in the title bar, with `aria-hidden="true"`. It is out of the flow (`position: absolute`), has `pointer-events: none`, and takes no room. Its right edge is 4 px left of the menu button.
- The three dots are circles of 5 px, with 3 px between them. Their colour is the token `--accent-fill`. Each dot pulses for 1 s (the token `--dur-pulse`, the curve `ease-in-out`). Its opacity goes from 0.2 to 1, and its scale from 0.8 to 1. The second dot starts 0.15 s after the first, and the third dot 0.15 s after the second.
- The preview is an element `panel__skeleton` in the body, with `aria-hidden="true"`. It is out of the flow, has `pointer-events: none`, and sits under the overlay. The overlay of an error, of a hint or of a note covers it. A text panel has no preview.
- The preview is a picture that the page draws from the JSON document of the dashboard only. It never reads the text of a query, in any language. It has no heuristic: no typical value, no guessed count and no guessed width.
- Where the document does not give a fact, the preview draws the least form that the document proves. This form is an empty region, or nothing. A drawn shape claims a fact, so the page draws only the facts that the document gives.
- The preview uses neutral tones: the tokens `--skel-fill` (9 % of `--text`), `--skel-edge` (17 %) and `--skel-grid` (10 %). It never uses a colour of the document, because a colour would claim that a series exists.
- A text that the document fixes shows as a bar. The bar has the measured width of the text. The font is the font of the loaded panel: `400 12px` and the font family of the page. The bar is 8 px high and has round ends.
- The code is in the folder `web/js/preview/`. The file `index.js` is the registry. It gives the family of each panel type and calls `build(panel, box, doc, ctx)` of the module of the family. The file `common.js` has the shared pure helpers: the nodes, the rectangles, the bounds and the width of a text.
- The argument `box` is `{ w, h }`: the content box of the body, which is the size of the loaded plot holder. The argument `ctx` is `{ rt, page }`. A module gives `{ elements, bounds, notes }`. The elements are a tree of nodes. A node has a tag, a class, custom properties, attributes and children.
- The page sets the custom properties with the CSSOM and writes no `style` attribute (9.29). The SVG nodes use the SVG namespace.
- The preview builds when its box has a size, and builds again when the size changes. The page observes the box with a `ResizeObserver`. A panel that never waits for a first load builds nothing.
- A module that throws gives the neutral form. A preview never breaks a panel.
- The families and their modules:

  | Family | Panel types | Module |
  |---|---|---|
  | `timeseries` | `timeseries`, `xychart` | `timeseries.js` |
  | `barchart` | `barchart` | `barchart.js` |
  | `piechart` | `piechart` | `piechart.js` |
  | `stat` | `stat` | `stat.js` |
  | `gauge` | `gauge` | `gauge.js` |
  | `bargauge` | `bargauge` | `bargauge.js` |
  | `table` | `table` | `table.js` |
  | `heatmap` | `heatmap` | `heatmap.js` |
  | `histogram` | `histogram` | `histogram.js` |
  | `timeline` | `state_timeline`, `status_history` | `timeline.js` |
  | `logs` | `logs` | `logs.js` |
  | `geomap` | `geomap` | `geomap.js` |

- A panel type that draws no queried data, a text, has no family and no preview.
- Each family module gives the neutral form until the family has its own rules: an empty body with the light and the dots. The neutral form claims no part.
- The pie chart (`piechart.js`) and the map (`geomap.js`) draw their own rules. The pie names the extra part `ring`. The map names the extra part `zoom`.
- The pie preview reads `options.legend`, `options.pie_type`, `fields.defaults.display_name` and `fields.overrides`. No other member changes it.
  - The CSS of the preview copies the rules of the loaded pie, so the browser gives the same boxes. The page sets no size in px from JavaScript. A resize or a late font changes both pictures in the same way.
  - With `legend.show` = `false`, the preview has no legend and no gap. The part `plot` is the whole box, and its class is `exact`.
  - With `legend.placement` = `bottom`, the preview keeps one line under a gap of 4 px. The line is 18.857 px high, and 42 % of the box height is its cap. The loaded legend can have more lines, so the class of `plot` is `most`. The class is `exact` when the box is lower than 44.9 px, because the cap then fixes the legend.
  - With `legend.placement` = `right`, the preview keeps the floor of 72 px under a gap of 12 px. The loaded legend can be wider, up to 52 % of the box width. The class of `plot` is `most`. The class is `exact` when the box is 138.46 px wide or less.
  - The part `legend` has the class `least`. The legend box has no mark: no swatch, no name, no value and no rule.
  - The members `legend.mode` and `legend.values` give the count of value columns. They add empty cells to the hidden row of a right legend. They change no size of the preview.
  - When `reduce.values` is `false`, `fields.defaults.display_name` is a text with no `$` and no `[[`, and no override sets `display_name`, every item has this name. A right legend with no value column then has a fixed width. The preview gives its name cell the measured width of the text. The class of `plot` is `exact`. The name is not drawn.
  - The part `ring` is the circle of the disc. Its diameter is the least side of the plot less 16 px, and not less than 20 px. The plot clips it. The class of `ring` is the class of `plot`.
  - With `options.pie_type` = `donut`, the ring has a hole of 0.55 of the radius. Any other value gives no hole. The ring has one tone: `--skel-fill`.
  - The preview has no slice, no slice edge, no label and no zero-total outline. The data fix the count of items, their names, values and colours, so no shape stands for them.
  - The ring can move when the data come, because the loaded legend can be wider or higher than the preview legend.
- The map preview reads `options.show_zoom` and `fields.overrides`. No other member changes it.
  - The map node takes the whole box. It has no fill, no legend, no gutter, no line, no tile and no marker. The data and the server configuration fix them. The part `plot` has the class `exact`.
  - With `options.show_zoom` = `true`, the map node holds two squares of 19.25 px. They are at 8 px from the top left corner, one under the other, with no gap. The map node clips them. The part `zoom` has the class `exact`.
  - The squares have the tone `--skel-fill` and no glyph. The state of a glyph depends on the kept view, so it is not in the document.
  - With any other value of `options.show_zoom`, the preview has no square.
- A warning line of an invalid `regex` override has its own place in the pie and in the map. The preview has the same element as the loaded line, with the same icon and text, in `visibility: hidden`. It keeps the room, and the browser gives its wrapped height. The part `warning` has the class `exact`. A stored document cannot hold such a pattern, so only a draft shows the line.
- Each element that a test measures has two attributes. The attribute `data-skel-part` names the part: `box`, `plot`, `legend`, `warning`, `axis-y`, `axis-x`, `label`, `head`, `row`, `column`, `tile`, `arc`, `track`, `scale` or `mark`. The attribute `data-skel-class` gives the tolerance class. The root element has `data-skel`, the key of the family.
- The tolerance classes tell how exact a part is, against the loaded panel:

  | Class | Meaning | Test |
  |---|---|---|
  | `exact` | The document fixes the place and the size. | x, y, width and height differ by 0.5 px at most. |
  | `least` | The document gives a lower bound. The preview takes the least room. | The width and the height of the preview are not more than those of the loaded part, plus 0.5 px. |
  | `most` | The document gives an upper bound. | The width and the height of the preview are not less than those of the loaded part, minus 0.5 px. |
  | `data` | The document gives nothing. No shape stands for the part. | No test. |

- The engine of the charts reads integers, so a size in px is a whole number where the engine rounds it. The preview rounds in the same way and never rounds a value to a tenth.
- One band of light crosses the preview from the left to the right in 2 s (the keyframes `panel-skel`). It is an element `::after` of `panel__skeleton`. It moves with `translateX` only, so no shape changes its size, its place or its opacity. Its colour is 72 % of the token `--surface-panel`, with a transparent edge.
- The measurement harness `tests/browser/lib/preview_measure.mjs` measures a dashboard in a real browser (16.10a). It holds the queries, reads the parts of each preview, releases the queries and reads the loaded panel with the extractor of its type. It gives one row for each part and the verdict of the tolerance class. The command `tests/browser/preview_measure.mjs` uses it on a dashboard of a server that is on.
- The page starts no timer for the loader in a panel that is not near the viewport, beyond the timer of the first wait. A print has no loader.
- "View data" opens a dialog with the fields of the panel after the transformations, as a table. The header is the
  display name, and the cells are the display texts. This table is the text alternative of the chart (9.26).
- The dialog has a button "Download CSV" (`data-testid="view-data-csv"`). The page builds the file in the browser.
  No server route takes part. The next rules give the file.
- The file of "Download CSV" has these parts:
  - The first row holds the display names of all the fields of the dialog, also the fields with `hidden = true`.
  - Each other row holds the display texts of one row of the data, in the order of the rows and of the fields.
  - A field is separated from the next one by `,`, and a row from the next one by U+000D U+000A. The last row has
    a row end. The bytes are UTF-8 with no byte order mark.
  - A field that holds `"`, `,`, U+000D or U+000A is in `"` marks, and each `"` in it is written twice.
  - A text cell that starts with `=`, `+`, `-`, `@`, U+0009 or U+000D gets a `'` before its first character. The header
    row always follows this rule. It comes before the quote rule. A spreadsheet
    then reads the cell as text, and never as a formula (OWASP).
  - A cell of a field of kind `number` or `number_text` gets no `'` for a number with at most a short unit. Examples are `-5`, `-1.25 K`, `-5.2%` and `-1.5 MB/s`. A text from a mapping or from `no_value` gets
    the `'` when it starts as above, also in a number field. Examples are `-`, `=SUM(A1)`, `-1+cmd|x` and `-5 =cmd`.
  - The file name has four parts: the title of the panel after the replacement of the variables, `-data-`, the UTC time
    as `YYYYMMDD-HHmmss`, and `.csv`. In the title, each character outside `A-Za-z0-9 _.-` becomes `_`. An empty
    title gives `panel`.
  - The type is `text/csv;charset=utf-8`. The page makes a `blob:` URL and a link with the attribute `download`,
    clicks it, and releases the URL. This needs no change of the content security policy (9.4). A browser test checks
    this (16.10).
  - The button "Download CSV" is in the dialog "View data". The panel menu has the same entry.
- "Inspect" shows after "View data", and only when `session.query_inspect` is `true` (6.10). It opens a dialog with the expanded SQL of the last answer, the query id, the interval and the number of rows. The dialog has the buttons "Copy SQL" and "Close". The page writes the SQL as text, never as markup.
- When the answer has no `sql` (6.20), the dialog says that the server does not give the SQL to this subject. It then has no "Copy SQL" button.
- The test hooks of "Inspect" are `panel-<id>-inspect`, the dialog `inspect-<id>`, `inspect-<id>-sql`, `inspect-<id>-none` and `inspect-copy`.
- An error of a transformation or of the panel type shows as the error of the panel. An example is a time series with
  no time column.
- `409 dashboard_changed` changes the whole page (9.8), not one panel.

### 9.12 Field model and values in the browser

`frame.js` turns a frame (7.16) into an array of fields (`FE-03`). A field has `name`, `type`, `kind`, `values`,
`labels` (`null` until long-to-wide) and `config` (9.14).

| Kind | `values` | Extra member |
|---|---|---|
| `time` | `Float64Array` of epoch milliseconds (seconds × 1000). NULL is `NaN`. | `nulls`: `Uint8Array`, 1 for a NULL |
| `number` | `Float64Array`. `"nan"` is `NaN`, `"inf"` is `Infinity`, `"-inf"` is `-Infinity`, NULL is `NaN`. | `nulls` |
| `number_text` | Array of exact texts, `null` for a NULL. | `num`: `Float64Array` of `parseFloat` of each text, and `nulls` |
| `text` | Array of texts, `null` for a NULL. | none |
| `bool` | Array of booleans, `null` for a NULL. | none |
| `null` | Array of `null`. | none |

The Grafana value of a cell is the value that Grafana would hold. The matchers, the mappings, the transformations and
the data links use it:

| Kind | Grafana value |
|---|---|
| `time` | The epoch milliseconds, a number. |
| `number` | The double. |
| `number_text` | The double (`num`). |
| `text` | The text. |
| `bool` | The boolean. |
| NULL of any kind | `null`. |

The text form of a Grafana value is `String(value)` of JavaScript: `12.5` gives `"12.5"`, and `null` gives `"null"`.
A table shows a `number_text` value from its exact text (9.15). Charts, stats, thresholds and transformations use the
double, so a value above 2^53 loses precision there.

### 9.13 Display names

The display name of a field is the text that a header, a legend and a tooltip show (`FE-04`). The rule restates the
Grafana function `calculateFieldDisplayName` for the one frame of a panel (4.17, item 5). The page first computes the base
display name of each field `f` of the fields `F`, with these steps:

1. A `time` field with no labels has its name as base display name, or `Time` when its name is empty.
2. Else the page makes a list of parts. When the name of `f` is not empty and is not `Value`, it adds the name.
3. When `f` has labels, the page looks at the label keys of all the fields of `F` that have labels. Maybe they all
   have exactly one key, the same key `K`. Then the page adds the value of `K` when this value is not empty. Else it
   adds the text `{k1="v1", k2="v2"}`, with the keys in code unit order, separated by `, `.
4. When the list is empty, the base display name is the name of `f`, or `Value` when the name is empty. Else it is the
   parts joined by one space.
5. Sometimes the base display name equals the name of `f`, and `n` fields of `F` have this name, with `n > 1`. Then
   the `k`-th of them gets `<name> <k>`. `k` counts from 1 in column order.

Step 1a comes after step 1. A field that has a `prefix` (9.22a, rule 5) is not a time field. Its list of parts starts with the prefix, which is the ref of its query. Then step 2 adds the name. Thus the field `a` of the query `A` has the base display name `A a`. A field with no prefix follows the steps 2 to 5 as before.

In step 5, the prefix is part of the name. Two fields of one query with the same name get a counter after the prefix.

| The fields | Base display names |
|---|---|
| `t`, `a` of the query `A` and `t`, `b` of the query `B`, after the join (9.22a) | `t`, `A a`, `B b` |
| The field `v` with `host="h0"` of the query `A`, and the field `v` of the query `B` | `A v h0`, `B v` |
| The field `a` of the only visible query | `a` |

The display name is the property `display_name` of the field (4.18) when its configuration sets it (9.14), else the
base display name. The matchers of 9.14 use the base display name, and they also use the name that a transform set (9.21a, rule 1).
The transforms of 9.21 and 9.21a use the current name. The page computes the configuration after the transforms. The property
`display_name` of the defaults or of an override replaces a name that a transform set.

The property `display_name` is a template, as in Grafana. The page replaces these tokens with text:

- `${__field.name}`: the name of the field.
- `${__field.displayName}`: the base display name.
- `${__field.labels.<key>}`: the value of this label, or the empty text.
- `${__field.labels}`: all labels, as `{k1="v1", k2="v2"}`.
- `${__series.name}`: the empty text.

The form `[[<name>]]` also works. A format suffix such as `:text` is read and has no effect. Another name stays as it is written. When the result is the empty text, the base display name applies. The result is text, never HTML.

| Fields | Display names |
|---|---|
| `t` (time), `cpu` | `t`, `cpu` |
| `a`, `a` | `a 1`, `a 2` |
| After long-to-wide: `weight` with `node="a1"`, `weight` with `node="b2"` | `weight a1`, `weight b2` |
| `weight` with `region="eu"` and `node="a1"` | `weight {node="a1", region="eu"}` |
| `Value` with `host="x"` | `x` |
| After `grouping_to_matrix` | The first field is `<row_field>\<column_field>` (9.21). |
| `score` with `display_name = "Score (pts)"` | `Score (pts)` |
| `score` with `region="eu"` and `display_name = "${__field.labels.region} Score"` | `eu Score` |

The parity harness compares the headers of each real panel with Grafana (15).

Variables in titles and texts (`FE-04`). The page replaces the placeholders of variables before it shows these texts:

- the `title` and the `description` of a panel (4.13);
- the `title` of a section (4.12);
- the `title` of a panel link and of a data link (4.23).

The content of a text panel is not in this list. 9.19b gives its own rule: the content shows the values of a variable, and a title shows its option text.

The stored text does not change. The replacement uses the current selection of each variable
(9.10). The forms are those of 7.6, with this table:

| Form | Text that replaces it |
|---|---|
| `$name`, `${name}`, `${name:text}` | The text of the selection: the option text, or the texts of a list joined by ` + `. "All" gives `All`, also with an `all_value`. A `text` variable gives its value. Grafana 12.4.2 shows the option text in a title in the same way. |
| `${name:glob}` | One value: the value. A list of 2 values or more: `{a,b}`. "All" with an `all_value`: the `all_value`. "All" without `all_value`: the values of all options, as a list. |
| `${name:csv}` | The values joined by `,`. "All" with an `all_value`: the `all_value`. "All" without `all_value`: the values of all options. |
| `${name:pipe}` | The values joined by `\|`. "All" follows the same rule as `${name:csv}`. |

- A placeholder with another format stays as it is written. A name that is not a variable of the document stays as it is
  written. A name that starts with `__` stays as it is written. The longest run of letters, digits and `_` is the name
  (7.6).
- The form `[[name]]` stays as it is written, as in 7.6.
- The page replaces the text again when the selection of a variable changes. A text with no placeholder does not need
  a replacement.
- The result is text. The page writes it with `textContent` and never as HTML (9.1). The value of a variable cannot
  change the structure of the page.
- The save gives a warning for a placeholder in these texts whose name is not a variable of the document (7.18,
  warning 6).
- A placeholder of an `interval` variable shows the text of the option, and `Auto` for the option "Auto". A placeholder of a `constant` shows its value. Grafana shows the computed interval for "Auto" (19.20k).

| Text | Selection | Result |
|---|---|---|
| `Latency of $service in ${env}` | `service` has the value `api` and the text `API`, `env` has the value `prod` and the text `Production` | `Latency of API in Production` |
| `Hosts ${host}` | `host` has the options `a` (text `Alpha`) and `b` (text `Beta`) | `Hosts Alpha + Beta` |
| `Hosts ${host:glob}` | `host` has the values `a` and `b` | `Hosts {a,b}` |
| `${env:csv} <b>` | `env` has the values `prod` and `dev` | `prod,dev <b>` (the `<b>` is text) |

### 9.14 Field configuration and matchers

The field properties are a closed list (4.17 and 4.18 give the members):

| Property | Panel types | Meaning |
|---|---|---|
| `unit` | all | The unit of 9.15. |
| `decimals` | all | 0 to 20 decimals. Absent: the automatic rule of 9.15. |
| `min`, `max` | all | The bounds of the scale: axis, bar gauge, percentage thresholds. Absent: the bounds of the data. |
| `color` | all | The colour object `{mode, fixed}` of 9.16. |
| `thresholds` | all | The thresholds of 9.16. |
| `mappings` | all | The value mappings of 9.17. |
| `links` | all | The data links of 9.18. |
| `display_name` | all | The display name of the field (9.13). |
| `no_value` | all | Only in `fields.defaults`. The text in place of a value that is `null` or empty (below). |
| `width` | `table` | The width of the column in CSS pixels, 10 to 4,000. Absent: automatic (9.20). |
| `align` | `table` | `auto`, `left`, `center` or `right` (9.20). |
| `cell` | `table` | The object `{type, mode}`: `type` is `auto`, `color_text`, `color_background`, `gauge` or `json`. `mode` is `gradient` or `basic` with `color_background`, `basic`, `gradient` or `lcd` with `gauge`, else `null` (9.20). |
| `filterable` | `table` | `true` gives the column filter of 9.20. |
| `hidden` | `table` | `true`: the table does not show the column (9.20). |
| `footer` | `table` | The object `{reducers}`: the calculations that the footer of the table shows for the column (9.20). |
| `inspect` | `table` | `true` gives the button "Inspect value" in each cell of the column (9.20). |
| `hide_from` | all | The object `{legend, tooltip, viz}`: where the series does not show (9.19). It has an effect in `timeseries`, `barchart`, `xychart`, `state_timeline`, `status_history` and `histogram` only. A `hide_from.viz` of a field of a `heatmap` leaves the field out of the rows (9.19e). |
| `axis_placement` | `timeseries` | `auto`, `left`, `right` or `hidden`: the side of the y axis of the series (9.19). |
| `axis_label` | `timeseries`, `barchart` | The title of the y axis, or of the value axis (9.19). |
| `soft_min`, `soft_max` | `timeseries`, `barchart` | The soft ends of the scale (9.19). |
| `draw`, `line_width`, `fill_opacity`, `show_points`, `point_size` | `timeseries` | The style of one series. Only in an override (9.19). |
| `line_dash` | `timeseries` | `solid`, `dash` or `dot` (9.19). |
| `transform` | `timeseries` | `none` or `negative_y` (9.19). |

`no_value` applies to a table cell, a stat value, a bar gauge value and a pie slice label. Each of them shows the text
of `no_value` in place of a value that is `null` or the empty text. A stat whose reduction gives `null` also shows it,
also for zero rows. Without `no_value`, a `null` cell is empty and a `null` stat shows "No data". The text has no unit.

A stat shows it as large as a value, at weight 500. The colour is the base step, the first step of the thresholds. Grafana does the same.
A stat and a bar gauge with zero rows draw their own "No data".

The page computes the configuration of each field after the transformations (`FE-05`):

1. It starts from the panel member `fields.defaults`.
2. It takes each override in document order. When its matcher matches the field, each property of its `set` replaces
   the current value. Thus a later override wins, property by property.
3. A property that no step gives takes the default value below.

A series gets its style in this order. The value of the last override that sets the member comes first. Then comes the option of the panel (`draw`, `line_width`, `fill_opacity`, `show_points`, `point_size`). Last comes the default value of the table above.

| Matcher (`match.by`) | Grafana id | A field matches when |
|---|---|---|
| `name` | `byName` | `match.value` equals the name of the field, or equals its base display name (9.13), or equals the name that `organize` or `rename_by_regex` set in `config.display_name`. The comparison is exact. |
| `regex` | `byRegexp` | The pattern `match.value`, in the Grafana form of 4.24, finds a match in the base display name. |
| `names` | `byNames` | With `match.mode` = `include`: the name of the field, or its base display name (9.13), equals one text of `match.names`. With `exclude`: it equals none of them. The comparison is exact. |
| `query` | `byFrameRefID` | The field comes from the query whose ref equals `match.value` (9.22a, rule 5). The joined time field comes from no query, so it never matches. A matcher with a ref that no query has matches no field. |

A matcher `name` also matches the name that `organize` or `rename_by_regex` set in `config.display_name`. Grafana does the same (measured). An override for `X` and an override for `x` both find the field that `organize` renamed from `x` to `X`.

The Grafana form is the form of the Grafana function `stringToJsRegex`:

| Pattern | JavaScript | Example |
|---|---|---|
| `/body/flags` | `new RegExp("body", "flags")`, with no anchor added | `/score/i` matches `Score daily`. `/^Total$/` matches only `Total`. |
| another text `t` | `new RegExp("^" + t + "$")`: an exact match | `score` matches only `score`. |

A `regex` pattern that JavaScript cannot compile matches no field. The panel then shows the warning "The override
pattern <pattern> is not a valid regular expression.".

The length of the text (T87). The page runs a pattern of the document only on a text of 1,024 code units or fewer. This number is the value of `MAX_VALUE_BYTES` (4.35). A longer text never matches, and the page does not give it to the engine of regular expressions.

The reason: a name or a cell can hold data of any length. A name of a long-to-wide holds a label value (9.22), and the name of a MongoDB column has no limit. The time of a common pattern such as `(.*)-(.*)` grows with the square of the length of the text. A text of 1,024 bytes or fewer, the limit of a name in the document, has 1,024 code units or fewer. Thus the rule never changes the result for such a text. The effect in each place:

| Place | Effect of a text of more than 1,024 code units |
|---|---|
| The matcher `regex` of an override (above) | The override does not apply to the field. |
| The member `fields` of the option `reduce` (9.19) | The option does not keep the field. |
| A mapping of the kind `regex` (9.17) | The mapping does not match. The next mapping can match. |
| A matcher `regex` of `filter_by_value` (9.21) | The filter does not pass. With one filter, `include` removes the row and `exclude` keeps it. |
| The inclusion pattern of `filter_fields_by_name` (9.21a, rule 10) | The pattern does not keep the field. |
| The exclusion pattern of `filter_fields_by_name` | The pattern does not remove the field. |
| `rename_by_regex` (9.21a, rule 11) | The field keeps its current name. |
| The `regex` of a `query` variable (9.10) | The option text has no match, so the option goes. |

The rule bounds the time of a pattern whose time grows with a power of the length. It does not bound a pattern with nested repetitions, such as `(a+)+$`. The time of such a pattern can grow exponentially on a short text. Only an author can write it (12.18).

| Property | Default value |
|---|---|
| `unit` | `none` |
| `decimals` | The automatic rule of 9.15. |
| `min`, `max` | The lowest and the highest finite value of the field. |
| `thresholds` | Mode `absolute`, steps `[{value: null, color: "green"}, {value: 80, color: "red"}]` (the Grafana default). |
| `color` | Mode `palette` (the series palette of ClickHouse Dash, 9.16) for `timeseries`, `barchart`, `piechart`, `xychart` and `histogram`. Mode `thresholds` for `stat`, `bargauge`, `gauge`, `table` and `status_history`, and for `heatmap`, where it has no effect (9.19e). Mode `continuous_gr_yl_rd` for `state_timeline` (9.19c). |
| `mappings`, `links`, `display_name`, `no_value` | None. |
| `align` | `auto`. |
| `cell` | `{type: "auto", mode: null}`. |
| `filterable`, `hidden`, `inspect` | `false`. |
| `footer` | None: the table has no footer for the column. |
| `hide_from` | `{legend: false, tooltip: false, viz: false}`. |
| `axis_placement` | `auto`. |
| `axis_label`, `soft_min`, `soft_max` | None. |
| `line_dash` | `solid`. |
| `transform` | `none`. |
| `draw`, `line_width`, `fill_opacity`, `show_points`, `point_size` | The option of the panel (4.28). |

Grafana writes the matcher `names` with `mode` = `exclude` when a viewer clicks a series in the legend and then saves
the dashboard. The override lists the names that stay visible, and `hide_from` hides all the others. The
viewer of Hune can also hide a series with a click on its legend entry (9.19). This state belongs to the page. It
never changes the document, and it never changes `hide_from`.

### 9.15 Units and decimals

The page formats a number with a unit and decimals into three parts: a prefix, a text and a suffix (`FE-06`). The
rules restate the `valueFormats` of Grafana.

`to_fixed(v, d)`:

1. `null` gives the empty text. `NaN` gives `NaN`. `Infinity` gives `∞` and `-Infinity` gives `-∞`.
2. When `d` is absent, `d` is `auto_decimals(v)`.
3. `v = 0` gives `(0).toFixed(d)`.
4. Else the text is `String(Math.round(v × 10^d) / 10^d)`. When this text holds `e`, it stays as it is. Else the page
   adds zeros so that the text has exactly `d` decimals.

`auto_decimals(v)`:

1. `dec = -floor(log10(|v|)) + 1`. The page computes `log10(|v|)` as `Math.log(|v|) / Math.LN10`, as Grafana does. Thus `99.99999999999999` gives `dec = 0` and the text `100.0`. `Math.log10` would give `dec = -1`.
2. `norm = |v| / 10^(-dec)`. When `norm > 2.25`, `dec = dec + 1`.
3. When `v` is a whole number, `dec = 0`.
4. The result is `max(0, dec)`.

Examples: `1234.5678` gives `1235`, `45.5` gives `45.5`, `0.012345` gives `0.0123`, and `3` gives `3`.

Exact path. The units with no scale are `none`, `percent`, `suffix:` and `prefix:`. For a `number_text` value and
such a unit, the page computes `d` from the double. Then it rounds the exact decimal text to `d` decimals, with a half rounded towards
positive infinity as `Math.round` does. It adds zeros to `d` decimals. A unit with a scale uses the double.

| Unit | Rule | Example |
|---|---|---|
| `none` | `to_fixed(v, d)`. | `1234.5678` gives `1235`. |
| `short` | Scale 1000 with the suffixes ``, ` K`, ` Mil`, ` Bil`, ` Tri`, ` Quadr`, ` Quint`, ` Sext`, ` Sept`. | `1534000` gives `1.53 Mil`. |
| `percent` | `to_fixed(v, d)` and the suffix `%`. | `45.5` gives `45.5%`. |
| `percentunit` | `to_fixed(100 × v, d)` and the suffix `%`. | `0.455` gives `45.5%`. |
| `ms` | Below 1,000: ` ms`. Below 60,000: `v / 1000`, ` s`. Below 3,600,000: `v / 60000`, ` min`. Below 86,400,000: `v / 3600000`, ` hour`. Below 31,536,000,000: `v / 86400000`, ` day`. Else `v / 31536000000`, ` year`. The bounds use `|v|`. A time word takes the plural form (below). | `1500` gives `1.50 s`. `160000` gives `2.67 mins`. |
| `s` | `0` gives `0 s`. Below 0.000001: `v × 10^9`, ` ns`. Below 0.001: `v × 10^6`, ` µs`. Below 1: `v × 1000`, ` ms`. Below 60: ` s`. Below 3,600: `v / 60`, ` min`. Below 86,400: `v / 3600`, ` hour`. Below 604,800: `v / 86400`, ` day`. Below 31,536,000: `v / 604800`, ` week`. Else `v / 31556900`, ` year`. The bounds use `|v|`. A time word takes the plural form (below). | `90` gives `1.50 mins`. |
| `bytes` | Scale 1024 with ` B`, ` KiB`, ` MiB`, ` GiB`, ` TiB`, ` PiB`, ` EiB`, ` ZiB`, ` YiB`. | `1536` gives `1.50 KiB`. |
| `decbytes` | The SI scale (below) with the unit `B`. | `1500` gives `1.50 kB`. `0.5` gives `500 mB`. |
| `suffix:<text>` | `to_fixed(v, d)`, then `<text>` exactly, with no added space (4.20). | `12.5` with `suffix: pts` gives `12.5 pts`. `12.5` with `suffix:pts` gives `12.5pts`. |
| `prefix:<text>` | `<text>` exactly, then `to_fixed(v, d)`, with no added space (4.20). | `12.5` with `prefix:$` gives `$12.5`. |
| `ns` | Below 1,000: ` ns`. Below 1e6: `v / 1000`, ` µs`. Below 1e9: `v / 1e6`, ` ms`. Below 6e10: `v / 1e9`, ` s`. Below 3.6e12: `v / 6e10`, ` min`. Below 8.64e13: `v / 3.6e12`, ` hour`. Else `v / 8.64e13`, ` day`. The bounds use `|v|`. A time word takes the plural form (below). | `1500` gives `1.50 µs`. |
| `µs` | Below 1,000: ` µs`. Below 1e6: `v / 1000`, ` ms`. Else `v / 1e6`, ` s`. | `2500` gives `2.50 ms`. |
| `m` | Below 60: ` min`, with no plural form. Below 1,440: `v / 60`, ` hour`. Below 10,080: `v / 1440`, ` day`. Below 604,800: `v / 10080`, ` week`. Else `v / 525948`, ` year`. | `90` gives `1.50 hours`. |
| `h` | Below 24: ` hour`, with no plural form. Below 168: `v / 24`, ` day`. Below 8,760: `v / 168`, ` week`. Else `v / 8760`, ` year`. | `36` gives `1.50 days`. |
| `d` | Below 7: ` day`. Below 365: `v / 7`, ` week`. Else `v / 365`, ` year`. A time word takes the plural form. | `10` gives `1.43 weeks`. |
| `dtdurationms` | The duration in words (below), for `v` in milliseconds. | `3723000` gives `1 hour`. With `decimals` 2 it gives `1 hour, 2 minutes, 3 seconds`. |
| `dtdurations` | The duration in words (below), for `v` in seconds. | `3723` with `decimals` 1 gives `1 hour, 2 minutes`. |
| `bits` | Scale 1024 with ` b`, ` Kib`, ` Mib`, ` Gib`, ` Tib`, ` Pib`, ` Eib`, ` Zib`, ` Yib`. | `1536` gives `1.50 Kib`. |
| `bps` | The SI scale (below) with the unit `b/s`. | `1500` gives `1.50 kb/s`. |
| `Bps` | The SI scale (below) with the unit `B/s`. | `1500` gives `1.50 kB/s`. |
| `binBps` | Scale 1024 with ` B/s`, ` KiB/s`, ` MiB/s`, ` GiB/s`, ` TiB/s`, ` PiB/s`, ` EiB/s`, ` ZiB/s`, ` YiB/s`. | `1536` gives `1.50 KiB/s`. |
| `reqps`, `ops`, `rps`, `iops`, `rowsps`, `rowspm`, `recps`, `cps`, `wps`, `opm`, `reqpm`, `rpm` | Scale 1000 with the suffixes ``, `K`, `M`, `B`, `T`. Then the text ` req/s`, ` ops/s`, ` rd/s`, ` io/s`, ` rows/s`, ` rows/m`, ` rec/s`, ` c/s`, ` wr/s`, ` ops/m`, ` req/m` or ` rd/m`, in the order of the ids. The index is never below 0. | `1500` with `reqps` gives `1.50K req/s`. `12` gives `12 req/s`. `0.5` with `rowsps` gives `0.500 rows/s`. |
| `kbytes`, `mbytes`, `gbytes`, `tbytes` | The scale 1024 of `bytes`, with the start step ` KiB`, ` MiB`, ` GiB` or ` TiB` (below). | `0.5` with `kbytes` gives `512 B`. `1234.5678` with `kbytes` gives `1.21 MiB`. |
| `deckbytes`, `decmbytes`, `decgbytes`, `dectbytes` | The SI scale with the unit `B` and the start step `k`, `M`, `G` or `T`. | `0.5` with `deckbytes` gives `500 B`. `1234.5678` gives `1.23 MB`. |
| `decbits` | The SI scale with the unit `b`. | `0.5` gives `500 mb`. |
| `Kbits`, `Mbits`, `Gbits` | The SI scale with the unit `b/s` and the start step `k`, `M` or `G`. | `1` with `Mbits` gives `1 Mb/s`. |
| `KBs`, `MBs`, `GBs` | The SI scale with the unit `B/s` and the start step `k`, `M` or `G`. | `1234.5678` with `KBs` gives `1.23 MB/s`. |
| `hertz` | The SI scale with the unit `Hz`. | `1234.5678` gives `1.23 kHz`. |
| `sishort` | The SI scale with no unit. The step 0 has one space as its name. | `1234.5678` gives `1.23 k`. `1` gives `1` and one space. |
| `dthms` | The clock of `v` seconds (below). | `1234.5678` gives `00:20:34`. `-42` gives `00:00:42 ago`. |
| `dtdhms` | The clock of `v` seconds, with `<n> d ` before it when the whole days `n` are 1 or more. | `98765432` gives `1143 d 02:50:32`. |
| `string` | `String(v)` of JavaScript. There is no rounding and no scale, and the decimals do not apply. | `1234.5678` gives `1234.5678`. `Infinity` gives `Infinity`. |
| `locale` | The text of `v.toLocaleString(language, {maximumFractionDigits: d})`. `language` is the language of the browser. Without `d`, the default of the browser applies. | `1234.5` in `en-US` gives `1,234.5`. |
| `dateTimeAsIso` | The epoch time `v` in milliseconds, as `YYYY-MM-DD HH:mm:ss` in the zone of the dashboard (`formatTime` of `zone.js`). | `1794211200000` in UTC gives `2026-11-09 08:00:00`. |
| `dateTimeFromNow` | The time from now, by the rules of the Moment function `fromNow` (below). | `v` 2 hours before now gives `2 hours ago`. |
| `bool`, `bool_yes_no`, `bool_on_off` | `False`, `No` or `Off` for 0. `True`, `Yes` or `On` for another number, `Infinity` included. `NaN` gives `NaN`. The decimals do not apply. A `bool` value: see the end of 9.15. | `0.5` with `bool` gives `True`. |
| `currencyUSD`, `currencyEUR` | Scale 1000 with the suffixes ``, `K`, `M`, `B`, `T`, applied to `|v|`. The text is the symbol (`$` or `€`), then `to_fixed(|v| / 1000^index, d)`, then the suffix. A negative `v` has `-` before the symbol. | `1500` gives `$1.50K`. `-1500` gives `-$1.50K`. |

- A scale: the index is `floor(log10(|v|) / log10(base))` for the base of the unit, 0 for `v = 0`. This is the
  `scaledUnits` function of Grafana 12.4.2. The text is `to_fixed(v / base^index, d)`.
- Each scale has a start step `s`: the position of the name of the value 1 in its names. The page keeps the
  index between `-s` and `last - s`, with `last` the position of the last name. The name of the value is the name at
  `index + s`. A scale with no start step has `s = 0`, so its index is never below 0.
- The start step of `kbytes`, `mbytes`, `gbytes` and `tbytes` is 1, 2, 3 and 4. Thus `0.5` with `kbytes` gives the
  index -1: `512 B`.
- The SI scale has these 14 names, each with one space before it: `f`, `p`, `n`, `µ`, `m`, none, `k`, `M`, `G`, `T`,
  `P`, `E`, `Z`, `Y`. Then the unit follows. With no start step in the table, `s` is 5, the name with no prefix. A start
  step `k`, `M`, `G` or `T` gives `s` 6, 7, 8 or 9. So `0.5` with `bps` gives `500 mb/s`, and `0.5` with `KBs` gives
  `500 B/s`.
- A scale gives the `to_fixed` text of 9.15 and no suffix for a value that is `NaN` or infinite. The clocks of
  `dthms` and `dtdhms` do the same.
- The clock of `dthms` and `dtdhms` follows Grafana. Let `a = |v|`. For `dtdhms`, `n = floor(a / 86400)`, and the
  clock uses `a - n × 24 × 3600`. The parts are `floor(a / 3600)` hours, `floor((a mod 3600) / 60)` minutes and
  `floor((a mod 3600) mod 60)` seconds.
- A part of 9 or less has a `0` before it, and the parts are joined by `:`. The hours have no cap: `98765432` with
  `dthms` gives `27434:50:32`. A negative `v` gives the clock of `|v|` and the suffix ` ago`. The decimals do not
  apply.
- A value that is `null` gives the empty text for each unit of this table.
- For `ms` and `s`, the text of each row is `to_fixed` of the scaled value with `d`.
- A time word is ` min`, ` hour`, ` day`, ` week` or ` year`. It takes the plural form (` mins`, ` hours`, ` days`, ` weeks`, ` years`) when the scaled value `size` has `|size| > 1`. The test uses the value before it is rounded.
- Thus `1` gives `1 min`, `1.00004` gives `1.00 mins`, `0.999` gives `0.999 year` and `-1` gives `-1 min`. For `ms`, `5000000` gives `1.39 hours`. A symbol (` ns`, ` µs`, ` ms`, ` s`) has no plural.
- The durations in words (`dtdurationms`, `dtdurations`) follow Grafana. The page changes `v` to milliseconds. A value of 0 gives `0 milliseconds` or `0 seconds`. A negative value gives the text of `|v|` and ` ago` at the end.
- For another value, the page goes through these parts in order: year (365 days), month (30 days), week, day, hour, minute, second and millisecond. It starts at the first part that is 1 or more. It writes `<n> <part>`, with `n` the whole number of this part. The word has an `s` at the end when `n` is not 1. The remainder goes to the next part.
- The text has `d + 1` parts, with `d` the decimals of the field (0 when absent). The parts are joined by `, `. Each part after the first is written, also when `n` is 0.
- The time from now (`dateTimeFromNow`) uses the clock of the browser and the rules of `moment().fromNow()`. The text is `in X` for a future time and `X ago` for a past time. `X` depends on the rounded difference to now:
  - up to 44 s: `a few seconds`; up to 89 s: `a minute`; below 45 min: `N minutes`;
  - up to 89 min: `an hour`; below 22 h: `N hours`; below 36 h: `a day`;
  - below 26 days: `N days`; up to 45 days: `a month`; below 11 months: `N months`;
  - below 18 months: `a year`; else `N years`.
  `N` is the rounded count of the unit.
- The `locale`, `dateTimeAsIso` and `dateTimeFromNow` units use the value as it is: a number or the epoch milliseconds of a `time` field. A `text` field shows its text.
- The page adds no group separator for thousands, as Grafana.
- A `time` field with the unit `none` (or no unit) shows `YYYY-MM-DD HH:mm:ss` in the zone of the dashboard. It adds `.SSS` when the mean step of the field is below 20 000 ms. The mean step is `|last − first| / (rows − 1)` over the first and last values that are not `null`. A field with one row shows no milliseconds.
- A time category of a bar chart and the times of section 10 show `YYYY-MM-DD HH:mm:ss`.
- A `text` field shows its text. A `bool` field shows `true` or `false`.
- A `bool` value with the unit `bool_yes_no` shows `Yes` or `No`. With another unit, also `bool` and `bool_on_off`, it
  shows `true` or `false`, as the display of Grafana 12.4.2 does (19.20n).
- The vectors of the 30 units are the texts of Grafana 12.4.2, measured on 2026-11-22
  (`tests/frontend/vectors/FE-06-units-h.json`). The differences are in 19.20n.
- Another unit is not valid in a document (4.20). The importer reports it (15).

### 9.16 Colours and thresholds

The rules of this paragraph have the vector id `FE-07`.

A colour value (4.19) is one of the 32 tokens: a token of the table below, `text` or `transparent`. It can also be
`#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb(R, G, B)` or `rgba(R, G, B, A)`. The table below gives the hex of each token in the dark theme. The light theme draws a token with another shade (below). `text` is the text colour of the theme. `css/dashboard.css` defines each token as `--viz-<token>`.

| Hue | `super-light-<hue>` | `light-<hue>` | `<hue>` | `semi-dark-<hue>` | `dark-<hue>` |
|---|---|---|---|---|---|
| `red` | `#FFA6B0` | `#FF7383` | `#F2495C` | `#E02F44` | `#C4162A` |
| `orange` | `#FFCB7D` | `#FFB357` | `#FF9830` | `#FF780A` | `#FA6400` |
| `yellow` | `#FFF899` | `#FFEE52` | `#FADE2A` | `#F2CC0C` | `#E0B400` |
| `green` | `#C8F2C2` | `#96D98D` | `#73BF69` | `#56A64B` | `#37872D` |
| `blue` | `#C0D8FF` | `#8AB8FF` | `#5794F2` | `#3274D9` | `#1F60C4` |
| `purple` | `#DEB6F2` | `#CA95E5` | `#B877D9` | `#A352CC` | `#8F3BB8` |

The light theme draws a named token in one of two ways. In both, the document keeps the token, and a hex or an `rgb` colour of the document never changes.

- A mark on the surface of the panel uses the table "Light marks". These marks are the lines, points, swatches and bars of the time series, bar chart and xy chart. The dark theme uses the table above.
- A fill that carries its own text uses the table "Light fills". These fills are a stat tile and its value, a bar gauge and a pie slice. They also are a table cell with a background or a coloured text.
- In the table "Light fills", each token takes the hex of the next darker shade of its hue. The step `dark-<hue>` has a shade of its own.

Light fills (measured on Grafana 12.4.2):

| Hue | `super-light-<hue>` | `light-<hue>` | `<hue>` | `semi-dark-<hue>` | `dark-<hue>` |
|---|---|---|---|---|---|
| `red` | `#FF7383` | `#F2495C` | `#E02F44` | `#C4162A` | `#AD0317` |
| `orange` | `#FFB357` | `#FF9830` | `#FF780A` | `#FA6400` | `#E55400` |
| `yellow` | `#FFEE52` | `#FADE2A` | `#F2CC0C` | `#E0B400` | `#CC9D00` |
| `green` | `#96D98D` | `#73BF69` | `#56A64B` | `#37872D` | `#19730E` |
| `blue` | `#8AB8FF` | `#5794F2` | `#3274D9` | `#1F60C4` | `#1250B0` |
| `purple` | `#CA95E5` | `#B877D9` | `#A352CC` | `#8F3BB8` | `#7C2EA3` |

A mark uses the light shade of Grafana when it has a contrast of 3:1 or more. The contrast is against the panel (`#ffffff`) and the page (`#f6f7f9`). Else the hue and the saturation stay, and the HSL lightness goes down until the contrast is 3:1. Each hue keeps its five steps in order. The yellows, most oranges, the light greens and the light blues are darker than the light shade of Grafana.

Light marks:

| Hue | `super-light-<hue>` | `light-<hue>` | `<hue>` | `semi-dark-<hue>` | `dark-<hue>` |
|---|---|---|---|---|---|
| `red` | `#FF4D61` | `#F2495C` | `#E02F44` | `#C4162A` | `#AD0317` |
| `orange` | `#D67500` | `#D86D00` | `#DB6200` | `#DE5900` | `#DD5100` |
| `yellow` | `#9E8F00` | `#9E8904` | `#9D8408` | `#9E7F00` | `#9E7A00` |
| `green` | `#43A136` | `#4C9A42` | `#4E9644` | `#37872D` | `#19730E` |
| `blue` | `#438DFF` | `#4689F1` | `#3274D9` | `#1F60C4` | `#1250B0` |
| `purple` | `#B972DC` | `#B46FD7` | `#A352CC` | `#8F3BB8` | `#7C2EA3` |

The gradient of a table cell turns the hue by 5 degrees. In the light theme it raises the lightness by 7 points. In the dark theme it lowers the lightness by 10 points (9.20).

The series palette is the palette of ClickHouse Dash. It is the one series palette. The page has no other series palette, and the editor offers no choice between series palettes.

The palette has 18 colours, one for each token `--qchart-1` to `--qchart-18` of the copied `00-tokens.css` (9.25). Each theme has its own hex for each slot. The page keeps one copy of the 36 hex values in `colors.js`. A test makes sure that the copy and the tokens are equal. The token `--qchart-other` is the colour of an index that is not a slot.

| Slot `i` | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| Token | `--qchart-1` | `--qchart-2` | `--qchart-3` | `--qchart-4` | `--qchart-5` | `--qchart-6` | `--qchart-7` | `--qchart-8` | `--qchart-9` |
| Dark | `#4296fb` | `#e86a34` | `#29ae81` | `#d28f09` | `#de669a` | `#4ba435` | `#9e8cf4` | `#49c1ea` | `#febad9` |
| Light | `#1a73d5` | `#c14802` | `#088963` | `#9c6900` | `#b84379` | `#227702` | `#644fb1` | `#046480` | `#700048` |

| Slot `i` | 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 17 | other |
|---|---|---|---|---|---|---|---|---|---|---|
| Token | `--qchart-10` | `--qchart-11` | `--qchart-12` | `--qchart-13` | `--qchart-14` | `--qchart-15` | `--qchart-16` | `--qchart-17` | `--qchart-18` | `--qchart-other` |
| Dark | `#ddd674` | `#7572ae` | `#a86751` | `#9fb83c` | `#0695b5` | `#cf95c1` | `#bdbcfd` | `#8e8945` | `#85e2ed` | `#898781` |
| Light | `#433f01` | `#39346a` | `#7c3f2a` | `#768c02` | `#078ead` | `#7c4972` | `#7b78b4` | `#656019` | `#03464c` | `#6f6d68` |

Slot `i` of a series is `i mod 18`, from 0. A negative index gives the colour `other`. A series after the 18th takes the colour of the slot `i mod 18`, as ClickHouse Dash does. Each colour has a contrast of 3:1 or more with the panel and with the page in both themes (`FE-07`). The lowest ratios are 4.08 in the dark theme (slot 11, `#a86751`) and 3.80 in the light theme (slot 12, `#768c02`). The panel is `#13161a` in the dark theme and `#ffffff` in the light theme (9.25).

The classic palette of Grafana 12.4.2 has 64 colours. No colour mode draws it now. The list stays here as the reference of the difference in 19.20a. The order is the one in which Grafana draws the series. First come the main shades of the six hues, then the six dark shades. Then come the six super-light shades and 46 hex colours.

The names in the list are tokens. The 46 hex colours are the same in both themes.

```text
green, semi-dark-yellow, blue, orange, red, purple,
dark-green, dark-yellow, dark-blue, dark-orange, dark-red, dark-purple,
super-light-green, super-light-yellow, super-light-blue, super-light-orange, super-light-red, super-light-purple,
#447EBC, #C15C17, #890F02, #0A437C, #6D1F62, #584477, #B7DBAB, #F4D598, #70DBED, #F9BA8F,
#F29191, #82B5D8, #E5A8E2, #AEA2E0, #629E51, #E5AC0E, #64B0C8, #E0752D, #BF1B00, #0A50A1,
#962D82, #614D93, #9AC48A, #F2C96D, #65C5DB, #F9934E, #EA6460, #5195CE, #D683CE, #806EB7,
#3F6833, #967302, #2F575E, #99440A, #58140C, #052B51, #511749, #3F2B5B, #E0F9D7, #FCEACA,
#CFFAFF, #F9E2D2, #FCE2DE, #BADFF4, #F9D9F9, #DEDAF7
```

What the series palette changes. Only a colour that the page chooses itself changes. This is the mode `palette`. It also is the default of a `timeseries`, a `barchart`, a `piechart` and an `xychart` (9.14).

Each colour that the document states keeps its value. These are a named colour, a hex colour, a threshold step, a value mapping, an override with a fixed colour and the continuous modes. A stat, a gauge, a bar gauge and a table keep the mode `thresholds` as their default. Thus they do not use the series palette, unless the document sets `palette`.

The importer maps each palette mode of Grafana to `palette` (docs/07). The Grafana mode `palette-classic` is not listed. The modes `palette-classic-by-name` and `palette-saturated` are `changed` items, because they lose their rule. The mode `palette` draws the series palette. This visual difference from Grafana is wanted (19.20a).

Colour modes (the property `color`, an object `{mode, fixed}`):

| `color.mode` | Colour of a value |
|---|---|
| `fixed` | `color.fixed`. |
| `thresholds` | The colour of the active step (below). |
| `palette` | Slot `i mod 18` of the series palette (above), in the theme of the page. `i` is the index of the field among the fields of the kinds `number` and `number_text`, in field order, from 0. |
| the 10 `continuous_` modes (4.19) | The colour of a gradient, by the place of the value (below). |

The page uses a continuous mode where it uses `thresholds` (9.14). In a `timeseries`, a `barchart`, a `piechart` and an `xychart`, a continuous mode keeps the rule that the page has for `thresholds`. A mode that a type does not use gives the default of the type (9.14).

Removed mode. The value `palette_by_name` of format 3 does not exist any more: the series palette has no assignment by name. A series takes the slot of its place, as in ClickHouse Dash. Thus the colour of a series changes when the series order changes. The editor has one entry for the series palette, "Series palette" (10.3).

Continuous modes (`FE-23`, paper V5):

- `f` is `(v - min) / (max - min)`, limited to 0 to 1. `min` and `max` come from the field configuration, else from the data of all the fields of the kind `number` of the panel together (9.14). When `max` equals `min`, `f` is 0.
- The stops of the mode are evenly spaced from 0 to 1. The table below gives them. The colour is a uniform cubic B-spline of the stops in sRGB, for each channel (the function `interpolateRgbBasis` of d3). It is not a linear interpolation. The formula is below.
- A stop is a token of this section. The dark theme uses the table of this section. The light theme uses the table "Light fills". The stop `panel` is the panel background: `#181B1F` in the dark theme and `#FFFFFF` in the light theme. A hex colour has no change of theme.
- A value that is `null`, `NaN` or `-Infinity` has `f` = 0. A value `+Infinity` has `f` = 1. This is the colour of the first stop, not the text colour.
- The colour of a value mapping wins over the colour mode (9.17).

The formula, for each channel, with `n` stops `s[0]` to `s[n-1]`, and `m` = `n - 1`. Let `i` be the integer part of `f * m`, and `i` = `m - 1` when `f` = 1. Let `t` be `f * m - i`. Let `a` be `s[i]` and `b` be `s[i+1]`.

Let `p` be `s[i-1]`, or `2a - b` when `i` is 0. Let `q` be `s[i+2]`, or `2b - a` when `i` is `m - 1`. The value is `((1 - 3t + 3t^2 - t^3) p + (4 - 6t^2 + 3t^3) a + (1 + 3t + 3t^2 - 3t^3) b + t^3 q) / 6`. Round it to a whole number and limit it to 0 to 255. With 2 stops, the result is the linear interpolation.

The stops of the 10 modes (they are the stops of 4.19, with the stop `panel` in four modes):

| Mode | Stops |
|---|---|
| `continuous_gr_yl_rd` | `green`, `yellow`, `red` |
| `continuous_rd_yl_gr` | `red`, `yellow`, `green` |
| `continuous_bl_yl_rd` | `dark-blue`, `super-light-yellow`, `dark-red` |
| `continuous_yl_rd` | `super-light-yellow`, `dark-red` |
| `continuous_bl_pu` | `blue`, `purple` |
| `continuous_yl_bl` | `super-light-yellow`, `dark-blue` |
| `continuous_blues` | `panel`, `dark-blue` |
| `continuous_reds` | `panel`, `dark-red` |
| `continuous_greens` | `panel`, `dark-green` |
| `continuous_purples` | `panel`, `dark-purple` |

Vectors of the colours, measured on Grafana 12.4.2. The measure has 23 values for each of the 20 cases. The model above is right to 1 level in each channel.

| Mode and theme | `f` = 0 | 0.25 | 0.5 | 0.75 | 1 |
|---|---|---|---|---|---|
| `continuous_gr_yl_rd`, dark | `#73BF69` | `#B4CB4C` | `#E2C03D` | `#F39045` | `#F2495C` |
| `continuous_gr_yl_rd`, light | `#56A64B` | `#A0B52E` | `#D5AC20` | `#E5792A` | `#E02F44` |
| `continuous_bl_yl_rd`, dark | `#1F60C4` | `#89A4AD` | `#D0B98E` | `#DC7F60` | `#C4162A` |
| `continuous_blues`, dark | `#181B1F` | `#1A2C48` | `#1C3E72` | `#1D4F9B` | `#1F60C4` |
| `continuous_blues`, light | `#FFFFFF` | `#C4D3EB` | `#89A8D8` | `#4D7CC4` | `#1250B0` |

The files of `target/ws/f2-facts/` hold the 23 values of each case (`v5-analysis.json`).

The vector set `FE-23` also holds the 36 hex values of the series palette (the tables above).

Thresholds (`{mode, steps}`):

- The steps are in the order of their values. The first step has the value `null`, which means minus infinity.
- The active step of a value `v` is the last step whose value is at or below `v`.
- In the mode `absolute`, the page compares `v` with the steps.
- In the mode `percentage`, the page compares `p = (v - min) / (max - min) × 100`. `min` and `max` come from the field
  configuration, else from the data (9.14). When `max` equals `min`, the active step is the first step.
- A `null` or `NaN` value has no threshold colour. It shows in the text colour of the theme.
- The colour of a value mapping wins over the colour mode (9.17).
- A `timeseries` or a `barchart` can also draw the steps as lines and areas in the plot (`thresholds_style`, 9.19).

### 9.17 Value mappings

The page checks the mappings of a field in array order (`FE-08`). The first mapping that matches gives the result. Each mapping
is one tagged object with `kind` (4.22). In this paragraph, the raw text of a value is the text form of its Grafana
value (9.12). Section 4.22 calls it the display text of the raw value.

| `kind` | Match |
|---|---|
| `value` | The raw text equals the member `value`. |
| `range` | The Grafana value is a number `v`, and `from <= v <= to`. A `null` bound has no limit. A text never matches. |
| `regex` | The member `pattern`, in the Grafana form of 4.24, matches the raw text. `/re/flags` gives `new RegExp("re", "flags")`. Another text `t` gives `new RegExp("^" + t + "$")`. A raw text of more than 1,024 code units never matches (9.14). |
| `special` | The member `match`. `null`: the value is `null`. `nan`: the value is `NaN`. `null_nan`: either. `true`, `false`: the boolean. `empty`: the empty text. |

Each mapping also has the members `text` and `color`. At least one of them is not `null`:

- `text` replaces the display text, and `null` keeps it. The unit does not apply to the new text. For a `regex`
  mapping, each group reference `$1` to `$9` in `text` takes its group of the match, as `String.prototype.replace`
  does.
- `color` replaces the colour of the value (9.16), and `null` keeps it.

### 9.18 Data links

- A field with `links` makes each of its cells a link. With one link the cell is an `<a>`. With several links, a click
  opens a menu of their titles.
- The panel member `links` gives the links of the panel header menu.
- A link has `title`, `url` and `target_blank` (4.23). The page sets `href` with a property setter, never as HTML.
- The `title` of a link can hold the placeholders of variables. The page replaces them as it does for a panel title (9.13), and it writes the text with `textContent`. The `url` keeps the tokens of the next table.

| Token | Value |
|---|---|
| `${__value.raw}` | The text form of the Grafana value of the cell (9.12). |
| `${__value.text}` | The display text of the cell. |
| `${__data.fields.<name>}`, `${__data.fields["<name>"]}` | The text form of the value of the field with this display name or name, in the same row. |
| `${__field.name}` | The display name of the field. |
| `${__from}`, `${__to}` | The epoch milliseconds of the current range. |
| `${__url_time_range}` | `from=<from>&to=<to>`. |
| `${__all_variables}` | `var-<name>=<value>` for each variable, joined by `&`, as in 9.5. |
| `$<name>`, `${<name>}` | The current values of the variable, joined by `,`. |
| `${<name>:queryparam}` | `var-<name>=<value>` for each current value of the variable, joined by `&`. "All" gives the value `$__all`. A variable with no value gives `var-<name>=`. |
| `${__value.time}` | The epoch milliseconds of the first field of the kind `time` in the row of the cell. The page rounds the value down to a whole millisecond, as Grafana does. A `null` time, or a frame with no such field, gives an empty text. |
| `${__value.numeric}` | The number of the cell value, as the display of Grafana gives it. A `number` or `number_text` value gives its number, a boolean gives `1` or `0`, and a time gives its whole milliseconds. `null`, the unit `string` and a `text` field with no unit give `NaN`. A `text` field with another unit gives the JavaScript `Number` of its text. |
| `${__series.name}` | The ref of the query of the frame (4.15a): the ref of the first field that has one, else `A`. |
| `${__timezone}` | The zone of the page (9.9): `utc` or a zone name. The zone `browser` gives the name of the zone of the browser, for example `Europe/Paris`. |
| `${__dashboard.uid}` | The uid of the dashboard. |

- The tokens of the first eight rows: the page encodes each value with `encodeURIComponent`, except
  `${__url_time_range}` and `${__all_variables}`, which are already encoded. An unknown token stays as it is.
- The last six rows are the tokens of Grafana 12. They have the encoding of Grafana 12.4.2, which the
  measures of 2026-11-22 give:
  - `${<name>:queryparam}` has the form of the Grafana function `urlUtil.toUrlParams`. The page encodes each value with
    `encodeURIComponent`. Then it decodes `@`, `:`, `$`, `,` and `;` again. It writes `!`, `'`, `(`, `)` and `*` as
    `%21`, `%27`, `%28`, `%29` and `%2A`. A blank is `%20`. A `constant` keeps `@`, `:`, `$`, `,` and `;` encoded,
    because Grafana keeps no URL state for a constant.
  - The other five tokens have the Grafana format `uriencode`: `encodeURI`, then `%21`, `%27`, `%28`, `%29` and `%2A`
    for `!`, `'`, `(`, `)` and `*`. Thus `/` and `+` stay, as in `Europe/Paris` and `1e+21`. Their values are numbers,
    zone names, uids and refs (4.3, 4.6), so they have no `&`, `=`, `?`, `#` and no blank.
  - A token outside a cell keeps its text: a panel link has no `${__value.time}`, `${__value.numeric}` and
    `${__series.name}`. A dashboard link replaces only the tokens of 9.8a.
- Security of the tokens (FE-09, B211). Each value is encoded, so a value cannot add a line end, a parameter or a
  fragment. The text of `${<name>:queryparam}` starts with `var-`, so it is never a scheme. The page replaces the tokens
  in one pass, so a token in a value stays text. The check of the next item applies to the final URL, so no value can
  give another scheme or a start with `//`. The work is linear in the length of the values.
- Known differences with Grafana 12.4.2 (measured on 2026-11-22). Grafana writes the tokens of the first eight rows with
  the format `uriencode`, and Hune keeps `encodeURIComponent`: `${__value.raw}` of `c&d` gives `c&d` in Grafana and
  `c%26d` in Hune. Grafana writes `${<name>}` of a multi variable as `{a b,c}`. Grafana 12 writes `${__url_time_range}`
  as `from=<ISO time>&to=<ISO time>&timezone=<zone>`, and Hune keeps the epoch milliseconds. A `text` field with a date
  unit gives the time of its text in Grafana, and `NaN` in Hune.
- After the replacement, the URL MUST start with `/`, `?`, `#`, `http://` or `https://`, the rule of the prefix of 4.23
  (`FE-09`). It MUST NOT start with `//` or `/\`, because a browser reads `/\` as `//`. The scheme compares with ASCII
  letters folded to lower case. Another URL gives no link: the cell shows plain text, and the page writes a warning in
  the console.
- `target_blank: true` gives `target="_blank"` and `rel="noopener noreferrer"`. Else the link opens in the same tab.
- The importer rewrites the links to `/grafana/d/<uid>` with its option `--link-prefix` (15). It removes each U+FEFF of
  a URL (15.6).

### 9.19 Panel types and their options

Hune has the 7 types of the real panels: `table`, `timeseries`, `stat`, `barchart`, `piechart`, `xychart` and
`bargauge`. It has the type `text` (9.19b), the types `state_timeline` and `status_history` (9.19c), and the types `heatmap` (9.19e), `histogram` (9.19f), `gauge` (9.19g) and `logs` (9.19h). Each type has only the options below.

Sections 4.26 to 4.33e give the members, and each member is necessary except the optional ones. The column "New" gives the value of a new panel (10.6). Before release 0.1.0, M0 can remove an option that
no real panel uses (4.26).

A panel can have several queries (4.15a). Each type draws the frames as 9.22a says. A type that draws one frame only, `barchart` and `xychart`, cannot have extra queries.

Shared objects (4.26):

| Object | Members | Behaviour | New |
|---|---|---|---|
| `legend` | `show`, `placement`, `mode`, `values`, `sort_by`, `sort_desc` | `show = false`: no legend. `placement`: `bottom` or `right`. `mode = "list"`: one line of series names with their colours. `mode = "table"`: one row for each series, with the columns of `values` (each one is a `calc` value of the next table, or `value` or `percent` in a `piechart`), the "Values" mode of the chart engine. `sort_by` and `sort_desc` sort the rows of the table (below). | `{show: true, placement: "bottom", mode: "list", values: []}` |
| `tooltip` | `mode`, `sort`, `hide_zeros` | `single`: the series under the cursor. `multi`: all series. `hidden`: no tooltip. `sort` and `hide_zeros` change the rows of a `multi` tooltip (below). | `{mode: "single"}` |
| `reduce` | `calc`, `fields`, `values` | `values = true`: one item for each row, with no reduction. `values = false`: one item for each field, reduced with `calc`. `fields = null`: each field of the kind `number` or `number_text`. Else `fields` is a pattern in the Grafana form of 4.24, as `match.value` of 9.14: it tests the base display name (9.13). Example: `/^Total$/` keeps the field `Total` only. | `{calc: "last_not_null", fields: null, values: false}` |

| `calc` | Result |
|---|---|
| `last_not_null` | The last value that is not `null` and not `NaN`, else `null`. |
| `last`, `first` | The last or the first value. |
| `first_not_null` | The first value that is not `null` and not `NaN`, else `null`. |
| `min`, `max` | The lowest or the highest value that is not `null` and not `NaN`, else `null`. |
| `mean` | The sum divided by the count of the values that are not `null` and not `NaN`, else `null`. |
| `total` | The sum of the values that are not `null` and not `NaN`, or 0. |
| `count` | The number of values, `null` included. |
| `median` | The middle of the values that are not `null` and not `NaN`, sorted up. With an even count, the mean of the two middle values. `null` when no value. |
| `variance` | The mean of the squares of the differences to `mean`. The sum is divided by the count, not by the count less 1. The values are the numbers that are not `null` and not `NaN`. It is 0 for a field with no such value, and `null` for no row. |
| `std_dev` | The square root of `variance`. |
| `range` | `max` less `min`, else `null`. |
| `diff` | `last_not_null` less `first_not_null`, else `null`. |
| `diff_percent` | `diff / first_not_null × 100`, by the division of JavaScript: a first value of 0 gives `∞`, `-∞` or `NaN`. |
| `delta` | The sum of the rises, as Grafana counts them (below). `null` for a field that is not of the kind `number`. |
| `distinct_count` | The number of different values that are not `null`. All the `NaN` values count as one. |
| `change_count` | The number of values, not `null`, that differ from the value before them. |

Some calculations select one value: `first`, `last`, `first_not_null`, `last_not_null`, `min` and `max`. For a
`number_text` field, they keep the exact text of the value, and `min` and `max` compare exact decimals. `mean` and
`total` use the doubles. A `null` result shows `no_value` (9.14), else "No data". The vector id of `calc` is `FE-10`.

The calculations from `median` down use the doubles. The result of `count`, `distinct_count` and `change_count` is a whole number, and it has no unit. Another calculation keeps the unit of the field, and `diff_percent` has the unit `percent`.

The rule of `delta`. Let `x1`, `x2` and so on be the values that are not `null` and not `NaN`, in order. `delta` starts at 0, and a flag `up` starts as `true`. For each `xk` after the first, `w = xk - x(k-1)`.

- When `x(k-1) > xk`, the flag `up` becomes `false`.
- Else, when `up` is `true`, `w` is added to `delta`. Then `up` becomes `true`.

Thus a fall (a counter reset) removes the first rise after it. Example: `1, 2, 3, 1, 2, 5` gives 1 + 1 + 0 + 3 = 5.

`table` (4.27; 9.20 gives the features):

| Option | Values | New |
|---|---|---|
| `show_header` | boolean | `true` |
| `sort` | 0 to 8 sort keys `{field: <display name>, desc: <boolean>}`: the first sort | `[]` |
| `wrap` | boolean, optional: `true` wraps a long cell text (9.20) | `false` |

`timeseries` (4.28):

| Option | Values | New |
|---|---|---|
| `draw` | `line`, `bars`, `points` | `line` |
| `interpolation` | `linear`, `smooth`, `step_before`, `step_after` | `linear` |
| `line_width` | 0 to 10 CSS pixels; 0 draws no line | 1 |
| `fill_opacity` | 0 to 100 percent; above 0 gives an area | 0 |
| `show_points` | `auto`, `always`, `never` | `auto` |
| `point_size` | 1 to 40 CSS pixels | 5 |
| `span_nulls` | boolean; `false`: a `null` breaks the line | `false` |
| `stacking` | `none`, `normal`, `percent` | `none` |
| `y_scale` | `linear`, `log` | `linear` |
| `legend` | The shared object. | The shared value. |
| `tooltip` | The shared object. | The shared value. |
| `thresholds_style` | `off`, `line`, `dashed`, `area`, `line_area`, `dashed_area` (optional) | `off` |
| `gradient_mode` | `none`, `opacity`, `hue`, `scheme` (optional): the gradient of the fill and of the line (below) | `none` |
| `log_base` | `2`, `10` (optional): the base of a log scale | `10` |
| `span_nulls_ms` | 1 to 1,000,000,000,000 ms (optional): the null threshold (below) | absent |
| `bar_alignment` | `before`, `center`, `after` (optional): the place of a bar against its time | `center` |

- The x axis is the first field of the kind `time`. A panel with no such field shows the error "The data has no time
  column.".
- Each field of the kind `number` or `number_text`, other than the x axis, is a series. `min` and `max` of the field
  configuration set the y axis.
- The page draws with the chart engine of 9.24: `xKind: "time"`, one `Float64Array` for each series, `type` from
  `draw` and `fill_opacity`, `stack` from `stacking`. `y_scale = "log"` is the engine option `yScale: "log"`: a
  log axis for positive values. Its base is 10, or 2 with `log_base` = 2 (log scale, below).
- The copied engine draws each value of these options (4.28). A value that it does not draw yet is an engine change of
  9.24.
- The x axis covers the requested time range, from `from` to `to`, as in Grafana. The exception is `draw = "bars"`: its x axis keeps the span of the data. A page with no range, such as the preview of the editor, also keeps the span of the data.
- The SQL makes the buckets with `$__timeInterval` and the panel width (7.4). For each pixel column of a series, the engine takes the first, the lowest, the highest and the last value.
- The engine draws one connected line through these values, in time order. The first and the last value only join the columns.
- A time series breaks its line only at a `null` value, unless `span_nulls` is `true`. A value between two `null` values draws as a mark. The mark has the width of the line, and it is not a dot. The other charts keep the rule of the engine: a gap of more than 1.5 steps breaks the line.

The automatic y axis applies to each scale of a time series, also to bars, to a stack and to a field with ends. `y_scale = "log"` keeps the rule of the engine (log scale, below). `stacking = "percent"` has its own soft ends (below).

The automatic range is the `rangeNum` rule of uPlot, as Grafana uses it. Let `lo` and `hi` be the lowest and the highest drawn value of the series of the scale, and `d = hi - lo`. In a stack, the drawn values are the tops of the stack. Its base does not count, so a stack far from 0 does not start at 0:

1. The range is flat when `d` is below 1e-24. It is also flat when the larger of `|lo|` and `|hi|` is above 10^10 times `d`. For a flat range `d` is 0, or 1e-24 when `lo` or `hi` is 0.
2. The scale `m` is `d`. When `d` is 0, `m` is the larger of `|lo|` and `|hi|`, or 1000 when that is 0 too.
3. The step of the edges is `r = 10^floor(log10(m)) / 10`.
4. The room on each side is `0.1 × m`. A flat range has the room `m`, but `0.1 × m` on a side whose value is 0.
5. `min` is `lo` less its room, rounded down to a multiple of `r`. `max` is `hi` plus its room, rounded up to a multiple of `r`.
6. When `lo` is 0 or more and `min` is 0 or less, `min` is 0. When `hi` is 0 or less and `max` is 0 or more, `max` is 0. Thus the room never crosses 0 when the data does not cross it. A soft end replaces this step on its side (y axes, rule 7).
7. When `max` is not above `min`, `max` is `min + 1`.

- The ticks are the multiples of a step inside the range. The step is the smallest of 1, 2, 2.5 and 5 times a power of ten that puts two ticks 30 px apart or more. On a plot of 150 px or less, two ticks are 15 px apart or more. This rule also applies when the field sets `min` or `max`, and to the axis of bars. The `xychart` keeps 30 px. With the unit `bytes`, the steps are powers of two.
- Grafana 12.4.2 has this rule in the code that it serves (`calculateSpace`), as for the histogram (9.19f). The measure of 2026-11-16 found the steps of Grafana with this rule on 115 time series panels. Their plots were 19 to 817 px high.
- With `stacking = "percent"`, the axis has the soft ends 0 and 100 % (y axes, rule 7). The ends of the field have no effect on it. Thus it goes from 0 to 100 % when no top is below 0, and the tops -100 % to 100 % give -120 % to 100 %. The ticks follow the rule above. A tick label is the percent with the least digits and the sign `%`, for example "20%", for each unit of the field. Grafana 12.4.2 has this rule (measure of 2026-11-21).
- A second measure of 2026-11-16 found the ranges of Grafana with these rules on 100 panels. The panels had bars, stacks, soft ends and hard ends, with plots of 57 to 817 px. For example, bars of the data -50 to 80 give -70 to 100. A stack with the tops 5 to 144 gives 0 to 160, and the tops 1,000 to 2,017 give 800 to 2,200.
- The edges of the range do not move to a tick. A field with `decimals` writes every tick with them, for example "70.0%". Else a tick label has the least digits that show the value, with no zero at the end of its decimals. The ticks 2, 1.5, 1, 0.5 and 0 of the step 0.5 read "2", "1.5", "1", "0.5" and "0". With a unit, the label is the text of 9.15.
- The cursor shows a dashed vertical line and a dashed horizontal line. It shows one dot of 4 px on the series that is nearest to the pointer. It shows no pill on the axes.

Stacks (`stacking` = `normal` or `percent`; engine edits E32 and E33 of 9.24). The stacks of a time series and of a vertical `barchart` follow Grafana 12.4.2 for lines, areas and bars:

1. A series goes in the group below 0 when more of its values are below 0 than above 0. Else it goes in the group above 0. Thus a tie goes above 0.
2. The count starts at the first value that is not `null` and stops at the last such value. Its step is `floor(n / 100)` values, and 1 at least. `n` is the number of values in that span. A value of 0 does not count. With `transform = "negative_y"`, the series goes in the other group.
3. Each group stacks from 0 in field order, inside one scale (y axes, rule 12). Thus the group below 0 stacks down. Each value adds to the top of the series before it in the group, with its sign. Thus a value below 0 of a series in the group above 0 goes down from that top.
4. Another series of the group can have a value at the time of a `null` value. Then the `null` value is at the top of the series before it in its group, or at 0. When no series of the group has a value there, the series has no point there. Its line breaks there, unless `span_nulls` is `true`.
5. With `percent`, a top is the sum of the group up to its series, divided by the sum of the whole group, times 100 %. The sums keep their signs, so the group below 0 goes from 0 to -100 %. A whole sum of 0 gives 0. A series with values of both signs can give a top below -100 % or above 100 %.
6. The range of the y axis uses the tops of both groups, with the automatic rule above.
7. A series with another `draw` than the chart goes in a group of its own. Thus the lines, the bars and the points of one chart stack apart, each from 0, and each group has its own percent. Grafana also puts two lines with another interpolation in two groups. Hune has the interpolation of the whole panel only (4.28), so the draw is the whole rule.

- The measure of 2026-11-21 found these rules on 100 panels of Grafana 12.4.2: 20 cases at 5 heights, with plots of 57 to 817 px. The cases had a series below 0, series of both signs, a tie, `null` values, `negative_y`, lines, areas, bars and percent.
- The ticks of the 100 panels agree with the rules. The drawn lines agree within 1 px. On the plots of 57 px, they agree within 1.5 px, because two ticks only give the scale of the measure there. For example, a series of 5 to 95 and a series of -70 to -10 give the axis -90 to 120.
- With these rules, Hune draws the same ticks on the 100 panels. Its lines are within 0.5 px of the lines of Grafana, and its bars are within 1.5 px.
- Grafana has the rules in the code that it serves (`getStackingGroups` and `preparePlotData2`). Its `barchart` uses the same code, with one group for all the fields. The `histogram` stacks its counts in field order (9.19f).
- The measure of 2026-11-22 found rule 7 on 24 time series panels: 8 cases at 3 heights, with plots of 133 to 437 px. Before rule 7, Hune drew other ticks or marks on 21 of the 24 panels. With it, Hune draws the same ticks within 0.5 px on the 21 panels that it can draw. Its lines, bars and points are within 1 px. The other 3 panels have a line interpolation for one series (19.20a).
- The same measure found these rules on 45 vertical bar charts: 15 cases at 3 heights, with stacks, values below 0, percent and soft ends. A series of both signs adds each value with its sign. Thus the bar of the value -25 on a part of 10 goes from 10 to -15.
- The plot of Grafana is 10 px lower there (19.20a). With the plots at one height, Hune draws the same ticks on 42 panels within 0.5 px, and its bars are within 1.5 px. Before, 3 panels were equal.

Log scale (`y_scale = "log"`). A time series keeps the rule of the engine on a log scale. Grafana 12.4.2 has other rules there (19.20a).

1. The series do not stack, also with `stacking` = `normal` or `percent`. Each series draws its own values. A value of 0 or below has no point.
2. The range goes from the lowest to the highest value of the series of the scale, with no room. When a value is 0 or below, the range starts at a tenth of the highest value. When no value is above 0, the range is 1 to 10. When the values are all equal, the range ends at ten times the value.
3. Before step 2, `soft_min` and `soft_max` move the data ends: the lowest value becomes the lower of it and `soft_min`. The highest value becomes the higher of it and `soft_max`. `min` and `max` have no effect.
4. The ticks are 1, 2 and 5 times each power of ten inside the range. When there are more than 7 ticks, only the powers of ten stay. When the highest end is less than 8 times the lowest end, the ticks are those of a linear axis with 4 intervals.
5. A tick label has the unit of the field (9.15). With no unit, it has the short form of the engine (K, M, B, T). The labels of a percent stack have no sign `%`.

Log scale of base 2 (`log_base` = 2). This scale follows Grafana 12.4.2 (uPlot `rangeLog` and `log2AxisValsFilt`). The rules above stay for `log_base` = 10.

1. The range starts at the power of 2 at or below the lowest value above 0. It ends at the power of 2 at or above the highest value. A value of 0 or below has no point and does not count.
2. `min` and `max` of the scale replace the data ends, rounded down and up to a power of 2. `soft_min` and `soft_max` have no effect, as in Grafana.
3. When the data has one value v, the range is v / 2 to 2 v. When no value is above 0, the range is 1 to 2. Grafana draws no axis there (19.20a).
4. Each power of 2 in the range has a grid line. A label is on every k-th power from the top. The number k is the least that puts two labels 30 px apart or more. On a plot of 150 px or less, 15 px is enough.
5. A label has the unit of the field (9.15). Its decimals are those of the linear step of the ticks of this rule for a linear axis with the same ends. Thus the range 0.0625 to 1 has the labels "0.1", "0.1", "0.3", "0.5" and "1", as in Grafana.
6. With `decimals` = 0, the range starts at 1 or above. A range of more than 400 powers keeps only the grid lines of its labels.

The measure found these rules on 10 panels. For example, the values 1 to 910 on a plot of 171 px give 11 grid lines. The labels are 1, 4, 16, 64, 256 and 1024.

Time zone of the page. The zone of the dashboard is the member `time.timezone` (4.6): `browser`, `utc` or an IANA name. The helper `zone.js` has the function `formatTime(ms, zone, pattern)`. `zone` is `browser`, `utc` or an IANA name that the browser knows (9.9). A name that the browser does not know gives the zone `browser`, also in `formatTime`.

`pattern` has the tokens `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS`, and the other characters stay as they are.

- Each time text of the page uses `formatTime` in the zone of the dashboard. Code never uses `toLocaleString` or `getHours` without a zone for such a text.
- These texts are the labels of the x axis, the head of the tooltip and the cursor time. They are also the cells of a `time` field (9.15) and the unit `dateTimeAsIso`. The texts of the pickers (9.9) and the times of the history (10.11) are in this group too.
- With `utc`, the engine puts the ticks of the x axis at UTC boundaries. The labels have the same patterns as with `browser`. The engine gets the zone as an option (9.24).
- A `utc` dashboard shows the same times in each browser zone. A test runs the page in two zones and compares the labels (16.10).

Thresholds in the plot (`thresholds_style`). The thresholds are those of the first series, in field order (9.14, 9.16). The page draws them under the series, and it does not change the range of the y axis.

- `line` and `dashed`: one horizontal line for each step except the first (the base step), at the value of the step. A line is 2 px wide. A `dashed` line has dashes of 10 px and gaps of 10 px.
- `area`: bands that fill the width of the plot. The band of a step goes from its value to the value of the next step. The first band starts at the bottom of the plot, and the last band ends at the top. A band has the colour of its step.
- `line_area` and `dashed_area`: the bands first, then the lines.
- A colour that has no alpha channel gets the alpha 0.7 for a line and 0.15 for a band. A colour with an alpha channel keeps it. The colour `transparent` draws nothing.
- Grafana has this rule for a step with the colour `transparent`. Each line from the second step to the first such step takes the colour of the step before it.
- With the mode `percentage`, the value of a step is `lo + (hi - lo) × p / 100`. `p` is the value of the step. `lo` and `hi` are `min` and `max` of the field configuration, else the ends of the y axis.
- A line outside the y axis is not drawn. A band is cut at the ends of the plot.
- In a `barchart`, the lines and bands follow the value axis. They are vertical when `orientation` is `horizontal`.
- The style has no effect on the legend, the tooltip or the scale. A field with no `thresholds` has the default steps of 9.14, so `line` draws a red line at 80.

Hidden series (`hide_from`). The property of 4.18 applies to each field in `timeseries`, `barchart` and `xychart`. The viewer of Grafana hides a series with a click on the legend. Grafana then saves an override that has the matcher `names` and `hide_from` (9.14).

- `viz = true`: the page does not draw the series. It does not count for the range of the axes, and it does not take part in the stack. The legend shows the entry as off, and a click on it shows the series again.
- `legend = true`: the legend has no entry for the series, in the modes `list` and `table`. A series with `viz = true` and `legend = false` keeps its legend entry, shown as off (half opacity).
- `tooltip = true`: the tooltip has no row for the series, and the cursor has no dot on it.
- A hidden series stays in the data. "View data" still shows its field, and a calculation of the legend can still use it.
- A click on a legend entry hides or shows its series for this page only (the engine function). The click never changes the document, and it never changes `hide_from`.

Y axes (`axis_placement`, `axis_label`, `soft_min`, `soft_max`). A chart has two scales at most: the left scale and the right scale.

1. The placement of a series is `axis_placement`. It is `auto` when the member is not set.
2. `left` puts the series on the left scale. `right` puts it on the right scale.
3. `U_L` is the unit (9.15) of the first series, in field order, whose placement is `auto` or `left`. A series with `hidden` never gives `U_L`. A series with `auto` or `hidden` goes on the left scale when its unit equals `U_L`, else on the right scale. When every series is `hidden`, no axis shows, and all series go on the left scale. This is the Grafana rule for the first two axes: `auto` goes left when no earlier series has a left axis, and goes right after that (paper, V1).
4. A chart with no series on the right scale has no right scale. A third unit shares the right scale. Grafana makes one scale and one axis for each set of unit, placement, `min`, `max`, soft ends and scale type. So it draws a third axis. It also draws two left axes for a `left` series and an `auto` series of another unit. These are known differences (19.20a).
5. Each scale has its own range. The range follows the automatic rule of this paragraph (steps 1 to 7) on the series of the scale.
6. The `min` and `max` of the first series of the scale that sets them are hard ends. A hard end on a side with no soft end is the end of the axis on that side. It also replaces the data end of that side (`lo` or `hi`) in steps 1 to 5. With a soft end on the same side, the hard end is a limit: the end of the axis does not go past it. For example, `min` = 0 with the data 5 to 95 gives 0 to 105.
7. A soft end applies after step 5 of the automatic rule, in place of step 6 on its side. When `lo` is `soft_min` or more, `min` is `soft_min`, else `min` stays the result of step 5 (also below 0). `soft_max` has the same rule for `hi` and `max`. Thus the axis ends at a soft end when the data stays inside it, and a soft end never cuts the data. With the data 5 to 95, `soft_max` = 200 gives 0 to 200, and 50 gives 0 to 104. `soft_min` = 10 gives -4 to 104.
8. A scale shows its axis unless every series of the scale has the placement `hidden`. A hidden axis takes no room. The series stays on its scale.
9. The title of a scale is the `axis_label` of its first series that sets it. It is a text of 12 px at weight 400 in the colour of the axis labels. The title of the left scale turns by -90 degrees, and the title of the right scale turns by 90 degrees. Each title takes 28 px, and its centre is 12 px from the outer edge of the plot area. A title that is longer than the axis ends with an ellipsis.
10. The left gutter is the width of the widest label of the left scale plus 9 px, rounded up. The labels end 5 px before the plot. The right gutter has the same rule for the right scale. Its labels start 5 px after the plot, and it replaces the margin of 8 px at the right. A title adds 28 px to its gutter. Grafana measures the first of the longest labels, so its gutter can be 1 px less.
11. The grid lines are those of the scale of the first series, in field order, whose axis shows. The ticks of the other scale follow the rule of the ticks of this paragraph. They draw no grid line. When no axis shows, the chart has no grid line.
12. The stacks of `stacking` work inside one scale. A series stacks only with the series of its own scale.
13. `y_scale = "log"` applies to each scale. The thresholds of `thresholds_style` draw on the scale of the first series, as before.
14. The cursor, the tooltip and the legend keep the unit and the decimals of each series. A table legend ends the name of a series on the right scale with ` (right y-axis)`. The mark has 12 px and the colour of the muted text. A list legend has no text mark. The items of the series on the right scale go after the other items, at the right end of the row. The rows of a table legend keep the field order (paper, V2).
15. A `barchart` has one value axis. `axis_label`, `soft_min` and `soft_max` of its first field that sets them apply to it. `axis_placement` has no effect there (4.18). A soft end of a `barchart` follows rule 7. A side with no soft end and no `min` or `max` has the soft end 0, as in Grafana 12.4.2.
16. The test reads the scales in the attribute `data-axes` of `.panel__plot` (9.29).

The rules of steps 3, 4, 9, 10, 11 and 14 follow the check on Grafana 12.4.2 (paper, 10.8, V1 and V2).

Series style (`draw`, `line_width`, `fill_opacity`, `show_points`, `point_size`, `line_dash`, `transform`). The rule of 9.14 gives the style of a series. A series can have a style that differs from the style of the panel.

- `draw` sets the series to a line, to bars or to points, with the rules of the option `draw` (4.28). The bars and the lines of one chart draw in field order.
- `line_width` 0 draws no line. `fill_opacity` above 0 draws an area. `show_points` and `point_size` follow the options of 4.28.
- `line_dash` = `dash` draws dashes of 10 px and gaps of 10 px: `setLineDash([10, 10])` with a butt cap. `dot` draws round dots 10 px apart: `setLineDash([0, 10])` with a round cap.
- `solid` draws a plain line. The arrays do not depend on the line width. The dash does not change the fill (paper, V3).
- `transform` = `negative_y` draws the value with the opposite sign: the point of the series at time `t` is at `-v`. The range of the scale follows the drawn values. The ends that the series sets (`min`, `max`, `soft_min` and `soft_max`) swap and change sign, because the axis shows the drawn values.
- The data of the field, "View data", the values of the legend and the values of the tooltip do not change with `negative_y`. The axis shows the drawn values (`-100%` to `0%`). Grafana does the same (paper, V3).
- The test reads each style in the attribute `data-series-style` of `.panel__plot` (9.29). Each style also holds `dashArray`, the array that the engine draws, or `null`.

Gradient modes (`gradient_mode`). The option applies to the fill and to the line of each series of a `timeseries`, and to the bars of a `barchart`. The values are those of Grafana 12.4.2. They come from the code that it serves and from a probe of the canvas calls (83 panels, 2026-11-22). The engine paints a gradient with a function of each series (E43, 9.24).

- `none` gives the plain colour of the series, as before.
- `opacity`: the fill is a linear gradient over the plot box, from its top to its bottom. The top has the colour of the series at the alpha `fill_opacity` / 100. The bottom has the alpha 0. The line keeps the plain colour. For the fill 50, Grafana draws `#73BF6980` at the top and `#73BF6900` at the bottom.
- `hue`: the fill and the line are linear gradients over the plot box. They go from the colour T at the top to the colour B at the bottom. The fill has the alpha `fill_opacity` / 100, and the line has the alpha 1.
- T is the colour of the series with the saturation 20 points higher and the hue turned by 20 degrees. Then each channel goes up by 10 % of 255, and the lightness goes up by 10 points. B is the colour with the hue turned by -25 degrees and the lightness 5 points lower. The dark theme makes B 10 more points darker.
- These steps are those of tinycolor2, as Grafana calls them. A saturation and a lightness keep two decimals of their percent, and the channels round before the step of 10 %. Thus `#73BF69` gives T = `#9AF3AE` and B = `#719A42` (dark) or `#8AB757` (light).
- `scheme`: the fill and the line take the colour of the colour mode (9.16) at each value of the scale of the series. The fill has the alpha `fill_opacity` / 100, and the line has the alpha 1. The mode `fixed` gives the plain colour, as `none`.

The ends of a scheme are `lo` and `hi`. `lo` is `min` of the field, else `soft_min`, else the lowest drawn value of the visible series of the scale (E30). `hi` has the same rule. When the data has one value only, the ends of the scale apply.

- The mode `thresholds` gives hard bands: each step starts at its value, and the first step starts at the bottom of the scale. A step of the mode `percentage` is at `lo + (hi - lo) × v / 100`. The colour `transparent` has the alpha 0.
- A continuous mode spreads its stops (4.19) evenly from `lo` to `hi`, with the shades of the theme. The canvas mixes two stops in a straight line, not with the spline of 9.16.
- The mode `palette` spreads the 18 colours of the series palette in the same way. Grafana spreads its classic palette of 64 colours there (19.20a).
- The gradient of a scale runs from the place of its first stop to the place of its last stop, as uPlot `scaleGradient`. The places are whole device pixels, and the last colour stays above its stop.
- The points of a line, and a value between two `null` values, take the gradient of the line. The legend and the tooltip keep the plain colour of the series, as in Grafana.
- A bar chart paints one gradient over the plot for all its bars. A horizontal bar chart (Hune code) paints the fill of each bar with one CSS gradient over the track. The gradient goes from the right end (T) to the left end (B). Its line keeps the plain colour, where Grafana paints the gradient (19.20a).
- With `fill_opacity` 0, no fill shows. Thus `opacity` changes nothing on a line with no fill, and the audit has an entry for it (10.3h).

Cosmetic series options. These members follow Grafana 12.4.2. The values come from the same probe and from the code that Grafana serves.

- Dash array (`line_dash_array`, 4.18). The engine calls `setLineDash` with the array in device pixels, as uPlot does (E45). Thus `[10, 20]` gives dashes of 10 px and gaps of 20 px at dpr 1, and of 5 and 10 CSS px at dpr 2. The cap is round for `dot` and butt for `dash`. The default arrays of `dash` and `dot` follow the same rule. A set with `line_dash` or `line_dash_array` replaces both members of an earlier set (4.18, 9.14).
- Null threshold (`span_nulls_ms`, 4.28). With `span_nulls` = `false`, a run of `null` values joins when the values on its two sides are less than `span_nulls_ms` apart on the time axis. A run at the start or at the end of a series stays a break. A series that draws bars keeps its gaps.
- Grafana has this rule in the code that it serves (`nullToUndefThreshold`, with a strict `<`). With values at a step of 1 minute, 150000 ms joins a gap of 2 minutes, and 120000 ms joins no gap.
- Bar alignment (`bar_alignment`, 4.28). `center` centres a bar on its time. `before` ends the bar at its time less half the line width. `after` starts it at its time plus half the line width (E44).
- Grafana 12.4.2 draws the bar of the time 25 px with a line of 1 px. The bar starts at 2 px (`center`), -21 px (`before`) or 26 px (`after`). Grafana rounds each edge to the pixel.
- Centred zero (`axis_centered_zero`, 4.18). The automatic range of the scale starts from the data ends `-m` and `m`. `m` is the larger of `|lo|` and `|hi|`, or 80 when both are 0. Then steps 1 to 7 and the ends of the scale apply. The first series of the scale gives the member.
- The centred zero has no effect on a log scale or on a percent stack. The measure gives -60 to 60 for the data 10 to 50, and -60 to 100 with `soft_max` 100. All values 0 give -100 to 100, and a bar chart of 10 to 40 gives -48 to 48.
- Axis width (`axis_width`, 4.18). The gutter of the axis is the width in CSS pixels, and a title adds 20 px (E46). The labels stay 5 px from the plot, also when they leave the canvas. The widest width of the series of the scale applies, because Grafana merges the axes of one scale.
- Axis border (`axis_border`, 4.18). The axis gets a line of 1 device pixel on the edge of the plot, from its top to its bottom. Each tick gets a mark of 4 px outside the plot. The labels move to 9 px from the plot, and the gutter does not change. The first series of the scale gives the member.
- Axis colour (`axis_color`, 4.18). `series` gives the labels, the line and the marks the colour of the first series of the scale. With `gradient_mode` = `scheme` and the colour mode `thresholds`, they take the scheme of that series. `text` keeps the colour of the labels.
- A horizontal bar chart (Hune code) applies the centred zero to its own range. Its axis band has the height `axis_width`, and the border is a line and marks of 4 px under the bars. A hidden axis has no width, no border and no colour.
- The symmetric log scale of Grafana (`scaleDistribution` `symlog`) is not in this version. The importer keeps it `unsupported` (15.6).

Legend sort (`sort_by`, `sort_desc`). A legend in the mode `table` sorts its rows when `sort_by` is set. A legend in the mode `list` ignores the sort.

- The key of a row is the result of the calculation `sort_by` on the field of the series (9.19 gives each calculation). For `name`, the key is the name of the series. The page computes the calculation even when `values` does not hold it.
- With `name`, the names compare as `Intl.Collator("en", {sensitivity: "base"}).compare` does. With a calculation, the numbers compare.
- A row with a key that is `null` or `NaN` goes last, in both directions. Rows with equal keys keep the series order.
- `sort_desc = true` puts the largest key first, or the last name first. A `piechart` uses the name, the value or the percent of the slice as the key.

Tooltip sort and zeros (`sort`, `hide_zeros`). These members change the rows of a tooltip with `mode = "multi"`. They have no effect with the modes `single` and `hidden`.

- `sort = "asc"`: the rows go from the lowest to the highest value. `desc`: from the highest to the lowest. `none`: the series order.
- A row with no value (`null` or `NaN`) goes last. Rows with equal values keep the series order.
- `hide_zeros = true`: the tooltip has no row for a value that equals 0.

Look of the charts (`timeseries`, `barchart`, `xychart`):

- The axis text is 12 px, in the font of the page and in the text colour, at weight 400.
- Series colours. A series with no explicit colour has the slot of its place in the series palette of 9.16. The place is the index `i` of the series among the number fields, in field order, from 0.
- The line, the fill, the bar, the point, the legend mark and the tooltip mark use that hex of the theme. A series that has an explicit colour keeps it. This is a fixed colour, a continuous mode, a threshold colour or a value mapping. A hidden series keeps its slot. The page gives each series its colour again when the theme changes.
- The legend stays readable. The legend text keeps the text colour of the theme, and its name has 4.5:1 with the panel. The mark has 3:1 with the panel (9.16).
- The axes have no base line, no zero line and no tick marks. The grid lines are the only lines. The exception is a y axis with `axis_border` = `true` (P14, below).
- The x axis has one row of labels, with no row for the date. Two ticks are the width of a label (in the format of its step) plus 11.5 px apart. The steps are those of Grafana: there is no step of 2 minutes, and there are steps of 4 and 8 hours. A day step starts at the first midnight in the range. A step of 1 minute writes the seconds.
- A range of more than 24 hours writes `MM/DD HH:mm`. A shorter range writes no date, also at midnight.
- The plot has a margin of 8 px at the top and at the right. It has a band of 21 px at the bottom. The margin at the left is the width of the widest y label plus 9 px, rounded up. A y label ends 5 px before the plot.
- When no y axis shows on either side, the plot has no margin at the top (E27, 9.24). This is the rule of Grafana 12.4.2 for a time series whose y axes are all hidden. In that case Grafana also hides the time axis, because the defaults of the panel apply to the time field too. Then its plot fills the canvas. Hune keeps the time axis with its band and its side margins. This is a known difference.
- A fill keeps the opacity of `fill_opacity`, also where fills of series that do not stack overlap. The fills add up there.
- The legend text is 12 px at weight 400 in the text colour. The mark of a series is a pill of 14 x 4 px, 8 px before the name. The items are 10 px apart. The legend, in the mode `table` or `list`, scrolls when it is more than 35 % of the inner height of the panel. This holds under the plot, also when a legend at the right has moved there (below).
- The legend has no "Values" or "List" button. The option `legend.mode` still sets the mode.
- The legend table has the column "Name" first. The columns of `legend.values` follow in the document order. A head is a button that sorts the table in three steps: up, down, series order. The head has `aria-sort` and the link colour. A row is 23.8 px high, with a rule of 1 px.
- A legend column has one precision for all the series of one unit. The precision is the most decimals that a field sets or that 9.15 gives for a value of the column. The total of a `percent` or `percentunit` series shows as a plain number with no `%`.
- A legend at the right keeps the plot at least 220 px wide. When the panel is too narrow for that, the legend goes under the plot.
- A legend that has no entry takes no room. This is the case when `hide_from.legend` is `true` for every series. At the right, it leaves no gap, and the plot has the whole width.
- A legend in the mode `table` with `values` empty has the column "Name" only.
- The tooltip is a card. It has the surface, the border and the radius (6 px) of a menu, and a shadow. The head band of the card is 30 px high. It shows the time (12 px, weight 500, full text colour) with a rule of 1 px under it.
- A row has a mark, a name and a value. The mark is a pill of 14 x 4 px. The name is 12 px and muted. The value has weight 500. No row is highlighted. The head shows the milliseconds only when the median step of the x axis is below 1 s.
- The card sits 10 px to the right of the cursor and 10 px under it. It flips to the other side at an edge of the plot.
- The value of a tooltip row has the decimals of the field when the field sets them. Else it has 3 significant digits in its unit, such as `31.2 K`. A whole number has no decimals ("69", not "69.0"), and zeros after the point drop ("1 KiB").

`stat` (4.29):

| Option | Values | New |
|---|---|---|
| `reduce` | The shared object. | The shared value. |
| `color_mode` | `none`, `value` (the text takes the colour), `background` (the box takes the colour) | `value` |
| `graph_mode` | `none`, `area` (a small area chart of the field over time under the value, when a time field exists) | `area` |
| `text_mode` | `auto`, `value`, `value_and_name`, `name`, `none` | `auto` |
| `orientation` | `auto`, `horizontal`, `vertical` | `auto` |
| `justify` | `auto`, `center` (optional) | `auto` |
| `percent_change` | `true`, `false` (optional) | `false` |
| `percent_change_color` | `standard`, `inverted`, `same_as_value` (optional) | `standard` |
| `title_size`, `value_size` | 1 to 200 px (optional). Absent: the page fits the size. | absent |

- `justify = "center"` centres the text of each item. `justify = "auto"` centres the text of an item that shows no name. It aligns to the left the text of an item that shows a name.
- Grafana 12.4.2 has this rule: the text is centred when the name text is empty. An item shows no name with `text_mode` equal to `value`, or `auto` with one item.
- `text_mode = "auto"` shows the value, and also the name when there are two or more stats. `text_mode = "name"` shows the name in the large slot of the value.
- `orientation = "horizontal"` stacks the tiles one below the other, as Grafana does. `vertical` puts the tiles side by side. `auto` puts them side by side when the panel is wider than it is high, else one below the other.
- Each item is a tile. The tiles are 3 px apart. A tile is 24 px high at least in a stack, and 56 px wide at least side by side. The panel scrolls before a tile gets smaller. The size of the tiles follows the rule of 9.19a.
- The value has the font of the page, weight 500 and a line height of 1.2. A tile has a padding of 12 px, or 8 px when its height is 100 px or less. The unit is 0.9 of the value size below 20 px, 0.8 below 30 px and 0.6 from 30 px up.
- A tile is wide when its width is more than 2.5 times its height. A wide tile shows the name at the left and the value at the right. Another tile shows the name above the value. In a wide tile the name is 0.7 of the value size.
- The page measures the text with its unit at 100 px, and scales it. The text uses 0.985 of the room in the width, and all the room in the height. The size is 6 px at least. The value has no upper limit, so a stat in the view of one panel is very large, as in Grafana.
- In a tile that is not wide, the name takes 0.15 of the room in the height at most. Its size is 30 px at most. The grid of tiles reaches the four edges of the panel content. Thus a background colour fills the whole panel. When a warning line shows above the panel (9.14), the grid starts under the line. It still reaches the three other edges.
- A stat with no value, and a stat with zero rows, show `no_value`, else "No data". The text has the size and the weight of a value (9.14).
- With `color_mode = "value"`, a colour with a contrast below 4.5:1 with the panel moves toward the text colour, up to 4.5:1. A value text of 24 px or more is a large text: its least contrast is 3:1.
- A named colour uses the table "Light fills" in the light theme (9.16).
- The page draws with the copied `app_ui_stat.js` and the line of `app_ui_chart.js` (9.24).

The percent change of a `stat` (`percent_change`) has the behaviour of Grafana 12.4.2. The rules below come from the bundle and from a measure in both themes (FE-82, B222):

- The change is (last - first) / first x 100. The first value and the last value are the results of `first_not_null` and `last_not_null` for the field. Thus it is the result of `diff_percent`. A field with no such value gives 0.
- The calculation of the item does not change the change. Only an item with a calculation (`reduce.values = false`) shows a change, as in Grafana.
- The text has 3 significant digits, the group separator and the minus sign of a negative value. Examples are `20%`, `-16.7%`, `0.00333%` and `1,230,000%`.
- A first value of 0 gives `∞%` or `-∞%`. A negative first value and no change give `-0%`. A first value and a last value of 0 give no number: the item shows no change.
- The page writes the number in the format of `en-US`. Grafana uses the language of the browser, so a browser in another language can show other separators.
- An arrow up comes before a rise, and an arrow down before a fall. A change of 0 has no arrow. The arrow is a line icon of the page, cut to the box of its strokes.
- With `standard`, a rise is green, and a fall or a change of 0 is red. `inverted` gives red to a rise and green to a fall, and a change of 0 stays red. `same_as_value` gives the colour of the value text.
- The green and the red are the named colours `green` and `red` of the theme (9.16). They follow the contrast rule of the value text above. Thus a change under 24 px in the light theme can show a darker green.
- In a tile with a fill (`color_mode = "background"`), the change has the ink of the tile in each mode, as in Grafana.
- The size of the text is 1/2.5 of the size of the value, and 12 px at least. The arrow is 1/3 of the size of the value, and 10 px at least. The gap between them is 1/3 of the text size, and 4 px at least.
- In a stacked tile, the change is under the value. Its top margin is minus 1/4 of its size when its size is more than 12 px.
- In a wide tile, the change is the last item of the row. It stands on the base line of the value, with a line height of 3. A wide tile with a name thus shows the name at the left, the value in the middle and the change at the right.
- In a tile with no graph, the value gets 0.75 of the height when the item has a change. This holds also when the change shows no number. A tile with a graph keeps the size of its value.
- With `text_mode = "none"`, the change fills 0.8 of the width and of the height of the tile, and its arrow is 0.8 of its size.

The options `title_size` and `value_size` (same item) set the font sizes of the name and of the value, as `text.titleSize` and `text.valueSize` of Grafana:

- The page does not fit a fixed size. A large size can thus go out of the tile, and the tile cuts it.
- A fixed name takes no height from the value in a stacked tile. In a wide tile, the value next to a fixed name can use the full width. Grafana 12.4.2 has the same two rules.
- With a fixed value size, a name that is not fixed is 0.7 of the value size in a wide tile. It is 30 px at most. The change follows the fixed value size.

`color_mode = "background"` fills the tile with the gradient of Grafana, `linear-gradient(120deg, A, B)`, for a colour `C` that is a plain colour. A theme token or a colour with an alpha channel gives a flat fill. In the dark theme, `A` is `C` with HSL lightness 15 points lower and hue +8 degrees. `B` is `C` with lightness 5 points lower and hue -8 degrees. In the light theme, the lightness goes up by 10.5 and 3.5 points instead. The ink of the text is chosen in two steps:

1. The ink of Grafana. The ink is the dark ink `#202226` when the brightness of `C` is above 180, else the light ink `#F7F8FA`. The brightness is `(299 R + 587 G + 114 B) / 1000`.
2. The step for contrast (9.26). The ink of Grafana stays when its contrast is 3:1 or more at the worst end of the gradient. The worst end is the lighter end for the light ink, and the darker end for the dark ink. Else the page takes the ink with the better contrast. Thus a green tile has dark digits, where Grafana has white digits at 2.4:1.

`barchart` (4.30):

| Option | Values | New |
|---|---|---|
| `orientation` | `vertical`, `horizontal`, `auto` | `auto` |
| `x_field` | `null`, or a display name. `null`: the first field of the kind `text` or `time`, in column order. | `null` |
| `stacking` | `none`, `normal`, `percent` | `none` |
| `show_values` | `auto`, `always`, `never` | `auto` |
| `bar_width` | 10 to 100: the percent of its slot that a bar takes | 97 |
| `group_width` | 10 to 100: the percent of its slot that a group takes | 70 |
| `x_tick_rotation` | -90 to 90 degrees | 0 |
| `legend`, `tooltip` | The shared objects. | The shared values. |
| `thresholds_style` | As for `timeseries`: lines and bands on the value axis (optional) | `off` |
| `fill_opacity` | 0 to 100: the opacity of the fill of a bar, in percent (optional) | 80 |
| `line_width` | 0 to 10: the width of the line around a bar, in CSS pixels (optional) | 1 |
| `gradient_mode` | `none`, `opacity`, `hue`, `scheme`: the gradient of the bars, as for `timeseries` (optional) | `none` |

- Each row is a group of bars. Each field of the kind `number` or `number_text` is a series.
- The page draws with the chart engine: `xKind: "category"` for a `text` x field, `"time"` for a `time` x field, and
  `type: "bar"`.
- A vertical bar chart writes the value of a bar above the bar, with a size that comes from the bar width. A stack shows no value (19.20a).
- The value axis has the automatic rule of the time series (steps 1 to 7 and the ticks). The rule uses the drawn ends: the values, or the tops of a stack (9.19, stacks). A side with no end has the soft end 0 (y axes, rule 15). Thus the bars start at 0 when the data does not cross 0. The value labels take no room.
- Grafana 12.4.2 has this rule in the code that it serves. The measure of 2026-11-22 found it on 45 bar charts. For example, bars of 5 to 59 give 0 to 65, also with value labels.
- With `stacking = "percent"`, the axis shows shares of 1 in the unit of the first field. The labels are "0.2" with no unit, and "20%" with the unit `percentunit`. A group below 0 gives shares below 0.
- A single bar, or a stack, takes `bar_width` of its band. A group takes `group_width`. The first and the last bar touch the edges of the plot. The chart has no vertical grid line.
- The fill and the line of the bars. The values are those of Grafana 12.4.2. A probe of the canvas calls measured them on 2026-11-22 (16 bar charts in one temporary dashboard of the folder `hune-parity`).
  - The fill has the colour of the series at the alpha `fill_opacity` / 100. Grafana draws `rgba(255, 0, 0, 0.61)` for 61 and the plain colour for 100. With 0, the page draws no fill, as Grafana.
  - The line has the plain colour of the series and the width `line_width`. With 0, the page draws no line, as Grafana.
  - The line stays inside the box of the bar on its four sides. The box of the path loses half the line width w at each side. Thus the box is x + w/2, y + w/2, the width less w and the height less w. Grafana drew the boxes 22, 352 (width 0), 23, 351 (width 1) and 24, 348 (width 4) for one bar. The page gives the engine `barInset: "box"` (E34, 9.24).
- `axis_grid` (4.18) of the first field that sets it. `false` removes the grid lines of the value axis. The labels of the axis stay. `true` and an absent member draw the grid lines, because a bar chart has one value axis (Grafana: Auto and On). The page gives the engine `gridScale: "none"` (E13).
- `orientation = "auto"` draws horizontal bars when the plot is narrower than it is high, else vertical bars. This is the rule of Grafana 12.4.2 (`width < height`). The size is the size of the box of the chart, with its legend. Equal sizes give vertical bars. A resize that crosses the rule draws the chart again in the other orientation. The rotation of the labels has an effect only while the bars are vertical.
- On a time axis, the bars cover the range. The first bar can be partial.
- `x_tick_rotation` turns the labels of the x axis (edit E25 of 9.24). The value is in degrees. It has an effect only when the bars are vertical: `orientation` is `vertical`, or `auto` in a plot that is not narrower than it is high. The value 0 gives flat labels, as before.
- A positive value turns the text clockwise, and the start of the text is at its tick. A negative value turns the text counter-clockwise, and the end of the text is at its tick. The value 90 gives text that reads downward, and -90 gives text that reads upward.
- Grafana 12.4.2 turns `xTickLabelRotation` in the same direction, so the stored value is the value of Grafana (4.30). The measure of 2026-11-15 found the canvas turn `rotate(+45°)` and the alignment `left` for 45. It found `rotate(-45°)` and `right` for -45.
- The label turns about its anchor, which is 9 px under the plot. The bottom margin of the plot is 9 px plus the height of the tallest turned label. A label of width w px that turns by a degrees has the height w sin(a) + 12 cos(a). The plot loses this height, and the band of 21 px is the least value.
- The page shows a label only when it keeps 6 px from the label before it. The room of a label is its turned width, w cos(a) + 12 sin(a). When the labels lie side by side, the room is 12 px / sin(a) if that value is less.
- A label never leaves the canvas. The page moves the anchor of a label that would leave it. A label that would leave less than 40 px of plot ends with an ellipsis.
- `orientation = "horizontal"` is drawn by code of Hune (`panels/barchart_h.js`), because the engine has no horizontal bars (9.24). It has one row for each category and one bar for each series in a row. It has a value axis at the bottom. The value of each bar shows at its end.
- A horizontal bar has the fill and the line of `fill_opacity` and `line_width`, inside its box (CSS `box-sizing: border-box`). With `axis_grid` = `false`, the value axis has its ticks and no grid line. The category labels are 12 px, right-aligned, in the full text colour. The legend has no mode button. It follows the shared object `legend` as the vertical form does, with the mode, the columns, the sort, `hide_from.legend` and the cap of 35 %. At the right, it has no gap, and under 220 px of plot it goes under the plot.
- A value label has a font of two thirds of the bar thickness, 10 to 28 px, weight 500. The label is 8 px after the end of the bar. With `show_values = "auto"`, a row of less than 12 px shows no value.
- The value axis starts at 0, or at the lowest negative value. It ends at the highest value plus 10 % of room. The tick step is 1, 2, 2.5 or 5 times a power of ten. Two ticks are 60 px apart or more. The mode `percent` has the axis 0 to 100 with the step 20.

`piechart` (4.31):

| Option | Values | New |
|---|---|---|
| `pie_type` | `pie`, `donut` | `pie` |
| `reduce` | The shared object. | The shared value. |
| `labels` | 0 to 3 items of `name`, `value`, `percent`, shown on the slices | `[]` |
| `legend` | The shared object, with `values` from `value` and `percent` | `{show: true, placement: "right", mode: "list", values: []}` |
| `tooltip` | The shared object. | The shared value. |

- With `reduce.values = false`, each field is one slice with its reduced value. With `reduce.values = true`, each row
  is one slice, named by the first `text` field.
- The slices go from the largest value to the smallest, and a tie keeps the data order. The first slice starts at 12 o'clock, and the slices go clockwise. A slice keeps the colour of its place in the data. The colours come from the colour mode: a slice with no explicit colour has the slot of its place in the series palette (9.16).
  code). A named colour uses the table "Light fills" in the light theme (9.16).
- The percent of a slice has the decimals of the field, or 0 decimals when the field sets none.
- A slice has a colour with a contrast of 3:1 or more with the panel. A colour that fails moves toward black on a light panel, or toward white on a dark panel. It moves in steps of 5 % up to the first step that passes.
- A label shows only when its whole box is inside its slice. The box is 4 px or more from the straight edges. The page tries all the parts of the label, then fewer parts. It tries one line, then one line for each part, at 12, 11 and 10 px. A label that fits nowhere does not show.
- The page sizes the circle from the real box of the canvas, with the fraction of a pixel (9.19a). The centre is the middle of the box. The radius is half of the shorter side less 8 px, and 10 px at the least. A `donut` has a hole with 0.55 of the radius at the same centre.
- The member `legend` is necessary (4.26). The page has no default value for it.
- The text of a label is `#000000` when the slice colour has a WCAG luminance of 0.179 or more. Else it is `#ffffff` (`FE-07`).

`xychart` (4.32):

| Option | Values | New |
|---|---|---|
| `x_field` | `null` or a display name. `null`: the first `number` or `number_text` field. | `null` |
| `y_fields` | 0 to 32 display names. Empty: each other `number` or `number_text` field. | `[]` |
| `draw` | `points`, `lines`, `points_and_lines` | `points` |
| `point_size` | 1 to 40 CSS pixels | 5 |
| `line_width` | 0 to 10 CSS pixels | 1 |
| `legend`, `tooltip` | The shared objects. | The shared values. |

- The page sorts the rows by the x value, up, with a stable sort. It draws with the chart engine (`xKind: "number"`). Thus `draw = "points_and_lines"` draws its line in the order of x.
- The x domain has 3 % of room at each end, or 6 % when `point_size` is above 12. Thus a point on the edge is a whole circle.
- The titles of the axes are the names of the fields. The ranges are automatic, and they do not force 0.

`bargauge` (4.33):

| Option | Values | New |
|---|---|---|
| `reduce` | The shared object. | The shared value. |
| `orientation` | `horizontal`, `vertical` | `horizontal` |
| `display_mode` | `basic` (one colour), `gradient` (the colours of the thresholds up to the value), `lcd` (cells) | `gradient` |
| `show_unfilled` | boolean; `true` shows the empty part of the bar | `true` |

- The scale of each bar is from `min` to `max` of the field configuration (9.14). `bargauge.js` draws the bars in the
  DOM (Hune code). A named colour uses the table "Light fills" in the light theme (9.16).
- With `display_mode = "gradient"`, each colour of the thresholds holds over its step. A soft edge ends at the next step. The edge is 6 % of the scale, and at most half of the step. A step that the bar does not reach adds nothing: the bar has no sliver of the next colour.
- A horizontal bar has its name above the bar when its item is 40 px high or more. Else the name is at the left of the bar. The box of a name above the bar has the height `min(0.45 × item height, 50 px)`.
- The value text has the colour of the item and weight 400. Its box is at most 0.2 of the item width wide, and 150 px at most.
- With `reduce.values = true`, an item has the name of its row (the first `text` field). When the data has several numeric fields, the name is `<row> <field>`.
- A vertical bar gauge shows the value on top and the name below. When `show_unfilled` is `false`, the value sits on top of the filled part.
- With `display_mode = "lcd"`, the bar is a row of cells. A cell is 10 px long, and two cells are 2 px apart. The cells start at the foot of a vertical bar and at the left of a horizontal bar. A track holds `max(1, floor((L + 2) / 12))` cells, where `L` is its length in px. An unlit cell has the colour of the step at 18 % opacity.
- In the mode `lcd`, `show_unfilled = true` draws the track of the other modes behind the cells: the colour `--bar-off` with a radius of 4 px. With `false`, the track has no colour. The unlit cells show in both cases.
- Two items are 10 px apart in a horizontal bar gauge, and 2 px apart in a vertical one. An item is 10 px high at least in a horizontal gauge, and 16 px wide at least in a vertical one. The name of a vertical item is a line of 22 px.
- The value of a vertical item has the size `max(6, min(0.1 × H, 0.975 × W × 100 / w100))`. `H` is the height of the gauge. `W` is the width of the item. `w100` is the width of the text at 100 px.

#### 9.19a No scroll bar of less than one pixel

A panel never shows a scroll bar for a size error. This rule applies to the stat, the bar gauge, the gauge, the pie chart, the bar chart, the time series and the XY chart. Only a real excess of content scrolls. Examples are tiles at their least size (9.19), the rows of a table, a long text, an error text and a legend in a table.

- The page sizes the parts of a panel from the real box of the panel, with the fraction of a pixel. It does not use the rounded `clientWidth` or `clientHeight`.
- The page subtracts the borders, the padding and the scroll bars of the box. It rounds the result down to the layout unit of the browser (1/64 px).
- The tiles of a stat and the items of a bar gauge share this width or height. Each share rounds down to the layout unit. Thus the items and their gaps never take more room than the box.
- This holds at each browser zoom. The browser case B55 checks the zoom levels 75, 90, 110 and 125 %.

### 9.19b Text panel (type `text`)

A panel of the type `text` shows Markdown text (4.33a). The parser is the pure module `markdown.js` (9.2). It reads the lines of `options.content` and gives a tree. The module `panels/text.js` builds DOM nodes from the tree. The parser is code of Hune with no new dependency.

The security rules. Each rule is a MUST.

1. No raw HTML. The character `<` is text. The page has no HTML parser for this text. The code of `markdown.js` and `panels/text.js` never uses `innerHTML`, `outerHTML`, `insertAdjacentHTML`, `DOMParser`, `createContextualFragment`, `document.write` or a `template` element.
2. A closed list of elements. The page makes only these elements inside the root `div`: `p`, `h1` to `h6`, `em`, `strong`, `code`, `pre`, `blockquote`, `ul`, `ol`, `li`, `hr`, `br`, `a`, `table`, `thead`, `tbody`, `tr`, `th` and `td`. That is 24 names. The name comes from a constant table with the kind of the node as key. No name comes from the text.
3. A closed list of attributes. The element `a` has `href` and `rel`. The root, `th` and `td` can have `class`, with a constant value. No other element has an attribute. No element has `style`, `src`, `srcdoc`, `id`, `name`, `title`, `target` or an attribute that starts with `on`.
4. No resource. The text never makes `img`, `picture`, `video`, `audio`, `source`, `link`, `iframe`, `object`, `embed`, `svg`, `math`, `script`, `style`, `form`, `input`, `base` or `meta`. The policy of 9.4 does not change.
5. The page writes each text with `createTextNode`. It sets each attribute with `setAttribute`, from a constant or from a checked destination.
6. A link follows the rules of "Links" below. No other URL goes in the page.
7. A value of a variable is text. "Variables" below gives the place and the form of the replacement.
8. The parser is linear and has limits. A text that breaks a limit shows a message and no text. The page never cuts a text to make it fit.
9. A static test reads the two files (16.10a). It fails on each token of rule 1, on each element name outside rule 2 and on each attribute name outside rule 3.

The constructs. The list is closed. The first table gives the blocks, and the second table gives the inline constructs.

| Construct | Source form | Result |
|---|---|---|
| Paragraph | One or more lines, up to a blank line or the start of another block | `p` |
| Heading | 0 to 3 spaces, 1 to 6 `#`, a space or a tab, the text. A closing run of `#` after a space is removed. | `h1` to `h6` |
| Block quote | Lines that start with `>`, after 0 to 3 spaces. One space after `>` is removed. The quote ends at the first line with no `>`. | `blockquote`, with blocks inside |
| Unordered list | Items that start with `-`, `*` or `+`, then a space | `ul` with `li` |
| Ordered list | Items that start with 1 to 9 digits, then `.` or `)`, then a space | `ol` with `li`. The number is not read: the list counts from 1, as Grafana does. |
| Horizontal rule | 3 or more of `-`, `*` or `_` on a line, with spaces between them | `hr` |
| Code block | A line of 3 or more backticks or tildes, the lines, and a line with a run of the same character that is as long or longer. The info string is not read. An open fence ends at the end of the container. | `pre` with `code` |
| Table | A header row, a delimiter row (`---`, `:--`, `--:`, `:-:`) with the same count of cells, then rows up to a blank line or a line with no `|` | `table` with `thead`, `tbody`, `th` and `td`. The alignment is the class `md-c` or `md-r`. |

| Construct | Source form | Result |
|---|---|---|
| Emphasis | `*text*` or `_text_` | `em` |
| Strong | `**text**` or `__text__` | `strong` |
| Inline code | A run of backticks, the text, and a run of the same length | `code` |
| Link | `[text](destination)` (rules below) | `a` |
| Autolink | A bare URL that starts with `http://` or `https://` (rules below) | `a` |
| Hard break | Two spaces or a backslash before a line end | `br` |
| Soft break | A line end in a paragraph | One space |
| Escape | A backslash and an ASCII punctuation character | The character, as text |

The block rules:

- A list item holds blocks. Its first line starts after the marker and the spaces that follow it. 1 to 4 spaces count. 5 spaces or more count as 1.
- The next lines belong to the item when they have this indent or more. Blank lines belong to it when a line with this indent follows them. The item ends at the first other line.
- A list has one kind. A line that starts an item of the other kind starts a new list.
- An item is tight when its first block is a paragraph and its other blocks, if any, are lists. A tight item shows the text of the paragraph with no `p`. In another item, each paragraph has its `p`.
- A line with an indent of 4 spaces or more is a line of text, never a code block. A tab at the start of a line counts as 4 spaces.
- A paragraph ends before a code fence, a heading, a rule, a block quote, a bullet item and an ordered item that starts with `1`. The item MUST have text.
- A table cell holds inline constructs. `\|` in a cell is a `|` character. A row that has too few cells gets empty cells, and a row that has too many loses the extra cells. A table has `MD_MAX_COLUMNS` columns at most.
- A text has no setext heading: a line of `=` or `-` under a text line is text or a rule.

The inline rules:

- A delimiter run is a series of `*`, or a series of `_`. It has a left edge when the next character is not white space. Then it is not punctuation, or the character before it is white space or punctuation.
- It has a right edge in the same way, with the two sides exchanged. White space is the start or the end of the text, U+0020, U+0009, U+000A and U+00A0.
- A run of `*` can open when it has a left edge, and it can close when it has a right edge.
- A run of `_` can open when it has a left edge. When it also has a right edge, the character before it MUST be punctuation. A run of `_` can close in the mirror case.
- A run that can close matches the nearest open run of the same character. Two characters make `strong` when both runs have two or more characters. Else one character makes `em`. The open runs between the two stay as text. A run that cannot match stays as text.
- A backtick run opens a code span when a run of the same length follows. Else it stays as text. A line end in a code span is a space. One space at each end is removed when both ends have one and the text is not only spaces.
- An escape makes one text leaf with the flag `lit`. A text leaf with this flag is never read for placeholders.
- No entity is read. The text `&amp;` shows as these characters.

Links. The rules are these:

1. The form is `[text](destination)`, with no space before `(`. The destination has 1 to `MD_MAX_URL_BYTES` characters. Each character is a printable ASCII character (U+0021 to U+007E) and is not `(` or `)`. It has no title and no angle brackets. The text of the link has at least one character. A text that breaks this rule stays as text.
2. The destination MUST pass the check of 4.23 and 9.18, with the prefixes as the server reads them. It starts with `/`, `?`, `#`, `http://` or `https://`. The prefixes are case sensitive: the destination `HTTP://a.example` is not a link. It does not start with `//` or `/\`.
3. A link holds no other link. The text of a link holds text, emphasis, strong and code. A `[text]` that holds a link stays as text.
4. An autolink starts at `http://` or `https://`. The character before MUST NOT be a letter or a digit. The place MUST NOT be in the text of a link, or after a `[` that is still open.
   - It ends before the first character that is not printable ASCII, or is `<`.
   - It loses the last characters `?`, `!`, `.`, `,`, `:`, `;`, `*`, `_`, `~`, `'` and `"`, and each `)` that has no `(`.
   - The result has more than the scheme, and it passes the check of rule 2.
5. The element is `<a href="<destination>" rel="noopener noreferrer">`. It has no `target`: the link opens in the same tab, as in Grafana. It has the underline and the colour `--link` (below).
6. The page never changes a destination. It replaces no placeholder, it adds no encoding and it normalises nothing.
7. A link that the page refuses is not a link. Its characters show as text. The page writes no warning.

| Source | Result |
|---|---|
| `[docs](https://wiki.example/a?b=1&c=2)` | A link. The `&` stays in the value of `href`. |
| `[x](/d/other-uid)`, `[x](#top)`, `[x](?x=1)` | A link. |
| `[x](HTTP://A.example)`, `[x](Https://a.example)` | Text. The scheme is case sensitive. |
| `[x](javascript:alert(1))`, `[x](JaVaScRiPt:x)`, `[x]( javascript:x)` | Text. |
| `[x](data:text/html,x)`, `[x](mailto:a@b.c)`, `[x](ftp://h/f)`, `[x](vbscript:x)` | Text. |
| `[x](//evil.example)`, `[x](/\evil.example)`, `[x](../up)` | Text. |
| `[x](http://a b)`, `[x](http://a/(b))`, `[x](http://a/é)` | Not a link. The characters show as text, and the bare URL in them can be an autolink (rule 4). |
| `![alt](http://a/i.png)` | The character `!` and a link with the text `alt`. No image shows. |
| `see https://a.example/x, or https://b.example/y).` | Two links. The comma and the last `).` stay out of them. |

Variables. The page parses the lines first. Then it replaces the placeholders in the text leaves. A value is never parsed, so it never makes a construct. The rules are these:

1. The place. The page replaces placeholders in a text leaf, in the text of an inline code span and in the lines of a code block. It does not replace them in a leaf with the flag `lit`, in a destination of a link or in another attribute. The text of an autolink is a leaf with the flag `lit`, so it stays equal to its destination.
2. The scan. The scan is the grammar of 7.6, as in 9.13: `$name` and `${name}` and `${name:format}`. The longest run of letters, digits and `_` is the name. A placeholder never goes over the end of a leaf.
3. The forms:

| Form | Text that replaces it |
|---|---|
| `$name`, `${name}` | The values of the selection, joined with `, `. "All" with an `all_value`: the `all_value`. "All" without `all_value`: the values of all the options, joined with `, `. A `text` variable: its value. |
| `${name:text}` | The option texts, joined with ` + `. "All" gives `All`. This is the form of 9.13. |
| `${name:csv}`, `${name:pipe}`, `${name:glob}` | As in 9.13. |

4. Some placeholders stay as they are written. They are a name that is not a variable of the document and a name that starts with `__`. They are also a format that the table does not list and the form `[[name]]`. The same rule holds in a title (9.13).
5. The difference to a title. A title shows the option text with `$name`. The content shows the value. The two measures are in 19.20c.
6. The escape. `\$name` shows `$name`. The escape of a `$` makes a leaf with the flag `lit` (inline rules).
7. The result is text. The page writes it with `createTextNode`. A value such as `<b>x</b>`, `*x*` or `[x](javascript:alert(1))` shows as these characters. A test builds the tree with a hostile value and with a harmless value. The two trees have the same shape: the same kinds and the same destinations.
8. The page keeps the tree. A change of a variable that the content names makes the DOM again from the same tree. The parser does not run again.
9. The size. The leaves after the replacement have `MD_MAX_EXPANDED_BYTES` bytes at most, in all. Above this limit the panel shows a message (below).

Limits and the state `text-limit`. The page checks the size before the parse. It checks the depths, the nodes and the columns during the parse, and it stops at the first limit that it finds. It checks `MD_MAX_EXPANDED_BYTES` when it builds the DOM.

| Constant | Value | The message of the panel |
|---|---|---|
| `MAX_TEXT_LINES`, `MAX_TEXT_BYTES` (4.35) | 2,000 and 65,536 | `This text is too large: 2,000 lines and 65,536 bytes at most.` |
| `MD_MAX_DEPTH` | 6 | `This text has lists and quotes more than 6 levels deep. Reduce the depth.` |
| `MD_MAX_INLINE_DEPTH` | 6 | `This text has emphasis and links more than 6 levels deep. Reduce the depth.` |
| `MD_MAX_NODES` | 4,000 | `This text has more than 4,000 elements. Shorten the text.` |
| `MD_MAX_COLUMNS` | 20 | `A table of this text has more than 20 columns.` |
| `MD_MAX_URL_BYTES` | 2,048 | None: a longer destination is not a link (Links, rule 1). |
| `MD_MAX_EXPANDED_BYTES` | 262,144 | `The text with the values of the variables has more than 262,144 bytes. Select fewer values.` |

- A node is a block, an inline element or a text leaf. The depth counts the blocks that hold blocks (quotes, lists and list items). The inline depth counts em, strong and link inside each other.
- A text that breaks a limit gives the state `text-limit` (9.11). The body shows the message. It shows no part of the text. The message is text, never HTML.
- The parser is linear. For a text of n characters, it makes 6 times n steps at most, line ends included.
  - It finds the end of a code span with one table that it builds before the scan.
  - It does not scan the open runs when no open run of the same character exists. Each open run leaves the stack one time.
  - A link destination ends at the first `(`, `)` or character that is not printable ASCII. Thus no character is in the scan of two links.
- A step is one visit of a line by a block pass, or of a character by an inline pass. The parser gives the count of its steps in `stats.steps`. The property tests check the bound with it (16.10a). A test of time in the browser checks that no task is longer than 200 ms (B105).
- A depth limit stops the parse at once. A line of `>` characters costs 7 passes over the line at most, and the panel shows the message.

Look. The values come from a measure of Grafana 12.4.2 (19.20c). The text panel of Grafana and the text panel of Hune have the same sizes. The colours are the tokens of Hune (9.25). The rules of 9.26 for the contrast apply.

| Part | Value |
|---|---|
| Body | The padding of the frame, 8 px (9.11). The root has the font `--font-sans`, 14 px, line height 22 px, weight 400, colour `--text` and `--letter-spacing-body`. |
| Scroll box | The root is `div.md` with `overflow: auto`, `tabindex="0"`, `role="region"` and `aria-label` equal to the title, or `Text`. It fills the body. |
| `h1` | 28 px, line height 32 px, weight 400 |
| `h2` | 24 px, line height 28 px, weight 400 |
| `h3` | 22 px, line height 24 px, weight 400 |
| `h4` | 18 px, line height 22 px, weight 400 |
| `h5` | 16 px, line height 22 px, weight 400 |
| `h6` | 14 px, line height 22 px, weight 500 |
| Headings | Top margin 0. Bottom margin 0.45 em: 12.6, 10.8, 9.9, 8.1, 7.2 and 6.3 px. |
| `p`, `ul`, `ol`, `blockquote`, `pre`, `table` | Bottom margin 16 px. Top margin 0. |
| `strong` | Weight 500. `em`: italic. |
| `ul`, `ol` | Left padding 24 px. A nested `ul` has the marker circle, and the next level has square. An `ol` has decimal markers. A nested list has no margin. |
| `blockquote` | Left padding 16 px. Left border 5 px in the colour `--muted`. |
| `hr` | Margin 16 px above and below. Border 1 px in the colour `--grid-line`. |
| `code` | Font `--font-mono`, 12 px, padding 2 px 5 px, margin 0 2 px, border 1 px `--grid-line`, radius 6 px, fill `--panel`. |
| `pre` | Font `--font-mono`, 12 px, line height 18.86 px, padding 10 px, border 1 px `--grid-line`, radius 6 px, fill `--panel`, `white-space: pre-wrap`, `overflow: auto`. A `code` in a `pre` has no border and no padding. |
| `table` | Border-collapse. Each cell has a border of 1 px `--grid-line` and padding 4 px 8 px. `th` has weight 500 and the fill `--surface-raised`. A cell with the class `md-c` has `text-align: center`, and `md-r` has `text-align: right`. |
| `a` | The colour `--link` (4.5:1 on the panel in both themes) and an underline. Grafana has no underline, and a link that has the colour only fails WCAG 1.4.1 (9.26). |
| Long words | `overflow-wrap: anywhere` for text. The scroll box shows a horizontal scroll bar only for a wide `table` or `pre`. |
| Empty content | An empty body. The frame is `ready`. Grafana shows an error for a text of spaces (19.20c). |

The CSS is in `panel-text.css`. It has no `style` attribute and no value that is not a token, except the sizes above. A test reads the computed values of both themes (B102).

The frame. The panel follows 9.11 with these changes:

- It has no query and no scheduler state (9.8).
- The frame imports the module `panels/text.js`. The state is `ready` after the first draw, or `text-limit`.
- The frame has the final size from the first frame. The `data-type` is `text`.
- A narrow screen has the floor of 160 px (9.8). The view of the panel (`viewPanel`) shows the text at full size. Print shows the text.

### 9.19c State timeline and status history (types `state_timeline` and `status_history`)

The two types draw the state of each value field of a frame over time (4.33b). The pure module `timeline_model.js` (9.2) makes the rows, the states, the segments, the cells and the caps from the fields. It has no DOM access. The module `panels/timeline.js` draws them on a canvas and builds the legend, the tooltip and the text alternative.

The code is code of Hune. The copied chart engine and its lock do not change. The page reuses `chartCore.timeTicks` and `chartCore.layoutXLabels` for the time axis. The engine exports them for charts that draw their own marks (9.24).

The figures are the figures that Grafana 12.4.2 draws, measured on the local instance (19.20d). A length is in CSS pixels. The page draws in device pixels, and it rounds each edge to a device pixel as Grafana does. The look has the same tokens as the other charts (9.25).

**1. The data.**

1. The x axis is the first field of the kind `time`. A frame with no such field gives the error `The data has no time column.` The panel is then in the state `error` (9.11), as a time series is.
2. A row is each other field of the kind `number`, `number_text`, `text` or `bool`. A field of the kind `null` and a second `time` field are not rows. A row with `hide_from.viz` equal to `true` is not drawn (9.19). A frame with no row gives the error `The data has no field to draw.`
3. The page sorts the sample indexes by time, up, with a stable sort. A sample with a `null` time is dropped. Two samples can have the same time. The later one in the input then wins, because the earlier one gets a segment of width 0 and the page does not draw it.
4. The query format `table` and the format `time_series` both work. With `time_series`, the long-to-wide step (9.22) gives a `null` value in a row at each time that has no sample for it. Such a `null` ends the segment (3.3), so the person sets `span_nulls` to `true` or uses the format `table`. The row name is the display name of the field (9.13).
5. A frame with no row of data shows "No data" as other panels do. The row labels come from the fields, so the empty frame has none.

**2. The state of a sample.** The state of a value is a text and a colour.

1. The page makes the display of each row with `makeDisplay` (9.14 to 9.17). The state text is the mapped text, else the text of the value with its unit and decimals. The state colour is the colour of the mapping, else the colour of the colour mode of the row (9.16).
2. The first mapping that matches gives the text and the colour (9.17). A mapping that has no text keeps the text of the value. A mapping that has no colour keeps the colour of the colour mode.
3. The colour mode `thresholds` on a row of numbers in a `state_timeline` changes each value that no mapping matches. The state is then the step of the thresholds. The text is `< v` for the base step, and `v+` for each other step. `v` is the value of the step, with the unit and the decimals of the field. For the base step, `v` is the value of the second step.
4. The percentage mode writes the step value as a percent. A row with one step has no such change. A `status_history` has no such change: its cell shows the value, coloured by its step.
5. A value of the kind `bool` has the text `true` or `false`. A value of the kind `number_text` has its exact text. A text value is the state text as it is, with no trim and no parse.
6. A value of `null`, or `NaN`, is "absent" when no mapping matches it. An absent sample has no state. A mapping of the kind `special` with `null`, `nan` or `null_nan` gives it a state. The empty text is a state, and it shows no text.
7. A value of the kind `text` that no mapping matches has the colour of the colour mode, as a number has. The default mode `continuous_gr_yl_rd` gives the first colour (green) to a text value. So a row of text needs mappings, or another colour mode.
8. The state key is the state text, a separator and the CSS colour of the state in both themes. Two samples have the same state when their keys are equal. The page computes the state of each different raw value once.

**3. The segments of a `state_timeline`.**

1. A segment starts at the time of a sample. It ends at the time of a later sample, or at the end of the time range for the last segment. A gap in the time of the data does not end a state: the segment goes on to the next sample.
2. With `merge_values` equal to `true`, the segment ends at the first later sample whose state differs, or at the first absent sample. With `false`, the segment ends at the next sample, absent or not.
3. An absent sample ends the segment, and it starts no segment. It leaves a gap in the row until the next sample that has a state.
4. With `span_nulls` equal to `true`, the page removes the absent samples of the row first. The segment then goes on over them. The option has no effect when no sample is absent.
5. The first segment starts at its sample, and the plot clips it at the left edge when the sample is before the range. The last segment ends at the right edge of the plot. A single sample gives one segment from its time to the right edge.
6. A segment of width 0 is not drawn and is not counted. A segment has a width of 1 device pixel at least, so a very short state is still visible.
7. The grafana option `insertNulls` and a time threshold in `spanNulls` have no equivalent (19.20d).

**4. The cells of a `status_history`.**

1. A cell is one sample of one row. The page does not merge cells. An absent sample has no cell.
2. The step is the time of the second sample less the time of the first sample. All cells use this step, also when the steps are not equal. The frame needs two samples at least, else the panel gives the error `A status history needs two samples at least.`
3. The x range is `[from', to']`. `from'` is `t0 - step / 2` when the start of the range is at or before the first sample, else the start of the range. `to'` is `tn + step / 2` when the end of the range is at or after the last sample, else the end of the range. `t0` and `tn` are the first and the last time of the frame. So the axis covers the data when the data covers the range.
4. A cell is centred on the time of its sample. Its width is `step_px * col_width / 100 - line_width`, with 1 device pixel at least. The text of a cell is always centred.
5. The cap of the points depends on the width. The panel does not draw when the frame has more data rows than half the width of the canvas in CSS pixels, rounded down (5.3).

**5. The caps. The page fails closed.**

1. The constant `MAX_TIMELINE_SEGMENTS` is 10,000. It is the number of segments or cells of all the rows after the merge. The page counts before it draws.
2. A panel above the cap draws nothing. It is in the state `timeline-limit` (9.11). The message for a `state_timeline` is "Too many states to draw: <n> segments. The limit is 10,000. Use a shorter time range or merge equal values." The message for a `status_history` is "Too many points to draw: <n> cells. The limit is 10,000."
3. A `status_history` whose frame has more than `floor(width / 2)` data rows shows the message "Too many points to draw: <n> rows. The most for a width of <w> px is <m>." The width is the width of the canvas in CSS pixels. A new width draws the panel again, and the panel can pass or fail.
4. The page never cuts the data to fit a cap and never merges more than the option says. The message has no data of the panel.
5. The reasons for 10,000. The widest plot is about 4,000 device pixels (a 2,000 px panel at a pixel ratio of 2). Above 2.5 segments for each device pixel in one row, the picture is noise. The prototype makes 10,000 segments in 7 ms and 100,000 in 43 ms. Grafana has no cap: it draws 100,000 states that change at each sample.
6. The cap is a constant of the page, as the limits of 9.19b are. It is not a key of the configuration (13).

**6. The geometry.** The canvas fills the body of the panel, under the legend when the legend is at the bottom. The values are measured on Grafana 12.4.2 at a pixel ratio of 1.

A text on the canvas is 12 CSS px high at each pixel ratio. The canvas has device pixels, so the page draws the text with the font size 12 times the pixel ratio. The page measures a width at 12 px, so the places do not depend on the pixel ratio.

| Part | Rule |
|---|---|
| Plot top, right | 8 px. |
| Plot bottom | The band of the time axis is 21 px high. |
| Plot left | `round(w + 20.5)` px, where `w` is the width of the widest row name at 12 px weight 400. With `show_labels` equal to `false` it is 8 px. |
| Row names | 12 px, weight 400, in the text colour, right-aligned 16 px before the plot, at the middle of the row. A name shows only when the pitch of the rows is 12 px or more. |
| Time axis | The ticks and the labels of the engine (`timeTicks`, `layoutXLabels`) in the zone of the dashboard (9.19). The labels are 12 px, at weight 400, centred on the tick, 9 px under the plot. |
| Vertical grid lines | One line of 1 px at each tick, under the fills, in the text colour at 9 % of its alpha. There is no horizontal line. |
| Rows | With `n` rows, `H` the plot height and `f` = `row_height / 100`. One row has `f` = 1. Row `k` (from 0) starts at `round(k * (f * H / n + (1 - f) * H / (n - 1)))` and is `round(f * H / n)` high. The first bar touches the top and the last bar touches the bottom of the plot. |
| Segment | The left edge is `round(x(from))`. The width is `round(x(to)) - round(x(from))`, 1 device pixel at least. The plot clips it. |
| Cell | The width is `step_px * col_width / 100 - line_width`, and the centre is `x(t)`. The height is the height of the row. |
| Fill | The colour of the state with the alpha `fill_opacity / 100`. A colour that has its own alpha (`#rrggbbaa`, `rgba()`) keeps it. The colour `transparent` stays transparent. |
| Line | With `line_width` above 0, a line of the colour of the state at full opacity. The line is inside the box: its path is inset by half the width. |
| Hover | The box under the pointer gets a white layer with the alpha 0.2. |

Example 1, measured. The canvas is 982 px wide and 242 px high. It has one row named `state` and the time range 00:00 to 05:00. The 30 samples are 10 minutes apart. The states are `OK` for 5 samples, `WARN` for 4, `FAIL` for 5, `WARN` for 1, `OK` for 7 and `DOWN` for 8.

The six segments start at x = 48, 202, 326, 480, 511 and 727. Their widths are 154, 124, 154, 31, 216 and 247. The bars are 213 px high from y = 8.

Example 2, measured. With 3 rows, `row_height` 90 and a plot of 213 px, the bars are 64 px high. They start at y = 8, 83 and 157.

**7. The text in a segment.**

1. `show_values` equal to `never` draws no text. `auto` and `always` draw the state text in the font of the panel at weight 500 and 12 px.
2. The number of characters that fit is `floor(w / CHAR_PX)`, with `CHAR_PX` = 8.5 and `w` the width of the visible part of the segment. With `auto`, the text shows when 2 characters fit or more. With `always`, it shows with 1 character, and a width of less than 8.5 px shows no text.
3. The text is cut to this number of characters. It has no ellipsis, as in Grafana. The cut does not split a pair of surrogates.
4. In a `state_timeline` the text is at `left + line_width + 2` for `left`, at `right - line_width - 2` for `right`, and at the middle of the segment for `center`. `left` uses the visible edge of a segment that the plot clips. A `status_history` always centres the text.
5. The text is vertically centred in the row. The ink is the ink of a table cell with a coloured background (`cellInk` of 9.20). It is computed on the fill composited over the colour of the panel, for the theme of the page. Grafana takes white when the contrast with its theme background is 3:1 or more. The page takes the ink with 4.5:1 (9.26).

**8. The colours.**

1. A state colour in the light theme is the light shade of a named colour (`resolveFill` of 9.16). The page draws the colours again when the theme changes.
2. The mode `palette` gives the slot of the row (the index among the rows drawn, from 0). A continuous mode uses `min` and `max` of the field, else the lowest and the highest value of the row.
3. A text, a bool and an absent value take a colour from the colour mode. It is the first colour of a continuous mode, the base colour of the thresholds, or the colour `fixed`. A bool does not take the colour of 1 or 0 (19.20d).

**9. The legend.** The legend is a list of states. It has no table mode and no values.

1. The legend shows when `legend.show` is `true` and the rows have states to list. It is under the plot (`bottom`) or at the right of the plot (`right`). The rules of the legend of 9.19 apply. They are the item size, the pill of 14 x 4 px and the scroll above 35 % of the height.
2. The colour mode of the first row drawn gives the list. With `thresholds` and 2 steps or more, the items are the steps of the thresholds, in order, with the texts of 2.3. They are the same for each value, also for a value that the data does not have.
3. With a continuous mode, the legend has no item.
4. With another mode, the items are the different state texts of all the rows, in the order of their first appearance. An item has the colour of its first state. A row with `hide_from.legend` adds no item. A state with an empty text has no item.
5. The legend keeps the text colour of the theme (9.19). The panel shrinks by the height of the legend at the bottom, or by its width at the right.
6. A click on an item does nothing. Grafana has the same rule for this panel.

**10. The tooltip and the hover.**

1. `tooltip.mode` equal to `hidden` shows no card. `single` shows the card for the box under the pointer. `multi` shows the card for all the rows.
2. A pointer that is between two bars (the gap that `row_height` leaves) is over no box. The `single` card then hides. The `multi` card shows for the time under the pointer, as it does in the bars.
3. The card is the card of 9.19. It has the surface, the border, the radius of 6 px and the shadow. It is 10 px to the right of the pointer and 10 px below it. It flips at an edge of the plot. The head and the rows are 12 px, and the head has weight 500.
4. The head of a `state_timeline` is the time of the start of the segment, a dash, a line break and the time of its end. The end of the last segment is the end of the range. The head of a `status_history` is the time of the sample. Both use the zone of the dashboard. The pattern is `YYYY-MM-DD HH:mm:ss`, and the milliseconds show when the step of the data is below 1 s.
5. A row of the card has the row name in the muted colour. It has the state text at weight 500. A round dot of 12 px in the colour of the state follows it, 4 px after the text. A `single` card of a `state_timeline` has a second row "Duration" with the duration of the segment.
6. The duration text has the first form that fits. The forms are `Ny Nmo Nw Nd` from a year, `Nmo Nw Nd` from a month, `Nw Nd` from a week and `Nd Nh` from a day. The next forms are `Nh Nm` from an hour, `Nm Ns` from a minute, `Ns Nms` from a second and `N ms` below a second. The text for 0 is `0`.
7. A form leaves out a unit that is 0. A year is 365 days and a month is 30 days. Examples are `50m`, `1h 30m` and `3d 4h`.
8. A `multi` card has one row for each row of the frame, in the order of the rows. A row has the state of its segment under the pointer. A row with no segment there, and a row with `hide_from.tooltip`, has no entry. The head is the start of the segment of the first row that has one.
9. The hover also draws the white layer of the table of section 6 on the box under the pointer. In `multi` mode it draws the layer on the box of each row at the time of the pointer.
10. The pointer of a `status_history` holds the half step on each side of a sample. A pointer in the gap between two narrow cells is over the nearer cell.

**11. The keyboard and the text alternative.**

1. The canvas has `role="img"`, `tabindex="0"` and the `aria-label` "<title>, state timeline" or "<title>, status history". An `aria-describedby` names a visually hidden paragraph (class `visually-hidden`). It has one sentence for each of the first 50 rows, and one sentence for the count of the other rows. A sentence is "<row name>: <n> states, last state <text> since <time>."
2. With the focus on the canvas, `ArrowRight` and `ArrowLeft` go to the next and the previous box of the active row. `ArrowDown` and `ArrowUp` change the active row. `Escape` hides the card. The card has the content of the pointer card, placed at the box.
3. The focus ring is the ring of 9.26. The table of "View data" (9.11) is the full text alternative.
4. The colour is never the only sign of a state. The text of the state is in the tooltip, in the legend and in the description. It is also in the box when the box is wide enough.

**12. The redraw and the time.**

1. The panel draws again when the size of its box changes, when the theme changes, and when the pixel ratio changes. A new frame draws a new picture.
2. The page makes the model (rows, states, segments) and the caps once for each frame. A new width of the box makes the pixel boxes again and the cap of a `status_history`. It does not make the states again.
3. A panel with 10,000 segments builds the model and draws in 100 ms or less on the main thread (16.10, B114).
4. The attribute `data-timeline` of `.panel__plot` holds a JSON text after each draw (9.29). The tests read it.

**13. The new panel.** The values of a new panel (10.6) are:

| Option | `state_timeline` | `status_history` |
|---|---|---|
| `merge_values` | `true` | none |
| `show_values` | `auto` | `auto` |
| `align_values` | `left` | none |
| `row_height` | 90 | 90 |
| `col_width` | none | 90 |
| `line_width` | 0 | 1 |
| `fill_opacity` | 70 | 70 |
| `span_nulls` | `false` | none |
| `show_labels` | `true` | `true` |
| `legend` | `{show: true, placement: "bottom", mode: "list", values: []}` | the same |
| `tooltip` | `{mode: "single", sort: "none"}` | the same |

The new panel has no mapping and no colour in `fields`, so the default modes of 9.14 apply.

### 9.19d Annotations on a time series

The page draws the marks of the annotations (4.5b) on each panel of the type `timeseries`. It does not draw them on other types. The measure is on Grafana 12.4.2 (19.20e).

1. The jobs. The module `runtime.js` keeps one result for each annotation index: `idle`, `queued`, `loading`, `ready` or `error`. 9.8 gives the triggers and the queue. The request is the route of 6.21a. It has the etag, the range of the dashboard and the values of the variables (9.10).
   - A panel with a time override uses its own range for its data, and it draws the marks of the dashboard range (9.8b).
2. One query, all the panels. A dashboard with 4 time series and 1 annotation sends 1 annotation request at a trigger. It never sends one for each panel. This is the rule of Grafana, measured: 1 query for each annotation, whatever the number of panels. The count is 8 requests at most.
3. The answer. `readAnswer` in `annotations.js` checks the shape of 6.21a: `annotations` is an array of at most 1,000 items, and `rows` equals its length. Each item is an array of 5 values with the right types, and a mark has 8 tags at most. A wrong shape is an error of the annotation with the message "The answer of the annotation is not valid.". The page never draws a value that it did not check.
4. The marks of a panel. The page takes the annotations that apply to the panel: `enabled` is not `false`, and `panels` is absent or has the id of the panel. It joins their results. The marks of one annotation have its colour. The page sorts the marks by time, and a tie keeps the order of the annotations.
5. The cap. The sum of the marks of the annotations of one panel MUST be `MAX_ANNOTATIONS_PER_PANEL` (2,000) or less. It counts the marks out of the visible range too. Above the cap the page draws no mark of this panel and shows the note "Annotations: too many (<n>, at most 2,000)". This is the fail-closed rule: no cut and no sample. A prototype draws 2,000 marks in 8 ms and 10,000 marks in 97 ms (headless Chrome, DPR 2), so the cap keeps the draw short.
6. The picture. The marks are on a canvas of their own (class `annot__marks`). The canvas covers the chart, comes above the chart canvases and has `pointer-events: none`. The page draws it after each draw of the chart (the engine option `onDraw`). So a zoom, a resize and a change of theme draw it again.
   - It resolves the colour with 9.16 for the theme. The engine and the lock do not change (9.24).
7. A point. A dashed line of 2 px with the dash `[5, 5]`, from the top to the bottom of the plot, in the colour of the annotation. A triangle of 10 px by 5 px is at the bottom of the plot. Its base is on the line of the bottom of the plot, and its tip points up at the time. The time maps with the scale of the chart. A mark outside the plot has no line and no triangle.
   - The page draws the triangle inside the plot, as the prototype does. The measure of Grafana (19.20e) says "under the plot". This place is an open point: the case B120 compares it with Grafana 12.4.2.
8. A region. A fill of the colour at the opacity 0.1, over the plot, from the start to the end (clipped to the plot). Two dashed lines as for a point, at the start and at the end. A bar of 5 px by the width of the region, in the colour, at the place of the triangles. A bar is at least 1 px wide. A region that starts before the range or ends after it keeps the visible part and draws only the line that is inside the plot.
9. The colour. A token or a colour of 4.19. A hex colour or an `rgb` colour is the same in both themes. The token `transparent` draws nothing and the card works. The page never puts the colour in markup: it sets `strokeStyle` and `fillStyle` of the canvas after a parse with the check of 9.16.
10. The hit zone. The zone is a band of 24 px. It starts 16 px above the bottom of the plot and ends 8 px under it. The mark under the pointer is the nearest mark within 8 px of its line. For a region, a pointer inside the width of the region matches.
    - When two marks match, the nearer wins, and then the earlier time. A hover on a line above the zone does nothing, as in Grafana.
11. The card. It is an element of the plot (`div.annot__card`, `role="tooltip"`) with 3 children: the head, the body and the tags. The page sets each child with `textContent` and never with `innerHTML`. The head is the time as `YYYY-MM-DD HH:mm:ss` in the zone of the dashboard (9.19), or `<start> - <end>` for a region. The body is the title, a line end, and the text.
    - The tags are chips, at most 8. The card uses the tokens `--surface-menu`, `--border`, `--r-md` and `--shadow-menu`. Its width is from 300 px to 420 px.
12. The text of the card. The body has 12 px, the muted colour, `white-space: pre-line`, `overflow-wrap: anywhere` and a clamp of 12 lines. The title and the text are one text with a line end between them. The title has no weight of its own in this version.
    - The text of the source is always plain text: `<b>x</b>` shows the characters. The server refuses such text before (7.37), and the page does the same by construction.
13. The place of the card. It is 12 px to the right of the pointer, and 8 px above the zone. At the right edge it flips to the left of the pointer. At the top edge it stays inside the plot. The card hides when the pointer leaves the zone, on `Escape`, on a scroll, when a draw removes its mark and on new data.
14. The engine tooltip. While the card shows, the page calls `hideTooltip()` of the engine, so one card shows at a time. The cursor line of the engine stays.
15. The keys. The plot has one more element for the keyboard: `div.annot__keys` with `tabindex="0"`, `role="group"` and the name "Annotations, <n>". <n> is the count of the marks in the visible range. The element is after the plot in the Tab order.
    - With the focus, `ArrowRight` and `ArrowLeft` go to the next and the previous mark in the visible range. `Home` and `End` go to the first and the last. `Escape` hides the card. The focus ring has 3:1 contrast.
    - The card shows near the active mark. A hidden element with `role="status"` and `aria-live="polite"` repeats the head and the body. The key `Enter` does nothing.
16. The note. The title bar of a panel with a problem shows a note (hook `panel-<id>-annot-note`). Its text is the text of rule 5, or "Annotations: <k> failed". The tooltip of the note lists the name of each failed annotation and its message.
    - A failed annotation draws no mark, and the other annotations draw. The state of the panel stays `ready`. The next trigger tries again, and the frame has no "Retry" for it.
17. The messages. The text of an error comes from `ApiError` (6.7). The page shows the code and the message, never a cell. An aborted request makes no note.
    - An annotation runs on a ClickHouse, a PostgreSQL or a MongoDB source. The page sends the same request and reads the same answer for the three kinds. The route refuses a kind that a later version adds with `400 bad_request` (6.21a). The page shows this refusal as the note of rule 16, with its message.
18. Where it draws. A `timeseries` panel with `data.reuse` and each copy of a `repeat` draw the marks that apply to their own id. A panel of another type shows nothing and has no note. The `state_timeline` and the `status_history` of Grafana draw annotations, but Hune does not draw them in this version.
19. The viewPanel shows the picture, the card and the keys at full size.
20. Print shows the lines, the regions and the triangles. It hides the card and the keys.
21. The cursor synchronisation (`cursor_sync`) and the drag zoom stay as they are. The canvas of the marks takes no event.
22. Edit mode. The panels of the draft grid draw no mark, because the route reads the saved document. The marks show again after the save.

The test hook of the picture is `data-annotations` on `.panel__plot` (9.29).

### 9.19e Heatmap (type `heatmap`)

A heatmap draws counts in a grid. The columns are time buckets and the rows are value buckets (4.33c). The pure module `heat_model.js` (9.2) makes the grid, the cells, the colour scale and the caps from the fields. It has no DOM access. The module `panels/heatmap.js` gives the cells to the chart engine, and it builds the colour bar, the card and the text alternative.

The copied chart engine draws the cells (option `cells`), the axes, the time ticks and the cursor. It has two marked edits for this type (9.24). E22 is the gap of the cells, and E27 is the top of a plot with no y axis.

The page maps the value axis to a linear plot coordinate `u`, so the engine needs no log scale and no reverse axis. For a linear scale, `u` is the value. For a log scale, it is the logarithm of the value. For an ordinal axis, it is the index of the row.

The figures are the figures that Grafana 12.4.2 draws, measured on the local instance (19.20g). A length is in CSS pixels.

**1. The data.** The page makes the data of the picture from the fields after the transforms (9.21).

1. The x axis is the first field of the kind `time`. A frame with no such field gives the error `The data has no time column.` The panel is then in the state `error` (9.11).
2. A value field is each field of the kind `number` or `number_text`. A field with `hide_from.viz` equal to `true` is not a value field. A value of the kind `number_text` is read with `Number(text)`. A text that is not a finite number is read as `NaN`. A frame with no value field gives the error `The data has no number column.`
3. The shape is `bucketed` when `calculate` is `false`, and `raw` when it is `true`. Both shapes work with the query formats `table` and `time_series`. With `time_series`, the long-to-wide step (9.22) makes the wide frame of the shape `bucketed`. The series name is the name of the row (rule 4 gives its form).
4. The shape `bucketed`. Each value field is one row of cells. The frame has one column for each time. The page sorts the sample indexes by time, up, with a stable sort. It drops a sample with a `null` time. When two samples have one time, the later one wins.
   - The frame needs two times at least. Else the panel gives the error `A heatmap needs two time rows at least.`
   - The time of a sample is the end of its bucket. A cell spans from `t - step` to `t`. The `step` is the second time less the first time, and it applies to all the columns, also when the steps are not equal.
   - The names of a long frame. The long-to-wide step (9.22) gives each row the display name `<value field> <label>`, for example `count 8`. The heatmap drops the prefix `<value field> ` when every row has it. Then `count 8` is `8`. A wide frame with real names keeps them. A frame where one row has no prefix keeps all its names.
   - The rows come in the order of the fields. When the display names of all the rows are numbers, the rows come in the order of these numbers, up, with a stable sort. A name is a number when it is a decimal text (an exponent is permitted), `+Inf` or `-Inf`.
   - Row `k` is the band `k - 0.5` to `k + 0.5` of an ordinal axis. Its label is the display name. The label has no unit.
   - A count that is `null` or `NaN` makes no cell. The count of a cell is the value of the field at its time. It can be negative or 0.
5. The shape `raw`. The values are the values of all the value fields together. The page pairs each value with the time of its row. A value that is `null`, `NaN` or not finite is skipped and is not counted. Section 2 gives the buckets. The unit and the decimals of the first value field give the unit and the decimals of the value axis and of the card.
6. Several queries (4.15a). The page joins the frames on time (9.22a). The fields of all the queries are then the value fields. The rows sort by name with the rule above.
7. The query. A query for the shape `bucketed` returns one column `time` and one column for each bucket. The upper bound is the name, as in `countIf(ms <= 10) AS "10"`. A query with `GROUP BY time, bucket` and the format `time_series` gives the same frame. The shape `raw` is for a query that returns the values, with at most `MAX_BUCKET_VALUES` values (section 3).

**2. The bucket maths of the shape `raw`.** The page makes the grid in one pass. The rules are exact.

1. The range is `from` to `to`, the range of the panel in milliseconds (9.8b).
2. The time size `sx` is `x_buckets.value` milliseconds with the mode `size`. With the mode `count`, `n` is `x_buckets.value`, or 20 when it is `null`. Then `X` is `(to - from) / max(n - 1, 1)`, and `sx` is the largest number of the list `T` that is not above `X`. When no number is, `sx` is 1.
3. The list `T` is, in milliseconds: 1, 2, 4, 5, 10, 20, 25, 40, 50, 100, 200, 250, 400, 500. Then come 1, 2, 4, 5, 10, 15, 20 and 30 seconds, and then the same numbers of minutes. Then come 1, 2, 4, 6, 8, 12 and 18 hours. Then come 1, 2, 3, 4, 5, 6, 7, 10, 15, 30, 45, 60, 90 and 180 days. The last numbers are 1 to 10 years of 365 days.
4. The columns. `c0` is `floor(from / sx)` and `c1` is `floor(to / sx)`. The page makes `nx = c1 - c0 + 1` columns. Column `j` spans `(c0 + j) * sx` to `(c0 + j + 1) * sx`. The columns follow the epoch, not the zone of the dashboard. A value with a time outside the columns is ignored.
   - A range with no column (`nx` below 1) gives the error `The time range is empty.`
5. The value range. `lo` and `hi` are the lowest and the highest value that is not skipped. A frame with no such value gives the error `The data has no value to bucket.`
6. The linear rows. With the mode `size`, `sy` is `y_buckets.value`. With the mode `count`, `n` is `y_buckets.value`, or 10 when it is `null`. When `hi` equals `lo`, `sy` is 1. Else `Y` is `(hi - lo) / max(n - 1, 1)`, and `sy` is the largest number of the list `N` that is not above `Y`. When no number is, `sy` is `1e-16`.
7. The list `N` has the numbers `m * 10^k` for `k` from -16 to 15 and `m` in 1, 1.5, 2, 2.5 up to 9.5. A number from 1 to below 10 is in the list only when it is whole. A number of 10 or more is whole, so all are in the list.
8. The rows. `r0` is `floor(lo / sy)` and `r1` is `floor(hi / sy)`. The page makes `ny = r1 - r0 + 1` rows. Row `j` spans `(r0 + j) * sy` to `(r0 + j + 1) * sy`.
   - The page uses no epsilon: a value on an edge goes to the upper bucket. A size such as 0.1 can put 0.3 in a lower bucket, because 0.3 / 0.1 is 2.9999999999999996. Grafana has the same rule.
9. The log rows. The base `b` is 2 for `log2` and 10 for `log10`. The split `s` is `y_buckets.value`, or 1 when it is `null`. The index of a value is `floor(log_b(v) * s)`, with `Math.log2` or `Math.log10`. Row `j` spans `b^((r0 + j) / s)` to `b^((r0 + j + 1) / s)`.
10. The values of 0 or less. A value of 0 or less cannot go on a log scale. The page skips it: it is in no cell and not in `lo` and `hi`.
   - The panel shows the note `<n> values of 0 or less are not on the log scale.` The note for one value is `1 value of 0 or less is not on the log scale.`
   - The note is a line of the muted colour under the plot (`data-testid="heatmap-<id>-skip"`). "View data" shows it above the table.
   - The page never skips a value with no note. When no value is left, the panel gives the error `The log scale needs values above 0. The data has <n> values of 0 or less.`
11. The count of a cell is the number of values in it. The page keeps the counts in a `Uint32Array` of `nx * ny` items. Index `column * ny + row`.
12. The axis range. It runs from the lower edge of row 0 to the upper edge of the last row, in value units, for both scales. `y_min` and `y_max` replace the ends. A cell outside the range is clipped by the plot. A log scale rounds `y_min` down and `y_max` up to a power of the base (read in the code of Grafana).

**3. The caps. The page fails closed.**

1. `MAX_BUCKET_VALUES` is 500,000. It is the number of values that the page reads for the shape `raw`: the rows times the value fields. A heatmap of the shape `bucketed` does not read values.
2. `MAX_HEAT_CELLS` is 250,000. It is the number of cells of the grid, empty cells included. The shape `raw` counts `nx * ny`, and the shape `bucketed` counts the times multiplied by the rows. The page counts before it makes an array.
3. Above a cap the panel draws nothing. It is in the state `bucket-limit` (9.11). The messages are `Too many values to bucket: <n>. The limit is 500,000. Bucket the values in the SQL.` and `Too many cells: <nx> x <ny>. The limit is 250,000.` The shape `bucketed` writes `<times> x <rows>`. A count of 1e15 or more, and a count that is not a number, is written `more than 1,000,000,000,000,000`.
4. The page never cuts the data to fit a cap, and never changes a bucket size. The message has no data of the panel.
5. The reasons are these:
   - A bucket pass of 1,000,000 values takes 28 ms to 35 ms in the prototype. So the cap on the values protects the memory more than the time.
   - The engine draws 250,000 cells in 0.67 s and 57,963 cells in 0.26 s in a test page.
   - Grafana draws a grid of 100,000 cells in 3.8 s with 93 MB of heap. It hides the 42,214 cells with a count of 0.
   - Grafana shows no data for 200,000 rows or more with this data source.
6. The caps are constants of the page, as the limits of 9.19b are. They are not keys of the configuration (13).

**4. The cells and the levels.**

1. A cell is drawn when its count is a number. With `hide_le` set, the count MUST be above it. With `hide_ge` set, the count MUST be below it. The default `hide_le` of 0 hides the empty cells.
2. The colour range `min` and `max` are `color.min` and `color.max`. A `null` takes the lowest and the highest count of the drawn cells.
3. The level of a count `v` is the index in the palette, from 0 to `steps - 1`. It is 0 when `v` is below `min`, and `steps - 1` when `v` is above `max`. Else it is `min(steps - 1, floor(steps * (v - min) / (max - min)))`. When `max` equals `min`, the divisor is 1. The scale is linear in the count. The page and Grafana give the same colour to each of the 126 cells of example 1.
4. The engine gets five arrays for the drawn cells, in column order. They are `x0`, `x1`, `y0`, `y1` (the edges, in the plot coordinate `u`) and `level` (the index plus 1).
5. A panel with no drawn cell shows "No data".

**5. The colour scale and the legend.**

1. The palette has `steps` colours. In the mode `scheme`, colour `j` is the colour of the scheme at `j / (steps - 1)`. The colour is the uniform cubic B-spline of the stops of the scheme in sRGB, as in 9.16.
   - The page then reverses the list in these cases. The rule is `always`, or `dark` in the dark theme, or `light` in the light theme. It reverses the list again when `color.reverse` is `true`.
2. The schemes and their stops (hex, without `#`):

| `scheme` | Rule | Stops |
|---|---|---|
| `oranges` | `dark` | `fff5eb fee6ce fdd0a2 fdae6b fd8d3c f16913 d94801 a63603 7f2704` |
| `blues` | `dark` | `f7fbff deebf7 c6dbef 9ecae1 6baed6 4292c6 2171b5 08519c 08306b` |
| `greens` | `dark` | `f7fcf5 e5f5e0 c7e9c0 a1d99b 74c476 41ab5d 238b45 006d2c 00441b` |
| `greys` | `dark` | `ffffff f0f0f0 d9d9d9 bdbdbd 969696 737373 525252 252525 000000` |
| `purples` | `dark` | `fcfbfd efedf5 dadaeb bcbddc 9e9ac8 807dba 6a51a3 54278f 3f007d` |
| `reds` | `dark` | `fff5f0 fee0d2 fcbba1 fc9272 fb6a4a ef3b2c cb181d a50f15 67000d` |
| `yl_or_rd` | `dark` | `ffffcc ffeda0 fed976 feb24c fd8d3c fc4e2a e31a1c bd0026 800026` |
| `rd_yl_gn` | `always` | `a50026 d73027 f46d43 fdae61 fee08b ffffbf d9ef8b a6d96a 66bd63 1a9850 006837` |
| `spectral` | `always` | `9e0142 d53e4f f46d43 fdae61 fee08b ffffbf e6f598 abdda4 66c2a5 3288bd 5e4fa2` |
| `viridis` | `light` | `440154 48186a 472d7b 424086 3b528b 33638d 2c728e 26828e 21908c 1f9f88 27ad81 3ebc74 5cc863 82d34d aadc32 d5e21a fde725` |

3. The first nine schemes of the table, up to `spectral`, have the exact stops of Grafana. The scheme `viridis` is the table of 256 colours of Grafana sampled at 17 places. The largest difference from Grafana is 7 in one channel of 255. The test checks the 64 colours of each scheme (16.10a, FE-45).
4. In the dark theme, `oranges` goes from `#7f2704` (the lowest count) to `#fff5eb`. In the light theme it goes from `#fff5eb` to `#7f2704`. So the lowest cells are almost white in the light theme (Grafana does the same, 19.20g).
5. The mode `opacity`. The colour is `color.fill`, as the theme draws a colour of that kind (9.16). Colour `j` has the opacity `j / (steps - 1)` for the scale `linear`, and `(j / (steps - 1)) ^ exponent` for the scale `exponential`. The opacity is rounded to 8 bits, as in a colour `#rrggbbaa`. The cells with the lowest count are then fully transparent. The mode `opacity` has no inversion and no `reverse`.
6. The page makes the palette again when the theme changes.
7. The legend shows when `show_legend` is `true` and the panel has a drawn cell. It is a canvas of 300 by 9 px. It draws the `steps` colours from the left to the right, so the lowest count is at the left.
   - The bar is 25 px from the left edge of the content box and 10 px under the top of the legend. The content box is the box of the picture, 8 px inside the body of the panel. Two labels in 11 px text are under the bar: the first count at the left end and the last count at the right end.
   - The legend takes 46 px under the plot. The plot is 46 px lower. The label colour is the text colour at 61 % of its alpha.
   - The counts are the `min` and the `max` of section 4, written in the short number form of 9.19 (`4.91 K`). A content box under 350 px gets a bar of its width less 50 px, 10 px at least. This is a rule of Hune, not measured.
   - A click on the legend does nothing.

**6. The axes.**

1. The x axis is the time axis of the engine, in the zone of the dashboard (9.19). Its range is `from` to `to`.
2. The value axis is on the left, or it is hidden when `y_axis` is `hidden`. A hidden axis leaves a margin of 8 px at the left. The plot then has no margin at the top (E27, 9.24), as in Grafana. The measure gave a plot at y 0 with 170 px, where a shown axis gives y 8 and 162 px.
3. Linear scale. The ticks are the ticks of the y axis of an xy chart (9.19): 30 px apart at least, at round numbers. The labels use the unit and the decimals of the fields (4.18).
4. Log scale. A tick is at a power of the base. The distance of two ticks is a whole number `m` of powers. `m` is the smallest whole number with `m * pitch` at 30 px or more, where `pitch` is the height in pixels of one power. The first tick is the upper edge of the last row, and the next ticks go down by `m` powers. A label is the value with the unit of the field.
5. Ordinal axis (the shape `bucketed`). There is one label at the middle of each row. When the plot is lower than 60 px, only the first and the last label show. Else the page drops every second label until 3 labels are left or `(plot height - 15) / labels` is 10 or more.
6. `y_reverse` puts the plot coordinate `-u` on the axis, so the small values are at the top.
7. The axis is as wide as its widest label plus 9 px (9.24). A long name of a row makes the plot narrower.
8. An ordinal axis ignores `y_min` and `y_max`: its range is the rows.

**7. The geometry.** The cells follow the engine, with these rules.

| Part | Rule |
|---|---|
| Cell | The edges are the edges of the buckets, as the plot maps them. A cell is smaller than its bucket by `cell_gap` px in width and in height. A cell of 3 px or less has no gap (E22). A cell is 0.5 px wide and high at least. |
| Hover | The cell under the pointer has a white layer with the opacity 0.3. |
| Cursor | The engine draws its cursor lines at the pointer. |
| Plot | The plot margins, the gutters and the grid are those of the engine (9.24). With `y_axis` = `hidden`, the plot has no margin at the top (E27). |

Example 1, measured. The panel is 1000 by 300 px. It shows the raw values of 20,000 rows with the defaults of 4.33c, over 5 hours. The time size is 15 minutes (`X` is 947,368 ms) and there are 21 columns, of which 20 have a cell. The value size is 50, with 10 rows.

Grafana draws 126 cells. Each cell is 46.1 px wide and 15.6 px high, with a gap of 1 px. The columns are 47.1 px apart.

Example 2, measured. The same values with `y_buckets` = `log2` and a split of 1 give 17 rows and 272 cells. A split of 2 gives 34 rows and 494 cells. `log10` with a split of 1 gives 6 rows and 101 cells. The three tests use only the 19,999 values above 0.

**8. The card and the hover.**

1. `tooltip.mode` equal to `hidden` shows no card. `single` shows the card for the cell under the pointer. `multi` shows the card for all the cells of its column. A pointer on a position with no drawn cell shows no card.
2. The card is the card of 9.19. It is 10 px to the right of the pointer and 10 px below it, and it flips at an edge of the plot.
3. The head is the end of the time bucket, in the zone of the dashboard, with the pattern `YYYY-MM-DD HH:mm:ss`.
4. With the shape `raw`, the card of `single` has two rows: `Count` with the count and a dot in the colour of the cell, and `Bucket` with the two edges of the row, as `0.500 ms-1 ms`. The edges use the unit and the decimals of the field. With the shape `bucketed`, the rows are `Value` with the count and a dot, and `Name` with the label of the row.
5. The card of `multi` has one row for each drawn cell of the column, from the lowest row. A row has the label `Bucket <low>-<high>`, or `Name <label>`, and the count with a dot.
6. The count is written in the short number form of 9.19. The card has no histogram of the column and no colour bar in this version.
7. The page highlights the cell with the region of the engine. The region has the class `heat__hover`.

**9. The keyboard and the text alternative.**

1. The canvas has `role="img"`, `tabindex="0"` and the `aria-label` "<title>, heatmap". An `aria-describedby` names a visually hidden paragraph (class `visually-hidden`). It says the number of columns and rows, the highest count with its time and bucket, and the number of drawn cells. It cuts a name at 120 characters and writes `…` after the cut.
2. With the focus on the canvas, `ArrowRight` and `ArrowLeft` move the active position by one column. `ArrowUp` and `ArrowDown` move it by one row. `Home` and `End` go to the first and the last column. `Escape` hides the card. The card shows the content of the pointer card at the active position, also for an empty position, with the count 0.
3. The focus ring is the ring of 9.26. The table of "View data" (9.11) is the full text alternative.
4. The colour is never the only sign of a count. The count is in the card, in the description and in the table.
5. The cells are colour data. The lowest colours of a scheme can have less than 3:1 with the panel, as in Grafana. This is the only graphic exception to the rule of 9.26.

**10. The redraw and the time.**

1. The panel draws again when the size of its box changes, when the theme changes and when the pixel ratio changes. A new frame or a new option makes the grid again.
2. The page makes the grid and the arrays once for each frame and each option. A new width of the box does not make the grid again. A new theme makes the palette again, and it does not make the grid again.
3. A panel of the shape `raw` with 500,000 values and 20,000 cells makes its grid in 50 ms or less. It draws in 300 ms or less on the main thread (B146).
4. The attribute `data-heatmap` of `.panel__plot` holds a JSON text after each draw (9.29). The tests read it.

**11. The new panel.** The values of a new panel (10.6) are:

| Option | Value |
|---|---|
| `calculate` | `false` |
| `x_buckets` | `{mode: "count", value: null}` |
| `y_buckets` | `{mode: "count", value: null, scale: "linear"}` |
| `y_axis` | `left` |
| `y_reverse` | `false` |
| `y_min`, `y_max` | `null` |
| `color` | `{mode: "scheme", scheme: "oranges", fill: "dark-orange", scale: "exponential", exponent: 0.5, steps: 64, reverse: false, min: null, max: null}` |
| `cell_gap` | 1 |
| `hide_le`, `hide_ge` | 0 and `null` |
| `show_legend` | `true` |
| `tooltip` | `{mode: "single"}` |

The new panel has no mapping and no colour in `fields`.

### 9.19f Histogram (type `histogram`)

A histogram draws bars. Each bar is the count of the values in one bucket (4.33d). The pure module `bucket_model.js` (9.2) makes the buckets and the caps. It also has the helpers that `heat_model.js` uses. The module `panels/histogram.js` gives them to the chart engine as bars on a number axis.

The engine draws the axes, the legend, the card and the keyboard cursor. Three marked edits of the engine serve this type. E23 is the overlap of the bars. E24 is the slot of a bar and the end of the choice of bars. E26 keeps the line of a bar inside the bar (9.24).

**1. The data.**

1. A value field is each field of the kind `number` or `number_text`. A field with `hide_from.viz` equal to `true` is not drawn. Fields of other kinds are ignored. A frame with no value field gives the error `The data has no number column.`
2. The shape `bucketed`. A frame that has a value field with the display name `xMin` or `BucketMin` holds buckets. The field `xMax` or `BucketMax` is optional. The other value fields are the counts: each one is a bar series. The query makes the buckets, for example with `intDiv(ms, 10) * 10 AS xMin`.
   - The rows sort by `xMin`, up, with a stable sort. A row with an `xMin` that is `null`, `NaN` or not finite is dropped. A count that is `null` or `NaN` makes no bar.
   - The width `w` is `xMax - xMin` of the first row after the sort. Without `xMax`, `w` is `xMin[1] - xMin[0]`, rounded to 6 decimals, and the frame needs two rows at least. `w` MUST be above 0, or the panel gives the error `The buckets have no width.`
   - Bucket `i` spans `xMin[i]` to `xMin[i] + w`. A frame with no count field gives the error `The data has no number column.`
   - The number of rows is the number of buckets. The cap of section 4 applies.
3. The shape `values`. The values of each field are the values to bucket. A value that is `null`, `NaN` or not finite is skipped and is not counted. The field setting `no_value` has no effect.
4. Several queries (4.15a). The fields of all the queries come one after the other.

**2. The bucket maths of the shape `values`.**

1. `n` is the number of values of all the fields. Above `MAX_BUCKET_VALUES` the panel gives the message of section 4. No value gives the error `The data has no value to bucket.`
2. The size `sz` is `bucket_size` when it is a number. Else the page sorts all the values, up. `H` is the smallest difference that is not 0 between two next values. It is 1 for one value, and it is infinite when all the values are equal. `Y` is `(max - min) / bucket_count`. `sz` is the first number `R` of the list `Z` with `Y < R` and `R >= H`.
3. The list `Z` has the numbers `m * 10^k` for `k` from -9 to 9 and `m` in 1, 2, 2.5, 4 and 5, in order. When all the values are equal, `sz` is 1. When no number fits, the panel gives the error `No bucket size fits the values.`
4. The index of a value `v` is `floor((v - bucket_offset) / sz)`. The low edge of bucket `i` is `i * sz + bucket_offset`, rounded to 9 decimals.
5. `b0` and `b1` are the indexes of the lowest and the highest value. The page makes `nb = b1 - b0 + 1` buckets, empty buckets included. Above `MAX_HIST_BUCKETS` the panel gives the message of section 4.
6. The count of a bucket is the number of values of the field in it. With `combine` equal to `true`, the page makes one series, `Count`, with the sum of the counts of all the fields. One size and one set of edges serve all the fields.
7. An empty bucket has the count 0 and no bar. A value can be negative.
8. The sort is the main cost. A prototype sorts 500,000 values in 100 ms (19.20g).

**3. The picture.**

1. The page gives the engine the type `bar`, the axis `xKind` = `number`, and one `xs` item for each bucket: the middle of the bucket. The width ratio is 1, and a bar has no cap of width. The slot of a bar is the bucket size (`barSlot`, E24), so one bucket fills the plot. A bar is its slot less 1 px (the rule of the engine), and 1 px wide at least. Two bars have a gap of 1 px, where the bars of Grafana touch.
2. The x range is the low edge of the first bucket to the high edge of the last bucket. The ticks are the edges of the buckets. The page keeps every `k`-th edge, with `k = ceil(50 / bucket pitch in px)`. The labels use the unit and the decimals of the first field.
3. The y axis is the number scale of Grafana 12.4.2 (measured on 28 plots of 19 to 481 px). The function `histYAxis` of `panels/series.js` holds the rule.
   - The axis starts at 0. Its end is the upper end of the automatic range of the time series (9.19, steps 1 to 7) for two values. These are the lowest and the highest drawn top of the series that show. Without a stack, they are the lowest count above 0 and the highest count. A series that a click on the legend hides does not count.
   - In a stack, the highest top is the top of the highest stack. The lowest top is the lowest top of the first series that shows, in the buckets where a series has a count. Thus it is 0 when only a later series has a count in a bucket: Grafana draws a bar of no height there.
   - One bucket has one count, so the axis ends at 2 times the count.
   - The ticks are the multiples of 1, 2, 2.5 or 5 times a power of ten. Two ticks are 30 px apart at least. On a plot of 150 px or less, they are 15 px apart at least.
   - With `stacking` = `percent`, the tops are shares of 1, and the highest top is 1. The axis shows the shares, for example 0 to 1.1 with the label "0.2". The engine draws them out of 100. The function `histAxis` of `panels/histogram.js` holds the rules of the stack.
   - The measure of 2026-11-22 found these rules on 24 histograms of Grafana 12.4.2: 8 cases at 3 heights, with plots of 133 to 437 px. Hune draws the same ticks within 0.5 px, and the same bars within 1.5 px.
4. With `stacking` equal to `none`, the bars of the fields overlap at the same place (E23). With `normal` or `percent`, the bars stack in the order of the fields.
5. `fill_opacity` is the opacity of the fill, and `line_width` is the width of the line around a bar. The line has the colour of the series at full opacity. A line of 0 draws no line.
   - The line stays inside the bar at its top and at its bottom, as in Grafana. The page gives the engine `barInset` (E26, 9.24), so the box of the bar loses half the line width at each end. The top of the line is at the count, and the bottom of the line is on the axis. The x and the width of the bar do not change.
6. The colour of a series is the colour of its field (9.16). The default is the series palette, one slot for each field.
7. The legend and the tooltip are those of the engine, with the options of 4.26. The title of the card is `<low> - <high>` of the bucket, in the unit and the decimals of the first field. A row has the series name and the count. A count has no unit. The legend values (4.26) are the counts of the series.
8. A field with `hide_from.legend` or `hide_from.tooltip` follows 9.19.

**4. The caps. The page fails closed.**

1. `MAX_BUCKET_VALUES` is 500,000, as for the heatmap (9.19e). `MAX_HIST_BUCKETS` is 4,000 buckets, empty buckets included. The page counts before it makes an array.
2. Above a cap the panel draws nothing. It is in the state `bucket-limit` (9.11). The note has `data-testid="histogram-<id>-note"`. The messages are `Too many values to bucket: <n>. The limit is 500,000. Bucket the values in the SQL.` and `Too many buckets: <n>. The limit is 4,000.`
3. The page never cuts the data and never changes a bucket size to fit a cap.
4. The reasons. The widest plot is about 4,000 device pixels, and a bar is 1 px wide at least. Grafana draws 3,950 bars of 1 px for 5,000 values with a bucket size of 0.01, and it has no cap. The engine would cut the bars above one third of the plot width, so E24 turns the cut off.
5. The caps are constants of the page. They are not keys of the configuration (13).

**5. The keyboard and the text alternative.**

1. The engine draws the chart with `role="img"` and an `aria-label` of the form "<title>, histogram chart" (9.26). Its keyboard cursor stays: the arrow keys move over the buckets and show the card.
2. A hidden paragraph (class `visually-hidden`, `aria-describedby`) says the number of buckets, the bucket size, and the highest count of each series with its bucket. It cuts a name at 120 characters and writes `…` after the cut.
3. The table of "View data" (9.11) is the frame of the panel. The paragraph above is the short text alternative.

**6. The redraw and the time.**

1. The page makes the buckets once for each frame and each option. A new width of the box does not make them again.
2. A panel with 500,000 values makes the buckets in 150 ms or less on the main thread (FE-46). A query sends 100,000 rows at most by default (13), so 500,000 values need 5 fields. B145b draws such a panel and checks that no task of the main thread is longer than 800 ms.
3. The attribute `data-histogram` of `.panel__plot` holds a JSON text after each draw (9.29).

**7. The new panel.** The values of a new panel (10.6) are:

| Option | Value |
|---|---|
| `bucket_count` | 30 |
| `bucket_size` | `null` |
| `bucket_offset` | 0 |
| `combine` | `false` |
| `stacking` | `none` |
| `line_width` | 1 |
| `fill_opacity` | 80 |
| `legend` | `{show: true, placement: "bottom", mode: "list", values: []}` |
| `tooltip` | `{mode: "single", sort: "none"}` |

The new panel has no mapping and no colour in `fields`, so the default modes of 9.14 apply.

### 9.19g Gauge (type `gauge`)

A gauge draws each value on an arc (4.33e). It is the classic gauge of Grafana 12.4.2: the instance of the parity harness has the flag `newGauge` off. The module `panels/gauge.js` draws each gauge on its own canvas, with no new dependency. The rules below come from the code that Grafana serves and from the measure of 2026-11-22: 58 gauges of 34 panels, in both themes. The test FE-84 holds these measures.

**1. The values.**

1. The items are those of a `stat` (9.19, `reduce`). There is one item for each number field, reduced with `calc`. With `reduce.values` = `true`, there is one item for each row. The panel then shows the first 25 rows. This is the default limit of Grafana. The page reads only these 25 rows.
2. Several queries give the lists of their fields one after the other (9.22a, rule 4).
3. A panel with no row shows one gauge with the text "No data", or the text of `no_value` (9.14). The text has the colour of the base step. The arc is empty.

**2. The scale.**

1. The minimum and the maximum of a field are `min` and `max` of its property set. Without such a member, the scale uses the lowest or the highest finite value of all the number fields. Grafana does the same.
2. A field with the unit `percent` has the scale 0 to 100 when the property set has no `min` or `max`. A field with the unit `percentunit` has the scale 0 to 1.
3. When the threshold markers show, the minimum and the maximum are rounded to `decimals` (2 when `decimals` is absent).
4. With the thresholds in the mode `percentage`, the scale is 0 to 100. The value is its share of the scale of the field, times 100.
5. An empty scale (the maximum equals the minimum, for example one value with no `min` and no `max`) gives no angle. The gauge then draws the empty arc and the text, as Grafana does. It draws no filled arc, no neutral mark and no part of the band, and the band keeps its room. With `show_threshold_labels`, the labels of the minimum and of the maximum still show. With no row, the scale is `min` to `max` of the defaults, else 0 to 0.

**3. The cells.**

1. With `orientation` = `auto`, the gauges make a grid. For each axis, the page finds the largest square cell that `n` cells can have in the box. It takes the larger of the two sides. The number of columns is the box width divided by that side, rounded down. The rows follow, and the last row shares the full width.
2. A cell is `W / c - 8 + 8 / c` wide and `H / r - 8 + 8 / r` high, for `c` columns and `r` rows. Two cells are 8 px apart.
3. With `horizontal`, the cells are one below the other. Each cell is `(H + 8) / n - 8` high, and 75 px high at least. With `vertical`, the cells are side by side. Each cell is `W / n - 8 + 8 / n` wide, and 75 px wide at least.
4. When the cells need more room than the box, the box scrolls on that axis. With `sizing` = `manual`, the least size is `min_height` for `horizontal` and `min_width` for `vertical`.

**4. One gauge in its cell.**

1. A gauge has a title when the panel has two items or more, or when the defaults of the fields have `display_name`. The title is the display name of the item, under the gauge.
2. The title size is `min(0.1 × w, 20)` px, where `w` is the cell width. With `vertical` it is `min(0.1 × h, 20)` px, where `h` is the cell height. `title_size` replaces it in the line of the title only. The line of the title is `title size × 22 / 14` px high. It is centred, it cuts a long name with an ellipsis, and it moves up by 4 px.
3. The gauge is `min(h - 1.5 × title size, w)` px high, or `min(h, w)` px with no title. Here the title size is the automatic size of rule 2, also when `title_size` is set. With `vertical`, the box of the gauge is as wide as it is high, at the left of the cell. Else it is as wide as the cell.
4. When the gauge and the title are higher than the cell, both get smaller in the ratio of their heights. When they are lower, they are in the middle of the cell.

**5. The geometry of the arc.** The size `T` is the smaller of the box width and the height of the gauge. With `vertical`, the size reads the cell width for the title size, as Grafana does.

1. The arc goes from 0.9 π to 2.1 π, clockwise: from the lower left, over the top, to the lower right. It has 216 degrees.
2. The width of the arc is `min(T / 5.5, 40)` px. With `show_threshold_labels` it is that width divided by 1.5. The band of the threshold markers is one fifth of the arc width.
3. The value text fits a width of `0.9 × min(box width, 1.3 × T)` less `2 × (arc width + marker room + 10 with labels) + 10`. The marker room is the band width when `show_threshold_markers` is not `false`, else 0. It stays when the colour mode draws no band. Its size is the smallest of `T`, the width at which the text fits, and `1.7 ×` the arc width. The page measures the text at 14 px in the font Inter. `value_size` replaces the size.
4. A threshold label has the size `max(value size / 2.5, 12)` px.
5. The radius is the smaller of two values. The first is the half width of the box less 5 px, the band and the room of the labels. The second comes from the height of the box less 10 px, divided by `1 + sin(0.1 π)`, less the band and the room of the labels. The value text can need more room under the centre. Then the second value is the height less 10 px, a margin of `max(1, box width / 30)` and half the value size.
6. The arc is in the middle of the box, vertically. When the arc width is not below the radius, the arc width is `max(3, radius / 3)`.

**6. The parts that the page draws, in this order.**

1. The empty arc: the full arc, in the colour of the text at 6 % over the panel, as the track of a bar gauge.
2. The filled arc goes from the start of the arc to the angle of the value. It is 1 px inside the empty arc on each side, with a line of 1 px in its colour. With `neutral`, it goes from the angle of the neutral value to the angle of the value. A value below the minimum or above the maximum stops at the end of the arc.
3. The neutral mark is a short part of the arc at the neutral value: the scale divided by 300. It has a line of 2 px in the colour of the value and the fill of the empty arc.
4. The threshold band, when `show_threshold_markers` is `true`, the colour mode is `thresholds` and the band has two entries or more. The entries are the minimum, each step inside the scale and the maximum. It is outside the arc, with a gap of 2 px. Each part has the colour of its step. The parts start at the minimum and end at the maximum. A step outside the scale has no part.
5. The labels, when `show_threshold_labels` is `true`: the minimum, the maximum, each step value inside the scale and the neutral value inside the scale. The text is the number as JavaScript writes it, with no unit. Each label turns with the arc and stays outside it, in the colour of the text.
6. The value text, in the middle of the arc, with the unit, in the colour of the value.

**7. The colours.**

1. The colour of the value text is the colour of its display (9.16). It is the colour of a mapping, of the step, of the fixed colour or of the continuous mode. A named colour has its light shade in the light theme. The text has 4.5:1 with the panel, or 3:1 from 24 px (9.26).
2. The colour of the filled arc comes from the threshold band: it is the colour of the part that holds the value. A value on a step has the colour of the part below the step in the arc. Its text has the colour of the step, as in Grafana. With another colour mode, the filled arc has the colour of the value and the band does not show.

**8. The keyboard and the text alternative.** The root of the gauges has `role="img"` and the name "<title>, gauge chart" (9.26). A hidden paragraph gives each name and value text. It cuts a name at 120 characters. The table of "View data" (9.11) is the full text alternative.

**9. The redraw.** The page draws the gauges again when the box changes size, when a font loads and when the theme changes. The attribute `data-gauge` of the root holds a JSON text after each draw (9.29).

**10. The new panel.** A new panel (10.6) has the options `{reduce: {calc: "last_not_null", fields: null, values: false}}`. Each optional member is absent. The new panel has no colour in `fields`, so the mode `thresholds` of 9.14 applies.

**11. The known differences from Grafana.**

1. Grafana shows each value in a text element of the page. Hune draws it on the canvas, at the same place and size.
2. The colours of the light theme and of the empty arc come from the theme of Hune (9.16, 9.25). They are not the colours of Grafana.
3. Grafana moves a label by half its height again when it draws the gauge a second time. Hune keeps the place of the first draw.

### 9.19h Logs (type `logs`)

A logs panel shows one line for each row of its frame (4.33f). The module `panels/logs.js` builds a model of the lines first, with no DOM access, and then the DOM. The measure of 2026-11-22 used the new logs panel of Grafana 12.4.2 (the flag `newLogsPanel` is on) and frames of the ClickHouse plugin 4.5.1.

A log line is untrusted text (threat T104). The page writes each time, level, body, field name and field value with `textContent` only. The caps of section 4 bound the lines and the characters that the page writes.

**1. The data.**

1. The rules of 7.16e give the time field, the body field, the level field and the level of each line.
2. A logs panel draws one frame (4.15a, rule 7). The transforms of the panel apply first (9.21).
3. A frame with no row shows "No data", as each panel does (9.11). A frame with no field of the kind `time` or no field of the kind `text` shows an empty state. Its title is "Cannot show log lines". Its text is "The data needs a time column and a text column to show log lines." Grafana shows "Cannot visualize data".

**2. The lines.**

1. The order. With `order` = `newest_first`, the line with the highest time is at the top. With `oldest_first`, the line with the lowest time is at the top. The sort is stable: lines with the same time keep the order of the frame. A line with no time comes last in both orders. Grafana shows "Invalid date" for such a line.
2. The columns. A line has the time (only with `show_time` = `true`), the level and the body, in this order. The gap between two columns is 12 px. It is the same gap with `wrap`.
3. The text. The font is `--font-mono` at 14 px, on a line height of 22 px. Each line has 6 px of space under it, so one line is 28 px high. A line with no text in its cells is also 28 px high. Grafana uses Roboto Mono. Hune has no copy of that font and uses the monospace font of the system.
4. The time. The time is the text `YYYY-MM-DD HH:mm:ss.SSS` in the zone of the dashboard (9.9). The column is 23 characters wide. Its colour is the text colour at 61 % in the dark theme and at 65 % in the light theme.
5. The level. The level column shows the text of the table below, with the weight 500. The column is as wide as the longest level text of the panel, and 1 character wide at least. The colours are the colours of Grafana 12.4.2, in both themes.
6. The body. The body has the text colour. An ANSI colour sequence (ESC, `[`, digits and `;`, then `m`) leaves the text, as in Grafana.
7. Without `wrap`, the line ends (CR, LF) leave the body, and a line has one row of text. A long line makes the list scroll to the side. With `wrap` = `true`, the body keeps its line ends and wraps at any character. The time and the level stay on the first row of the line. The time has one row of 23 characters, and the level column keeps its width.
8. A line under the pointer, and a line with an open details row, have the hover fill of a row.
9. Each text of a line is isolated for the bidirectional order (CSS `unicode-bidi: isolate`). A control character of a body, such as U+202E, changes the order inside the body only.

| Level | Text | Dark theme | Light theme |
|---|---|---|---|
| `critical` | `CRIT` | `#b877d9` | `#b877d9` |
| `error` | `ERROR` | `#ff5286` | `#cf0e5b` |
| `warning` | `WARN` | `#fbad37` | `#fbad37` |
| `info` | `INFO` | `#6ccf8e` | `#6ccf8e` |
| `debug` | `DEBUG` | `#6e9fff` | `#6e9fff` |
| `trace` | `TRACE` | the muted text | the muted text |
| `unknown` | no text | none | none |

**3. The details row.**

1. With `details` = `true`, a click on a line opens its details row under it. A second click closes the row. The keys Enter and Space do the same on the focused line. More than one row can be open.
2. The row has the title "Fields". Then it has one name and one value for each other field of the frame, in the order of the frame. The time field and the body field are not in the row. The level field is one of them. A field whose value is `null` in this row is not shown. A row with no such field shows "The line has no other field."
3. A value is the text of the raw value of the frame. A text stays as it is, and a `number_text` keeps its digits. A number has its JavaScript text, and a boolean is `true` or `false`. A field of the kind `time` shows its epoch milliseconds, as in Grafana.
4. The row uses the text font of the page at 14 px, on lines of 24 px. It has 8 px of space above it, 16 px at its sides and 12 px under it, as in Grafana. The names and the values are a list of terms (`dl`, `dt` and `dd`) in a box with the raised fill and an edge.
5. Grafana also has a tool bar, the section "Log line", buttons that filter by a value, and a sidebar mode. Hune does not have them.

**4. The caps. The page fails closed.**

1. `MAX_LOG_LINES` is 10,000 lines. The page draws the first lines in the order of the option `order`, so it draws the newest lines with `newest_first`.
2. `MAX_LOG_LINE_CHARS` is 10,000 characters (UTF-16 code units). A longer body or field value shows its first 10,000 characters and the sign `…`. The cut never splits a pair of surrogates. The page removes the ANSI sequences of a body (section 2, rule 6) only up to the cut. The text is the same as a removal in the full body, also for a sequence across the cut point.
3. `MAX_LOG_CHARS` is 2,000,000 characters for all the bodies of a panel. The line that passes this cap, and the lines after it, are not drawn.
4. Above a cap, the list starts with a note. The note is "The panel shows the first <n> of <total> lines (the limit of lines)." The cap of characters gives "(the limit of characters)". The numbers use the separator `,`.
5. The reasons. The server sends 100,000 rows of 32 MiB at most by default (8.3). The page draws a DOM element for each line and has no virtual list, so the caps keep the DOM small. A line off the screen skips its layout (CSS `content-visibility: auto`).
6. The caps are constants of the page. They are not keys of the configuration (13).

**5. The keys and the text alternative.**

1. The list has `role="list"`. With `details` = `true`, a line has `role="button"` and `aria-expanded`. The list has one tab stop: the arrow keys up and down, Home and End move the focus from line to line. The arrow keys left and right scroll the list to the side.
2. With `details` = `false`, a line has `role="listitem"`, and the list itself has the tab stop, so that the keys can scroll it.
3. The text of the lines is the text alternative. "View data" (9.11) shows the frame.

**6. The time.**

1. A frame of 10,000 lines gives the model and the DOM in 250 ms or less in the front test (16.10a). The level scan of 2,000,000 characters takes 150 ms or less.
2. The attribute `data-logs` of the list holds a JSON text after each draw (9.29).

**7. The new panel.** A new panel of this type (10.6) has the options `{}`. It shows no time and no wrap, it has the details row, and the newest line is first. The change of the type to `logs` sets the format of the query to `logs` (10.3). A change to another type sets the format `logs` to `table`.

The sections above list the differences from Grafana 12.4.2 that remain. Grafana also has a menu button of 20 px at the start of each line. It also has the deduplication, the labels, the infinite scroll and the highlight of parts of the text (15.6).

### 9.19i Map (type `geomap`)

A map draws one layer of markers (4.33g). The pure module `geo_model.js` (9.2) holds the grammar of the tile URL, Web Mercator and the tiles of a view. It also holds the caps, the positions and the sizes, and it has no DOM access. The module `panels/geomap.js` draws on one canvas of Hune: the tiles or the plain background, then the markers. The page uses no map library.

**1. The tile source.**

1. The page reads the member `geomap` of the session (6.10). With `null`, the map has no tile source.
2. The page checks the template again with the grammar of 13.13 (the function `checkTileUrl`). A template that fails, a zoom outside 0 to 22 and an empty attribution give no tile source. The page then contacts no host.
3. The tile source never comes from the document, from the URL of the page or from a variable.

**2. The data.**

1. The map reads the frame of its one query (4.15a). The latitude field is the field that `lat_field` names. With `null`, it is the first field whose name in lower case is `lat` or `latitude`. A field whose display name is one of these two texts also counts. The longitude field has the names `lon`, `lng` and `longitude`. This is the rule of the mode `auto` of Grafana 12.4.2.
2. A position field of a kind other than `number` and `number_text` gives the error `The latitude field <name> is not a number field.` (or `longitude`). No such field gives `The data has no latitude field: name it lat or latitude, or choose the field in the options.` A name of the options that no field has gives `No field has the name <name>.`
3. A value of a `number_text` field MUST match the number grammar of 7.7. A valid position has a finite latitude from -90 to 90 and a finite longitude from -180 to 180. The page skips each other row. The note `<n> rows have no valid position.` shows at the bottom left, and "View data" shows it too.
4. A frame with no valid position shows "No data".
5. Grafana also reads a geohash, a name in a gazetteer and the field type `geo`. Hune does not.

**3. The projection and the view.**

1. The projection is Web Mercator, with tiles of 256 CSS pixels. A latitude above 85.0511 or below -85.0511 draws on the edge of the map.
2. The zoom is a whole number from 0 to the highest zoom. The highest zoom is `geomap.max_zoom` with a tile source, else 22. A start zoom above the highest zoom starts at the highest zoom.
3. With `mode` = `fixed`, the start view is the centre `lat`, `lon` at `zoom`.
4. With `mode` = `fit` and no tile source, the page fits the box of the valid positions with a padding of 5 percent. The zoom is the highest whole zoom that fits, up to `zoom`. One position gives the zoom `zoom`.
5. With a tile source, the page always uses `lat`, `lon` and `zoom`. A fit would tell the tile host the area of the data.
6. The map does not repeat the world to the left and to the right. Grafana repeats it.
7. A drag pans the map. The mouse wheel zooms by 1 at the pointer when `mouse_wheel_zoom` is `true`. The buttons zoom by 1 at the centre.
8. On the canvas, the arrow keys pan by 64 CSS pixels. The keys `+` and `=` zoom in, `-` zooms out, and Escape closes the card.
9. A new draw of the map keeps the view of the viewer, for example after a refresh or a new result. The page keeps the view of each map in a table, by the uid of the dashboard and the key of the panel.
10. A change of the start view (`view`), of the template or of the highest zoom puts the map back at the start view. With `mode` = `fit` and no tile source, each new draw fits the view again.

**4. The picture.**

1. With no tile source, the map has a plain background in the bounds of the world. It is the fill of the panel, a graticule of 30 degrees in the colour `--grid-line` and the equator in the colour `--border-strong`. The page has no coastline data.
2. With a tile source, the page draws the tiles of the view and no graticule. The place of a tile that has not come yet shows the fill of the panel.
3. A marker is a disc with the radius `size` plus 0.5 CSS pixels, with no line. Its fill has the alpha of the square of `opacity`: 40 gives 0.16.
4. Item 3 is the picture of Grafana 12.4.2. The measure of 2026-11-22 found the alpha 0.16 for the opacity 0.4, 0.64 for 0.8 and 1 for 1, in both themes. A size of 5 gives a disc of 11 px.
5. With `size_field`, the radius goes linearly from `size_min` at the `min` of the field to `size_max` at its `max` (9.14). A value outside the range takes the near end. A field with no range, a range of 0 and a value that is not a number give `size_min`. This is the linear scale of Grafana 12.4.2.
6. The colour is `color`, or the colour of the value of `color_field` by its field settings (9.16).
7. A later row draws on top of an earlier row.
8. The buttons "Zoom in" and "Zoom out" are two squares of 19.25 px, 8 px from the top left corner, as Grafana draws them. A button is off at the end of the zoom range. With `show_zoom` = `false` they do not show.

**5. The card.**

1. With `details`, the card of the marker under the pointer shows in the panel. Its corner is 10 px to the right of the pointer and 10 px under it. With `none`, a click shows the card, and a second click closes it.
2. The card has one row for each field of the frame, with the display name and the display text of the value (9.13, 9.15). A field with `hide_from.tooltip` has no row. The card has 32 rows at most, and the line `<n> more` counts the rest.
3. Grafana adds a header with the name of the layer. Hune has no layer name.
4. Each text goes in with textContent.

**6. The tiles. The page fails closed.**

1. The URL of a tile is the template with `{z}`, `{x}` and `{y}` replaced by three whole numbers in range. The URL holds no other data of the page: no variable, no value, no uid and no title.
2. The page loads a tile as an `Image` with `crossOrigin` = `anonymous` and `referrerPolicy` = `no-referrer`. The browser sends no cookie, no HTTP authentication and no Referer. It sends the header `Origin` with the origin of Hune. A host that gives no CORS header gives no tile.
3. The page asks for a tile only for a map in the viewport, 200 ms after the last pan or zoom (`GEO_TILE_DELAY_MS`). It asks only for the tiles of the view: no prefetch. The page does not ask again for a tile that failed in the same view. A pan, a zoom or a new size of the map makes a new view.
4. A new draw of the map at the same size is the same view (section 3, rule 9). Thus a refresh does not ask again for a failed tile. A tile that failed stays empty until a new view or a reload of the page.
5. One page has 6 tile requests in flight at most (`MAX_GEO_TILES_IN_FLIGHT`), for all its maps. A new view stops the requests of the old view.
6. A view that needs more than 256 tiles (`MAX_GEO_TILES_VIEW`) shows the plain background and the note `The map needs <n> tiles. The limit is 256.`
7. The page keeps 256 decoded tiles (`MAX_GEO_TILE_CACHE`). It drops a tile whose natural size is not 256 x 256. The cache keeps a bitmap of 256 KiB for each tile, so it holds 64 MiB at most. The page removes the source of the image after the decode, so the browser can release the downloaded bytes. The page closes a bitmap that leaves the cache.
8. The browser downloads all the bytes of a tile before the page can check its size. The page cannot cap these bytes (12.8b).
9. The policy of 12.8 permits the images of the tile host only below the path of the template (9.4). A redirect to another host gives no tile.
10. The attribution of the block shows at the bottom right while the tiles show, with textContent and no link. Grafana collapses it behind a button "i". Hune always shows it, because the terms of a tile provider often ask for it.

**7. The caps.**

1. `MAX_GEO_POINTS` is 50,000 rows. The page counts the rows before it reads a position. Above the cap the panel draws nothing. It shows the message `Too many points: <n>. The limit is 50,000.` in the state `geo-limit` (9.11). It takes no sample.
2. The caps are constants of the page (4.35). They are not keys of the configuration.

**8. The new panel.** The values of a new panel (10.6) are:

| Option | Value |
|---|---|
| `view` | `{mode: "fixed", lat: 0, lon: 0, zoom: 1}` |
| `layer` | `{lat_field: null, lon_field: null, size: 5, size_min: 2, size_max: 15, size_field: null, color: "dark-green", color_field: null, opacity: 40}` |
| `tooltip` | `details` |
| `show_zoom` | `true` |
| `mouse_wheel_zoom` | `true` |

These are the defaults of Grafana 12.4.2. The view is "(0°, 0°)" at zoom 1. The size is 5, from 2 to 15. The colour is `dark-green` and the opacity is 0.4. The new panel has no colour in `fields`, so the mode `thresholds` of 9.14 applies.

**9. The keyboard and the text alternative.**

1. The canvas has `role="img"`, the name `<title>, map with <n> markers` and a hidden description. It takes the focus.
2. The table of "View data" (9.11) is the frame of the panel.
3. The attribute `data-geomap` of `.panel__plot` holds a JSON text after each draw (9.29).

### 9.20 Table features

- The columns are the fields after the transformations, in their order. The header shows the display name (9.13).
- Width: a field with `width` has this width in CSS pixels. The columns with no `width` share the rest of the view in
  equal parts, with 150 px at least. The text of the cells does not change a width. When the sum is wider than the view,
  the table scrolls on the x axis. The width of the view is the width without the vertical scroll bar.
- Grid look: the table sits inside the padding of the panel (8 px). The header row is 34 px high and a body row is 36 px high. The 1 px rule is inside these heights. A cell has a padding of 6 px.
- Coarse pointer: the filter button of a header has a target of 40 × 40 px. A header that shows one filter button or more is then 47 px high: 40 px, 6 px of padding and the rule of 1 px. A header with no filter button stays 34 px high. A body row stays 36 px high. With `show_header` = `false`, the header has no height.
- Each cell has a rule of 1 px at its bottom, and at its right except in the last column. The rules have the grid line colour of the theme.
- The header text is 14 px at weight 500 in the muted colour. The body text is 14 px at weight 400 in the text colour. A row has a grey hover fill, not a blue one.
- The sort mark shows only on the sorted column. The column filter button shows at all times in a filterable column.
- Align: `auto` puts the kinds `number` and `number_text` on the right and the others on the left.
- The display text of a cell is the `text` of a mapping that matches (9.17). Else, for a `null` or empty value, it is
  `no_value` (9.14). Else it is the formatted value (9.15).
- A field with `hidden = true` has no column in the table. "View data" (9.11) still shows it.
- `cell.type = "auto"`: the text has the text colour of the theme. `cell.type = "color_text"`: the text has the colour
  of the value (9.16).
- A `color_text` colour with a contrast below 4.5:1 with the panel moves toward the text colour of the theme. It moves in steps of 10 % up to the first step that has 4.5:1. The colour in the document does not change. A colour with an alpha channel stays as it is.
- `cell.type = "color_background"`: the background has the colour `C` of the value (9.16). A named colour uses the table "Light fills" in the light theme. With `cell.mode = "basic"`,
  it is the plain colour `C`. With `cell.mode = "gradient"`, it is `linear-gradient(120deg, C2, C)`. `C2` is `C` with its hue turned by 5 degrees (Grafana `darken(10).spin(5)`).
- In the dark theme, `C2` has the HSL lightness of `C` less 10 points. In the light theme, it has the lightness of `C` plus 7 points.
- On a background colour, the ink follows Grafana: `#202226` when the brightness of `C` is above 180 (9.19), else `#F7F8FA`. The ink of Grafana stays when it has a contrast of 4.5:1 or more with the whole fill. For a gradient, the whole fill is both ends.
- Else the page tries the other ink of Grafana. Then it tries the pure colour of the same kind as the first ink (white or black). Then it tries the other pure colour. It takes the first that has 4.5:1.
- When none passes, the page takes the ink with the best contrast. A colour with an alpha channel gets the text colour of the theme.
- `cell.type = "gauge"`: the cell draws a bar and the display text. The scale of the bar is from `min` to `max` of the field configuration (9.14). The default of 9.14 is the lowest and the highest finite value of the column. The filled part has the width `(v - min) / (max - min)` of the bar, kept between 0 and 1. A `null` value, and a scale with `max` not above `min`, give an empty bar.
- The `mode` of a `gauge` cell is `basic`, `gradient` or `lcd`, as the `display_mode` of the bar gauge (9.19). `basic` fills the bar with the colour of the value (9.16). `gradient` fills it with the colours of the thresholds up to the value. `lcd` draws cells that have the colour of the step at their place. The display text has a contrast of 4.5:1 with the colour under it: the ink rule of `color_background` applies.
- `cell.type = "json"`: the cell shows the value as one line of text. The page reads a `text` value as JSON. A valid JSON gives its compact form (`JSON.stringify`). Else the text shows as it is. A `null` value shows `no_value`. The page writes the text with `textContent` and never as HTML (9.1).
- Inspect (measured on Grafana 12.4.2): a column with `inspect = true` (4.18) has the button "Inspect value" in each cell, with each cell type. The button has the eye icon and a target of 24 × 24 px.
  - The button shows when the pointer is on the cell and when the focus is in the cell. Grafana shows it on hover only. It sits at the right edge of the cell, on a fill of the panel colour at 70 %. A cell with the alignment `right` has it at the left edge, as in Grafana.
  - The key `Tab` on a focused cell moves the focus to the button. The key `Enter` on a focused cell opens the drawer, as a click on the button does.
  - The drawer is the form `drawer` of `openDialog`. Its title is "Inspect value", and its subtitle is the name of the column. The body has one block with the value as text. The page writes it with `textContent` only.
  - The value is the raw value of the cell, not the display text. An exact decimal keeps its digits, a time is its number of milliseconds, and `null` gives an empty text. The page removes the blanks at the two ends of the text, as Grafana does.
  - A text that starts with `{` or `[` and that `JSON.parse` accepts shows with an indent of 2 spaces. The indent comes from the tokens of the text, so a number keeps its digits and a string keeps its escapes. When the indent comes to a 65th level of objects and arrays, the text stays as it is.
  - Grafana indents the cell type `json` and the values of the field type `other` only. Hune has no field type `other`: a ClickHouse `Map`, `Array` or `JSON` value is a JSON text (7.16). So Hune indents each such text.
  - The block shows 1,048,576 UTF-16 code units at most. The page cuts a longer text and puts a note above the block. The cut never splits a surrogate pair. The indent stops at the same limit, so the work is linear.
  - The footer has "Copy to Clipboard" and "Close". "Copy to Clipboard" writes the shown text to the clipboard, and a notice gives the result. The key `Escape` closes the drawer.
  - When the drawer closes, the focus goes back to the element that had it: the button or the cell. Grafana puts the focus on the page body.
- A cell of a field with `links` is a link (9.18). It has the link colour of Grafana, and an underline on hover and on keyboard focus only.
- Sort: the initial sort applies the keys of the option `sort` in order (4.27). The second key orders the rows that the
  first key finds equal, and so on. A click on a header makes a sort on that column only, from "up" to "down" to "no
  sort". The header has `aria-sort`.
- Numbers, exact decimals and times compare as numbers, with the exact compare of the copied `app_results_sort.js`.
  Texts compare with the same module.
- The sort puts `null` values last in both directions. The sort is stable. The sort and the column filter below have
  the vector id `FE-11`.
- Column filter: a field with `filterable = true` has a filter button in its header. The button opens the distinct
  display texts of the column, in sorted order, each with a check box. The popup also has a search field, "Select all"
  and "Clear".
- "Apply" keeps the rows whose display text is selected, in each filtered column. The filter is page state: it is not
  in the URL and not in the document. A filtered column shows a marked filter button.
- Virtual rows: from 500 rows, the DOM holds only the rows in view and 20 rows above and below. A row is 36 px high, and
  the header is 34 px high (47 px with a filter button on a coarse pointer). The header stays at the top when the table scrolls.
- Keys: `Tab` goes into the table, and the arrows move the focus from cell to cell (the copied `app_ui_table.js`).
- Wrap: with `options.wrap = false`, a cell text has one line, and an ellipsis ends a text that is too long. With `true`, the text wraps at spaces, and a word longer than the column breaks at any place. The row grows to the height of its tallest cell, with 36 px as the least height. A header text never wraps.
- Wrap and virtual rows: from 500 rows, the page measures the rows that it draws, and it counts 36 px for the others. The scroll bar can then change its size while the viewer scrolls.
- Footer: a column with the property `footer` (4.18) has one footer line for each item of `footer.reducers`. The footer is at the bottom of the table and stays in view when the table scrolls. A footer line is 22 px high, and the footer has 6 px of padding above and below. The footer has as many lines as the column with the most items.
- A footer line has the name of its calculation at the left, in the muted colour (12 px, upper case). The value is at the right (weight 500).
- The name is the Grafana name. `last_not_null` is `Last *`, and `first_not_null` is `First *`. `total` is `Total`, and `std_dev` is `StdDev`. `diff` is `Difference`, and `diff_percent` is `Difference percent`. `change_count` is `Change Count`, and `distinct_count` is `Distinct Count`. The other names are the calculation with a capital letter.
- Maybe all the columns that have a footer have the same items, and the first visible column has none. Then the first column shows the names, and the other columns show only the values. Else each cell shows its names and its values.
- The footer uses the rows that stay after the column filters, in any order of sort. The page computes the calculation again when a filter changes. The value uses the unit, the decimals and the mappings of the field. The calculations `count`, `distinct_count` and `change_count` show a whole number with no unit.
- A column of the kind `text` or `bool` shows only `first`, `first_not_null`, `last`, `last_not_null`, `count`, `distinct_count` and `change_count`. Another item gives an empty line.
- v1 has no pages. The copy of one value is in the drawer "Inspect value" of a column with `inspect`. The CSV download is in "View data" (9.11).

### 9.21 Transformations

The page applies the transforms of a panel (member `transforms`, 4.25) in array order, after long-to-wide (9.22). A transform of the two kinds of this section is a pure function from the fields of one frame to the fields of one frame. 9.21a gives the eleven other kinds. A panel with several queries applies the transforms to the list of frames (9.22a, rule 2), before the frames are combined. A filter that names a field that a frame does not have leaves the rows of that frame.

`filter_by_value` (Grafana `filterByValue`, `FE-12`). Members: `mode` (`include` or `exclude`), `match` (`any` or
`all`) and `filters`, a list of `{field, matcher}`. The `matcher` is a tagged object with `kind`.

1. Each filter finds its field by its current name (9.21a, rule 1). With no earlier rename, the current name is the base display name (9.13). A filter whose field does not exist never passes.
2. With `match = "any"`, a row passes when one filter passes. With `match = "all"`, it passes when each filter passes.
3. `include` keeps the rows that pass. `exclude` keeps the other rows. Each field keeps these rows, in their order.

The matchers compare the Grafana value `v` of the cell (9.12) with the rules of JavaScript, as Grafana does:

| `matcher.kind` | Grafana id | Passes when |
|---|---|---|
| `is_null` | `isNull` | `v === null` |
| `is_not_null` | `isNotNull` | `v !== null` |
| `equal` | `equal` | `v == matcher.value` |
| `not_equal` | `notEqual` | `v != matcher.value` |
| `greater` | `greater` | `!isNaN(v) && v > matcher.value` |
| `greater_or_equal` | `greaterOrEqual` | `!isNaN(v) && v >= matcher.value` |
| `lower` | `lower` | `!isNaN(v) && v < matcher.value` |
| `lower_or_equal` | `lowerOrEqual` | `!isNaN(v) && v <= matcher.value` |
| `range` | `between` | `!isNaN(v) && v > matcher.from && v < matcher.to` (both bounds excluded) |
| `regex` | `regex` | `new RegExp(matcher.pattern).test(String(v))`, with the plain form of 4.24. The page compiles the pattern one time for all the rows. A text of more than 1,024 code units never passes (9.14). |

Example: `{kind: "filter_by_value", mode: "exclude", match: "any", filters: [{field: "status", matcher: {kind:
"equal", value: "failed"}}]}` removes each row whose `status` is `failed`.

`grouping_to_matrix` (Grafana `groupingToMatrix`, `FE-13`). Members: `column_field`, `row_field` and `value_field` (display
names), and `empty_value` (`empty`, `null`, `true` or `false`).

1. The page finds the three fields by their current name (9.21a, rule 1). With no earlier rename, the current name is the base display name. When one does not exist, the fields do not change.
2. The column keys are the distinct Grafana values of the column field, in the order of their first row. The row keys
   are the same for the row field. Two keys are the same when JavaScript `SameValueZero` says so.
3. A column key and a row key give one cell. Its value is the value of the value field in the last row with these
   keys. The page finds the keys of a row by their text form.
4. The first output field has the name `<row_field>\<column_field>`, the kind of the row field and the row keys as
   values.
5. Each column key gives one output field. Its name is the text form of the key, and its kind and configuration are
   those of the value field. Its values are the cells, in the order of the row keys. An empty cell takes the value of
   `empty_value`: `""`, `null`, `true` or `false`.

| `host` | `metric` | `value` |
|---|---|---|
| a | cpu | 10 |
| a | mem | 20 |
| b | cpu | 30 |

With `column_field = "metric"`, `row_field = "host"`, `value_field = "value"` and `empty_value = "empty"`, the result
has the fields `host\metric` = `[a, b]`, `cpu` = `[10, 30]` and `mem` = `[20, ""]`.

### 9.21a More transforms

The pure module `transform_more.js` holds the eleven kinds of 4.25a (`FE-70` to `FE-75`). The page keeps the frames of a panel as a list of frames (9.22a). A frame transform maps each frame alone. A list transform reads all the frames at one time.

The kinds `merge` and `join_by_field` are list transforms. The kind `partition_by_values` maps one frame to a list of frames. The other kinds are frame transforms. The function `applyList` of `transform.js` applies the transforms of a panel to the list (9.22a, rule 2). A kind that the page does not know gives the panel error "The transform <kind> is not available in this version of the page.". It never skips the kind.

1. The current name. The page gives each field a current name. It is the display name of 9.13, with the name that an earlier `organize` or `rename_by_regex` set in `config.display_name`.
   - The page finds a field by this name in all the kinds of 9.21 and 9.21a, with two exceptions. `merge` and `join_by_field` find a field by its `name`. The reason is that the display name has the prefix of its query when a panel has two visible queries (9.22a, rule 5). With two visible queries, the name `x` finds no field in a frame transform: the current name is `A x`.
   - A rename sets `config.display_name` and keeps `name`. A rename text is plain text, and the page writes it with `textContent`.
   - A name that is not a field of a frame does nothing in that frame. This is the rule of the two old kinds and the measured rule of Grafana.
   - A frame that has no field at all stays an empty list, and the panel shows "No data".
2. `organize`. For each frame, the page removes the fields of `hide`. It puts the fields of `order` first, in the order of the array. The other fields follow in the order of the frame. It renames the fields of `rename`.
   - The names in `order`, `hide` and `rename` are the current names before the rename.
   - A field in `hide` and in `rename` is removed. A rename to the name of another field is permitted, and both fields then have the same name.
3. `calculate_field`. For each frame, the page adds one field of the kind `number`, or none. A name that is not a field gives no new field and no error.
   - The name of the new field is `alias`, else the default name.
   - The default name for `binary` is `<left> <symbol> <right>`. The symbols are `+`, `-`, `*` and `/`. An operand is its field name or its number.
   - The default name for `reduce_row` is the title of the reducer (the table below). For `unary` it is `<operator>(<field>)`. For `index` it is `Row`.
   - The operands of `binary` and `unary` are fields of the kind `number`, `number_text` or `time`, or `bool`. A `bool` counts as 1 or 0. A field of another kind gives the panel error "The transform calculate_field failed: the field <name> is not a number".
   - A null cell counts as 0 in `binary` and `unary`, as Grafana does with JavaScript. A division by 0 gives infinity or NaN, and the cell is not null. `ln` of a value of 0 or less gives negative infinity or NaN. `round` is `Math.round`.
   - `percent` gives the value divided by the sum of the finite values of the field. A sum of 0 gives the value divided by 1.
   - `reduce_row` applies the calculation of 9.19 to the cells of one row. The cells are the fields of `include`, or the fields of the kind `number` and `number_text` when `include` is empty. The row is a field of the same length, so the calculations of 9.19 apply with no change. A calculation that gives no value gives a null cell. The calculations `median`, `distinct_count` and `count` can differ from Grafana for a row with a null cell (19.20j).
   - `index` gives 0 to n-1, or i/n with `as_percentile`, and then the field has the unit `percentunit`.
   - With `replace_fields`, the frame keeps its first field of the kind `time` and the new field.
4. `sort_by`. For each frame, the page sorts the rows by the keys of `sort`, the first key first. It uses the table sort of 9.20 (`rowOrder`).
   - A null comes last in both directions. Text compares by the rule of the table. The sort is stable.
   - The result can differ from Grafana for null values, case, booleans and a second key (19.20j).
   - A key that is not a field is ignored. With no key left, the frame does not change.
5. `limit`. For each frame, the page keeps the first `count` rows, or the last `count` rows with `end` = `last`. A `count` above the number of rows keeps all rows. A `count` of 0 keeps a frame with zero rows.
6. `merge`. A list transform. It does nothing with fewer than two frames that have a field, or when no `name` is in all those frames. Else:
   - The result has one frame. Its fields are the distinct names of all the frames, in the order of first appearance. A field has the kind and the configuration of its first frame, and it has no `ref` and no `prefix`.
   - The key fields are the names that all frames have. The key of a row is the list of its values in the key fields.
   - The page goes through the rows of each frame in order. A row joins the first earlier row that has the same key and no field with a different value. A null in the earlier row, and a field that the frame does not have, never conflict. A null in the new row conflicts with a value. Else the row is a new row. The result keeps the order of first appearance.
   - The rows of one frame that are equal join too, as in Grafana (measured).
   - Two frames with the same name and two kinds give the panel error "The transform merge failed: the field <name> has two types". Grafana keeps the first kind and loses the values.
   - A compare of two rows is one step for each field of the result. The count of steps is `MAX_MERGE_STEPS` at most. A higher count gives the error "The transform merge failed: too many comparisons; add a field that all frames share".
   - Before the first row, the page checks the fields and the cells of rule 13. The rows of this check are the rows of all the frames. The result cannot have more rows.
7. `join_by_field`. A list transform. It does nothing with fewer than two frames. Else:
   - The key field of a frame is its first field of the kind `time` when `field` is `null`, else its field of that `name`. A frame with no key field gives the panel error "The transform join_by_field failed: a frame has no field <name>" (or "no time field"). The key fields MUST have the same kind, else the error "the key fields have two types".
   - The result has one frame. Its first field is the key field of the first frame, with no `ref` and no `prefix`. The other fields of the frames follow, frame by frame, in their order. A field keeps its kind, configuration, labels, `ref` and `prefix`.
   - `outer`: the keys are the distinct values of all key fields. A number key and a time key sort up. A text key and a bool key keep the order of first appearance. A null is a key that joins with a null. When a frame has several rows for one key, the last row wins. A frame without the key has `null` in its fields (`NaN` for the kind `number`).
   - `inner`: the page goes through the rows of the first frame in order. A row stays when each other frame has a row with its key. Then it makes one output row for each combination of the rows that share the key. The first frame is the slowest index, so equal keys multiply.
   - The count of output rows is `MAX_TRANSFORM_ROWS` at most. The page checks the output fields, then the output rows, then the cells (rule 13). It does this before it builds a row.
   - `outer` counts the distinct keys. `inner` counts the rows of each combination in a first pass, and a second pass builds them.
   - The Grafana mode `outerTabular` has no kind (15.6).
8. `reduce`. For each frame, the page reduces each field that is not of the kind `time`. A text field is a row too.
   - `series_to_rows`: the result is one frame. Its first field is the text field `Field`, with the current names of the reduced fields. Then one field comes for each reducer, in order, with the title of the reducer as its name. The cell is the value of the calculation of 9.19. A reducer field has the kind `number` when all its cells are numbers or null, else the kind `text` with the cells as text.
   - `reduce_fields`: the result has one row. Each reduced field and each reducer give one field. It has the name of the reduced field and the label `reducer` equal to the title. The names then show as `x Min`, as in Grafana (9.13, step 3).
   - Grafana reduces all frames to one frame, and Hune reduces each frame alone (19.20j).
   - The count of a field with null cells follows 9.19: it includes them. Grafana does not include them in this transform (19.20j).

| Calculation | Title | Calculation | Title | Calculation | Title |
|---|---|---|---|---|---|
| `last_not_null` | `Last *` | `max` | `Max` | `diff` | `Difference` |
| `last` | `Last` | `mean` | `Mean` | `diff_percent` | `Difference percent` |
| `first` | `First` | `total` | `Total` | `delta` | `Delta` |
| `first_not_null` | `First *` | `count` | `Count` | `distinct_count` | `Distinct Count` |
| `min` | `Min` | `median` | `Median` | `change_count` | `Change Count` |
| `std_dev` | `StdDev` | `variance` | `Variance` | `range` | `Range` |

9. `partition_by_values`. For each frame, the page splits the rows by the values of the key fields. A key that is not a field is ignored, and with no key left the frame does not change.
   - One new frame comes out for each distinct list of values, in the order of first appearance. A value is its text form: a null is `null`, and the empty text is another value.
   - A new frame has the fields of the frame, with the key fields removed unless `keep_fields` is `true`. Each field gets the labels `{<key name>: <value>}` added to its own labels.
   - A frame keeps its `ref`. The combination of 9.22a then works on the list. A `table` shows each new frame in its select, with the option text `<ref> <value> ...` (9.22a, rule 9).
   - The result has `MAX_TRANSFORM_FRAMES` frames at most. A higher count gives an error.
   - Grafana splits the first frame only and loses the others. Hune splits each frame (19.20j).
10. `filter_fields_by_name`. For each frame, a field passes the inclusion when the inclusion is empty. It also passes when its current name is in `names` or the pattern matches it. The pattern has the Grafana form (4.24), so a plain text is an exact match.
   - A field that passes stays unless the exclusion matches it in the same way.
   - A pattern never matches a current name of more than 1,024 code units (9.14). Thus an inclusion pattern does not keep such a field, and an exclusion pattern does not remove it. A name of `names` has 1,024 bytes at most, so it never equals such a name.
   - The order of the frame does not change. A frame with no field left is an empty list.
11. `rename_by_regex`. For each field of each frame, the page calls `name.replace(re, rename_pattern)`, with `re` from `regex` (4.24) and `name` the current name. When the result differs from the name and is not empty, it sets `config.display_name`. Else the field keeps its current name (9.13).
   - `$1` to `$99`, `$$`, `` $` ``, `$'` and `$&` follow the JavaScript rule. `$0` and `${name}` stay as text.
   - A current name of more than 1,024 code units keeps its name. The page does not run the pattern on it (9.14).
   - A pattern that does not compile gives the panel error "The transform rename_by_regex failed: the pattern is not valid". The check of the document refuses such a pattern first.
12. `convert_field_type`. For each frame, each conversion applies to the field of that current name. The new field keeps the name, the labels and the configuration. A null stays null. The rules of the new value:

| From, to | Rule |
|---|---|
| any, `text` | The text form of the Grafana value (9.12). A `time` is an ISO 8601 text in UTC with the letter `Z`. A `bool` is `true` or `false`. |
| `text`, `number` | The text, with the white space cut, MUST match `^[+-]?(?:[0-9]+(?:\.[0-9]*)?\|\.[0-9]+)(?:[eE][+-]?[0-9]+)?$`. Else the value is null. A run of digits matches in one way only, so the time of the test is linear in the length of the text (T87). |
| `bool`, `number` | 1 or 0 |
| `time`, `number` | The epoch in milliseconds |
| `text`, `time` | An ISO 8601 text: `YYYY-MM-DD` (UTC) or `YYYY-MM-DDTHH:MM[:SS[.fff]]` with a zone (`Z` or `+HH:MM`). Else the value is null. |
| `number`, `time` | The value is the epoch in milliseconds |
| `number`, `boolean` | `false` for 0, else `true`. NaN gives null. |
| `text`, `boolean` | `true` or `1` gives `true`. `false` or `0` gives `false`. Else null. |
| `bool`, `time`; `time`, `boolean` | null |
| the same kind | No change |

   - Grafana gives other values for several cells (19.20j). The Grafana option `dateFormat` has no field.
13. The caps. After each transform, the page checks the frames. A frame has `MAX_TRANSFORM_ROWS` rows and `MAX_TRANSFORM_FIELDS` fields at most. The list has `MAX_TRANSFORM_FRAMES` frames at most. A higher count gives the panel error "The transform <kind> failed: <n> rows, 200,000 at most" (or fields, or frames). The page never cuts, samples or rounds.
   - The query limits of 8.5 bound the input. These caps bound the output of one step. The query limit of a source can be lower than the cap of a frame. A join of three frames of 99,000 rows goes above the cap (case B185).
   - Some steps make new cells: `join_by_field`, `merge`, `grouping_to_matrix` (9.21), the long-to-wide of 9.22 and the join on time of 9.22a. Each one checks its result before it builds a cell. The result has `MAX_TRANSFORM_FIELDS` fields at most, then `MAX_TRANSFORM_CELLS` cells (fields times rows) at most.
   - `MAX_TRANSFORM_CELLS` is the value of the key `server.transform_max_cells` (13.8). The page reads it from the member `transform_max_cells` of the session (6.10) at each check. It keeps no copy of the number. The default is 10,000,000.
   - A higher count gives the panel error "The transform <kind> failed: <n> cells, <max> at most" (or fields). `<max>` is the cap in force with commas, for example `10,000,000`. The long-to-wide gives "The long-to-wide failed: ..." and the join on time gives "The join on time failed: ...".
   - Each of these steps fails without a positive integer in the member `transform_max_cells` of the session. The error is "The transform <kind> failed: the session has no valid transform_max_cells", with the start of the line above. The page never guesses a cap (fail closed).
   - The other steps have no cap of cells. They keep or move the cells, add one field at most (`calculate_field`) or make one row for each field (`reduce`). Thus a frame of 200,000 rows and 60 fields passes through `organize`.
14. The errors. An error of a transform stops the work of the panel. It shows as the error of the panel (9.11), with the text of rule 3, 6, 7 or 13. The panel keeps the state `error`, and the other panels do not change. The page never shows a half result.
15. The cost. A transform of 200,000 rows takes 300 ms or less on the main thread. `merge` and `join_by_field` take the same for 200,000 rows in all the frames. A front unit test checks these times (16.10a, `FE-75`).
   - The test takes the best of 7 runs. It MUST run on a machine with a low load. It does not run when the load average of one minute is above half of the cores.
   - A machine with a high load makes these times longer. At a load average of 29, one measure took 520 ms to 960 ms for these steps.
16. A `reuse` panel (9.23) applies the transforms of its source and its own to the list of frames, with the same code. `merge` and `join_by_field` read the list of frames of the source panel.

Example. The panel has two queries. `A` gives `host` = `[a, b]` and `x` = `[1, 2]`. `B` gives `host` = `[b, c]` and `y` = `[20, 30]`. The transform `join_by_field` with `field` = `host` and `mode` = `outer` gives `host` = `[a, b, c]`, `x` = `[1, 2, null]` and `y` = `[null, 20, 30]`. The mode `inner` gives `host` = `[b]`, `x` = `[2]` and `y` = `[20]`.

### 9.22 Long-to-wide

Long-to-wide (`FE-14`) restates the `LongToWide` function of Grafana, which the ClickHouse plugin applies to a query of the
format `time_series`. The page applies it when the panel query has `format = "time_series"` and the fields hold at
least one `text` field. It runs before the transformations.

1. The time field is the first field of the kind `time`. With no time field, the panel shows the error "A time series
   query needs a time column.".
2. The label fields are the `text` fields. The value fields are the other fields, except the time field. With no value
   field, the fields do not change.
3. The page leaves out each row with a `null` time. It sorts the rows by time, up, with a stable sort. Grafana refuses
   rows out of order, so no real panel depends on that error.
4. The distinct times, in order, make the new time field, with the name of the time field.
5. The label set of a row is the values of its label fields. A `null` label value counts as the empty text.
6. The page orders the label sets by their label values, in code unit order. It compares the label fields one after the other, in column order. The order of the rows does not matter. Then, for each label set and for each value field in column order, it makes one field.

   The field has the name of the value field and the labels `{<label field name>: <value>}`. It has the kind and the
   configuration of the value field. It has one value for each distinct time. This value comes from the row with this
   time and this label set, else it is `null`.
7. When two rows have the same time and the same label set, the last row wins.
8. The caps. The result has one field for each label set and each value field, and the time field. It has one row for each distinct time. Before it builds a field, the page checks `MAX_TRANSFORM_FIELDS` and then `MAX_TRANSFORM_CELLS` (4.35, 9.21a, rule 13).
   - A higher count gives the panel error "The long-to-wide failed: <n> fields, 2,000 at most" (or cells). The cap of the cells is the value of `server.transform_max_cells`. Grafana has no cap.
   - A text column with a different value in each row gives one field for each row. 8,000 such rows gave 64,000,000 cells before this rule.

| `t` | `node` | `weight` |
|---|---|---|
| t1 | a1 | 10 |
| t1 | b2 | 20 |
| t2 | a1 | 11 |

The result has the fields `t` = `[t1, t2]`, `weight` with `node="a1"` = `[10, 11]`, and `weight` with
`node="b2"` = `[20, null]`. Their display names are `t`, `weight a1` and `weight b2` (9.13). When the rows of `b2` come first, the series `a1` still comes first.

A panel query with `format = "table"` gets no long-to-wide.

### 9.22a Frames of several queries

A panel with several visible queries (4.15a) gets one frame for each query that answered. The page turns these frames into the fields of the panel with the steps below. The pure module `queries.js` holds the steps. A panel with one visible query follows the pipeline of 9.22, 9.21 and 9.14 as before.

1. Each frame becomes fields (9.12). The long-to-wide of 9.22 applies to a frame when its own query has `format = "time_series"`. Each field gets the member `ref`, which is the ref of its query.
   - With two visible queries or more, a field that is not a time field also gets the member `prefix`.
   - The `prefix` is the same ref (rule 5).
2. The transforms of the panel apply in array order to the list of frames. A frame transform maps each frame alone. This holds for all kinds except the next two.
   - `partition_by_values` maps each frame to a list of frames.
   - `merge` and `join_by_field` read the whole list and give a list of one frame (9.21a).
   - A `reuse` panel with `with_transforms` applies the transforms of its source first, then its own.
   - The combination of rule 3 reads the list after the last transform. Thus a `merge` or a `join_by_field` leaves one frame, and rule 3 has nothing to combine.
3. With two frames or more, the page combines the lists of fields by the type of the panel (rule 4). With one frame, the list is the result.
4. The combination by type:

| Type | Combination |
|---|---|
| `timeseries`, `state_timeline`, `status_history`, `heatmap` | The join on time (rules 6 to 8). The result is one list with one time field. |
| `table` | One frame at a time. A select under the table lists the refs of the frames. The first frame shows at first. |
| `stat`, `bargauge`, `piechart`, `histogram`, `gauge` | The lists one after the other, in the order of the queries. There is no join and no padding: each field keeps its own rows. |
| `barchart`, `xychart`, `logs` | No combination. These types have no extra query (4.15a, rule 7). A `reuse` panel of these types has a source with one query (4.15, rule 6). |

5. The prefix and the matcher. The `prefix` is set when the panel has two visible queries or more, also when a query failed or has no row. The display name of the field has the prefix first (9.13, step 1a). The matcher `query` (9.14) compares its value with the `ref` of the field. The joined time field has no `ref` and no `prefix`. The rule counts the visible queries and not the answers, so a name does not change when one query fails (9.8d).
   - The heatmap is the one exception to the names of 9.22. It drops the value-field prefix of a long frame (9.19e, section 1, rule 4). With two visible queries, the prefix is the ref, so the heatmap keeps the name `A count 8`.
6. The join. It needs a time field in each frame. The first field of the kind `time` of a frame is its time field. A frame with no time field gives the error "The frame of query <ref> has no time column.", and the panel has the state `error`.
   - The page drops each row with a `null` time. It sorts the rows by time, up, with a stable sort.
   - When two rows of one frame have the same time, the last row wins.
   - These are the rules of 9.22, steps 3 and 7.
7. The result of the join has one time field. Its values are the distinct times of all the frames, in order. Its name and type are those of the time field of the first frame.
   - Then come the other fields of the first frame, then those of the second frame, and so on. A second field of the kind `time` is a value field.
   - A field has one value for each time. This value is the value of its frame at this time, else `null` (`NaN` for the kind `number`).
   - The kind, the configuration and the labels of a field do not change.
8. The cap of the join. The frames have `MAX_JOIN_ROWS` = 200,000 rows at most in all. A larger set gives the error "The frames have <n> rows. A join takes 200,000 rows at most." and the panel has the state `error`. The check comes before any work, and the page never cuts a frame. The join on time also checks the fields and the cells of its output before it builds them (9.21a, rule 13).
9. The table. The select (`data-testid="panel-<key>-frames"`) has one option for each frame, with the ref as its text. Its label is "Query". A change draws the table again with the fields of that frame. The choice stays while the ref is in the list, and it goes back to the first frame when the ref leaves. The select shows only when two frames or more answered.
   - A frame from `partition_by_values` has the option text `<ref> <label value> ...`. The label values come from its first field. The choice stays while this text is in the list.
10. The names that the legend, the tooltip, "View data" and the CSV show are the display names (9.13). The order of the series is the order of the queries, then the order of the fields in the frame. This is the order of Grafana (measured). "View data" and the CSV of a collected list have one column for each field. A field with fewer rows than the longest field has empty cells.
11. The cost. The join takes 150 ms or less on the main thread for 8 frames and 200,000 rows in all. A front unit test checks this time (16.10a).

Example 1. The frame of `A` has `time` = `[0, 300, 600]` and `a` = `[1, 2, 3]`. The frame of `B` has `time` = `[150, 300, 750]` and `b` = `[10, 20, 30]`. The join gives `time` = `[0, 150, 300, 600, 750]`, `A a` = `[1, null, 2, 3, null]` and `B b` = `[null, 10, 20, null, 30]`.

Example 2. A `stat` with the same two queries has the fields `time`, `A a`, `time` and `B b`. Each field has 3 rows. The stat shows two tiles.

### 9.23 Panel reuse

A panel with the data `reuse` (4.15) shows the result of another panel of the same dashboard (`FE-15`). The source
panel MUST have a query: a reuse of a reuse is not permitted.

1. The page starts from the frames of the visible queries of the source panel (9.8d), each one after its own long-to-wide (9.22). A source with one query gives one frame, as before.
2. When `with_transforms` is `true`, it applies the transformations of the source panel.
3. It applies the transformations of the reuse panel.
4. It computes the field configuration with the defaults and the overrides of the reuse panel (9.14).
5. With two frames or more, the page combines them with the rule of the type of the reuse panel (9.22a, rule 4). The transforms of the source apply to each frame alone (9.22a, rule 2).

- The reuse panel never sends a query. When it needs data, the source panel queries, also out of view or in a
  collapsed section (9.8).
- When the result of the source changes, the reuse panel shows the new result.
- An error of the source gives the error "The source panel <title> failed: <message>" in the reuse panel. A partial result of the source (9.8d) is a partial result of the reuse panel, with the same badge.

### 9.24 The copy of the ClickHouse Dash chart engine

Hune copies the chart engine of ClickHouse Dash and its local imports at a recorded commit. The copy has a lock file
and the MIT notice. Hune owns the copy.

- Upstream: the ClickHouse Dash repository, tag `v1.3.1`, commit
  `e970694e5e0b17b07cf187f3128af93fa1a82b76`, licence MIT.
- Hune copies whole files, minus the removals that `docs/engine-patches.md` lists (E17 to E21 and
  E35 to E42). A JavaScript file goes to `web/js/vendor/chdash/` and a CSS file to
  `web/css/vendor/chdash/`, with its upstream file name.

| Upstream file | Use in Hune | Its imports |
|---|---|---|
| `src/static/app_chart_core.js` | Time series, bar chart and xy chart (canvas, per-pixel decimation, legend, tooltip, zoom) | `app_dom`, `app_ui_state`, `app_util` |
| `src/static/app_format.js` | None: no file imports it after the removals E35 and E36 | none |
| `src/static/app_util.js` | `escapeHtml`, `rafOnce` | none |
| `src/static/app_ui_chart.js` | The line of a stat | `app_util` |
| `src/static/app_ui_stat.js` | The card of a stat | `app_util` |
| `src/static/app_ui_table.js` | Sort header and keys of the rows of a table | `app_dom` |
| `src/static/app_results_sort.js` | Exact compare of numbers, decimals and texts | `app_results_values` |
| `src/static/app_sql_lex.js` | SQL lexer of the page (7.5, the dialect `clickhouse`) | none |
| `src/static/app_highlight.js` | SQL colours of the editor (10.3) | `app_dom`, `app_loader`, `app_state`, `app_sql_lex`, `app_ui_sql`, `app_util`, `app_wasm` |
| `src/static/css/00-tokens.css` | Tokens of both themes (9.25) | none |
| `src/static/css/10-components/chart.css`, `stat.css`, `table.css`, `sql.css` | Styles of the copied modules | none |

Shim modules:

- The copies import some modules that Hune does not copy. Hune writes these modules, with the same file names, in
  `web/js/vendor/chdash/`: `app_dom.js`, `app_state.js`, `app_ui_state.js`, `app_loader.js`, `app_wasm.js`,
  `app_results_values.js` and `app_ui_sql.js`.
- Each shim exports exactly the names that the copies import, with the behaviour that the copies use. These names are
  `dom`, `storage`, `state`, `uiState`, `loader`, `wasm`, `NUMERIC_RE` and `sqlBlock`. The removal E20 took away the
  shim `app_ui_copy.js` and the names `h`, `shell` and `copy`.
- `wasm.ops` is an empty object, `wasm.get()` gives `null` and `wasm.load()` gives a promise of `null`.
  `loader.loadGroup()` gives a promise that is already fulfilled. Thus each copied module takes its JavaScript path.
- The keys of `localStorage` of the copies start with `hune.chdash.`.
- The copies are the upstream files, except `app_chart_core.js`, `app_highlight.js`, `app_results_sort.js`,
  `app_ui_chart.js`, `app_ui_table.js` and `app_util.js`. They have marked edits (below).

Later edits:

- A panel can need a feature that the engine does not have. The first choice is a change in ClickHouse Dash, then a
  new copy with a new lock.
- Else Hune MAY edit a copied file. Each changed block has a comment `// hune:` with the reason, and the lock records
  the new hash. The field `upstream_sha256` keeps the hash of the upstream file. Hune code outside `vendor/` never changes the engine at run time.
- The engine at the commit `e970694e` does not have a UTC time axis. It uses the zone of the browser. It also does not have threshold lines and areas, a sort of the legend, a sort of the tooltip, or a right y axis. A dashboard with `timezone = "utc"` needs the UTC axis. M0 tells which gaps the real panels use (19).
- The first four gaps close with marked edits of the copy (edits 7 to 9 below). The look of the bars and of the time axis adds the edits 10 to 12. The right y axis, the style of each series and the title of the right axis (edits 13 to 15). Edit 16 is in `app_highlight.js`: it pushes the tokens with a loop, because a spread of 125,000 tokens throws (10.3c).
- The edits 17 to 21 remove code that Hune never calls. They are the exemplar
  markers (17), the 2-D brush (18) and the engine annotations (19) of `app_chart_core.js`. They are also the helpers of
  `app_ui_table.js` that `panels/table.js` does not call (20), and the sort of the rows of `app_results_sort.js` (21).
- The removals keep each part that a panel type, F31 or F43 can call: the regions, the pick, the cells and the bars.
  `docs/engine-patches.md` gives the proof, the size of each removal and the procedure at an update. The next edit is 48.
- The edits 35 to 42 also remove code that Hune never calls. Edits 35 and 36 remove the helpers of
  `app_util.js` and the range of a bucket. Edit 37 removes the colour kernel and the copy `app_palette.js`. Edits 38
  and 39 remove the members of the shim `app_ui_state.js` and the card of `app_ui_chart.js`. Edits 40 to 42 remove the
  alignment groups, the filter and totals legends, the button of the legend mode and other options.
- The test `tests/frontend/engine_removals.test.mjs` fails when a removed name comes back as a call, an option or an
  import in `web/js` (16.10a).
- Hune changes the local copy only. No pull request, issue or message goes to the upstream repository from this work. The file `docs/engine-patches.md` holds each change as a proposal.
- The engine does not have horizontal bars. The module `panels/barchart_h.js` draws them (9.19). It is not an edit of the engine.
- The engine has more gaps against the options of 4.26:

| Option | The engine at `e970694e` | How Hune covers it |
|---|---|---|
| `legend.placement = "right"` | The legend is always under the plot. | CSS of Hune places the legend at the right and moves it under the plot when the panel is narrow (9.19). The engine does not change. |
| `legend.mode` | The list or table mode is a preference of the viewer (`readLegendMode`). | Hune calls `setLegendMode` for each panel, so the option of the document wins. The removal E41 takes away the preference and the button of the mode. |
| `tooltip.mode = "hidden"` | The modes are `single`, multi and `none`. | No change: the page maps `hidden` to the engine mode `none`. |

M0 counts the panels that need each change (18, 19). The gaps that Hune code does not cover stay until a change is in the copy.

- The pie chart and the bar gauge are Hune modules (9.19), not edits of the engine.
- The state timeline and the status history are Hune modules (9.19c). They are not edits of the engine, and the lock does not change. They call `chartCore.timeTicks` and `chartCore.layoutXLabels`, which the engine exports for charts that draw their own marks.
- The marks of the annotations are a canvas of Hune (9.19d). They use the option `onDraw` and the call `layout()` of the engine, and they are not an edit of the engine. The lock does not change. The engine has the option `regions`, and it had the options `annotations` and `markers` (removed by the edits 19 and 17). Hune does not use them for the annotations, for three reasons.
- The engine writes the label of an annotation in a strip over the plot with one colour for all. A region has a class and no colour. The markers sample the marks above one for each 40 px, which hides marks with no sign. The text of the marker tooltip is HTML (9.19d, rule 12).
- The heatmap and the histogram use the engine (9.19e, 9.19f). The engine has the heatmap cells (the option `cells`), the pick of a cell under the pointer and the bars on a number axis. Three marked edits are necessary. Each one has the comment `// hune:`, and the lock records the new hash. `docs/engine-patches.md` holds the rows E22 to E24.

| Id | What the edit does |
|---|---|
| E22 | The option `cellGap` (CSS pixels, default 1) replaces the fixed gap of 1 px of `drawCells`. A cell of 3 px or less still has no gap. |
| E23 | The option `barOverlap` makes all the bar series share one place on the x axis, so the bars overlap, as in Grafana. Without it, the series stand side by side. It has no effect on a stacked chart. |
| E24 | The option `barSlot` (in x units) is the slot of a bar. It replaces the smallest gap of the x values, so one bucket of a histogram fills the plot. The option `barDecimate: false` turns off the choice of the highest bar for each 3 px, which the engine makes when it has more bars than one third of the plot width. Without the options, the engine keeps its rules. |

The edits are about 0.4 KB. The engine at `e970694e` has no other gap for these types. The page does not use a log scale of the engine, a reverse axis or a right axis for them. The page gives the engine a plot coordinate (9.19e).

The bar chart turns the labels of its x axis (4.31, 9.19). One marked edit is necessary. It has the comment `// hune: E25`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E25 | The option `xTickRotation` (degrees) turns the labels of a category axis about their anchor. A positive angle is clockwise, and the start of the text is at the tick. A negative angle is counter-clockwise, and the end of the text is at the tick, as in Grafana. The bottom margin grows by the height of the tallest turned label, and a turned label is shown by its turned width. The anchor of a label stays where the whole label is in the canvas. The value 0, or no option, gives the same picture as the engine of before, byte for byte. |

The edit is about 3.3 KB with its comments. The function `layoutXLabels` gets a last parameter, the angle. The test `tests/frontend/tick_rotation.test.mjs` checks the angle, the margin, the labels that show and the edges of the canvas (16.10a). The browser case B153 checks that the control changes the picture.

The histogram keeps the line of a bar inside the bar (9.19f, section 3). One marked edit is necessary. It has the comment `// hune: E26`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E26 | The option `barInset` moves the top of the box of each bar down by half the line width, and the bottom up by the same amount. The line on the edge of the box then ends at the value and at the axis, as in Grafana. The x and the width do not change. Without the option, or with no line, the boxes are the same as before. Only the histogram gives the option, so the bar chart keeps its picture. |

The edit is about 0.3 KB with its comments. The test `tests/frontend/bar_inset.test.mjs` checks the boxes with and without the option (16.10a). The visual capture of 15.14c checks the bars of the fixture `histogram_f` with a line of 3 px.

A plot with no y axis has no margin at the top, as in Grafana 12.4.2 (9.19, 9.19e). One marked edit is necessary. It has the comment `// hune: E27`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E27 | When the left axis and the right axis both do not show, the plot starts at the top of the canvas and takes the 8 px of the margin. An axis that shows on one side keeps the margin of 8 px. The edit adds no option. Two panels hide every y axis: a heatmap with `y_axis` = `hidden`, and a time series whose series all have `axis_placement` = `hidden`. |

The edit is about 0.2 KB with its comment. The fit of a turned label (E25) still subtracts the constant margin of 8 px. This has no effect, because a bar chart always shows its value axis. The test `tests/frontend/plot_top.test.mjs` checks the top of the plot with and without an axis (16.10a). The browser cases B60 and B143 check it on a page. The visual capture of 15.14c checks it on the fixtures `heatmap_c` and `parity_axes`.

Review v3 of the page code (2026-11-16) found two patterns whose time grows with the square of the cell length. A person who writes data can put such a cell in a result (T87). Two marked edits are necessary. Each one has the comment `// hune:`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E28 | The shim `app_results_values.js` gives `NUMERIC_RE` the form `^[+-]?(?:\d+(?:\.\d*)?\|\.\d+)(?:[eE][+-]?\d+)?$`. It matches the same texts as the upstream form, but a run of digits matches in one way only. The page also tests the cells of `convert_field_type` and the row names of a heatmap with it. |
| E29 | `app_results_sort.js` cuts the sign and the zeros of a decimal text with string steps. The upstream pattern took 0.9 s for one compare of two PostgreSQL `numeric` texts of 16,385 characters. `normalizeNumericForSort` keeps the parts of a long decimal text, and the file exports it with `compareNormalizedForSort`. Thus the sort of a table normalises each row once (9.20). |

The two edits are about 0.9 KB with their comments. The shim is not a copy, so `vendor/chdash.patch` holds E29 only. The test `tests/frontend/page_security_v3.test.mjs` checks the times, the order and that no file of `web/js` holds a pattern of the quadratic shape (16.10a).

The y range of a time series follows Grafana for bars, a stack and the soft ends (9.19). The security review of 2026-11-16 found that the time to cut a long axis title grows with the square of its length. Two marked edits are necessary. Each one has the comment `// hune:`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E30 | The hook `yAxis` of a scale gets a fourth argument: the data ends `[lo, hi]` of the visible series of the scale, or `null`. In a stack, the tops count and the base does not. The soft ends of `ySoft` do not count. The first three arguments do not change, so a hook that does not read the fourth one works as before. |
| E31 | `fitTitle` cuts a title to 256 characters first. Then a binary search finds the longest start of the text that fits with the ellipsis. A title of any length takes 10 measures or less, not one measure for each character. A title of 256 characters or less gets the same text as before. |

The two edits are about 0.9 KB with their comments. The test `tests/frontend/y_range.test.mjs` checks the data ends of a stack and of a scale with soft ends (16.10a). It also counts the measures for a title of 40,000 characters, and it compares the text of normal titles.

The stacks of a time series follow Grafana 12.4.2: a series below 0 stacks down from 0 (9.19, stacks). One marked edit is necessary. It has the comment `// hune: E32`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E32 | The option `stackSign: "series"` puts each series of a stack in the group of the sign of most of its values. The group below 0 stacks down from 0, and each value adds with its sign. A `null` value takes the top of its group when another series of the group has a value there. Else the series has no point there, and its line breaks only where the series flags a `null` (`nulls`). A percent is of the signed sum of the group. Without the option, the stack of the engine does not change. The time series and the bar chart set it. |

The edit is about 1.8 KB with its comments. The test `tests/frontend/stack_sign.test.mjs` checks the ticks of 100 panels and the drawn lines against the measure of Grafana (16.10a). It also checks that the engine keeps its stack without the option, and the rule of a log scale.

The groups of a stack also follow the draw of each series, as in Grafana 12.4.2 (9.19, stacks; point 3). One marked edit is necessary. It has the comment `// hune: E33`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E33 | With the option `stackSign`, a series that has a `type` stacks in a group of its own. A series has a `type` when it draws another way than the chart: bars, points or a line. Each group stacks from 0, and a percent is of the sum of its own group. |

The edit is about 0.3 KB with its comments. The test `tests/frontend/stack_parity.test.mjs` checks the ticks and the drawn marks against the measure of Grafana (16.10a). It also checks the bar chart and the histogram of 9.19f.

The fill and the line of a bar follow the bar chart of Grafana 12.4.2 (9.19, `barchart`). One marked edit is necessary. It has the comment `// hune: E34`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E34 | The value `"box"` of the option `barInset` also moves the left side of the box of each bar right by half the line width, and the right side left by the same amount. The line then stays inside the bar on its four sides. A bar with the fill opacity 0 gets no fill call. The value `true` of E26 and an absent option keep their boxes. |

The edit is about 0.3 KB with its comments. The test `tests/frontend/barchart_p03.test.mjs` checks the boxes, the fill and the line of the canvas calls against the measure of Grafana (16.10a).

The gradient modes and the cosmetic series options follow Grafana 12.4.2 (9.19). Four marked edits are necessary. Each one has the comment `// hune: E<n>`, and the lock records the new hash.

| Id | What the edit does |
|---|---|
| E43 | A series can hold a function `paint(part, ctx, L, c, alpha)`. The engine calls it for the fill (`fill`) and for the line and the points (`line`) of a line, of a stacked area and of bars. The function gives a canvas style, or `null` for the plain colour. `L` is the view of the scale of the series. The view of a log scale holds the view in the units of the values (`real`). |
| E44 | The option `barAlign` (-1 or 1) moves each bar by half its width plus half its line. Thus the bar ends at its x less half the line, or starts at its x plus half the line. Without the option, a bar is centred, as before. |
| E45 | The engine sets the dash array of a series in device pixels: it divides each length by the device pixel ratio. At the ratio 1, the picture does not change. |
| E46 | The options `yWidth`, `yBorder` and `yColor` of the left scale, and the same members of `y2`, give the look of a y axis. `yWidth` is the gutter in CSS pixels, and a title adds 20 px. `yBorder` draws a line on the edge of the plot and a mark of 4 px at each tick, and moves the labels 4 px out. `yColor` is a CSS colour or a function that gives a canvas style. It paints the labels, the line and the marks. |

The four edits are about 3.4 KB with their comments. The test `tests/frontend/fe83_series_options.test.mjs` checks the canvas calls of the engine against the measure of Grafana (16.10a).

The copy of `app_chart_core.js` has these marked edits. Each one follows the look of Grafana (9.19, 9.25):

1. The axis text is 12 px, with weight 400 and the full text colour.
2. The plot has a margin of 8 px at the top and at the right. A band of 21 px is under it. The gutter at the left is the widest y label plus 9 px. A y label ends 5 px before the plot. With no y axis on either side, the plot has no margin at the top (E27).
3. The axes have no tick marks, no base line and no zero line, unless the option `yBorder` of E46 is set. The x axis has one row of labels.
4. A dense line goes through the first, lowest, highest and last value of each pixel column. It is one connected line. A value between two `null` values is a mark of the width of the line.
5. The tooltip card sits 10 px to the right of the cursor and 10 px under it. It flips at an edge of the plot.
6. The engine has no "Values" or "List" button (removal E41). The mode is the option of the document.
7. The zone of the chart. The option gives `utc`, `browser` or an IANA name. The x axis, the cursor time and the head of the tooltip use this zone, with the patterns of 9.19. The function `formatTime` of `zone.js` makes the texts.
8. The thresholds of the plot. The option holds the steps, the mode, the style and the colours. A hook draws the lines and the bands before the series (9.19). The hook draws nothing for the style `off`.
9. The legend and the tooltip. The options give the sort of the legend table (9.19) and the sort of the rows of a `multi` tooltip. They also remove the rows that equal 0. The series flags of `hide_from` (9.19) hide a series from the plot, the legend or the tooltip.

10. The bars. New options set the fill opacity of a bar and a line around it. They also set the maximum bar width and the share of a bar in its group. One more option is a function that draws the value of each bar. A time axis draws the bars next to the range, cut by the plot. The option `xGrid: false` draws no vertical grid line.
11. The titles of the axes. The title of the x axis is under the labels, and it takes 20 px. The title of the y axis is turned, at the left, and it takes 28 px. A host can give the ticks of a number axis.
12. The x ticks. Two ticks are the width of a label plus 11.5 px apart. The steps are those of Grafana, and the label of a range over 24 hours has the date (9.19).
13. The second y scale. The option `scales` maps the id of a series to `y` or `y2`. The option `y2` holds the options of the right scale (`yAxis`, `yBounds`, `ySoft`, `yTickFormat`, `yTitle`, `yScale` and `shown`). The plot draws a right axis with its own range, ticks and gutter. The grid and the cursor follow the left scale. The range of a scale takes the hard ends and the soft ends of 9.19.
14. The style of one series. A series can hold `type`, `width`, `fill`, `dash`, `dashCap`, `points`, `pointSize` and `negate`. The option `drawOrder` = `series` makes the engine draw the bars and the lines of one chart in field order. The engine draws a dash with `setLineDash` in device pixels (E45), and `dashCap` is the line cap (butt for `dash`, round for `dot`). A series with `negate` draws `-v`, and the range of its scale follows the drawn values.
15. The titles of the axes. Edit 11 gives a title to the left axis. This edit gives a title to the right axis and turns it by 90 degrees. The soft ends of a scale enter the data range of the engine before its rule. A time series applies them in its hook with the data ends of E30 (9.19).

The lock file is `web/js/vendor/chdash/chdash.lock.json`:

```json
{
  "upstream": "ClickHouse Dash",
  "tag": "v1.3.1",
  "commit": "e970694e5e0b17b07cf187f3128af93fa1a82b76",
  "licence": "MIT",
  "patch": "vendor/chdash.patch",
  "files": [
    { "path": "js/vendor/chdash/app_chart_core.js", "origin": "copy",
      "upstream_path": "src/static/app_chart_core.js",
      "upstream_sha256": "<64 hexadecimal characters>", "sha256": "<64 hexadecimal characters>" },
    { "path": "js/vendor/chdash/app_wasm.js", "origin": "hune", "sha256": "<64 hexadecimal characters>" }
  ]
}
```

- `origin = "copy"`: `sha256` is the hash of the file in `web/`. It equals `upstream_sha256` when no edit exists.
- `origin = "hune"`: a shim module.
- The CI (16) checks: each file of the two `vendor/chdash/` directories is in the lock, and each hash is correct. A copy
  whose `sha256` differs from `upstream_sha256` holds `// hune:` comments. Each import of a copied file names a file of
  the lock. The reverse of `vendor/chdash.patch`, applied to the copies, gives the upstream hashes of the lock.
- `vendor/chdash.patch` is the readable diff of all the marked edits against the upstream files. It is outside `web/`,
  because the build refuses a file of `web/` with an extension that 9.3 does not list.
- `web/js/vendor/chdash/LICENSE` holds the MIT notice of ClickHouse Dash. The `LICENSE-THIRD-PARTY` file of the release
  lists it (17).
- The tool `tools/update-chdash.sh` prints the differences between the copy and a ClickHouse Dash checkout at another
  commit (17). No tool of Hune changes the copy by itself.
- An update copies the new upstream files and applies `vendor/chdash.patch` again with `patch -p1 -d web`. Then
  `tools/check-vendor-lock.sh --chdash-update <checkout>` writes the patch and the hashes of the lock again.

### 9.25 Themes

- The page has a light theme and a dark theme. The colour tokens of the chrome are in the copied `00-tokens.css`. `css/tokens.css` loads after it. It sets the type, the geometry, the roles that only Hune has and the colours of the data. The attribute `data-theme` of `<html>` is `light` or `dark`. The attribute `data-theme-choice` holds the choice of the viewer.
- The choice is in `localStorage` key `hune.theme`: `system` (the default), `light` or `dark`. `system` follows `prefers-color-scheme` and changes when it changes. Another tab that changes the key changes the theme of this page (the `storage` event).
- `boot_theme.js` is a classic script in `<head>`. It sets `data-theme` before the first paint, so the page does not
  change colour after it shows. It reads `localStorage` once, and it does not wait for any request. The shell holds its first paint (9.6): the first frame has the final theme, also on a reload with a cold cache.
- The theme button is in the header (9.6), on every page: the list, the dashboard, the editor and the pages of an error. Its size and its height are the same on each page. On the dashboard page the button "More actions" (9.9) follows it. Its distance from the right edge is then larger by the width of that button and one gap.
- The button is one icon button of 36 px by 32 px (32 px by 32 px at 480 px or less). It shows the icon of the choice: a screen for "System", a moon for "Dark" and a sun for "Light". The accessible name and the tooltip are `Theme: System`, `Theme: Dark` or `Theme: Light`. The three icons are in the button, and the CSS shows the one of `data-theme-choice`. Thus the first paint has the right icon.
- A click on the button sets the next choice, in this order: "System", "Dark", "Light", then "System" again. A stored value that is not a choice counts as "System", and the next one is "Dark". The function `nextChoice` gives the next choice.
- The button opens no menu and no popup. It has no `aria-haspopup`, no `aria-expanded` and no options. The keys Enter and Space click it, and the focus stays on it. This differs from ClickHouse Dash on purpose: Hune has one button and no menu.
- The button has no motion of its own. The change of the choice has the colour transition of the page only, and `prefers-reduced-motion: reduce` has no effect on the button.
- Each page has the same button, with the same `data-testid` (9.29). A change of the choice sends the event `hune-theme` on `document`. The pages that draw on a canvas draw again.
- The chart engine and the pie chart draw again when the theme changes. The named colours (9.16) have the same names in both themes. The series palette has one hex for each theme (9.16). `text` follows the theme.

Fonts and type:

- Hune ships the font Inter 4.1 as one variable file, `fonts/inter/InterVariable.woff2`, with its licence (SIL Open Font License 1.1). `css/fonts.css` has the font face, with the weights 100 to 900 and `font-display: block`. The text then never shows in the system font and moves after it. The shell preloads the file. The stack is Inter, then the system fonts.
- The body text is 14 px, with a line height of 22 px and `letter-spacing` 0.0107 em. The type scale has 12 px (the smallest text), 14 px and 16 px. The 16 px size is for the titles of sections, dialogs and pages. The weights are 400, 500 and 600.
- `font-optical-sizing` is `none`. Else the variable font draws large text with its narrow display cut. The digits of a stat are then 7 % narrower than in Grafana.
- The page waits for the font before its first render (9.6), and it holds its first paint until the font is ready. Thus the text of the canvas and the measures of text use Inter, and no text moves when the font arrives.

Colours of the chrome:

- The chrome has a calm look, in the light and the dark theme. The chrome is the header, the menus, the dialogs, the list page and the editor. It is also the forms, the frame of a table and the pages of an error.
- The base is the tokens of the copied `00-tokens.css`. The file `css/look.css` loads after `css/tokens.css`. It sets again the tokens of the look, in the same four contexts as the copied file.
- These tokens are the surfaces, the text, the lines, the accent, the buttons, the fields and the selected fill. The file also adds the tokens of the cards and of the tag chips. `css/tokens.css` does not set them. It names them for the roles that only Hune has (for example `--surface-menu` is `--panel`).
- `css/look.css` leaves three groups to the copied file: the edge of a field (`--control-border`), the focus ring and the status colours. The edge of a field has 3:1 on each surface (9.26). A field is a little less quiet than the mockup because of this rule.
- The shared components use the shape tokens of `css/look.css`: `--r-control` (8 px) for a button, a field, a menu, a dialog and a segmented control, and `--r-card` (10 px) for a card. A button of a joined group keeps its own corners.
- The data of the panels keep the values measured on Grafana 12.4.2. These are the thresholds, the colours of a stat and a gauge, the named colours of 9.16 and the continuous modes.
- The series palette is the exception. The mode `palette` draws `--qchart-1` to `--qchart-18` of the copied tokens (9.16). It is the one series palette.
- The link blue `--link` and the text on a coloured stat `--text-strong` also keep the Grafana values.

| Role | Token | Dark | Light |
|---|---|---|---|
| Page | `--bg` | `#0e1116` | `#f6f7f9` |
| Panel, header, menu, dialog, card | `--panel` | `#161b22` | `#ffffff` |
| Raised surface | `--raised` | `#1d232c` | `#f1f3f6` |
| Field and code area | `--input-bg` | `#1d232c` | `#f1f3f6` |
| Text | `--text` | `#e6e9ee` | `#14181f` |
| Muted text | `--muted` | `#98a2b3` | `#5d6675` |
| Border and line | `--border` | `#262d38` | `#e1e5eb` |
| Strong border | `--border-strong` | `rgba(255, 255, 255, 0.14)` | `rgba(15, 18, 25, 0.18)` |
| Hover tint | `--hover-tint` | `rgba(255, 255, 255, 0.06)` | `rgba(15, 18, 25, 0.05)` |
| Hover of a row or a menu item | `--row-hover` | `rgba(53, 111, 230, 0.08)` | `rgba(37, 88, 217, 0.06)` |
| Selected fill and edge | `--selected-bg`, `--selected-edge` | `#1c2540` and `#6c8cff` | `#e8eefc` and `#2f5bd8` |
| Primary button, and its hover | `--accent-fill`, `--accent-fill-hover` | `#6c8cff` and `#8199ff` | `#2f5bd8` and `#264bb8` |
| Text on a primary button | `--on-fill` | `#0b1020` | `#ffffff` |
| Accent text | `--accent-text` | `#8ba5ff` | `#2f5bd8` |
| Button, and its hover | `--button-bg`, `--button-bg-hover` | `#161b22` and `#1d232c` | `#ffffff` and `#f1f3f6` |
| Border of a button | `--button-border` | `#262d38` | `#e1e5eb` |
| Border of a control | `--control-border` | `rgba(255, 255, 255, 0.4)` | `rgba(15, 18, 25, 0.54)` |
| Focus ring | `--focus-ring-color` | `#3b82f6` | `#2563eb` |
| Error text, and the fill of a destructive button | `--danger`, `--danger-fill` | `#f87171` and `#dc2626` | `#b91c1c` and `#dc2626` |
| Text on a destructive button | `--on-danger` | `#ffffff` | `#ffffff` |
| Warning text | `--warning` | `#fbbf24` | `#9a5b00` |
| Success text | `--success` | `#34d399` | `#137333` |
| Shadow of a card | `--shadow-card` | `0 1px 2px rgba(0, 0, 0, 0.4)` and `0 6px 18px rgba(0, 0, 0, 0.25)` | `0 1px 2px rgba(15, 23, 42, 0.06)` and `0 4px 14px rgba(15, 23, 42, 0.05)` |
| Shadow of an overlay | `--shadow-overlay` | `0 8px 24px rgba(0, 0, 0, 0.45)` and a ring of 1 px | `0 8px 24px rgba(15, 18, 25, 0.12)` and a ring of 1 px |
| Ink of a tag chip: the base and the share of the tag colour | `--tag-ink-base`, `--tag-ink-share` | `#ffffff` and 40 % | `#000000` and 70 % |

The fill of a tag chip is 32 % of the colour of the tag, and its edge is 65 % (`--tag-fill-share`, `--tag-edge-share`). The 29 colours of the tags give 4.5:1 or more for the text on the fill, on `--panel`, `--bg`, `--raised` and `--selected-bg`, in both themes. A unit test checks it.

The primary button is light in the dark theme, so its text is dark (`--on-fill`). The white line of the logo has 3:1 or more on `--accent-fill` in both themes. The favicon and the images of the site use the fixed accent `#2f5bd8`, which has 4.5:1 with the white line.

- Hune keeps the states of ClickHouse Dash. The hover of a button is `--button-bg-hover`. The selected state is `--selected-bg` with `--selected-edge`. The disabled state is a lower opacity. The focus ring is 2 px of `--focus-ring-color`.
- The copied theme meets 9.26 with one exception. The frame of the dark editor has 2.0:1. Hune uses the border of a control (`--control-border`) for it, so it has 3:1 in both themes.
- The border of a button has less than 3:1. The label of the button names it (9.26).
- A menu, a popup and a tooltip have the panel colour, a strong border and the shadow of an overlay.

- The header is opaque. It has the colour of the panel and a line of 1 px below it, in the border colour.

### 9.26 Accessibility

- The shell, the list page, the chrome of the dashboard page and the editor meet WCAG 2.2 level AA. The chrome is the
  header, the pickers, the variables bar, the panel frames, the menus and the dialogs.
- Each control works with the keyboard. The focus is visible, and the tab order follows the page. `Escape` closes a menu
  or a dialog and puts the focus back on its control.
- The frame of a placed control (9.10) has `role="group"`. Its `aria-label` is the `title` of the placement, else the label or the name of its variable. The control keeps its own accessible name: `<label>: <selected text>` for a select, and `aria-labelledby` the chip for a text field. With `hide = "label"`, the `aria-label` stays.
- The Tab order follows the DOM order, which is the order of the grid: `y`, then `x`, then id (9.8). In the flow it is the same order. A control in a collapsed section is not in the Tab order (the section body is hidden).
- The keys of the popup (9.10) work in the grid. After `Escape`, the focus returns to the button of the control. The button has `aria-controls` only while the list is open. The focus ring is visible and has 3:1 contrast on the frame.
- A variable can move from the bar to the grid, or back, after an edit. A control that had the focus keeps it after the move.
- In edit mode, `Alt` with an arrow key moves the frame of a placed control. `Alt` with `Shift` and an arrow key resizes it (10.5a), as for a panel.
- Each chart has `role="img"` and an `aria-label` of the form "<panel title>, <type> chart". The table of "View data" is
  its text alternative (9.11). The keyboard cursor of the engine stays.
- A table has `<th scope="col">` header cells and `aria-sort`.
- Colour is never the only sign. A diff line has `+` or `-`, and an error has an icon and a text. A cell with a
  threshold colour shows its value.
- Each text token of both themes has a contrast of 4.5:1 or more with its surface. Each graphic token has 3:1 or more. Where a Grafana value fails, Hune uses the nearest value that passes. An example is the border of a control (9.25).
- A mark of a chart in the light theme has 3:1 on the panel and on the page (9.16). A pie slice has 3:1 (9.19). Each colour of the series palette has 3:1 with the panel and the page in both themes (9.16).
- A coloured value text of a stat has 4.5:1 with the panel (9.19). A large text of 24 px or more has 3:1. A coloured text of a table cell has 4.5:1 (9.20).
- The ink on a stat tile with a background colour has 3:1 or more (9.19). The ink on a table cell with a background colour has 4.5:1 or more (9.20). Where Grafana has less, Hune uses the ink with the better contrast.
- Notices use `aria-live="polite"`. An error of a form field has `aria-describedby`.
- No animation is longer than 200 ms. With `prefers-reduced-motion: reduce`, the page has no animation. The pulse of the loader of a panel and the light of its preview (9.11) are the only exceptions. The pulse repeats each 1 s, and the light each 2 s. They stop when the panel has its result.
- With `prefers-reduced-motion: reduce`, the loader has no pulse and the preview has no light. The dots show at 60 % of their opacity and the shapes are still. They are decoration, so they have `aria-hidden="true"` and no contrast rule.
- A copy of a repeat, of a panel or of a section, has the accessible name `<title>` after the replacement of the variables (9.8c). When this title has no placeholder of the repeat variable, the name is the title, a comma and the option text of the copy. Thus a person with a screen reader tells the copies apart.
- The copies are in the DOM order of the grid: the displayed `y`, then `x`, then the number n (9.8). The Tab order follows this order.
- The messages of the states `repeat-empty` and `repeat-limit` have `role="note"`. The page does not announce them, because the person changed the selection and sees the result.
- The control of the variable keeps the focus after a change (9.8c). The copies that the change makes are not announced one by one.
- The field of an open variable follows the pattern "editable combobox with list autocomplete" of `WAI-ARIA Authoring Practices`. The DOM focus stays in the field. `aria-activedescendant` names the active row. The row has an `id` and the class `listbox__item--active`. The indicator of the active row has 3:1 contrast with the popup in both themes.
- The error of a typed value has `role="alert"`, and the field has `aria-describedby` to it. The row `Use` is an option with the name `Use "<text>"`.
- The control with no option has the name `<label>: no options`. The notice of a replaced value is in the area of notices, which has `aria-live="polite"`.
- The text of a typed value, of the notice and of the editor preview is a text node, never HTML (T79). A value such as `<img src=x onerror=alert(1)>` shows as text.
- The body of a text panel is a region (`role="region"`) with the name of the panel. It is in the Tab order (`tabindex="0"`), so a person can scroll it with the keyboard. A link is in the Tab order and has the focus ring of 3:1.
- The headings of the text are `h1` to `h6` as the author wrote them. The title of the panel is not a heading of the text.
- A link has an underline. The colour of a link has 4.5:1 with the panel in both themes. The text, the code and the cells have 4.5:1. The lines of a table have the token `--grid-line`, as the table panel has (9.20), and no contrast rule.
- The messages of the state `text-limit` have `role="note"`.
- The canvas of a `state_timeline` and of a `status_history` has `role="img"`, `tabindex="0"` and a description with one sentence for each row (9.19c). The arrow keys move a cursor over the boxes, and `Escape` hides the card.
- The ink of the text in a segment has 4.5:1 with the fill over the panel. Where Grafana has less, Hune uses the ink with the better contrast (19.20d). A state is never the only sign: the text of the state is in the tooltip, the legend and the description.
- The messages of the state `timeline-limit` have `role="note"`.
- The canvas of a `heatmap` has `role="img"`, `tabindex="0"` and a description (9.19e). The arrow keys move a cursor over the cells, and `Escape` hides the card.
- The colour of a heatmap cell is data. The lowest colours of a scheme can have less than 3:1 with the panel, as in Grafana. The count of each cell is in the card, in the description and in the table "View data". The page never uses the colour as the only sign of a count.
- The message of the state `bucket-limit` of a `heatmap` has `role="note"`. The note of the values that a log scale skips has `role="note"` too.
- A `histogram` has the chart of the engine with `role="img"` and a description (9.19f). The messages of the state `bucket-limit` have `role="note"`.
- A `logs` panel is a list (`role="list"`) with one tab stop. Its lines are buttons with `aria-expanded` when the details row is on (9.19h, section 5). The note of a cap has `role="note"`.
- The root of the gauges of a `gauge` has `role="img"` and a hidden description with each name and value (9.19g). A value text in a colour has 4.5:1 with the panel, or 3:1 from 24 px, as the value of a `stat`.
- The marks of the annotations have a text alternative: the second table of "View data" (9.11). A mark is never the only sign, because the card, the keys and the table give its time, its title, its text and its tags.
- The element `annot__keys` of a time series is in the Tab order. It has a name and a count. The arrow keys read the marks and the card shows what they read. Grafana has no keyboard access to its marks.
- The card has 4.5:1 for its text with its surface in both themes. The line and the triangle have the colour of the author. A colour with less than 3:1 on the panel is the choice of the author, and the validation does not check it. The text of the mark is in the card and in the table.
- The note of an annotation (9.19d, rule 16) has an icon and a text. The page does not announce it.
- The SQL area keeps the text area as its only accessible text. The folded view, the band and the pair marks are `aria-hidden`. A screen reader reads the full text, also when blocks are folded (10.3e).
- The fold markers are buttons with a name and `aria-expanded`. They are not in the Tab order (`tabindex="-1"`), because a query can have 2,000 of them. The keys of 10.3g and the button "Unfold all" do the same work.
- The status line of the SQL area has `aria-live="polite"`. It gives each result of the format and of a fold in one short sentence (10.3g).
- The band of the current block has 4.5:1 for each token colour in both themes, and its bar has 3:1. The pair mark has 3:1 with the surface and changes no text colour (10.3f). Colour is not the only sign: the status line names an unmatched bracket.
- The badge of a partial result has `role="img"` and `tabindex="0"`. Its `aria-label` is "Error:" and then one line for each failed query, in the order of the queries. The tooltip shows the same text. A person with a keyboard can read it.
- The select of a table with several queries (9.22a, rule 9) is a native `<select>` with the label "Query". It is in the Tab order, after the table. The page does not announce a change, because the table shows the choice.
- A panel with all queries hidden shows "No data" as text. The panel is not in a state that a person must repair.
- The chips of the editor (10.3) are buttons with `aria-pressed` for the selection and for the hide state. Each one has an accessible name with its ref and its state.
- The browser tests run axe-core in both themes on the list page and on a dashboard with each panel type. They also
  run it on the panel editor, the history and the conflict screen. They accept zero violations of the rules of WCAG
  2.0, 2.1 and 2.2, levels A and AA (16).
- A choice of an option form is a `<fieldset>` with a `<legend>` and one native radio input for each item. The arrow keys move the choice, and Tab enters the group once. A list of check boxes is a `<fieldset>` with a `<legend>`. A slider and its number field are one group with the name of the option (10.3h).
- A row that a rule hides has the attribute `hidden`. It is not in the Tab order and not in the tree of the screen reader. The panel editor tells the change in one polite status line (`options-status`): "Shown: <names>. Hidden: <names>." (10.3h, rule 9).
- Each control of the options pane has an accessible name that equals its label in the table of 10.3h. The test B203 reads the names from the accessibility tree of the browser. The page does not use axe-core for this check, because axe-core needs the network.
- The target of a control of the options pane is 24 by 24 CSS pixels or more, at 390 px too (WCAG 2.5.8). The text of a label has 4.5:1 with the pane. The border of a control and the focus ring have 3:1, in both themes (B204).

### 9.27 WebAssembly policy

- v1 has no WebAssembly kernel. The CSP has no `'wasm-unsafe-eval'` (9.4). The shim `app_wasm.js` has no kernel.
- A kernel comes only after a measure on the real dashboards. The measure MUST show a compute task above 50 ms in the
  browser. The first candidate is the numeric sort of a long table.
- The kernel then follows the rules of ClickHouse Dash `docs/wasm.md`. It has the same interface as the JavaScript
  code. The JavaScript code stays the reference and the fallback, with identical results.
- The kernel loads only on demand, and its failure never breaks a page. A short input stays on the JavaScript path. A
  long call runs in a Web Worker. The binary is committed with a lock, and the tests compare both paths.
- The release with the first kernel adds `'wasm-unsafe-eval'` to `script-src`, with a change that the project reviews.

### 9.28 Browser support

- Hune supports the current and the previous major release of Chrome, Edge, Firefox and Safari on desktop. A narrow screen gets the stacked layout of 9.8.
- The page uses ES2022 modules, dynamic `import()`, `AbortController`, `IntersectionObserver`, `ResizeObserver`,
  `structuredClone`, `crypto.getRandomValues`, `Intl.DateTimeFormat` with `timeZone`, CSS grid and CSS custom
  properties. It loads no polyfill.
- The page does not use `crypto.randomUUID`, because it exists only in a secure context. It makes UUIDs with
  `crypto.getRandomValues` (10.9).
- A browser that does not run ES modules shows the text of `<noscript>` or an empty page with the title. Hune does not
  test it.

### 9.29 Browser test hooks

The hooks are in each build. They show only what the page shows. Hune has no test build and no test API in
JavaScript.

| Hook | Element |
|---|---|
| `data-testid="app-header"` | The header. |
| `data-testid="time-control"`, `"time-picker"`, `"refresh-picker"` | The time control (9.9) and its two groups: the time group and the refresh group. |
| `data-testid="time-picker-button"`, `"time-back"`, `"time-forward"`, `"zoom-out"`, `"refresh-now"`, `"refresh-interval"`, `"refresh-select"` | The buttons of the time group and of the refresh group (9.9). |
| `data-testid="dashboard-menu"`, `"view-back"` | The button "More actions" and the button "Back to dashboard" (9.9, 9.8). |
| `data-testid="history-button"`, `"export-button"`, `"delete-button"`, `"menu-time-back"`, `"menu-time-forward"`, `"menu-zoom-out"` | The items of the menu "More actions" (9.9). The items `menu-*` show up to 900 px. |
| `data-testid="header-actions"` | The controls part of the header (9.6). |
| `data-testid="app-version"` | The version line beside the product name (9.6). |
| `data-testid="theme-toggle"` | The theme button (9.25). It has no menu and no option. |
| `data-testid="variables-bar"`, `"variable-<name>"` | The bar and each control. A control is in the bar or in the frame of its placement (9.10): one place at a time. |
| `data-testid="variable-<name>-use"` | The row `Use "<text>"` of the popup of an open variable (9.10). |
| `data-notice="variable"` on a notice | The notice of a value that a closed variable dropped (9.10). |
| `data-testid="variable-form-allow-custom"`, `"variable-sql-examples"`, `"variable-sql-example-<n>"`, `"variable-form-preview"`, `"variable-preview-status"`, `"variable-preview-list"` | The switch of an open variable, the menu of SQL examples with its items from 0, the button "Show options", its status line and its list (10.3b). |
| `data-testid="section-<index>"` | Each section, from 0. |
| `data-testid="panel-<key>"`, `"panel-<key>-menu"`, `"panel-<key>-error"` | Each panel frame, its menu and its error. `<key>` is the key of the instance: the id for a panel and for the first copy, and `<id>-r<n>` for the copy n (9.8c). The frame is the element whose inner width gives `max_data_points` (9.8). The other hooks of a panel (`panel-<id>-inspect` and the like) use `<key>` in place of `<id>` for a copy. |
| `data-failed` on `data-testid="panel-<key>"` | The refs of the queries that failed, separated by `,`. Absent when none failed (9.8d). |
| `data-testid="panel-<key>-frames"` | The select of the frames of a table with several queries (9.22a). |
| `data-testid="panel-<key>-status"` | The badge of a partial result and of an error (9.8d). |
| `data-testid="inspect-<id>-sql-<ref>"`, `"inspect-<id>-none-<ref>"`, `"inspect-<id>-meta-<ref>"`, `"inspect-copy-<ref>"` | The blocks of "Inspect" of a panel with several queries (9.11). |
| `data-testid="query-chip-<ref>"`, `"query-add"`, `"query-hide-<ref>"`, `"query-delete-<ref>"`, `"query-ref"`, `"query-rename-dialog"` | The chips and the controls of the editor (10.3). |
| `data-testid="panel-<id>-inspect"`, `"inspect-<id>"`, `"inspect-<id>-sql"`, `"inspect-<id>-none"`, `"inspect-copy"` | The menu item Inspect, its dialog, the SQL text, the text that says there is no SQL, and the button "Copy SQL" (9.11). |
| `data-testid="table-<id>"` | The table of a table panel. |
| `.cell-inspect`, `data-testid="inspect-value"`, `"inspect-value-text"` | The button "Inspect value" of a cell, the drawer and its text block (9.20). |
| `data-testid="text-<id>"`, `"text-<id>-note"` | The root of the text of a text panel, and the message of the state `text-limit` (9.19b). |
| `data-testid="text-content"`, `"text-count"`, `"text-error"`, `"text-no-query"`, `"text-help"` | The text area of the content in the panel editor, the line "<n> of 65,536 bytes", the error under it, the note of the tab "Query" and the help text (10.3). |
| `data-testid="sql-format"`, `"sql-unfold-all"`, `"sql-undo-format"`, `"sql-status"`, `"sql-info"`, `"sql-keys"` | The buttons "Format", "Unfold all" and "Undo format" of the SQL area, its status line, the text of its block and its list of keys (10.3g). |
| `data-testid="sql-folded"`, `"sql-fold"`, `"sql-chip"`, `"sql-block"`, `"sql-pair"` | The folded view, a fold marker (with `data-from`, the offset of the first folded character), the chip of a fold, the band of the current block (with `data-type`) and a mark of the bracket pair (10.3e, 10.3f). |
| `data-testid="add-text"`, `"section-<index>-add-text"`, `"type-change-dialog"` | The item "Add text panel" of the menu of "Add", the item "Add text panel here" of the menu "Section actions", and the dialog of a change of the type to or from `text` (10.2, 10.3, 10.6). |
| `data-testid="edit-toggle"`, `"edit-bar"`, `"save-button"`, `"discard-button"`, `"panel-editor"`, `"json-editor"` | The switch "Edit" (`role="switch"`), the group of "Add", "Save" and "Revert", "Save", "Revert" and the editor (10.2 to 10.4). |
| `data-testid="add-group"`, `"add-button"`, `"add-menu"`, `"add-menu-list"` | The split button "Add", its main part (it adds a panel), its menu part and its menu (10.2). |
| `data-testid="add-panel"`, `"add-section"`, `"add-variable"`, `"add-variable-list"`, `"add-variable-<name>"` | The items of the menu of "Add", the menu of the choice of a variable and its items (10.2). |
| `data-testid="json-tab"`, `"dash-settings"`, `"discard-menu"` | The items of the edit mode in the menu "More actions" (10.2). The item `discard-menu` shows up to 640 px. |
| `data-testid="conflict-screen"`, `"history-list"`, `"diff-view"` | 10.10, 10.11, 10.15. |
| `data-testid="notice"` | Each notice. |
| `data-state` on a panel frame | `idle`, `queued`, `loading`, `ready`, `empty`, `error`, `text-limit`, `timeline-limit`, `bucket-limit`, `repeat-empty` or `repeat-limit` (9.11). |
| `data-repeat-n`, `data-repeat-count` on a panel frame | The number n of the copy and the number of copies of the repeat (9.11). |
| `data-testid="section-<index>-r<n>"` | The copy n of a repeated section, with n from 2 (9.8c). The copy 1 is `section-<index>`. |
| `data-testid="repeat-band-<id>"` | The band of a panel with `direction` = `h` (9.8c). It holds the copies of that panel. |
| `data-testid="repeat-note-<id>"`, `"section-<index>-note"` | The message of a panel and of a section in the state `repeat-empty` or `repeat-limit` (9.8c). |
| `data-testid="repeat-variable"`, `"repeat-direction"`, `"repeat-max-per-row"`, `"repeat-hint"`, `"repeat-count"` | The fields of the group "Repeat options" of the panel editor, the hint and the count of copies (10.3). |
| `data-option` on the root of a control of the options pane | The path of the member that the control writes, from the panel, for example `options.draw` or `fields.defaults.display_name`. A form with a list or an editor (the thresholds, the mappings, the links, the overrides, the transforms) has one `data-option` on its root. A control inside it has the member of that root as its prefix. Each control of the pane is inside an element with `data-option` (10.3h, rule 12). |
| `data-testid="opt-<id>"` | The control of the row `<id>` of the table of its type (10.3h). The tables give each `<id>`. The rows of existing controls keep their hooks. |
| `data-group` on the head of an option group | The key of the group, for example `style`, `legend`, `tooltip`. |
| `data-hidden-by-rule` on a row | `true` while a rule of display hides the row. The row also has the attribute `hidden`. |
| `data-testid="options-status"` | The polite status line of the options pane (10.3h, rule 9). |
| `data-draft-writes` on `.peditor__options` | The number of writes that the options pane made to the draft since the pane was drawn. The test B205 reads it. |
| `data-testid="section-<index>-repeat"`, `"section-<index>-repeat-dialog"`, `"section-<index>-repeat-variable"`, `"section-<index>-repeat-badge"` | The item "Repeat by a variable" of the section menu, its dialog, its select and the badge in the header of the section (10.6). |
| `data-testid="placement-<name>"`, `"placement-<name>-menu"`, `"placement-<name>-menu-popup"` | The frame of a placed control, its menu button and its menu (edit mode, 9.11). |
| `data-testid="placement-<name>-edit"`, `"placement-<name>-remove"` | The entries "Edit the variable" and "Put back in the top bar" of that menu. |
| `data-testid="variable-form-placed"`, `"variable-form-placement-section"`, `"variable-form-placement-title"`, `"variable-form-placement-x"`, `"-y"`, `"-w"`, `"-h"` | The switch "Place in the grid" and its fields in the form of a variable (10.3b). |
| `data-thresholds` on the plot element `.panel__plot` | JSON that the page writes after each draw of a panel with a canvas chart. It holds the threshold style, the plot box, the range of the y axis, the flag `dashed`, and the lines and the bands. Each line has its value, its position in px, its CSS colour and its alpha. Each band has its range, its top and bottom in px, its CSS colour and its alpha. The hook is read-only. The page sets it also when it draws no line and no band (9.19). |
| `data-axes` on the plot element `.panel__plot` | JSON that the page writes after each draw of a time series or a bar chart. It holds the scales in order, left first. Each scale has `side`, `min`, `max`, `label`, `shown` and the names of its series (9.19). |
| `data-series-style` on `.panel__plot` | JSON that the page writes after each draw of a time series. For each series it holds `name`, `draw`, `lineWidth`, `fillOpacity`, `dash`, `showPoints`, `pointSize` and `negate` (9.19). |
| `data-annotations` on `.panel__plot` | JSON that the page writes after each draw of the marks of a time series. It holds `n` (the marks that apply), `drawn` (the marks in the visible range), `cut` (`true` when the cap broke), `plot` (`[left, top, width, height]`) and `marks`. Each item of `marks` is `[x, x_end, kind, color, index]`: the x in CSS pixels, `x_end` equal to `x` for a point, `kind` 0 for a point and 1 for a region, the colour as the page draws it, and the index of the annotation. The array holds 200 items at most. |
| `data-testid="annot-<id>-card"`, `"annot-<id>-keys"`, `"annot-<id>-status"` | The card, the element of the keys and the hidden status text of the time series `<id>` (9.19d). `<id>` is the key of the panel instance (9.8c). |
| `data-testid="panel-<id>-annot-note"` | The note of the title bar (9.19d, rule 16). |
| `data-testid="view-data-annotations"` | The second table of "View data" (9.11). |
| `data-timeline` on `.panel__plot` | JSON that the page writes after each draw of a `state_timeline` or a `status_history`. It holds `kind`, `box` (`[left, top, width, height]`), `rows` (`name`, `y`, `h`, `n`), `segments` (`[row, x, w, state]` for each box), `states` (`[text, color]`), `texts` (`[text, x, y, color]`), `ticks` (`[time, x]` for each grid line), `labels` (`[text, x, left, right, top]` for each label of the time axis) and `cut`. The arrays `segments` and `texts` hold 2,000 items at most, and `cut` is `true` when the page cut them. The lengths are in CSS pixels. |
| `data-testid="timeline-<id>"`, `"timeline-<id>-note"`, `"timeline-<id>-legend"`, `"timeline-<id>-tip"`, `"timeline-<id>-desc"` | The root of the picture, the message of the state `timeline-limit`, the legend, the card and the hidden description (9.19c). |
| `data-heatmap` on `.panel__plot` | JSON that the page writes after each draw of a `heatmap`. It holds `shape` (`bucketed` or `raw`), `cols`, `rows`, `drawn` (the cells that the engine drew in the range of the time), `sx`, `sy` (the sizes, or `null`), `scale` (`linear`, `log2`, `log10` or `ordinal`), `min`, `max`, `steps`, `palette` (the first and the last colour), `legend` (`[first count, last count]` or `null`) and `cells` (`[column, row, count, level]` for the first 2,000 cells that have a count to draw). `level` is the index in the palette. `cut` is `true` when the page cut the list. |
| `data-testid="heatmap-<id>"`, `"heatmap-<id>-note"`, `"heatmap-<id>-legend"`, `"heatmap-<id>-tip"`, `"heatmap-<id>-desc"`, `"heatmap-<id>-skip"` | The root of the picture, the message of the state `bucket-limit`, the colour bar, the card, the hidden description and the note of the values that a log scale skipped (9.19e). The dialog "View data" has the same note as `view-data-<id>-skip`. |
| `data-histogram` on `.panel__plot` | JSON that the page writes after each draw of a `histogram`. It holds `shape`, `size`, `lo`, `buckets`, `series` (`name`, `max`, `at`) and `bars` (`[series, bucket, count]` for the first 2,000 bars). `cut` is `true` when the page cut the list. |
| `data-testid="histogram-<id>"`, `"histogram-<id>-note"`, `"histogram-<id>-desc"` | The root of the picture, the message of the state `bucket-limit` and the hidden description (9.19f). |
| `data-geomap` on `.panel__plot` | JSON that the page writes after each draw of a `geomap` and after each tile result. It holds `points` (the rows with a valid position), `skipped`, `drawn` (the markers in the view), `mode` (`tiles` or `plain`), `zoom`, `lat` and `lon` (the centre), and `tiles`. `tiles` is `null` with no tile source. Else it holds `needed`, `over`, `drawn`, `requested`, `failed`, `loaded` and `peak` (the most requests in flight on the page). Above `MAX_GEO_POINTS` it holds `points` 0, `skipped` 0 and `limit`. The test ids are `geomap-<id>`, `geomap-<id>-tip`, `geomap-<id>-zoom-in`, `geomap-<id>-zoom-out`, `geomap-<id>-attribution`, `geomap-<id>-skip`, `geomap-<id>-tiles-note` and `geomap-<id>-note`. |
| `data-logs` on `.logs` | JSON that the page writes after each draw of a `logs` panel: `lines` (the lines drawn), `total` (the rows of the frame) and `cut` (`null`, `lines` or `chars`) (9.19h). |
| `data-testid="logs-<key>"`, `"logs-<key>-details"`, `"logs-<key>-shape"` | The list of the lines, each open details row, and the message of a frame with no time or no text column (9.19h). The class `logs__line` is a line, and `logs__note` is the note of a cap. |
| `data-geometry` on `.pie__canvas` | The circle of the last draw of a `piechart`: four numbers with a space between them. They are `cx`, `cy`, the outer radius and the inner radius, in CSS pixels of the canvas box. A slice under the pointer does not change them. The inner radius is 0 for a `pie` (9.19). |
| `data-gauge` on the root `gauge-<id>` | JSON that the page writes after each draw of a `gauge`: one object for each gauge with its cell (`x`, `y`, `w`, `h`), `canvasW`, `canvasH`, `titleSize`, `cx`, `cy`, `radius`, `arcW`, `markW`, `valueSize`, `labelSize`, `min`, `max`, `bands` and `text` (9.19g). |
| `data-testid="gauge-<id>"`, `"gauge-item-<id>"`, `"gauge-<id>-desc"` | The root of the gauges, each cell and the hidden description (9.19g). |
| `data-testid="dash-links"`, `"dash-link-<n>"`, `"dash-link-<n>-button"`, `"dash-link-<n>-item-<uid>"` | The group of the dashboard links, each link, its dropdown button and each dashboard item (9.8a). |
| `data-testid="panel-<id>-time-note"` | The note of the time override in the title bar (9.8b). |
| `data-notice="zone"` on a notice | The notice of a time zone that the browser does not know (9.9). |
| `data-testid="dash-settings"`, `"settings-tab-<name>"`, `"settings-links"`, `"settings-variables"` | The item "Settings" of the menu "More actions", the tabs of the settings view and its two lists (10.3b). |
| `data-testid="settings-title"`, `"settings-tags"`, `"settings-time-from"`, `"settings-time-to"`, `"settings-zone"`, `"settings-zone-name"`, `"settings-zone-warning"`, `"settings-refresh"`, `"settings-link-<n>-<member>"` | The fields of the tabs "General", "Time" and "Links" and the warning of an unknown zone (10.3b). `<member>` is the name of a member of the link. |
| `data-testid="variable-list"`, `"variable-<n>"`, `"variable-<n>-open"`, `"variable-add"`, `"variable-add-kind"`, `"variable-back"`, `"variable-form-<name>"` | The list of the variables of the settings view, a row, its button, the button "Add variable" with its select, the way back and the form of a variable (10.3b). |
| `data-testid="query-time-from"`, `"query-time-shift"`, `"query-hide-time"` | The fields "Relative time", "Time shift" and "Hide time info" of the panel editor. They are in the popover "Query options" (10.3). |
| `data-testid="query-options"`, `"query-options-pop"`, `"query-options-dot"`, `"query-options-note"` | The button "Options" of the row of the tab "Query", its popover, its dot and the text of the dot for the screen reader (10.3). |
| `data-testid="color-mode"`, `"defaults-<id>"` | The select of the colour mode, and the control of a property of `fields.defaults` (10.3a). |
| `data-testid="overrides"`, `"override-<n>"`, `"override-<n>-by"`, `"override-add"`, `"override-<n>-property-add"`, `"override-<n>-property-<id>"` | The override form (10.3a). |
| `data-testid="mappings"`, `"mapping-<n>"`, `"mapping-add"`, `"links-<scope>"`, `"link-<scope>-<n>"`, `"link-<scope>-add"`, `"transforms"`, `"transform-<n>"`, `"transform-add"` | The forms of the mappings, the links and the transforms (10.3a). `<scope>` is `defaults` or `panel`. |
| `data-testid="transform-<n>-kind"`, `"transform-<n>-<field>"` | The select of the kind of the transform `n`, and the control of each member of its kind. `<field>` is the member name with `_` as `-` (10.3a). |
| `data-hune-hold` on `<html>` | Present while the shell holds its first paint (9.6). |
| `data-hune-state` on `<html>` | `busy` while a request is in flight or queued or a panel draws, else `idle`. |
| `data-hune-view` on `<html>` | The state of the page after the view is in the page: `list`, `not-found`, `view`, `panel-view`, `edit`, `panel-edit`, `json`, `settings` or `history` (10.2a). It is absent while a view loads and on the pages of an error. |
| `data-hune-origin` on `<html>` | The kind of the origin of an editor state: `view` or `edit`. It is absent when the state has no origin (10.2a). |

The tests wait for `data-hune-state="idle"`. Playwright reads the CSP violations and the console errors with its own
listeners (16).

### 9.30 Performance budgets

- The dashboard of 45 panels shows its grid within 300 ms after the shell loads. The queries are not in this time.
- While its 45 panels draw, no task of the main thread is longer than 200 ms.
- The JavaScript of the dashboard page in view mode is 1,126 KB at most (1,153,024 bytes), before compression. The limit is the measure (1,148,036 bytes) plus 4,096 bytes, rounded up to a whole KiB. The 4,988 bytes of room are for fixes that a test finds, not for a new feature.
- The page budget is a measured limit with 4 KiB of room, rounded up to a whole KiB. A new feature raises the limit in a reviewed change. When the feature is complete, the limit goes back to the measure plus 4 KiB. A guard stops the work when the page goes above the limit minus 4,096 bytes before the feature is complete.
- These ways to stay under the limit are not permitted:
  - a shorter name;
  - a joined line;
  - a shorter or removed documentation comment;
  - a change of the measure;
  - a minifier.
- The measure counts the bytes before compression, and it stays so. With `server.compress_static = true` (9.3), the browser gets fewer bytes. On 2026-11-15, the 997,047 bytes of the measure gave 333,765 bytes with the gzip of Hune (33.5 %).
- The measure counts each module that the page can load, also with `import()`, except the editor. The modules of section 10 load only on demand.
- The module `view_state.js` and the driver of the states (10.2a) add about 21 KB.
- The repeat adds the module `repeat.js` and small changes in `runtime.js`, `grid.js`, `dashboard.js`, `view_state.js`, `url_state.js` and `panel_frame.js`. The forms of the repeat are in the editor (10.3, 10.6), which is not in the measure.
- The text panel adds the modules `markdown.js` (15,673 bytes) and `panels/text.js` (4,469 bytes). It also makes small changes in `panel_frame.js` and `runtime.js`.
- The frame imports `panels/text.js` with a static import, because the load on demand needs more code and the measure counts the module in both cases.
- The parser makes the tree of a text of 65,536 bytes in 50 ms or less on the main thread. A front unit test checks the steps, and the browser case B105 checks the time (16.10a).
- The functions of `repeat.js` for a document of 200 panels and 120 copies run in 50 ms or less on the main thread. A front unit test checks this time (16.10a).
- The state timeline and the status history add the modules `timeline_model.js` (10,904 bytes) and `panels/timeline.js` (16,303 bytes). They make small changes in `panel_frame.js`, `fields.js` and `panels/common.js`.
- The two modules have 27,207 bytes with the documentation comments. The new CSS is not in the measure.
- The feature already shares what it can. It reuses the time ticks of the engine, the display, the mappings, the colours, the ink of `cellInk`, the zone clock and the legend styles. It does not copy the 175 KB of the engine, and the engine would not draw bands.
- The measure counts `panels/timeline.js` and `timeline_model.js`, because the page can load them with `import()`. The load on demand does not lower the measure.
- A front unit test checks the cap and the time of the model for 10,000 and 100,000 samples (16.10a). The browser case B114 checks the time of the draw.
- The browser tests measure these values on a synthetic dashboard (16).
- The annotations add the modules `annotations.js` (7,142 bytes) and `panels/annotate.js` (9,162 bytes). They make small changes in `runtime.js`, `api.js`, `panel_frame.js`, `dashboard.js` and `panels/timeseries.js`. The server makes the marks (7.37), so the page does not hold the rules of the columns.
- The page does not use the options `annotations` and `regions` of the engine. These options give one colour to all annotations and write labels over the plot (9.24). They also drop dense marks with no sign, and their layer has no text for the screen reader.
- The order of the annotation jobs (9.8) needed a fix of 358 bytes in `runtime.js`.
- A front unit test checks the cap and the time of `marks` and `hitTest` for 2,000 and 10,000 marks (16.10a). The browser case B123 checks the time of the draw.
- The page does not scan a query text any more (9.10). The server gives the names in the member `names` of the answer of 6.14. `runtime.js` no longer imports `sqlscan.js`, so `sqlscan.js` (5,012 bytes), `sql_lex_pg.js` (8,989 bytes) and `vendor/chdash/app_sql_lex.js` (10,805 bytes) leave the measure: 24,806 bytes. The editor keeps them, and it loads them on demand.
- The function `readNames` (the shape check), the method `panelNames` and a few lines in `runtime.js` add 4,555 bytes with their comments. The notice of the member `names` in `dashboard.js` adds to this. The page has 930,612 bytes after the change, which is 20,251 bytes less.
- A front unit test checks that `runtime.js` and `dashboard.js` do not name `sqlscan` (16.10a). The golden test of the server checks that no name is lost (7.19).
- The tree of the query text (F42) is all under `web/js/editor/`. It loads with the editor and is not in the page measure. No module of the page imports a module of `editor/ast/`, with a quoted path or with a template string. The test `static_rules.test.mjs` checks this (16.10a).
- The parse of a text of 262,144 bytes takes 50 ms or less on the reference machine. The parse runs once, after the last input and when the browser is idle (10.3c). It never runs for each key.
- The colour layer of the SQL area draws only the rows near the visible rows (fixed). A key in a text of 246 KB costs the same as a key in a short text. The script takes about 8 ms on the reference machine. The module is `editor/sql_colour.js`. It is under `editor/` and is not in the page measure.
- The format of a text of 262,144 bytes takes 300 ms or less, and it runs only on request (10.3d). The case B176 measures both times and the longest task of the main thread.
- The several queries of a panel add the module `queries.js`. They make changes in `runtime.js`, `panel_frame.js`, `transform.js`, `names.js` and `fields.js`. `api.js` needs no change. The server runs one query for each request. The page thus holds the join and the state of the panel, and no rule of the SQL.
- The editor code of the several queries (chips, dialogs, `query_ops.js`) and the new CSS are not in the measure. The feature adds 19,497 bytes with its comments.
- The slots reuse the jobs of the annotations in `runtime.js`. A slot and an annotation share the queue, the abort code and the limit of 6 requests. One code path runs several requests for a panel.
- A front unit test checks the time of the join (16.10a). The browser case B130 checks the draw of a panel with 3 queries.
- A reduction of the page: it has 945,311 bytes, which is 4,798 bytes less than the 950,109 bytes before.
   - The routes that only the editor calls go from `api.js` to `editor/api_editor.js`. This saves 3,123 bytes.
   - Four exports that only the editor uses go into `editor/draft.js` and `editor/validators.js`. This saves 830 bytes.
   - The code that no file uses is removed (`confirmDialog`, `relativeUrl`, `track`, `STYLES`). This saves 1,156 bytes.
   - A panel type that the page cannot draw shows a fixed message in the error state. This adds 311 bytes.
   - A static test refuses these routes outside `editor/` (16.10a).
- The option forms of the data types (F43) are editor code. They are in `js/editor/forms/` and `js/editor/forms/options/`. The editor loads them on demand, so the measure above does not count them. The test `static_rules.test.mjs` already checks that no file of the measure imports a file of `js/editor/`. The estimate is 50 KB for these files (31 KB of tables, 14 KB of renderer and helpers, and 5 KB of other changes). The test FE-79 fails when the new files of the forms have more than 102,400 bytes.
- The closed or free value of a variable makes changes in `var_options.js`, `vars.js`, `runtime.js` and `dashboard.js`. It adds no module to the page. The form, the SQL examples and the preview of the options are in the editor, which is not in the measure.
- The feature adds 10,407 bytes with its comments, so the page has 955,718 bytes after it.

| Module | Bytes added | Bytes now | What it holds |
|---|---|---|---|
| `vars.js` | 5,569 | 25,689 | The combobox, the typed rows, the row `Use`, the keys, the error, "No options". |
| `var_options.js` | 3,169 | 12,327 | `typedValueError`, the rules of the selection, `droppedValues`, `replacedNotice`, `replacement`. |
| `runtime.js` | 758 | 38,806 | The option `onNotice` and the call at the load and at a reload. |
| `dashboard.js` | 911 | 46,490 | The notice with its key and its hook, and the new value in the URL. |

- The comment share of the changed modules is 15 % in `vars.js`, 29 % in `var_options.js`, 26 % in `runtime.js` and 24 % in `dashboard.js`.
- A front unit test checks the time of the selection for 10,000 values and 10,000 options (16.10a, FE-48).
- The engine patch removes code of the copied engine that Hune never calls (9.24, edits 17 to 21). With the same other modules, the measure goes from 956,700 bytes to 927,072 bytes, which is 29,628 bytes less. The removals take 11,400 bytes from `app_chart_core.js`, 7,801 bytes from `app_ui_table.js` and 7,035 bytes from `app_results_sort.js`. They also take 2,268 bytes from the shim `app_dom.js` and the 1,124 bytes of the shim `app_ui_copy.js`.
- The variable kinds `interval` and `constant` make small changes in `var_options.js` and `dashboard.js`. They add no module to the page. The text of "Auto" comes from the server, so the page holds no table of intervals. The forms are in the editor, which is not in the measure.
- The measured code is 982 bytes. The page had 955,718 bytes before it and 956,700 bytes after it.

| Module | Bytes added | Bytes now | What it holds |
|---|---|---|---|
| `var_options.js` | 888 | 13,215 | `AUTO`, the options of an `interval`, the selection, the URL and the request of a `constant`, the default as an array |
| `dashboard.js` | 94 | 46,584 | The URL value of a `constant`, which the page ignores |

- The comment share of the changed modules is 30 % in `var_options.js` and 24 % in `dashboard.js`. The modules `vars.js` and `runtime.js` have no change. The control of a `custom` variable serves the `interval` kind. The request body and the display text need no new branch.
- The engine patch (above) lowered the measure to 927,072 bytes after these changes.
- The feature adds 24,633 bytes with its comments to the 927,072 bytes of the start, so the page has 951,705 bytes.

| Module | Bytes added | Bytes now | What it holds |
|---|---|---|---|
| `transform_more.js` | 22,735 | 22,735 | The eleven kinds, the caps, the errors, the titles of the calculations |
| `transform.js` | 1,605 | 11,507 | `applyList` (the transforms on the list of frames), the refs of the frames, the option text of a partition |
| `fields.js` | 293 | 6,022 | The matcher `name` and the name that a transform set |
| `names.js` | 0 | 5,102 | No change |

- The comment share of the changed modules is 27 % in `transform_more.js`, 23 % in `transform.js` and 25 % in `fields.js`. The bare code of `transform_more.js` is 16.5 KB.
- All eleven kinds are in the page, so no smaller set was necessary.
- The steps take 47 ms to 263 ms for 200,000 rows. This is the best of 7 runs, at a load average of 8 on 22 cores. A front unit test checks them (16.10a, `FE-75`). The browser case B183 checks the draw of a panel with a `join_by_field`, and the case B185 checks the caps.
- The heatmap adds `panels/heatmap.js` and `heat_model.js`. It makes small changes in `panel_frame.js`, `colors.js` and `queries.js`.

| Module | Bytes added | Bytes now | What it holds |
|---|---|---|---|
| `panels/heatmap.js` | 12,683 | 12,683 | The engine options, the value axis, the pick of a cell, the card, the keys, the colour bar, the hook |
| `heat_model.js` | 15,071 | 15,071 | The grid, the cells, the palette of the 10 schemes, the caps. It was 15,482 bytes before the page used `basisChannel` of `colors.js` |
| `panel_frame.js` | 356 | 40,401 | The loader, the range of the panel and the note of "View data" |
| `colors.js` | 7 | 16,839 | The export of `basisChannel` |
| `queries.js` | 11 | 8,066 | `heatmap` joins the frames on time |

- The step adds 28,128 bytes. The comment share is 20 % in `panels/heatmap.js` and 28 % in `heat_model.js`.
- The editor code is not in the measure (10.3). The glyph and the rules of the panel are in `panel-bucket.css`, which the measure does not count.
- A front unit test checks the time of the grid for 500,000 values (16.10a). The browser case B146 checks the draw of a grid of 250,000 cells in 700 ms or less. It also checks that no task of the main thread is longer than 800 ms.

## 10. Editor, history and conflicts

### 10.1 Scope

The editor shows the source text first and the forms second (a rule of the editor). Viewer and editor are both in 1.0.

| In v1 | In v1.1 (10.16) |
|---|---|
| Edit mode on the dashboard page (10.2). | Upload of a Grafana file in the page. |
| Panel editor: SQL tab, panel form, panel JSON tab, preview, "Run" (10.3). | List and restore of deleted dashboards in the page. |
| Dashboard JSON tab with every error by JSON pointer (10.4). | Diff side by side. |
| Layout by grid numbers, "Move up", "Move down" (10.5). Drag and resize on the grid (10.5a). | Completion in the SQL text. |
| Add, duplicate and delete a panel. Add, rename, move, collapse and delete a section (10.6). | |
| New dashboard, folders and move (10.7). | |
| Draft in `localStorage` (10.8). | |
| Save with `If-Match`, `Idempotency-Key` and a message (10.9). | |
| Conflict screen with three choices (10.10). | |
| History list, view, diff and restore (10.11). | |
| Export and import of one document (10.12). | |
| Delete with a deletion version, and the subcommand `restore` (10.13). | |
| Forms for overrides, mappings, data links and transformations (10.3a). Forms for the settings, the dashboard links and the variables (10.3b). | |

The forms of 10.3a and 10.3b edit the overrides, the mappings, the data links, the transformations, the settings, the dashboard links and the variables. The JSON tabs (10.3, 10.4) edit the same members and the options of each panel type. The two views always agree.

### 10.2 Edit mode

- The header (9.9) has the switch "Edit" (`data-testid="edit-toggle"`) when two conditions are true. `editing` is `true` (6.10), and the dashboard has `access.write = true` (6.14). A user without these rights has no switch and no slot for "Save".
- The switch is a button with `role="switch"` and `aria-checked`. It is `false` in the view and `true` while the editor holds a draft: the grid, the panel editor, the JSON tab and the settings. The key Space or Enter changes it, as a click does. Its focus is visible. Its name is "Edit".
- The switch turns the editor on or off. The event `edit-on` adds `edit=1` to the URL and loads the modules of `js/editor/` with `import()`. In the state `panel-view`, it opens the editor of the panel that the view shows (10.2a). Off with no change leaves the edit mode at once. Off with changes opens the dialog "Exit edit mode" (see below).
- The draft starts as a copy of the loaded document, with its etag and version as the base (10.8).
- The edit mode has no row of its own. The header stays one row of 44 px (9.6). It has a group of controls for the edit mode (`data-testid="edit-bar"`) just before the time control. The group has no reserved room. It is in the page in edit mode only, and no button of the view moves. In this order, it has "Add" (`data-testid="add-button"`, with its menu part `add-menu`), "Save" (`data-testid="save-button"`) and "Revert" (`data-testid="discard-button"`).
- "Add" is a split button. The main part has the icon and the label "Add". Its name and its tooltip are "Add panel". One click on it adds a panel to the last section and opens the panel editor (10.6). The menu part is a chevron with the name "Add menu". It opens a menu with these items, in this order:
  - "Add panel" (`add-panel`). It does the same as the main part.
  - The item "Add panel" has no type choice. The type changes in the panel editor (10.3). The new types are in the choice of the type, and they need a source like each panel with data.
  - "Add text panel" (`add-text`). It adds a text panel to the last section (10.6). It needs no source, so it works when `GET api/sources` is empty.
  - "Add section" (`add-section`). It adds a section at the end of the draft (10.6). A status for assistive technology says "Section added."
  - A separator, then "Place a variable in the grid" (`add-variable`, 10.3b). It acts on the variables that can go to the grid. Such a variable has no placement (4.7a), and its `hide` is not `variable` (4.7).
    - With one such variable, the item puts its control in the grid at once.
    - With more than one, the item opens a menu (`add-variable-list`) of these variables. A choice puts its control in the grid.
    - With none, the item is not active.
    - The control goes to the bottom of the last section, with 6 columns and 2 rows. The switch "Place in the grid" of 10.3b does the same.
- The menu part opens with a click, with Enter, with Space and with the key ArrowDown. The focus goes to the first item. The arrow keys move, Enter chooses, and Escape closes the menu and gives the focus back to the menu part. Tab goes in the order: "Add", the menu part, "Save", "Revert", the time control, the switch.
- "Add" acts on the draft grid. In the other views of the editor it keeps its place and it is not active, so the header does not change. Up to 900 px, "Add" shows its icon only (9.6, 9.9). A viewer has no "Add".
- "Save" is the primary button. It opens the dialog of 10.9. The key Ctrl+S or Cmd+S does the same. Above 1100 px it shows the text "Save". Up to 1100 px it shows the icon, and its name stays "Save".
- "Revert" is an icon button with the name "Revert all changes". Up to 640 px it has no button in the header (9.6). The menu "More actions" has the item "Revert all changes" (`discard-menu`) at its top. It discards all the changes and goes back to the last saved version. It asks first when there are changes, with the dialog "Discard your changes". The dialog says "Discard your changes and show the saved dashboard?" Its buttons are "Stay" and "Discard". "Revert" does not leave the edit mode.
- The state of the draft has no visible text. The page shows no label, no dot and no pill for it. "Save" and "Revert" show the state.
- "Revert" is active when the draft is not the base document. "Save" is active in that case too.
- "Save" is also active when the current range or the current refresh differs from the default of the draft (10.9).
- "Save" has a `title`. It reads "Save the changes" when the draft has changes. It reads "Save the time range or the refresh as the default" when only the range or the refresh differs. It reads "There are no changes to save" when the button is not active.
- A live region that only a screen reader can read says "Unsaved changes" or "No changes". These two texts follow the draft only. The protections do not change: the event `beforeunload` (10.8) and the dialog "Exit edit mode" stay.
- The other actions of the edit mode are in the menu "More actions" (9.9), at its top. Each keeps its `data-testid`:
  - "Dashboard JSON" (`json-tab`) and "Settings" (`dash-settings`, 10.3b) show in each view of the editor except their own view.
  - The variables and the dashboard links are in "Settings" (10.3b).
  - The annotations are in "Settings" (10.3b). The marks of a draft show after the save, because the route reads the saved document.
  - "Add panel here" and the menu "Section actions" are in the headers of the sections (10.6).
- The old buttons "Add panel", "Add section", "Dashboard JSON", "Settings", "Discard" and "Exit edit" are gone. The switch replaces "Exit edit". The button "Add" replaces "Add panel" and "Add section". The menu items replace the others. "Revert" replaces "Discard". "Add panel" and "Add section" are no more in the menu "More actions".
- Each panel frame in edit mode gets the menu entries "Edit", "Duplicate", "Move up", "Move down" and "Delete" (10.3 to 10.6). It has no item "View" (10.2a, rule 5). In the view modes, the panel menu has "Edit" for a user who can edit.
- Each section header gets "Rename", "Add panel here" and the menu "Section actions" (10.6). These controls show on hover and on keyboard focus inside the header. On a touch screen they always show. The collapse control now changes the member `collapsed` of the draft.
- The switch off, with changes, opens the dialog "Exit edit mode" with the buttons "Stay", "Discard" and "Save". Its text is: "You have changes
  that are not saved." "Discard" removes the draft and shows the base document. "Stay" keeps the edit mode.
  The same dialog shows for `edit-off`, and for `leave` of a panel editor that a panel view opened (10.2a, rule 7). The browser "Back" never shows it (10.2a, rule 9).

Data in edit mode:

- A panel can have the source, the format, the minimum interval and the SQL of the base document. Then it shows the
  result of the query route (6.20).
- Else it shows the last result of "Run" (10.3), or the text "Run the query to see the data.".
- A panel that the draft does not change keeps the frame of the view, with its chart and its result (10.2a, rule 17). The switch "Edit" sends no query for it.
- The variables bar uses the variables of the base document. When the draft changes the variable list, the page shows
  a notice. Its text is: "Save the dashboard to load the options of the changed variables." "Run" sends the variables
  of the draft (6.22).

### 10.2a States of the page and their transitions

The dashboard page has one state at a time. The URL names this state (9.5), and the page shows only its view. A reload, a pasted URL, and the browser "Back" and "Forward" give the state that the URL names.
This section gives the rule for each change of the state. It names events, not buttons, so a new layout of the header does not change it.

The states are in this table. `<q>` stands for the names `from`, `to`, `timezone`, `var-<name>` and `refresh` (9.5). Every state keeps their values. A name shows only when its value differs from the default of the dashboard.

| State | Name | Names in the URL, after `<q>` | The main area shows |
|---|---|---|---|
| Dashboard view | `view` | none | The grid (9.8). |
| Panel view | `panel-view` | `viewPanel=<id>` | One panel at full size (9.8). |
| Dashboard edit | `edit` | `edit=1` | The grid of the draft (10.2). |
| Panel editor | `panel-edit` | `edit=1`, `editPanel=<id>` | The panel editor (10.3). |
| Dashboard JSON | `json` | `edit=1`, `editview=json` | The JSON tab (10.4). |
| Dashboard settings | `settings` | `edit=1`, `editview=settings` | The settings view (10.3b). |
| History | `history` | `editview=history`, and `edit=1` when edit mode is on, and `version`, `base` (10.11) | The history (10.11). |

Edit mode is on in the states `edit`, `panel-edit`, `json` and `settings`. It is on in the state `history` when the URL has `edit=1`.
The state `history` does not need the write right. The other editor states need it.

The origin of a state is the state from which the page opened it with `history.pushState`. The page writes the origin in the new entry. `history.state` is `{ "hune": { "from": "<relative URL of the entry that was current>" } }`.
The browser keeps this value over a reload. A new tab, a pasted URL, a link and the entry redirect (9.7) give an entry with no origin.
The kind of the origin is `view` for the states `view` and `panel-view`, and `edit` for the other states.

The events are these:

| Event | Meaning |
|---|---|
| `view-panel` | The user opens the view of a panel: the item "View" of the panel menu, or a link to a `viewPanel` URL. |
| `close-view` | The key Escape, or the arrow after the selector in the panel view. |
| `edit-on`, `edit-off` | The Edit switch changes to on or to off. The button "Exit edit" is `edit-off`. |
| `panel-edit` | The item "Edit" of a panel menu, or "Add panel" (a new panel). |
| `open-json`, `open-settings`, `open-history` | The items "Dashboard JSON", "Settings" and "History" of the menu "More actions", and the links to them in an error box. |
| `history-nav`, `up` | In the history: a version, a diff or the list. `history-nav` opens a version or a diff, and `up` goes back to the list. |
| `leave` | The control that goes up one level: the arrow after the selector in the panel editor (10.3), "Back to dashboard" of the JSON tab and of the settings view, and "Back to the dashboard" of the history. A restore of a version in the history is a `leave` too. |
| `save`, `discard` | A save that succeeds (10.9). A "Discard" that the user confirms (10.2). |
| `back`, `forward` | The buttons of the browser. |
| `reload` | The key F5, or a reload of the page. |
| `deep-link` | A full load of a URL with no `history.state` of the page. |
| `change` | A change of the time range, of the refresh or of a variable (9.9, 9.10). |
The history column of the next table has four values. `push` is `history.pushState`. `replace` is `history.replaceState`. `pop` is `history.back()`. `none` means that the page does not write the history.

A `push` that opens an editor state writes the origin. "Dialog" means the dialog "Exit edit mode" of 10.2. It shows only when the draft differs from the base document.

| State | Event | Next state | Names in the URL | History |
|---|---|---|---|---|
| `view` | `view-panel` (id) | `panel-view` | `viewPanel=<id>` | push |
| `view` | `edit-on` | `edit` | `edit=1` | push, origin `view` |
| `view` | `panel-edit` (id) | `panel-edit` | `edit=1`, `editPanel=<id>` | push, origin `view` |
| `view` | `open-history` | `history` | `editview=history` | push, origin `view` |
| `panel-view` | `close-view` | `view` | none | push |
| `panel-view` | `view-panel` (the same id) | `panel-view` | no change | none |
| `panel-view` | `edit-on` | `panel-edit` of this panel | `edit=1`, `editPanel=<id>` | push, origin `view` |
| `panel-view` | `open-history` | `history` | `editview=history` | push, origin `view` |
| `edit` | `panel-edit` (id) | `panel-edit` | `edit=1`, `editPanel=<id>` | push, origin `edit` |
| `edit` | `open-json`, `open-settings` | `json`, `settings` | `edit=1`, `editview=<name>` | push, origin `edit` |
| `edit` | `open-history` | `history` | `edit=1`, `editview=history` | push, origin `edit` |
| `edit` | `edit-off`, `save` | `view` | none | pop, or replace (rule 7) |
| `edit` | `discard` | `edit` | no change | none |
| `panel-edit` | `leave`, origin `edit` | `edit` | `edit=1` | pop |
| `panel-edit` | `leave`, origin `view` | the origin: `panel-view` of this panel, or `view` | the names of the origin | pop, dialog |
| `panel-edit` | `leave`, no origin | `edit` | `edit=1` | replace |
| `panel-edit` | `edit-off`, `save` | the origin when its kind is `view`, else `view` | the names of that state | pop, or replace (rule 7), dialog for `edit-off` |
| `panel-edit` | `discard`, the panel is in the base document | `panel-edit` | no change | none |
| `panel-edit` | `discard`, the panel is new | `edit` (the `leave` rules) | `edit=1` | pop or replace |
| `json`, `settings` | `leave` | `edit`, or the origin | `edit=1` | pop, or replace with no origin |
| `json`, `settings` | `edit-off`, `save` | `view` | none | pop, or replace (rule 7), dialog for `edit-off` |
| `history` | `leave` | the origin, or `edit` when edit mode is on, else `view` | by the state | pop, or replace with no origin |
| `history` | `edit-on` | `history` with edit mode on | `edit=1` added | replace |
| `history` | `edit-off` | `history` with edit mode off | `edit=1` removed | replace, dialog |
| `history` | `history-nav` (version, base) | `history` with the names `version` and `base` | `version`, `base` | push, origin |
| `history` with `version` or `base` | `up` | the origin (the list), or `history` with no `version` and no `base` | by the state | pop, or replace with no origin |
| any | `back`, `forward` | the state of the URL of the entry | the URL as it is | none |
| any | `reload` | the same state | the same | replace, to normalise only |
| any | `deep-link` | the state of rule 1 | by rule 4 | replace, to normalise only |
| any | `change` | the same state | the same | replace |
| any, edit mode on | `link` | another page | the URL of the link | push, dialog |

The rules are these:

1. Precedence. The page reads the view names of the URL and chooses the first match in this list:
   1. `editview=history`: the state `history`.
   2. The user cannot edit (rule 8): the names `edit`, `editPanel` and `editview` of the values `json` and `settings` do not count.
   3. `editview=json` or `editview=settings`: the state `json` or `settings`.
   4. `editPanel=<id>` and the draft holds a panel with this id: the state `panel-edit`.
   5. `edit=1`: the state `edit`.
   6. `viewPanel=<value>` and the value names a panel that has a view (9.8). The value is an id, `panel-<id>`, or the key of a copy: `<id>-r<n>` or `panel-<id>-r<n>`, with `n` from 2 (9.8c).
      - The page also reads the form `<x>$panel-<id>` of Grafana. The text `<x>` is the value of a copy, not its number. It is all the text before the last `$panel-`, and it is not empty.
      - The page finds the copy whose value is `<x>`: a copy of the panel `<id>`, or of its section. The match is exact: the case counts, and the option text does not match. The page compares `<x>` as text and uses it for nothing else.
      - The page then writes the key of that copy: the number `<id>` for the first copy, else `<id>-r<n>`. It never writes the form of Grafana.
      - A text `<x>` that is not the value of a copy names no panel. Rule 4 removes it with the notice. Examples are a value that is not selected or not an option, and the index of a copy.
      - A key of a copy is valid when the panel has a `repeat`, or is in a section that has a `repeat`. The number `n` is at most the limit of that repeat (4.35, 9.8c).
      - Rule 16 gives the check of the count, and the wait for the values that the form of Grafana needs.
   7. None of these: the state `view`.

   A name of a lower level never changes the state, so `editPanel` wins over `viewPanel`. The draft is the stored draft of the user when it exists, else the saved document.
2. One state. The page closes the view of the old state before it shows the view of the new state. No frame shows two views, and no frame shows an empty main area.
3. Right and draft. The page reads the right to edit (rule 8) and the draft before it chooses the state.
4. Normal form. The page removes with `replaceState` each view name that rule 1 does not use. It also removes an id that does not exist and a value that the contract does not know.
   For an id, the notice is "This dashboard has no panel <id>.". A `popstate` writes the URL only for this reason. It never removes a name that rule 1 uses.
5. Open a state. `edit-on` in the state `panel-view` opens the editor of the panel that the view shows, not the dashboard edit. The panel menu of the panel view and of the dashboard view also has the item "Edit" for a user who can edit.
   The panel menu of the state `edit` has no item "View", because the editor already shows the panel. When the panel of an `editPanel` is not in the stored draft, the page shows the state `edit` and the notice of rule 4.
6. Leave. A `leave` with an origin in the previous entry is a `pop`: the page calls `history.back()` and shows the state of that entry. A `leave` with no origin is a `replace` to the target of the table.
   The stack after an open and a leave is the stack before them.
   The `close-view` of the panel view stays a `push` (9.5). It ends no mode, and the browser "Back" shows the panel view again.
7. End of edit mode. `edit-off` and `save` end edit mode and show the origin when its kind is `view`, else the dashboard view. The page uses `pop` when the entry of the state has the origin of the kind `view`, else `replace`.
   A state that is deeper than the first editor state (for example `panel-edit` with the origin `edit`) always uses `replace`. `edit-off` with changes shows the dialog first, and the three buttons of 10.2 apply.
   After a `save`, the panel view shows the saved panel.
8. Rights. The user can edit when `editing` is `true` (6.10) and `access.write` is `true` (6.14). A URL with edit names and no right gives the state of rule 1 without those names.
   The page shows the notice "You cannot edit this dashboard.". When a draft exists, the notice adds "Your draft stays in this browser.".
   The page shows no "Resume" to a user who cannot edit, and it does not remove the draft.
9. Back and Forward. The browser cannot stop a `back` or a `forward`. When one of them ends edit mode and the draft has changes, the page keeps the draft.
   It shows the notice with "Resume" and "Discard" (10.8), and each open dialog of the page closes at once.
   The page sets the range, the refresh and the variables to those of the entry. A name that the entry lacks gives the default (9.5).
10. Reload. A reload gives the same state, the same URL and the same origin (`history.state` stays). The draft comes back with the notice "Your draft is back.".
    The page holds the first paint (9.6) until the view of the state is complete. For an editor state, this view includes the editor. The reload shows no frame of a state that the URL does not name.
11. Deep link. A pasted URL has no origin. `leave` then goes to the dashboard in edit mode by `replace`. The URL of a deep link is in normal form when rule 1 gives its state, removes no name and has no form `panel-<id>`.
    The order of the names does not count. The page writes the form `panel-<id>` as the number (9.5).
12. Time, refresh and variables. A `change` uses `replaceState` and keeps the view names, in every state. It writes the names of 9.5 that differ from the defaults. A drag on a time series is a `push` (9.8).
    A `pop` that the page makes keeps the values that the person set: the page writes them in the entry that it reaches. A Back or Forward of the browser shows the values of that entry.
13. Entry redirect. The redirect of 9.7 runs only in the list page. It uses `replace`. A state URL never gives a redirect.
14. Focus. When `panel-view` opens, also after a `leave`, the focus goes to the view (9.8). When an editor state opens, the focus goes to its region (`role="region"`), which shows no ring to a pointer user. The state `edit` that the Edit switch opens keeps the focus on the switch.
    After a `leave` or a `close-view`, the focus goes to the control that opened the state. This is the menu button of the panel, or the button "More actions".
15. Draft. No transition removes the draft. Only `save`, a confirmed "Discard", "Load theirs" (10.10) and a delete (10.13) remove it. A `leave` that stays in edit mode keeps it in the page and in `localStorage` (10.8).
16. Repeat. The page knows the number of copies only after the options load.
    - A `viewPanel` that names a copy keeps the state `panel-view` until then. The view shows its frame with the state `idle`.
    - The form `<x>$panel-<id>` of Grafana (rule 1) waits for the values in the same way. Until then, the view shows the frame of the panel `<id>` with the state `idle`. The page sends no query for it and does not write the URL.
    - When the values are known, the page shows the copy whose value is `<x>` and writes its key with `replaceState`. When no copy has this value, rule 4 applies.
    - When the values are known, and after each later change of the selection, the page checks the key. A key that names no copy gives rule 4. The page removes the name with `replaceState` and shows the notice "This dashboard has no panel <value>.". The state is then `view`.
    - An `editPanel` names the stored panel by its id only. A value with the form of a key of a copy is a value that the contract does not know. Rule 4 removes it with a notice.
    - A `change` is a `replace` as in rule 12, also when it makes the page remove a `viewPanel`.
    - The vectors `FE-26` and `FE-27` hold the cases (16.10a).
17. Frames. A change between the states `view` and `edit` keeps the frame of each panel that the draft does not change. The frame keeps its chart, its canvas and its last result.
    - The draft grid takes the frame from the base grid when the state `edit` opens. It returns the frame to its place when the state `edit` closes. The page moves the frames in one task, so the viewer never sees a frame without its chart.
    - A frame that the draft grid takes keeps the runtime of the base document. It has the panel menu of the edit mode, no zoom by drag and no mark (9.19d, rule 22).
    - A panel that the draft changes gets a new frame. The rules of "Data in edit mode" (10.2) give its data.
    - The same rule applies when the panel editor, the JSON tab, the settings or the history closes, and to "Back" and "Forward" between these states.
    - The grid is not in the page while the panel editor shows, and the engine then releases its canvas. A chart that returns to the page draws in the task that shows the state. It draws before the page writes the hook `data-hune-view` (9.29).
    - For this, the page sends the event `hune-shown` on `document` just before it writes the hook. Each chart that had no width draws at once. Thus no paint and no script after the change finds a blank canvas.
    - The grid sends the same event at the end of the expand of a section (9.8).
    - These transitions send no query, because no data changed. No frame shows the state `idle`, `queued` or `loading`, the loader or the preview (9.11) for them.
    - The browser case B207 checks this rule in both themes, with a panel and a section that repeat. Its probe reads the frames after each paint and at the end of the task that writes the hook. Its steps wait for states, not for a fixed time.

An invariant covers all the rules. For each state, `parse(render(state))` is that state. For each URL, `render(parse(url))` is the normal form of the URL. The unit tests of 16.10a check this with the vectors `FE-26`.

### 10.3 Panel editor

`editPanel=<id>` opens the panel editor. The editor fills the window under the header and has no scrollbar. At 900 px or less it becomes one column, and the content scrolls. It has these parts:

- The panel editor has no top bar and no row of its own. The editor starts at the foot of the header, so it has 48 px more height than before.
- The box of the editor starts at the foot of the header and has a top padding of `--page-pad` (9.6). The preview, the row of the variables and the top of the options pane start at this gap. The preview never touches the header. Without a row of the variables, the top of the options pane and the top of the preview are at the same height. The pane keeps its place at the right edge of the window.
- The way back is the arrow after the selector in the header (see below). The editor has no visible text for the state of the draft, no "Save" and no "Discard". The header has "Save" and "Revert" (10.2). The header has no name for this view. The header keeps all its controls, with the switch "Edit" on.
- The time controls are in the header (9.9). The row of the variables bar is under the header, over the preview.
- The preview shows the panel with the draft and its current result. Under it is the query area, with three tabs of 38 px: "Query", "Panel" and "JSON". The preview and the query area have the same height.
- The preview is a panel frame like the frame of the view (9.8). It has the fill and the edge of the theme. When the draft has `transparent` = `true`, it has neither of them.
- The options pane is at the right edge of the window, 330 px wide. It has a header of 48 px with the name of the type and the link "Change". The link opens the group "Visualisation", which is the choice of the type. The pane is hidden on the tab "JSON".
- A control of the editor is 32 px high, with a text of 14 px. A selected segment has a neutral fill.

Each change applies to the draft at once. A change of a text field applies 300 ms after the last key.

Each control of the options pane changes the draft, and the preview draws the draft again in the same task (10.3h, rule 7). The browser case B153 changes each control of each type. It checks that the draft changes in the member of the control, and that the preview changes. It also checks that each member of each type has a control.

A few controls change the draft and not the picture of the preview. Each one is in the closed list of 10.3h (rule 14), with a class and a written cause. The list has 30 entries at most. A control in the list that does change the preview makes B153 fail. An entry that no control matches makes it fail too.

The forms for the options of each type are in 10.3h. The tab "JSON" stays. It edits the whole panel, with the same members as the forms. A change in a form shows in the tab, and a valid change in the tab shows in the forms.

The way back of the panel editor is the arrow after the selector in the header. These rules apply:

- The way back is a link with an arrow and no text. It has the hook `data-testid="editor-back"`.
- A click with no modifier key is the event `leave` of 10.2a. It shows the state from which the editor was opened.
- The name and the tooltip tell the true target. For a panel view they are "Back to panel view". For the dashboard view they are "Back to dashboard".
- For edit mode, and for an entry with no origin, they are "Back to edit mode".
- A click with Ctrl, Shift, Alt or Meta, or with another button, does not send the event. The browser then uses the address of the link.
- The key Enter on the link is a click.
- The arrow never hides, at any width. The name and the tooltip stay.
- The focus goes back as in 10.2a, rule 14. It does not depend on the control that sent the event.
- The key Escape and the "Back" of the browser do not change. The browser case B151 checks these rules.

Tab "Query": the tab has a strip of chips above its fields. There is one chip for each query of the draft, with its ref: `A`, `B` and so on (`data-testid="query-chip-<ref>"`). A chip is a button. It has an eye button (`query-hide-<ref>`, `aria-pressed`). With two queries or more, it also has a button "Delete" (`query-delete-<ref>`).

The chip of a hidden query is muted. The button "Add query" (`query-add`) ends the strip. The fields of the table below apply to the query of the selected chip. The first chip is selected at first.

| Field | Member (section 4) | Rule |
|---|---|---|
| Name | `query.ref`, `extra_queries[i].ref` | A text (`query-ref`) of the grammar of 4.3. The error is "A name starts with a letter, then letters, digits or _, 16 characters at most.". A name that another query of the panel uses gives "Another query has this name.". `A` for the first query writes no member. |
| Data | `data` | "Query" or "Reuse a panel". The choice is in the popover "Query options" (rules of the row below). |
| Source | `query.source` or `extra_queries[i].source` | A select of the items of `GET api/sources` (6.22). An item shows the label, the kind and the state: `Label (MongoDB, <state>)` for a source of the kind `mongodb`. The kind names are `ClickHouse`, `PostgreSQL` and `MongoDB`. An unknown kind shows as the API gives it. A source that the list does not hold shows as `<id> (not available to you)`. |
| Format | `query.format` or `extra_queries[i].format` | `table` or `time_series`. A panel of the type `logs` also has `logs` (4.33f). |
| Minimum interval | `query.min_interval` | Empty, or a duration of 4.4 from `1ms` to `1d` (4.15). The visible label is "Min interval". |
| Max data points | `query.max_data_points` | Empty, or 1 to 10,000. The visible label is "Max points". |
| Relative time | `query.time_from` | Empty, or a text of 4.15, such as `now-6h` or `now/d`. The error is "A relative time starts with now-, as in now-6h, or with now/, as in now/d." |
| Time shift | `query.time_shift` | Empty, or a text of 4.15, such as `1d`. The error is "A time shift has a count and a unit, as in 1d." |
| Hide time info | `query.hide_time_override` | A check box. |
| SQL | `query.sql` or `extra_queries[i].sql` | The SQL text area below. |
| Source panel | `reuse.panel` | For "Reuse a panel": a select of the panels with a query, other than this panel. |
| Apply its transformations | `reuse.with_transforms` | A check box. |

The preview and "Run" use the range of 9.8b with the values of the draft. The fields have the hooks `query-time-from`, `query-time-shift` and `query-hide-time` (9.29).

The fields "Minimum interval", "Max data points", "Relative time", "Time shift" and "Hide time info" are fields of the panel. They apply to all the queries. The editor writes them in `data.query` (4.15a, rule 4).

The tab "Query" has one row for the settings of the query. The rules of the row are these:

- The row is under the chips. It is one element with `role="group"` and the name "Query settings". The tab has no other row of settings.
- The order of the controls is the order of the DOM and the visual order. The order is: "Source", "Format", "Name", "Min interval", "Max points", "Options" and "Run".
- "Source" is a native select. Its label is for the screen reader only, and its title is "Source". Its width is 150 px at least and 280 px at most.
- "Format" is a group of two choices with the name "Format", or three choices in a logs panel ("Logs"). It has no visible label. A segment is 26 px high in a group of 32 px.
- "Name", "Min interval" and "Max points" have a label chip of 12 px at the left of the control, as the variables bar has (9.10). The control of "Name" is 48 px wide. The two other controls are 52 px wide.
- The placeholder of "Min interval" is "1ms". Its title is "Minimum interval. Empty (1ms), or a duration from 1ms to 1d.". The placeholder of "Max points" is "auto". Its title is "Max data points. Empty (the width of the plot), or 1 to 10,000.".
- The rules of the values and the texts of the errors are those of the table above. An error shows in a box under its field. The box floats, so the row keeps its height.
- Each control is 32 px high. At 1280 px the tops of the items of the row differ by 2 px at most. Each target is 24 px wide and 24 px high at least (the rule of B153).
- The button "Options" (`query-options`) has the name "Query options", `aria-expanded` and `aria-controls`. A click on it opens the popover (`query-options-pop`, `role="group"`, name "Query options").
- The popover holds the choice "Data", the fields "Relative time" and "Time shift", and the box "Hide time info". It is 300 px wide at most. It opens under the button, with its right edge on the right edge of the button, inside the window.
- The popover follows its button in the DOM, so the Tab key goes from the button into the popover and then to "Run". The key Escape closes it and gives the focus back to the button. The editor stays open.
- A click outside the popover, or a focus that leaves it, closes it. A redraw of the tab also closes it.
- The button has a dot (`query-options-dot`, 8 px) when a setting of the popover is not at its default. The settings are a reuse, a `time_from`, a `time_shift` and `hide_time_override` = `true`. The button then has `data-set="true"` and the hidden text ", some options are set".
- A panel with "Reuse a panel" has a row with the select "Source panel", the box "Apply its transformations" and the button "Options". The popover holds the choice "Data" only. The dot is on.
- The row has one line at 1280 px. At a smaller width the items wrap in their order. The row has 2 lines at 768 px at most, and 4 lines at 390 px at most. It never gives a horizontal scroll.
- No member leaves the document format. The row and the popover write the same members as before.

The browser case B209 checks these rules. The code of the row is in the editor, so it is outside the measure of 9.30.

The rules of the queries:

- "Add query" adds an item at the end of `extra_queries`. The new query has the source and the format of the selected query and an empty SQL line. Its ref is the first free ref in the list `A` to `Z`, then `AA` to `AZ`.
  - The button is not active at 8 queries, and its tooltip says "A panel has 8 queries at most.".
  - It is not active for a panel that has `repeat` or is in a repeated section (4.15a, rule 8). It is not active for a panel of the type `barchart`, `xychart` or `geomap`. The tooltip gives the reason in each case.
- "Delete" removes the query. When the first query leaves, the first item of `extra_queries` becomes `data.query`, with the members of the panel (4.15a, rule 4). The editor removes `extra_queries` when the last extra query leaves. The last query of a panel cannot be deleted.
- The eye writes `hide` = `true`, or removes the member. A panel with all queries hidden shows "No data" and the note "All the queries are hidden.".
- A rename changes the `ref` and the matchers `query` that name it in this panel. When a matcher changes, the dialog `query-rename-dialog` lists them first, and the rename waits for the end of the entry. The editor never changes the panel that reuses this panel.
- "Run" and `Ctrl+Enter` send one preview for each visible query (6.22). The previews start from the first query, with 6 at most at the same time. Each body holds the selected query as `data.query`, with the members of the panel, and no `ref` and no `hide`. The editor combines the answers as the page does (9.22a). An error of one query shows as a partial result (9.8d). An error with a line and a column marks the query that gave it: a click on the error selects its chip.
- The choice of the type `barchart`, `xychart` or `geomap` for a panel that has extra queries opens the dialog "Change to Bar chart?" (`type-change-dialog`). It says that the extra queries leave the draft. The choice of "Reuse a panel" for a panel that has extra queries opens the same dialog. "Change" applies the change. "Cancel" does nothing.
- The group "Repeat options" is not active for a panel that has extra queries, and its tooltip says why (4.15a, rule 8).
- The list of the source panels of "Reuse a panel" shows a source with extra queries with the note "<n> queries". For a panel of the type `barchart`, `xychart` or `geomap` the list has no such source (4.15, rule 6).
- The tab "Panel" shows the number of queries and the number that are hidden.

The SQL text area:

- It is a `<textarea>` in a monospace font, with line numbers and the colours of the copied `app_highlight.js`.
- The page splits the text at each LF into the array of lines of the member `sql`. It changes each CR LF to LF.
- `Tab` adds two spaces. `Escape` and then `Tab` moves the focus out of the text area.
- The attribute `spellcheck` of the text area is `"false"`, with the text value. The browser then draws no wavy line under a word such as `$__timeFilter`. The editor itself marks no macro. A macro is an error only when the server says so (7.10, 7.22), and then the error box shows the message of the server.
- The text area is transparent, and the layer under it shows the colours. The two layers set each property of the text in the same way. These properties are the font, the size, the line height, `letter-spacing`, `word-spacing`, `tab-size`, `font-kerning`, ligatures, `text-rendering`, padding and border.
- A text area does not inherit `letter-spacing` and `text-rendering` from the page, so the style sets them in both layers. The colour layer does not scroll by itself. It takes its scroll position from the text area, and it has room for the scroll bars of the text area. The width of the line numbers is a whole number of CSS pixels.
- The mouse selection and the colours must line up. The gap between the glyphs of the two layers is less than 0.5 px. This holds at the scale factors 1, 0.9, 1.1 and 1.25, in the light and in the dark theme (case B57).
- The colour layer draws only the rows that are visible and 100 rows above and below, as the gutter does. A gap above and a gap below keep the height of all rows. The scroll height and the width are then the same as for the whole text.
- A scroll that comes within 30 rows of the border of the drawn rows draws them again. A key draws them again at once, with the scroll position of the last scroll event.
- The spans of the whole text give the state of a string or of a comment at the first drawn row. A comment that starts above the window keeps its colour. The editor does not change the highlighter of the engine. It gives the highlighter only the plain text between two spans.
- One difference from a draw of the whole text is known. A name in backticks that holds a pair of keywords, such as `order by`, can show another colour at the border of a window.
- The colours follow the lexer of the dialect `clickhouse` for each source. For a `postgres` source, a `#` shows as a comment, and an `E` string is not coloured as such. This is a known limit. It changes no request. For a `mongodb` source, the query is a JSON text, and the editor shows it with no SQL colours in the milestone MMG.
- The structure of the text follows the lexer of the dialect of the source (10.3c). The structure is the format, the folds, the bracket pair and the current block. For a `postgres` source, it reads `#` as an operator and reads an `E` string as the server does. The colours keep the known limit above. Where the two differ, the structure is the correct one.
- An error with a line and a column (6.8 `sql_invalid`, or an item of `invalid_document`) marks its place. The page
  converts a byte column to UTF-16 units with the rule of `utf16_offset` of ClickHouse Dash.
- An item of `invalid_document` can have the pointer of a SQL line and a message that starts with "column C: " (7.18). The error box then shows "line L, column C" as the place, and the message without this start.
- "Run" and `Ctrl+Enter` send `POST api/sources/<source>/preview` (6.22) with the draft panel and the current values.
  The body has `uid` for a saved dashboard, or `folder` for a new one. It holds only the draft variables that the panel
  names, with their parents. The result shows in the top area and stays as the draft result of the panel.
- The pointers of an `invalid_document` of the preview are relative to the body (6.2). The editor removes the prefix
  `/panel` and shows the rest, relative to the panel. An error under `/dashboard_variables` links to the dashboard
  JSON.
- "Run" exists only when the source is in `GET api/sources`, so in mode `authzen` the subject has `source.query`
  (11).
- The SQL area has a format on request, folds, a bracket pair and a current block. Sections 10.3c to 10.3g give the rules. A variable (10.3b) and an annotation (10.3b) have the same area. A panel with several queries (4.15a) has one area for each query.

Tab "Panel" is a read-only summary under the preview. It shows the type, the data, the place, the unit, the thresholds and the number of overrides.

The options pane holds the fields of the table below. They are in the groups "Panel options", "Visualisation" (the field Type), "Standard options", "Thresholds", "Colour" and "Position". The group "Overrides" holds the override form (10.3a). The groups "Value mappings", "Data links", "Panel links" and "Transformations" follow it.

| Field | Member | Rule |
|---|---|---|
| Title | `title` | The title rule of 4.13. |
| Description | `description` | Text on several lines. |
| Transparent background | `transparent` | A switch (`data-testid="panel-transparent"`). The preview shows its effect at once (10.3). |
| Type | `type` | The 11 types of 9.19, 9.19b, 9.19c and 9.19f, `logs` (9.19h) and `geomap` (9.19i). A new type keeps the shared members and takes the "New" options of 9.19. The type `logs` sets the format of the query to `logs`, and another type changes the format `logs` to `table` (4.33f). A logs panel has no extra query, so they leave. |
| Unit | `fields.defaults.unit` | Text, with a list of the units of 9.15. |
| Decimals | `fields.defaults.decimals` | Empty (automatic) or 0 to 20. |
| Min, Max | `fields.defaults.min`, `.max` | Empty or a number. |
| Thresholds | `fields.defaults.thresholds` | The mode (`absolute`, `percentage`) and a list of steps. The first step is "Base" with no value. Each step has a value and a colour. "Add step" and "Remove" change the list. |
| Colour | `fields.defaults.color` | The member `mode`: a select of the 13 modes of 9.16 (`color-mode`). The entry "Series palette" is the one series palette. The select has no other entry for a series palette. The member `fixed`: a colour with `fixed`, else `null`. |
| No value | `fields.defaults.no_value` | Empty (not set) or 1 to 256 characters (9.14). |
| Position | `grid.x`, `grid.y`, `grid.w`, `grid.h` | The ranges of 4.14, with `x + w <= 24`. |
| Repeat by variable | `repeat.variable` | A select (`repeat-variable`). Its items are "None" and each variable of the draft that has the kind `query` or `custom` and `multi` = `true`. "None" removes the member `repeat`. The group is not active for a panel with `data.reuse`, for the source of a reuse and for a panel in a repeated section, and its tooltip says why (4.13a, rule 4). |
| Direction | `repeat.direction` | A select (`repeat-direction`) with "Horizontal" (`h`) and "Vertical" (`v`). It shows when a variable is chosen. The first choice is "Horizontal". The choice "Horizontal" writes `grid.x` = 0 and `grid.w` = 24 in the draft, and the fields `x` and `w` of "Position" show these values and are not active. |
| Max per row | `repeat.max_per_row` | A number field (`repeat-max-per-row`), from 1 to 24. It shows for "Horizontal" only. Empty: the member is absent, and the field shows the hint "Empty: 4". The choice "Vertical" removes the member. |

The group "Repeat options" comes after "Position". It also shows two lines under its fields:

- The count (`repeat-count`) says "The page shows <k> copies now.", or the message of 9.8c for the state `repeat-empty` or `repeat-limit`. It uses the selection of the draft (9.10) and the limits of the session (9.8c).
- The hint (`repeat-hint`) shows when neither the title, the description nor the SQL of the panel names the variable. It says "Neither the title nor the SQL names this variable. The copies are the same." The hint does not stop a save. The name scan of 9.10 and the placeholder rule of 9.13 give the answer.

The preview of the panel editor is one panel. It runs one query with the selection of the draft, so with "All" it shows the data of all the values. The dashboard shows one panel for each value.

A colour field has a select of the named colours of 9.16 and a text field for a colour value.

Tab "JSON":

- The text area holds the JSON of the whole panel object, with an indent of two spaces.
- The page applies the text to the draft only when the text is a JSON object with no duplicate member. Else it shows
  the line and the column of the fault, and the draft keeps the last valid panel.
- 1 s after the last change, the page checks the whole draft with `validate_only=true` (6.15). It lists the errors whose
  pointer is in this panel, with the pointer relative to the panel. A click on an error puts the cursor on the member
  when the page finds it.
- For the errors in other parts of the document, the page shows their number and a link to the dashboard JSON.
- The forms of 10.3a edit the overrides, the mappings, the data links and the transformations. This tab edits them too. It also edits the options of the type.

The editor of a text panel (4.33a, 9.19b):

- The type is `text`. The options pane has the groups "Panel options", "Visualisation", "Text", "Panel links" and "Position". It has no "Standard options", "Thresholds", "Colour", "Value mappings", "Data links", "Transformations" or "Overrides".
- The tab "Query" shows the note "A text panel has no query." (`text-no-query`). It has no "Source", no "Run" and no field. The tab "Panel" shows the type, the place and the number of lines. The tab "JSON" works as for the other types.
- The group "Text" has the field "Content" (`text-content`). It is a `<textarea>` with 12 rows, in the font `--font-mono`. It grows to the height that the person gives it. It has `spellcheck="true"`, `autocomplete="off"`, `autocapitalize="off"` and `wrap="soft"`. The text is prose, so the wavy line of the browser helps.
- The field is a plain `<textarea>`. It has no second layer and no colour. The rules of the SQL area for the two layers do not apply.
- The key `Tab` moves the focus out of the field. The field never traps the focus (WCAG 2.1.2).
- The page splits the text at each line end into the lines of `options.content`. It changes each CR LF and each lone CR to LF.
- A change applies to the draft 300 ms after the last key (10.3). The preview above shows the panel with the draft, so the preview is the Markdown preview.
- The line `text-count` says "<n> of 65,536 bytes, <m> of 2,000 lines".
- The line `text-error` shows the first error that the page finds. The field has `aria-describedby` for it. The order of the errors is this list:
  1. A line with a control character: "Line L has a control character."
  2. A line above 8,192 bytes: "Line L has more than 8,192 bytes."
  3. The count of lines and the count of bytes.
  4. The messages of the limits of 9.19b.
- The text `text-help` says "Markdown. HTML shows as text. Images do not show. A link starts with /, ?, #, http:// or https://. $name shows the values of a variable."
- The choice of the type `text` for a panel that has a query opens the dialog "Change to Text?" (`type-change-dialog`). It says that the query, the transformations and the field options of this panel leave the draft. The choice of another type for a text panel opens the same dialog. It says that the text leaves the draft, and that the panel gets the query of a new panel (10.6). The button "Change" applies the change to the draft. "Cancel" does nothing. A "Revert" restores the saved panel, as for each edit.
- When no source is available to the person, the change from `text` shows "No source is available to you." and changes nothing.
- The list of the source panels of "Reuse a panel" has no text panel.

The editor of a `state_timeline` and of a `status_history` (4.33b, 9.19c):

- The options pane has the groups of a panel with data. The group "State timeline" or "Status history" holds the options of 4.33b. The groups "Standard options", "Thresholds", "Colour", "Value mappings", "Data links", "Panel links", "Transformations" and "Overrides" are as for a time series.
- The group has one control for each option. A switch has `merge_values`, `span_nulls` and `show_labels`. A segmented control has `show_values` ("Auto", "Always", "Never") and `align_values` ("Left", "Center", "Right"). A number field with a slider has `row_height` (10 to 100), `col_width` (10 to 100), `line_width` (0 to 10) and `fill_opacity` (0 to 100).
- The slider writes the draft at each move, at most one write in each frame (10.3h, rule 7). The number field writes 300 ms after the last key, and only for an integer in its range.
- The group "Legend" has the switch `legend.show` and the choice of the place (`bottom` or `right`). It has no mode and no value list. The group "Tooltip" has the choice `single`, `multi` ("All") or `hidden`, and no sort.
- The group "Colour" has the select of the colour modes (10.3). A note under it says that a value mapping gives the colour of a state. The colour mode gives the colour of the other values.
- The properties of an override are the properties of 9.14 that these types permit. The style members of a series are not in the list (4.33b, rule 5).
- A change of the type between the types with data keeps `data`, `transforms` and `fields`, and it takes the "New" options of 9.19c. A change from a type with `reduce` (`stat`, `piechart` or `bargauge`) to one of these types opens the dialog "Change to <type>?" (`type-change-dialog`). It says that the calculation of the panel leaves the draft.
- The preview shows the picture. A panel that breaks a cap shows the message in the preview too.
- The hooks are `data-testid="timeline-merge"`, `"timeline-show-values"`, `"timeline-align"`, `"timeline-row-height"`, `"timeline-col-width"`, `"timeline-line-width"`, `"timeline-fill-opacity"`, `"timeline-span-nulls"` and `"timeline-labels"`. The legend and the card have `"timeline-legend-show"`, `"timeline-legend-placement"` and `"timeline-tooltip-mode"`. The note of the colour has `"timeline-colour-note"`.

### 10.3a Forms of the panel editor

The order of the forms is: overrides, mappings, data links, transformations, then the forms of 10.3b.

Rules for each form:

- A form reads the draft and writes the draft. It writes only the members that section 4 lists. It sends no request.
- A change applies to the draft at once. A change of a text field applies 300 ms after the last key (10.3).
- The JSON tab shows each change of a form. A valid change in the JSON tab shows in the forms.
- A list has the buttons "Move up", "Move down" and "Remove" for each item. They are icon buttons with an accessible name. The order of an array matters (4.17, 4.22, 4.25).
- The button "Add" has the attribute `aria-disabled` at the limit of 4.35. Its tooltip names the limit.
- The page checks each grammar and each range of section 4 in the control, with the same rule. An error shows under the control, and the control has `aria-invalid`.
- The other errors come from the check of 10.3 (`validate_only`). They show under the group, with their pointer relative to the panel.
- The page writes each text with `textContent` or `value`, never as markup.
- A form shows only the members that the type of the panel permits (4.18).

Group "Overrides" (`fields.overrides`, 0 to 512 items):

- The group shows the number of overrides and the button "Add override" (`override-add`).
- An override has a matcher and a list of properties.
- The matcher has a select (`override-<n>-by`) with three items: "Fields with name" (`name`), "Fields with regex" (`regex`) and "Fields with names" (`names`).
- With `name` and `regex`, the matcher has one text of 1 to 1024 bytes. With `regex`, the page shows the error "The pattern is not valid." when `new RegExp` fails or the pattern breaks 4.24.
- With `names`, the matcher has a list of 1 to 64 texts and a select "Include" or "Exclude" (`mode`).
- A text for a name has a list of the field names of the last result of the panel, when the page has one.
- The list of properties has one row for each member of `set`. A row has the name of the property and its control.
- The select "Add property" (`override-<n>-property-add`) lists the properties of 4.18 that are not set. It does not list `no_value`. It lists only the properties that the type of the panel permits.
- The style members (`draw`, `line_width`, `fill_opacity`, `show_points`, `point_size`) show for a `timeseries` only.
- "Remove" on the last property of an override is not active. The person removes the override instead, because `set` has 1 member or more (4.17).

The controls of the properties:

| Property | Control |
|---|---|
| `unit` | A text with the list of the units of 9.15. The forms `suffix:` and `prefix:` are texts. |
| `decimals` | An integer from 0 to 20. |
| `min`, `max`, `soft_min`, `soft_max` | A number. The page shows the error of 4.18 when `min` is not below `max`, or `soft_min` is not below `soft_max`. |
| `color` | The select of the 13 modes of 9.16 (the modes of the group "Colour" of 10.3) and, with `fixed`, the field of the colour. |
| `thresholds` | The editor of the thresholds of the group "Thresholds". |
| `mappings` | The form of the mappings (below). |
| `links` | The form of the data links (below). |
| `display_name` | A text of 1 to 256 characters. |
| `width` | An integer from 10 to 4000. |
| `align` | A select. |
| `filterable`, `hidden`, `inspect` | A switch. |
| `cell` | A select of the type, and a select of the mode with the types `color_background` and `gauge`. |
| `footer` | A choice of 1 to 8 calculations. |
| `hide_from` | Three switches: `legend`, `tooltip` and `viz`. |
| `axis_placement`, `line_dash`, `transform`, `draw`, `show_points` | A select. |
| `axis_label` | A text of 1 to 256 characters. |
| `line_width` | An integer from 0 to 10. |
| `fill_opacity` | An integer from 0 to 100. |
| `point_size` | An integer from 1 to 40. |

Group "Value mappings" (`fields.defaults.mappings` in "Standard options", and the property `mappings`; 0 to 256 items):

- The button "Add mapping" (`mapping-add`) has a select of the four kinds. A row shows the members of its kind (4.22).
- A `value` row has a text of 0 to 1024 bytes. A `range` row has `from` and `to`: a number, or empty for `null`.
- A `regex` row has a pattern with the error of 4.24. A `special` row has a select of the six values of `match`.
- Each row has `text` (empty means `null`) and a colour. The colour has the select of 9.16, a text field and the choice "No colour" for `null`.
- A row shows the error "A mapping needs a text or a colour." when both are `null` (4.34).

Groups "Data links" (`fields.defaults.links`) and "Panel links" (`links`), 0 to 16 items each:

- A row has `title` (1 to 256 characters), `url` (1 to 2048 bytes) and the switch `target_blank`.
- The field `url` shows the error "The URL must start with /, ?, #, http:// or https://."
- The page uses `checkUrl` of `links.js`, so the check is the check of 4.23 and 9.18.

Group "Transformations" (`transforms`, 0 to 16 items):

- The button "Add transformation" (`transform-add`) has a select of the thirteen kinds (4.25 and 4.25a).
- Each transform has a select of its kind (`transform-<n>-kind`). A new kind replaces the transform with a new transform of that kind.
- `filter_by_value` has the selects `mode` and `match`, and a list of 1 to 32 filters.
- A filter has a field name, a select of the 10 matcher kinds, and the values of the kind. The field name is a text with the list of the field names.
- `equal` and `not_equal` take one value: a text, or a number with a select "Number". The four comparisons take a number. `range` takes `from` and `to`. `regex` takes a pattern. `is_null` and `is_not_null` take no value.
- `grouping_to_matrix` has the texts `column_field`, `row_field` and `value_field`, and a select `empty_value`.

Forms of the eleven kinds of 4.25a (the editor code is outside the measure of 9.30). The file `forms/transform_more_form.js` holds them.

| Kind | Form |
|---|---|
| `organize` | A list of the fields of the last data. Each row has the check box "Hide", the text field "Rename" and the buttons "Up" and "Down". The form writes `hide`, `rename` and `order`. A field that the data does not hold stays in the list with the mark "not in the data". |
| `calculate_field` | A select "Mode". The controls of the mode follow: two operands (a select of the kind, and a field name or a number) with a select of the operator, a select of the calculation with a list of fields, a select of the operator with a field name, or a switch "As percentile". Then come a text field "Alias" and a switch "Replace all fields". |
| `sort_by` | A list of 1 to 8 rows. Each row has a field name, a switch "Descending" and the buttons "Up", "Down" and "Remove". The button "Add key" adds a row. |
| `limit` | A number field "Limit" and a select "From" with `first` and `last`. |
| `merge` | No field. The text "Merge needs two queries or more." shows when the panel has one query. |
| `reduce` | A select "Mode" and a list of check boxes of the 18 calculations. |
| `partition_by_values` | A list of 1 to 4 field names and a switch "Keep fields". |
| `join_by_field` | A select "Field" (the choice "Time" is `null`) and a select "Mode". |
| `filter_fields_by_name` | The groups "Include" and "Exclude". Each group has a list of names and a text field "Pattern". |
| `rename_by_regex` | The text fields "Match" and "Replace". |
| `convert_field_type` | A list of 1 to 16 rows. Each row has a field name and a select of the new type. |

- A form shows the error of 4.37 under the field, as the other forms do. A control that has an error writes nothing in the draft.
- A field name is a text with the list of the current names of the last data of the panel. A person can also type a name that the data does not hold.
- A rule that no single control holds has its own error text. Two operands that are both numbers give "One operand must be a field." The draft keeps the last valid operands.
- The form never runs a transform. The panel shows the result after the draft applies, as for the other kinds.
- A new transform is valid at once and changes no data. It has these members:
  - `organize` has empty arrays. `limit` has `count` 100 and `end` `first`. `reduce` has `series_to_rows` and `["max"]`.
  - `join_by_field` has `field` `null` and `mode` `outer`. `filter_fields_by_name` has two objects with no name and no pattern. `rename_by_regex` has the text `(.*)` and the replacement `$1`.
  - `sort_by` has one key on the first field of the last data. `calculate_field` has the mode `index`. `partition_by_values` has the first field of the last data.
  - `convert_field_type` has one conversion of the first field to `text`. `merge` has no member.
- A row that a person adds to a list is valid at once. The row takes a name that the list does not hold yet.
- The hook of a control is `transform-<n>-<member>`, with `_` written as `-` (9.29). A row of a list adds its index: `transform-<n>-sort-<i>-field`, `transform-<n>-conversions-<i>-to`, `transform-<n>-fields-<i>-name`, `transform-<n>-hide-<i>` and `transform-<n>-rename-<i>`.
- The unit tests FE-70 to FE-75 and the test `transform_more_form.test.mjs` check the rules of the forms. The browser case B186 checks the forms in the page.

### 10.3b Dashboard settings, dashboard links and variables

The menu "More actions" (9.9, 10.2) has an item "Settings" (`dash-settings`) after "Dashboard JSON". It opens the view `editview=settings` (9.5) in the main area, as the JSON tab does.

The view has the tabs "General", "Time", "Links", "Variables", "Annotations" and "JSON". The rules of 10.3a apply to each form. A tab has the hook `settings-tab-<name>`, with the name in lower case. "Back to dashboard" closes the view and removes `editview`.

- The tab "JSON" is the dashboard JSON editor of 10.4. It has no title of its own and no back button of its own.
- The view has no button "Save" and no button "Discard": the header has "Save" and "Revert" (10.2).

Tab "General":

| Field | Member | Rule |
|---|---|---|
| Title | `title` | 1 to 256 characters. |
| Description | `description` | 0 to 4096 characters, on several lines. |
| Tags | `tags` | 0 to 32 tags of 1 to 64 characters. The Enter key adds a tag. A tag that exists gives the error "This tag exists." |
| Cursor sync | `cursor_sync` | A select: `off` or `crosshair`. |

Tab "Time":

| Field | Member | Rule |
|---|---|---|
| From, To | `time.from`, `time.to` | A time text (4.4). The page uses the check of the time picker (9.9). When both are absolute, From must be below To. |
| Time zone | `time.timezone` | A select with `browser`, `utc` and "Other". "Other" opens a text with the list of `Intl.supportedValuesOf("timeZone")` when the browser has it. The error is "The time zone is not valid." when the text breaks 4.6. A name that the browser does not know gives the warning "This browser does not know this time zone." The draft keeps the name. |
| Refresh | `time.refresh` | A select: "Off" (`null`) and the values of `REFRESH_CHOICES`. |

The Save dialog (10.9) is the short way to make the current range and the current refresh the defaults. This tab sets them by hand.

Tab "Links" (`links`, 0 to 16 items):

- The button "Add link" (`settings-links`) has a select of the two kinds (4.5a).
- A `link` row has `title`, `url` (with the error of 10.3a) and the switches `target_blank`, `include_vars` and `keep_time`.
- A `dashboards` row has `title`, a list of 0 to 8 tags, and the switches `as_dropdown`, `target_blank`, `include_vars` and `keep_time`. With `as_dropdown`, the title needs 1 character or more.

Tab "Variables" (`variables`, 0 to 64 items; hook `settings-variables`):

- A row shows the name, the kind and the label of a variable. A click opens its form.
- "Add variable" has a select of the six kinds: `query`, `custom`, `text`, `interval`, `constant` and `adhoc` ("Ad hoc filters"). The kind of a variable does not change. The person removes the variable and adds another.
- All kinds have `name` (the rule of 4.3), `label` (empty means `null`) and `hide` (a select of three values).
- A `query` variable has `source` (a select of `GET api/sources`) and `sql` (the text area of 10.3, with no "Run"). It has `multi`, `include_all`, `refresh`, `sort`, `regex` (the error of 4.24), `default` and the switch of the next bullets.
- `all_value` of a `query` variable is not active when `include_all` is `false`. `default` is a list of 0 to 10,000 values, with the rules of 4.11.
- A `custom` variable has a table of 1 to 10,000 options with `text` and `value`. It has `multi`, `include_all`, `all_value` and `default`, as a `query` variable has. A repeated `value` gives the error "This value exists."
- A `query` or a `custom` variable has the switch `Allow custom values` (`variable-form-allow-custom`, member `allow_custom_value`). It is after the switch `Multi-value`. It is off for a new variable. A person turns it on to make the variable open. The switch off removes the member from the draft. The member follows `default` and comes before `placement` (4.38).
- The help text under the switch of a `query` variable says: "Off: the value must be one that the query returns. On: a person can also type a value."
- The help text of a `custom` variable says: "Off: the value must be one of the options. On: a person can also type a value. A typed value is free text: the SQL cannot use it in a quoted name, and in code it must be a number."
- The switch refers to its help text with `aria-describedby`.
- The save runs the checks of 7.18. A placeholder of an open `custom` variable in a quoted name gives the error `not_permitted` with the line of the SQL. The form shows no other check on this switch.
- The rule of the default of a `custom` variable depends on the switch (4.11). With the switch off, a default that is not an option gives the error "The default <value> is not an option.". With the switch on, a default can be a typed value. The error is for a text of more than 1,024 bytes or with a control character only. A default of a `query` variable has no such check, with the switch on or off.
- A `query` variable has a menu `Examples` (`variable-sql-examples`) in the tool row of its SQL area (10.3g). The menu shows for a source of the kind `clickhouse` only. It follows a change of the select `source`. It has three items, from 0 (`variable-sql-example-<n>`). The page loads no file for them and no resource of another host.
- An item puts its text in the SQL area. The text goes at the cursor, in place of the selected text. The page uses `execCommand("insertText")`, so the undo of the browser works in one step. If the browser refuses the command, the page writes the text with `setRangeText` and sends the event `input`. The area keeps the focus. The three texts are these:

  - Item 0, "Values of the last 3 days, newest first": `SELECT service FROM events WHERE timestamp >= now() - INTERVAL 3 DAY GROUP BY service ORDER BY max(timestamp) DESC LIMIT 1000`, on four lines.
  - Item 1, "Values in the time range of the dashboard": `SELECT DISTINCT service FROM events WHERE $__timeFilter(timestamp) ORDER BY service LIMIT 1000`, on four lines. The item shows the hint "Set Refresh to On time range change." under its label.
  - Item 2, "Value and text, sorted": `SELECT DISTINCT toString(id) AS value, name AS text FROM services ORDER BY text LIMIT 1000`, on four lines.

- The names `events`, `service`, `timestamp`, `services`, `id` and `name` are names to change. Each text is valid ClickHouse SQL and passes the checks of 7.18. A front unit test checks the three texts with the lexer of the page (16.10a).
- A `query` variable has the button "Show options" (`variable-form-preview`) under its SQL area. It sends `POST api/sources/<id>/preview` with the member `variable` (6.22). The body holds the draft variable and its parents in `dashboard_variables`, and the values of the parents in `variables`. The values are the current values of the page, or the defaults of the draft for a parent that the page does not know. The body holds the time range of the page. The page applies `regex`, the removal of duplicates and `sort` to the answer (9.10, `buildOptions`).
- The status line (`variable-preview-status`, `aria-live="polite"`) says "<n> options" ("1 option" for one) or the error of the answer. For the error `invalid_document`, it adds the pointer and the message of each error (3 at most). For a source error with a line, it adds the line and the column. The list (`variable-preview-list`) shows the first 50 options as `value` and `text`, then the text "and <n> more".
- The button "Show options" is not active while a request runs. The block is absent for a source that the person cannot use (10.3). A change of the SQL, the source, `regex` or `sort` empties the result. The answer of a request that this change cancelled is not shown.
- The preview shows the first 50 rows only. It never changes the draft and never writes a value to a log. It writes each value, each option and each error with `textContent`, never with HTML (T79).
- A new dashboard that is not saved has no uid to name. For it, the button gives the notice "Save the dashboard to show the options." in the status line, and sends no request.
- A `text` variable has `value_type` and `default`, with the rule of 4.10.
- An `interval` variable has a text area "Intervals" (`variable-form-intervals`). It holds one text on each line, or texts that commas separate. The check of 4.10a shows its error under the area: a count of 1 to 50, an item of the grammar, no repeated item. The form splits the text at the commas and at the line ends. It does not copy the split of Grafana, which also cuts at a point and at a minus sign (19.20k).
- The hint under the area gives one ClickHouse example: `toStartOfInterval(ts, toIntervalSecond(parseTimeDelta('${bucket}')))`. The editor shows it for the kind `interval` only.
- An `interval` variable also has a switch "Auto option" (`variable-form-auto`). It has a field "Step count" (`variable-form-auto-count`, a whole number from 1 to 1,000) and a select "Min interval" (`variable-form-auto-min`, the 30 texts of 7.4a). Both are not active while the switch is off.
- The select "Default" (`variable-form-interval-default`) lists "First option" (`null`), "Auto" when `auto` is `true`, and the items. A change of the list or of the switch that removes the chosen default sets `default` to `null` in the same change.
- A new `interval` variable has `auto` off, `auto_count` 30, `auto_min` `10s`, `intervals` `["1m", "10m", "1h", "1d"]` and `default` `null`.
- An `adhoc` variable (4.10c) has the select `source` (`variable-form-source`) with the `clickhouse` sources only. It has the text area "Keys" (`variable-form-keys`) with one key on each line. The area refuses no key, more than 64 keys, a key that is not a plain column name and a repeated key. The checks at save compare the keys with the configured list (7.18). An `adhoc` variable has no group "Place in the grid".
- A new `adhoc` variable has the first `clickhouse` source and no key. The preview of a draft panel with `$__adhocFilters` sends the `adhoc` variable of its source too.
- A `constant` has a text field "Value" (`variable-form-value`, 0 to 1,024 bytes, no control character). The help text says: "The viewer never sees or changes this value." The field "Hide" shows `Variable` and is read only. A `constant` has no group "Place in the grid". A new `constant` has `hide` = `variable` and the value `""`.
- The group "Place in the grid" has a switch (`variable-form-placed`). A variable with `hide = "variable"` has no control, so its switch is not active and its tooltip says why. The select `hide` of a placed variable does not offer `variable`. Its tooltip says: "The variable is in the grid. Put it back in the top bar first."
- The switch on gives the placement of 10.6: the last section, `x = 0`, `y` = the bottom of that section, `w = 6`, `h = 2`. The switch off removes the member `placement`: the variable is back in the top bar.
- With the switch on, the form shows the select "Section", the text field "Title" and the fields `x`, `y`, `w` and `h` (4.7a). An empty title shows the label of the variable. The title has 256 characters at most. A new section gives the bottom of that section with the same size. A new place that overlaps a panel or a placed control moves the other items, as the grid rules of 10.5 say.
- A rename of a variable keeps its placement: the placement is a member of the variable. A delete of a variable removes its placement with it.
- The row of a placed variable shows the text "In the grid" and the section, under its name.
- A rename of a variable does not change the SQL. After a rename, the page shows a notice. The notice lists the panels whose SQL still names the old name (the scan of 9.10).
- A cycle of references gives the error `bad_reference` of 4.34. It shows under the list.
- The variables bar shows the changes of the variables only after a save (10.2). The draft grid shows the placements of the draft at once (9.10).
- A rename of a variable changes `repeat.variable` of each panel and section that names it, in the same draft change. The form shows no error for it.
- The switch `multi` of a variable is not active while a panel or a section of the draft repeats by this variable. Its tooltip says "A repeat uses this variable. It needs several values." The person removes the repeat first.
- A delete of a variable that a repeat names opens the dialog "Delete the variable <name>?". The dialog lists, by title, the panels and the sections that lose their repeat. The delete removes their member `repeat` in the same draft change (4.13a, rule 9).
- The select "Section" of the switch "Place in the grid" does not offer a section that has `repeat` (4.7a, rule 11). The item is not active, and its tooltip says "This section repeats."

Tab "Annotations" (`annotations`, 0 to 8 items; hook `settings-annotations`):

- A row shows the name, the source and the colour of an annotation. A click opens its form. The button "Add annotation" is not active at 8 items, and its tooltip says "A dashboard has 8 annotations at most.".
- The form has these controls: `name` (1 to 256 characters, unique: the error is "This name exists.") and `source`. The select `source` lists the sources of `GET api/sources` of the kinds `clickhouse`, `postgres` and `mongodb`. Its hint is "A ClickHouse, PostgreSQL or MongoDB source.".
- The form also has `sql` (the text area of 10.3, with no "Run") and `color` (the colour control of 10.3a). The syntax tree of `sql` has the dialect of the kind of the source (10.3c). The form has the switch `enabled` and the group "Panels".
- The group "Panels" has the choice "All time series" (no `panels` member) and the choice "Some panels". The second choice shows the list of the panels of the draft with the type `timeseries` and a check box for each one. It needs 1 panel or more: the last check box stays on, with the message "Choose one panel at least.".
- A new annotation has the colour `red`, `enabled` equal to `true` and the choice "All time series". Its source is the first ClickHouse source of the list, else the first source of the list. Its SQL template is ClickHouse SQL.
- The form has no "Run" and no preview, because the route of 6.22 has no annotation. The person saves and looks at the panel. The page shows the note of 9.19d for an error.
- The checks of the save show under the form. They are the checks of the kind of the source (7.18, 7.26 or 7.36) and the rules of 4.5b. An error names the line.
- A delete of a panel that an annotation lists removes its id from `panels` in the same draft change. When the list becomes empty, the change removes `panels` and sets `enabled` to `false`. The page then shows the notice "The annotation <name> had only this panel. It is off.". The dialog of the delete (10.6) lists the annotations that change. The delete of a section does the same for each panel of the section.
- A rename of a variable does not change the SQL. The notice of the rename also lists the annotations whose SQL still names the old name (the scan of 9.10).
- A change of the type of a panel keeps its id in `panels`. The annotation draws on the panel only while its type is `timeseries`.
- The order of the rows is the order of the array. The index of an annotation is its place in the array, so a move or a delete changes the index. The jobs of the page use the saved document and its etag, so a change of `annotations` in the draft acts after the save (10.9). A job never sends the index of a draft.

### 10.3c The syntax tree of the query text

The SQL area builds a minimal syntax tree of its text. The tree gives structure only. Four functions use it: the format (10.3d), the folds (10.3e), the current block and the bracket pair (10.3f).

Rules:

1. The tree is never a security check. The server is the only authority on the validity of a query (7, 11). A fault of the tree never stops a save, a run, a preview or a change of the draft. The tree adds no member to the document and sends no request.
2. The tree is built from the lexers that exist. The colours, the selection (case B57) and the tree then agree on comments, strings and names. The module `ast/tokens_ch.js` reads the spans of `app_sql_lex.js` and the code between them. The module `ast/tokens_pg.js` reads `lexPg` of `sql_lex_pg.js`. The module `ast/json_tree.js` has its own strict JSON reader with positions.
3. The dialect follows the kind of the source of the query. The kind `clickhouse` gives the ClickHouse rules. The kind `postgres` gives the PostgreSQL rules. The kind `mongodb` gives the JSON rules. A query with no source (a new panel) has the ClickHouse rules. A change of the source builds the tree again, because the page gives the editor a function that reads the kind of the source now.
4. A macro (`$__name` and `$__name(...)`) and a variable (`$name`, `${name}`, `${name:format}`, with the forms of 7.6) are opaque. The tree never splits them, and the format never writes them again. A literal, a name and a comment keep their bytes.
5. A position is a UTF-16 offset in the text, as `selectionStart` gives it. An `end` is not in its node.
6. The tree is tolerant. A text that is not complete or not correct gives a partial tree with nodes of the type `error`. It never gives an exception. A text that no dialect reads gives a tree that has one statement and errors.
7. The code is iterative, with an explicit stack and no recursion that depends on the input. The time is linear in the size of the text.

The tokens cover the text with no gap and no overlap. A token has `kind`, `start`, `end` and `closed`. The kinds are `blank`, `comment`, `string`, `name`, `word`, `number`, `punct`, `operator`, `macro`, `variable`, `param` and `other`. A `closed` value of `false` marks a string, a name or a comment that the text ends before it closes. A line comment ends before its line end. The next blank token holds the line end.

A PostgreSQL dollar quote (`$$` or `$tag$`), an `E` string and a prefixed string have the kind `string`.

The tree has these node types:

| Type | Content |
|---|---|
| `script` | The whole text, with its statements. |
| `statement` | The text up to a `;` that is code at the level of the script, with the `;`. |
| `with`, `cte` | A `WITH` clause and each item of it. A `cte` has a `name` and a body that is a `subquery`. |
| `query` | The clauses of one `SELECT`. A `UNION`, an `EXCEPT` or an `INTERSECT` stays a clause of the query. |
| `clause` | A keyword, and what follows it up to the next clause of the same query. `name` has the upper-case keywords with one space between them. |
| `subquery`, `paren`, `array`, `call` | A pair of brackets, with `open` and `close` (the offsets of the brackets). A `subquery` holds a query. A `call` is a word that touches its bracket. |
| `list`, `expr`, `literal`, `macro`, `variable`, `comment`, `error` | The parts of a clause. An `error` has `error` equal to `unclosed` or `unmatched`. |
| `doc`, `object`, `member`, `array`, `string`, `number`, `literal`, `error` | The JSON tree of a MongoDB query. A `member` has `name` and one value. |

- A node that has `open` and `close` on two lines or more has `foldable` equal to `true`. This holds for a subquery, a paren, a call, an array, an object and a JSON array.
- A node has `close` only when the text holds the bracket that ends it. If not, it has `error` equal to `unclosed`.
- Each bracket that is code belongs to exactly one node with `open` or `close`, or to an `error` node with `error` equal to `unmatched`. A bracket in a string, a name or a comment is not code.
- The clause names are listed for each dialect in the tables of `sql_tree.js`. Both dialects have `SELECT`, `FROM`, `WHERE`, `GROUP BY`, `HAVING`, `ORDER BY`, `LIMIT`, `OFFSET`, `WINDOW`, `UNION` with `ALL` or `DISTINCT`, `EXCEPT`, `INTERSECT`, `VALUES` and each kind of `JOIN`. ClickHouse also has `PREWHERE`, `QUALIFY`, `ARRAY JOIN`, `LEFT ARRAY JOIN`, `SETTINGS` and `FORMAT`. PostgreSQL also has `RETURNING`.
- A `WITH` that is not at the start of a query, as in `WITH FILL` or `WITH TOTALS`, is part of its clause. `RECURSIVE` after `WITH` is part of the `with` node.
- A lambda `x -> e` (ClickHouse), a cast `::` and a JSON operator (PostgreSQL) have no node of their own. They stay tokens of an expression.

Caps. The caps are constants of `ast/nodes.js`. They are not keys of the configuration, so the same text gives the same tree in each browser.

| Constant | Value | Applies to | Reason |
|---|---|---|---|
| `MAX_AST_INPUT_BYTES` | 262,144 | The UTF-8 size of the text. This is `MAX_SQL_BYTES` (4.35). | `too_large` |
| `MAX_AST_DEPTH` | 128 | The nesting of brackets and queries. | `too_deep` |
| `MAX_AST_NODES` | 100,000 | The nodes of one tree. | `too_many_nodes` |
| `MAX_FOLD_MARKERS` | 2,000 | The fold markers that the editor draws. The first 2,000 in the order of the text. | none |

- Above a cap the result has `capped` with `reason` and `at`. The format, the folds, the block and the pair stop, and the status line shows the notice of 10.3g. The colours go on. The save and the run go on.
- A text of exactly the cap value works. A text of one byte, one level or one node more is capped.
- The text of a source of the kind `mongodb` has the same caps. The server has its own limits (7.28). They are not the caps of the tree.

Time:

- The editor parses 150 ms after the last `input`, in `requestIdleCallback` with a time-out of 500 ms. Where this function does not exist (Safari), it parses in a `setTimeout` of 0 ms after the 150 ms.
- A key press never waits for the parse. At each `input` the editor removes the markers, the band and the pair marks at once. It draws them again when the parse of the new text ends.
- The editor reads the dialect when a parse starts. After a change of the source, the tree is built again at the next focus, key or parse.
- A result is valid for one version of the text. The editor holds a counter of versions and drops a result whose counter is old.
- The parse of 262,144 bytes takes 50 ms or less on the reference machine. The tokenizers read character codes and make no object for a blank. They compare a keyword by its codes or with a fixed table. They slice a text only for the name of a clause or of a call.
- A fold of a text of 262,144 bytes makes the browser lay out the page once. The fold reads the scroll position before it changes the page. It sets the scroll position of the folded view last.
- A layout pass of a page that holds a text of this size takes about 140 ms. The text area causes this cost, and no containment rule of CSS removes it. A fold, a key and a move of the caret start no layout pass. They change only layers that the browser lays out alone (10.3e).
- One fold takes 100 ms or less (measured: 9 ms for the handler and 33 ms with two frames). The case B176 asserts this. The unit test has a limit of 300 ms for the least of seven runs of the JavaScript of the fold.
- A fault of the colours (an exception of the highlighter) never stops a change of the draft. At each `input` the editor arms the `onChange` timer and the parse first, and it draws the colours last. After a fault the text stays plain and the status line shows the notice of a fault of the structure code (10.3g).
- The unit test of the time has a limit of 100 ms for the median of five runs. A loaded runner then does not fail it. The target stays 50 ms. The case B176 measures it in the browser.

The structure functions are in `ast/structure.js`. They are pure, and each one gives `null` or an empty result for a capped result.

- `folds(result, text)` gives `{ items, total, truncated }`. An item is a node with brackets on two lines or more. It has its offsets, its type and its number of lines. The items are in the order of the text. `items` has 2,000 folds at most, `total` counts all of them, and `truncated` is true when there are more. A macro never folds.
- `bracketPartner(result, offset)` gives the bracket before the offset, else the bracket at the offset, with its partner. It gives `null` for an offset outside the text.
- `blockAt(result, offset, text)` gives the current block of 10.3f. It needs the text to count the lines.
- The module also has the pure model of the folded view. It gives the outer folded ranges and the rows of the view. It also counts the non-blank characters that the caret of the format uses.

### 10.3d The format

The format changes only the blanks of the query text. The person chooses it. Nothing runs it at a save, at a key press, at a paste, at a blur or at a load.

Entry:

- The button "Format" (`sql-format`) is under the SQL area. The key `Ctrl+Shift+F` (`Cmd+Shift+F` on macOS) does the same (10.3g).
- With no selection, the format works on the whole text. With a selection, it works on each statement that the selection touches, and the rest of the text stays as it is. The smallest unit is a statement. A selection inside a subquery formats the whole statement of it.
- A JSON text (the kind `mongodb`) formats as one value. A selection is not used for it.

Refusal. The format changes nothing, and the status line says why, when one of these holds:

1. The tree has `capped`. The notice is the one of 10.3g.
2. The statements that the format reads have an `error` node, or a token with `closed` equal to `false`. The status line says "Format needs a complete text", with the line, the column and the reason. The reason is "a bracket has no match" or "a string, a name or a comment has no end". For a JSON text, the reason can be "the text is not valid JSON". The caret goes to this place.
3. The check of the result fails (below). The status line says "Format stopped: the check of the result failed. The text is not changed."
4. The statement does not start with `SELECT`, `WITH` or `VALUES`. The format keeps such a statement as it is. A script with only such statements says "Nothing to format."
5. The new text would have more than 262,144 bytes. The status line says "Format stopped: the result has more than 262,144 bytes. The text is not changed."

Properties. The tests check them (16.10a):

- The non-blank tokens before and after the format have the same kinds and the same texts. Only blank tokens change or appear. A comment, a macro, a variable, a string and a name keep their bytes.
- A line comment keeps its line end. A token that comes after a line comment is on a new line. The format never writes a comma or a bracket on the line of a line comment, so it never joins a comment and code.
- The format is idempotent: format(format(x)) equals format(x). The layout depends on the tokens, on the presence of a blank and on the line end before a comment. It does not depend on the number of blanks.
- The keywords keep their case. Only a blank token may change, so the format never writes `select` as `SELECT`.
- The format checks itself. It tokenizes its result with the same tokenizer and compares the non-blank tokens with those of the input. On a difference it returns the input and the reason of refusal 3.
- The format of an empty text, or of a text of blanks, gives "Nothing to format."
- The time is 300 ms or less for 262,144 bytes on the reference machine. The format is linear in the number of tokens.

The style of the SQL formats (the kinds `clickhouse` and `postgres`). A line has 80 columns at most, where a break point exists. A column is a UTF-16 unit. An indent step is 2 spaces and never a TAB.

1. A statement starts in column 0, and its `;` touches the last token. Each statement starts on a new line. A run of blank lines between two statements becomes one blank line. The format adds no blank line. The text has no blank line at its start. It ends with one line end only when the input ended with a line end.
2. Each clause keyword starts a line, at the indent of its query. The words of one keyword (`GROUP BY`, `LEFT JOIN`, `ARRAY JOIN`, `UNION ALL`) have one space between them.
3. A clause "fits" when its keyword and its flat body make a line of 80 columns or less. The flat body is the tokens of the clause, joined with the spaces of rule 9. A clause cannot fit when it holds a line comment, a token with a line end, or a subquery that does not fit (rule 7).
4. A clause that fits is one line: the keyword, one space and the body. `LIMIT`, `OFFSET` and `FORMAT` are always one line, also above 80 columns.
5. A clause that does not fit takes one of two forms. This is for a list clause (`SELECT`, `FROM`, `GROUP BY`, `ORDER BY`, `ARRAY JOIN`, `SETTINGS`, `WINDOW`, `WITH`, `VALUES`, `RETURNING`). With two items or more, the keyword is alone on its line. Each item has its own line, 2 spaces deeper, and a comma ends it. With one item, the item stays on the line of the keyword and its brackets break (rule 7).
6. A condition clause that does not fit puts its first operand on the line of the keyword. The clauses are `WHERE`, `PREWHERE`, `HAVING`, `QUALIFY` and the condition of `ON`. Each next operand starts a line, 2 spaces deeper, with its `AND` or `OR` first. The `AND` of `BETWEEN x AND y` does not break the line.
7. A pair of brackets fits when its flat form fits in the rest of the line. If it does not fit, the open bracket ends the line. The content goes 2 spaces deeper than that line. The close bracket starts a line at the indent of that line, and the text after it stays on that line. A subquery of two clauses or more always breaks. A subquery of one clause fits when its flat form fits.
8. A call, a paren, an array or a tuple that breaks has one item on each line, and a comma ends each item. If all its items are literals, variables or macros, the items fill the lines. Each line has as many as fit in 80 columns. A `CASE` that does not fit has its `WHEN` and `ELSE` lines 2 spaces deeper than its own line. `END` goes at the indent of that line.
9. The spaces. The format removes a blank after `(`, `[` and `{`, and before `)`, `]`, `}`, `,` and `;`. It writes one space after `,`. It writes one space on each side of `=`, `==`, `<>`, `!=`, `<`, `<=`, `>`, `>=` and `||`. For each other pair of tokens, it keeps a space where the input had a blank, and keeps no space where the input had none. Thus `x::Int`, `a.b`, `-1`, `f(x)` and `x -> y` stay as written.
10. `JOIN`. A join that fits is one line: `LEFT JOIN b ON a.id = b.id`. A join that does not fit keeps the keyword and the table on one line. Its `ON` or `USING` starts the next line, 2 spaces deeper, and its condition follows the rule 6 with a continuation of 4 spaces.
11. `SELECT DISTINCT` and `SELECT ALL` keep the modifier on the line of the keyword, as does `WITH RECURSIVE`. A `WITH` clause is a list clause: each item (`a AS (...)`) has its own line, and its subquery is 2 spaces deeper.
12. A comment. A comment that was on its own line stays on its own line, at the indent of the next token. A comment that followed a token on the same line stays after that token, with one space. A comment inside an item forces the clause to break. A token after a line comment starts a line, 2 spaces deeper than the line of the item.
13. A line can have more than 80 columns where no rule gives a break. An example is a long string or a long name.

ClickHouse. The clause keywords are the keywords of 10.3c. A `SETTINGS` clause is a list clause. A `{name:Type}` query parameter and a lambda follow rule 9. A `FORMAT` name stays on its line.

PostgreSQL. A dollar quote is one token. It can break the fit of its clause, because it has line ends. A `::` cast and a JSON operator (`->`, `->>`, `#>`, `@>`) follow rule 9. `WITH RECURSIVE`, `RETURNING` and `LATERAL JOIN` follow rules 5 and 10.

The style of the JSON format (the kind `mongodb`):

1. An object or an array that is empty is `{}` or `[]`. A key is followed by `: ` (a colon and one space). Items are separated by `, `.
2. An object or an array fits when its flat form fits in the rest of the line. If it does not fit, the open bracket ends the line. Each member or item has its own line, 2 spaces deeper, with a comma at its end. The close bracket starts a line at the indent of the line of the open bracket.
3. An array whose items are all numbers, strings or literals fills the lines, as in rule 8 of the SQL style.
4. The strings, the numbers and the keys keep their bytes, also the escapes and the case of `e` in a number. The order of the members does not change, and a duplicate member stays. The format refuses a text that is not valid JSON, with the message of the refusal 2. A placeholder is inside a string, so the format never changes it (7.34).

How the editor puts the result in the text area:

- The caret goes to the place that has the same number of non-blank characters before it. This is exact, because the non-blank tokens do not change. The scroll position stays. The selection of the person ends: a collapsed caret remains.
- The format ends the folded view first (10.3e).
- The format gives the least change: the range that differs and its new text. The editor writes only this range. The number of lines of this new text chooses the way of the write.
- A result of 500 lines or less is written with `document.execCommand("insertText")` on the selected range. One undo step, `Ctrl+Z`, then restores the text, and one redo step writes it again. The call is slow for a long text, because each line fires an `input` event (the prototype measured 374 to 443 ms for 500 lines).
- A result of more than 500 lines goes in with `setRangeText`. This clears the undo history of the browser. The editor then keeps an own entry with the text before and after.
- While the entry exists, `Ctrl+Z` and `Cmd+Z` restore the text before the format. `Ctrl+Shift+Z`, `Cmd+Shift+Z` and `Ctrl+Y` write the format again. The events `beforeinput` of the types `historyUndo` and `historyRedo` do the same. Any other `input` removes the entry. The status line says "Format undone." and "Format applied again."
- The tool row then shows the button "Undo format" (`sql-undo-format`) next to the status line. The button stays while the entry exists and the format is not undone. The status line says "Formatted. The undo history of the browser is cleared."
- The editor sets a flag during the write. The handler of `input` does not paint or schedule a parse for each event. It does this one time after the write.
- When `execCommand` returns `false`, or the text is not the result, the editor uses the own entry.
- The status line says "Formatted." after a result of 500 lines or less. It says "Nothing to change." when the result is the text, and then it writes nothing and makes no undo step.
- The change is a normal edit of the draft: the page sends `onChange` 300 ms after it (10.3). A format of a text in the draft does not run the query.

Example of the SQL style:

```
-- input
WITH recent AS (SELECT host, avg(cpu) AS cpu FROM metrics.cpu WHERE $__timeFilter(ts) AND host IN (${host}) GROUP BY host) SELECT r.host, cpu FROM recent AS r LEFT JOIN top ON r.host = top.host WHERE r.cpu > (SELECT avg(cpu) FROM recent) ORDER BY cpu DESC
-- output
WITH recent AS (
  SELECT host, avg(cpu) AS cpu
  FROM metrics.cpu
  WHERE $__timeFilter(ts) AND host IN (${host})
  GROUP BY host
)
SELECT r.host, cpu
FROM recent AS r
LEFT JOIN top ON r.host = top.host
WHERE r.cpu > (
  SELECT avg(cpu)
  FROM recent
)
ORDER BY cpu DESC
```

The format does not change the case of a keyword. An input with `with` and `select` gives an output with `with` and `select`.

### 10.3e The folds and the folded view

A fold hides the content of a pair of brackets that spans two lines or more. The text of the draft never changes for a fold.

What folds:

- A node with an open bracket and a close bracket on two lines or more can fold. The editor reads the brackets of the node and not the member `foldable`. A macro is opaque and never folds. A JSON text folds at each object and array of two lines or more. The folded range goes from the character after the open bracket to the character before the close bracket. The brackets stay visible.
- A folded range shows as a chip with the text `… N lines`. N is the number of lines of the node, from the line of the open bracket to the line of the close bracket. An example is `(… 12 lines)`.
- A fold in a fold stays folded when the outer fold opens. The view shows only the outer folded ranges.
- A text with an `error` node can fold at each node that has a close bracket. A capped tree has no folds.
- The editor draws at most 2,000 fold markers (10.3c). The notice "Only the first 2,000 blocks can fold." shows when there are more.

The text area:

- The text area keeps the focus and the text. Its value, its selection and its undo steps do not change for a fold (measured).
- The folded view is a layer over the text area and over the colour layer. It has the same font, the same size and the same line height of 20 px. It is `aria-hidden="true"`, as the colour layer is.
- The layer is after the text area in the page order and has an opaque background. It covers the caret, the selection and the scroll bar of the text area.
- A fold changes no style of the text area and no style of the colour layer. A style change of these two elements makes the browser lay out the whole text again.
- A change of the attribute `hidden` also makes the browser lay out the text area again. So does a change of the style `display` or of a text in the flow of the page. Each costs about 140 ms for 262,144 bytes.
- The folded view and the band show and hide with the style `visibility` and keep their box. The button "Unfold all" does the same, and it keeps its place at the end of the tool row. The status line and the text of the block are in an absolute box with a fixed height (`sql__live`). The browser lays out this box alone.
- A fold moves the caret out of a folded range. The caret goes to the offset just after the open bracket of the range.

The folded view:

- It shows the coloured text with each outer folded range replaced by its chip. A chip is inline and has the line height of the text. It is a target for the mouse and has the hook `sql-chip`.
- The gutter shows the real line numbers of the rows. The numbers jump over the hidden lines. A line that has a server error (10.3) keeps its mark. When the line is hidden, the mark is on the number of the row that hides it.
- The gutter has a column of markers. A marker is a `<button type="button">` with `aria-expanded`, the name "Fold lines 3 to 9" or "Unfold lines 3 to 9", `tabindex="-1"` and the hook `sql-fold`. It is on the row of the open bracket. A click toggles that node. The target is 24 px wide.
- The folded view scrolls by itself. The text area scroll is kept and used again when the view ends.
- The folded view and the gutter draw only the rows that are visible and 100 rows above and below. A gap above and a gap below keep the height of the other rows, so the scroll height is the height of all rows. A scroll that comes within 30 rows of the border of the drawn rows draws them again. The colours of the drawn rows come from the colour layer (10.3). A fold of a text of 262,144 bytes takes the same work as a fold of a small text.
- The page has no marker for a row that is not drawn. The attribute `data-count` of the group of markers has the number of all markers.
- The button "Unfold all" (`sql-unfold-all`) shows in the tool row only while a fold exists. When it does not show, it has `visibility: hidden` and keeps its place at the end of the row.

The keys and the mouse:

- A `beforeinput` on the text area ends the folded view first. The edit then runs natively at the mapped caret, with one undo step.
- The keys `ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`, `Home`, `End`, `PageUp` and `PageDown` end the folded view, and then they move the caret natively.
- A pointer down in the folded view maps the point to an offset with `caretPositionFromPoint`. Where this function is not there, the view uses `caretRangeFromPoint`. The pointer down sets the selection of the text area, and the focus stays there. A move with the button down extends the selection. The folded view paints the selection with `CSS.highlights`. A click on a chip toggles its fold.
- `Ctrl+C` stays native. It copies the full text of the selection, with the hidden lines (measured: a drag over two folds copied the 214 characters). A cut or a paste ends the folded view first.
- `Ctrl+A` selects the full text.
- The keys of 10.3g fold and unfold the node at the caret. A click on a marker keeps the focus where it was.
- "Run" and `Ctrl+Enter` work in the folded view, because the text is the same. `focusAt` (an error place, 10.3) and `setLines` end the folded view first.
- An `input` that is not a fold ends the folded view and clears the fold state.

The fold state:

- The fold state is not part of the draft, the document or the URL. It lives in the editor until the first edit, an external `setLines` or the end of the editor.
- The state is a set of offsets of open brackets. It is valid only while the text is the same. Any change of the text clears it.

Status messages: "Folded 1 block, 12 lines.", "Unfolded 1 block.", "Unfolded all.", "Nothing to fold here." and "Nothing is folded.". They go to the status line (10.3g).

### 10.3f The current block and the bracket pair

The current block is the innermost node of the type `subquery`, `cte` body or `clause` that holds the caret and spans two lines or more. When the caret is in none of them, there is no block. A band shows the block.

- The band is a layer under the colour layer. It is `aria-hidden="true"` and has the hook `sql-block` and `data-type`. It covers the whole lines of the block, from its first line to its last line, over the full width.
- The band has a fill of 4 % of the text colour (`--text`) and a bar of 2 px at its left edge in `--accent-fill`. Each colour of the text has 4.5:1 or more over the fill, in both themes. The lowest measured value is 4.54, for `--sql-comment` in the light theme. The bar has 4.09:1 in the dark theme and 6.04:1 in the light theme with the surface. The fill is only an aid, so it has no 3:1 rule. The bar is the sign.
- The band shows only when five things hold. The text area has the focus and the caret has no selection. The tree is valid for the text and has no `capped`. The text is not folded. The band moves 50 ms after a caret move (`selectionchange`). It does not show while the person types.
- The tool row says "Block: subquery, lines 3 to 9." for the current block. For a clause, it adds the name: "Block: clause WHERE, lines 3 to 9." The hook is `sql-info`. This line is not live.
- For a JSON text, the current block is the innermost `object` or `array` of two lines or more. A SQL array is not a block.

The bracket pair:

- The pair is the bracket before the caret, else the bracket at the caret, and its partner. Both brackets get a mark. A mark is an outline of 1 px in `--accent-fill` with an offset of -1 px, and no fill. It has 4.09:1 in the dark theme and 6.04:1 in the light theme with the surface, and it changes no text colour.
- The mark is a `span` with the class `sql__pair` inside the colour layer, made with a `Range` around one character. It has no padding, no border, no margin and no change of weight, so no glyph moves (measured: 0 px). The editor removes it and joins the text nodes again before it draws a new one. The hook is `sql-pair`.
- A bracket with no partner gets a mark in `--danger`. This is an `error` node with `unmatched`, or a node with no `close`. The tool row then says "This bracket has no match."
- The pair shows under the same conditions as the band.
- The key `Ctrl+Shift+\` moves the caret to the partner: after a close bracket, or before an open bracket. In the folded view it ends the folded view first.
- The marks are in the layer that the case B57 compares with the text area. B57 runs again with a mark and a band. The gap stays under 0.5 px.

### 10.3g Keys, notices and accessibility

The keys work only when the text area has the focus. The editor calls `preventDefault` only for a key that it handles. The keys use `event.code`, so the layout of the keyboard does not change them. A key with `Alt`, or with `Meta` where the table says `Ctrl` only, is not handled. A key that repeats (`event.repeat`) does not run the format.

| Key | Action |
|---|---|
| `Ctrl+Shift+F` (`Cmd+Shift+F` on macOS) | Format (10.3d). |
| `Ctrl+Shift+[` (code `BracketLeft`, `Ctrl` only) | Fold the innermost node at the caret. Unfold it when it is folded. |
| `Ctrl+Shift+]` (code `BracketRight`, `Ctrl` only) | Unfold all. |
| `Ctrl+Shift+\` (code `Backslash`, `Ctrl` only) | Jump to the partner of the bracket (10.3f). |
| `Tab`, `Escape` then `Tab`, `Ctrl+Enter` | As before (10.3). The new keys do not use them. |

- The keys do not take a key of the browser that a page cannot take back: `Ctrl+N`, `Ctrl+T`, `Ctrl+W` and `Ctrl+Tab`. They do not take the single keys of the screen readers (NVDA and JAWS use `Insert` or `CapsLock`, VoiceOver uses `Ctrl+Option`). They have a modifier, so they follow success criterion 2.1.4 of WCAG. `Cmd+Shift+[` and `Cmd+Shift+]` change tabs in the browsers of macOS, so the fold keys use `Ctrl` there too.
- The case B170 checks in Chrome that each key reaches the page and that `preventDefault` stops the browser. The check in Firefox and in Safari is a manual check before 1.0.0 (16.19).
- The text area has `aria-keyshortcuts` with `Control+Shift+F Control+Shift+BracketLeft Control+Shift+BracketRight Control+Shift+Backslash`. It has `aria-describedby` for the list of keys (`sql-keys`). This list is a short text under the area, with the keys of the table above.

The tool row is under the SQL area. It has the buttons "Format" and, while a fold exists, "Unfold all" (`btn btn--sm`).

The status line and the text of the block are in a box at the bottom of the row. A form can add buttons to the row (the option `tools` and the function `addTool` of the editor). They come after "Format", and "Undo format" and "Unfold all" come after them. The row does not change the height of the area, so B57 is not affected.

At 900 px or less the row wraps.

The status line (`sql-status`) has `aria-live="polite"` and shows one message at a time. The messages are these:

| When | Text |
|---|---|
| The tree has `too_large` | "Format and folds are off: the text has more than 262,144 bytes." |
| The tree has `too_deep` | "Format and folds are off: the text goes deeper than 128 levels." |
| The tree has `too_many_nodes` | "Format and folds are off: the text has more than 100,000 nodes." |
| More than 2,000 fold markers | "Only the first 2,000 blocks can fold." |
| The format ran | "Formatted.", "Nothing to change.", "Nothing to format." |
| The format refused | The messages of 10.3d, refusals 2 and 3. |
| A fold key ran | The messages of 10.3e. |
| A jump key ran | "No bracket at the caret.", "This bracket has no match." |
| The own undo of the format ran | "Format undone.", "Format applied again." |
| The structure code threw an exception | "Format and folds stopped: an error occurred in the structure code. The text is not changed." |

- A notice of a cap shows as soon as the parse ends, and it stays while the cap holds. The notice of a fault of the structure code stays until the text changes, and the structure keys do nothing for that text. The colours, the save and the run go on.
- A message that is not a cap goes away 6 s after it shows, or at the next message. A cap notice has no time-out.
- The page does not announce the band, the pair marks or the markers. The person can read the full text with the screen reader, because the text area holds it.

Accessibility. The rules of 9.26 apply. The text area is the only accessible text. The focus never leaves it for a fold, a format or a jump. The markers are named buttons that are not in the Tab order, and the keys are their equivalent. The fold and the band use no animation.

### 10.3h Option forms of the data types

The panel editor has a form for the options of each type with data (F43). The types are `timeseries`, `barchart`, `xychart`, `stat`, `bargauge`, `piechart` and `table`. The types `heatmap` and `histogram` get the same kind of form when F31 is built. The type `logs` has a form too (F33). The type `gauge` has its form too.

The types `state_timeline` and `status_history` keep the form of 10.3, with two small changes (rules 7 and 14). The type `text` keeps its field "Content" and has no other form, because `mode` has one value. The tab "JSON" stays and edits the same members.

A form writes only members that 4.18 and 4.27 to 4.33d list. A control of Grafana that has no such member has no control in Hune (19.20m).

1. One table for each type.
   - The code has one table of rows for each type, in `js/editor/forms/options/<type>.js`. One renderer (`js/editor/forms/option_form.js`) reads a table and builds the groups.
   - The tables below are the authority. The test FE-79 compares them with the code, with the members of 4.27 to 4.33d and with `newOptions` of `draft.js`.
2. The members of a row.
   - `id` is the name of the row, unique in its type. `member` is the path of the member from the panel. `kind` is the kind of control (rule 3). `label` is the visible text. `group` is the key of the group.
   - `items` is the list of values with their texts. `min`, `max` and `step` give a range. `default` is the value of a new panel (9.19).
   - `optional` and `absent` tell that the member is optional (rule 4). `when` is a pure function of the options, and `whenText` is its text in the table. `writes` lists other members that the row changes with its own member.
   - `build(ctx)` is the function of a row of the kind List or Old control. It returns the control. `ctx` has `scope`, `get`, `set` and `hints`.
3. The kinds of control. The renderer builds each kind with the helpers of `controls.js`.
   - Choice: a `<fieldset>` with a `<legend>` and one native radio input for each item. The radio is visually hidden and keeps the focus ring. The label of the radio is the segment, in the segmented look of the editor (26 px segments).
   - A choice is 280 px wide at most: 7.4 px for each character and 24 px for each item. A longer choice is a select.
   - Select: a native `<select>` with a `<label>`. A member that is optional has an item for "not set". This item removes the member.
   - Switch: the switch of `controls.js` with its own visible label.
   - Slider and number: a group with the name of the option. It has a slider (`<input type="range">`, step 1) and a number field (69 px wide, 32 px high). Both show the same value. The slider has the name "<label>, slider". The number field has the name "<label>, value".
   - Number and text: a `<label>` and a text field. A number field has `inputmode="decimal"`. An empty field writes `null` when the row is nullable. Else it shows the error of the range and writes nothing.
   - Check boxes: a `<fieldset>` with a `<legend>` and one check box for each item. The box scrolls when it is higher than 12 rem (`tabindex="0"`). At the limit of the row, the unchecked boxes are `disabled`. A hint says "<n> of <max> chosen".
   - List: the list editor of 10.3a. An item of the sort list has a field name and the switch `desc`. The field name is a text with the field names of the last result.
   - Select of fields: a select with an item for `null` and the field names of the last result. Check boxes of fields: one check box for each field name. A name in the draft that the result does not hold shows as "<name> (not in the data)".
   - Old control: a control of 10.3a that is already in the editor, with its own hook. The renderer places it and does not build it again.
4. The write rule.
   - A control writes one member with one valid value, in the same task as the event (rule 7). A value that breaks the range of the row is an error under the control, and nothing is written. The page never changes a value to make it fit.
   - An optional member is written when its value is not its `absent` value. When the value is `absent`, the member leaves the panel. The members of `options` keep the order of the member table of the type (4.27 to 4.33d, 4.38). They do not follow the order of the groups.
   - The write of the legend `sort_by` "None" also removes `sort_desc`. The write of the heatmap scale `log2` or `log10` also writes `y_buckets.mode` = `size` (4.33c, rule 2). The list `writes` of a row names such extra members.
5. A value that the row does not know.
   - A value outside the row (for example `draw` = `"x"` from the tab "JSON") shows with no item chosen. The control has `aria-invalid="true"`.
   - The error "The value <v> is not valid. Choose a value." shows under the control. The check of `validate_only` (10.3) gives the same error in the group of errors.
6. The groups.
   - The groups of a type come after "Visualisation" and before "Standard options", in the order of its table below. This is the order of Grafana (19.20m).
   - The groups of the type are open at the first draw, and the page keeps the state of each group across redraws. The group "Visualisation" is closed at the first draw. Its group is 390 px high. The groups of the type start 769 px from the top of a window of 900 px, so they are under the fold. The link "Change" opens it (10.3), as in Grafana.
   - The group "Thresholds" gets the row "Show thresholds" of `timeseries` and `barchart`, at its end. The group "Standard options" gets the row "Display name" of each type, after "Max" and before "No value".
   - The old groups "Axis" and "Series style" of a `timeseries` leave their place after "Colour". Their controls join the groups "Axis" and "Graph styles" with the same hooks.
7. The live preview rule.
   - Each change of a control writes the draft and draws the preview in the same task. The write calls `apply`, which calls `E.change` and `drawTop` (10.3). The frame of the preview is a new element when the event ends. The canvas is drawn in the next two frames.
   - A slider writes at each `input` event, and at most once in each frame (`requestAnimationFrame`). A number field and a text field write 300 ms after the last key, as in 10.3a.
   - The key `Enter`, a blur or a click on another control writes at once. A change is one task of 100 ms or less in the preview of B205.
   - The attribute `data-draft-writes` counts the writes.
8. The rules of display.
   - A row with a `when` shows only while the function returns `true`. A row that a rule hides has `hidden`, `data-hidden-by-rule="true"` and no place in the Tab order.
   - A hidden row keeps its member in the draft. The page never removes a member because its row is hidden.
   - The "state" of a type is a set of option values. It gives a set of shown rows. The test B206 proves that a hidden row has no effect in the preview (rule 15).
9. The status line.
   - When a change shows or hides rows, the page writes one sentence in the line `options-status` (`role="status"`, class `visually-hidden`): "Shown: <labels>. Hidden: <labels>."
   - A list with more than 3 labels has the count: "Shown: 4 options." A part with no row is left out. The line is empty when no row changes.
   - The page does not move the focus. If the row that has the focus becomes hidden, the focus goes to the head of its group.
10. The keys.
   - The options pane follows the order of the DOM, which is the visual order. A radio group takes one stop of the Tab key. The arrow keys, `Home` and `End` move the choice, and each move writes.
   - The key `Space` toggles a switch and a check box. A slider has the keys of a native range: the arrows move by 1, and `Home` and `End` go to the ends.
   - The key `Escape` in a number field puts back the value of the draft. The key `Enter` writes at once.
11. The look.
   - The sizes are those that the Grafana editor has (19.20m). A control is 32 px high, and a label is 12 px at weight 500. A segment is 26 px high, a switch is 32 by 16 px, and a number field next to a slider is 69 px wide.
   - The colours come from the tokens of the theme, in light and dark. The pressed segment has a fill and weight 500, so colour is not the only sign.
   - At 900 px or less, the pane is one column and has no fixed place (10.3). At 390 px a choice that is too wide wraps to a second line. The page never has a horizontal scroll bar.
12. The hooks.
   - A control has `data-testid="opt-<id>"`. The root of each control has `data-option` with the path of its member (9.29).
   - The old controls get the attribute too. These are the controls of "Panel options", "Position", "Repeat options" and "Standard options". The roots of the forms of 10.3a get it too.
   - The test B153 uses `data-option` to find the member of a control.
13. The types that change. A change of type resets the options to the "New" values (10.6). The form of the new type shows them. The dialogs of 10.3 stay.
14. The audit (B153). The audit proves that each control works. Its rule has these parts.
   - States. For each type, the audit starts with the options of a new panel. It adds a state for each row that no earlier state shows. It makes the state with one change of a choice, a switch or a select, or with two changes when one is not enough. The legend with the mode `table` and a `sort_by` is a state.
   - Probes. In each state, the audit changes each shown control to a value that differs from its value, as a person does.
   - A choice gives one probe for each other item. A check list gives one probe for each of the first two boxes. A list gets one probe: its button "Add", on a fresh page.
   - A slider, a number and a select get up to 4 values. The audit stops at the first value that changes the preview. After the probe, the audit puts the old value back.
   - The hover probe. For a control with a member of `tooltip`, the audit moves the pointer to the plot before each read of the preview. It does it again after each change. The print of the preview then holds the card.
   - F1, the draft. The members that changed in the panel MUST be the member of the control and the members in the `writes` of its row. Else B153 fails. The member of a control is the `data-option` of its root. An old control may change members under its prefix only.
   - F2, the preview. The print of the preview MUST change within 1.5 s. The print is the markup of the frame without ids, a hash of each canvas, and the card. Else B153 fails, unless the control is in the closed list (F4).
   - F3, the round trip. After the old value is put back, the print MUST equal the first print. Else B153 fails with the message "the preview is not stable". This closes the case of a print that changes by itself.
   - F4, the closed list. An entry has a member (or a member and a value, `options.tooltip.mode=multi`), the types, a class and a cause of 20 characters or more. The class is `design` (the control cannot change the picture by design) or `data` (the data of the audit does not show the effect).
   - An entry covers its member and the members under it. An entry for a control that changes the preview makes B153 fail (a stale entry). An entry that no control matches makes B153 fail (a dead entry).
   - The list has 30 entries at most. A new entry needs a reviewed change of the list.
   - F5, the coverage. In each state, the controls of the pane MUST be the rows that the table shows. A row with no control, a control with no row and a control with no `data-option` make B153 fail.
   - A member of `newOptions(type)`, or an optional member of 4.26, MUST have a row or an entry in NO_CONTROL. Else B153 fails. A page that has no form for a type fails the audit.
   - F6, no skip. A control that the audit cannot try fails with the message "not tried". The only exception is a select with one item.
   - The rule is a pure function in `tests/browser/lib/audit_rules.mjs`. A front test gives it synthetic results (FE-79).
   - The audit data has two series, a null and no zero. So `tooltip.hide_zeros` is in the list as `data`.
15. The check of the hidden rows (B206).
   - For each row with a `when`, take a state where the row is hidden. The case writes the member through the tab "JSON" and compares the print.
   - The print MUST NOT change. A row that has an effect while hidden has a wrong rule, and the case fails.

#### The tables of the types

The groups of a type come in the order of its table. The rows with "Old control" keep the control and the hook of 10.3a. The column "New" is the value of a new panel (9.19). The word "absent" means that the member is optional and a new panel has none. A choice shows the text of the item. A row shows when its rule in "Shown when" is true.

##### The calculations

The check lists and the selects of calculations (the reduction, the legend values, the sort of the legend, the footer) use these texts. They are the words of Grafana 12.4.2. The value is the member of 4.26.

| Value | Text | Value | Text | Value | Text |
|---|---|---|---|---|---|
| `last_not_null` | Last * | `first_not_null` | First * | `last` | Last |
| `first` | First | `min` | Min | `max` | Max |
| `mean` | Mean | `total` | Total | `count` | Count |
| `median` | Median | `std_dev` | StdDev | `variance` | Variance |
| `range` | Range | `diff` | Difference | `diff_percent` | Difference percent |
| `delta` | Delta | `distinct_count` | Distinct Count | `change_count` | Change Count |

The select "Sort by" of a legend has the item "None" (the member leaves the panel), the item "Name" and these 18 items. The legend of a pie chart has "None", "Name", "Value" and "Percent".

##### timeseries (4.28)

Groups: Tooltip, Legend, Axis, Graph styles. The group "Thresholds" gets "Show thresholds". The group "Standard options" gets "Display name". The group "Axis" has the old controls "Placement", "Label", "Soft min" and "Soft max", and the new choice "Scale". The group "Graph styles" has the old controls "Line style" and "Transform", and the rows of the notes below.

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Tooltip | Tooltip mode | `tooltip.mode` | Choice | `single` "Single", `multi` "All", `hidden` "Hidden" | `single` | always | `opt-tooltip-mode` |
| Tooltip | Values sort order | `tooltip.sort` | Choice | `none` "None", `asc` "Ascending", `desc` "Descending" | absent | `tooltip.mode` is `multi` | `opt-tooltip-sort` |
| Tooltip | Hide zeros | `tooltip.hide_zeros` | Switch |  | absent | `tooltip.mode` is `multi` | `opt-tooltip-hide-zeros` |
| Legend | Visibility | `legend.show` | Switch |  | `true` | always | `opt-legend-show` |
| Legend | Mode | `legend.mode` | Choice | `list` "List", `table` "Table" | `list` | `legend.show` is `true` | `opt-legend-mode` |
| Legend | Placement | `legend.placement` | Choice | `bottom` "Bottom", `right` "Right" | `bottom` | `legend.show` is `true` | `opt-legend-placement` |
| Legend | Values | `legend.values` | Check boxes | 0 to 5 of: 18 calculations of 4.26 | `[]` | `legend.mode` is `table` | `opt-legend-values` |
| Legend | Sort by | `legend.sort_by` | Select | 18 calculations of 4.26 and `name` | absent | `legend.mode` is `table` | `opt-legend-sort-by` |
| Legend | Sort descending | `legend.sort_desc` | Switch |  | absent | `legend.mode` is `table` and `legend.sort_by` is set | `opt-legend-sort-desc` |
| Axis | Placement | `defaults.axis_placement` | Old control |  | absent | always | `defaults-axis_placement` |
| Axis | Label | `defaults.axis_label` | Old control |  | absent | always | `defaults-axis_label` |
| Axis | Width | `defaults.axis_width` | Number | 0 to 500, or empty | absent | always | `opt-axis-width` |
| Axis | Color | `defaults.axis_color` | Choice | `text` "Text", `series` "Series" | absent | always | `opt-axis-color` |
| Axis | Show border | `defaults.axis_border` | Switch |  | absent | always | `opt-axis-border` |
| Axis | Scale | `y_scale` | Choice | `linear` "Linear", `log` "Logarithmic" | `linear` | always | `opt-y-scale` |
| Axis | Log base | `log_base` | Choice | `2` "2", `10` "10" | absent | `y_scale` is `log` | `opt-log-base` |
| Axis | Centered zero | `defaults.axis_centered_zero` | Switch |  | absent | `y_scale` is not `log` | `opt-axis-centered-zero` |
| Axis | Soft min | `defaults.soft_min` | Old control |  | absent | always | `defaults-soft_min` |
| Axis | Soft max | `defaults.soft_max` | Old control |  | absent | always | `defaults-soft_max` |
| Graph styles | Style | `draw` | Choice | `line` "Lines", `bars` "Bars", `points` "Points" | `line` | always | `opt-draw` |
| Graph styles | Line interpolation | `interpolation` | Select | `linear` "Linear", `smooth` "Smooth", `step_before` "Step before", `step_after` "Step after" | `linear` | `draw` is `line` | `opt-interpolation` |
| Graph styles | Bar alignment | `bar_alignment` | Choice | `before` "Before", `center` "Center", `after` "After" | absent | `draw` is `bars` | `opt-bar-alignment` |
| Graph styles | Line width | `line_width` | Slider and number | 0 to 10 | `1` | `draw` is not `points` | `opt-line-width` |
| Graph styles | Fill opacity | `fill_opacity` | Slider and number | 0 to 100 | `0` | `draw` is not `points` | `opt-fill-opacity` |
| Graph styles | Gradient mode | `gradient_mode` | Choice | `none` "None", `opacity` "Opacity", `hue` "Hue", `scheme` "Scheme" | absent | always | `opt-gradient-mode` |
| Graph styles | Line style | `defaults.line_dash` | Old control |  | absent | `draw` is `line` | `defaults-line_dash` |
| Graph styles | Dash array | `defaults.line_dash_array` | Text | 1 to 8 whole numbers from 0 to 100, as `10, 20`, or empty | absent | `draw` is `line` | `opt-dash-array` |
| Graph styles | Connect null values | `span_nulls` | Switch |  | `false` | `draw` is `line` | `opt-span-nulls` |
| Graph styles | Null threshold | `span_nulls_ms` | Number | 1 to 1000000000000 ms, or empty | absent | `draw` is `line` and `span_nulls` is `false` | `opt-span-nulls-ms` |
| Graph styles | Show points | `show_points` | Choice | `auto` "Auto", `always` "Always", `never` "Never" | `auto` | `draw` is `line` | `opt-show-points` |
| Graph styles | Point size | `point_size` | Slider and number | 1 to 40 | `5` | `draw` is `points`, or `draw` is `line` and `show_points` is not `never` | `opt-point-size` |
| Graph styles | Stack series | `stacking` | Choice | `none` "Off", `normal` "Normal", `percent` "100%" | `none` | always | `opt-stacking` |
| Graph styles | Transform | `defaults.transform` | Old control |  | absent | always | `defaults-transform` |
| Thresholds | Show thresholds | `thresholds_style` | Select | `off` "Off", `line` "As lines", `dashed` "As lines (dashed)", `area` "As filled regions", `line_area` "As filled regions and lines", `dashed_area` "As filled regions and lines (dashed)" | absent | always | `opt-thresholds-style` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

Notes. The width of `fill_opacity` and `line_width` applies to bars as the line and the fill of a bar (9.19). `show_points` and `point_size` have no effect with `draw` = `bars`, because the engine draws marks on a line and on points only.

The rows "Width", "Color", "Show border", "Log base", "Centered zero", "Bar alignment", "Gradient mode" and "Null threshold" are new. Each one writes an optional member, and the value of an absent member removes it. The rows "Width", "Color" and "Show border" come after "Label", as in Grafana. The rows "Log base" and "Centered zero" come after "Scale". The rows "Bar alignment", "Gradient mode" and "Null threshold" follow "Line interpolation", "Fill opacity" and "Connect null values".

- The row "Dash array" is a text, as `10, 20`. Grafana offers the arrays `10, 10` to `50, 10` for a dash and `0, 10` to `0, 3, 3` for dots, and the field takes these texts too. An empty field removes the member.
- The row "Log base" shows only with a log scale, and the row "Centered zero" only with a linear scale. Each one has no effect on the other scale.
- The row "Null threshold" is a number of ms. It shows when `span_nulls` is `false`, because `true` joins each run of nulls.
- A new time series has `fill_opacity` 0, so the choice "Opacity" of "Gradient mode" draws no fill there. Thus the closed list of the audit has the entry `options.gradient_mode=opacity` for `timeseries` (class `data`, rule 14).

##### barchart (4.30)

Groups: Bar chart, Tooltip, Legend, Axis. The group "Thresholds" gets "Show thresholds". The group "Standard options" gets "Display name".

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Bar chart | X Axis | `x_field` | Select of fields | `null` "First string or time field", or a field name | `null` | always | `opt-x-field` |
| Bar chart | Orientation | `orientation` | Choice | `auto` "Auto", `horizontal` "Horizontal", `vertical` "Vertical" | `auto` | always | `opt-orientation` |
| Bar chart | Rotate x-axis tick labels | `x_tick_rotation` | Slider and number | -90 to 90 | `0` | `orientation` is not `horizontal` | `opt-x-tick-rotation` |
| Bar chart | Show values | `show_values` | Choice | `auto` "Auto", `always` "Always", `never` "Never" | `auto` | `stacking` is `none` | `opt-show-values` |
| Bar chart | Stacking | `stacking` | Choice | `none` "Off", `normal` "Normal", `percent` "100%" | `none` | always | `opt-stacking` |
| Bar chart | Group width | `group_width` | Slider and number | 10 to 100 | `70` | `stacking` is `none` | `opt-group-width` |
| Bar chart | Bar width | `bar_width` | Slider and number | 10 to 100 | `97` | always | `opt-bar-width` |
| Bar chart | Line width | `line_width` | Slider and number | 0 to 10 | absent | always | `opt-line-width` |
| Bar chart | Fill opacity | `fill_opacity` | Slider and number | 0 to 100 | absent | always | `opt-fill-opacity` |
| Bar chart | Gradient mode | `gradient_mode` | Choice | `none` "None", `opacity` "Opacity", `hue` "Hue", `scheme` "Scheme" | absent | always | `opt-gradient-mode` |
| Tooltip | Tooltip mode | `tooltip.mode` | Choice | `single` "Single", `multi` "All", `hidden` "Hidden" | `single` | always | `opt-tooltip-mode` |
| Tooltip | Values sort order | `tooltip.sort` | Choice | `none` "None", `asc` "Ascending", `desc` "Descending" | absent | `tooltip.mode` is `multi` | `opt-tooltip-sort` |
| Tooltip | Hide zeros | `tooltip.hide_zeros` | Switch |  | absent | `tooltip.mode` is `multi` | `opt-tooltip-hide-zeros` |
| Legend | Visibility | `legend.show` | Switch |  | `true` | always | `opt-legend-show` |
| Legend | Mode | `legend.mode` | Choice | `list` "List", `table` "Table" | `list` | `legend.show` is `true` | `opt-legend-mode` |
| Legend | Placement | `legend.placement` | Choice | `bottom` "Bottom", `right` "Right" | `bottom` | `legend.show` is `true` | `opt-legend-placement` |
| Legend | Values | `legend.values` | Check boxes | 0 to 5 of: 18 calculations of 4.26 | `[]` | `legend.mode` is `table` | `opt-legend-values` |
| Legend | Sort by | `legend.sort_by` | Select | 18 calculations of 4.26 and `name` | absent | `legend.mode` is `table` | `opt-legend-sort-by` |
| Legend | Sort descending | `legend.sort_desc` | Switch |  | absent | `legend.mode` is `table` and `legend.sort_by` is set | `opt-legend-sort-desc` |
| Axis | Label | `defaults.axis_label` | Old control |  | absent | always | `defaults-axis_label` |
| Axis | Width | `defaults.axis_width` | Number | 0 to 500, or empty | absent | always | `opt-axis-width` |
| Axis | Show grid lines | `defaults.axis_grid` | Switch |  | absent | always | `opt-axis-grid` |
| Axis | Color | `defaults.axis_color` | Choice | `text` "Text", `series` "Series" | absent | always | `opt-axis-color` |
| Axis | Show border | `defaults.axis_border` | Switch |  | absent | always | `opt-axis-border` |
| Axis | Centered zero | `defaults.axis_centered_zero` | Switch |  | absent | always | `opt-axis-centered-zero` |
| Axis | Soft min | `defaults.soft_min` | Old control |  | absent | always | `defaults-soft_min` |
| Axis | Soft max | `defaults.soft_max` | Old control |  | absent | always | `defaults-soft_max` |
| Thresholds | Show thresholds | `thresholds_style` | Select | `off` "Off", `line` "As lines", `dashed` "As lines (dashed)", `area` "As filled regions", `line_area` "As filled regions and lines", `dashed_area` "As filled regions and lines (dashed)" | absent | always | `opt-thresholds-style` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

Notes. `show_values` has no effect with a stack, because a stack has no value label (9.19). `group_width` has no effect with a stack, because a stack is one group. `x_tick_rotation` has no effect with `orientation` = `horizontal`, because that picture has the category labels at the left. A bar chart has no `axis_placement` (4.18).

The rows of the bar chart: "Line width" and "Fill opacity" come after "Bar width", in the order of Grafana. A slider at the default (1 and 80) removes the member. Thus the absent member and the default are one state. "Show grid lines" is a switch, where Grafana has the choice Auto, On and Off. A bar chart has one value axis, so On draws the same picture as Auto. A third item would be a control that changes nothing (rule 14, F2).

The rows "Gradient mode", "Width", "Color", "Show border" and "Centered zero" have the rules of the time series. The row "Gradient mode" follows "Fill opacity". The rows "Width", "Color", "Show border" and "Centered zero" take the order of Grafana around "Show grid lines". A bar chart has no log scale, so the row "Centered zero" always shows.

The switch is on when the member is absent or `true`. Off writes `false`, and on removes the member. The preview of the audit is wider than it is high, so the choice "Vertical" draws the same picture as the default "Auto". Thus the closed list has the entry `options.orientation=vertical` for `barchart` (class `data`).

##### xychart (4.32)

Groups: XY chart, Tooltip, Legend. The type has no axis group, because the axes have the names of the fields (9.19).

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| XY chart | X field | `x_field` | Select of fields | `null` "First field of type number", or a field name | `null` | always | `opt-x-field` |
| XY chart | Y fields | `y_fields` | Check boxes of fields | 0 to 32 field names | `[]` | always | `opt-y-fields` |
| XY chart | Show | `draw` | Choice | `points` "Points", `lines` "Lines", `points_and_lines` "Both" | `points` | always | `opt-draw` |
| XY chart | Point size | `point_size` | Slider and number | 1 to 40 | `5` | `draw` is not `lines` | `opt-point-size` |
| XY chart | Line width | `line_width` | Slider and number | 0 to 10 | `1` | `draw` is not `points` | `opt-line-width` |
| Tooltip | Tooltip mode | `tooltip.mode` | Choice | `single` "Single", `multi` "All", `hidden` "Hidden" | `single` | always | `opt-tooltip-mode` |
| Tooltip | Values sort order | `tooltip.sort` | Choice | `none` "None", `asc` "Ascending", `desc` "Descending" | absent | `tooltip.mode` is `multi` | `opt-tooltip-sort` |
| Tooltip | Hide zeros | `tooltip.hide_zeros` | Switch |  | absent | `tooltip.mode` is `multi` | `opt-tooltip-hide-zeros` |
| Legend | Visibility | `legend.show` | Switch |  | `true` | always | `opt-legend-show` |
| Legend | Mode | `legend.mode` | Choice | `list` "List", `table` "Table" | `list` | `legend.show` is `true` | `opt-legend-mode` |
| Legend | Placement | `legend.placement` | Choice | `bottom` "Bottom", `right` "Right" | `bottom` | `legend.show` is `true` | `opt-legend-placement` |
| Legend | Values | `legend.values` | Check boxes | 0 to 5 of: 18 calculations of 4.26 | `[]` | `legend.mode` is `table` | `opt-legend-values` |
| Legend | Sort by | `legend.sort_by` | Select | 18 calculations of 4.26 and `name` | absent | `legend.mode` is `table` | `opt-legend-sort-by` |
| Legend | Sort descending | `legend.sort_desc` | Switch |  | absent | `legend.mode` is `table` and `legend.sort_by` is set | `opt-legend-sort-desc` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

##### stat (4.29)

Groups: Value options, Stat styles, Text size. The group "Standard options" gets "Display name".

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Value options | Show | `reduce.values` | Choice | `false` "Calculate", `true` "All values" | `false` | always | `opt-reduce-values` |
| Value options | Calculation | `reduce.calc` | Select | 18 calculations of 4.26 | `last_not_null` | `reduce.values` is `false` | `opt-reduce-calc` |
| Value options | Fields | `reduce.fields` | Text | a number or empty | `null` | always | `opt-reduce-fields` |
| Stat styles | Orientation | `orientation` | Choice | `auto` "Auto", `horizontal` "Horizontal", `vertical` "Vertical" | `auto` | always | `opt-orientation` |
| Stat styles | Text mode | `text_mode` | Select | `auto` "Auto", `value` "Value", `value_and_name` "Value and name", `name` "Name", `none` "None" | `auto` | always | `opt-text-mode` |
| Stat styles | Color mode | `color_mode` | Select | `none` "None", `value` "Value", `background` "Background" | `value` | always | `opt-color-mode` |
| Stat styles | Graph mode | `graph_mode` | Choice | `none` "None", `area` "Area" | `area` | always | `opt-graph-mode` |
| Stat styles | Text alignment | `justify` | Choice | `auto` "Auto", `center` "Center" | absent | always | `opt-justify` |
| Stat styles | Show percent change | `percent_change` | Switch |  | absent | `reduce.values` is `false` | `opt-percent-change` |
| Stat styles | Percent change color mode | `percent_change_color` | Select | `standard` "Standard", `inverted` "Inverted", `same_as_value` "Same as Value" | absent | `reduce.values` is `false` and `percent_change` is `true` | `opt-percent-change-color` |
| Text size | Title | `title_size` | Number | 1 to 200, or empty | absent | `text_mode` is `auto` or `value_and_name` | `opt-title-size` |
| Text size | Value | `value_size` | Number | 1 to 200, or empty | absent | always | `opt-value-size` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

The rows of the stat have the labels and the order of Grafana 12.4.2. The row "Show percent change" shows only with a calculation, as in Grafana, because an item of each row has no change. The row "Percent change color mode" shows only when the change shows. The row "Title" shows only for a text mode that can show a name (rule 14), and Grafana shows it in each mode. An empty field "Title" or "Value" removes the member, and the page fits the size. Grafana also has the field "Percent change" (`text.percentSize`), and Hune has no row for it (15.6).

The control "Fields" is a text field. An empty text writes `null`. A text that is not a pattern of 4.24 gives the error "The pattern is not valid." (10.3a).

##### bargauge (4.33)

Groups: Value options, Bar gauge. The group "Standard options" gets "Display name".

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Value options | Show | `reduce.values` | Choice | `false` "Calculate", `true` "All values" | `false` | always | `opt-reduce-values` |
| Value options | Calculation | `reduce.calc` | Select | 18 calculations of 4.26 | `last_not_null` | `reduce.values` is `false` | `opt-reduce-calc` |
| Value options | Fields | `reduce.fields` | Text | a number or empty | `null` | always | `opt-reduce-fields` |
| Bar gauge | Orientation | `orientation` | Choice | `horizontal` "Horizontal", `vertical` "Vertical" | `horizontal` | always | `opt-orientation` |
| Bar gauge | Display mode | `display_mode` | Choice | `gradient` "Gradient", `lcd` "Retro LCD", `basic` "Basic" | `gradient` | always | `opt-display-mode` |
| Bar gauge | Show unfilled area | `show_unfilled` | Switch |  | `true` | always | `opt-show-unfilled` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

The row "Show unfilled area" shows for each display mode, because the bar gauge of Hune draws the unfilled track in the mode `lcd` too. Grafana hides this control for `lcd` (19.20m).

##### piechart (4.31)

Groups: Value options, Pie chart, Tooltip, Legend. The group "Standard options" gets "Display name".

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Value options | Show | `reduce.values` | Choice | `false` "Calculate", `true` "All values" | `false` | always | `opt-reduce-values` |
| Value options | Calculation | `reduce.calc` | Select | 18 calculations of 4.26 | `last_not_null` | `reduce.values` is `false` | `opt-reduce-calc` |
| Value options | Fields | `reduce.fields` | Text | a number or empty | `null` | always | `opt-reduce-fields` |
| Pie chart | Pie chart type | `pie_type` | Choice | `pie` "Pie", `donut` "Donut" | `pie` | always | `opt-pie-type` |
| Pie chart | Labels | `labels` | Check boxes | 0 to 3 of: `name` "Name", `value` "Value", `percent` "Percent" | `[]` | always | `opt-labels` |
| Tooltip | Tooltip mode | `tooltip.mode` | Choice | `single` "Single", `multi` "All", `hidden` "Hidden" | `single` | always | `opt-tooltip-mode` |
| Legend | Visibility | `legend.show` | Switch |  | `true` | always | `opt-legend-show` |
| Legend | Mode | `legend.mode` | Choice | `list` "List", `table` "Table" | `list` | `legend.show` is `true` | `opt-legend-mode` |
| Legend | Placement | `legend.placement` | Choice | `bottom` "Bottom", `right` "Right" | `right` | `legend.show` is `true` | `opt-legend-placement` |
| Legend | Legend values | `legend.values` | Check boxes | 0 to 2 of: `value` "Value", `percent` "Percent" | `[]` | `legend.mode` is `table` | `opt-legend-values` |
| Legend | Sort by | `legend.sort_by` | Select | `name` "Name", `value` "Value", `percent` "Percent" | absent | `legend.mode` is `table` | `opt-legend-sort-by` |
| Legend | Sort descending | `legend.sort_desc` | Switch |  | absent | `legend.mode` is `table` and `legend.sort_by` is set | `opt-legend-sort-desc` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

A pie chart shows one slice under the pointer, so `tooltip.sort` and `tooltip.hide_zeros` have no row (NO_CONTROL below). The members stay valid in the format. The choice "All" of the tooltip equals "Single" in a pie chart.

##### table (4.27, 4.18)

Groups: Table, Table footer, Cell options. The group "Standard options" gets "Display name". The rows with `defaults.` in the member are the members of `fields.defaults` that were not in the editor.

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Table | Show table header | `show_header` | Switch |  | `true` | always | `opt-show-header` |
| Table | Sort by | `sort` | List | 0 to 8 sort keys (4.27) | `[]` | always | `opt-sort` |
| Table | Column width | `defaults.width` | Number | 10 to 4000, or empty | absent | always | `opt-col-width` |
| Table | Column alignment | `defaults.align` | Choice | `auto` "Auto", `left` "Left", `center` "Center", `right` "Right" | absent | always | `opt-col-align` |
| Table | Wrap text | `wrap` | Switch |  | absent | always | `opt-wrap` |
| Table | Column filter | `defaults.filterable` | Switch |  | absent | always | `opt-filterable` |
| Table footer | Calculation | `defaults.footer` | Check boxes | 0 to 8 of: 18 calculations of 4.26 | absent | always | `opt-footer` |
| Cell options | Cell type | `defaults.cell` | Old control |  | absent | always | `defaults-cell-type` |
| Cell options | Cell value inspect | `defaults.inspect` | Switch |  | absent | always | `opt-inspect` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

Grafana shows "Cell value inspect" only for the cell types auto, JSON, coloured text and coloured background. Hune shows the switch at all times, because the button works with each cell type (9.20).

The row "Cell type" is the control of 10.3a with its select of the mode. It shows the types `auto`, `color_text`, `color_background`, `gauge` and `json`. An empty list of footer calculations removes the member `footer`. An empty column width removes `width`.

##### heatmap (4.33c, 9.19e)

Groups: Heatmap, Y axis, Colors, Cell display, Tooltip, Legend. The group "Standard options" gets "Display name". The groups "Thresholds" and "Colour" of the fields are not shown, because they have no effect (4.33c, rule 5).

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Heatmap | Calculate from data | `calculate` | Choice | `true` "Yes", `false` "No" | `false` | always | `heat-calculate` |
| Heatmap | X Bucket | `x_buckets.mode` | Choice | `size` "Size", `count` "Count" | `count` | `calculate` is `true` | `heat-x-mode` |
| Heatmap | X Bucket value | `x_buckets.value` | Text | `count`: an integer from 1 to 1,000. `size`: a duration such as `10m`. Or empty (`null`). | `null` | `calculate` is `true` | `heat-x-value` |
| Heatmap | Y Bucket | `y_buckets.mode` | Choice | `size` "Size", `count` "Count" | `count` | `calculate` is `true` and `y_buckets.scale` is `linear` | `heat-y-mode` |
| Heatmap | Y Bucket scale | `y_buckets.scale` | Choice | `linear` "Linear", `log2` "Log base 2", `log10` "Log base 10" | `linear` | `calculate` is `true` | `heat-y-scale` |
| Heatmap | Y Bucket value | `y_buckets.value` | Number | `count`: an integer from 1 to 1,000. `size`: a number above 0. A log scale: an integer from 1 to 16. Or empty (`null`). | `null` | `calculate` is `true` | `heat-y-value` |
| Y axis | Placement | `y_axis` | Choice | `left` "Left", `hidden` "Hidden" | `left` | always | `heat-y-axis` |
| Y axis | Reverse | `y_reverse` | Switch |  | `false` | always | `heat-y-reverse` |
| Y axis | Min value | `y_min` | Number | a number or empty | `null` | `calculate` is `true` | `heat-y-min` |
| Y axis | Max value | `y_max` | Number | a number or empty | `null` | `calculate` is `true` | `heat-y-max` |
| Colors | Mode | `color.mode` | Choice | `scheme` "Scheme", `opacity` "Opacity" | `scheme` | always | `heat-color-mode` |
| Colors | Scheme | `color.scheme` | Select | `oranges` "Oranges", `blues` "Blues", `greens` "Greens", `greys` "Greys", `purples` "Purples", `reds` "Reds", `yl_or_rd` "YlOrRd", `rd_yl_gn` "RdYlGn", `spectral` "Spectral", `viridis` "Viridis" | `oranges` | `color.mode` is `scheme` | `heat-scheme` |
| Colors | Color | `color.fill` | Old control | a colour (4.19) | `dark-orange` | `color.mode` is `opacity` | `heat-fill` |
| Colors | Scale | `color.scale` | Choice | `exponential` "Exponential", `linear` "Linear" | `exponential` | `color.mode` is `opacity` | `heat-scale` |
| Colors | Exponent | `color.exponent` | Number | 0.1 to 2 | `0.5` | `color.mode` is `opacity` and `color.scale` is `exponential` | `heat-exponent` |
| Colors | Steps | `color.steps` | Slider and number | 2 to 128 | `64` | always | `heat-steps` |
| Colors | Reverse | `color.reverse` | Switch |  | `false` | always | `heat-reverse` |
| Colors | Start color scale from value | `color.min` | Number | a number or empty | `null` | always | `heat-min` |
| Colors | End color scale at value | `color.max` | Number | a number or empty | `null` | always | `heat-max` |
| Cell display | Cell gap | `cell_gap` | Slider and number | 0 to 25 | `1` | always | `heat-gap` |
| Cell display | Hide cells with values <= | `hide_le` | Number | a number or empty | `0` | always | `heat-hide-le` |
| Cell display | Hide cells with values >= | `hide_ge` | Number | a number or empty | `null` | always | `heat-hide-ge` |
| Tooltip | Tooltip mode | `tooltip.mode` | Choice | `single` "Single", `multi` "All", `hidden` "Hidden" | `single` | always | `opt-tooltip-mode` |
| Legend | Show legend | `show_legend` | Switch |  | `true` | always | `heat-legend` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

Notes.

- A time size is a duration such as `10m`, and the document stores milliseconds (the text of F31 gives the grammar). With a log scale the row "Y Bucket" is hidden and the member is `size`.
- The row "Y Bucket value" is then the split factor, an integer from 1 to 16.
- The rows "Exponent", "Color" and "Scale" write members that are necessary also when the mode does not use them (4.33c). Hidden rows keep their values.
- The group "Colors" ends with a preview of the colour bar (F31). It is a picture and not a control, so it has no `data-option`. The hooks of the heatmap are those of F31.

##### histogram (4.33d, 9.19f)

Groups: Histogram, Tooltip, Legend. The group "Standard options" gets "Display name".

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Histogram | Bucket count | `bucket_count` | Number | 1 to 1000 | `30` | `bucket_size` is `null` | `hist-count` |
| Histogram | Bucket size | `bucket_size` | Number | or empty | `null` | always | `hist-size` |
| Histogram | Bucket offset | `bucket_offset` | Number |  | `0` | always | `hist-offset` |
| Histogram | Combine series | `combine` | Switch |  | `false` | always | `hist-combine` |
| Histogram | Stacking | `stacking` | Choice | `none` "Off", `normal` "Normal", `percent` "100%" | `none` | `combine` is `false` | `hist-stacking` |
| Histogram | Line width | `line_width` | Slider and number | 0 to 10 | `1` | always | `hist-line-width` |
| Histogram | Fill opacity | `fill_opacity` | Slider and number | 0 to 100 | `80` | always | `hist-fill-opacity` |
| Tooltip | Tooltip mode | `tooltip.mode` | Choice | `single` "Single", `multi` "All", `hidden` "Hidden" | `single` | always | `opt-tooltip-mode` |
| Tooltip | Values sort order | `tooltip.sort` | Choice | `none` "None", `asc` "Ascending", `desc` "Descending" | absent | `tooltip.mode` is `multi` | `opt-tooltip-sort` |
| Tooltip | Hide zeros | `tooltip.hide_zeros` | Switch |  | absent | `tooltip.mode` is `multi` | `opt-tooltip-hide-zeros` |
| Legend | Visibility | `legend.show` | Switch |  | `true` | always | `opt-legend-show` |
| Legend | Mode | `legend.mode` | Choice | `list` "List", `table` "Table" | `list` | `legend.show` is `true` | `opt-legend-mode` |
| Legend | Placement | `legend.placement` | Choice | `bottom` "Bottom", `right` "Right" | `bottom` | `legend.show` is `true` | `opt-legend-placement` |
| Legend | Values | `legend.values` | Check boxes | 0 to 5 of: 18 calculations of 4.26 | `[]` | `legend.mode` is `table` | `opt-legend-values` |
| Legend | Sort by | `legend.sort_by` | Select | 18 calculations of 4.26 and `name` | absent | `legend.mode` is `table` | `opt-legend-sort-by` |
| Legend | Sort descending | `legend.sort_desc` | Switch |  | absent | `legend.mode` is `table` and `legend.sort_by` is set | `opt-legend-sort-desc` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

The hooks of the histogram are those of F31. The row "Bucket count" is hidden while `bucket_size` is a number, because the size gives the buckets (4.33d). The row with the label `Stacking` is hidden while `combine` is `true`, because one series has nothing to stack.

##### gauge (4.33e, 9.19g)

Groups: Value options, Gauge, Text size. The group "Standard options" gets "Display name".

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Value options | Show | `reduce.values` | Choice | `false` "Calculate", `true` "All values" | `false` | always | `opt-reduce-values` |
| Value options | Calculation | `reduce.calc` | Select | 18 calculations of 4.26 | `last_not_null` | `reduce.values` is `false` | `opt-reduce-calc` |
| Value options | Fields | `reduce.fields` | Text | a number or empty | `null` | always | `opt-reduce-fields` |
| Gauge | Orientation | `orientation` | Choice | `auto` "Auto", `horizontal` "Horizontal", `vertical` "Vertical" | absent | always | `opt-orientation` |
| Gauge | Show threshold labels | `show_threshold_labels` | Switch |  | absent | always | `opt-threshold-labels` |
| Gauge | Show threshold markers | `show_threshold_markers` | Switch |  | absent | always | `opt-threshold-markers` |
| Gauge | Gauge size | `sizing` | Choice | `auto` "Auto", `manual` "Manual" | absent | `orientation` is `horizontal` or `vertical` | `opt-sizing` |
| Gauge | Min width | `min_width` | Slider and number | 0 to 600 | absent | `sizing` is `manual` and `orientation` is `vertical` | `opt-min-width` |
| Gauge | Min height | `min_height` | Slider and number | 0 to 600 | absent | `sizing` is `manual` and `orientation` is `horizontal` | `opt-min-height` |
| Gauge | Neutral | `neutral` | Number | a number or empty | absent | always | `opt-neutral` |
| Text size | Title | `title_size` | Number | 1 to 200, or empty | absent | always | `opt-title-size` |
| Text size | Value | `value_size` | Number | 1 to 200, or empty | absent | always | `opt-value-size` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

Each member of the group "Gauge" and of the group "Text size" is optional. A switch or a choice that comes back to the default removes its member (rule 4), and an empty number field removes it too. The labels are the labels of the classic gauge of Grafana 12.4.2. In Grafana, "Neutral" is a setting of a field in the group "Gauge". Hune has one neutral value for the panel (4.33e). The choice "Gauge size" with the sizes 75 gives the same cells for "Manual" and "Auto", so the audit lists it (rule 14, F4).

##### logs (4.33f, 9.19h)

Groups: Logs. The labels are the labels of Grafana 12.4.2. Each member is optional. A switch or a choice at its default value removes the member (rule 4), so a new panel has the options `{}`.

The field settings have no effect on the lines (4.33f, rule 4). Thus the groups "Standard options", "Thresholds", "Colour", "Overrides", "Value mappings" and "Data links" are not shown.

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Logs | Show timestamps | `show_time` | Switch |  | absent | always | `opt-logs-time` |
| Logs | Wrap lines | `wrap` | Switch |  | absent | always | `opt-logs-wrap` |
| Logs | Enable log details | `details` | Switch |  | absent | always | `opt-logs-details` |
| Logs | Order | `order` | Choice | `newest_first` "Newest first", `oldest_first` "Oldest first" | absent | always | `opt-logs-order` |

The absent value of the switches "Show timestamps" and "Wrap lines" is off, and the absent value of "Enable log details" is on. The absent value of "Order" is "Newest first".

##### geomap (4.33g, 9.19i)

Groups: Map view, Markers, Map controls, Tooltip. The group "Standard options" gets "Display name". The form has no row for a tile source: the tile source comes only from the block `geomap` of the configuration (13.12c).

| Group | Label | Member | Control | Values or range | New | Shown when | Hook |
|---|---|---|---|---|---|---|---|
| Map view | View | `view.mode` | Choice | `fixed` "Coordinates", `fit` "Fit to data" | `fixed` | always | `geo-view-mode` |
| Map view | Latitude | `view.lat` | Number | -90 to 90 | `0` | always | `geo-view-lat` |
| Map view | Longitude | `view.lon` | Number | -180 to 180 | `0` | always | `geo-view-lon` |
| Map view | Zoom | `view.zoom` | Slider and number | 0 to 22 | `1` | always | `geo-view-zoom` |
| Markers | Latitude field | `layer.lat_field` | Select of fields | `null` "Field named lat or latitude", or a field name | `null` | always | `geo-lat-field` |
| Markers | Longitude field | `layer.lon_field` | Select of fields | `null` "Field named lon, lng or longitude", or a field name | `null` | always | `geo-lon-field` |
| Markers | Size field | `layer.size_field` | Select of fields | `null` "Fixed size", or a field name | `null` | always | `geo-size-field` |
| Markers | Size | `layer.size` | Slider and number | 1 to 100 | `5` | always | `geo-size` |
| Markers | Min size | `layer.size_min` | Number | 1 to 100, at most the max size | `2` | always | `geo-size-min` |
| Markers | Max size | `layer.size_max` | Number | 1 to 100, at least the min size | `15` | always | `geo-size-max` |
| Markers | Color field | `layer.color_field` | Select of fields | `null` "Fixed color", or a field name | `null` | always | `geo-color-field` |
| Markers | Color | `layer.color` | Old control | a colour (4.19) | `dark-green` | always | `geo-color` |
| Markers | Fill opacity | `layer.opacity` | Slider and number | 0 to 100 | `40` | always | `geo-opacity` |
| Map controls | Show zoom control | `show_zoom` | Switch |  | `true` | always | `geo-show-zoom` |
| Map controls | Mouse wheel zoom | `mouse_wheel_zoom` | Switch |  | `true` | always | `geo-wheel-zoom` |
| Tooltip | Tooltip | `tooltip` | Choice | `details` "Details", `none` "None" | `details` | always | `geo-tooltip` |
| Standard options | Display name | `defaults.display_name` | Text | 1 to 256, or empty | absent | always | `opt-display-name` |

The labels are the labels of Grafana 12.4.2 where Grafana has the option. The row "View" has the hint "With a tile source, the map always opens on the coordinates." (9.19i, section 3). The rows of the size and of the colour always show, with a hint that tells when each one applies. A "Min size" above the max size gives an error and writes nothing (4.33g, rule 2). A "Max size" below the min size does the same.

##### state_timeline and status_history (4.33b)

The form of F30 stays, with its hooks. These are its groups and rows. Its two changes are the slider rule of rule 7 and the hover probe of rule 14.

| Type | Group | Labels | Hooks |
|---|---|---|---|
| `state_timeline` | State timeline | Merge equal values, Show values, Align values, Row height, Line width, Fill opacity, Connect over missing values, Show row names | `timeline-merge`, `timeline-show-values`, `timeline-align`, `timeline-row-height`, `timeline-line-width`, `timeline-fill-opacity`, `timeline-span-nulls`, `timeline-labels` |
| `status_history` | Status history | Show values, Row height, Column width, Line width, Fill opacity, Show row names | `timeline-show-values`, `timeline-row-height`, `timeline-col-width`, `timeline-line-width`, `timeline-fill-opacity`, `timeline-labels` |
| both | Legend, Tooltip | Show legend, Placement, Tooltip mode | `timeline-legend-show`, `timeline-legend-placement`, `timeline-tooltip-mode` |

##### The members with no control (NO_CONTROL)

| Type | Member | Cause |
|---|---|---|
| `text` | `options.mode` | The member has one value, `markdown` (4.33a). |
| `text` | `options.content` | The group "Text" has the field "Content" (10.3). |
| `piechart` | `options.tooltip.sort` | A pie chart shows one slice at a time, so a sort has no row to order. |
| `piechart` | `options.tooltip.hide_zeros` | A slice of 0 has no area under the pointer. |
| `table` | `fields.defaults.hidden` | A default that hides each column. The override form sets it for one column. |
| `timeseries`, `barchart`, `xychart`, `histogram`, `state_timeline`, `status_history` | `fields.defaults.hide_from` | A default that hides each series. The override form sets it for one series, as the legend click of Grafana does (9.19). |

### 10.4 Dashboard JSON tab

`editview=json` opens the JSON of the whole draft document.

- The text starts as the canonical text of the draft (6.15). When it has none, it is the JSON with an indent of two.
- Each JSON text area has `spellcheck="false"`, as the SQL text area has (10.3). The browser draws no wavy line in it.
- The JSON view fills the page. Its box has the height of the window, less the header and less the padding of the page above and below. The area takes the room that the bar and the list of errors leave, with 160 px at least. The page has no scroll of its own for the area, at any width of the window. The tab "JSON" of the settings view (10.3b) has the same rule (case B240).
- The area looks like the editor of VS Code: the palette Light+ in the light theme and Dark+ in the dark theme. The colours are the tokens `--json-*` of `look.css` (9.25). The SQL area keeps its own.
- A key, a text, a number, `true`, `false` and `null`, and the punctuation each have a colour: `--json-key`, `--json-str`, `--json-num`, `--json-const` (for `true`, `false` and `null`) and `--json-punct`. Each text colour has 4.5:1 or more on the area and on the band of the caret line, in both themes (9.26). A bracket colour has it on the box of a bracket too. Three Light+ colours are darker here: the number, the second colour of the brackets and the colour of a bracket with no pair. A text that is not valid has colours too.
- A bracket `{`, `}`, `[` or `]` has one of three colours by its depth: `--json-bracket-1`, `--json-bracket-2` and `--json-bracket-3`, and the three repeat. A bracket with no pair has `--json-bracket-bad`. A bracket inside a text does not count. The page finds the pairs in one pass over the text after each change.
- The text is 14 px, with a row of 20 px (a line height of 1.43) and a tab of two columns. The font is the font of the code of the page. The script has the same row height as the css (`ROW` and `--json-row`).
- The text area is under the layer, and its text is transparent. The layer draws the colours with the same text metrics as the SQL area (10.3). The text area keeps the text, the selection, the caret and the scroll bars. The layer draws only the visible rows and 100 rows above and below. A scroll that comes within 30 rows of the edge of the drawn rows draws them again.
- A JSON text never holds a line feed inside a text, so each row has its own colours. A row has 2,000 coloured parts at most. The rest of the row keeps the plain colour. A key does not read the scroll position or the size of the area from the page. A text of 6,600 lines then does not slow the key.
- The gutter has the line numbers and the marks of the changes. A mark shows a line that differs from the saved dashboard. The saved text is the JSON of the saved document with an indent of two. A draft with no change has the same text, so it has no mark.
  - A green bar at the left edge of the gutter is a line that is added. A blue bar is a line that is modified. A small red arrow on the border between two lines shows that one or more lines of the saved text are gone there.
  - A red band under a line number marks the first syntax fault of the check in the page. The list of errors gives the same line.
  - The marks follow the text 120 ms after each key. A revert or an apply of the draft draws them again. After a revert the area shows the saved text, so no mark stays.
  - The page compares the two texts line by line. It cuts the common start and the common end first. A change can leave more than 1,000 lines at the start and at the end both. The page then compares the lines by their place, and a line that moved shows as modified. The gutter draws 500 marks at most.
  - The gutter and its marks are hidden from assistive technology (`aria-hidden="true"`). The marks are an aid for sight. The bar of the edit mode says "Unsaved changes" (10.2), and the list of errors gives each syntax fault as text.
- The gutter has no border. A line number has the colour `--json-lineno`. The number of the line of the caret has the colour `--json-lineno-active`.
- The line of the caret has a band under the text, with the colour `--json-line-active`. The band is absent when the selection is not empty.
- The white space at the start of a line has a guide every two columns, in `--json-guide`. The guide of the pair of brackets that holds the caret is darker (`--json-guide-active`) and runs along the lines between the two brackets.
- A caret that touches a bracket puts a box on it and on its pair. The box has an edge of 1 px, `--json-match-edge`, and the fill `--json-match-bg`. A bracket with no pair has no box.
- The page takes the pairs from the last scan of the text, so a move of the caret parses nothing. A key, a click or a move of the selection asks for the update, and one frame answers. The update reads no layout value.
- The selection has the colour `--json-selection`, and `--json-selection-idle` when the area has no focus. The coloured text shows over the selection.
- The first syntax error has a wavy line under its token, in the colour `--json-squiggle`. The token is the part of the text at the column of the error. The red band of the gutter marks the same line.
- The scroll bars are 10 px wide, with a grey thumb (`--json-thumb`, and `--json-thumb-hover` on hover).
- The area does not fold lines and has no minimap.
- The cases B270 and B271 read these rules in the page.
- The JSON area of the panel (10.3) has the same look and the same layers. It has no marks of change.
- The page checks the text 1 s after the last change, and when the user selects "Check":
  1. In the page: the JSON syntax and duplicate members, with their line and column.
  2. On the server: `PUT api/dashboards/<uid>?validate_only=true`, or `POST api/dashboards?validate_only=true` for a
     dashboard that is not saved yet.
- The body of this request is the text `{"document":` and the text of the tab and `}`. Thus the server reads the exact
  text, with its duplicate members (6.2).
- The page lists every error with its JSON pointer, its code and its message (4.37), up to `MAX_ERRORS` (4.35). A
  click on an error puts the cursor on the member when the page finds it.
- "Apply" is active when the last check is valid. It replaces the draft with the canonical text of the check. "Format"
  replaces the text of the tab with this canonical text.
- Text that is not applied stays in the draft as `json_text` (10.8). To leave the tab with such text, the user confirms
  in a dialog.

### 10.5 Layout edits

- The fields of "Position" (10.3) change the grid numbers of a panel. Then the page normalizes the layout of the
  section.
- "Move up" and "Move down" use the order of the panels of the section: by `y`, then `x`, then `id`. The panel
  changes its place with the previous or the next panel: the two panels exchange their `x` and `y`. Then the page
  normalizes the layout. The control is not active for the first or the last panel of the section.
- To move a panel to another section, the user changes the JSON of the dashboard (10.4).

Normalization (`FE-16`) is the vertical compaction of the grid of Grafana. The page takes the panels of one section by
`y`, then `x`, then `id`. For each panel it does these steps:

1. The page starts with the current `y` of the panel.
2. While `y > 0` and the panel at `y - 1` overlaps no panel that is already placed, `y` becomes `y - 1`.
3. While the panel overlaps a placed panel `P`, `y` becomes `P.y + P.h`.
4. The panel is now placed. `x`, `w` and `h` do not change.

The page normalizes after an add, a duplicate, a delete, a move and a change of "Position". It never normalizes the
text of a JSON tab.

### 10.5a Drag and resize on the grid

- In edit mode, the user drags the title bar of a panel to move it. The user drags the bottom-right corner of a panel to
  change its size.
- A narrow screen (768 px or less) has no drag, because the grid is a flow (9.8). The form "Position" and the items
  "Move up" and "Move down" stay in all cases (9.26).
- When the pointer moves less than 4 px, the action is a click, not a drag. A button, a link or the info icon in the
  title bar never starts a drag.
- The dragged panel follows the pointer. A placeholder shows the cell of the drop. Its colour is the primary colour at
  15 %.
- The cell of a drag is `x = round((left - 8) / P)` and `y = round((top - 8) / 38)`. `left` and `top` are the pixels
  from the corner of the grid. `P` is the width of one column plus 8 px.
- The size of a resize is `w = round((width + 8) / P)` and `h = round((height + 8) / 38)`.
- The page keeps the cell in the section: `0 <= x <= 24 - w`, `y >= 0`, `1 <= w <= 24 - x` and `1 <= h <= 200`.
- Each new cell changes the layout of the section. The panels that the moved panel meets go above it when there is
  room (the two swap). Else they go below it. Then the compaction of 10.5 pulls each panel up.
- The layout of a step is the input of the next step, as in Grafana. The same final cell can give two layouts after
  two different paths.
- Escape or a cancelled pointer ends the drag. The draft does not change.
- At the drop, the page changes the `grid` members of the panels in the draft, as the form "Position" does (10.5). The
  draft, the save and the conflict rules do not change.
- Keys: when the focus is in a panel, Alt with an arrow key moves the panel by one cell. Alt with Shift and an arrow key
  changes the size by one unit. A status line gives the new cell. When the move changes nothing, the status line says
  "The panel cannot go there.".
- A drag does not move a panel to another section. The user changes the JSON for that (10.5).
- The draft grid draws the copies of each repeat (10.6). Only the first copy of a panel has the title bar handle, the resize corner and the menu button. A copy n from 2 is a view: it has none of them, and the keys of this section do not act on it.
- A drag of the first copy of a panel with direction `h` changes `y` only. The page keeps `x` = 0 and `w` = 24. A resize of it changes `h` only. A drag or a resize of a panel with direction `v` changes `x`, `y`, `w` and `h` as for a panel, and the copies follow. The moves of 9.8c apply to the draft grid, so the panels below the copies are drawn at their displayed place. A drop writes the stored `grid` of the panels, never the displayed place.

### 10.6 Add, duplicate and delete a panel; sections

- "Add panel" adds a panel to the last section, and "Add panel here" adds it to that section. The new panel has:
  - `id`: the highest panel id of the document plus 1;
  - `type`: `table`, with the "New" options of 9.19, `title`: "New panel" and `description`: the empty text;
  - `grid`: `x = 0`, `y` = the bottom of the section, `w = 12`, `h = 8`;
  - `data.query`: the first source of `GET api/sources` in id order, `format = "table"`, `sql = ["SELECT 1"]`,
    `min_interval = null` and `max_data_points = null` (4.15);
  - `fields`: `defaults` with the default thresholds of 9.14 only, and `overrides = []`;
  - `transforms = []` and `links = []`.
- When `GET api/sources` is empty, the page shows "No source is available to you." and adds nothing. After an add, the
  panel editor opens.
- "Duplicate" copies the panel with the same title and a new id: the highest id plus 1. The copy has `y` = the
  original `y` plus its `h`. A reuse panel of the original keeps its source.
- "Delete" opens the dialog "Delete the panel <title>?". When other panels reuse this panel, the page refuses and shows
  "The panels <titles> reuse this panel. Change them first.".
- The dialog of "Delete" also lists the annotations that name the panel in `panels` (10.3b). The delete removes the id from each one in the same draft change. "Duplicate" gives the copy a new id that no `panels` list names.
- "Add section" adds a section at the end, with the title "New section", `collapsed = false` and no panel.
- "Rename" puts a text field in the section header. `Enter` applies and `Escape` cancels. An empty title shows an error.
- The menu "Section actions" of a section header has the items "Move up", "Move down" and "Delete section". The first two items change the place of the section. The item "Delete section" opens the dialog "Delete the section <title>?". The section and its panels then leave the draft.
- The page refuses "Delete section" for the last section, with the notice "A dashboard needs at least one section.". It also refuses it when a panel of another section reuses a panel of this section. The notice is "The panels <titles> reuse a panel of this section. Change them first.".

Text panel (4.33a):

- "Add text panel" adds a panel to the last section. The item "Add text panel here" (`section-<index>-add-text`) of the menu "Section actions" adds it to that section. The new panel has these members:
  - `id` is the highest panel id of the document plus 1. `type` is `text`.
  - `title` is "New text". `description` is the empty text. `transparent` is `false`.
  - `grid` has `x` = 0, `y` = the bottom of the section, `w` = 12 and `h` = 8. `links` is `[]`.
  - `options` has `mode` = `markdown`, and `content` = `["# Title", "", "Write text in Markdown."]`.
  - The panel has no `data`, no `transforms` and no `fields`.
- The message "No source is available to you." of "Add panel" does not stop "Add text panel". After an add, the panel editor opens.
- "Duplicate" copies the text with the same title and a new id, as for each panel. "Delete" opens the dialog of the panel. No panel reuses a text panel, so the dialog never lists one.

Placed controls (4.7a):

- A person places a variable from its form (10.3b) or from the menu of "Add" (10.2). A person puts it back with "Put back in the top bar" in the menu of its frame. All of these change the draft only, as all edits do (10.2).
- The draft grid draws the frame of each placed control. A drag, a resize and the keys of 10.5a change `x`, `y`, `w` and `h` of the `placement`, and nothing else. The rules of the grid (4.14) and the compaction of 10.5 apply to the placed controls and the panels together.
- A move of a section ("Move up" or "Move down") changes the `section` of its placements. It does the same for the other section. "Delete section" removes the placements in the section. These variables are then back in the top bar. The dialog of the section says it when the section has a placement.
- The entry "Edit the variable" of the menu of a frame opens the tab "Variables" of the settings with the form of that variable.

Repeat (4.13a):

- The menu "Section actions" has the item "Repeat by a variable" (`section-<index>-repeat`). It opens a dialog (`section-<index>-repeat-dialog`) with a select of the variables of the draft that have the kind `query` or `custom` and `multi` = `true` (`section-<index>-repeat-variable`). The item "None" removes the member `repeat`. The buttons are "Apply" and "Cancel".
- The item is not active in three cases, and its tooltip says why. The section has `title` = `null`, or a variable has a placement in the section, or the section holds a panel with `repeat` or with `data.reuse`.
- A section with a repeat shows a badge in its header (`section-<index>-repeat-badge`) with the text "Repeats by <label>". The draft grid draws the copies. Only the first copy has the menu "Section actions". The copy n from 2 is a view.
- A "Duplicate" of a panel that has a `repeat` keeps the `repeat`. The copy is below the original, so the page draws it below the region of the original (9.8c).
- A "Delete" of a panel that has a `repeat` removes the stored panel, and so all its copies. A panel that has a `repeat` has no reuse panel (4.13a, rule 8), so the dialog never lists one for it.
- "Move up" and "Move down" of a section keep its `repeat`. A section with a repeat has no placement, so no `section` index changes.
- A control of a variable goes to the last section that has no repeat (4.7a, rule 11). The item "Place a variable in the grid" is not active when every section repeats.
- The placed control of an open variable is the same control as in the bar (9.10). Its popup opens over the grid. A frame of 2 rows or more shows the field, and the combobox has the keys of 9.10. A move or a resize of the frame changes no value.
- The list "Reuse a panel" of the panel editor offers no panel that repeats. It offers no panel of a section that repeats (4.13a, rule 8).

### 10.7 New dashboard, folders and move

These controls are on the list page (9.7). They exist only when `editing` is `true`, and when the `access` flags of
6.11 and 6.12 permit them (11.25).

- "New dashboard" opens a dialog with "Title" (necessary) and "Folder" (a select). The select has the entry "Root (no folder)" first, when the member `root` of `GET api/folders` has `access.write` (6.11). Then it has the folders with `access.write`. The first entry is the chosen one. The dialog opens when only the root accepts a dashboard.
  - The page makes a uid: 14 characters of `[a-z0-9]`, from `crypto.getRandomValues`, with no bias.
  - It makes the smallest valid document of 4.5 with this uid and this title: `description` empty, `tags = []`,
    `time = {from: "now-6h", to: "now", timezone: "browser", refresh: null}`, `cursor_sync = "off"` and
    `variables = []`. It has one section `{title: null, collapsed: false, panels: []}`.
  - It sends `POST api/dashboards` with a new `save_id` (10.9). The member `folder` is the id of the folder, or `null` for the root. On `409 already_exists` it makes a new uid and tries
    one more time.
  - It opens `d/<uid>?edit=1`.
- "New folder" needs `access.create`. It is in the header of the list page (9.7). It opens a dialog with one field, "Title". It sends `POST api/folders`.
  - The page makes the id from the title (4.3). The id has lower-case letters and digits, one dash for each run of other characters, and 40 characters at most.
  - A title with no letter and no digit gives `folder-` and four random characters.
  - When the id exists (`already_exists`), the page adds a dash and four random characters. It sends again, three times at most.
- "Rename" and "Delete" of a folder are in the menu on the folder title (9.7).
- "Rename" sends `PUT api/folders/<id>`. "Delete" asks for a confirmation and sends
  `DELETE api/folders/<id>`. `409 folder_not_empty` gives "The folder holds dashboards. Move or delete them first.".
- "Move" of a dashboard is in the menu of its card (9.7). It opens a select of the folders with `access.write`. The entry "Root (no folder)" is the first entry when the member `root` has `access.write` and the dashboard is not at the root. The chosen entry is the folder of the dashboard. It sends `POST api/dashboards/<uid>/move` with the folder (`null` for the root),
  `If-Match` (the etag of the list item) and a new `save_id`. When the dashboard is there already, the notice says "The dashboard is already at the root." or "The dashboard is already in this folder.".
- A dialog shows the message of each error of 6.8 under its fields.
- The buttons of a dialog come in this order: the safe choice first ("Cancel" or "Stay"), the action last.
- A modal dialog is 500 px wide, and the import dialog is 520 px wide (10.12). Its top edge is at 10 % of the window height. The radius is 10 px, and the title is 18 px. On a phone the dialog has the full width. The delete and exit dialogs keep their texts, and one click confirms.

### 10.8 Draft in localStorage

The key is `hune.draft.v1.<uid>`. The value is:

```json
{ "base_etag": "1b4f0e9870cf6b5e3a5b8c2d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f", "base_version": 12,
  "subject": "user:alice", "folder": "transit", "updated_at_ms": 1794232800000,
  "document": { "format_version": 3, "uid": "nodes" },
  "json_text": null, "save_id": null, "message": "" }
```

- The page writes the draft 500 ms after the last change, and before the page unloads.
- The page removes the draft after these events: a save that succeeds, "Revert" and "Discard", the choice "Load theirs" of 10.10,
  and a delete.
- A dashboard with a draft shows a notice with "Resume" and "Discard". Its text is: "You have changes that are not
  saved, from <time>." "Resume" opens edit mode with the draft.
  The page shows this notice in the states `view` and `panel-view`. It shows it only to a user who can edit (10.2a, rule 8). "Resume" opens the state `edit` and closes the panel view.
  The page shows the same notice when the browser "Back" or "Forward" ends edit mode while the draft has changes (10.2a, rule 9).
- When `base_etag` differs from the current etag, the notice adds: "The dashboard changed since then." The next save
  then gives the conflict screen (10.10).
- The page removes, with a notice, a draft that is not valid JSON, and a draft whose `document.uid` differs from its key. It also removes a draft whose `document.format_version` is not the current one (4.42). The server refuses such a document. After the upgrade to format 3, the page removes a draft of format 2 with the notice "A draft of an older format was removed. Save or discard your drafts before an upgrade." So the people save or discard their drafts before the migration (4.42, item 5).
- The member `subject` is the `subject` of the session (6.10) that wrote the draft. When it differs from the subject of
  the current session, the page removes the draft and does not show it. Thus a second person on a shared browser does
  not see the draft of the first person in the page. This removal loses the draft of the first person. Until a page of
  another subject removes it, the draft stays readable with the tools of the browser (12.18).
- `base_version` is 0 for a uid with no version (6.12).
- When `localStorage` fails (full, or a private window), the page shows "Hune cannot keep your draft in this
  browser. Save soon." The edit continues in memory.
- While the draft differs from the base, the event `beforeunload` asks the question of the browser. A link of the page
  opens the dialog of 10.2 for "Exit". The browser "Back" and "Forward" do not open it: they keep the draft and show the notice above.
- In edit mode, `409 dashboard_changed` does not read the document again. The page shows "The dashboard changed. Save
  to compare your changes with the new version.".
- Two tabs on one dashboard share one draft: the last write wins. The `storage` event tells the other tab, which shows
  "This dashboard is open for edit in another tab.".

### 10.9 Save

1. The user selects "Save" (`data-testid="save-button"`) or presses `Ctrl+S`. A drawer opens (the look is below).
   The tab "Details" shows the field "Message" (optional, 500 characters at most) and the button "Save".
2. When the draft has no `save_id`, the page makes one and keeps it in the draft. The `save_id` is a UUID version 4
   from `crypto.getRandomValues`. The high four bits of byte 6 are `0100`, and the high two bits of byte 8 are `10`.
   The text is lower case, in the canonical form.
3. The page sends `PUT api/dashboards/<uid>` with `If-Match: "<base_etag>"`, `Idempotency-Key: <save_id>` and the body
   `{"message": "<message>", "document": <draft document>}`. A dashboard that is not saved yet uses
   `POST api/dashboards` (10.7).
4. The page acts on the answer with the table below.
5. A change of the draft after a save that failed removes `save_id`. Thus the next save is a new attempt.
6. The tab "Details" has two boxes under the field "Message". Both are off at first. A box shows only when its value differs from the default of the draft.
   - "Save the current time range as the default" shows when the current range differs from `time.from` or `time.to`. A change of one end counts as a change of the range.
   - "Save the current refresh as the default" shows when the current refresh differs from `time.refresh`.
   - Each box has a hint with the current value and the default, for example "Now: 30s. Default: Off."
   - The values are those of the header controls. The range gives `time.from` and `time.to`, and the refresh gives `time.refresh` (`null` for off).
   - Before the first send, the page puts each chosen value in the draft. It checks the range with the rules of the tab "Time" (10.3b). A range that fails is not saved, and the dialog shows the reason.
   - A retry sends the same document with the same `save_id`. The page puts the values in the draft once.
   - After the save, the dashboard has the new defaults. The address then names only the values that still differ (9.5).

| Answer | The page |
|---|---|
| `200` | It removes the draft, takes the new etag and version as the base and ends edit mode (10.2a, rule 7). A panel editor that a panel view opened gives the panel view again, with the saved panel. It shows "Saved as version <n>.", or "Nothing changed." for `noop: true`, and each warning. |
| `409 conflict` | It opens the conflict screen (10.10). |
| `422 invalid_document` | It lists the errors with their pointers. A click opens the JSON tab at the member. The draft stays. |
| `422 idempotency_key_reused` | It makes a new `save_id` and shows "Select Save again.". |
| `503 storage_unavailable`, a network error, or no answer after 30 s | It shows "Saving is not possible now. Your draft is kept in this browser." with "Retry". "Retry" sends the same request with the same `save_id`. |
| `403 forbidden` | It shows the message. The draft stays. |
| `401 unauthenticated` | The full page of 9.6. |
| `413 payload_too_large` | It shows "The dashboard is too large.". |
| `507 storage_full` | It shows "The store is full. Tell the operator.". |
| Another error | It shows the code and the message. The draft stays. |

The drawer of the save dialog (`data-testid="save-dialog"`) has this look:

- It is at the right edge of the window, with the full height. Its width is 50 vw. Up to 768 px it is the window less 16 px.
- It has a title, a muted subtitle with the title of the dashboard, and a close button.
- It has two tabs, "Details" and "Changes". The arrow keys move between the tabs.
- "Details" has the field "Message" with its count of characters (500 at most), the buttons "Cancel" and "Save", and the table of errors. The buttons hide on "Changes".
- "Details" also has the two boxes of rule 6 when a value differs.
- The tab "Changes" shows the diff with the boxes that are on. The hooks are `save-defaults`, `save-default-range` and `save-default-refresh`.
- "Changes" shows the line diff from the stored version to the draft (10.15). The tab shows the number of changed lines. When the diff fails, the tab shows "The changes are not available now.".
- The test ids and the answers of the table above do not change.

The same `save_id` on "Retry" makes the save safe after a lost answer (6.6). For example, during a PostgreSQL
failover, the first request can commit and lose its answer. The retry then gets the stored answer, and the store holds
one version.

### 10.10 Conflict screen

`409 conflict` on a save opens the conflict screen (`data-testid="conflict-screen"`). It shows:

- The version of the current document, `saved_by` and `saved_at` (in the zone of the dashboard), from the error
  members (6.7). Above them is the text "The dashboard changed after you opened it."
- A line diff (10.15) from theirs to mine. "Theirs" is the canonical text of
  `GET api/dashboards/<uid>?download=true`. "Mine" is the canonical text of the draft from `validate_only=true`.
- When the draft is not valid, "mine" is its JSON with an indent of two spaces. The screen then adds the text "Your
  draft is not a valid document."

Three choices:

1. "Load theirs and keep my draft". The page downloads the draft as `<uid>-draft-<YYYYMMDD-HHmmss>.json`, and shows it
   in a read-only side panel "My draft" with "Copy". Then it removes the draft and loads theirs as the new base. Edit
   mode continues on theirs, and the user copies the changes by hand. The side panel stays until the page closes.
2. "Overwrite with mine". A dialog says "Their version stays in the history." After the confirmation, the page sends
   `PUT` with `If-Match` = `current_etag`, a new `save_id` and the draft. The answer follows 10.9. A new `409` opens the
   conflict screen again, with the newer version.
3. "Cancel". The screen closes. Edit mode continues with the draft and the old base.

The focus goes to the first choice when the screen opens. `Escape` is "Cancel".

Two other cases of the same `409`:

- A move. When `current_etag` equals the base etag and `current_folder` differs from the folder of the page, the
  conflict comes from a move (5.5, rule 2). The screen shows no diff, and offers "Save in the new folder" and "Cancel".
  - After a move to a folder, the text is: "The dashboard moved to the folder <title> after you opened it."
  - After a move to the root, the text is: "The dashboard moved to the root after you opened it."
- "Save in the new folder" ("Save at the root" after a move to the root) sends the same `PUT` with a new `save_id`. The server updated its catalog after the
  conflict (6.15), so the save can pass.
- A delete. When `GET api/dashboards/<uid>?download=true` gives `404` for "theirs", the dashboard was deleted after the
  conflict. The screen shows "The dashboard was deleted after you opened it." It offers "Download my draft" and
  "Cancel". An operator can bring the dashboard back with `hune restore` (10.13).

### 10.11 History: list, view, diff and restore

"History" in the menu "More actions" (9.9) opens `editview=history`. The page exists only when `access.history` is `true` (6.17, 11).
A URL that a user opens by hand, without this right, shows the page of `403` of 9.8 ("You have no access to this dashboard."). The page makes no request to the history routes. The routes refuse too: `history_disabled`, `forbidden` or `history_unavailable`.

List (`data-testid="history-list"`):

- The columns are "Version", "Kind", "Saved at" (in the zone of the dashboard), "Saved by", "Message" and "Size". The
  newest version is first, with the mark "current". Under 900 px the columns "Kind", "Saved by" and "Size" hide.
- "Load more" reads the next page while `next` is not `null` (6.24).
- Each row has "View" and "Compare with current". With `access.export` equal to `true`, the current text is `download=true` of 6.14. Without it, the text of the newest version of the list stands for the current text.
- A row that is not current, not a deletion version and not an `external` version without bytes also has "Restore". This needs `editing = true` and
  `access.write` on the dashboard.
- The user can select two rows with their check boxes. "Compare" then shows the diff from the older to the newer one.

View (`editview=history&version=<n>`):

- The page shows version `n` as a read-only dashboard, with the banner "Version <n> of <time>, by <subject>. This view
  is read-only.".
- The banner has "Restore this version" (when permitted), "Compare with current", "Export this version" (only when `access.export` is `true`) and "Back to
  the history".
- Each panel shows the text "Data not shown".
- The banner has one button "Show data" for the whole page, when the user can use the preview. This is true when `editing` is `true`
  and each source of the version is in `GET api/sources`.
- "Show data" runs each panel of the version that has a query with the preview route (6.22), three at a time. It uses the variables of the version.
  The options of its `query` variables come from the preview with `variable`.
- A version that the current rules cannot read (`422 stored_document_invalid`) shows its errors. It also shows its
  stored text from `download=true` (6.17, 4.42). "Restore" is not active for it.
- An `external` version without bytes (6.17) has no document to show. The view shows the text "The file of this
  version was not a valid document. Hune keeps only its etag: <etag>." "Restore", "Compare" and "Export" are not
  active for it. The history list shows this version with the size "-".

Diff (`editview=history&base=<a>&version=<b>`, `data-testid="diff-view"`):

- `a` and `b` are version numbers, or `current`. "Compare with current" opens `base=<n>&version=current`.
- The page reads both versions with `download=true` (6.14, 6.17). It shows the line diff of 10.15 with the title
  "Changes from version <a> to version <b>".

Restore:

- "Restore" opens the dialog "Restore version <n> as a new version?" with the field "Message".
- The page sends `POST api/dashboards/<uid>/restore` with `If-Match` = the current etag and a new `save_id`.
- `200` gives "Version <n> is restored as version <m>." and the page shows the current dashboard again.
- `409 conflict` gives "The dashboard changed. Load the history again.".

### 10.12 Import and export of one document

Export:

- "Export" of the dashboard menu and of the menu of a card reads `download=true` (6.14). Both need `access.export` equal to `true`.
- "Export this version" of a version view reads `download=true` (6.17). It needs `access.history` and `access.export` equal to `true`.
- The browser saves the canonical bytes as `<uid>.json`, or `<uid>-v<n>.json` for a version.

Import (the list page, when `editing` is `true` and one folder has `access.write`):

1. The user selects a `.json` file. A file larger than `limits.document_bytes` (6.10) gives an error. The page reads
   the file as UTF-8.
2. A Grafana file stops the import, with the text "This is a Grafana file. Use the command `hune import-grafana`."
   A Grafana file is JSON with one of these:
   - an `apiVersion` that starts with `dashboard.grafana.app/`;
   - the members `schemaVersion` and `panels`;
   - a member `dashboard` that holds `panels`.
3. A dialog of 520 px shows the uid of the document and a select. The select has the entry "Root (no folder)" first, when the member `root` has `access.write` (6.11), then the folders with `access.write`. The first entry is the chosen one. The page sends `folder: null` for the root, in the check and in the creation.

   With `access.create`, the select has the last entry "New folder...". The entry shows a field for the title of the folder. The page makes the folder with the rule of "New folder" (10.7) when the user chooses "Import". Then it makes the dashboard in it. A second try after a refusal of the dashboard uses the same folder.

   The dialog has a drop zone for the file. The zone has the title "Upload dashboard JSON file" and the hint "Drag and drop here or click to browse". It also has a button "Choose file" and the name of the chosen file. A file that the user drops on the zone counts as a chosen file.
4. The page sends `POST api/dashboards?validate_only=true` with the exact text, as 10.4 does. Each error shows with its
   pointer, and the import stops until the user gives a valid file.
5. The page reads `GET api/dashboards/<uid>`. When it gives `200`, the uid exists. The dialog then offers two choices.
   "Replace the current dashboard" shows the line diff from the current document to the file. After a confirmation, it
   sends `PUT` with `If-Match` = the current etag and a new `save_id`. The folder stays the same.
   - `422 stored_document_invalid` also means that the uid exists. The page then takes the `etag` of its list item
     (6.12), and shows the stored bytes of `download=true` in place of the current document. It offers the same two
     choices.
6. "Import as a new dashboard" makes a new uid (10.7) and puts it in the member `uid`. The page checks the document
   again with `validate_only=true`, then sends `POST api/dashboards`.
7. When the uid does not exist, the page sends `POST api/dashboards` with the folder of the dialog. A `403` of step 5
   (mode `authzen`) also leads to this step. There, `409 already_exists` gives the text "A dashboard with this uid
   exists, but you cannot read it."
8. After a success, the page opens the dashboard.

The store keeps the canonical bytes of the imported document (4.38), not the bytes of the file.

### 10.13 Delete and restore of a dashboard

- "Delete" exists in the menu "More actions" of the dashboard page (9.9) and in the menu of a card (9.7). It needs `editing` `true` and `access.write`. It opens the dialog "Delete the dashboard <title>? It leaves the list. An operator can bring it back."
  with "Delete" and "Cancel".
- The page sends `DELETE api/dashboards/<uid>` with `If-Match` = the etag that it holds and a new `save_id` (6.16).
  The store keeps a deletion version and the history (5.4).
- `200` opens the list page with the notice "The dashboard <title> is deleted.". The page removes the draft of the uid.
- After a lost answer, the page sends the delete again with the same `save_id`. A `404 not_found` on this retry counts
  as a success, because the first delete was done (6.6).
- `409 conflict` gives "The dashboard changed. Read it again and try again.".
- In v1, the subcommand `restore` of the server brings a deleted dashboard back (5.40). The page has no list of deleted
  dashboards in v1 (10.16).

### 10.14 States that stop edits

| State | The page |
|---|---|
| `access.history = false` (no history in the store, `server.history` is `false`, or the PDP refuses `dashboard.history`) | No "History" entry. The page of 10.11 shows the refusal page of 9.6 when a user opens its URL by hand. |
| `access.export = false` (`server.export` is `false`, or the PDP refuses `dashboard.export`) | No "Export" entry and no "Download" entry on the card. No "Export this version" on a version view. |
| `read_only = true` on a dashboard or a folder (the store is read-only, 5.42) | A lock on the card and in the header of the editor. The editor opens read-only. The page offers "New dashboard", "New folder" and "Move" only in the primary store. A write that the page sends gets `403 read_only_store`, and the page shows its message. |
| `stale = "file_invalid"` on a dashboard (5.44) | A warning on the card and in the header of the dashboard: the page shows the last valid state, and the file of the volume is not valid now. The text never holds a path or a message of the parser. |
| `access.write = false` for a dashboard or a folder, or `access.create = false` (mode `authzen`, 11.25) | The same controls are absent for this dashboard or this folder. "New folder" is absent without `access.create`. |
| A source of the dashboard is not in `GET api/sources` | Edit mode opens, with the notice "A save needs the query right on the source <id>." (6.9). "Run" is absent for this source. |
| `503 storage_unavailable` on a write | The notice of 10.9. Edit mode stays. |
| `503 pdp_unavailable` | The notice of 9.6. The controls stay. The server refuses the request (fail closed). |
| A URL with `edit`, `editPanel` or `editview=json` or `settings`, and the user cannot edit (a deep link, "Back", "Forward" or a reload) | The page shows the state of 10.2a, rule 1, with no edit names. It shows the notice "You cannot edit this dashboard.". The draft stays in the browser. The page shows no "Resume". |
| The user opens `editview=history`, has `access.history`, and cannot edit | The history shows (10.11). It has no "Restore". |

The server checks each operation. The absence of a control is only a display rule (11).

### 10.15 Line diff

- The vector id of the diff is `FE-17`.
- The input is two texts. The page splits each text at each LF into lines.
- The algorithm is the shortest edit script of Myers (O(ND)), on exact line equality.
- The output has hunks with 3 lines of context. Each hunk starts with `@@ -a,b +c,d @@`.
- Each line starts with ` ` (no change), `-` (removed) or `+` (added), as text. Two gutter columns give the line
  numbers of both sides. A removed line and an added line also have their colour tokens.
- The diff has a limit: an edit distance of 10,000 lines, and 100,000 lines in a text. Past it, the page offers the
  download of each text, with this text: "The two versions differ too much to show the changes."
- The diff module is a pure function, and the tests run it with Node (16).

Example: from `["a", "b", "c"]` to `["a", "x", "c"]`, the diff is one hunk `@@ -1,3 +1,3 @@` with the lines ` a`,
`-b`, `+x` and ` c`.

### 10.16 What is in v1.1

These items are not in v1. Each one needs a reviewed design before its work starts (18):

- upload of a Grafana file in the page;
- a list of deleted dashboards and their restore in the page;
- a diff side by side;
- completion of names in the SQL text.

Drag and resize of the panels on the grid (10.5a) and the forms of 10.3a and 10.3b are done. They are not in this list.
