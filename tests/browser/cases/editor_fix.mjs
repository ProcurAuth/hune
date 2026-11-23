// Browser cases B56 and B57 (16.10): fixes of the query editor.
// B56 a valid macro is never an error in the editor (the SQL area, the error box, the panel JSON tab and the dashboard JSON
//     tab), a wrong macro gives the message of the server, and the text areas switch the spell check off.
// B57 the mouse selection of the SQL text area and the coloured layer under it line up at the scale factors 1, 0.9, 1.1 and
//     1.25, in the light and in the dark theme (10.3).
import { chmodSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { inflateSync } from "node:zlib";
import { doc, panel, section } from "../lib/docs.mjs";
import { WORK, api } from "../lib/env.mjs";
import { enterEdit, makeSeed, openPanelEditor, moreItem } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const SQL_INPUT = '[data-testid="sql-input"]';

// ---------------------------------------------------------------------------------------------------------------------
// B56: macros

// One query for each valid macro of 7.10, in the places that the spec names: WHERE, SELECT, blanks, line breaks, a string.
const CLICKHOUSE_VALID = [
  ["WHERE", "SELECT count() AS n FROM analytics.seed WHERE $__timeFilter(event_time)"],
  ["WHERE with blanks", "SELECT count() AS n FROM analytics.seed WHERE $__timeFilter(   event_time   )"],
  ["WHERE with line breaks", "SELECT count() AS n\nFROM analytics.seed\nWHERE\n  $__timeFilter(\n    event_time\n  )"],
  ["timeFilter_ms", "SELECT count() AS n FROM analytics.seed WHERE $__timeFilter_ms(event_time)"],
  ["dateFilter", "SELECT count() AS n FROM analytics.seed WHERE $__dateFilter(toDate(event_time))"],
  ["dateTimeFilter", "SELECT count() AS n FROM analytics.seed WHERE $__dateTimeFilter(toDate(event_time), event_time)"],
  ["fromTime and toTime", "SELECT count() AS n FROM analytics.seed WHERE event_time >= $__fromTime AND event_time <= $__toTime"],
  ["fromTime_ms and toTime_ms", "SELECT $__fromTime_ms AS a, $__toTime_ms AS b"],
  ["from, to, interval_s, interval_ms in SELECT", "SELECT $__from AS a, $__to AS b, $__interval_s AS c, $__interval_ms AS d"],
  ["timeInterval", "SELECT $__timeInterval(event_time) AS t, count() AS n FROM analytics.seed GROUP BY t ORDER BY t"],
  ["timeInterval_ms", "SELECT $__timeInterval_ms(event_time) AS t, count() AS n FROM analytics.seed GROUP BY t ORDER BY t"],
  ["conditionalAll", "SELECT count() AS n FROM analytics.seed WHERE $__conditionalAll(node IN (${node}), ${node})"],
  ["in a string, not a macro", "SELECT '$__timeFilter(event_time) $__TIMEFILTER(x) $__foo' AS literal"],
  ["in a comment, not a macro", "SELECT 1 AS one -- $__timeFilter(event_time) $__bad\n/* $__nope */"],
  ["a JSON text of the MongoDB kind", '{"find": "events", "filter": {"ts": {"$gte": "$__timeFrom"}}}'],
];
// A wrong case, an unknown name, a built-in variable with an argument, an unknown macro: the server refuses each one (7.10).
const CLICKHOUSE_WRONG = [
  ["wrong case", "SELECT count() AS n FROM analytics.seed WHERE $__TIMEFILTER(event_time)", "$__TIMEFILTER is not a macro of Hune"],
  ["name with a suffix", "SELECT count() AS n FROM analytics.seed WHERE $__timeFilterX(event_time)", "$__timeFilterX is not a macro of Hune"],
  ["unknown name", "SELECT $__foo AS x", "$__foo is not a macro of Hune"],
  ["$__range", "SELECT $__range AS x", "$__range is not a macro of Hune"],
  ["$__interval in code", "SELECT $__interval AS x", "$__interval is not a macro of Hune"],
];

const GOOD_ALL = [
  "SELECT $__timeInterval(event_time) AS t, count() AS n",
  "FROM analytics.seed",
  "WHERE $__timeFilter(event_time) AND $__timeFilter_ms(event_time) AND $__dateFilter(toDate(event_time))",
  "  AND $__dateTimeFilter(toDate(event_time), event_time)",
  "  AND event_time >= $__fromTime AND event_time <= $__toTime",
  "  AND event_time >= $__fromTime_ms AND event_time <= $__toTime_ms",
  "  AND $__from > 0 AND $__to > 0 AND $__interval_s > 0 AND $__interval_ms > 0",
  "GROUP BY t",
];

// The valid macros of 7.22, for the PostgreSQL source. The source of the case is down: the checks of the document need no connection.
const PG_VALID = [
  "SELECT $__time(created_at), $__timeEpoch(created_at) FROM metrics.http_requests",
  "SELECT count(*) FROM metrics.http_requests WHERE $__timeFilter(created_at)",
  "SELECT count(*) FROM metrics.http_requests WHERE $__timeFilter(\n  created_at\n)",
  "SELECT count(*) FROM metrics.http_requests WHERE created_at >= $__timeFrom AND created_at <= $__timeTo",
  "SELECT count(*) FROM metrics.http_requests WHERE created_at >= $__timeFrom() AND created_at <= $__timeTo()",
  "SELECT $__timeGroup(created_at, '5m') AS t, count(*) FROM metrics.http_requests GROUP BY 1",
  "SELECT $__timeGroupAlias(created_at, $__interval), count(*) FROM metrics.http_requests GROUP BY 1",
  "SELECT count(*) FROM metrics.http_requests WHERE $__unixEpochFilter(epoch_s) AND $__unixEpochNanoFilter(epoch_ns)",
  "SELECT $__unixEpochFrom, $__unixEpochTo, $__unixEpochNanoFrom, $__unixEpochNanoTo FROM metrics.http_requests",
  "SELECT $__unixEpochGroup(epoch_s, '1h'), $__unixEpochGroupAlias(epoch_s, '1h') FROM metrics.http_requests",
  "SELECT $__interval_s, $__interval_ms, $__from, $__to FROM metrics.http_requests",
  "SELECT 1 FROM metrics.http_requests WHERE note = '$__timeFilter(created_at)'",
];
// The macros of ClickHouse that the dialect `postgres` does not list are errors there (7.22).
const PG_WRONG = [
  ["$__timeFilter_ms", "SELECT count(*) FROM metrics.http_requests WHERE $__timeFilter_ms(created_at)", "$__timeFilter_ms is not a macro of Hune"],
  ["$__fromTime", "SELECT count(*) FROM metrics.http_requests WHERE created_at >= $__fromTime", "$__fromTime is not a macro of Hune"],
  ["wrong case", "SELECT count(*) FROM metrics.http_requests WHERE $__TIMEFILTER(created_at)", "$__TIMEFILTER is not a macro of Hune"],
];

const lines = (text) => text.split("\n");

/** The state of the SQL area and of the error box of the panel editor. */
async function editorState(p) {
  return p.eval(`(() => {
    const box = document.querySelector('[data-testid="panel-errors"]');
    return {
      errorsHidden: box.hidden,
      errorsText: box.hidden ? "" : box.textContent,
      gutterMarks: document.querySelectorAll(".sql__no--error").length,
      tokErr: document.querySelectorAll(".sql__hl .tok-err").length,
      underline: [...document.querySelectorAll(".sql__hl, .sql__hl *")].filter((e) => /underline/.test(getComputedStyle(e).textDecorationLine)).length,
    };
  })()`);
}

export async function setSql(p, text) {
  await p.fill(p.tid("sql-input"), text);
  await sleep(450); // 300 ms debounce
}

async function runQuery(p) {
  await p.click(p.tid("run-button"));
  await sleep(1100);
}

/** The panel JSON tab shows the checks of the draft: its list of errors. */
async function panelJsonErrors(p) {
  await p.click(p.tid("tab-json"));
  await sleep(1700); // 1 s debounce of the check, and the request
  const r = await p.eval(`(() => { const e = document.querySelector('[data-testid="panel-json-errors"]'); return e.hidden ? "" : e.textContent; })()`);
  await p.click(p.tid("tab-query"));
  return r;
}

async function dashboardJsonStatus(p) {
  await p.click(p.tid("editor-back"));
  await p.waitGone(p.tid("panel-editor"));
  await moreItem(p, "json-tab");
  await p.waitSel(p.tid("json-editor"));
  await p.waitFor(`/valid|error/.test(document.querySelector(".ebar__status").textContent)`, { what: "JSON check", timeout: 10000 });
  const r = await p.eval(`({ status: document.querySelector(".ebar__status").textContent, errors: document.querySelector('[data-testid="json-errors"]').hidden ? "" : document.querySelector('[data-testid="json-errors"]').textContent })`);
  await p.click(p.tid("editor-back"));
  await p.waitGone(p.tid("json-editor"));
  return r;
}

function pgBlock(passwordFile) {
  return {
    block: [
      'postgres "pgdown" {',
      '  label              = "PostgreSQL down"',
      '  host               = "127.0.0.1"',
      "  port               = 1",
      '  database           = "analytics"',
      '  user               = "hune_ro"',
      `  password_file      = ${JSON.stringify(passwordFile)}`,
      '  search_path        = "metrics"',
      '  sslmode            = "disable"',
      '  connect_timeout    = "1s"',
      '  max_execution_time = "1m"',
      "  max_result_rows    = 100000",
      "  max_result_bytes   = 33554432",
      "  max_query_size     = 1048576",
      "  max_concurrent     = 16",
      '  queue_wait         = "5s"',
      "}",
    ].join("\n"),
  };
}

// ---------------------------------------------------------------------------------------------------------------------
// B57: selection alignment

// Metrics that the two layers must share. The test reads the computed value of each one on both elements.
const METRICS = [
  "fontFamily", "fontSize", "fontWeight", "fontStyle", "fontStretch", "lineHeight", "letterSpacing", "wordSpacing", "textIndent",
  "textTransform", "textAlign", "direction", "unicodeBidi", "textRendering", "fontKerning", "fontVariantLigatures", "fontFeatureSettings",
  "fontVariationSettings", "fontSynthesis", "tabSize", "whiteSpace", "wordBreak", "overflowWrap", "hyphens", "boxSizing",
  "paddingTop", "paddingLeft", "borderTopWidth", "borderLeftWidth", "borderRightWidth", "borderBottomWidth",
];

const LONG = "SELECT host, avg(cpu) AS cpu_average_over_the_whole_range, max(cpu) AS cpu_max_value, sum(requests) AS requests_total_count FROM analytics.demo_metrics WHERE x = 1 AND y = 2 AND z = 3 AND w = 4";
export const TEXT = ["SELECT 1", LONG, "\tWHERE host = 'é à µ ü ñ ç' AND note = 'x'\t-- tab inside", "  FROM t WHERE a = $__timeFilter(ts) AND b IN (${host})", "LAST", "", "l7", "l8", "l9", "l10", "l11", "l12"].join("\n");

// Selections to measure: [line from 1, column from 0, length]. The long line (2) holds the far columns, where a drift adds up.
export const SELECTIONS = [[1, 0, 4], [1, 5, 3], [2, 0, 4], [2, 40, 3], [2, 90, 3], [2, 150, 6], [2, 175, 8], [3, 0, 3], [3, 10, 8], [3, 14, 10], [4, 20, 12], [4, 30, 5]];
export const SEL = { list: SELECTIONS };

function decodePng(buf) {
  let off = 8;
  let width = 0;
  let height = 0;
  let colourType = 0;
  const idat = [];
  while (off < buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.toString("ascii", off + 4, off + 8);
    const body = buf.subarray(off + 8, off + 8 + len);
    if (type === "IHDR") {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      colourType = body[9];
    } else if (type === "IDAT") idat.push(body);
    off += 12 + len;
  }
  const channels = colourType === 6 ? 4 : 3;
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const data = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const src = y * (stride + 1) + 1;
    const dst = y * stride;
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? data[dst + x - channels] : 0;
      const b = y ? data[dst - stride + x] : 0;
      const c = x >= channels && y ? data[dst - stride + x - channels] : 0;
      let v = raw[src + x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const pa = Math.abs(b - c);
        const pb = Math.abs(a - c);
        const pc = Math.abs(a + b - 2 * c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      data[dst + x] = v & 255;
    }
  }
  return { width, height, channels, data };
}

async function shot(p, clip) {
  const r = await p.send("Page.captureScreenshot", { format: "png", clip: { ...clip, scale: 1 } });
  return decodePng(Buffer.from(r.data, "base64"));
}

const frames = `new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 60))))`;

async function scrollTo(p, left, top) {
  await p.eval(`(async () => { const t = document.querySelector('${SQL_INPUT}'); t.scrollLeft = ${left}; t.scrollTop = ${top}; t.dispatchEvent(new Event("scroll")); await ${frames}; })()`);
}

const hasMagenta = (img) => {
  for (let i = 0; i < img.data.length; i += img.channels) if (img.data[i] > 240 && img.data[i + 1] < 20 && img.data[i + 2] > 240) return true;
  return false;
};

/** Per band of one line height: the centre of the ink in x, and the ink. Used to compare the two layers. */
function bandInk(img, band) {
  let sum = 0;
  let sumX = 0;
  for (let y = band.top; y < band.top + band.height && y < img.height; y++) {
    for (let x = 0; x < img.width; x++) {
      const d = 255 - img.data[(y * img.width + x) * img.channels + 1];
      sum += d;
      sumX += d * (x + 0.5);
    }
  }
  return { sum, x: sum ? sumX / sum : 0 };
}

/** Ink test: the glyphs of the coloured layer against the glyphs of the text area, both drawn in black on white. */
export async function inkGaps(p, dsf) {
  await p.eval(`(() => {
    const s = document.createElement("style");
    s.id = "b57-ink";
    s.textContent = ".sql__body{background:#fff !important} .sql__hl,.sql__hl *{color:#000 !important;text-decoration:none !important;background:none !important;font-weight:400 !important} .sql__input{color:#000 !important;caret-color:transparent !important;background:#fff !important}";
    document.head.append(s);
    document.querySelector('${SQL_INPUT}').blur();
  })()`);
  const worst = [];
  for (const [left, top] of [[0, 0], [400, 0], [250, 37], [1e6, 1e6]]) {
    await scrollTo(p, left, top);
    const r = await p.eval(`(() => { const b = document.querySelector(".sql__body").getBoundingClientRect(); const t = document.querySelector('${SQL_INPUT}'), h = document.querySelector(".sql__hl"); const hb = h.getBoundingClientRect(); const hs = getComputedStyle(h); return { x: b.left, y: b.top, w: t.clientWidth, h: t.clientHeight, ta: [t.scrollLeft, t.scrollTop], pre: [h.scrollLeft, h.scrollTop], lineTop: hb.top + parseFloat(hs.borderTopWidth) + parseFloat(hs.paddingTop) - h.scrollTop, lh: parseFloat(hs.lineHeight) }; })()`);
    // The two layers must scroll to the same place, also at the end of the text.
    if (r.ta[0] !== r.pre[0] || r.ta[1] !== r.pre[1]) worst.push(`scroll ${left},${top}: text area ${r.ta}, coloured layer ${r.pre}`);
    const clip = { x: r.x + 3, y: r.y + 3, width: r.w - 8, height: r.h - 14 };
    await p.eval(`document.querySelector('${SQL_INPUT}').style.visibility = "hidden"`);
    const a = await shot(p, clip);
    await p.eval(`document.querySelector('${SQL_INPUT}').style.visibility = "visible"; document.querySelector(".sql__hl").style.visibility = "hidden"`);
    const b = await shot(p, clip);
    await p.eval(`document.querySelector(".sql__hl").style.visibility = "visible"`);
    // The bands are the line boxes of the text (they start at the first line, whatever the vertical place of the area is): a glyph then stays in
    // its band at every scale factor. Bands that start elsewhere cut a line in two, and the gap of the ink depends on where the cut falls.
    let maxBand = 0;
    const bands = [];
    const first = (r.lineTop - clip.y) * dsf;
    for (let k = -Math.ceil(first / (r.lh * dsf)); ; k++) {
      const top2 = Math.round(first + k * r.lh * dsf);
      const end = Math.round(first + (k + 1) * r.lh * dsf);
      if (end > a.height) break;
      if (top2 < 0) continue;
      const ia = bandInk(a, { top: top2, height: end - top2 });
      const ib = bandInk(b, { top: top2, height: end - top2 });
      bands.push({ ia, ib });
      maxBand = Math.max(maxBand, ia.sum);
    }
    // Only the bands with a good share of the ink. Each glyph is drawn on a whole device pixel, so the centre of one line can
    // differ by less than one device pixel from the other layer. The whole area (below) must stay under 0.5 px.
    for (const { ia, ib } of bands) {
      if (ia.sum < maxBand * 0.25) continue;
      const gap = Math.abs(ia.x - ib.x) / dsf;
      if (gap >= 1 / dsf) worst.push(`ink of one line at scroll ${left},${top}: gap ${gap.toFixed(2)} px`);
    }
    const all = (img) => bandInk(img, { top: 0, height: img.height });
    const gapAll = Math.abs(all(a).x - all(b).x) / dsf;
    if (gapAll >= 0.5) worst.push(`ink of the whole area at scroll ${left},${top}: gap ${gapAll.toFixed(2)} px`);
    const inkRatio = Math.abs(all(a).sum - all(b).sum) / Math.max(1, all(a).sum);
    if (inkRatio > 0.01) worst.push(`ink amount differs by ${(inkRatio * 100).toFixed(1)} % at scroll ${left},${top}`);
  }
  await p.eval(`document.getElementById("b57-ink").remove()`);
  return worst;
}

/** Selection test: the edges of a selection of the text area against the span of the same characters in the coloured layer. */
export async function selectionGaps(p, dsf, list = SELECTIONS, verbose = false) {
  await p.eval(`(() => {
    const s = document.createElement("style");
    s.id = "b57-sel";
    s.textContent = ".sql__input::selection{background:rgb(255,0,255) !important;color:transparent !important}";
    document.head.append(s);
  })()`);
  const found = [];
  // Chrome snaps the painted selection to whole device pixels, on each edge: the two edges can differ by two device pixels.
  const snap = 2 / dsf;
  for (const [line, col, len] of list) {
    // Bring the span to the middle of the area, then select it.
    const info = await p.eval(`(async () => {
      const t = document.querySelector('${SQL_INPUT}'), pre = document.querySelector(".sql__hl");
      const lines = t.value.split("\\n");
      let idx = 0;
      for (let i = 0; i < ${line} - 1; i++) idx += lines[i].length + 1;
      const a = idx + ${col};
      const b = a + ${len};
      const spanOf = (from, to) => {
        const w = document.createTreeWalker(pre, NodeFilter.SHOW_TEXT);
        const at = (k) => { w.currentNode = pre; let c = 0; let node; while ((node = w.nextNode())) { if (c + node.length >= k) return [node, k - c]; c += node.length; } return null; };
        const [n1, o1] = at(from);
        const [n2, o2] = at(to);
        const r = document.createRange();
        r.setStart(n1, o1);
        r.setEnd(n2, o2);
        return r.getBoundingClientRect();
      };
      // The focus scrolls the caret into view, so the selection comes first and the scroll position after it.
      t.focus(); t.setSelectionRange(a, b);
      t.scrollLeft = 0; t.scrollTop = 0; t.dispatchEvent(new Event("scroll")); await ${frames};
      const base = document.querySelector(".sql__body").getBoundingClientRect().left;
      t.scrollLeft = Math.max(0, spanOf(a, b).left - base - 250); t.dispatchEvent(new Event("scroll")); await ${frames};
      const rr = spanOf(a, b);
      const pb = pre.getBoundingClientRect(), cs = getComputedStyle(pre), body = document.querySelector(".sql__body").getBoundingClientRect();
      const lh = parseFloat(cs.lineHeight);
      return { left: rr.left, right: rr.right, top: pb.top + parseFloat(cs.borderTopWidth) + parseFloat(cs.paddingTop) + (${line} - 1) * lh - pre.scrollTop, lh, visible: rr.left > body.left + 4 && rr.right < body.right - 20 };
    })()`);
    if (!info.visible) {
      found.push(`line ${line} column ${col}: the span is not in view`);
      continue;
    }
    const clip = { x: Math.max(0, info.left - 20), y: info.top - 2, width: info.right - info.left + 40, height: info.lh + 4 };
    let img = await shot(p, clip);
    // The selection shows when the text area has the focus: ask again if the first capture came too early.
    for (let again = 0; again < 3 && !hasMagenta(img); again++) {
      await sleep(150);
      img = await shot(p, clip);
    }
    const bg = img.data[1];
    const cover = (x, y) => {
      const o = (y * img.width + x) * img.channels;
      const r = img.data[o];
      const g = img.data[o + 1];
      const b = img.data[o + 2];
      return r > g + 40 && b > g + 40 ? Math.min(1, (bg - g) / bg) : 0;
    };
    const ymid = Math.round(img.height / 2);
    let first = -1;
    let last = -1;
    for (let x = 0; x < img.width; x++) if (cover(x, ymid) > 0.02) {
      if (first < 0) first = x;
      last = x;
    }
    if (first < 0) {
      found.push(`line ${line} column ${col}: no selection in the capture`);
      continue;
    }
    const toCss = (v) => (clip.x * dsf + v) / dsf;
    const left = toCss(first + (1 - cover(first, ymid)));
    const right = toCss(last + cover(last, ymid));
    const column = Math.round(img.width / 2);
    let top = -1;
    let bottom = -1;
    for (let y = 0; y < img.height; y++) if (cover(column, y) > 0.02) {
      if (top < 0) top = y;
      bottom = y;
    }
    const topCss = (clip.y * dsf + top) / dsf;
    const bottomCss = (clip.y * dsf + bottom + 1) / dsf;
    const gaps = { left: Math.abs(left - info.left), right: Math.abs(right - info.right), top: Math.abs(topCss - info.top), bottom: Math.abs(bottomCss - (info.top + info.lh)) };
    if (verbose) console.log(line, col, len, JSON.stringify(Object.fromEntries(Object.entries(gaps).map(([k, v]) => [k, +v.toFixed(2)]))));
    for (const [edge, gap] of Object.entries(gaps)) {
      // The gap beyond the snap of the device pixels must stay below 0.5 px. A drift of the glyphs adds up along the line and fails here.
      const beyond = Math.max(0, gap - snap);
      if (beyond >= 0.5) found.push(`line ${line} column ${col} length ${len}: ${edge} edge gap ${gap.toFixed(2)} px`);
    }
  }
  await p.eval(`document.getElementById("b57-sel").remove()`);
  return found;
}

export const cases = [
  {
    id: "B56",
    name: "editor: no false error for a valid macro (SQL area, error box, panel JSON tab, dashboard JSON tab), the server message for a wrong one, spell check off",
    async run(t) {
      await makeSeed(t, "ed56");
      const p = await t.open(t.dash("ed56"));
      t.allow(p, /Failed to load resource/);
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      await openPanelEditor(p, 2);

      // The text areas switch the native spell check off: the red wavy line of the spell checker under a word such as
      // timeFilter looked like an error. The attribute is the text "false" (a boolean false is skipped by el()).
      const spell = await p.eval(`(() => { const t = document.querySelector('${SQL_INPUT}'); return { property: t.spellcheck, attribute: t.getAttribute("spellcheck"), autocorrect: t.getAttribute("autocorrect") }; })()`);
      t.eq([spell.property, spell.attribute], [false, "false"], "spellcheck of the SQL area");

      // Each valid macro: the SQL area marks nothing, the error box stays hidden, and the query runs.
      for (const [name, sql] of CLICKHOUSE_VALID) {
        await setSql(p, sql);
        const before = await editorState(p);
        t.eq([before.tokErr, before.underline, before.gutterMarks], [0, 0, 0], `${name}: marks before Run`);
        const json = !sql.startsWith("{");
        if (json) {
          await runQuery(p);
          const s = await editorState(p);
          t.assert(s.errorsHidden, `${name}: no error after Run, but got: ${s.errorsText}`);
          t.eq([s.tokErr, s.underline, s.gutterMarks], [0, 0, 0], `${name}: marks after Run`);
        }
      }
      t.allow(p, /\b(422|400)\b/);

      // The same texts in the checks of the draft: the panel JSON tab and the dashboard JSON tab.
      await setSql(p, GOOD_ALL.join("\n"));
      t.eq(await panelJsonErrors(p), "", "panel JSON tab: valid macros give no error");
      let dash = await dashboardJsonStatus(p);
      t.eq([dash.status, dash.errors], ["The document is valid.", ""], "dashboard JSON tab: valid macros");
      await openPanelEditor(p, 2);
      await runQuery(p);
      const all = await editorState(p);
      t.assert(all.errorsHidden, "all valid macros together run without error: " + all.errorsText);

      // A wrong macro: the message of the server, with its line and column, and one mark in the gutter.
      for (const [name, sql, message] of CLICKHOUSE_WRONG) {
        await setSql(p, sql);
        await runQuery(p);
        const s = await editorState(p);
        t.assert(!s.errorsHidden && s.errorsText.includes(message), `${name}: the error box says "${message}", it says: ${s.errorsText}`);
        t.assert(/line 1, column \d+/.test(s.errorsText), `${name}: the error box names the place: ${s.errorsText}`);
        t.eq(s.gutterMarks, 1, `${name}: one mark in the gutter`);
      }
      await setSql(p, "SELECT $__TIMEFILTER(event_time) AS x");
      const json = await panelJsonErrors(p);
      t.assert(json.includes("$__TIMEFILTER is not a macro of Hune"), "panel JSON tab: the wrong macro is an error: " + json);
      dash = await dashboardJsonStatus(p);
      t.assert(/1 error/.test(dash.status) && dash.errors.includes("is not a macro of Hune"), "dashboard JSON tab: the wrong macro is an error: " + JSON.stringify(dash));
      // The JSON text areas have the spell check off too.
      await moreItem(p, "json-tab");
      await p.waitSel(p.tid("json-text"));
      t.eq(await p.eval(`document.querySelector('[data-testid="json-text"]').getAttribute("spellcheck")`), "false", "spellcheck of the dashboard JSON");
      await p.click(p.tid("editor-back"));
      await p.waitGone(p.tid("json-editor"));
      await openPanelEditor(p, 2);
      await p.click(p.tid("tab-json"));
      t.eq(await p.eval(`document.querySelector('[data-testid="panel-json"]').getAttribute("spellcheck")`), "false", "spellcheck of the panel JSON");

      // PostgreSQL: the macros of 7.22 in a source of the kind postgres. The source is down: only the checks of the document run.
      const passwordFile = join(WORK, "b56-pg.password");
      writeFileSync(passwordFile, "unused-password\n", { mode: 0o600 });
      chmodSync(passwordFile, 0o600);
      const server = await t.ownServer("pg", { postgres: pgBlock(passwordFile) });
      const pgDoc = (sqlLines, uid) => doc(uid, "Case " + uid, [section(null, [panel(1, "table", "pg", sqlLines, { source: "pgdown", grid: { x: 0, y: 0, w: 12, h: 8 } })])]);
      const own = api(server);
      const put = (uid, sqlLines) => own.create("transit", pgDoc(sqlLines, uid));
      for (const [i, sql] of PG_VALID.entries()) {
        const r = await put(`pgv${i}`, lines(sql));
        t.assert(r.status === 201, `PostgreSQL macro text ${i} is valid: ${r.status} ${r.text.slice(0, 200)} for ${JSON.stringify(sql)}`);
      }
      for (const [name, sql, message] of PG_WRONG) {
        const r = await own.call("POST", "api/dashboards?validate_only=true", { body: { folder: "transit", document: pgDoc(lines(sql), "pgw") } });
        t.assert(r.status === 422 && r.text.includes(message), `PostgreSQL ${name}: ${r.status} ${r.text.slice(0, 200)}`);
      }
      const q = await t.page();
      t.allow(q, /Failed to load resource/);
      await q.goto(server.url + "d/pgv1");
      await q.waitSel('[data-testid="panel-1"]');
      await enterEdit(q);
      await openPanelEditor(q, 1);
      for (const sql of PG_VALID) {
        await setSql(q, sql);
        const s = await editorState(q);
        t.assert(s.errorsHidden, `PostgreSQL SQL area: no error mark for ${JSON.stringify(sql)}: ${s.errorsText}`);
        t.eq([s.tokErr, s.underline, s.gutterMarks], [0, 0, 0], `PostgreSQL marks for ${JSON.stringify(sql)}`);
      }
      await setSql(q, PG_VALID.join(";\n").replace(/;\n/g, " UNION ALL\n"));
      t.eq(await panelJsonErrors(q), "", "PostgreSQL panel JSON tab: valid macros give no error");
      await setSql(q, PG_WRONG[0][1]);
      t.assert((await panelJsonErrors(q)).includes(PG_WRONG[0][2]), "PostgreSQL panel JSON tab: a macro of ClickHouse is an error");
    },
  },
  {
    id: "B57",
    name: "editor: the selection of the SQL text area lines up with the coloured layer at the scale factors 1, 0.9, 1.1 and 1.25, in light and dark",
    async run(t) {
      await makeSeed(t, "ed57");
      const problems = [];
      for (const theme of ["light", "dark"]) {
        const p = await t.page({ width: 1400, height: 1000 });
        await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
        await p.goto(t.dash("ed57"));
        await p.idle();
        await waitPanelsReady(p, [1, 2, 3]);
        await enterEdit(p);
        await openPanelEditor(p, 2);
        await setSql(p, TEXT);
        for (const dsf of [1, 0.9, 1.1, 1.25]) {
          await p.send("Emulation.setDeviceMetricsOverride", { width: 1400, height: 1000, deviceScaleFactor: dsf, mobile: false });
          await sleep(300);
          const tag = `${theme} @${dsf}`;
          // The two layers share the metrics of the text and the same box.
          const same = await p.eval(`(() => {
            const a = getComputedStyle(document.querySelector(".sql__hl")), b = getComputedStyle(document.querySelector('${SQL_INPUT}'));
            const diff = [];
            for (const k of ${JSON.stringify(METRICS)}) if (a[k] !== b[k]) diff.push(k + ": " + a[k] + " / " + b[k]);
            const ra = document.querySelector(".sql__hl").getBoundingClientRect(), rb = document.querySelector('${SQL_INPUT}').getBoundingClientRect();
            if (ra.left !== rb.left || ra.top !== rb.top || ra.width !== rb.width || ra.height !== rb.height) diff.push("box " + [ra.left, ra.top, ra.width, ra.height] + " / " + [rb.left, rb.top, rb.width, rb.height]);
            return diff;
          })()`);
          for (const d of same) problems.push(`${tag}: ${d}`);
          for (const d of await inkGaps(p, dsf)) problems.push(`${tag}: ${d}`);
          for (const d of await selectionGaps(p, dsf)) problems.push(`${tag}: ${d}`);
        }
      }
      t.eq(problems, [], "gaps between the selection and the coloured text");
    },
  },
];
