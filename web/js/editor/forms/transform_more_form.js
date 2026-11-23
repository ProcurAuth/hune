// The forms of the eleven more transforms (10.3a, 4.25a, F28). The body of one transform of the list: `moreBody(scope, t, ctx, deps)`.
// Each control writes the draft through `ctx.update`, and a control that has an error writes nothing (4.37). The form never runs a transform.
// The editor loads this file on demand, so the measure of 9.30 does not count it. Text of a document goes in the DOM with textContent only.
// The hooks are `<item>-<member>` with `_` as `-` (9.29): the item is `transform-<n>`.

import { button, el, uniqueId } from "../../ui.js";
import { checksField, iconBtn, numberField, selectField, setInactive, switchField, textField } from "./controls.js";
import { moveItem, removeItem, replaceItem } from "./model.js";
import { charLength } from "./rules.js";
import {
  CALC_MODES, CONVERT_TYPES, JOIN_MODES, LIMIT_ENDS, MORE_LIMITS, OPERAND_KINDS, OPERATORS, REDUCER_ITEMS, REDUCE_MODES, UNARY_OPERATORS,
  calcKey, calcOfMode, nameError, newNameError, operandsError, patternError,
} from "./transform_more_model.js";

// A datalist of the field names of the last data of the panel: a person can also type a name that the data does not hold (10.3a).
function namesList(hints) {
  const id = uniqueId("fnames");
  return { id, node: el("datalist", { id }, ...hints().map((n) => el("option", { value: n }))) };
}

// A name that is not in `used`: the first name of the data that is free, else "Value", "Value 2", ... (a new row is valid at once, 4.25a).
function fresh(hints, used) {
  const free = hints().find((n) => !used.includes(n));
  if (free !== undefined) return free;
  for (let i = 1; ; i++) {
    const name = i === 1 ? "Value" : `Value ${i}`;
    if (!used.includes(name)) return name;
  }
}

const errorLine = () => el("p", { class: "field-error", role: "alert", hidden: true });
const show = (node, text) => {
  node.textContent = text || "";
  node.hidden = !text;
};

/**
 * A list of rows with "Remove" on each row and "Add" under the list (and "Up" and "Down" when `move`).
 *  get(), set(array): read and write the array of the member.   min, max: the bounds of 4.25a.
 *  row(item, i, put): the nodes of the row i. `put(next)` writes the item i with no redraw.
 *  newItem(): the item that "Add" appends.
 */
function rowsEditor(scope, { testid, label, get, set, min = 0, max, addLabel, newItem, row, move = false }) {
  const root = el("div", { class: "flist flist--nested", dataset: { testid } });
  let focus = null;
  const structural = (fn) => {
    scope.flush();
    fn();
  };
  const commit = (next, at) => {
    set(next);
    focus = at;
    draw();
  };
  function draw() {
    const items = get();
    root.replaceChildren();
    if (label) root.append(el("span", { class: "field__label", text: label }));
    const ol = el("ol", { class: "flist__items" });
    items.forEach((item, i) => {
      const id = `${testid}-${i}`;
      const put = (next) => set(replaceItem(get(), i, next));
      const actions = [];
      if (move) {
        actions.push(iconBtn("Move up", "arrow-up", () => structural(() => commit(moveItem(get(), i, -1), `${id}-up`)), { disabled: i === 0, testid: `${id}-up` }));
        actions.push(iconBtn("Move down", "arrow-down", () => structural(() => commit(moveItem(get(), i, 1), `${id}-down`)), { disabled: i === items.length - 1, testid: `${id}-down` }));
      }
      const remove = iconBtn("Remove", "trash", () => {
        if (remove.getAttribute("aria-disabled") === "true") return;
        structural(() => commit(removeItem(get(), i), `${testid}-add`));
      }, { testid: `${id}-remove` });
      setInactive(remove, items.length <= min, `At least ${min} ${min === 1 ? "item stays" : "items stay"}.`);
      actions.push(remove);
      ol.append(el("li", { class: "fitem fitem--row", dataset: { testid: id } }, el("div", { class: "fitem__body" }, ...row(item, i, put)), el("span", { class: "fitem__actions" }, ...actions)));
    });
    const add = button(addLabel, { dataset: { testid: `${testid}-add` }, on: { click: () => {
      if (add.getAttribute("aria-disabled") === "true") return;
      structural(() => commit([...get(), newItem()], `${testid}-${get().length}`));
    } } }, "plus", { size: "sm" });
    setInactive(add, items.length >= max, `A limit of ${max} items applies.`);
    root.append(ol, el("div", { class: "flist__add" }, add));
    if (focus) {
      const target = root.querySelector(`[data-testid="${focus}"]`);
      focus = null;
      if (target && !target.disabled) target.focus();
      else add.focus();
    }
  }
  draw();
  return root;
}

// A list of names with a datalist: 4.25a gives the bounds, and no two names are equal.
function namesEditor(scope, { testid, label, addLabel = "Add name", get, set, min = 0, max = MORE_LIMITS.items, list, hints }) {
  return rowsEditor(scope, {
    testid, label, get, set, min, max, addLabel, newItem: () => fresh(hints, get()),
    row: (name, i, put) => [textField(scope, {
      ariaLabel: `${label || "Name"} ${i + 1}`, value: name, list, testid: `${testid}-${i}-name`,
      check: (v) => nameError(replaceItem(get(), i, v), i),
      apply: (v) => put(v),
    }).node],
  });
}

// The error line of a whole group: a rule that no single control can hold.
function groupError() {
  const line = errorLine();
  return { line, set: (text) => show(line, text) };
}

// --- organize ---------------------------------------------------------------------------------------------------------------------------

function organizeBody(scope, t, ctx, { get, hints }) {
  const dataNames = [...new Set(hints())];
  const root = el("div", { class: "flist flist--nested", dataset: { testid: `${ctx.testid}-fields` } });
  const err = groupError();
  const cur = () => get();
  // The rows: the names of `order` first, then the other names of the data, then the names that only `hide` or `rename` hold.
  const rows = () => {
    const t0 = cur();
    const seq = [...t0.order];
    for (const n of [...dataNames, ...t0.hide, ...t0.rename.map((r) => r.field)]) if (!seq.includes(n)) seq.push(n);
    return seq;
  };
  const natural = () => {
    const t0 = cur();
    const seq = [...dataNames];
    for (const n of [...t0.hide, ...t0.rename.map((r) => r.field)]) if (!seq.includes(n)) seq.push(n);
    return seq;
  };
  const write = (patch) => {
    const next = { ...cur(), ...patch };
    const bad = ["hide", "order", "rename"].find((k) => next[k].length > MORE_LIMITS.items);
    if (bad) {
      err.set(`The member ${bad} has ${MORE_LIMITS.items} items at most.`);
      return false;
    }
    err.set("");
    ctx.update(patch);
    return true;
  };
  function draw(focusId = null) {
    root.replaceChildren();
    const seq = rows();
    const ol = el("ol", { class: "flist__items" });
    seq.forEach((name, i) => {
      const id = ctx.testid;
      const inData = dataNames.includes(name);
      const hide = cur().hide.includes(name);
      const renamed = cur().rename.find((r) => r.field === name);
      const hideBox = el("input", { type: "checkbox", checked: hide, "aria-label": `Hide ${name}`, dataset: { testid: `${id}-hide-${i}` },
        on: { change: (e) => {
          const next = e.target.checked ? [...cur().hide.filter((n) => n !== name), name] : cur().hide.filter((n) => n !== name);
          if (!write({ hide: seq.filter((n) => next.includes(n)) })) e.target.checked = !e.target.checked;
        } } });
      const rename = textField(scope, {
        ariaLabel: `New name of ${name}`, value: renamed ? renamed.name : "", testid: `${id}-rename-${i}`, placeholder: name,
        check: (v) => (v === "" ? "" : newNameError(v, "A new name")),
        apply: (v) => {
          const rest = cur().rename.filter((r) => r.field !== name);
          const next = v === "" ? rest : [...rest, { field: name, name: v }];
          write({ rename: seq.flatMap((n) => next.filter((r) => r.field === n)) });
        },
      });
      const move = (dir) => () => {
        scope.flush();
        const next = moveItem(rows(), i, dir);
        if (!next) return;
        const nat = natural();
        if (write({ order: JSON.stringify(next) === JSON.stringify(nat) ? [] : next })) draw(`${id}-${dir < 0 ? "up" : "down"}-${i + dir}`);
      };
      ol.append(el("li", { class: "fitem fitem--row", dataset: { testid: `${id}-field-${i}` } },
        el("div", { class: "fitem__body" },
          el("label", { class: "check" }, hideBox, el("span", { text: "Hide" })),
          el("span", { class: "fitem__title", text: name }),
          ...(inData ? [] : [el("span", { class: "muted", text: "not in the data" })]),
          rename.node),
        el("span", { class: "fitem__actions" },
          iconBtn("Move up", "arrow-up", move(-1), { disabled: i === 0, testid: `${id}-up-${i}` }),
          iconBtn("Move down", "arrow-down", move(1), { disabled: i === seq.length - 1, testid: `${id}-down-${i}` }))));
    });
    root.append(ol, err.line);
    if (focusId) {
      const target = root.querySelector(`[data-testid="${focusId}"]`);
      if (target && !target.disabled) target.focus();
    }
  }
  draw();
  return [root];
}

// --- calculate_field ----------------------------------------------------------------------------------------------------------------------

// One operand: a select of the kind, and a field name or a number.
function operandControl(scope, { which, operand, ctx, list, put }) {
  const host = el("div", { class: "fmatcher" });
  let current = operand;
  const draw = () => {
    host.replaceChildren(
      current.kind === "field"
        ? textField(scope, { label: `${which} field`, value: current.field, list, testid: `${ctx.testid}-${which.toLowerCase()}-field`,
          check: (v) => nameError([v], 0), apply: (v) => put((current = { kind: "field", field: v })) }).node
        : numberField(scope, { label: `${which} number`, value: current.value, testid: `${ctx.testid}-${which.toLowerCase()}-value`,
          apply: (v) => put((current = { kind: "number", value: v })) }).node);
  };
  const kind = selectField({ label: `${which} operand`, options: OPERAND_KINDS, value: current.kind, testid: `${ctx.testid}-${which.toLowerCase()}-kind`,
    apply: (k) => {
      scope.flush();
      const next = k === "field" ? { kind: "field", field: "Value" } : { kind: "number", value: 1 };
      if (put(next)) {
        current = next;
        draw();
      } else kind.control.value = current.kind;
    } });
  draw();
  return [kind.node, host];
}

function calculateBody(scope, t, ctx, { get, hints }) {
  const names = namesList(hints);
  const first = hints()[0] || "Value";
  const host = el("div", { class: "fgroup" });
  const err = groupError();
  const cur = () => get();
  const draw = () => {
    const c = cur();
    const key = calcKey(c.mode);
    const o = c[key];
    const nodes = [];
    if (c.mode === "binary") {
      const put = (side) => (operand) => {
        const next = { ...cur().binary, [side]: operand };
        const bad = operandsError(next);
        err.set(bad);
        if (bad) return false;
        ctx.update({ binary: next });
        return true;
      };
      nodes.push(...operandControl(scope, { which: "Left", operand: o.left, ctx, list: names.id, put: put("left") }));
      nodes.push(selectField({ label: "Operator", options: OPERATORS, value: o.operator, testid: `${ctx.testid}-operator`, apply: (v) => ctx.update({ binary: { ...cur().binary, operator: v } }) }).node);
      nodes.push(...operandControl(scope, { which: "Right", operand: o.right, ctx, list: names.id, put: put("right") }));
      nodes.push(err.line);
    } else if (c.mode === "reduce_row") {
      nodes.push(selectField({ label: "Calculation", options: REDUCER_ITEMS, value: o.reducer, testid: `${ctx.testid}-reducer`, apply: (v) => ctx.update({ reduce: { ...cur().reduce, reducer: v } }) }).node);
      nodes.push(namesEditor(scope, { testid: `${ctx.testid}-include`, label: "Fields (all number fields when empty)", get: () => cur().reduce.include,
        set: (list) => ctx.update({ reduce: { ...cur().reduce, include: list } }), list: names.id, hints }));
    } else if (c.mode === "unary") {
      nodes.push(selectField({ label: "Operation", options: UNARY_OPERATORS, value: o.operator, testid: `${ctx.testid}-operator`, apply: (v) => ctx.update({ unary: { ...cur().unary, operator: v } }) }).node);
      nodes.push(textField(scope, { label: "Field", value: o.field, list: names.id, testid: `${ctx.testid}-field`, check: (v) => nameError([v], 0),
        apply: (v) => ctx.update({ unary: { ...cur().unary, field: v } }) }).node);
    } else {
      nodes.push(switchField({ label: "As percentile", checked: o.as_percentile, testid: `${ctx.testid}-as-percentile`, apply: (v) => ctx.update({ index: { as_percentile: v } }) }).node);
    }
    host.replaceChildren(...nodes);
  };
  draw();
  const mode = selectField({ label: "Mode", options: CALC_MODES, value: t.mode, testid: `${ctx.testid}-mode`,
    apply: (m) => {
      scope.flush();
      ctx.replace(calcOfMode(cur(), m, first));
    } });
  const alias = textField(scope, { label: "Alias", value: t.alias === null ? "" : t.alias, testid: `${ctx.testid}-alias`, hint: "Empty: the name that the calculation gives.",
    check: (v) => (v === "" ? "" : newNameError(v, "An alias")), apply: (v) => ctx.update({ alias: v === "" ? null : v }) });
  const replace = switchField({ label: "Replace all fields", checked: t.replace_fields, testid: `${ctx.testid}-replace-fields`, apply: (v) => ctx.update({ replace_fields: v }) });
  return [mode.node, host, alias.node, replace.node, names.node];
}

// --- the other kinds ----------------------------------------------------------------------------------------------------------------------

function sortBody(scope, t, ctx, { get, hints }) {
  const names = namesList(hints);
  const first = hints()[0] || "Value";
  const err = groupError();
  const rows = rowsEditor(scope, {
    testid: `${ctx.testid}-sort`, label: "Sort by", get: () => get().sort, set: (sort) => ctx.update({ sort }), min: 1, max: MORE_LIMITS.sort, addLabel: "Add key", move: true,
    newItem: () => {
      // A key that is not in the list (4.25a): a name of the data, ascending first, then descending. Placeholder names follow the data.
      const keys = get().sort;
      const pool = [...new Set([...hints(), ...Array.from({ length: MORE_LIMITS.sort }, (_, i) => (i === 0 ? "Value" : `Value ${i + 1}`))])];
      for (const desc of [false, true]) {
        const free = pool.find((n) => !keys.some((k) => k.field === n && k.desc === desc));
        if (free !== undefined) return { field: free, desc };
      }
      return { field: first, desc: false };
    },
    row: (key, i, put) => [
      textField(scope, { ariaLabel: `Field of key ${i + 1}`, value: key.field, list: names.id, testid: `${ctx.testid}-sort-${i}-field`,
        check: (v) => {
          const bad = nameError([v], 0);
          const twice = get().sort.some((k, j) => j !== i && k.field === v && k.desc === get().sort[i].desc);
          return bad || (twice ? "This key is in the list twice." : "");
        },
        apply: (v) => put({ ...get().sort[i], field: v }) }).node,
      switchField({ label: "Descending", checked: key.desc, testid: `${ctx.testid}-sort-${i}-desc`,
        apply: (v) => {
          const next = { ...get().sort[i], desc: v };
          const twice = get().sort.some((k, j) => j !== i && k.field === next.field && k.desc === v);
          err.set(twice ? "This key is in the list twice." : "");
          if (!twice) put(next);
        } }).node,
    ],
  });
  return [rows, err.line, names.node];
}

function limitBody(scope, t, ctx) {
  return [
    numberField(scope, { label: "Limit", value: t.count, integer: [0, MORE_LIMITS.count], testid: `${ctx.testid}-count`, apply: (n) => ctx.update({ count: n }) }).node,
    selectField({ label: "From", options: LIMIT_ENDS, value: t.end, testid: `${ctx.testid}-end`, apply: (v) => ctx.update({ end: v }) }).node,
  ];
}

function mergeBody(scope, t, ctx, { queryCount }) {
  // "Merge needs two queries or more." shows when the panel has one query. The transform stays valid.
  const note = el("p", { class: "muted", dataset: { testid: `${ctx.testid}-note` }, text: "Merge needs two queries or more." });
  note.hidden = queryCount() >= 2;
  return [note];
}

function reduceBody(scope, t, ctx, { get }) {
  const checks = checksField({
    label: "Calculations", items: REDUCER_ITEMS, value: t.reducers, max: MORE_LIMITS.reducers, testid: `${ctx.testid}-reducers`,
    apply: (list) => {
      if (list.length < 1) {
        checks.error("Choose one calculation or more.");
        return;
      }
      checks.error("");
      ctx.update({ reducers: list });
    },
  });
  return [selectField({ label: "Mode", options: REDUCE_MODES, value: t.mode, testid: `${ctx.testid}-mode`, apply: (v) => ctx.update({ mode: v }) }).node, checks.node];
}

function partitionBody(scope, t, ctx, { get, hints }) {
  const names = namesList(hints);
  return [
    namesEditor(scope, { testid: `${ctx.testid}-fields`, label: "Fields", addLabel: "Add field", get: () => get().fields, set: (fields) => ctx.update({ fields }), min: 1, max: MORE_LIMITS.partition, list: names.id, hints }),
    switchField({ label: "Keep fields", checked: t.keep_fields, testid: `${ctx.testid}-keep-fields`, apply: (v) => ctx.update({ keep_fields: v }) }).node,
    names.node,
  ];
}

function joinBody(scope, t, ctx, { hints }) {
  // The choice "Time" is `null`. A name that the data does not hold stays as a choice.
  const names = [...new Set(hints())];
  if (t.field !== null && !names.includes(t.field)) names.push(t.field);
  return [
    selectField({ label: "Field", options: [["", "Time"], ...names.map((n) => [n, n])], value: t.field === null ? "" : t.field, testid: `${ctx.testid}-field`, apply: (v) => ctx.update({ field: v === "" ? null : v }) }).node,
    selectField({ label: "Mode", options: JOIN_MODES, value: t.mode, testid: `${ctx.testid}-mode`, apply: (v) => ctx.update({ mode: v }) }).node,
  ];
}

function filterBody(scope, t, ctx, { get, hints }) {
  const names = namesList(hints);
  const group = (side, title) => el("div", { class: "fgroup", dataset: { testid: `${ctx.testid}-${side}` } },
    el("h5", { class: "fgroup__title", text: title }),
    namesEditor(scope, { testid: `${ctx.testid}-${side}-names`, label: "Names", get: () => get()[side].names, set: (list) => ctx.update({ [side]: { ...get()[side], names: list } }), list: names.id, hints }),
    textField(scope, { label: "Pattern", value: t[side].pattern === null ? "" : t[side].pattern, mono: true, testid: `${ctx.testid}-${side}-pattern`, hint: "Empty: no pattern.",
      check: (v) => patternError(v, true), apply: (v) => ctx.update({ [side]: { ...get()[side], pattern: v === "" ? null : v } }) }).node);
  return [group("include", "Include"), group("exclude", "Exclude"), names.node];
}

function renameBody(scope, t, ctx) {
  return [
    textField(scope, { label: "Match", value: t.regex, mono: true, testid: `${ctx.testid}-regex`, check: (v) => patternError(v), apply: (v) => ctx.update({ regex: v }) }).node,
    textField(scope, { label: "Replace", value: t.rename_pattern, mono: true, testid: `${ctx.testid}-rename-pattern`, hint: "$1 is the first group.",
      check: (v) => (charLength(v) > MORE_LIMITS.nameChars ? `The text has ${MORE_LIMITS.nameChars} characters at most.` : ""), apply: (v) => ctx.update({ rename_pattern: v }) }).node,
  ];
}

function convertBody(scope, t, ctx, { get, hints }) {
  const names = namesList(hints);
  const rows = rowsEditor(scope, {
    testid: `${ctx.testid}-conversions`, label: "Conversions", get: () => get().conversions, set: (conversions) => ctx.update({ conversions }), min: 1, max: MORE_LIMITS.conversions, addLabel: "Add conversion",
    newItem: () => ({ field: fresh(hints, get().conversions.map((c) => c.field)), to: "text" }),
    row: (c, i, put) => [
      textField(scope, { ariaLabel: `Field of conversion ${i + 1}`, value: c.field, list: names.id, testid: `${ctx.testid}-conversions-${i}-field`,
        check: (v) => nameError([v], 0) || (get().conversions.some((x, j) => j !== i && x.field === v) ? "This field is in the list twice." : ""),
        apply: (v) => put({ ...get().conversions[i], field: v }) }).node,
      selectField({ ariaLabel: `Type of conversion ${i + 1}`, options: CONVERT_TYPES, value: c.to, testid: `${ctx.testid}-conversions-${i}-to`, apply: (v) => put({ ...get().conversions[i], to: v }) }).node,
    ],
  });
  return [rows, names.node];
}

const BODY = {
  organize: organizeBody,
  calculate_field: calculateBody,
  sort_by: sortBody,
  limit: limitBody,
  merge: mergeBody,
  reduce: reduceBody,
  partition_by_values: partitionBody,
  join_by_field: joinBody,
  filter_fields_by_name: filterBody,
  rename_by_regex: renameBody,
  convert_field_type: convertBody,
};

/**
 * The body of one transform of the eleven kinds: an array of nodes, or null for another kind.
 *  t, ctx: the transform and the item of `listEditor` (`ctx.update`, `ctx.replace`, `ctx.testid`).
 *  deps.get(): the transform now.   deps.hints(): the current names of the last data.   deps.queryCount(): the count of queries of the panel.
 */
export function moreBody(scope, t, ctx, deps) {
  const body = BODY[t.kind];
  return body ? body(scope, t, ctx, { queryCount: () => 2, ...deps }) : null;
}
