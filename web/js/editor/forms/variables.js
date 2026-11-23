// The form of the variables (10.3b, 4.7 to 4.11): the tab "Variables" of the settings view.
// A list shows the name, the kind and the label of each variable. A click opens the form of one variable.
// The kind of a variable does not change: the person removes the variable and adds another.

import * as api from "../../api.js";
import * as editorRoutes from "../api_editor.js";
import { button, el, notify, openDialog, openMenu } from "../../ui.js";
import { PLACEMENT_TITLE_MAX, defaultPlacement, lastPlainSection, movePlacement, placeInSection, placementRefusal, placementTitleError, removeRepeatsOf, renameRepeatVariable, repeatUsers, setPlacement } from "../draft.js";
import { positionErrors } from "../layout.js";
import { variablePreviewBody } from "../preview_body.js";
import { createSqlEditor } from "../sql_editor.js";
import { listEditor, numberField, selectField, switchField, textField } from "./controls.js";
import { AUTO_MIN_TEXTS, LIMITS, SORTS, VARIABLE_KINDS, intervalsError, newVariable, splitIntervals, setAllowCustom, staleAnnotations, stalePanels, staleVariables, uniqueVariableName, variableCycle } from "./model.js";
import { checkBytes, checkChars, checkPattern, checkVariableName, hasControlChar } from "./rules.js";
import { addExampleHints, createPreview, exampleItems, exampleText, insertExample } from "./var_examples.js";

const HIDE = [["none", "Show the label and the control"], ["label", "Show the control only"], ["variable", "Show nothing"]];
const KIND_TEXT = Object.fromEntries(VARIABLE_KINDS);
const ALL = "$__all";

// The help text under the switch "Allow custom values" (10.3b, ID-K210).
const ALLOW_HELP = {
  query: "Off: the value must be one that the query returns. On: a person can also type a value.",
  custom: "Off: the value must be one of the options. On: a person can also type a value. A typed value is free text: the SQL cannot use it in a quoted name, and in code it must be a number.",
};

/** The errors of the `default` of a `query` or `custom` variable (4.11): a text, or "". */
export function defaultErrors(v) {
  const d = v.default;
  if (!v.multi && d.length > 1) return "A variable with one value has one default at most.";
  if (d.includes(ALL)) {
    if (!v.include_all) return "The default $__all needs the option All.";
    if (d.length > 1) return "The default $__all is the only default.";
    return "";
  }
  if (v.kind === "custom" && v.allow_custom_value === true) {
    // An open `custom` variable: a default is a typed value, so only its size and its characters count (4.11, rule 5).
    for (const x of d) {
      if (new TextEncoder().encode(x).length > 1024) return `The default ${JSON.stringify(x.slice(0, 20))}... has more than 1024 bytes.`;
      if (hasControlChar(x)) return `The default ${JSON.stringify(x)} has a control character.`;
    }
    return "";
  }
  if (v.kind === "custom") {
    const values = new Set(v.options.map((o) => o.value));
    const bad = d.find((x) => !values.has(x));
    if (bad !== undefined) return `The default ${JSON.stringify(bad)} is not an option.`;
  }
  return "";
}

/** The error of the cycle of references of the draft (4.34): a text, or "". */
export function cycleError(variables) {
  const name = variableCycle(variables);
  return name === null ? "" : `[bad_reference] The variable ${name} is in a cycle of references.`;
}

/**
 * `E` is the editor: `E.doc`, `E.change(doc)`, `E.sources`. `get()` gives the variables now. `set(array)` writes them.
 * It returns { el, destroy }.
 */
export function createVariables(E, scope) {
  const root = el("div", { class: "vars-form", dataset: { testid: "settings-variables" } });
  let openAt = null;
  let sqlEditor = null;
  let optionsPreview = null;
  const get = () => E.doc.variables;
  // `write` changes the variables. `set` is for the list: a variable that leaves the list takes the repeats that name it with it (4.13a, rule 9).
  const write = (arr) => E.change({ ...structuredClone(E.doc), variables: arr });
  const set = (arr) => {
    let doc = { ...structuredClone(E.doc), variables: arr };
    if (arr.length < get().length) for (const v of get()) if (!arr.some((x) => x.name === v.name)) doc = removeRepeatsOf(doc, v.name);
    E.change(doc);
  };

  // The delete of a variable that a repeat names asks first: the dialog lists the panels and the sections that lose their repeat (10.3b).
  function confirmDelete(v, i, proceed) {
    const users = repeatUsers(E.doc, v.name);
    if (!users.panels.length && !users.sections.length) return false;
    const names = [...users.panels.map((p) => p.title || `Panel ${p.id}`), ...users.sections.map(({ section }) => section.title)];
    openDialog({
      title: `Delete the variable ${v.name}?`,
      testid: "delete-variable-dialog",
      build: (body) => body.append(
        el("p", { text: "These panels and sections lose their repeat:" }),
        el("ul", { dataset: { testid: "delete-variable-users" } }, ...names.map((n) => el("li", { text: n })))),
      buttons: [{ label: "Cancel" }, { label: "Delete", danger: true, onClick: () => proceed() }],
    });
    return true;
  }

  function cycleBox() {
    const text = cycleError(get());
    return el("p", { class: "field-error", role: "alert", hidden: !text, text, dataset: { testid: "variables-cycle" } });
  }

  // --- the list ------------------------------------------------------------------------------------------------------------

  function drawList() {
    destroySql();
    openAt = null;
    const list = listEditor(scope, {
      testid: "variable-list",
      itemTestid: (i) => `variable-${i}`,
      get,
      set,
      beforeRemove: confirmDelete,
      limit: LIMITS.variables,
      limitWhat: "variables",
      emptyText: "No variable.",
      title: (v) => v.name,
      add: {
        label: "Add variable",
        testid: "variable-add",
        kinds: VARIABLE_KINDS,
        kindLabel: "Kind of the variable",
        make(kind) {
          // An adhoc variable needs a ClickHouse source (4.10c).
          const source = ((kind === "adhoc" ? E.sources.find((s) => s.kind === "clickhouse") : E.sources[0]) || {}).id || "";
          const v = newVariable(kind, uniqueVariableName(get(), kind === "text" ? "text" : "var"), source);
          const at = get().length;
          queueMicrotask(() => drawForm(at));
          return v;
        },
      },
      renderItem(v, i, ctx) {
        const section = v.placement ? E.doc.sections[v.placement.section] : null;
        return el("div", { class: "vrow" },
          el("span", { class: "muted", text: `${KIND_TEXT[v.kind] || v.kind}${v.label ? `, label ${v.label}` : ""}` }),
          ...(v.placement ? [el("span", { class: "muted vrow__placed", dataset: { testid: `${ctx.testid}-placed` }, text: `In the grid, ${section && section.title ? `section ${section.title}` : `section ${v.placement.section + 1}`}` })] : []),
          button("Edit", { dataset: { testid: `${ctx.testid}-open` }, on: { click: () => { scope.flush(); drawForm(i); } } }, "edit", { size: "sm" }));
      },
    });
    root.replaceChildren(list, cycleBox());
  }

  // --- the form of one variable --------------------------------------------------------------------------------------------

  function destroySql() {
    if (sqlEditor) {
      sqlEditor.destroy();
      sqlEditor = null;
    }
    if (optionsPreview) {
      optionsPreview.destroy();
      optionsPreview = null;
    }
  }

  function drawForm(i) {
    scope.flush();
    destroySql();
    openAt = i;
    const v = get()[i];
    if (!v) {
      drawList();
      return;
    }
    const cur = () => get()[i];
    const update = (patch) => write(get().map((x, k) => (k === i ? { ...x, ...patch } : x)));
    const t = (name) => `variable-form-${name}`;
    const cycle = cycleBox();
    const recheck = () => {
      const text = cycleError(get());
      cycle.textContent = text;
      cycle.hidden = !text;
    };
    const back = button("Back to the variables", { dataset: { testid: "variable-back" }, on: { click: () => { scope.flush(); drawList(); } } }, "arrow-left", { size: "sm" });
    const parts = [];
    parts.push(textField(scope, {
      label: "Name",
      value: v.name,
      check: (name) => checkVariableName(name, get().filter((_, k) => k !== i).map((x) => x.name)),
      apply(name) {
        const old = cur().name;
        if (old === name) return;
        // A rename changes the repeats that name the variable in the same draft change (4.13a, rule 9).
        E.change(renameRepeatVariable({ ...structuredClone(E.doc), variables: get().map((x, k) => (k === i ? { ...x, name } : x)) }, old, name));
        announceRename(old, name);
        recheck();
      },
      testid: t("name"),
    }).node);
    parts.push(textField(scope, {
      label: "Label",
      value: v.label === null ? "" : v.label,
      check: (x) => (x === "" ? "" : checkChars(x, 1, 256, "The label")),
      apply: (x) => update({ label: x === "" ? null : x }),
      testid: t("label"),
      hint: "Empty: the bar shows the name.",
    }).node);
    // A placed variable cannot have `hide = "variable"`: the select does not offer it (4.20).
    const placed = Boolean(v.placement);
    // A constant has `hide` = `variable` and no placement: the field is read only (4.10b, rule 1).
    const constant = v.kind === "constant";
    const hide = selectField({ label: "Hide", options: constant ? [["variable", "Variable"]] : placed ? HIDE.filter(([value]) => value !== "variable") : HIDE, value: v.hide, apply: (x) => update({ hide: x }), testid: t("hide") });
    if (placed) hide.control.title = "The variable is in the grid. Put it back in the top bar first.";
    if (constant) hide.control.disabled = true;
    parts.push(hide.node);
    // An adhoc variable has no placement: its control is in the variables bar (4.10c, rule 4).
    if (!constant && v.kind !== "adhoc") parts.push(placementGroup(v, cur, t));

    if (v.kind === "text") {
      let defaultField = null;
      const numberOk = (text) => /^-?[0-9]+(\.[0-9]+)?$/.test(text);
      parts.push(selectField({
        label: "Value type",
        options: [["text", "Text"], ["number", "Number"]],
        value: v.value_type,
        testid: t("value-type"),
        apply(x) {
          update({ value_type: x });
          if (defaultField) defaultField.error(x === "number" && !numberOk(cur().default) ? "A number has digits, a sign and a point only." : "");
        },
      }).node);
      defaultField = textField(scope, {
        label: "Default value",
        value: v.default,
        check: (x) => (cur().value_type === "number" && !numberOk(x) ? "A number has digits, a sign and a point only." : checkBytes(x, 0, 1024, "The value")),
        apply: (x) => update({ default: x }),
        testid: t("default"),
      });
      parts.push(defaultField.node);
    } else if (v.kind === "interval") {
      parts.push(...intervalMembers(cur, update, t));
    } else if (v.kind === "adhoc") {
      parts.push(...adhocMembers(v, update, t));
    } else if (constant) {
      parts.push(textField(scope, {
        label: "Value",
        value: v.value,
        check: (x) => checkBytes(x, 0, 1024, "The value") || (hasControlChar(x) ? "The value has a control character." : ""),
        apply: (x) => update({ value: x }),
        testid: t("value"),
        hint: "The viewer never sees or changes this value.",
      }).node);
    } else {
      parts.push(...choiceMembers(v, i, cur, update, recheck, t));
    }
    root.replaceChildren(el("div", { class: "vform" }, back, el("h3", { class: "vform__title", text: `${KIND_TEXT[v.kind]} variable` }), ...parts, cycle));
  }

  // The placement of the variable in the grid (4.20, 10.6): a switch, the section, the title and the place. An absent placement is the top bar.
  // Each change goes to the draft at once and the form is drawn again, as the grid and the bar change with it.
  function placementGroup(v, cur, t) {
    const commit = (doc) => {
      scope.flush();
      E.change(doc);
      drawForm(openAt);
    };
    const refusal = placementRefusal(cur());
    const on = switchField({
      label: "Place in the grid",
      checked: Boolean(cur().placement),
      testid: t("placed"),
      apply: (checked) => {
        const target = lastPlainSection(E.doc);
        if (checked && (refusal || target < 0)) return commit(E.doc);
        commit(setPlacement(E.doc, cur().name, checked ? defaultPlacement(E.doc, target) : null));
      },
    });
    const parts = [on.node];
    if ((refusal || lastPlainSection(E.doc) < 0) && !cur().placement) {
      on.input.disabled = true;
      on.input.title = refusal || "Every section repeats.";
    }
    const p = cur().placement;
    if (p) {
      const name = cur().name;
      const sectionField = selectField({
        label: "Section",
        options: E.doc.sections.map((s, i) => [String(i), s.title || `Section ${i + 1}`]),
        value: String(p.section),
        testid: t("placement-section"),
        apply: (x) => commit(placeInSection(E.doc, name, Number(x))),
      });
      // A section that repeats has no placement (4.7a, rule 11): its item is not active.
      E.doc.sections.forEach((s, i) => {
        const option = sectionField.control.options[i];
        if (s.repeat && option) {
          option.disabled = true;
          option.title = "This section repeats.";
        }
      });
      parts.push(sectionField.node);
      const title = textField(scope, {
        label: "Title",
        value: p.title === undefined ? "" : p.title,
        check: (x) => placementTitleError(x),
        apply(x) {
          const next = { ...cur().placement };
          if (x === "") delete next.title;
          else next.title = x;
          E.change(setPlacement(E.doc, name, next));
        },
        testid: t("placement-title"),
        hint: `Empty: the control shows the label of the variable. ${PLACEMENT_TITLE_MAX} characters at most.`,
      });
      parts.push(title.node);
      const grid = {};
      const row = el("div", { class: "field-grid" });
      for (const key of ["x", "y", "w", "h"]) {
        grid[key] = textField(scope, {
          label: key,
          value: String(p[key]),
          numeric: true,
          check: (x) => (/^-?[0-9]+$/.test(x.trim()) ? "" : "Use a whole number."),
          apply(x) {
            const g = { x: p.x, y: p.y, w: p.w, h: p.h, [key]: Number(x) };
            const errors = positionErrors(g);
            if (errors.length) return grid[key].error(errors.join(" "));
            E.change(movePlacement(E.doc, name, g));
            return undefined;
          },
          testid: t(`placement-${key}`),
        });
        row.append(grid[key].node);
      }
      parts.push(row);
    }
    return el("fieldset", { class: "vform__placement", dataset: { testid: t("placement") } }, el("legend", { class: "field__label", text: "Place in the grid" }), ...parts);
  }

  // The members of an `interval` variable (4.10a): the list, the option "Auto" and the default. A change of the list or of the switch
  // that removes the chosen default sets `default` to null in the same change (4.10a, rule 4).
  // The members of an `adhoc` variable (4.10c): a ClickHouse source and 1 to 64 keys, one on each line. The checks at save compare the
  // keys with the configured list adhoc_columns of the source (7.18).
  function adhocMembers(v, update, t) {
    const options = E.sources.filter((s) => s.kind === "clickhouse").map((s) => [s.id, s.label || s.id]);
    if (!options.some(([id]) => id === v.source)) options.unshift([v.source, `${v.source} (not available to you)`]);
    const keysError = (lines) => {
      if (lines.length < 1 || lines.length > 64) return "Give 1 to 64 keys, one on each line.";
      const bad = lines.find((k) => !/^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(k));
      if (bad !== undefined) return `The key ${JSON.stringify(bad)} is not a plain column name.`;
      return new Set(lines).size === lines.length ? "" : "A key is repeated.";
    };
    const split = (x) => x.split("\n").map((k) => k.trim()).filter((k) => k !== "");
    return [
      selectField({ label: "Source", options, value: v.source, apply: (x) => update({ source: x }), testid: t("source"), hint: "A ClickHouse source with the key adhoc_columns." }).node,
      textField(scope, {
        label: "Keys",
        value: v.keys.join("\n"),
        multiline: true,
        mono: true,
        check: (x) => keysError(split(x)),
        apply: (x) => update({ keys: split(x) }),
        testid: t("keys"),
        hint: "The columns that a viewer can filter, one on each line. Each one must be in the configured list of the source. Add AND $__adhocFilters after WHERE in a panel query.",
      }).node,
    ];
  }

  function intervalMembers(cur, update, t) {
    const parts = [];
    const choices = () => [...(cur().auto ? [["$__auto", "Auto"]] : []), ...cur().intervals.map((x) => [x, x])];
    const dflt = selectField({ label: "Default", options: [["", "First option"], ...choices()], value: cur().default || "", apply: (x) => update({ default: x === "" ? null : x }), testid: t("interval-default") });
    const paintDefault = () => {
      const keep = cur().default;
      dflt.control.replaceChildren(...[["", "First option"], ...choices()].map(([value, text]) => el("option", { value, text })));
      dflt.control.value = keep || "";
    };
    const fix = (patch) => {
      const next = { ...cur(), ...patch };
      const ok = next.default === null || next.intervals.includes(next.default) || (next.auto && next.default === "$__auto");
      update(ok ? patch : { ...patch, default: null });
      paintDefault();
    };
    parts.push(textField(scope, {
      label: "Intervals",
      value: cur().intervals.join("\n"),
      multiline: true,
      rows: 5,
      mono: true,
      check: (x) => intervalsError(splitIntervals(x)),
      apply: (x) => fix({ intervals: splitIntervals(x) }),
      testid: t("intervals"),
      hint: "One interval on each line, or separated by commas: 1m, 1h, 7d. In a ClickHouse query: toStartOfInterval(ts, toIntervalSecond(parseTimeDelta('${bucket}')))",
    }).node);
    let count = null;
    let min = null;
    const paintAuto = () => {
      for (const f of [count, min]) f.control.disabled = !cur().auto;
    };
    parts.push(switchField({ label: "Auto option", checked: cur().auto, testid: t("auto"), apply: (x) => { fix({ auto: x }); paintAuto(); } }).node);
    count = numberField(scope, { label: "Step count", value: cur().auto_count, integer: [1, 1000], apply: (n) => update({ auto_count: n }), testid: t("auto-count"), hint: "The time range divided by this number gives the interval of Auto." });
    min = selectField({ label: "Min interval", options: AUTO_MIN_TEXTS.map((x) => [x, x]), value: cur().auto_min, apply: (x) => update({ auto_min: x }), testid: t("auto-min") });
    parts.push(count.node, min.node, dflt.node);
    paintAuto();
    return parts;
  }

  // The members of a `query` or a `custom` variable (4.8, 4.9).
  function choiceMembers(v, i, cur, update, recheck, t) {
    const parts = [];
    const defaultList = defaultsList(scope, { get: () => cur().default, set: (arr) => { update({ default: arr }); showDefault(); }, cur, t });
    function showDefault() {
      defaultList.showError(defaultErrors(cur()));
    }
    if (v.kind === "query") {
      const options = E.sources.map((s) => [s.id, s.label || s.id]);
      if (!E.sources.some((s) => s.id === v.source)) options.unshift([v.source, `${v.source} (not available to you)`]);
      // The menu "Examples" shows for a ClickHouse source only (10.3b). The tool row of the SQL area holds it.
      const isClickhouse = () => (E.sources.find((s) => s.id === cur().source) || {}).kind === "clickhouse";
      const examples = button("Examples", {
        hidden: !isClickhouse(),
        dataset: { testid: "variable-sql-examples" },
        on: {
          mousedown: (event) => event.preventDefault(),
          click: () => addExampleHints(openMenu(examples, exampleItems((n) => insertExample(sqlEditor.input, exampleText(n))), { testid: "variable-sql-examples-menu" })),
        },
      }, null, { size: "sm" });
      parts.push(selectField({
        label: "Source",
        options,
        value: v.source,
        apply: (x) => {
          update({ source: x });
          examples.hidden = !isClickhouse();
          optionsPreview?.sync();
          optionsPreview?.clear();
        },
        testid: t("source"),
      }).node);
      sqlEditor = createSqlEditor({
        lines: v.sql,
        label: "Variable SQL",
        // The dialect of the syntax tree follows the kind of the source now (10.3c); the JSON reader needs the names of the variables.
        dialect: () => ((E.sources.find((s) => s.id === cur().source) || {}).kind || "clickhouse"),
        variables: () => (E.doc.variables || []).map((x) => x.name),
        tools: [examples],
        onChange: (lines) => {
          update({ sql: lines });
          optionsPreview?.clear();
          recheck();
        },
      });
      parts.push(el("div", { class: "field" }, el("span", { class: "field__label", text: "Query" }), sqlEditor.el));
      // "Show options" (10.3b): the preview of 6.22 with the draft variable and its parents. It shows the first 50 options.
      optionsPreview = createPreview(document, {
        getVariable: cur,
        canUse: () => E.sources.some((s) => s.id === cur().source),
        hasDashboard: () => Boolean(E.uid),
        isAbort: api.isAbort,
        send: (signal) => editorRoutes.preview(cur().source, variablePreviewBody({ doc: E.doc, variable: cur(), target: { uid: E.uid }, range: E.api.runtime.range, known: E.draftRt.requestVariables() }), signal),
      });
      parts.push(optionsPreview.node);
    } else {
      parts.push(optionsList(scope, { get: () => cur().options, set: (arr) => { update({ options: arr }); showDefault(); } }));
    }
    // A repeat needs several values: the switch is not active while a panel or a section repeats by this variable (4.13a, 10.3b).
    const multi = switchField({ label: "Multi-value", checked: v.multi, testid: t("multi"), apply: (x) => { update({ multi: x }); showDefault(); } });
    const users = repeatUsers(E.doc, v.name);
    if (v.multi && (users.panels.length || users.sections.length)) {
      multi.input.disabled = true;
      multi.input.title = "A repeat uses this variable. It needs several values.";
    }
    parts.push(multi.node);
    // The switch "Allow custom values" (4.8, 4.9, ID-K210): off for a new variable. The switch off removes the member from the draft.
    const open = switchField({
      label: "Allow custom values",
      checked: v.allow_custom_value === true,
      testid: t("allow-custom"),
      apply: (x) => {
        write(get().map((y, k) => (k === i ? setAllowCustom(y, x) : y)));
        showDefault();
      },
    });
    const helpId = `${t("allow-custom")}-help`;
    open.input.setAttribute("aria-describedby", helpId);
    open.node.append(el("span", { class: "field__hint", id: helpId, text: ALLOW_HELP[v.kind] }));
    parts.push(open.node);
    let allField = null;
    const paintAll = () => {
      const on = cur().include_all;
      allField.control.disabled = !on;
      allField.control.setAttribute("aria-disabled", String(!on));
    };
    parts.push(switchField({ label: "Include the option All", checked: v.include_all, testid: t("include-all"), apply: (x) => {
      update({ include_all: x, ...(x ? {} : { all_value: null }) });
      if (!x) allField.set("");
      paintAll();
      showDefault();
    } }).node);
    allField = textField(scope, {
      label: "Custom value of All",
      value: v.all_value === null ? "" : v.all_value,
      check: (x) => (x === "" ? "" : checkBytes(x, 1, 1024, "The value")),
      apply: (x) => update({ all_value: x === "" ? null : x }),
      testid: t("all-value"),
      hint: "Empty: the list of all the options. Active with the option All only.",
    });
    parts.push(allField.node);
    paintAll();
    if (v.kind === "query") {
      parts.push(selectField({ label: "Refresh", options: [["load", "On dashboard load"], ["time_change", "On time range change"]], value: v.refresh, apply: (x) => update({ refresh: x }), testid: t("refresh") }).node);
      parts.push(selectField({ label: "Sort", options: SORTS, value: v.sort, apply: (x) => { update({ sort: x }); optionsPreview?.clear(); }, testid: t("sort") }).node);
      parts.push(textField(scope, {
        label: "Regex",
        value: v.regex === null ? "" : v.regex,
        mono: true,
        check: (x) => (x === "" ? "" : checkPattern(x)),
        apply: (x) => {
          update({ regex: x === "" ? null : x });
          optionsPreview?.clear();
        },
        testid: t("regex"),
        hint: "Empty: no filter of the options.",
      }).node);
    }
    parts.push(el("div", { class: "field" }, el("span", { class: "field__label", text: "Default values" }), defaultList));
    showDefault();
    return parts;
  }

  // A rename does not change the SQL: the notice names the panels and the variables that still use the old name (10.3b).
  function announceRename(old, name) {
    const panels = stalePanels(E.doc, old);
    const vars = staleVariables(get(), old, name);
    const names = [...panels.map((p) => p.title || `panel ${p.id}`), ...vars.map((n) => `variable ${n}`), ...staleAnnotations(E.doc, old).map((n) => `annotation ${n}`)];
    const text = names.length
      ? `The variable ${old} is now ${name}. The SQL of ${names.join(", ")} still names $${old}.`
      : `The variable ${old} is now ${name}. No SQL names $${old}.`;
    notify({ level: names.length ? "warning" : "info", key: "rename", text });
  }

  // The menu of a placed control opens the form of its variable (editor/place.js).
  const start = E.openVariable ? get().findIndex((x) => x.name === E.openVariable) : -1;
  E.openVariable = null;
  if (start >= 0) drawForm(start);
  else drawList();
  return {
    el: root,
    redraw: () => (openAt === null ? drawList() : drawForm(openAt)),
    destroy: destroySql,
  };
}

// The list of the default values (4.11): 0 to 10,000 texts. A variable with one value has one default at most.
function defaultsList(scope, { get, set, cur, t }) {
  return listEditor(scope, {
    testid: t("defaults"),
    itemTestid: (n) => t(`default-${n}`),
    get: () => get().map((value) => ({ value })),
    set: (arr) => set(arr.map((x) => x.value)),
    limit: LIMITS.defaults,
    limitWhat: "default values",
    emptyText: "No default: the first option applies.",
    pageSize: 50,
    title: (x, n) => `Default ${n + 1}`,
    add: { label: "Add default", testid: t("default-add"), make: () => ({ value: cur().kind === "custom" && cur().options[0] ? cur().options[0].value : ALL }) },
    renderItem: (x, n, ctx) => textField(scope, { ariaLabel: `Default ${n + 1}`, value: x.value, check: (v) => checkBytes(v, 1, 1024, "The value") || (cur().kind === "custom" && cur().allow_custom_value === true && hasControlChar(v) ? "The value has a control character." : ""), apply: (v) => ctx.update({ value: v }), testid: `${ctx.testid}-value` }).node,
  });
}

// The options of a `custom` variable (4.9): 1 to 10,000 pairs of text and value. A repeated value gives an error.
function optionsList(scope, { get, set }) {
  return listEditor(scope, {
    testid: "variable-options",
    itemTestid: (n) => `variable-option-${n}`,
    get,
    set,
    limit: LIMITS.options,
    limitWhat: "options",
    minItems: 1,
    removeHint: "A custom variable needs one option.",
    pageSize: 50,
    title: (o) => o.text,
    add: { label: "Add option", testid: "variable-option-add", make: () => ({ text: `Option ${get().length + 1}`, value: String(get().length + 1) }) },
    renderItem(o, n, ctx) {
      return el("div", { class: "field-grid" },
        textField(scope, { label: "Text", value: o.text, check: (v) => checkChars(v, 1, 256, "The text"), apply: (v) => ctx.update({ text: v }), testid: `${ctx.testid}-text` }).node,
        textField(scope, {
          label: "Value",
          value: o.value,
          check: (v) => checkBytes(v, 1, 1024, "The value") || (get().some((x, k) => k !== n && x.value === v) ? "This value exists." : ""),
          apply: (v) => ctx.update({ value: v }),
          testid: `${ctx.testid}-value`,
        }).node);
    },
  });
}
