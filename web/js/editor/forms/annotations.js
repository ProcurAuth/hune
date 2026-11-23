// The form of the annotations (10.3b, 4.5b): the tab "Annotations" of the settings view.
// A list shows the name, the source and the colour of each annotation. A click opens the form of one annotation.
// The form has no "Run" and no preview: the route of 6.22 has no annotation. The person saves and looks at the panel.

import { toggle, uniqueId, button, el } from "../../ui.js";
import { allPanels } from "../draft.js";
import { createSqlEditor } from "../sql_editor.js";
import { colorChoice, listEditor, switchField, textField, selectField } from "./controls.js";
import { cssColor } from "./colors_form.js";
import { ANNOTATION_ADD_LIMIT, LIMITS, annotationFaults, newAnnotation, uniqueAnnotationName, withAnnotations } from "./model.js";
import { checkChars } from "./rules.js";

/** The kinds of source that an annotation can use (4.5b). A kind that a later version adds is not listed. */
const ANNOTATION_KINDS = ["clickhouse", "postgres", "mongodb"];

/** The sources that an annotation can use: the sources of the three kinds of `ANNOTATION_KINDS`. */
export const annotationSources = (sources) => sources.filter((s) => ANNOTATION_KINDS.includes(s.kind));

/**
 * `E` is the editor: `E.doc`, `E.change(doc)`, `E.sources`. It returns { el, redraw, destroy }.
 * A change of the draft acts on the page after the save, because the route reads the saved document (10.3b).
 */
export function createAnnotations(E, scope) {
  const root = el("div", { class: "vars-form", dataset: { testid: "settings-annotations" } });
  let sqlEditor = null;
  const get = () => E.doc.annotations || [];
  const set = (arr) => E.change(withAnnotations(structuredClone(E.doc), arr));
  const choices = () => annotationSources(E.sources);

  function destroySql() {
    if (sqlEditor) sqlEditor.destroy();
    sqlEditor = null;
  }

  // --- the list ------------------------------------------------------------------------------------------------------------

  function drawList() {
    destroySql();
    const list = listEditor(scope, {
      testid: "annotation-list",
      itemTestid: (i) => `annotation-${i}`,
      get,
      set,
      limit: LIMITS.annotations,
      limitWhat: "annotations",
      limitMessage: ANNOTATION_ADD_LIMIT,
      emptyText: "No annotation.",
      title: (a) => a.name,
      add: {
        label: "Add annotation",
        testid: "annotation-add",
        make() {
          const at = get().length;
          queueMicrotask(() => drawForm(at));
          // A new annotation takes the first ClickHouse source, else the first source: its SQL template is ClickHouse SQL (4.5b).
          const first = choices().find((s) => s.kind === "clickhouse") || choices()[0];
          return newAnnotation(uniqueAnnotationName(get()), (first && first.id) || "");
        },
      },
      renderItem(a, i, ctx) {
        const dot = el("span", { class: "step__dot", "aria-hidden": "true" });
        dot.style.setProperty("--swatch-color", cssColor(a.color));
        return el("div", { class: "vrow" },
          dot,
          el("span", { class: "muted", text: `${a.source}${a.enabled === false ? ", off" : ""}, ${a.color}` }),
          button("Edit", { dataset: { testid: `${ctx.testid}-open` }, on: { click: () => { scope.flush(); drawForm(i); } } }, "edit", { size: "sm" }));
      },
    });
    root.replaceChildren(list);
  }

  // --- the form of one annotation ------------------------------------------------------------------------------------------

  function drawForm(i) {
    scope.flush();
    destroySql();
    const a = get()[i];
    if (!a) return drawList();
    const cur = () => get()[i];
    const update = (patch) => {
      set(get().map((x, k) => (k === i ? { ...x, ...patch } : x)));
      showFaults();
    };
    const faults = el("ul", { class: "errors", role: "alert", dataset: { testid: "annotation-faults" } });
    const showFaults = () => {
      const list = annotationFaults(E.doc, i, E.sources);
      faults.replaceChildren(...list.map((f) => el("li", { text: `[${f.code}] ${f.pointer}: ${f.message}` })));
      faults.hidden = !list.length;
    };
    const t = (name) => `annotation-form-${name}`;
    const back = button("Back to the annotations", { dataset: { testid: "annotation-back" }, on: { click: () => { scope.flush(); drawList(); } } }, "arrow-left", { size: "sm" });
    const parts = [];
    parts.push(textField(scope, {
      label: "Name",
      value: a.name,
      check: (x) => checkChars(x, 1, 256, "The name") || (get().some((b, k) => k !== i && b.name === x) ? "This name exists." : ""),
      apply: (x) => update({ name: x }),
      testid: t("name"),
    }).node);
    const options = choices().map((s) => [s.id, s.label || s.id]);
    if (!options.some(([id]) => id === a.source)) options.unshift([a.source, `${a.source} (not available to you)`]);
    parts.push(selectField({ label: "Source", options, value: a.source, apply: (x) => update({ source: x }), testid: t("source"), hint: "A ClickHouse, PostgreSQL or MongoDB source." }).node);
    sqlEditor = createSqlEditor({
      lines: a.sql,
      label: "Annotation SQL",
      // The dialect of the syntax tree follows the kind of the source (10.3c), as for a panel and a variable.
      dialect: () => ((E.sources.find((s) => s.id === cur().source) || {}).kind || "clickhouse"),
      variables: () => (E.doc.variables || []).map((x) => x.name),
      onChange: (lines) => update({ sql: lines }),
    });
    parts.push(el("div", { class: "field" }, el("span", { class: "field__label", text: "Query" }), sqlEditor.el));
    parts.push(el("div", { class: "field" }, el("span", { class: "field__label", text: "Colour" }), colorChoice(scope, { value: a.color, ariaLabel: "Annotation colour", apply: (x) => update({ color: x }), testid: t("color") }).node));
    parts.push(switchField({ label: "Enabled", checked: a.enabled !== false, testid: t("enabled"), apply: (x) => update({ enabled: x }) }).node);
    parts.push(panelsGroup(cur, update, t, showFaults));
    showFaults();
    root.replaceChildren(el("div", { class: "vform" }, back, el("h3", { class: "vform__title", text: "Annotation" }), ...parts, faults));
  }

  // The group "Panels": "All time series" (no member `panels`), or "Some panels" with a check box for each time series of the draft.
  // The list keeps 1 id or more. An id of a panel that is not a time series stays in the list: the page draws nothing for it (4.5b, rule 4).
  function panelsGroup(cur, update, t, showFaults) {
    const series = allPanels(E.doc).filter((p) => p.type === "timeseries");
    const group = uniqueId("annot-panels");
    const message = el("p", { class: "field-error", role: "alert", hidden: true, dataset: { testid: t("panels-error") } });
    const boxes = el("div", { class: "annot-panels", dataset: { testid: t("panels") } });
    const radio = (value, label, checked) => el("label", { class: "check" },
      el("input", { type: "radio", name: group, value, checked, dataset: { testid: t(`panels-${value}`) }, on: { change: () => choose(value) } }), el("span", { text: label }));
    const warn = (text) => {
      message.textContent = text;
      message.hidden = !text;
    };
    function choose(value) {
      if (value === "all") {
        const { panels, ...rest } = cur();
        set(get().map((x) => (x === cur() ? rest : x)));
        warn("");
      } else if (series.length) {
        update({ panels: [series[0].id] });
        warn("");
      } else warn("The draft has no time series.");
      drawBoxes();
      showFaults();
    }
    function drawBoxes() {
      boxes.replaceChildren();
      boxes.hidden = !cur().panels;
      if (!cur().panels) return;
      for (const p of series) {
        const box = toggle({
          label: `${p.title || "Untitled"} (${p.id})`,
          checked: cur().panels.includes(p.id),
          kind: "check",
          testid: t(`panel-${p.id}`),
          onChange(on, ev) {
            const now = cur().panels;
            const next = on ? [...now, p.id] : now.filter((id) => id !== p.id);
            if (!next.length) {
              ev.target.checked = true;
              warn("Choose one panel at least.");
              return;
            }
            warn("");
            update({ panels: next });
          },
        });
        boxes.append(box.el);
      }
    }
    drawBoxes();
    return el("fieldset", { class: "vform__placement", dataset: { testid: t("panels-group") } },
      el("legend", { class: "field__label", text: "Panels" }), radio("all", "All time series", !cur().panels), radio("some", "Some panels", Boolean(cur().panels)), boxes, message);
  }

  drawList();
  return {
    el: root,
    redraw: drawList,
    destroy: destroySql,
  };
}
