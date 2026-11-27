// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B58 and B59 (9.6, 9.8, 9.9, 9.25).
// B58: a reload (F5) of a dashboard page shows one stable render. The first frame that the viewer sees is already complete (the header
//      with its controls, the icons, the font, the theme and every panel frame in its final place). Nothing moves or disappears after it.
// B59: the header sticks to the top of the window when the page scrolls. The height and the size of each button are fixed in every state
//      (range text, refresh interval, refresh running, Save enabled or not, view, edit mode, each view of the editor, rights). No button of
//      the view moves when the edit mode starts or ends: the group of the edit mode (Add, Save, Revert) is at the left of the time
//      control and takes its room from the selector, which gets narrower by the width of the group and one gap. The brand, the time control,
//      the switch, the theme button and the button More actions do not move (B150 measures the fixed four at five widths). The switch
//      makes exactly two layouts, with no frame between them. The page has no Share control.
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { SQL, doc, panel, section } from "../lib/docs.mjs";
import { SEED_FROM, SEED_TO } from "../lib/env.mjs";
import { moreItem } from "../lib/ed.mjs";
import { panelStates, sleep, waitPanelsReady } from "../lib/h.mjs";

const UID58 = "flash58";
const UID59 = "flash59";
const IDS = [1, 2, 3, 4, 5, 6, 10, 11, 12, 20, 21];

// Six stat panels, a time series, a table and a bar chart in the first section and four more panels in a second one, with a variable
// bar. The page is taller than a window of 900 px, so it has a scroll bar (the case that moved the page by the width of the bar).
function flashDoc(uid) {
  const g = (x, y, w, h) => ({ x, y, w, h });
  const stats = [1, 2, 3, 4, 5, 6].map((id) => panel(id, "stat", `Stat ${id}`, `SELECT ${id * 7} AS value`, { grid: g((id - 1) * 4, 0, 4, 4), description: id === 1 ? "The first stat." : "" }));
  return doc(uid, `Flash ${uid.slice(5)}`, [
    section("Overview", [
      ...stats,
      panel(10, "timeseries", "Series", SQL.series, { grid: g(0, 4, 12, 8) }),
      panel(11, "table", "Table", SQL.table, { grid: g(12, 4, 12, 8) }),
      panel(12, "barchart", "Bars", SQL.bars, { grid: g(0, 12, 12, 8) }),
    ]),
    section("Details", [
      panel(20, "piechart", "Pie", SQL.bars, { grid: g(12, 12, 6, 8) }),
      panel(21, "bargauge", "Gauge", SQL.bars, { grid: g(18, 12, 6, 8) }),
    ]),
  ], {
    // The default range is the range that `t.dash` writes in the address: the button Save is active when the range differs from the default (10.9).
    time: { from: String(SEED_FROM), to: String(SEED_TO), timezone: "browser", refresh: null },
    variables: [{ kind: "query", name: "node", label: "Node", hide: "none", source: "prod", sql: ["SELECT DISTINCT node", "FROM analytics.seed", "ORDER BY node"], multi: true, include_all: true, all_value: null, refresh: "time_change", sort: "alpha_asc", regex: null, default: ["$__all"] }],
  });
}

// The probe runs in the page from the first moment of the document (Page.addScriptToEvaluateOnNewDocument). At each animation frame
// (one frame is one possible paint) it logs what the viewer would see. `hold` is true while the shell holds its first paint.
const PROBE = `(() => {
  const log = (window.__flash = { id: Math.random(), t0: performance.now(), frames: [] });
  const box = (e) => { const b = e.getBoundingClientRect(); return [b.left, b.top + document.getElementById("app-main").scrollTop, b.width, b.height].map((v) => Math.round(v * 2) / 2); };
  const frame = () => {
    const root = document.documentElement;
    const header = document.getElementById("app-header");
    const main = document.getElementById("app-main");
    if (root && header && main) {
      const panels = [...main.querySelectorAll('[data-testid^="panel-"][data-state]')].filter((e) => /^panel-\\d+$/.test(e.dataset.testid));
      const buttons = [...header.querySelectorAll("button")].filter((e) => e.offsetParent !== null);
      const toolbar = main.querySelector(".dash-toolbar");
      log.frames.push({
        t: Math.round(performance.now() - log.t0),
        hold: root.hasAttribute("data-hune-hold"),
        theme: root.getAttribute("data-theme"),
        bg: getComputedStyle(document.body).backgroundColor,
        header: box(header),
        buttons: buttons.map(box),
        panels: panels.map(box),
        content: panels.filter((e) => e.querySelector(".panel__plot, .panel__overlay:not(:empty)")).length,
        toolbar: toolbar ? box(toolbar) : null,
        font: document.fonts.check("14px Inter") && document.fonts.check("500 14px Inter"),
        sprite: !!document.getElementById("hune-sprite"),
        external: [...document.querySelectorAll("svg use")].filter((u) => !(u.getAttribute("href") || "").startsWith("#")).length,
        icons: document.querySelectorAll("svg.icon").length,
        place: header.querySelectorAll(".location > *").length,
        scroll: [root.clientWidth, root.scrollHeight],
      });
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
})();`;

const same = (a, b, tol = 1) => a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) <= tol);
const sameList = (a, b, tol = 1) => a.length === b.length && a.every((v, i) => same(v, b[i], tol));

/** Loads the dashboard and reloads it with the probe on. Returns the frames of the reload. */
async function reloadFrames(t, { scheme, stored = null, cache, cpu = 4, width = 1280 }) {
  const p = await t.page({ width, height: 900 });
  await p.send("Emulation.setDeviceMetricsOverride", { width, height: 900, deviceScaleFactor: 1, mobile: false });
  await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
  if (stored) await p.send("Page.addScriptToEvaluateOnNewDocument", { source: `try { localStorage.setItem("hune.theme", ${JSON.stringify(stored)}); } catch (e) {}` });
  await p.send("Page.addScriptToEvaluateOnNewDocument", { source: PROBE });
  await p.send("Network.setCacheDisabled", { cacheDisabled: cache === "cold" });
  await p.goto(t.dash(UID58));
  await p.idle();
  await waitPanelsReady(p, IDS, "ready");
  const first = await p.eval("window.__flash.id");
  if (cpu > 1) await p.send("Emulation.setCPUThrottlingRate", { rate: cpu });
  await p.send("Page.reload", {});
  await p.waitFor(`window.__flash && window.__flash.id !== ${JSON.stringify(first)}`, { what: "the new document" });
  await p.waitFor(`document.documentElement.getAttribute("data-hune-state") === "idle" && window.__flash.frames.length > 0`, { timeout: 30000, what: "the reloaded page is idle" });
  await sleep(300);
  await p.idle();
  await sleep(300);
  const frames = await p.eval("window.__flash.frames");
  const final = await p.eval(`({ theme: document.documentElement.getAttribute("data-theme"), bg: getComputedStyle(document.body).backgroundColor })`);
  if (cpu > 1) await p.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  return { frames, final };
}

/** The problems of a timeline: what the viewer sees must be complete at the first frame and must not change after it. */
function judge(frames, final, expectTheme, label) {
  const out = [];
  const shown = frames.filter((f) => !f.hold);
  if (shown.length < 3) return [`${label}: only ${shown.length} frames after the release of the first paint`];
  const v0 = shown[0];
  const last = shown[shown.length - 1];
  // The theme and the background of every frame, also the frames that the hold hides (the first frame of the document).
  for (const f of frames) {
    if (f.theme !== expectTheme) out.push(`${label}: frame at ${f.t} ms has theme ${f.theme}, expected ${expectTheme}`);
    if (f.bg !== final.bg) out.push(`${label}: frame at ${f.t} ms has background ${f.bg}, final ${final.bg}`);
  }
  // The first frame that the viewer sees is complete.
  if (v0.panels.length !== IDS.length) out.push(`${label}: the first frame has ${v0.panels.length} panels, expected ${IDS.length}`);
  if (v0.buttons.length < 8) out.push(`${label}: the first frame has ${v0.buttons.length} header buttons`);
  if (!v0.font) out.push(`${label}: the font Inter is not loaded at the first frame`);
  if (!v0.sprite || v0.external > 0) out.push(`${label}: the icons are not inline at the first frame (sprite ${v0.sprite}, external ${v0.external})`);
  if (v0.place < 1) out.push(`${label}: the selector is not in the first frame`);
  if (v0.toolbar === null) out.push(`${label}: the variables row is not in the first frame`);
  // Nothing moves after it: the header, the buttons, the variables row and each panel frame (at the first frame, 100 ms later and at the end).
  const at100 = shown.find((f) => f.t >= v0.t + 100) || last;
  for (const [name, f] of [["100 ms later", at100], ["at the end", last]]) {
    if (!same(f.header, v0.header, 0.5)) out.push(`${label}: header ${JSON.stringify(v0.header)} then ${JSON.stringify(f.header)} ${name}`);
    if (!sameList(f.buttons, v0.buttons, 0.5)) out.push(`${label}: header buttons moved ${name}`);
    if (!sameList(f.panels, v0.panels, 0.5)) out.push(`${label}: panel frames moved ${name}`);
    if (f.toolbar && v0.toolbar && !same(f.toolbar, v0.toolbar, 0.5)) out.push(`${label}: variables row ${JSON.stringify(v0.toolbar)} then ${JSON.stringify(f.toolbar)} ${name}`);
    if (f.scroll[0] !== v0.scroll[0]) out.push(`${label}: page width ${v0.scroll[0]} then ${f.scroll[0]} ${name} (a scroll bar came or went)`);
  }
  let prev = v0;
  for (const f of shown) {
    if (f.panels.length < prev.panels.length) out.push(`${label}: panels went from ${prev.panels.length} to ${f.panels.length} at ${f.t} ms`);
    if (!sameList(f.panels, v0.panels, 0.5)) out.push(`${label}: a panel frame moved at ${f.t} ms`);
    if (!same(f.header, v0.header, 0.5)) out.push(`${label}: the header changed at ${f.t} ms`);
    if (f.content < prev.content) out.push(`${label}: the content of a panel went away at ${f.t} ms (${prev.content} then ${f.content})`);
    if (f.external > 0) out.push(`${label}: an icon is an external reference at ${f.t} ms`);
    prev = f;
  }
  return [...new Set(out)].slice(0, 6);
}

const EXPECTED_BG = { dark: "rgb(14, 17, 22)", light: "rgb(246, 247, 249)" }; // --bg of the look (css/look.css)

// ----------------------------------------------------------------------------------------------------------------------------
// B59 helpers

// The buttons of the header. The group of the edit mode (the last four) exists only in edit mode and is at the left of the others. At
// 640 px or less "Revert" is in the menu More actions, and its button does not show (display none).
const VIEW_BUTTONS = ["time-back", "time-picker-button", "time-forward", "zoom-out", "refresh-now", "refresh-interval", "edit-toggle", "dashboard-menu"];
const EDIT_BUTTONS = ["add-button", "add-menu", "save-button", "discard-button"];
const BUTTONS = [...VIEW_BUTTONS, ...EDIT_BUTTONS];
// No element of the view moves when the edit mode starts. Only the width of the selector place changes.
const NEVER_MOVE = ["brand", ...VIEW_BUTTONS, "theme"];
const RECTS = `(() => {
  const out = {};
  const box = (e) => { if (!e || e.offsetParent === null) return null; const b = e.getBoundingClientRect(); return [b.left, b.top, b.width, b.height].map((v) => Math.round(v * 2) / 2); };
  for (const id of ${JSON.stringify(BUTTONS)}) out[id] = box(document.querySelector('[data-testid="' + id + '"]'));
  out.brand = box(document.querySelector(".brand"));
  out.lock = box(document.querySelector('[data-testid="read-only-lock"]'));
  out.place = box(document.querySelector(".location"));
  out.theme = box(document.querySelector(".theme-select__button"));
  out.bar = box(document.querySelector('[data-testid="edit-bar"]'));
  out.header = [document.getElementById("app-header").getBoundingClientRect().height];
  out.hw = [document.getElementById("app-header").getBoundingClientRect().width];
  out.gap = [parseFloat(getComputedStyle(document.querySelector(".header-actions__row")).columnGap)];
  return out;
})()`;

/** The keys whose rect differs between two readings (a button that one reading has not is not compared). */
function moved(a, b, keys) {
  const out = [];
  for (const k of keys) if (a[k] && b[k] && !same(a[k], b[k], 0.5)) out.push(`${k} ${JSON.stringify(a[k])} -> ${JSON.stringify(b[k])}`);
  return out;
}

/** The keys whose size (width, height) differs between two readings. */
function resized(a, b, keys) {
  const out = [];
  for (const k of keys) if (a[k] && b[k] && !same(a[k].slice(2), b[k].slice(2), 0.5)) out.push(`${k} ${JSON.stringify(a[k].slice(2))} -> ${JSON.stringify(b[k].slice(2))}`);
  return out;
}

/** The rule for a reading `before` of the view and a reading `after` of the edit mode: nothing moves, the selector gives the room. */
function judgeEditMoves(before, after, label) {
  const out = [];
  const bar = after.bar;
  if (!bar) return [`${label}: the edit mode has no group of Add, Save and Revert`];
  // The distance: the width of the group and one gap of the row.
  const distance = bar[2] + after.gap[0];
  // The place keep their left edge and give the distance from their width.
  if (Math.abs(after.place[0] - before.place[0]) > 0.5 || Math.abs(after.place[2] - before.place[2] + distance) > 0.5) out.push(`${label}: the selector place went from ${JSON.stringify(before.place)} to ${JSON.stringify(after.place)}, expected a width smaller by ${distance}`);
  // No element of the view moves.
  out.push(...moved(before, after, NEVER_MOVE).map((m) => `${label}: ${m} (it must not move)`));
  // The group is at the left of the time control, one gap from it.
  const first = after["time-back"] || after["time-picker-button"];
  if (Math.abs(first[0] - (bar[0] + bar[2] + after.gap[0])) > 0.5) out.push(`${label}: the group ends at ${bar[0] + bar[2]}, the time control starts at ${first[0]} (one gap of ${after.gap[0]} expected)`);
  // No size changes: each button has its size.
  out.push(...resized(before, after, VIEW_BUTTONS).map((m) => `${label}: ${m}`));
  if (after.header[0] !== before.header[0]) out.push(`${label}: header height ${before.header[0]} -> ${after.header[0]}`);
  return out;
}

/** In the view no blank is at the right: the theme button follows the last control of the page after one gap. */
function judgeNoBlank(r, last, label) {
  const gap = r.theme[0] - (r[last][0] + r[last][2]);
  return Math.abs(gap - r.gap[0]) > 0.5 ? [`${label}: ${gap} px between ${last} and the theme button, expected ${r.gap[0]} (a blank slot?)`] : [];
}

// The sampler of a transition: at each animation frame it records the layout of the header (the box of each control and of the selector place).
// The layouts that it sees, in order, are the layouts that a viewer can see.
const SAMPLER = `(() => {
  const ids = ${JSON.stringify([...BUTTONS, "edit-bar"])};
  const log = (window.__layouts = { frames: [] });
  const sig = () => {
    const box = (e) => { if (!e || e.offsetParent === null) return "-"; const b = e.getBoundingClientRect(); return [b.left, b.top, b.width, b.height].map((v) => Math.round(v * 2) / 2).join(","); };
    const parts = ids.map((id) => id + "=" + box(document.querySelector('[data-testid="' + id + '"]')));
    parts.push("place=" + box(document.querySelector(".location")), "theme=" + box(document.querySelector(".theme-select__button")), "brand=" + box(document.querySelector(".brand")));
    parts.push("h=" + document.getElementById("app-header").getBoundingClientRect().height, "w=" + document.documentElement.scrollWidth);
    return parts.join(" ");
  };
  const frame = () => { log.frames.push(sig()); requestAnimationFrame(frame); };
  requestAnimationFrame(frame);
})()`;

/** The layouts of a sampled transition, in order, with the repeats removed. */
async function sampledLayouts(p) {
  const frames = await p.eval("window.__layouts.frames");
  return { frames: frames.length, layouts: frames.filter((f, i) => i === 0 || f !== frames[i - 1]) };
}

async function openAt(t, url, width) {
  const p = await t.page({ width, height: 900 });
  await p.send("Emulation.setDeviceMetricsOverride", { width, height: 900, deviceScaleFactor: 1, mobile: false });
  await p.goto(url);
  await p.idle();
  // A range outside the seed data gives empty panels. A panel below the fold of a narrow window does not query: it stays idle.
  // The panels are done when the first panel is done and no panel is queued or loading.
  await p.waitFor(`(() => { const all = [...document.querySelectorAll('[data-testid^="panel-"][data-state]')].filter((e) => /^panel-\\d+$/.test(e.dataset.testid)); const first = document.querySelector('[data-testid="panel-1"]'); return all.length === ${IDS.length} && first && (first.dataset.state === "ready" || first.dataset.state === "empty") && all.every((e) => e.dataset.state !== "queued" && e.dataset.state !== "loading"); })()`, { what: `the panels are done at ${url}` }).catch(async (e) => {
    throw new Error(`${e.message}: ${JSON.stringify(await panelStates(p))}`);
  });
  await p.idle();
  await sleep(150);
  return p;
}

export const cases = [
  {
    id: "B58",
    name: "a reload shows one stable render: the first frame is complete (theme, font, icons, header, panel frames) and nothing moves or goes away after it (9.6, 9.8, 9.25)",
    async run(t) {
      const r = await t.api.create("transit", flashDoc(UID58));
      t.assert(r.status === 201, `create ${UID58}: ${r.status} ${r.text.slice(0, 300)}`);
      const found = [];
      // The system theme, light and dark, with a warm and a cold cache; then a stored choice that differs from the system.
      const runs = [
        { scheme: "dark", cache: "warm", expect: "dark" },
        { scheme: "dark", cache: "cold", expect: "dark" },
        { scheme: "light", cache: "warm", expect: "light" },
        { scheme: "light", cache: "cold", expect: "light" },
        { scheme: "light", stored: "dark", cache: "warm", expect: "dark" },
        { scheme: "dark", stored: "light", cache: "cold", expect: "light" },
      ];
      for (const run of runs) {
        const label = `${run.scheme}${run.stored ? "+" + run.stored : ""} ${run.cache}`;
        const { frames, final } = await reloadFrames(t, run);
        if (final.theme !== run.expect) found.push(`${label}: final theme ${final.theme}, expected ${run.expect}`);
        if (final.bg !== EXPECTED_BG[run.expect]) found.push(`${label}: final background ${final.bg}, expected ${EXPECTED_BG[run.expect]}`);
        found.push(...judge(frames, final, run.expect, label));
      }
      t.eq(found, [], "flash");
    },
  },
  {
    id: "B59",
    name: "the header sticks to the top; each button has one height and one size in every state; no button of the view moves when the edit mode starts (the selector gives the room), with two layouts and no frame between them; no Share control (9.6, 9.9, 10.2)",
    async run(t) {
      const r = await t.api.create("transit", flashDoc(UID59));
      t.assert(r.status === 201, `create ${UID59}: ${r.status} ${r.text.slice(0, 300)}`);
      const found = [];
      const keys = [...VIEW_BUTTONS, "theme", "brand", "place"];

      for (const width of [1920, 1280, 768, 390]) {
        const label = `${width}`;
        // 1. The header sticks: after a scroll, its top is the top of the window and it is above the content. Nothing moves.
        const p = await openAt(t, t.dash(UID59), width);
        const before = await p.eval(RECTS);
        found.push(...judgeNoBlank(before, "edit-toggle", `${label} view`));
        await p.eval(`document.getElementById("app-main").scrollTo(0, 400)`);
        await sleep(150);
        const stuck = await p.eval(`(() => { const h = document.getElementById("app-header"); const b = h.getBoundingClientRect(); const x = Math.min(b.width - 4, 40); const at = document.elementFromPoint(x, Math.min(b.height - 4, 20)); return { scrollY: Math.round(document.getElementById("app-main").scrollTop), top: b.top, fixed: !document.getElementById("app-main").contains(h), covers: !!at && h.contains(at) }; })()`);
        if (stuck.scrollY < 100) found.push(`${label}: the content did not scroll (${stuck.scrollY})`);
        if (!stuck.fixed || Math.abs(stuck.top) > 0.5 || !stuck.covers) found.push(`${label}: the header does not stay on top (${JSON.stringify(stuck)})`);
        const scrolled = await p.eval(RECTS);
        found.push(...moved(before, scrolled, keys).map((m) => `${label} after a scroll: ${m}`));
        await p.eval(`document.getElementById("app-main").scrollTo(0, 0)`);

        // 2. The buttons keep their place and size in the view: a long range text, another interval, no interval, a long quick range.
        for (const [name, query] of [["absolute range", "from=1700000000000&to=1700003600000&refresh=off"], ["interval 30m", "from=now-90d&to=now&refresh=30m"], ["interval off", "refresh=off"]]) {
          const q = await openAt(t, t.url(`d/${UID59}?${query}`), width);
          const now = await q.eval(RECTS);
          found.push(...moved(before, now, keys).map((m) => `${label} ${name}: ${m}`));
          if (now.header[0] !== before.header[0]) found.push(`${label} ${name}: header height ${before.header[0]} -> ${now.header[0]}`);
        }

        // 3. Refresh running: the spinner replaces the icon and the button stays.
        await p.click(p.tid("refresh-now"));
        const running = await p.eval(RECTS);
        found.push(...moved(before, running, keys).map((m) => `${label} refresh running: ${m}`));
        await p.idle();

        // 4. The edit mode starts. A sampler records the layout at each frame: the viewer sees the layout of the view, then the layout of the
        //    edit mode, and no layout between them.
        await p.eval(SAMPLER);
        await p.click(p.tid("edit-toggle"));
        await p.waitSel(p.tid("edit-bar"));
        await p.waitSel('.edit-grid [data-testid^="panel-"][data-state]');
        await p.idle();
        await sleep(300);
        const start = await sampledLayouts(p);
        if (start.frames < 5) found.push(`${label}: the sampler saw only ${start.frames} frames`);
        if (start.layouts.length !== 2) found.push(`${label}: the switch gave ${start.layouts.length} layouts in ${start.frames} frames, expected 2 (view, edit mode)`);
        else if (!/add-button=-/.test(start.layouts[0]) || /add-button=-/.test(start.layouts[1])) found.push(`${label}: the two layouts are not the view and the edit mode`);
        const edit = await p.eval(RECTS);
        found.push(...judgeEditMoves(before, edit, `${label} edit mode`));

        // 5. In edit mode: Save disabled then enabled, nothing moves and no size changes.
        const saveWasOff = await p.eval(`document.querySelector('[data-testid="save-button"]').disabled`);
        await moreItem(p, "add-section");
        await p.waitFor(`document.querySelector('[data-testid="save-button"]').disabled === false`, { what: "Save enabled" });
        const edit2 = await p.eval(RECTS);
        if (!saveWasOff) found.push(`${label}: Save was not disabled before the change`);
        found.push(...moved(edit, edit2, [...keys, ...EDIT_BUTTONS]).map((m) => `${label} edit mode, Save on: ${m}`));
        if (edit2.header[0] !== edit.header[0]) found.push(`${label} edit mode: header height ${edit.header[0]} -> ${edit2.header[0]}`);
        // The order is Add, Save, Revert (no button up to 640 px), the time control, the switch, and the group is inside the edit bar.
        const order = await p.eval(`(() => { const x = (id) => { const e = document.querySelector('[data-testid="' + id + '"]'); return e.offsetParent === null ? null : e.getBoundingClientRect().left; }; return ["add-button", "add-menu", "save-button", "discard-button", "time-picker-button", "edit-toggle"].map(x).filter((v) => v !== null); })()`);
        if (order.some((v, i) => i > 0 && v <= order[i - 1])) found.push(`${label}: the order Add, Save, Revert, time control, switch is wrong: ${JSON.stringify(order)}`);
        const inBar = await p.eval(`(() => { const s = document.querySelector('[data-testid="edit-bar"]').getBoundingClientRect(); return ["add-button", "add-menu", "save-button"].every((id) => { const b = document.querySelector('[data-testid="' + id + '"]').getBoundingClientRect(); return b.left >= s.left - 0.5 && b.right <= s.right + 0.5; }); })()`);
        if (!inBar) found.push(`${label}: Add and Save are not inside the edit bar`);

        // 6. A view of the editor (the panel editor): the header is the same, Add keeps its place and is not active.
        await p.click(p.tid("panel-1-menu"));
        await p.click(p.tid("panel-1-edit"));
        await p.waitSel(p.tid("panel-editor"));
        await p.idle();
        const editor = await p.eval(RECTS);
        // The panel editor has no page scroll bar, so the whole header shifts by the width of the bar at the left edge only. The positions
        // that count are the distances to the right edge of the header, and the left of the brand and of the selector place.
        const rel = (r) => Object.fromEntries(Object.entries(r).map(([k, v]) => [k, v && k !== "header" && k !== "gap" && k !== "brand" && k !== "place" ? [v[0] - r.hw[0], ...v.slice(1)] : v]));
        found.push(...moved(rel(edit2), rel(editor), [...keys.filter((k) => k !== "place"), ...EDIT_BUTTONS]).map((m) => `${label} panel editor: ${m}`));
        if (Math.abs(editor.place[0] - edit2.place[0]) > 0.5) found.push(`${label} panel editor: the selector place starts at ${editor.place[0]}, was ${edit2.place[0]}`);
        await p.click(p.tid("editor-back"));
        await p.waitGone(p.tid("panel-editor"));
        await p.idle();

        // 7. No Share control: not in the header, not in the panel menu.
        const shareIn = await p.eval(`[...document.querySelectorAll('[data-testid*="share"], [aria-label*="Share"], [data-tooltip="Share"]')].length + (/\\bShare\\b/.test(document.getElementById("app-header").textContent) ? 1 : 0)`);
        if (shareIn) found.push(`${label}: a Share control is in the header`);

        // 8. The edit mode ends (Discard in the question): the layout of the view comes back, in one step.
        await p.eval(SAMPLER);
        await p.click(p.tid("edit-toggle"));
        await p.waitSel(p.tid("exit-dialog"));
        await p.clickText('[data-testid="exit-dialog"] button', "Discard");
        await p.waitGone(p.tid("edit-bar"));
        await p.idle();
        await sleep(300);
        const back = await p.eval(RECTS);
        found.push(...moved(before, back, keys).map((m) => `${label} after the edit mode: ${m}`));
        const end = await sampledLayouts(p);
        if (end.layouts.length !== 2) found.push(`${label}: the end of the edit mode gave ${end.layouts.length} layouts in ${end.frames} frames, expected 2`);
      }

      // 9. The rights: a viewer has no "Edit" and no group of the edit mode. The theme and More actions are at the right end, so they keep the
      //    place they have for an editor, and the viewer has no blank slot at the right.
      const viewer = await t.ownServer("viewer", { editing: false, prepare: (volume) => writeFileSync(join(volume, "transit", `${UID59}.json`), JSON.stringify(flashDoc(UID59))) });
      for (const width of [1920, 1280, 768, 390]) {
        const e = await openAt(t, t.dash(UID59), width);
        const asEditor = await e.eval(RECTS);
        const v = await openAt(t, `${viewer.url}d/${UID59}?${t.range}`, width);
        const asViewer = await v.eval(RECTS);
        if (asViewer["edit-toggle"]) found.push(`${width}: the viewer has an Edit button`);
        if (!asEditor["edit-toggle"]) found.push(`${width}: the editor has no Edit button`);
        if (asViewer.bar) found.push(`${width}: the viewer has an edit bar`);
        found.push(...moved(asEditor, asViewer, ["dashboard-menu", "theme"]).map((m) => `${width} viewer: ${m}`));
        // A dashboard of a read-only store has no lock and no switch Edit: the time control is the last control of the page.
        if (asViewer.lock) found.push(`${width}: the viewer has a lock`);
        found.push(...judgeNoBlank(asViewer, "refresh-interval", `${width} viewer`));
        if (asViewer.header[0] !== asEditor.header[0]) found.push(`${width}: the header height differs for a viewer (${asViewer.header[0]}, ${asEditor.header[0]})`);
      }

      // 10. The panel menu has no Share item, and the route api/share does not exist.
      const p = await openAt(t, t.dash(UID59), 1280);
      await p.click(p.tid("panel-10-menu"));
      await p.waitSel(".menu");
      const items = await p.eval(`[...document.querySelectorAll('.menu [role="menuitem"]')].map((x) => x.textContent.trim())`);
      if (items.some((x) => /share/i.test(x))) found.push(`the panel menu has a Share item: ${JSON.stringify(items)}`);
      if (!items.includes("View") || !items.includes("View data")) found.push(`the panel menu lost its items: ${JSON.stringify(items)}`);
      await p.press("Escape");
      const answer = await fetch(t.url("api/share")).then((x) => x.status);
      if (answer !== 404) found.push(`GET api/share answers ${answer}, expected 404`);
      const answerPost = await fetch(t.url("api/share"), { method: "POST", headers: { "content-type": "application/json" }, body: "{}" }).then((x) => x.status);
      if (answerPost !== 404 && answerPost !== 405) found.push(`POST api/share answers ${answerPost}, expected 404 or 405`);
      t.eq(found, [], "header");
    },
  },
];
