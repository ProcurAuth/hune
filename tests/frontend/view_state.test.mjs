// SPDX-License-Identifier: MIT OR Apache-2.0
// Tests of the states of the page (10.2a): the vectors FE-26, the round trip, the normal form and a walk of events on a model of the history.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { EVENTS, PAGES, baseState, isEditorState, kindOf, leaveTarget, nextState, normalSearch, originValue, parseView, readOrigin, renderSearch, sameState, viewNames } from "../../web/js/view_state.js";

const vectors = JSON.parse(readFileSync(join(dirname(fileURLToPath(import.meta.url)), "vectors/FE-26-view-state.json"), "utf8"));
const S = ([page, panel, editing, version, base]) => ({ page, panel, editing, version, base });
const PANELS = vectors.panels;
const CTX = { panels: PANELS, canEdit: true };

test("FE-26 set 1: the state of each URL, its notices and its normal form (rule 1, 3, 4, 8)", () => {
  for (const v of vectors.urls) {
    const got = parseView(v.search, { panels: vectors.panels, canEdit: v.canEdit });
    assert.deepEqual(got.state, S(v.state), v.name);
    assert.deepEqual(got.notices, v.notices, v.name);
    assert.equal(normalSearch(v.search, got.state), v.normal, v.name);
    // The normal form names the same state (the round trip of the invariant).
    assert.deepEqual(parseView(v.normal, { panels: vectors.panels, canEdit: v.canEdit }).state, got.state, `${v.name}: normal form`);
    assert.deepEqual(parseView(v.normal, { panels: vectors.panels, canEdit: v.canEdit }).notices.filter((n) => n.code !== "no-right"), [], `${v.name}: normal form has no notice`);
  }
});

test("FE-26 set 2: the rows of the table of transitions", () => {
  for (const v of vectors.transitions) {
    const origin = v.origin ? { kind: v.origin.kind, state: S(v.origin.state) } : null;
    const got = nextState(S(v.state), v.event, v.arg, { origin, dirty: v.dirty, newPanel: v.newPanel });
    const name = `${v.state[0]} ${v.event}${v.origin ? ` (origin ${v.origin.kind})` : ""}${v.dirty ? " dirty" : ""}`;
    assert.deepEqual(got.state, S(v.next), name);
    assert.equal(got.history, v.history, `${name}: history`);
    assert.equal(got.writeOrigin, v.writeOrigin, `${name}: origin`);
    assert.equal(got.dialog, v.dialog, `${name}: dialog`);
  }
});

test("FE-26 set 3: the target of leave for each origin", () => {
  for (const v of vectors.leave) {
    const origin = v.origin ? { kind: v.origin.kind, state: S(v.origin.state) } : null;
    assert.deepEqual(leaveTarget(S(v.state), origin), S(v.target), `${v.state[0]} ${origin ? origin.kind : "no origin"}`);
  }
});

test("round trip: parse(render(state)) is the state, for the whole state space", () => {
  const states = [baseState()];
  for (const id of PANELS) states.push({ ...baseState(), page: "panel-view", panel: id });
  states.push({ ...baseState(), page: "edit", editing: true });
  for (const id of PANELS) states.push({ ...baseState(), page: "panel-edit", panel: id, editing: true });
  for (const page of ["json", "settings"]) states.push({ ...baseState(), page, editing: true });
  for (const editing of [true, false]) for (const [version, base] of [[null, null], ["3", null], ["current", "2"]]) states.push({ ...baseState(), page: "history", editing, version, base });
  assert.equal(new Set(states.map((s) => s.page)).size, PAGES.length);
  for (const st of states) {
    const query = { from: "now-6h", to: "now", refresh: "1m" };
    const url = renderSearch(st, query);
    const back = parseView(url, CTX);
    assert.deepEqual(back.state, st, url);
    assert.deepEqual(back.notices, []);
    assert.equal(renderSearch(back.state, query), url);
    assert.equal(normalSearch(url, st), url, `${url} is its own normal form`);
    assert.deepEqual(viewNames(st), viewNames(back.state));
  }
});

test("the origin is read from history.state and written in the same form", () => {
  assert.equal(readOrigin(null), null);
  assert.equal(readOrigin({}), null);
  assert.equal(readOrigin({ hune: {} }), null);
  assert.equal(readOrigin({ hune: { from: "" } }), null);
  assert.equal(readOrigin({ hune: { from: 3 } }), null);
  assert.equal(readOrigin(originValue("d/x?edit=1")), "d/x?edit=1");
  assert.equal(kindOf(baseState()), "view");
  assert.equal(kindOf({ ...baseState(), page: "panel-view", panel: 1 }), "view");
  assert.equal(kindOf({ ...baseState(), page: "history" }), "edit");
  assert.equal(isEditorState(baseState()), false);
  assert.equal(isEditorState({ ...baseState(), page: "history" }), true);
});

test("Edit from the view of a copy opens the editor of the stored panel (9.8c, rule 5)", () => {
  for (const panel of ["2-r3", 2]) {
    const pv = { ...baseState(), page: "panel-view", panel };
    const a = nextState(pv, "edit-on", undefined, { origin: null, dirty: false });
    assert.equal(a.state.page, "panel-edit");
    assert.equal(a.state.panel, 2);
    assert.equal(renderSearch(a.state), "?edit=1&editPanel=2");
  }
});

test("the bug: panel view, Edit, leave gives the panel view and the same stack", () => {
  const pv = { ...baseState(), page: "panel-view", panel: 2 };
  const a = nextState(pv, "edit-on", undefined, { origin: null, dirty: false });
  assert.equal(a.state.page, "panel-edit");
  assert.equal(a.state.panel, 2);
  assert.equal(a.history, "push");
  assert.equal(a.writeOrigin, true);
  assert.equal(renderSearch(a.state), "?edit=1&editPanel=2");
  const b = nextState(a.state, "leave", undefined, { origin: { kind: "view", state: pv }, dirty: false });
  assert.deepEqual(b.state, pv);
  assert.equal(b.history, "pop");
  assert.equal(nextState(a.state, "leave", undefined, { origin: { kind: "view", state: pv }, dirty: true }).dialog, true);
  const d = nextState(a.state, "leave", undefined, { origin: null, dirty: true });
  assert.equal(d.state.page, "edit");
  assert.equal(d.history, "replace");
  assert.equal(d.dialog, false);
});

test("an open and a leave leave the stack as it was", () => {
  const E = { ...baseState(), page: "edit", editing: true };
  const V = baseState();
  for (const [ev, arg] of [["panel-edit", 2], ["open-json"], ["open-settings"], ["open-history"]]) {
    const out = nextState(E, ev, arg, { origin: { kind: "view", state: V }, dirty: false });
    assert.equal(out.history, "push");
    assert.equal(out.writeOrigin, true);
    const back = nextState(out.state, "leave", undefined, { origin: { kind: "edit", state: E }, dirty: true });
    assert.equal(back.history, "pop");
    assert.deepEqual(back.state, E);
    assert.equal(back.dialog, false);
  }
});

// A model of the history stack: each entry has a state and the flag of its origin. The walk runs random events and checks, after each of them,
// that the URL of the entry names its state (round trip), that a pop goes to the previous entry, and that the stack does not grow without a push.
function walk(seed, steps) {
  let x = seed >>> 0;
  const rnd = (n) => {
    x = (Math.imul(x, 1664525) + 1013904223) >>> 0;
    return x % n;
  };
  let stack = [{ state: baseState(), from: false }];
  let idx = 0;
  let dirty = false;
  const log = [];
  const cur = () => stack[idx];
  const originOf = () => (cur().from && idx > 0 ? { kind: kindOf(stack[idx - 1].state), state: stack[idx - 1].state } : null);
  const check = (why) => {
    const url = renderSearch(cur().state);
    const back = parseView(url, CTX);
    assert.deepEqual(back.state, cur().state, `${why}: ${url} (seed ${seed}) ${log.join(" > ")}`);
    assert.deepEqual(back.notices, []);
  };
  const eventsOf = EVENTS.filter((e) => e !== "history-nav");
  for (let i = 0; i < steps; i++) {
    const r = rnd(100);
    const st = cur().state;
    let ev;
    if (r < 8) ev = ["back"];
    else if (r < 14) ev = ["forward"];
    else if (r < 17) ev = ["reload"];
    else if (r < 20) ev = ["deep-link", { page: PAGES[rnd(PAGES.length)], panel: PANELS[rnd(PANELS.length)] }];
    else if (r < 24) ev = ["change"];
    else if (r < 28) ev = ["dirty"];
    else if (r < 31) ev = ["history-nav", { version: String(1 + rnd(3)), base: rnd(2) ? String(1 + rnd(3)) : null }];
    else ev = [eventsOf[rnd(eventsOf.length)], PANELS[rnd(PANELS.length)]];
    log.push(ev[0]);
    if (ev[0] === "back") {
      if (idx > 0) idx--;
      check("back");
      continue;
    }
    if (ev[0] === "forward") {
      if (idx < stack.length - 1) idx++;
      check("forward");
      continue;
    }
    if (ev[0] === "reload" || ev[0] === "change") {
      check(ev[0]);
      continue;
    }
    if (ev[0] === "dirty") {
      dirty = !dirty;
      continue;
    }
    if (ev[0] === "deep-link") {
      const p = ev[1];
      const withPanel = p.page === "panel-view" || p.page === "panel-edit";
      const s2 = { page: p.page, panel: withPanel ? p.panel : null, editing: ["edit", "panel-edit", "json", "settings"].includes(p.page), version: null, base: null };
      stack = stack.slice(0, idx + 1);
      stack.push({ state: s2, from: false });
      idx++;
      check("deep-link");
      continue;
    }
    const ctx = { origin: originOf(), dirty, newPanel: rnd(2) === 0 };
    const out = nextState(st, ev[0], ev[1], ctx);
    if (out.history === "none") {
      assert.ok(sameState(out.state, st), `none changes the state on ${ev[0]} (seed ${seed})`);
      continue;
    }
    if (out.dialog && rnd(2) === 0) continue; // The user presses "Stay".
    if (out.history === "push") {
      stack = stack.slice(0, idx + 1);
      stack.push({ state: out.state, from: out.writeOrigin });
      idx++;
    } else if (out.history === "replace") {
      stack[idx] = { state: out.state, from: stack[idx].from };
    } else if (out.history === "pop") {
      assert.ok(idx > 0 && cur().from, `pop without an origin on ${ev[0]} (seed ${seed})`);
      assert.ok(sameState(stack[idx - 1].state, out.state), `the pop target differs: ${JSON.stringify(stack[idx - 1].state)} against ${JSON.stringify(out.state)} on ${ev[0]} (seed ${seed})`);
      idx--;
    }
    if (!out.state.editing && st.editing && ev[0] !== "back") dirty = false;
    check(ev[0]);
    assert.ok(sameState(cur().state, out.state));
  }
}

test("a walk of 2000 events on a model of the history keeps the URL and the state equal (40 seeds)", () => {
  for (let seed = 1; seed <= 40; seed++) walk(seed, 2000);
});

// F23 (4.13a, 9.8c): the key of a copy in `viewPanel`, and rule 16 of 10.2a.
test("a viewPanel with the key of a copy is a state when the panel repeats and n is up to the limit", () => {
  // The plan holds the copies a, b and c of the panel 2. The form of Grafana names a copy by its value (rule 1), so `3$panel-2` names no copy.
  const plan = { instances: new Map([[1, { id: 1, n: null, value: null }], [2, { id: 2, n: 1, value: "a", key: 2 }], ["2-r2", { id: 2, n: 2, value: "b", key: "2-r2" }], ["2-r3", { id: 2, n: 3, value: "c", key: "2-r3" }]]) };
  const ctx = { canEdit: true, panels: [1, 2, 3], repeatLimits: new Map([[2, 24], [3, 12]]), plan };
  for (const [search, panel] of [["?viewPanel=2-r2", "2-r2"], ["?viewPanel=panel-2-r24", "2-r24"], ["?viewPanel=b%24panel-1", null], ["?viewPanel=c%24panel-2", "2-r3"], ["?viewPanel=a%24panel-2", 2], ["?viewPanel=3%24panel-2", null], ["?viewPanel=3-r12", "3-r12"]]) {
    const got = parseView(search, ctx);
    if (panel === null) {
      assert.equal(got.state.page, "view", search);
      continue;
    }
    assert.equal(got.state.page, "panel-view", search);
    assert.equal(got.state.panel, panel, search);
    assert.deepEqual(got.notices, [], search);
  }
  // A copy of a panel that does not repeat, a number above the limit and a form that is not valid give the notice of rule 4.
  for (const search of ["?viewPanel=1-r2", "?viewPanel=2-r25", "?viewPanel=3-r13", "?viewPanel=2-r1", "?viewPanel=9-r2"]) {
    const got = parseView(search, ctx);
    assert.equal(got.state.page, "view", search);
    assert.equal(got.notices.length, 1, search);
    assert.equal(got.notices[0].code, "no-panel", search);
  }
  // The limits come from the session: a smaller cap makes a key above it a value that is not known.
  assert.equal(parseView("?viewPanel=2-r4", { ...ctx, repeatLimits: new Map([[2, 3]]) }).state.page, "view");
});

test("an editPanel with the form of a copy is a value that the contract does not know (rule 16)", () => {
  const got = parseView("?editPanel=2-r2", { canEdit: true, panels: [1, 2], repeatLimits: new Map([[2, 24]]) });
  assert.equal(got.state.page, "view");
  assert.deepEqual(got.notices.map((n) => n.code), ["no-panel"]);
});

test("the normal form writes the number for the first copy and `<id>-r<n>` for the others", () => {
  const second = { ...baseState(), page: "panel-view", panel: "2-r2" };
  const first = { ...baseState(), page: "panel-view", panel: 2 };
  assert.equal(normalSearch("?viewPanel=panel-2-r2", second), "?viewPanel=2-r2");
  // The form of Grafana: the state that the values give (rule 1) is written, and while the values load the state keeps the form (rule 16).
  assert.equal(normalSearch("?viewPanel=b%24panel-2", second), "?viewPanel=2-r2");
  assert.equal(normalSearch("?viewPanel=a%24panel-2", first), "?viewPanel=2");
  assert.equal(normalSearch("?viewPanel=b%24panel-2", { ...second, panel: "b$panel-2" }), "?viewPanel=b%24panel-2");
  assert.equal(normalSearch("?viewPanel=b$panel-2", { ...second, panel: "b$panel-2" }), "?viewPanel=b$panel-2");
  assert.equal(normalSearch("?viewPanel=2-r2", second), "?viewPanel=2-r2");
  assert.equal(renderSearch(second, {}), "?viewPanel=2-r2");
  assert.deepEqual(parseView(renderSearch(second, {}), { canEdit: false, panels: [2], repeatLimits: new Map([[2, 24]]) }).state, second);
  assert.equal(nextState(baseState(), "view-panel", "2-r2", { origin: null, dirty: false }).state.panel, "2-r2");
});
