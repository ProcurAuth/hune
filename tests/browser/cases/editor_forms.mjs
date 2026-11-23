// SPDX-License-Identifier: MIT OR Apache-2.0
// Browser cases B66, B67, B68 (16.10): the forms of the panel editor (10.3a) and the settings view (10.3b).
import { draftOf, enterEdit, makeSeed, openPanelEditor, waitDraft, waitNotice, moreItem } from "../lib/ed.mjs";
import { sleep, waitPanelsReady } from "../lib/h.mjs";

const T = 800; // a text field applies 300 ms after the last key, and the draft reaches localStorage 500 ms after a change

const panelOf = async (p, uid, id) => {
  const d = await draftOf(p, uid);
  for (const s of d.document.sections) for (const x of s.panels) if (x.id === id) return x;
  return null;
};

async function openGroup(p, key) {
  const open = await p.eval(`document.querySelector('[data-group="${key}"]').getAttribute("aria-expanded") === "true"`);
  if (!open) await p.click(`[data-group="${key}"]`);
}

/** Wait until the draft panel satisfies `pred`. */
async function waitPanel(p, uid, id, pred, what) {
  await p.waitFor(`(() => { try { const r = JSON.parse(localStorage.getItem("hune.draft.v1.${uid}")); for (const s of r.document.sections) for (const x of s.panels) if (x.id === ${id}) return (${pred.toString()})(x); } catch (e) {} return false; })()`, { what });
}

async function saveFrom(p, button) {
  await p.click(p.tid(button));
  await p.waitSel(p.tid("save-dialog"));
  await p.click(p.tid("save-confirm"));
  await p.waitFor(`document.querySelector('[data-testid="save-dialog"]') === null`, { what: "save dialog closed" });
}

/** Open the popover "Query options" of the tab "Query" (10.3): the fields "Relative time" and "Time shift" are in it. */
async function openQueryOptions(p) {
  const open = await p.eval(`document.querySelector('[data-testid="query-options"]').getAttribute("aria-expanded") === "true"`);
  if (!open) await p.click(p.tid("query-options"));
}

const ariaDisabled = (p, sel) => p.eval(`document.querySelector(${JSON.stringify(sel)}).getAttribute("aria-disabled") === "true"`);

export const cases = [
  {
    id: "B66",
    name: "override form: each matcher kind, a property of each control type, the limit of the properties, the error of a bad pattern, a save",
    async run(t) {
      const uid = "ed66";
      await makeSeed(t, uid);
      const p = await t.open(t.dash(uid));
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      await openPanelEditor(p, 1);
      await openGroup(p, "overrides");
      const ov = (n, name) => p.tid(`override-${n}-${name}`);
      const prop = (n, id) => p.tid(`override-${n}-property-${id}-value`);
      await p.click(p.tid("override-add"));
      await p.waitSel(p.tid("override-0"));
      await waitDraft(p, uid, (d) => d.document.sections[0].panels[0].fields.overrides.length === 1);
      const x = await panelOf(p, uid, 1);
      t.eq(x.fields.overrides[0].match.by, "name", "a new override matches a name");
      t.assert(Object.keys(x.fields.overrides[0].set).length === 1, "a new override has one property");
      // the matcher kind "name"
      await p.fill(ov(0, "value"), "Weight");
      await waitPanel(p, uid, 1, (q) => q.fields.overrides[0].match.value === "Weight", "name written");
      // the matcher kind "regex": a bad pattern shows the error and the draft keeps the last good value
      await p.select(ov(0, "by"), "regex");
      await p.waitSel(ov(0, "value"));
      await p.fill(ov(0, "value"), "/^St/i");
      await waitPanel(p, uid, 1, (q) => q.fields.overrides[0].match.by === "regex" && q.fields.overrides[0].match.value === "/^St/i", "regex written");
      await p.fill(ov(0, "value"), "(?=x)");
      await sleep(T);
      t.assert(/The pattern is not valid\./.test(await p.eval(`document.querySelector('[data-testid="override-0"]').textContent`)), "the error of a bad pattern");
      t.eq(await p.eval(`document.querySelector('[data-testid="override-0-value"]').getAttribute("aria-invalid")`), "true", "aria-invalid");
      t.eq((await panelOf(p, uid, 1)).fields.overrides[0].match.value, "/^St/i", "the draft keeps the last valid pattern");
      await p.fill(ov(0, "value"), "/^Weight/");
      // the matcher kind "names"
      await p.select(ov(0, "by"), "names");
      await p.waitSel(p.tid("override-0-name-add"));
      await p.click(p.tid("override-0-name-add"));
      await p.select(ov(0, "mode"), "exclude");
      await waitPanel(p, uid, 1, (q) => q.fields.overrides[0].match.by === "names" && q.fields.overrides[0].match.names.length === 2 && q.fields.overrides[0].match.mode === "exclude", "names written");
      t.eq(Object.keys((await panelOf(p, uid, 1)).fields.overrides[0].match), ["by", "names", "mode"], "the matcher has by, names and mode");
      // the last name stays: Remove is not active when one name is left
      await p.click(p.tid("override-0-name-1-remove"));
      await waitPanel(p, uid, 1, (q) => q.fields.overrides[0].match.names.length === 1, "one name left");
      t.eq(await ariaDisabled(p, p.tid("override-0-name-0-remove")), true, "Remove of the last name is not active");
      await p.select(ov(0, "by"), "name");
      await p.waitSel(ov(0, "value"));
      // properties
      const add = async (id) => {
        await p.select(p.tid("override-0-property-add"), id);
        await p.waitSel(p.tid(`override-0-property-${id}`));
      };
      for (const id of ["axis_placement", "soft_max", "draw", "line_dash", "line_width", "fill_opacity", "show_points", "point_size", "transform", "axis_label", "color", "hide_from", "decimals"]) await add(id);
      t.assert(!(await p.eval(`!!document.querySelector('[data-testid="override-0-property-add"] option[value="no_value"]')`)), "no_value is not in the list");
      t.assert(!(await p.eval(`!!document.querySelector('[data-testid="override-0-property-add"] option[value="width"]')`)), "a table property is not in the list for a time series");
      await p.select(prop(0, "axis_placement"), "right");
      await p.fill(prop(0, "soft_max"), "100");
      await p.select(prop(0, "draw"), "bars");
      await p.select(prop(0, "line_dash"), "dash");
      await p.fill(prop(0, "line_width"), "3");
      await p.fill(prop(0, "fill_opacity"), "30");
      await p.select(prop(0, "show_points"), "never");
      await p.fill(prop(0, "point_size"), "4");
      await p.select(prop(0, "transform"), "negative_y");
      await p.fill(prop(0, "axis_label"), "Latency");
      await p.fill(prop(0, "decimals"), "1");
      await p.select(p.tid("override-0-property-color-value-mode"), "continuous_blues");
      await p.click(p.tid("override-0-property-hide_from-value-tooltip"));
      await sleep(T);
      const set = (await panelOf(p, uid, 1)).fields.overrides[0].set;
      t.eq([set.axis_placement, set.soft_max, set.draw, set.line_dash, set.line_width, set.fill_opacity, set.show_points, set.point_size, set.transform, set.axis_label, set.decimals],
        ["right", 100, "bars", "dash", 3, 30, "never", 4, "negative_y", "Latency", 1], "the values of the controls");
      t.eq(set.color, { mode: "continuous_blues", fixed: null }, "a continuous colour mode has fixed null");
      t.eq(set.hide_from, { legend: false, tooltip: true, viz: true }, "hide_from");
      // an integer out of range gives an error and is not written
      await p.fill(prop(0, "line_width"), "11");
      await sleep(T);
      t.eq(await p.eval(`document.querySelector('[data-testid="override-0-property-line_width-value"]').getAttribute("aria-invalid")`), "true", "line_width 11 is invalid");
      t.eq((await panelOf(p, uid, 1)).fields.overrides[0].set.line_width, 3, "line_width stays 3");
      await p.fill(prop(0, "line_width"), "3");
      // the order of the members is the canonical order
      t.eq(Object.keys((await panelOf(p, uid, 1)).fields.overrides[0].set),
        ["unit", "decimals", "color", "hide_from", "axis_placement", "axis_label", "soft_max", "draw", "line_width", "fill_opacity", "line_dash", "show_points", "point_size", "transform"], "the members are in the canonical order");
      // the JSON tab agrees with the form
      await p.click(p.tid("tab-json"));
      const pj = JSON.parse(await p.eval(`document.querySelector('[data-testid="panel-json"]').value`));
      t.eq(pj.fields.overrides[0].set.axis_placement, "right", "the JSON tab shows the form");
      await p.click(p.tid("tab-query"));
      // a property goes away: Remove, and the last property cannot go
      await p.click(p.tid("override-0-property-point_size-remove"));
      await waitPanel(p, uid, 1, (q) => q.fields.overrides[0].set.point_size === undefined, "property removed");
      // a second override with the limit of one property
      await p.click(p.tid("override-add"));
      await p.waitSel(p.tid("override-1"));
      t.eq(await ariaDisabled(p, p.tid("override-1-property-unit-remove")), true, "the only property of an override cannot go");
      // move and remove an override
      await p.click(p.tid("override-1-up"));
      await waitPanel(p, uid, 1, (q) => q.fields.overrides[1].set.axis_placement === "right", "override moved up");
      await p.click(p.tid("override-0-remove"));
      await waitPanel(p, uid, 1, (q) => q.fields.overrides.length === 1, "override removed");
      // the time override of the query: a bad text gives the error of 4.15 and is not written; an empty text removes the member
      await openQueryOptions(p);
      await p.fill(p.tid("query-time-from"), "6h");
      await sleep(T);
      t.eq(await p.eval(`document.querySelector('[data-testid="query-time-from"]').getAttribute("aria-invalid")`), "true", "6h is not a time_from");
      t.assert(!("time_from" in (await panelOf(p, uid, 1)).data.query), "a bad time_from is not written");
      await p.fill(p.tid("query-time-from"), "now-24h");
      await p.fill(p.tid("query-time-shift"), "1d/d");
      await sleep(T);
      t.eq(await p.eval(`document.querySelector('[data-testid="query-time-shift"]').getAttribute("aria-invalid")`), "true", "1d/d is not a time_shift");
      await p.fill(p.tid("query-time-shift"), "1d");
      await p.click(p.tid("query-hide-time"));
      await waitPanel(p, uid, 1, (q) => q.data.query.time_from === "now-24h" && q.data.query.time_shift === "1d" && q.data.query.hide_time_override === true, "time override written");
      t.eq(Object.keys((await panelOf(p, uid, 1)).data.query).slice(-3), ["time_from", "time_shift", "hide_time_override"], "canonical order");
      await p.click(p.tid("query-hide-time"));
      await p.fill(p.tid("query-time-shift"), "");
      await waitPanel(p, uid, 1, (q) => !("hide_time_override" in q.data.query) && !("time_shift" in q.data.query), "members removed");
      // save: the document has the override
      await p.click(p.tid("override-add"));
      await waitPanel(p, uid, 1, (q) => q.fields.overrides.length === 2, "second override");
      await saveFrom(p, "save-button");
      await p.waitFor(`document.querySelector('[data-testid="panel-editor"]') === null`, { what: "editor closed after save" });
      const doc = (await t.api.dashboard(uid)).json.document;
      const saved = doc.sections[0].panels.find((q) => q.id === 1).fields.overrides;
      t.eq(saved.length, 2, "two overrides saved");
      t.eq(saved[1].match.by, "name", "the saved matcher");
      const q = doc.sections[0].panels.find((x) => x.id === 1).data.query;
      t.eq([q.time_from, "time_shift" in q, "hide_time_override" in q], ["now-24h", false, false], "the saved time override");
    },
  },

  {
    id: "B67",
    name: "forms of the mappings, the data links, the panel links and the transformations: add each kind, move, remove, the error of a URL, a save",
    async run(t) {
      const uid = "ed67";
      await makeSeed(t, uid);
      const p = await t.open(t.dash(uid));
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      await openPanelEditor(p, 1);
      // mappings: the four kinds
      await openGroup(p, "mappings");
      for (const kind of ["value", "range", "regex", "special"]) {
        await p.select(p.tid("mapping-add-kind"), kind);
        await p.click(p.tid("mapping-add"));
      }
      await waitPanel(p, uid, 1, (q) => (q.fields.defaults.mappings || []).length === 4, "four mappings");
      let m = (await panelOf(p, uid, 1)).fields.defaults.mappings;
      t.eq(m.map((x) => x.kind), ["value", "range", "regex", "special"], "the four kinds");
      await p.fill(p.tid("mapping-0-text"), "");
      await sleep(T);
      t.assert(/A mapping needs a text or a colour\./.test(await p.eval(`document.querySelector('[data-testid="mapping-0"]').textContent`)), "a mapping with no text and no colour gives its error");
      await p.fill(p.tid("mapping-0-text"), "One");
      await p.select(p.tid("mapping-0-color"), "red");
      await p.fill(p.tid("mapping-1-from"), "5");
      await p.fill(p.tid("mapping-2-pattern"), "(?=x)");
      await sleep(T);
      t.assert(/The pattern is not valid\./.test(await p.eval(`document.querySelector('[data-testid="mapping-2"]').textContent`)), "the error of a bad pattern in a mapping");
      await p.fill(p.tid("mapping-2-pattern"), "^a+$");
      await p.select(p.tid("mapping-3-match"), "nan");
      await p.click(p.tid("mapping-0-down"));
      await waitPanel(p, uid, 1, (q) => q.fields.defaults.mappings[1].kind === "value" && q.fields.defaults.mappings[1].color === "red", "mapping moved");
      await p.click(p.tid("mapping-0-remove"));
      await waitPanel(p, uid, 1, (q) => q.fields.defaults.mappings.length === 3, "mapping removed");
      m = (await panelOf(p, uid, 1)).fields.defaults.mappings;
      t.eq(m.map((x) => x.kind), ["value", "regex", "special"], "three mappings left");
      t.eq(m.find((x) => x.kind === "special").match, "nan", "special value");
      t.eq(m.find((x) => x.kind === "regex").pattern, "^a+$", "regex value");
      // data links and panel links
      await openGroup(p, "datalinks");
      await openGroup(p, "panellinks");
      await p.click(p.tid("link-defaults-add"));
      await p.click(p.tid("link-panel-add"));
      await p.fill(p.tid("link-defaults-0-url"), "//x");
      await sleep(T);
      t.assert(/The URL must start with \/, \?, #, http:\/\/ or https:\/\//.test(await p.eval(`document.querySelector('[data-testid="link-defaults-0"]').textContent`)), "the error of the URL //x");
      await p.fill(p.tid("link-defaults-0-url"), "https://x.example.test/${__value.raw}");
      await p.fill(p.tid("link-defaults-0-title"), "Explorer");
      await p.click(p.tid("link-defaults-0-blank"));
      await p.fill(p.tid("link-panel-0-url"), "/dashboards/d/other");
      await p.click(p.tid("link-defaults-add"));
      await p.click(p.tid("link-defaults-1-up"));
      await waitPanel(p, uid, 1, (q) => (q.fields.defaults.links || []).length === 2 && q.fields.defaults.links[1].title === "Explorer" && q.fields.defaults.links[1].target_blank === true && q.links.length === 1 && q.links[0].url === "/dashboards/d/other", "links written");
      await p.click(p.tid("link-defaults-0-remove"));
      await waitPanel(p, uid, 1, (q) => q.fields.defaults.links.length === 1, "link removed");
      // transformations
      await openGroup(p, "transforms");
      await p.select(p.tid("transform-add-kind"), "filter_by_value");
      await p.click(p.tid("transform-add"));
      await p.select(p.tid("transform-add-kind"), "grouping_to_matrix");
      await p.click(p.tid("transform-add"));
      await waitPanel(p, uid, 1, (q) => q.transforms.length === 2, "two transformations");
      await p.select(p.tid("transform-0-filter-0-kind"), "range");
      await p.fill(p.tid("transform-0-filter-0-matcher-from"), "1");
      await p.fill(p.tid("transform-0-filter-0-matcher-to"), "9");
      await p.click(p.tid("transform-0-filter-add"));
      await p.select(p.tid("transform-0-filter-1-kind"), "is_null");
      await p.select(p.tid("transform-0-mode"), "exclude");
      await p.fill(p.tid("transform-1-column_field"), "c");
      await p.select(p.tid("transform-1-empty"), "null");
      await sleep(T);
      const tr = (await panelOf(p, uid, 1)).transforms;
      t.eq(tr[0].filters[0].matcher, { kind: "range", from: 1, to: 9 }, "range matcher");
      t.eq(tr[0].filters[1].matcher, { kind: "is_null" }, "is_null has no value");
      t.eq([tr[0].mode, tr[1].column_field, tr[1].empty_value], ["exclude", "c", "null"], "transform members");
      await p.click(p.tid("transform-1-up"));
      await waitPanel(p, uid, 1, (q) => q.transforms[0].kind === "grouping_to_matrix", "transformation moved");
      await p.click(p.tid("transform-0-remove"));
      await waitPanel(p, uid, 1, (q) => q.transforms.length === 1 && q.transforms[0].kind === "filter_by_value", "transformation removed");
      // the JSON tab agrees
      await p.click(p.tid("tab-json"));
      const pj = JSON.parse(await p.eval(`document.querySelector('[data-testid="panel-json"]').value`));
      t.eq(pj.transforms.length, 1, "the JSON tab shows the form");
      await p.click(p.tid("tab-query"));
      await saveFrom(p, "save-button");
      await p.waitFor(`document.querySelector('[data-testid="panel-editor"]') === null`, { what: "editor closed after save" });
      const saved = (await t.api.dashboard(uid)).json.document.sections[0].panels.find((q) => q.id === 1);
      t.eq(saved.fields.defaults.mappings.length, 3, "mappings saved");
      t.eq(saved.fields.defaults.links.length, 1, "data link saved");
      t.eq(saved.links[0].url, "/dashboards/d/other", "panel link saved");
      t.eq(saved.transforms.length, 1, "transformation saved");
    },
  },

  {
    id: "B68",
    name: "settings view: title, tags, time and zone, dashboard links, one variable of each kind, a rename and its notice, a save",
    async run(t) {
      const uid = "ed68";
      await makeSeed(t, uid);
      const p = await t.open(t.dash(uid));
      await waitPanelsReady(p, [1, 2, 3]);
      await enterEdit(p);
      await moreItem(p, "dash-settings");
      await p.waitSel(p.tid("settings-view"));
      t.assert((await p.eval(`location.search`)).includes("editview=settings"), "the address has editview=settings");
      const doc = async () => (await draftOf(p, uid)).document;
      // General
      await p.fill(p.tid("settings-title"), "Settings case");
      await p.fill(p.tid("settings-tags-input"), "chain");
      await p.press("Enter");
      await p.fill(p.tid("settings-tags-input"), "chain");
      await p.press("Enter");
      t.assert(/This tag exists\./.test(await p.eval(`document.querySelector('[data-testid="settings-tags"]').textContent`)), "a tag that exists gives the error");
      await p.select(p.tid("settings-cursor-sync"), "crosshair");
      await waitDraft(p, uid, (d) => d.document.title === "Settings case" && d.document.tags.join() === "chain" && d.document.cursor_sync === "crosshair");
      // Time
      await p.click(p.tid("settings-tab-time"));
      await p.fill(p.tid("settings-time-from"), "now-24h");
      await p.select(p.tid("settings-zone"), "other");
      await p.fill(p.tid("settings-zone-name"), "Europe/");
      await sleep(T);
      t.assert(/The time zone is not valid\./.test(await p.eval(`document.querySelector('[data-testid="settings-view"]').textContent`)), "a bad zone name gives the error");
      await p.fill(p.tid("settings-zone-name"), "Asia/Tokyo");
      await p.select(p.tid("settings-refresh"), "30s");
      await waitDraft(p, uid, (d) => d.document.time.from === "now-24h" && d.document.time.timezone === "Asia/Tokyo" && d.document.time.refresh === "30s");
      await p.fill(p.tid("settings-zone-name"), "Mars/Olympus");
      await sleep(T);
      t.assert(/This browser does not know this time zone\./.test(await p.eval(`document.querySelector('[data-testid="settings-zone-warning"]').textContent`)), "an unknown name gives the warning");
      t.eq((await doc()).time.timezone, "Mars/Olympus", "the draft keeps the name");
      await p.fill(p.tid("settings-zone-name"), "Asia/Tokyo");
      // Links
      await p.click(p.tid("settings-tab-links"));
      await p.select(p.tid("settings-link-add-kind"), "link");
      await p.click(p.tid("settings-link-add"));
      await p.fill(p.tid("settings-link-0-url"), "https://wiki.example.test/runbook");
      await p.fill(p.tid("settings-link-0-title"), "Runbook");
      await p.click(p.tid("settings-link-0-keep_time"));
      await p.select(p.tid("settings-link-add-kind"), "dashboards");
      await p.click(p.tid("settings-link-add"));
      await p.fill(p.tid("settings-link-1-tags-input"), "chain");
      await p.press("Enter");
      await p.click(p.tid("settings-link-1-as_dropdown"));
      await sleep(T);
      t.assert(/A dropdown needs a title/.test(await p.eval(`document.querySelector('[data-testid="settings-link-1"]').textContent`)), "a dropdown needs a title");
      await p.fill(p.tid("settings-link-1-title"), "Related");
      await waitDraft(p, uid, (d) => (d.document.links || []).length === 2 && d.document.links[1].title === "Related");
      const links = (await doc()).links;
      t.eq(Object.keys(links[0]), ["kind", "title", "url", "target_blank", "include_vars", "keep_time"], "members of a link");
      t.eq(Object.keys(links[1]), ["kind", "title", "tags", "as_dropdown", "target_blank", "include_vars", "keep_time"], "members of a dashboards link");
      t.eq([links[0].keep_time, links[1].as_dropdown, links[1].tags], [true, true, ["chain"]], "values");
      // Variables: one of each kind
      await p.click(p.tid("settings-tab-variables"));
      const before = (await doc()).variables.length;
      for (const kind of ["query", "custom", "text"]) {
        await p.select(p.tid("variable-add-kind"), kind);
        await p.click(p.tid("variable-add"));
        await p.waitSel(p.tid("variable-back"));
        await p.click(p.tid("variable-back"));
      }
      await waitDraft(p, uid, (d) => d.document.variables.length === before + 3);
      const vars = (await doc()).variables;
      t.eq(vars.slice(-3).map((v) => v.kind), ["query", "custom", "text"], "one variable of each kind");
      // Rename a variable that a panel names: the notice lists the panels
      const named = vars.find((v) => v.name === "node");
      t.assert(named, "the seed has the variable node");
      const at = vars.findIndex((v) => v.name === "env");
      await p.click(p.tid(`variable-${at}-open`));
      await p.fill(p.tid("variable-form-name"), "environment");
      await waitNotice(p, /The variable env is now environment\./);
      await waitDraft(p, uid, (d) => d.document.variables.some((v) => v.name === "environment"));
      await p.fill(p.tid("variable-form-name"), "node");
      await sleep(T);
      t.assert(/This name exists\./.test(await p.eval(`document.querySelector('[data-testid="settings-variables"]').textContent`)), "a repeated name gives the error");
      await p.click(p.tid("variable-back"));
      // the tab JSON shows the same document
      await p.click(p.tid("settings-tab-json"));
      await p.waitSel(p.tid("json-text"));
      const txt = JSON.parse(await p.eval(`document.querySelector('[data-testid="json-text"]').value`));
      t.eq(txt.title, "Settings case", "the tab JSON agrees with the forms");
      t.eq(txt.links.length, 2, "links in the tab JSON");
      // save
      await saveFrom(p, "save-button");
      await p.waitFor(`document.querySelector('[data-testid="settings-view"]') === null`, { what: "settings closed after save" });
      const saved = (await t.api.dashboard(uid)).json.document;
      t.eq(saved.format_version, 3, "a document of format 3");
      t.eq([saved.title, saved.time.timezone, saved.links.length], ["Settings case", "Asia/Tokyo", 2], "saved members");
    },
  },
];
