# Changelog

This file follows the layout of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and uses ASD-STE100 Simplified Technical English. The versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, a minor version can change a key, a route or the format of a document.

Each version has a section `## <version> - <YYYY-MM-DD>`. The newest version is first. The release scripts read the section of the version as the notes of the release. The section `## Unreleased` holds the changes that no release has yet.

## Unreleased

### Changed (breaking)

- A file needs exactly one store with `primary = true`. A file with no primary store, and a file with two, stop the start. The mode with no primary store is removed. Add `primary = true` to one store of each file. This breaks the file of an installation that ran with a read-only volume only: it needs a second, empty primary volume.
- The two keys `server.history` and `server.export` are mandatory. They are Booleans with no default. A file without them stops the start. The value `false` removes the right for each user, also in the mode `open`.
- The PDP gets two new actions, `dashboard.history` and `dashboard.export`. Hune publishes each one only when its key is `true`. A policy that grants neither action removes the history and the export for each subject. Add the two actions to the policy before the update.
- The error `editing_disabled` and the contradiction C21 are removed. The session keeps the member `editing`, always `true`. Its removal is planned.
- The PostgreSQL schema goes to version 2. Run `hune migrate-store` before `serve`. The step is one way: a binary of schema version 1 refuses a database of schema version 2.
- A file `<name>.json` at the root of a volume, with a name that fits the uid rule, was ignored. It is now a dashboard. A file that is not a valid dashboard, for example `package.json`, stops the start.
- The importer of Grafana puts the dashboards of the root of Grafana in no folder. The id `general` is no more reserved for the root. The report shows `null` for their folder.

### Added

- Each dashboard has `access: { write, history, export }`. The page hides "History", "Export" and "Download" when the flag is `false`. A dashboard of a read-only store has no history: the history routes give `404 history_unavailable`.
- The errors `403 history_disabled`, `403 export_disabled` and `404 history_unavailable`.
- A dashboard can have no folder. The API writes `null` for its folder. The create body and the move body take `"folder": null`. `GET /api/dashboards?folder=` with an empty value selects these dashboards, and `GET /api/folders` has the new member `root`.
- A volume reads a file `<uid>.json` at the root of its path as a dashboard without a folder. In PostgreSQL, the column `folder_id` can be null after migration 2.
- The access tree names the place of these dashboards the node `folder:_root`. A grant on a real folder does not cover them. The options route lists `_root`, "Root (no folder)", as the first folder.
- The list page shows the dashboards with no folder in a root group first, with no title. The root dashboards of a read-only store are the first group under its title. The header selector lists them first, each on its own.
- The dialogs New dashboard, Import and Move have the entry "Root (no folder)" first, when the user can write at the root. The page sends `null` for the folder. A conflict after a move to the root says so.
- The control of a `query` variable shows its state. While the options load, it shows a spinner and `Loading…`, and it is disabled. It keeps its width, so the bar and the grid do not move when the options come.
- A variable below a variable waits for its parent. The control says `waiting for <label>`, and the page sends no request of options for it until the parent is ready.
- A failed options request has a short sentence by type of error, for example "The query used too much memory." for the ClickHouse error 241. The popup of the control has a button "Retry" and the full text of the error, folded. The error lies over the page, so the bar does not move.
- A variable keeps its selection when its options fail. The panels run with it. A person can type a value in a failed open variable.
- The page keeps the answers of the options requests for 60 s (relative range) or 5 minutes (absolute range). A refresh sends no request of options when the answer is valid. A "Retry" ignores the cache.
- The control of a `multi` variable shows chips. A click on a chip opens the selector, and the cross removes the value. "All" is one chip. The chip "+<n>" shows the chips that do not fit. `Backspace` in the field of an open variable removes the last value.
- The browser cases B290 to B296.
- The browser cases B380 to B384 and the unit tests `charts_spec.test.mjs`. They test the chart panels against 4.26 and 9.19.
- The modules `var_errors.js`, `options_cache.js` and `var_chips.js`, and the style `var-select.css`.


### Changed

- A refresh or a change of the range asks again only for the variables whose key changed. A `query` variable below a `time_change` variable asks again when the values of its parents changed, and not at each change of the range.
- The page cancels the options requests that are in flight when the person leaves the dashboard. It also cancels them when a newer request of the same variable starts.
- The page budget of the front is 1116 KB (9.30). The new modules and styles use the room.

- The preview of a panel that loads for the first time uses the JSON document of the dashboard only. It never reads the text of a query, and it has no heuristic. Where the document gives no fact, the preview draws the least form that the document proves, drawn empty. A text that the document fixes shows as a bar of its measured width. The preview uses neutral tones and never a colour of the document.
- One light crosses the preview in 2 s. It moves with a transform only. With reduced motion, the preview is still.
- The preview is now in the folder `web/js/preview/`: a registry (`index.js`), shared helpers (`common.js`) and one module for each of the 12 families. Each family module gives the neutral form for now: an empty body with the light and the dots. The module `panel_preview.js`, the function `previewOf` of `panel_wait.js` and the heuristic sizes are gone. The browser cases B280 to B282 are replaced by B320 and B321.
- The measurement harness `tests/browser/lib/preview_measure.mjs` and the command `tests/browser/preview_measure.mjs` measure each preview and each loaded panel. They give the differences by tolerance class (9.11).

### Fixed

- A request of options that ended after a newer request of the same variable removed the cancel of the newer request. A cancel at the exit of the page then missed it.
- The comment of the SQL editor has 4.5:1 over the band of the current block in the light theme. Before, it had 4.36:1.
- A table legend with `values` empty showed five columns. It now has the column "Name" only, as 4.26 says. This holds for the time series, the xy chart, the histogram and the bar chart.
- A legend at the right that every entry leaves (`hide_from.legend`) kept the gap of 8 px, so the plot was 8 px too narrow. The plot now has the whole width.
- A legend at the right can move under the plot in a narrow panel. It had no cap for a list and a cap of 112 px for a table. It now scrolls beyond 35 % of the height, as the legend at the bottom does.
- The x axis of a time series with `draw` = `bars` covered the requested range. It now keeps the span of the data, as 9.19 says. A line, an area and points keep the requested range.
- A time series with every y axis hidden put all its series on the right scale, and the table legend marked each one "(right y-axis)". All series now go on the left scale, and no mark shows.
- The legend of a horizontal bar chart ignored `legend.mode`, `legend.values`, `legend.sort_by`, `hide_from.legend` and the cap. It showed an empty legend with no series. It now follows the shared object `legend` as the vertical form does. It draws a list or a table with its sort, and a hidden series has no entry. It keeps a cap of 35 % of the height. It moves under the plot when the plot would be less than 220 px wide.
- The value label of a horizontal bar was 8 px at least. It is now 10 px at least, as in 9.19 and in the vertical bars.
- The value axis of a horizontal bar chart put two ticks 18 px apart for small values, so the labels ran together. Two ticks are now 60 px apart or more, as 9.19 says.

## 0.2.0 - 2026-11-25

### Removed (breaking)

- The key `storage.backend`. The blocks tell the stores now. An old file stops the start with the error `unknown attribute`.
- The key `storage.volume.read_only`. A volume is read-only when it is not the primary store.
- The key `server.editing`. Edits are possible if and only if a store is the primary store. A file with no primary store behaves as `editing = false` did: the nine write routes answer `403 editing_disabled`, and Hune publishes no `dashboard.write` and no `source.query`.
- The contradiction C10 (`read_only` and `editing`) and the old text of C2 and C21.

### Changed (breaking)

- The block `storage.volume` becomes `storage.volume "<id>"`. It has `path`, `rescan_interval`, `title`, `primary` and `history_keep`. A file can have up to 16 volumes.
- `history_keep` is only for the primary store, and it is necessary there. The block `storage.postgres` has `primary`, `title` and `history_keep` too.
- A file of a volume that cannot be read or that is not valid stops the start, for each volume. The message names the label of the store, the file and the reason. A volume cannot show an invalid card any more. A row of PostgreSQL that is not valid still can.
- The commands of the command line work on the primary store only. `migrate-store` needs `storage.postgres` as the primary store.
- The folder ids and the dashboard uids are unique across the stores. The start stops when two stores share one.

### Added

- The JSON view of the edit mode looks like the editor of VS Code. It has the palettes Light+ and Dark+ (the tokens `--json-*`) with 4.5:1 or more in both themes. The JSON area of the panel has them too. The SQL area does not change.
- Brackets have three colours by depth. A band marks the line of the caret, and its number has the strong colour. A caret on a bracket puts a box on it and on its pair. The white space at the start of a line has guides, and the guide of the pair that holds the caret is darker.
- The selection has the colours of VS Code. The first syntax error has a wavy line. The scroll bars are 10 px wide. The row is 20 px. The area does not fold lines and has no minimap.
- The gutter of the dashboard JSON view marks each line that differs from the saved dashboard. A green bar is an added line, a blue bar is a modified line, and a red arrow shows where lines are gone. A red band shows the line of a syntax fault.
- The case B240 (`tests/browser/cases/json_view.mjs`) and the tests of `tests/frontend/json_area.test.mjs`.
- The list page shows a read-only store as one root group with a lock and its title, and its folders under it. A card of a read-only store has a lock and no three dots. A card whose file is not valid now has a warning mark, and it shows the last valid state.
- The header selector shows a read-only store as a row with a lock. The dashboard page of a read-only store has a lock in the header, in place of the switch "Edit". A dashboard with a file that is not valid shows one warning notice.
- The message of the code `read_only_store`: "This dashboard is in a read-only store. It cannot be changed."
- The list page has one `h1` for assistive technology, with no visible title.
- The module `web/js/stores.js`, and the cases B241 to B244 (`tests/browser/cases/stores.mjs`).
- The file `web/js/switcher.js`: the selector of the dashboards in the header. The button shows the title of the dashboard. Its menu lists the dashboards that the user can see. When the user sees two folders or more, each folder is a row that opens and closes its dashboards.
- The function `bindMenu` in `web/js/ui.js`. A button opens its menu with a click or with the keys Down and Up. A letter goes to the next line that starts with it.
- A preview for a panel that loads for the first time. It has simple shapes for the type of the panel: dots, bars, a ring, cells or lines. The shapes change their opacity only. They stop under `prefers-reduced-motion`.
- The file `web/js/panel_wait.js` and the style sheet `web/css/panel-loading.css`.
- The Save dialog has two boxes, "Save the current time range as the default" and "Save the current refresh as the default". A box shows when its value differs from the default of the draft. Both are off at first.
- The key `primary = true` on one store block. The primary store is the only store that accepts writes. Each other store is read-only and needs a `title`, the name of its root folder.
- The error code `read_only_store` (403). A write into a read-only store gets it. Hune asks `dashboard.read` first and never asks `dashboard.write` for such a target.
- The members `read_only` and `store` on the items of the list, on a dashboard and on the folders. The member `stores` in the session. The contradictions C30 to C34.
- The member `stale` with the code `file_invalid`. After the start, a file of a volume that becomes not valid keeps its last valid state, and the dashboard has this member. The log line `store.rescan_rejected` and the gauge `hune_store_invalid_files` tell it.
- Each store has its own period of poll.
- A person can select and copy the text of a panel. A drag over a legend item, a sortable header or the link of a cell selects its text and does nothing else. A double click selects a word and a triple click selects the text. The key Ctrl+C on a focused legend item copies its text. The module is `web/js/panels/copy_text.js`.
- A layer of transparent text over each chart canvas holds the axes, the titles and the labels that the chart draws. A person selects them and copies them, with one text on each line. A layer holds 400 texts at most. The module is `web/js/panels/text_layer.js`, and the chart engine of ClickHouse Dash is not changed.
- The browser cases B260 to B263 for the select and copy rules, with real mouse drags.
- A copy of a selection over log lines gives one text line for each log line, with the cells joined by one space. The rows of the horizontal bar chart and of the bar gauge give the name and the value on one line. The module is `web/js/panels/row_copy.js`.
- A triple click on a log line selects the whole line and leaves its details as they were. The key Ctrl+A with the focus on a log line selects the lines of the panel. The cursor over the text of data is the text cursor.
- The browser cases B264 to B266 for the logs, the cursor and the right button.

### Changed

- The page scrolls in one container under the header. The document does not scroll, so the scrollbar of the page starts under the header and the header keeps the full width of the window. The header is at the top at every width. A Back or a Forward puts the content at its old position. The browser case B272 measures this.
- The JSON view of the edit mode fills the page, down to one page padding above the bottom edge of the window. Before, it stopped about 84 px above the edge at 1440 by 900 px. The tab "JSON" of the settings view has the same rule.
- The page writes a name of the address (`from`, `to`, `timezone`, `var-<name>`, `refresh`) only when its value differs from the default of the dashboard. The address of the edit mode is `?edit=1` with the defaults. A copied address does not fix a value at the default.
- The theme control is one icon button. Each click gives the next choice in the order System, Dark, Light and System again. It has no menu and no popup. This differs from ClickHouse Dash on purpose: Hune has one button and no menu.
- The function `nextChoice` in `web/js/theme.js` gives the next theme choice. The exports `menuTarget` and `THEME_MOTION_MS` are gone.
- One primary button "New" replaces the buttons "New folder", "Import" and "New dashboard" in the header of the list page and in its empty state. It has a plus icon and a chevron. Its menu has "New dashboard", "New folder" and "Import", as the user may use them. It is the same button at every width.
- The tag chips are more vivid: the fill is 32 % of the colour of the tag and the edge is 65 %. The ink of the light theme is black at 30 %.
- The menu of the selector of the dashboards has the width of its button exactly. The button has a fixed width of 22 rem, or less in a narrow place. On a phone the menu is 224 px wide at least. A long title ends with an ellipsis.
- The button of the sort on the list page shows the order that applies, "A–Z" or "Z–A", and never the text "Sort".
- The dialog "New folder" has the field "Title" only. The page makes the id from the title, and adds a short suffix when the id exists.
- The dialog "Import" has the entry "New folder..." in the select of the folders, when the user can create a folder.
- The interval button of the time control shows a dash, "-", when the refresh is off.
- The selector of the dashboards replaces the crumbs of the header. In the view of one panel and in the panel editor, an arrow after the selector is the way back. These views have no name in the header: the texts "View panel" and "Edit panel" are gone. The button "Back to dashboard" of the view of one panel is gone: the arrow stays at every width.
- The sort control, the tag filter and the selector of the dashboards use one menu. The menu touches its button, as in ClickHouse Dash and in Stowr. The chosen line has a check mark. A count is at the right of its line.
- The badge `Last change <age>` is at the top right of a card, and the three dots are after it. The menu of the three dots has "Edit", "Export", "Move" and "Delete". The pencil and the bin are gone, and a card that the user cannot edit has no button. The badge shows only for a change that Hune recorded, when `server.editing` is `true`. A dashboard of a read-only volume has no badge.
- The three dots of a panel that waits are at the top right of the title bar, left of the menu. Before, they were in the middle of the body.
- The button "Save" is also active when the current range or refresh differs from the default. "Revert" follows the draft only.
- A panel that reloads keeps its content as it is. The page does not dim it any more, and the text "No data" stays.
- A reload that gives the same data and the same range draws nothing again. A new picture replaces the old one in one step, with no empty frame between them.

### Fixed

- A revert shows the saved text in the JSON view again. Before, the text of the discarded draft stayed in the area.
- The range, the refresh and the variables that a person sets in a view stay when the view ends. Before, the Back that ends it gave the old values.
- The resize handles of the panels no longer stay after the edit mode ends. The drag mark and the key shortcuts of a panel are also removed.
- The view of one panel has no page scrollbar any more: the panel fills the window with the page padding above and under it.
- A drag over a sortable header of a table, a legend item or a link of a cell did not select its text. A drag over a header sorted the column, and a drag over a link followed the link. The value at the end of a bar of the horizontal bar chart could not be selected.

## 0.1.3 - 2026-11-25

### Added

- The key `server.list_filters_min_dashboards`. It is a whole number from 0 to 1000, and the default is 6. The list page shows the sort control and the tag filter only when the user sees more dashboards than this number. The session gives the number to the page.
- The file `web/css/look.css` with the tokens of a calmer look, in the light and the dark theme.
- A new logo: a square with round corners and one line that goes up. The favicon, the sprite of the icons and the images of the site use it.
- The browser case B228 for the cards of the list page.

### Changed

- The list page shows one card for each dashboard. A card has the title on one line, a short note with the last change (`now`, `2h`, `1d`, `1w`, `1mo`) and the tags. The grid has columns of 400 px and is centred in the window.
- A card that the user can edit has a pencil that opens the edit mode, a menu with "Export" and "Move", and a bin. The bin shows under the same condition as the pencil. A card that the user cannot edit has no button.
- A folder title shows only when the user sees two folders or more. The menu of a folder with "Rename" and "Delete" is on the folder title.
- The sort control and the tag filter are in the header, after the brand.
- The tag filter keeps the cards with one tag. The URL parameter `q` holds this tag name. Before, `q` was a text that matched a part of a title, a uid or a tag.
- The buttons, the fields, the menus, the dialogs and the tag chips use the new tokens. The tag chip is a tint of its colour. The primary button has dark text in the dark theme.
- The entry redirect of the list page does not read `access.create`. This is as before. The text of the specification now says it.
- `tests/ci/all.sh` runs the jobs of the gate at the same time. It writes one log for each job in `target/ci-logs/` and prints the duration of each job. It runs each job to its end, also after a failure. The record `target/ci-all.ok` still needs all jobs to pass.
- The options of `tests/ci/all.sh` are `--list`, `--only JOB[,JOB]`, `--from JOB` and `--max-jobs N`.
- No job of the gate uses a fixed port of the host. The function `ci_free_port` takes a free port and reserves it.
- The vendor step does nothing when `Cargo.lock` has not changed, so the release binary is not built again for it.
- The jobs `pgsource` and `mongosource` run their two versions at the same time.
- The seeded random tests of `hune-core` run their seeds on all cores. The seeds 1 to 64, the cases and the checks did not change.
- Four timing tests of the front end take more runs under load, and the job `frontend` of the gate runs when the other jobs end. The limits did not change.
- The test of the PostgreSQL source PS14 repeats its warm-up when the source is busy. The tool `fetch-plugin.mjs` tries the download of the plugin up to 5 times. The check of the digest did not change.

### Removed

- The search field of the list page, the view control (folder view and list view) and the collapse of a folder. The keys `hune.list.layout` and `hune.list.collapsed` of `localStorage` are not read any more.
- The page title "Dashboards", the count of dashboards and the words "Can edit" and "Read only" on the list page.

## 0.1.2 - 2026-11-24

### Added

- The public documentation: a website at `https://procurauth.github.io/hune/` that `scripts/build-site.py` makes from the Markdown files, and the checks of `scripts/check-docs.py` for links, site, ASD-STE100 and hygiene.
- The guides `docs/GETTING-STARTED.md`, `docs/CONFIGURATION.md`, `docs/ARCHITECTURE.md`, `docs/SECURITY-MODEL.md`, `docs/OPERATIONS.md` and `docs/DEVELOPMENT.md`, and the index `docs/SPECIFICATION.md`.
- The demo in `examples/demo/`: synthetic ClickHouse data, a configuration and demo dashboards.
- `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, the issue forms, the pull request template and `THIRD-PARTY-NOTICES.md`.
- `hune help`, `hune --help` and `hune -h` write the usage text on the standard output and exit with 0. A known command with only `--help` or `-h` after it does the same.
- `hune --version` and `hune -V` are aliases of `hune version`.
- `hune healthcheck` is an alias of `hune health`. It takes the same options and gives the same exit codes.
- `hune config-reference` is an alias of `hune print-reference --format markdown`. It takes no option.
- The usage text gives the exit codes of the commands and the exit codes of `import-grafana`.
- The ops listener answers `GET /healthz` and `HEAD /healthz`. This path is an alias of `/livez`, with the same status, body and headers.
- Each answer of `server.listen` has the header `Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=()`. The ops listener does not send it.
- The image recipe `build/Dockerfile` sets the label `org.opencontainers.image.licenses` and `STOPSIGNAL SIGTERM`. SIGTERM was already the stop signal of Docker.
- The page has a skip link "Skip to content". It is the first stop of the Tab key, and it moves the focus to the main region.
- The page has the meta `color-scheme` with the value `dark light`, and the meta `robots` with the value `noindex,nofollow`.
- The page "not found" and the page of a dashboard that does not exist have a title, "Page not found" or "Dashboard not found". The browser tab shows "Page not found - Hune" or "Dashboard not found - Hune".
- The CI job `no-image` stops the CI when a file in `.github` publishes a container image or logs in to a registry. It needs no Docker and no secret. The release workflow does the same check before the build.
- The files `.editorconfig` and `.gitattributes` of the ProcurAuth family. Git stores each text file with LF line ends. Git never changes the bytes of pictures, archives, fonts and fuzz inputs.

### Changed

- The project licence is `MIT OR Apache-2.0`. The files `LICENSE-MIT` and `LICENSE-APACHE` replace `LICENSE`, and the release archive holds both. Before, the licence was Apache-2.0 only.
- The repository is `ProcurAuth/hune`. The git dependencies on `procurauth-core` and `procurauth-pep` use `https://github.com/ProcurAuth/procurauth`, at the same commit.
- The specification and the other documents are rewritten as public documents. The decision ids and the internal notes are removed. The section numbers stay the same.
- The class of the page selector of the editor lists has a new name, `flist__pages`.
- The workflow `ci` runs on a push to `main` and on each pull request. Before, it ran on a push to any branch or tag.
- The workflows pin each action to a full commit SHA, with its version in a comment. No checkout keeps the credentials in the work tree.
- Each job of the workflows `ci`, `conformance` and `release` has a time limit from 5 to 90 minutes. A job that hangs fails at this limit. Before, the GitHub limit of 360 minutes applied.
- The job `procurauth` of the workflow `conformance` runs `tests/conformance/run.sh` directly. When the script is absent, the job fails. Only the job `failover` keeps a stub.
- The git dependencies `procurauth-core` and `procurauth-pep` use the commit of the tag v1.1.1 of ProcurAuth. Before, they used a commit before v1.1. The series U of the conformance run has 127 cases: 65 passed, 0 failed and 62 skipped.
- An answer of the PDP with a reason `approval_required`, `approval_pending`, `approval_denied` or `approval_expired` gives `403` and the body `{"error": "approval_cannot_wait"}`. Hune has no approval flow. Before, the body was the usual `forbidden` error (case U113).
- The release workflow stops before the build when the section of the version in `CHANGELOG.md` is absent or empty.
- The release workflow stops when it cannot read the tags of `origin`. Before, it continued as if the tag did not exist.
- `.gitignore` also ignores `.env`, `.env.*`, `*.pem`, `*.key`, `*.swp` and `.DS_Store`. The test certificates and keys in `crates/hune-source/tests/fixtures/tls/`, `tests/api/tls/` and `tests/fixtures/config/` stay in the repository.
- `SECURITY.md` has a section about two known limits (RUSTSEC-2026-0387 and one large PostgreSQL row) and a table of the public test values. Its response times are in business days.
- `CONTRIBUTING.md` and the pull request template name the job scripts of `tests/ci/` and the tools that they use on the host. They also give the command of the STE checker.
- `docs/ACCESS.md` tells that Hune sets `plain_retries = 1` in the PEP, and when the PEP sends a question to the PDP again.
- Docs: the website uses the shared ProcurAuth style sheet and script. The main menu has the same eight entries as the other sites of the family. Reference and Changelog are new in it.
- Docs: Access, Specification, Status, Conformance, Development and Chart engine left the main menu. Each of these pages marks the menu entry of its section, and the footer links to it.
- Docs: the footer has a ProcurAuth column with links to the family home page, the specification, portal and Stowr. The page 404 uses the common layout of the family.
- Docs: each local image has its width and height. The hero image loads first. The other images load when they come near the visible part of the page.
- Docs: the social preview image `docs/assets/og.png` has no colour bands now.
- Docs: the README has the section "The ProcurAuth family", with the same picture and table of parts as the other ProcurAuth repositories.
- Docs: the steps to apply the edit E25 again in `docs/engine-patches.md` are a numbered list, with one instruction in each step. The status of the applied edits is "applied, not yet reviewed".
- Tools: `tools/ste_lint.py` gives the warning S20 only for a procedural sentence. It checks the text of a block quote. A bold run-in label of 8 words or fewer does not count as a sentence.
- Tools: `scripts/check-docs.py` reports a local image that has no width and height.

### Fixed

- A token with an `nbf` in the future, a signed `nbf` or a fractional `nbf` gets `401`. The newer PEP library checks it (case U120). Before, Hune accepted such a token.
- The timing tests MS05 and A41 no longer fail on a slow host. They are tests only: the product did not change.
- In the forced colors mode of the system, the mask glyphs show in the system text colour. These glyphs are the sort mark of a table, the check mark of a list and the resize mark of a panel. The type glyphs of the editor keep their own colours.
- `check-config` names `access.identity.clock_skew` for a PEP error on the clock skew. Hune sets some PEP values from constants. A PEP error on such a value is now an internal error, not an error on a key of the file. With the pinned PEP, no output changes.
- The job `import` no longer hangs on a runner. The service `plugin-fetch` ends with the code 0 at the start. `docker compose up --abort-on-container-exit` then stopped ClickHouse, 5 of the 323 expansion vectors differed, and the project did not end. The job now uses `docker compose run`, shows the output at once, and has a time limit of 25 minutes.

### Removed

- The script `tools/rename-hune.py`.

## 0.1.1 - 2026-11-23

This release changes the loader of a panel. The checks ran on a local machine with `tests/ci/all.sh`. It has the `amd64` archive only.

### Added

- The script `tests/ci/all.sh` runs the jobs of the merge gate in order and stops at the first failure. The option `--from` continues a run after a fix.
- The script `tests/ci/release.sh` makes the release from a local machine. It checks the input, makes `SHA256SUMS`, makes and checks the draft release with `gh`, and publishes it. The option `--amd64-only` releases the `amd64` archive only.
- With these two scripts, a maintainer can run every job and the release with no GitHub Actions and no GitHub billing.

### Changed

- The loader of a panel is now three small dots that pulse in the middle of the body. They show 300 ms after the request starts. The old loader was a dashed bar under the title bar.
- A panel that refreshes still dims its earlier content to 60 %. The spinner in the title bar and the grey block of a panel that waits are removed.
- With `prefers-reduced-motion: reduce`, the dots do not pulse. A print has no dots.
- The workflows in `.github/workflows` stay as an option. They run the CI and the release on GitHub. Hune does not depend on them.
- The script `tests/conformance/run.sh` takes the commit of ProcurAuth from `Cargo.toml` only. No second pin exists.
- The job `policy` scans `tests/compose`, `tests/conformance` and the deployment files for image pushes, in addition to the workflows.

## 0.1.0 - 2026-11-22

This is the first release of Hune. It holds the work of the milestones that exists on this date. Version 1.0.0 comes after the exit criteria of the milestone M7.

### Added

- The server `hune` and the commands `serve`, `health`, `version`, `check-config`, `print-config` and `print-reference`. The configuration file is closed HCL, with a generated reference.
- The document format 3 in JSON: a closed schema, canonical bytes and an etag. The command `migrate-documents` changes the formats 1 and 2 to format 3.
- Two storage backends: a volume (a folder) and a PostgreSQL database. The commands `migrate-store`, `copy-store`, `export`, `import` and `restore` work on both.
- Three kinds of data source: ClickHouse, PostgreSQL and MongoDB. Each kind has a closed check of the query text, limits, cancellation and a probe.
- The importer `hune import-grafana` for the dashboards of Grafana 12.4.2, with a report, an acceptance file and an nginx map.
- The page: the list, the dashboard view, the time picker, the variables, 15 panel types, the transformations and two colour themes.
- The editor: the panel forms, the query editor with a syntax tree, the history of versions, the conflict screen and the restore.
- The access modes `open` and `authzen`. In mode `authzen`, Hune is a PEP of ProcurAuth.
- The logs, the metrics, the audit lines and an OTLP exporter of traces.
- The panel types `gauge`, `logs` and `geomap`. The map is off by default. Only the closed configuration block `geomap` gives it a tile source.
- The stat: the percent change and the text sizes.
- The time series and the bar chart: the gradient modes and the series options (dashes, axis width, border and colour, log base 2).
- The bar chart: the fill opacity, the line width, the grid lines and the orientation `auto`.
- Ad hoc filters on a ClickHouse source, for the columns of the configuration key `adhoc_columns` only, with the operators `=` and `!=`.
- The ClickHouse query parameters in the strict form `{name:Type}`, with a closed list of types.
- Annotations from PostgreSQL and MongoDB sources, with the rules of the ClickHouse annotations.
- 30 more units and 6 more data link tokens of Grafana.
- The time forms of Grafana in the URL, and the cell value inspect of a table.

### Changed

- Development builds before this release used another product name. This release is the first one with the name Hune. No compatibility layer exists for those builds.

### Security

- Security reviews of the code on 2026-11-14 and on 2026-11-16 found problems. Each high and medium problem has a fix and a test.
- The review of 2026-11-22 examined the new features of this release. Each high and medium problem has a fix and a test.
- Fixed: the ClickHouse lexer of Hune did not know three rules of ClickHouse 26.8. A comment can start with `//`, a line comment ends at LF only, and a heredoc tag can start with a digit. With such SQL, a variable value could run as code. A test sends 87 forms to ClickHouse and compares them with the lexer of Hune.
- Fixed: `$__adhocFilters` must be one AND term at the top level of the WHERE or PREWHERE clause, so a filter never adds rows. A map does not ask again for a failed tile, and it keeps decoded tiles only.

### Known limits

- The milestone M0 and the checks on real data (the parity spike and the verification V1 to V5) are not done. All tests use synthetic data.
- This release can have no `arm64` archive. The release workflow builds `arm64` only when its input `arm64` is `true`.
- The nightly job runs no fuzz target, because no pinned image has a nightly toolchain.
- The fuzz campaigns, the 30-minute load run and the CloudNativePG drill of M7 are not done.
- Some open questions remain. The file `docs/implementation-status.md` and section 19 of the specification list them.
