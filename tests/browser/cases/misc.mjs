// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B14, B15 (16.10). The case B23 is in authzen.mjs.
import { allTypesDoc } from "../lib/docs.mjs";
import { enterEdit, makeSeed, openPanelEditor, saveDialog, moreItem } from "../lib/ed.mjs";
import { breakFile, sleep, waitPanelsReady } from "../lib/h.mjs";

/** The scripted accessibility audit that runs inside the page. It returns a list of problems. */
const AUDIT = `(() => {
  const problems = [];
  const where = (e) => e.tagName.toLowerCase() + (e.dataset && e.dataset.testid ? "[" + e.dataset.testid + "]" : "") + (e.className && typeof e.className === "string" ? "." + e.className.split(" ")[0] : "");
  const shown = (e) => {
    const s = getComputedStyle(e);
    if (s.visibility === "hidden" || s.display === "none") return false;
    if (e.closest("[hidden], [inert]")) return false;
    const r = e.getBoundingClientRect();
    return r.width > 0 || r.height > 0;
  };
  const byIds = (ids) => ids.split(/\\s+/).map((id) => document.getElementById(id));
  const nameOf = (e) => {
    const al = e.getAttribute("aria-label");
    if (al && al.trim()) return al.trim();
    const lb = e.getAttribute("aria-labelledby");
    if (lb) {
      const t = byIds(lb).filter(Boolean).map((x) => x.textContent.trim()).join(" ");
      if (t) return t;
    }
    if (e.labels && e.labels.length) {
      const t = [...e.labels].map((l) => l.textContent.trim()).join(" ");
      if (t) return t;
    }
    const t = e.textContent.trim();
    if (t) return t;
    if (e.getAttribute("title")) return e.getAttribute("title");
    if (e.tagName === "INPUT" && (e.type === "submit" || e.type === "button") && e.value) return e.value;
    return "";
  };
  // 1. a name for each control and each role that needs one
  const sel = 'button, a[href], input:not([type=hidden]), select, textarea, [role=button], [role=menuitem], [role=option], [role=tab], [role=checkbox], [role=img], [role=meter], [role=region], [role=dialog], dialog[open], [role=listbox], [role=toolbar], [role=group]';
  for (const e of document.querySelectorAll(sel)) {
    if (!shown(e)) continue;
    if (e.matches("[role=group]") && !e.getAttribute("aria-label") && !e.getAttribute("aria-labelledby")) { problems.push("group without a name: " + where(e)); continue; }
    if (!nameOf(e)) problems.push("no accessible name: " + where(e));
  }
  // 2. unique ids
  const seen = new Map();
  for (const e of document.querySelectorAll("[id]")) seen.set(e.id, (seen.get(e.id) || 0) + 1);
  for (const [id, n] of seen) if (n > 1) problems.push("duplicate id " + id + " x" + n);
  // 3. references to ids
  for (const e of document.querySelectorAll("[aria-labelledby], [aria-describedby]")) {
    for (const attr of ["aria-labelledby", "aria-describedby"]) {
      const v = e.getAttribute(attr);
      if (v && byIds(v).some((x) => !x)) problems.push(attr + " points to a missing id: " + where(e) + " " + v);
    }
  }
  for (const l of document.querySelectorAll("label[for]")) if (!document.getElementById(l.htmlFor)) problems.push("label for a missing id: " + l.htmlFor);
  // 4. header cells have a scope
  for (const th of document.querySelectorAll("th")) if (!th.getAttribute("scope")) problems.push("th without scope: " + th.textContent.trim().slice(0, 30));
  // 5. page level
  if (!document.documentElement.lang) problems.push("html has no lang");
  if (!document.title.trim()) problems.push("empty title");
  if (document.querySelectorAll("h1").length !== 1) problems.push("h1 count " + document.querySelectorAll("h1").length);
  if (!document.querySelector("main")) problems.push("no main landmark");
  // 6. images and icons: svg icons are hidden from the tree, img has alt
  for (const i of document.querySelectorAll("img:not([alt])")) problems.push("img without alt");
  for (const s of document.querySelectorAll("svg.icon")) if (s.getAttribute("aria-hidden") !== "true") problems.push("icon not hidden from assistive technology");
  // 7. each row of a form has a control with a name (select, textarea, input)
  // 8. a table of data has a header row
  for (const t of document.querySelectorAll("table")) if (shown(t) && !t.querySelector("th")) problems.push("table without header cell: " + where(t));
  return problems;
})()`;

/** Approximate contrast check (WCAG 1.4.3): text with an opaque resolved background. It returns offenders. */
const CONTRAST = `(() => {
  const parse = (s) => { const m = /rgba?\\(([^)]+)\\)/.exec(s); if (!m) return null; const p = m[1].split(/[ ,\\/]+/).filter(Boolean).map(Number); return { r: p[0], g: p[1], b: p[2], a: p.length > 3 ? p[3] : 1 }; };
  const lum = (c) => { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); }; return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b); };
  const over = (top, bottom) => ({ r: top.r * top.a + bottom.r * (1 - top.a), g: top.g * top.a + bottom.g * (1 - top.a), b: top.b * top.a + bottom.b * (1 - top.a), a: 1 });
  const backdrop = (e) => {
    const layers = [];
    for (let n = e; n; n = n.parentElement) {
      const s = getComputedStyle(n);
      if (s.backgroundImage !== "none") return null;
      const c = parse(s.backgroundColor);
      if (c && c.a > 0) { layers.push(c); if (c.a >= 1) break; }
      if (parseFloat(s.opacity) < 1) return null;
    }
    let acc = { r: 255, g: 255, b: 255, a: 1 };
    if (!layers.length || layers[layers.length - 1].a < 1) { const b = parse(getComputedStyle(document.body).backgroundColor); if (b) acc = over(b, acc); }
    for (let i = layers.length - 1; i >= 0; i--) acc = over(layers[i], acc);
    return acc;
  };
  const out = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const seen = new Set();
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (!n.textContent.trim()) continue;
    const e = n.parentElement;
    if (!e || seen.has(e)) continue;
    seen.add(e);
    // .textLayer holds the transparent copy of the text that a chart draws on its canvas: it exists to select and copy that text, its colour is transparent on purpose and the visible text is on the canvas.
    if (e.closest("canvas, svg, [hidden], .visually-hidden, .textLayer, [disabled], [aria-disabled=true], script, style")) continue;
    const s = getComputedStyle(e);
    if (s.visibility === "hidden" || s.display === "none") continue;
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    const fg = parse(s.color);
    const bg = backdrop(e);
    if (!fg || !bg) continue;
    const f = over(fg, bg);
    const l1 = lum(f), l2 = lum(bg);
    const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
    const px = parseFloat(s.fontSize), bold = parseInt(s.fontWeight, 10) >= 700;
    const need = px >= 24 || (px >= 18.66 && bold) ? 3 : 4.5;
    if (ratio < need) out.push(e.tagName.toLowerCase() + (e.dataset.testid ? "[" + e.dataset.testid + "]" : "") + "." + String(e.className).split(" ")[0] + " " + JSON.stringify(e.textContent.trim().slice(0, 24)) + " " + ratio.toFixed(2) + "<" + need);
  }
  return out;
})()`;

async function audit(t, p, label) {
  const problems = await p.eval(AUDIT);
  t.assert(problems.length === 0, `${label}: ${problems.slice(0, 6).join("; ")}`);
}

async function contrast(t, p, label) {
  const bad = await p.eval(CONTRAST);
  t.assert(bad.length === 0, `${label} contrast: ${bad.slice(0, 5).join("; ")}`);
}

async function sequence(t, theme) {
  const uid = `a11y-${theme}`;
  const seed = await makeSeed(t, uid);
  await t.api.create("transit", allTypesDoc(`a11y-types-${theme}`));
  const p = await t.page();
  t.allow(p, /Failed to load resource/);
  await p.goto(t.url(""));
  await p.eval(`localStorage.setItem("hune.theme", ${JSON.stringify(theme)})`);
  await p.reload();
  await p.idle();
  t.eq(await p.attr("html", "data-theme"), theme, "theme set");
  // the checks detect a fault: a button with no name, a duplicate id, a th without scope, a pale text
  await p.eval(`(() => {
    const d = document.createElement("div");
    d.id = "selftest";
    d.innerHTML = '<button id="dup"></button><span id="dup">x</span><table><tr><th>h</th></tr></table><p style="color:#bbbbbb;background:#ffffff">pale text</p>';
    document.getElementById("app-main").append(d);
  })()`);
  const found = await p.eval(AUDIT);
  t.assert(found.some((x) => /no accessible name/.test(x)) && found.some((x) => /duplicate id dup/.test(x)) && found.some((x) => /th without scope/.test(x)), "the audit finds planted faults: " + JSON.stringify(found));
  t.assert((await p.eval(CONTRAST)).some((x) => /pale text/.test(x)), "the contrast check finds a pale text");
  await p.eval(`document.getElementById("selftest").remove()`);
  // list page
  await audit(t, p, `list ${theme}`);
  await contrast(t, p, `list ${theme}`);
  // the list with a card that has the warning of a file that is not valid
  const restore = await breakFile(t, "stale-one");
  await p.reload();
  await p.idle();
  await p.waitSel(p.tid("card-stale-stale-one"));
  await audit(t, p, `list with a stale card ${theme}`);
  await contrast(t, p, `list with a stale card ${theme}`);
  await restore();
  // dashboard with the seven panel types
  await p.goto(t.dash(`a11y-types-${theme}`));
  await p.idle();
  await waitPanelsReady(p, [1, 2, 3, 4, 5, 6, 7, 8]);
  await audit(t, p, `dashboard ${theme}`);
  await contrast(t, p, `dashboard ${theme}`);
  // each chart has role img and a label of the form "<title>, <type> chart"
  const charts = await p.eval(`[...document.querySelectorAll('[data-testid^="panel-"][data-state] [role=img]')].map((e) => e.getAttribute("aria-label"))`);
  t.assert(charts.length >= 5 && charts.every((l) => /^.+, .+ chart$/.test(l)), "chart labels: " + JSON.stringify(charts));
  // the variable popup and the time picker popup
  await p.click(p.tid("time-picker-button"));
  await audit(t, p, `time picker ${theme}`);
  await contrast(t, p, `time picker ${theme}`);
  await p.press("Escape");
  // View data dialog
  await p.click(p.tid("panel-2-menu"));
  await p.click(p.tid("panel-2-view-data"));
  await p.waitSel(p.tid("view-data-2"));
  await audit(t, p, `view data ${theme}`);
  await p.press("Escape");
  // editor
  await p.goto(t.dash(uid));
  await p.idle();
  await waitPanelsReady(p, [1, 2, 3]);
  await enterEdit(p);
  await audit(t, p, `edit mode ${theme}`);
  await contrast(t, p, `edit mode ${theme}`);
  await openPanelEditor(p, 2);
  await audit(t, p, `panel editor query ${theme}`);
  await contrast(t, p, `panel editor query ${theme}`);
  await p.click(p.tid("tab-panel"));
  await audit(t, p, `panel editor panel ${theme}`);
  await contrast(t, p, `panel editor panel ${theme}`);
  await p.click(p.tid("tab-json"));
  await audit(t, p, `panel editor json ${theme}`);
  await p.click(p.tid("editor-back"));
  await p.waitGone(p.tid("panel-editor"));
  await moreItem(p, "json-tab");
  await p.waitSel(p.tid("json-text"));
  await p.idle();
  await audit(t, p, `dashboard json ${theme}`);
  await contrast(t, p, `dashboard json ${theme}`);
  await p.click(p.tid("editor-back"));
  await p.waitSel(p.tid("edit-bar"));
  // conflict screen: another save first, then a save of a draft
  await moreItem(p, "add-section");
  await p.waitFor(`document.querySelector('[data-testid="save-button"]') && !document.querySelector('[data-testid="save-button"]').disabled`);
  const doc2 = (await t.api.dashboard(uid)).json.document;
  doc2.title = "Changed elsewhere";
  const r = await t.api.save(uid, seed.etag, doc2, "elsewhere");
  t.assert(r.status === 200, "save through the API");
  await p.click(p.tid("save-button"));
  await p.waitSel(p.tid("save-dialog"));
  await audit(t, p, `save dialog ${theme}`);
  await p.click(p.tid("save-confirm"));
  await p.waitSel(p.tid("conflict-screen"));
  await p.idle();
  await audit(t, p, `conflict screen ${theme}`);
  await contrast(t, p, `conflict screen ${theme}`);
  await p.press("Escape");
  await p.waitGone(p.tid("conflict-screen"));
  await p.click(p.tid("discard-button"));
  await p.clickText("dialog button, .dialog button", "Discard");
  // history pages
  await moreItem(p, "history-button");
  await p.waitSel(p.tid("history-list"));
  await p.waitFor(`document.querySelectorAll('[data-testid="history-list"] tbody tr').length >= 2`);
  await audit(t, p, `history list ${theme}`);
  await contrast(t, p, `history list ${theme}`);
  await p.click(p.tid("history-view-1"));
  await p.waitSel(p.tid("version-banner"));
  await p.idle();
  await audit(t, p, `version view ${theme}`);
  await contrast(t, p, `version view ${theme}`);
  await p.clickText(`${p.tid("version-banner")} button`, "Compare with current");
  await p.waitSel(p.tid("diff-view"));
  await p.idle();
  await audit(t, p, `diff view ${theme}`);
  await contrast(t, p, `diff view ${theme}`);
}

export const cases = [
  {
    id: "B14",
    name: "accessibility, scripted checks in both themes (axe-core not run: no network to fetch it)",
    async run(t) {
      await sequence(t, "light");
      await sequence(t, "dark");
      t.note("axe-core (WCAG 2.2 AA rules) is NOT run: the package is not available offline. Names, unique ids, scope on headers, aria references, landmarks, h1 and an approximate text contrast check ran instead.");
    },
  },

  {
    id: "B15",
    name: "no CSP violation and no console error; the CSP header of 9.4 on the page and the API",
    async run(t) {
      const CSP = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; worker-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";
      for (const path of ["", "d/nodes", "api/session", "api/dashboards"]) {
        const res = await fetch(t.url(path));
        t.eq(res.headers.get("content-security-policy"), CSP, `CSP header of ${path || "/"}`);
        t.assert(res.headers.get("x-content-type-options") === "nosniff", `nosniff of ${path || "/"}`);
      }
      const shell = await (await fetch(t.url(""))).text();
      const inline = [...shell.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/g)].filter((m) => !/\bsrc=/.test(m[1]) && !/type="application\/json"/.test(m[1]));
      t.eq(inline.length, 0, "no inline script in the shell");
      // the pages of the suite give no violation and no error
      const p = await t.open(t.url(""));
      await p.goto(t.dash("nodes"));
      await p.idle();
      await waitPanelsReady(p, [1, 2, 3]);
      await moreItem(p, "history-button");
      await p.waitSel(p.tid("history-list"));
      await p.idle();
      await p.goto(t.dash("nodes", "edit=1"));
      await p.waitSel(p.tid("edit-bar"));
      await p.idle();
      await p.goto(t.url("d/does-not-exist"));
      t.allow(p, /Failed to load resource.*404/);
      await p.idle();
      t.eq(p.unexpectedProblems(), [], "console of the pages");
      // the detector works: an inline script is a CSP violation that the page listener logs
      const q = await t.page();
      await q.goto(t.url(""));
      await q.idle();
      await q.eval(`(() => { const s = document.createElement("script"); s.textContent = "window.__x = 1"; document.head.append(s); })()`);
      await sleep(300);
      t.assert(q.problems.some((x) => /CSP violation/.test(x) || /Content Security Policy/.test(x)), "the detector sees a CSP violation: " + JSON.stringify(q.problems));
      t.eq(await q.eval("window.__x === undefined"), true, "the inline script did not run");
      q.problems.length = 0;
      void saveDialog;
    },
  },
];
