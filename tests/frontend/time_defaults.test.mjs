// SPDX-License-Identifier: MIT OR Apache-2.0
// The default range and the default refresh from the Save dialog (10.9), and the carry of the query over the end of a view (10.2a, rule 7).
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { rangeProblem, refreshText, timeDifferences, withTimeDefaults } from "../../web/js/editor/time_defaults.js";

const read = (path) => readFileSync(new URL(`../../web/${path}`, import.meta.url), "utf8");
const TIME = { from: "now-6h", to: "now", timezone: "browser", refresh: null };
const doc = () => ({ uid: "d", title: "T", time: { ...TIME }, sections: [] });

test("10.9: nothing differs when the current range and refresh are the defaults", () => {
  assert.deepEqual(timeDifferences(TIME, { from: "now-6h", to: "now", refresh: null }), { range: false, refresh: false });
});

test("10.9: a change of the range, of one end of it, or of the refresh is a difference", () => {
  assert.deepEqual(timeDifferences(TIME, { from: "now-24h", to: "now", refresh: null }), { range: true, refresh: false });
  assert.deepEqual(timeDifferences(TIME, { from: "now-6h", to: "now-1h", refresh: null }), { range: true, refresh: false });
  assert.deepEqual(timeDifferences(TIME, { from: "now-6h", to: "now", refresh: "30s" }), { range: false, refresh: true });
  assert.deepEqual(timeDifferences({ ...TIME, refresh: "1m" }, { from: "now-6h", to: "now", refresh: null }), { range: false, refresh: true });
});

test("10.9: the range counts as one thing and the refresh as another", () => {
  const current = { from: "now-24h", to: "now", refresh: "30s" };
  assert.deepEqual(withTimeDefaults(doc(), current, { range: true, refresh: false }).time, { from: "now-24h", to: "now", timezone: "browser", refresh: null });
  assert.deepEqual(withTimeDefaults(doc(), current, { range: false, refresh: true }).time, { from: "now-6h", to: "now", timezone: "browser", refresh: "30s" });
  assert.deepEqual(withTimeDefaults(doc(), current, { range: true, refresh: true }).time, { from: "now-24h", to: "now", timezone: "browser", refresh: "30s" });
});

test("10.9: no box means no change, and the draft is never changed in place", () => {
  const draft = doc();
  const before = JSON.stringify(draft);
  const same = withTimeDefaults(draft, { from: "now-24h", to: "now", refresh: "30s" }, { range: false, refresh: false });
  assert.equal(JSON.stringify(same), before);
  withTimeDefaults(draft, { from: "now-24h", to: "now", refresh: "30s" }, { range: true, refresh: true });
  assert.equal(JSON.stringify(draft), before, "the function works on a copy");
});

test("10.9: only `time.from`, `time.to` and `time.refresh` change; the zone and the other members stay", () => {
  const draft = doc();
  draft.time.timezone = "Europe/Paris";
  draft.title = "Kept";
  const next = withTimeDefaults(draft, { from: "1700000000000", to: "1700003600000", refresh: "5m" }, { range: true, refresh: true });
  assert.equal(next.time.timezone, "Europe/Paris");
  assert.equal(next.title, "Kept");
  assert.equal(next.uid, "d");
});

test("10.9: a range that the settings form refuses is refused here with the same text", () => {
  assert.equal(rangeProblem({ from: "now-6h", to: "now" }), "");
  assert.equal(rangeProblem({ from: "1700000000000", to: "1700003600000" }), "");
  assert.match(rangeProblem({ from: "yesterday", to: "now" }), /Use a time such as now-6h/);
  assert.match(rangeProblem({ from: "now", to: "tomorrow" }), /Use a time such as now-6h/);
  assert.equal(rangeProblem({ from: "1700003600000", to: "1700000000000" }), "From must be before To.");
});

test("10.9: the refresh label is the value, or Off for no refresh", () => {
  assert.equal(refreshText(null), "Off");
  assert.equal(refreshText(undefined), "Off");
  assert.equal(refreshText("30s"), "30s");
});

test("10.9: the Save dialog has the two boxes, off at first, and applies them once through the draft", () => {
  const save = read("js/editor/save.js");
  assert.match(save, /Save the current time range as the default/);
  assert.match(save, /Save the current refresh as the default/);
  assert.match(save, /let applied = false;/);
  assert.match(save, /if \(!applied && \(pick\.range \|\| pick\.refresh\)\)/, "a retry does not apply the choice twice (it keeps the save_id)");
  assert.match(save, /E\.change\(withTimeDefaults\(E\.doc, current, pick\)\)/);
  assert.match(save, /const problem = pick\.range \? rangeProblem\(current\) : "";/);
  assert.match(read("css/editor.css"), /\.save-defaults\[hidden\] \{ display: none; \}/);
});

test("10.9: the button Save is active for a difference of the range or the refresh, and Revert is not", () => {
  const edit = read("js/editor/edit_mode.js");
  assert.match(edit, /this\.listen\(document, "hune-query", \(\) => this\.paintBar\(\)\);/);
  assert.match(edit, /const timeOnly = !dirty && \(time\.range \|\| time\.refresh\);/);
  assert.match(edit, /this\.bar\.saveButton\.disabled = !dirty && !timeOnly;/);
  assert.match(edit, /this\.bar\.discard\.disabled = !dirty;/);
  const page = read("js/dashboard.js");
  assert.match(page, /get currentTime\(\) \{\s+const r = currentRange\(\);\s+return \{ from: r\.fromText, to: r\.toText, refresh \};/);
  assert.match(page, /document\.dispatchEvent\(new Event\("hune-query"\)\);\s+if \(!stillHere\(\)\) return;/, "writeUrl tells the editor about each change");
});

test("10.2a rule 12: the page ends a view with a Back that carries the range, the refresh and the variables", () => {
  const page = read("js/dashboard.js");
  // The only call of history.back() is the carry: the entry that the view started from has the values from before the view.
  assert.equal((page.match(/history\.back\(\)/g) || []).length, 1, "one Back, in popKeepingQuery");
  assert.match(page, /function popKeepingQuery\(\) \{\s+carriedQuery = \{ at: Date\.now\(\), query: queryState\(\) \};\s+window\.history\.back\(\);/);
  assert.equal((page.match(/popKeepingQuery\(\)/g) || []).length, 3, "the definition and the two ends of a view (perform and finishEdit)");
  assert.match(page, /const carried = carriedQuery && Date\.now\(\) - carriedQuery\.at < 3000 \? carriedQuery\.query : null;/, "the carry lasts 3 s, so it never reaches a Back of the person");
  assert.match(page, /s\.from = carried\.from;\s+s\.to = carried\.to;\s+s\.refresh = carried\.refresh;\s+s\.vars = carried\.vars;/);
  assert.match(page, /if \(carried\) writeUrl\(\);/, "the entry that the page reaches gets the values that the page keeps");
});
