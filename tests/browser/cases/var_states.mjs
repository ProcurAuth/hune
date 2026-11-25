// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B290 to B296 (9.10): the states of the control of a query variable, the error, the variables that wait, the failure that blocks
// nothing, the cache of the options, the cancel of a request that leaves with the page, and the chips of the selected values.
// B290: loading shows a spinner and "Loading…", the control is disabled, and the bar and the grid do not move when the options come.
// B291: a failed request: a short sentence by error type (a table that does not exist, the memory limit), Retry, the full text folded, no change of layout.
// B292: a variable below a variable waits for it and sends no request (when the parent fails, and while it loads), and it names its parent.
// B293: a failure blocks nothing: the default stays, the panels run, and the person can type a value in a failed open variable.
// B294: the cache: a refresh within its time sends no request, a new period asks, a return to a known period does not.
// B295: a request of options is cancelled when the person leaves the page, and when a newer request for the variable starts.
// B296: the chips: a click on the text opens the selector, the cross removes a value, All and the last value have no cross, "+n", the keyboard.
import { doc, panel, section } from "../lib/docs.mjs";
import { openVariable, optionQueries, panelQueries, sleep, waitPanelsReady } from "../lib/h.mjs";

const SEED_SQL = ["SELECT DISTINCT node", "FROM analytics.seed", "ORDER BY node"];
const BROKEN_SQL = ["SELECT * FROM analytics.nothing_here"];
// A query that reads a lot and has a low limit of memory: ClickHouse stops it with the code 241 (MEMORY_LIMIT_EXCEEDED).
const MEMORY_SQL = ["SELECT DISTINCT concat(toString(rand()), node) AS v", "FROM analytics.seed ARRAY JOIN range(200000) AS k", "ORDER BY v", "SETTINGS max_memory_usage = 2000000"];
const TIME_SQL = ["SELECT DISTINCT node", "FROM analytics.seed", "WHERE $__timeFilter(event_time)", "ORDER BY node"];

const withMember = (v, o) => {
  // The members follow the canonical order: `default`, then `allow_custom_value`, then `placement` (4.38).
  const { placement, ...rest } = v;
  return { ...rest, ...(o.open ? { allow_custom_value: true } : {}), ...(o.placement ? { placement: o.placement } : placement ? { placement } : {}) };
};
const qv = (name, o = {}) => withMember({ kind: "query", name, label: o.label ?? null, hide: "none", source: "prod", sql: o.sql ?? SEED_SQL, multi: !!o.multi, include_all: !!o.all, all_value: null, refresh: o.refresh ?? "load", sort: "alpha_asc", regex: null, default: o.default ?? [] }, o);
const cv = (name, o = {}) => withMember({ kind: "custom", name, label: o.label ?? null, hide: "none", options: o.options, multi: !!o.multi, include_all: !!o.all, all_value: null, default: o.default ?? [] }, o);
const table = (id, sql) => panel(id, "table", `Rows ${id}`, sql, { grid: { x: 0, y: (id - 1) * 6, w: 24, h: 6 } });
// A list goes in code (7.8): the names with the prefix `*` are lists.
const inList = (names) => ["SELECT count() AS n FROM analytics.seed", `WHERE ${names.map((n) => (n.startsWith("*") ? `node IN (\${${n.slice(1)}})` : `node = '\${${n}}'`)).join(" AND ")}`];

async function ensure(t, document, folder = "transit") {
  const have = await t.api.dashboard(document.uid);
  if (have.status === 200) return;
  const r = await t.api.create(folder, document);
  t.assert(r.status === 201, `create ${document.uid}: ${r.status} ${r.text.slice(0, 400)}`);
}

const sel = (n) => `[data-testid="variable-${n}"]`;
const btn = (n) => `[data-testid="variable-${n}-button"]`;
const urlVars = async (p, n) => new URLSearchParams(await p.eval("location.search")).getAll(`var-${n}`);
const optionNames = (p) => optionQueries(p).map((r) => /variables\/([^/]+)\/options/.exec(r.url)[1]);
// The room of the bar and the place of the first panel: what must not move when the options come.
const metrics = (p) => p.eval(`(() => { const bar = document.querySelector(".dash-toolbar"); const first = document.querySelector('[data-testid="panel-1"]'); return { barH: bar ? Math.round(bar.getBoundingClientRect().height) : 0, panelTop: first ? Math.round(first.getBoundingClientRect().top) : null }; })()`);
const lastBody = (p, id) => {
  const list = panelQueries(p).filter((r) => r.url.endsWith(`/panels/${id}/query`));
  return list.length ? JSON.parse(list[list.length - 1].body) : null;
};
const chipTexts = (p, n) => p.eval(`[...document.querySelectorAll('${sel(n)} .chip:not(.chip--more) .chip__text')].map((e) => e.textContent)`);

export const cases = [
  {
    id: "B290",
    name: "a variable that loads shows a spinner and Loading…, is disabled, and keeps its room: the bar and the grid do not move when the options come (9.10)",
    async run(t) {
      const d = doc("vs290", "Variable loading", [section(null, [table(1, inList(["s", "*m"]))])], { variables: [qv("s", { default: ["v1"] }), qv("m", { multi: true, all: true, default: ["$__all"] })] });
      await ensure(t, d);
      const p = await t.page({ width: 1280, height: 900 });
      const hold = p.holdWhen((r) => /variables\/(s|m)\/options/.test(r.url));
      await p.goto(t.dash("vs290"));
      await p.waitFor(`!!document.querySelector('${btn("s")}') && !!document.querySelector('${btn("m")}')`, { what: "the controls" });
      await sleep(400);
      t.eq(await p.text(`${btn("s")} .select__value`), "Loading…", "the single control shows Loading…");
      t.eq(await p.text(`${btn("m")} .select__value`), "Loading…", "the control with chips shows Loading… too");
      t.eq(await p.eval(`[document.querySelector('${btn("s")}').disabled, document.querySelector('${btn("m")}').disabled]`), [true, true], "both are disabled");
      t.eq(await p.attr(btn("s"), "aria-busy"), "true", "aria-busy");
      t.eq(await p.attr(btn("s"), "aria-label"), "s: Loading…", "the name says it loads");
      t.assert(await p.exists(`${sel("s")} .icon--spin`), "a spinner turns in the single control");
      t.assert(await p.exists(`${sel("m")} .icon--spin`), "a spinner turns in the control with chips");
      const during = await metrics(p);
      hold.release();
      await p.waitFor(`!document.querySelector('${btn("s")}').disabled && !document.querySelector('${btn("m")}').disabled`, { what: "the controls are ready" });
      await waitPanelsReady(p, [1]);
      t.eq(await p.text(`${btn("s")} .select__value`), "v1", "the value is back");
      t.assert(!(await p.exists(`${sel("s")} .icon--spin`)), "no spinner after");
      const after = await metrics(p);
      t.eq(after, during, "the bar and the first panel did not move when the options came");
    },
  },

  {
    id: "B291",
    name: "a failed options request: a short sentence by error type, Retry, the full text folded, and no change of the layout (9.10)",
    async run(t) {
      const d = doc("vs291", "Variable error", [section(null, [table(1, inList(["bad"]))])], {
        variables: [qv("bad", { sql: BROKEN_SQL, default: ["x"] }), qv("mem", { sql: MEMORY_SQL, default: ["m"] }), qv("fine", { default: ["v1"] })],
      });
      await ensure(t, d);
      const p = await t.page({ width: 1280, height: 900 });
      t.allow(p, /Failed to load resource/);
      const hold = p.holdWhen((r) => /variables\/(bad|mem|fine)\/options/.test(r.url));
      await p.goto(t.dash("vs291"));
      await p.waitFor(`!!document.querySelector('${btn("bad")}')`, { what: "the controls" });
      await sleep(400);
      const during = await metrics(p);
      hold.release();
      await p.waitFor(`!!document.querySelector('${sel("bad")} .select__button.is-failed') && !!document.querySelector('${sel("mem")} .select__button.is-failed')`, { what: "both controls failed" });
      await waitPanelsReady(p, [1]);
      t.eq(await metrics(p), during, "the error did not change the room of the bar or the place of the grid");
      t.eq(await p.count(".select__error"), 0, "no error text lies in the bar");
      const label = (n) => p.attr(btn(n), "aria-label");
      t.eq(await label("bad"), "bad: x. Options failed: The query names a table that does not exist.", "ClickHouse code 60");
      t.eq(await label("mem"), "mem: m. Options failed: The query used too much memory.", "ClickHouse code 241");
      t.eq(await p.text(`${btn("bad")} .select__value`), "x", "the value that the control had stays on screen");
      // The popup: the sentence, Retry, and the full text, folded.
      await openVariable(p, "bad");
      await p.waitSel(`${sel("bad")} .select__popup`);
      t.eq(await p.text(`${sel("bad")} .select__fail-text`), "The query names a table that does not exist.", "the short sentence");
      t.assert(await p.exists(p.tid("variable-bad-retry")), "the button Retry");
      t.eq(await p.eval(`document.querySelector('${p.tid("variable-bad-detail")}').open`), false, "the detail is folded");
      t.assert((await p.text(`${p.tid("variable-bad-detail")} .select__detail-text`)).includes("Code: 60."), "the full text has the code of ClickHouse");
      await p.eval(`document.querySelector('${p.tid("variable-bad-detail")} summary').click()`);
      t.eq(await p.eval(`document.querySelector('${p.tid("variable-bad-detail")}').open`), true, "the detail opens");
      t.eq(await metrics(p), during, "the open popup and its detail do not move the page");
      // Retry sends one new request for this variable and closes the popup.
      p.clearRequests();
      await p.click(p.tid("variable-bad-retry"));
      await p.waitFor(`document.querySelector('${sel("bad")} .select__popup') === null`, { what: "the popup closes" });
      await p.waitFor(`!!document.querySelector('${sel("bad")} .select__button.is-failed')`, { what: "the control fails again" });
      t.eq(optionNames(p).filter((n) => n === "bad").length, 1, "Retry sent one request of options for the variable");
      await p.press("Escape");
    },
  },

  {
    id: "B292",
    name: "a variable below a variable waits for it: no request while the parent fails or loads, and the control names its parent (9.10)",
    async run(t) {
      const kidSql = (parent) => ["SELECT DISTINCT node", "FROM analytics.seed", `WHERE '\${${parent}}' = '\${${parent}}'`, "ORDER BY node"];
      const d = doc("vs292", "Variable wait", [section(null, [table(1, inList(["kid", "free"]))])], {
        variables: [qv("parent", { sql: BROKEN_SQL, label: "Parent label", default: ["p"] }), qv("kid", { sql: kidSql("parent"), default: ["v1"] }), qv("free", { default: ["v0"] })],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("vs292"));
      t.allow(p, /Failed to load resource/);
      await waitPanelsReady(p, [1]);
      const names = optionNames(p);
      t.assert(names.includes("parent") && names.includes("free"), `the parent and the free variable ask: ${names}`);
      t.assert(!names.includes("kid"), `the variable below the failed parent sends no request: ${names}`);
      t.eq(await p.text(`${btn("kid")} .select__value`), "waiting for Parent label", "the control names its parent by its label");
      t.eq(await p.eval(`document.querySelector('${btn("kid")}').disabled`), true, "the waiting control is disabled");
      t.eq(await p.attr(btn("kid"), "aria-label"), "kid: waiting for Parent label", "the name says it waits");
      t.eq(await p.eval(`!!document.querySelector('${sel("free")} .select__button.is-failed') || document.querySelector('${btn("free")}').disabled`), false, "the free variable is ready");
      // The panels still run, with the values that the variables have.
      t.eq(lastBody(p, 1).variables.kid, { all: false, values: ["v1"] }, "the waiting variable keeps its default in the request");

      // A good parent that loads slowly: the variable below sends nothing before the answer.
      const good = doc("vs292b", "Variable wait 2", [section(null, [table(1, inList(["gk"]))])], { variables: [qv("gp", { label: "GP", default: ["v0"] }), qv("gk", { sql: kidSql("gp"), default: ["v1"] })] });
      await ensure(t, good);
      const q = await t.page({ width: 1280, height: 900 });
      const hold = q.holdWhen((r) => /variables\/gp\/options/.test(r.url));
      await q.goto(t.dash("vs292b"));
      await q.waitFor(`!!document.querySelector('${btn("gk")}')`, { what: "the controls" });
      await sleep(500);
      t.assert(!optionNames(q).includes("gk"), "no request of the variable below while its parent loads");
      t.eq(await q.text(`${btn("gk")} .select__value`), "waiting for GP", "it says that it waits for the parent");
      hold.release();
      await q.waitFor(`!document.querySelector('${btn("gk")}').disabled`, { what: "the variable below loads after its parent" });
      t.assert(optionNames(q).includes("gk"), "the request of the variable below came after the parent");
    },
  },

  {
    id: "B293",
    name: "a failed options request blocks nothing: the default stays, the panels run, and a failed open variable takes a typed value (9.10)",
    async run(t) {
      const d = doc("vs293", "Variable no block", [section(null, [table(1, inList(["*all", "open"]))])], {
        variables: [qv("all", { sql: BROKEN_SQL, multi: true, all: true, default: ["$__all"] }), qv("open", { sql: BROKEN_SQL, open: true, default: ["v0"] })],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("vs293"));
      t.allow(p, /Failed to load resource/);
      await waitPanelsReady(p, [1]);
      t.eq(await p.text(`${btn("all")} .select__value`), "All", "$__all stays");
      t.eq(await p.text(`${btn("open")} .select__value`), "v0", "the default of the open variable stays");
      t.eq((await urlVars(p, "all")).concat(await urlVars(p, "open")), [], "the page writes no value that is the default");
      t.eq(lastBody(p, 1).variables.open, { all: false, values: ["v0"] }, "the panel runs with the default of the failed variable");
      // The person types a value in the failed open variable.
      await openVariable(p, "open");
      await p.waitSel(`${sel("open")} .select__popup`);
      t.assert(await p.exists(p.tid("variable-open-fail")), "the popup shows the failure");
      t.assert(await p.exists(p.tid("variable-open-search")), "and the field to type a value");
      await p.eval(`document.querySelector('${p.tid("variable-open-search")}').focus()`);
      await p.send("Input.insertText", { text: "zzz" });
      await p.press("Enter");
      await p.waitFor(`document.querySelector('${sel("open")} .select__popup') === null`, { what: "the popup closes" });
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-open").join() === "zzz"`, { what: "the URL has the typed value" });
      await sleep(300);
      t.eq(lastBody(p, 1).variables.open, { all: false, values: ["zzz"] }, "the panel runs with the typed value");
    },
  },

  {
    id: "B294",
    name: "the cache of the options: a refresh within its time sends no request, a new period asks, and a return to a known period does not (9.10)",
    async run(t) {
      const d = doc("vs294", "Variable cache", [section(null, [table(1, inList(["top", "tv"]))])], {
        variables: [qv("top", { default: ["v0"], refresh: "load" }), qv("tv", { sql: TIME_SQL, default: ["v0"], refresh: "time_change" })],
      });
      await ensure(t, d);
      const p = await t.open(t.url("d/vs294?from=now-6h&to=now"));
      await waitPanelsReady(p, [1]);
      const first = optionNames(p);
      t.eq([first.filter((n) => n === "top").length, first.filter((n) => n === "tv").length], [1, 1], `one request for each variable at the load: ${first}`);
      p.clearRequests();
      await p.click(p.tid("refresh-now"));
      await p.idle();
      await waitPanelsReady(p, [1]);
      t.eq(optionNames(p), [], "a refresh within the time of the cache sends no request of options");
      p.clearRequests();
      await p.click(p.tid("time-picker-button"));
      await p.clickText(".picker__quick-item", "Last 5 minutes");
      await p.waitFor(`location.search.includes("from=now-5m")`, { what: "the new range" });
      await p.idle();
      t.eq(optionNames(p), ["tv"], "a new period asks for the variable that follows the period, and for it alone");
      p.clearRequests();
      await p.click(p.tid("time-picker-button"));
      await p.clickText(".picker__quick-item", "Last 6 hours");
      await p.waitFor(`document.querySelector('[data-testid="time-picker-button"]').textContent.trim() === "Last 6 hours"`, { what: "the first range again" });
      await p.idle();
      t.eq(optionNames(p), [], "a return to a known period sends no request");
    },
  },

  {
    id: "B295",
    name: "a request of options is cancelled when the person leaves the page, and when a newer request for the variable starts (9.10)",
    async run(t) {
      const d = doc("vs295", "Variable cancel", [section(null, [table(1, inList(["slow"]))])], { variables: [qv("slow", { default: ["v0"], refresh: "time_change" })] });
      await ensure(t, d);
      const p = await t.page({ width: 1280, height: 900 });
      // The page keeps the signal of each request of options, so that the case can read it after the page left.
      await p.send("Page.addScriptToEvaluateOnNewDocument", { source: `(() => { const f = window.fetch.bind(window); window.__optionSignals = []; window.fetch = (u, o) => { if (String(u).includes("/options")) window.__optionSignals.push(o && o.signal); return f(u, o); }; })();` });
      const hold = p.holdWhen((r) => /variables\/slow\/options/.test(r.url));
      await p.goto(t.url("d/vs295?from=now-6h&to=now"));
      await p.waitFor(`window.__optionSignals && window.__optionSignals.length === 1`, { what: "the request starts" });
      t.eq(await p.eval("window.__optionSignals[0].aborted"), false, "the request runs");
      // A refresh starts a newer request for the same variable: the older one is cancelled.
      await p.click(p.tid("refresh-now"));
      await p.waitFor(`window.__optionSignals.length === 2`, { what: "a newer request" });
      t.eq(await p.eval("[window.__optionSignals[0].aborted, window.__optionSignals[1].aborted]"), [true, false], "the older request is cancelled, the newer runs");
      // The person leaves the page (the brand goes to the list): the request that runs is cancelled.
      await p.click("a.brand");
      await p.waitFor(`location.pathname === "/"`, { what: "the list of dashboards" });
      t.eq(await p.eval(`window.__optionSignals.length`), 2, "the page did not load again: the signals are the same");
      await p.waitFor(`window.__optionSignals[1].aborted === true`, { what: "the request is cancelled when the page leaves" });
      hold.release();
    },
  },

  {
    id: "B296",
    name: "the chips of the selected values: a click on the text opens the selector, the cross removes a value, All and the last value have no cross, +n, the keys (9.10)",
    async run(t) {
      const options = Array.from({ length: 12 }, (_, i) => ({ text: `Option number ${i + 1}`, value: `o${i + 1}` }));
      const d = doc("vs296", "Variable chips", [section(null, [table(1, inList(["*m"]))])], {
        variables: [qv("m", { multi: true, all: true, default: ["v0", "v1", "v2"] }), cv("c", { multi: true, options, default: options.map((o) => o.value) }), qv("o", { multi: true, open: true, default: ["v0", "typed"] })],
      });
      await ensure(t, d);
      const p = await t.open(t.dash("vs296"), { width: 1280, height: 900 });
      await waitPanelsReady(p, [1]);
      t.eq(await chipTexts(p, "m"), ["v0", "v1", "v2"], "one chip for each value");
      t.eq(await p.text(`${btn("m")} .select__value`), "v0 + v1 + v2", "the name for assistive technology keeps the text of the selection");
      t.eq(await p.count(`${sel("m")} [data-testid="variable-m-remove"]`), 3, "a cross on each chip of several");
      t.eq(await p.attr(`${sel("m")} [data-testid="variable-m-remove"][data-value="v1"]`, "aria-label"), "Remove v1", "the name of a cross");
      const height = () => p.eval(`Math.round(document.querySelector('${sel("m")} .select__face').getBoundingClientRect().height)`);
      const h0 = await height();
      // A click on the text of a chip opens the selector.
      await p.clickText(`${sel("m")} .chip__text`, "v1");
      await p.waitSel(`${sel("m")} .select__popup`);
      await p.press("Escape");
      await p.waitFor(`document.querySelector('${sel("m")} .select__popup') === null`, { what: "the selector closes" });
      // The cross removes the value at once, and the page applies it.
      p.clearRequests();
      await p.click(`${sel("m")} [data-testid="variable-m-remove"][data-value="v1"]`);
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-m").join() === "v0,v2"`, { what: "the URL has two values" });
      t.eq(await chipTexts(p, "m"), ["v0", "v2"], "the chip went");
      t.eq(await height(), h0, "the control keeps its height");
      await p.idle();
      t.eq(lastBody(p, 1).variables.m, { all: false, values: ["v0", "v2"] }, "the panel asks with the two values");
      // The keys: a cross takes the focus, and Enter removes. The last value has no cross.
      await p.eval(`document.querySelector('${sel("m")} [data-testid="variable-m-remove"][data-value="v0"]').focus()`);
      await p.press("Enter");
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-m").join() === "v2"`, { what: "the URL has one value" });
      t.eq(await p.count(`${sel("m")} [data-testid="variable-m-remove"]`), 0, "the last value has no cross: a selection is never empty");
      // All is one chip with no cross.
      await openVariable(p, "m");
      await p.waitSel(`${sel("m")} .select__popup`);
      await p.click(`${sel("m")} .listbox__item[data-value="$__all"]`);
      // The focus leaves the popup: a change of a multi variable applies then.
      await p.press("Tab");
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-m").join() === "$__all"`, { what: "All in the URL" });
      t.eq(await chipTexts(p, "m"), ["All"], "All is one chip");
      t.eq(await p.count(`${sel("m")} [data-testid="variable-m-remove"]`), 0, "the chip All has no cross");
      // Backspace in the empty field of an open variable removes the last value of the draft.
      await openVariable(p, "o");
      await p.waitSel(`${sel("o")} .select__popup`);
      await p.eval(`document.querySelector('${p.tid("variable-o-search")}').focus()`);
      await p.press("Backspace");
      await p.press("Enter");
      await p.waitFor(`new URLSearchParams(location.search).getAll("var-o").join() === "v0"`, { what: "Backspace removed the last typed value" });
      // Twelve values do not fit in a narrow window: the first chips show and the chip +n opens the selector.
      const narrow = await t.open(t.dash("vs296"), { width: 390, height: 800 });
      await waitPanelsReady(narrow, [1]);
      await narrow.waitFor(`!!document.querySelector('${sel("c")} .chip--more')`, { what: "the chip +n" });
      const more = await narrow.eval(`(() => { const chips = [...document.querySelectorAll('${sel("c")} .chip:not(.chip--more)')]; const shown = chips.filter((c) => !c.hidden).length; const m = document.querySelector('${sel("c")} .chip--more'); return { shown, more: m.textContent, total: chips.length, moreRight: Math.round(m.getBoundingClientRect().right), chipsRight: Math.round(document.querySelector('${sel("c")} .select__chips').getBoundingClientRect().right), faceRight: Math.round(document.querySelector('${sel("c")} .select__face').getBoundingClientRect().right) }; })()`);
      t.assert(more.shown >= 1 && more.shown < more.total, `some chips show: ${JSON.stringify(more)}`);
      t.eq(more.more, `+${more.total - more.shown}`, `the chip +n counts the hidden chips: ${JSON.stringify(more)}`);
      t.assert(more.moreRight <= more.chipsRight + 1, `the chip +n is inside the room of the chips: ${JSON.stringify(more)}`);
      await narrow.click(`${sel("c")} .chip--more`);
      await narrow.waitSel(`${sel("c")} .select__popup`);
      await narrow.press("Escape");
    },
  },
];
