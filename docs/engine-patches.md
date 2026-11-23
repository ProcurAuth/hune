# Edits of the chart engine

This document holds the edits that Hune makes in the local copy of the ClickHouse Dash chart engine (9.24).
It is not normative.

## Rules

- Nothing is sent upstream by this work. No pull request, no issue and no message goes to the ClickHouse Dash repository.
- Each edit in a copied file of `web/js/vendor/chdash/` has a comment `// hune:` with the reason. The edited copies are `app_chart_core.js`, `app_highlight.js`, `app_results_sort.js` and `app_ui_table.js`.
- The lock file `web/js/vendor/chdash/chdash.lock.json` holds the new hash of the file. The field `upstream_sha256` keeps the hash of the upstream file.
- The table below has one row for each edit. The rows that name the same lock hash are in one file.
- The rows E17 to E21 and E35 to E42 remove code that Hune never calls. They are in the section "Removals" below. The page measure of 9.30 limits the size of the page, so savings come first.
- The rows E22 to E24 are the edits for the heatmap and the histogram. The row E25 is the rotation of the x labels of a bar chart. The row E26 keeps the line of a bar inside the bar. The row E27 is the top of a plot with no y axis.
- The rows E28 and E29 remove two patterns whose time grows with the square of the cell length. E28 is in the shim `app_results_values.js`, which is not a copy, so the patch does not hold it.
- The row E30 gives the data ends of a scale to the hook `yAxis`. A time series then has the y range of Grafana. The row E31 cuts a long axis title with a binary search.
- The rows E32 and E33 stack a series below 0 down from 0, in a group of its own, as Grafana 12.4.2 does. The row E34 keeps the line of a bar inside the bar on its four sides. It also draws no fill at the opacity 0.
- The rows E43 to E46 are the gradient modes and the cosmetic series options. The next free number is E50.
- The row E47 makes the page lexer read `//` as a line comment, as ClickHouse 26.8.34.13 does (7.5). It is in `app_sql_lex.js`.
- The file `vendor/chdash.patch` holds the diff of all the marked edits against the upstream files (section "Removals").

## Proposals

| Id | Area | Edit 9.24 | What the edit does | Upstream file and place | Lock hash after the edit | Test | Status |
|---|---|---|---|---|---|---|---|
| E7 | Time zone | 7 | The chart gets a zone: a clock with `parts(ms)` and `epoch(y, mo, d, h, mi, s)` (option `zone`, made by `zoneClock` of `web/js/zone.js`). The ticks of the x axis, the label of a tick, the cursor time and the tooltip head use it. Without the option, the clock is the one of the browser. | `src/static/app_chart_core.js`: the time section (`timeTicks`, `subDayTicks`, `dayTicks`, `monthTicks`, `yearTicks`, `formatInstant`) and `xReadout` | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/zone.test.mjs` | proposal |
| E8 | Thresholds | 8 | The option `drawUnder(ctx, layout, env)` draws inside the plot after the grid and before the series. The page draws the lines and the bands of the thresholds with it (`web/js/panels/thresholds.js`). | `src/static/app_chart_core.js`: `draw` | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/panel_chart.test.mjs` (shapes), `vendor_lock.test.mjs` | proposal |
| E9 | Legend | 9 | The legend table takes any calculation as a column, in the order of the option. A head is a button that sorts the rows (up, down, series order), and the first head reads "Name". The options `legendSort`, `legendCalc`, `tooltipSort` (`asc`), `tooltipHideZeros` and the series flags `legendHidden` and `tooltipHidden` apply the format. The initial option `hidden` hides the series with `hide_from.viz`. | `src/static/app_chart_core.js`: `legendColumns`, `renderLegend`, `renderTooltip`, `tooltipSeries`, the cursor dots | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/panel_chart.test.mjs`, `vendor_lock.test.mjs` | proposal |
| E10 | Bars | 10 | Bars on a time axis draw the bars next to the range, cut by the plot (a partial bar at the left). New options: `barFillAlpha`, `barLineWidth` (a line around each bar), `barMaxWidth` (replaces the cap of 56 or 64 px), `barInnerRatio` (the share of a bar in its group), `barLabel(ctx, bar, layout)` (the value of each bar) and `xGrid: false`. | `src/static/app_chart_core.js`: `viewOf`, `drawBars`, `drawAxes` | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/panel_axis.test.mjs`, `vendor_lock.test.mjs` | proposal |
| E11 | XY chart | 11 | The titles of the axes (`xTitle` under the labels, `yTitle` turned at the left) take 20 px at the bottom and 27 px at the left. The option `xTicksFn` gives the ticks of a number axis. | `src/static/app_chart_core.js`: `yAxisOf`, `ownGutter`, `drawAxes`, `xTicks` | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/panel_axis.test.mjs`, `vendor_lock.test.mjs` | proposal |
| E12 | X ticks | 12 | The x axis keeps the width of a label and 11.5 px between two ticks (it was the width and 22 px). The steps are those of Grafana: no step of 2 minutes, steps of 4 and 8 hours, steps of 1 to 6 and 7 and 14 days that start at the first midnight in the range. A step of one minute writes the seconds. A range over 24 hours writes the date before the time, and a shorter one does not write the date, also at midnight. | `src/static/app_chart_core.js`: `timeTicks`, `clockText`, `X_TICK_GAP` | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/zone.test.mjs` | proposal |
| E13 | Right y axis | 13 | The chart gets a second y scale, `y2`, and a right axis. The option `scales` maps the id of a series to `y` or `y2`. The option `y2` holds the options of the right scale (`yAxis`, `yBounds`, `ySoft`, `yTickFormat`, `yTitle`, `yScale`, `shown`). Each scale has its own range, ticks and labels. The right gutter is the widest label plus 9 px, and the left gutter is now the widest label plus 9 px (it was 11 px). A label ends 5 px before the plot at the left, and starts 5 px after it at the right (it was 6 px). A side with no axis has a margin of 8 px (`yShown`, `y2.shown`). The grid follows one scale (`gridScale`). A stack works inside one scale. A series draws and has its cursor dot on the view of its scale. The table legend ends the name of a series of the right scale with ` (right y-axis)`, and the list legend puts these items at the right end of the row. | `src/static/app_chart_core.js`: `yAxisOf`, `axisNow`, `ownGutter`, `computeLayout`, `computeStacks`, `drawAxes`, `drawOverlay`, `nearestSeries`, `legendItemHtml`, `renderLegend` | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/panel_axis.test.mjs`, `fe24_axis_plan.test.mjs`, `vendor_lock.test.mjs`, browser case B60 | proposal |
| E14 | Series style | 14 | A series can hold `type`, `width`, `fill`, `dash`, `dashCap`, `points`, `pointSize` and `negate`. The option `drawOrder` = `series` makes the engine draw the bars and the lines in the order of the series. The bar series are one cluster, and each one draws in its turn. A width of 0 draws no line. A dash uses `setLineDash` with the cap of the series (`butt` for `dash`, `round` for `dot`). A series with `negate` draws `-v`: the range, the stacks, the dots and the lines use the drawn values. The values of the series, the legend and the tooltip stay the data. | `src/static/app_chart_core.js`: `drawSeries`, `drawColumns`, `drawBars`, `draw`, `washOf`, `strokeDashed`, `ownMark`, `yAxisOf` | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/panel_chart.test.mjs`, `vendor_lock.test.mjs`, browser case B61 | proposal |
| E15 | Right y axis | 15 | The title of the right axis turns by 90 degrees. Each title has the font of the labels, takes 28 px (it was 27 px) and has its centre 12 px from the outer edge. A title longer than the axis ends with an ellipsis. The option `ySoft` = `[soft_min, soft_max]` moves the end of the data of a scale before the automatic rule. | `src/static/app_chart_core.js`: `drawAxes`, `yAxisOf` | `69e714f54bb49d3990dbe53b19bcbe468a433df1f82d438cc07172377e035b6a` | `tests/frontend/panel_axis.test.mjs`, `vendor_lock.test.mjs`, browser case B60 | proposal |
| E16 | Highlighter | 16 | The highlighter pushes the tokens of a text with a loop (`pushAll`) and not with a spread. A text of about 125,000 tokens, for example `SELECT 1,1,1,...`, made the spread throw RangeError "Maximum call stack size exceeded" in `lexAllJs`, `tokenizePlain` and the partial scan. | `src/static/app_highlight.js`: `tokenizePlain`, `lexAllJs`, `scanPartial` | `5312c150842c0fcda4d3dcf03680f07ac28f737d74c80b9de0ea8e3d5139abf0` (the file `app_highlight.js`; the other rows name the hash of `app_chart_core.js`) | `tests/frontend/ast_fix.test.mjs`, `vendor_lock.test.mjs` | proposal |
| E22 | Heatmap, histogram | 22 | The option `cellGap` (CSS pixels, default 1) replaces the fixed gap of 1 px of `drawCells`. A cell of 3 px or less still has no gap. | `src/static/app_chart_core.js`: `drawCells` | `8d5cbfc05a571e9923f4dde9e015cf5f3ea6b86025fc13c9d06e9a9622185952` | `tests/frontend/vendor_lock.test.mjs`, browser case B142 | applied, to re-read by the owner |
| E23 | Heatmap, histogram | 23 | The option `barOverlap` makes all the bar series of a chart that is not stacked share one place on the x axis, so the bars overlap, as in Grafana. | `src/static/app_chart_core.js`: `drawBars` | `8d5cbfc05a571e9923f4dde9e015cf5f3ea6b86025fc13c9d06e9a9622185952` | `tests/frontend/vendor_lock.test.mjs`, browser case B145 | applied, to re-read by the owner |
| E24 | Heatmap, histogram | 24 | The option `barSlot` (x units) is the slot of a bar and replaces the smallest gap of the x values, so one bucket fills the plot. The option `barDecimate: false` turns off the choice of the highest bar for each 3 px. | `src/static/app_chart_core.js`: `drawBars` | `8d5cbfc05a571e9923f4dde9e015cf5f3ea6b86025fc13c9d06e9a9622185952` | `tests/frontend/vendor_lock.test.mjs`, browser cases B145 and B145b | applied, to re-read by the owner |
| E25 | Bar chart | 25 | The option `xTickRotation` (degrees, positive is clockwise, the start of the text at the tick, as `xTickLabelRotation` of Grafana 12.4.2; the sign changed on 2026-11-15 after a measure on Grafana) turns the labels of a category axis about their anchor. The bottom margin is 9 px plus the height of the tallest turned label. A turned label is shown by its turned width (`turnedBox`), a label is cut with an ellipsis when it would leave less than 40 px of plot, and the anchor stays where the whole label is in the canvas. The value 0, or no option, keeps the old code path and the same picture. | `src/static/app_chart_core.js`: `layoutXLabels` (new last parameter), the new function `turnedBox`, `computeLayout`, `yAxisOf` (the band), `short` and `rotation` (new closure functions), `xTicks` (`every`), `drawAxes` (the labels and the y of the title) | `7a73b9438b64d1d24aa7220910e4f71fc05d33d8312621a057747a4fc084aa18` | `tests/frontend/tick_rotation.test.mjs`, browser case B153 | applied, to re-read by the owner |
| E26 | Histogram | 26 | The option `barInset` keeps the line of a bar inside the bar at its top and its bottom, as in Grafana: the box of the bar loses half the line width at each end. The x and the width do not change. Without the option, or with no line, the boxes are the same as before. | `src/static/app_chart_core.js`: `drawBars` | `7a73b9438b64d1d24aa7220910e4f71fc05d33d8312621a057747a4fc084aa18` | `tests/frontend/bar_inset.test.mjs`, the visual capture of `histogram_f` (15.14c) | applied, to re-read by the owner |
| E27 | Heatmap, time series | 27 | When the left axis and the right axis both do not show, the plot starts at the top of the canvas and takes the 8 px of the margin, as in Grafana 12.4.2 (a heatmap with a hidden axis: y 0 and 170 px; a time series with all its y axes hidden: y 0). An axis that shows on one side keeps the margin. No new option: only `panels/heatmap.js` and `panels/timeseries.js` set `yShown`. The fit of a turned label (`short`, E25) still subtracts the constant `PAD_TOP`, which has no effect because a bar chart always shows its value axis. | `src/static/app_chart_core.js`: `yAxisOf` (a new line after the line of E19, the plot height and the returned `padTop`) | `4431fe602662f40bf329b813783b7925eb19bad9b35cac4bfe62f4d459dd4935` | `tests/frontend/plot_top.test.mjs`, browser cases B60 and B143, the visual capture of `heatmap_c` and `parity_axes` (15.14c) | applied, to re-read by the owner |
| E28 | Security | 28 | `NUMERIC_RE` gets the form `^[+-]?(?:\d+(?:\.\d*)?\|\.\d+)(?:[eE][+-]?\d+)?$`. It matches the same texts as the upstream form (no difference on 2,000,000 random texts and on each text of 6 characters or less of a small alphabet). A run of digits matches in one way only, so a cell of 40,001 characters takes 0.1 ms, not 1.1 s. The page also uses it for `convert_field_type` (`transform_more.js`) and for the row names of a heatmap (`heat_model.js`). | `src/static/app_results_values.js`: `NUMERIC_RE` (the shim of Hune; upstream line 10) | `1281308647e09149e5fd915c965a6e3ec6a99b3da7a852b53d84c92b25cc3a87` (the shim) | `tests/frontend/page_security_v3.test.mjs`, `vendor_lock.test.mjs` | applied, to re-read by the owner |
| E29 | Security | 29 | The function `decimalParts` cuts the sign, the zeros at the start of the integer part and the zeros at the end of the fraction with string steps. It replaces the pattern `DECIMAL_TEXT`, which took 0.9 s for one compare of two PostgreSQL `numeric` texts of 16,385 characters. `normalizeNumericForSort` keeps the parts of a long decimal text (member `p`). `compareNumericForSort` is now two steps, `normalizeNumericForSort` and `compareNormalizedForSort`, and the file exports both. `table_model.js` normalises each row once. The result of each compare does not change. | `src/static/app_results_sort.js`: `DECIMAL_TEXT`, `compareDecimalText`, `normalizeNumericForSort`, `compareNumericForSort`, the export line | `756098fcb2de82a605c2e7246427abaca2821b660d14fb2c87226d7f7d24a1ae` | `tests/frontend/page_security_v3.test.mjs`, `vendor_lock.test.mjs` | applied, to re-read by the owner |
| E30 | Y range | 30 | The hook `yAxis(min, max, plotH, raw)` gets a fourth argument `raw`: `[lo, hi]`, the lowest and the highest drawn value of the visible series of the scale, or `null` when no series shows. In a stack, the tops count and the bases do not. The soft ends of `ySoft` do not count. The first three arguments do not change. The time series computes the range of Grafana 12.4.2 from `raw` and its own ends (9.19). The bar chart and the other hooks do not read `raw`, so their axes do not change. | `src/static/app_chart_core.js`: `yAxisOf` (the loop of the data ends and the call of `customYAxis`), `customYAxis`, the list of the options | `e017be8dfd7142fd4dc618cbfdb5f80eeb369e0c2d18f200155d7ad284d57ea2` | `tests/frontend/y_range.test.mjs`, `fe24_axis_plan.test.mjs`, browser cases B60 and B61, the visual capture of the fixture of the worker y-range | applied, to re-read by the owner |
| E31 | Security | 31 | `fitTitle` cuts the text to `TITLE_MAX_CHARS` (256) characters first. Then a binary search finds the longest start that fits with the ellipsis, and 1 character at least. The upstream code took one character away at a time and measured again each time: a series name of 40,000 characters as the title of an xy chart made 40,000 measures and filled and cleared the width cache of 2,000 texts 20 times. Now a title takes 9 measures or less. A title of 256 characters or less gets the same text as before when the width grows with the length. | `src/static/app_chart_core.js`: `fitTitle`, the new constant `TITLE_MAX_CHARS` after `MAX_CATEGORY_CHARS` | `e017be8dfd7142fd4dc618cbfdb5f80eeb369e0c2d18f200155d7ad284d57ea2` | `tests/frontend/y_range.test.mjs` | applied, to re-read by the owner |
| E32 | Stack | 32 | The option `stackSign: "series"` (only the time series sets it) puts each series of a stack in a group by the sign of most of its values. The count reads about 100 values from the first value to the last one, and a value of 0 does not count. A series with more values below 0 goes in the group below 0, and `negate` changes the group. Each group stacks from 0. A value adds to the top of its group with its sign: a series that stacks up can go down. A `null` value takes the top of its group when another series of the group has a value at that x, else the top is `NaN` and has no point. The line of a stacked area then breaks only where the series flags a `null` (`nulls`), so `span_nulls` connects it. In percent, a top is the signed sum below it divided by the signed sum of the group, times 100. Without the option, the stack does not change: the values add up, and bars stack their values below 0 down. | `src/static/app_chart_core.js`: `computeStacks`, `drawSeries` (the `trace` call of a stacked area), the list of the options | `d7dc3ca7bed1c1d9f8e182939caa9cc78b1042c3a4ee93adab48bb1f8d36cf06` | `tests/frontend/stack_sign.test.mjs`, `y_range.test.mjs`, the visual capture of the fixture of the worker y-stackneg | applied, to re-read by the owner |
| E33 | Stack | 33 | With the option `stackSign: "series"`, the key of the stack group of a series also holds its `type` when it has one. A series with a `type` draws another way than the chart: bars, points or a line. Thus a line series and a bars series of one chart stack in two groups, each from 0, and a percent is of the sum of its own group. Grafana 12.4.2 has this rule in `getStackingGroups`: the key holds the draw style, and the bar alignment or the line interpolation. Hune has no bar alignment and no interpolation for one series, so the type is the whole rule. The time series and the bar chart set the option. | `src/static/app_chart_core.js`: `computeStacks` (`stackKey`), the list of the options | `fda9edce0ba59096d1dace23f910d54f7768f36f775449359bd46fc10a482b6a` | `tests/frontend/stack_parity.test.mjs`, the visual capture of the fixtures of the worker stack-parity | applied, to re-read by the owner |
| E34 | Bar chart | 34 | The value `"box"` of the option `barInset` (E26) also moves the left side of the box of each bar right by half the line width, and the right side left by the same amount. The line of the bar then stays inside the bar on its four sides. A bar series with the fill alpha 0 gets no `fill` call. Grafana 12.4.2 draws the box of a bar in this way, and it makes no fill call at `fillOpacity` 0 (probe of the canvas calls on 2026-11-22: the box 22, 17, 352, 190 at the line width 0 and 24, 19, 348, 186 at 4). The value `true` of E26 and an absent option keep their boxes, so the histogram does not change. Only the bar chart panel gives `"box"`. | `src/static/app_chart_core.js`: `drawBars` (the `rect` of a bar and the `fill` of a series), the list of the options | `8a86fda64317edf348cc64e0c776d5ba6c9d2e64d8a07647ad8ac886b65d8cac` | `tests/frontend/barchart_p03.test.mjs` (10 tests, 9 fail before the edit), browser case B216 | applied, to re-read by the owner |
| E43 | Gradients | 43 | A series can hold `paint(part, ctx, L, c, alpha)`. The engine calls it for the fill (`fill`) and for the line and the points (`line`) of a line, a dense line, a stacked area and bars, and uses the style that it gives, or the plain colour for `null`. The view of a log scale holds the view in the units of the values (`real`). The time series and the bar chart give it for `gradient_mode` (9.19). | `src/static/app_chart_core.js`: the new helper `paintOf` before `washOf`, `drawColumns`, `drawSeries`, `drawBars`, `applyLogScale` | `08714c7b11a98e083d294e590ecb73016fe2bc25f99f060be18a32db7521cb03` | `tests/frontend/fe83_series_options.test.mjs`, browser case B223, the visual capture of the fixture of the worker hB-series | applied, to re-read by the owner |
| E44 | Series options | 44 | The option `barAlign` (-1 or 1) moves each bar by half its width plus half its line, so the bar ends half its line before its x or starts half its line after it, as uPlot `bars({align})` of Grafana 12.4.2. Without the option, the bars do not move. | `src/static/app_chart_core.js`: `drawBars` | `08714c7b11a98e083d294e590ecb73016fe2bc25f99f060be18a32db7521cb03` | `tests/frontend/fe83_series_options.test.mjs`, browser case B224 | applied, to re-read by the owner |
| E45 | Series options | 45 | `strokeDashed` divides each length of the dash array by the device pixel ratio, so the array is in device pixels, as Grafana draws it. At the ratio 1 the picture does not change; at 2 the dashes of `dash` and `dot` become half as long in CSS pixels, as in Grafana. | `src/static/app_chart_core.js`: `strokeDashed` | `08714c7b11a98e083d294e590ecb73016fe2bc25f99f060be18a32db7521cb03` | `tests/frontend/fe83_series_options.test.mjs`, browser case B224 | applied, to re-read by the owner |
| E46 | Series options | 46 | The options `yWidth`, `yBorder` and `yColor` (and `y2.yWidth`, `y2.yBorder`, `y2.yColor`) give the gutter of a y axis in CSS pixels (a title adds 20 px), a line on the edge of the plot with a mark of 4 px at each tick (the labels move 4 px out), and the colour of the labels, the line and the marks (a CSS colour or a function that gives a canvas style). Without the options, the axes do not change. | `src/static/app_chart_core.js`: `ownGutter`, `rightGutter`, the new functions `axisPaint` and `axisBorder` before `drawAxes`, `drawAxes` | `08714c7b11a98e083d294e590ecb73016fe2bc25f99f060be18a32db7521cb03` | `tests/frontend/fe83_series_options.test.mjs`, browser case B224 | applied, to re-read by the owner |
| E47 | Security (7.5) | 47 | `spanAt` reads `//` as the start of a line comment (`LINE`), as `--`. The comment ends at the next LF, as before. The head comment of the file now says that ClickHouse does not end a comment at CR, and that `//` starts one. Without the edit, the page read the quote marks of two `//` comments as one string: the scan of placeholders (9.10) and the colours then differed from ClickHouse. The server never uses this lexer for a check. | `src/static/app_sql_lex.js`: the head comment, `spanAt`, the comment in `nextSpan` | `6dff9475422e639225b590bf88bab436027cf433aa1c4f6c16f18a4b1749e783` (the file `app_sql_lex.js`) | `tests/frontend/sqlscan.test.mjs` (the case "slash comments hide their quote marks" and the test of the page lexer) | applied, to re-read by the owner |

## Removals

The local copy loses the code that Hune never calls. Nothing goes upstream.

### Rules of the removals

- Each place of a removal has a comment `// hune: E<n>` with the reason. The code that a panel type, F31 or F43 can call stays. Code in doubt stays.
- The file `vendor/chdash.patch` is the readable diff of all the marked edits E1 to E21 against the upstream files of the lock commit. Its paths are the paths of the lock, under `web/`.
- The patch is outside `web/`, because the build refuses a file of `web/` with an extension that 9.3 does not list.
- `tools/check-vendor-lock.sh --chdash` applies the reverse of the patch to the copies. The result must give the upstream hashes of the lock. The job `policy` runs this check (16.18).
- After an edit of a copy, run `tools/check-vendor-lock.sh --chdash-update <checkout>`. The checkout is a ClickHouse Dash repository. The tool reads each upstream file at the lock commit with `git show` and changes no file of the checkout. It writes the patch and the hashes of the lock again.
- Then read the patch and run `tools/check-vendor-lock.sh --chdash`. The file list of the lock stays a manual edit.
- The test `tests/frontend/engine_removals.test.mjs` fails when a removed name comes back in `web/js` as a call, an engine option or an import.

### Proof that Hune never calls the removed code

- The import graphs: the page (the walk of 9.30, 73 files) and the editor (`editor_loader.js` and each module of `js/editor/`, 135 files). The tests of `tests/` are in the search too.
- Four files call the engine: `panels/common.js` makes each chart for `timeseries.js`, `barchart.js` and `xychart.js`. Their option objects have no key `markers`, `brush` or `annotations`. Hune calls only `update`, `layout`, `hideTooltip`, `destroy`, `themeChanged`, `setLegendMode` and `root` of a chart.
- `panels/table.js` is the only importer of `app_ui_table.js`. It reads the members `sortHeader`, `setSort` and `rovingRows` only. It never reads a member by a computed name, and it never gives the object to other code.
- `reduce.js` and `table_model.js` import `compareNumericForSort` and `compareTextForSort` only from `app_results_sort.js`. No test imports this module.
- Only `app_ui_table.js` imported `app_ui_copy.js`, `h` and `shell`. A search of each removed name finds no use outside its file. Three names are also names of Hune code with another use: a parameter `rowHeight`, `copyCell` of `transform_more.js` and `copyText` of `ui.js`.
- The queued specs F27, F28, F31, F41 and F43 name no removed function or option.
- E35 to E42: the search covers each removed function, option and member. It finds no use outside its engine file in `web/js` (the page and the editor) or in `tests/`. The only option of these removals that Hune gave is `legendModeButton: false` in `panels/common.js`. E41 removes it with the button.
- `app_util.js`: the page and the editor read only `util.escapeHtml` (`app_chart_core.js`, `app_ui_chart.js`, `app_ui_stat.js`, `app_highlight.js`) and `util.rafOnce` (`app_chart_core.js`).
- `app_palette.js`: `app_chart_core.js` was its only importer, and it read only `palette.batch`. The shim `app_wasm.js` gives `null` for each kernel, so the kernel never ran and `parseColors` always called `parseColor`.
- `app_ui_state.js`: the engine calls only `uiState.announce`. `app_ui_chart.js`: `panels/stat.js` calls only `chartUi.sparkline.draw`.
- E40 to E42: without the option, each removed branch was a path that the code did not take. The code that stays gives the same result. For example, `legendToggles` still toggles on Ctrl or Cmd+click, and the title of a legend item keeps its text.

### What stays, and why

- The regions, `placeBox` and `setRegions`: the heatmap of F31 marks the cell under the pointer with a region (9.19e).
- The pick, `pickTooltip`, `showTooltip` and `hideTooltip`: F31 uses the pick of a cell, and `panels/annotate.js` calls `hideTooltip`.
- The cells, the bars on a number axis, the place of the bars and the choice of bars: F31 and its edits E22 to E24 need them.
- `points()` and `toClient()`: no file calls them now, but a browser case can use them (a doubt).
- The test `tooltipKey !== "marker"` is now always true. It stays, so that the patch is smaller.
- The CSS copies stay whole. The measure of 9.30 does not count CSS. The CSS of the button of the legend mode also stays (`chart.css`, `panel-chart.css`).
- The copy `app_format.js` stays in the lock, but no file imports it after E35 and E36. The measure of 9.30 does not count it. It can go in a later change.
- A chart keeps `legendMode()` and `setLegendMode`, because `panels/common.js` calls `setLegendMode` for each panel (E41).

### Table

| Id | Upstream file and place | What the removal takes away | Bytes | Reason | How to apply it again at an update | Lock hash after the edit | Status |
|---|---|---|---|---|---|---|---|
| E17 | `src/static/app_chart_core.js`: the markers block (`markerNode` to `bindMarkers`), the call `placeMarkers()` in `draw`, the API member `setMarkers`, the text of the option `markers` | The exemplar markers: diamonds on the plot under a pool of links, their sample, their tooltip and their click. | 7,432 | Hune never sets the option `markers` and never calls `setMarkers`. Its annotations are a canvas of Hune (9.19d). | Remove the markers block before "lifecycle", the call in `draw` and `setMarkers`. Look for a new upstream call of a marker function. | `13dba7285104e2fdc076013fbea65f881eeaf917c8af3ab2b0153f34b660b9ef` | applied, to re-read by the owner |
| E18 | `src/static/app_chart_core.js`: the section "2-D brush" without `placeBox`, the box element, the press in `pointerdown`, the test in `moveCursor`, `cancelBox` in `release`, `destroyExtras`, the text of the option `brush` | The 2-D brush: a drag that selects a box, its tooltip, its keys and its listeners. | 2,862 | Hune never sets the option `brush`. A drag on a time series zooms. | Remove the same parts. Keep `placeBox`, because the regions use it. | `13dba7285104e2fdc076013fbea65f881eeaf917c8af3ab2b0153f34b660b9ef` | applied, to re-read by the owner |
| E19 | `src/static/app_chart_core.js`: `ANNOTATION_GUTTER`, the gutter in the layout, the layer `chartCore__annotations`, `placeAnnotations` and its call, the text of the option `annotations` | The engine annotations: vertical lines with a label in a strip over the plot. | 1,106 | Hune never sets the option `annotations`. | Remove the same parts. The top of the plot is `PAD_TOP`. | `13dba7285104e2fdc076013fbea65f881eeaf917c8af3ab2b0153f34b660b9ef` | applied, to re-read by the owner |
| E20 | `src/static/app_ui_table.js`: `sortHeadHtml`, `bindSort`, the cell helpers (`barEligible` to `bindCopy`), `keyboardMenuTarget`, `scrollCues` and their imports. The shim `app_ui_copy.js` goes. The shim `app_dom.js` exports `dom` only. | The HTML sort header, the bars, the shares, the copy buttons, the title text, the row heights, the keyboard menu target and the scroll cues. | 11,193 (7,801 in `app_ui_table.js`, 1,124 for `app_ui_copy.js`, 2,268 in `app_dom.js`) | `panels/table.js` reads only `sortHeader`, `setSort` and `rovingRows`. | Remove the same functions, the members of `table` and the imports `copy`, `util`, `h` and `shell`. A new upstream use of the copy shim needs the shim again. | `8c1cc367a3359b81bb25bb1fcfdc3b70188b49717dabbd9489aebf5075cead65` (`app_dom.js`: `60c2d6ba35253f01857d341a783ca14c99d3a2eb6ebbff2284bfc45937a8c009`) | applied, to re-read by the owner |
| E21 | `src/static/app_results_sort.js`: from "numeric sort keys" to `setSortWasm`, the imports `loader` and `wasm` | The sort of the rows (numbers and texts) and the call of its WebAssembly kernel. | 7,035 | Hune imports only `compareNumericForSort` and `compareTextForSort`. Its tables sort with `table_model.js`. | Remove the same part and keep the two compare functions and their helpers. | `9857b5f68bca52997e4ce7773e6b861599137fbc90ec5f1c9aca8d93aeb23b48` | applied, to re-read by the owner |
| E35 | `src/static/app_util.js`: each function except `escapeHtml` and `rafOnce`, their constants, the import of `app_format.js` and the other members of `util` | The text and error helpers, the number and byte formats, the debounce, the scroll focus, the latest request and the layout tests. | 17,761 (18,809 to 1,048 in `app_util.js`) | The page and the editor read only `util.escapeHtml` and `util.rafOnce`. | Keep `escapeHtml` and `rafOnce`, and give `util` only these two members. Look for a new upstream use of another member. | `1a441524ae392ac528db4d5788f2caa2eecd53891ca81218cac238704190b373` | applied, to re-read by the owner |
| E36 | `src/static/app_chart_core.js`: the import of `app_format.js`, the branch of `bucketMs` in `xReadout` | The readout of a time axis of buckets (`bucketMs`, `bucketAlign`, `format.range`). The copy `app_format.js` leaves the page. | 22,265 (230 in `app_chart_core.js`, 22,035 for `app_format.js`) | Hune never sets `bucketMs` or `bucketAlign`. | Remove the import and the branch. | `4dd76bfc57c74a06c977710458ba171a93932a63b780e78811d6a56672134473` | applied, to re-read by the owner |
| E37 | `src/static/app_chart_core.js`: the import of `app_palette.js`, the kernel path of `parseColors`, `rgbaBatch` and its member of `colors`. The copy `app_palette.js` goes. | The WebAssembly colour kernel of the engine. The shims `app_loader.js` and `app_wasm.js` leave the page. | 19,345 (1,275 in `app_chart_core.js`, 17,175 for `app_palette.js`, 895 for the two shims) | v1 has no kernel. `parseColors` always called `parseColor`, and no file calls `rgbaBatch`. | Remove the same parts, the file `app_palette.js` and its entry in the file list of the lock. | `4dd76bfc57c74a06c977710458ba171a93932a63b780e78811d6a56672134473` | applied, to re-read by the owner |
| E38 | The shim `app_ui_state.js` (origin "hune"): `block`, `put`, `empty`, `error`, `loading`, `banner`, `busy` and the four HTML texts | The members of the shim that the engine does not call. | 1,344 (2,325 to 981 in the shim) | The engine calls only `uiState.announce`. | The shim is not a copy, so an update does not change it. A new upstream call of another member needs the member again. | `4b37d9535ef4b0acfc6e7d74d23ea92a1156642297cae867169f0fac770fd484` (the shim) | applied, to re-read by the owner |
| E39 | `src/static/app_ui_chart.js`: `chartCardHtml`, its text in the head comment and the member `cardHtml` | The card of a chart. | 1,640 (5,742 to 4,102 in `app_ui_chart.js`) | `panels/stat.js` calls only `chartUi.sparkline.draw`. | Remove the same parts. | `24a2e263f8b4ec938c10da9a48288f076e2ee9dda87d2850d77a1d1491d7504d` | applied, to re-read by the owner |
| E40 | `src/static/app_chart_core.js`: the section "Aligned y axes", `Y_MAX_INTERVALS`, `alignGroups`, `alignGutter`, the section "Alignment groups" of `create`, `gutterFor`, `xScaleKey`, the branches of `aligned` and `member`, `yKind` and `yInteger` of `scaleOpts`, the export `alignedAxis`, the text of the options | The charts that share an `alignKey`: one gutter, the same number of y intervals and one legend mode. | 11,648 | Hune never sets `alignKey`. Without it, `aligned` was false and `member` was `null`. | Remove the same parts. The first draw is at once, as for a chart with no `alignKey`. | `4dd76bfc57c74a06c977710458ba171a93932a63b780e78811d6a56672134473` | applied, to re-read by the owner |
| E41 | `src/static/app_chart_core.js`: the import of `app_state.js`, the preference of the mode (`LEGEND_STORE_KEY`, `legendPref`, `readLegendMode`, `storeLegendMode`), the legend that filters, the totals legend, the button of the mode and its click, `legendHint`, `onLegendMode`, the member `legendMode` of `chartCore`. `web/js/panels/common.js`: `legendModeButton: false` and the label of the button. | The legend that filters, the totals legend, the button "Values" or "List" and the stored mode. The shim `app_state.js` leaves the page. | 8,559 (5,098 in `app_chart_core.js`, 593 in `panels/common.js`, 2,868 for `app_state.js`) | Hune never sets `legendClick` or `legend: "totals"`, and it left out the button. `panels/common.js` sets the mode of each chart with `setLegendMode`. | Remove the same parts. A chart starts in the mode "list". Keep `setLegendMode` and `legendMode()` of a chart. | `4dd76bfc57c74a06c977710458ba171a93932a63b780e78811d6a56672134473` | applied, to re-read by the owner |
| E42 | `src/static/app_chart_core.js`: `bridgeGaps`, `instances`, `of(el)`, `dateText`, `includedY`, `capTooltipRows`, the branches and the defaults of `xDateOnly`, `tooltipTitle`, `tooltipFooter`, `tooltipNulls`, `onHiddenChange`, `yInclude` and `cursorPoints`, the text of these options | Options and helpers that Hune never sets or calls. | 3,025 | No file gives these options or calls `bridgeGaps` or `chartCore.of`. | Remove the same parts. | `4dd76bfc57c74a06c977710458ba171a93932a63b780e78811d6a56672134473` | applied, to re-read by the owner |

The five removals E17 to E21 take 29,628 bytes from the page measure of 9.30. The eight removals E35 to E42 take 85,587 bytes (1,023,019 to 937,432 bytes from the commit before them). With them, `transform.js` imports `copyCell` from `transform_more.js` and loses its equal copy (117 bytes).
The test `tests/frontend/engine_removals.test.mjs` holds the names, the options and the files of E35 to E42 too. Three of its five tests fail on the tree before these edits.

### The edit E25: size and how to apply it again

- Size: 3,258 bytes in `app_chart_core.js` (163,719 to 166,977) and 152 bytes in `web/js/panels/barchart.js` (7,398 to 7,550), which is 3,410 bytes of the page measure of 9.30. Most of the bytes are the comments of the edit.
- The page passes `xTickRotation: o.x_tick_rotation` for a vertical bar chart only. No other panel type reads the option.
- Marks: seven `// hune: E25` comments in `app_chart_core.js`. `vendor/chdash.patch` holds the whole diff.
- To apply it again after an update of the copy: run `patch -p1 -d web < vendor/chdash.patch`. If a hunk is rejected, do these edits by hand. First add `turnedBox` before `layoutXLabels`, and give `layoutXLabels` the parameter `rot = 0` with the box of each label. Then add `rotation` and `short` before `xTicks`, and use `short` in `xTicks`. Then add the height of the turned labels to the band in `yAxisOf`, pass `rotation()` in `computeLayout`, and draw the turned labels in `drawAxes`.
- Then run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.
- The sign of 2026-11-15: the measure used a bar chart with 12 categories on Grafana 12.4.2. For 45, it found the canvas turn `rotate(+45°)` and the alignment `left`. For -45, it found `rotate(-45°)` and `right`. The edit now turns a positive angle clockwise and puts the start of the text at the tick. The change is 34 bytes (comments), in `turnedBox`, `drawAxes` and the list of the options.

### The edit E26: size and how to apply it again

- Size: 285 bytes in `app_chart_core.js` (with the comments) and 20 bytes in `web/js/panels/histogram.js`.
- The histogram passes `barInset: true`. No other panel type reads the option, so the bar chart keeps its picture.
- Mark: one `// hune: E26` comment in `drawBars`, and the list of the options at the end of the file names `barInset`.
- To apply it again by hand, compute `ins` in `drawBars` before the function `each`. It is half the line width of the series when `opts.barInset` is true, else 0. Then give `ctx.rect` the top `yTop + ins` and the height `yBottom - yTop - 2 * ins`, with the floor of `1 / dpr`.

### The edit E27: size and how to apply it again

- Size: 174 bytes in `app_chart_core.js` (167,296 to 167,470), with the comment.
- No option and no page change: the engine reads `yShown` and `y2.shown`, which the heatmap and the time series already give.
- Marks: one `// hune: E27` comment before the new line in `yAxisOf`, and one at the end of its `return` line. The line of E19 does not change.
- To apply it again by hand, add `const top = leftShown() || rightShown() ? padTop : 0;` after the line of E19 in `yAxisOf`. Then use `top` for the plot height, and return `padTop: top`.

### The edits E28 and E29: size and how to apply them again

- Size of E28: 135 bytes in the shim `app_results_values.js` (214 to 349), with the comment. The page modules `transform_more.js` and `heat_model.js` import `NUMERIC_RE` and lose their own patterns: 5 bytes less.
- Size of E29: 761 bytes in `app_results_sort.js` (4,763 to 5,524) and 245 bytes in `web/js/table_model.js` (6,995 to 7,240), with the comments.
- Marks: one `// hune: E28` comment in the shim. Four `// hune: E29` comments in `app_results_sort.js`: before `decimalParts`, before `compareDecimalText`, before `compareNormalizedForSort` and on the export line.
- To apply E28 again, give `NUMERIC_RE` of the shim the form of the row E28. The shim is not a copy, so an update of the copy does not change it.
- To apply E29 again by hand, replace `DECIMAL_TEXT` with `decimalParts`, which gives the same array as `DECIMAL_TEXT.exec`. Give `compareDecimalText` the last parameters `ma` and `mb`, with `decimalParts` as their default values. Add the member `p` to the value of the kind `d`, and pass `na.p` and `nb.p` to `compareDecimalText`.
- Then move the body of `compareNumericForSort` into `compareNormalizedForSort`, and export it with `normalizeNumericForSort`.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edits E30 and E31: size and how to apply them again

- Size of E30: 384 bytes in `app_chart_core.js`, with the comments. The page modules change too: `panels/series.js` grows by 648 bytes (the soft and hard ends of `autoYRange`), and `panels/timeseries.js` loses 86 bytes (one hook for each scale).
- Size of E31: 340 bytes in `app_chart_core.js`, with the comments. The file goes from 167,470 to 168,194 bytes with both edits.
- Marks: five `// hune: E30` comments. Three are in `yAxisOf`, one is at `customYAxis` and one is in the list of the options. Two `// hune: E31` comments are at the constant and at `fitTitle`.
- To apply E30 again by hand, declare `rMin` and `rMax` next to `yMin` and `yMax` in `yAxisOf`. In the loop over the arrays of a series, take the ends of each array that is not the base of a stack. Give `customYAxis` the last parameter `raw = null`, pass `[rMin, rMax]` (or `null` when `rMin > rMax`), and give `raw` to the hook.
- To apply E31 again by hand, add the constant `TITLE_MAX_CHARS = 256`, and replace the loop of `fitTitle` with the binary search over the first `TITLE_MAX_CHARS` characters. A text that is longer than the constant always ends with the ellipsis.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E32: size and how to apply it again

- Size: 1,849 bytes in `app_chart_core.js` (168,194 to 170,043), with the comments. The page module `panels/timeseries.js` sets the option and loses 48 bytes. Its percent axis is now the automatic rule with the soft ends 0 and 100.
- Marks: six `// hune: E32` comments. Four are in `computeStacks`, one is at the `trace` call of a stacked area in `drawSeries`, and one is in the list of the options.
- To apply it again by hand, compute the side of each visible series in `computeStacks` when `opts.stackSign === "series"`. Add `"\u0002"` to the key of a series of the side below 0.
- Run the loop of the totals also for this option. Add the signed values there, and mark each x where a series of the group has a value (`has`). Read `sum` only in percent.
- In the main loop, take the path of the option first. The base is the top of the group, and a value that is not `NaN` adds to the top. The top is `NaN` when no series of the group has a value.
- Pass `nulls` to `trace` for a stacked area.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E33: size and how to apply it again

- Size: 253 bytes in `app_chart_core.js` (170,043 to 170,296), with the comments.
- Marks: two `// hune: E33` comments. One is at `stackKey` in `computeStacks`, and one is in the list of the options.
- To apply it again by hand, change `stackKey`. When `stackSign` is set, add `"\u0003"` and the `type` of a series to its key.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E34: size and how to apply it again

- Size: 286 bytes in `app_chart_core.js` (170,296 to 170,582), with the comments.
- Marks: three `// hune: E34` texts. The first is at the new constant `insX` in `drawBars`. The second is at the `fill` of a bar series, and the third is in the list of the options.
- To apply it again by hand, add `const insX = opts.barInset === "box" ? ins : 0;` after the inset `ins` of E26. Give the `rect` of a bar the x `x + insX` and the width `Math.max(1 / dpr, drawW - 2 * insX)`. Call `ctx.fill()` only when the fill alpha is above 0.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E35: size and how to apply it again

- Size: 17,761 bytes in `app_util.js` (18,809 to 1,048), with the comments.
- Marks: two `// hune: E35` comments, at the top of the file and before `util`.
- To apply it again by hand, keep `escapeHtml` and `rafOnce`. Remove the import of `app_format.js`, the other functions and their constants. Give `util` the members `rafOnce` and `escapeHtml` only.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E36: size and how to apply it again

- Size: 230 bytes in `app_chart_core.js` (170,582 to 170,352). The copy `app_format.js` (22,035 bytes) leaves the page, so the page measure loses 22,265 bytes.
- Marks: two `// hune: E36` comments, at the imports and before `xReadout`.
- To apply it again by hand, remove the import of `app_format.js` and the branch of `bucketMs` in `xReadout`.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E37: size and how to apply it again

- Size: 1,275 bytes in `app_chart_core.js` (170,352 to 169,077). The copy `app_palette.js` (17,175 bytes) goes. The shims `app_loader.js` and `app_wasm.js` (895 bytes) leave the page, but the editor loads them with `app_highlight.js`.
- Marks: three `// hune: E37` comments, at the imports, at `parseColors` and at the member `colors`.
- To apply it again by hand, remove the import of `app_palette.js` and the function `rgbaBatch`. Make `parseColors` call `parseColor` for each text, and remove `rgbaBatch` from `colors`.
- Remove the file `app_palette.js` and its entry in the file list of the lock. Then run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E38: size and how to apply it again

- Size: 1,344 bytes in the shim `app_ui_state.js` (2,325 to 981), with the comment.
- Mark: one `// hune: E38` comment before the export.
- The shim is not a copy. An update of the copy does not change it, and the patch does not hold it.

### The edit E39: size and how to apply it again

- Size: 1,640 bytes in `app_ui_chart.js` (5,742 to 4,102).
- Mark: one `// hune: E39` comment in the head comment.
- To apply it again by hand, remove `chartCardHtml`, its text in the head comment and the member `cardHtml` of `chartUi`.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E40: size and how to apply it again

- Size: 11,648 bytes in `app_chart_core.js` (169,077 to 157,429), with the comments.
- Marks: eight `// hune: E40` comments. Four are at the head of the file, the old aligned axes, the old group in `create` and the first draw. The others are in `scaleOpts`, `yAxisOf`, `computeLayout` and the list of the options.
- To apply it again by hand, remove the section "Aligned y axes", `Y_MAX_INTERVALS`, `alignGroups` and `alignGutter`. In `create`, remove `alignKey`, `member`, `knownWidth`, `settleGroup`, `xScaleKey`, `gutterFor` and each branch of `aligned` and `member`.
- Compute `left` with `ownGutter`, and call `draw()` at the end of `create`. Remove `yKind` and `yInteger` from `scaleOpts`, and `alignedAxis` from the exports.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E41: size and how to apply it again

- Size: 5,098 bytes in `app_chart_core.js` (157,429 to 152,331) and 593 bytes in `web/js/panels/common.js` (16,865 to 16,272). The shim `app_state.js` (2,868 bytes) leaves the page, so the page measure loses 8,559 bytes.
- Marks: ten `// hune: E41` comments in `app_chart_core.js`. The comment of `createChart` in `panels/common.js` names E41.
- To apply it again by hand, remove the import of `app_state.js`, `LEGEND_STORE_KEY` and the three functions of the preference. Start `legendMode` at "list".
- In the legend, remove the totals legend, the legend that filters, the button of the mode and its click. Give `legendItemHtml` the title of a legend that hides, and make `legendToggles` toggle on Ctrl or Cmd+click.
- Remove `onLegendMode` from `setLegendMode`, and the member `legendMode` of `chartCore`. Then run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### The edit E42: size and how to apply it again

- Size: 3,025 bytes in `app_chart_core.js` (152,331 to 149,306), with the comments.
- Marks: eight `// hune: E42` comments. They are at `dateText`, `bridgeGaps`, `xTicks`, `xReadout`, the points of the cursor, the tooltip, the list of the options and `of`.
- To apply it again by hand, remove `bridgeGaps`, `instances`, `of(el)`, `dateText`, `includedY` and `capTooltipRows`. Remove the branches and the defaults of `xDateOnly`, `tooltipTitle`, `tooltipFooter`, `tooltipNulls`, `onHiddenChange`, `yInclude` and `cursorPoints`.
- Run `tools/check-vendor-lock.sh --chdash-update <checkout>` and `tools/check-vendor-lock.sh --chdash`.

### Procedure at an update of the copy

1. Run `tools/update-chdash.sh <checkout>`, and read the upstream changes.
2. Copy each upstream file of the lock at the new commit over its copy. Write the new commit and the new tag in the lock.
3. Run `patch -p1 -d web < vendor/chdash.patch`. Do the edit of each rejected hunk by hand with the table.
4. Look for new upstream calls of the removed code. A new call changes the removal or cancels it.
5. Run `tools/check-vendor-lock.sh --chdash-update <checkout>`, read the patch, then run `tools/check-vendor-lock.sh --chdash` and the front tests.

## Open

- The status of each row stays `proposal` until the project decides to send it upstream.

## Local copy of the MongoDB driver

Hune uses a patched copy of the crate `mongodb` 3.10.0. The copy reads no environment variable.
The edits are not for the upstream repository. No pull request, no issue and no message goes to the driver project.

### Rules

- The source is the package of `mongodb` 3.10.0 that cargo had downloaded already (checksum below). Nothing else was downloaded. No other crate and no other version is in the copy.
- The copy is `vendor/mongodb/`. The root `Cargo.toml` and `fuzz/Cargo.toml` select it with `[patch.crates-io]`. `Cargo.lock` and `fuzz/Cargo.lock` show the crate with no `source` and no `checksum`.
- Each edit has a comment `// hune:` with the reason. The package has no licence file, so the copy holds `LICENSE` with the standard Apache-2.0 text of the repository. The package has no NOTICE file.
- `vendor/mongodb.lock.json` holds the SHA-256 of each file of the copy and of the original, and the checksum of the original package.
- `vendor/mongodb.patch` is the readable diff of the edits against the original. `tools/check-vendor-lock.sh --mongodb` applies it in reverse to the copy and compares the result with the upstream hashes of the lock.
- After an edit, run `tools/check-vendor-lock.sh --mongodb-update`, read `vendor/mongodb.patch`, and fill the table below.
- The job `policy` has two checks for the copy: the lock and the patch (above), and the search for each environment read (`tests/ci/env-read-allow.txt` is empty).
- `deps.lock.txt` lists the crate as `mongodb 3.10.0 (vendor)`. `deny.toml` is unchanged: the rule `wrappers = ["mongodb"]` names the package.

Checksum of the original package (the `checksum` of `mongodb` 3.10.0 in `Cargo.lock` before the change): `6466d4601ff396952e1895d88083d1ab03930b812277180eb8258bbe1a10bb0f`.

### Edits

| Id | File | What the original reads | What the edit does | Effect in Hune |
|---|---|---|---|---|
| M1 | `src/cmap/establish/handshake.rs` | `AWS_EXECUTION_ENV`, `AWS_LAMBDA_RUNTIME_API`, `AWS_REGION`, `AWS_LAMBDA_FUNCTION_MEMORY_SIZE`, `FUNCTIONS_WORKER_RUNTIME`, `K_SERVICE`, `FUNCTION_NAME`, `FUNCTION_MEMORY_MB`, `FUNCTION_TIMEOUT_SEC`, `FUNCTION_REGION`, `VERCEL`, `VERCEL_REGION`, `KUBERNETES_SERVICE_HOST`, and the file `/.dockerenv` | `RuntimeEnvironment::new()` and `FaasEnvironmentName::new()` return `None`. The helper `var_set` and the import `env` are gone. The enum `FaasEnvironmentName` has `#[allow(dead_code)]`, because no variant is built. | The `hello` has no key `client.env`. `is_faas()` is false, so the monitoring mode `auto` selects the stream protocol. |
| M2 | `src/otel.rs` | `OTEL_RUST_INSTRUMENTATION_MONGODB_ENABLED`, `OTEL_RUST_INSTRUMENTATION_MONGODB_QUERY_TEXT_MAX_LENGTH` | The default is "off" and 0. | None. The feature `opentelemetry` is off, so the file is not compiled. |
| M3 | `src/client/auth/oidc.rs` | `AZURE_FEDERATED_TOKEN_FILE`, `AWS_WEB_IDENTITY_TOKEN_FILE` | The path of the token is the Kubernetes default. | None. Hune uses SCRAM-SHA-256 only. |
| M4 | `src/client/csfle/state_machine.rs` | `GCE_METADATA_HOST` | The host is `metadata.google.internal`. | None. The feature `in-use-encryption` is off. |

The files `src/test.rs` and `src/client/options/test.rs` of the copy read variables in test code. Cargo does not compile them for Hune, and the policy check skips them.

### Test

The test file `crates/hune-source/tests/mongosource_hello.rs` starts the driver with the platform variables set and records the `hello` on the server side. It fails with the original `handshake.rs`: the message then holds `client.env`.

### Open

- A new release of the driver is a new change. The edits M1 to M4 must be made again, and the lock and the patch must be made again from the new package.
- The audit of the other crates of the tree is in 13.23. Each read that stays is a decision of the project.

