// The states that the screenshot harness captures.
//
// A state has an id, a text, and a function run(c). `c` is the context of one capture: a fresh page (c.page), the
// theme and the width, the servers, and the helpers open(), shot() and the API of the work server.
// A state that does not call c.shot() gets one screenshot of the page at the end (the viewport or the full page,
// by its `view` member). `c.shot(suffix, options)` adds a screenshot with a suffix in its name.
//
// Members:
//   server: "main" (read only use), "work" (a copy of the volume that the state can change) or "empty".
//   view:   "full" (the whole page, the default), "viewport" (the first screen only).
//   themes: "all" (the four themes), "core" (light and dark). Default "core".
//   widths: "all" (1280, 1920, 768, 390), "wide" (1280, 1920). Default "wide".
import { DATA_FROM, DATA_TO } from "./infra.mjs";
import { hold, hover, moveMouse, sleep, sweepHover } from "./kit.mjs";

const states = [];
function state(id, text, opts, run) {
  // The states that change data run on the work server (a copy of the volume), never on the main one.
  const server = /^(edit-|history-|diff-|restore-|delete-dashboard)/.test(id) ? "work" : "main";
  states.push({ id, text, server, view: "full", themes: "core", widths: "wide", ...opts, run });
}

const RANGE = `from=${DATA_FROM}&to=${DATA_TO}`;
const RANGE_48H = `from=${DATA_TO - 48 * 3600 * 1000}&to=${DATA_TO}`;

/** The panel frame selector. */
const P = (id) => `[data-testid="panel-${id}"]`;
const T = (id) => `[data-testid="${id}"]`;

/** Click an item of the menu "More actions" of the header: History and the views have no button of their own (9.9). "Add panel" and
 *  "Add section" are in the menu of the button "Add" (10.2). */
async function moreItem(c, id) {
  await c.page.click(T(id === "add-panel" || id === "add-section" ? "add-menu" : "dashboard-menu"));
  await c.page.waitSel(`.menu ${T(id)}`);
  await c.page.click(`.menu ${T(id)}`);
}

async function menuOpen(c, id) {
  await c.page.click(T(`panel-${id}-menu`));
  await c.page.waitFor(`document.querySelector('[role=menu]') !== null || document.querySelector('.menu') !== null`, { what: "a menu" }).catch(() => {});
  await sleep(150);
}

// ---- list pages -----------------------------------------------------------------------------------------------------

state("list", "List page: folders, tags, an invalid entry", { view: "full", themes: "all", widths: "all" }, async (c) => {
  await c.open(c.url(""));
});

state("empty-list", "List page of an empty volume", { server: "empty", themes: "all", widths: "all" }, async (c) => {
  await c.open(c.url(""));
});

state("list-search", "List page with a search text", {}, async (c) => {
  await c.open(c.url(""));
  await c.page.fill(T("list-search"), "table");
  await sleep(500);
  await c.settle();
});

state("list-search-none", "List page with a search that finds nothing", {}, async (c) => {
  await c.open(c.url(""));
  await c.page.fill(T("list-search"), "zzzz-no-such-dashboard");
  await sleep(500);
  await c.settle();
});

state("list-invalid-errors", "List page: the errors of the invalid entry shown", {}, async (c) => {
  await c.open(c.url(""));
  await c.page.clickText(`${T("row-broken-layout")} button`, "Show errors");
  await sleep(200);
});

state("list-row-menu", "List page: the menu of a row", { view: "viewport" }, async (c) => {
  await c.open(c.url(""));
  await c.page.click(T("row-menu-types-tour"));
  await sleep(250);
});

state("new-dashboard-dialog", "Dialog: new dashboard", { view: "viewport" }, async (c) => {
  await c.open(c.url(""));
  await c.page.click(T("new-dashboard"));
  await c.page.waitSel(T("new-dashboard-dialog"));
  await sleep(200);
});

state("new-folder-dialog", "Dialog: new folder", { view: "viewport" }, async (c) => {
  await c.open(c.url(""));
  await c.page.click(T("new-folder"));
  await c.page.waitSel(T("new-folder-dialog"));
  await sleep(200);
});

state("import-dialog", "Dialog: import", { view: "viewport" }, async (c) => {
  await c.open(c.url(""));
  await c.page.click(T("import"));
  await c.page.waitSel(T("import-dialog"));
  await sleep(200);
});

// ---- dashboards: top, scrolled and full -----------------------------------------------------------------------------

const DEMOS = [
  ["demo", "Demo hosts", RANGE],
  ["types-tour", "Panel types tour", RANGE],
  ["tables-and-cells", "Tables, cells and transforms", RANGE],
  ["variables-lab", "Variables lab", RANGE_48H],
  ["edge-states", "Edge states", RANGE],
  ["density-20", "Density, 20 panels", RANGE],
  ["service-overview", "Service overview", RANGE],
];

for (const [uid, title, range] of DEMOS) {
  const main = uid === "types-tour" || uid === "service-overview";
  state(`dash-${uid}-full`, `Dashboard ${title}: the whole page`, { themes: main ? "all" : "core", widths: main ? "all" : "wide" }, async (c) => {
    await c.open(c.url(`d/${uid}?${range}`));
  });
  state(`dash-${uid}-top`, `Dashboard ${title}: the first screen`, { view: "viewport", themes: "core", widths: "wide" }, async (c) => {
    await c.open(c.url(`d/${uid}?${range}`));
  });
  state(`dash-${uid}-scrolled`, `Dashboard ${title}: scrolled by one screen`, { view: "viewport", themes: "core", widths: "wide" }, async (c) => {
    await c.open(c.url(`d/${uid}?${range}`));
    await c.page.eval(`window.scrollTo(0, innerHeight * 0.9)`);
    await c.settle();
  });
}

// ---- panel types: close-ups -----------------------------------------------------------------------------------------

const CLOSEUPS = [
  ["timeseries", "types-tour", 1],
  ["timeseries-bars", "types-tour", 2],
  ["timeseries-smooth", "types-tour", 3],
  ["stat", "types-tour", 11],
  ["stat-area", "types-tour", 10],
  ["bargauge", "types-tour", 14],
  ["bargauge-lcd", "types-tour", 15],
  ["barchart", "types-tour", 20],
  ["barchart-stacked", "types-tour", 21],
  ["piechart", "types-tour", 22],
  ["xychart", "types-tour", 24],
  ["table", "tables-and-cells", 1],
  ["table-matrix", "tables-and-cells", 3],
];
for (const [name, uid, id] of CLOSEUPS) {
  state(`panel-${name}`, `Close-up of one panel: ${name}`, { themes: "core", widths: "wide" }, async (c) => {
    await c.open(c.url(`d/${uid}?${RANGE}`));
    await c.page.eval(`document.querySelector(${JSON.stringify(P(id))}).scrollIntoView({ block: "center" })`);
    await c.settle();
    await c.shot("", { selector: P(id), pad: 6 });
  });
}

state("legend-table-bottom", "Legend table at the bottom (time series)", {}, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.shot("", { selector: P(1), pad: 6 });
});

state("legend-table-right", "Legend table at the right (time series)", {}, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.shot("", { selector: P(3), pad: 6 });
});

state("legend-list-right", "Legend list at the right (time series)", {}, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.shot("", { selector: P(2), pad: 6 });
});

state("legend-pie-table", "Legend table of a pie chart", {}, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.shot("", { selector: P(22), pad: 6 });
});

/** A clip around a panel with room for a tooltip that leaves the panel. */
const TIP_PAD = 110;

async function hoverChart(c, uid, id, ys = null, xs = null) {
  await c.open(c.url(`d/${uid}?${RANGE}`));
  await c.page.eval(`document.querySelector(${JSON.stringify(P(id))}).scrollIntoView({ block: "center" })`);
  await c.settle();
  const hit = await sweepHover(c.page, `${P(id)} .chartCore__overlay`, `${P(id)} .chartCore__tooltip`, { ys, xs });
  if (!hit) throw new Error(`no tooltip appeared on panel ${id}`);
  await sleep(250);
  await c.shot("", { full: false });
}

state("tooltip-timeseries", "Tooltip on a time series (multi)", {}, async (c) => {
  await hoverChart(c, "types-tour", 1, [0.4, 0.5, 0.3], [0.6, 0.5, 0.7]);
});
state("tooltip-bars", "Tooltip on stacked bars", {}, async (c) => {
  await hoverChart(c, "types-tour", 2, [0.8, 0.9, 0.7]);
});
state("tooltip-barchart", "Tooltip on a bar chart", {}, async (c) => {
  await hoverChart(c, "types-tour", 20, [0.9, 0.8, 0.7, 0.6, 0.5]);
});
state("tooltip-xychart", "Tooltip on an XY chart", {}, async (c) => {
  await hoverChart(c, "types-tour", 24, [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8]);
});
state("tooltip-pie", "Tooltip on a pie chart (a real slice)", {}, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.eval(`document.querySelector(${JSON.stringify(P(22))}).scrollIntoView({ block: "center" })`);
  await c.settle();
  // The first slice of the donut: a point in the ring at a small angle from the top.
  const pt = await c.page.eval(`(() => {
    const canvas = document.querySelector(${JSON.stringify(P(22))} + " canvas");
    const b = canvas.getBoundingClientRect();
    const radius = Math.max(10, Math.min(b.width, b.height) / 2 - 8);
    const a = -Math.PI / 2 + 0.6, r = radius * 0.79;
    return { x: b.left + b.width / 2 + Math.cos(a) * r, y: b.top + b.height / 2 + Math.sin(a) * r };
  })()`);
  await moveMouse(c.page, pt.x - 3, pt.y - 3);
  await sleep(80);
  await moveMouse(c.page, pt.x, pt.y);
  await c.page.waitFor(`(() => { const e = document.querySelector(${JSON.stringify(P(22))} + " .pie__tip"); return !!e && !e.hidden; })()`, { timeout: 4000, what: "pie tooltip" });
  await sleep(250);
  await c.shot("", { full: false });
});

state("tooltip-pie-pie", "Tooltip on a plain pie (a real slice)", {}, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.eval(`document.querySelector(${JSON.stringify(P(23))}).scrollIntoView({ block: "center" })`);
  await c.settle();
  const pt = await c.page.eval(`(() => {
    const canvas = document.querySelector(${JSON.stringify(P(23))} + " canvas");
    const b = canvas.getBoundingClientRect();
    const radius = Math.max(10, Math.min(b.width, b.height) / 2 - 8);
    const a = -Math.PI / 2 + 2.6, r = radius * 0.6;
    return { x: b.left + b.width / 2 + Math.cos(a) * r, y: b.top + b.height / 2 + Math.sin(a) * r };
  })()`);
  await moveMouse(c.page, pt.x - 3, pt.y - 3);
  await sleep(80);
  await moveMouse(c.page, pt.x, pt.y);
  await c.page.waitFor(`(() => { const e = document.querySelector(${JSON.stringify(P(23))} + " .pie__tip"); return !!e && !e.hidden; })()`, { timeout: 4000, what: "pie tooltip" });
  await sleep(250);
  await c.shot("", { full: false });
});

// ---- panel states ---------------------------------------------------------------------------------------------------

state("loading", "Dashboard while the queries are in flight", { view: "viewport", themes: "all" }, async (c) => {
  const h = await hold(c.page, (r) => /\/panels\/\d+\/query/.test(r.url) || /\/options$/.test(r.url));
  try {
    await c.page.goto(c.url(`d/types-tour?${RANGE}`));
    await c.page.waitFor(`document.querySelectorAll('[data-testid^="panel-"][data-state="loading"], [data-testid^="panel-"][data-state="queued"]').length > 2`, { timeout: 15000, what: "loading panels" });
    await sleep(900); // the progress bar of a slow panel shows after a delay
    await c.shot("");
  } finally {
    await h.release();
  }
});

state("loading-progress", "A slow query: the panel progress while it runs", { view: "viewport" }, async (c) => {
  // Hold the query of the slow panel, so that the capture is before the end of the query.
  const h = await hold(c.page, (r) => /\/panels\/33\/query/.test(r.url));
  try {
    await c.page.goto(c.url(`d/edge-states?${RANGE}`));
    await c.page.waitSel(P(33));
    await c.page.eval(`document.querySelector(${JSON.stringify(P(33))}).scrollIntoView({ block: "center" })`);
    await c.page.waitFor(`document.querySelector(${JSON.stringify(P(33))}).dataset.state === "loading"`, { timeout: 15000, what: "slow panel loading" });
    await sleep(900); // the progress bar of a slow panel shows after a delay
    await c.shot("", { selector: P(33), pad: 24, noGrow: true });
    await c.shot("page", { full: false });
  } finally {
    await h.release();
  }
  await c.settle();
});

state("panel-error", "Panel with an error (bad SQL)", {}, async (c) => {
  await c.open(c.url(`d/edge-states?${RANGE}`));
  await c.shot("", { selectors: [P(1), P(2), P(3)], pad: 6 });
});

state("panel-no-data", "Panels with no data, one of each type", {}, async (c) => {
  await c.open(c.url(`d/edge-states?${RANGE}`));
  await c.shot("", { selectors: [P(10), P(11), P(12), P(13), P(14), P(15), P(16), P(20), P(21)], pad: 6 });
});

state("panel-gaps", "Time series with gaps: broken and joined", {}, async (c) => {
  await c.open(c.url(`d/edge-states?${RANGE}`));
  await c.shot("", { selectors: [P(30), P(31), P(32)], pad: 6 });
});

state("panel-description", "Panel header with a description and links", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/edge-states?${RANGE}`));
  await c.page.eval(`window.scrollTo(0, document.querySelector(${JSON.stringify(P(30))}).getBoundingClientRect().top + scrollY - 120)`);
  await c.settle();
  await c.page.click(T("panel-30-links"));
  await sleep(250);
});

state("section-collapsed", "Section collapsed (last section of the tour)", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.eval(`window.scrollTo(0, document.documentElement.scrollHeight)`);
  await c.settle();
});

state("section-expanded", "Section expanded after a click on its header", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.eval(`window.scrollTo(0, document.documentElement.scrollHeight)`);
  await c.settle();
  await c.page.click(`${T("section-3")} .section__toggle`);
  await c.settle();
  await c.page.eval(`window.scrollTo(0, document.documentElement.scrollHeight)`);
  await c.settle();
});

// ---- header, pickers, variables, menus ------------------------------------------------------------------------------

state("variables-bar", "Variables bar with many variables", { view: "viewport", themes: "all", widths: "all" }, async (c) => {
  await c.open(c.url(`d/variables-lab?${RANGE_48H}`));
});

state("variable-multi-open", "Variable multi-select popup open (query variable, All)", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/variables-lab?${RANGE_48H}`));
  await c.page.click(T("variable-host-button"));
  await sleep(300);
});

state("variable-custom-open", "Variable multi-select popup open (custom variable)", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/variables-lab?${RANGE_48H}`));
  await c.page.click(T("variable-window-button"));
  await sleep(300);
});

state("variable-single-open", "Variable single-select popup open", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/variables-lab?${RANGE_48H}`));
  await c.page.click(T("variable-env-button"));
  await sleep(300);
});

state("time-picker-open", "Time picker open", { view: "viewport", themes: "all", widths: "all" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.click(T("time-picker-button"));
  await sleep(300);
});

state("time-picker-relative", "Time picker with a relative range (no URL range)", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/service-overview`));
  await c.page.click(T("time-picker-button"));
  await sleep(300);
});

state("time-picker-error", "Time picker with a wrong text", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.click(T("time-picker-button"));
  await c.page.fill(T("time-from"), "yesterday");
  await c.page.click(T("time-apply"));
  await sleep(300);
});

state("refresh-picker", "Refresh interval menu open (themed menu)", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.click(T("refresh-interval"));
  await c.page.waitFor(`document.querySelector('[role="menuitemradio"]') !== null`, { timeout: 5000, what: "refresh menu items" });
  await sleep(250);
});

state("panel-menu-open", "Panel menu open", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await menuOpen(c, 1);
});

state("dashboard-menu-open", "Dashboard actions menu open", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.click(T("dashboard-menu"));
  await sleep(250);
});

state("panel-links-menu", "Panel links menu open", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/tables-and-cells?${RANGE}`));
  await c.page.click(T("panel-1-links"));
  await sleep(250);
});

state("view-data", "Dialog: View data of a panel", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await menuOpen(c, 14);
  await c.page.click(T("panel-14-view-data"));
  await c.page.waitSel(T("view-data-14"));
  await sleep(250);
});

state("view-data-table", "Dialog: View data of a wide table", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/tables-and-cells?${RANGE}`));
  await menuOpen(c, 4);
  await c.page.click(T("panel-4-view-data"));
  await c.page.waitSel(T("view-data-4"));
  await sleep(250);
});

state("view-panel", "View of one panel (viewPanel)", { themes: "all", widths: "all", view: "full" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}&viewPanel=1`));
  await c.page.waitSel(T("view-panel"));
});

state("view-panel-table", "View of one table panel", { view: "full" }, async (c) => {
  await c.open(c.url(`d/tables-and-cells?${RANGE}&viewPanel=4`));
  await c.page.waitSel(T("view-panel"));
});

state("notice", "A notice (toast): a refresh value that the page does not offer", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}&refresh=7s`));
  await c.page.waitSel(T("notice"));
  await sleep(300);
});

state("notice-error", "A notice of level error (a failed refresh)", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  const stop = c.page.failWhen((r) => /\/panels\/\d+\/query/.test(r.url));
  await c.page.click(T("refresh-now"));
  await sleep(1200);
  await c.settle();
  stop();
});

// ---- full pages: 401, no answer, 404 ---------------------------------------------------------------------------------

state("page-401", "The 401 page", { view: "viewport", themes: "all" }, async (c) => {
  c.page.fulfillWhen((r) => /\/api\/session/.test(r.url), { status: 401, headers: { "content-type": "application/json" }, body: JSON.stringify({ error: { code: "unauthenticated", message: "No identity." } }) });
  await c.page.goto(c.url(""));
  await c.page.waitSel(T("unauthorized"));
  await c.settle();
});

state("page-no-answer", "The page 'Hune does not answer.'", { view: "viewport", themes: "all" }, async (c) => {
  c.page.failWhen((r) => /\/api\//.test(r.url));
  await c.page.goto(c.url(""));
  await c.page.waitSel(T("no-answer"));
  await c.settle();
});

state("page-404", "The 404 page (unknown address)", { view: "viewport", themes: "all", widths: "all" }, async (c) => {
  await c.open(c.url("no-such-page"));
  await c.page.waitSel(T("not-found"));
});

state("page-dashboard-404", "The page of a dashboard that does not exist", { view: "viewport" }, async (c) => {
  await c.open(c.url("d/no-such-dashboard"));
  await c.page.waitSel(T("dashboard-not-found"));
});

state("page-dashboard-invalid", "The page of a dashboard that is not valid", { view: "viewport" }, async (c) => {
  await c.open(c.url("d/broken-layout"));
  await c.page.waitSel(T("dashboard-invalid"));
});

state("page-dashboard-error", "The page 'The dashboard did not load' (a 500)", { view: "viewport" }, async (c) => {
  c.page.fulfillWhen((r) => /\/api\/dashboards\/types-tour(\?|$)/.test(r.url), { status: 500, headers: { "content-type": "application/json" }, body: JSON.stringify({ error: { code: "internal", message: "The store failed." } }) });
  await c.page.goto(c.url(`d/types-tour?${RANGE}`));
  await c.page.waitSel(T("dashboard-error"));
  await c.settle();
});

// ---- focus and the theme control ------------------------------------------------------------------------------------

state("focus-header", "Keyboard focus visible on the header (Tab)", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  for (let i = 0; i < 4; i++) await c.page.press("Tab");
  await sleep(150);
});

state("focus-panel-menu", "Keyboard focus visible on a panel menu button", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.press("Tab");
  await c.page.focus(T("panel-1-menu"));
  await sleep(150);
});

state("focus-variable", "Keyboard focus visible on a variable control", { view: "viewport" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.press("Tab");
  await c.page.focus(T("variable-host-button"));
  await sleep(150);
});

state("theme-selector", "Theme selector: the button hovered", { view: "viewport", themes: "all", widths: "all" }, async (c) => {
  await c.open(c.url(""));
  await hover(c.page, T("theme-toggle"));
});

state("theme-selector-open", "Theme selector: the menu open (System, Dark, Light), on a dashboard", { view: "viewport", themes: "all", widths: "all" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.click(T("theme-toggle"));
  await sleep(250);
});

state("theme-selector-cycle", "Theme selector: one choice made (the other theme), on a dashboard", { view: "viewport", widths: "all" }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  const dark = await c.page.eval(`document.documentElement.getAttribute("data-theme") === "dark"`);
  await c.page.click(T("theme-toggle"));
  await c.page.click(T(dark ? "theme-light" : "theme-dark"));
  await c.settle();
});

// ---- print ----------------------------------------------------------------------------------------------------------

state("print-dashboard", "Print media: one dashboard at the width of an A4 page", { themes: "core", widths: [1280] }, async (c) => {
  await c.open(c.url(`d/service-overview?${RANGE}`));
  await c.page.send("Emulation.setEmulatedMedia", { media: "print", features: [{ name: "prefers-reduced-motion", value: "reduce" }, { name: "prefers-color-scheme", value: "light" }] });
  await c.setViewportWidth(794);
  await c.settle();
});

state("print-tour", "Print media: the panel types tour", { themes: "core", widths: [1280] }, async (c) => {
  await c.open(c.url(`d/types-tour?${RANGE}`));
  await c.page.send("Emulation.setEmulatedMedia", { media: "print", features: [{ name: "prefers-reduced-motion", value: "reduce" }, { name: "prefers-color-scheme", value: "light" }] });
  await c.setViewportWidth(794);
  await c.settle();
});

// ---- editor (work server) -------------------------------------------------------------------------------------------

async function enterEdit(c) {
  await c.page.click(T("edit-toggle"));
  await c.page.waitSel(T("edit-bar"));
  await c.page.waitSel('.edit-grid [data-testid^="panel-"][data-state]');
  await c.settle();
}

async function openPanelEditor(c, id) {
  await c.page.click(T(`panel-${id}-menu`));
  await c.page.click(T(`panel-${id}-edit`));
  await c.page.waitSel(T("panel-editor"));
  await c.settle();
}

state("edit-mode", "Edit mode of a dashboard", { themes: "all", widths: "all" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
});

state("edit-mode-tour", "Edit mode of a dashboard with sections", { server: "work" }, async (c) => {
  const d = await c.fresh("types-tour");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
});

state("edit-panel-menu", "Edit mode: the menu of a panel", { view: "viewport" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await c.page.click(T("panel-5-menu"));
  await sleep(250);
});

state("edit-panel-query", "Panel editor: tab Query", { view: "full" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await openPanelEditor(c, 5);
  await c.page.click(T("tab-query"));
  await c.settle();
});

state("edit-panel-options", "Panel editor: tab Panel", { view: "full" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await openPanelEditor(c, 5);
  await c.page.click(T("tab-panel"));
  await c.settle();
});

state("edit-panel-json", "Panel editor: tab JSON", { view: "full" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await openPanelEditor(c, 5);
  await c.page.click(T("tab-json"));
  await c.settle();
});

state("edit-panel-json-error", "Panel editor: tab JSON with an error", { view: "full" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await openPanelEditor(c, 5);
  await c.page.click(T("tab-json"));
  const cur = await c.page.eval(`document.querySelector('[data-testid="panel-json"]').value`);
  await c.page.fill(T("panel-json"), cur.replace('"type": "timeseries"', '"type": "heatmap"'));
  await c.page.waitFor(`!document.querySelector('[data-testid="panel-json-errors"]').hidden`, { timeout: 8000, what: "panel json errors" });
  await c.settle();
});

state("edit-panel-query-error", "Panel editor: Run with a bad query", { view: "full" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await openPanelEditor(c, 5);
  await c.page.click(T("tab-query"));
  await c.page.fill(T("sql-input"), "SELECT broken FROM analytics.demo_metrics");
  await sleep(450);
  await c.page.click(T("run-button"));
  await c.settle();
  await sleep(300);
});

state("edit-json", "Dashboard JSON editor", { view: "full" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await moreItem(c, "json-tab");
  await c.page.waitSel(T("json-text"));
  await c.settle();
});

state("edit-json-error", "Dashboard JSON editor with errors", { view: "full" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await moreItem(c, "json-tab");
  await c.page.waitSel(T("json-text"));
  const cur = JSON.parse(await c.page.eval(`document.querySelector('[data-testid="json-text"]').value`));
  cur.sections[0].panels[0].grid.w = 30;
  cur.sections[0].panels[0].grid.heigth = 8;
  cur.tags = ["a", "a"];
  await c.page.fill(T("json-text"), JSON.stringify(cur, null, 2));
  await c.page.click(T("json-check"));
  await c.page.waitFor(`document.querySelectorAll('[data-testid="json-errors"] li').length >= 2`, { what: "json errors" });
  await c.settle();
});

state("edit-save-dialog", "Dialog: save", { view: "viewport" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await moreItem(c, "add-section");
  await c.page.waitFor(`document.querySelector('[data-testid="save-button"]') && !document.querySelector('[data-testid="save-button"]').disabled`, { what: "save enabled" });
  await c.page.click(T("save-button"));
  await c.page.waitSel(T("save-dialog"));
  await c.page.fill(T("save-message"), "Add a section for the new service");
  await sleep(200);
});

state("edit-conflict", "Conflict screen: the dashboard changed after it was opened", { view: "full" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await moreItem(c, "add-section");
  await c.page.waitFor(`document.querySelector('[data-testid="save-button"]') && !document.querySelector('[data-testid="save-button"]').disabled`, { what: "save enabled" });
  // Another writer saves first, through the API.
  const cur = await c.api.dashboard(d.uid);
  const doc = cur.json.document;
  doc.title = "Changed elsewhere";
  doc.sections[0].panels[0].title = "Requests (changed by a colleague)";
  const r = await c.api.save(d.uid, cur.etag, doc, "Changed by a colleague");
  if (r.status !== 200) throw new Error(`save through the API: ${r.status} ${r.text.slice(0, 200)}`);
  await c.page.click(T("save-button"));
  await c.page.waitSel(T("save-dialog"));
  await c.page.click(T("save-confirm"));
  await c.page.waitSel(T("conflict-screen"));
  await c.settle({ panels: false });
});

state("edit-delete-panel-dialog", "Dialog: delete a panel", { view: "viewport" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await c.page.click(T("panel-5-menu"));
  await c.page.click(T("panel-5-delete"));
  await c.page.waitSel(T("delete-panel-dialog"));
  await sleep(200);
});

state("edit-exit-dialog", "Dialog: leave edit mode with changes", { view: "viewport" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await moreItem(c, "add-section");
  await c.page.waitFor(`document.querySelector('[data-testid="save-button"]') && !document.querySelector('[data-testid="save-button"]').disabled`, { what: "save enabled" });
  await c.page.click(T("edit-toggle"));
  await c.page.waitSel(T("exit-dialog"));
  await sleep(200);
});

state("edit-section-rename", "Edit mode: rename a section", { view: "viewport" }, async (c) => {
  const d = await c.fresh("types-tour");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await c.page.click(T("section-1-rename"));
  await sleep(250);
});

state("delete-dashboard-dialog", "Dialog: delete a dashboard", { view: "viewport" }, async (c) => {
  const d = await c.fresh("service-overview");
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await c.page.click(T("dashboard-menu"));
  await c.page.clickText('[role="menuitem"], .menu button, .menu a', "Delete");
  await c.page.waitSel(T("delete-dashboard-dialog"));
  await sleep(200);
});

async function withHistory(c) {
  const d = await c.fresh("service-overview");
  for (const [i, title] of ["Service overview, first change", "Service overview, second change"].entries()) {
    const cur = await c.api.dashboard(d.uid);
    const doc = cur.json.document;
    doc.title = title;
    doc.sections[0].panels[i].title = `${doc.sections[0].panels[i].title} (v${i + 2})`;
    const r = await c.api.save(d.uid, cur.etag, doc, `Change number ${i + 1}`);
    if (r.status !== 200) throw new Error(`save: ${r.status} ${r.text.slice(0, 200)}`);
  }
  return d;
}

state("history-list", "History list of a dashboard", { view: "full", themes: "all" }, async (c) => {
  const d = await withHistory(c);
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await moreItem(c, "history-button");
  await c.page.waitSel(T("history-list"));
  await c.page.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length >= 3`, { what: "3 versions" });
  await c.settle();
});

state("history-version", "History: one old version with its banner", { view: "full" }, async (c) => {
  const d = await withHistory(c);
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await moreItem(c, "history-button");
  await c.page.waitSel(T("history-list"));
  await c.page.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length >= 3`, { what: "3 versions" });
  await c.page.click(T("history-view-1"));
  await c.page.waitSel(T("version-banner"));
  await c.settle();
});

state("diff-view", "History: the diff of two versions", { view: "full" }, async (c) => {
  const d = await withHistory(c);
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await moreItem(c, "history-button");
  await c.page.waitSel(T("history-list"));
  await c.page.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length >= 3`, { what: "3 versions" });
  await c.page.click('[data-testid="history-list"] tr[data-version="1"] input[type=checkbox]');
  await c.page.click('[data-testid="history-list"] tr[data-version="3"] input[type=checkbox]');
  await c.page.click(T("history-compare"));
  await c.page.waitSel(T("diff-view"));
  await c.settle();
});

state("restore-dialog", "Dialog: restore a version", { view: "viewport" }, async (c) => {
  const d = await withHistory(c);
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await moreItem(c, "history-button");
  await c.page.waitSel(T("history-list"));
  await c.page.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length >= 3`, { what: "3 versions" });
  await c.page.click(T("history-restore-1"));
  await c.page.waitSel(T("restore-dialog"));
  await sleep(200);
});


// ---- editor groups: Thresholds, Colour and Overrides expanded ------------------------------------------------------

/** Open the panel editor of a panel of a fresh copy of a demo dashboard, on the tab Panel. */
async function editorPanelTab(c, base, id) {
  const d = await c.fresh(base);
  await c.open(c.url(`d/${d.uid}?${RANGE}`));
  await enterEdit(c);
  await openPanelEditor(c, id);
  await c.page.click(T("tab-panel"));
  await c.settle();
}

/** Expand the groups of the panel editor whose head contains one of the texts (if they are collapsed). */
async function expandGroups(c, names) {
  for (const name of names) {
    await c.page.eval(`(() => {
      for (const b of document.querySelectorAll('[data-testid="panel-editor"] .ogroup__head')) {
        if (b.textContent.trim().startsWith(${JSON.stringify(name)}) && b.getAttribute("aria-expanded") === "false") b.click();
      }
    })()`);
  }
  await sleep(300);
}

/** Scroll the options column (it scrolls inside its own box) so that the head of a group is at its top. */
async function scrollToGroup(c, name) {
  await c.page.eval(`(() => {
    for (const b of document.querySelectorAll('[data-testid="panel-editor"] .ogroup__head')) {
      if (b.textContent.trim().startsWith(${JSON.stringify(name)})) { b.scrollIntoView({ block: "start" }); return true; }
    }
    return false;
  })()`);
  await sleep(250);
}

state("edit-panel-thresholds", "Panel editor: the group Thresholds expanded", { widths: "all" }, async (c) => {
  await editorPanelTab(c, "types-tour", 11);
  await expandGroups(c, ["Thresholds"]);
  await scrollToGroup(c, "Thresholds");
});

state("edit-panel-colour", "Panel editor: the group Colour expanded", { widths: "all" }, async (c) => {
  await editorPanelTab(c, "types-tour", 1);
  await expandGroups(c, ["Colour"]);
  await scrollToGroup(c, "Colour");
});

state("edit-panel-overrides", "Panel editor: the group Overrides expanded (time series)", { widths: "all" }, async (c) => {
  await editorPanelTab(c, "types-tour", 1);
  await expandGroups(c, ["Overrides"]);
  await scrollToGroup(c, "Overrides");
});

state("edit-panel-overrides-table", "Panel editor: the group Overrides expanded (table with cell types)", { widths: "all" }, async (c) => {
  await editorPanelTab(c, "tables-and-cells", 1);
  await expandGroups(c, ["Overrides"]);
  await scrollToGroup(c, "Overrides");
});

state("edit-panel-groups-open", "Panel editor: Thresholds, Colour and Overrides all expanded", { widths: "all" }, async (c) => {
  await editorPanelTab(c, "types-tour", 1);
  await expandGroups(c, ["Thresholds", "Colour", "Overrides"]);
  await c.shot("");
  await scrollToGroup(c, "Overrides");
  await c.shot("overrides");
});

// ---- more widths: menus, dialogs, toasts, full pages, pickers and the editor at 768 and 390 -------------------------

const ALL_WIDTH_STATES = [
  "list-row-menu", "new-dashboard-dialog", "new-folder-dialog", "import-dialog", "list-invalid-errors", "list-search",
  "panel-menu-open", "dashboard-menu-open", "panel-links-menu", "panel-description",
  "view-data", "view-data-table", "view-panel-table",
  "notice", "notice-error",
  "page-dashboard-404", "page-dashboard-invalid", "page-dashboard-error",
  "time-picker-relative", "time-picker-error", "refresh-picker",
  "variable-multi-open", "variable-custom-open", "variable-single-open",
  "focus-header", "focus-panel-menu", "focus-variable", "theme-selector", "theme-selector-open", "theme-selector-cycle",
  "section-collapsed", "section-expanded", "loading", "loading-progress",
  "tooltip-timeseries", "tooltip-bars", "tooltip-barchart", "tooltip-xychart", "tooltip-pie",
  "edit-panel-menu", "edit-panel-query", "edit-panel-options", "edit-panel-json", "edit-panel-json-error", "edit-panel-query-error",
  "edit-json", "edit-json-error", "edit-save-dialog", "edit-conflict", "edit-delete-panel-dialog", "edit-exit-dialog",
  "edit-section-rename", "edit-mode-tour", "delete-dashboard-dialog",
  "history-version", "diff-view", "restore-dialog",
  "dash-variables-lab-full", "dash-tables-and-cells-full", "dash-edge-states-full", "dash-density-20-full",
];
for (const s of states) if (ALL_WIDTH_STATES.includes(s.id)) s.widths = "all";

export { states, moveMouse };
