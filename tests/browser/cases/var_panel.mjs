// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B80 to B86 (4.7a, 9.10, 9.26, 10.3b, 10.6): the placement of a variable in the grid.
// B80: the controls of three variables in the grid, the bar with the rest, the same URL and effects as the bar.
// B81: the toolbar row has no height when no variable is left, and nothing moves at the first frame.
// B82: keyboard, accessible names and scripted accessibility checks (axe-core is not run: no network).
// B83: the modes of the page (viewPanel, panel editor, collapsed section, print).
// B84: the editor (place from the variable form, the title, the position, the menu, the keys, the sections, save).
// B85: the layouts at four widths and the zoom levels.
// B86: the format and the server (422 bad_reference and not_permitted; a document with no placement keeps its form).
import { doc, panel, section } from "../lib/docs.mjs";
import { draftOf, enterEdit, moreItem, waitDraft } from "../lib/ed.mjs";
import { panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";

const STAT_SQL = ["SELECT sum(weight) AS total, '${env}' AS env, count() AS n", "FROM analytics.seed", "WHERE node IN (${node})", "  AND weight >= ${min_weight}"];
const SEED_SQL = ["SELECT DISTINCT node", "FROM analytics.seed", "ORDER BY node"];

const at = (section, x, y, w, h, title) => ({ section, x, y, w, h, ...(title === undefined ? {} : { title }) });
const customVar = (name, o = {}) => ({ kind: "custom", name, label: o.label ?? null, hide: o.hide ?? "none", options: [{ text: "Production", value: "prod" }, { text: "Preproduction", value: "preprod" }], multi: false, include_all: false, all_value: null, default: ["prod"], ...(o.placement ? { placement: o.placement } : {}) });
const queryVar = (name, o = {}) => ({ kind: "query", name, label: o.label ?? null, hide: o.hide ?? "none", source: "prod", sql: SEED_SQL, multi: o.multi ?? false, include_all: o.multi ?? false, all_value: null, refresh: "load", sort: "alpha_asc", regex: null, default: o.multi ? ["$__all"] : ["v0"], ...(o.placement ? { placement: o.placement } : {}) });
const textVar = (name, o = {}) => ({ kind: "text", name, label: o.label ?? null, hide: o.hide ?? "none", value_type: o.value_type ?? "text", default: o.default ?? "", ...(o.placement ? { placement: o.placement } : {}) });
const stat = (id, sql, grid) => panel(id, "stat", `Stat ${id}`, sql, { grid });

/** The document of B80, B82 and B83: env, node and min_weight are in the grid; account is in the bar; region has no control. */
function placedDoc(uid, mutate = null) {
  const d = doc(uid, `Placed ${uid}`, [
    section(null, [stat(1, STAT_SQL, { x: 0, y: 2, w: 12, h: 6 })]),
    section("Filters", [stat(2, ["SELECT 2 AS two"], { x: 0, y: 2, w: 12, h: 4 })]),
  ], {
    variables: [
      { ...customVar("env"), placement: at(0, 0, 0, 6, 2) },
      { ...queryVar("node", { label: "Node", hide: "label", multi: true }), placement: at(0, 6, 0, 12, 2) },
      { ...textVar("min_weight", { label: "Minimum weight", value_type: "number", default: "0" }), placement: at(1, 0, 0, 8, 2, "Threshold") },
      textVar("account", { label: "Account" }),
      customVar("region", { hide: "variable" }),
    ],
  });
  if (mutate) mutate(d);
  return d;
}

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

/** Where a hook is: the testid of the frame of the placed control or of the bar that holds it (one entry for each occurrence). */
async function whereIs(p, names) {
  return p.eval(`(() => {
    const out = {};
    for (const n of ${JSON.stringify(names)}) {
      out[n] = [...document.querySelectorAll('[data-testid="variable-' + n + '"]')].map((e) => { const f = e.closest('[data-testid^="placement-"]'); const b = e.closest('[data-testid="variables-bar"]'); return f ? f.dataset.testid : b ? "bar" : "other"; });
    }
    return out;
  })()`);
}

const barNames = (p) => p.eval(`[...document.querySelectorAll('[data-testid="variables-bar"] > .var')].map((e) => e.dataset.testid.replace("variable-", ""))`);
const search = async (p) => new URLSearchParams(await p.eval("location.search"));
const btnText = (p, n) => p.text(`${p.tid(`variable-${n}-button`)} .select__value`);
const lastBody = (p, id) => {
  const list = panelQueries(p).filter((r) => r.url.endsWith(`/panels/${id}/query`));
  return list.length ? JSON.parse(list[list.length - 1].body) : null;
};
const variableOf = (d, name) => d.variables.find((v) => v.name === name);

// The probe of B81 runs in the page from the first moment of the document. At each animation frame it logs the height of the toolbar row
// and the top of the first panel: what a viewer would see.
const PROBE = `(() => {
  const log = (window.__row = { id: Math.random(), frames: [] });
  const frame = () => {
    const main = document.getElementById("app-main");
    if (main) {
      const tb = main.querySelector(".dash-toolbar");
      const first = main.querySelector('[data-testid="panel-1"]');
      log.frames.push({ hold: document.documentElement.hasAttribute("data-hune-hold"), toolbar: tb ? Math.round(tb.getBoundingClientRect().height * 2) / 2 : 0, top: first ? Math.round((first.getBoundingClientRect().top + scrollY) * 2) / 2 : null });
    }
    requestAnimationFrame(frame);
  };
  requestAnimationFrame(frame);
})();`;

async function rowFrames(t, uid, query = "") {
  const p = await t.page({ width: 1280, height: 900 });
  await p.send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
  await p.send("Page.addScriptToEvaluateOnNewDocument", { source: PROBE });
  await p.goto(t.dash(uid, query));
  await p.idle();
  await waitPanelsReady(p, [1], "ready");
  await sleep(400);
  const frames = (await p.eval("window.__row.frames")).filter((f) => !f.hold && f.top !== null);
  return { p, frames };
}

export const cases = [
  {
    id: "B80",
    name: "placed controls: one hook for each variable in its frame, the bar has the rest, a change writes the URL and runs the panel, the number grammar, a reload (4.7a, 9.10)",
    async run(t) {
      await ensure(t, placedDoc("var80"));
      const p = await t.open(t.dash("var80"));
      await waitPanelsReady(p, [1]);
      // Each hook occurs one time and is in its frame; the bar shows `account` only (`region` has hide = "variable").
      t.eq(await whereIs(p, ["env", "node", "min_weight", "account", "region"]), { env: ["placement-env"], node: ["placement-node"], min_weight: ["placement-min_weight"], account: ["bar"], region: [] }, "places of the controls");
      t.eq(await barNames(p), ["account"], "the bar");
      t.eq(await p.eval(`['env', 'node', 'min_weight'].map((n) => document.querySelector('[data-testid="placement-' + n + '"]').dataset.state)`), ["ready", "ready", "ready"], "a placed frame is ready at once");
      // The frame is in the grid of its section, at its place.
      const boxes = await p.eval(`(() => { const g = (id) => document.querySelector('[data-testid="' + id + '"]').style.gridColumn + " | " + document.querySelector('[data-testid="' + id + '"]').style.gridRow; return [g("placement-env"), g("placement-node"), g("placement-min_weight")]; })()`);
      t.eq(boxes, ["1 / span 6 | 1 / span 2", "7 / span 12 | 1 / span 2", "1 / span 8 | 1 / span 2"], "the grid places of the placements");
      t.eq(await p.eval(`document.querySelector('[data-testid="section-1"]').contains(document.querySelector('[data-testid="placement-min_weight"]'))`), true, "min_weight is in its section");
      t.eq(await p.text('[data-testid="placement-min_weight"] .panel__title'), "Threshold", "the title of the placement");
      t.eq(panelQueries(p).filter((r) => !/\/panels\/[12]\/query$/.test(r.url)).length, 0, "no other query than the two panels");
      t.eq(lastBody(p, 1).variables.env.values, ["prod"], "first request: env");
      // A change of env in its frame: the URL and the stat.
      p.clearRequests();
      await p.click(p.tid("variable-env-button"));
      await p.click('.listbox__item[data-value="preprod"]');
      await p.waitFor(`new URLSearchParams(location.search).get("var-env") === "preprod"`, { what: "var-env in the URL" });
      await p.idle();
      t.eq(await btnText(p, "env"), "Preproduction", "the select shows the new text");
      t.eq(lastBody(p, 1).variables.env.values, ["preprod"], "the stat runs again with the new value");
      // node (multi, All): select v1 and v2, close by a click outside, as in the bar (B08).
      p.clearRequests();
      await p.click(p.tid("variable-node-button"));
      await p.click('.listbox__item[data-value="v1"]');
      await p.click('.listbox__item[data-value="v2"]');
      t.eq(panelQueries(p).length, 0, "a multi change does not apply before the popup closes");
      await p.click("h1.visually-hidden, #app-main", {});
      await p.idle();
      t.eq(await btnText(p, "node"), "v1 + v2", "texts joined");
      t.eq((await search(p)).getAll("var-node"), ["v1", "v2"], "var-node repeated in the URL");
      t.eq(lastBody(p, 1).variables.node.values, ["v1", "v2"], "the stat runs with the two values");
      // min_weight (number): a bad entry shows the error and writes nothing; a good entry applies.
      p.clearRequests();
      await p.fill(p.tid("variable-min_weight-input"), "12abc");
      await p.press("Enter");
      t.assert(/is not a number/.test(await p.text('[data-testid="placement-min_weight"] .field-error')), "number grammar");
      t.eq(await p.eval(`document.querySelector('[data-testid="variable-min_weight-input"]').getAttribute("aria-invalid")`), "true", "aria-invalid");
      t.eq(panelQueries(p).length, 0, "a bad number runs nothing");
      await p.fill(p.tid("variable-min_weight-input"), "0.25");
      await p.press("Enter");
      await p.idle();
      t.eq((await search(p)).get("var-min_weight"), "0.25", "var-min_weight");
      t.eq(lastBody(p, 1).variables.min_weight.values, ["0.25"], "the stat runs with the number");
      // The bar control works as before.
      await p.fill(p.tid("variable-account-input"), "w1");
      await p.press("Enter");
      await p.waitFor(`new URLSearchParams(location.search).get("var-account") === "w1"`, { what: "var-account" });
      // A reload with var-env in the URL shows the value in the frame.
      const q = await t.open(t.dash("var80", "var-env=preprod&var-min_weight=3"));
      await waitPanelsReady(q, [1]);
      t.eq(await btnText(q, "env"), "Preproduction", "env after the reload");
      t.eq(await q.eval(`document.querySelector('[data-testid="variable-min_weight-input"]').value`), "3", "min_weight after the reload");
      t.eq((await whereIs(q, ["env"])).env, ["placement-env"], "the control is in its frame after the reload");
    },
  },
  {
    id: "B81",
    name: "the toolbar row has no height when no variable is left; nothing moves at the first frame or after the options; with a link the row holds the link only (9.10)",
    async run(t) {
      const g = { x: 0, y: 2, w: 12, h: 4 };
      const SQL1 = ["SELECT 1 AS one"];
      // (a) Every variable is placed or has hide = "variable", and the dashboard has no link: no row.
      const a = doc("var81a", "Row a", [section(null, [stat(1, SQL1, g)])], { variables: [{ ...customVar("env"), placement: at(0, 0, 0, 6, 2) }, { ...queryVar("node"), placement: at(0, 6, 0, 6, 2) }, customVar("region", { hide: "variable" })] });
      await ensure(t, a);
      const ra = await rowFrames(t, "var81a");
      t.assert(ra.frames.length >= 3, `frames of a: ${ra.frames.length}`);
      t.eq(await ra.p.eval(`(() => { const e = document.querySelector(".dash-toolbar"); return e ? [getComputedStyle(e).display, e.getBoundingClientRect().height] : null; })()`), ["none", 0], "the toolbar row has display none and no height");
      t.eq(ra.frames.map((f) => f.toolbar).filter((h) => h !== 0), [], "no frame has a toolbar height");
      t.eq([...new Set(ra.frames.map((f) => f.top))].length, 1, `the first panel never moves: ${JSON.stringify([...new Set(ra.frames.map((f) => f.top))])}`);
      t.eq(await ra.p.count('[data-testid="variables-bar"] > *'), 0, "the bar has no control");
      // (b) With a dashboard link, the row stays and holds the link only; the bar part has zero width.
      const b = doc("var81b", "Row b", [section(null, [stat(1, SQL1, g)])], { variables: [{ ...customVar("env"), placement: at(0, 0, 0, 6, 2) }] });
      b.links = [{ kind: "link", title: "Runbook", url: "https://wiki.example.test/runbook", target_blank: true, include_vars: false, keep_time: false }];
      await ensure(t, b);
      const rb = await rowFrames(t, "var81b");
      const rowB = await rb.p.eval(`(() => {
        const row = document.querySelector(".dash-toolbar");
        const bar = row.querySelector('[data-testid="variables-bar"]');
        const r = row.getBoundingClientRect();
        return { display: getComputedStyle(row).display, height: r.height, barWidth: bar.getBoundingClientRect().width, barDisplay: getComputedStyle(bar).display, links: row.querySelectorAll(".dash-links a, .dash-links button").length };
      })()`);
      t.assert(rowB.display !== "none" && rowB.height > 0, `the row stays for the link: ${JSON.stringify(rowB)}`);
      t.eq([rowB.barWidth, rowB.barDisplay, rowB.links], [0, "none", 1], "the bar part has zero width; one link");
      t.eq([...new Set(rb.frames.map((f) => f.top))].length, 1, "the first panel never moves with a link");
      // (c) Variables with no placement: the bar shows exactly these.
      const c = doc("var81c", "Row c", [section(null, [stat(1, SQL1, g)])], { variables: [{ ...customVar("env"), placement: at(0, 0, 0, 6, 2) }, textVar("account", { label: "Account" }), queryVar("free", { label: "Free" }), customVar("secret", { hide: "variable" })] });
      await ensure(t, c);
      const rc = await rowFrames(t, "var81c");
      t.eq(await barNames(rc.p), ["account", "free"], "the bar shows exactly the variables with no placement and no hide = variable");
      t.eq([...new Set(rc.frames.map((f) => f.top))].length, 1, "the first panel never moves while the options load");
      t.assert((await rc.p.eval(`document.querySelector(".dash-toolbar").getBoundingClientRect().height`)) > 30, "the row has the height of a control");
    },
  },
  {
    id: "B82",
    name: "keyboard and accessibility: the Tab order is the order of the grid, the keys of the popup, Escape, the text on Enter and on blur, the names, the group role, scripted checks in both themes (9.26)",
    async run(t) {
      await ensure(t, placedDoc("var82"));
      const CONTRAST = `
        const parse = (c) => { const m = /rgba?\\(([^)]+)\\)/.exec(c); const v = m[1].split(/[ ,\\/]+/).filter(Boolean).map(Number); return { r: v[0], g: v[1], b: v[2], a: v.length > 3 ? v[3] : 1 }; };
        const lum = ({ r, g, b }) => { const f = (x) => { x /= 255; return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4); }; return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b); };
        const over = (top, bottom) => ({ r: top.r * top.a + bottom.r * (1 - top.a), g: top.g * top.a + bottom.g * (1 - top.a), b: top.b * top.a + bottom.b * (1 - top.a), a: 1 });
        const backdrop = (el) => { const chain = []; for (let e = el; e; e = e.parentElement) chain.push(parse(getComputedStyle(e).backgroundColor)); let c = { r: 255, g: 255, b: 255, a: 1 }; for (const layer of chain.reverse()) c = over(layer, c); return c; };
        const ratio = (fg, bg) => { const a = lum(fg), b = lum(bg); return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05); };`;
      const CHECK = `(() => {${CONTRAST}
        const out = { ids: [], refs: [], contrast: [], names: {}, groups: {} };
        const seen = new Set();
        for (const e of document.querySelectorAll("[id]")) { if (seen.has(e.id)) out.ids.push(e.id); seen.add(e.id); }
        for (const e of document.querySelectorAll("[aria-labelledby], [aria-describedby], [aria-controls]")) for (const a of ["aria-labelledby", "aria-describedby", "aria-controls"]) for (const id of (e.getAttribute(a) || "").split(" ").filter(Boolean)) if (!document.getElementById(id)) out.refs.push(a + " " + id);
        for (const n of ["env", "node", "min_weight"]) {
          const frame = document.querySelector('[data-testid="placement-' + n + '"]');
          out.groups[n] = [frame.getAttribute("role"), frame.getAttribute("aria-label")];
          for (const e of frame.querySelectorAll(".var__label, .select__value, input.input, .panel__title")) {
            const bg = backdrop(e); const fg = over(parse(getComputedStyle(e).color), bg); const r = ratio(fg, bg);
            if (r < 4.5) out.contrast.push(n + " " + e.className + " " + r.toFixed(2));
          }
        }
        for (const [n, sel] of [["env", "[data-testid=variable-env-button]"], ["node", "[data-testid=variable-node-button]"], ["min_weight", "[data-testid=variable-min_weight-input]"]]) {
          const e = document.querySelector(sel);
          out.names[n] = [e.getAttribute("aria-label"), e.getAttribute("aria-labelledby") && document.getElementById(e.getAttribute("aria-labelledby")).textContent];
        }
        return out;
      })()`;
      const RING = `(() => {${CONTRAST}
        const e = document.activeElement; const cs = getComputedStyle(e); const frame = e.closest(".panel");
        const bg = backdrop(frame);
        const ring = cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) > 0 ? over(parse(cs.outlineColor), bg) : parse(cs.boxShadow.match(/rgba?\\([^)]*\\)/)?.[0] || "rgb(0,0,0)");
        return { testid: e.closest(".var")?.dataset.testid, outline: cs.outlineStyle + " " + cs.outlineWidth, ratio: ratio(ring, bg) };
      })()`;
      for (const scheme of ["dark", "light"]) {
        const p = await t.page({ width: 1280, height: 900 });
        await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
        await p.goto(t.dash("var82"));
        await p.idle();
        await waitPanelsReady(p, [1]);
        // 1. The Tab order is the order of the grid: the bar, then env, node (section 0), min_weight (section 1).
        await p.eval("document.activeElement && document.activeElement.blur(); window.scrollTo(0, 0)");
        const order = [];
        for (let i = 0; i < 40 && order.length < 4; i++) {
          await p.press("Tab");
          const at = await p.eval(`(() => { const v = document.activeElement.closest(".var"); return v ? v.dataset.testid.replace("variable-", "") : null; })()`);
          if (at && !order.includes(at)) order.push(at);
        }
        t.eq(order, ["account", "env", "node", "min_weight"], `${scheme}: Tab order`);
        const ringMin = await p.eval(RING);
        t.eq(ringMin.testid, "variable-min_weight", "the focus is on min_weight");
        t.assert(ringMin.ratio >= 3 && ringMin.outline.startsWith("solid"), `${scheme}: focus ring in a frame with a title: ${JSON.stringify(ringMin)}`);
        await p.focus(p.tid("variable-env-button"));
        await p.press("Tab");
        await p.press("Tab", { modifiers: 8 });
        const ringEnv = await p.eval(RING);
        t.assert(ringEnv.ratio >= 3 && ringEnv.outline.startsWith("solid"), `${scheme}: focus ring in a frame: ${JSON.stringify(ringEnv)}`);
        // 2. The keys of the popup: Enter opens, ArrowDown moves, Enter chooses; Escape closes and the focus returns to the button.
        await p.focus(p.tid("variable-env-button"));
        await p.press("Enter");
        await p.waitSel(".select__popup");
        t.eq(await p.eval(`document.activeElement.dataset.testid`), "variable-env-search", "the search field has the focus");
        await p.press("ArrowDown");
        await p.press("ArrowDown");
        await p.press("Enter");
        await p.waitFor(`new URLSearchParams(location.search).get("var-env") === "preprod"`, { what: "preprod by the keys" });
        t.eq(await p.eval(`document.activeElement.dataset.testid`), "variable-env-button", "after a choice the focus is on the button");
        await p.press(" ");
        await p.waitSel(".select__popup");
        await p.press("Escape");
        await p.waitGone(".select__popup");
        t.eq(await p.eval(`document.activeElement.dataset.testid`), "variable-env-button", "after Escape the focus returns to the button");
        t.eq(await btnText(p, "env"), "Preproduction", "Escape changed nothing");
        await p.focus(p.tid("variable-node-button"));
        await p.press("Enter");
        await p.waitSel(".select__popup");
        await p.type(p.tid("variable-node-search"), "v2");
        t.eq(await p.eval(`[...document.querySelectorAll(".listbox__item")].filter((e) => !e.hidden).map((e) => e.textContent)`), ["v2"], "the search keeps v2");
        await p.press("Escape");
        await p.waitGone(".select__popup");
        // 3. A text control applies on Enter and on blur.
        await p.fill(p.tid("variable-min_weight-input"), "7");
        await p.press("Enter");
        await p.waitFor(`new URLSearchParams(location.search).get("var-min_weight") === "7"`, { what: "7 on Enter" });
        await p.fill(p.tid("variable-min_weight-input"), "8");
        await p.press("Tab");
        await p.waitFor(`new URLSearchParams(location.search).get("var-min_weight") === "8"`, { what: "8 on blur" });
        // 4. Names, roles, ids, references, contrast.
        const c = await p.eval(CHECK);
        t.eq(c.ids, [], `${scheme}: duplicate ids`);
        t.eq(c.refs, [], `${scheme}: references that do not resolve`);
        t.eq(c.contrast, [], `${scheme}: texts below 4.5:1`);
        t.eq(c.groups, { env: ["group", "env"], node: ["group", "Node"], min_weight: ["group", "Threshold"] }, "role group and its name: the title, else the label");
        t.assert(/^env: /.test(c.names.env[0]), `select name: ${c.names.env[0]}`);
        t.assert(/^Node: /.test(c.names.node[0]), `hide = label keeps the name: ${c.names.node[0]}`);
        t.eq(c.names.min_weight, [null, "Minimum weight"], "a text field is named by its chip");
        await p.fill(p.tid("variable-min_weight-input"), "x");
        await p.press("Enter");
        t.eq(await p.eval(`document.querySelector('[data-testid="placement-min_weight"] .field-error').getAttribute("role")`), "alert", "the error is an alert");
      }
    },
  },
  {
    id: "B83",
    name: "the modes of the page: viewPanel and the panel editor show every variable in the bar once, a collapsed section keeps its control out of the bar, print (9.8, 9.10)",
    async run(t) {
      await ensure(t, placedDoc("var83"));
      await ensure(t, placedDoc("var83c", (d) => { d.sections[1].collapsed = true; }));
      const ALL4 = ["env", "node", "min_weight", "account"];
      const v = await t.open(t.dash("var83", "viewPanel=1"));
      await v.waitSel(`${v.tid("view-panel")} ${v.tid("panel-1")}`);
      t.eq(await barNames(v), ALL4, "viewPanel: the bar shows every variable");
      t.eq(await whereIs(v, [...ALL4, "region"]), { env: ["bar"], node: ["bar"], min_weight: ["bar"], account: ["bar"], region: [] }, "viewPanel: one control for each variable");
      await v.eval("document.querySelector('[data-testid=\"view-panel\"]').focus(); document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))");
      await v.waitFor(`!new URLSearchParams(location.search).has("viewPanel")`, { what: "no viewPanel" });
      await v.idle();
      t.eq(await barNames(v), ["account"], "after the view: the bar shows account only");
      t.eq((await whereIs(v, ["env"])).env, ["placement-env"], "after the view: env is in its frame");
      // In view mode a placed frame has no menu button.
      t.eq(await v.count('[data-testid$="-menu"][data-testid^="placement-"]'), 0, "no menu button in view mode");
      // Edit mode: the draft grid holds the controls. The panel editor of a panel: the bar shows every variable once.
      const e = await t.open(t.dash("var83"));
      await waitPanelsReady(e, [1]);
      await enterEdit(e);
      t.eq(await whereIs(e, ["env", "node", "min_weight", "account"]), { env: ["placement-env"], node: ["placement-node"], min_weight: ["placement-min_weight"], account: ["bar"] }, "edit grid: the draft grid holds the controls");
      await e.click(e.tid("panel-1-menu"));
      await e.click(e.tid("panel-1-edit"));
      await e.waitSel(e.tid("panel-editor"));
      await e.idle();
      t.eq(await barNames(e), ALL4, "panel editor: the bar shows every variable");
      t.eq(await whereIs(e, ALL4), { env: ["bar"], node: ["bar"], min_weight: ["bar"], account: ["bar"] }, "panel editor: one control for each variable");
      // A collapsed section: no control is visible, and the bar does not show the variable. The open section shows the control.
      const c = await t.open(t.dash("var83c"));
      await waitPanelsReady(c, [1]);
      t.eq(await barNames(c), ["account"], "collapsed: the bar does not show min_weight");
      t.eq(await c.eval(`(() => { const e = document.querySelector('[data-testid="variable-min_weight"]'); return !e || e.offsetParent === null; })()`), true, "collapsed: the control is not visible");
      await c.click('[data-testid="section-1"] .section__toggle');
      await c.waitFor(`(() => { const e = document.querySelector('[data-testid="variable-min_weight"]'); return !!e && e.offsetParent !== null; })()`, { what: "the control shows after the open" });
      t.eq(await barNames(c), ["account"], "open: the bar still has account only");
      t.eq((await whereIs(c, ["min_weight"])).min_weight, ["placement-min_weight"], "open: the control is in its frame");
      // Print: the placed frames are hidden; the line of the values keeps each selection, also of a variable that is in the grid.
      await c.send("Emulation.setEmulatedMedia", { media: "print" });
      const printed = await c.eval(`(() => ({ hidden: [...document.querySelectorAll('.panel--variable')].map((e) => getComputedStyle(e).display), meta: document.querySelector(".dash__print-meta").textContent }))()`);
      t.eq(printed.hidden, ["none", "none", "none"], "print hides the placed frames");
      t.assert(/Minimum weight: 0/.test(printed.meta) && /env: prod/.test(printed.meta) && /Node: All/.test(printed.meta), `the line of the values: ${printed.meta}`);
      await c.send("Emulation.setEmulatedMedia", { media: "" });
    },
  },
  {
    id: "B84",
    name: "the editor: place a variable from its form, the title, the position, the menu, the keys, the sections, the saved document equals the draft (10.3b, 10.6)",
    async run(t) {
      const d = doc("var84", "Editor 84", [section("One", [stat(1, ["SELECT 1 AS one"], { x: 0, y: 0, w: 12, h: 4 })]), section("Two", [stat(2, ["SELECT 2 AS two"], { x: 0, y: 0, w: 12, h: 4 })])], { variables: [customVar("alpha"), textVar("beta", { label: "Beta" }), customVar("hid", { hide: "variable" })] });
      await ensure(t, d);
      const p = await t.open(t.dash("var84"));
      await waitPanelsReady(p, [1]);
      await enterEdit(p);
      t.eq(await barNames(p), ["alpha", "beta"], "both variables are in the bar");
      const record = async (pred) => {
        try {
          return (await waitDraft(p, "var84", (r) => pred(r.document))).document;
        } catch (e) {
          throw new Error(`${e.message} (waiting for ${pred.toString().slice(0, 90)}; the draft has ${JSON.stringify(((await draftOf(p, "var84")) || { document: { variables: [] } }).document.variables.map((v) => v.placement || null))})`);
        }
      };
      const openForm = async (i) => {
        await moreItem(p, "dash-settings");
        await p.waitSel(p.tid("settings-tab-variables"));
        await p.click(p.tid("settings-tab-variables"));
        await p.waitSel(p.tid("variable-list"));
        await p.click(p.tid(`variable-${i}-open`));
        await p.waitSel(p.tid("variable-form-name"));
      };
      // 1. Place alpha from its form: the defaults, the last section, 6 columns, 2 rows.
      await openForm(0);
      t.eq(await p.count(p.tid("variable-form-placement-section")), 0, "no section select before the placement");
      await p.click(`${p.tid("variable-form-placed")}`);
      const d1 = await record((r) => r.variables[0].placement !== undefined);
      t.eq(variableOf(d1, "alpha").placement, at(1, 0, 4, 6, 2), "the new placement: bottom of the last section");
      t.eq(Object.keys(variableOf(d1, "alpha")).at(-1), "placement", "the placement is the last member");
      t.eq(await p.eval(`[...document.querySelector('[data-testid="variable-form-hide"]').options].map((o) => o.value)`), ["none", "label"], "hide does not offer `variable` for a placed variable");
      // The section, the title and a field of the position.
      await p.select(p.tid("variable-form-placement-section"), "0");
      const d2 = await record((r) => r.variables[0].placement.section === 0);
      t.eq(variableOf(d2, "alpha").placement, at(0, 0, 4, 6, 2), "another section: its bottom, same size");
      await p.fill(p.tid("variable-form-placement-title"), "Pick an environment");
      const d3 = await record((r) => r.variables[0].placement.title === "Pick an environment");
      t.eq(variableOf(d3, "alpha").placement.title, "Pick an environment", "the title");
      await p.fill(p.tid("variable-form-placement-w"), "9");
      const d4 = await record((r) => r.variables[0].placement.w === 9);
      t.eq(variableOf(d4, "alpha").placement, at(0, 0, 4, 9, 2, "Pick an environment"), "the width");
      await p.fill(p.tid("variable-form-placement-title"), "");
      const d5 = await record((r) => r.variables[0].placement.title === undefined);
      t.eq("title" in variableOf(d5, "alpha").placement, false, "an empty title removes the member");
      // 2. The grid shows the frame; the bar has beta only; the control is the real one.
      await p.click(p.tid("editor-back"));
      await p.waitSel(p.tid("edit-grid"));
      await p.idle();
      t.eq(await barNames(p), ["beta"], "the bar has beta only");
      t.eq((await whereIs(p, ["alpha"])).alpha, ["placement-alpha"], "alpha is in its frame in the draft grid");
      // A change in the frame acts on the selection of the page.
      await p.click(p.tid("variable-alpha-button"));
      await p.click('.listbox__item[data-value="preprod"]');
      await p.waitFor(`new URLSearchParams(location.search).get("var-alpha") === "preprod"`, { what: "var-alpha" });
      // 3. The menu of the frame, and the keys: a move and a resize change the placement only.
      await p.click(p.tid("placement-alpha-menu"));
      await p.waitSel(p.tid("placement-alpha-menu-popup"));
      t.eq(await p.eval(`[...document.querySelectorAll('[data-testid="placement-alpha-menu-popup"] [role=menuitem]')].map((e) => e.textContent.trim())`), ["Edit the variable", "Put back in the top bar"], "the menu of a placed frame");
      await p.press("Escape");
      const before = variableOf(await record(() => true), "alpha");
      await p.focus(p.tid("placement-alpha-menu"));
      await p.press("ArrowRight", { modifiers: 1 });
      const moved = variableOf(await record((r) => r.variables[0].placement.x === 1), "alpha");
      t.eq({ ...moved, placement: null }, { ...before, placement: null }, "a move changes the placement only");
      t.eq([moved.placement.x, moved.placement.w, moved.placement.section], [1, 9, 0], "x changed, the size did not");
      await p.focus(p.tid("placement-alpha-menu"));
      await p.press("ArrowLeft", { modifiers: 9 });
      const sized = variableOf(await record((r) => r.variables[0].placement.w === 8), "alpha");
      t.eq([sized.placement.x, sized.placement.w, sized.placement.h], [1, 8, 2], "a resize changes the size only");
      // 4. A move of a section changes the index; a delete of a section puts the variable back in the bar.
      await p.click(p.tid("section-0-menu"));
      await p.click(p.tid("section-0-down"));
      t.eq(variableOf(await record((r) => r.sections[1].panels[0].id === 1 || r.variables[0].placement.section === 1), "alpha").placement.section, 1, "the section of the placement follows the move");
      await p.click(p.tid("section-1-menu"));
      await p.click(p.tid("section-1-delete"));
      await p.waitSel(p.tid("delete-section-dialog"));
      await p.clickText(`${p.tid("delete-section-dialog")} button`, "Delete");
      const d6 = await record((r) => r.sections.length === 1);
      t.eq("placement" in variableOf(d6, "alpha"), false, "the placement went with its section");
      t.eq(await barNames(p), ["alpha", "beta"], "alpha is back in the bar at once");
      // 5. Place beta, then put it back with the menu of its frame.
      await openForm(1);
      await p.click(p.tid("variable-form-placed"));
      await record((r) => r.variables[1].placement !== undefined);
      await p.click(p.tid("editor-back"));
      await p.waitSel(p.tid("edit-grid"));
      await p.click(p.tid("placement-beta-menu"));
      await p.click(p.tid("placement-beta-remove"));
      const d7 = await record((r) => r.variables[1].placement === undefined);
      t.eq(d7.variables.map((v) => "placement" in v), [false, false, false], "no placement is left");
      t.eq(await barNames(p), ["alpha", "beta"], "beta is back in the bar");
      // 6. A rename keeps the placement; a save gives a document that equals the draft; a variable with no placement has its earlier form.
      await openForm(0);
      await p.click(p.tid("variable-form-placed"));
      await record((r) => r.variables[0].placement !== undefined);
      await p.fill(p.tid("variable-form-name"), "alpha2");
      const d8 = await record((r) => r.variables[0].name === "alpha2");
      t.eq(variableOf(d8, "alpha2").placement.section, 0, "the placement moves with the variable");
      await p.click(p.tid("editor-back"));
      await p.waitSel(p.tid("edit-grid"));
      await p.click(p.tid("save-button"));
      await p.waitSel(p.tid("save-dialog"));
      await p.click(p.tid("save-confirm"));
      await p.waitFor(`document.querySelector('[data-testid="edit-bar"]') === null`, { what: "saved" });
      const stored = (await t.api.dashboard("var84")).json.document;
      const canon = (x) => (Array.isArray(x) ? x.map(canon) : x && typeof x === "object" ? Object.fromEntries(Object.keys(x).sort().map((k) => [k, canon(x[k])])) : x);
      t.eq(canon(stored), canon(d8), "the saved document equals the draft");
      t.eq("placement" in stored.variables[1], false, "a variable with no placement has no member placement");
      const q = await t.open(t.dash("var84"));
      await waitPanelsReady(q, [2]);
      t.eq(await barNames(q), ["beta"], "after the reload: beta in the bar");
      t.eq((await whereIs(q, ["alpha2"])).alpha2, ["placement-alpha2"], "after the reload: alpha2 in its frame");
    },
  },
  {
    id: "B85",
    name: "the layouts at 1920, 1280, 768 and 390 px and at the zoom levels: the control stays in its box, the chip shows an ellipsis, no horizontal scroll, the flow keeps the order of the grid, h = 1 scrolls (9.19, 9.19a)",
    async run(t) {
      const long = "A very long label of a variable that cannot fit";
      const d = doc("var85", "Layouts 85", [
        section(null, [stat(1, ["SELECT 1 AS one"], { x: 0, y: 4, w: 12, h: 4 })]),
      ], { variables: [
        { ...customVar("env", { label: long }), placement: at(0, 0, 0, 6, 2) },
        { ...queryVar("node", { label: long }), placement: at(0, 6, 0, 3, 2) },
        { ...textVar("weight", { label: "Weight", value_type: "number", default: "1" }), placement: at(0, 9, 0, 5, 2, "With a title") },
        { ...textVar("tiny", { label: "Tiny" }), placement: at(0, 14, 0, 4, 1) },
        { ...textVar("wide", { label: long }), placement: at(0, 0, 2, 24, 2) },
      ] });
      await ensure(t, d);
      const DETECT = `(() => {
        const out = [];
        const de = document.documentElement;
        if (de.scrollWidth > de.clientWidth) out.push("page: horizontal scroll " + (de.scrollWidth - de.clientWidth));
        for (const f of document.querySelectorAll('.panel--variable')) {
          const id = f.dataset.testid;
          const content = f.querySelector(".panel__content");
          const box = content.getBoundingClientRect();
          const h = Number(f.style.getPropertyValue("--panel-h"));
          const r = f.querySelector(".var").getBoundingClientRect();
          if (r.right > box.right + 0.5 || r.left < box.left - 0.5) out.push(id + ": the control leaves its box on the x axis");
          const hasHead = !!f.querySelector(".panel__title");
          if (h >= 2 && !hasHead && (r.bottom > box.bottom + 0.5 || r.top < box.top - 0.5)) out.push(id + ": the control leaves its box on the y axis");
          const excessY = content.scrollHeight - content.clientHeight;
          const excessX = content.scrollWidth - content.clientWidth;
          if (h >= 2 && !hasHead && (excessY > 0.01 || excessX > 0.01)) out.push(id + ": the body scrolls with h = " + h + " (" + excessX.toFixed(2) + "/" + excessY.toFixed(2) + ")");
          const chip = f.querySelector(".var__label");
          if (chip && getComputedStyle(chip).textOverflow !== "ellipsis") out.push(id + ": the chip has no ellipsis");
          if (f.dataset.state !== "ready") out.push(id + ": state " + f.dataset.state);
        }
        return out;
      })()`;
      const found = [];
      const open = async (width, dsf = 1, theme = "dark") => {
        const cssW = Math.round(width / dsf);
        const p = await t.page({ width: cssW, height: Math.round(900 / dsf) });
        await p.send("Emulation.setDeviceMetricsOverride", { width: cssW, height: Math.round(900 / dsf), deviceScaleFactor: dsf, mobile: false });
        await p.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: theme }] });
        await p.goto(t.dash("var85"));
        await p.idle();
        await waitPanelsReady(p, [1]);
        await p.eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");
        return p;
      };
      for (const width of [1920, 1280, 768, 390]) {
        const p = await open(width);
        for (const x of await p.eval(DETECT)) found.push(`${width}: ${x}`);
        if (width >= 1280) {
          const clipped = await p.eval(`(() => { const c = document.querySelector('[data-testid="placement-node"] .var__label'); return c.scrollWidth > c.clientWidth; })()`);
          if (!clipped) found.push(`${width}: the chip of the short frame shows no ellipsis`);
        }
        if (width <= 768) {
          // The flow keeps the order of the grid: y, then x, then id; the order of the DOM is the order of the Tab key.
          const order = await p.eval(`[...document.querySelectorAll('.dash__grid > .panel')].map((e) => e.dataset.testid)`);
          t.eq(order, ["placement-env", "placement-node", "placement-weight", "placement-tiny", "placement-wide", "panel-1"], `${width}: the DOM order is the order of the grid`);
          const tops = await p.eval(`[...document.querySelectorAll('.dash__grid > .panel')].map((e) => Math.round(e.getBoundingClientRect().top))`);
          if (!tops.every((v, i) => i === 0 || v >= tops[i - 1])) found.push(`${width}: the flow does not keep the order of the grid ${tops}`);
          // No floor in the flow: a frame of h = 2 is 68 px high and a frame of h = 1 is 30 px.
          // (From 481 to 768 px the grid has two columns, so a frame of 1 row shares its row with a frame of 2 rows.)
          const hs = await p.eval(`['env', 'tiny'].map((n) => Math.round(document.querySelector('[data-testid="placement-' + n + '"]').getBoundingClientRect().height))`);
          t.eq(hs, width <= 480 ? [68, 30] : [68, 68], `${width}: the height in the flow is the formula only`);
        }
        const tiny = await p.eval(`(() => { const f = document.querySelector('[data-testid="placement-tiny"]'); const c = f.querySelector(".panel__content"); return { state: f.dataset.state, scrolls: c.scrollHeight > c.clientHeight, overflow: getComputedStyle(c).overflowY, err: !!f.querySelector(".panel__error") }; })()`);
        // In the two columns of 481 to 768 px the frame shares its row with a frame of 2 rows, so it is not low there.
        t.eq([tiny.state, tiny.scrolls || (width > 480 && width <= 768), tiny.overflow, tiny.err], ["ready", true, "auto", false], `${width}: h = 1 scrolls and shows no error`);
        await p.close();
      }
      for (const width of [1920, 1280]) {
        for (const dsf of [0.75, 0.9, 1.1, 1.25]) {
          const p = await open(width, dsf, dsf === 0.9 ? "light" : "dark");
          for (const x of await p.eval(DETECT)) found.push(`${width}@${dsf}: ${x}`);
          await p.close();
        }
      }
      t.eq(found, [], "layout problems");
    },
  },
  {
    id: "B86",
    name: "the format and the server: a placement in a section that does not exist or on a hidden variable is refused (422), a variable with no placement keeps its form, the placed form is canonical (4.7a)",
    async run(t) {
      const good = placedDoc("var86");
      await ensure(t, good);
      const got = await t.api.dashboard("var86");
      const stored = got.json.document;
      t.eq(Object.keys(stored.variables[0]).at(-1), "placement", "placement is the last member of a variable");
      t.eq(stored.variables[0].placement, at(0, 0, 0, 6, 2), "the placement as sent");
      t.eq(stored.variables[2].placement, at(1, 0, 0, 8, 2, "Threshold"), "the title of a placement");
      t.eq("placement" in stored.variables[3], false, "a variable with no placement has no member placement");
      const refuse = async (name, mutate, code, pointer) => {
        const bad = structuredClone(good);
        mutate(bad);
        const put = await t.api.save("var86", got.etag, bad);
        t.eq([name, put.status, put.json && put.json.error && put.json.error.code], [name, 422, "invalid_document"], `${name}: PUT is refused`);
        const e0 = put.json.error;
        const errors = e0.errors || (e0.extra && e0.extra.errors) || (e0.details && e0.details.errors) || [];
        t.eq(errors.map((e) => [e.code, e.pointer]), [[code, pointer]], `${name}: the code and the pointer`);
      };
      await refuse("section out of range", (b) => { b.variables[0].placement.section = 5; }, "bad_reference", "/variables/0/placement/section");
      await refuse("hidden variable", (b) => { b.variables[4].placement = at(0, 0, 4, 6, 2); }, "not_permitted", "/variables/4/placement");
      const create = await t.api.create("transit", { ...structuredClone(good), uid: "var86b", variables: good.variables.map((v, i) => (i === 0 ? { ...v, placement: at(7, 0, 0, 6, 2) } : v)) });
      t.eq(create.status, 422, "POST of such a document is refused too");
      t.eq((await t.api.dashboard("var86b")).status, 404, "it is not stored");
      // A document with no placement keeps its form: the same bytes of the variables.
      const plain = placedDoc("var86p", (d) => { for (const v of d.variables) delete v.placement; });
      await ensure(t, plain);
      const back = (await t.api.dashboard("var86p")).json.document;
      t.eq(back.variables.some((v) => "placement" in v), false, "no member placement");
    },
  },
];
